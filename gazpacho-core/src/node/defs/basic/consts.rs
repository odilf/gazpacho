use color_eyre::eyre;

use crate::{
    data::{DataType, DataValue, Frame, SimpleDataType, SimpleDataValue},
    node::{NodeId, NodeSpec},
};

// TODO: Actually now we don't have to store `SimpleDataValue`s...
macro_rules! define_const_nodes {
    ($($const_name:ident, $id:expr, $typ_name:ident, $typ:ty;)*) => {
        $(
            pub const $const_name: NodeSpec = NodeSpec {
                id: NodeId($id),
                inputs_ref: &[],
                inputs_own: &[],
                outputs: &[(DataType::$typ_name().named("output"), |_ref, _own, data| {
                    let val = data.downcast_ref::<SimpleDataValue>().expect("Stored data should be a SimpleDataValue");
                    if val.typ() != SimpleDataType::$typ_name() {
                        eyre::bail!("Stored value is not of correct type!")
                    }
                    Ok(DataValue::Simple(val.clone()))
                })],
                init_data: || Box::new(<$typ>::default())
            };
        )*

        impl SimpleDataType {
            pub const fn const_node(&self) -> &'static NodeSpec {
                match *self {
                    $(SimpleDataType::$const_name => &$const_name,)*
                }
            }
        }
        impl DataType {
            pub const fn const_node(&self) -> Option<&'static NodeSpec> {
                match *self {
                    $(DataType::$const_name => Some(&$const_name),)*
                    _ => None,
                }
            }
        }

        impl NodeSpec {
            pub fn is_const(&self) -> Option<SimpleDataType> {
                match self.id {
                    $(NodeId($id) => Some(SimpleDataType::$const_name),)*
                    _ => None,
                }
            }
        }
    };
}

define_const_nodes! {
    INT, "const-int", int, i64;
    FLOAT, "const-float", float, f64;
    VFRAME, "const-vframe", vframe, Frame;
    STRING, "const-string", string, String;
    ANY, "const-any", any, DataValue;
}
