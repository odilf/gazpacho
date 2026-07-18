//! Media metadata probing.
//!
//! Rewrite in progress: the types below are the target shape, driven by
//! `tests/synthetic.rs` (run with `--features fixtures`). Pure time math is
//! implemented; probing is `todo!()`.

use std::{
    collections::HashMap,
    fmt,
    ops::Range,
    process::{Command, Stdio},
};

use eyre::{WrapErr as _, bail, ensure};
use ffmpeg_sidecar::ffprobe::ffprobe_path;
use num_rational::Ratio;

use crate::read::Resolution;

/// Media-local time in seconds, exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MediaTime(pub(crate) Ratio<i64>);
impl MediaTime {
    fn from_duration_secs(secs: Ratio<u64>) -> MediaTime {
        MediaTime(Ratio::new(*secs.numer() as i64, *secs.denom() as i64))
    }

    pub fn advance_secs(&self, delta: Ratio<u64>) -> MediaTime {
        MediaTime(self.0 + MediaTime::from_duration_secs(delta).0)
    }
}

impl fmt::Display for MediaTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}s", self.0)
    }
}

/// Frame rate in frames per second, as an exact rational.
///
/// Exactness matters: NTSC rates like `24000/1001` accumulate drift if held
/// as floats, and frame-index math in the reader must be exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fps(Ratio<u64>);

impl Fps {
    /// `None` unless `numer/denom` is a positive, finite rate.
    pub fn new(numer: u64, denom: u64) -> Option<Self> {
        (numer != 0 && denom != 0).then(|| Fps(Ratio::new(numer, denom)))
    }

    pub fn get(self) -> Ratio<u64> {
        self.0
    }

    /// Exact timestamp of frame `index`, relative to the stream's start.
    pub fn time_at(self, index: u32) -> Ratio<u64> {
        Ratio::from_integer(u64::from(index)) / self.0
    }

    /// Exact display duration of one frame.
    pub fn frame_length(self) -> Ratio<u64> {
        self.0.recip()
    }

    /// The frame on screen at `time` (relative to stream start): the largest
    /// `i` with `time_at(i) <= time`.
    pub fn frame_index_at(self, time: Ratio<u64>) -> u32 {
        u32::try_from((time * self.0).to_integer()).expect("frame index fits u32")
    }

    /// Frame index if `time` lies exactly on a frame boundary; errors
    /// otherwise. No float tolerance — rationals make "exact" meaningful.
    pub fn exact_frame_index(self, time: Ratio<u64>) -> eyre::Result<u32> {
        let index = time * self.0;
        eyre::ensure!(
            index.is_integer(),
            "time {time} is not on a frame boundary (frame {index})"
        );
        Ok(u32::try_from(index.to_integer()).expect("frame index fits u32"))
    }
}

/// Per-frame timing of a video stream.
#[derive(Debug, Clone, PartialEq)]
pub enum Timing {
    /// Constant frame rate: frame `i` is presented at `start + i / fps`.
    Constant(Fps),
    /// Variable frame rate: the exact absolute presentation timestamp of
    /// every frame, ascending (`timestamps[0] == start`).
    Variable(Box<[MediaTime]>),
}

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
    /// Keyframe frame indices. Ascending, starting at 0. Empty if none were
    /// reported.
    pub keyframes: Box<[u32]>,
    pub stream_index: u8,
    pub video_stream_index: u8,
    pub parent_stream_index: Option<u8>,
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
    pub audio_stream_index: u8,
    pub parent_stream_index: Option<u8>,
}

// TODO(streams rework): metadata is really a tree.
#[derive(Debug, Clone)]
pub struct MediaMetadata {
    pub video: Vec<VideoMetadata>,
    pub audio: Vec<AudioMetadata>,
}

