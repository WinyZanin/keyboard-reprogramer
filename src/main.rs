// GUI application: no console window.
#![windows_subsystem = "windows"]

mod actions;
mod gui;
mod hook;
mod i18n;
mod keys;

fn main() {
    i18n::set(i18n::detect());
    if let Err(e) = gui::run() {
        // There is no console, so the error goes to a message box.
        let text: Vec<u16> = e.encode_utf16().chain(Some(0)).collect();
        let title: Vec<u16> = "Keyboard Reprogramer".encode_utf16().chain(Some(0)).collect();
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
                std::ptr::null_mut(),
                text.as_ptr(),
                title.as_ptr(),
                windows_sys::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
            );
        }
        std::process::exit(1);
    }
}
