//! Synthetic video fixtures for testing the reader.
//!
//! Every generated clip encodes its own ground truth: frame `i`'s pixels are a
//! 4x4 grid of black/white blocks spelling `i` in binary ([`stamp`]), so after
//! decoding through *any* path, [`recover_index`] tells you exactly which frame
//! you got, at any resolution, through any lossy codec.
//!
//! Tests assert against [`Spec`] math (which frame *should* be visible at time
//! `t`), never against ffmpeg's opinion of its own output, so we don't end up
//! testing ffmpeg with ffmpeg.
//!
//! No special machinery is needed to generate before tests run: [`corpus`]
//! is lazy (`OnceLock`) and idempotent on disk. Files are stored in
//! `target/synthetic-fixtures/<version>/`. Concurrent test binaries are safe
//! because generation writes to a temp name and `rename`s into place.
//!
//! [`VERSION`] needs to be bumped if generation logic changes.

use std::path::Path;
use std::process::{Command, Stdio};

use eyre::{WrapErr as _, ensure};
use ffmpeg_sidecar::paths::ffmpeg_path;

use crate::MediaReader;
use crate::read::{Frame, Resolution};

/// To invalidate the cache.
const VERSION: &str = "v1";

mod generation;
mod spec;

pub use generation::{BASELINE, Corpus, Fixture, corpus, recover_index, stamp};
pub use spec::{Codec, Container, Spec, Timing};

// === Helpers for tests ======================================================

/// Decode *every* frame of a file to RGBA via a plain ffmpeg pipe, in
/// presentation order.
///
/// This is a decode path independent of [`crate::read`], used to validate the
/// fixtures themselves and as a reference to compare the reader against.
pub fn decode_all_rgba(path: &Path, resolution: Resolution) -> eyre::Result<Vec<Frame>> {
    let Resolution { width, height } = resolution;
    let output = Command::new(ffmpeg_path())
        .args(["-hide_banner", "-loglevel", "error"])
        .arg("-i")
        .arg(path)
        // One output frame per coded frame — otherwise ffmpeg CFR-izes VFR
        // files by duplicating frames to fill the timestamp gaps.
        .args(["-fps_mode", "passthrough"])
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

/// Install a `RUST_LOG`-aware tracing subscriber for tests. Defaults to
/// `gazpacho_media=debug` so fixture generation is visible under `cargo test
/// -- --nocapture`.
#[track_caller]
pub fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("gazpacho_media=debug"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_test_writer()
        .try_init()
        .unwrap_or_else(|err| tracing::warn!(err, "Tracer already initialized."));
}

pub fn reader() -> MediaReader {
    init_tracing();
    MediaReader::default()
}

/// The index stamped in a decoded frame; panics with context if unreadable.
pub fn recovered(fixture: &Fixture, frame: &Frame) -> u32 {
    recover_index(&frame)
        .unwrap_or_else(|err| panic!("{}: unreadable stamp: {err}", fixture.spec.name))
}

// === Self-tests: validate the oracle without involving the reader ==========

#[cfg(test)]
mod tests {
    use crate::read::Resolution;

    use super::*;

    #[test]
    fn stamp_roundtrips_in_memory() {
        let res = Resolution {
            width: 160,
            height: 120,
        };
        for index in [0, 1, 42, 0x5555, 0xAAAA, 0xFFFF] {
            let stamped_frame = stamp(res, index);
            assert_eq!(recover_index(&stamped_frame).unwrap(), index);
        }
    }

    #[test]
    fn corpus_generates() {
        init_tracing();
        let corpus = corpus();
        assert!(
            corpus.all().len() >= 40,
            "expected the full matrix, got {} fixtures",
            corpus.all().len()
        );
        for fixture in corpus.all() {
            let size = std::fs::metadata(&fixture.path)
                .unwrap_or_else(|_| panic!("{} missing", fixture.path.display()))
                .len();
            assert!(size > 0, "{} is empty", fixture.spec.name);
        }
        corpus.baseline(); // Must always exist.
    }

    /// The core oracle check: after a full encode → decode round trip through
    /// an *independent* ffmpeg pipe, every frame still announces its index,
    /// in presentation order. Covers the lossiest codec, VFR, and B-frame
    /// reordering.
    #[test]
    fn stamp_survives_encode_and_decode() {
        init_tracing();
        let corpus = corpus();
        for name in [BASELINE, "vp9_420p_g250_ntsc", "vfr_h264", "h264_bf2_ts"] {
            let fixture = corpus.expect(name);
            let frames = decode_all_rgba(&fixture.path, fixture.spec.resolution).unwrap();
            assert_eq!(frames.len(), fixture.spec.frames as usize, "{name}");
            for (i, frame) in frames.iter().enumerate() {
                let recovered =
                    recover_index(frame).unwrap_or_else(|err| panic!("{name} frame {i}: {err}"));
                assert_eq!(recovered, i as u32, "{name} frame {i}");
            }
        }
    }

    /// Same as [`stamp_survives_encode_and_decode`] but over every fixture. Slower, so opt-in:
    /// `cargo test -p gazpacho-media -- --ignored`
    #[test]
    #[ignore = "full-corpus sweep; run with --ignored"]
    fn stamp_survives_encode_and_decode_full_corpus() {
        init_tracing();
        for fixture in corpus().all() {
            let name = &fixture.spec.name;
            let frames = decode_all_rgba(&fixture.path, fixture.spec.resolution).unwrap();
            assert_eq!(frames.len(), fixture.spec.frames as usize, "{name}");
            for (i, frame) in frames.iter().enumerate() {
                let recovered =
                    recover_index(&frame).unwrap_or_else(|err| panic!("{name} frame {i}: {err}"));
                assert_eq!(recovered, i as u32, "{name} frame {i}");
            }
        }
    }
}
