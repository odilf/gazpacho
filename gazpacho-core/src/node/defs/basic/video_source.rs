use std::{cell::RefCell, collections::BTreeMap};

use color_eyre::eyre;
use ffmpeg_sidecar::command::FfmpegCommand;

use crate::{
    data::{DataType, Frame, track::Track},
    ffmpeg::{VideoMetadata, get_keyframes, get_video_metadata},
    node::{NodeId, NodeSpec},
};

pub const VIDEO_SOURCE: NodeSpec = NodeSpec {
    id: NodeId("video-source"),
    inputs_ref: &[
        DataType::string().named("path"),
        DataType::int().named("frame-index"),
    ],
    inputs_own: &[],
    outputs: &[
        (
            DataType::vframe().named("output"),
            |inputs_ref, _inputs_own, data| {
                let path = <&str>::try_from(inputs_ref[0])?;
                let frame_index = *<&i64>::try_from(inputs_ref[1])?;

                let track = data.downcast_mut::<Option<VideoSourceTrack>>().unwrap();
                let track = match track.as_ref() {
                    Some(track) => &track,
                    None => &*track.insert(VideoSourceTrack::new(path)?),
                };

                Ok(track.render(u64::try_from(frame_index)?, path).into())
            },
        ),
        (
            DataType::float().named("fps"),
            |inputs_ref, _inputs_own, data| {
                let path = <&str>::try_from(inputs_ref[0])?;
                let source = match data.downcast_ref::<VideoSourceTrack>() {
                    Some(data) => data,
                    None => &VideoSourceTrack::new(path)?,
                };

                Ok(source.fps().into())
            },
        ),
        (
            DataType::float().named("len"),
            |inputs_ref, _inputs_own, data| {
                let path = <&str>::try_from(inputs_ref[0])?;
                let source = match data.downcast_ref::<VideoSourceTrack>() {
                    Some(data) => data,
                    None => &VideoSourceTrack::new(path)?,
                };

                Ok(i64::try_from(source.len())?.into())
            },
        ),
    ],
    init_data: || Box::new(None::<VideoSourceTrack>),
};

pub struct VideoSourceTrack {
    metadata: VideoMetadata,
    keyframes: Vec<f64>,
    cache: RefCell<BTreeMap<u32, Frame>>,
}

impl VideoSourceTrack {
    pub fn new(path: &str) -> eyre::Result<Self> {
        let metadata = get_video_metadata(path)?;
        let keyframes = get_keyframes(path)?;

        Ok(Self {
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

    fn render(&self, frame_num: u64, path: &str) -> Frame {
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
            .input(path)
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

impl Track for VideoSourceTrack {
    type Ty = Frame;

    fn len(&self) -> u64 {
        self.metadata.frame_count
    }

    fn fps(&self) -> f64 {
        self.metadata.fps
    }
}
