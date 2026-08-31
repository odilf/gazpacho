use std::{
    io::{BufReader, Write as _},
    process::ChildStdin,
    sync::mpsc::{Receiver, sync_channel},
    thread::{self},
};

use eyre::{Context as _, OptionExt};
use ffmpeg_sidecar::{
    child::FfmpegChild,
    command::FfmpegCommand,
    event::{FfmpegEvent, LogLevel},
    log_parser::FfmpegLogParser,
};
use gazpacho_datatypes::{Fps, Frame, Resolution};

pub struct MediaWriter {
    stdin: ChildStdin,
    child: FfmpegChild,
    ffmpeg_event_receiver: Receiver<FfmpegEvent>,
}

impl MediaWriter {
    pub fn new(path: &str, fps: Fps, resolution: Resolution) -> eyre::Result<Self> {
        let mut cmd = FfmpegCommand::new();
        cmd.args(["-f", "rawvideo", "-pix_fmt", "rgba"])
            // This assumes that the `Display` is the same as the `ffmpeg` format, but honestly
            // both use the only reasonable option there is.
            .args(["-s", &resolution.to_string()])
            // Same note for fps.
            .args(["-r", &fps.value().to_string()])
            .input("-")
            // TODO: Allow adjusting what codec to use and settings to use
            .args(["-c:v", "libx264", "-crf", "19", "-preset", "slow"])
            // TODO: Always overriding file. Kind of dangerous.
            .args(["-y", path]);

        tracing::debug!(?cmd, "Running ffmpeg command");

        let mut child = cmd.spawn().wrap_err("Failed to spawn ffmpeg writer")?;

        let stdin = child
            .take_stdin()
            .ok_or_eyre("Couldn't take ffmpeg writer stdin")?;

        let stderr = child
            .take_stderr()
            .ok_or_eyre("Couldn't take ffmpeg writer stdin")?;
        let (ffmpeg_event_sender, ffmpeg_event_receiver) = sync_channel::<FfmpegEvent>(0);

        let span = tracing::debug_span!("ffmpeg {path}");
        thread::spawn(move || {
            let _enter = span.enter();
            let reader = BufReader::new(stderr);
            let mut parser = FfmpegLogParser::new(reader);
            loop {
                match parser.parse_next_event() {
                    Ok(FfmpegEvent::LogEOF) => {
                        if let Err(err) = ffmpeg_event_sender.send(FfmpegEvent::LogEOF) {
                            tracing::error!("{err}")
                        }
                        break;
                    }
                    Ok(event) => ffmpeg_event_sender.send(event).ok(),
                    Err(e) => {
                        eprintln!("Error parsing ffmpeg output: {e}");
                        break;
                    }
                };
            }
        });
        // No frames are generated, only consumed.
        // let stdout = child.take_stdout();

        Ok(Self {
            stdin,
            child,
            ffmpeg_event_receiver,
        })
    }

    pub fn write_frame(&mut self, frame: Frame) -> eyre::Result<()> {
        while let Ok(event) = self.ffmpeg_event_receiver.try_recv() {
            match event {
                FfmpegEvent::Log(level, e) => match level {
                    LogLevel::Info => tracing::trace!("{e}"),
                    LogLevel::Warning => tracing::warn!("{e}"),
                    LogLevel::Error | LogLevel::Fatal => tracing::error!("{e}"),
                    LogLevel::Unknown => tracing::trace!("{e}"),
                },
                FfmpegEvent::Progress(progress) => tracing::debug!(?progress),
                _ => {}
            }
        }

        Ok(self.stdin.write_all(frame.bytes())?)
    }
}

impl Drop for MediaWriter {
    fn drop(&mut self) {
        if let Err(err) = self.child.quit() {
            tracing::warn!(
                "Error when trying to quit ffmpeg process: {err} (if it says 'Missing child stdin' then that probably means that it has already exited, so it's fine)"
            )
        }
    }
}
