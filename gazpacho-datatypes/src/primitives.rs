use std::fmt;

use ordered_float::OrderedFloat;
use string_interner::{StringInterner, backend::BucketBackend, symbol::SymbolU32};

/// True or false.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Bool(bool);

/// 64-bit signed integer
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Int(i64);

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Str {
    sym: SymbolU32,
    // We cannot do this yet because `Module` is in `ast` and this is in `datatypes`.
    // #[cfg(debug_assertions)]
    // parent: *const Module,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrInterner(StringInterner<BucketBackend>);

impl StrInterner {
    pub fn new() -> Self {
        Self(StringInterner::new())
    }

    pub fn get_or_intern(&mut self, value: &str) -> Str {
        Str {
            sym: self.0.get_or_intern(value),
        }
    }

    pub fn get_or_intern_static(&mut self, value: &'static str) -> Str {
        Str {
            sym: self.0.get_or_intern_static(value),
        }
    }

    pub fn get(&self, value: &str) -> Option<Str> {
        self.0.get(value).map(|sym| Str { sym })
    }

    // TODO: Document invariant to make `unwarp` safe
    pub fn resolve(&self, value: Str) -> &str {
        // TODO: We could, in debug mode, track _where_ the str comes from and then make sure we never read
        // from a `Module` we don't own. But I guess this would mean storing the `Module`s pointer value?
        #[expect(
            clippy::unwrap_used,
            reason = "Honestly, not yet properly documented. See TODO above."
        )]
        self.0.resolve(value.sym).unwrap()
    }
}

impl Default for StrInterner {
    fn default() -> Self {
        Self::new()
    }
}
