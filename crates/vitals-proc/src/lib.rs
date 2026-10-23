//! Windows process sampling and control. The only crate that opens processes and reads system counters.

mod actions;
mod sampler;
mod startup;
mod win;

pub use actions::{end_task, resume, set_priority, suspend};
pub use sampler::Sampler;
pub use startup::{set_startup_enabled, startup_items, StartupItem, StartupScope, StartupSource};

pub use win::display_name;
