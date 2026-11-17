// Aplicativo gráfico: sem janela de console.
#![windows_subsystem = "windows"]

mod actions;
mod gui;
mod hook;
mod keys;

fn main() {
    if let Err(e) = gui::run() {
        // Sem console, então o erro vai para uma caixa de mensagem.
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
