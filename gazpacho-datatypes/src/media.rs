use std::fmt;

use num_rational::Ratio;

mod time;
pub use time::*;

/// Frame rate in frames per second, as an exact rational.
///
/// Expressed as a ratio to be exact, since NTSC rates like `24000/1001`
/// accumulate drift if held as floats, so this keeps frame-index math exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fps(
    // TODO: Could (and should) this not be pub?
    pub Ratio<u64>,
);

impl Fps {
    pub fn value(&self) -> Ratio<u64> {
        self.0
    }

    /// Exact display duration of one frame.
    pub fn frame_length(self) -> Ratio<u64> {
        self.0.recip()
    }

    // TODO: Add other standard fps.
    pub const THIRTY: Self = Self(Ratio::new_raw(30, 1));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    data: Box<[[u8; 4]]>,
}

impl Frame {
    pub fn new(resolution: Resolution, data: impl Into<Box<[[u8; 4]]>>) -> Self {
        let data = data.into();
        let area = resolution.width * resolution.height;
        assert_eq!(data.len() as u32, area);

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
        let i = (y * self.resolution.width + x) as usize;
        #[expect(clippy::indexing_slicing, reason = "checked in bounds above")]
        self.data[i]
    }

    /// The raw RGBA8 pixels, row-major.
    pub fn data(&self) -> &[[u8; 4]] {
        &self.data
    }

    /// The bytes of [`Self::data`].
    pub fn bytes(&self) -> &[u8] {
        self.data().as_flattened()
    }

    pub fn map(self, f: impl FnMut([u8; 4]) -> [u8; 4]) -> Frame {
        Frame {
            resolution: self.resolution,
            data: self.data.into_iter().map(f).collect(),
        }
    }
}

impl fmt::Debug for Frame {
    // Manual impl: dumping megabytes of pixels into assert messages helps no one.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Frame")
            .field("resolution", &self.resolution)
            // NIT: Allocation can be avoided.
            .field("data", &format!("<{}-byte array>", self.data.len()))
            .finish()
    }
}

impl std::hash::Hash for Frame {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.data.hash(state)
    }
}
