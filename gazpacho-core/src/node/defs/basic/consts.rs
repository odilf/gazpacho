use crate::{
    data::{DataType, DataValue, SimpleDataType},
    node::{NodeId, NodeSpec},
};

macro_rules! define_const_nodes {
    ($($const_name:ident, $id:expr, $typ_name:ident;)*) => {
        $(
            pub const $const_name: NodeSpec = NodeSpec {
                id: NodeId($id),
                inputs: &[],
                outputs: &[(DataType::$typ_name().named("output"), |_, const_val| {
                    let val = const_val.unwrap();
                    assert!(val.typ() == SimpleDataType::$typ_name());
                    DataValue::Simple(val.clone())
                })],
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
    INT, "const-int", int;
    FLOAT, "const-float", float;
    VFRAME, "const-vframe", vframe;
    STRING, "const-string", string;
}
