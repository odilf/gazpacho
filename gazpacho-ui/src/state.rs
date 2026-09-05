use std::path::PathBuf;

use gazpacho_ast::Module;
use gazpacho_compile::RenderGraph;
use gazpacho_datatypes::{Fps, Resolution, Time};
use gazpacho_operations::NodeId;
use gazpacho_render::Renderer;

use crate::App;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct LayoutState {
    pub left_sidebar_width: f32,
    pub right_sidebar_width: f32,
    pub timeline_height: f32,
    pub inspector_visible: bool,
    pub active_sidebar: Sidebar,
}

impl Default for LayoutState {
    fn default() -> Self {
        Self {
            left_sidebar_width: 240.0,
            right_sidebar_width: 280.0,
            timeline_height: 180.0,
            inspector_visible: false,
            active_sidebar: Sidebar::Project,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub enum Sidebar {
    Project,
    Media,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub enum Selection {
    ProjectFile(usize),
    MediaItem(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub enum RenderStatus {
    Idle,
    Rendering,
    Done,
    Error,
}

// TODO(serialize): derive serde
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub struct ProjectState {
    pub root: Option<PathBuf>,
    pub active_gzp: Option<PathBuf>,
    pub source_text: Option<String>,
    pub module: Option<Module>,
    pub render_graph: Option<(RenderGraph, NodeId)>,
    /// Runtime state: holds live ffmpeg processes and frame caches, so it is
    /// not serialized. Reconstructed from `render_graph` + `module` on load.
    #[serde(skip)]
    pub renderer: Option<Renderer>,
    pub parse_errors: Vec<gazpacho_ast::ParseError>,
    pub compile_error: Option<String>,
    pub diagnostics: Vec<Diagnostic>,
    pub media_items: Vec<MediaItem>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Diagnostic {
    pub file: Option<PathBuf>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub severity: Severity,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// A media item that is shown in the media browser.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct MediaItem {
    pub path: String,
    pub resolution: Option<Resolution>,
    pub duration: Option<Time>,
    pub fps: Option<Fps>,
    pub available: bool,
    pub error: Option<String>,
}

impl App {
    pub fn load_folder(&self, _path: &PathBuf) -> eyre::Result<()> {
        todo!("Load folder into app state")
    }
}
