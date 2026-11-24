//! Notification area ("tray") icon, through `Shell_NotifyIconW`.
//!
//! The icon stays while the app runs. Its tooltip shows the status line of the main window,
//! and it turns gray when no key is being remapped (the gray copy is made at runtime from the
//! app icon, so there is no second icon file). Mouse and keyboard events arrive at the main
//! window as `WM_APP_TRAY` (see `gui::wnd_proc`).
//!
//! If Explorer restarts, the icon disappears; `gui` calls `add` again when it gets the
//! `taskbar_created` message.

use std::cell::RefCell;
use std::mem::{size_of, zeroed};
use std::ptr::null_mut;
use std::sync::atomic::{AtomicU32, Ordering};

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Gdi::{
    CreateDIBSection, DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS,
};
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIIF_LARGE_ICON, NIIF_USER, NIM_ADD,
    NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NOTIFYICONDATAW, NOTIFYICON_VERSION_4,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, GetIconInfo, RegisterWindowMessageW, HICON, ICONINFO, SM_CXICON, SM_CXSMICON, WM_APP,
};

use crate::gui::app_icon;

/// Tray icon events. With `NOTIFYICON_VERSION_4` the event is in the low word of `lParam`
/// and the anchor point (screen coordinates) in `wParam`.
pub const WM_APP_TRAY: u32 = WM_APP + 3;
/// `NIN_SELECT | NINF_KEY`: the icon was activated with the keyboard.
pub const NIN_KEYSELECT: u32 = 0x0401;

const ICON_ID: u32 = 1;

struct State {
    normal: HICON,
    gray: HICON,
    /// Last tooltip and active state, re-applied when the icon is added again.
    tip: String,
    active: bool,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Copies `text` into a fixed, null-terminated UTF-16 buffer, truncating if needed.
fn copy_into(buf: &mut [u16], text: &str) {
    let max = buf.len() - 1;
    let mut n = 0;
    for (dst, src) in buf.iter_mut().zip(text.encode_utf16().take(max)) {
        *dst = src;
        n += 1;
    }
    buf[n] = 0;
}

/// Registered message Explorer broadcasts when the taskbar is (re)created.
pub fn taskbar_created() -> u32 {
    static MSG: AtomicU32 = AtomicU32::new(0);
    let mut id = MSG.load(Ordering::Relaxed);
    if id == 0 {
        id = unsafe { RegisterWindowMessageW(w("TaskbarCreated").as_ptr()) };
        MSG.store(id, Ordering::Relaxed);
    }
    id
}

/// Gray, semi-transparent copy of a 32-bit icon. Null if the icon has no color bitmap or a
/// GDI call fails.
unsafe fn gray_copy(icon: HICON) -> HICON {
    let mut info: ICONINFO = zeroed();
    if GetIconInfo(icon, &mut info) == 0 {
        return null_mut();
    }
    let mut result = null_mut();
    let mut bm: BITMAP = zeroed();
    if !info.hbmColor.is_null() && GetObjectW(info.hbmColor, size_of::<BITMAP>() as i32, (&mut bm as *mut BITMAP).cast()) != 0 {
        let (width, height) = (bm.bmWidth, bm.bmHeight);
        let mut bmi: BITMAPINFO = zeroed();
        bmi.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..zeroed()
        };
        let mut pixels = vec![0u32; (width * height).max(0) as usize];
        let dc = GetDC(null_mut());
        let read = GetDIBits(dc, info.hbmColor, 0, height as u32, pixels.as_mut_ptr().cast(), &mut bmi, DIB_RGB_COLORS);
        if read == height {
            // Icons without alpha (all zero) are opaque where the mask says so: keep alpha as is.
            let has_alpha = pixels.iter().any(|p| p >> 24 != 0);
            for p in &mut pixels {
                let (a, r, g, b) = (*p >> 24, (*p >> 16) & 0xFF, (*p >> 8) & 0xFF, *p & 0xFF);
                let y = (r * 299 + g * 587 + b * 114) / 1000;
                let a = if has_alpha { a * 55 / 100 } else { a };
                *p = (a << 24) | (y << 16) | (y << 8) | y;
            }
            let mut bits = null_mut();
            let color = CreateDIBSection(dc, &bmi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if !color.is_null() && !bits.is_null() {
                std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast::<u32>(), pixels.len());
                let gray_info = ICONINFO { fIcon: 1, xHotspot: 0, yHotspot: 0, hbmMask: info.hbmMask, hbmColor: color };
                result = CreateIconIndirect(&gray_info);
            }
            if !color.is_null() {
                DeleteObject(color);
            }
        }
        ReleaseDC(null_mut(), dc);
    }
    if !info.hbmColor.is_null() {
        DeleteObject(info.hbmColor);
    }
    if !info.hbmMask.is_null() {
        DeleteObject(info.hbmMask);
    }
    result
}

fn base(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut data: NOTIFYICONDATAW = unsafe { zeroed() };
    data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = hwnd;
    data.uID = ICON_ID;
    data
}

/// Icon and tooltip fields from the saved state.
fn with_state(data: &mut NOTIFYICONDATAW) {
    STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            data.uFlags |= NIF_ICON | NIF_TIP | NIF_SHOWTIP;
            data.hIcon = if state.active || state.gray.is_null() { state.normal } else { state.gray };
            copy_into(&mut data.szTip, &state.tip);
        }
    });
}

/// Adds the icon (again, after an Explorer restart) with the last tooltip and state.
pub unsafe fn add(hwnd: HWND) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.is_none() {
            let normal = app_icon(SM_CXSMICON);
            *s = Some(State { normal, gray: gray_copy(normal), tip: "Keyboard Reprogramer".into(), active: true });
        }
    });
    let mut data = base(hwnd);
    data.uFlags = NIF_MESSAGE;
    data.uCallbackMessage = WM_APP_TRAY;
    with_state(&mut data);
    Shell_NotifyIconW(NIM_ADD, &data);
    data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
    Shell_NotifyIconW(NIM_SETVERSION, &data);
}

/// Updates the tooltip and switches between the normal and the gray icon.
pub unsafe fn update(hwnd: HWND, tip: &str, active: bool) {
    let changed = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let Some(state) = s.as_mut() else { return false };
        let changed = state.tip != tip || state.active != active;
        state.tip = tip.to_string();
        state.active = active;
        changed
    });
    if changed {
        let mut data = base(hwnd);
        with_state(&mut data);
        Shell_NotifyIconW(NIM_MODIFY, &data);
    }
}

/// Shows a notification ("balloon") from the icon, with the app icon.
pub unsafe fn notify(hwnd: HWND, title: &str, text: &str) {
    let mut data = base(hwnd);
    data.uFlags = NIF_INFO;
    copy_into(&mut data.szInfoTitle, title);
    copy_into(&mut data.szInfo, text);
    data.dwInfoFlags = NIIF_USER | NIIF_LARGE_ICON;
    data.hBalloonIcon = app_icon(SM_CXICON);
    Shell_NotifyIconW(NIM_MODIFY, &data);
}

pub unsafe fn remove(hwnd: HWND) {
    let data = base(hwnd);
    Shell_NotifyIconW(NIM_DELETE, &data);
}
