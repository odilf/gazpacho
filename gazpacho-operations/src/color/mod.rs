use gazpacho_datatypes::Frame;

pub fn contrast_scalar(v: u8, amount: f64) -> u8 {
    let v = f64::from(v);
    const MID: f64 = u8::MAX as f64 / 2.0;
    let delta = v - MID;
    (v + delta * amount).round() as u8
}

// Really, `contrast` could be generic over all scalars.
pub fn contrast(frame: Frame, amount: f64) -> Frame {
    frame.map(|[r, g, b, a]| {
        [
            contrast_scalar(r, amount),
            contrast_scalar(g, amount),
            contrast_scalar(b, amount),
            a,
        ]
    })
}
