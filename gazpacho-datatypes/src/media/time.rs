use std::{fmt, ops, range::Range};

use num_rational::{Ratio, Rational64};
use num_traits::ToPrimitive as _;

/// A local-media time.
///
/// TODO: Define and document semantics.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Time(Rational64);

impl Time {
    pub fn from_secs(value: impl Into<Rational64>) -> Self {
        Self(value.into())
    }

    pub fn as_secs(&self) -> Rational64 {
        self.0
    }

    pub fn advance_secs(&self, delta: Duration) -> Time {
        Time(self.0 + to_i64_ratio(delta))
    }

    pub const ZERO: Self = Time(Ratio::ZERO);

    pub fn duration_since(self, start: Time) -> Option<Duration> {
        let t = self.0 - start.0;
        Some(Duration(Ratio::new(
            u64::try_from(*t.numer()).ok()?,
            u64::try_from(*t.denom()).ok()?,
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

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Duration(Ratio<u64>);

impl Duration {
    pub fn as_secs(&self) -> Ratio<u64> {
        self.0
    }
}

impl From<Ratio<u64>> for Duration {
    fn from(value: Ratio<u64>) -> Self {
        Self(value)
    }
}

impl ops::Mul<Duration> for Ratio<u64> {
    type Output = Duration;

    fn mul(self, rhs: Duration) -> Self::Output {
        Duration(self * rhs.0)
    }
}

/// A contigious time-range.
///
// TODO: Property test this
/// `start` is guaranteed to be before `end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Extent(Range<Time>);

// Manual deserialization to make sure we can't get invalid `Extent`s from deserialization,
// and manual serialization bc `serde` doesn't yet support `std::range::Range` :(
// https://github.com/serde-rs/serde/pull/3092
#[cfg(feature = "serde")]
impl serde::Serialize for Extent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        (self.start, self.end).serialize(serializer)
    }
}
#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Extent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let (start, end) = <(Time, Time)>::deserialize(deserializer)?;
        Extent::new(start, end).ok_or_else(|| serde::de::Error::custom("extent end precedes start"))
    }
}

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
        #[expect(
            clippy::cast_sign_loss,
            reason = "Extent is guaranteed to be `start <= end`, so `t = end - start >= 0`"
        )]
        Duration(Ratio::new(*t.numer() as u64, *t.denom() as u64))
    }
}

fn to_i64_ratio(duration: Duration) -> Rational64 {
    Rational64::new(
        i64::try_from(*duration.as_secs().numer())
            .expect("duration numerator fits in i64"),
        i64::try_from(*duration.as_secs().denom())
            .expect("duration denominator fits in i64"),
    )
}

impl ops::Add<Duration> for Time {
    type Output = Time;
    fn add(self, rhs: Duration) -> Self::Output {
        Time(self.0 + to_i64_ratio(rhs))
    }
}

impl ops::Sub<Duration> for Time {
    type Output = Time;
    fn sub(self, rhs: Duration) -> Self::Output {
        Time(self.0 - to_i64_ratio(rhs))
    }
}
