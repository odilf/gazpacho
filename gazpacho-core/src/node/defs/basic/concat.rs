use crate::{
    data::{
        DataType, Frame, HasDataType,
        track::{DynTrack, Track},
    },
    node::{NodeId, NodeSpec},
};

fn concat<T: HasDataType>(a: impl Track<Ty = T>, b: impl Track<Ty = T>) -> impl Track<Ty = T> {
    ConcatTrack::new(a, b).unwrap()
}

// define_node!(concat: fn(DynTrack, DynTrack) -> { output: DynTrack });
const CONCAT_VIDEO: NodeSpec = NodeSpec {
    id: NodeId("concat-video"),
    inputs: &[
        DataType::GenericTrack.named("a"),
        DataType::GenericTrack.named("b"),
    ],
    outputs: &[(DataType::video_track().named("output"), |inputs, _| {
        let mut inputs = inputs.iter().copied();
        let a: &DynTrack = inputs.next().unwrap().try_into().unwrap();
        let b: &DynTrack = inputs.next().unwrap().try_into().unwrap();
        assert_eq!(a.typ(), b.typ());

        DynTrack::from_dyn(ConcatTrack::new(a.clone(), b.clone()), a.typ()).into()
    })],
};

struct ConcatTrack<T, A: Track<Ty = T>, B: Track<Ty = T>>(A, B);

impl<T, A, B> ConcatTrack<T, A, B>
where
    A: Track<Ty = T>,
    B: Track<Ty = T>,
{
    pub fn new(a: A, b: B) -> Option<Self> {
        (a.fps() == b.fps()).then(|| Self(a, b))
    }
}

impl<T, A, B> Track for ConcatTrack<T, A, B>
where
    A: Track<Ty = T>,
    B: Track<Ty = T>,
{
    type Ty = T;
    fn len(&self) -> u64 {
        self.0.len() + self.1.len()
    }

    fn fps(&self) -> f64 {
        debug_assert_eq!(self.0.fps(), self.1.fps());
        self.0.fps()
    }

    fn render(&self, frame_num: u64) -> Self::Ty {
        if frame_num < self.0.len() {
            self.0.render(frame_num)
        } else {
            self.1.render(frame_num - self.0.len())
        }
    }
}
