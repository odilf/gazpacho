//! Streaming ffmpeg decode pipe used by the readers.

use std::fmt;
use std::io::{BufReader, Read as _};
use std::process::{Child, ChildStdout, Command, Stdio};

use eyre::{WrapErr as _, ensure};
use ffmpeg_sidecar::paths::ffmpeg_path;

use crate::read::{Frame, Resolution};

/// Decoded RGBA frames of one video stream, in presentation order, streamed
/// from the start of the file.
pub(crate) struct FramePipe {
    child: Child,
    stdout: BufReader<ChildStdout>,
    resolution: Resolution,
}

impl FramePipe {
    /// Spawn ffmpeg decoding the stream at container index `stream_index` of
    /// `path`, scaled to `resolution`.
    pub fn open(path: &str, stream_index: u8, resolution: Resolution) -> eyre::Result<Self> {
        let mut child = Command::new(ffmpeg_path())
            .args(["-hide_banner", "-loglevel", "error"])
            .args(["-i", path])
            // Explicit absolute stream index: default stream selection can
            // pick a different "best" stream (e.g. cover art) than the one
            // the metadata describes.
            .args(["-map", &format!("0:{stream_index}")])
            // One output frame per coded frame — otherwise ffmpeg CFR-izes
            // VFR files by duplicating frames to fill the timestamp gaps.
            .args(["-fps_mode", "passthrough"])
            .args([
                "-vf",
                &format!("scale={}:{}", resolution.width, resolution.height),
            ])
            .args(["-f", "rawvideo", "-pix_fmt", "rgba", "-"])
            .stdin(Stdio::null())
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .wrap_err("spawning ffmpeg to decode")?;

        let stdout = child.stdout.take().expect("stdout was piped");
        Ok(Self {
            child,
            stdout: BufReader::new(stdout),
            resolution,
        })
    }

    /// The next decoded frame, or `None` once the stream ends cleanly.
    pub fn next_frame(&mut self) -> eyre::Result<Option<Frame>> {
        let frame_size = (self.resolution.width * self.resolution.height * 4) as usize;
        let mut data = vec![0u8; frame_size];

        let mut filled = 0;
        while filled < frame_size {
            #[expect(clippy::indexing_slicing, reason = "filled < frame_size == data.len(), checked by the loop condition")]
            let n = self.stdout.read(&mut data[filled..])?;
            if n == 0 {
                break;
            }
            filled += n;
        }

        if filled == 0 {
            self.check_exit()?;
            return Ok(None);
        }
        ensure!(
            filled == frame_size,
            "truncated frame from ffmpeg: {filled} of {frame_size} bytes ({})",
            self.stderr_tail(),
        );
        Ok(Some(Frame::new(self.resolution, data)))
    }

    /// After a clean EOF on stdout, surface a decode failure as an error
    /// instead of silently looking like a short stream.
    fn check_exit(&mut self) -> eyre::Result<()> {
        let status = self.child.wait().wrap_err("waiting on ffmpeg")?;
        ensure!(
            status.success(),
            "ffmpeg decode failed ({status}): {}",
            self.stderr_tail(),
        );
        Ok(())
    }

    /// Whatever ffmpeg wrote to stderr, for error messages.
    fn stderr_tail(&mut self) -> String {
        let mut out = String::new();
        if let Some(stderr) = self.child.stderr.as_mut()
            && let Err(err) = stderr.read_to_string(&mut out)
        {
            tracing::debug!(?err, "couldn't read ffmpeg's stderr");
        }
        out.trim().to_string()
    }
}

impl fmt::Debug for FramePipe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FramePipe")
            .field("resolution", &self.resolution)
            .finish_non_exhaustive()
    }
}

impl Drop for FramePipe {
    fn drop(&mut self) {
        // We usually stop reading before the stream ends; kill first so
        // wait() can't hang on a process still producing output.
        if let Err(err) = self.child.kill() {
            tracing::debug!(?err, "couldn't kill ffmpeg child (already exited?)");
        }
        if let Err(err) = self.child.wait() {
            tracing::debug!(?err, "couldn't reap ffmpeg child");
        }
    }
}
