use std::borrow::Cow;

use egui::{TextEdit, Ui, Widget, widgets::DragValue};
use gazpacho_core::{data::SimpleDataValue, graph::NodeRef};

use crate::view::node::NodeView;

impl NodeView<'_> {
    /// Inline widget for const int nodes.
    ///
    /// If `node_ref` is not a const Node, the widget panics.
    pub fn const_int_widget(&mut self, node_ref: NodeRef) -> impl Widget {
        DragValue::from_get_set(move |x| match x {
            Some(x) => {
                let x = x.round();
                self.graph
                    .set_const(node_ref, (x as i64).into())
                    .expect("Node is const");
                x
            }
            None => self
                .graph
                .get_const(node_ref)
                .and_then(|val| <&i64>::try_from(val).ok())
                .copied()
                .unwrap_or(0) as f64,
        })
        .max_decimals(0)
    }

    pub fn render_const_int(&mut self, ui: &mut Ui, node_ref: NodeRef) {
        ui.add(self.const_int_widget(node_ref));
        self.render_generic_outputs(ui, node_ref);
    }

    /// Inline widget for const float nodes.
    ///
    /// If `node_ref` is not a const Node, the widget panics.
    pub fn const_float_widget(&mut self, node_ref: NodeRef) -> impl Widget {
        DragValue::from_get_set(move |x| match x {
            Some(x) => {
                self.graph.set_const(node_ref, x.into()).unwrap();
                x
            }
            None => self
                .graph
                .get_const(node_ref)
                .and_then(|val| <&f64>::try_from(val).ok())
                .copied()
                .unwrap_or(0.0),
        })
    }

    pub fn render_const_float(&mut self, ui: &mut Ui, node_ref: NodeRef) {
        ui.add(self.const_float_widget(node_ref));
        self.render_generic_outputs(ui, node_ref);
    }

    /// Inline widget for const string nodes.
    ///
    /// If `node_ref` is not a const Node, the widget panics.
    pub fn const_string_widget(&mut self, node_ref: NodeRef) -> impl Widget {
        move |ui: &mut Ui| {
            let mut text = Cow::Borrowed(
                self.graph
                    .get_const(node_ref)
                    .and_then(|val| <&str>::try_from(val).ok())
                    .unwrap_or(""),
            );

            let response = ui.add(TextEdit::singleline(&mut text));
            if response.changed() {
                self.graph
                    .set_const(node_ref, SimpleDataValue::string(text.into_owned()))
                    .unwrap();
            }
            response
        }
    }

    pub fn render_const_string(&mut self, ui: &mut Ui, node_ref: NodeRef) {
        ui.add(self.const_string_widget(node_ref));
        self.render_generic_outputs(ui, node_ref);
    }
}
