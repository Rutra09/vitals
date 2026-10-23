//! Global Win32 shortcut ownership.
//!
//! `RegisterHotKey` cannot take Ctrl+Shift+Esc: Windows reserves it for Task Manager. A priority
//! `WH_KEYBOARD_LL` hook sees it first and returns 1 to swallow the key, so Task Manager never opens.
//! When Vitals is not running the hook is gone and Windows' shortcut works again.

use std::{
    sync::{
        OnceLock,
        atomic::{AtomicBool, AtomicU8, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread::{self, JoinHandle},
};

use anyhow::{Context, Result};
use windows::Win32::{
    Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM},
    System::{
        LibraryLoader::GetModuleHandleW,
        Threading::{GetCurrentThread, GetCurrentThreadId, SetThreadPriority, THREAD_PRIORITY_TIME_CRITICAL},
    },
    UI::{
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, VK_CONTROL, VK_ESCAPE, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_RCONTROL,
            VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT,
        },
        WindowsAndMessaging::{
            CallNextHookEx, GetMessageW, HC_ACTION, KBDLLHOOKSTRUCT, LLKHF_INJECTED, LLKHF_UP, MSG, PostThreadMessageW,
            SetTimer, SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_QUIT, WM_TIMER,
        },
    },
};

const REINSTALL_INTERVAL_MS: u32 = 600_000;
const CONFIG_TASK_MANAGER: u8 = 1;
const CONFIG_LEGACY: u8 = 1 << 1;

static CONFIG: AtomicU8 = AtomicU8::new(CONFIG_TASK_MANAGER | CONFIG_LEGACY);
static EVENTS: OnceLock<Sender<Hotkey>> = OnceLock::new();
static SWALLOW_ESCAPE: AtomicBool = AtomicBool::new(false);
static ESCAPE_DOWN: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hotkey {
    QuickView,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HookConfig {
    pub task_manager: bool,
    pub legacy_task_manager: bool,
}

impl Default for HookConfig {
    fn default() -> Self {
        Self { task_manager: true, legacy_task_manager: true }
    }
}

impl From<&vitals_core::Settings> for HookConfig {
    fn from(settings: &vitals_core::Settings) -> Self {
        Self {
            task_manager: settings.replace_task_manager,
            legacy_task_manager: settings.replace_legacy_task_manager,
        }
    }
}

impl HookConfig {
    fn bits(self) -> u8 {
        (self.task_manager as u8 * CONFIG_TASK_MANAGER) | (self.legacy_task_manager as u8 * CONFIG_LEGACY)
    }
}

pub struct InputHook {
    thread: Option<JoinHandle<()>>,
    thread_id: u32,
}

impl InputHook {
    /// Installs the hook and returns the stream of shortcuts it owns. Fails only when Windows refuses the hook.
    pub fn start(config: HookConfig) -> Result<(Self, Receiver<Hotkey>)> {
        CONFIG.store(config.bits(), Ordering::Relaxed);
        let (events, receiver) = mpsc::channel();
        let (ready, wait) = mpsc::sync_channel::<Result<u32>>(1);
        let events = EVENTS.get_or_init(|| events).clone();
        let thread = thread::Builder::new()
            .name("vitals-hotkey".into())
            .spawn(move || run_hook_thread(events, ready))
            .context("could not start the hotkey thread")?;
        let thread_id = wait.recv().context("hotkey thread stopped before reporting")??;
        Ok((Self { thread: Some(thread), thread_id }, receiver))
    }

    pub fn set_config(&self, config: HookConfig) {
        CONFIG.store(config.bits(), Ordering::Relaxed);
    }
}

impl Drop for InputHook {
    fn drop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run_hook_thread(_events: Sender<Hotkey>, ready: mpsc::SyncSender<Result<u32>>) {
    unsafe {
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL);
        let _ = windows::Win32::UI::WindowsAndMessaging::PeekMessageW(
            &mut MSG::default(),
            None,
            0,
            0,
            windows::Win32::UI::WindowsAndMessaging::PM_NOREMOVE,
        );
    }
    let Ok(module) = (unsafe { GetModuleHandleW(None) }) else {
        let _ = ready.send(Err(anyhow::anyhow!("no module handle")));
        return;
    };
    let mut hook = match unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), Some(HINSTANCE(module.0)), 0) } {
        Ok(hook) => hook,
        Err(error) => {
            let _ = ready.send(Err(error.into()));
            return;
        }
    };
    let thread_id = unsafe { GetCurrentThreadId() };
    let timer = unsafe { SetTimer(None, 0, REINSTALL_INTERVAL_MS, None) };
    let _ = ready.send(Ok(thread_id));

    let mut message = MSG::default();
    loop {
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if result.0 <= 0 {
            break;
        }
        if message.message == WM_TIMER && message.wParam.0 == timer {
            if let Ok(replacement) =
                unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), Some(HINSTANCE(module.0)), 0) }
            {
                unsafe {
                    let _ = UnhookWindowsHookEx(hook);
                }
                hook = replacement;
            }
        }
    }
    unsafe {
        let _ = UnhookWindowsHookEx(hook);
        if timer != 0 {
            let _ = windows::Win32::UI::WindowsAndMessaging::KillTimer(None, timer);
        }
    }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let event = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        if !event.flags.contains(LLKHF_INJECTED) {
            let vk = event.vkCode as u16;
            let down = !event.flags.contains(LLKHF_UP);
            if vk == VK_ESCAPE.0 {
                if down {
                    if SWALLOW_ESCAPE.load(Ordering::Relaxed) {
                        return LRESULT(1);
                    }
                    if !ESCAPE_DOWN.swap(true, Ordering::Relaxed) {
                        let ctrl = down_any(&[VK_CONTROL, VK_LCONTROL, VK_RCONTROL]);
                        let shift = down_any(&[VK_SHIFT, VK_LSHIFT, VK_RSHIFT]);
                        let alt = down_any(&[VK_MENU, VK_LMENU, VK_RMENU]);
                        let win = down_any(&[VK_LWIN, VK_RWIN]);
                        let config = CONFIG.load(Ordering::Relaxed);
                        let quick = ctrl
                            && !alt
                            && !win
                            && ((shift && config & CONFIG_TASK_MANAGER != 0)
                                || (!shift && config & CONFIG_LEGACY != 0));
                        if quick {
                            SWALLOW_ESCAPE.store(true, Ordering::Relaxed);
                            if let Some(events) = EVENTS.get() {
                                let _ = events.send(Hotkey::QuickView);
                            }
                            return LRESULT(1);
                        }
                    }
                } else {
                    ESCAPE_DOWN.store(false, Ordering::Relaxed);
                    if SWALLOW_ESCAPE.swap(false, Ordering::Relaxed) {
                        return LRESULT(1);
                    }
                }
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn down_any(keys: &[windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY]) -> bool {
    keys.iter().any(|key| unsafe { GetAsyncKeyState(key.0 as i32) as u16 & 0x8000 != 0 })
}
