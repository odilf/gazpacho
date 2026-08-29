//! The types of the gazpacho data model.
//!
//! Includes classic primitives like integers, floats, bools, strings; but also
//! video-specific types such as video frames, time and fps (as rationals for
//! exact arithmetic).
//!
//! Essentially, the types of the "gazapcho runtime".

mod primitives;
pub use primitives::*;

mod media;
pub use media::*;

macro_rules! def_value_enum {
    (
        $($Type:ident, $type:ident);* $(;)?
    ) => {
        /// A simple, copyable, plain-old-data type.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum SimpleValue {
            $($Type($Type)),*
        }

        impl SimpleValue {
            $(
                #[must_use]
                pub fn $type(self) -> Option<$Type> {
                    match self {
                        Self::$Type(v) => Some(v),
                        _ => None,
                    }
                }
            )*
        }
    };
}

def_value_enum! {
    Bool, bool;
    Int, int;
    Float, float;
    Time, time;
    Str, str;
}
