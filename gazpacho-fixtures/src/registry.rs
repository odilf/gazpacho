//! The unified registry of every test video, across all kinds.
//!
//! Each kind is an independent build stage ([`generate_kind`]) that yields
//! [`TestVideo`]s, so new kinds (e.g. a downloaded reference corpus) drop in
//! without touching the rest — and the CLI (`src/main.rs`) can run any stage
//! on its own. Capability is expressed by `spec`: `Some` means the clip went
//! through the stamping pipeline and carries exact ground truth; `None` means
//! only self-consistency properties apply.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Instant;

use rand::SeedableRng as _;
use rand::rngs::StdRng;
use rand::seq::SliceRandom as _;

use crate::chromium;
use crate::generation::{self, BASELINE};
use crate::random;
use crate::spec::{Spec, all_specs};

/// File extensions treated as videos, for `GAZPACHO_REAL_VIDEOS_DIR` and the
/// Chromium corpus alike.
pub(crate) const REAL_EXTENSIONS: &[&str] =
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

/// The single root every fixture lives under: generated clips directly
/// inside, derived edge cases in `edge/`, the Chromium corpus in `chromium/`.
///
/// The directory name is *stable* across code changes. Staleness of the
/// generated clips is tracked by [`generation_hash`] recorded in a file inside
/// (see [`generation_is_stale`]); when the code changes we overwrite the
/// generated files in place rather than spawning a new hash-named directory.
/// The Chromium corpus tracks its own pinned commit the same way (a text file,
/// not the code hash — a `stamp()` tweak must not trigger an 80 MB
/// re-download; see [`chromium`]).
pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/fixtures")
}

/// The generation-code hash from `build.rs` (digest of this crate's sources).
pub fn generation_hash() -> &'static str {
    env!("GAZPACHO_FIXTURES_HASH")
}

fn hash_file(dir: &Path) -> PathBuf {
    dir.join(".generation-hash")
}

/// Whether the generated fixtures on disk predate the current generation code
/// (the recorded hash is missing or differs). The caller regenerates with
/// overwrite and then calls [`record_generation_hash`].
///
/// This latches the decision at the start of a run: a concurrent process that
/// starts *after* another has finished regenerating (and rewritten the hash)
/// will see a match and safely reuse the now-current files; a process that
/// started earlier keeps overwriting with identical bytes. Both are safe
/// because every write is temp-plus-rename.
pub fn generation_is_stale(dir: &Path) -> bool {
    match fs::read_to_string(hash_file(dir)) {
        Ok(recorded) => recorded.trim() != generation_hash(),
        Err(_) => true,
    }
}

/// Record the current generation hash, marking the generated fixtures on disk
/// as current. Only call after regenerating the *complete* generated set
/// (matrix, random, derived) — recording it while any generated kind is still
/// stale would make later runs wrongly skip regeneration.
pub fn record_generation_hash(dir: &Path) {
    let tmp = dir.join(format!(".generation-hash-{}", std::process::id()));
    let written =
        fs::write(&tmp, generation_hash()).and_then(|()| fs::rename(&tmp, hash_file(dir)));
    if let Err(err) = written {
        tracing::warn!(%err, "could not record generation hash");
        let _ = fs::remove_file(&tmp);
    }
}

/// Remove every generated fixture (matrix, random, derived, and the hash
/// marker) while leaving the Chromium corpus untouched — for the CLI's
/// `--force`. Single-process; concurrent test binaries never call this.
pub fn clear_generated(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        if entry.file_name() == "chromium" {
            continue;
        }
        let path = entry.path();
        let _ = if path.is_dir() {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
    }
}

/// Generate (or discover) every video of one kind, into/under `dir` for the
/// generated kinds. Idempotent on disk; safe to run concurrently with other
/// processes thanks to temp-plus-rename writes. `overwrite` re-encodes the
/// generated kinds even when a file already exists (used when the generation
/// code changed); it is ignored by the Chromium and real-world kinds, which
/// track their own freshness.
pub fn generate_kind(dir: &Path, kind: Kind, overwrite: bool) -> Vec<TestVideo> {
    match kind {
        Kind::Synthetic => generate_specs(dir, all_specs().into_iter(), Kind::Synthetic, overwrite),
        Kind::Random => generate_specs(
            dir,
            random::specs(random::seed(), random::count()),
            Kind::Random,
            overwrite,
        ),
        Kind::Derived => {
            let baseline = ensure_baseline(dir, overwrite);
            generation::derived_edge_files(dir, &baseline, overwrite)
                .into_iter()
                .map(|(name, path)| TestVideo {
                    name,
                    path,
                    spec: None,
                    kind: Kind::Derived,
                })
                .collect()
        }
        Kind::Chromium => chromium::corpus_files(dir)
            .into_iter()
            .map(|(name, path)| TestVideo {
                name,
                path,
                spec: None,
                kind: Kind::Chromium,
            })
            .collect(),
        Kind::RealWorld => scan_real_videos(),
    }
}

/// The baseline clip's path, generating it if missing (the derived kind needs
/// it as input and must be runnable on its own).
fn ensure_baseline(dir: &Path, overwrite: bool) -> PathBuf {
    let spec = all_specs()
        .into_iter()
        .find(|spec| spec.name == BASELINE)
        .expect("the fixed matrix contains the baseline");
    generation::generate(&spec, dir, overwrite).expect("baseline must generate")
}

/// The registry of all test videos, generating any missing files on first
/// call.
///
/// Lazy and cached: within a process via `OnceLock`, across processes via the
/// files themselves (generation is skipped when the file already exists, and
/// writes are tempfile-plus-rename so concurrent test binaries can't observe
/// half-written fixtures). When the generation code has changed
/// ([`generation_is_stale`]) the generated files are overwritten in place.
/// Specs whose encoder is missing from the local ffmpeg are skipped with a
/// warning rather than failing the registry.
pub fn videos() -> &'static Registry {
    static REGISTRY: LazyLock<Registry> = LazyLock::new(|| {
        let dir = fixtures_dir();
        fs::create_dir_all(&dir).expect("could not create fixtures directory");
        let overwrite = generation_is_stale(&dir);

        let started = Instant::now();
        let videos: Vec<TestVideo> = [
            Kind::Synthetic,
            Kind::Random,
            Kind::Derived,
            Kind::Chromium,
            Kind::RealWorld,
        ]
        .into_iter()
        .flat_map(|kind| generate_kind(&dir, kind, overwrite))
        .collect();

        if overwrite {
            record_generation_hash(&dir);
        }

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
    });

    &REGISTRY
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

fn generate_specs(
    dir: &Path,
    specs: impl Iterator<Item = Spec>,
    kind: Kind,
    overwrite: bool,
) -> Vec<TestVideo> {
    let mut videos = Vec::new();
    for spec in specs {
        if !generation::encoder_available(spec.codec.encoder()) {
            tracing::warn!(
                name = %spec.name,
                encoder = spec.codec.encoder(),
                "skipping fixture: encoder not available in this ffmpeg build"
            );
            continue;
        }
        match generation::generate(&spec, dir, overwrite) {
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
    videos
}

fn scan_real_videos() -> Vec<TestVideo> {
    let mut videos = Vec::new();
    let Ok(dir) = std::env::var("GAZPACHO_REAL_VIDEOS_DIR") else {
        return videos;
    };
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(err) => {
            tracing::warn!(%dir, %err, "cannot read GAZPACHO_REAL_VIDEOS_DIR; skipping");
            return videos;
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
    videos
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
