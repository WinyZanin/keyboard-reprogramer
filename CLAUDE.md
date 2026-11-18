# Conventions

- Write all code in English: identifiers, comments, doc comments, technical terms, error messages and commit messages.
- User-visible UI text never goes inline in the code: add it to the `Strings` tables in `src/i18n.rs` (every language) and read it via `i18n::t()`. Anything shown in the GUI must be re-applied in `gui::retranslate` when the language changes.
- `README.md` is the English version; `README.pt-BR.md` is the Portuguese version. Keep both in sync when changing documentation.
- Build with `cargo build --release`; keep `cargo clippy --release` warning-free.
