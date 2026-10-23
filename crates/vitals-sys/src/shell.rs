use std::{ffi::c_void, path::Path};

use anyhow::{Result, ensure};
use windows::{
    Win32::{
        Foundation::{HANDLE, HWND},
        System::{
            Com::CoTaskMemFree,
            DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData},
            Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
            Ole::CF_UNICODETEXT,
        },
        UI::{
            Shell::{ILCreateFromPathW, SHOpenFolderAndSelectItems, ShellExecuteW},
            WindowsAndMessaging::SW_SHOWNORMAL,
        },
    },
    core::{PCWSTR, w},
};

/// Opens Explorer with `path` selected.
pub fn reveal(path: &Path) -> Result<()> {
    ensure!(path.exists(), "{} does not exist", path.display());
    // `explorer.exe /select,<path>` goes through Explorer's own command-line parser, which often loses the
    // selection when the path contains spaces or quotes. The shell API is the direct way to open the containing
    // folder with the item focused.
    let wide = crate::win::wide(path);
    // SAFETY: `wide` is a valid NUL-terminated path for the duration of the call. The returned PIDL is freed below.
    let pidl = unsafe { ILCreateFromPathW(PCWSTR(wide.as_ptr())) };
    ensure!(!pidl.is_null(), "could not resolve {}", path.display());
    // SAFETY: `pidl` came from ILCreateFromPathW and is valid until CoTaskMemFree.
    let result = unsafe { SHOpenFolderAndSelectItems(pidl, None, 0) };
    // SAFETY: PIDLs returned by ILCreateFromPathW are allocated with the COM task allocator.
    unsafe { CoTaskMemFree(Some(pidl.cast::<c_void>())) };
    result.map_err(Into::into)
}

pub fn open_url(url: &str) -> Result<()> {
    let url = crate::win::wide(url);
    let result = unsafe {
        ShellExecuteW(
            Some(HWND::default()),
            w!("open"),
            PCWSTR(url.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    ensure!(result.0 as isize > 32, "ShellExecuteW failed");
    Ok(())
}

/// Opens a file, folder or document with its registered handler.
pub fn open_path(path: &Path) -> Result<()> {
    ensure!(path.exists(), "{} does not exist", path.display());
    open_url(&path.to_string_lossy())
}

/// Puts Unicode text on the clipboard, replacing its previous contents.
pub fn copy_text(text: &str) -> Result<()> {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = wide.len() * size_of::<u16>();
    unsafe {
        OpenClipboard(None)?;
        let result = (|| -> Result<()> {
            EmptyClipboard()?;
            let block = GlobalAlloc(GMEM_MOVEABLE, bytes)?;
            let destination = GlobalLock(block);
            ensure!(!destination.is_null(), "GlobalLock failed");
            std::ptr::copy_nonoverlapping(wide.as_ptr().cast::<u8>(), destination.cast::<u8>(), bytes);
            let _ = GlobalUnlock(block);
            SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(block.0)))?;
            Ok(())
        })();
        let _ = CloseClipboard();
        result
    }
}
