//! The Chromium media test corpus (`media/test/data` in the Chromium tree),
//! downloaded once and cached under `target/chromium-fixtures/<commit>/`.
//!
//! We get the sources from `chromium.googlesource.com`, which serves a tar.gz
//! of just that directory, pinned on a [`COMMIT`] (which we use as cache key)
//!
//! This corpus contains encrypted, corrupted, truncated, and audio-only files.
//! Since we defer to ffmpeg to decode, we try to decode each video and only
//! keep the ones where this decoding works.
//!
//!  Test can be run with this source disabled with
//! `GAZPACHO_CHROMIUM_VIDEOS=0`.
//!
//! Videos are fetched with `curl` and decompressed with `tar` and `gzip`, so
//! those need to be available on the system.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use eyre::{WrapErr as _, ensure};
use ffmpeg_sidecar::paths::ffmpeg_path;

use crate::fixtures::registry::REAL_EXTENSIONS;

/// The pinned Chromium commit the corpus is downloaded at.
///
/// To find the latest commit touching the corpus:
/// `curl -s "https://api.github.com/repos/chromium/chromium/commits?path=media/test/data&per_page=1"`
const COMMIT: &str = "acb10adca5300302643fa4014825eae9ceaf7adc";

/// Accepted-file cache. Bump the version to re-run decode validation without
/// re-downloading the data.
const MANIFEST: &str = "manifest-v2.txt";

/// Worker threads for the one-time decode validation pass.
const VALIDATE_THREADS: usize = 8;

/// `(registry name, path)` pairs for the corpus, downloading and validating
/// on first use. Never fails: problems degrade to a warning and an empty
/// corpus so the rest of the registry stays usable offline.
pub(super) fn corpus_files() -> Vec<(String, PathBuf)> {
    match std::env::var("GAZPACHO_CHROMIUM_VIDEOS").as_deref() {
        Ok("0") => return Vec::new(),
        Ok("1") | Err(_) => {}
        Ok(other) => {
            panic!("GAZPACHO_CHROMIUM_VIDEOS={other:?} is not valid (use `0` or `1`)")
        }
    }
    match ensure_corpus() {
        Ok(files) => files,
        Err(err) => {
            tracing::warn!(%err, "Chromium corpus unavailable; continuing without it");
            Vec::new()
        }
    }
}

fn ensure_corpus() -> eyre::Result<Vec<(String, PathBuf)>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/chromium-fixtures")
        .join(&COMMIT[..12]);

    fs::create_dir_all(&root).wrap_err("creating chromium fixtures directory")?;

    let data = root.join("data");
    if !data.exists() {
        download_and_extract(&root, &data)?;
    }

    let manifest = root.join(MANIFEST);
    let accepted: Vec<String> = match fs::read_to_string(&manifest) {
        Ok(contents) => contents.lines().map(str::to_owned).collect(),
        Err(_) => {
            let accepted = validate_all(&data)?;
            // Temp-plus-rename, like the generated fixtures.
            let tmp = root.join(format!(".manifest-{}", std::process::id()));
            fs::write(&tmp, accepted.join("\n") + "\n")?;
            fs::rename(&tmp, &manifest).wrap_err("moving manifest into place")?;
            accepted
        }
    };

    Ok(accepted
        .iter()
        .filter(|file| !file.is_empty())
        .map(|file| {
            let stem = Path::new(file)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unnamed");
            let ext = Path::new(file)
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("bin");
            // Extension included so e.g. bear.mp4/bear.webm stay distinct.
            (format!("chromium_{stem}_{ext}"), data.join(file))
        })
        .collect())
}

