//! Notification-area icon and its native menu. The window proc only forwards commands through a channel.

use std::sync::{
    OnceLock,
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
};

use anyhow::{Result, ensure};
use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Shell::{
                NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION,
                NOTIFYICONDATAW, NOTIFYICON_VERSION_4, Shell_NotifyIconW,
            },
            WindowsAndMessaging::{
                AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DispatchMessageW,
                GetCursorPos, IDI_APPLICATION, IMAGE_ICON, LR_DEFAULTSIZE, LR_SHARED, LoadIconW, LoadImageW, MF_CHECKED,
                MF_SEPARATOR, MF_STRING, MSG, PostMessageW, PostQuitMessage, RegisterClassW, RegisterWindowMessageW, SendMessageW,
                SW_SHOW, ShowWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenuEx, TranslateMessage, WM_APP,
                WM_COMMAND, WM_CONTEXTMENU, WM_DESTROY, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP, WNDCLASSW, WS_OVERLAPPED,
                WINDOW_EX_STYLE,
            },
        },
    },
    core::{PCWSTR, w},
};

const CALLBACK: u32 = WM_APP + 1;
const MENU_OPEN: u32 = 1;
const MENU_QUICK: u32 = 2;
const MENU_PAUSE: u32 = 3;
const MENU_SETTINGS: u32 = 4;
const MENU_QUIT: u32 = 5;

const NIN_SELECT: u32 = 0x0400;
const NIN_KEYSELECT: u32 = 0x0401;

static SENDER: OnceLock<Sender<TrayCommand>> = OnceLock::new();
static PAUSED: AtomicBool = AtomicBool::new(false);
static CLASS_REGISTERED: OnceLock<()> = OnceLock::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayCommand {
    Open,
    QuickView,
    Pause(bool),
    Settings,
    Quit,
    Recreate,
}

pub struct Tray {
    hwnd: HWND,
    data: NOTIFYICONDATAW,
}

fn tooltip_buffer(text: &str) -> [u16; 128] {
    let mut result = [0u16; 128];
    for (destination, source) in result[..127].iter_mut().zip(text.encode_utf16()) {
        *destination = source;
    }
    if (0xd800..=0xdbff).contains(&result[126]) {
        result[126] = 0;
    }
    result
}

impl Tray {
    pub fn install(sender: Sender<TrayCommand>, paused: bool) -> Result<Self> {
        let _ = SENDER.set(sender);
        PAUSED.store(paused, Ordering::Relaxed);
        let module = unsafe { GetModuleHandleW(None) }?;
        let instance = HINSTANCE(module.0);
        register_class(instance)?;
        let class = crate::win::wide("Vitals.Tray");
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                PCWSTR(class.as_ptr()),
                w!("Vitals"),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance),
                None,
            )
        }?;
        let icon = unsafe {
            let loaded = LoadImageW(
                Some(instance),
                PCWSTR(1 as *const u16),
                IMAGE_ICON,
                0,
                0,
                LR_DEFAULTSIZE | LR_SHARED,
            );
            match loaded {
                Ok(icon) if !icon.is_invalid() => windows::Win32::UI::WindowsAndMessaging::HICON(icon.0),
                _ => LoadIconW(None, IDI_APPLICATION).unwrap_or_default(),
            }
        };
        let mut data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP,
            uCallbackMessage: CALLBACK,
            hIcon: icon,
            szTip: tooltip_buffer(&tooltip(paused)),
            ..Default::default()
        };
        ensure!(unsafe { Shell_NotifyIconW(NIM_ADD, &data) }.as_bool(), "could not add the tray icon");
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        if !unsafe { Shell_NotifyIconW(NIM_SETVERSION, &data) }.as_bool() {
            unsafe {
                let _ = Shell_NotifyIconW(NIM_DELETE, &data);
            }
            anyhow::bail!("could not set the tray icon version");
        }
        Ok(Self { hwnd, data })
    }

    pub fn set_paused(&mut self, paused: bool) -> Result<()> {
        PAUSED.store(paused, Ordering::Relaxed);
        self.data.szTip = tooltip_buffer(&tooltip(paused));
        self.data.uFlags = NIF_TIP | NIF_SHOWTIP;
        ensure!(unsafe { Shell_NotifyIconW(NIM_MODIFY, &self.data) }.as_bool(), "could not update the tray tooltip");
        Ok(())
    }

    pub fn recreate(&mut self) -> Result<()> {
        self.data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
        ensure!(unsafe { Shell_NotifyIconW(NIM_ADD, &self.data) }.as_bool(), "could not re-add the tray icon");
        self.data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        ensure!(unsafe { Shell_NotifyIconW(NIM_SETVERSION, &self.data) }.as_bool(), "could not set the tray icon version");
        Ok(())
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &self.data);
        }
    }
}

