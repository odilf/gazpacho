mod consts;

use egui::{Direction, Label, Layout, Rect, Response, RichText, Ui, Vec2, Widget};
use gazpacho_core::{
    data::SimpleDataType,
    graph::{Graph, NodeRef},
    node,
};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    view::{GraphViewState, View},
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodeViewState {
    pub selection: Option<NodeRef>,
}

pub struct NodeView<'a> {
    state: &'a mut NodeViewState,
    graph: &'a mut Graph,
    view: &'a mut View,
    graph_view: &'a mut GraphViewState,
}

impl AppState {
    pub fn node_view(&mut self) -> NodeView<'_> {
        NodeView {
            state: &mut self.node_view,
            graph: &mut self.graph,
            view: &mut self.view,
            graph_view: &mut self.graph_view,
        }
    }
}

impl Widget for NodeView<'_> {
    fn ui(mut self, ui: &mut Ui) -> Response {
        let Some(selection) = self.state.selection else {
            return ui
                .with_layout(Layout::centered_and_justified(Direction::TopDown), |ui| {
                    ui.label(RichText::new("No node selected.").size(32.0))
                })
                .response;
        };

        let node = self.graph.get(selection);
        ui.label(
            RichText::new(format!("Node: {}", node.spec().id().to_string()))
                .heading()
                .size(36.0),
        );

        if ui.button("View in graph").clicked() {
            *self.view = View::Graph;
            self.graph_view.focus(selection);
        }

        let render = match *self.graph.get(selection).spec() {
            n if n == node::INT => Self::render_const_int,
            n if n == node::FLOAT => Self::render_const_float,
            n if n == node::STRING => Self::render_const_string,
            _ => Self::render_generic_node,
        };

        render(&mut self, ui, selection);
        ui.response()
    }
}

impl NodeView<'_> {
    fn render_generic_node(&mut self, ui: &mut Ui, node_ref: NodeRef) {
        ui.label("(using generic node renderer)");

        self.render_generic_inputs(ui, node_ref);
        self.render_generic_outputs(ui, node_ref);
    }

    fn render_generic_inputs(&mut self, ui: &mut Ui, node_ref: NodeRef) {
        ui.heading(RichText::new("Inputs").heading().size(24.0));
        let node = self.graph.get(node_ref);
        // TODO: This allocation is avoidable, we know that below we only change the consts.
        for (port, input) in node.inputs().collect::<Box<[_]>>() {
            let port = self.graph.get_port(port);
            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), 12.0),
                Layout::left_to_right(egui::Align::TOP),
                |ui| {
                    ui.label(format!("{}: {:?}", port.name(), port.typ()));
                    ui.spacing();
                    ui.label("connected to");
                    let Some(port) = input else {
                        ui.label("nothing");
                        return;
                    };

                    let node = self.graph.get(port.node());

                    // Shorthand for consts
                    if let Some(typ) = node.spec().is_const() {
                        match typ {
                            SimpleDataType::Int => ui.add(self.const_int_widget(node_ref)),
                            SimpleDataType::Float => ui.add(self.const_float_widget(node_ref)),
                            SimpleDataType::String => ui.add(self.const_string_widget(node_ref)),
                            SimpleDataType::VideoFrame => ui.label("hmm"),
                        };
                        return;
                    }

                    let response = ui.button((
                        node.spec().id().to_string(),
                        format!("(port {})", self.graph.get_port(port).name()),
                    ));

                    if response.double_clicked_by(egui::PointerButton::Primary) {
                        self.state.selection = Some(port.node())
                    }
                },
            );
        }
    }

    fn render_generic_outputs(&mut self, ui: &mut Ui, node_ref: NodeRef) {
        ui.heading(RichText::new("Outputs").heading().size(24.0));
        let node = self.graph.get(node_ref);
        for (port, outputs) in node.outputs() {
            let port = self.graph.get_port(port);
            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), 12.0),
                Layout::left_to_right(egui::Align::TOP),
                |ui| {
                    ui.label(format!("{}: {:?}", port.name(), port.typ()));
                    ui.spacing();
                    ui.label("connected to");
                    if outputs.iter().len() == 0 {
                        ui.label("nothing");
                    }

                    for &port in outputs.iter() {
                        let node = self.graph.get(port.node());
                        let response = ui.button((
                            node.spec().id().to_string(),
                            format!("(port {})", self.graph.get_port(port).name()),
                        ));

                        if response.double_clicked_by(egui::PointerButton::Primary) {
                            self.state.selection = Some(port.node())
                        }
                    }
                },
            );
        }
    }
}
