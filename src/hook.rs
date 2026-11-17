//! Hook global de teclado de baixo nível (WH_KEYBOARD_LL).
//!
//! Para cada evento de teclado do sistema, o Windows chama `low_level_keyboard_proc`
//! antes de entregá-lo ao aplicativo em foco. Quando o evento é da tecla remapeada,
//! retornamos 1, o que descarta o evento (a função original não acontece), e disparamos
//! a ação de mídia via SendInput.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT,
    LLKHF_EXTENDED, LLKHF_INJECTED, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN,
    WM_SYSKEYUP,
};

use crate::actions::{self, MediaAction};
use crate::keys::SourceKey;

/// Mapeamento ativo: tecla de origem -> ação de mídia.
pub struct Mapping {
    pub key: &'static SourceKey,
    pub action: &'static MediaAction,
}

static MAPPING: OnceLock<Mapping> = OnceLock::new();
/// Canal de log: o callback não pode bloquear (escrever no console pode travar,
/// ex.: modo de seleção do QuickEdit), então quem imprime é outra thread.
static LOG: OnceLock<Sender<String>> = OnceLock::new();
/// A tecla remapeada está pressionada (evita repetir ações que não devem repetir).
static PRESSED: AtomicBool = AtomicBool::new(false);

pub struct Hook(HHOOK);

impl Drop for Hook {
    fn drop(&mut self) {
        unsafe {
            UnhookWindowsHookEx(self.0);
        }
    }
}

/// Instala o hook na thread atual. A thread precisa rodar um loop de mensagens.
pub fn install(mapping: Mapping, log: Sender<String>) -> Result<Hook, String> {
    MAPPING.set(mapping).map_err(|_| "hook já instalado".to_string())?;
    let _ = LOG.set(log);
    let hook = unsafe {
        SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(low_level_keyboard_proc),
            GetModuleHandleW(std::ptr::null()),
            0,
        )
    };
    if hook.is_null() {
        return Err(format!(
            "SetWindowsHookExW falhou (erro {})",
            std::io::Error::last_os_error()
        ));
    }
    Ok(Hook(hook))
}

unsafe extern "system" fn low_level_keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        if let Some(mapping) = MAPPING.get() {
            let kb = &*(lparam as *const KBDLLHOOKSTRUCT);
            let injected = kb.flags & LLKHF_INJECTED != 0;
            let extended = kb.flags & LLKHF_EXTENDED != 0;
            let matches = kb.vkCode == mapping.key.vk as u32
                && !injected
                && (!mapping.key.require_extended || extended);

            if matches {
                match wparam as u32 {
                    WM_KEYDOWN | WM_SYSKEYDOWN => {
                        let was_pressed = PRESSED.swap(true, Ordering::Relaxed);
                        if !was_pressed || mapping.action.repeat {
                            actions::send(mapping.action.vk);
                            if !was_pressed {
                                if let Some(log) = LOG.get() {
                                    let _ = log.send(format!(
                                        "[{}] -> {}",
                                        mapping.key.name, mapping.action.name
                                    ));
                                }
                            }
                        }
                    }
                    WM_KEYUP | WM_SYSKEYUP => PRESSED.store(false, Ordering::Relaxed),
                    _ => {}
                }
                // Descarta o evento original: a tecla não executa sua função normal.
                return 1;
            }
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}
