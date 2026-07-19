use std::collections::HashSet;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::Instant;

use eyre::{WrapErr as _, ensure};
use ffmpeg_sidecar::paths::ffmpeg_path;
use num_rational::Ratio;

use crate::fixtures::{Codec, Container, Spec, Timing};
use crate::metadata::MediaTime;
use crate::read::{Frame, Resolution};

/// Stamp grid size.
const GRID: u32 = 4;
/// Total bits that can be stamped.
const STAMP_BITS: u32 = GRID * GRID;

/// Name of the plain-vanilla fixture: H.264, yuv420p, GOP 12, 30 fps, mp4.
pub const BASELINE: &str = "h264_420p_g12_30";

/// A frame with `index` stamped as a [`GRID`]x[`GRID`] grid of
/// black/white blocks, MSB first in raster order.
pub fn stamp(resolution: Resolution, index: u32) -> Frame {
    let Resolution { width, height } = resolution;

    assert!(
        index < 1 << STAMP_BITS,
        "index {index} does not fit the stamp"
    );
    let mut data = vec![0u8; 4 * (width * height) as usize];
    for y in 0..height {
        let row = y * GRID / height;
        for x in 0..width {
            let col = x * GRID / width;
            let bit = STAMP_BITS - 1 - (row * GRID + col);
            let color = if index >> bit & 1 == 1 { 255 } else { 0 };
            let i = 4 * (y * width + x) as usize;
            data[i] = color;
            data[i + 1] = color;
            data[i + 2] = color;
            data[i + 3] = 255;
        }
    }

    Frame::new(resolution, data)
}

/// Read the stamped frame index back from a decoded frame.
///
/// Works at any resolution (blocks are sampled by relative position) and for
/// gray, RGB, or RGBA data (channel count inferred from the buffer length;
/// channel 0 is sampled). Each block's central region is averaged and
/// thresholded; a block that lands in the ambiguous middle is an error rather
/// than a guess, so corrupted frames fail loudly.
pub fn recover_index(frame: &Frame) -> eyre::Result<u32> {
    let Resolution { width, height } = frame.resolution();

    let mut index = 0u32;
    for row in 0..GRID {
        for col in 0..GRID {
            // Average over the central half of the block to avoid compresion artifacts.
            let x0 = ((col as f64 + 0.25) / GRID as f64 * width as f64) as u32;
            let x1 = (((col as f64 + 0.75) / GRID as f64 * width as f64) as u32).max(x0 + 1);
            let y0 = ((row as f64 + 0.25) / GRID as f64 * height as f64) as u32;
            let y1 = (((row as f64 + 0.75) / GRID as f64 * height as f64) as u32).max(y0 + 1);

            let mut sum = 0u64;
            let mut count = 0u64;
            for y in y0..y1.min(height) {
                for x in x0..x1.min(width) {
                    sum += u64::from(frame.get(x, y)[0]);
                    count += 1;
                }
            }
            ensure!(count > 0, "block ({row},{col}) sampled no pixels");
            let avg = sum as f64 / count as f64;
            ensure!(
                !(64.0..192.0).contains(&avg),
                "block ({row},{col}) is ambiguous (average luma {avg:.1})"
            );
            index = index << 1 | u32::from(avg >= 128.0);
        }
    }
    Ok(index)
}

// === Generation =============================================================

