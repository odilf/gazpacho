pub mod basic;
pub mod color;

// macro_rules! define_node {
//     ($const_name: ident: $(fn $name:ident($($arg:ident: $typ:ty),*) -> $out_typ:ty { $($body:tt)* })*) => {
//         // Actually define all functions
//         $(pub fn $name($($arg: $typ),*) -> $out_typ { $($body)* })*

//         // and then define node.
//         pub const $const_name: crate::node::NodeSpec = crate::node::NodeSpec {
//             id: define_node!(@id $($name)*),
//             inputs: define_node!(@inputs $(fn $name($($arg: $typ),*))*),
//             outputs: &[$(
//                 (
//                     <$out_typ as crate::data::HasDataType>::DATA_TYPE.named(stringify!($name)),
//                     |inputs, _const_val| {
//                         let mut inputs = inputs.iter();
//                         $(let $arg: $typ = inputs.next().copied().unwrap().try_into().unwrap();)*
//                         let output = $name($($arg),*);
//                         <crate::data::DataValue as From<$out_typ>>::from(output)
//                     }
//                 )
//             ),*]
//         };
//     };

//     // Used to get first instance of type signature to define the input types.
//     //
//     // Will naturally fail if the definitions are not compatible.
//     (@inputs fn $name:ident($($arg:ident: $typ:ty),*) $($rest:tt)*) => {
//         &[$(<$typ as crate::data::HasDataType>::DATA_TYPE.named(stringify!($arg))),*]
//     };

//     (@id $name:ident $($rest:tt)*) => {
//         crate::node::NodeId(stringify!($name))
//     };
// }

// pub(crate) use define_node;
