use crate::data::{Frame, SimpleDataType, SimpleDataValue};
use crate::ffmpeg::{VideoMetadata, get_keyframe_indices, get_video_metadata};
use color_eyre::eyre;
use ffmpeg_sidecar::command::FfmpegCommand;
use std::collections::BTreeMap;

pub trait Track {
    fn length(&self) -> u64;
    fn typ(&self) -> SimpleDataType;
    fn render(&mut self, frame_num: u64) -> SimpleDataValue;
}

pub struct SourceTrack {
    path: String,
    metadata: VideoMetadata,
    keyframes: Vec<u64>,
    cache: BTreeMap<u32, Frame>,
}

impl SourceTrack {
    pub fn new(path: String) -> eyre::Result<Self> {
        let metadata = get_video_metadata(&path)?;
        let keyframes = get_keyframe_indices(&path)?;

        Ok(Self {
            path,
            metadata,
            cache: BTreeMap::new(),
            keyframes,
        })
    }

    fn find_nearest_keyframes(&self, frame_num: u64) -> [u64; 2] {
        debug_assert!(self.keyframes.is_sorted());
        let partition = self.keyframes.partition_point(|&kf| kf <= frame_num);
        [
            self.keyframes[partition - 1],
            self.keyframes
                .get(partition)
                .copied()
                .unwrap_or(self.metadata.frame_count)
                - 1,
        ]
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }
}

impl Track for SourceTrack {
    fn length(&self) -> u64 {
        self.metadata.frame_count
    }

    fn typ(&self) -> SimpleDataType {
        SimpleDataType::VideoFrame
    }

    fn render(&mut self, frame_num: u64) -> SimpleDataValue {
        // Check cache first
        if let Some(frame) = self.cache.get(&(frame_num as u32)) {
            return frame.clone().into();
        }

        let [start, end] = self.find_nearest_keyframes(frame_num);
        let start_time = format!("{:.4}", start as f32 / self.metadata.fps);
        let end_time = format!("{:.4}", end as f32 / self.metadata.fps);

        dbg!(&start_time);

        eprintln!("Should cache from {start} to {end}");

        let iter = FfmpegCommand::new()
            .seek(&start_time)
            .to(&end_time)
            .input(&self.path)
            .rawvideo()
            .spawn()
            .unwrap()
            .iter()
            .unwrap()
            .filter_frames();

        let mut iter = iter.peekable();
        eprintln!(
            "Starting caching at {}",
            iter.peek().unwrap().frame_num + start as u32
        );
        let frame_byte_len = iter.peek().unwrap().width * iter.peek().unwrap().height * 3;

        while let Some(frame) = iter.next() {
            if iter.peek().is_none() {
                eprintln!("Ending caching at {}", frame.frame_num + start as u32)
            }
            self.cache
                .insert(frame.frame_num + start as u32, Frame::from(frame));
        }

        eprintln!(
            "Cache size is now caclulated to be {} MB",
            self.cache.len() * frame_byte_len as usize / 1_000_000
        );

        self.cache
            .get(&(frame_num as u32))
            .expect("Frame should be cached by now.")
            .clone()
            .into()
    }
}
