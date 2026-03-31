pub mod frame;

pub use frame::Frame;

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimpleDataType {
    Integer,
    Float,
    VideoFrame,
    // AudioFrame,
    String,
    Path,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataType {
    Simple(SimpleDataType),
    Track(SimpleDataType),
}

impl DataType {
    pub fn float() -> DataType {
        DataType::Simple(SimpleDataType::Float)
    }

    pub fn integer() -> DataType {
        DataType::Simple(SimpleDataType::Integer)
    }

    pub fn vframe() -> DataType {
        DataType::Simple(SimpleDataType::VideoFrame)
    }

    pub fn path() -> DataType {
        DataType::Simple(SimpleDataType::Path)
    }

    pub fn video_track() -> DataType {
        DataType::Track(SimpleDataType::VideoFrame)
    }

    pub fn named(self, name: &'static str) -> Bind {
        Bind { name, typ: self }
    }
}

impl TryFrom<DataValue> for i64 {
    type Error = ();
    fn try_from(value: DataValue) -> Result<Self, Self::Error> {
        match value {
            DataValue::Simple(SimpleDataValue::Integer(i)) => Ok(i),
            _ => Err(()),
        }
    }
}
impl TryFrom<DataValue> for f64 {
    type Error = ();
    fn try_from(value: DataValue) -> Result<Self, Self::Error> {
        match value {
            DataValue::Simple(SimpleDataValue::Float(v)) => Ok(v),
            _ => Err(()),
        }
    }
}

impl TryFrom<DataValue> for Frame {
    type Error = ();
    fn try_from(value: DataValue) -> Result<Self, Self::Error> {
        match value {
            DataValue::Simple(SimpleDataValue::VideoFrame(v)) => Ok(v),
            _ => Err(()),
        }
    }
}
impl TryFrom<SimpleDataValue> for Frame {
    type Error = ();
    fn try_from(value: SimpleDataValue) -> Result<Self, Self::Error> {
        match value {
            SimpleDataValue::VideoFrame(v) => Ok(v),
            _ => Err(()),
        }
    }
}

impl TryFrom<DataValue> for PathBuf {
    type Error = ();
    fn try_from(value: DataValue) -> Result<Self, Self::Error> {
        match value {
            DataValue::Simple(SimpleDataValue::Path(v)) => Ok(v),
            _ => Err(()),
        }
    }
}

impl From<i64> for DataValue {
    fn from(value: i64) -> Self {
        DataValue::Simple(SimpleDataValue::Integer(value))
    }
}

impl From<f64> for DataValue {
    fn from(value: f64) -> Self {
        DataValue::Simple(SimpleDataValue::Float(value))
    }
}

impl From<PathBuf> for DataValue {
    fn from(value: PathBuf) -> Self {
        DataValue::Simple(SimpleDataValue::Path(value))
    }
}

impl From<Frame> for SimpleDataValue {
    fn from(value: Frame) -> Self {
        SimpleDataValue::VideoFrame(value)
    }
}

impl From<f64> for SimpleDataValue {
    fn from(value: f64) -> Self {
        SimpleDataValue::Float(value)
    }
}
impl From<PathBuf> for SimpleDataValue {
    fn from(value: PathBuf) -> Self {
        SimpleDataValue::Path(value)
    }
}

#[derive(Debug, Clone)]
pub struct Bind {
    name: &'static str,
    typ: DataType,
}

impl Bind {
    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn typ(&self) -> DataType {
        self.typ
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SimpleDataValue {
    Integer(i64),
    Float(f64),
    VideoFrame(Frame),
    String(String),
    Path(PathBuf),
}

pub enum DataValue {
    Simple(SimpleDataValue),
    Track {
        length: u64,
        renderer: Box<dyn Fn(u64) -> SimpleDataValue>,
        typ: SimpleDataType,
    },
}

impl SimpleDataValue {
    pub fn typ(&self) -> SimpleDataType {
        match self {
            SimpleDataValue::Integer(_) => SimpleDataType::Integer,
            SimpleDataValue::Float(_) => SimpleDataType::Float,
            SimpleDataValue::VideoFrame(_) => SimpleDataType::VideoFrame,
            SimpleDataValue::String(_) => SimpleDataType::String,
            SimpleDataValue::Path(_) => SimpleDataType::Path,
        }
    }
}

impl DataValue {
    pub fn frame(value: Frame) -> Self {
        DataValue::Simple(SimpleDataValue::VideoFrame(value))
    }

    pub fn typ(&self) -> DataType {
        match self {
            Self::Simple(x) => DataType::Simple(x.typ()),
            Self::Track { typ, .. } => DataType::Track(*typ),
        }
    }
}
