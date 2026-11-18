//! Janela principal em Win32 puro (controles nativos BUTTON, COMBOBOX, STATIC e LISTBOX),
//! sem frameworks.
//!
//! Layout:
//! - Topo: botão geral (desativa/reativa tudo) e status.
//! - Painel rolável com uma linha por tecla: [tecla] [ação ▾] [Desativar] [Excluir],
//!   seguido do botão "+ Adicionar", que cria uma nova linha e já captura a tecla.
//! - Histórico de acionamentos.

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
use crate::keys;

const ID_MASTER: i32 = 10;
const ID_STATUS: i32 = 11;
const ID_PANEL: i32 = 12;
const ID_LOG: i32 = 13;
const ID_ADD: i32 = 20;
/// Controles das linhas: ID = ROW_ID_BASE + uid * 4 + parte.
const ROW_ID_BASE: i32 = 1000;
const PART_HOTKEY: usize = 0;
const PART_ACTION: usize = 1;
const PART_TOGGLE: usize = 2;
const PART_DELETE: usize = 3;

const ROW_H: i32 = 36;
const PAD: i32 = 6;
const MAX_LOG_LINES: isize = 200;
const ACTCTX_FLAG_RESOURCE_NAME_VALID: u32 = 0x8;

const CAPTURE_TEXT: &str = "Pressione uma tecla...";
const EMPTY_KEY_TEXT: &str = "Clique para definir a tecla";

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
    /// Controles: tecla, ação, ativar/desativar, excluir.
    ctl: [HWND; 4],
}

struct App {
    main: HWND,
    panel: HWND,
    add: HWND,
    rows: Vec<Row>,
    /// Botão geral: desligado = nenhuma tecla é remapeada, sem perder o estado das linhas.
    master: bool,
    /// Linha cuja tecla está sendo capturada.
    capturing: Option<u32>,
    next_uid: u32,
    scroll: i32,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    /// Tecla recém-capturada: seus repeats e o key-up não devem chegar aos controles.
    static SWALLOW: Cell<Option<(u32, bool)>> = const { Cell::new(None) };
}

/// Acesso ao estado. A closure só deve ler/alterar dados: chamadas Win32 que podem
/// reentrar nos window procs ficam fora dela. Se o estado já estiver em uso (reentrada),
/// retorna None.
fn with<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f)))
}

/// Converte para UTF-16 terminado em zero (formato das APIs "W" do Windows).
fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Ativa os estilos visuais (Common Controls v6) sem precisar embutir um manifesto no exe:
/// usa o manifesto que o próprio shell32.dll carrega como recurso 124.
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

/// Cria um controle filho com a fonte do sistema (coordenadas em pixels a 96 DPI).
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
    key.map(|k| keys::name(&k)).unwrap_or_else(|| "(sem tecla)".into())
}

// ---------------------------------------------------------------------------------------
// Estado -> hook e status

/// Envia as regras ativas ao hook e atualiza o status e o botão geral.
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

    let status = if !master {
        "○ Remapeamento desativado (geral)".to_string()
    } else if rules.is_empty() {
        "○ Nenhuma tecla ativa".to_string()
    } else {
        format!("● {} tecla(s) remapeada(s) ativa(s)", rules.len())
    };
    let status_ctl = GetDlgItem(main, ID_STATUS);
    set_text(status_ctl, &status);
    InvalidateRect(status_ctl, null(), 1);
    set_text(GetDlgItem(main, ID_MASTER), if master { "Desativar tudo" } else { "Ativar tudo" });
}

