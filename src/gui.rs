//! Janela principal em Win32 puro (os mesmos controles nativos do C++: COMBOBOX, BUTTON,
//! STATIC, LISTBOX e a ListView do Common Controls), sem frameworks.

use std::cell::RefCell;
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{COLORREF, GetLastError, HWND, INVALID_HANDLE_VALUE, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CreateFontIndirectW, GetSysColor, GetSysColorBrush, SetBkMode, SetTextColor, COLOR_BTNFACE,
    COLOR_GRAYTEXT, HBRUSH, HDC, HFONT, InvalidateRect, TRANSPARENT,
};
use windows_sys::Win32::System::ApplicationInstallationAndServicing::{ActivateActCtx, CreateActCtxW, ACTCTXW};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::SystemInformation::{GetLocalTime, GetSystemDirectoryW};
use windows_sys::Win32::UI::Controls::*;
use windows_sys::Win32::UI::HiDpi::{GetDpiForSystem, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_SYSTEM_AWARE};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, VK_DELETE, VK_SPACE};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::actions::ACTIONS;
use crate::hook::{self, Rule, WM_APP_TRIGGERED};
use crate::keys::KEYS;

const ID_KEY: i32 = 101;
const ID_ACTION: i32 = 102;
const ID_ADD: i32 = 103;
const ID_LIST: i32 = 104;
const ID_TOGGLE: i32 = 105;
const ID_REMOVE: i32 = 106;
const ID_STATUS: i32 = 107;
const ID_LOG: i32 = 108;
const MAX_LOG_LINES: isize = 200;
const ACTCTX_FLAG_RESOURCE_NAME_VALID: u32 = 0x8;
const SS_RIGHT: u32 = 0x2;

thread_local! {
    /// Regras configuradas na lista (ativas e desativadas). Só a thread da GUI acessa.
    static RULES: RefCell<Vec<Rule>> = const { RefCell::new(Vec::new()) };
}

fn rules() -> Vec<Rule> {
    RULES.with(|r| r.borrow().clone())
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
            dwICC: ICC_STANDARD_CLASSES | ICC_LISTVIEW_CLASSES,
        };
        InitCommonControlsEx(&icc);
    }
}

struct Layout {
    dpi: i32,
    font: HFONT,
}

impl Layout {
    fn s(&self, v: i32) -> i32 {
        v * self.dpi / 96
    }
}

/// Cria um controle filho com a fonte do sistema.
unsafe fn child(
    parent: HWND,
    layout: &Layout,
    class: &str,
    text: &str,
    style: u32,
    id: i32,
    (x, y, cx, cy): (i32, i32, i32, i32),
) -> HWND {
    let hwnd = CreateWindowExW(
        0,
        w(class).as_ptr(),
        w(text).as_ptr(),
        WS_CHILD | WS_VISIBLE | style,
        layout.s(x),
        layout.s(y),
        layout.s(cx),
        layout.s(cy),
        parent,
        id as usize as HMENU,
        GetModuleHandleW(null()),
        null(),
    );
    SendMessageW(hwnd, WM_SETFONT, layout.font as WPARAM, 1);
    hwnd
}

unsafe fn item(hwnd: HWND, id: i32) -> HWND {
    GetDlgItem(hwnd, id)
}

unsafe fn set_text(hwnd: HWND, text: &str) {
    SetWindowTextW(hwnd, w(text).as_ptr());
}

unsafe fn combo_sel(hwnd: HWND, id: i32) -> usize {
    SendMessageW(item(hwnd, id), CB_GETCURSEL, 0, 0).max(0) as usize
}

/// Linha selecionada na lista de regras.
unsafe fn selected_row(hwnd: HWND) -> Option<usize> {
    let i = SendMessageW(item(hwnd, ID_LIST), LVM_GETNEXTITEM, usize::MAX, LVNI_SELECTED as LPARAM);
    usize::try_from(i).ok()
}

