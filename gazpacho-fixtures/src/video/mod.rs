//! Test videos for gazpacho, exposed as a typed registry loaded from the
//! manifest that `scripts/generate.py` produces under
//! `target/gazpacho-fixtures/manifest.json`.
//!
//! Videos that could not be generated (e.g. a missing encoder) stay in the
//! manifest with a `failed: Option<String>` reason; consumers are expected to
//! mark them *ignored* rather than drop them silently.
//!
//! This crate is independent of `gazpacho-media` to avoid testing itself, so
//! it has simple implementations of [`Frame`], [`Resolution`], and the spec
//! math.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;

use eyre::WrapErr as _;
use serde::Deserialize;

mod decode;
mod frame;
mod spec;
mod test_harness;

pub use decode::{decode_all_rgba, decode_rgba_prefix};
pub use frame::{Frame, Resolution, recover_index, stamp};
pub use spec::{Codec, Container, PixFmt, Spec, Timing};
pub use test_harness::{run_tests, test_properties};

const MANIFEST_FILE: &str = "manifest.json";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct Manifest {
    generation_hash: String,
    videos: Fixtures,
}

/// All test video files.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Fixtures {
    pub synthetic: Vec<SyntheticVideo>,
    pub derived: Vec<DerivedVideo>,
    pub chromium: Vec<ChromiumVideo>,
    pub realistic: Vec<RealisticVideo>,
}

impl Fixtures {
    pub fn iter(&self) -> impl Iterator<Item = &TestVideo> {
        self.synthetic
            .iter()
            .map(TestVideo::as_generic)
            .chain(self.derived.iter().map(TestVideo::as_generic))
            .chain(self.chromium.iter().map(TestVideo::as_generic))
            .chain(self.realistic.iter().map(TestVideo::as_generic))
    }

    pub fn spec_backed(&self) -> impl Iterator<Item = (&TestVideo, &Spec)> {
        self.synthetic
            .iter()
            .map(|v| (v.as_generic(), &v.meta))
            .chain(
                self.derived
                    .iter()
                    .map(|v| (v.as_generic(), &v.meta.baseline)),
            )
    }
}

#[repr(C)]
#[derive(Debug, Clone, Deserialize)]
pub struct TestVideo<M = ()> {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub failed: Option<String>,
    #[serde(flatten)]
    pub meta: M,
}

impl<M> TestVideo<M> {
    fn as_generic(&self) -> &TestVideo<()> {
        const _: () = {
            assert!(
                std::mem::offset_of!(TestVideo<()>, meta)
                    == std::mem::offset_of!(TestVideo<()>, failed)
                        + std::mem::size_of::<Option<String>>()
            );
        };

        // SAFETY: `TestVideo` is `#[repr(C)]`, so field offsets depend only on
        // the fields preceding them. `name`, `path`, and `failed` are identical,
        // non-generic types in `TestVideo<M>` and `TestVideo<()>`, so they share
        // the same offsets. `meta` is the last field, and `()` is a ZST, so the
        // leading bytes of a `TestVideo<M>` are a valid `TestVideo<()>`.
        //
        // There is a compiler check above that should fail if `meta` stops
        // being the last field.
        unsafe { std::mem::transmute::<&TestVideo<M>, &TestVideo<()>>(self) }
    }
}

pub type SyntheticVideo = TestVideo<Spec>;

/// One derived edge case (trim, audio track, cover art).
#[derive(Debug, Clone, Deserialize)]
pub struct DerivedMeta {
    pub baseline: Spec,
    pub baseline_path: String,
    pub edit: DerivedEdit,
}

pub type DerivedVideo = TestVideo<DerivedMeta>;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DerivedEdit {
    Trimmed,
    WithAudio,
    WithCover,
}

/// One file of the Chromium media corpus, with its one-time annotations.
#[derive(Debug, Clone, Deserialize)]
pub struct ChromiumMeta {
    pub sha256: String,
    pub size: u64,
    pub decodes_cleanly: bool,
    pub has_video_packets: bool,
    pub extension: String,
}

pub type ChromiumVideo = TestVideo<ChromiumMeta>;

