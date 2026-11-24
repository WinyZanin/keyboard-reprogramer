# Keyboard Reprogramer

**English** | [Português (Brasil)](README.pt-BR.md)

<img src="assets/icon.png" width="96" alt="Icon">

Windows app, written in Rust, that reprograms rarely used keys (e.g. **Insert**) to perform **media control actions**. Useful on keyboards without media keys.

While the app is open, the chosen key **no longer performs its original function** and triggers the media action instead. When the app is closed, the key goes back to normal.

## Installation

Download the latest (non pre-release) version from the project's GitHub **Releases** page (each file is listed in `SHA256SUMS.txt`):

| File | Use |
| --- | --- |
| `keyboard-reprogramer-X.Y.Z-x64-setup.exe` | Recommended installer (English/Portuguese wizard). |
| `keyboard-reprogramer-X.Y.Z-x64.msi` | MSI for managed or silent deployment (English UI). |
| `keyboard-reprogramer-X.Y.Z-x64-portable.zip` | Portable exe, no installation. |

Both installers install **per user by default** (no administrator rights, in `%LOCALAPPDATA%\Programs\Keyboard Reprogramer`), add a Start Menu shortcut and an optional desktop shortcut, and appear in *Settings → Apps* for uninstalling. Installing a newer version upgrades the existing one in place. If the app is running, the installer asks to close it first.

Silent install:

```sh
keyboard-reprogramer-X.Y.Z-x64-setup.exe /VERYSILENT /CURRENTUSER [/TASKS=desktopicon]
msiexec /i keyboard-reprogramer-X.Y.Z-x64.msi /qn [DESKTOPSHORTCUT=1] [ALLUSERS=1]
```

`/ALLUSERS` (setup.exe) or `ALLUSERS=1` (MSI) installs for all users under Program Files, which requires administrator rights.

The installers aren't code-signed yet, so Windows SmartScreen may warn on first run (*More info → Run anyway*).

## Usage

1. Run `keyboard-reprogramer.exe`.
2. Click **+ Add**. A new row appears, already waiting: **press the key** you want to reprogram (any physical key, including Print Screen, Pause, Menu or numeric keypad keys). **Esc** cancels.
3. Choose the media action in the row's menu: Play/Pause, Next track, Previous track, Stop, Volume +, Volume - or Mute.
4. Repeat for as many keys as you want. In each row:
   - Click the key button to change it to another key.
   - **Disable/Enable** turns only that key off and on.
   - **Delete** removes the row.
5. **Disable all**, at the top, returns every key to normal without losing the rows' configuration; **Enable all** turns them back on.
6. The history shows every trigger. The keys stay remapped while the app runs, also with the window hidden in the notification area (see below); **Options → Exit** returns all keys to normal.

The same key cannot be in two rows. Keys are identified by their physical position (scan code), so Insert and Num 0 are different keys, whether NumLock is on or not.

Volume +/- repeats while the key is held down. The other actions fire once per press.

Only one instance runs at a time: opening the app again brings the existing window to the front, even when it is hidden in the notification area. **Help → About** shows the installed version and the license.

## Notification area and options

While the app runs, its icon stays in the notification area (next to the clock; Windows may put it under the **^** overflow arrow). The tooltip shows the status, and the icon turns gray when no key is being remapped. Click the icon to open the window; right-click it for **Show window**, **Disable all / Enable all**, **Options** and **Exit**.

