# Keyboard Reprogramer

[English](README.md) | **Português (Brasil)**

<img src="assets/icon.png" width="96" alt="Ícone">

Aplicativo para Windows, escrito em Rust, que reprograma teclas pouco usadas (ex.: **Insert**) para executar **ações de controle de mídia**. É útil em teclados sem teclas de mídia.

Enquanto o app está aberto, a tecla escolhida **não executa mais a função original** e dispara a ação de mídia. Ao fechar o app, a tecla volta ao normal.

## Instalação

Baixe a versão mais recente (que não seja pre-release) na página **Releases** do projeto no GitHub (cada arquivo está listado em `SHA256SUMS.txt`):

| Arquivo | Uso |
| --- | --- |
| `keyboard-reprogramer-X.Y.Z-x64-setup.exe` | Instalador recomendado (assistente em inglês/português). |
| `keyboard-reprogramer-X.Y.Z-x64.msi` | MSI para implantação gerenciada ou silenciosa (interface em inglês). |
| `keyboard-reprogramer-X.Y.Z-x64-portable.zip` | Exe portátil, sem instalação. |

Os dois instaladores instalam **por usuário por padrão** (sem direitos de administrador, em `%LOCALAPPDATA%\Programs\Keyboard Reprogramer`), criam um atalho no Menu Iniciar e um atalho opcional na área de trabalho, e aparecem em *Configurações → Aplicativos* para desinstalar. Instalar uma versão mais nova atualiza a existente no lugar. Se o app estiver aberto, o instalador pede para fechá-lo antes.

Instalação silenciosa:

```sh
keyboard-reprogramer-X.Y.Z-x64-setup.exe /VERYSILENT /CURRENTUSER [/TASKS=desktopicon]
msiexec /i keyboard-reprogramer-X.Y.Z-x64.msi /qn [DESKTOPSHORTCUT=1] [ALLUSERS=1]
```

`/ALLUSERS` (setup.exe) ou `ALLUSERS=1` (MSI) instala para todos os usuários em Arquivos de Programas, o que exige direitos de administrador.

Os instaladores ainda não têm assinatura de código, então o SmartScreen do Windows pode avisar na primeira execução (*Mais informações → Executar assim mesmo*).

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

Só uma instância roda por vez: abrir o app de novo traz a janela existente para a frente. **Ajuda → Sobre** mostra a versão instalada.

## Idioma

O menu **Idioma**, abaixo da barra de título, troca o idioma da interface na hora. Disponíveis: **English** e **Português (Brasil)**. Ao abrir, o app segue o idioma de exibição do Windows (português → Português, qualquer outro → English). A escolha ainda não é salva.

### Adicionar um idioma

Todo o texto da interface fica em [`src/i18n.rs`](src/i18n.rs), em uma tabela `Strings` por idioma:

1. Adicione uma variante em `Lang` e em `Lang::ALL`, e o nome dela (no próprio idioma) em `native_name()`.
2. Crie uma nova tabela `static` `Strings` e retorne-a em `Lang::strings()`.
3. Opcionalmente, mapeie o idioma do Windows em `detect()`.

As tabelas são structs Rust comuns, então uma tradução faltando é erro de compilação.

## Interface

Usa a API Win32 pura (`CreateWindowExW` com os controles nativos `COMBOBOX`, `BUTTON`, `STATIC` e `LISTBOX`, uma barra de menu e um painel rolável próprio) através do crate `windows-sys`, sem framework de GUI. Os estilos visuais modernos (Common Controls v6) são ativados em tempo de execução com o manifesto do `shell32.dll`, então não é preciso compilador de recursos. O app usa a fonte do sistema e ajusta a escala ao DPI.

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

## Versionamento e releases

O projeto segue o [Versionamento Semântico](https://semver.org/lang/pt-BR/) e os [Conventional Commits](https://www.conventionalcommits.org/pt-br/). A `version` do `Cargo.toml` é a única fonte da verdade: o `build.rs` grava ela nas propriedades do exe, **Ajuda → Sobre** mostra, e o script de empacotamento repassa aos dois instaladores. As releases são automatizadas pelo [release-please](https://github.com/googleapis/release-please):

1. **Toda mudança passa por uma branch e um pull request** para a `main`. Os PRs são mesclados com squash, e o **título do PR** precisa ser um Conventional Commit (verificado pelo CI), porque vira a mensagem de commit que decide a próxima versão:
   - `fix: ...` → patch (0.1.0 → 0.1.1)
   - `feat: ...` → minor (0.1.0 → 0.2.0)
   - `feat!: ...` ou um rodapé `BREAKING CHANGE:` → major (enquanto abaixo de 1.0, sobe o minor)
   - `docs:`, `chore:`, `ci:`, `refactor:`, `test:` → sozinhos não geram release
2. **A cada merge na `main`**, o release-please abre ou atualiza um **PR de versão** (`chore(main): release X.Y.Z`) que sobe a versão no `Cargo.toml`/`Cargo.lock` e escreve a entrada do `CHANGELOG.md`. Ao mesmo tempo é publicado um **beta** como *pre-release* do GitHub `vX.Y.Z-beta.N` (N = commits desde a última release), só com o exe portátil.
3. **Mesclar o PR de versão** cria a tag `vX.Y.Z` e publica a release final no GitHub com o setup.exe, o MSI, o zip portátil e o `SHA256SUMS.txt`. Em seguida os betas dessa versão (e os mais antigos) são apagados automaticamente, junto com as tags.

Workflows: `.github/workflows/ci.yml` (build, clippy, título do PR), `release-please.yml` (PR de versão, betas, release final) e `package.yml` (gera e anexa os artefatos finais; também roda quando uma tag final `vX.Y.Z` é enviada à mão). Configuração: `release-please-config.json` e `.release-please-manifest.json` (última versão lançada; o release-please atualiza).

Para empacotar localmente, rode `tools/package.ps1` (saída em `dist\`). Ele precisa do [Inno Setup 6](https://jrsoftware.org/isinfo.php) (`winget install JRSoftware.InnoSetup`) e do [WiX 5](https://wixtoolset.org) (`dotnet tool install --global wix --version 5.0.2`, depois `wix extension add --global WixToolset.UI.wixext/5.0.2`); `-SkipInno` / `-SkipMsi` pulam um deles. Os fontes dos instaladores ficam em `installer/inno` e `installer/wix`; nunca altere os GUIDs `AppId` / `UpgradeCode`, que ligam as versões entre si para as atualizações.

## Ícone

`assets/icon.ico` (16 a 256 px) é gerado por `python tools/make_icon.py` (requer Pillow). O `build.rs` monta a seção de recursos (`.rsrc`) com o ícone e as informações de versão do `Cargo.toml` direto em Rust e a entrega ao linker. Não é preciso `windres` nem `rc.exe`.

## Licença

[MIT](LICENSE) © 2026 WinyZanin. Os instaladores e o zip portátil incluem uma cópia como `LICENSE.txt`.

## Limitações

- Só funciona com o app aberto, e a configuração (teclas e idioma) ainda não é salva ao fechar.
- Uma tecla por vez: combinações (ex.: Ctrl+F1) ainda não são suportadas.
- Em janelas de programas executados **como administrador**, o Windows (UIPI) não deixa o hook bloquear a tecla. Para cobrir esses casos, execute o `.exe` também como administrador.
