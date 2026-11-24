//! Configuration persisted between sessions.
//!
//! Stored as TOML in `%APPDATA%\Keyboard Reprogramer\config.toml` (per user). The GUI saves it
//! after every change, so nothing is lost if the process is killed. Writes are atomic: the
//! text goes to `config.toml.tmp`, which then replaces `config.toml`.
//!
//! Actions and the language are stored by stable string ids (`MediaAction::id`,
//! `Lang::code`), never by index, so reordering those lists doesn't break existing files.
//! Unknown fields are ignored and missing fields get their defaults.

use std::ffi::c_void;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::PathBuf;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::UI::Shell::{FOLDERID_RoamingAppData, SHGetKnownFolderPath, KF_FLAG_DEFAULT};

use crate::hook::Key;

/// Current file format. Bump it (and migrate in `load`) on incompatible changes.
pub const VERSION: u32 = 1;

const APP_DIR: &str = "Keyboard Reprogramer";
const FILE_NAME: &str = "config.toml";
const HEADER: &str = "# Keyboard Reprogramer configuration, saved automatically by the app.\n\
                      # Close the app before editing it by hand; delete it to reset.\n\n";

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    /// `Lang::code`; missing or unknown = follow the Windows display language.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Master switch.
    pub enabled: bool,
    pub options: Options,
    #[serde(rename = "rule")]
    pub rules: Vec<RuleConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Config { version: VERSION, language: None, enabled: true, options: Options::default(), rules: Vec::new() }
    }
}

/// App behavior options (Options menu). "Start with Windows" isn't here: it lives in the
/// registry (see `autostart`), so Task Manager can change it.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(default)]
pub struct Options {
    /// Closing the window hides it in the notification area instead of exiting.
    pub close_to_tray: bool,
    /// Minimizing the window hides it in the notification area instead of the taskbar.
    pub minimize_to_tray: bool,
    /// When started with Windows (`--autostart`), start hidden in the notification area.
    pub start_minimized: bool,
    /// Always start with remapping on, instead of restoring the last master switch state.
    pub start_enabled: bool,
    /// The one-time "still running in the notification area" notice was already shown.
    pub tray_hint_shown: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            close_to_tray: true,
            minimize_to_tray: false,
            start_minimized: true,
            start_enabled: false,
            tray_hint_shown: false,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct RuleConfig {
    /// `MediaAction::id`.
    pub action: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub key: KeyConfig,
}

/// Physical key, as in `hook::Key`.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct KeyConfig {
    pub vk: u16,
    #[serde(default)]
    pub scan: u32,
    #[serde(default)]
    pub extended: bool,
}

impl From<Key> for KeyConfig {
    fn from(k: Key) -> Self {
        KeyConfig { vk: k.vk, scan: k.scan, extended: k.extended }
    }
}

impl From<KeyConfig> for Key {
    fn from(k: KeyConfig) -> Self {
        Key { vk: k.vk, scan: k.scan, extended: k.extended }
    }
}

fn default_true() -> bool {
    true
}

pub enum LoadError {
    /// The file isn't valid TOML or doesn't match the format. It was renamed to `.bak`.
    Invalid(String),
    /// The file couldn't be read (or the folder couldn't be found). Saving is disabled so
    /// the existing file isn't overwritten.
    Read(String),
}

/// False when saving would destroy data: unreadable file or one written by a newer version.
static WRITABLE: AtomicBool = AtomicBool::new(true);
/// Last text written, to skip writes when nothing changed.
static LAST_SAVED: Mutex<Option<String>> = Mutex::new(None);

/// `%APPDATA%\Keyboard Reprogramer\config.toml`.
fn path() -> io::Result<PathBuf> {
    unsafe {
        let mut raw = null_mut();
        let hr = SHGetKnownFolderPath(&FOLDERID_RoamingAppData, KF_FLAG_DEFAULT as u32, null_mut(), &mut raw);
        // The buffer must be freed even when the call fails.
        let result = if hr >= 0 && !raw.is_null() {
            let len = (0..).take_while(|&i| *raw.add(i) != 0).count();
            Ok(String::from_utf16_lossy(std::slice::from_raw_parts(raw, len)))
        } else {
            Err(io::Error::from_raw_os_error(hr))
        };
        CoTaskMemFree(raw as *const c_void);
        Ok(PathBuf::from(result?).join(APP_DIR).join(FILE_NAME))
    }
}

