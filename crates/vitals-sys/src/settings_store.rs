//! Settings persistence. A corrupt or older file always falls back to defaults instead of blocking startup.

use std::path::PathBuf;

use anyhow::{Context, Result};
use vitals_core::Settings;

use crate::paths;

pub fn settings_path() -> Result<PathBuf> {
    Ok(paths::settings_dir()?.join("settings.json"))
}

pub fn load() -> Settings {
    let Ok(path) = settings_path() else { return Settings::default() };
    let Ok(text) = std::fs::read_to_string(&path) else { return Settings::default() };
    serde_json::from_str(&text).unwrap_or_else(|error| {
        log::warn!("settings at {} could not be read: {error}", path.display());
        Settings::default()
    })
}

pub fn save(settings: &Settings) -> Result<()> {
    let path = settings_path()?;
    let temporary = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(settings)?;
    std::fs::write(&temporary, text).with_context(|| format!("writing {}", temporary.display()))?;
    std::fs::rename(&temporary, &path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}
