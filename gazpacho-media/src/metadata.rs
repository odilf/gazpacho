//! Media metadata probing.
//!
//! Rewrite in progress: the types below are the target shape, driven by
//! `tests/synthetic.rs` (run with `--features fixtures`). Pure time math is
//! implemented; probing is `todo!()`.

use std::{
    fmt,
    io::{self, BufRead as _, BufReader, Lines},
    ops::Range,
    process::{Child, ChildStdout, Command, Stdio},
    str::FromStr,
};

use eyre::{OptionExt, WrapErr as _, ensure};
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
pub struct Fps(pub(crate) Ratio<u64>);

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
    /// Keyframe frame indices, ascending. Never empty: frame 0 counts as a
    /// seek point even when the container flags no sync samples.
    ///
    /// A *hint*, not ground truth: containers can flag sync samples wrongly
    /// in both directions, so consumers must tolerate a "keyframe" that
    /// isn't cleanly seekable (and real keyframes that are missing here).
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
    /// (including keyframes), so it is relatively inexpensive. The exception
    /// is files suspected of holding invisible alt-ref frames (see
    /// [`invisible_frames_suspected`]), which fall back to a decode.
    pub fn load(path: &str) -> eyre::Result<Self> {
        let streams = probe_streams(path)?.collect::<eyre::Result<Vec<_>>>()?;
        let packets = video_timings_by_demux(path)?.collect::<io::Result<Vec<_>>>()?;

        if invisible_frames_suspected(&streams, &packets) {
            tracing::debug!(
                path,
                "invisible alt-ref frames suspected; timing via decode"
            );
            return Self::assemble(streams.into_iter().map(Ok), video_timings_by_decode(path)?);
        }
        Self::assemble(streams.into_iter().map(Ok), packets.into_iter().map(Ok))
    }

    /// Equivalent to [`load`](Self::load), but always derives timing from a
    /// full decode instead of container packets. Slower, but serves as the
    /// ground truth `load` is checked against in tests.
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
                StreamInfo::Other => stream_map.push((CodecType::Other, 0)),
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
            // Cover art / thumbnails are not decodable tracks; they are kept
            // in the routing above (their packets, if any, must go somewhere)
            // but must not surface as a `VideoMetadata`.
            .filter(|(info, _)| !info.attached_pic)
            .map(|(info, mut packets)| {
                // Stable sort: equal-pts packets must keep stream order for
                // the dedup below.
                packets.sort_by_key(|p| p.pts);
                // VP8/VP9 alt-ref ("invisible") frames are stored as their
                // own packets sharing a pts with the visible frame that
                // follows them, but the decoder emits one frame per pts.
                // Collapse each run to its last packet, keeping any keyframe
                // flag.
                let mut deduped: Vec<TimingPacket> = Vec::with_capacity(packets.len());
                for packet in packets {
                    match deduped.last_mut() {
                        Some(last) if last.pts == packet.pts => {
                            let key_frame = last.key_frame || packet.key_frame;
                            *last = packet;
                            last.key_frame = key_frame;
                        }
                        _ => deduped.push(packet),
                    }
                }
                VideoMetadata::from_ffprobe_info_and_timing_packets(&info, &deduped)
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
    /// Any stream we don't model (data/metadata tracks, subtitles,
    /// attachments). Kept so `stream_map` stays aligned with container
    /// stream indices.
    Other,
}

impl FromStr for CodecType {
    type Err = eyre::Report;
    fn from_str(s: &str) -> eyre::Result<Self> {
        match s {
            "video" => Ok(Self::Video),
            "audio" => Ok(Self::Audio),
            _ => Ok(Self::Other),
        }
    }
}

/// Whether the packet stream looks like it contains invisible alt-ref
/// frames: packets that decode into reference-only frames and never display.
///
/// The container gives such packets ordinary-looking timing (regular
/// duration, no flags), so demuxing alone cannot count displayed frames.
/// The tell is the libvpx convention of stamping an alt-ref with the
/// previous frame's pts plus one tick, on a codec that supports invisible
/// frames in the first place.
///
/// TODO: Parsing the codec-level `show_frame` bit from the packet payloads
/// would detect these exactly instead of falling back to a full decode.
fn invisible_frames_suspected(streams: &[StreamInfo], packets: &[(u8, TimingPacket)]) -> bool {
    let risky: Vec<u8> = streams
        .iter()
        .filter_map(|stream| match stream {
            StreamInfo::Video(video)
                if matches!(video.codec_name.as_str(), "vp8" | "vp9" | "av1") =>
            {
                Some(video.index)
            }
            _ => None,
        })
        .collect();
    if risky.is_empty() {
        return false;
    }

    let mut previous_pts = std::collections::HashMap::new();
    packets.iter().any(|(index, packet)| {
        if !risky.contains(index) {
            return false;
        }
        let previous = previous_pts.insert(*index, packet.pts);
        previous.is_some_and(|previous| packet.pts == previous + 1)
    })
}

// === ffprobe probing ========================================================

/// Metadata fields you can get directly from `ffprobe`.
enum StreamInfo {
    Video(VideoStreamInfo),
    Audio(AudioStreamInfo),
    /// A stream we don't model; holds a `stream_map` slot only.
    Other,
}

struct VideoStreamInfo {
    index: u8,
    codec_name: String,
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
        let keyframes: Box<[u32]> = packets
            .iter()
            .enumerate()
            .filter(|(_, p)| p.key_frame)
            .map(|(i, _)| i as u32)
            .collect();
        // A decodable stream's first frame is a seek point even when the
        // container flags no sync samples at all (e.g. fragmented mp4 with
        // the keyframe marked non-sync); the decoder reports it as a
        // keyframe, so the packet path must agree.
        let keyframes = if keyframes.is_empty() {
            Box::new([0])
        } else {
            keyframes
        };

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
    // Example output (the trailing `-90` only appears on streams carrying a
    // display-matrix rotation).
    //
    // ```
    // stream,0,h264,video,1080,1920,22500/751,1/22500,17.122800,0,-90
    // stream,1,aac,audio,48000,0/0,1/48000,17.111000,0
    // ```
    let lines = FfprobeLines::new(
        &[
            "-show_entries",
            "stream=index,codec_name,codec_type,width,height,r_frame_rate,time_base,sample_rate,duration:stream_disposition=attached_pic:stream_side_data=rotation",
            "-of",
            "csv",
        ],
        path,
    )?;

    // mpegts wraps its streams in `program` sections, which csv prints as
    // extra rows (`program,stream,...` for the section's first stream, bare
    // `stream,...` for the rest, plus blank spacer lines) *without* the
    // trailing disposition column. Every stream also appears in the
    // top-level `streams` section, printed last with the full column set,
    // so per stream index we keep only the last row.
    let mut rows: Vec<(u8, String)> = Vec::new();
    for line_res in lines {
        let line = line_res?;
        let mut fields = line.split(',');
        if fields.next() != Some("stream") {
            continue;
        }
        let Some(index) = fields.next().and_then(|f| f.parse().ok()) else {
            continue;
        };
        match rows.iter_mut().find(|(i, _)| *i == index) {
            Some((_, existing)) => *existing = line,
            None => rows.push((index, line)),
        }
    }
    rows.sort_unstable_by_key(|(index, _)| *index);

    Ok(rows.into_iter().map(|(index, line)| {
        let mut entries = line.split(",");
        let _section = entries.next(); // "stream", checked above
        let _index = entries.next(); // parsed above
        let codec_name = entries
            .next()
            .ok_or_eyre("expected `codec_name` entry")?
            .to_owned();
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
                // Not parsed: mkv/webm report `N/A` at the stream level,
                // and video timing comes from packets anyway.
                let _duration = entries.next().ok_or_eyre("expected `duration` entry")?;
                let attached_pic =
                    entries.next().ok_or_eyre("expected `attached_pic` entry")? == "1";
                // Remaining columns are stream side data; `rotation` is
                // the only field requested, so the first numeric one is
                // the display-matrix rotation in degrees.
                let rotation: f64 = entries.find_map(|e| e.parse().ok()).unwrap_or(0.0);
                let (width, height) = if (rotation.round() as i64).rem_euclid(180) == 90 {
                    // What plays is the rotated image (decoders apply the
                    // display matrix), so report display dimensions.
                    (height, width)
                } else {
                    (width, height)
                };
                Ok(StreamInfo::Video(VideoStreamInfo {
                    index,
                    codec_name,
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
                // Not parsed: audio streams report `0/0`.
                let _r_frame_rate = entries.next().ok_or_eyre("expected `r_frame_rate` entry")?;
                let _time_base = entries.next().ok_or_eyre("expected `time_base` entry")?;
                let duration = entries
                    .next()
                    .ok_or_eyre("expected `duration` entry")?
                    .parse()
                    // mkv/webm report `N/A` at the stream level.
                    .unwrap_or(f64::NAN);
                let _attached_pic = entries.next().ok_or_eyre("expected `attached_pic` entry")?;
                Ok(StreamInfo::Audio(AudioStreamInfo {
                    index,
                    sample_rate,
                    duration,
                }))
            }
            CodecType::Other => Ok(StreamInfo::Other),
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
    use std::path::Path;

    use crate::fixtures::{self, videos};

    use super::*;

    // === classify_timing: pure unit tests, no files or ffmpeg ===============

    fn cfr_packets(pts: impl IntoIterator<Item = i64>, duration: Option<i64>) -> Vec<TimingPacket> {
        pts.into_iter()
            .map(|pts| TimingPacket {
                pts,
                duration,
                key_frame: false,
            })
            .collect()
    }

    fn zero() -> MediaTime {
        MediaTime(Ratio::from_integer(0))
    }

    /// The mkv/webm case: NTSC timestamps rounded to whole milliseconds must
    /// still classify as *exactly* `24000/1001` — absolute containers round
    /// each PTS independently, so the rounding never accumulates.
    #[test]
    fn ntsc_rounded_to_milliseconds_classifies_as_exact_constant() {
        let fps = Fps(Ratio::new(24000, 1001));
        let n = 60i64;
        // pts_i = round(i * 1001/24000 * 1000), in a 1/1000 time base.
        let pts = (0..n).map(|i| (i * 1_001_000 + 12_000) / 24_000);
        let (timing, end) = classify_timing(
            &cfr_packets(pts, None),
            Ratio::new(1, 1000),
            Some(fps),
            zero(),
        );

        assert_eq!(timing, Timing::Constant(fps));
        assert_eq!(end, MediaTime(Ratio::new(n * 1001, 24000)));
    }

    #[test]
    fn exact_integer_rate_classifies_as_constant() {
        let fps = Fps(Ratio::from_integer(30));
        let (timing, end) = classify_timing(
            &cfr_packets((0..10).map(|i| i * 512), Some(512)),
            Ratio::new(1, 15360),
            Some(fps),
            zero(),
        );

        assert_eq!(timing, Timing::Constant(fps));
        assert_eq!(end, MediaTime(Ratio::new(10, 30)));
    }

    /// One timestamp pushed more than half a frame off the grid: the declared
    /// rate is a lie, so every timestamp is kept exactly as VFR.
    #[test]
    fn jitter_beyond_half_frame_classifies_as_variable() {
        // 10 fps in ms: grid is 0, 100, 200, ... — 370 is 70 ms off (> 50).
        let pts = [0, 100, 200, 370, 400];
        let (timing, end) = classify_timing(
            &cfr_packets(pts, Some(100)),
            Ratio::new(1, 1000),
            Some(Fps(Ratio::from_integer(10))),
            zero(),
        );

        let Timing::Variable(timestamps) = timing else {
            panic!("jittered stream classified as constant")
        };
        let expected: Vec<MediaTime> = pts
            .iter()
            .map(|&ms| MediaTime(Ratio::new(ms, 1000)))
            .collect();
        assert_eq!(&*timestamps, expected.as_slice());
        assert_eq!(end, MediaTime(Ratio::new(500, 1000)), "last pts + duration");
    }

    /// No declared rate: exact variable timestamps; the end honors the last
    /// packet's duration and falls back to its timestamp without one.
    #[test]
    fn missing_r_frame_rate_keeps_exact_variable_timestamps_and_duration_end() {
        let pts = [0, 33, 67, 100];
        let tb = Ratio::new(1, 1000);

        let (timing, end) = classify_timing(&cfr_packets(pts, Some(33)), tb, None, zero());
        assert!(matches!(timing, Timing::Variable(_)));
        assert_eq!(end, MediaTime(Ratio::new(133, 1000)));

        let (_, end) = classify_timing(&cfr_packets(pts, None), tb, None, zero());
        assert_eq!(
            end,
            MediaTime(Ratio::new(100, 1000)),
            "no-duration fallback"
        );
    }

    /// The mpegts-style case: a stream starting at 1.4 s still classifies as
    /// constant, with the grid anchored at `start`.
    #[test]
    fn nonzero_start_offsets_the_constant_grid() {
        let fps = Fps(Ratio::from_integer(30));
        let start = MediaTime(Ratio::new(7, 5));
        // 90 kHz time base: start at 126000 ticks, 3000 ticks per frame.
        let (timing, end) = classify_timing(
            &cfr_packets((0..30).map(|i| 126_000 + i * 3_000), None),
            Ratio::new(1, 90000),
            Some(fps),
            start,
        );

        assert_eq!(timing, Timing::Constant(fps));
        assert_eq!(end, MediaTime(Ratio::new(7, 5) + Ratio::from_integer(1)));
    }

    // === Edge-case files the synthetic `Spec` matrix can't express =========

    /// A trimming edit list keeps the frames before the cut in the container
    /// (as discard-flagged, pre-roll packets) even though they never present.
    /// Metadata must reflect what *plays*: the discarded packets are excluded
    /// from the frame count, the start is the presentation zero (not the
    /// negative pre-roll), and keyframe indices are renumbered against the
    /// presented frames.
    #[test]
    fn trimming_edit_list_excludes_discarded_frames() {
        fixtures::init_tracing();
        let baseline = videos().baseline();
        let spec = baseline.expect_spec();
        let resolution = spec.resolution;
        let total = spec.frames;

        let path = fixtures::trimmed_baseline();
        let path = path.to_str().unwrap();

        // Ground truth via an independent decode (the edit list is applied, so
        // only presented frames come out); every frame announces its original
        // index.
        let frames = fixtures::decode_all_rgba(Path::new(path), resolution).unwrap();
        let presented: Vec<u32> = frames
            .iter()
            .map(|f| fixtures::recover_index(f).unwrap())
            .collect();
        let first = presented[0];
        assert!(first > 0, "seek should trim into the stream, past frame 0");
        assert!(
            (frames.len() as u32) < total,
            "trim should drop whole frames"
        );
        assert_eq!(
            presented,
            (first..total).collect::<Vec<_>>(),
            "presented frames are a contiguous tail of the original"
        );

        let meta = MediaMetadata::load(path).unwrap();
        let video = &meta.video[0];

        // The whole point: discarded packets are NOT counted. Without discard
        // handling this would be the raw packet count (== `total`), not
        // `frames.len()`.
        assert_eq!(video.frame_count as usize, frames.len());
        // Presentation starts at zero, not at the discarded packets' negative
        // pre-roll.
        assert_eq!(video.start, spec.start_offset);

        // Keyframes renumber against presented frames: the original keyframes
        // at or after the cut, shifted down by `first`. (The first presented
        // frame is mid-GOP, so index 0 is deliberately not a keyframe here.)
        let expected_keyframes: Vec<u32> = (0..total)
            .step_by(spec.gop as usize)
            .filter(|&k| k >= first)
            .map(|k| k - first)
            .collect();
        assert_eq!(&*video.keyframes, expected_keyframes.as_slice());
        assert_ne!(video.keyframes.first(), Some(&0));
    }

    /// An audio track is probed into `AudioMetadata` alongside the video.
    #[test]
    fn audio_stream_is_probed() {
        fixtures::init_tracing();
        let path = fixtures::baseline_with_audio();
        let meta = MediaMetadata::load(path.to_str().unwrap()).unwrap();

        assert_eq!(meta.video.len(), 1);
        assert_eq!(meta.audio.len(), 1);
        let audio = &meta.audio[0];
        assert_eq!(audio.sample_rate, 44100);
        assert_eq!(
            audio.stream_index, 1,
            "audio is the second container stream"
        );
        assert!(audio.length > 0.0, "audio length should be probed");
    }

    /// Cover art is an `attached_pic` video stream — a single embedded still,
    /// not a real track — and must not surface as a `VideoMetadata`.
    #[test]
    fn attached_picture_is_skipped() {
        fixtures::init_tracing();
        let path = fixtures::baseline_with_cover_art();
        let meta = MediaMetadata::load(path.to_str().unwrap()).unwrap();

        assert_eq!(
            meta.video.len(),
            1,
            "the attached_pic stream should be ignored, leaving one real video stream"
        );
        assert_eq!(meta.video[0].frame_count, 60);
        assert_eq!(meta.video[0].stream_index, 0);
    }
}
