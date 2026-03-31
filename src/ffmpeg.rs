use color_eyre::eyre;
use ffmpeg_sidecar::command::FfmpegCommand;
use ffmpeg_sidecar::event::FfmpegEvent;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct VideoMetadata {
    pub fps: f32,
    pub duration: Duration,
    pub frame_count: u64,
}

pub fn get_frame_count(file_path: &str) -> eyre::Result<VideoMetadata> {
    let ffmpeg = FfmpegCommand::new()
        .input(file_path)
        .spawn()?
        .iter()
        .unwrap();

    let mut video_fps: Option<f32> = None;
    let mut video_duration: Option<Duration> = None;

    for event in ffmpeg {
        match event {
            FfmpegEvent::ParsedDuration(duration_event) => {
                video_duration = Some(Duration::from_secs_f64(duration_event.duration));
            }
            FfmpegEvent::ParsedInputStream(stream_event) => {
                if let Some(video_data) = stream_event.video_data() {
                    video_fps = Some(video_data.fps)
                }
            }
            FfmpegEvent::Log(log_level, msg) => {
                // Handle errors if needed
                if matches!(log_level, ffmpeg_sidecar::event::LogLevel::Error) {
                    eprintln!("FFmpeg error: {}", msg);
                }
            }
            _ => {}
        }

        // Break early once we have both values
        if video_fps.is_some() && video_duration.is_some() {
            break;
        }
    }

    match (video_fps, video_duration) {
        (Some(fps), Some(duration)) => {
            let frame_count = (fps * duration.as_secs_f32()) as u64;
            Ok(VideoMetadata {
                fps,
                duration,
                frame_count,
            })
        }
        _ => eyre::bail!("Failed to extract video metadata"),
    }
}
