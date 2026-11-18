//! Nome legível de uma tecla física.

use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

use crate::hook::Key;

/// Nome da tecla no idioma do layout de teclado do Windows (ex.: "INSERT", "NUM 0").
pub fn name(key: &Key) -> String {
    // Teclas que o GetKeyNameText nomeia errado ou não nomeia.
    let fixed = match key.vk {
        VK_PAUSE => Some("Pause/Break"),
        VK_CANCEL => Some("Break"),
        VK_SNAPSHOT => Some("Print Screen"),
        VK_NUMLOCK => Some("Num Lock"),
        VK_LWIN => Some("Windows (esq.)"),
        VK_RWIN => Some("Windows (dir.)"),
        VK_APPS => Some("Menu"),
        VK_MEDIA_PLAY_PAUSE => Some("Mídia: Play/Pause"),
        VK_MEDIA_NEXT_TRACK => Some("Mídia: Próxima"),
        VK_MEDIA_PREV_TRACK => Some("Mídia: Anterior"),
        VK_MEDIA_STOP => Some("Mídia: Stop"),
        VK_VOLUME_UP => Some("Volume +"),
        VK_VOLUME_DOWN => Some("Volume -"),
        VK_VOLUME_MUTE => Some("Mudo"),
        VK_BROWSER_HOME => Some("Navegador: Início"),
        VK_BROWSER_BACK => Some("Navegador: Voltar"),
        VK_BROWSER_FORWARD => Some("Navegador: Avançar"),
        VK_BROWSER_SEARCH => Some("Navegador: Pesquisar"),
        VK_BROWSER_FAVORITES => Some("Navegador: Favoritos"),
        VK_BROWSER_REFRESH => Some("Navegador: Atualizar"),
        VK_LAUNCH_MAIL => Some("E-mail"),
        VK_LAUNCH_APP1 => Some("App 1"),
        VK_LAUNCH_APP2 => Some("Calculadora"),
        _ => None,
    };
    if let Some(name) = fixed {
        return name.to_string();
    }
    if (VK_F1..=VK_F24).contains(&key.vk) {
        return format!("F{}", key.vk - VK_F1 + 1);
    }

    // GetKeyNameText: bits 16-23 = scan code, bit 24 = tecla estendida.
    let lparam = (((key.scan & 0xFF) << 16) | ((key.extended as u32) << 24)) as i32;
    let mut buf = [0u16; 64];
    let len = unsafe { GetKeyNameTextW(lparam, buf.as_mut_ptr(), buf.len() as i32) };
    if len > 0 {
        let name = String::from_utf16_lossy(&buf[..len as usize]);
        // Deixa "INSERT" como "Insert", mas preserva nomes curtos como "F5" ou "Ç".
        let mut chars = name.chars();
        match chars.next() {
            Some(first) if name.chars().count() > 3 => {
                first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
            }
            _ => name,
        }
    } else {
        format!("Tecla 0x{:02X}", key.vk)
    }
}
