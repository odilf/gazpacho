//! Media metadata probing.
//!
//! Rewrite in progress: the types below are the target shape, driven by
//! `tests/synthetic.rs` (run with `--features fixtures`). Pure time math is
//! implemented; probing is `todo!()`.

use std::{
    collections::HashMap,
    fmt,
    io::{self, BufRead as _, BufReader, Lines},
    ops::Range,
    process::{Child, ChildStdout, Command, Stdio},
    str::FromStr,
};

use eyre::{OptionExt, WrapErr as _, bail, ensure};
use ffmpeg_sidecar::ffprobe::ffprobe_path;
use num_rational::Ratio;

use crate::read::Resolution;

/// Media-local time in seconds, exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MediaTime(pub(crate) Ratio<i64>);
impl MediaTime {
    pub fn advance_secs(&self, delta: Ratio<u64>) -> MediaTime {
        let delta = Ratio::new(*delta.numer() as i64, *delta.denom() as i64);
        MediaTime(self.0 + delta)
    }
}

impl fmt::Display for MediaTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}s", self.0)
    }
}

/// Frame rate in frames per second, as an exact rational.
///
/// Expressed as a ratio to be exact, since NTSC rates like `24000/1001`
/// accumulate drift if held as floats, so this keeps frame-index math exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fps(Ratio<u64>);

impl Fps {
    pub fn value(&self) -> Ratio<u64> {
        self.0
    }

    /// Exact display duration of one frame.
    pub fn frame_length(self) -> Ratio<u64> {
        self.0.recip()
    }
}

/// The way frames are presented in video. Usually just constant, but can be variable.
#[derive(Debug, Clone, PartialEq)]
pub enum Timing {
    /// Constant frame rate: frame `i` is presented at `start + i / fps`.
    Constant(Fps),
    /// Variable frame rate: the exact absolute presentation timestamp of every
    /// frame, ascending (`timestamps[0] == start`).
    Variable(Box<[MediaTime]>),
}

/// Data around a video stream.
///
/// Along with the usual data, we also store timing data (`start`, `end` and
/// `keyframes`). These are exact and reflect what actually plays. For instance,
/// samples the container discards (e.g. the trimmed head of an edit list) are
/// excluded, and B-frame decode order is normalized to presentation order.
#[derive(Debug, Clone)]
pub struct VideoMetadata {
    pub resolution: Resolution,
    pub timing: Timing,
    /// Presentation timestamp of the first frame (not always zero!, mpegts
    /// preload, edit lists, trimmed streams).
    pub start: MediaTime,
    /// Total number of frames.
    pub frame_count: u32,
    /// End of the last frame's display window, so the stream covers
    /// `start..end`.
    pub end: MediaTime,
    /// Every time is a multiple of this.
    pub time_base: Ratio<u64>,
    /// Keyframe frame indices. Ascending, starting at 0. Empty if none were
    /// reported.
    pub keyframes: Box<[u32]>,
    pub stream_index: u8,
    pub parent_stream_index: Option<u8>,
    // TODO: I don't like this.
    pub attached_pic: bool,
}

