//! Seeded random [`Spec`]s: the same stamping pipeline as the fixed matrix,
//! but with encoding parameters drawn from a seed. Reproducible — the seed is
//! part of each spec's name, so clips from different seeds coexist in the
//! on-disk cache.

use num_rational::Ratio;
use rand::rngs::StdRng;
use rand::seq::IndexedRandom as _;
use rand::{Rng as _, SeedableRng as _};

use crate::fixtures::{Codec, Container, PixFmt, Spec, Timing};
use crate::metadata::MediaTime;
use crate::read::Resolution;

/// `count` random specs derived from `seed`. Spec `i` gets its own RNG seeded
/// from `seed ^ i`, so changing the count doesn't shift earlier specs.
pub fn random_specs(seed: u64, count: u32) -> Vec<Spec> {
    (0..count)
        .map(|i| random_spec(StdRng::seed_from_u64(seed ^ u64::from(i)), seed, i))
        .collect()
}

/// Containers each codec can be muxed into with the current encode pipeline.
fn allowed_containers(codec: Codec) -> &'static [Container] {
    match codec {
        Codec::H264 | Codec::Hevc => &[Container::Mp4, Container::Mkv, Container::MpegTs],
        Codec::Vp9 => &[Container::WebM, Container::Mkv],
        Codec::Ffv1 => &[Container::Mkv],
    }
}

fn random_spec(mut rng: StdRng, seed: u64, i: u32) -> Spec {
    let codec = *[Codec::H264, Codec::Hevc, Codec::Vp9, Codec::Ffv1]
        .choose(&mut rng)
        .unwrap();
    let container = *allowed_containers(codec).choose(&mut rng).unwrap();
    let pix_fmt = *[PixFmt::Yuv420p, PixFmt::Yuv444p].choose(&mut rng).unwrap();
    // Even dimensions: required for yuv420p chroma subsampling.
    let resolution = Resolution {
        width: rng.random_range(16..=160) * 2,
        height: rng.random_range(12..=120) * 2,
    };
    let frames = rng.random_range(10..=120);

    let cfr_rates = [
        Ratio::from_integer(30),
        Ratio::from_integer(25),
        Ratio::from_integer(60),
        Ratio::from_integer(15),
        Ratio::new(24000, 1001),
        Ratio::new(30000, 1001),
    ];
    // VFR is only proven on non-mpegts muxers (the two-pass pipeline).
    let cfr = container == Container::MpegTs || rng.random_bool(0.7);
    let timing = if cfr {
        Timing::Cfr {
            fps: *cfr_rates.choose(&mut rng).unwrap(),
        }
    } else {
        Timing::Vfr {
            durations: (0..frames)
                .map(|_| Ratio::new(rng.random_range(10..=100), 1000))
                .collect(),
        }
    };

    // Nonzero first PTS: only for the container/timing combos the fixed matrix
    // proves (CFR in mp4/mpegts).
    let offset_ok = cfr && matches!(container, Container::Mp4 | Container::MpegTs);
    let start_offset = if offset_ok && rng.random_bool(0.3) {
        MediaTime(Ratio::new(rng.random_range(100..=2000), 1000))
    } else {
        MediaTime(Ratio::from_integer(0))
    };

    // B-frames only with CFR: the VFR pipeline drops its sentinel frame by
    // keeping exactly `frames` frames, which counts *decode* order — B-frame
    // reordering would drop a real frame and keep the sentinel.
    let bframes = match codec {
        Codec::H264 | Codec::Hevc if cfr => rng.random_range(0..=3),
        _ => 0,
    };

    Spec {
        name: format!("rand_{seed:016x}_{i:02}"),
        codec,
        container,
        pix_fmt,
        timing,
        frames,
        resolution,
        gop: rng.random_range(1..=250),
        bframes,
        start_offset,
    }
}
