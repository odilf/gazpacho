use gazpacho_datatypes::{Extent, Fps, Frame, Resolution, SimpleValue, Str};

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Simple(SimpleValue),
    Frame(Frame),
    Extent(Extent),
    Fps(Fps),
    Resolution(Resolution),
}

impl Value {
    pub fn to_frame(self) -> eyre::Result<Frame> {
        match self {
            Self::Frame(frame) => Ok(frame),
            _ => eyre::bail!("not a frame"),
        }
    }

    pub fn to_float(self) -> eyre::Result<f64> {
        match self {
            Self::Simple(SimpleValue::Float(v)) => Ok(v.into()),
            _ => eyre::bail!("not a float"),
        }
    }

    pub fn to_str(self) -> eyre::Result<Str> {
        match self {
            Self::Simple(SimpleValue::Str(v)) => Ok(v),
            _ => eyre::bail!("not a string"),
        }
    }
}
