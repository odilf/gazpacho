#![allow(dead_code, reason = "work in progress")]

/// Smallest chunk we bother decoding, if for instance every frame is a keyframe.
const DEFAULT_MIN_CHUNK_FRAMES: u32 = 5;
/// Largest chunk we decode in one go. Caps cache-entry size and miss latency.
const DEFAULT_MAX_CHUNK_FRAMES: u32 = 64;
/// Default frame cache budget, in decoded bytes (512 MB).
const DEFAULT_CACHE_BYTES: usize = 512 * 1024 * 1024;
