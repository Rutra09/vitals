//! Small safe wrappers for the Win32 calls the sampler makes.

use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    os::windows::ffi::OsStringExt,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{CloseHandle, FILETIME, HANDLE},
        Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY},
        Storage::FileSystem::{
            GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
        },
        System::Threading::{
            OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
            QueryFullProcessImageNameW,
        },
    },
};

const FALLBACK_TRANSLATIONS: [(u16, u16); 3] = [(0x0409, 0x04b0), (0x0409, 0x04e4), (0x0000, 0x04b0)];

pub fn wide(text: impl AsRef<OsStr>) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    text.as_ref().encode_wide().chain(std::iter::once(0)).collect()
}

pub fn from_wide(text: &[u16]) -> String {
    OsString::from_wide(text).to_string_lossy().into_owned()
}

pub struct OwnedHandle(pub HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

pub fn open_process(pid: u32, rights: windows::Win32::System::Threading::PROCESS_ACCESS_RIGHTS) -> Option<OwnedHandle> {
    unsafe { OpenProcess(rights, false, pid) }.ok().map(OwnedHandle)
}

pub fn query_handle(pid: u32) -> Option<OwnedHandle> {
    open_process(pid, PROCESS_QUERY_LIMITED_INFORMATION)
}

pub fn process_path(process: &OwnedHandle) -> Option<PathBuf> {
    let mut buffer = [0u16; 1024];
    let mut length = buffer.len() as u32;
    unsafe { QueryFullProcessImageNameW(process.0, PROCESS_NAME_WIN32, windows::core::PWSTR(buffer.as_mut_ptr()), &mut length) }
        .ok()?;
    Some(PathBuf::from(from_wide(&buffer[..length as usize])))
}

pub fn is_elevated(process: &OwnedHandle) -> bool {
    let mut token = HANDLE::default();
    if unsafe { OpenProcessToken(process.0, TOKEN_QUERY, &mut token) }.is_err() {
        return false;
    }
    let token = OwnedHandle(token);
    let mut elevation = TOKEN_ELEVATION::default();
    let mut length = 0;
    let read = unsafe {
        GetTokenInformation(
            token.0,
            windows::Win32::Security::TokenElevation,
            Some((&mut elevation as *mut TOKEN_ELEVATION).cast()),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut length,
        )
    };
    read.is_ok() && elevation.TokenIsElevated != 0
}

pub fn filetime(value: FILETIME) -> u64 {
    ((value.dwHighDateTime as u64) << 32) | value.dwLowDateTime as u64
}

/// The app's FileDescription (e.g. "Google Chrome"), else the file stem. Cached per path.
pub fn display_name(exe: &Path) -> String {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, String>>> = OnceLock::new();
    let map = CACHE.get_or_init(Default::default);
    if let Some(name) = map.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).get(exe) {
        return name.clone();
    }
    let name = file_description(exe).unwrap_or_else(|| {
        exe.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default()
    });
    map.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(exe.to_path_buf(), name.clone());
    name
}

fn file_description(exe: &Path) -> Option<String> {
    let block = version_block(exe)?;
    translations(&block).into_iter().chain(FALLBACK_TRANSLATIONS).find_map(|(language, codepage)| {
        query_string(&block, &format!("\\StringFileInfo\\{language:04x}{codepage:04x}\\FileDescription"))
    })
}

fn version_block(exe: &Path) -> Option<Vec<u8>> {
    let path = wide(exe);
    let size = unsafe { GetFileVersionInfoSizeW(PCWSTR(path.as_ptr()), None) };
    if size == 0 {
        return None;
    }
    let mut block = vec![0u8; size as usize];
    unsafe { GetFileVersionInfoW(PCWSTR(path.as_ptr()), None, size, block.as_mut_ptr().cast()) }.ok()?;
    Some(block)
}

fn query_value(block: &[u8], sub_block: &str) -> Option<(*const u8, usize)> {
    let sub_block = wide(sub_block);
    let mut value = std::ptr::null_mut();
    let mut length = 0u32;
    let found =
        unsafe { VerQueryValueW(block.as_ptr().cast(), PCWSTR(sub_block.as_ptr()), &mut value, &mut length) }.as_bool();
    (found && !value.is_null() && length > 0).then_some((value.cast_const().cast(), length as usize))
}

fn translations(block: &[u8]) -> Vec<(u16, u16)> {
    let Some((value, bytes)) = query_value(block, "\\VarFileInfo\\Translation") else {
        return Vec::new();
    };
    let table = unsafe { std::slice::from_raw_parts(value, bytes) };
    table
        .chunks_exact(4)
        .map(|entry| (u16::from_le_bytes([entry[0], entry[1]]), u16::from_le_bytes([entry[2], entry[3]])))
        .collect()
}

fn query_string(block: &[u8], sub_block: &str) -> Option<String> {
    let (value, characters) = query_value(block, sub_block)?;
    let text = from_wide(unsafe { std::slice::from_raw_parts(value.cast::<u16>(), characters) });
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