/// Ensure the spec's file exists in `dir`, encoding it if necessary.
pub(super) fn generate(spec: &Spec, dir: &Path) -> eyre::Result<PathBuf> {
    let path = dir.join(spec.file_name());
    if path.exists() {
        tracing::debug!(name = %spec.name, "fixture already on disk");
        return Ok(path);
    }

    let started = Instant::now();
    // Encode to a temp name and rename into place, so a concurrently running
    // test binary either sees the complete file or none at all.
    let tmp = dir.join(format!(
        ".{}-{}.{}",
        spec.name,
        std::process::id(),
        spec.container.ext()
    ));

    let result = match &spec.timing {
        Timing::Cfr { fps } => encode_cfr(spec, *fps, &tmp),
        Timing::Vfr { durations } => encode_vfr(spec, durations, &tmp),
    };
    if let Err(err) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(err.wrap_err(format!("encoding fixture {}", spec.name)));
    }
    std::fs::rename(&tmp, &path).wrap_err("moving fixture into place")?;

    tracing::info!(
        name = %spec.name,
        codec = spec.codec.encoder(),
        container = spec.container.ext(),
        frames = spec.frames,
        gop = spec.gop,
        bframes = spec.bframes,
        elapsed_ms = started.elapsed().as_millis() as u64,
        "generated fixture"
    );
    Ok(path)
}

/// Constant-frame-rate encode: pipe stamped gray frames into ffmpeg's stdin
/// as rawvideo.
fn encode_cfr(spec: &Spec, fps: Ratio<u64>, out: &Path) -> eyre::Result<()> {
    let mut cmd = Command::new(ffmpeg_path());
    cmd.args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(["-f", "rawvideo", "-pix_fmt", "rgba"])
        .args(["-video_size", &spec.resolution.to_string()])
        .args(["-framerate", &format!("{}/{}", fps.numer(), fps.denom())])
        .args(["-i", "-"]);
    codec_args(spec, &mut cmd);
    output_args(spec, &mut cmd)?;
    cmd.arg(out);

    run_feeding_frames(cmd, spec)
}

/// Variable-frame-rate encode with *exact* per-frame timestamps.
///
/// The obvious concat-demuxer approach doesn't work: it reads image inputs on a
/// fixed 25 fps grid and quantizes every timestamp (a `duration 0.033` frame
/// still lands on a 40 ms boundary). Instead we feed the stamped frames through
/// the image2 demuxer at a fine rate and rewrite each frame's presentation
/// timestamp exactly with a `setpts` lookup — frame `N` maps to its millisecond
/// prefix sum.
///
/// One wrinkle: a frame's display duration is inferred from the *next* frame's
/// timestamp, so the last frame would have none. Pass 1 therefore appends a
/// throwaway sentinel frame at the stream's end, giving the last real frame a
/// successor; pass 2 (`-c copy`, which preserves each sample's stored duration)
/// drops the sentinel by keeping exactly `frames` frames.
fn encode_vfr(spec: &Spec, durations: &[Ratio<u64>], out: &Path) -> eyre::Result<()> {
    ensure!(
        durations.len() == spec.frames as usize,
        "need one duration per frame"
    );
    let staging = out.with_extension("frames");
    let intermediate = out.with_extension("inter.mp4");
    std::fs::create_dir_all(&staging).wrap_err("creating VFR staging dir")?;

    // Clean up staging even on error paths; the closure keeps `?` usable.
    let result = (|| -> eyre::Result<()> {
        // Prefix sums in whole milliseconds: `prefix[k]` is frame `k`'s
        // presentation time, and `prefix[frames]` is the stream's end (where the
        // sentinel goes).
        let mut prefix = vec![0u64];
        for &duration in durations {
            let last = *prefix.last().unwrap();
            prefix.push(last + duration_millis(duration)?);
        }

        // Real frames `f000..`, then one sentinel at `f{frames}`. The sentinel's
        // pixels never survive pass 2, so reuse frame 0's stamp.
        for i in 0..spec.frames {
            write_pgm(&staging, i, &stamp(spec.resolution, i))?;
        }
        write_pgm(&staging, spec.frames, &stamp(spec.resolution, 0))?;

        // `setpts` in a 1 ms timebase: `N -> prefix[N]`. Commas inside the
        // expression are escaped so libavfilter doesn't read them as filter
        // separators; a filter-script file also sidesteps shell quoting.
        let mut filter = String::from("settb=1/1000,setpts=");
        for (n, ms) in prefix.iter().enumerate() {
            if n > 0 {
                filter.push('+');
            }
            write!(filter, "eq(N\\,{n})*{ms}")?;
        }
        let script = staging.join("setpts.txt");
        std::fs::write(&script, &filter)?;

        // Pass 1: encode every frame (real + sentinel), rewriting timestamps.
        let mut cmd = Command::new(ffmpeg_path());
        cmd.args(["-hide_banner", "-loglevel", "error", "-y"])
            .args(["-framerate", "1000"])
            .arg("-i")
            .arg(staging.join("f%03d.pgm"))
            .arg("-filter_script:v")
            .arg(&script);
        codec_args(spec, &mut cmd);
        cmd.args(["-fps_mode", "passthrough", "-video_track_timescale", "1000"])
            .arg(&intermediate);
        run(cmd)?;

        // Pass 2: drop the sentinel by copying exactly `frames` frames. Stream
        // copy keeps each sample's stored duration, so the last real frame
        // retains the duration the sentinel gave it. Container/offset options
        // apply to this real output.
        let mut cmd = Command::new(ffmpeg_path());
        cmd.args(["-hide_banner", "-loglevel", "error", "-y"])
            .arg("-i")
            .arg(&intermediate)
            .args(["-c", "copy"])
            .args(["-frames:v", &spec.frames.to_string()])
            .args(["-video_track_timescale", "1000"]);
        output_args(spec, &mut cmd)?;
        cmd.arg(out);
        run(cmd)
    })();

    if let Err(err) = std::fs::remove_dir_all(&staging) {
        tracing::warn!(dir = %staging.display(), %err, "could not clean up VFR staging dir");
    }
    let _ = std::fs::remove_file(&intermediate);
    result
}

