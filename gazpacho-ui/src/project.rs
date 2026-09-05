use std::path::{Path, PathBuf};

use egui::RichText;
use gazpacho_ast::Module;
use gazpacho_render::Engine;

use crate::is_gzp;

// TODO(serialize): derive serde
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Project {
    pub root: PathBuf,
    pub files: Vec<(PathBuf, Option<FileData>)>,
    pub active: Option<usize>,
    pub selected_file: Option<usize>,
}

impl Project {
    pub fn new(root: PathBuf) -> eyre::Result<Project> {
        fn walk(dir: &Path, files: &mut Vec<(PathBuf, Option<FileData>)>) -> eyre::Result<()> {
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, files)?;
                } else {
                    files.push((path, None));
                }
            }

            Ok(())
        }

        let mut files = Vec::new();
        walk(&root, &mut files)?;
        files.sort_by(|(path_a, _), (path_b, _)| path_a.cmp(path_b));

        let default_active = files.iter().position(|(path, _)| is_gzp(&path));

        Ok(Project {
            root,
            files,
            active: default_active,
            selected_file: None,
        })
    }

    pub fn show_inspector(&self, ui: &mut egui::Ui) {
        match self.selected_file {
            Some(i) => {
                let (path, data) = &self.files[i];
                match data {
                    None => {
                        ui.label("Loading...");
                    }
                    Some(FileData::Source(source)) => show_source_inspector(ui, path, source),
                    Some(FileData::Media(media)) => show_media_inspector(ui, path, media),
                    Some(FileData::Other) => {
                        ui.label("Inspector for non-source non-media files");
                    }
                }
            }
            None => show_project_inspector(ui, self),
        };
    }

    fn get_active(&self) -> Option<(&Path, &SourceFile)> {
        let (path, data) = &self.files[self.active?];
        match data.as_ref()? {
            FileData::Source(data) => Some((path, data)),
            _ => unreachable!("Active should always point to a source file."),
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub enum FileData {
    Source(SourceFile),
    Media(MediaFile),
    Other,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct SourceFile {
    module: Module,
    engine: Engine,
    diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub struct MediaFile {}

fn show_project_inspector(ui: &mut egui::Ui, project: &Project) {
    if let Some((path, active)) = project.get_active() {
        ui.label(RichText::new("Active File").strong());
        ui.label(
            RichText::new(path.to_string_lossy())
                .small()
                .color(ui.visuals().weak_text_color()),
        );
        ui.add_space(8.0);

        let module = &active.module;
        ui.label(RichText::new("Module Info").strong());
        ui.label(RichText::new(format!("Defs: {}", module.defs.len())).small());
        ui.add_space(8.0);

        // TODO: Show better data
        ui.label(RichText::new("Render Graph").strong());
        ui.label(RichText::new(format!("Output node: {:?}", active.engine.output)).small());
        ui.label("TODO: Show better engine data");

        if !active.diagnostics.is_empty() {
            ui.add_space(8.0);
            ui.label(
                RichText::new("Diagnostics")
                    .strong()
                    .color(ui.visuals().error_fg_color),
            );
            show_diagnostics(ui, &active.diagnostics);
        }
    } else {
        ui.label("TODO: Show general info if no file is active");
    }
}

fn show_source_inspector(ui: &mut egui::Ui, path: &Path, data: &SourceFile) {
    ui.label(RichText::new("File").strong());
    ui.label(
        RichText::new(path.to_string_lossy())
            .small()
            .color(ui.visuals().weak_text_color()),
    );

    if !data.diagnostics.is_empty() {
        ui.label("Diagnostics");
        show_diagnostics(ui, &data.diagnostics);
    }
}

fn show_media_inspector(ui: &mut egui::Ui, path: &Path, data: &MediaFile) {
    ui.label(RichText::new("Media").strong());
    ui.label(
        RichText::new(path.to_string_lossy())
            .small()
            .color(ui.visuals().weak_text_color()),
    );

    ui.label(format!("TODO: Media insepctor for {data:?}"));
}

fn show_diagnostics(ui: &mut egui::Ui, diagnostics: &[String]) {
    for diag in diagnostics {
        ui.label(
            RichText::new(diag)
                .small()
                .color(ui.visuals().error_fg_color),
        );
    }
}
