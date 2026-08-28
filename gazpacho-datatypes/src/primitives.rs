use std::fmt;

use ordered_float::OrderedFloat;
use string_interner::{StringInterner, backend::BucketBackend, symbol::SymbolU32};

/// True or false.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Bool(bool);

/// 64-bit signed integer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Int(i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Float(OrderedFloat<f64>);

macro_rules! impl_wrapper {
    ($outer:tt($inner:ty $(, $other:ty),*)) => {
        impl From<$inner> for $outer {
            fn from(value: $inner) -> Self {
                $outer(value)
            }
        }

        impl From<$outer> for $inner {
            fn from(value: $outer) -> Self {
                value.0
            }
        }

        $(
            impl From<$other> for $outer {
                fn from(value: $other) -> Self {
                    $outer(value.into())
                }
            }

            impl From<$outer> for $other {
                fn from(value: $outer) -> Self {
                    value.0.into()
                }
            }
        )*

        impl fmt::Display for $outer {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

impl_wrapper!(Bool(bool));
impl_wrapper!(Int(i64));
impl_wrapper!(Float(OrderedFloat<f64>, f64));

/// Strings, interned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Str(SymbolU32);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrInterner(StringInterner<BucketBackend>);

impl StrInterner {
    pub fn new() -> Self {
        Self(StringInterner::new())
    }

    pub fn get_or_intern(&mut self, value: &str) -> Str {
        Str(self.0.get_or_intern(value))
    }

    pub fn get_or_intern_static(&mut self, value: &'static str) -> Str {
        Str(self.0.get_or_intern_static(value))
    }

    pub fn resolve(&self, value: Str) -> Option<&str> {
        self.0.resolve(value.0)
    }
}
