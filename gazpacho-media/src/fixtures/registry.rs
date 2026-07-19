//! The unified registry of every test video, across all kinds.
//!
//! Each kind is an independent build stage that appends [`TestVideo`]s, so new
//! kinds (e.g. a downloaded reference corpus) drop in without touching the
//! rest. Capability is expressed by `spec`: `Some` means the clip went through
//! the stamping pipeline and carries exact ground truth; `None` means only
//! self-consistency properties apply.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Instant;

use rand::SeedableRng as _;
use rand::rngs::StdRng;
use rand::seq::SliceRandom as _;

use crate::fixtures::VERSION;
use crate::fixtures::chromium;
use crate::fixtures::generation::{self, BASELINE};
use crate::fixtures::random::random_specs;
use crate::fixtures::spec::{Spec, all_specs};

/// Default master seed for the random kind: fixed so the on-disk cache stays
/// warm across runs. Override with `GAZPACHO_RANDOM_SEED` to explore new
/// parameter combinations.
const DEFAULT_RANDOM_SEED: u64 = 0x6A5A_9AC0;
/// Default number of random specs; override with `GAZPACHO_RANDOM_COUNT`.
const DEFAULT_RANDOM_COUNT: u32 = 8;

/// File extensions treated as videos, for `GAZPACHO_REAL_VIDEOS_DIR` and the
/// Chromium corpus alike.
pub(super) const REAL_EXTENSIONS: &[&str] =
    &["mp4", "mkv", "webm", "mov", "ts", "m4v", "avi", "ogv"];

/// Always generated and always part of any sample: tests target these by name
/// via [`Registry::expect`].
const PINNED: &[&str] = &[
    BASELINE,
    "h264_420p_g250_30",
    "vfr_h264",
    "h264_bf2",
    "h264_bf2_offset",
    "h264_bf2_ts",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The fixed spec matrix.
    Synthetic,
    /// Seeded randomly-parameterized specs (same stamping pipeline).
    Random,
    /// Edge-case files derived from the baseline (trim, audio, cover art).
    Derived,
    /// The Chromium `media/test/data` corpus, downloaded and cached.
    Chromium,
    /// User-supplied files from `GAZPACHO_REAL_VIDEOS_DIR`.
    RealWorld,
}

/// One video available to tests.
#[derive(Debug)]
pub struct TestVideo {
    pub name: String,
    pub path: PathBuf,
    /// Ground truth, when the clip was generated through the stamping
    /// pipeline. `Some` implies every frame carries a recoverable index stamp.
    pub spec: Option<Spec>,
    pub kind: Kind,
}

impl TestVideo {
    /// Path as `&str`, the form the reader API takes.
    pub fn path_str(&self) -> &str {
        self.path.to_str().expect("fixture paths are valid UTF-8")
    }

    /// The spec, panicking with the video's name if it has none.
    pub fn expect_spec(&self) -> &Spec {
        self.spec
            .as_ref()
            .unwrap_or_else(|| panic!("{} has no spec (kind {:?})", self.name, self.kind))
    }
}

#[derive(Debug)]
pub struct Registry {
    pub dir: PathBuf,
    videos: Vec<TestVideo>,
    /// Indices selected by `GAZPACHO_TEST_VIDEOS`; every index when full.
    sampled: Vec<usize>,
}

impl Registry {
    /// The sampled selection — what property tests iterate.
    pub fn all(&self) -> impl Iterator<Item = &TestVideo> {
        self.sampled.iter().map(|&i| &self.videos[i])
    }

    /// Every registered video, ignoring sampling (for oracle self-tests).
    pub fn all_full(&self) -> &[TestVideo] {
        &self.videos
    }

    /// The sampled videos that carry ground truth.
    pub fn spec_backed(&self) -> impl Iterator<Item = (&TestVideo, &Spec)> {
        self.all().filter_map(|v| Some((v, v.spec.as_ref()?)))
    }

    /// Lookup by name; ignores sampling so targeted tests stay total.
    pub fn get(&self, name: &str) -> Option<&TestVideo> {
        self.videos.iter().find(|v| v.name == name)
    }

    /// Fetch by name, for tests targeting a specific video. Panics if not
    /// found.
    pub fn expect(&self, name: &str) -> &TestVideo {
        self.get(name)
            .unwrap_or_else(|| panic!("video {name:?} missing from registry (generation failed?)"))
    }

    pub fn baseline(&self) -> &TestVideo {
        self.expect(BASELINE)
    }
}

