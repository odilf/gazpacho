//! Minimal pixel-buffer types, deliberately independent of `gazpacho-media`:
//! this crate is a plain dependency of its tests, so ground truth must not be
//! expressed in the types under test. Conversions happen at test call sites
//! (dimensions and raw bytes).

use std::fmt;

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
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// The index stamped into this frame's pixels; see
    /// [`recover_index`](crate::recover_index).
    pub fn recover_index(&self) -> eyre::Result<u32> {
        crate::generation::recover_index(self.resolution, &self.data)
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
