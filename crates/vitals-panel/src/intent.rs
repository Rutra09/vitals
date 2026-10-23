//! Commands the window views send to the app. The panel never performs OS actions itself.

use std::path::PathBuf;

use vitals_core::{Priority, Settings};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Processes,
    Performance,
    Startup,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Processes, Tab::Performance, Tab::Startup];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Processes => "Processes",
            Tab::Performance => "Performance",
            Tab::Startup => "Startup",
        }
    }
}

/// One startup entry as the UI needs it. The app converts `vitals_proc::StartupItem` into this shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartupRow {
    /// Stable identifier used to enable or disable the entry.
    pub id: String,
    pub name: String,
    pub command: String,
    pub source: String,
    pub scope: String,
    pub enabled: bool,
}

impl StartupRow {
    pub fn executable(&self) -> Option<PathBuf> {
        command_executable(&self.command)
    }
}

/// First quoted token of a command line, or the first unquoted path-like token.
pub fn command_executable(command: &str) -> Option<PathBuf> {
    let command = command.trim();
    let first = if let Some(rest) = command.strip_prefix('"') {
        rest.split('"').next().unwrap_or_default()
    } else {
        command.split_whitespace().next().unwrap_or_default()
    };
    let first = first.trim();
    if first.is_empty() {
        return None;
    }
    let path = PathBuf::from(first);
    path.is_absolute().then_some(path)
}

#[derive(Clone, Debug, PartialEq)]
pub enum PanelIntent {
    ShowDashboard,
    ShowTab(Tab),
    /// Open Processes filtered to this PID, selecting its group.
    ShowProcess { pid: u32 },
    HideDashboard,
    ShowSettings,
    HideSettings,
    HideQuickView,
    ShowQuickView,
    Quit,
    Refresh,
    RefreshStartup,
    SetStartupEnabled { id: String, enabled: bool },
    OpenTaskManager,
    OpenStartup { command: String },
    SetPaused(bool),
    EndTask { pids: Vec<u32>, name: String },
    Suspend { pids: Vec<u32>, name: String },
    Resume { pids: Vec<u32>, name: String },
    SetPriority { pids: Vec<u32>, priority: Priority, name: String },
    OpenLocation(PathBuf),
    CopyText(String),
    OpenDataFolder,
    OpenUrl(String),
    SettingsChanged(Box<Settings>),
}
