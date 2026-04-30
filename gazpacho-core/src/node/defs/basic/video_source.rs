use std::{cell::RefCell, collections::BTreeMap};

use color_eyre::eyre;
use ffmpeg_sidecar::command::FfmpegCommand;

use crate::{
    data::{
        DataType, Frame, track::{DynTrack, Track}
    },
    ffmpeg::{VideoMetadata, get_keyframes, get_video_metadata},
    node::{NodeSpec, define_node},
};

// define_node! {
//     VIDEO_SOURCE:
//         // TODO: This should return an `impl Track`.
//         // TODO: This should take a `String`.
//         fn video_source(path: &str) -> VideoSourceTrack {
//             VideoSourceTrack::new(path.to_string()).unwrap()
//             // DynTrack::new(VideoSourceTrack::new(path.to_string()).unwrap())
//         }

//         fn fps(path: &str) -> f64 {
//             let metadata = get_video_metadata(path).unwrap();
//             metadata.fps as f64
//         }
// }

const VIDEO_SOURCE: NodeSpec = NodeSpec {
    id: NodeId("video-source"),
    inputs: &[
        DataType::string().named("path")
    ],
    outputs: &[
        (DataType::video_track().named("output"),
            |inputs, _| {
                crate::data::DataValue::Track(DynTrack {
                    render: |inputs, frame| {                
                        let path: &String = inputs[0].try_into().unwrap();
                        let track = VideoSourceTrack::new(path.clone());
                        track.render(frame)
                    },
                    fps: 
                    
                })
            }
        ),
        
    ]
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
