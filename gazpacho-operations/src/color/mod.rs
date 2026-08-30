use eyre;
use gazpacho_datatypes::Frame;

use crate::{
    Request,
    traits::{Operation, Renderer},
};

// Really, `contrast` could be generic over all scalars.
#[expect(clippy::cast_sign_loss, reason = "all values always stay positive")]
pub fn contrast(v: u8, amount: f64) -> u8 {
    let v = f64::from(v);
    const MID: f64 = u8::MAX as f64 / 2.0;
    let delta = v - MID;
    (v + delta * amount).round() as u8
}

crate::op! {
    pub struct Contrast { input, amount } as "contrast"
}

impl Operation for Contrast {
    fn frame(&self, renderer: &mut impl Renderer, req: Request) -> eyre::Result<Frame> {
        let frame = renderer.eval(self.input(), req)?.to_frame()?;
        let amount = renderer.eval(self.amount(), req)?.to_float()?;

        Ok(frame.map(|[r, g, b, a]| {
            [
                contrast(r, amount),
                contrast(g, amount),
                contrast(b, amount),
                a,
            ]
        }))
    }
}