/// Write frame `index` as an 8-bit grayscale PGM (`f{index:03}.pgm`) into `dir`.
fn write_pgm(dir: &Path, index: u32, frame: &Frame) -> eyre::Result<()> {
    let Resolution { width, height } = frame.resolution();
    let mut contents = format!("P5\n{width} {height}\n255\n").into_bytes();
    // PGM (P5) is one byte per pixel; `stamp` returns RGBA, so take channel 0.
    contents.extend(frame.data().iter().step_by(4).copied());
    std::fs::write(dir.join(format!("f{index:03}.pgm")), contents)?;
    Ok(())
}

/// A whole-millisecond duration as an integer count of milliseconds.
fn duration_millis(duration: Ratio<u64>) -> eyre::Result<u64> {
    let ms = duration * Ratio::from_integer(1000);
    ensure!(
        ms.is_integer(),
        "duration {duration}s is not a whole millisecond"
    );
    Ok(ms.to_integer())
}

fn codec_args(spec: &Spec, cmd: &mut Command) {
    let gop = spec.gop.to_string();
    match spec.codec {
        Codec::H264 => {
            cmd.args(["-c:v", "libx264", "-preset", "ultrafast", "-crf", "18"])
                .args(["-g", &gop, "-keyint_min", &gop])
                .args(["-bf", &spec.bframes.to_string()])
                // Exact GOP placement: no scene-cut keyframes.
                .args(["-x264-params", "scenecut=0"]);
        }
        Codec::Hevc => {
            // Ultrafast messes up, for instance, `rand_000000006a5a9ac0_04.ts`. Inspecting the file
            // manually it works fine on playback but if I seek most pixels are suddenly gray.
            cmd.args(["-c:v", "libx265", "-preset", "fast", "-crf", "12"])
                .args([
                    "-x265-params",
                    &format!(
                        "keyint={gop}:min-keyint={gop}:scenecut=0:bframes={}:log-level=error",
                        spec.bframes
                    ),
                ]);
        }
        Codec::Vp9 => {
            cmd.args([
                "-c:v",
                "libvpx-vp9",
                "-deadline",
                "realtime",
                "-cpu-used",
                "8",
            ])
            .args(["-crf", "32", "-b:v", "0"])
            .args(["-g", &gop]);
        }
        Codec::Ffv1 => {
            cmd.args(["-c:v", "ffv1", "-level", "3", "-g", "1"]);
        }
    }
    cmd.args(["-pix_fmt", spec.pix_fmt.ffmpeg_name()]);
}

