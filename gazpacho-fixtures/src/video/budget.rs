//! Budget resolution and seeded sampling for the fixture-driven property
//! suites.

use std::hash::{Hash as _, Hasher as _};

use rand::SeedableRng;

pub const FAST_BUDGET: u64 = 1_000_000;
pub const DEFAULT_BUDGET: u64 = 2_000_000;

/// How many videos to produce. If `None`, it means there is no limit.
///
/// Resolution order:
/// 1. `GAZPACHO_FIXTURES_BUDGET`
/// 2. `NEXTEST_PROFILE`
pub fn get_budget() -> Option<u64> {
    if let Some(raw) = std::env::var_os("GAZPACHO_FIXTURES_BUDGET") {
        let value = raw
            .to_str()
            .and_then(|s| s.trim().parse().ok())
            .expect("GAZPACHO_FIXTURES_BUDGET must be a non-negative integer");
        return Some(value);
    }

    match std::env::var("NEXTEST_PROFILE").as_deref() {
        Ok("fast") => Some(FAST_BUDGET),
        Ok("full") => None,
        Ok("default") | _ => Some(DEFAULT_BUDGET),
    }
}

/// The deterministic seed for the cost-weighted shuffle.
///
/// Resolution order:
/// 1. `GAZPACHO_FIXTURES_SEED`
/// 2. `NEXTEST_RUN_ID`
pub fn get_seed() -> Option<u64> {
    if let Some(raw) = std::env::var_os("GAZPACHO_FIXTURES_SEED") {
        if let Some(value) = raw.to_str().and_then(|s| s.trim().parse().ok()) {
            return Some(value);
        }
    }
    if let Ok(run_id) = std::env::var("NEXTEST_RUN_ID") {
        let mut hasher = std::hash::DefaultHasher::new();
        run_id.hash(&mut hasher);
        return Some(hasher.finish());
    }

    None
}

pub fn rng() -> rand::rngs::SmallRng {
    rand::rngs::SmallRng::seed_from_u64(get_seed().unwrap_or_default())
}