unsafe fn create_controls(hwnd: HWND, layout: &Layout) {
    let combo_style = WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWNLIST as u32;

    child(hwnd, layout, "STATIC", "Tecla:", 0, -1, (12, 16, 40, 20));
    let keys = child(hwnd, layout, "COMBOBOX", "", combo_style, ID_KEY, (54, 12, 140, 300));
    for key in KEYS {
        SendMessageW(keys, CB_ADDSTRING, 0, w(key.name).as_ptr() as LPARAM);
    }
    SendMessageW(keys, CB_SETCURSEL, 0, 0);

    child(hwnd, layout, "STATIC", "Ação:", 0, -1, (206, 16, 36, 20));
    let actions = child(hwnd, layout, "COMBOBOX", "", combo_style, ID_ACTION, (244, 12, 128, 300));
    for action in ACTIONS {
        SendMessageW(actions, CB_ADDSTRING, 0, w(action.name).as_ptr() as LPARAM);
    }
    SendMessageW(actions, CB_SETCURSEL, 0, 0);

    child(hwnd, layout, "BUTTON", "Adicionar", WS_TABSTOP | BS_DEFPUSHBUTTON as u32, ID_ADD, (380, 11, 80, 26));

    let list = child(
        hwnd,
        layout,
        "SysListView32",
        "",
        WS_TABSTOP | WS_BORDER | LVS_REPORT | LVS_SINGLESEL | LVS_SHOWSELALWAYS | LVS_NOSORTHEADER,
        ID_LIST,
        (12, 48, 448, 160),
    );
    let ex = (LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER) as LPARAM;
    SendMessageW(list, LVM_SETEXTENDEDLISTVIEWSTYLE, ex as WPARAM, ex);
    for (i, (title, width)) in [("Tecla", 150), ("Ação", 160), ("Status", 110)].iter().enumerate() {
        let text = w(title);
        let mut col: LVCOLUMNW = zeroed();
        col.mask = LVCF_TEXT | LVCF_WIDTH;
        col.cx = layout.s(*width);
        col.pszText = text.as_ptr() as *mut u16;
        SendMessageW(list, LVM_INSERTCOLUMNW, i, &col as *const _ as LPARAM);
    }

    child(hwnd, layout, "BUTTON", "Desativar", WS_TABSTOP, ID_TOGGLE, (12, 216, 100, 26));
    child(hwnd, layout, "BUTTON", "Remover", WS_TABSTOP, ID_REMOVE, (120, 216, 100, 26));
    child(hwnd, layout, "STATIC", "", SS_RIGHT, ID_STATUS, (230, 221, 230, 20));

    child(hwnd, layout, "STATIC", "Histórico:", 0, -1, (12, 254, 100, 20));
    child(
        hwnd,
        layout,
        "LISTBOX",
        "",
        WS_BORDER | WS_VSCROLL | LBS_NOINTEGRALHEIGHT as u32 | LBS_NOSEL as u32,
        ID_LOG,
        (12, 274, 448, 100),
    );
    refresh(hwnd, None);
}

unsafe fn set_cell(list: HWND, row: usize, col: i32, text: &str) {
    let text = w(text);
    let mut it: LVITEMW = zeroed();
    it.iSubItem = col;
    it.pszText = text.as_ptr() as *mut u16;
    SendMessageW(list, LVM_SETITEMTEXTW, row, &it as *const _ as LPARAM);
}

/// Redesenha a lista a partir de `RULES`, seleciona `select` e atualiza botões e status.
unsafe fn refresh(hwnd: HWND, select: Option<usize>) {
    let list = item(hwnd, ID_LIST);
    let rules = rules();
    SendMessageW(list, LVM_DELETEALLITEMS, 0, 0);
    for (row, rule) in rules.iter().enumerate() {
        let mut it: LVITEMW = zeroed();
        it.mask = LVIF_TEXT;
        it.iItem = row as i32;
        let text = w(KEYS[rule.key].name);
        it.pszText = text.as_ptr() as *mut u16;
        SendMessageW(list, LVM_INSERTITEMW, 0, &it as *const _ as LPARAM);
        set_cell(list, row, 1, ACTIONS[rule.action].name);
        set_cell(list, row, 2, if rule.enabled { "● Ativo" } else { "○ Desativado" });
    }
    if let Some(row) = select.filter(|&r| r < rules.len()) {
        let mut it: LVITEMW = zeroed();
        it.stateMask = LVIS_SELECTED | LVIS_FOCUSED;
        it.state = LVIS_SELECTED | LVIS_FOCUSED;
        SendMessageW(list, LVM_SETITEMSTATE, row, &it as *const _ as LPARAM);
        SendMessageW(list, LVM_ENSUREVISIBLE, row, 0);
    }
    update_buttons(hwnd);

    let active = rules.iter().filter(|r| r.enabled).count();
    let status = match (rules.len(), active) {
        (0, _) => "Nenhuma tecla remapeada".to_string(),
        (total, 0) => format!("○ Todas desativadas ({total})"),
        (total, n) => format!("● {n} de {total} tecla(s) ativa(s)"),
    };
    set_text(item(hwnd, ID_STATUS), &status);
    InvalidateRect(item(hwnd, ID_STATUS), null(), 1);
}