/// Container/timestamp options: start offsets and mpegts determinism.
fn output_args(spec: &Spec, cmd: &mut Command) -> eyre::Result<()> {
    if spec.container == Container::MpegTs {
        // Kill the mpegts muxer's default ~1.4s preload delay so the start
        // offset below is *exactly* the first PTS, keeping the spec the
        // single source of truth.
        cmd.args(["-muxdelay", "0", "-muxpreload", "0"]);
    }
    if spec.container == Container::Mp4
        && let Timing::Cfr { fps } = &spec.timing
    {
        // A timescale in which both the frame duration (1/fps) and any
        // whole-millisecond start offset are exact: the muxer's default can
        // quantize the offset (e.g. 1.566s became 4009 ticks at 2560 Hz).
        // The VFR pipeline already forces a 1 kHz timescale.
        let num = *fps.numer();
        let timescale = 1000 / gcd(1000, num) * num;
        cmd.args(["-video_track_timescale", &timescale.to_string()]);
    }
    if spec.start_offset != MediaTime(Ratio::from_integer(0)) {
        cmd.args([
            "-output_ts_offset",
            &format_seconds_signed(spec.start_offset.0)?,
        ]);
    }
    Ok(())
}

fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// Format an exact rational second count as a decimal string ffmpeg parses
/// back exactly (ffmpeg time parsing is decimal microseconds, not float).
fn format_seconds_signed(t: Ratio<i64>) -> eyre::Result<String> {
    let micros = t * Ratio::from_integer(1_000_000);
    ensure!(
        micros.is_integer(),
        "time {t} is not representable in whole microseconds"
    );
    let micros = micros.to_integer();
    Ok(format!("{}.{:06}", micros / 1_000_000, micros % 1_000_000))
}

