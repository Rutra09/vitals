//! Shared small Win32 helpers.

use std::{ffi::OsStr, os::windows::ffi::OsStrExt};

use vitals_core::{PointI, RectI};
use windows::Win32::{
    Foundation::POINT,
    Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
    },
    UI::{HiDpi::GetDpiForMonitor, WindowsAndMessaging::GetCursorPos},
};

pub fn wide(text: impl AsRef<OsStr>) -> Vec<u16> {
    text.as_ref().encode_wide().chain(std::iter::once(0)).collect()
}

pub fn cursor_pos() -> PointI {
    let mut point = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut point);
    }
    PointI::new(point.x, point.y)
}

pub fn work_area_at(point: PointI) -> RectI {
    let monitor = unsafe { MonitorFromPoint(POINT { x: point.x, y: point.y }, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
    if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        RectI::from_ltrb(info.rcWork.left, info.rcWork.top, info.rcWork.right, info.rcWork.bottom)
    } else {
        RectI::new(0, 0, 1920, 1080)
    }
}

pub fn scale_at(point: PointI) -> f32 {
    let monitor = unsafe { MonitorFromPoint(POINT { x: point.x, y: point.y }, MONITOR_DEFAULTTONEAREST) };
    let mut dpi_x = 96;
    let mut dpi_y = 96;
    let _ = unsafe { GetDpiForMonitor(monitor, windows::Win32::UI::HiDpi::MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) };
    dpi_x as f32 / 96.0
}

pub fn monitor_rect_at(point: PointI) -> RectI {
    let monitor = unsafe { MonitorFromPoint(POINT { x: point.x, y: point.y }, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
    if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        RectI::from_ltrb(info.rcMonitor.left, info.rcMonitor.top, info.rcMonitor.right, info.rcMonitor.bottom)
    } else {
        RectI::new(0, 0, 1920, 1080)
    }
}
