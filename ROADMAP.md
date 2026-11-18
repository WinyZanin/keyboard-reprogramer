# Roadmap

Known issues, observations and planned improvements. Items move to
[`CHANGELOG.md`](CHANGELOG.md) once they ship.

## Security

- [ ] **Validate the About link before opening it.** The About dialog opens the clicked
  `href` with `ShellExecuteW`. Today the only link is the project URL, compiled in from
  `Cargo.toml`, so nothing external can change it. But a future translation in `src/i18n.rs`
  could add an `<a href>` that points to a file or another URI scheme (e.g. `ms-msdt:`), and it
  would be opened without any check. Fix: in `about_callback` (`src/gui.rs`), open only the
  project URL (or at least only `https://` links) and ignore anything else.
- [ ] **Don't launch the browser elevated.** The README suggests running the app as
  administrator so the hook also works in elevated windows. In that case, clicking the About
  link can start the browser with administrator rights, if it isn't already open, and browsing
  as admin is unsafe. Fix: when the process is elevated, don't open the link; show it as text
  or copy it to the clipboard instead.
- [ ] **Pin GitHub Actions to commit SHAs** instead of tags (`@v4`, `@v5`), and enable
  Dependabot for `github-actions` and `cargo` to keep them up to date. This protects the release
  pipeline against a compromised or retagged third-party action.

## Features

- [ ] **Save the configuration** (keys, actions, enabled state and language) between sessions.
  Today everything is lost when the app closes.
- [ ] **Key combinations** (e.g. Ctrl+F1). Today only single keys can be remapped.
- [ ] **Start with Windows** (optional), probably together with running minimized to the
  notification area (tray icon).

## Known limitations

- **Elevated windows (UIPI).** In programs running as administrator, Windows doesn't let the
  hook block the key unless the app runs as administrator too. See also *Don't launch the
  browser elevated* above.

## Distribution and releases

- [ ] **Code signing (Authenticode).** The installers and the exe aren't signed, so Windows
  SmartScreen warns on first run. Signing needs a certificate (e.g. Azure Trusted Signing or an
  OV/EV certificate) and a signing step in `.github/workflows/package.yml`.
- [ ] **Translate the MSI wizard.** The MSI UI is English only; the Inno Setup installer already
  has English and Portuguese. WiX supports localization files (`.wxl`) per language.
- [ ] **CI on the release PR needs manual approval.** The release PR is opened by
  `github-actions[bot]`; GitHub creates its CI run but holds it until a maintainer clicks
  *Approve workflows to run*. Without approval, the post-merge CI run on `main` is the only check
  (and `build` / `pr-title` can't be required checks). Option: give release-please a GitHub App
  token or a fine-grained PAT, and make sure the tag it creates doesn't also trigger
  `package.yml` a second time.
- [ ] **Revisit WiX 6.** WiX is pinned to 5.0.2 in `tools/package.ps1` and the workflows. WiX 6
  requires accepting the Open Source Maintenance Fee EULA; check the terms before upgrading.

## Notes

- Betas (`vX.Y.Z-beta.N`) ship only the portable exe, by design. Installers, MSI, zip and
  checksums are built only for final releases.
- Never change the `AppId` (Inno Setup) or `UpgradeCode` (WiX) GUIDs: they link versions
  together for upgrades and uninstall.