/// Spawn ffmpeg, feed stamped frames on stdin, and surface stderr on failure.
fn run_feeding_frames(mut cmd: Command, spec: &Spec) -> eyre::Result<()> {
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().wrap_err("spawning ffmpeg")?;

    let mut stdin = child.stdin.take().expect("stdin was piped");
    let write_result =
        (0..spec.frames).try_for_each(|i| stdin.write_all(&stamp(spec.resolution, i).data()));
    drop(stdin);

    let output = child.wait_with_output().wrap_err("waiting for ffmpeg")?;
    ensure!(
        output.status.success(),
        "ffmpeg failed ({}): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    );
    // A broken pipe with a zero exit would be bizarre; don't swallow it.
    write_result.wrap_err("writing frames to ffmpeg stdin")?;
    Ok(())
}

/// Run an ffmpeg command with no stdin, surfacing stderr on failure.
fn run(mut cmd: Command) -> eyre::Result<()> {
    let output = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .wrap_err("running ffmpeg")?;
    ensure!(
        output.status.success(),
        "ffmpeg failed ({}): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}

// === Derived edge-case fixtures =============================================
//
// One-off files that don't fit the `Spec` matrix but exercise real-world
// metadata quirks: a trimming edit list, an audio track, and cover art. Each is
// built from the baseline clip and cached on disk like the generated specs.

/// Build all derived edge files under `<dir>/edge/`, returning
/// `(registry name, path)` pairs.
pub(super) fn derived_edge_files(dir: &Path, baseline: &Path) -> Vec<(String, PathBuf)> {
    [
        ("trimmed", "trimmed.mp4", build_trimmed as BuildFn),
        ("with_audio", "with_audio.mp4", build_with_audio),
        ("with_cover", "with_cover.mp4", build_with_cover_art),
    ]
    .into_iter()
    .map(|(name, file, build)| (name.to_owned(), derived(dir, baseline, file, build)))
    .collect()
}

type BuildFn = fn(&Path, &Path) -> eyre::Result<()>;

/// Build `<dir>/edge/<file>` from the baseline once, caching it on disk.
/// `build` receives the baseline path and the output path.
fn derived(dir: &Path, baseline: &Path, file: &str, build: BuildFn) -> PathBuf {
    let dir = dir.join("edge");
    std::fs::create_dir_all(&dir).expect("could not create edge-fixtures directory");
    let out = dir.join(file);
    if out.exists() {
        return out;
    }

    // Temp-plus-rename so concurrent test binaries never see a half-written file.
    let tmp = dir.join(format!(".{}-{file}", std::process::id()));
    let result = build(baseline, &tmp);
    if let Err(err) = result {
        let _ = std::fs::remove_file(&tmp);
        panic!("building derived fixture {file}: {err}");
    }
    std::fs::rename(&tmp, &out).expect("moving derived fixture into place");
    out
}

/// Baseline trimmed to a non-keyframe start, producing an mp4 with a *trimming
/// edit list*: the frames before the cut stay in the file (as discard-flagged
/// packets with pre-roll timestamps) but never present. The seek lands mid-GOP,
/// so some frames are discarded and the first presented frame is not a keyframe.
fn build_trimmed(base: &Path, out: &Path) -> eyre::Result<()> {
    let mut cmd = Command::new(ffmpeg_path());
    cmd.args(["-hide_banner", "-loglevel", "error", "-y", "-ss", "0.2"])
        .arg("-i")
        .arg(base)
        .args(["-map", "0:v:0", "-c", "copy"])
        .arg(out);
    run(cmd)
}

/// Baseline with a stereo 44.1 kHz silent AAC audio track muxed in.
fn build_with_audio(base: &Path, out: &Path) -> eyre::Result<()> {
    let mut cmd = Command::new(ffmpeg_path());
    cmd.args(["-hide_banner", "-loglevel", "error", "-y"])
        .arg("-i")
        .arg(base)
        .args(["-f", "lavfi", "-i", "anullsrc=r=44100:cl=stereo"])
        .args(["-map", "0:v", "-map", "1:a"])
        .args(["-c:v", "copy", "-c:a", "aac", "-shortest"])
        .arg(out);
    run(cmd)
}

/// Baseline with a still image muxed in as cover art (an `attached_pic`
/// disposition video stream — a single frame, not a real track).
fn build_with_cover_art(base: &Path, out: &Path) -> eyre::Result<()> {
    let cover = out.with_file_name(".cover.png");
    let mut mk = Command::new(ffmpeg_path());
    mk.args(["-hide_banner", "-loglevel", "error", "-y"])
        .args([
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=64x64:d=1",
            "-frames:v",
            "1",
        ])
        .arg(&cover);
    run(mk)?;

    let mut cmd = Command::new(ffmpeg_path());
    cmd.args(["-hide_banner", "-loglevel", "error", "-y"])
        .arg("-i")
        .arg(base)
        .arg("-i")
        .arg(&cover)
        .args(["-map", "0:v", "-map", "1:v", "-c", "copy"])
        .args(["-disposition:v:1", "attached_pic"])
        .arg(out);
    let result = run(cmd);
    let _ = std::fs::remove_file(&cover);
    result
}

pub(super) fn encoder_available(name: &str) -> bool {
    static ENCODERS: OnceLock<HashSet<String>> = OnceLock::new();
    ENCODERS
        .get_or_init(|| {
            let output = Command::new(ffmpeg_path())
                .args(["-hide_banner", "-encoders"])
                .output();
            match output {
                Ok(output) => String::from_utf8_lossy(&output.stdout)
                    .lines()
                    // Lines look like ` V....D libx264  H.264 / ...`.
                    .filter_map(|line| Some(line.split_whitespace().nth(1)?.to_owned()))
                    .collect(),
                Err(err) => {
                    tracing::error!(%err, "could not list ffmpeg encoders");
                    HashSet::new()
                }
            }
        })
        .contains(name)
}
