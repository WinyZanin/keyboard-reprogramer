//! "Start with Windows" through the per-user `Run` registry key.
//!
//! The registry is the source of truth (nothing is stored in `config.toml`): the app writes
//! `"<exe>" --autostart` to `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`. Task Manager
//! lists that entry under *Startup apps* (name and icon come from the exe's version resource),
//! and disabling it there doesn't delete it: Windows records the choice in
//! `...\Explorer\StartupApproved\Run`. `is_enabled` honors both, so a change made in Task
//! Manager shows up in the app's menus.

use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows_sys::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_BINARY, RRF_RT_REG_SZ,
};

/// Command-line flag of the `Run` entry: tells the app it was started at logon.
pub const ARG_AUTOSTART: &str = "--autostart";
/// Removes the `Run` entry and exits (used by the uninstallers).
pub const ARG_REMOVE: &str = "--remove-autostart";

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APPROVED_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
/// Value name in both keys (also what Task Manager falls back to without a version resource).
const VALUE_NAME: &str = "Keyboard Reprogramer";

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Full path of the running exe.
fn exe_path() -> Result<String, String> {
    let mut buf = vec![0u16; 1024];
    loop {
        let len = unsafe { GetModuleFileNameW(null_mut(), buf.as_mut_ptr(), buf.len() as u32) } as usize;
        if len == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        if len < buf.len() {
            return Ok(String::from_utf16_lossy(&buf[..len]));
        }
        // Truncated: retry with a bigger buffer (long paths).
        buf.resize(buf.len() * 2, 0);
    }
}

/// The command written to the `Run` key for this exe.
fn command() -> Result<String, String> {
    Ok(format!("\"{}\" {ARG_AUTOSTART}", exe_path()?))
}

/// Reads a value from HKCU. `None` if it doesn't exist (or can't be read).
fn read(key: &str, flags: u32) -> Option<Vec<u8>> {
    let (key, name) = (w(key), w(VALUE_NAME));
    let mut size = 0u32;
    unsafe {
        let status = RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), flags, null_mut(), null_mut(), &mut size);
        if status != ERROR_SUCCESS {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        let status = RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            flags,
            null_mut(),
            data.as_mut_ptr().cast(),
            &mut size,
        );
        if status != ERROR_SUCCESS {
            return None;
        }
        data.truncate(size as usize);
        Some(data)
    }
}

fn read_string(key: &str) -> Option<String> {
    let bytes = read(key, RRF_RT_REG_SZ)?;
    let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|&c| u16::from_le_bytes(c)).collect();
    let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
    Some(String::from_utf16_lossy(&units[..end]))
}

/// Deletes a value from HKCU; a missing value (or key) isn't an error.
fn delete(key: &str) -> Result<(), String> {
    let (key, name) = (w(key), w(VALUE_NAME));
    let status = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr()) };
    match status {
        ERROR_SUCCESS | ERROR_FILE_NOT_FOUND => Ok(()),
        e => Err(std::io::Error::from_raw_os_error(e as i32).to_string()),
    }
}

fn write_run(command: &str) -> Result<(), String> {
    let (key, name, data) = (w(RUN_KEY), w(VALUE_NAME), w(command));
    let status = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            REG_SZ,
            data.as_ptr().cast(),
            (data.len() * 2) as u32,
        )
    };
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(std::io::Error::from_raw_os_error(status as i32).to_string())
    }
}

/// `StartupApproved` data: the low bit of the first byte set (e.g. 0x03, 0x07) means the user
/// disabled the entry in Task Manager or Settings; 0x02/0x06 mean enabled.
fn approved_disabled(data: &[u8]) -> bool {
    data.first().is_some_and(|b| b & 1 != 0)
}

/// True if the app starts with Windows: the `Run` entry exists and wasn't disabled in Task
/// Manager.
pub fn is_enabled() -> bool {
    read_string(RUN_KEY).is_some() && !read(APPROVED_KEY, RRF_RT_REG_BINARY).is_some_and(|d| approved_disabled(&d))
}

/// Turns starting with Windows on or off. Turning it on also clears a "disabled" mark left by
/// Task Manager, so the app's checkbox always wins.
pub fn set(enabled: bool) -> Result<(), String> {
    if enabled {
        write_run(&command()?)?;
        delete(APPROVED_KEY)
    } else {
        delete(RUN_KEY)?;
        delete(APPROVED_KEY)
    }
}

/// Removes the entry only if it starts this exe, so uninstalling one copy doesn't turn off
/// another one (e.g. a portable exe) that the user set to start with Windows.
pub fn remove_if_own() -> Result<(), String> {
    match read_string(RUN_KEY) {
        Some(current) if current.eq_ignore_ascii_case(&command()?) => set(false),
        _ => Ok(()),
    }
}

/// If the `Run` entry exists but points to another path (the portable exe was moved, or the
/// app was reinstalled elsewhere), points it to this exe. Keeps a Task Manager "disabled" mark.
pub fn refresh_path() -> Result<(), String> {
    let Some(current) = read_string(RUN_KEY) else { return Ok(()) };
    let wanted = command()?;
    if current.eq_ignore_ascii_case(&wanted) {
        return Ok(());
    }
    write_run(&wanted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approved_flags() {
        assert!(!approved_disabled(&[0x02, 0, 0, 0]));
        assert!(!approved_disabled(&[0x06, 0, 0, 0]));
        assert!(approved_disabled(&[0x03, 0, 0, 0]));
        assert!(approved_disabled(&[0x07, 0, 0, 0]));
        assert!(!approved_disabled(&[]));
    }
}
