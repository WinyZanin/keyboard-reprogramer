//! Main window in pure Win32 (native BUTTON, COMBOBOX, STATIC and LISTBOX controls),
//! no frameworks.
//!
//! Layout:
//! - Menu bar: Language, Help (About).
//! - Top: master button (disables/re-enables everything) and status.
//! - Scrollable panel with one row per key: [key] [action ▾] [Disable] [Delete],
//!   followed by the "+ Add" button, which creates a new row and starts capturing its key.
//! - Trigger history.
//!
//! All user-visible text comes from `i18n::t()`; `retranslate` re-applies it when the
//! language changes.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicI32, AtomicPtr, Ordering};

use windows_sys::Win32::Foundation::{COLORREF, GetLastError, HWND, INVALID_HANDLE_VALUE, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CreateFontIndirectW, GetSysColorBrush, InvalidateRect, SetBkMode, SetTextColor, COLOR_BTNFACE,
    COLOR_WINDOW, HBRUSH, HDC, TRANSPARENT,
};
use windows_sys::Win32::System::ApplicationInstallationAndServicing::{ActivateActCtx, CreateActCtxW, ACTCTXW};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::SystemInformation::{GetLocalTime, GetSystemDirectoryW};
use windows_sys::Win32::System::Diagnostics::Debug::MessageBeep;
use windows_sys::Win32::UI::Controls::{InitCommonControlsEx, SetScrollInfo, ICC_STANDARD_CLASSES, INITCOMMONCONTROLSEX};
use windows_sys::Win32::UI::HiDpi::{GetDpiForSystem, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_SYSTEM_AWARE};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, MapVirtualKeyW, SetFocus, MAPVK_VSC_TO_VK_EX, VK_CONTROL, VK_ESCAPE, VK_MENU, VK_SHIFT, VK_SNAPSHOT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::actions::ACTIONS;
use crate::hook::{self, Key, Rule, WM_APP_TRIGGERED};
use crate::i18n::{self, t, Lang};
use crate::keys;

/// Main window class; also used by `main` to find the running instance.
pub const MAIN_CLASS: &str = "KeyboardReprogramerWindow";

const ID_MASTER: i32 = 10;
const ID_STATUS: i32 = 11;
const ID_PANEL: i32 = 12;
const ID_LOG: i32 = 13;
const ID_HISTORY_LABEL: i32 = 14;
const ID_ADD: i32 = 20;
/// Help > About menu item.
const ID_ABOUT: i32 = 90;
/// Language menu items: ID = ID_LANG_BASE + index in `Lang::ALL`.
const ID_LANG_BASE: i32 = 100;
/// Row controls: ID = ROW_ID_BASE + uid * 4 + part.
const ROW_ID_BASE: i32 = 1000;
const PART_HOTKEY: usize = 0;
const PART_ACTION: usize = 1;
const PART_TOGGLE: usize = 2;
const PART_DELETE: usize = 3;

const ROW_H: i32 = 36;
const PAD: i32 = 6;
const MAX_LOG_LINES: isize = 200;
const ACTCTX_FLAG_RESOURCE_NAME_VALID: u32 = 0x8;

static DPI: AtomicI32 = AtomicI32::new(96);
static FONT: AtomicPtr<c_void> = AtomicPtr::new(null_mut());

fn s(v: i32) -> i32 {
    v * DPI.load(Ordering::Relaxed) / 96
}

struct Row {
    uid: u32,
    key: Option<Key>,
    action: usize,
    enabled: bool,
    /// Controls: key, action, enable/disable, delete.
    ctl: [HWND; 4],
}

struct App {
    main: HWND,
    panel: HWND,
    add: HWND,
    rows: Vec<Row>,
    /// Master switch: off = no key is remapped, without losing the rows' state.
    master: bool,
    /// Row whose key is being captured.
    capturing: Option<u32>,
    next_uid: u32,
    scroll: i32,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    /// Just-captured key: its repeats and key-up must not reach the controls.
    static SWALLOW: Cell<Option<(u32, bool)>> = const { Cell::new(None) };
}

/// State access. The closure must only read/change data: Win32 calls that may re-enter
/// the window procs stay outside of it. Returns None if the state is already borrowed
/// (re-entrancy).
fn with<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f)))
}

