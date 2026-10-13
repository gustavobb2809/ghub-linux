# ghub-gui — design (Plano 4)

Data: 2026-10-12 · Status: aguardando revisão

Detalha a seção 5.4 de `2026-10-12-ghub-linux-design.md`. A GUI é um cliente D-Bus do `ghubd`; não fala HID++ nem toca em `hidraw`.

Mocks (fonte visual): https://claude.ai/artifact/SJAZxtEFWq5tiRotHBAN34

## 1. Objetivo e critérios de sucesso

A GUI está pronta quando, com o `ghubd` rodando e o G703 conectado:

1. **Visão geral** mostra nome, estado da conexão, bateria (%, carregando), DPI atual, polling e perfil ativo.
2. **Sensibilidade** edita os níveis de DPI (até 5), o nível ativo e o polling; a mudança vale no mouse.
3. **Configurações** troca/cria/apaga perfis e liga/desliga "Iniciar com o sistema".
4. A janela reflete mudanças feitas fora dela (bandeja, `ghubctl`, reconexão) sem ação do usuário.
5. Daemon parado: a GUI mostra um estado claro e oferece iniciá-lo; nunca trava nem fecha sozinha.
6. Clicar em "Abrir" na bandeja abre a GUI no ambiente gráfico correto; uma segunda abertura apenas traz a janela existente para a frente.
7. Nada exige `sudo`. Instalação do usuário via `packaging/install-user.sh`.

## 2. Fora do escopo (v1)

- Abas **Iluminação** (v1.1) e **Botões** (v1.2): aparecem desabilitadas com selo de versão.
- Múltiplos dispositivos simultâneos na interface: a v1 mostra o primeiro dispositivo conectado (seletor só se houver mais de um; ver 5.4).
- Temas claros, internacionalização (textos em português; strings centralizadas para facilitar depois).
- Teste automatizado de aparência/QML além do smoke headless.

## 3. Decisões de design (com padrões escolhidos)

| Tema | Decisão | Motivo |
|---|---|---|
| Binding | `cxx-qt` 0.10 + `cxx-qt-lib` (`qt_full`), Qt 6.11 do sistema | Validado em protótipo descartável (propriedades, sinais, `qt_thread().queue`) |
| D-Bus | `ghub_ipc::DaemonProxyBlocking` (zbus 5, sem tokio) em threads próprias | Mesmo cliente dos testes do daemon; sem runtime assíncrono novo |
| Tema | Singleton QML `Theme` (Gruvbox escuro) via `QmlFile::singleton(true)` | Uma fonte das cores; suportado pelo `cxx-qt-build` |
| Fontes | `Inter` (UI) e `JetBrains Mono` (números/valores) do sistema, com fallback `sans-serif`/`monospace` | IBM Plex dos mocks não está instalada no usuário; a spec geral citava Plex, esta spec a substitui |
| Controles | Qt Quick Controls `Basic`, estilizados por `Theme`; sem depender do estilo do qt6ct | Aparência igual em qualquer desktop |
| DPI do botão do mouse | A GUI **não** esconde a diferença: mostra "DPI do mouse (X) difere do perfil (Y)" com botão "Aplicar perfil"; o daemon continua reaplicando ao acordar (decisão provisória do usuário) | Evita decisão silenciosa; reavaliável sem mudar a GUI |
| Instância única | Nome D-Bus `org.ghublinux.Gui` (flag `DBusActivatable` no `.desktop`); segunda execução chama `Activate` e sai | Padrão freedesktop; evita duas janelas |
| Lançamento pela bandeja | `ghubd` passa a lançar `ghub-gui` por `systemd-run --user --scope --collect`, relatando falha por notificação | Resolve pendência 9: ambiente gráfico correto e a GUI sobrevive ao restart do daemon |

## 4. Arquitetura

Novo crate `crates/ghub-gui` (binário `ghub-gui`), separando o que é testável sem Qt:

