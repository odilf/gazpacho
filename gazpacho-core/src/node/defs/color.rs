use crate::{data::Frame, node::define_node};

define_node! {
    // TODO: Should take `f64`, not `&f64`.
    CONTRAST: fn contrast(amount: &f64, frame: &Frame) -> Frame {
        let average = frame.average();
        frame
            .clone()
            .map(|pixel| (average + amount * (pixel as f64 - average)) as u8)
    }
}