/// Converts to null-terminated UTF-16 (the format of the Windows "W" APIs).
fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Enables visual styles (Common Controls v6) without embedding a manifest in the exe:
/// uses the manifest that shell32.dll itself ships as resource 124.
fn enable_visual_styles() {
    unsafe {
        let mut dir = [0u16; 260];
        let len = GetSystemDirectoryW(dir.as_mut_ptr(), dir.len() as u32) as usize;
        if len > 0 && len < dir.len() {
            let path = w(&(String::from_utf16_lossy(&dir[..len]) + "\\shell32.dll"));
            let mut ctx: ACTCTXW = zeroed();
            ctx.cbSize = size_of::<ACTCTXW>() as u32;
            ctx.dwFlags = ACTCTX_FLAG_RESOURCE_NAME_VALID;
            ctx.lpSource = path.as_ptr();
            ctx.lpResourceName = std::ptr::without_provenance(124);
            let handle = CreateActCtxW(&ctx);
            if handle != INVALID_HANDLE_VALUE {
                let mut cookie = 0usize;
                ActivateActCtx(handle, &mut cookie);
            }
        }
        let icc = INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_STANDARD_CLASSES,
        };
        InitCommonControlsEx(&icc);
    }
}

/// Creates a child control with the system font (coordinates in pixels at 96 DPI).
unsafe fn child(parent: HWND, class: &str, text: &str, style: u32, id: i32, (x, y, cx, cy): (i32, i32, i32, i32)) -> HWND {
    child_ex(parent, 0, class, text, style, id, (x, y, cx, cy))
}

unsafe fn child_ex(
    parent: HWND,
    ex_style: u32,
    class: &str,
    text: &str,
    style: u32,
    id: i32,
    (x, y, cx, cy): (i32, i32, i32, i32),
) -> HWND {
    let hwnd = CreateWindowExW(
        ex_style,
        w(class).as_ptr(),
        w(text).as_ptr(),
        WS_CHILD | WS_VISIBLE | style,
        s(x),
        s(y),
        s(cx),
        s(cy),
        parent,
        id as isize as HMENU,
        GetModuleHandleW(null()),
        null(),
    );
    SendMessageW(hwnd, WM_SETFONT, FONT.load(Ordering::Relaxed) as WPARAM, 1);
    hwnd
}

unsafe fn set_text(hwnd: HWND, text: &str) {
    SetWindowTextW(hwnd, w(text).as_ptr());
}

unsafe fn log(msg: &str) {
    let Some(main) = with(|a| a.main) else { return };
    let list = GetDlgItem(main, ID_LOG);
    let mut t = zeroed();
    GetLocalTime(&mut t);
    let line = format!("{:02}:{:02}:{:02}   {}", t.wHour, t.wMinute, t.wSecond, msg);
    SendMessageW(list, LB_ADDSTRING, 0, w(&line).as_ptr() as LPARAM);
    let count = SendMessageW(list, LB_GETCOUNT, 0, 0);
    if count > MAX_LOG_LINES {
        SendMessageW(list, LB_DELETESTRING, 0, 0);
    }
    SendMessageW(list, LB_SETTOPINDEX, (count - 1).max(0) as WPARAM, 0);
}

fn key_label(key: Option<Key>) -> String {
    key.map(|k| keys::name(&k)).unwrap_or_else(|| t().no_key.into())
}

// ---------------------------------------------------------------------------------------
// State -> hook and status

/// Sends the active rules to the hook and updates the status and the master button.
unsafe fn sync() {
    let Some((rules, master, main)) = with(|a| {
        let rules: Vec<Rule> = a
            .rows
            .iter()
            .filter(|r| a.master && r.enabled)
            .filter_map(|r| r.key.map(|key| Rule { uid: r.uid, key, action: r.action }))
            .collect();
        (rules, a.master, a.main)
    }) else {
        return;
    };
    hook::set_rules(&rules);

    let s = t();
    let status = if !master {
        s.status_master_off.to_string()
    } else if rules.is_empty() {
        s.status_none.to_string()
    } else {
        (s.status_active)(rules.len())
    };
    let status_ctl = GetDlgItem(main, ID_STATUS);
    set_text(status_ctl, &status);
    InvalidateRect(status_ctl, null(), 1);
    set_text(GetDlgItem(main, ID_MASTER), if master { s.master_disable } else { s.master_enable });
}

