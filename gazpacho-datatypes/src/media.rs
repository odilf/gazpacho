use std::fmt;

use num_rational::{Ratio, Rational64};
use num_traits::ToPrimitive;

/// A local-media time.
///
/// TODO: Define and document semantics.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Time(Rational64);

impl Time {
    pub fn from_secs(value: impl Into<Rational64>) -> Self {
        Self(value.into())
    }

    pub fn as_secs(&self) -> Rational64 {
        self.0
    }

    pub fn advance_secs(&self, delta: Ratio<u64>) -> Time {
        let delta = Ratio::new(*delta.numer() as i64, *delta.denom() as i64);
        Time(self.0 + delta)
    }

    pub const ZERO: Self = Time(Ratio::ZERO);
}

impl fmt::Debug for Time {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:.4}s",
            self.0
                .to_f32()
                .expect("Value should be representable by f32")
        )
    }
}

impl fmt::Display for Time {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let secs = self.as_secs();
        let (numer, denom) = (*secs.numer(), *secs.denom());
        if let Some(scaled) = numer.checked_mul(1000)
            && scaled % denom == 0
        {
            let ms = scaled / denom;
            if ms % 1000 == 0 {
                write!(f, "{}s", ms / 1000)
            } else {
                write!(f, "{ms}ms")
            }
        } else {
            write!(f, "({numer}s / {denom})")
        }
    }
}

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
        #[expect(clippy::indexing_slicing, reason = "checked in bounds above")]
        let bytes = &self.data[i..i + 4];
        #[expect(
            clippy::unwrap_used,
            reason = "a 4-byte slice always converts to [u8; 4]"
        )]
        bytes.try_into().unwrap()
    }

    /// The raw RGBA8 pixels, row-major.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn map(self, f: impl FnMut(u8) -> u8) -> Frame {
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
