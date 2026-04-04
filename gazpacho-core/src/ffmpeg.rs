use color_eyre::eyre::{self, Context, ContextCompat, OptionExt};
use serde::Deserialize;
use std::process::Command;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct VideoMetadata {
    pub fps: f64,
    pub duration: Duration,
    pub frame_count: u64,
}

#[derive(Debug, Deserialize)]
struct ProbeOutput {
    streams: Vec<VideoStream>,
}

#[derive(Debug, Deserialize)]
struct VideoStream {
    r_frame_rate: String,
    nb_frames: Option<String>,
    duration: Option<String>,
}

#[tracing::instrument(level = "debug")]
pub fn get_video_metadata(path: &str) -> eyre::Result<VideoMetadata> {
    let output = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=r_frame_rate,nb_frames,duration",
            "-of",
            "json",
            path,
        ])
        .output()?;

    let probe: ProbeOutput = serde_json::from_slice(&output.stdout)?;

    let stream = probe
        .streams
        .first()
        .ok_or_else(|| eyre::eyre!("No video stream found"))?;

    // Parse fractional FPS (e.g., "30000/1001" or "30/1")
    let fps = parse_rational_fps(&stream.r_frame_rate)?;

    let duration = stream
        .duration
        .as_ref()
        .and_then(|d| d.parse::<f64>().ok())
        .map(std::time::Duration::from_secs_f64)
        .wrap_err_with(|| format!("Couldn't parse duration: {:?}", stream.duration))?;

    let frame_count = stream
        .nb_frames
        .as_ref()
        .and_then(|n| n.parse::<u64>().ok())
        .wrap_err("Couldn't get frame count (look at source, try to use fallback)")?
        // .or_else(|| {
        //     // Fallback: calculate from duration and fps
        //     duration.map(|d| (d.as_secs_f32() * fps) as u64)
        // })
        ;

        tracing::info!(?frame_count);

    Ok(VideoMetadata {
        fps,
        duration,
        frame_count,
    })
}

fn parse_rational_fps(fps_str: &str) -> eyre::Result<f64> {
    let parts: Vec<&str> = fps_str.split('/').collect();
    match parts.as_slice() {
        [num, den] => {
            let numerator = num.parse::<f64>()?;
            let denominator = den.parse::<f64>()?;
            Ok(numerator / denominator)
        }
        _ => eyre::bail!("Invalid FPS format: {}", fps_str),
    }
}

#[tracing::instrument]
pub fn get_keyframes(path: &str) -> eyre::Result<Vec<f64>> {
    let output = Command::new("ffprobe")
        .args([
            "-loglevel",
            "error",
            "-skip_frame",
            "nokey",
            "-select_streams",
            "v:0",
            "-show_entries",
            "frame=best_effort_timestamp_time",
            "-of",
            "compact",
            path,
        ])
        .output()?;

    let stdout = String::from_utf8(output.stdout)?;

    // TODO: Change when try blocks.
    let parse_line = |line: &str| {
        let val = line
            .split('|')
            .nth(1)
            .ok_or_eyre("Missing `|` separators")?
            .split_once('=')
            .ok_or_eyre("Missing `=`")?
            .1;
        val.parse::<f64>()
            .wrap_err_with(|| format!("Couldn't parse {val:?} as float"))
    };

    let keyframes: Vec<f64> = stdout
        .lines()
        .map(|line| {
            parse_line(line)
                .wrap_err_with(|| format!("Couldn't parse ffprobe output line: {line:?}"))
        })
        .collect::<eyre::Result<_>>()?;

    if !keyframes.is_sorted() {
        // TODO: This is easily recoverable, but I want it to be visible.
        eyre::bail!("Keyframes were not sorted! {keyframes:?}")
    }

    tracing::debug!(?keyframes);

    Ok(keyframes)
}