/// Updates the texts and states of a row's controls.
unsafe fn update_row(uid: u32) {
    let Some(Some((ctl, key, action, enabled, capturing))) = with(|a| {
        let capturing = a.capturing == Some(uid);
        a.rows.iter().find(|r| r.uid == uid).map(|r| (r.ctl, r.key, r.action, r.enabled, capturing))
    }) else {
        return;
    };
    let s = t();
    let hotkey = if capturing {
        s.capture_prompt.to_string()
    } else {
        key.map(|k| keys::name(&k)).unwrap_or_else(|| s.empty_key.into())
    };
    set_text(ctl[PART_HOTKEY], &hotkey);
    // Key button stays "pressed" while capturing.
    SendMessageW(ctl[PART_HOTKEY], BM_SETSTATE, capturing as WPARAM, 0);
    SendMessageW(ctl[PART_ACTION], CB_SETCURSEL, action, 0);
    EnableWindow(ctl[PART_ACTION], enabled as i32);
    set_text(ctl[PART_TOGGLE], if enabled { s.toggle_disable } else { s.toggle_enable });
    set_text(ctl[PART_DELETE], s.delete);
}

/// Fills an action combo box with the action names in the current language.
unsafe fn fill_actions(combo: HWND) {
    SendMessageW(combo, CB_RESETCONTENT, 0, 0);
    for name in t().action_names {
        SendMessageW(combo, CB_ADDSTRING, 0, w(name).as_ptr() as LPARAM);
    }
}

// ---------------------------------------------------------------------------------------
// Scrollable panel

/// Positions the rows and the Add button according to the scroll and updates the scroll bar.
unsafe fn relayout() {
    let Some((panel, add, ctls, mut scroll)) = with(|a| (a.panel, a.add, a.rows.iter().map(|r| r.ctl).collect::<Vec<_>>(), a.scroll)) else {
        return;
    };
    let mut rc: RECT = zeroed();
    GetClientRect(panel, &mut rc);
    let page = rc.bottom - rc.top;
    let content = s(PAD + (ctls.len() as i32 + 1) * ROW_H + PAD);
    scroll = scroll.clamp(0, (content - page).max(0));
    with(|a| a.scroll = scroll);

    let row_y = |i: usize| s(PAD + i as i32 * ROW_H) - scroll;
    let flags = SWP_NOZORDER | SWP_NOACTIVATE;
    for (i, ctl) in ctls.iter().enumerate() {
        let y = row_y(i);
        SetWindowPos(ctl[PART_HOTKEY], null_mut(), s(6), y, s(178), s(28), flags);
        SetWindowPos(ctl[PART_ACTION], null_mut(), s(190), y + s(3), s(136), s(300), flags);
        SetWindowPos(ctl[PART_TOGGLE], null_mut(), s(332), y, s(76), s(28), flags);
        SetWindowPos(ctl[PART_DELETE], null_mut(), s(414), y, s(56), s(28), flags);
    }
    SetWindowPos(add, null_mut(), s(6), row_y(ctls.len()), s(130), s(28), flags);

    let si = SCROLLINFO {
        cbSize: size_of::<SCROLLINFO>() as u32,
        fMask: SIF_RANGE | SIF_PAGE | SIF_POS | SIF_DISABLENOSCROLL,
        nMin: 0,
        nMax: content - 1,
        nPage: page as u32,
        nPos: scroll,
        nTrackPos: 0,
    };
    SetScrollInfo(panel, SB_VERT, &si, 1);
    InvalidateRect(panel, null(), 1);
}

unsafe fn scroll_to(pos: i32) {
    with(|a| a.scroll = pos);
    relayout();
}

unsafe fn on_vscroll(panel: HWND, request: u32) {
    let mut si: SCROLLINFO = zeroed();
    si.cbSize = size_of::<SCROLLINFO>() as u32;
    si.fMask = SIF_ALL;
    GetScrollInfo(panel, SB_VERT, &mut si);
    let pos = match request as i32 {
        SB_LINEUP => si.nPos - s(ROW_H),
        SB_LINEDOWN => si.nPos + s(ROW_H),
        SB_PAGEUP => si.nPos - si.nPage as i32,
        SB_PAGEDOWN => si.nPos + si.nPage as i32,
        SB_THUMBTRACK | SB_THUMBPOSITION => si.nTrackPos,
        SB_TOP => 0,
        SB_BOTTOM => i32::MAX,
        _ => return,
    };
    scroll_to(pos);
}

