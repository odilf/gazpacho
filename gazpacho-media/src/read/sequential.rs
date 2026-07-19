use std::cell::RefCell;
use std::ops::Range;

use num_rational::Ratio;

use crate::metadata::{MediaTime, Timing, VideoMetadata};
use crate::read::pipe::FramePipe;
use crate::read::{Frame, Resolution};

/// Media reader optimized for sequential access.
///
/// Keeps one long-lived ffmpeg pipe decoding forward from the start of the
/// file and advances it frame by frame. There is no seeking: a backward jump
/// restarts the pipe and re-decodes from the beginning. That makes this
/// implementation trivially correct for any access order — it serves as the
/// correctness reference for fancier access strategies.
#[derive(Debug, Default)]
pub struct SequentialReader {
    state: RefCell<Option<State>>,
}

#[derive(Debug)]
struct State {
    pipe: FramePipe,
    /// Index of the next frame the pipe will produce.
    next_index: u32,
    /// The frame at `next_index - 1`, kept so repeated queries within one
    /// frame's display window don't touch the pipe.
    last: Option<Frame>,
    path: String,
    resolution: Resolution,
}

impl SequentialReader {
    pub fn frame(
        &self,
        path: &str,
        time: MediaTime,
        resolution: Resolution,
        meta: &VideoMetadata,
    ) -> eyre::Result<Frame> {
        let mut slot = self.state.borrow_mut();

        // Drop state that can't serve this request: wrong file or resolution
        // (expected churn), or a pipe already decoded past `time` (a backward
        // jump, which this reader only handles by starting over).
        if let Some(state) = slot.as_ref() {
            if state.path != path || state.resolution != resolution {
                *slot = None;
            } else if time < window_of(meta, state.next_index.saturating_sub(1)).start {
                tracing::warn!(
                    %time,
                    path,
                    "non-sequential (backward) access; re-decoding from the start"
                );
                *slot = None;
            }
        }

        let state = match slot.as_mut() {
            Some(state) => state,
            None => slot.insert(State {
                pipe: FramePipe::open(path, meta.stream_index, resolution)?,
                next_index: 0,
                last: None,
                path: path.to_string(),
                resolution,
            }),
        };

        // The most recently produced frame may still cover `time`.
        if let Some(last) = &state.last
            && window_of(meta, state.next_index - 1).contains(&time)
        {
            return Ok(last.clone());
        }

        // Advance until the produced frame's display window contains `time`.
        loop {
            let index = state.next_index;
            let frame = state.pipe.next_frame()?.ok_or_else(|| {
                eyre::eyre!("stream ended at frame {index}, before reaching t={time}")
            })?;
            state.next_index = index + 1;
            if window_of(meta, index).contains(&time) {
                state.last = Some(frame.clone());
                return Ok(frame);
            }
        }
    }
}

/// Presentation timestamp of frame `index`; `index == frame_count` yields the
/// stream's end, so consecutive timestamps delimit display windows.
fn timestamp_of(meta: &VideoMetadata, index: u32) -> MediaTime {
    debug_assert!(index <= meta.frame_count);
    if index >= meta.frame_count {
        return meta.end;
    }
    match &meta.timing {
        Timing::Constant(fps) => meta
            .start
            .advance_secs(Ratio::from_integer(u64::from(index)) * fps.frame_length()),
        Timing::Variable(timestamps) => timestamps[index as usize],
    }
}

/// Display window of frame `index`: it is visible for `t` in
/// `[timestamp_of(index), timestamp_of(index + 1))`.
fn window_of(meta: &VideoMetadata, index: u32) -> Range<MediaTime> {
    timestamp_of(meta, index)..timestamp_of(meta, index + 1)
}

#[cfg(test)]
mod tests {
    use crate::metadata::Fps;
    use crate::read::Resolution;

    use super::*;

    fn meta(timing: Timing, start: Ratio<i64>, frame_count: u32, end: Ratio<i64>) -> VideoMetadata {
        VideoMetadata {
            resolution: Resolution {
                width: 64,
                height: 48,
            },
            timing,
            start: MediaTime(start),
            frame_count,
            end: MediaTime(end),
            time_base: Ratio::new(1, 1000),
            keyframes: Box::new([0]),
            stream_index: 0,
            parent_stream_index: None,
            attached_pic: false,
        }
    }

    fn at(secs: Ratio<i64>) -> MediaTime {
        MediaTime(secs)
    }

    /// NTSC rate with a nonzero start: timestamps stay exact rationals, and
    /// the sentinel index `frame_count` is the stream end.
    #[test]
    fn cfr_timestamps_are_exact() {
        let fps = Fps(Ratio::new(24000, 1001));
        let start = Ratio::new(7, 5);
        let end = start + Ratio::new(10 * 1001, 24000);
        let meta = meta(Timing::Constant(fps), start, 10, end);

        assert_eq!(timestamp_of(&meta, 0), at(start));
        assert_eq!(
            timestamp_of(&meta, 3),
            at(start + Ratio::new(3 * 1001, 24000))
        );
        assert_eq!(timestamp_of(&meta, 10), at(end));
    }

    /// VFR gives back the exact probed timestamps, with the same end sentinel.
    #[test]
    fn vfr_timestamps_come_from_the_table() {
        let stamps = [0, 33, 100, 400].map(|ms| MediaTime(Ratio::new(ms, 1000)));
        let end = Ratio::new(450, 1000);
        let meta = meta(Timing::Variable(Box::new(stamps)), Ratio::new(0, 1), 4, end);

        for (i, stamp) in stamps.iter().enumerate() {
            assert_eq!(timestamp_of(&meta, i as u32), *stamp);
        }
        assert_eq!(timestamp_of(&meta, 4), at(end));
    }

    /// Windows are half-open: the frame's own timestamp is inside, the next
    /// frame's timestamp is not, and anything in between belongs to it.
    #[test]
    fn windows_are_half_open() {
        let fps = Fps(Ratio::from_integer(10));
        let meta = meta(Timing::Constant(fps), Ratio::new(0, 1), 5, Ratio::new(1, 2));

        let window = window_of(&meta, 2);
        assert!(
            window.contains(&at(Ratio::new(2, 10))),
            "start is inclusive"
        );
        assert!(window.contains(&at(Ratio::new(29, 100))), "mid-window");
        assert!(!window.contains(&at(Ratio::new(3, 10))), "end is exclusive");
        assert!(
            !window.contains(&at(Ratio::new(19, 100))),
            "before the start"
        );

        // The last frame's window is closed by the stream end.
        let last = window_of(&meta, 4);
        assert!(last.contains(&at(Ratio::new(49, 100))));
        assert!(!last.contains(&at(Ratio::new(1, 2))));
    }
}