```
crates/ghub-gui/
  build.rs            CxxQtBuilder::new_qml_module(...)
  src/
    main.rs           init Qt, registra módulo QML, instância única, carrega Main.qml
    model/            LÓGICA PURA (sem Qt, testes unitários)
      mod.rs
      format.rs       bateria, DPI, polling, textos
      dpi.rs          validação/normalização de DPI e de níveis
      state.rs        ViewState a partir de DeviceState/DeviceSummary + estado do daemon
      debounce.rs     debounce/coalescência de escritas do slider
    client/           ACESSO AO DAEMON (sem Qt)
      mod.rs          trait DaemonApi (o que a GUI usa) + impl real sobre DaemonProxyBlocking
      worker.rs       thread de comandos (fila) e thread de sinais
      fake.rs         FakeDaemon p/ testes (#[cfg(test)] e feature "fake-daemon")
    backend.rs        QObject cxx-qt `Backend`: propriedades, invokables, sinais
  qml/
    Main.qml          janela, navegação lateral, StackLayout
    Theme.qml         singleton (cores, tamanhos, fontes)
    components/       Card, DpiSlider, BatteryBadge, NavTab, PillButton, Toast...
    pages/            Overview.qml, Sensitivity.qml, Settings.qml, Disconnected.qml
  data/
    org.ghublinux.Gui.desktop
    icons/            hicolor 16-128 (logo A), SVG
```

Fronteiras:

- `model` e `client::DaemonApi` não importam Qt: testáveis com `cargo test` puro.
- `backend.rs` é fino: converte `ViewState` ↔ propriedades Qt e encaminha invokables para a fila de comandos. Nenhuma regra de negócio ali.
- QML só apresenta e dispara invokables; não valida nada (a validação fica em `model::dpi` e é exposta como invokable para feedback imediato).

### 4.1 Fluxo de dados

1. Início: `Backend` pede `Devices()`; se houver dispositivo conectado, `GetDeviceState(id)` + `ListProfiles()` + `GetActiveProfile()`.
2. Uma **thread de sinais** assina `DevicesChanged`, `DeviceChanged`, `ProfileChanged`, `BatteryLow` e `NameOwnerChanged` do daemon; cada evento vira uma releitura (coalescida, no máximo uma por ~100 ms) e o resultado volta por `qt_thread().queue(...)`, que atualiza as propriedades.
3. Uma **thread de comandos** executa as escritas (`SetDpiLevels`, `SetActiveDpiLevel`, `SetPollingRate`, perfis, `Rescan`) em ordem, com timeout de 5 s do daemon; erro volta como sinal `errorOccurred(text)` e a GUI relê o estado real (nunca mantém valor otimista após falha).
4. O slider de DPI atualiza a UI imediatamente e envia a escrita só após o debounce (150 ms) ou ao soltar; apenas o último valor é enviado (coalescência), evitando enfileirar escritas no mouse.

### 4.2 Superfície do `Backend` (QML)

Propriedades (somente leitura para QML): `daemonState` (`connecting|running|stopped`), `deviceName`, `connected`, `batteryPercent` (`-1` desconhecida), `batteryState`, `dpiCurrent`, `dpiLevels` (lista), `activeLevel`, `dpiMin/dpiMax/dpiStep`, `pollingHz`, `pollingOptions`, `profiles`, `activeProfile`, `profileDiffers` (bool), `autostartEnabled`, `lastError`.

Invokables: `setDpiLevel(index, dpi)`, `commitDpiLevels()`, `selectLevel(index)`, `setPolling(hz)`, `activateProfile(name)`, `createProfile(name)`, `deleteProfile(name)`, `applyProfileToDevice()`, `setAutostart(on)`, `startDaemon()`, `rescan()`, `validateDpi(dpi) -> string` (vazio = válido).

## 5. Comportamento

### 5.1 Estados do daemon

