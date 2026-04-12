use color_eyre::eyre;

pub trait Track {
    type Ty;

    /// Number of frames in the track
    fn len(&self) -> u64;

    /// Frames per second.
    fn fps(&self) -> f64;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Time of the track
    fn length(&self) -> f64 {
        self.len() as f64 / self.fps()
    }

    fn frame_length(&self) -> f64 {
        1.0 / self.fps()
    }

    fn to_frame_index(&self, time: f64) -> eyre::Result<u64> {
        let tol: f64 = self.fps() * 0.1;
        let f = self.fps() * time;
        let r = f.round();
        let diff = f - r;
        if diff.abs() >= tol {
            eyre::bail!("Timestamp {time} doesn't land on a frame (off by {diff})")
        }

        Ok(r as u64)
    }
}
