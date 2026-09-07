use std::{
    path::PathBuf,
    process::{Command, Stdio},
};

use eyre::Context as _;

use super::{Frame, Resolution};

/// Decode *every* frame of a file to RGBA via a plain ffmpeg pipe, in
/// presentation order.
///
/// This is a decode path independent of `gazpacho_media::read`, used to
/// validate the fixtures themselves and as a reference to compare the reader
/// against.
pub fn decode_all_rgba(path: &str, resolution: Resolution) -> eyre::Result<Vec<Frame>> {
    decode_rgba(path, None, resolution, None)
}

/// Like [`decode_all_rgba`], but reads the stream at container index
/// `stream_index` and stops after `limit` frames. Useful for comparing against
/// a reader on multi-track or real-world files too large to hold decoded in
/// memory.
pub fn decode_rgba_prefix(
    path: &str,
    stream_index: u8,
    resolution: Resolution,
    limit: usize,
) -> eyre::Result<Vec<Frame>> {
    decode_rgba(path, Some(stream_index), resolution, Some(limit))
}

fn decode_rgba(
    path: &str,
    stream_index: Option<u8>,
    resolution: Resolution,
    limit: Option<usize>,
) -> eyre::Result<Vec<Frame>> {
    let Resolution { width, height } = resolution;
    let mut cmd = Command::new(ffmpeg_path());
    cmd.args(["-hide_banner", "-loglevel", "error"])
        .arg("-i")
        .arg(path)
        // One output frame per coded frame, otherwise ffmpeg CFR-izes VFR.
        .args(["-fps_mode", "passthrough"]);

    if let Some(index) = stream_index {
        cmd.args(["-map", &format!("0:{index}")]);
    }
    if let Some(limit) = limit {
        cmd.args(["-frames:v", &limit.to_string()]);
    }
    let output = cmd
        .args(["-f", "rawvideo", "-pix_fmt", "rgba", "-"])
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .wrap_err("running ffmpeg to decode")?;
    eyre::ensure!(
        output.status.success(),
        "ffmpeg decode failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );

    let frame_size = (width * height * 4) as usize;
    eyre::ensure!(
        output.stdout.len().is_multiple_of(frame_size),
        "decoded byte count {} is not a whole number of {width}x{height} RGBA frames",
        output.stdout.len()
    );
    Ok(output
        .stdout
        .chunks_exact(frame_size)
        .map(|data| Frame::new(resolution, data))
        .collect())
}

/// The ffmpeg binary to use: `FFMPEG_PATH` when set (matching the Python
/// generator), else an adjacent sidecar, else `ffmpeg` on `PATH`.
fn ffmpeg_path() -> PathBuf {
    std::env::var_os("FFMPEG_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(ffmpeg_sidecar::paths::ffmpeg_path)
}
