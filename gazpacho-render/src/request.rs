use gazpacho_datatypes::Time;
use gazpacho_media::read::ResolutionRequest;
use gazpacho_operations::RequestDeps;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Request {
    pub resolution: ResolutionRequest,
    pub time: Time,
}

impl Request {
    /// A value to pass in when you don't have a request available, for nodes
    /// that you don't expect should depend on the request, i.e., constants.
    pub const fn sentinel() -> Self {
        Self {
            resolution: ResolutionRequest::auto(),
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
            partial.resolution = ResolutionRequest::auto()
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
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PartialRequest {
    resolution: ResolutionRequest,
    time: Time,
}
