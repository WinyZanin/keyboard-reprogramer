use std::mem::size_of;

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
    VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE, VK_MEDIA_PREV_TRACK, VK_MEDIA_STOP, VK_VOLUME_DOWN,
    VK_VOLUME_MUTE, VK_VOLUME_UP,
};

/// Ação de mídia que substitui a função original da tecla.
pub struct MediaAction {
    pub name: &'static str,
    pub vk: u16,
    /// Repete enquanto a tecla é segurada (útil para volume).
    pub repeat: bool,
}

pub const ACTIONS: &[MediaAction] = &[
    MediaAction { name: "Play/Pause", vk: VK_MEDIA_PLAY_PAUSE, repeat: false },
    MediaAction { name: "Próxima faixa", vk: VK_MEDIA_NEXT_TRACK, repeat: false },
    MediaAction { name: "Faixa anterior", vk: VK_MEDIA_PREV_TRACK, repeat: false },
    MediaAction { name: "Stop", vk: VK_MEDIA_STOP, repeat: false },
    MediaAction { name: "Volume +", vk: VK_VOLUME_UP, repeat: true },
    MediaAction { name: "Volume -", vk: VK_VOLUME_DOWN, repeat: true },
    MediaAction { name: "Mudo", vk: VK_VOLUME_MUTE, repeat: false },
];

/// Marca os eventos enviados pelo próprio app (dwExtraInfo), para o hook ignorá-los
/// sem ignorar outras entradas injetadas (teclado virtual, automação etc.).
pub const SELF_INJECTED_TAG: usize = 0x4B52_4D50; // "KRMP"

fn key_input(vk: u16, up: bool) -> INPUT {
    let mut flags = KEYEVENTF_EXTENDEDKEY;
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: SELF_INJECTED_TAG },
        },
    }
}

/// Envia ao sistema um pressionamento completo (down + up) da tecla virtual de mídia.
pub fn send(vk: u16) {
    let inputs = [key_input(vk, false), key_input(vk, true)];
    unsafe {
        SendInput(inputs.len() as u32, inputs.as_ptr(), size_of::<INPUT>() as i32);
    }
}