/// Parses the file text. The error is a short, single-line description.
fn parse(text: &str) -> Result<Config, String> {
    toml::from_str(text).map_err(|e| match e.span() {
        Some(span) => {
            let line = text[..span.start.min(text.len())].matches('\n').count() + 1;
            format!("{line}: {}", e.message().trim())
        }
        None => e.message().trim().to_string(),
    })
}

/// Reads the saved configuration. `Ok(None)` = no file yet (first run).
pub fn load() -> Result<Option<Config>, LoadError> {
    let fail = |e: io::Error| {
        WRITABLE.store(false, Ordering::Relaxed);
        LoadError::Read(e.to_string())
    };
    let path = path().map_err(fail)?;
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(fail(e)),
    };
    match parse(&text) {
        Ok(config) => {
            if config.version > VERSION {
                WRITABLE.store(false, Ordering::Relaxed);
            }
            Ok(Some(config))
        }
        Err(detail) => {
            // Keeps the broken file for the user instead of overwriting it on the next save.
            fs::rename(&path, path.with_extension("toml.bak")).map_err(fail)?;
            Err(LoadError::Invalid(detail))
        }
    }
}

/// Writes the configuration if it changed. Does nothing when saving is disabled (see
/// `LoadError::Read` and newer versions).
pub fn save(config: &Config) -> Result<(), String> {
    if !WRITABLE.load(Ordering::Relaxed) {
        return Ok(());
    }
    let text = HEADER.to_string() + &toml::to_string(config).map_err(|e| e.to_string())?;
    let mut last = LAST_SAVED.lock().unwrap_or_else(|e| e.into_inner());
    if last.as_deref() == Some(text.as_str()) {
        return Ok(());
    }
    write_atomic(&text).map_err(|e| e.to_string())?;
    *last = Some(text);
    Ok(())
}

fn write_atomic(text: &str) -> io::Result<()> {
    let path = path()?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("toml.tmp");
    let mut file = File::create(&tmp)?;
    file.write_all(text.as_bytes())?;
    // Flushes to disk before the rename, so a crash leaves either the old or the new file.
    file.sync_all()?;
    drop(file);
    // On Windows this is MoveFileExW(MOVEFILE_REPLACE_EXISTING).
    fs::rename(&tmp, &path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Config {
        Config {
            version: VERSION,
            language: Some("pt-BR".into()),
            enabled: false,
            options: Options {
                close_to_tray: false,
                minimize_to_tray: true,
                start_minimized: false,
                start_enabled: true,
                tray_hint_shown: true,
            },
            rules: vec![
                RuleConfig {
                    action: "play_pause".into(),
                    enabled: true,
                    key: KeyConfig { vk: 0x2D, scan: 0x52, extended: true },
                },
                RuleConfig {
                    action: "volume_up".into(),
                    enabled: false,
                    key: KeyConfig { vk: 0x91, scan: 0x46, extended: false },
                },
            ],
        }
    }

    #[test]
    fn round_trip() {
        let text = HEADER.to_string() + &toml::to_string(&sample()).unwrap();
        assert_eq!(parse(&text).unwrap(), sample());
    }

    #[test]
    fn missing_and_unknown_fields() {
        let text = "future = 1\n[[rule]]\naction = \"mute\"\nkey = { vk = 19 }\n";
        let config = parse(text).unwrap();
        assert_eq!(config.version, VERSION);
        assert_eq!(config.language, None);
        assert!(config.enabled);
        assert_eq!(config.options, Options::default());
        assert_eq!(
            config.rules,
            vec![RuleConfig { action: "mute".into(), enabled: true, key: KeyConfig { vk: 19, scan: 0, extended: false } }]
        );
    }

    #[test]
    fn partial_options() {
        let config = parse("[options]\nstart_enabled = true\n").unwrap();
        assert_eq!(config.options, Options { start_enabled: true, ..Options::default() });
    }

    #[test]
    fn empty_file_is_default() {
        assert_eq!(parse("").unwrap(), Config::default());
    }

    #[test]
    fn invalid_reports_line() {
        let err = parse("version = 1\nenabled = maybe\n").unwrap_err();
        assert!(err.starts_with("2: "), "{err}");
    }
}