/// Habilita os botões da linha selecionada e ajusta o texto de Ativar/Desativar.
unsafe fn update_buttons(hwnd: HWND) {
    let rule = selected_row(hwnd).and_then(|row| rules().get(row).copied());
    EnableWindow(item(hwnd, ID_TOGGLE), rule.is_some() as i32);
    EnableWindow(item(hwnd, ID_REMOVE), rule.is_some() as i32);
    let label = if rule.is_some_and(|r| !r.enabled) { "Ativar" } else { "Desativar" };
    set_text(item(hwnd, ID_TOGGLE), label);
}

/// Aplica as regras ao hook. Em caso de erro, avisa e desfaz a alteração.
unsafe fn apply(hwnd: HWND, new_rules: Vec<Rule>, select: Option<usize>) -> bool {
    match hook::set_rules(&new_rules, hwnd) {
        Ok(()) => {
            RULES.with(|r| *r.borrow_mut() = new_rules);
            refresh(hwnd, select);
            true
        }
        Err(e) => {
            let _ = hook::set_rules(&rules(), hwnd);
            MessageBoxW(hwnd, w(&e).as_ptr(), w("Erro").as_ptr(), MB_ICONERROR | MB_OK);
            false
        }
    }
}

unsafe fn log(hwnd: HWND, msg: &str) {
    let list = item(hwnd, ID_LOG);
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

fn describe(rule: &Rule) -> String {
    format!("{} → {}", KEYS[rule.key].name, ACTIONS[rule.action].name)
}

/// Adiciona a regra escolhida nos combos. Se a tecla já estiver na lista, troca a ação dela.
unsafe fn on_add(hwnd: HWND) {
    let rule = Rule { key: combo_sel(hwnd, ID_KEY), action: combo_sel(hwnd, ID_ACTION), enabled: true };
    let mut new_rules = rules();
    let (row, verb) = match new_rules.iter().position(|r| r.key == rule.key) {
        Some(row) => {
            new_rules[row] = rule;
            (row, "Atualizado")
        }
        None => {
            new_rules.push(rule);
            (new_rules.len() - 1, "Adicionado")
        }
    };
    if apply(hwnd, new_rules, Some(row)) {
        log(hwnd, &format!("{verb}: {}", describe(&rule)));
    }
}

unsafe fn on_toggle(hwnd: HWND) {
    let Some(row) = selected_row(hwnd) else { return };
    let mut new_rules = rules();
    let Some(rule) = new_rules.get_mut(row) else { return };
    rule.enabled = !rule.enabled;
    let msg = format!("{}: {}", if rule.enabled { "Ativado" } else { "Desativado" }, describe(rule));
    if apply(hwnd, new_rules, Some(row)) {
        log(hwnd, &msg);
    }
}

unsafe fn on_remove(hwnd: HWND) {
    let Some(row) = selected_row(hwnd) else { return };
    let mut new_rules = rules();
    if row >= new_rules.len() {
        return;
    }
    let removed = new_rules.remove(row);
    let select = (!new_rules.is_empty()).then(|| row.min(new_rules.len() - 1));
    if apply(hwnd, new_rules, select) {
        log(hwnd, &format!("Removido: {} (tecla de volta ao normal)", describe(&removed)));
    }
}

/// Ao selecionar uma linha, os combos mostram a regra dela (facilita trocar a ação).
unsafe fn on_row_selected(hwnd: HWND, row: usize) {
    if let Some(rule) = rules().get(row) {
        SendMessageW(item(hwnd, ID_KEY), CB_SETCURSEL, rule.key, 0);
        SendMessageW(item(hwnd, ID_ACTION), CB_SETCURSEL, rule.action, 0);
    }
    update_buttons(hwnd);
}

unsafe fn on_list_notify(hwnd: HWND, hdr: &NMHDR, lparam: LPARAM) -> LRESULT {
    match hdr.code {
        LVN_ITEMCHANGED => {
            let nm = &*(lparam as *const NMLISTVIEW);
            let now = nm.uNewState & LVIS_SELECTED != 0;
            let before = nm.uOldState & LVIS_SELECTED != 0;
            if now && !before {
                on_row_selected(hwnd, nm.iItem as usize);
            } else if before && !now {
                update_buttons(hwnd);
            }
        }
        NM_DBLCLK => on_toggle(hwnd),
        LVN_KEYDOWN => {
            let nm = &*(lparam as *const NMLVKEYDOWN);
            match nm.wVKey {
                VK_DELETE => on_remove(hwnd),
                VK_SPACE => on_toggle(hwnd),
                _ => {}
            }
        }
        NM_CUSTOMDRAW => {
            // Linhas desativadas em cinza.
            let cd = &mut *(lparam as *mut NMLVCUSTOMDRAW);
            match cd.nmcd.dwDrawStage {
                CDDS_PREPAINT => return CDRF_NOTIFYITEMDRAW as LRESULT,
                CDDS_ITEMPREPAINT => {
                    let row = cd.nmcd.dwItemSpec;
                    if rules().get(row).is_some_and(|r| !r.enabled) {
                        cd.clrText = GetSysColor(COLOR_GRAYTEXT);
                    }
                    return CDRF_DODEFAULT as LRESULT;
                }
                _ => {}
            }
        }
        _ => {}
    }
    0
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            let id = (wparam & 0xffff) as i32;
            let notify = ((wparam >> 16) & 0xffff) as u32;
            if notify == BN_CLICKED {
                match id {
                    ID_ADD => on_add(hwnd),
                    ID_TOGGLE => on_toggle(hwnd),
                    ID_REMOVE => on_remove(hwnd),
                    _ => {}
                }
            }
            0
        }
        WM_NOTIFY => {
            let hdr = &*(lparam as *const NMHDR);
            if hdr.idFrom == ID_LIST as usize {
                return on_list_notify(hwnd, hdr, lparam);
            }
            0
        }
        WM_APP_TRIGGERED => {
            if wparam < KEYS.len() && (lparam as usize) < ACTIONS.len() {
                log(hwnd, &format!("{} → {}", KEYS[wparam].name, ACTIONS[lparam as usize].name));
            }
            0
        }
        WM_CTLCOLORSTATIC => {
            let hdc = wparam as HDC;
            SetBkMode(hdc, TRANSPARENT as i32);
            if GetDlgCtrlID(lparam as HWND) == ID_STATUS {
                let any_active = rules().iter().any(|r| r.enabled);
                let color: COLORREF = if any_active { 0x00_30_80_10 } else { 0x00_60_60_60 };
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

pub fn run() -> Result<(), String> {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_SYSTEM_AWARE);
        enable_visual_styles();

        let hinstance = GetModuleHandleW(null());
        let class_name = w("KeyboardReprogramerWindow");
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: app_icon(SM_CXICON),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: (COLOR_BTNFACE + 1) as usize as HBRUSH,
            lpszMenuName: null(),
            lpszClassName: class_name.as_ptr(),
            hIconSm: app_icon(SM_CXSMICON),
        };
        if RegisterClassExW(&wc) == 0 {
            return Err(format!("RegisterClassExW falhou ({})", GetLastError()));
        }

        let mut ncm: NONCLIENTMETRICSW = zeroed();
        ncm.cbSize = size_of::<NONCLIENTMETRICSW>() as u32;
        SystemParametersInfoW(SPI_GETNONCLIENTMETRICS, ncm.cbSize, &mut ncm as *mut _ as *mut c_void, 0);
        let layout = Layout { dpi: GetDpiForSystem() as i32, font: CreateFontIndirectW(&ncm.lfMessageFont) };

        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
        let mut rect = RECT { left: 0, top: 0, right: layout.s(472), bottom: layout.s(386) };
        AdjustWindowRectEx(&mut rect, style, 0, 0);

        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            w("Keyboard Reprogramer").as_ptr(),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            rect.right - rect.left,
            rect.bottom - rect.top,
            null_mut(),
            null_mut(),
            hinstance,
            null(),
        );
        if hwnd.is_null() {
            return Err(format!("CreateWindowExW falhou ({})", GetLastError()));
        }
        create_controls(hwnd, &layout);
        ShowWindow(hwnd, SW_SHOW);

        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            // IsDialogMessage dá navegação com Tab/Enter entre os controles.
            if IsDialogMessageW(hwnd, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        Ok(())
    }
}