// ---------------------------------------------------------------------------------------
// User actions

unsafe fn add_row() {
    let Some((panel, uid)) = with(|a| {
        a.next_uid += 1;
        (a.panel, a.next_uid)
    }) else {
        return;
    };
    let id = |part: usize| ROW_ID_BASE + uid as i32 * 4 + part as i32;
    let r = (0, 0, 10, 10); // position is set in relayout
    let s = t();
    let hotkey = child(panel, "BUTTON", s.empty_key, WS_TABSTOP, id(PART_HOTKEY), r);
    let combo = child(panel, "COMBOBOX", "", WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWNLIST as u32, id(PART_ACTION), r);
    fill_actions(combo);
    let toggle = child(panel, "BUTTON", s.toggle_disable, WS_TABSTOP, id(PART_TOGGLE), r);
    let delete = child(panel, "BUTTON", s.delete, WS_TABSTOP, id(PART_DELETE), r);

    with(|a| {
        a.rows.push(Row { uid, key: None, action: 0, enabled: true, ctl: [hotkey, combo, toggle, delete] });
    });
    scroll_to(i32::MAX); // shows the new row
    update_row(uid);
    SetFocus(hotkey);
    start_capture(uid);
}

unsafe fn start_capture(uid: u32) {
    let previous = with(|a| a.capturing.replace(uid)).flatten();
    // While capturing, the hook passes everything on so the window gets even remapped keys.
    hook::set_paused(true);
    if let Some(prev) = previous.filter(|&p| p != uid) {
        update_row(prev);
    }
    update_row(uid);
    if let Some(Some(i)) = with(|a| a.rows.iter().position(|r| r.uid == uid)) {
        log(&(t().log_capture)(i + 1));
    }
}

unsafe fn cancel_capture() {
    if let Some(uid) = with(|a| a.capturing.take()).flatten() {
        hook::set_paused(false);
        update_row(uid);
    }
}

/// Capture through the message loop (like the Windows Hotkey control): while a row is
/// capturing, the next key that reaches the window (WM_KEYDOWN/WM_SYSKEYDOWN) becomes the
/// row's key. lParam carries the scan code and the extended flag, the same data the hook
/// compares. Returns true if the message was consumed.
unsafe fn capture_filter(msg: &MSG) -> bool {
    if !matches!(msg.message, WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP) {
        return false;
    }
    let down = matches!(msg.message, WM_KEYDOWN | WM_SYSKEYDOWN);
    let scan = ((msg.lParam >> 16) & 0xFF) as u32;
    let extended = (msg.lParam >> 24) & 1 != 0;

    if SWALLOW.get() == Some((scan, extended)) {
        if !down {
            SWALLOW.set(None);
        }
        return true;
    }
    if with(|a| a.capturing.is_some()) != Some(true) {
        return false;
    }
    let mut vk = msg.wParam as u16;
    // Print Screen sends no WM_KEYDOWN, only WM_KEYUP; other key-ups are ignored.
    if !down && vk != VK_SNAPSHOT {
        return true;
    }
    // Shift/Ctrl/Alt arrive generic; the scan code tells left from right.
    if matches!(vk, VK_SHIFT | VK_CONTROL | VK_MENU) {
        let ext_scan = scan | if extended { 0xE000 } else { 0 };
        vk = MapVirtualKeyW(ext_scan, MAPVK_VSC_TO_VK_EX) as u16;
    }
    if down {
        SWALLOW.set(Some((scan, extended)));
    }
    on_captured(Key { vk, scan, extended });
    true
}

unsafe fn on_captured(key: Key) {
    let Some(uid) = with(|a| a.capturing.take()).flatten() else { return };
    hook::set_paused(false);
    if key.vk == VK_ESCAPE {
        update_row(uid);
        return;
    }
    let name = keys::name(&key);
    let duplicate = with(|a| {
        a.rows.iter().position(|r| r.uid != uid && r.key.is_some_and(|k| k.same_physical(&key)))
    })
    .flatten();
    if let Some(i) = duplicate {
        MessageBeep(MB_ICONWARNING);
        log(&(t().log_key_in_use)(&name, i + 1));
        update_row(uid);
        return;
    }
    let info = with(|a| {
        let i = a.rows.iter().position(|r| r.uid == uid)?;
        a.rows[i].key = Some(key);
        Some((i, a.rows[i].action))
    })
    .flatten();
    update_row(uid);
    sync();
    if let Some((i, action)) = info {
        log(&(t().log_mapped)(i + 1, &name, t().action_names[action]));
    }
}

