//! Oracle self-tests: validate the fixtures themselves (registry integrity
//! and the stamp's survival through encode → independent decode), without
//! involving any consumer of this crate.
//!
//! Custom libtest-mimic harness: the registry is enumerated (generating any
//! missing files — the "build step") before trials run, and every spec-backed
//! video becomes its own parallel, individually-filterable test case. Works
//! under both `cargo test` and `cargo nextest run`.

use eyre::{WrapErr as _, ensure};
use gazpacho_fixtures::{BASELINE, Registry, TestVideo, decode_all_rgba, videos};
use libtest_mimic::{Arguments, Trial};

fn main() {
    let args = Arguments::from_args();
    gazpacho_fixtures::init_tracing_stderr();
    let registry = videos();

    let mut trials = vec![Trial::test("registry_sanity", move || {
        registry_sanity(registry).map_err(|err| format!("{err:?}").into())
    })];
    for video in registry.all_full().iter().filter(|v| v.spec.is_some()) {
        trials.push(Trial::test(
            format!("stamp_survives_encode_and_decode::{}", video.name),
            move || stamp_survives(video).map_err(|err| format!("{err:?}").into()),
        ));
    }
    libtest_mimic::run(&args, trials).exit();
}

fn registry_sanity(registry: &Registry) -> eyre::Result<()> {
    let generated: Vec<_> = registry
        .all_full()
        .iter()
        .filter(|v| v.spec.is_some())
        .collect();
    ensure!(
        generated.len() >= 40,
        "expected the full matrix, got {} fixtures",
        generated.len()
    );
    for video in registry.all_full() {
        let size = std::fs::metadata(&video.path)
            .wrap_err_with(|| format!("{} missing", video.path.display()))?
            .len();
        ensure!(size > 0, "{} is empty", video.name);
    }
    // Names must be unique: they key lookups and label failures.
    let mut names: Vec<_> = registry.all_full().iter().map(|v| &v.name).collect();
    names.sort_unstable();
    names.dedup();
    ensure!(
        names.len() == registry.all_full().len(),
        "duplicate names"
    );
    // Targeted lookups tests rely on must always exist, even in samples.
    for name in [
        BASELINE,
        "h264_420p_g250_30",
        "vfr_h264",
        "h264_bf2",
        "h264_bf2_offset",
        "h264_bf2_ts",
        "trimmed",
        "with_audio",
        "with_cover",
    ] {
        registry.expect(name)?;
    }
    Ok(())
}

/// The core oracle check: after a full encode → decode round trip through an
/// *independent* ffmpeg pipe, every frame still announces its index, in
/// presentation order. Covers the lossiest codec, VFR, B-frame reordering,
/// and the seeded random specs.
fn stamp_survives(video: &TestVideo) -> eyre::Result<()> {
    let spec = video.expect_spec()?;
    let frames = decode_all_rgba(&video.path, spec.resolution)?;
    ensure!(
        frames.len() == spec.frames as usize,
        "expected {} frames, got {}",
        spec.frames,
        frames.len()
    );
    for (i, frame) in frames.iter().enumerate() {
        let recovered = frame
            .recover_index()
            .wrap_err_with(|| format!("frame {i}"))?;
        ensure!(
            recovered == i as u32,
            "frame {i}: expected index {i}, got {recovered}"
        );
    }
    Ok(())
}