`connecting` (spinner, até 1,5 s) → `running` ou `stopped`. Em `stopped`, a página **Disconnected** mostra "Serviço não está rodando" e o botão **Iniciar serviço** (`org.freedesktop.DBus.StartServiceByName("org.ghublinux.Daemon")`, ativação pelo arquivo já instalado; se falhar, exibe o erro e a dica `systemctl --user start ghubd`). O `NameOwnerChanged` leva a GUI de volta a `running` sozinha.

### 5.2 Dispositivo

- Nenhum dispositivo conectado: mostra o último dispositivo conhecido esmaecido, com "Mouse desconectado ou dormindo — mova o mouse" e controles desabilitados. Dispositivos fantasma (`connected=false`) nunca ocupam o lugar de um conectado.
- `dpi_current == 0` com `connected=true` é tratado como "lendo..." (sem formatar `0 DPI`).
- Bateria `-1` mostra "—"; estado carregando mostra o raio; ≤ 15 % usa a cor de alerta.

### 5.3 Sensibilidade

- Até 5 níveis (limite do daemon); adicionar/remover exige ≥ 1 nível.
- Valor aceito se: `dpi_min ≤ v ≤ dpi_max`, múltiplo de `dpi_step` a partir de `dpi_min`, e, quando `dpi_list` não for vazia, pertencer a ela. Campo numérico com entrada livre + slider; inválido fica destacado e **não** é enviado; o texto do erro vem de `validateDpi`.
- Mudar o nível ativo chama `SetActiveDpiLevel`; editar níveis chama `SetDpiLevels(levels, active)` completo.
- `profileDiffers`: quando `dpiCurrent` ≠ nível ativo do perfil (ex.: botão de DPI do mouse), mostra o aviso da seção 3 com **Aplicar perfil** (`applyProfileToDevice` = `SetActiveDpiLevel` com o nível do perfil).
- Polling: botões segmentados com as opções de `pollingOptions`.

### 5.4 Perfis e Configurações

- Lista vinda de `ListProfiles()`, relida em todo `ProfileChanged` (criar/apagar trazem o nome do ativo).
- Criar: nome não vazio, único, ≤ 40 caracteres; apagar o ativo exige outro perfil e confirma em diálogo da própria janela.
- "Iniciar com o sistema": lê/altera `systemctl --user is-enabled|enable|disable ghubd.service` (mesma regra da bandeja; falha vira `lastError`). Se mais de um dispositivo estiver conectado, a Visão geral ganha um seletor de dispositivo; um só dispositivo não mostra seletor.
- Seção "Sobre": versão, link para a documentação, nota de que o projeto não é afiliado à Logitech.

### 5.5 Erros

Toast curto e não bloqueante para `errorOccurred`; erro persistente (ex.: `profiles.toml` inválido, quando o daemon expuser `Warnings()`) fica para o item 4 das pendências do daemon. Timeout (`Failed` após 5 s) ⇒ estado considerado desconhecido ⇒ releitura.

## 6. Janela, tema e identidade

- Janela 1280×800, redimensionável, mínimo 900×600; barra lateral à esquerda com Visão geral, Sensibilidade, Iluminação (desabilitada, selo v1.1), Botões (desabilitada, selo v1.2), Configurações.
- Cores Gruvbox escuro em `Theme.qml` (`bg #282828`, `bg1 #3c3836`, `fg #ebdbb2`, acento laranja `#fe8019`, verde `#b8bb26`, vermelho `#fb4934`, amarelo `#fabd2f`).
- Ícone do app: logo A (selo laranja + pinguim) em SVG e PNGs 16–128; `.desktop` com `Name=ghub`, `Icon=org.ghublinux.Gui`, `StartupWMClass=ghub-gui`, categorias `Settings;HardwareSettings;`.
- Fechar a janela encerra só a GUI; o daemon continua.

## 7. Empacotamento e ambiente

