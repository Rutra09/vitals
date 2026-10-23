use serde::{Deserialize, Serialize};

/// Saved as JSON in `%APPDATA%\Vitals\settings.json`. Every field has a default so older files keep loading.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemeMode,
    pub launch_at_login: bool,
    /// Start in the tray without opening the dashboard.
    pub start_minimized: bool,
    /// Sampling period in milliseconds.
    pub sampling_ms: u64,
    /// Take over Ctrl+Shift+Esc from Task Manager.
    pub replace_task_manager: bool,
    /// Also take over the legacy Ctrl+Alt+Esc shortcut.
    pub replace_legacy_task_manager: bool,
    /// Ask before ending a task.
    pub confirm_end_task: bool,
    /// Keep Vitals in the notification area after the window closes.
    pub close_to_tray: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeMode::System,
            launch_at_login: false,
            start_minimized: false,
            sampling_ms: 1000,
            replace_task_manager: true,
            replace_legacy_task_manager: true,
            confirm_end_task: true,
            close_to_tray: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShortcutSettings {
    pub replace_task_manager: bool,
    pub replace_legacy_task_manager: bool,
}

impl Default for ShortcutSettings {
    fn default() -> Self {
        Self { replace_task_manager: true, replace_legacy_task_manager: true }
    }
}

impl From<&Settings> for ShortcutSettings {
    fn from(settings: &Settings) -> Self {
        Self {
            replace_task_manager: settings.replace_task_manager,
            replace_legacy_task_manager: settings.replace_legacy_task_manager,
        }
    }
}

pub const SAMPLE_CHOICES: [u64; 3] = [500, 1000, 2000];

impl Settings {
    pub fn sampling_label(&self) -> &'static str {
        sample_label(self.sampling_ms)
    }
}

pub fn sample_label(ms: u64) -> &'static str {
    match ms {
        0..=700 => "0.5 s",
        701..=1500 => "1 s",
        _ => "2 s",
    }
}
