use std::path::PathBuf;

use anyhow::{Context, Result};

fn env_dir(variable: &str) -> Result<PathBuf> {
    Ok(PathBuf::from(std::env::var_os(variable).with_context(|| format!("{variable} is unavailable"))?))
}

fn created(path: PathBuf) -> Result<PathBuf> {
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

/// `%APPDATA%\Vitals`: settings.
pub fn settings_dir() -> Result<PathBuf> {
    created(env_dir("APPDATA")?.join("Vitals"))
}

/// `%LOCALAPPDATA%\Vitals`: logs and install metadata.
pub fn local_data_dir() -> Result<PathBuf> {
    created(env_dir("LOCALAPPDATA")?.join("Vitals"))
}

/// `%LOCALAPPDATA%\Programs\Vitals`.
pub fn install_dir() -> Result<PathBuf> {
    created(env_dir("LOCALAPPDATA")?.join("Programs").join("Vitals"))
}

pub fn log_path() -> Result<PathBuf> {
    Ok(local_data_dir()?.join("vitals.log"))
}