/// Atualiza os textos e estados dos controles de uma linha.
unsafe fn update_row(uid: u32) {
    let Some(Some((ctl, key, action, enabled, capturing))) = with(|a| {
        let capturing = a.capturing == Some(uid);
        a.rows.iter().find(|r| r.uid == uid).map(|r| (r.ctl, r.key, r.action, r.enabled, capturing))
    }) else {
        return;
    };
    let hotkey = if capturing {
        CAPTURE_TEXT.to_string()
    } else {
        key.map(|k| keys::name(&k)).unwrap_or_else(|| EMPTY_KEY_TEXT.into())
    };
    set_text(ctl[PART_HOTKEY], &hotkey);
    // Botão da tecla "afundado" enquanto captura.
    SendMessageW(ctl[PART_HOTKEY], BM_SETSTATE, capturing as WPARAM, 0);
    SendMessageW(ctl[PART_ACTION], CB_SETCURSEL, action, 0);
    EnableWindow(ctl[PART_ACTION], enabled as i32);
    set_text(ctl[PART_TOGGLE], if enabled { "Desativar" } else { "Ativar" });
}

// ---------------------------------------------------------------------------------------
// Painel rolável

/// Posiciona as linhas e o botão Adicionar conforme a rolagem e atualiza a barra de rolagem.
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
// Ações do usuário

unsafe fn add_row() {
    let Some((panel, uid)) = with(|a| {
        a.next_uid += 1;
        (a.panel, a.next_uid)
    }) else {
        return;
    };
    let id = |part: usize| ROW_ID_BASE + uid as i32 * 4 + part as i32;
    let r = (0, 0, 10, 10); // posição definida no relayout
    let hotkey = child(panel, "BUTTON", EMPTY_KEY_TEXT, WS_TABSTOP, id(PART_HOTKEY), r);
    let combo = child(panel, "COMBOBOX", "", WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWNLIST as u32, id(PART_ACTION), r);
    for action in ACTIONS {
        SendMessageW(combo, CB_ADDSTRING, 0, w(action.name).as_ptr() as LPARAM);
    }
    let toggle = child(panel, "BUTTON", "Desativar", WS_TABSTOP, id(PART_TOGGLE), r);
    let delete = child(panel, "BUTTON", "Excluir", WS_TABSTOP, id(PART_DELETE), r);

    with(|a| {
        a.rows.push(Row { uid, key: None, action: 0, enabled: true, ctl: [hotkey, combo, toggle, delete] });
    });
    scroll_to(i32::MAX); // mostra a linha nova
    update_row(uid);
    SetFocus(hotkey);
    start_capture(uid);
}

unsafe fn start_capture(uid: u32) {
    let previous = with(|a| a.capturing.replace(uid)).flatten();
    // Durante a captura o hook repassa tudo, para a janela receber até teclas já remapeadas.
    hook::set_paused(true);
    if let Some(prev) = previous.filter(|&p| p != uid) {
        update_row(prev);
    }
    update_row(uid);
    if let Some(Some(i)) = with(|a| a.rows.iter().position(|r| r.uid == uid)) {
        log(&format!("Linha {}: pressione a tecla a reprogramar (Esc cancela)", i + 1));
    }
}

unsafe fn cancel_capture() {
    if let Some(uid) = with(|a| a.capturing.take()).flatten() {
        hook::set_paused(false);
        update_row(uid);
    }
}

/// Captura pelo loop de mensagens (como o controle Hotkey do Windows): enquanto uma linha
/// está em captura, a próxima tecla que chega à janela (WM_KEYDOWN/WM_SYSKEYDOWN) define a
/// tecla da linha. O lParam traz o scan code e a flag extended, os mesmos dados que o hook
/// usa para comparar. Retorna true se a mensagem foi consumida.
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
    // Print Screen não gera WM_KEYDOWN, só WM_KEYUP; os demais key-ups são ignorados.
    if !down && vk != VK_SNAPSHOT {
        return true;
    }
    // Shift/Ctrl/Alt chegam genéricos; o scan code diz se é o esquerdo ou o direito.
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
        log(&format!("{name} já está em uso na linha {}", i + 1));
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
        log(&format!("Linha {}: {} → {}", i + 1, name, ACTIONS[action].name));
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
        let verb = if enabled { "ativada" } else { "desativada" };
        log(&format!("Linha {} ({}) {verb}", i + 1, key_label(key)));
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
    log(&format!("Linha {} ({}) excluída", i + 1, key_label(key)));
}

