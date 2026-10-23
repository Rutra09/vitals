//! The three Vitals surfaces: the main dashboard, the Ctrl+Shift+Esc quick view and the settings window.
//! This crate is pure UI over `vitals-core`; it never opens a process or touches Windows settings. Every command
//! leaves through `PanelIntent`, which the app turns into an action.

mod components;
pub mod dashboard;
pub mod demo;
pub mod intent;
pub mod quick;
pub mod settings;

pub use dashboard::DashboardView;
pub use intent::{PanelIntent, StartupRow, Tab};
pub use quick::QuickView;
pub use settings::SettingsView;
