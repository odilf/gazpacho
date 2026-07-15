use crate::{
    data::DataType,
    node::{Ctx, NodeId, NodeSpec},
};

/// Concatenates two streams along the time axis.
///
/// Inputs are paired: `(len-a, frame-a)` and `(len-b, frame-b)`. Each `len-*`
/// is wired from the upstream source's `len` output so concat can dispatch
/// without having to evaluate both branches.
pub const CONCAT: NodeSpec = NodeSpec {
    id: NodeId("concat"),
    inputs: &[
        DataType::int().named("len-a"),
        DataType::vframe().named("frame-a"),
        DataType::int().named("len-b"),
        DataType::vframe().named("frame-b"),
    ],
    outputs: &[
        (
            DataType::vframe().named("output"),
            |mut inputs, ctx, _data| {
                let len_a = u64::try_from(i64::try_from(inputs.eval(0, ctx)?)?)?;
                if ctx.frame_index < len_a {
                    inputs.eval(1, ctx)
                } else {
                    let ctx = Ctx {
                        frame_index: ctx.frame_index - len_a,
                    };
                    inputs.eval(3, ctx)
                }
            },
        ),
        (
            DataType::int().named("len"),
            |mut inputs, ctx, _data| {
                let a = i64::try_from(inputs.eval(0, ctx)?)?;
                let b = i64::try_from(inputs.eval(2, ctx)?)?;
                Ok((a + b).into())
            },
        ),
    ],
    init_data: || Box::new(()),
};