fn tooltip(paused: bool) -> String {
    if paused { "Vitals — sampling paused".into() } else { "Vitals — Ctrl+Shift+Esc".into() }
}

fn register_class(instance: HINSTANCE) -> Result<()> {
    if CLASS_REGISTERED.get().is_some() {
        return Ok(());
    }
    let class = crate::win::wide("Vitals.Tray");
    let wnd_class = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        lpszClassName: PCWSTR(class.as_ptr()),
        ..Default::default()
    };
    let atom = unsafe { RegisterClassW(&wnd_class) };
    if atom == 0 {
        anyhow::bail!("could not register the tray window class");
    }
    let _ = CLASS_REGISTERED.set(());
    Ok(())
}

unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        CALLBACK => {
            let event = (lparam.0 as u32) & 0xFFFF;
            match event {
                WM_LBUTTONUP | WM_RBUTTONUP | WM_CONTEXTMENU | NIN_SELECT | NIN_KEYSELECT => {
                    if event == WM_RBUTTONUP || event == WM_CONTEXTMENU {
                        show_menu(hwnd);
                    } else {
                        send(TrayCommand::Open);
                    }
                    return LRESULT(0);
                }
                _ => {}
            }
        }
        WM_COMMAND => {
            let id = (wparam.0 as u32) & 0xFFFF;
            let command = match id {
                MENU_OPEN => Some(TrayCommand::Open),
                MENU_QUICK => Some(TrayCommand::QuickView),
                MENU_PAUSE => Some(TrayCommand::Pause(!PAUSED.load(Ordering::Relaxed))),
                MENU_SETTINGS => Some(TrayCommand::Settings),
                MENU_QUIT => Some(TrayCommand::Quit),
                _ => None,
            };
            if let Some(command) = command {
                send(command);
            }
            return LRESULT(0);
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            return LRESULT(0);
        }
        _ => {
            let taskbar = taskbar_created_message();
            if taskbar != 0 && message == taskbar {
                send(TrayCommand::Recreate);
                return LRESULT(0);
            }
        }
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

fn taskbar_created_message() -> u32 {
    static MESSAGE: OnceLock<u32> = OnceLock::new();
    *MESSAGE.get_or_init(|| unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) })
}

fn send(command: TrayCommand) {
    if let Some(sender) = SENDER.get() {
        let _ = sender.send(command);
    }
}

fn show_menu(hwnd: HWND) {
    let paused = PAUSED.load(Ordering::Relaxed);
    let Ok(menu) = (unsafe { CreatePopupMenu() }) else { return };
    let append = |flags: windows::Win32::UI::WindowsAndMessaging::MENU_ITEM_FLAGS, id: u32, label: &str| {
        let text = crate::win::wide(label);
        unsafe {
            let _ = AppendMenuW(menu, flags, id as usize, PCWSTR(text.as_ptr()));
        }
    };
    append(MF_STRING, MENU_OPEN, "Open Vitals");
    append(MF_STRING, MENU_QUICK, "Quick view\tCtrl+Shift+Esc");
    append(MF_SEPARATOR, 0, "");
    append(if paused { MF_STRING | MF_CHECKED } else { MF_STRING }, MENU_PAUSE, "Pause sampling");
    append(MF_STRING, MENU_SETTINGS, "Settings…");
    append(MF_SEPARATOR, 0, "");
    append(MF_STRING, MENU_QUIT, "Quit Vitals");
    let mut point = windows::Win32::Foundation::POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut point);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let selected = TrackPopupMenuEx(menu, (TPM_RETURNCMD | TPM_RIGHTBUTTON).0, point.x, point.y, hwnd, None).0 as u32;
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        if selected != 0 {
            let _ = SendMessageW(hwnd, WM_COMMAND, Some(WPARAM(selected as usize)), Some(LPARAM(0)));
        }
        let _ = DestroyMenu(menu);
    }
}

/// Runs a message loop for windows created on this thread; used only by the standalone tray test.
pub fn pump_messages() {
    let mut message = MSG::default();
    while unsafe { windows::Win32::UI::WindowsAndMessaging::GetMessageW(&mut message, None, 0, 0) }.0 > 0 {
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}
