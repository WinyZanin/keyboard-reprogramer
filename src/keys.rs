//! Human-readable name of a physical key.

use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

use crate::hook::Key;
use crate::i18n::t;

/// Key name. Most names come from `GetKeyNameTextW`, in the language of the Windows
/// keyboard layout (e.g. "INSERT", "NUM 0"); the rest come from the UI translation.
pub fn name(key: &Key) -> String {
    let s = t();
    // Keys that GetKeyNameText names wrong or doesn't name at all.
    let fixed = match key.vk {
        VK_PAUSE => Some("Pause/Break"),
        VK_CANCEL => Some("Break"),
        VK_SNAPSHOT => Some("Print Screen"),
        VK_NUMLOCK => Some("Num Lock"),
        VK_LWIN => Some(s.key_win_left),
        VK_RWIN => Some(s.key_win_right),
        VK_APPS => Some(s.key_menu),
        VK_MEDIA_PLAY_PAUSE => Some(s.key_media_play_pause),
        VK_MEDIA_NEXT_TRACK => Some(s.key_media_next),
        VK_MEDIA_PREV_TRACK => Some(s.key_media_prev),
        VK_MEDIA_STOP => Some(s.key_media_stop),
        VK_VOLUME_UP => Some(s.key_volume_up),
        VK_VOLUME_DOWN => Some(s.key_volume_down),
        VK_VOLUME_MUTE => Some(s.key_volume_mute),
        VK_BROWSER_HOME => Some(s.key_browser_home),
        VK_BROWSER_BACK => Some(s.key_browser_back),
        VK_BROWSER_FORWARD => Some(s.key_browser_forward),
        VK_BROWSER_SEARCH => Some(s.key_browser_search),
        VK_BROWSER_FAVORITES => Some(s.key_browser_favorites),
        VK_BROWSER_REFRESH => Some(s.key_browser_refresh),
        VK_LAUNCH_MAIL => Some(s.key_mail),
        VK_LAUNCH_APP1 => Some(s.key_app1),
        VK_LAUNCH_APP2 => Some(s.key_calculator),
        _ => None,
    };
    if let Some(name) = fixed {
        return name.to_string();
    }
    if (VK_F1..=VK_F24).contains(&key.vk) {
        return format!("F{}", key.vk - VK_F1 + 1);
    }

    // GetKeyNameText: bits 16-23 = scan code, bit 24 = extended key.
    let lparam = (((key.scan & 0xFF) << 16) | ((key.extended as u32) << 24)) as i32;
    let mut buf = [0u16; 64];
    let len = unsafe { GetKeyNameTextW(lparam, buf.as_mut_ptr(), buf.len() as i32) };
    if len > 0 {
        let name = String::from_utf16_lossy(&buf[..len as usize]);
        // Turns "INSERT" into "Insert", but keeps short names such as "F5" or "Ç".
        let mut chars = name.chars();
        match chars.next() {
            Some(first) if name.chars().count() > 3 => {
                first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
            }
            _ => name,
        }
    } else {
        (s.key_unknown)(key.vk)
    }
}
