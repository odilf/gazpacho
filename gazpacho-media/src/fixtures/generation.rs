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

use crate::fixtures::{Codec, Container, Spec, Timing, VERSION, spec::all_specs};
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

/// A generated clip on disk plus the spec that predicts its exact contents.
#[derive(Debug)]
pub struct Fixture {
    pub spec: Spec,
    pub path: PathBuf,
}

impl Fixture {
    /// Path as `&str`, the form the reader API takes.
    pub fn path_str(&self) -> &str {
        self.path.to_str().expect("fixture paths are valid UTF-8")
    }
}

#[derive(Debug)]
pub struct Corpus {
    pub dir: PathBuf,
    pub fixtures: Vec<Fixture>,
}

impl Corpus {
    pub fn all(&self) -> &[Fixture] {
        &self.fixtures
    }

    pub fn get(&self, name: &str) -> Option<&Fixture> {
        self.fixtures.iter().find(|f| f.spec.name == name)
    }

    /// Fetch by name, panicking with a clear message — for tests targeting a
    /// specific fixture.
    pub fn expect(&self, name: &str) -> &Fixture {
        self.get(name)
            .unwrap_or_else(|| panic!("fixture {name:?} missing from corpus (generation failed?)"))
    }

    pub fn baseline(&self) -> &Fixture {
        self.expect(BASELINE)
    }
}

/// The synthetic corpus, generating any missing files on first call.
///
/// Lazy and cached: within a process via `OnceLock`, across processes via the
/// files themselves (generation is skipped when the file already exists, and
/// writes are tempfile-plus-rename so concurrent test binaries can't observe
/// half-written fixtures). Specs whose encoder is missing from the local
/// ffmpeg are skipped with a warning rather than failing the corpus.
pub fn corpus() -> &'static Corpus {
    static CORPUS: OnceLock<Corpus> = OnceLock::new();
    CORPUS.get_or_init(|| {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../target/synthetic-fixtures")
            .join(VERSION);
        std::fs::create_dir_all(&dir).expect("could not create fixtures directory");

        let started = Instant::now();
        let mut fixtures = Vec::new();
        for spec in all_specs() {
            if !encoder_available(spec.codec.encoder()) {
                tracing::warn!(
                    name = %spec.name,
                    encoder = spec.codec.encoder(),
                    "skipping fixture: encoder not available in this ffmpeg build"
                );
                continue;
            }
            match generate(&spec, &dir) {
                Ok(path) => fixtures.push(Fixture { spec, path }),
                Err(err) => {
                    tracing::error!(name = %spec.name, %err, "failed to generate fixture")
                }
            }
        }
        tracing::info!(
            count = fixtures.len(),
            dir = %dir.display(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "synthetic fixture corpus ready"
        );
        Corpus { dir, fixtures }
    })
}

/// Ensure the spec's file exists in `dir`, encoding it if necessary.
fn generate(spec: &Spec, dir: &Path) -> eyre::Result<PathBuf> {
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

/// Variable-frame-rate encode: stamped frames as PGM files driven by a concat
/// list with exact per-frame durations, muxed with `-fps_mode vfr` so the
/// output keeps the irregular timestamps.
fn encode_vfr(spec: &Spec, durations: &[Ratio<u64>], out: &Path) -> eyre::Result<()> {
    ensure!(
        durations.len() == spec.frames as usize,
        "need one duration per frame"
    );
    let staging = out.with_extension("frames");
    std::fs::create_dir_all(&staging).wrap_err("creating VFR staging dir")?;

    // Clean up staging even on error paths; the closure keeps `?` usable.
    let result = (|| -> eyre::Result<()> {
        let Resolution { width, height } = spec.resolution;
        let mut list = String::from("ffconcat version 1.0\n");
        for i in 0..spec.frames {
            let pgm = format!("f{i:03}.pgm");
            let mut contents = format!("P5\n{width} {height}\n255\n").into_bytes();
            // PGM (P5) is 8-bit grayscale: one byte per pixel. `stamp` returns
            // RGBA (4 bytes/pixel) where R=G=B=color, so take channel 0 only.
            contents.extend(stamp(spec.resolution, i).data().iter().step_by(4).copied());
            std::fs::write(staging.join(&pgm), contents)?;
            writeln!(list, "file '{pgm}'")?;
            writeln!(list, "duration {}", format_seconds(durations[i as usize])?)?;
        }
        let list_path = staging.join("list.ffconcat");
        std::fs::write(&list_path, list)?;

        let mut cmd = Command::new(ffmpeg_path());
        cmd.args(["-hide_banner", "-loglevel", "error", "-y"])
            .args(["-f", "concat", "-safe", "0"])
            .arg("-i")
            .arg(&list_path);
        codec_args(spec, &mut cmd);
        // Keep the concat timestamps instead of snapping to a constant rate,
        // and store them in a millisecond timebase so they stay exact.
        cmd.args(["-fps_mode", "vfr", "-video_track_timescale", "1000"]);
        output_args(spec, &mut cmd)?;
        cmd.arg(out);
        run(cmd)
    })();

    if let Err(err) = std::fs::remove_dir_all(&staging) {
        tracing::warn!(dir = %staging.display(), %err, "could not clean up VFR staging dir");
    }
    result
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
            cmd.args(["-c:v", "libx265", "-preset", "ultrafast", "-crf", "20"])
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
    if spec.start_offset != MediaTime(Ratio::from_integer(0)) {
        cmd.args([
            "-output_ts_offset",
            &format_seconds_signed(spec.start_offset.0)?,
        ]);
    }
    Ok(())
}

/// Format an exact rational second count as a decimal string ffmpeg parses
/// back exactly (ffmpeg time parsing is decimal microseconds, not float).
fn format_seconds(t: Ratio<u64>) -> eyre::Result<String> {
    let micros = t * Ratio::from_integer(1_000_000);
    ensure!(
        micros.is_integer(),
        "time {t} is not representable in whole microseconds"
    );
    let micros = micros.to_integer();
    Ok(format!("{}.{:06}", micros / 1_000_000, micros % 1_000_000))
}

/// Like [`format_seconds`] but with a signed time.
// TODO: The implementation is literally the same but writing the trait bounds is annoying.
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

fn encoder_available(name: &str) -> bool {
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
