//! Pure data for Vitals: geometry, images, the process snapshot model, settings and formatting.
//! No Windows calls live here; the sampler and the UI meet through these types.

pub mod format;
pub mod geom;
pub mod image;
pub mod model;
pub mod settings;

pub use format::*;
pub use geom::*;
pub use image::Image;
pub use model::*;
pub use settings::{Settings, ShortcutSettings, ThemeMode, SAMPLE_CHOICES};

/// Unix milliseconds, the clock used by snapshots and saved settings.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
