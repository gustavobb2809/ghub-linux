# ghub-gui — interface gráfica do ghub-linux

`ghub-gui` é a janela do ghub-linux (Qt 6/QML com a lógica em Rust via `cxx-qt`). Ela não fala com o mouse: é um cliente do `ghubd` pelo D-Bus de sessão (ver `docs/daemon.md`). Verificação manual: `docs/gui-verification.md`.

![ghub](../packaging/icons/ghub-logo.svg)

A marca (emblema com mouse + palavra "ghub" em traços redondos) é original e independente da Logitech. A barra lateral a desenha nativamente em `qml/components/Logo.qml` (Shapes, sem imagens nem fontes); o ícone do app é `packaging/icons/org.ghublinux.Gui.svg` e o da bandeja é rasterizado pelo `ghubd`.

## Arquitetura

O crate `crates/ghub-gui` tem quatro peças:

- `src/model/`: lógica pura e testável sem Qt (estado da GUI, `Props` exibidas, validação de DPI contra `dpi_min`/`dpi_max`/`dpi_step`/`dpi_list`, formatação, nomes de perfil, debounce de edição).
- `src/client/`: acesso ao daemon atrás do trait `DaemonApi`. `real.rs` usa o D-Bus de sessão (e `StartServiceByName` para iniciar o serviço), `fake.rs` é um daemon em memória com cenários (feature `fake-daemon`), `controller.rs` roda numa thread própria, aplica comandos e eventos e publica as `Props`, e `single_instance.rs` garante uma só janela (nome `org.ghublinux.Gui` no barramento; a segunda abertura traz a janela existente).
- `src/backend.rs`: o objeto `Backend` exposto ao QML (módulo `org.ghublinux.gui`); propriedades em camelCase (`daemon_state` vira `daemonState`) e invocáveis como `commitDpi`.
- `qml/`: `AppWindow.qml`, o singleton `Theme.qml` (paleta gruvbox), `pages/` (`Overview`, `Sensitivity`, `Settings`, `Disconnected`), `components/` e `Smoke.qml` (cenários de teste headless).

Fluxo: eventos do daemon e comandos da UI chegam ao controller; ele atualiza o estado, e o `Backend` publica as propriedades para a thread do Qt.

## Dependências

Fedora: `qt6-qtbase-devel` e `qt6-qtdeclarative-devel` (além do Rust e de `cmake`/compilador C++). Para a ativação por bandeja, o `ghubd` precisa estar instalado (`packaging/install-user.sh`).

## Rodar

```bash
cargo run -p ghub-gui
```

O alvo de compilação com Qt é grande: se o `/tmp` for tmpfs, compile fora dele:

```bash
export CARGO_TARGET_DIR=$HOME/.cache/ghub-target
cargo run -p ghub-gui
```

O `packaging/install-user.sh` respeita `CARGO_TARGET_DIR` e instala o `ghub-gui`, o `.desktop` ("ghub" no menu de aplicativos) e o ícone SVG sob `~/.local`; `--no-gui` pula essa parte.

## Sem daemon nem mouse: `--fake`

Com a feature `fake-daemon`, a GUI usa um daemon em memória em vez do D-Bus:

```bash
cargo run -p ghub-gui --features fake-daemon -- --fake running
```

Cenários de `--fake`: `running` (padrão, mouse conectado), `stopped` (daemon parado), `sleeping` (mouse dormindo: "lendo…", controles desabilitados) e `no-led` (mouse sem suporte a LED: a aba Iluminação some). Sem a feature, `--fake` sai com código 2.

## Smoke headless

`--smoke <nome>` (exige `--fake`, exceto `theme`, `fail` e `load-failure`; sem `--fake` sai com 2) carrega `Smoke.qml`, verifica a condição do cenário e sai com 0 (ok) ou 7 (falha); não usa a instância única nem o barramento. Em CI/terminal:

```bash
QT_QPA_PLATFORM=offscreen QT_LOGGING_TO_CONSOLE=1 \
  cargo run -p ghub-gui --features fake-daemon -- --smoke pages --fake running; echo $?
```

`QT_LOGGING_TO_CONSOLE=1` mostra os erros de QML. Nomes: `theme`, `running`, `stopped`, `sleeping`, `pages`, `disconnected`, `dimmed`, `nav`, `rebind`, `release`, `edit`, `lighting` (aba Iluminação reflete o LED e Aplicar grava), `lighting-hidden` (sem suporte a LED a aba some) e `load-failure` (aponta para um QML inexistente e deve sair com 1: uma falha ao carregar o QML encerra o processo em vez de ficar parado). A suíte `crates/ghub-gui/tests/smoke.rs` roda cada cenário e uma contraprova (cenário errado deve falhar com 7).

## Testes

```bash
RUSTFLAGS="-D warnings" cargo test --workspace --exclude ghub-gui
CARGO_TARGET_DIR=$HOME/.cache/ghub-target RUSTFLAGS="-D warnings" \
  cargo test -p ghub-gui --features fake-daemon
```
