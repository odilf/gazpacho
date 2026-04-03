// TODO: Split this up into more modules

use crate::{
    data::{
        DataValue, Frame, HasDataType,
        track::{Track, VideoSourceTrack},
    },
    ffmpeg::get_video_metadata,
    node::{NodeDescriptor, NodeId},
};

macro_rules! define_node {
    ($const_name: ident: $(fn $name:ident($($arg:ident: $typ:ty),*) -> $out_typ:ty { $($body:tt)* })*) => {
        // Actually define all functions
        $(pub fn $name($($arg: $typ),*) -> $out_typ { $($body)* })*

        // and then define node.
        pub const $const_name: NodeDescriptor = NodeDescriptor {
            id: define_node!(@id $($name)*),
            inputs: define_node!(@inputs $(fn $name($($arg: $typ),*))*),
            outputs: &[$(
                (
                    <$out_typ as HasDataType>::DATA_TYPE.named(stringify!($name)),
                    |inputs| {
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