unsafe fn action_changed(uid: u32, combo: HWND) {
    let sel = SendMessageW(combo, CB_GETCURSEL, 0, 0);
    let Ok(action) = usize::try_from(sel) else { return };
    let info = with(|a| {
        let i = a.rows.iter().position(|r| r.uid == uid)?;
        a.rows[i].action = action;
        Some((i, a.rows[i].key))
    })
    .flatten();
    sync();
    if let Some((i, Some(key))) = info {
        log(&format!("Linha {}: {} → {}", i + 1, keys::name(&key), ACTIONS[action].name));
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
        Some(true) => log("Remapeamento reativado"),
        Some(false) => log("Remapeamento desativado: todas as teclas voltaram ao normal"),
        None => {}
    }
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
                    // Clicar de novo durante a captura não cancela (só o Esc cancela).
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
            if (wparam & 0xffff) as i32 == ID_MASTER && ((wparam >> 16) & 0xffff) as u32 == BN_CLICKED {
                toggle_master();
            }
            0
        }
        WM_APP_TRIGGERED => {
            let info = with(|a| {
                a.rows.iter().position(|r| r.uid == wparam as u32).map(|i| (i, a.rows[i].key, a.rows[i].action))
            })
            .flatten();
            if let Some((i, key, action)) = info {
                log(&format!("[linha {}] {} → {}", i + 1, key_label(key), ACTIONS[action].name));
            }
            0
        }
        WM_ACTIVATEAPP => {
            // Saiu do app: não captura teclas digitadas em outras janelas.
            if wparam == 0 {
                cancel_capture();
            }
            0
        }
        WM_MOUSEWHEEL => {
            // Rolagem sobre a janela principal rola o painel de linhas.
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

/// Há alguma regra efetivamente ativa (para a cor do status)?
fn no_active_rules() -> bool {
    with(|a| !a.master || !a.rows.iter().any(|r| r.enabled && r.key.is_some())).unwrap_or(true)
}

// ---------------------------------------------------------------------------------------

/// Ícone do app (recurso 1, embutido pelo build.rs) no tamanho do sistema indicado
/// (SM_CXICON = barra de tarefas/Alt+Tab, SM_CXSMICON = barra de título).
/// Sem o recurso, usa o ícone padrão do Windows.
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
        return Err(format!("RegisterClassExW falhou ({})", GetLastError()));
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

        let main_class = w("KeyboardReprogramerWindow");
        let panel_class = w("KeyboardReprogramerRows");
        register_class(&main_class, Some(wnd_proc), COLOR_BTNFACE, true)?;
        register_class(&panel_class, Some(panel_proc), COLOR_WINDOW, false)?;

        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
        let mut rect = RECT { left: 0, top: 0, right: s(520), bottom: s(414) };
        AdjustWindowRectEx(&mut rect, style, 0, 0);
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
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
        if main.is_null() {
            return Err(format!("CreateWindowExW falhou ({})", GetLastError()));
        }

        child(main, "BUTTON", "Desativar tudo", WS_TABSTOP, ID_MASTER, (12, 10, 130, 28));
        child(main, "STATIC", "", 0, ID_STATUS, (154, 16, 354, 20));
        // WS_EX_CONTROLPARENT: o Tab navega pelos controles das linhas.
        let panel = child_ex(
            main,
            WS_EX_CLIENTEDGE | WS_EX_CONTROLPARENT,
            "KeyboardReprogramerRows",
            "",
            WS_VSCROLL | WS_TABSTOP,
            ID_PANEL,
            (12, 46, 496, 240),
        );
        let add = child(panel, "BUTTON", "+ Adicionar", WS_TABSTOP, ID_ADD, (6, PAD, 130, 28));
        child(main, "STATIC", "Histórico:", 0, -1, (12, 296, 100, 20));
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
            // IsDialogMessage dá navegação com Tab entre os controles.
            if IsDialogMessageW(main, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        Ok(())
    }
}
