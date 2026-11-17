use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    VK_APPS, VK_DELETE, VK_END, VK_HOME, VK_INSERT, VK_NEXT, VK_PAUSE, VK_PRIOR, VK_RCONTROL,
    VK_SCROLL, VK_SNAPSHOT,
};

/// Tecla física que pode ser remapeada.
pub struct SourceKey {
    pub name: &'static str,
    pub vk: u16,
    /// Exige a flag LLKHF_EXTENDED. Distingue a tecla do bloco de navegação
    /// da tecla equivalente do teclado numérico (ex.: Insert x Numpad 0 com NumLock desligado).
    pub require_extended: bool,
}

pub const KEYS: &[SourceKey] = &[
    SourceKey { name: "Insert", vk: VK_INSERT, require_extended: true },
    SourceKey { name: "Scroll Lock", vk: VK_SCROLL, require_extended: false },
    SourceKey { name: "Pause/Break", vk: VK_PAUSE, require_extended: false },
    SourceKey { name: "Print Screen", vk: VK_SNAPSHOT, require_extended: false },
    SourceKey { name: "Home", vk: VK_HOME, require_extended: true },
    SourceKey { name: "End", vk: VK_END, require_extended: true },
    SourceKey { name: "Page Up", vk: VK_PRIOR, require_extended: true },
    SourceKey { name: "Page Down", vk: VK_NEXT, require_extended: true },
    SourceKey { name: "Delete", vk: VK_DELETE, require_extended: true },
    SourceKey { name: "Ctrl direito", vk: VK_RCONTROL, require_extended: false },
    SourceKey { name: "Menu (Apps)", vk: VK_APPS, require_extended: false },
];
