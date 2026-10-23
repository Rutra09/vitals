//! Windows integration that is not process sampling: the global shortcut hook, tray, single instance,
//! settings storage, startup registration and small shell helpers.

pub mod hook;
pub mod install;
pub mod instance;
pub mod paths;
pub mod settings_store;
pub mod shell;
pub mod tray;

mod win;

pub use hook::{HookConfig, Hotkey, InputHook};
pub use tray::{Tray, TrayCommand};
pub use win::{cursor_pos, monitor_rect_at, scale_at, work_area_at};
