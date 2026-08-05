//! Reading, writing and working with media files; including video and audio.

pub mod metadata;
pub mod read;
pub mod write;

pub use read::MediaReader;
pub use write::MediaWriter;