/// One real-world clip downloaded from the web (Blender open movies served by
/// test-videos.co.uk, or public-domain live-action from Wikimedia Commons),
/// with its one-time annotations.
#[derive(Debug, Clone, Deserialize)]
pub struct RealisticMeta {
    /// Human-readable origin of the clip, for attribution.
    pub source: String,
    pub sha256: String,
    pub size: u64,
    pub decodes_cleanly: bool,
    pub has_video_packets: bool,
    pub extension: String,
}

pub type RealisticVideo = TestVideo<RealisticMeta>;

/// A registered video regar
/// The single root every fixture lives under.
fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/gazpacho-fixtures")
}

/// Load the registry, generating any missing/stale fixtures on first call.
///
/// Lazy and cached per process.
pub fn videos() -> &'static Fixtures {
    static FIXTURES: LazyLock<Fixtures> = LazyLock::new(|| {
        let root = fixtures_dir();
        let manifest = root.join(MANIFEST_FILE);
        let text = std::fs::read_to_string(&manifest)
            .wrap_err_with(|| {
                format!(
                    "fixture manifest {} is still missing after generation",
                    manifest.display()
                )
            })
            .unwrap();

        let manifest: Manifest = serde_json::from_str(&text)
            .wrap_err_with(|| format!("parsing manifest ({})", manifest.display()))
            .unwrap();

        manifest.videos
    });

    &FIXTURES
}

/// Run the generator. This has an idempotent result, but you should probably
/// avoid calling it many times concurrently since that might lead to
/// duplicating encoding work.
pub fn run_generator() -> eyre::Result<()> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/main.py");
    tracing::info!(script = %script.display(), "regenerating fixtures");
    let status = Command::new("python3")
        .arg(&script)
        .status()
        .wrap_err_with(|| format!("spawning python3 to run {}", script.display()))?;
    eyre::ensure!(
        status.success(),
        "fixture generator {} failed with {status}",
        script.display()
    );
    Ok(())
}

#[test]
fn registry_sanity() -> eyre::Result<()> {
    let generated = videos().synthetic.len();
    eyre::ensure!(
        generated >= 40,
        "expected the full matrix, got {generated} fixtures"
    );
    for video in videos().iter() {
        let size = std::fs::metadata(&video.path)
            .wrap_err_with(|| format!("{} missing", video.path))?
            .len();

        eyre::ensure!(size > 0, "{} is empty", video.name);
    }
    // Names must be unique across the whole manifest: they key lookups and
    // label failures. `all_present` excludes non-video corpus files and failed
    // clips, so check the typed arrays directly.
    let mut names: Vec<&str> = videos()
        .synthetic
        .iter()
        .map(|v| v.name.as_str())
        .chain(videos().derived.iter().map(|v| v.name.as_str()))
        .chain(videos().chromium.iter().map(|v| v.name.as_str()))
        .chain(videos().realistic.iter().map(|v| v.name.as_str()))
        .collect();

    names.sort_unstable();
    let distinct = {
        let mut deduped = names.clone();
        deduped.dedup();
        deduped.len()
    };
    eyre::ensure!(distinct == names.len(), "duplicate names");

    // Targeted lookups tests rely on must always exist.
    for name in [
        "h264_420p_g12_30.mp4",
        "h264_420p_g250_30.mp4",
        "h264_420p_g12_vfr.mp4",
        "h264_420p_g12_30_b2.mp4",
        "h264_420p_g12_30_b2_offset.mp4",
        "h264_420p_g12_30_b2_offset.ts",
    ] {
        videos()
            .synthetic
            .iter()
            .find(|video| video.name == name)
            .unwrap();
    }

    for name in [
        "trimmed__h264_420p_g12_30.mp4",
        "with_audio__h264_420p_g12_30.mp4",
        "with_cover__h264_420p_g12_30.mp4",
    ] {
        videos()
            .derived
            .iter()
            .find(|video| video.name == name)
            .unwrap();
    }

    for name in [
        "bigbuckbunny_720_10s_10MB.mp4",
        "sintel_720_10s_5MB.mp4",
        "jellyfish_1080_10s_10MB.mp4",
        "the_cook_1918.webm",
        "bombing_of_hamburg.ogv",
    ] {
        videos()
            .realistic
            .iter()
            .find(|video| video.name == name)
            .unwrap();
    }

    Ok(())
}
