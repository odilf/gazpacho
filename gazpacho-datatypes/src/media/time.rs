use std::{fmt, ops, range::Range};

use num_rational::{Ratio, Rational64};
use num_traits::ToPrimitive as _;

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

    pub fn duration_since(self, start: Time) -> Option<Duration> {
        let t = self.0 - start.0;
        Some(Duration(Ratio::new(
            u32::try_from(*t.numer()).ok()?,
            u32::try_from(*t.denom()).ok()?,
        )))
    }
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

pub struct Duration(Ratio<u32>);

/// A contigious time-range.
///
// TODO: Property test this
/// `start` is guaranteed to be before `end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Extent(Range<Time>);

impl ops::Deref for Extent {
    type Target = Range<Time>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Extent {
    pub fn new(start: Time, end: Time) -> Option<Self> {
        if start > end {
            return None;
        }

        Some(Self((start..end).into()))
    }

    pub fn duration(&self) -> Duration {
        let t = self.end.0 - self.start.0;
        Duration(Ratio::new(*t.numer() as u32, *t.denom() as u32))
    }
}

impl ops::Add<Duration> for Time {
    type Output = Time;
    fn add(self, rhs: Duration) -> Self::Output {
        Time(self.0 + Rational64::new(i64::from(*rhs.0.numer()), i64::from(*rhs.0.denom())))
    }
}

impl ops::Sub<Duration> for Time {
    type Output = Time;
    fn sub(self, rhs: Duration) -> Self::Output {
        Time(self.0 - Rational64::new(i64::from(*rhs.0.numer()), i64::from(*rhs.0.denom())))
    }
}
