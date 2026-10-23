//! Install, uninstall and launch-at-login. Everything is per-user: no elevation, no machine-wide changes.

use std::{
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use windows::{
    Win32::{
        Foundation::ERROR_SUCCESS,
        System::Registry::{
            HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
            RegCreateKeyExW, RegDeleteTreeW, RegDeleteValueW, RegSetValueExW,
        },
    },
    core::PCWSTR,
};

use crate::paths;

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "Vitals";
const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Vitals";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn installed_exe() -> Option<PathBuf> {
    let path = paths::install_dir().ok()?.join("vitals.exe");
    path.exists().then_some(path)
}

pub fn is_installed() -> bool {
    installed_exe().is_some()
}

/// Copies this executable into `%LOCALAPPDATA%\Programs\Vitals`, registers launch-at-login and an
/// Apps & Features entry. Returns the installed path.
pub fn install() -> Result<PathBuf> {
    let source = std::env::current_exe().context("cannot locate vitals.exe")?;
    let target = paths::install_dir()?.join("vitals.exe");
    if source != target {
        std::fs::copy(&source, &target)
            .with_context(|| format!("copying {} to {}", source.display(), target.display()))?;
    }
    set_launch_at_login(true)?;
    let mut settings = crate::settings_store::load();
    if !settings.launch_at_login {
        settings.launch_at_login = true;
        if let Err(error) = crate::settings_store::save(&settings) {
            log::warn!("could not update launch-at-login in settings: {error:#}");
        }
    }
    write_uninstall_entry(&target)?;
    Ok(target)
}

pub fn uninstall(purge: bool) -> Result<()> {
    set_launch_at_login(false)?;
    let path = crate::win::wide(UNINSTALL);
    unsafe {
        let _ = RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(path.as_ptr()));
    }
    if purge {
        for dir in [paths::settings_dir().ok(), paths::local_data_dir().ok()].into_iter().flatten() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
    // The running executable cannot delete itself; let a detached shell finish after this process exits.
    if let Ok(dir) = paths::install_dir() {
        let command = format!("timeout /t 2 /nobreak >NUL & rmdir /S /Q \"{}\"", dir.display());
        let _ = std::process::Command::new("cmd").args(["/C", &command]).creation_flags(CREATE_NO_WINDOW).spawn();
    }
    Ok(())
}

pub fn set_launch_at_login(enabled: bool) -> Result<()> {
    if !enabled {
        delete_value(RUN, RUN_VALUE)?;
        return Ok(());
    }
    let exe = installed_exe().or_else(|| std::env::current_exe().ok()).context("cannot locate vitals.exe")?;
    let command = format!("\"{}\" --background", exe.display());
    write_string(RUN, RUN_VALUE, &command)
}

fn write_uninstall_entry(exe: &Path) -> Result<()> {
    write_string(UNINSTALL, "DisplayName", "Vitals")?;
    write_string(UNINSTALL, "DisplayVersion", env!("CARGO_PKG_VERSION"))?;
    write_string(UNINSTALL, "Publisher", "Vitals contributors")?;
    write_string(UNINSTALL, "InstallLocation", &paths::install_dir()?.display().to_string())?;
    write_string(UNINSTALL, "DisplayIcon", &exe.display().to_string())?;
    write_string(UNINSTALL, "UninstallString", &format!("\"{}\" --uninstall", exe.display()))?;
    write_string(UNINSTALL, "QuietUninstallString", &format!("\"{}\" --uninstall", exe.display()))
}

fn write_string(path: &str, name: &str, value: &str) -> Result<()> {
    let key = create_key(path)?;
    let name = crate::win::wide(name);
    let data: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = unsafe { std::slice::from_raw_parts(data.as_ptr().cast::<u8>(), data.len() * 2) };
    let status = unsafe { RegSetValueExW(key, PCWSTR(name.as_ptr()), None, REG_SZ, Some(bytes)) };
    unsafe {
        let _ = RegCloseKey(key);
    }
    ensure!(status == ERROR_SUCCESS, "RegSetValueExW failed with {status:?}");
    Ok(())
}

fn delete_value(path: &str, name: &str) -> Result<()> {
    let path = crate::win::wide(path);
    let mut key = HKEY::default();
    let opened = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(path.as_ptr()),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )
    };
    if opened != ERROR_SUCCESS {
        return Ok(());
    }
    let name = crate::win::wide(name);
    unsafe {
        let _ = RegDeleteValueW(key, PCWSTR(name.as_ptr()));
        let _ = RegCloseKey(key);
    }
    Ok(())
}

fn create_key(path: &str) -> Result<HKEY> {
    let path = crate::win::wide(path);
    let mut key = HKEY::default();
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(path.as_ptr()),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut key,
            None,
        )
    };
    ensure!(status == ERROR_SUCCESS, "RegCreateKeyExW failed with {status:?}");
    Ok(key)
}
