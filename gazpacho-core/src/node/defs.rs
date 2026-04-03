// TODO: Split this up into more modules

use crate::{
    data::{
        DataType, DataValue, Frame, HasDataType, SimpleDataType,
        track::{Track, VideoSourceTrack},
    },
    ffmpeg::get_video_metadata,
    node::{NodeId, NodeSpec},
};

macro_rules! define_node {
    ($const_name: ident: $(fn $name:ident($($arg:ident: $typ:ty),*) -> $out_typ:ty { $($body:tt)* })*) => {
        // Actually define all functions
        $(pub fn $name($($arg: $typ),*) -> $out_typ { $($body)* })*

        // and then define node.
        pub const $const_name: NodeSpec = NodeSpec {
            id: define_node!(@id $($name)*),
            inputs: define_node!(@inputs $(fn $name($($arg: $typ),*))*),
            outputs: &[$(
                (
                    <$out_typ as HasDataType>::DATA_TYPE.named(stringify!($name)),
                    |inputs, _const_val| {
                        let mut inputs = inputs.iter();
                        // TODO: Handle errors
                        $(let $arg: $typ = inputs.next().copied().unwrap().try_into().unwrap();)*
                        let output = $name($($arg),*);
                        <DataValue as From<$out_typ>>::from(output)
                    }
                )
            ),*]
        };
    };

    // Used to get first instance of type signature to define the input types.
    //
    // Will naturally fail if the definitions are not compatible.
    (@inputs fn $name:ident($($arg:ident: $typ:ty),*) $($rest:tt)*) => {
        &[$(<$typ as HasDataType>::DATA_TYPE.named(stringify!($arg))),*]
    };

    (@id $name:ident $($rest:tt)*) => {
        NodeId(stringify!($name))
    };
}

// define_nodes! {
//     contrast: fn(amount: &f64, frame: &Frame) -> Frame;
//         output = contrast;
//     VIDEO_SOURCE: output(amount: &f64, frame: &Frame) -> Frame
// }

define_node! {
    // TODO: Should take `f64`, not `&f64`.
    CONTRAST: fn contrast(amount: &f64, frame: &Frame) -> Frame {
        let average = frame.average();
        frame
            .clone()
            .map(|pixel| (average + amount * (pixel as f64 - average)) as u8)
    }
}

define_node! {
    VIDEO_SOURCE:
        // TODO: This should be `impl Track`.
        // TODO: This should take a `String`.
        fn output(path: &str) -> Box<dyn Track> {
            Box::new(VideoSourceTrack::new(path.to_string()).unwrap())
        }

        fn fps(path: &str) -> f64 {
            let metadata = get_video_metadata(path).unwrap();
            metadata.fps as f64
        }
}

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
    };
}

define_const_nodes! {
    INT, "const-int", int;
    FLOAT, "const-float", float;
    VFRAME, "const-vframe", vframe;
    STRING, "const-string", string;
}
