pub mod frame;

pub use frame::Frame;

use std::{fmt, path::PathBuf};

macro_rules! define_data {
    ($($name:ident, $struct_name:ident, $ty:ty);* $(;)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum SimpleDataType {
            $($struct_name),*
        }

        #[derive(Debug, Clone,PartialEq)]
        pub enum SimpleDataValue {
            $($struct_name($ty)),*
        }

        impl SimpleDataType {
            $(
                pub fn $name() -> Self {
                    Self::$struct_name
                }
            )*
        }


        impl DataType {
            $(
                pub fn $name() -> Self {
                    Self::Simple(SimpleDataType::$struct_name)
                }
            )*
        }

        impl SimpleDataValue {
                $(
                    pub fn $name(value: $ty) -> Self {
                        Self::$struct_name(value)
                    }
                )*

                pub fn typ(&self) -> SimpleDataType {
                    match self {
                        $(Self::$struct_name(_) => SimpleDataType::$struct_name),*
                    }
                }
        }

        impl DataValue {
                $(
                    pub fn $name(value: $ty) -> Self {
                        Self::Simple(SimpleDataValue::$struct_name(value))
                    }
                )*

        }

        // Conversions
        $(
            impl TryFrom<SimpleDataValue> for $ty {
                type Error = DataValueConversionError;
                fn try_from(value: SimpleDataValue) -> Result<Self, Self::Error> {
                    match value {
                        SimpleDataValue::$struct_name(x) => Ok(x),
                        other => Err(DataValueConversionError { needed: SimpleDataType::$struct_name, got: DataType::Simple(other.typ()) })
                    }
                }
            }

            impl TryFrom<DataValue> for $ty {
                type Error = DataValueConversionError;
                fn try_from(value: DataValue) -> Result<Self, Self::Error> {
                    match value {
                        DataValue::Simple(x) => x.try_into(),
                        other => Err(DataValueConversionError { needed: SimpleDataType::$struct_name, got: other.typ() })
                    }
                }
            }


            impl From<$ty> for SimpleDataValue {
                fn from(value: $ty) -> Self {
                    Self::$struct_name(value)
                }
            }

            impl From<$ty> for DataValue {
                fn from(value: $ty) -> Self {
                    Self::Simple(value.into())
                }

            }

        )*

    };
}

trait HasDataType {
    const DATA_TYPE: DataType;
}

define_data! {
    int, Int, i64;
    float, Float, f64;
    vframe, VideoFrame, Frame;
    string, String, String;
    path, Path, PathBuf;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataType {
    Simple(SimpleDataType),
    Track(SimpleDataType),
}

impl DataType {
    pub fn named(self, name: &'static str) -> Port {
        Port { name, typ: self }
    }

    pub fn video_track() -> Self {
        Self::Track(SimpleDataType::vframe())
    }
}

pub enum DataValue {
    Simple(SimpleDataValue),
    Track {
        length: u64,
        renderer: Box<dyn Fn(u64) -> SimpleDataValue>,
        typ: SimpleDataType,
    },
}

impl DataValue {
    pub fn typ(&self) -> DataType {
        match self {
            Self::Simple(x) => DataType::Simple(x.typ()),
            Self::Track { typ, .. } => DataType::Track(*typ),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Port {
    name: &'static str,
    typ: DataType,
}

impl Port {
    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn typ(&self) -> DataType {
        self.typ
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DataValueConversionError {
    needed: SimpleDataType,
    got: DataType,
}

impl fmt::Display for DataValueConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Needed {:?} but got {:?}", self.needed, self.got)
    }
}
