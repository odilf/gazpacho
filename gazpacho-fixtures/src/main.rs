//! Manually generate test videos, per kind: `cargo run -p gazpacho-fixtures
//! -- [--force] [kind…]`.
//!
//! Tests do this on demand anyway (the registry is lazy and idempotent), so
//! this exists to warm or rebuild the cache explicitly — e.g. after changing
//! generation code, or to pre-download the Chromium corpus before going
//! offline. Everything lands under `target/fixtures/`.

use eyre::WrapErr as _;
use gazpacho_fixtures::{
    Kind, chromium, fixtures_dir, generate_kind, generation_is_stale, record_generation_hash,
};

const USAGE: &str = "\
usage: cargo run -p gazpacho-fixtures -- [--force] [kind…]
kinds: synthetic | random | derived | chromium | all   (default: all)
--force deletes the affected caches first: the generated fixtures under
target/fixtures/ for synthetic/random/derived, target/fixtures/chromium/ for
chromium.";

const GENERATED: [Kind; 3] = [Kind::Synthetic, Kind::Random, Kind::Derived];

fn main() -> eyre::Result<()> {
    use tracing_subscriber::EnvFilter;
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let mut force = false;
    let mut kinds: Vec<Kind> = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--force" => force = true,
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(());
            }
            "all" => kinds.extend([Kind::Synthetic, Kind::Random, Kind::Derived, Kind::Chromium]),
            "synthetic" => kinds.push(Kind::Synthetic),
            "random" => kinds.push(Kind::Random),
            "derived" => kinds.push(Kind::Derived),
            "chromium" => kinds.push(Kind::Chromium),
            other => {
                eprintln!("unknown argument {other:?}\n{USAGE}");
                std::process::exit(2);
            }
        }
    }
    if kinds.is_empty() {
        kinds.extend([Kind::Synthetic, Kind::Random, Kind::Derived, Kind::Chromium]);
    }
    kinds.dedup();

    let dir = fixtures_dir();
    let touches_generated = kinds.iter().any(|kind| GENERATED.contains(kind));
    if force {
        if touches_generated && dir.exists() {
            tracing::info!("--force: clearing generated fixtures");
            std::fs::remove_dir_all(&dir).wrap_err("removing generated fixtures")?;
        }
        if kinds.contains(&Kind::Chromium) && chromium::cache_dir(&dir).exists() {
            tracing::info!("--force: removing Chromium corpus");
            std::fs::remove_dir_all(chromium::cache_dir(&dir)).wrap_err("removing Chromium cache")?;
        }
    }
    std::fs::create_dir_all(&dir).wrap_err("could not create fixtures directory")?;

    let overwrite = generation_is_stale(&dir);
    for kind in &kinds {
        let videos = generate_kind(&dir, *kind, overwrite)?;
        tracing::info!(?kind, count = videos.len(), "kind ready");
    }

    // Only mark the generated set current if this run actually produced all of
    // it; otherwise a later run must still regenerate the missing kinds.
    if overwrite && GENERATED.iter().all(|kind| kinds.contains(kind)) {
        record_generation_hash(&dir);
    }
    tracing::info!(dir = %dir.display(), "done");
    Ok(())
}
