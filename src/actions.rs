use std::mem::size_of;

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
    VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE, VK_MEDIA_PREV_TRACK, VK_MEDIA_STOP, VK_VOLUME_DOWN,
    VK_VOLUME_MUTE, VK_VOLUME_UP,
};

/// Media action that replaces the key's original function.
/// Display names live in `i18n::Strings::action_names`, in the same order.
pub struct MediaAction {
    pub vk: u16,
    /// Repeats while the key is held down (useful for volume).
    pub repeat: bool,
}

pub const ACTION_COUNT: usize = 7;

pub const ACTIONS: [MediaAction; ACTION_COUNT] = [
    MediaAction { vk: VK_MEDIA_PLAY_PAUSE, repeat: false },
    MediaAction { vk: VK_MEDIA_NEXT_TRACK, repeat: false },
    MediaAction { vk: VK_MEDIA_PREV_TRACK, repeat: false },
    MediaAction { vk: VK_MEDIA_STOP, repeat: false },
    MediaAction { vk: VK_VOLUME_UP, repeat: true },
    MediaAction { vk: VK_VOLUME_DOWN, repeat: true },
    MediaAction { vk: VK_VOLUME_MUTE, repeat: false },
];

/// Tags events sent by the app itself (dwExtraInfo) so the hook ignores them
/// without ignoring other injected input (on-screen keyboard, automation, etc.).
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

/// Sends a full press (down + up) of the media virtual key to the system.
pub fn send(vk: u16) {
    let inputs = [key_input(vk, false), key_input(vk, true)];
    unsafe {
        SendInput(inputs.len() as u32, inputs.as_ptr(), size_of::<INPUT>() as i32);
    }
}
