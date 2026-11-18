//! Hook global de teclado de baixo nível (WH_KEYBOARD_LL).
//!
//! Para cada evento de teclado do sistema, o Windows chama `low_level_keyboard_proc`
//! antes de entregá-lo ao aplicativo em foco. Ele faz duas coisas:
//!
//! - **Remapeamento**: se o evento é de uma tecla com regra ativa, retorna 1 (o evento
//!   é descartado e a função original não acontece) e dispara a ação de mídia via SendInput.
//! - **Pausa**: enquanto a GUI captura uma tecla (pelas mensagens normais da janela),
//!   o hook repassa tudo, para que mesmo uma tecla já remapeada chegue à janela.
//!
//! As teclas são identificadas pelo scan code + flag "extended" (a posição física da tecla),
//! o que distingue, por exemplo, o Insert do Num 0 independentemente do NumLock.
//! O hook fica instalado enquanto o app roda; sem regras, ele só repassa os eventos.

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

/// Uma regra disparou. `wparam` = `Rule::uid`.
pub const WM_APP_TRIGGERED: u32 = WM_APP + 1;

/// Tecla física.
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

/// Regra de remapeamento ativa.
#[derive(Clone, Copy)]
pub struct Rule {
    pub uid: u32,
    pub key: Key,
    pub action: usize,
}

struct RuleState {
    rule: Rule,
    /// A tecla está pressionada (evita repetir ações que não devem repetir).
    pressed: bool,
}

static HOOK: AtomicPtr<c_void> = AtomicPtr::new(null_mut());
static NOTIFY_HWND: AtomicPtr<c_void> = AtomicPtr::new(null_mut());
static RULES: Mutex<Vec<RuleState>> = Mutex::new(Vec::new());
/// Pausado: repassa todas as teclas (usado durante a captura).
static PAUSED: AtomicBool = AtomicBool::new(false);

/// Instala o hook na thread atual (que precisa rodar um loop de mensagens).
/// `notify` recebe `WM_APP_TRIGGERED`.
pub fn install(notify: HWND) -> Result<(), String> {
    NOTIFY_HWND.store(notify, Ordering::Relaxed);
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

/// Substitui as regras ativas (lista vazia = nada é remapeado).
pub fn set_rules(rules: &[Rule]) {
    let mut state = RULES.lock().unwrap_or_else(|e| e.into_inner());
    *state = rules
        .iter()
        .filter(|r| r.action < ACTIONS.len())
        .map(|&rule| RuleState { rule, pressed: false })
        .collect();
}

/// Pausa/retoma o remapeamento (sem alterar as regras).
pub fn set_paused(on: bool) {
    PAUSED.store(on, Ordering::Relaxed);
}

unsafe extern "system" fn low_level_keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kb = &*(lparam as *const KBDLLHOOKSTRUCT);
        // Ignora só o que o próprio app injetou (evita recursão com as teclas de mídia).
        let own = kb.flags & LLKHF_INJECTED != 0 && kb.dwExtraInfo == actions::SELF_INJECTED_TAG;
        if !own {
            if let Some(result) = handle(kb, wparam as u32) {
                return result;
            }
        }
    }
    CallNextHookEx(null_mut(), code, wparam, lparam)
}

/// Retorna `Some(1)` para descartar o evento, `None` para repassá-lo.
unsafe fn handle(kb: &KBDLLHOOKSTRUCT, msg: u32) -> Option<LRESULT> {
    let key = Key { vk: kb.vkCode as u16, scan: kb.scanCode, extended: kb.flags & LLKHF_EXTENDED != 0 };
    let down = matches!(msg, WM_KEYDOWN | WM_SYSKEYDOWN);
    let up = matches!(msg, WM_KEYUP | WM_SYSKEYUP);
    let notify = NOTIFY_HWND.load(Ordering::Relaxed);
    if PAUSED.load(Ordering::Relaxed) {
        return None;
    }

    // try_lock: o callback nunca pode ficar esperando. Se a GUI estiver trocando as
    // regras neste instante, o evento passa adiante sem alteração.
    let mut rules = RULES.try_lock().ok()?;
    let state = rules.iter_mut().find(|s| s.rule.key.same_physical(&key))?;
    let action = &ACTIONS[state.rule.action];
    if down {
        let was_pressed = std::mem::replace(&mut state.pressed, true);
        if !was_pressed || action.repeat {
            actions::send(action.vk);
        }
        if !was_pressed {
            // PostMessage não bloqueia: o callback precisa retornar rápido.
            PostMessageW(notify, WM_APP_TRIGGERED, state.rule.uid as WPARAM, 0);
        }
    } else if up {
        state.pressed = false;
    }
    // Descarta o evento original: a tecla não executa sua função normal.
    Some(1)
}
