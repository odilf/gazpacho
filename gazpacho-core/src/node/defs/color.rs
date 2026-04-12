use color_eyre::eyre::{self, Context};

use crate::{
    data::{DataType, Frame},
    node::{NodeId, NodeSpec},
};

pub const CONTRAST: NodeSpec = NodeSpec {
    id: NodeId("contrast"),
    inputs_ref: &[],
    inputs_own: &[
        DataType::vframe().named("frame"),
        DataType::float().named("amount"),
    ],
    outputs: &[(
        DataType::vframe().named("output"),
        |inputs_ref, inputs_own, _data| {
            let [amount] = inputs_ref
                .try_into()
                .wrap_err("Wrong number of referenced inputs")?;

            let [frame] = *Box::try_from(inputs_own)
                .map_err(|_| eyre::eyre!("Wrong number of owned inputs"))?;

            let amount = *<&f64>::try_from(amount)?;
            let frame = <Frame>::try_from(frame)?;

            Ok(contrast(amount, frame).into())
        },
    )],
    init_data: || Box::new(()),
};

fn contrast(amount: f64, frame: Frame) -> Frame {
    let average = frame.average();
    frame.map(|pixel| (average + amount * (pixel as f64 - average)) as u8)
}
