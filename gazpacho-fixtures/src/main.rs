//! Manually generate test videos, per kind: `cargo run -p gazpacho-fixtures
//! -- [--force] [kind…]`.
//!
//! Tests do this on demand anyway (the registry is lazy and idempotent), so
//! this exists to warm or rebuild the cache explicitly — e.g. after changing
//! generation code, or to pre-download the Chromium corpus before going
//! offline.

use gazpacho_fixtures::{Kind, chromium_cache_dir, collect_garbage, fixtures_dir, generate_kind};

const USAGE: &str = "\
usage: cargo run -p gazpacho-fixtures -- [--force] [kind…]
kinds: synthetic | random | derived | chromium | all   (default: all)
--force deletes the affected caches first: the generated-fixtures directory
for synthetic/random/derived, the downloaded corpus for chromium.";

fn main() {
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
                return;
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
    if force {
        let generated = kinds.iter().any(|kind| *kind != Kind::Chromium);
        for target in [
            (generated, dir.clone()),
            (kinds.contains(&Kind::Chromium), chromium_cache_dir()),
        ]
        .into_iter()
        .filter_map(|(wanted, path)| (wanted && path.exists()).then_some(path))
        {
            tracing::info!(dir = %target.display(), "--force: removing cache");
            std::fs::remove_dir_all(&target).expect("removing cache directory");
        }
    }
    std::fs::create_dir_all(&dir).expect("could not create fixtures directory");
    collect_garbage();

    for kind in kinds {
        let videos = generate_kind(&dir, kind);
        tracing::info!(?kind, count = videos.len(), "kind ready");
    }
    tracing::info!(dir = %dir.display(), "done");
}
