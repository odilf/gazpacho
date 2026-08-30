use eyre;
use gazpacho_datatypes::Frame;

use crate::{
    Request, Signature,
    traits::{NodeInput, Operation, Renderer, RequestDeps},
};

// Really, `contrast` could be generic over all scalars.
#[expect(clippy::cast_sign_loss, reason = "all values always stay positive")]
pub fn contrast(v: u8, amount: f64) -> u8 {
    let v = f64::from(v);
    const MID: f64 = u8::MAX as f64 / 2.0;
    let delta = v - MID;
    (v + delta * amount).round() as u8
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Contrast([NodeInput; 2]);

impl Contrast {
    pub fn input(self) -> NodeInput {
        self.0[0]
    }
    pub fn amount(self) -> NodeInput {
        self.0[1]
    }
}

impl Operation for Contrast {
    const NAME: &str = "contrast";
    const SIGNATURE: Signature = Signature::new(&["input", "amount"]);
    const DEPS: RequestDeps = RequestDeps::empty();
    const INDEPS: RequestDeps = RequestDeps::empty();

    fn inputs(&self) -> &[NodeInput] {
        &self.0
    }

    fn inputs_mut(&mut self) -> &mut [NodeInput] {
        &mut self.0
    }

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
