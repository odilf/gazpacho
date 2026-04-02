use std::fmt;

use ffmpeg_sidecar::event::OutputVideoFrame;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frame(#[serde(with = "OutputVideoFrameDef")] OutputVideoFrame);

impl Frame {
    pub fn average(&self) -> f64 {
        let n = f64::from(self.0.width * self.0.height);
        self.0.data.iter().map(|&x| f64::from(x) / n).sum()
    }

    pub fn map(mut self, mut f: impl FnMut(u8) -> u8) -> Self {
        for datum in &mut self.0.data {
            *datum = f(*datum)
        }

        self
    }

    pub fn data(&self) -> &[u8] {
        &self.0.data
    }

    pub fn width(&self) -> u32 {
        self.0.width
    }

    pub fn height(&self) -> u32 {
        self.0.height
    }
}

impl From<OutputVideoFrame> for Frame {
    fn from(value: OutputVideoFrame) -> Self {
        Self(value)
    }
}

impl fmt::Display for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "frame {}x{} {}",
            self.0.width, self.0.height, self.0.pix_fmt
        )
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "OutputVideoFrame")]
struct OutputVideoFrameDef {
    width: u32,
    height: u32,
    pix_fmt: String,
    output_index: u32,
    data: Vec<u8>,
    frame_num: u32,
    timestamp: f32,
}
