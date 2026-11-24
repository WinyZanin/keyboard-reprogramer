// GUI application: no console window.
#![windows_subsystem = "windows"]

mod actions;
mod autostart;
mod config;
mod gui;
mod hook;
mod i18n;
mod keys;
mod tray;

use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, FindWindowW, GetWindowThreadProcessId, MessageBoxW, PostMessageW, MB_ICONERROR,
};

/// Named mutex held while the app runs. The installers (Inno Setup `AppMutex`) use it to
/// detect a running instance, and a second launch uses it to bring the first one forward.
/// Keep in sync with `installer/inno/keyboard-reprogramer.iss`.
const INSTANCE_MUTEX: &str = "KeyboardReprogramerMutex";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Returns false if another instance is already running (after asking it to show its window,
/// which may be hidden in the notification area).
fn claim_single_instance() -> bool {
    unsafe {
        // The handle is intentionally never closed: Windows releases it when the process exits.
        let mutex = CreateMutexW(std::ptr::null(), 0, wide(INSTANCE_MUTEX).as_ptr());
        if mutex.is_null() || GetLastError() != ERROR_ALREADY_EXISTS {
            return true;
        }
        let window = FindWindowW(wide(gui::MAIN_CLASS).as_ptr(), std::ptr::null());
        if !window.is_null() {
            // This process was just started by the user, so it may hand the foreground over.
            let mut pid = 0;
            GetWindowThreadProcessId(window, &mut pid);
            AllowSetForegroundWindow(pid);
            PostMessageW(window, gui::WM_APP_SHOW, 0, 0);
        }
        false
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == autostart::ARG_REMOVE) {
        // Run by the uninstallers; there is no UI to report a failure to.
        let _ = autostart::remove_if_own();
        return;
    }
    let at_logon = args.iter().any(|a| a == autostart::ARG_AUTOSTART);
    if !claim_single_instance() {
        return;
    }
    let loaded = config::load();
    let saved_lang = match &loaded {
        Ok(Some(c)) => c.language.as_deref().and_then(i18n::Lang::from_code),
        _ => None,
    };
    i18n::set(saved_lang.unwrap_or_else(i18n::detect));
    if let Err(e) = gui::run(loaded, at_logon) {
        // There is no console, so the error goes to a message box.
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                wide(&e).as_ptr(),
                wide("Keyboard Reprogramer").as_ptr(),
                MB_ICONERROR,
            );
        }
        std::process::exit(1);
    }
}
