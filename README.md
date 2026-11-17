# Keyboard Reprogramer

<img src="assets/icon.png" width="96" alt="Ícone">

Aplicativo para Windows, escrito em Rust, que reprograma uma tecla pouco usada (ex.: **Insert**) para executar uma **ação de controle de mídia**. É útil em teclados sem teclas de mídia.

Enquanto o app está aberto, a tecla escolhida **não executa mais a função original** e dispara a ação de mídia. Ao fechar o app, a tecla volta ao normal.

## Uso

1. Execute `keyboard-reprogramer.exe`.
2. Escolha a **Tecla** (Insert, Scroll Lock, Pause/Break, Print Screen, Home, End, Page Up, Page Down, Delete, Ctrl direito, Menu) e a **Ação** (Play/Pause, Próxima faixa, Faixa anterior, Stop, Volume +, Volume -, Mudo) e clique em **Adicionar**. Repita para remapear várias teclas ao mesmo tempo.
   - Se a tecla já estiver na lista, a ação dela é atualizada.
   - Clicar numa linha carrega a tecla e a ação dela nos seletores.
3. Com uma linha selecionada:
   - **Desativar/Ativar** (ou duplo clique ou Espaço) liga e desliga só aquela tecla, que volta à função original enquanto estiver desativada.
   - **Remover** (ou Delete) tira a tecla da lista.
4. O histórico mostra cada acionamento. Fechar a janela devolve todas as teclas ao normal.

Volume +/- repete enquanto a tecla está segurada. As outras ações disparam uma vez por pressionamento.

## Interface

Usa a API Win32 pura (`CreateWindowExW` com os controles nativos `COMBOBOX`, `BUTTON`, `STATIC` e `LISTBOX`) através do crate `windows-sys`, sem framework de GUI. Os estilos visuais modernos (Common Controls v6) são ativados em tempo de execução com o manifesto do `shell32.dll`, então não é preciso compilador de recursos. O app usa a fonte do sistema e ajusta a escala ao DPI.

## Como funciona

- `SetWindowsHookExW(WH_KEYBOARD_LL)` instala um hook global que vê todas as teclas antes de chegarem aos aplicativos.
- Quando a tecla escolhida passa pelo hook, o callback **retorna 1**, descartando o evento, e envia a tecla virtual de mídia (`VK_MEDIA_PLAY_PAUSE`, `VK_VOLUME_UP`...) com `SendInput`.
- Eventos com a flag `LLKHF_INJECTED` são ignorados, o que evita recursão.
- Teclas do bloco de navegação exigem `LLKHF_EXTENDED`: o Insert remapeado não afeta o Numpad 0 com o NumLock desligado.

## Compilar

Requer o Rust (`winget install Rustlang.Rustup`). O toolchain GNU dispensa o Visual Studio Build Tools:

```sh
rustup default stable-x86_64-pc-windows-gnu
cargo build --release
```

Saída: `target\release\keyboard-reprogramer.exe`, um executável único e portátil.

## Ícone

`assets/icon.ico` (16 a 256 px) é gerado por `python tools/make_icon.py` (requer Pillow). O `build.rs` monta a seção de recursos (`.rsrc`) com o ícone e as informações de versão do `Cargo.toml` direto em Rust e a entrega ao linker. Não é preciso `windres` nem `rc.exe`.

## Limitações

- Só funciona com o app aberto.
- Em janelas de programas executados **como administrador**, o Windows (UIPI) não deixa o hook bloquear a tecla. Para cobrir esses casos, execute o `.exe` também como administrador.