impl MediaMetadata {
    /// Probe `path` with `ffprobe`, reconstructing exact timing.
    ///
    /// Two probes, both demux-only (no decoding the whole file): one for
    /// stream-level info (`-show_streams`) and one for per-packet timing and
    /// keyframes (`-show_packets`, video only). Frame rates come back as exact
    /// rationals, per-frame timestamps in the stream timebase, and B-frame
    /// decode order is undone by sorting on the presentation timestamp.
    ///
    /// Timing comes from container packet timestamps, which match the decoder's
    /// `best_effort_timestamp` for every codec/container we handle (verified
    /// including B-frame and non-zero-start streams). A trimming edit list — an
    /// mp4 that presents from partway into the media — is the one case the two
    /// could differ; none of our inputs use one.
    pub fn load(path: &str) -> eyre::Result<Self> {
        let streams = probe_streams(path)?;
        let mut packets_by_stream = probe_video_packets(path)?;

        let mut video = Vec::new();
        let mut audio = Vec::new();
        for stream in &streams {
            match stream.codec_type.as_str() {
                // Cover art shows up as a single-frame video stream; it's not a
                // real track, so drop it (and don't spend an ordinal on it).
                "video" if stream.attached_pic => {}
                "video" => {
                    let packets = packets_by_stream.remove(&stream.index).unwrap_or_default();
                    let ordinal = video.len() as u8;
                    video.push(build_video(stream, packets, ordinal)?);
                }
                "audio" => {
                    let ordinal = audio.len() as u8;
                    audio.push(build_audio(stream, ordinal)?);
                }
                _ => {}
            }
        }

        Ok(MediaMetadata { video, audio })
    }
}

// === ffprobe probing ========================================================

/// One stream's raw fields, straight from `ffprobe -show_streams`.
struct StreamInfo {
    index: u8,
    codec_type: String,
    width: Option<u32>,
    height: Option<u32>,
    /// The container's base ("raw") frame rate, exact. For CFR streams this is
    /// the true rate even when the container rounds per-frame timestamps.
    r_frame_rate: Option<Ratio<u64>>,
    time_base: Ratio<i64>,
    sample_rate: Option<u32>,
    duration: Option<f64>,
    /// Cover art / thumbnail: a `codec_type=video` stream that is really a
    /// single embedded still, not a decodable track.
    attached_pic: bool,
}

/// One video packet's timing, from `ffprobe -show_packets`.
struct Packet {
    /// Presentation timestamp in `time_base` units.
    pts: i64,
    /// Display duration in `time_base` units, when the container carries it.
    duration: Option<i64>,
    /// Whether this packet holds a keyframe (ffprobe flag `K`).
    key_frame: bool,
}

fn build_video(
    stream: &StreamInfo,
    mut packets: Vec<Packet>,
    ordinal: u8,
) -> eyre::Result<VideoMetadata> {
    ensure!(
        !packets.is_empty(),
        "video stream {} has no packets",
        stream.index
    );
    let (Some(width), Some(height)) = (stream.width, stream.height) else {
        bail!("video stream {} is missing width/height", stream.index);
    };
    let time_base = stream.time_base;

    // Undo B-frame decode order: timing and keyframe indices are in
    // presentation order.
    packets.sort_by_key(|p| p.pts);

    let frame_count = u32::try_from(packets.len()).wrap_err("frame count exceeds u32")?;
    let start = MediaTime(Ratio::from_integer(packets[0].pts) * time_base);
    let keyframes = packets
        .iter()
        .enumerate()
        .filter(|(_, p)| p.key_frame)
        .map(|(i, _)| i as u32)
        .collect();

    let (timing, end) = classify_timing(&packets, time_base, stream.r_frame_rate, start);

    Ok(VideoMetadata {
        resolution: Resolution { width, height },
        timing,
        start,
        frame_count,
        end,
        keyframes,
        stream_index: stream.index,
        video_stream_index: ordinal,
        parent_stream_index: None,
    })
}

