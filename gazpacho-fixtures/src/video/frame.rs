//! Minimal pixel-buffer types, deliberately independent of `gazpacho-media`:
//! this crate is a plain dependency of its tests, so ground truth must not be
//! expressed in the types under test. Conversions happen at test call sites
//! (dimensions and raw bytes).

use std::fmt;

use eyre::ensure;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Frame dimensions. Serialized as `[width, height]` to match the manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

impl fmt::Display for Resolution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

impl Serialize for Resolution {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (self.width, self.height).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Resolution {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (width, height) = <(u32, u32)>::deserialize(deserializer)?;
        Ok(Self { width, height })
    }
}

/// A CPU frame: RGBA8, row-major, tightly packed.
#[derive(Clone, PartialEq, Eq)]
pub struct Frame {
    resolution: Resolution,
    data: Box<[u8]>,
}

impl Frame {
    pub fn new(resolution: Resolution, data: impl Into<Box<[u8]>>) -> Self {
        let data = data.into();
        let area = resolution.width * resolution.height;
        assert_eq!(data.len() as u32, area * 4);

        Self { resolution, data }
    }

    pub fn resolution(&self) -> Resolution {
        self.resolution
    }

    pub fn get(&self, x: u32, y: u32) -> [u8; 4] {
        assert!(
            x < self.resolution.width && y < self.resolution.height,
            "({x}, {y}) is out of bounds for a {} frame",
            self.resolution
        );
        let i = 4 * (y * self.resolution.width + x) as usize;
        self.data
            .get(i..i + 4)
            .expect("checked in bounds above")
            .try_into()
            .expect("slice of length 4 always converts to [u8; 4]")
    }

    /// The raw RGBA8 pixels, row-major.
    pub fn bytes(&self) -> &[u8] {
        &self.data
    }

    /// The index stamped into this frame's pixels; see
    /// [`recover_index`](crate::recover_index).
    pub fn recover_index(&self) -> eyre::Result<u32> {
        recover_index(self.resolution, &self.data)
    }
}

impl fmt::Debug for Frame {
    // Manual impl: dumping megabytes of pixels into assert messages helps no one.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Frame")
            .field("resolution", &self.resolution)
            .field("data", &format_args!("<{}-byte array>", self.data.len()))
            .finish()
    }
}

// === Stamping ==============================================================

/// Stamp grid size.
const GRID: u32 = 4;
/// Total bits that can be stamped.
const STAMP_BITS: u32 = GRID * GRID;

/// A frame with `index` stamped as a [`GRID`]x[`GRID`] grid of black/white
/// blocks, MSB first in raster order.
///
/// Mirrors `stamp()` in `scripts/encode.py`.
pub fn stamp(resolution: Resolution, index: u32) -> Frame {
    let Resolution { width, height } = resolution;

    assert!(
        index < 1 << STAMP_BITS,
        "index {index} does not fit the stamp"
    );
    let mut data = vec![0u8; 4 * (width * height) as usize];
    for y in 0..height {
        let row = y * GRID / height;
        for x in 0..width {
            let col = x * GRID / width;
            let bit = STAMP_BITS - 1 - (row * GRID + col);
            let color = if index >> bit & 1 == 1 { 255 } else { 0 };
            let i = 4 * (y * width + x) as usize;
            data.get_mut(i..i + 4)
                .expect("i is always in bounds: data is sized 4 * width * height, and x < width, y < height")
                .copy_from_slice(&[color, color, color, 255]);
        }
    }

    Frame::new(resolution, data)
}

/// Read the stamped frame index back from decoded pixels.
///
/// Takes dimensions plus raw bytes (rather than [`Frame`]) so callers in other
/// crates can pass their own frame types' data. Works at any resolution
/// (blocks are sampled by relative position) and for gray, RGB, or RGBA data
/// (channel count inferred from the buffer length; channel 0 is sampled).
/// Each block's central region is averaged and thresholded; a block that lands
/// in the ambiguous middle is an error rather than a guess, so corrupted
/// frames fail loudly.
///
/// Mirrors the contract laid out in `scripts/encode.py`.
pub fn recover_index(resolution: Resolution, data: &[u8]) -> eyre::Result<u32> {
    let Resolution { width, height } = resolution;
    let area = (width * height) as usize;
    ensure!(
        area > 0 && data.len().is_multiple_of(area) && !data.is_empty(),
        "buffer of {} bytes is not a whole number of {width}x{height} channels",
        data.len()
    );
    let channels = data.len() / area;

    let mut index = 0u32;
    for row in 0..GRID {
        for col in 0..GRID {
            // Average over the central half of the block to avoid compresion
            // artifacts. Block bounds are `(n + {0.25, 0.75}) / GRID * dimension`;
            // computed as exact integer ratios (numerator scaled by 4 first) so
            // there's no float rounding and no sign to lose.
            let x0 = (4 * col + 1) * width / (4 * GRID);
            let x1 = ((4 * col + 3) * width / (4 * GRID)).max(x0 + 1);
            let y0 = (4 * row + 1) * height / (4 * GRID);
            let y1 = ((4 * row + 3) * height / (4 * GRID)).max(y0 + 1);

            let mut sum = 0u64;
            let mut count = 0u64;
            for y in y0..y1.min(height) {
                for x in x0..x1.min(width) {
                    let i = channels * (y * width + x) as usize;
                    sum += u64::from(
                        *data
                            .get(i)
                            .ok_or_else(|| eyre::eyre!("pixel {i} out of bounds"))?,
                    );
                    count += 1;
                }
            }
            ensure!(count > 0, "block ({row},{col}) sampled no pixels");
            let avg = sum as f64 / count as f64;
            ensure!(
                !(64.0..192.0).contains(&avg),
                "block ({row},{col}) is ambiguous (average luma {avg:.1})"
            );
            index = index << 1 | u32::from(avg >= 128.0);
        }
    }
    Ok(index)
}