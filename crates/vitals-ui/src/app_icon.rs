//! Shell icons for process rows, rasterized to straight-alpha BGRA and cached per path and size.

use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
};

use windows::{
    Win32::{
        Foundation::S_OK,
        Graphics::Gdi::{
            BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject,
            GetDIBits, GetObjectW, HBITMAP, HDC, HGDIOBJ,
        },
        Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES,
        UI::{
            Shell::{SHDefExtractIconW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGetFileInfoW},
            WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO},
        },
    },
    core::PCWSTR,
};

use crate::{Bitmap, Image};

const MAX_ICON_PX: u32 = 256;

type Icons = HashMap<(PathBuf, u32), Option<std::rc::Rc<Bitmap>>>;

thread_local! {
    static CACHE: RefCell<Icons> = RefCell::new(HashMap::new());
}

/// The shell icon of `exe`, `size_px` square when the file allows it. Cached per path and size on the UI thread.
pub fn shell_icon(exe: &Path, size_px: u32) -> Option<std::rc::Rc<Bitmap>> {
    let key = (exe.to_path_buf(), size_px);
    if let Some(icon) = CACHE.with(|cache| cache.borrow().get(&key).cloned()) {
        return icon;
    }
    let icon = icon_image(&extract_icon(exe, size_px)?).map(Bitmap::new);
    CACHE.with(|cache| cache.borrow_mut().insert(key, icon.clone()));
    icon
}

struct OwnedIcon(HICON);
impl Drop for OwnedIcon {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyIcon(self.0);
        }
    }
}

struct OwnedBitmap(HBITMAP);
impl Drop for OwnedBitmap {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe {
                let _ = DeleteObject(HGDIOBJ(self.0.0));
            }
        }
    }
}

struct MemoryDc(HDC);
impl Drop for MemoryDc {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteDC(self.0);
        }
    }
}

/// Releases cached GPU bitmaps while the device that created them is still alive.
///
/// The cache is thread-local, so it would otherwise be dropped during thread teardown after the D2D device has
/// already gone away, which can fault inside the graphics driver.
pub fn clear_cache() {
    CACHE.with(|cache| cache.borrow_mut().clear());
}

fn wide(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
}

fn extract_icon(exe: &Path, size_px: u32) -> Option<OwnedIcon> {
    let path = wide(exe);
    let mut icon = HICON::default();
    let size = size_px.clamp(1, MAX_ICON_PX);
    let extracted = unsafe { SHDefExtractIconW(PCWSTR(path.as_ptr()), 0, 0, Some(&mut icon), None, size) };
    if extracted == S_OK && !icon.is_invalid() {
        return Some(OwnedIcon(icon));
    }
    let mut info = SHFILEINFOW::default();
    let found = unsafe {
        SHGetFileInfoW(
            PCWSTR(path.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut info),
            size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        )
    };
    (found != 0 && !info.hIcon.is_invalid()).then_some(OwnedIcon(info.hIcon))
}

fn icon_image(icon: &OwnedIcon) -> Option<Image> {
    let mut info = ICONINFO::default();
    unsafe { GetIconInfo(icon.0, &mut info) }.ok()?;
    let color = OwnedBitmap(info.hbmColor);
    let mask = OwnedBitmap(info.hbmMask);
    if color.0.is_invalid() {
        return None;
    }
    let (width, height) = bitmap_size(&color)?;
    let mut pixels = bitmap_pixels(&color, width, height)?;
    if pixels.chunks_exact(4).all(|pixel| pixel[3] == 0) {
        apply_mask(&mut pixels, &bitmap_pixels(&mask, width, height)?);
    }
    Some(Image::from_bgra(width, height, pixels))
}

fn bitmap_size(bitmap: &OwnedBitmap) -> Option<(u32, u32)> {
    let mut info = BITMAP::default();
    let written =
        unsafe { GetObjectW(HGDIOBJ(bitmap.0.0), size_of::<BITMAP>() as i32, Some((&mut info as *mut BITMAP).cast())) };
    (written > 0 && info.bmWidth > 0 && info.bmHeight > 0).then_some((info.bmWidth as u32, info.bmHeight as u32))
}

fn bitmap_pixels(bitmap: &OwnedBitmap, width: u32, height: u32) -> Option<Vec<u8>> {
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    let dc = MemoryDc(unsafe { CreateCompatibleDC(None) });
    if dc.0.is_invalid() {
        return None;
    }
    let lines =
        unsafe { GetDIBits(dc.0, bitmap.0, 0, height, Some(pixels.as_mut_ptr().cast()), &mut info, DIB_RGB_COLORS) };
    (lines == height as i32).then_some(pixels)
}

fn apply_mask(pixels: &mut [u8], mask: &[u8]) {
    for (pixel, mask) in pixels.chunks_exact_mut(4).zip(mask.chunks_exact(4)) {
        pixel[3] = if mask[0] == 0 { 255 } else { 0 };
    }
}
