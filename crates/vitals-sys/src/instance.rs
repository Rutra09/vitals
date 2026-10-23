//! Single instance, cross-process activation and focus forwarding. The second copy asks the first to show its
//! dashboard (or quit) and exits.

use anyhow::{Context, Result};
use windows::{
    Win32::{
        Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE},
        System::Threading::{
            CreateEventW, CreateMutexW, EVENT_MODIFY_STATE, INFINITE, OpenEventW, SetEvent, WaitForSingleObject,
        },
        UI::WindowsAndMessaging::{
            FindWindowW, IsIconic, SW_RESTORE, SW_SHOW, SetForegroundWindow, ShowWindow,
        },
    },
    core::{PCWSTR, w},
};

use crate::win::wide;

pub const ACTIVATE_EVENT: &str = r"Local\Vitals.Activate";
pub const QUIT_EVENT: &str = r"Local\Vitals.Quit";

pub struct InstanceGuard(HANDLE);

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub enum InstanceRole {
    Primary(InstanceGuard),
    Secondary,
}

pub fn acquire() -> InstanceRole {
    unsafe {
        match CreateMutexW(None, false, w!("Local\\Vitals.SingleInstance")) {
            Ok(handle) => {
                if GetLastError() == ERROR_ALREADY_EXISTS {
                    InstanceRole::Secondary
                } else {
                    InstanceRole::Primary(InstanceGuard(handle))
                }
            }
            Err(error) => {
                log::error!("single-instance mutex failed: {error}");
                InstanceRole::Secondary
            }
        }
    }
}

/// A named, auto-reset event owned by the running copy. Waiting threads block until another process signals it.
pub struct Signal(HANDLE);

impl Signal {
    pub fn create(name: &str) -> Result<Self> {
        let name = wide(name);
        let handle = unsafe { CreateEventW(None, false, false, PCWSTR(name.as_ptr())) }
            .with_context(|| format!("could not create {}", String::from_utf16_lossy(&name[..name.len() - 1])))?;
        Ok(Self(handle))
    }

    pub fn wait(&self) {
        unsafe {
            WaitForSingleObject(self.0, INFINITE);
        }
    }

    pub fn request(name: &str) -> Result<bool> {
        let name = wide(name);
        let handle = match unsafe { OpenEventW(EVENT_MODIFY_STATE, false, PCWSTR(name.as_ptr())) } {
            Ok(handle) => handle,
            Err(_) => return Ok(false),
        };
        let result = unsafe { SetEvent(handle) };
        unsafe {
            let _ = CloseHandle(handle);
        }
        result.context("could not signal Vitals")?;
        Ok(true)
    }
}

// SAFETY: Windows event handles are process-wide kernel handles and may be waited on from any thread.
unsafe impl Send for Signal {}

impl Drop for Signal {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Asks the running copy to open its dashboard. False when no copy is listening.
pub fn request_activate() -> Result<bool> {
    Signal::request(ACTIVATE_EVENT)
}

/// Asks the running copy to exit. False when no copy is listening.
pub fn request_quit() -> Result<bool> {
    Signal::request(QUIT_EVENT)
}

/// Fallback for older builds: brings the primary dashboard forward. Returns false when it is not on screen yet.
pub fn activate_existing() -> Result<bool> {
    let title = wide("Vitals");
    for attempt in 0..20 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        if let Ok(window) = unsafe { FindWindowW(PCWSTR::null(), PCWSTR(title.as_ptr())) } {
            if !window.is_invalid() {
                unsafe {
                    let _ = ShowWindow(window, if IsIconic(window).as_bool() { SW_RESTORE } else { SW_SHOW });
                    let _ = SetForegroundWindow(window);
                }
                return Ok(true);
            }
        }
    }
    Ok(false)
}
