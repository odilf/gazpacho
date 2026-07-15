use color_eyre::eyre;

use crate::{
    data::{DataType, DataValue, Frame, SimpleDataType, SimpleDataValue},
    node::{NodeId, NodeSpec},
};

macro_rules! define_simple_const_nodes {
    ($($const_name:ident, $id:expr, $typ_name:ident, $typ:ty;)*) => {
        $(
            pub const $const_name: NodeSpec = NodeSpec {
                id: NodeId($id),
                inputs: &[],
                outputs: &[(DataType::$typ_name().named("output"), |_inputs, _ctx, data| {
                    let val = data.downcast_ref::<SimpleDataValue>()
                        .ok_or_else(|| eyre::eyre!("Const data wasn't a `SimpleDataValue`"))?;
                    if val.typ() != SimpleDataType::$typ_name() {
                        eyre::bail!("Stored value is not of correct type!")
                    }
                    Ok(DataValue::Simple(val.clone()))
                })],
                init_data: || Box::new(SimpleDataValue::$typ_name(<$typ>::default()))
            };
        )*

        impl SimpleDataType {
            pub const fn const_node(&self) -> &'static NodeSpec {
                match *self {
                    $(SimpleDataType::$const_name => &$const_name,)*
                    SimpleDataType::Any => &ANY,
                }
            }
        }

        impl DataType {
            pub const fn const_node(&self) -> Option<&'static NodeSpec> {
                match *self {
                    $(DataType::$const_name => Some(&$const_name),)*
                    DataType::ANY => Some(&ANY),
                    _ => None,
                }
            }
        }

        impl NodeSpec {
            pub fn is_const(&self) -> Option<SimpleDataType> {
                match self.id() {
                    $(NodeId($id) => Some(SimpleDataType::$const_name),)*
                    NodeId("const-any") => Some(SimpleDataType::Any),
                    _ => None,
                }
            }
        }
    };
}

define_simple_const_nodes! {
    INT, "const-int", int, i64;
    FLOAT, "const-float", float, f64;
    VFRAME, "const-vframe", vframe, Frame;
    STRING, "const-string", string, String;
}

/// Const node holding an arbitrary [`DataValue`] (may be a node reference too).
pub const ANY: NodeSpec = NodeSpec {
    id: NodeId("const-any"),
    inputs: &[],
    outputs: &[(
        DataType::any().named("output"),
        |_inputs, _ctx, data| {
            let val = data
                .downcast_ref::<DataValue>()
                .ok_or_else(|| eyre::eyre!("Const data wasn't a `DataValue`"))?;
            Ok(val.clone())
        },
    )],
    init_data: || Box::new(DataValue::default()),
};
