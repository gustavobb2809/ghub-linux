# ghub-linux — design

Data: 2026-10-12 · Status: aguardando revisão

Alternativa nativa para Linux ao Logitech G HUB, começando pelo **G703 Lightspeed** e estruturada para crescer para outros dispositivos Logitech.

Mocks de UI (fonte visual desta spec): https://claude.ai/artifact/SJAZxtEFWq5tiRotHBAN34

## 1. Objetivo e critérios de sucesso

Um aplicativo com GUI e um serviço em segundo plano que configura o G703 sem o G HUB.

A v1 está pronta quando, com o G703 conectado pelo receptor Lightspeed:

1. A GUI mostra o dispositivo, a bateria e o estado da conexão.
2. É possível alterar DPI (5 níveis, nível ativo) e taxa de polling, e a mudança vale no mouse.
3. Perfis são salvos em disco e reaplicados automaticamente quando o mouse reconecta ou o computador reinicia.
4. Um ícone na bandeja do sistema mostra o estado e permite trocar DPI e perfil sem abrir a janela.
5. O usuário pode ligar ou desligar "Iniciar com o sistema".
6. Nada exige `sudo` em uso normal. A regra udev é instalada uma vez, com autorização explícita.

## 2. Fora do escopo

- Copiar código, assets ou marca do G HUB. O G HUB é só referência de funcionalidades.
- Suporte a teclados, headsets, volantes e webcams (specs próprias, depois).
- Troca automática de perfil por aplicativo em foco.
- Windows e macOS.

## 3. Fases

| Fase | Entrega |
|---|---|
| **v1** | Enumeração de dispositivos, bateria, DPI, polling, perfis no host, tray, autostart, GUI de Visão geral, Sensibilidade e Configurações |
| **v1.1** | RGB do logo (aba Iluminação) |
| **v1.2** | Perfis onboard e remapeamento de botões / macros (aba Botões) |
| **Depois** | Outras famílias de dispositivos, uma spec por categoria |

Cada fase tem plano e implementação próprios. Esta spec detalha a **v1** e define as interfaces que v1.1 e v1.2 vão reutilizar.

## 4. Stack

Rust (edição 2021 ou mais nova), Qt6/QML via `cxx-qt` para a GUI, D-Bus (`zbus`) entre daemon e GUI, serviço de usuário do systemd.

Ambiente verificado em 2026-10-12: `cargo 1.98.1` instalado; receptor `046d:c539` presente; `qt6-qtbase-devel` e `qt6-qtdeclarative-devel` **não** instalados (instalação depende de autorização do usuário).

## 5. Arquitetura

Workspace Cargo com quatro crates:

```
crates/
  hidpp/      biblioteca HID++ (sem D-Bus, sem Qt)
  ghub-ipc/   tipos e interface D-Bus compartilhados
  ghubd/      daemon: hotplug, perfis, D-Bus, bandeja
  ghub-gui/   GUI cxx-qt + QML
```

### 5.1 `hidpp`

Responsabilidade: falar HID++ com dispositivos Logitech por `hidraw`.

- `Transport` (trait): enviar/receber relatórios. Implementações: `HidrawTransport` (real) e `FakeTransport` (testes, reproduz conversas gravadas).
- Enumeração: varre `/dev/hidraw*` / udev, filtra vendor `046d`, identifica receptor e dispositivos pareados.
- Descoberta de features (HID++ 2.0 `IRoot`/`IFeatureSet`): mapeia ID de feature para índice no dispositivo.
- API tipada por feature, cada uma em módulo próprio:
  - bateria (`0x1004` ou `0x1000`, conforme o dispositivo expõe),
  - DPI ajustável (`0x2201`),
  - taxa de report (`0x8060`),
  - reservados para fases futuras: RGB (`0x8071`), perfis onboard (`0x8100`), remapeamento de botões (`0x1b04`).
- Erros tipados (`HidppError`): dispositivo ausente, timeout, erro do protocolo (código HID++), feature não suportada.

**Regra de ouro:** uma feature só é usada se o dispositivo a anunciar. A UI esconde o que o dispositivo não suporta.

**Risco conhecido:** os IDs de feature acima vêm do conhecimento público do protocolo e precisam ser **confirmados no hardware real** com um comando de diagnóstico (`ghubctl probe`) antes de qualquer escrita. O G703 pode usar HID++ 4.2; a primeira tarefa do plano é um spike que lista as features reais do aparelho.

### 5.2 `ghub-ipc`

Interface D-Bus no barramento de sessão, nome `org.ghublinux.Daemon1`, objeto `/org/ghublinux/Daemon`:

