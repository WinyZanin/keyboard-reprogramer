# Keyboard Reprogramer

Aplicativo de console para Windows, escrito em Rust, que reprograma uma tecla pouco usada (ex.: **Insert**) para executar uma **ação de controle de mídia**. É útil em teclados sem teclas de mídia.

Enquanto o app está aberto, a tecla escolhida **não executa mais a função original** e dispara a ação de mídia. Ao fechar o app, a tecla volta ao normal.

## Uso

1. Execute `keyboard-reprogramer.exe`.
2. Escolha a tecla: Insert, Scroll Lock, Pause/Break, Print Screen, Home, End, Page Up, Page Down, Delete, Ctrl direito ou Menu.
3. Escolha a ação: Play/Pause, Próxima faixa, Faixa anterior, Stop, Volume +, Volume - ou Mudo.
4. Deixe a janela aberta. Para sair, use **Ctrl+C** ou feche a janela.

Volume +/- repete enquanto a tecla está segurada. As outras ações disparam uma vez por pressionamento.

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

## Limitações

- Só funciona com o app aberto.
- Em janelas de programas executados **como administrador**, o Windows (UIPI) não deixa o hook bloquear a tecla. Para cobrir esses casos, execute o `.exe` também como administrador.