The **Options** menu (in the menu bar and in the icon's menu):

| Option | Default | What it does |
| --- | --- | --- |
| Start with Windows | off | Starts the app when you sign in to Windows. |
| Start minimized in the notification area | on | When started with Windows, starts with only the icon, no window. Opening the app yourself always shows the window. |
| Always start with remapping enabled | off | Starts with remapping on even if **Disable all** was on when the app closed. Off: restores the last state. |
| Close to the notification area | on | The window's **X** (and Alt+F4) hides the window instead of exiting. The first time, a notification says the app is still running. |
| Minimize to the notification area | off | Minimizing hides the window in the notification area instead of the taskbar. |

To really exit, use **Options → Exit** or **Exit** in the icon's menu.

**Start with Windows** adds a `Keyboard Reprogramer` entry to `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` (per user, no administrator rights). It shows up in **Task Manager → Startup apps** and in **Settings → Apps → Startup**, where it can also be disabled; the checkbox in the app follows that choice. If the portable exe is moved, the entry is updated the next time the app runs from the new place. Uninstalling removes the entry if it points to the uninstalled copy (`keyboard-reprogramer.exe --remove-autostart` does the same by hand). When started with Windows, the app runs without administrator rights, even if you usually run it as administrator.

## Configuration

The rows (key, action, enabled), the **Disable all** state, the language and the options (except **Start with Windows**, which lives in the registry) are saved automatically after every change and restored the next time the app opens. They are stored per user in:

```
%APPDATA%\Keyboard Reprogramer\config.toml
```

It is a plain [TOML](https://toml.io) file. To edit it by hand, close the app first; to reset everything, delete it. If the file is invalid, the app renames it to `config.toml.bak`, starts with an empty configuration and says so in the history. Rows whose key was never captured are not saved. Uninstalling keeps the file, so a reinstall picks up the same configuration.

## Language

The **Language** menu, below the title bar, switches the interface language instantly. Available: **English** and **Português (Brasil)**. On startup the app follows the Windows display language (Portuguese → Português, anything else → English). Once you pick a language it is saved, and the app opens in it from then on.

### Adding a language

All interface text lives in [`src/i18n.rs`](src/i18n.rs), as one `Strings` table per language:

1. Add a variant to `Lang` and to `Lang::ALL`, its name (in the language itself) to `native_name()` and its code (BCP 47, e.g. `es`) to `code()`.
2. Create a new `static` `Strings` table and return it from `Lang::strings()`.
3. Optionally, map the Windows language in `detect()`.

The tables are plain Rust structs, so a missing translation is a compile error.

## Interface

Uses the pure Win32 API (`CreateWindowExW` with the native `COMBOBOX`, `BUTTON`, `STATIC` and `LISTBOX` controls, a menu bar and a custom scrollable panel) through the `windows-sys` crate, with no GUI framework. Modern visual styles (Common Controls v6) are enabled at runtime with the `shell32.dll` manifest, so no resource compiler is needed. The app uses the system font and scales with DPI.

## How it works

- `SetWindowsHookExW(WH_KEYBOARD_LL)` installs a global hook that sees every key before it reaches applications.
- When the chosen key goes through the hook, the callback **returns 1**, dropping the event, and sends the media virtual key (`VK_MEDIA_PLAY_PAUSE`, `VK_VOLUME_UP`...) with `SendInput`.
- Keys sent by the app itself carry a tag (`dwExtraInfo`) and are ignored by the hook, which avoids recursion. Keys injected by other programs (on-screen keyboard, remote access, keyboard software) work normally.
- Key capture ("Press a key...") uses the regular messages of the focused window (`WM_KEYDOWN`/`WM_SYSKEYDOWN`), like the Windows Hotkey control. `lParam` carries the scan code and the extended flag. Meanwhile the hook is paused and passes everything through.
- Rules compare scan code + the `LLKHF_EXTENDED` flag, which identify the physical key.

## Building

Requires Rust (`winget install Rustlang.Rustup`). The GNU toolchain doesn't need Visual Studio Build Tools:

```sh
rustup default stable-x86_64-pc-windows-gnu
cargo build --release
```

Output: `target\release\keyboard-reprogramer.exe`, a single portable executable.

## Versioning and releases

The project follows [Semantic Versioning](https://semver.org) and [Conventional Commits](https://www.conventionalcommits.org/). The `version` in `Cargo.toml` is the single source of truth: `build.rs` writes it into the exe properties, **Help → About** shows it, and the packaging script passes it to both installers. Releases are automated by [release-please](https://github.com/googleapis/release-please):

1. **Every change goes through a branch and a pull request** into `main`. PRs are squash-merged, and the **PR title** must be a Conventional Commit (checked by CI; if the branch is named `<type>/<description>`, e.g. `feat/save-config`, CI sets the title to `feat: save config` automatically), because it becomes the commit message that decides the next version:
   - `fix: ...` → patch (0.1.0 → 0.1.1)
   - `feat: ...` → minor (0.1.0 → 0.2.0)
   - `feat!: ...` or a `BREAKING CHANGE:` footer → major (while below 1.0, a minor bump)
   - `docs:`, `chore:`, `ci:`, `refactor:`, `test:` → no release on their own
2. **On each merge into `main`**, release-please opens or updates a **release PR** (`chore(main): release X.Y.Z`) that bumps `Cargo.toml`/`Cargo.lock` and writes the `CHANGELOG.md` entry. At the same time a **beta** is published as a GitHub *pre-release* `vX.Y.Z-beta.N` (N = commits since the last release), containing only the portable exe.
3. **Merging the release PR** tags `vX.Y.Z` and publishes the final GitHub release with the setup.exe, MSI, portable zip and `SHA256SUMS.txt`. The betas of that version (and any older ones) are then deleted automatically, together with their tags.

Workflows: `.github/workflows/ci.yml` (build, clippy, PR title; runs on pull requests), `release-please.yml` (the *Main* workflow, one run per push to `main`: runs CI first, then the release PR, betas and final release) and `package.yml` (builds and attaches the final artifacts; also runs when a final tag `vX.Y.Z` is pushed by hand). Configuration: `release-please-config.json` and `.release-please-manifest.json` (last released version; release-please updates it).

To package locally, run `tools/package.ps1` (output in `dist\`). It needs [Inno Setup 6](https://jrsoftware.org/isinfo.php) (`winget install JRSoftware.InnoSetup`) and [WiX 5](https://wixtoolset.org) (`dotnet tool install --global wix --version 5.0.2`, then `wix extension add --global WixToolset.UI.wixext/5.0.2`); `-SkipInno` / `-SkipMsi` skip either one. Installer sources are in `installer/inno` and `installer/wix`; never change their `AppId` / `UpgradeCode` GUIDs, which link versions together for upgrades.

## Icon

`assets/icon.ico` (16 to 256 px) is generated by `python tools/make_icon.py` (requires Pillow). `build.rs` builds the resource section (`.rsrc`) with the icon and the version information from `Cargo.toml` directly in Rust and hands it to the linker. Neither `windres` nor `rc.exe` is needed.

## License

[MIT](LICENSE) © 2026 WinyZanin. The installers and the portable zip include a copy as `LICENSE.txt`.

## Limitations

- Only works while the app is open.
- One key at a time: combinations (e.g. Ctrl+F1) are not supported yet.
- In windows of programs running **as administrator**, Windows (UIPI) doesn't let the hook block the key. To cover those cases, run the `.exe` as administrator too.

Planned improvements, known issues and security notes are tracked in [`ROADMAP.md`](ROADMAP.md) (English).
