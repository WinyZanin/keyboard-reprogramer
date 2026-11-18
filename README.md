# Keyboard Reprogramer

<img src="assets/icon.png" width="96" alt="Ícone">

Aplicativo para Windows, escrito em Rust, que reprograma uma tecla pouco usada (ex.: **Insert**) para executar uma **ação de controle de mídia**. É útil em teclados sem teclas de mídia.

Enquanto o app está aberto, a tecla escolhida **não executa mais a função original** e dispara a ação de mídia. Ao fechar o app, a tecla volta ao normal.

## Uso

1. Execute `keyboard-reprogramer.exe`.
2. Clique em **+ Adicionar**. Uma linha nova aparece já esperando: **aperte a tecla** que quer reprogramar (qualquer tecla física, inclusive Print Screen, Pause, Menu ou teclas do teclado numérico). **Esc** cancela.
3. Escolha a ação de mídia no menu da linha: Play/Pause, Próxima faixa, Faixa anterior, Stop, Volume +, Volume - ou Mudo.
4. Repita para quantas teclas quiser. Em cada linha:
   - Clique no botão da tecla para trocar por outra.
   - **Desativar/Ativar** liga e desliga só aquela tecla.
   - **Excluir** remove a linha.
5. **Desativar tudo**, no topo, devolve todas as teclas ao normal sem perder a configuração das linhas; **Ativar tudo** religa.
6. O histórico mostra cada acionamento. Fechar a janela devolve todas as teclas ao normal.

Uma mesma tecla não pode estar em duas linhas. As teclas são identificadas pela posição física (scan code), então o Insert e o Num 0 são teclas diferentes, com o NumLock ligado ou não.

Volume +/- repete enquanto a tecla está segurada. As outras ações disparam uma vez por pressionamento.

## Interface

Usa a API Win32 pura (`CreateWindowExW` com os controles nativos `COMBOBOX`, `BUTTON`, `STATIC` e `LISTBOX` e um painel rolável próprio) através do crate `windows-sys`, sem framework de GUI. Os estilos visuais modernos (Common Controls v6) são ativados em tempo de execução com o manifesto do `shell32.dll`, então não é preciso compilador de recursos. O app usa a fonte do sistema e ajusta a escala ao DPI.

## Como funciona

- `SetWindowsHookExW(WH_KEYBOARD_LL)` instala um hook global que vê todas as teclas antes de chegarem aos aplicativos.
- Quando a tecla escolhida passa pelo hook, o callback **retorna 1**, descartando o evento, e envia a tecla virtual de mídia (`VK_MEDIA_PLAY_PAUSE`, `VK_VOLUME_UP`...) com `SendInput`.
- As teclas que o próprio app envia levam uma marca (`dwExtraInfo`) e são ignoradas pelo hook, o que evita recursão. Teclas injetadas por outros programas (teclado virtual, acesso remoto, software do teclado) funcionam normalmente.
- A captura da tecla ("Pressione uma tecla...") usa as mensagens normais da janela em foco (`WM_KEYDOWN`/`WM_SYSKEYDOWN`), como o controle Hotkey do Windows. O `lParam` traz o scan code e a flag extended. Enquanto isso, o hook fica em pausa e repassa tudo.
- As regras comparam scan code + flag `LLKHF_EXTENDED`, que identificam a tecla física.

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

- Só funciona com o app aberto, e a configuração ainda não é salva ao fechar.
- Uma tecla por vez: combinações (ex.: Ctrl+F1) ainda não são suportadas.
- Em janelas de programas executados **como administrador**, o Windows (UIPI) não deixa o hook bloquear a tecla. Para cobrir esses casos, execute o `.exe` também como administrador.