impl VideoMetadata {
    /// The time range this stream covers.
    pub fn extent(&self) -> Range<MediaTime> {
        self.start..self.end
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AudioMetadata {
    pub sample_rate: u32,
    pub length: f64,
    pub stream_index: u8,
    pub parent_stream_index: Option<u8>,
}

#[derive(Debug, Clone)]
pub struct MediaMetadata {
    pub video: Vec<VideoMetadata>,
    pub audio: Vec<AudioMetadata>,
    /// How stream indices relate to the order we stored them.
    pub stream_map: Vec<(CodecType, u8)>,
}

impl MediaMetadata {
    /// Load media metadata (using `ffprobe`).
    ///
    /// All metadata is retrieved via demuxing container packets, not decoding
    /// (including keyframes), so it is relatively inexpensive.
    pub fn load(path: &str) -> eyre::Result<Self> {
        Self::assemble(probe_streams(path)?, video_timings_by_demux(path)?)
    }

    /// Equivalent to [`load`](Self::load), but derives timing from a full
    /// decode instead of container packets. Slower, but serves as the ground
    /// truth `load` is checked against in tests.
    #[cfg(any(test, feature = "fixtures"))]
    pub fn load_by_decode(path: &str) -> eyre::Result<Self> {
        Self::assemble(probe_streams(path)?, video_timings_by_decode(path)?)
    }

    /// Glue to share the impl of [`Self::load`] and [`Self::load_by_decode`].
    fn assemble(
        streams: impl Iterator<Item = eyre::Result<StreamInfo>>,
        timing_packets: impl Iterator<Item = io::Result<(u8, TimingPacket)>>,
    ) -> eyre::Result<Self> {
        let mut video = Vec::new();
        let mut audio = Vec::new();
        let mut stream_map = Vec::new();
        for stream in streams {
            let stream = stream?;
            match stream {
                StreamInfo::Video(info) => {
                    stream_map.push((CodecType::Video, video.len() as u8));
                    video.push((info, Vec::new()));
                }
                StreamInfo::Audio(info) => {
                    stream_map.push((CodecType::Audio, audio.len() as u8));
                    audio.push(AudioMetadata::from_ffprobe_info(&info))
                }
            }
        }

        for pair_res in timing_packets {
            let (index, packet) = pair_res?;
            let (_, video_index) = stream_map[index as usize];
            let (_info, packets) = &mut video[video_index as usize];
            packets.push(packet);
        }

        let video = video
            .into_iter()
            .map(|(info, mut packets)| {
                packets.sort_unstable_by_key(|p| p.pts);
                VideoMetadata::from_ffprobe_info_and_timing_packets(&info, &packets)
            })
            .collect::<eyre::Result<Vec<_>>>()?;

        Ok(MediaMetadata {
            video,
            audio,
            stream_map,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecType {
    Video,
    Audio,
}

impl FromStr for CodecType {
    type Err = eyre::Report;
    fn from_str(s: &str) -> eyre::Result<Self> {
        match s {
            "video" => Ok(Self::Video),
            "audio" => Ok(Self::Audio),
            other => bail!("Uknown codec type '{other}'"),
        }
    }
}

// === ffprobe probing ========================================================

/// Metadata fields you can get directly from `ffprobe`.
enum StreamInfo {
    Video(VideoStreamInfo),
    Audio(AudioStreamInfo),
}

struct VideoStreamInfo {
    index: u8,
    width: u32,
    height: u32,
    /// The container's base ("raw") frame rate, exact. For CFR streams this is
    /// the true rate even when the container rounds per-frame timestamps.
    r_frame_rate: Option<Fps>,
    time_base: Ratio<u64>,
    /// Cover art / thumbnail: a `codec_type=video` stream that is really a
    /// single embedded still, not a decodable track.
    attached_pic: bool,
}

struct AudioStreamInfo {
    index: u8,
    sample_rate: u32,
    duration: f64,
}

/// One video packet's timing, from `ffprobe -show_packets`.
struct TimingPacket {
    /// Presentation timestamp in `time_base` units.
    pts: i64,
    /// Display duration in `time_base` units, when the container carries it.
    duration: Option<i64>,
    /// Whether this packet holds a keyframe (ffprobe flag `K`).
    key_frame: bool,
}

impl VideoMetadata {
    fn from_ffprobe_info_and_timing_packets(
        stream: &VideoStreamInfo,
        packets: &[TimingPacket],
    ) -> eyre::Result<Self> {
        ensure!(
            !packets.is_empty(),
            "video stream {} has no packets",
            stream.index
        );

        let time_base = stream.time_base;
        let frame_count = u32::try_from(packets.len()).wrap_err("frame count exceeds u32")?;
        let start = MediaTime(Ratio::from_integer(packets[0].pts) * to_i64_ratio(time_base));
        let keyframes = packets
            .iter()
            .enumerate()
            .filter(|(_, p)| p.key_frame)
            .map(|(i, _)| i as u32)
            .collect();

        let (timing, end) = classify_timing(&packets, time_base, stream.r_frame_rate, start);

        Ok(VideoMetadata {
            resolution: Resolution {
                width: stream.width,
                height: stream.height,
            },
            timing,
            start,
            frame_count,
            end,
            keyframes,
            time_base,
            stream_index: stream.index,
            // TODO: Retrieve parent stream indices.
            parent_stream_index: None,
            attached_pic: stream.attached_pic,
        })
    }
}

impl AudioMetadata {
    fn from_ffprobe_info(stream: &AudioStreamInfo) -> Self {
        AudioMetadata {
            sample_rate: stream.sample_rate,
            length: stream.duration,
            stream_index: stream.index,
            // TODO: Get parent stream
            parent_stream_index: None,
        }
    }
}

/// Decide the [`Timing`] for `packets` (non-empty, already in presentation
/// order) and the stream's `end`. Chooses [`Timing::Constant`] when
/// `r_frame_rate` is fits the actual timestamps, otherwise [`Timing::Variable`]
/// with every timestamp kept exact.
fn classify_timing(
    packets: &[TimingPacket],
    time_base: Ratio<u64>,
    r_frame_rate: Option<Fps>,
    start: MediaTime,
) -> (Timing, MediaTime) {
    let time_at = |ticks: i64| Ratio::from_integer(ticks) * to_i64_ratio(time_base);

    if let Some(fps) = r_frame_rate {
        // A stream is CFR when every packet timestamp sits within half a
        // frame of a single constant rate. Absolute containers round each PTS
        // independently, so that rounding never accumulates — an NTSC clip
        // stored in millisecond timestamps still reads back as exact
        // `24000/1001`.
        let period = to_i64_ratio(fps.frame_length()); // seconds per frame
        let tolerance = period / 2;
        let is_cfr = packets.iter().enumerate().all(|(i, p)| {
            let expected = start.0 + Ratio::from_integer(i as i64) * period;
            let diff = time_at(p.pts) - expected;
            -tolerance <= diff && diff <= tolerance
        });
        if is_cfr {
            let frames_secs = Ratio::from_integer(u64::from(packets.len() as u32));
            let end = start.advance_secs(frames_secs * fps.frame_length());
            return (Timing::Constant(fps), end);
        }
    }

    // Not CFR, or no declared rate: keep every timestamp exactly, as VFR.
    let timestamps = packets.iter().map(|p| MediaTime(time_at(p.pts))).collect();
    let last = packets.last().expect("packets is non-empty");
    let end = match last.duration {
        Some(duration) => MediaTime(time_at(last.pts + duration)),
        // No per-frame duration: fall back to the last packet's own timestamp.
        None => MediaTime(time_at(last.pts)),
    };
    (Timing::Variable(timestamps), end)
}

/// `Ratio<u64>` -> `Ratio<i64>`, for mixing durations into signed media time.
fn to_i64_ratio(r: Ratio<u64>) -> Ratio<i64> {
    Ratio::new(*r.numer() as i64, *r.denom() as i64)
}

/// Get raw stream info easily accessible from `ffprobe`.
fn probe_streams(path: &str) -> io::Result<impl Iterator<Item = eyre::Result<StreamInfo>>> {
    // Example output.
    //
    // ```
    // stream,0,video,1080,1920,22500/751,1/22500,17.122800,0
    // stream,1,audio,48000,0/0,1/48000,17.111000,0
    // ```
    let lines = FfprobeLines::new(
        &[
            "-show_entries",
            "stream=index,codec_type,width,height,r_frame_rate,time_base,sample_rate,duration:stream_disposition=attached_pic",
            "-of",
            "csv",
        ],
        path,
    )?;

    Ok(lines.parse(|line| {
        let mut entries = line.split(",");
        ensure!(entries.next() == Some("stream"));
        let index = entries
            .next()
            .ok_or_eyre("Expected `index` entry")?
            .parse()?;
        let codec_type = entries
            .next()
            .ok_or_eyre("expected `codec_type` entry.")?
            .parse()?;

        match codec_type {
            CodecType::Video => {
                let width = entries
                    .next()
                    .ok_or_eyre("expected `width` entry")?
                    .parse()?;
                let height = entries
                    .next()
                    .ok_or_eyre("expected `height` entry")?
                    .parse()?;
                let r_frame_rate = entries
                    .next()
                    .ok_or_eyre("expected `r_frame_rate` entry")?
                    .parse()
                    .ok()
                    .map(Fps);
                let time_base = entries
                    .next()
                    .ok_or_eyre("expected `time_base` entry")?
                    .parse()?;
                // TODO: Maybe use duration?
                let _duration: f64 = entries
                    .next()
                    .ok_or_eyre("expected `duration` entry")?
                    .parse()?;
                let attached_pic =
                    entries.next().ok_or_eyre("expected `attached_pic` entry")? == "1";
                Ok(StreamInfo::Video(VideoStreamInfo {
                    index,
                    width,
                    height,
                    r_frame_rate,
                    time_base,
                    attached_pic,
                }))
            }
            CodecType::Audio => {
                let sample_rate = entries
                    .next()
                    .ok_or_eyre("expected `sample_rate` entry")?
                    .parse()?;
                let _r_frame_rate = Fps(entries
                    .next()
                    .ok_or_eyre("expected `r_frame_rate` entry")?
                    .parse()?);
                let _time_base = entries.next().ok_or_eyre("expected `time_base` entry")?;
                let duration = entries
                    .next()
                    .ok_or_eyre("expected `duration` entry")?
                    .parse()?;
                let _attached_pic = entries.next().ok_or_eyre("expected `attached_pic` entry")?;
                Ok(StreamInfo::Audio(AudioStreamInfo {
                    index,
                    sample_rate,
                    duration,
                }))
            }
        }
    }))
}

/// Retrieves timing information from a path using `ffprobe`. This is
/// implemented as demux-only, so we never decode and should be reasonably fast.
/// We also exclude samples the container discards (e.g. the trimmed head of an
/// edit list), so results should match what you would get from a full decode
/// (which is in fact what [`video_packets_by_decode`] does).
fn video_timings_by_demux(
    path: &str,
) -> eyre::Result<impl Iterator<Item = io::Result<(u8, TimingPacket)>>> {
    let lines = FfprobeLines::new(
        &[
            "-select_streams",
            "v",
            "-show_entries",
            "packet=stream_index,pts,duration,flags",
            // csv is safe here: the requested packet fields are always present,
            // so any trailing side-data columns can be ignored by position.
            "-of",
            "csv=p=0",
        ],
        path,
    )?;

    // TODO: Don't swallow errors.
    Ok(lines.filter_map(|line_res| {
        let line = match line_res {
            Ok(line) => line,
            Err(err) => return Some(Err(err)),
        };

        let line = line.trim();
        if line.is_empty() {
            tracing::warn!("Empty packet");
            return None;
        }
        let mut fields = line.split(',');
        let stream_index: u8 = fields.next().and_then(|v| v.parse().ok())?;
        let pts = fields.next().and_then(|v| v.parse().ok())?;
        let duration = fields.next().and_then(|v| v.parse().ok());

        // ffprobe renders packet flags as e.g. `K__`: `K` marks a keyframe,
        // `D` a packet the demuxer discards. A trimming edit list (an mp4
        // presenting from partway into the media) leaves the trimmed samples in
        // the packet stream flagged `D`, with pre-roll timestamps, even though
        // they never display. Skipping them keeps the frame count, `start`,
        // and keyframe indices aligned with what actually plays (i.e. with the
        // decoder's own `best_effort_timestamp`).
        let flags = fields.next().unwrap_or("");
        if flags.contains('D') {
            return None;
        }

        Some(Ok((
            stream_index,
            TimingPacket {
                pts,
                duration,
                key_frame: flags.contains('K'),
            },
        )))
    }))
}

/// Same idea as [`video_timings_by_demux`], except that here we
/// do a full decode. I.e., slower but less error-prone version of
/// [`video_timings_by_demux`].
#[cfg(any(test, feature = "fixtures"))]
fn video_timings_by_decode(
    path: &str,
) -> eyre::Result<impl Iterator<Item = io::Result<(u8, TimingPacket)>>> {
    let lines = FfprobeLines::new(
        &[
            "-select_streams",
            "v",
            "-show_entries",
            "frame=stream_index,key_frame,best_effort_timestamp,duration",
            "-of",
            "csv=p=0",
        ],
        path,
    )?;

    // TODO: Don't swallow errors, use parse.
    Ok(lines.filter_map(|line_res| {
        let line = match line_res {
            Ok(line) => line,
            Err(err) => return Some(Err(err)),
        };

        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        let mut fields = line.split(',');
        let stream_index: u8 = fields.next().and_then(|v| v.parse().ok())?;
        // ffprobe emits frame csv columns in its own fixed internal order, not
        // the order requested: stream_index, key_frame,
        // best_effort_timestamp, duration.
        let key_frame = fields.next() == Some("1");
        let pts = fields.next().and_then(|v| v.parse().ok())?;
        let duration = fields.next().and_then(|v| v.parse().ok());

        Some(Ok((
            stream_index,
            TimingPacket {
                pts,
                duration,
                key_frame,
            },
        )))
    }))
}

/// Helper that streams Ffprobe lines and kills the process properly.
struct FfprobeLines {
    child: Child,
    lines: Lines<BufReader<ChildStdout>>,
}

impl FfprobeLines {
    fn new(args: &[&str], path: &str) -> std::io::Result<Self> {
        let mut child = Command::new(ffprobe_path())
            .args(["-loglevel", "error"])
            .args(args)
            .arg(path)
            .stdin(Stdio::null())
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;

        let stdout = child.stdout.take().expect("stdout was piped");
        Ok(Self {
            child,
            lines: BufReader::new(stdout).lines(),
        })
    }

    fn parse<F, T>(self, mut f: F) -> impl Iterator<Item = eyre::Result<T>>
    where
        F: FnMut(&str) -> eyre::Result<T> + 'static,
    {
        self.map(move |line_res| f(&line_res?))
    }
}

impl Iterator for FfprobeLines {
    type Item = std::io::Result<String>;
    fn next(&mut self) -> Option<Self::Item> {
        self.lines.next()
    }
}

impl Drop for FfprobeLines {
    fn drop(&mut self) {
        // If we might stop iterating early, kill first so wait() can't hang on
        // a process that's still producing output:
        let _ = self.child.kill();
        let _ = self.child.wait(); // reap
    }
}

#[cfg(test)]
mod tests {
    use eyre::Result;

    use crate::{
        fixtures::corpus,
        metadata::{video_timings_by_decode, video_timings_by_demux},
    };

    #[test]
    fn timing_packets_are_in_stream_index_order() -> Result<()> {
        for fixture in corpus().all() {
            let mut previous_index = 0;
            for entry in video_timings_by_demux(fixture.path_str())? {
                let (index, _packet) = entry?;
                assert!(index >= previous_index);
                previous_index = index;
            }
        }

        for fixture in corpus().all() {
            let mut previous_index = 0;
            for entry in video_timings_by_decode(fixture.path_str())? {
                let (index, _packet) = entry?;
                assert!(index >= previous_index);
                previous_index = index;
            }
        }

        Ok(())
    }
}
