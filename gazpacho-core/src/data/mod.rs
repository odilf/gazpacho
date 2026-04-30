pub mod frame;
pub mod track;

pub use frame::Frame;

use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

use crate::data::track::DynTrack;

define_data! {
    int, INT: Int(i64) from [i32, u32];
    float, FLOAT: Float(f64) from [f32];
    frame, FRAME: Frame(Frame);
    string, STRING: String(String) ref into [str];
}

/// A static type that has a corresponding dynamic (simple) type.
///
/// See also [`HasDataType`].
pub trait HasSimpleDataType {
    const SIMPLE_DATA_TYPE: SimpleDataType;
}

/// A static type that has a corresponding dynamic type.
///
/// Automatically implemented for any type that has a simple data type.
///
/// See also [`HasSimpleDataType`].
pub trait HasDataType {
    const DATA_TYPE: DataType;
}

impl<T: HasSimpleDataType> HasDataType for T {
    const DATA_TYPE: DataType = DataType::Simple(T::SIMPLE_DATA_TYPE);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataType {
    Simple(SimpleDataType),
    Track(SimpleDataType),
    GenericTrack,
}

impl DataType {
    pub const fn named(self, name: &'static str) -> Port {
        Port { name, typ: self }
    }

    // TODO: Move to automatically-generated names
    pub const fn video_track() -> Self {
        Self::Track(SimpleDataType::frame())
    }
}

pub enum DataValue {
    Simple(SimpleDataValue),
    Track(DynTrack),
}

impl fmt::Debug for DataValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Simple(value) => write!(f, "DataValue {{ {value:?} }}"),
            Self::Track(track) => write!(f, "DataValue {{ track ({}) }}", track.typ()),
        }
    }
}

impl DataValue {
    pub fn typ(&self) -> DataType {
        match self {
            Self::Simple(x) => DataType::Simple(x.typ()),
            Self::Track(track) => DataType::Track(track.typ()),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
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
    needed: DataType,
    got: DataType,
}

impl fmt::Display for DataValueConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "needed {:?} but got {:?}", self.needed, self.got)
    }
}

impl Error for DataValueConversionError {}

/// Generates all impls that are repetitive based on type. Rest of impls that do not need to enumarate types are below.
#[macro_export]
macro_rules! define_data {
    ($(
        $name:ident, $const_name:ident: $struct_name:ident($ty:ty)
        $(from [$($from_type:ty),*])?
        $(ref into [$($ref_into_type:ty),*])?
    );* $(;)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        pub enum SimpleDataType {
            $($struct_name),*
        }

        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        pub enum SimpleDataValue {
            $($struct_name($ty)),*
        }

        impl SimpleDataType {
            $(
                pub const fn $name() -> Self {
                    Self::$struct_name
                }

                pub const $const_name: Self = SimpleDataType::$struct_name;
            )*
        }

        impl fmt::Display for SimpleDataType {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    $(Self::$struct_name => write!(f, "$name")),*
                }
            }
        }

        impl DataType {
            $(
                pub const fn $name() -> Self {
                    Self::Simple(SimpleDataType::$struct_name)
                }

                pub const $const_name: Self = Self::Simple(SimpleDataType::$struct_name);
            )*
        }


        impl SimpleDataValue {
                $(
                    pub const fn $name(value: $ty) -> Self {
                        Self::$struct_name(value)
                    }
                )*

                pub const fn typ(&self) -> SimpleDataType {
                    match self {
                        $(Self::$struct_name(_) => SimpleDataType::$struct_name),*
                    }
                }
        }

        impl fmt::Display for SimpleDataValue {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    $(Self::$struct_name(value) => write!(f, "{value}")),*
                }
            }
        }


        impl DataValue {
                $(
                    pub const fn $name(value: $ty) -> Self {
                        Self::Simple(SimpleDataValue::$struct_name(value))
                    }
                )*

        }

        // Conversions
        $(
            impl HasSimpleDataType for $ty {
                const SIMPLE_DATA_TYPE: SimpleDataType = SimpleDataType::$name();
            }

            impl HasSimpleDataType for &$ty {
                const SIMPLE_DATA_TYPE: SimpleDataType = SimpleDataType::$name();
            }

            $($(
                impl HasSimpleDataType for $from_type {
                    const SIMPLE_DATA_TYPE: SimpleDataType = SimpleDataType::$name();
                }
            )*)?

            $($(
                impl HasSimpleDataType for &$ref_into_type {
                    const SIMPLE_DATA_TYPE: SimpleDataType = SimpleDataType::$name();
                }
            )*)?

            impl TryFrom<SimpleDataValue> for $ty {
                type Error = DataValueConversionError;
                fn try_from(value: SimpleDataValue) -> Result<Self, Self::Error> {
                    match value {
                        SimpleDataValue::$struct_name(x) => Ok(x),
                        other => Err(DataValueConversionError { needed: DataType::$name(), got: DataType::Simple(other.typ()) })
                    }
                }
            }

            impl<'a> TryFrom<&'a SimpleDataValue> for &'a $ty {
                type Error = DataValueConversionError;
                fn try_from(value: &'a SimpleDataValue) -> Result<Self, Self::Error> {
                    match value {
                        SimpleDataValue::$struct_name(x) => Ok(x),
                        other => Err(DataValueConversionError { needed: DataType::$name(), got: DataType::Simple(other.typ()) })
                    }
                }
            }

            // These `DataValue` impls can be defined generically outside because the type variable (i.e., `T`) needs to come before the
            // foreign types (i.e., `TryFrom<DataValue>`).
            impl TryFrom<DataValue> for $ty {
                type Error = DataValueConversionError;
                fn try_from(value: DataValue) -> Result<Self, Self::Error> {
                    match value {
                        DataValue::Simple(x) => x.try_into(),
                        other => Err(DataValueConversionError { needed: DataType::$name(), got: other.typ() })
                    }
                }
            }
            impl<'a> TryFrom<&'a DataValue> for &'a $ty {
                type Error = DataValueConversionError;
                fn try_from(value: &'a DataValue) -> Result<Self, Self::Error> {
                    match value {
                        DataValue::Simple(x) => x.try_into(),
                        other => Err(DataValueConversionError { needed: DataType::$name(), got: other.typ() })
                    }
                }
            }

            impl From<$ty> for SimpleDataValue {
                fn from(value: $ty) -> Self {
                    Self::$struct_name(value)
                }
            }

            $($(
                impl From<$from_type> for DataValue {
                    fn from(value: $from_type) -> Self {
                        Self::$name(value.into())
                    }
                }
            )*)?

            $($(
                impl<'a> TryFrom<&'a SimpleDataValue> for &'a $ref_into_type {
                    type Error = DataValueConversionError;
                    fn try_from(value: &'a SimpleDataValue) -> Result<Self, Self::Error> {
                        match value {
                            SimpleDataValue::$struct_name(x) => Ok(x.as_ref()),
                            other => Err(DataValueConversionError { needed: DataType::$name(), got: DataType::Simple(other.typ()) })
                        }
                    }
                }
            )*)?
        )*

    };
}

pub(self) use define_data;

impl<T: Into<SimpleDataValue>> From<T> for DataValue {
    fn from(value: T) -> Self {
        Self::Simple(value.into())
    }
}
