// GUI application: no console window.
#![windows_subsystem = "windows"]

mod actions;
mod gui;
mod hook;
mod i18n;
mod keys;

use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    FindWindowW, IsIconic, MessageBoxW, SetForegroundWindow, ShowWindow, MB_ICONERROR, SW_RESTORE,
};

/// Named mutex held while the app runs. The installers (Inno Setup `AppMutex`) use it to
/// detect a running instance, and a second launch uses it to bring the first one forward.
/// Keep in sync with `installer/inno/keyboard-reprogramer.iss`.
const INSTANCE_MUTEX: &str = "KeyboardReprogramerMutex";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Returns false if another instance is already running (after bringing it to the front).
fn claim_single_instance() -> bool {
    unsafe {
        // The handle is intentionally never closed: Windows releases it when the process exits.
        let mutex = CreateMutexW(std::ptr::null(), 0, wide(INSTANCE_MUTEX).as_ptr());
        if mutex.is_null() || GetLastError() != ERROR_ALREADY_EXISTS {
            return true;
        }
        let window = FindWindowW(wide(gui::MAIN_CLASS).as_ptr(), std::ptr::null());
        if !window.is_null() {
            if IsIconic(window) != 0 {
                ShowWindow(window, SW_RESTORE);
            }
            SetForegroundWindow(window);
        }
        false
    }
}

fn main() {
    if !claim_single_instance() {
        return;
    }
    i18n::set(i18n::detect());
    if let Err(e) = gui::run() {
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
