use crate::{
    data::{DataType, Frame},
    node::{NodeId, NodeSpec},
};

pub const CONTRAST: NodeSpec = NodeSpec {
    id: NodeId("contrast"),
    inputs: &[
        DataType::float().named("factor"),
        DataType::vframe().named("frame"),
    ],
    outputs: &[(
        DataType::vframe().named("output"),
        |mut inputs, ctx, _data| {
            let factor = f64::try_from(inputs.eval(0, ctx)?)?;
            let frame = Frame::try_from(inputs.eval(1, ctx)?)?;
            Ok(contrast(factor, frame).into())
        },
    )],
    init_data: || Box::new(()),
};

/// Adjusts the contrast of a frame around the midpoint.
///
/// - `factor = 1.0`: no change
/// - `factor < 1.0`: reduces contrast (`0.0` is plain gray)
/// - `factor > 1.0`: increases contrast
fn contrast(factor: f64, frame: Frame) -> Frame {
    frame.map(|pixel| {
        let mid = u8::MAX as f64 / 2.0;
        (mid + factor * (pixel as f64 - mid)).clamp(0.0, u8::MAX as f64) as u8
    })
}

/// Like [`contrast`], but adjusts the contrast around the average light level
/// of the frame instead of the midpoint of the possible lightness.
#[expect(dead_code)]
fn adaptive_contrast(factor: f64, frame: Frame) -> Frame {
    let mean = frame.average();
    frame.map(|pixel| (mean + factor * (pixel as f64 - mean)).clamp(0.0, u8::MAX as f64) as u8)
}
