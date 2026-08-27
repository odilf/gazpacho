use std::cell::RefCell;
use std::ops::Range;

use eyre::{self, Context as _};

use crate::metadata::{MediaTime, Timing, VideoMetadata};
use crate::read::pipe::FramePipe;
use crate::read::{Frame, Resolution};

/// Media reader optimized for sequential access.
///
/// Keeps one long-lived ffmpeg pipe decoding forward from the start of the file
/// and advances it frame by frame. If you request a previous frame, it throws
/// out the process and start re-decoding from the start.
///
/// This implementation does no seeking. In essence, it is a dumb implementation
/// that can serve as the correctness reference for fancier access strategies.
#[derive(Debug, Default)]
pub struct SequentialReader {
    state: RefCell<Option<State>>,
}

#[derive(Debug)]
struct State {
    pipe: FramePipe,
    frame_index: i32,
    window: Range<MediaTime>,
    path: String,
    resolution: Resolution,
}

impl State {
    fn new(path: &str, meta: &VideoMetadata, resolution: Resolution) -> eyre::Result<Self> {
        Ok(State {
            pipe: FramePipe::open(path, meta.stream_index, resolution)?,
            // Set to a "-1" state so that `Self::advance` starts by going to the 0th state.
            frame_index: -1,
            window: meta.start..meta.start,
            path: path.to_string(),
            resolution,
        })
    }

    /// Advances the state to the next frame and returns the frame associated with the _new_ state.
    ///
    /// First call to advance returns the 0th frame.
    fn advance(&mut self, meta: &VideoMetadata) -> eyre::Result<Frame> {
        let frame = self.pipe.next_frame()?.ok_or_else(|| {
            eyre::eyre!(
                "video ran out prematurely (frame {}, path={})",
                self.frame_index,
                self.path
            )
        })?;

        self.frame_index += 1;
        self.window.start = self.window.end;
        self.window.end = match &meta.timing {
            Timing::Constant(fps) => self.window.end.advance_secs(fps.frame_length()),
            Timing::Variable(timestamps) => {
                #[expect(
                    clippy::cast_sign_loss,
                    reason = "frame_index was just incremented above, so it's always >= 0 here"
                )]
                let next = self.frame_index as usize + 1;
                timestamps.get(next).copied().unwrap_or(meta.extent().end)
            }
        };

        Ok(frame)
    }
}

impl SequentialReader {
    pub fn new() -> Self {
        Self {
            state: RefCell::new(None),
        }
    }

    /// Gets the requested frame by decoding all frames in order until we reach the correct one.
    ///
    /// Assumes `time` is within `meta.extent()`.
    pub fn frame(
        &self,
        path: &str,
        time: MediaTime,
        resolution: Resolution,
        meta: &VideoMetadata,
    ) -> eyre::Result<Frame> {
        let _enter =
            tracing::trace_span!("getting sequential frame", ?path, time=?time.0).entered();
        debug_assert!(meta.extent().contains(&time));

        let mut slot = self.state.borrow_mut();

        // Invalidate non-sequential accesses.
        slot.take_if(|state| {
            if state.window.end > time {
                tracing::debug!(?state.window, ?time, "non-sequential (backward) access");
                return true;
            }
            state.resolution != resolution || state.path.as_str() != path
        });

        // Repopulate if necessary
        if slot.is_none() {
            *slot = Some(State::new(path, meta, resolution)?)
        }

        // Ugh. `Option::get_or_try_insert_with` is unstable...
        #[expect(clippy::unwrap_used, reason = "just populated above if it was None")]
        let state = slot.as_mut().unwrap();

        loop {
            let frame = state.advance(meta).wrap_err("Couldn't advance frame")?;
            if state.window.contains(&time) {
                return Ok(frame);
            }
        }
    }
}

// TODO(test):
// - doesn't go backward unecessarily