/// Decide constant vs variable frame rate and compute the stream's end.
///
/// A stream is treated as CFR when its actual per-frame timestamps all sit
/// within half a frame of a single constant rate (`r_frame_rate`). Absolute
/// containers round each PTS independently, so that rounding never accumulates
/// — an NTSC clip stored in millisecond timestamps still reads as exact
/// `24000/1001`. Anything that genuinely drifts is kept as VFR with every
/// timestamp preserved exactly.
fn classify_timing(
    packets: &[Packet],
    time_base: Ratio<i64>,
    r_frame_rate: Option<Ratio<u64>>,
    start: MediaTime,
) -> (Timing, MediaTime) {
    let time_at = |ticks: i64| Ratio::from_integer(ticks) * time_base;

    if let Some(fps) = r_frame_rate.and_then(|r| Fps::new(*r.numer(), *r.denom())) {
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

    // Variable: keep every presentation timestamp exactly.
    let timestamps = packets
        .iter()
        .map(|p| MediaTime(time_at(p.pts)))
        .collect();
    let last = packets.last().expect("packets is non-empty");
    let end = match last.duration {
        Some(duration) => MediaTime(time_at(last.pts + duration)),
        // No per-frame duration: fall back to the last packet's own timestamp.
        None => MediaTime(time_at(last.pts)),
    };
    (Timing::Variable(timestamps), end)
}

fn build_audio(stream: &StreamInfo, ordinal: u8) -> eyre::Result<AudioMetadata> {
    let Some(sample_rate) = stream.sample_rate else {
        bail!("audio stream {} is missing sample rate", stream.index);
    };
    Ok(AudioMetadata {
        sample_rate,
        length: stream.duration.unwrap_or(0.0),
        stream_index: stream.index,
        audio_stream_index: ordinal,
        parent_stream_index: None,
    })
}

/// `Ratio<u64>` → `Ratio<i64>`, for mixing durations into signed media time.
fn to_i64_ratio(r: Ratio<u64>) -> Ratio<i64> {
    Ratio::new(*r.numer() as i64, *r.denom() as i64)
}

/// Parse `"num/den"` into a positive rational, or `None` if either side is zero
/// (ffprobe reports `0/0` for a missing rate) or unparseable.
fn parse_ratio_u64(s: &str) -> Option<Ratio<u64>> {
    let (num, den) = s.split_once('/')?;
    let num: u64 = num.trim().parse().ok()?;
    let den: u64 = den.trim().parse().ok()?;
    (num != 0 && den != 0).then(|| Ratio::new(num, den))
}

/// Parse a `"num/den"` timebase into a signed rational. Timebases are always
/// well-formed with a nonzero denominator.
fn parse_ratio_i64(s: &str) -> Option<Ratio<i64>> {
    let (num, den) = s.split_once('/')?;
    let num: i64 = num.trim().parse().ok()?;
    let den: i64 = den.trim().parse().ok()?;
    (den != 0).then(|| Ratio::new(num, den))
}

/// `"N/A"` (or an absent field) maps to `None`.
fn field<'a>(map: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    match map.get(key).map(String::as_str) {
        Some("N/A") | None => None,
        Some(value) => Some(value),
    }
}

