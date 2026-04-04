use crate::data::{Frame, HasSimpleDataType, SimpleDataType, SimpleDataValue};
use crate::ffmpeg::{VideoMetadata, get_keyframes, get_video_metadata};
use color_eyre::eyre;
use ffmpeg_sidecar::command::FfmpegCommand;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ops;

pub trait Track {
    type Ty;

    /// Number of frames in the track
    fn len(&self) -> u64;

    /// Frames per second.
    fn fps(&self) -> f64;
    fn render(&self, frame_num: u64) -> Self::Ty;

    /// Time of the track
    fn length(&self) -> f64 {
        self.len() as f64 / self.fps()
    }

    fn frame_length(&self) -> f64 {
        1.0 / self.fps()
    }

    fn to_frame_index(&self, time: f64) -> eyre::Result<u64> {
        let tol: f64 = self.fps() * 0.1;
        let f = self.fps() * time;
        let r = f.round();
        let diff = f - r;
        if diff.abs() >= tol {
            eyre::bail!("Timestamp {time} doesn't land on a frame (off by {diff})")
        }

        Ok(r as u64)
    }
}

pub struct DynTrack {
    typ: SimpleDataType,
    track: Box<dyn Track<Ty = SimpleDataValue>>,
}

impl DynTrack {
    pub fn typ(&self) -> SimpleDataType {
        self.typ
    }

    pub fn track(&self) -> &dyn Track<Ty = SimpleDataValue> {
        self.track.as_ref()
    }

    pub fn new<T: HasSimpleDataType + Into<SimpleDataValue>>(
        track: impl Track<Ty = T> + 'static,
    ) -> Self {
        Self {
            typ: T::SIMPLE_DATA_TYPE,
            track: Box::new(DynTrackShim(track)),
        }
    }
}

impl ops::Deref for DynTrack {
    type Target = dyn Track<Ty = SimpleDataValue>;
    fn deref(&self) -> &Self::Target {
        self.track.as_ref()
    }
}

impl ops::DerefMut for DynTrack {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.track.as_mut()
    }
}

pub struct DynTrackShim<T>(T);

impl<T: HasSimpleDataType + Into<SimpleDataValue>, Tr: Track<Ty = T>> Track for DynTrackShim<Tr> {
    type Ty = SimpleDataValue;
    fn len(&self) -> u64 {
        self.0.len()
    }

    fn fps(&self) -> f64 {
        self.0.fps()
    }

    fn render(&self, frame_num: u64) -> Self::Ty {
        self.0.render(frame_num).into()
    }
}

pub struct VideoSourceTrack {
    path: String,
    metadata: VideoMetadata,
    keyframes: Vec<f64>,
    cache: RefCell<BTreeMap<u32, Frame>>,
}

impl VideoSourceTrack {
    pub fn new(path: String) -> eyre::Result<Self> {
        let metadata = get_video_metadata(&path)?;
        let keyframes = get_keyframes(&path)?;

        Ok(Self {
            path,
            metadata,
            cache: RefCell::new(BTreeMap::new()),
            keyframes,
        })
    }

    fn find_nearest_keyframes(&self, time: f64) -> [f64; 2] {
        debug_assert!(self.keyframes.is_sorted());
        let partition = self.keyframes.partition_point(|&kf_time| kf_time <= time);
        [
            partition
                .checked_sub(1)
                .and_then(|p| self.keyframes.get(p).copied())
                .unwrap_or(0.0),
            self.keyframes.get(partition).copied().unwrap_or_else(|| {
                tracing::debug!("No keyframes after, getting length");
                self.length()
            }),
        ]
    }

    pub fn clear_cache(&self) {
        self.cache.borrow_mut().clear();
    }
}

impl Track for VideoSourceTrack {
    type Ty = Frame;

    fn len(&self) -> u64 {
        self.metadata.frame_count
    }

    fn fps(&self) -> f64 {
        self.metadata.fps
    }

    fn render(&self, frame_num: u64) -> Frame {
        // Check cache first
        if let Some(frame) = self.cache.borrow().get(&(frame_num as u32)) {
            return frame.clone();
        }

        let time = frame_num as f64 / self.fps();

        let [start_time, end_time] = self.find_nearest_keyframes(time);
        let start_frame = self.to_frame_index(start_time).unwrap();

        tracing::info!(
            "Should cache from {start_time} (frame {start_frame}) to {end_time} (frame {})",
            self.to_frame_index(end_time).unwrap()
        );

        let iter = FfmpegCommand::new()
            .seek(start_time.to_string())
            .to((end_time + self.frame_length()).to_string())
            .input(&self.path)
            .rawvideo()
            .spawn()
            .unwrap()
            .iter()
            .unwrap()
            .filter_frames();

        let mut iter = iter.peekable();
        let frame_byte_len = iter.peek().unwrap().width * iter.peek().unwrap().height * 3;

        tracing::debug!(
            "Starting cache at {}",
            start_time + iter.peek().unwrap().frame_num as f64 / self.fps()
        );

        while let Some(frame) = iter.next() {
            if iter.peek().is_none() {
                tracing::debug!(
                    "Ending cache at {} (frame {})",
                    start_time + frame.frame_num as f64 / self.fps(),
                    start_frame + frame.frame_num as u64
                );
            }
            self.cache
                .borrow_mut()
                .insert(start_frame as u32 + frame.frame_num, Frame::from(frame));
        }

        tracing::debug!(
            "Cache size is now caclulated to be {} MB",
            self.cache.borrow().len() * frame_byte_len as usize / 1_000_000
        );

        self.cache
            .borrow()
            .get(&(frame_num as u32))
            .ok_or_else(|| eyre::eyre!("Frame {frame_num} should be cached by now."))
            .unwrap()
            .clone()
    }
}

// TODO: Remove
// macro_rules! make_dyn_track {
//     ($concrete:ty, $dyn:ident) => {
//         pub struct $dyn($concrete);

//         impl Track for $dyn {
//             type Ty = SimpleDataValue;
//             fn len(&self) -> u64 {
//                 self.0.len()
//             }
//             fn fps(&self) -> f64 {
//                 self.0.fps()
//             }
//             fn render(&self, frame_index: u64) -> SimpleDataValue {
//                 self.0.render(frame_index).into()
//             }
//         }

//         impl $dyn {
//             pub fn to_dyn_track(self) -> DynTrack {
//                 DynTrack {
//                     typ: <<$concrete as Track>::Ty as HasSimpleDataType>::SIMPLE_DATA_TYPE,
//                     track: Box::new(self),
//                 }
//             }
//         }

//         impl ::std::ops::Deref for $dyn {
//             type Target = $concrete;
//             fn deref(&self) -> &Self::Target {
//                 &self.0
//             }
//         }

//         impl ::std::ops::DerefMut for $dyn {
//             fn deref_mut(&mut self) -> &mut Self::Target {
//                 &mut self.0
//             }
//         }
//     };
// }

// make_dyn_track!(VideoSourceTrack, DynVideoSourceTrack);
