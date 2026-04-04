use serde::{Deserialize, Serialize};


#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodeViewState {}

pub struct NodeView {
    // TODO
}

impl NodeView {
}

// For later
// Some(SimpleDataType::Int) => ui.place(
//     rect,
//     egui::widgets::DragValue::from_get_set(|x| match x {
//         Some(x) => {
//             let x = x.round();
//             if !shift {
//                 self.graph.set_const(node_ref, (x as i64).into());
//             }
//             x
//         }
//         None => self
//             .graph
//             .get_const(node_ref)
//             .map(|val| <&i64>::try_from(val).ok())
//             .flatten()
//             .copied()
//             .unwrap_or(0) as f64,
//     })
//     .max_decimals(0),
// ),

// Some(SimpleDataType::Float) => ui.place(
//     rect,
//     egui::widgets::DragValue::from_get_set(|x| match x {
//         Some(x) => {
//             if !shift {
//                 self.graph.set_const(node_ref, x.into());
//             }
//             x
//         }
//         None => self
//             .graph
//             .get_const(node_ref)
//             .map(|val| <&f64>::try_from(val).ok())
//             .flatten()
//             .copied()
//             .unwrap_or(0.0),
//     }),
// ),

// Some(SimpleDataType::String) => {
//     let mut text = self
//         .graph
//         .get_const(node_ref)
//         .map(|val| <&str>::try_from(val).ok())
//         .flatten()
//         .map(|x| x.to_string())
//         .unwrap_or("".to_string());
//     let response = ui.place(rect, egui::widgets::TextEdit::singleline(&mut text));
//     if response.changed() {
//         self.graph.set_const(node_ref, text.clone().into());
//     }
//     response
// }

// Some(SimpleDataType::VideoFrame) => ui.place(
//     rect,
//     egui::widgets::Label::new("Don't know how to render VideoFrame input. :/"),
// ),
