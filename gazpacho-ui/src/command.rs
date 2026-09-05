#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum Pane {
    #[default]
    Sidebar,
    Viewer,
    Timeline,
    Inspector,
}

impl Pane {
    pub const ALL: &[Pane] = &[Pane::Sidebar, Pane::Viewer, Pane::Timeline, Pane::Inspector];

    pub fn label(self) -> &'static str {
        match self {
            Pane::Sidebar => "Sidebar",
            Pane::Viewer => "Viewer",
            Pane::Timeline => "Timeline",
            Pane::Inspector => "Inspector",
        }
    }

    pub fn next(self) -> Pane {
        let idx = self as usize;
        #[expect(
            clippy::indexing_slicing,
            reason = "index is reduced mod the non-empty ALL array"
        )]
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Pane {
        let idx = self as usize;
        #[expect(
            clippy::indexing_slicing,
            reason = "index is reduced mod the non-empty ALL array"
        )]
        Self::ALL[(idx + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone)]
pub enum Command {
    // Command palette
    ToggleCommandPalette,

    // Focus
    FocusPane(Pane),
    FocusNextPane,
    FocusPrevPane,
    Escape,

    // Panel visibility
    ToggleInspector,
    ToggleLeftSidebar,
    ResetLayout,
    ResetAllState,

    // File operations
    OpenFolder,
    LoadGzpFile(usize),
    SelectFile(usize),
    ReloadActiveGzp,
    RevealActiveFileManager,
    CopyActiveFilePath,

    // Playback & timeline
    PlayPause,
    FrameForward,
    FrameBackward,
    SeekToStart,
    SeekToEnd,
    TimelineZoomIn,
    TimelineZoomOut,

    // Other
    Quit,
}

impl Command {
    pub fn label(&self) -> String {
        match self {
            Command::ToggleCommandPalette => "Toggle Command Palette".into(),
            Command::FocusPane(pane) => format!("Focus {}", pane.label()),
            Command::FocusNextPane => "Focus Next Pane".into(),
            Command::FocusPrevPane => "Focus Previous Pane".into(),
            Command::Escape => "Escape".into(),
            Command::ToggleInspector => "Toggle Inspector".into(),
            Command::ToggleLeftSidebar => "Toggle Left Sidebar".into(),
            Command::OpenFolder => "Open Folder".into(),
            Command::LoadGzpFile(_) => "Load `.gzp` file".into(),
            Command::SelectFile(_) => "Select file".into(),
            Command::ReloadActiveGzp => "Reload Active .gzp".into(),
            Command::RevealActiveFileManager => "Reveal in File Manager".into(),
            Command::CopyActiveFilePath => "Copy File Path".into(),
            Command::PlayPause => "Play / Pause".into(),
            Command::FrameForward => "Frame Forward".into(),
            Command::FrameBackward => "Frame Backward".into(),
            Command::SeekToStart => "Seek to Start".into(),
            Command::SeekToEnd => "Seek to End".into(),
            Command::TimelineZoomIn => "Timeline Zoom In".into(),
            Command::TimelineZoomOut => "Timeline Zoom Out".into(),
            Command::ResetLayout => "Reset Layout".into(),
            Command::ResetAllState => "Reset All State".into(),
            Command::Quit => "Quit".into(),
        }
    }

    pub fn shortcut_hint(&self) -> Option<&'static str> {
        match self {
            Command::ToggleCommandPalette => Some("Ctrl+Shift+P"),
            Command::FocusPane(Pane::Sidebar) => Some("1"),
            Command::FocusPane(Pane::Viewer) => Some("2"),
            Command::FocusPane(Pane::Timeline) => Some("3"),
            Command::FocusPane(Pane::Inspector) => Some("4"),
            Command::FocusNextPane => Some("Tab"),
            Command::FocusPrevPane => Some("Shift+Tab"),
            Command::Escape => Some("Esc"),
            Command::ToggleInspector => Some("I"),
            Command::ReloadActiveGzp => Some("Ctrl+R"),
            Command::RevealActiveFileManager => Some("Ctrl+Shift+R"),
            Command::PlayPause => Some("Space"),
            Command::FrameForward => Some("."),
            Command::FrameBackward => Some(","),
            Command::SeekToStart => Some("Home"),
            Command::SeekToEnd => Some("End"),
            Command::TimelineZoomIn => Some("="),
            Command::TimelineZoomOut => Some("-"),
            Command::Quit => Some("Ctrl+Q"),
            _ => None,
        }
    }
}

/// Collects all available commands for the command palette.
pub fn all_commands() -> &'static [Command] {
    &[
        Command::ToggleCommandPalette,
        Command::FocusPane(Pane::Sidebar),
        Command::FocusPane(Pane::Viewer),
        Command::FocusPane(Pane::Timeline),
        Command::FocusPane(Pane::Inspector),
        Command::FocusNextPane,
        Command::FocusPrevPane,
        Command::ToggleInspector,
        Command::ToggleLeftSidebar,
        Command::OpenFolder,
        Command::ReloadActiveGzp,
        Command::RevealActiveFileManager,
        Command::CopyActiveFilePath,
        Command::PlayPause,
        Command::FrameForward,
        Command::FrameBackward,
        Command::SeekToStart,
        Command::SeekToEnd,
        Command::TimelineZoomIn,
        Command::TimelineZoomOut,
        Command::ResetLayout,
        Command::ResetAllState,
        Command::Quit,
    ]
}
