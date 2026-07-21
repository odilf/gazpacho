//! Test videos for gazpacho, organized as a registry of several [`Kind`]s:
//! - fixed synthetic matrix
//! - seeded random specs
//! - derived edge cases
//! - downloaded Chromium media test corpus (`GAZPACHO_CHROMIUM_VIDEOS=0` to
//!   disable)
//! - and optional real-world files (`GAZPACHO_REAL_VIDEOS_DIR`).
//!
//! Generated clips encode their own "ground truth" by stamping the `i`th frame
//! with a 4x4 [stamping](stamp) of `i` in binary. [`recover_index`] recovers
//! said index, even through any lossy codec. The information is stored in
//! [`Spec`] (says which frame *should* be visible at time `t`, never against
//! ffmpeg's opinion of its own output, so we don't end up testing ffmpeg with
//! ffmpeg).
//!
//! Spec-less videos (derived, real-world) get self-consistency properties only.
//!
//! This crate is independent of `gazpacho-media` to avoid testing itself, so it
//! has simple implementations of [`Frame`], [`Resolution`] and [`Timing`], for
//! instance.
//!
//! `cargo run -p gazpacho-fixtures` loads the (optionally per kind) videos.
//! Files are stored in `target/fixtures/<hash>/`, where `<hash>` digests
//! this crate's sources (see `build.rs`). Therefore editing generation code
//! invalidates the cache automatically (this is also a reason why this is in a
//! separate crate with no path dependencies).
//!
//! Concurrent test binaries are safe because generation writes to a temp name
//! and `rename`s into place.
//!
//! Iteration can be cut down for quick local runs with
//! `GAZPACHO_TEST_VIDEOS=sample:<N>[:<seed>]` (see [`Registry::all`]).

use std::path::Path;
use std::process::{Command, Stdio};

use eyre::{WrapErr as _, ensure};
use ffmpeg_sidecar::paths::ffmpeg_path;

pub mod chromium;
pub mod generation;

mod frame;
mod random;
mod registry;
mod spec;

pub use frame::{Frame, Resolution};
pub use generation::{BASELINE, recover_index, stamp};
pub use registry::{
    Kind, Registry, TestVideo, baseline_with_audio, baseline_with_cover_art, clear_generated,
    fixtures_dir, generate_kind, generation_hash, generation_is_stale, record_generation_hash,
    trimmed_baseline, videos,
};
pub use spec::{Codec, Container, PixFmt, Spec, Timing};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::TestWriter;

/// Decode *every* frame of a file to RGBA via a plain ffmpeg pipe, in
/// presentation order.
///
/// This is a decode path independent of `gazpacho_media::read`, used to
/// validate the fixtures themselves and as a reference to compare the reader
/// against.
pub fn decode_all_rgba(path: &Path, resolution: Resolution) -> eyre::Result<Vec<Frame>> {
    decode_rgba(path, None, resolution, None)
}

/// Like [`decode_all_rgba`], but reads the stream at container index
/// `stream_index` and stops after `limit` frames. Useful for comparing against
/// a reader on multi-track or real-world files too large to hold decoded in
/// memory.
pub fn decode_rgba_prefix(
    path: &Path,
    stream_index: u8,
    resolution: Resolution,
    limit: usize,
) -> eyre::Result<Vec<Frame>> {
    decode_rgba(path, Some(stream_index), resolution, Some(limit))
}

fn decode_rgba(
    path: &Path,
    stream_index: Option<u8>,
    resolution: Resolution,
    limit: Option<usize>,
) -> eyre::Result<Vec<Frame>> {
    let Resolution { width, height } = resolution;
    let mut cmd = Command::new(ffmpeg_path());
    cmd.args(["-hide_banner", "-loglevel", "error"])
        .arg("-i")
        .arg(path)
        // One output frame per coded frame, otherwise ffmpeg CFR-izes VFR.
        .args(["-fps_mode", "passthrough"]);

    if let Some(index) = stream_index {
        cmd.args(["-map", &format!("0:{index}")]);
    }
    if let Some(limit) = limit {
        cmd.args(["-frames:v", &limit.to_string()]);
    }
    let output = cmd
        .args(["-f", "rawvideo", "-pix_fmt", "rgba", "-"])
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .wrap_err("running ffmpeg to decode")?;
    ensure!(
        output.status.success(),
        "ffmpeg decode failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );

    let frame_size = (width * height * 4) as usize;
    ensure!(
        output.stdout.len() % frame_size == 0,
        "decoded byte count {} is not a whole number of {width}x{height} RGBA frames",
        output.stdout.len()
    );
    Ok(output
        .stdout
        .chunks_exact(frame_size)
        .map(|data| Frame::new(resolution, data))
        .collect())
}

/// Tracing for tests compatible
#[track_caller]
pub fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(env_filter())
        .with_writer(TestWriter::with_stderr())
        .try_init()
        .unwrap_or_else(|err| tracing::warn!(err, "Tracer already initialized."));
}

/// Like [`init_tracing`], but writing to stderr. Needed for custom test
/// harnesses.
#[track_caller]
pub fn init_tracing_stderr() {
    tracing_subscriber::fmt()
        .with_env_filter(env_filter())
        .with_writer(std::io::stderr)
        .try_init()
        .unwrap_or_else(|err| tracing::warn!(err, "Tracer already initialized."));
}

fn env_filter() -> EnvFilter {
    EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("gazpacho_fixtures=warn,gazpacho_media=debug"))
}
