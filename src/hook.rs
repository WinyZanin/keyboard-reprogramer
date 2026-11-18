//! Global low-level keyboard hook (WH_KEYBOARD_LL).
//!
//! For every keyboard event in the system, Windows calls `low_level_keyboard_proc`
//! before delivering it to the focused application. It does two things:
//!
//! - **Remapping**: if the event belongs to a key with an active rule, it returns 1 (the
//!   event is dropped and the original function doesn't happen) and fires the media action
//!   via SendInput.
//! - **Pause**: while the GUI captures a key (through regular window messages), the hook
//!   passes everything through, so even an already remapped key reaches the window.
//!
//! Keys are identified by scan code + "extended" flag (the physical key position), which
//! tells, for example, Insert apart from Num 0 regardless of NumLock.
//! The hook stays installed while the app runs; with no rules it just passes events on.

use std::ffi::c_void;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::Mutex;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, PostMessageW, SetWindowsHookExW, UnhookWindowsHookEx, HC_ACTION,
    KBDLLHOOKSTRUCT, LLKHF_EXTENDED, LLKHF_INJECTED, WH_KEYBOARD_LL, WM_APP, WM_KEYDOWN, WM_KEYUP,
    WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use crate::actions::{self, ACTIONS};

/// A rule fired. `wparam` = `Rule::uid`.
pub const WM_APP_TRIGGERED: u32 = WM_APP + 1;

/// Physical key.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Key {
    pub vk: u16,
    pub scan: u32,
    pub extended: bool,
}

impl Key {
    pub fn same_physical(&self, other: &Key) -> bool {
        if self.scan != 0 && other.scan != 0 {
            self.scan == other.scan && self.extended == other.extended
        } else {
            self.vk == other.vk
        }
    }
}

/// Active remapping rule.
#[derive(Clone, Copy)]
pub struct Rule {
    pub uid: u32,
    pub key: Key,
    pub action: usize,
}

struct RuleState {
    rule: Rule,
    /// The key is held down (avoids repeating actions that must not repeat).
    pressed: bool,
}

static HOOK: AtomicPtr<c_void> = AtomicPtr::new(null_mut());
static NOTIFY_HWND: AtomicPtr<c_void> = AtomicPtr::new(null_mut());
static RULES: Mutex<Vec<RuleState>> = Mutex::new(Vec::new());
/// Paused: passes every key through (used during capture).
static PAUSED: AtomicBool = AtomicBool::new(false);

/// Installs the hook on the current thread (which must run a message loop).
/// `notify` receives `WM_APP_TRIGGERED`.
pub fn install(notify: HWND) -> Result<(), String> {
    NOTIFY_HWND.store(notify, Ordering::Relaxed);
    if !HOOK.load(Ordering::Relaxed).is_null() {
        return Ok(());
    }
    let hook = unsafe {
        SetWindowsHookExW(WH_KEYBOARD_LL, Some(low_level_keyboard_proc), GetModuleHandleW(std::ptr::null()), 0)
    };
    if hook.is_null() {
        return Err(format!("SetWindowsHookExW failed: {}", std::io::Error::last_os_error()));
    }
    HOOK.store(hook, Ordering::Relaxed);
    Ok(())
}

/// Removes the hook; every key gets its original function back.
pub fn uninstall() {
    let hook = HOOK.swap(null_mut(), Ordering::Relaxed);
    if !hook.is_null() {
        unsafe {
            UnhookWindowsHookEx(hook);
        }
    }
}

/// Replaces the active rules (empty list = nothing is remapped).
pub fn set_rules(rules: &[Rule]) {
    let mut state = RULES.lock().unwrap_or_else(|e| e.into_inner());
    *state = rules
        .iter()
        .filter(|r| r.action < ACTIONS.len())
        .map(|&rule| RuleState { rule, pressed: false })
        .collect();
}

/// Pauses/resumes remapping (without changing the rules).
pub fn set_paused(on: bool) {
    PAUSED.store(on, Ordering::Relaxed);
}

unsafe extern "system" fn low_level_keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kb = &*(lparam as *const KBDLLHOOKSTRUCT);
        // Ignores only what the app itself injected (avoids recursion with the media keys).
        let own = kb.flags & LLKHF_INJECTED != 0 && kb.dwExtraInfo == actions::SELF_INJECTED_TAG;
        if !own {
            if let Some(result) = handle(kb, wparam as u32) {
                return result;
            }
        }
    }
    CallNextHookEx(null_mut(), code, wparam, lparam)
}

/// Returns `Some(1)` to drop the event, `None` to pass it on.
unsafe fn handle(kb: &KBDLLHOOKSTRUCT, msg: u32) -> Option<LRESULT> {
    let key = Key { vk: kb.vkCode as u16, scan: kb.scanCode, extended: kb.flags & LLKHF_EXTENDED != 0 };
    let down = matches!(msg, WM_KEYDOWN | WM_SYSKEYDOWN);
    let up = matches!(msg, WM_KEYUP | WM_SYSKEYUP);
    let notify = NOTIFY_HWND.load(Ordering::Relaxed);
    if PAUSED.load(Ordering::Relaxed) {
        return None;
    }

    // try_lock: the callback must never wait. If the GUI is swapping the rules right
    // now, the event goes through unchanged.
    let mut rules = RULES.try_lock().ok()?;
    let state = rules.iter_mut().find(|s| s.rule.key.same_physical(&key))?;
    let action = &ACTIONS[state.rule.action];
    if down {
        let was_pressed = std::mem::replace(&mut state.pressed, true);
        if !was_pressed || action.repeat {
            actions::send(action.vk);
        }
        if !was_pressed {
            // PostMessage doesn't block: the callback must return quickly.
            PostMessageW(notify, WM_APP_TRIGGERED, state.rule.uid as WPARAM, 0);
        }
    } else if up {
        state.pressed = false;
    }
    // Drops the original event: the key doesn't perform its normal function.
    Some(1)
}