- `packaging/install-user.sh` passa a compilar e instalar `ghub-gui` em `~/.local/bin`, o `.desktop` em `~/.local/share/applications`, os ícones em `~/.local/share/icons/hicolor`, e atualiza o cache (`update-desktop-database`/`gtk-update-icon-cache` quando existirem). Mantém `--dry-run` e escrita só sob `$HOME`.
- Dependências de build já instaladas: `qt6-qtbase-devel`, `qt6-qtdeclarative-devel`. Documentar que `qt6-qtdeclarative` (runtime) e o módulo Qt Quick Controls são necessários para rodar.
- **Builds de Qt/cxx-qt usam `CARGO_TARGET_DIR` no `/home`** (ex.: `~/.cache/ghub-target`), nunca `/tmp` (tmpfs de 16 GB enche).
- Os muitos *warnings* de C++ do `cxx-qt-lib` com GCC 16 são esperados e não quebram `RUSTFLAGS="-D warnings"` (que vale para o Rust).

## 8. Mudanças no `ghubd` (pequenas, parte deste plano)

1. `GuiLauncher`: lançar por `systemd-run --user --scope --collect --unit=ghub-gui-<n> ghub-gui`; capturar status/saída do filho e notificar falha. Teste com launcher falso (já existe a abstração).
2. Se a GUI já estiver rodando (nome `org.ghublinux.Gui` presente), o clique chama `Activate` em vez de lançar outra.
3. Expor a verificação de autostart de forma reutilizável (a GUI usa `systemctl` direto; sem nova API D-Bus na v1).

## 9. Testes

- **Unitários (Rust puro, `cargo test`)**: `model::format`, `model::dpi` (limites, passo, `dpi_list`, 0, negativo, estouro), `model::state` (`dpi_current==0`, bateria `-1`, fantasmas, `profileDiffers`), `model::debounce` (coalescência, ordem), `client` com `FakeDaemon` (erro de timeout, reconexão, coalescência de releituras).
- **Smoke headless de Qt** (`QT_QPA_PLATFORM=offscreen`): carrega `Main.qml` com `FakeDaemon` e verifica por **código de saída** (`Qt.exit(ok ? 0 : 7)`; `console.log` não aparece neste ambiente) que: a janela cria; `Theme` resolve; cada página instancia; estados `stopped` e `running` aparecem conforme o fake; uma atualização vinda de outra thread chega às propriedades. Contraprova obrigatória (condição errada ⇒ código ≠ 0).
- **Integração com o daemon real**: opcional, feature `real-daemon`, só leitura por padrão; escritas só com autorização do usuário e restaurando DPI 1200/perfil `[1200]`.
- **Verificação manual guiada pelos mocks** (`docs/gui-verification.md`): cada tela, tray → abrir GUI, segunda abertura, daemon parado/iniciado, mouse dormindo, janela redimensionada.
- Critério de qualidade: `RUSTFLAGS="-D warnings" cargo test --workspace` (sem clippy/rustfmt no ambiente).

## 10. Riscos

| Risco | Mitigação |
|---|---|
| Compilação lenta/pesada (cxx-qt, GCC 16) | target no `/home`; CI local usa cache; o crate `ghub-gui` fica fora do `default-members` para não atrasar `cargo test` dos demais (a lógica pura tem feature para rodar sem Qt) |
| API do cxx-qt 0.x ainda muda | versão fixada `=0.10.0`; código Qt isolado em `backend.rs` |
| Slider inundar o mouse | debounce + coalescência no cliente |
| Sem `qmltestrunner6` | smoke próprio por código de saída |
| Ambiente do `systemd-run` sem display | GUI lida com falha de `QGuiApplication` (mensagem + código ≠ 0) e o `ghubd` notifica |

## 11. Decisões em aberto

- Nome definitivo e licença (herdado da spec geral); `Name=ghub` no `.desktop` é provisório.
- Se o botão de DPI do mouse deve passar a ser respeitado pelo daemon (hoje reaplica); a GUI já se adapta (5.3).
- Seletor de múltiplos dispositivos: implementado só se houver 2+ conectados; sem hardware para validar além do G703.
