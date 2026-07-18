//! Reading, writing and working with media files; including video and audio.

pub mod metadata;
pub mod read;
pub mod render;

pub use read::MediaReader;

// Synthetic test-video generation. Available to unit tests unconditionally,
// and to integration tests / downstream crates via `--features fixtures`.
#[cfg(any(test, feature = "fixtures"))]
pub mod fixtures;
