use crate::data::{
    DataType, DataValue, DataValueConversionError, HasDataType, HasSimpleDataType, SimpleDataType,
    SimpleDataValue,
};
use color_eyre::eyre;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FrameIndex(u32);

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Fps(pub f32);

#[derive(Debug, Clone, Copy)]
pub struct DynTrack {
    // TODO: un-pub
    pub render: fn(&[&DataValue], FrameIndex) -> SimpleDataValue,
    pub fps: Fps,
    pub len: u32,
    pub typ: SimpleDataType,
}

fn render_to_dyn<T: HasSimpleDataType>(render: fn(&[&DataValue, FrameIndex]) -> T) -> fn(&[&DataValue], FrameIndex) ->  {
    
}

impl DynTrack {
    pub fn new<T: HasSimpleDataType, Tr: Track<Ty = T>>(track: Tr) -> Self {
        Self {
            render: track.render_from_scratch(),
            fps: track.fps(),
            len: track.len(),
            typ: T::SIMPLE_DATA_TYPE,
        }
    }

    pub fn from_dyn<Tr: Track<Ty = SimpleDataValue>>(track: Tr, typ: SimpleDataType) -> Self {
        Self {
            render: track.render_from_scratch(),
            fps: track.fps(),
            len: track.len(),
            typ,
        }
    }

    pub fn fps(&self) -> Fps {
        self.fps
    }

    pub fn len(&self) -> u32 {
        self.len
    }

    pub fn typ(&self) -> SimpleDataType {
        self.typ
    }

    pub fn frame_indices(&self) -> impl ExactSizeIterator<Item = FrameIndex> + use<> {
        (0..self.len()).map(FrameIndex)
    }
}

pub trait Track {
    type Ty;

    /// Number of frames in the track
    fn len(&self) -> u32;

    /// Frames per second.
    fn fps(&self) -> Fps;

    fn render(&self, frame: FrameIndex) -> Self::Ty;
    fn render_from_scratch(&self) -> fn(&[&DataValue], FrameIndex) -> Self::Ty;

    /// Time of the track
    fn length(&self) -> f32 {
        self.len() as f32 / self.fps().0
    }

    fn frame_length(&self) -> f32 {
        1.0 / self.fps().0
    }

    fn to_frame_index(&self, time: f32) -> eyre::Result<FrameIndex> {
        let tol: f32 = self.fps().0 * 0.1;
        let f = self.fps().0 * time;
        let r = f.round();
        let diff = f - r;
        if diff.abs() >= tol {
            eyre::bail!("Timestamp {time} doesn't land on a frame (off by {diff})")
        }

        Ok(FrameIndex(r as u32))
    }
}

impl From<DynTrack> for DataValue {
    fn from(value: DynTrack) -> Self {
        Self::Track(value)
    }
}

impl TryFrom<DataValue> for DynTrack {
    type Error = DataValueConversionError;
    fn try_from(value: DataValue) -> Result<Self, Self::Error> {
        match value {
            DataValue::Track(track) => Ok(track),
            other => Err(DataValueConversionError {
                needed: DataType::GenericTrack,
                got: other.typ(),
            }),
        }
    }
}

impl<'a> TryFrom<&'a DataValue> for &'a DynTrack {
    type Error = DataValueConversionError;
    fn try_from(value: &'a DataValue) -> Result<Self, Self::Error> {
        match value {
            DataValue::Track(track) => Ok(track),
            other => Err(DataValueConversionError {
                needed: DataType::GenericTrack,
                got: other.typ(),
            }),
        }
    }
}

impl Track for DynTrack {
    type Ty = SimpleDataValue;
    fn len(&self) -> u32 {
        self.len
    }

    fn fps(&self) -> Fps {
        self.fps
    }

    fn render(&self, frame: FrameIndex) -> Self::Ty {
        unimplemented!("whoposie daisy")
    }

    fn render_from_scratch(&self, inputs: &[&DataValue], frame: FrameIndex) -> Self::Ty {
        (self.render)(inputs, frame)
    }
}

// pub struct DynTrackShim<T>(T);

// impl<T: HasSimpleDataType + Into<SimpleDataValue>, Tr: Track<Ty = T>> Track for DynTrackShim<Tr> {
//     type Ty = SimpleDataValue;
//     fn len(&self) -> u64 {
//         self.0.len()
//     }

//     fn fps(&self) -> f64 {
//         self.0.fps()
//     }

//     fn render(&self, frame_num: u64) -> Self::Ty {
//         self.0.render(frame_num).into()
//     }
// }

// struct ConcreteTrackShim<T>(PhantomData<T>, Box<dyn Track<Ty = SimpleDataValue>>);

// impl<T> Track for ConcreteTrackShim<T>
// where
//     SimpleDataValue: TryInto<T>,
//     <SimpleDataValue as TryInto<T>>::Error: fmt::Debug,
// {
//     type Ty = T;
//     fn len(&self) -> u64 {
//         self.1.len()
//     }

//     fn fps(&self) -> f64 {
//         self.1.fps()
//     }

//     fn render(&self, frame_num: u64) -> Self::Ty {
//         self.1.render(frame_num).try_into().unwrap()
//     }
// }
