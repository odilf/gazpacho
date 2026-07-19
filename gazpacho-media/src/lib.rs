//! Reading, writing and working with media files; including video and audio.

pub mod metadata;
pub mod read;
pub mod render;

pub use read::MediaReader;