/// The registry of all test videos, generating any missing files on first
/// call.
///
/// Lazy and cached: within a process via `OnceLock`, across processes via the
/// files themselves (generation is skipped when the file already exists, and
/// writes are tempfile-plus-rename so concurrent test binaries can't observe
/// half-written fixtures). Specs whose encoder is missing from the local
/// ffmpeg are skipped with a warning rather than failing the registry.
pub fn videos() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../target/synthetic-fixtures")
            .join(VERSION);
        std::fs::create_dir_all(&dir).expect("could not create fixtures directory");

        let started = Instant::now();
        let mut videos = Vec::new();

        // Stage 1: the fixed matrix and the seeded random specs, through the
        // same stamped generation pipeline.
        generate_specs(&dir, all_specs(), Kind::Synthetic, &mut videos);
        generate_specs(
            &dir,
            random_specs(random_seed(), random_count()),
            Kind::Random,
            &mut videos,
        );

        // Stage 2: edge cases derived from the baseline. Registered spec-less
        // so the universal property suite covers them automatically.
        let baseline = videos
            .iter()
            .find(|v| v.name == BASELINE)
            .expect("baseline must generate")
            .path
            .clone();
        for (name, path) in generation::derived_edge_files(&dir, &baseline) {
            videos.push(TestVideo {
                name,
                path,
                spec: None,
                kind: Kind::Derived,
            });
        }

        // Stage 3: the Chromium media test corpus, downloaded once and
        // validated by decode (see the `chromium` module). Spec-less, and
        // subject to sampling like the generated kinds.
        for (name, path) in chromium::corpus_files() {
            videos.push(TestVideo {
                name,
                path,
                spec: None,
                kind: Kind::Chromium,
            });
        }

        // Stage 4: user-supplied real-world files.
        scan_real_videos(&mut videos);

        let sampled = sample(&videos);
        tracing::info!(
            count = videos.len(),
            sampled = sampled.len(),
            dir = %dir.display(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "test-video registry ready"
        );
        Registry {
            dir,
            videos,
            sampled,
        }
    })
}

/// Baseline trimmed to a non-keyframe start (trimming edit list).
pub fn trimmed_baseline() -> PathBuf {
    videos().expect("trimmed").path.clone()
}

/// Baseline with a silent AAC audio track.
pub fn baseline_with_audio() -> PathBuf {
    videos().expect("with_audio").path.clone()
}

/// Baseline with a still image attached as cover art.
pub fn baseline_with_cover_art() -> PathBuf {
    videos().expect("with_cover").path.clone()
}

fn generate_specs(dir: &Path, specs: Vec<Spec>, kind: Kind, videos: &mut Vec<TestVideo>) {
    for spec in specs {
        if !generation::encoder_available(spec.codec.encoder()) {
            tracing::warn!(
                name = %spec.name,
                encoder = spec.codec.encoder(),
                "skipping fixture: encoder not available in this ffmpeg build"
            );
            continue;
        }
        match generation::generate(&spec, dir) {
            Ok(path) => videos.push(TestVideo {
                name: spec.name.clone(),
                path,
                spec: Some(spec),
                kind,
            }),
            Err(err) => {
                tracing::error!(name = %spec.name, %err, "failed to generate fixture")
            }
        }
    }
}

fn scan_real_videos(videos: &mut Vec<TestVideo>) {
    let Ok(dir) = std::env::var("GAZPACHO_REAL_VIDEOS_DIR") else {
        return;
    };
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(err) => {
            tracing::warn!(%dir, %err, "cannot read GAZPACHO_REAL_VIDEOS_DIR; skipping");
            return;
        }
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let is_video = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| REAL_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()));
        if !path.is_file() || !is_video {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unnamed");
        videos.push(TestVideo {
            name: format!("real_{stem}"),
            path: path.clone(),
            spec: None,
            kind: Kind::RealWorld,
        });
    }
}

fn random_seed() -> u64 {
    parse_env("GAZPACHO_RANDOM_SEED", DEFAULT_RANDOM_SEED)
}

fn random_count() -> u32 {
    parse_env("GAZPACHO_RANDOM_COUNT", DEFAULT_RANDOM_COUNT)
}

fn parse_env<T: std::str::FromStr>(var: &str, default: T) -> T {
    match std::env::var(var) {
        Ok(value) => value
            .parse()
            .unwrap_or_else(|_| panic!("{var}={value:?} is not valid")),
        Err(_) => default,
    }
}

/// Selected indices per `GAZPACHO_TEST_VIDEOS` (`full` | `sample:<N>` |
/// `sample:<N>:<seed>`). Samples are deterministic, always include the pinned
/// names and every real-world video, and log their contents so failures are
/// reproducible.
fn sample(videos: &[TestVideo]) -> Vec<usize> {
    let full = || (0..videos.len()).collect();
    let var = match std::env::var("GAZPACHO_TEST_VIDEOS") {
        Ok(var) => var,
        Err(_) => return full(),
    };
    let invalid = || -> ! {
        panic!(
            "GAZPACHO_TEST_VIDEOS={var:?} is not valid \
             (use `full`, `sample:<N>`, or `sample:<N>:<seed>`)"
        )
    };

    let (n, seed): (usize, u64) = match var.split(':').collect::<Vec<_>>()[..] {
        ["full"] => return full(),
        ["sample", n] => (n.parse().unwrap_or_else(|_| invalid()), 0),
        ["sample", n, seed] => (
            n.parse().unwrap_or_else(|_| invalid()),
            seed.parse().unwrap_or_else(|_| invalid()),
        ),
        _ => invalid(),
    };

    let mut indices: Vec<usize> = (0..videos.len()).collect();
    indices.shuffle(&mut StdRng::seed_from_u64(seed));
    let mut selected: Vec<usize> = indices
        .into_iter()
        .filter(|&i| {
            // Pinned and real-world videos are added unconditionally below.
            !PINNED.contains(&videos[i].name.as_str()) && videos[i].kind != Kind::RealWorld
        })
        .take(n)
        .collect();
    selected.extend(
        videos
            .iter()
            .enumerate()
            .filter(|(_, v)| PINNED.contains(&v.name.as_str()) || v.kind == Kind::RealWorld)
            .map(|(i, _)| i),
    );
    selected.sort_unstable();

    let names: Vec<&str> = selected.iter().map(|&i| videos[i].name.as_str()).collect();
    tracing::info!(n, seed, ?names, "sampled test-video subset");
    selected
}
