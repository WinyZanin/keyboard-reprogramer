//! User interface translations.
//!
//! Every user-visible string lives in a [`Strings`] table, one `static` per language.
//! Tables are plain Rust structs, so a missing or mistyped translation is a compile error.
//! Parameterized messages are `fn` fields, which keeps placeholders type-checked.
//!
//! To add a language: add a [`Lang`] variant (and to [`Lang::ALL`]), give it a native name
//! and a `Strings` table, and map it in [`Lang::strings`]. Optionally detect it in [`detect`].

use std::sync::atomic::{AtomicU8, Ordering};

use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;

use crate::actions::ACTION_COUNT;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Lang {
    En = 0,
    PtBr = 1,
}

impl Lang {
    /// Languages in the order shown in the menu.
    pub const ALL: [Lang; 2] = [Lang::En, Lang::PtBr];

    /// Name of the language in the language itself (as shown in the menu).
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::PtBr => "Português (Brasil)",
        }
    }

    pub fn strings(self) -> &'static Strings {
        match self {
            Lang::En => &EN,
            Lang::PtBr => &PT_BR,
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(Lang::En as u8);

pub fn set(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

pub fn current() -> Lang {
    Lang::ALL.into_iter().find(|&l| l as u8 == CURRENT.load(Ordering::Relaxed)).unwrap_or(Lang::En)
}

/// Strings of the current language.
pub fn t() -> &'static Strings {
    current().strings()
}

/// Picks the language from the Windows display language (falls back to English).
pub fn detect() -> Lang {
    const LANG_PORTUGUESE: u16 = 0x16;
    // The low 10 bits of a LANGID are the primary language.
    let primary = unsafe { GetUserDefaultUILanguage() } & 0x3FF;
    match primary {
        LANG_PORTUGUESE => Lang::PtBr,
        _ => Lang::En,
    }
}

pub struct Strings {
    // Menu
    pub menu_language: &'static str,
    pub menu_help: &'static str,
    pub menu_about: &'static str,

    // About dialog
    pub about_title: &'static str,
    /// Body of the About dialog (the app name is the heading). `<a href="...">` tags become
    /// clickable links.
    pub about_text: fn(version: &str, url: &str) -> String,

    // Main window
    pub master_disable: &'static str,
    pub master_enable: &'static str,
    pub status_master_off: &'static str,
    pub status_none: &'static str,
    pub status_active: fn(usize) -> String,
    pub add: &'static str,
    pub history: &'static str,

    // Rows
    pub capture_prompt: &'static str,
    pub empty_key: &'static str,
    pub no_key: &'static str,
    pub toggle_disable: &'static str,
    pub toggle_enable: &'static str,
    pub delete: &'static str,

    // History log
    pub log_capture: fn(row: usize) -> String,
    pub log_key_in_use: fn(key: &str, row: usize) -> String,
    pub log_mapped: fn(row: usize, key: &str, action: &str) -> String,
    pub log_row_enabled: fn(row: usize, key: &str) -> String,
    pub log_row_disabled: fn(row: usize, key: &str) -> String,
    pub log_row_deleted: fn(row: usize, key: &str) -> String,
    pub log_master_on: &'static str,
    pub log_master_off: &'static str,
    pub log_triggered: fn(row: usize, key: &str, action: &str) -> String,
    pub log_language: fn(name: &str) -> String,

    /// Same order as `actions::ACTIONS`.
    pub action_names: [&'static str; ACTION_COUNT],

    // Key names that GetKeyNameText gets wrong or doesn't provide
    pub key_win_left: &'static str,
    pub key_win_right: &'static str,
    pub key_menu: &'static str,
    pub key_media_play_pause: &'static str,
    pub key_media_next: &'static str,
    pub key_media_prev: &'static str,
    pub key_media_stop: &'static str,
    pub key_volume_up: &'static str,
    pub key_volume_down: &'static str,
    pub key_volume_mute: &'static str,
    pub key_browser_home: &'static str,
    pub key_browser_back: &'static str,
    pub key_browser_forward: &'static str,
    pub key_browser_search: &'static str,
    pub key_browser_favorites: &'static str,
    pub key_browser_refresh: &'static str,
    pub key_mail: &'static str,
    pub key_app1: &'static str,
    pub key_calculator: &'static str,
    pub key_unknown: fn(vk: u16) -> String,
}

static EN: Strings = Strings {
    menu_language: "&Language",
    menu_help: "&Help",
    menu_about: "&About...",

    about_title: "About Keyboard Reprogramer",
    about_text: |version, url| {
        format!(
            "Version {version}\n\n\
             Remaps rarely used keys to media control actions.\n\n\
             Copyright © 2026 WinyZanin\n\
             Released under the MIT License.\n\n\
             <a href=\"{url}\">{url}</a>"
        )
    },

    master_disable: "Disable all",
    master_enable: "Enable all",
    status_master_off: "○ Remapping disabled (all)",
    status_none: "○ No active keys",
    status_active: |n| format!("● {n} remapped key(s) active"),
    add: "+ Add",
    history: "History:",

    capture_prompt: "Press a key...",
    empty_key: "Click to set the key",
    no_key: "(no key)",
    toggle_disable: "Disable",
    toggle_enable: "Enable",
    delete: "Delete",

    log_capture: |row| format!("Row {row}: press the key to remap (Esc cancels)"),
    log_key_in_use: |key, row| format!("{key} is already used in row {row}"),
    log_mapped: |row, key, action| format!("Row {row}: {key} → {action}"),
    log_row_enabled: |row, key| format!("Row {row} ({key}) enabled"),
    log_row_disabled: |row, key| format!("Row {row} ({key}) disabled"),
    log_row_deleted: |row, key| format!("Row {row} ({key}) deleted"),
    log_master_on: "Remapping enabled again",
    log_master_off: "Remapping disabled: all keys are back to normal",
    log_triggered: |row, key, action| format!("[row {row}] {key} → {action}"),
    log_language: |name| format!("Language: {name}"),

    action_names: ["Play/Pause", "Next track", "Previous track", "Stop", "Volume +", "Volume -", "Mute"],

    key_win_left: "Windows (left)",
    key_win_right: "Windows (right)",
    key_menu: "Menu",
    key_media_play_pause: "Media: Play/Pause",
    key_media_next: "Media: Next",
    key_media_prev: "Media: Previous",
    key_media_stop: "Media: Stop",
    key_volume_up: "Volume +",
    key_volume_down: "Volume -",
    key_volume_mute: "Mute",
    key_browser_home: "Browser: Home",
    key_browser_back: "Browser: Back",
    key_browser_forward: "Browser: Forward",
    key_browser_search: "Browser: Search",
    key_browser_favorites: "Browser: Favorites",
    key_browser_refresh: "Browser: Refresh",
    key_mail: "Mail",
    key_app1: "App 1",
    key_calculator: "Calculator",
    key_unknown: |vk| format!("Key 0x{vk:02X}"),
};

static PT_BR: Strings = Strings {
    menu_language: "&Idioma",
    menu_help: "Aj&uda",
    menu_about: "&Sobre...",

    about_title: "Sobre o Keyboard Reprogramer",
    about_text: |version, url| {
        format!(
            "Versão {version}\n\n\
             Reprograma teclas pouco usadas para ações de controle de mídia.\n\n\
             Copyright © 2026 WinyZanin\n\
             Distribuído sob a licença MIT.\n\n\
             <a href=\"{url}\">{url}</a>"
        )
    },

    master_disable: "Desativar tudo",
    master_enable: "Ativar tudo",
    status_master_off: "○ Remapeamento desativado (geral)",
    status_none: "○ Nenhuma tecla ativa",
    status_active: |n| format!("● {n} tecla(s) remapeada(s) ativa(s)"),
    add: "+ Adicionar",
    history: "Histórico:",

    capture_prompt: "Pressione uma tecla...",
    empty_key: "Clique para definir a tecla",
    no_key: "(sem tecla)",
    toggle_disable: "Desativar",
    toggle_enable: "Ativar",
    delete: "Excluir",

    log_capture: |row| format!("Linha {row}: pressione a tecla a reprogramar (Esc cancela)"),
    log_key_in_use: |key, row| format!("{key} já está em uso na linha {row}"),
    log_mapped: |row, key, action| format!("Linha {row}: {key} → {action}"),
    log_row_enabled: |row, key| format!("Linha {row} ({key}) ativada"),
    log_row_disabled: |row, key| format!("Linha {row} ({key}) desativada"),
    log_row_deleted: |row, key| format!("Linha {row} ({key}) excluída"),
    log_master_on: "Remapeamento reativado",
    log_master_off: "Remapeamento desativado: todas as teclas voltaram ao normal",
    log_triggered: |row, key, action| format!("[linha {row}] {key} → {action}"),
    log_language: |name| format!("Idioma: {name}"),

    action_names: ["Play/Pause", "Próxima faixa", "Faixa anterior", "Stop", "Volume +", "Volume -", "Mudo"],

    key_win_left: "Windows (esq.)",
    key_win_right: "Windows (dir.)",
    key_menu: "Menu",
    key_media_play_pause: "Mídia: Play/Pause",
    key_media_next: "Mídia: Próxima",
    key_media_prev: "Mídia: Anterior",
    key_media_stop: "Mídia: Stop",
    key_volume_up: "Volume +",
    key_volume_down: "Volume -",
    key_volume_mute: "Mudo",
    key_browser_home: "Navegador: Início",
    key_browser_back: "Navegador: Voltar",
    key_browser_forward: "Navegador: Avançar",
    key_browser_search: "Navegador: Pesquisar",
    key_browser_favorites: "Navegador: Favoritos",
    key_browser_refresh: "Navegador: Atualizar",
    key_mail: "E-mail",
    key_app1: "App 1",
    key_calculator: "Calculadora",
    key_unknown: |vk| format!("Tecla 0x{vk:02X}"),
};
