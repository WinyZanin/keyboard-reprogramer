//! Hook global de teclado de baixo nível (WH_KEYBOARD_LL).
//!
//! Para cada evento de teclado do sistema, o Windows chama `low_level_keyboard_proc`
//! antes de entregá-lo ao aplicativo em foco. Quando o evento é de uma tecla remapeada
//! (e a regra está ativa), retornamos 1, o que descarta o evento (a função original não
//! acontece), e disparamos a ação de mídia via SendInput.
//!
//! O hook é instalado na thread da GUI, cujo loop de mensagens o mantém funcionando.
//! Ele só fica instalado enquanto houver pelo menos uma regra ativa.

use std::ffi::c_void;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::Mutex;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, PostMessageW, SetWindowsHookExW, UnhookWindowsHookEx, HC_ACTION,
    KBDLLHOOKSTRUCT, LLKHF_EXTENDED, LLKHF_INJECTED, WH_KEYBOARD_LL, WM_APP, WM_KEYDOWN, WM_KEYUP,
    WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use crate::actions::{self, ACTIONS};
use crate::keys::KEYS;

/// Postada para a janela de notificação a cada acionamento.
/// `wparam` = índice da tecla em `KEYS`, `lparam` = índice da ação em `ACTIONS`.
pub const WM_APP_TRIGGERED: u32 = WM_APP + 1;

/// Uma regra de remapeamento: `KEYS[key] -> ACTIONS[action]`.
#[derive(Clone, Copy)]
pub struct Rule {
    pub key: usize,
    pub action: usize,
    pub enabled: bool,
}

struct RuleState {
    rule: Rule,
    /// A tecla está pressionada (evita repetir ações que não devem repetir).
    pressed: bool,
}

static HOOK: AtomicPtr<c_void> = AtomicPtr::new(null_mut());
static NOTIFY_HWND: AtomicPtr<c_void> = AtomicPtr::new(null_mut());
static RULES: Mutex<Vec<RuleState>> = Mutex::new(Vec::new());

/// Substitui o conjunto de regras. Instala o hook se houver alguma regra ativa
/// e o remove caso contrário. `notify` recebe `WM_APP_TRIGGERED`.
pub fn set_rules(rules: &[Rule], notify: HWND) -> Result<(), String> {
    NOTIFY_HWND.store(notify, Ordering::Relaxed);
    {
        let mut state = RULES.lock().unwrap_or_else(|e| e.into_inner());
        *state = rules
            .iter()
            .filter(|r| r.enabled && r.key < KEYS.len() && r.action < ACTIONS.len())
            .map(|&rule| RuleState { rule, pressed: false })
            .collect();
    }
    if rules.iter().any(|r| r.enabled) {
        install()
    } else {
        uninstall();
        Ok(())
    }
}

fn install() -> Result<(), String> {
    if !HOOK.load(Ordering::Relaxed).is_null() {
        return Ok(());
    }
    let hook = unsafe {
        SetWindowsHookExW(WH_KEYBOARD_LL, Some(low_level_keyboard_proc), GetModuleHandleW(std::ptr::null()), 0)
    };
    if hook.is_null() {
        return Err(format!("SetWindowsHookExW falhou: {}", std::io::Error::last_os_error()));
    }
    HOOK.store(hook, Ordering::Relaxed);
    Ok(())
}

/// Remove o hook; todas as teclas voltam à função original.
pub fn uninstall() {
    let hook = HOOK.swap(null_mut(), Ordering::Relaxed);
    if !hook.is_null() {
        unsafe {
            UnhookWindowsHookEx(hook);
        }
    }
}

unsafe extern "system" fn low_level_keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kb = &*(lparam as *const KBDLLHOOKSTRUCT);
        if kb.flags & LLKHF_INJECTED == 0 {
            let extended = kb.flags & LLKHF_EXTENDED != 0;
            // try_lock: o callback nunca pode ficar esperando. Se a GUI estiver trocando as
            // regras neste instante, o evento passa adiante sem alteração.
            if let Ok(mut rules) = RULES.try_lock() {
                let found = rules.iter_mut().find(|s| {
                    let key = &KEYS[s.rule.key];
                    kb.vkCode == key.vk as u32 && (!key.require_extended || extended)
                });
                if let Some(state) = found {
                    let action = &ACTIONS[state.rule.action];
                    match wparam as u32 {
                        WM_KEYDOWN | WM_SYSKEYDOWN => {
                            let was_pressed = std::mem::replace(&mut state.pressed, true);
                            if !was_pressed || action.repeat {
                                actions::send(action.vk);
                            }
                            if !was_pressed {
                                // PostMessage não bloqueia: o callback precisa retornar rápido.
                                PostMessageW(
                                    NOTIFY_HWND.load(Ordering::Relaxed),
                                    WM_APP_TRIGGERED,
                                    state.rule.key,
                                    state.rule.action as LPARAM,
                                );
                            }
                        }
                        WM_KEYUP | WM_SYSKEYUP => state.pressed = false,
                        _ => {}
                    }
                    // Descarta o evento original: a tecla não executa sua função normal.
                    return 1;
                }
            }
        }
    }
    CallNextHookEx(null_mut(), code, wparam, lparam)
}