unsafe fn toggle_row(uid: u32) {
    let info = with(|a| {
        let i = a.rows.iter().position(|r| r.uid == uid)?;
        let row = &mut a.rows[i];
        row.enabled = !row.enabled;
        Some((i, row.enabled, row.key))
    })
    .flatten();
    update_row(uid);
    sync();
    if let Some((i, enabled, key)) = info {
        let msg = if enabled { t().log_row_enabled } else { t().log_row_disabled };
        log(&msg(i + 1, &key_label(key)));
    }
}

unsafe fn delete_row(uid: u32) {
    if with(|a| a.capturing == Some(uid)) == Some(true) {
        cancel_capture();
    }
    let Some(Some((i, row_ctl, key, add))) = with(|a| {
        let i = a.rows.iter().position(|r| r.uid == uid)?;
        let row = a.rows.remove(i);
        Some((i, row.ctl, row.key, a.add))
    }) else {
        return;
    };
    SetFocus(add);
    for ctl in row_ctl {
        DestroyWindow(ctl);
    }
    relayout();
    sync();
    log(&(t().log_row_deleted)(i + 1, &key_label(key)));
}

unsafe fn action_changed(uid: u32, combo: HWND) {
    let sel = SendMessageW(combo, CB_GETCURSEL, 0, 0);
    let Some(action) = usize::try_from(sel).ok().filter(|&a| a < ACTIONS.len()) else { return };
    let info = with(|a| {
        let i = a.rows.iter().position(|r| r.uid == uid)?;
        a.rows[i].action = action;
        Some((i, a.rows[i].key))
    })
    .flatten();
    sync();
    if let Some((i, Some(key))) = info {
        log(&(t().log_mapped)(i + 1, &keys::name(&key), t().action_names[action]));
    }
}

unsafe fn toggle_master() {
    cancel_capture();
    let master = with(|a| {
        a.master = !a.master;
        a.master
    });
    sync();
    match master {
        Some(true) => log(t().log_master_on),
        Some(false) => log(t().log_master_off),
        None => {}
    }
}

// ---------------------------------------------------------------------------------------
// Language

/// Builds the menu bar in the current language, with the current language checked.
unsafe fn build_menu() -> HMENU {
    let bar = CreateMenu();
    let langs = CreatePopupMenu();
    for (i, lang) in Lang::ALL.iter().enumerate() {
        AppendMenuW(langs, MF_STRING, (ID_LANG_BASE + i as i32) as usize, w(lang.native_name()).as_ptr());
    }
    let current = Lang::ALL.iter().position(|&l| l == i18n::current()).unwrap_or(0) as u32;
    let first = ID_LANG_BASE as u32;
    let last = first + Lang::ALL.len() as u32 - 1;
    CheckMenuRadioItem(langs, first, last, first + current, MF_BYCOMMAND);
    AppendMenuW(bar, MF_POPUP, langs as usize, w(t().menu_language).as_ptr());
    let help = CreatePopupMenu();
    AppendMenuW(help, MF_STRING, ID_ABOUT as usize, w(t().menu_about).as_ptr());
    AppendMenuW(bar, MF_POPUP, help as usize, w(t().menu_help).as_ptr());
    bar
}

/// Re-applies every visible text in the current language.
unsafe fn retranslate() {
    let Some((main, add, rows)) = with(|a| {
        (a.main, a.add, a.rows.iter().map(|r| (r.uid, r.ctl[PART_ACTION])).collect::<Vec<_>>())
    }) else {
        return;
    };
    let old_menu = GetMenu(main);
    SetMenu(main, build_menu());
    if !old_menu.is_null() {
        DestroyMenu(old_menu);
    }
    DrawMenuBar(main);

    set_text(add, t().add);
    set_text(GetDlgItem(main, ID_HISTORY_LABEL), t().history);
    for (uid, combo) in rows {
        fill_actions(combo);
        update_row(uid); // also restores the combo selection
    }
    sync();
}

