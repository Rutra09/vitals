//! Startup entries from the two Run keys and both Startup folders, with enable/disable through the same
//! `StartupApproved` state Windows uses in Task Manager.

use std::path::Path;

use anyhow::{Context, Result, bail};
use windows::{
    Win32::{
        Foundation::ERROR_SUCCESS,
        System::Registry::{
            HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE, REG_BINARY,
            REG_EXPAND_SZ, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey, RegCreateKeyExW, RegEnumValueW,
            RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
        },
    },
    core::{PCWSTR, PWSTR},
};

use crate::win::{from_wide, wide};

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const STARTUP_USER: &str = r"Microsoft\Windows\Start Menu\Programs\Startup";
const APPROVED_RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const APPROVED_STARTUP_FOLDER: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupScope {
    User,
    Machine,
}

impl StartupScope {
    pub fn label(self) -> &'static str {
        match self {
            Self::User => "This account",
            Self::Machine => "All users",
        }
    }

    fn root(self) -> HKEY {
        match self {
            Self::User => HKEY_CURRENT_USER,
            Self::Machine => HKEY_LOCAL_MACHINE,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupSource {
    RunKey,
    StartupFolder,
}

impl StartupSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::RunKey => "Registry Run key",
            Self::StartupFolder => "Startup folder",
        }
    }

    fn approved_path(self) -> &'static str {
        match self {
            Self::RunKey => APPROVED_RUN,
            Self::StartupFolder => APPROVED_STARTUP_FOLDER,
        }
    }

    fn id_prefix(self) -> &'static str {
        match self {
            Self::RunKey => "run",
            Self::StartupFolder => "startup-folder",
        }
    }
}

#[derive(Clone, Debug)]
pub struct StartupItem {
    pub name: String,
    pub command: String,
    pub source: StartupSource,
    pub scope: StartupScope,
    pub enabled: bool,
    /// Value name in the `StartupApproved` key. For startup folders this includes the file extension, while
    /// `name` is only the display stem.
    approval_name: String,
}

impl StartupItem {
    /// Stable across refreshes for a given entry, without exposing registry formats to the UI.
    pub fn id(&self) -> String {
        format!("{}:{:?}:{}", self.source.id_prefix(), self.scope, self.approval_name)
    }
}

pub fn startup_items() -> Vec<StartupItem> {
    let mut items = Vec::new();
    items.extend(run_values(HKEY_CURRENT_USER, RUN, StartupScope::User));
    items.extend(run_values(HKEY_LOCAL_MACHINE, RUN, StartupScope::Machine));
    if let Ok(appdata) = std::env::var("APPDATA") {
        items.extend(folder_items(Path::new(&appdata).join(STARTUP_USER), StartupScope::User));
    }
    if let Ok(programdata) = std::env::var("PROGRAMDATA") {
        items.extend(folder_items(Path::new(&programdata).join(STARTUP_USER), StartupScope::Machine));
    }
    items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    items
}

/// Enables or disables a startup entry using the native `StartupApproved` values. All-users entries need
/// administrator rights because they live under HKLM.
pub fn set_startup_enabled(id: &str, enabled: bool) -> Result<()> {
    let item = startup_items()
        .into_iter()
        .find(|item| item.id() == id)
        .context("That startup entry no longer exists.")?;
    let result = write_approval(
        item.scope.root(),
        item.source.approved_path(),
        &item.approval_name,
        enabled,
    );
    result.with_context(|| {
        let verb = if enabled { "enable" } else { "disable" };
        match item.scope {
            StartupScope::User => format!("Could not {verb} \"{}\".", item.name),
            StartupScope::Machine => format!(
                "Could not {verb} \"{}\". All-users startup entries need administrator rights.",
                item.name
            ),
        }
    })
}

