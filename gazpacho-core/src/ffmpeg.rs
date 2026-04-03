use color_eyre::eyre::{self, ContextCompat};
use serde::Deserialize;
use std::process::Command;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct VideoMetadata {
    pub fps: f32,
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

    Ok(VideoMetadata {
        fps,
        duration,
        frame_count,
    })
}

fn parse_rational_fps(fps_str: &str) -> eyre::Result<f32> {
    let parts: Vec<&str> = fps_str.split('/').collect();
    match parts.as_slice() {
        [num, den] => {
            let numerator = num.parse::<f32>()?;
            let denominator = den.parse::<f32>()?;
            Ok(numerator / denominator)
        }
        _ => eyre::bail!("Invalid FPS format: {}", fps_str),
    }
}

pub fn get_keyframe_indices(path: &str) -> eyre::Result<Vec<u64>> {
    let output = Command::new("ffprobe")
        .args([
            "-loglevel",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "frame=pict_type",
            "-of",
            "csv=print_section=0",
            path,
        ])
        .output()?;

    let stdout = String::from_utf8(output.stdout)?;

    let keyframes: Vec<u64> = stdout
        .lines()
        .enumerate()
        .filter(|(_, line)| line.trim() == "I")
        .map(|(idx, _)| idx as u64)
        .collect();

    assert!(keyframes.is_sorted());

    Ok(keyframes)
}