/// Shows the app name and version (taken from Cargo.toml at build time).
unsafe fn show_about(owner: HWND) {
    let text = (t().about_text)(env!("CARGO_PKG_VERSION"));
    MessageBoxW(owner, w(&text).as_ptr(), w(t().about_title).as_ptr(), MB_OK | MB_ICONINFORMATION);
}

unsafe fn set_language(lang: Lang) {
    if lang == i18n::current() {
        return;
    }
    i18n::set(lang);
    retranslate();
    log(&(t().log_language)(lang.native_name()));
}

// ---------------------------------------------------------------------------------------
// Window procs

unsafe extern "system" fn panel_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            let id = (wparam & 0xffff) as i32;
            let notify = ((wparam >> 16) & 0xffff) as u32;
            if id == ID_ADD && notify == BN_CLICKED {
                add_row();
            } else if id >= ROW_ID_BASE {
                let uid = ((id - ROW_ID_BASE) / 4) as u32;
                match (((id - ROW_ID_BASE) % 4) as usize, notify) {
                    // Clicking again while capturing doesn't cancel (only Esc does).
                    (PART_HOTKEY, BN_CLICKED) => {
                        if with(|a| a.capturing == Some(uid)) != Some(true) {
                            start_capture(uid);
                        }
                    }
                    (PART_ACTION, CBN_SELCHANGE) => action_changed(uid, lparam as HWND),
                    (PART_TOGGLE, BN_CLICKED) => toggle_row(uid),
                    (PART_DELETE, BN_CLICKED) => delete_row(uid),
                    _ => {}
                }
            }
            0
        }
        WM_VSCROLL => {
            on_vscroll(hwnd, (wparam & 0xffff) as u32);
            0
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam >> 16) & 0xffff) as u16 as i16 as i32;
            if let Some(scroll) = with(|a| a.scroll) {
                scroll_to(scroll - delta * s(ROW_H) / 120);
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            let id = (wparam & 0xffff) as i32;
            let notify = ((wparam >> 16) & 0xffff) as u32;
            if id == ID_MASTER && notify == BN_CLICKED {
                toggle_master();
            } else if lparam == 0 && id == ID_ABOUT {
                show_about(hwnd);
            } else if lparam == 0 && id >= ID_LANG_BASE {
                // Menu item (lParam is 0 for menu commands).
                if let Some(&lang) = Lang::ALL.get((id - ID_LANG_BASE) as usize) {
                    set_language(lang);
                }
            }
            0
        }
        WM_APP_TRIGGERED => {
            let info = with(|a| {
                a.rows.iter().position(|r| r.uid == wparam as u32).map(|i| (i, a.rows[i].key, a.rows[i].action))
            })
            .flatten();
            if let Some((i, key, action)) = info {
                log(&(t().log_triggered)(i + 1, &key_label(key), t().action_names[action]));
            }
            0
        }
        WM_ACTIVATEAPP => {
            // Left the app: don't capture keys typed in other windows.
            if wparam == 0 {
                cancel_capture();
            }
            0
        }
        WM_MOUSEWHEEL => {
            // Scrolling over the main window scrolls the rows panel.
            if let Some(panel) = with(|a| a.panel) {
                return SendMessageW(panel, msg, wparam, lparam);
            }
            0
        }
        WM_CTLCOLORSTATIC => {
            let hdc = wparam as HDC;
            SetBkMode(hdc, TRANSPARENT as i32);
            if GetDlgCtrlID(lparam as HWND) == ID_STATUS {
                let active = !no_active_rules();
                let color: COLORREF = if active { 0x00_30_80_10 } else { 0x00_60_60_60 };
                SetTextColor(hdc, color);
            }
            GetSysColorBrush(COLOR_BTNFACE) as LRESULT
        }
        WM_DESTROY => {
            hook::uninstall();
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// True if no rule is effectively active (for the status color).
fn no_active_rules() -> bool {
    with(|a| !a.master || !a.rows.iter().any(|r| r.enabled && r.key.is_some())).unwrap_or(true)
}

// ---------------------------------------------------------------------------------------

/// App icon (resource 1, embedded by build.rs) at the given system size
/// (SM_CXICON = taskbar/Alt+Tab, SM_CXSMICON = title bar).
/// Without the resource, falls back to the default Windows icon.
unsafe fn app_icon(metric: SYSTEM_METRICS_INDEX) -> HICON {
    let size = GetSystemMetrics(metric);
    let icon = LoadImageW(GetModuleHandleW(null()), std::ptr::without_provenance(1), IMAGE_ICON, size, size, LR_DEFAULTCOLOR);
    if icon.is_null() {
        LoadIconW(null_mut(), IDI_APPLICATION)
    } else {
        icon
    }
}

unsafe fn register_class(name: &[u16], proc_: WNDPROC, background: i32, icon: bool) -> Result<(), String> {
    let wc = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: proc_,
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: GetModuleHandleW(null()),
        hIcon: if icon { app_icon(SM_CXICON) } else { null_mut() },
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        hbrBackground: (background + 1) as usize as HBRUSH,
        lpszMenuName: null(),
        lpszClassName: name.as_ptr(),
        hIconSm: if icon { app_icon(SM_CXSMICON) } else { null_mut() },
    };
    if RegisterClassExW(&wc) == 0 {
        return Err(format!("RegisterClassExW failed ({})", GetLastError()));
    }
    Ok(())
}