/// Run `ffprobe -v error <args> <path>` and return stdout.
fn run_ffprobe(args: &[&str], path: &str) -> eyre::Result<String> {
    let output = Command::new(ffprobe_path())
        .args(["-v", "error"])
        .args(args)
        .arg(path)
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .wrap_err("running ffprobe")?;
    ensure!(
        output.status.success(),
        "ffprobe failed ({}): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    );
    String::from_utf8(output.stdout).wrap_err("ffprobe output was not UTF-8")
}

/// Stream-level probe. Uses the keyed `default` format (not csv): csv drops
/// fields that don't apply to a stream type, sliding the remaining columns, so
/// an audio `sample_rate` would masquerade as a video `width`. mpegts also lists
/// each stream twice (once inside its `[PROGRAM]`), so streams are deduplicated
/// by their container index.
fn probe_streams(path: &str) -> eyre::Result<Vec<StreamInfo>> {
    let text = run_ffprobe(
        &[
            "-show_entries",
            "stream=index,codec_type,width,height,r_frame_rate,time_base,sample_rate,duration:stream_disposition=attached_pic",
            "-of",
            "default",
        ],
        path,
    )?;

    let mut streams = Vec::new();
    let mut seen = HashMap::new();
    let mut current: Option<HashMap<String, String>> = None;
    for line in text.lines() {
        let line = line.trim();
        match line {
            "[STREAM]" => current = Some(HashMap::new()),
            "[/STREAM]" => {
                if let Some(map) = current.take() {
                    push_stream(map, &mut streams, &mut seen)?;
                }
            }
            _ => {
                if let (Some(map), Some((key, value))) = (current.as_mut(), line.split_once('=')) {
                    map.insert(key.to_owned(), value.to_owned());
                }
            }
        }
    }
    Ok(streams)
}

fn push_stream(
    map: HashMap<String, String>,
    streams: &mut Vec<StreamInfo>,
    seen: &mut HashMap<u8, ()>,
) -> eyre::Result<()> {
    let index: u8 = field(&map, "index")
        .ok_or_else(|| eyre::eyre!("stream missing index"))?
        .parse()
        .wrap_err("stream index out of range")?;
    if seen.insert(index, ()).is_some() {
        return Ok(()); // mpegts program duplicate
    }
    let codec_type = field(&map, "codec_type").unwrap_or("").to_owned();
    let time_base = field(&map, "time_base")
        .and_then(parse_ratio_i64)
        .unwrap_or_else(|| Ratio::from_integer(1));
    streams.push(StreamInfo {
        index,
        codec_type,
        width: field(&map, "width").and_then(|v| v.parse().ok()),
        height: field(&map, "height").and_then(|v| v.parse().ok()),
        r_frame_rate: field(&map, "r_frame_rate").and_then(parse_ratio_u64),
        time_base,
        sample_rate: field(&map, "sample_rate").and_then(|v| v.parse().ok()),
        duration: field(&map, "duration").and_then(|v| v.parse().ok()),
        attached_pic: field(&map, "DISPOSITION:attached_pic") == Some("1"),
    });
    Ok(())
}

/// Per-packet probe of every video stream, grouped by container stream index.
///
/// `-show_packets` demuxes without decoding, so it's cheap — no full-file
/// decode. Each packet carries a presentation timestamp, a display duration, and
/// a keyframe flag, which is everything the timing and keyframe math needs. csv
/// is safe here: the requested fields are always present for video packets, so
/// any trailing columns can be ignored by position.
fn probe_video_packets(path: &str) -> eyre::Result<HashMap<u8, Vec<Packet>>> {
    let text = run_ffprobe(
        &[
            "-select_streams",
            "v",
            "-show_entries",
            "packet=stream_index,pts,duration,flags",
            "-of",
            "csv=p=0",
        ],
        path,
    )?;

    let mut by_stream: HashMap<u8, Vec<Packet>> = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut fields = line.split(',');
        let stream_index: u8 = match fields.next().and_then(|v| v.parse().ok()) {
            Some(index) => index,
            None => continue,
        };
        let pts = match fields.next().and_then(parse_i64_field) {
            Some(pts) => pts,
            // A packet without a presentation timestamp can't be placed; skip it.
            None => continue,
        };
        let duration = fields.next().and_then(parse_i64_field);
        // ffprobe renders packet flags as e.g. `K__`; `K` marks a keyframe.
        let key_frame = fields.next().is_some_and(|flags| flags.contains('K'));
        by_stream.entry(stream_index).or_default().push(Packet {
            pts,
            duration,
            key_frame,
        });
    }
    Ok(by_stream)
}

fn parse_i64_field(s: &str) -> Option<i64> {
    s.parse().ok()
}