- `Devices() -> a(sssbu)` (id, nome, modelo, conectado, bateria %)
- `GetDeviceState(id) -> a{sv}` (DPI ativo, níveis, polling, perfil ativo)
- `SetDpiLevels(id, au)` · `SetActiveDpiLevel(id, u)` · `SetPollingRate(id, u)`
- `Profiles() -> as` · `ActivateProfile(name)` · `SaveProfile(name, a{sv})`
- Sinais: `DeviceChanged(id)`, `DevicesChanged()`, `BatteryLow(id, u)`.

### 5.3 `ghubd`

- Monitora hotplug via udev (`udev` crate) e reabre dispositivos que voltam.
- Mantém o estado desejado (perfil ativo) e o reaplica ao reconectar.
- Perfis em TOML: `~/.config/ghub-linux/profiles.toml`. Escrita atômica (arquivo temporário + rename).
- Expõe a interface D-Bus acima.
- **Bandeja**: StatusNotifierItem via crate `ksni` (Rust puro, sem Qt), então funciona com a GUI fechada e em Waybar, Quickshell e KDE.
  - Clique abre a GUI; menu de contexto: bateria/perfil, DPI, perfil, "Iniciar com o sistema", "Abrir", "Sair e parar o serviço".
  - Estados do ícone: normal, bateria baixa (ponto vermelho), carregando (raio), desconectado (esmaecido).
- **Iniciar com o sistema**: o item do menu e a opção em Configurações habilitam/desabilitam `ghubd.service` do usuário (`systemctl --user enable|disable`). Só o daemon inicia com a sessão; a GUI abre sob demanda.
- Notificação de bateria baixa (limite configurável, padrão 15%).
- Fecha a janela da GUI sem parar o daemon.

### 5.4 `ghub-gui`

- App Qt6/QML via `cxx-qt`, comunicando só com `ghubd` por D-Bus.
- Telas da v1 conforme os mocks: Visão geral, Sensibilidade, Configurações. Iluminação e Botões entram como abas desabilitadas com selo de versão até as fases v1.1 e v1.2.
- Tema Gruvbox escuro; tipografia IBM Plex Sans e JetBrains Mono.
- Se o daemon não estiver rodando, a GUI oferece iniciá-lo.

## 6. Identidade visual

- **Logo A**: selo laranja (`#fe8019`) com pinguim, nome "ghub" em sans arredondada minúscula e "LINUX" em mono pequeno.
- Ícone do app em 16 a 128 px; ícone de bandeja **simbólico**, monocromático, em 24 px (SVG), com os 4 estados acima.
- O pinguim e o logotipo são originais. Não usar o logo, o nome "Logitech" ou "G HUB" como marca do projeto: **"ghub" é nome provisório**, a decidir antes da primeira publicação pública.

## 7. Permissões

- Arquivo `/etc/udev/rules.d/70-ghub-linux.rules` concedendo acesso `uaccess` ao hidraw dos dispositivos Logitech (`ATTRS{idVendor}=="046d"`).
- Instalação: manual e documentada, ou via comando `ghubctl install-udev` que pede autorização explícita. **Nunca** executar `sudo` sem aprovação do usuário.
- Rodar sem a regra: a GUI mostra o diagnóstico "sem acesso ao hidraw" com instrução, sem falhar silenciosamente.

## 8. Tratamento de erros

- Dispositivo some durante escrita: marca como desconectado, mantém o estado desejado, reaplica na volta.
- Erro HID++ ou timeout: erro tipado propagado pelo D-Bus; a GUI mostra mensagem curta e mantém o valor anterior.
- `profiles.toml` inválido: mantém cópia `.bak`, carrega perfil padrão e avisa na GUI.
- Escritas no dispositivo são idempotentes; reaplicar o mesmo perfil duas vezes é seguro.

## 9. Testes

- `hidpp`: testes unitários com `FakeTransport` (codificação/decodificação de relatórios, descoberta de features, erros).
- `ghubd`: testes de integração com transporte falso (perfil reaplicado ao reconectar, escrita atômica de TOML).
- Teste com hardware real, opcional, atrás de uma feature flag (`--features hardware`), só leitura por padrão.
- GUI: verificação manual guiada pelos mocks; sem teste automatizado de QML na v1.

## 10. Ordem de implementação sugerida (base do plano)

1. Spike: `ghubctl probe` lista as features do G703 (somente leitura).
2. Crate `hidpp` + `FakeTransport` + bateria, DPI, polling.
3. `ghubd` com perfis e D-Bus, depois hotplug e reaplicação.
4. Bandeja e "Iniciar com o sistema".
5. `ghub-gui` (Visão geral, Sensibilidade, Configurações).
6. Regra udev, empacotamento (`.desktop`, unit do systemd, ícones), README.

## 11. Decisões em aberto

- Nome definitivo do projeto (hoje "ghub", provisório).
- Local do repositório: assumido `~/Documentos/Projetos/ghub-linux`.
- Licença (sugestão: GPL-3.0-or-later ou MIT; a decidir pelo usuário).
- IDs reais das features do G703 (resolvido pelo spike do passo 1).
