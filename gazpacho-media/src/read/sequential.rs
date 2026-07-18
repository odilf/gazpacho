use std::{cell::RefCell, fmt};

use ffmpeg_sidecar::iter::FfmpegIterator;

use crate::read::{Frame, MediaTime, ResolutionRequest};

/// Media reader optimized for sequential access.
#[derive(Debug, Default)]
pub struct SequentialReader {
    state: RefCell<Option<State>>,
}

struct State {
    iterator: FfmpegIterator,
    frame_index: u32,
    time: MediaTime,
    path: String,
}

impl fmt::Debug for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("State")
            .field("iterator", &"<ffmpeg-sidecar-iterator>")
            .field("frame_index", &self.frame_index)
            .field("time", &self.time)
            .field("path", &self.path)
            .finish()
    }
}

impl State {
    /// Try to get the
    fn try_next_frame(&mut self, path: &str, time: MediaTime) -> eyre::Result<Frame> {
        if self.path != path {
            eyre::bail!("Iterating video different from requested.")
        }

        if self.time > time {
            eyre::bail!("State is too far forward.")
        }

        while let Some(event) = self.iterator.next() {
            dbg!(event);
            // match event {
            //     FfmpegEvent::OutputFrame(frame) => {}
            // }
        }

        todo!()
    }
}

impl SequentialReader {
    pub fn frame(
        &self,
        path: &str,
        time: MediaTime,
        resolution: ResolutionRequest,
    ) -> eyre::Result<Frame> {
        if let Some(state) = self.state.borrow_mut().as_mut() {
            match state.try_next_frame(path, time) {
                Ok(frame) => return Ok(frame),
                Err(err) => tracing::debug!(?err, "Non-sequential access."),
            }
        }

        todo!()
    }
}