fn run_values(root: HKEY, path: &str, scope: StartupScope) -> Vec<StartupItem> {
    let mut items = Vec::new();
    let key_path = wide(path);
    let mut key = HKEY::default();
    let opened = unsafe { RegOpenKeyExW(root, PCWSTR(key_path.as_ptr()), Some(0), KEY_READ, &mut key) };
    if opened != ERROR_SUCCESS {
        return items;
    }
    let mut index = 0u32;
    loop {
        let mut name = [0u16; 512];
        let mut name_len = name.len() as u32;
        let mut data = [0u8; 8192];
        let mut data_len = data.len() as u32;
        let mut kind = 0u32;
        let status = unsafe {
            RegEnumValueW(
                key,
                index,
                Some(PWSTR(name.as_mut_ptr())),
                &mut name_len,
                None,
                Some(&mut kind),
                Some(data.as_mut_ptr()),
                Some(&mut data_len),
            )
        };
        if status != ERROR_SUCCESS {
            break;
        }
        index += 1;
        if kind != REG_SZ.0 && kind != REG_EXPAND_SZ.0 {
            continue;
        }
        let value = String::from_utf16_lossy(
            &data[..data_len as usize]
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect::<Vec<_>>(),
        );
        let value = value.trim_end_matches('\0').to_string();
        if value.is_empty() {
            continue;
        }
        let name = from_wide(&name[..name_len as usize]).trim_end_matches('\0').to_string();
        let enabled = approval_enabled(root, APPROVED_RUN, &name);
        items.push(StartupItem {
            approval_name: name.clone(),
            name,
            command: value,
            source: StartupSource::RunKey,
            scope,
            enabled,
        });
    }
    unsafe {
        let _ = RegCloseKey(key);
    }
    items
}

fn folder_items(folder: std::path::PathBuf, scope: StartupScope) -> Vec<StartupItem> {
    let Ok(entries) = std::fs::read_dir(&folder) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| {
            let path = entry.path();
            let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or_default().to_lowercase();
            if extension != "lnk" && extension != "exe" && extension != "bat" && extension != "cmd" {
                return None;
            }
            let file_name = path.file_name()?.to_string_lossy().into_owned();
            Some(StartupItem {
                name: path.file_stem()?.to_string_lossy().into_owned(),
                approval_name: file_name.clone(),
                command: path.to_string_lossy().into_owned(),
                source: StartupSource::StartupFolder,
                scope,
                enabled: approval_enabled(scope.root(), APPROVED_STARTUP_FOLDER, &file_name),
            })
        })
        .collect()
}

/// Reads the first byte of the `StartupApproved` blob: `03` means disabled, anything else (or a missing
/// value) means Windows will start it.
fn approval_enabled(root: HKEY, path: &str, name: &str) -> bool {
    let key_path = wide(path);
    let mut key = HKEY::default();
    let opened = unsafe { RegOpenKeyExW(root, PCWSTR(key_path.as_ptr()), Some(0), KEY_READ, &mut key) };
    if opened != ERROR_SUCCESS {
        return true;
    }
    let name = wide(name);
    let mut data = [0u8; 16];
    let mut data_len = data.len() as u32;
    let status = unsafe {
        RegQueryValueExW(
            key,
            PCWSTR(name.as_ptr()),
            None,
            None,
            Some(data.as_mut_ptr()),
            Some(&mut data_len),
        )
    };
    unsafe {
        let _ = RegCloseKey(key);
    }
    status != ERROR_SUCCESS || data_len == 0 || data[0] != 3
}

fn write_approval(root: HKEY, path: &str, name: &str, enabled: bool) -> Result<()> {
    let key_path = wide(path);
    let mut key = HKEY::default();
    let opened = unsafe {
        RegCreateKeyExW(
            root,
            PCWSTR(key_path.as_ptr()),
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
        bail!("StartupApproved key could not be opened ({opened:?}).");
    }
    let name = wide(name);
    // 02 = enabled, 03 = disabled. The remaining bytes hold the disable timestamp; zero is accepted.
    let mut data = [0u8; 12];
    data[0] = if enabled { 2 } else { 3 };
    let status = unsafe { RegSetValueExW(key, PCWSTR(name.as_ptr()), None, REG_BINARY, Some(&data)) };
    unsafe {
        let _ = RegCloseKey(key);
    }
    if status != ERROR_SUCCESS {
        bail!("StartupApproved value could not be written ({status:?}).");
    }
    Ok(())
}
