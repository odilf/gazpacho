use bitflags::bitflags;
use gazpacho_datatypes::{Resolution, Time};

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Request {
    pub time: Time,
    pub resolution: Resolution,
}

impl Request {
    /// A value to pass in when you don't have a request available, for nodes
    /// that you don't expect should depend on the request. I.e., constants.
    pub const fn sentinel() -> Self {
        Self {
            resolution: Resolution {
                width: 0,
                height: 0,
            },
            time: Time::ZERO,
        }
    }

    pub fn select(self, deps: RequestDeps) -> PartialRequest {
        let mut partial = PartialRequest {
            resolution: self.resolution,
            time: self.time,
        };

        if !deps.contains(RequestDeps::TIME) {
            partial.time = Time::ZERO;
        }
        if !deps.contains(RequestDeps::RESOLUTION) {
            partial.resolution = Resolution {
                width: 0,
                height: 0,
            }
        }

        partial
    }
}

/// A [`Request`] where only some of the fields matter. Obtained from [`Request::select`]
///
/// There is an implementation detail leak in the fact that the partial request
/// "forgets" which values it has ignored, so some requests are considered
/// "equal" even though semantically they seem like they shouldn't.
///
/// However, regular equality semantics _are_ guaranteed for partial requests
/// originating from the same [`RequestDeps`].
///
/// Note that this is almost trivial to fix by adding [`RequestDeps`] to the body, but I
/// just think it's unecessary.
// TODO: We could add it only on debug assertions? And then verify that we never compare two different partial requests?
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PartialRequest {
    resolution: Resolution,
    time: Time,
}

bitflags! {
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct RequestDeps: u8 {
        const TIME = 0b00000001;
        const RESOLUTION = 0b00000010;
    }
}