fn download_and_extract(root: &Path, data: &Path) -> eyre::Result<()> {
    let url = format!(
        "https://chromium.googlesource.com/chromium/src/+archive/{COMMIT}/media/test/data.tar.gz"
    );
    let started = Instant::now();
    tracing::info!(%url, "downloading Chromium media test corpus (~80 MB)");

    let pid = std::process::id();
    let tarball = root.join(format!(".corpus-{pid}.tar.gz"));
    let staging = root.join(format!(".corpus-{pid}"));
    // Inside a closure to cleanup dir if failed.
    let result = (|| {
        run_tool(
            Command::new("curl")
                .args(["-fsSL", "--retry", "3", "--connect-timeout", "30", "-o"])
                .arg(&tarball)
                .arg(&url),
        )?;
        fs::create_dir_all(&staging)?;
        run_tool(
            Command::new("tar")
                .arg("-xzf")
                .arg(&tarball)
                .arg("-C")
                .arg(&staging),
        )?;
        match fs::rename(&staging, data) {
            Ok(()) => Ok(()),
            // Lost the race with a concurrent test binary; its copy is
            // complete because dir renames are atomic.
            Err(_) if data.exists() => {
                let _ = fs::remove_dir_all(&staging);
                Ok(())
            }
            Err(err) => Err(err).wrap_err("moving corpus into place"),
        }
    })();
    if let Err(err) = fs::remove_file(&tarball) {
        tracing::error!(?tarball, ?err, "couldn't remove file")
    }
    if result.is_err() {
        if let Err(err) = fs::remove_dir_all(&staging) {
            tracing::error!(?staging, ?err, "couldn't remove directory")
        }
    }
    result?;

    tracing::info!(
        elapsed_ms = started.elapsed().as_millis() as u64,
        "Chromium corpus downloaded"
    );
    Ok(())
}

/// Every top-level corpus file with a video extension that ffmpeg decodes
/// cleanly, sorted by name (registry order must be deterministic for
/// reproducible `GAZPACHO_TEST_VIDEOS` samples).
fn validate_all(data: &Path) -> eyre::Result<Vec<String>> {
    let mut candidates: Vec<String> = fs::read_dir(data)
        .wrap_err("reading corpus data directory")?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| {
            Path::new(name)
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| REAL_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
        })
        .collect();
    candidates.sort_unstable();

    let started = Instant::now();
    tracing::info!(
        candidates = candidates.len(),
        "validating Chromium corpus by decode (one-time, cached in manifest)"
    );

    let cursor = AtomicUsize::new(0);
    let accepted = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..VALIDATE_THREADS {
            scope.spawn(|| {
                loop {
                    let Some(name) = candidates.get(cursor.fetch_add(1, Ordering::Relaxed)) else {
                        return;
                    };
                    let path = data.join(name);
                    if !decodes_cleanly(&path) {
                        tracing::debug!(name, "rejecting: does not decode cleanly");
                        continue;
                    }
                    if !has_video_packets(&path) {
                        // TODO: Inspect this better.
                        tracing::warn!(name, "rejecting: does not have video packets");
                        continue;
                    }
                    accepted.lock().unwrap().push(name.clone());
                }
            });
        }
    });
    let mut accepted = accepted.into_inner().unwrap();
    accepted.sort_unstable();

    ensure!(
        !accepted.is_empty(),
        "no corpus file decodes cleanly ({} candidates) — broken download or ffmpeg?",
        candidates.len()
    );
    tracing::info!(
        accepted = accepted.len(),
        rejected = candidates.len() - accepted.len(),
        elapsed_ms = started.elapsed().as_millis() as u64,
        "Chromium corpus validated"
    );
    Ok(accepted)
}

// TODO: Not sure about this.
/// Whether the file's first video stream carries any packets at all —
/// rejects metadata-track / init-segment-style files whose video track
/// exists but has no samples (decoding those "succeeds" with zero frames).
fn has_video_packets(path: &Path) -> bool {
    let output = Command::new(ffmpeg_sidecar::ffprobe::ffprobe_path())
        .args(["-loglevel", "error", "-select_streams", "v:0"])
        .args(["-show_entries", "packet=pts", "-of", "csv=p=0"])
        // Stop after the first packet; existence is all that matters.
        .args(["-read_intervals", "%+#1"])
        .arg(path)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    output.is_ok_and(|out| out.status.success() && !out.stdout.trim_ascii().is_empty())
}

/// Whether ffmpeg decodes the file's first video stream start to finish
/// without a single error. Useful to filter out the corpus's encrypted, corrupted,
/// truncated, and audio-only files without a name blocklist.
fn decodes_cleanly(path: &Path) -> bool {
    Command::new(ffmpeg_path())
        .args(["-hide_banner", "-loglevel", "error", "-xerror"])
        .arg("-i")
        .arg(path)
        .args(["-map", "0:v:0", "-f", "null", "-"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Run and surface errors.
fn run_tool(cmd: &mut Command) -> eyre::Result<()> {
    let name = cmd.get_args().next().unwrap().to_string_lossy().to_string();
    let output = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .wrap_err_with(|| format!("running {name} (is it installed?)"))?;

    ensure!(
        output.status.success(),
        "{name} failed ({}): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    );

    Ok(())
}
