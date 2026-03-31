use ffmpeg_sidecar::event::OutputVideoFrame;

#[derive(Debug, Clone, PartialEq)]
pub struct Frame(OutputVideoFrame);

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