pub fn run() -> Result<(), String> {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_SYSTEM_AWARE);
        enable_visual_styles();
        DPI.store(GetDpiForSystem() as i32, Ordering::Relaxed);
        let mut ncm: NONCLIENTMETRICSW = zeroed();
        ncm.cbSize = size_of::<NONCLIENTMETRICSW>() as u32;
        SystemParametersInfoW(SPI_GETNONCLIENTMETRICS, ncm.cbSize, &mut ncm as *mut _ as *mut c_void, 0);
        FONT.store(CreateFontIndirectW(&ncm.lfMessageFont), Ordering::Relaxed);

        let main_class = w(MAIN_CLASS);
        let panel_class = w("KeyboardReprogramerRows");
        register_class(&main_class, Some(wnd_proc), COLOR_BTNFACE, true)?;
        register_class(&panel_class, Some(panel_proc), COLOR_WINDOW, false)?;

        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
        let mut rect = RECT { left: 0, top: 0, right: s(520), bottom: s(414) };
        // bMenu = 1: the client area keeps its size below the menu bar.
        AdjustWindowRectEx(&mut rect, style, 1, 0);
        let main = CreateWindowExW(
            0,
            main_class.as_ptr(),
            w("Keyboard Reprogramer").as_ptr(),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            rect.right - rect.left,
            rect.bottom - rect.top,
            null_mut(),
            build_menu(),
            GetModuleHandleW(null()),
            null(),
        );
        if main.is_null() {
            return Err(format!("CreateWindowExW failed ({})", GetLastError()));
        }

        let s = t();
        child(main, "BUTTON", s.master_disable, WS_TABSTOP, ID_MASTER, (12, 10, 130, 28));
        child(main, "STATIC", "", 0, ID_STATUS, (154, 16, 354, 20));
        // WS_EX_CONTROLPARENT: Tab navigates through the row controls.
        let panel = child_ex(
            main,
            WS_EX_CLIENTEDGE | WS_EX_CONTROLPARENT,
            "KeyboardReprogramerRows",
            "",
            WS_VSCROLL | WS_TABSTOP,
            ID_PANEL,
            (12, 46, 496, 240),
        );
        let add = child(panel, "BUTTON", s.add, WS_TABSTOP, ID_ADD, (6, PAD, 130, 28));
        child(main, "STATIC", s.history, 0, ID_HISTORY_LABEL, (12, 296, 100, 20));
        child(
            main,
            "LISTBOX",
            "",
            WS_BORDER | WS_VSCROLL | LBS_NOINTEGRALHEIGHT as u32 | LBS_NOSEL as u32,
            ID_LOG,
            (12, 316, 496, 86),
        );

        APP.with(|a| {
            *a.borrow_mut() = Some(App {
                main,
                panel,
                add,
                rows: Vec::new(),
                master: true,
                capturing: None,
                next_uid: 0,
                scroll: 0,
            })
        });
        hook::install(main)?;
        relayout();
        sync();
        ShowWindow(main, SW_SHOW);

        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            if capture_filter(&msg) {
                continue;
            }
            // IsDialogMessage provides Tab navigation between the controls.
            if IsDialogMessageW(main, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        Ok(())
    }
}
