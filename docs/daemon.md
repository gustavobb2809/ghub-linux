# ghubd — daemon do ghub-linux

`ghubd` roda na sessão do usuário, encontra os mouses Logitech HID++ ligados a um receptor, mantém o perfil ativo aplicado a eles e expõe tudo pelo D-Bus de sessão. Não precisa de `sudo`, mas precisa de acesso aos nós `/dev/hidraw*` do receptor (regra udev em `docs/udev/70-ghub-linux.rules`).

## Compilar e rodar

Em primeiro plano (útil para depurar; as mensagens vão para o stderr):

```bash
cargo run -p ghubd
```

Instalação para o usuário atual (sem `sudo`):

```bash
packaging/install-user.sh
```

O script compila em modo release e instala o binário em `~/.local/bin/ghubd`, a unit em `~/.config/systemd/user/ghubd.service` e o arquivo de ativação do D-Bus em `~/.local/share/dbus-1/services/org.ghublinux.Daemon.service`. Depois roda `systemctl --user daemon-reload` e pede ao barramento de sessão que releia a configuração (ver "Iniciar com o sistema e ativação sob demanda"). Ele **não** habilita nem inicia o serviço:

```bash
systemctl --user start ghubd    # inicia agora
systemctl --user enable ghubd   # inicia com a sessão (opcional)
```

O script também compila e instala a interface (`ghub-gui`, o `.desktop` e o ícone em `~/.local/share`; ver `docs/gui.md`); `--no-gui` pula essa parte e `CARGO_TARGET_DIR` é respeitado. `packaging/install-user.sh --dry-run` só imprime os comandos (linhas `+ ...`) e termina com `(simulação: nada foi alterado)`; `--dry-run` e `--no-gui` combinam em qualquer ordem e qualquer outro argumento mostra o uso e sai com código 2.

A unit é `Type=dbus` com `BusName=org.ghublinux.Daemon` e `Restart=on-failure`.

## Arquivo de perfis

`$XDG_CONFIG_HOME/ghub-linux/profiles.toml`, ou `~/.config/ghub-linux/profiles.toml` quando `XDG_CONFIG_HOME` não está definido (ou está vazio).

```toml
active = "Padrão"

[[profile]]
name = "Padrão"
dpi_levels = [1200]
active_level = 0
polling_hz = 1000
```

- Sem o arquivo, a primeira execução **semeia** o perfil `Padrão` com o DPI e o polling lidos do primeiro dispositivo encontrado (`dpi_levels = [<DPI atual>]`) e não escreve nada no mouse. Se não conseguir ler esses valores, não aplica nada e tenta de novo no próximo ciclo.
- Cada perfil tem de 1 a 5 níveis de DPI (nenhum 0), `active_level` dentro da lista e `polling_hz` diferente de 0. Nomes de perfil: 1 a 64 caracteres, sem caracteres de controle.
- A escrita é atômica (arquivo temporário, `fsync`, `rename`). Um arquivo inválido é renomeado para `profiles.toml.bak` e o daemon começa com o perfil padrão (avisando no stderr). Se o arquivo não pôde ser lido, ou não pôde ser renomeado, salvar fica desativado até reiniciar, para nunca sobrescrever o arquivo do usuário sem cópia.

## Interface D-Bus

- Barramento: sessão
- Nome: `org.ghublinux.Daemon`
- Objeto: `/org/ghublinux/Daemon`
- Interface: `org.ghublinux.Daemon1`

Os tipos e o proxy de cliente estão em `crates/ghub-ipc/src/lib.rs`; a implementação em `crates/ghubd/src/service.rs`.

### Métodos

| Método | Argumentos | Retorno | Erros |
|---|---|---|---|
| `Devices` | — | `a(ssbis)` (lista de `DeviceSummary`) | `Failed` |
| `GetDeviceState` | `id: s` | `(ssbuauuuuuauuauiss)` (`DeviceState`) | `UnknownObject`, `Failed` |
| `SetDpiLevels` | `id: s`, `levels: au`, `active: u` | — | `InvalidArgs`, `UnknownObject`, `Failed` |
| `SetActiveDpiLevel` | `id: s`, `active: u` | — | `InvalidArgs`, `UnknownObject`, `Failed` |
| `SetPollingRate` | `id: s`, `hz: u` | — | `InvalidArgs`, `UnknownObject`, `Failed` |
| `ListProfiles` | — | `as` | `Failed` |
| `GetActiveProfile` | — | `s` | `Failed` |
| `ActivateProfile` | `name: s` | — | `UnknownObject`, `Failed` |
| `CreateProfile` | `name: s` | — | `InvalidArgs`, `Failed` |
| `DeleteProfile` | `name: s` | — | `InvalidArgs`, `UnknownObject`, `Failed` |
| `Rescan` | — | — | `Failed` |

- `SetDpiLevels` troca a lista de níveis do perfil ativo e o nível ativo (índice a partir de 0) e aplica o DPI ativo. `SetActiveDpiLevel` só troca o índice. `SetPollingRate` troca o polling do perfil ativo. As três gravam o perfil e só escrevem no mouse o que difere do estado atual.
- `CreateProfile` cria uma cópia do perfil ativo com o novo nome; não a ativa.
- `ActivateProfile` grava o perfil ativo e o aplica a todos os dispositivos conectados.
- `Rescan` varre os receptores na hora (o daemon também varre sozinho, ver abaixo) e responde quando a varredura termina.

### Erros

Os nomes são os de `org.freedesktop.DBus.Error.*`:

| Erro | Quando |
|---|---|
| `InvalidArgs` | Valor fora do que o dispositivo anuncia (DPI fora da grade/lista, polling não suportado); lista de DPI vazia, com mais de 5 níveis ou com 0; nível ativo fora da lista; valor que não cabe em 16 bits; nome de perfil vazio, longo demais, com caracteres de controle ou já existente; apagar o perfil ativo ou o último perfil. Nada é escrito no mouse nesses casos. |
| `UnknownObject` | ID de dispositivo ou nome de perfil inexistente. |
| `Failed` | Dispositivo offline; falha na escrita HID++; o gerente de dispositivos parou ou não respondeu em 5 s. |

Quando uma escrita no dispositivo falha, o perfil **já foi** atualizado e gravado; só a aplicação falhou. Se o mouse não responder mais ao `ping`, ele é marcado offline.

### Sinais

| Sinal | Argumentos | Quando |
|---|---|---|
| `DevicesChanged` | — | Um dispositivo apareceu, voltou ou ficou offline. |
| `DeviceChanged` | `id: s` | Estado de um dispositivo mudou (online/offline, perfil aplicado, bateria em % ou estado). |
| `ProfileChanged` | `name: s` | O perfil ativo foi editado ou trocado, ou um perfil foi criado/apagado. Sempre traz o nome do perfil **ativo**. |
| `BatteryLow` | `id: s`, `percent: u` | A bateria ficou em 15% ou menos sem estar carregando. Repete só depois de voltar a 20% ou mais (ou de começar a carregar). |

Os sinais são emitidos diretamente na conexão (não são declarados no bloco `#[interface]`), então não aparecem na introspecção do objeto; use `busctl --user monitor org.ghublinux.Daemon` para vê-los.

### `DeviceSummary` — `(ssbis)`

| Campo | Tipo | Significado |
|---|---|---|
| `id` | `s` | ID do dispositivo (ver abaixo). |
| `name` | `s` | Nome lido do dispositivo (feature DeviceName `0x0005`); se falhar, o nome do nó HID. |
| `connected` | `b` | O dispositivo responde agora. |
| `battery_percent` | `i` | 0–100; `-1` = desconhecido. |
| `battery_state` | `s` | `discharging`, `charging`, `full` ou `unknown`. |

### `DeviceState` — `(ssbuauuuuuauuauiss)`

| Campo | Tipo | Significado |
|---|---|---|
| `id` | `s` | ID do dispositivo. |
| `name` | `s` | Nome do dispositivo. |
| `connected` | `b` | O dispositivo responde agora. |
| `dpi_current` | `u` | DPI lido do dispositivo neste momento; `0` se desconhecido (sempre `0` com `connected=false`). |
| `dpi_levels` | `au` | Níveis de DPI do perfil ativo. |
| `active_level` | `u` | Índice (a partir de 0) do nível ativo em `dpi_levels`. |
| `dpi_min` | `u` | Menor DPI anunciado pelo dispositivo. |
| `dpi_max` | `u` | Maior DPI anunciado pelo dispositivo. |
| `dpi_step` | `u` | Passo da grade de DPI; `0` quando o dispositivo anuncia uma lista explícita (ver `dpi_list`). |
| `dpi_list` | `au` | Lista explícita de DPIs aceitos; vazia quando o dispositivo usa faixa + passo. |
| `polling_hz` | `u` | Polling lido do dispositivo agora; `0` se desconhecido. |
| `polling_supported` | `au` | Taxas de polling anunciadas pelo dispositivo, na ordem em que ele as informa. |
| `battery_percent` | `i` | 0–100; `-1` = desconhecido. |
| `battery_state` | `s` | `discharging`, `charging`, `full` ou `unknown`. |
| `profile` | `s` | Nome do perfil ativo. |

Com o dispositivo offline, os campos de capacidade (`dpi_min`, `dpi_max`, `dpi_step`, `dpi_list`, `polling_supported`) vêm zerados/vazios; `dpi_levels`, `active_level` e `profile` continuam vindo do perfil.

### ID do dispositivo

`<vendor><product>-<slot>`, com vendor e product do **receptor** em 4 dígitos hexadecimais minúsculos e o slot (1 a 6) do dispositivo no receptor. Exemplo: o G703 LS no slot 1 do receptor LIGHTSPEED `046d:c539` é `046dc539-1`.

## Exemplos com `busctl`

Chamadas usadas na verificação no hardware (`docs/hardware/daemon-verification.md`):

```bash
D="org.ghublinux.Daemon /org/ghublinux/Daemon org.ghublinux.Daemon1"

busctl --user call $D Devices
busctl --user call $D GetDeviceState s 046dc539-1

# três níveis (800, 1200, 1600), ativo = índice 1 (1200)
busctl --user call $D SetDpiLevels sauu 046dc539-1 3 800 1200 1600 1
# troca para o nível 0 (800)
busctl --user call $D SetActiveDpiLevel su 046dc539-1 0
# rejeitado: 125 não está na grade do G703 (100..25600, passo 50)
busctl --user call $D SetDpiLevels sauu 046dc539-1 1 125 0
# rejeitado: 333 Hz não é anunciado
busctl --user call $D SetPollingRate su 046dc539-1 333

# acompanhar os sinais
busctl --user monitor org.ghublinux.Daemon
```

Outras chamadas (mesma sintaxe; não exercitadas no hardware):

```bash
busctl --user call $D ListProfiles
busctl --user call $D GetActiveProfile
busctl --user call $D CreateProfile s Trabalho
busctl --user call $D ActivateProfile s Trabalho
busctl --user call $D DeleteProfile s Trabalho   # precisa não ser o ativo
busctl --user call $D Rescan
```

## Funcionamento

- Uma única thread gerente faz todo o acesso ao hardware e aos perfis; as chamadas D-Bus viram comandos para ela e esperam a resposta por até 5 s.
- A cada 5 s o gerente faz `ping` em cada dispositivo conectado (quem não responde fica offline), varre os receptores de novo se não houver dispositivos ou se algum estiver offline, e relê a bateria.
- Um monitor udev do subsistema `hidraw` pede uma varredura imediata quando algo é conectado ou removido.
- Quando um dispositivo aparece ou volta, o daemon lê as capacidades e reaplica o perfil ativo (só escreve o que difere e só valores que o dispositivo anuncia).
- A bateria vem do sysfs do kernel (`/sys/class/power_supply/hidpp_battery_*`, criado pelo driver `hid-logitech-hidpp`), não do HID++, e é relida a cada ciclo (5 s). Para `BatteryLow`, um estado de carga desconhecido conta como descarregando.

## Bandeja

O daemon publica um ícone na bandeja (StatusNotifierItem, via crate `ksni`, sem Qt) e funciona com a GUI fechada. Se nenhum host estiver no ar (login antes do shell), ele espera o `StatusNotifierWatcher` aparecer; se a bandeja falhar de vez, o daemon segue sem ela. Código: `crates/ghubd/src/tray.rs` (integração com o `ksni`), `tray_model.rs` (o que o menu mostra), `tray_icon.rs` (ícone) e `hub.rs` (atualização).

### Menu

| Item | Significado |
|---|---|
| `<nome> — <bateria>` | Linha informativa do dispositivo principal (o primeiro conectado, senão o primeiro da lista): bateria em %, "(carregando)", "(carga completa)" ou "bateria desconhecida". Sem dispositivos: "Nenhum dispositivo encontrado". |
| `<dpi> DPI · <hz> Hz` | DPI e polling lidos do mouse agora. Com o mouse offline vira "Inativo ou dormindo". |
| DPI ▸ | Níveis de DPI do perfil ativo (rádio); escolher um chama `SetActiveDpiLevel` no dispositivo principal. Desabilitado com o mouse offline. |
| Perfil: `<nome>` ▸ | Perfis (rádio); escolher um ativa o perfil. |
| Iniciar com o sistema | ✓ quando `ghubd.service` está habilitado. Desabilitado quando o serviço não está instalado. |
| Abrir G HUB Linux | Abre a GUI (igual ao clique esquerdo no ícone). |
| Sair e parar o serviço | Encerra o daemon (ver abaixo). |

Clique esquerdo no ícone abre a GUI (`ghub-gui`) por `systemd-run --user --scope --collect --quiet ghub-gui`: a GUI roda num escopo transitório próprio, fora do cgroup do daemon (herda o ambiente gráfico da sessão e sobrevive a um restart do `ghubd`). Sem `systemd-run`, o daemon lança `ghub-gui` direto. O `ghubd` acompanha o processo por 1,5 s: se ele sair com erro nesse prazo, a primeira linha do stderr (limitado a 2 KiB) vira notificação; se o `ghub-gui` não existir, a notificação avisa que a interface ainda não está instalada. A segunda abertura é tratada pela própria GUI (instância única), sem mudança no `ghubd`. Falhas das ações do menu (DPI, perfil, autostart, parar) também viram notificação.

O tooltip traz o nome do dispositivo e "`<bateria>` · `<dpi>` DPI" (ou "Inativo ou dormindo · `<bateria>`"). Nem todo host desenha tooltips: o Quickshell da verificação não desenha.

O menu é reconstruído a cada evento do gerente (juntando rajadas de 150 ms) e a cada 30 s (o que também relê o estado do autostart no systemd).

### Estados do ícone

Pinguim desenhado em código (22, 32 e 48 px), cor `#ebdbb2`:

| Estado | Quando | Aparência |
|---|---|---|
| Normal | Dispositivo conectado | Pinguim. |
| Bateria baixa | Bateria ≤ 15 % e não está com carga completa | Ponto vermelho (`#fb4934`) no canto inferior direito. |
| Carregando | Bateria carregando | Ponto verde (`#b8bb26`) no canto inferior direito. |
| Desconectado | Nenhum dispositivo, ou o principal offline/dormindo | Pinguim esmaecido (40 % de opacidade). |

## Notificações

Enviadas por `org.freedesktop.Notifications.Notify` no barramento de sessão (app "G HUB Linux", 8 s). Sem servidor de notificações, vão para o stderr/journal.

- **Bateria baixa**: a cada sinal `BatteryLow` — "Bateria baixa", "`<nome>`: `<n>` %", urgência crítica.
- **Erros** das ações da bandeja: urgência normal.
- **Teste**: `ghubd --notify-test` envia uma notificação de teste, imprime "notificação de teste enviada" e sai (não inicia o daemon). `ghubd --help` mostra o uso.

## Iniciar com o sistema e ativação sob demanda

- **Unit** (`packaging/systemd/ghubd.service` → `~/.config/systemd/user/ghubd.service`): `Type=dbus`, `BusName=org.ghublinux.Daemon`, `ExecStart=%h/.local/bin/ghubd`, `Restart=on-failure`, `WantedBy=default.target`.
- **Arquivo de ativação** (`packaging/dbus/org.ghublinux.Daemon.service` → `~/.local/share/dbus-1/services/`): `Name=org.ghublinux.Daemon`, `SystemdService=ghubd.service`. Uma chamada ao nome com o serviço parado faz o systemd iniciá-lo. O arquivo não expande variáveis, então o instalador grava o caminho absoluto em `Exec=`.
- **Primeira instalação**: o dbus-broker só enxerga um arquivo de ativação novo depois de reler a configuração (ou de um novo login); antes disso a chamada falha com "The name is not activatable". Por isso `packaging/install-user.sh [--dry-run]` roda, depois do `daemon-reload`, `busctl --user call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus ReloadConfig`; se isso falhar, só avisa e segue.
- A primeira chamada que ativa o daemon pode receber `Failed` ("o gerente de dispositivos não respondeu a tempo") se a varredura inicial demorar mais de 5 s (visto uma vez, com o mouse meio acordado; numa segunda medição a primeira resposta veio 0,34 s após o início).
- **"Iniciar com o sistema"** na bandeja = `systemctl --user enable ghubd.service` / `systemctl --user disable ghubd.service` (estado lido com `systemctl --user is-enabled`). Só o daemon inicia com a sessão; a GUI abre sob demanda.
- **"Sair e parar o serviço"**: quando o daemon é o processo principal da unit (`INVOCATION_ID` definido e `SYSTEMD_EXEC_PID` igual ao próprio PID), roda `systemctl --user stop --no-block ghubd.service` e o systemd o encerra (sem reinício, pois o `Restart=on-failure` não se aplica a uma parada pedida). Iniciado à mão, só encerra o processo.

## Compatibilidade com hosts de bandeja

O menu é um `com.canonical.dbusmenu` e cada host o relê de um jeito. O Quickshell da verificação (`docs/hardware/tray-verification.md`) relê o menu só em momentos específicos e ignora atualizações posteriores:

- ao clicar num item de submenu, ele envia `Event(clicked)`, `Event(closed)` e, 4 ms depois, `GetLayout`; uma atualização que chegue depois disso é ignorada;
- ao clicar num item de topo, ele não relê nada, e nunca aplica `toggle-state` recebido por `ItemsPropertiesUpdated`; só aplica mudanças estruturais (`LayoutUpdated`).

Por isso o menu usa três medidas, todas em `crates/ghubd/src/tray.rs`:

1. **Atualização otimista**: o clique já marca a nova seleção (rádio de DPI/perfil, ✓ do autostart) antes de devolver o controle ao host.
2. **Espera pelo resultado (`SETTLE_TIMEOUT`, 800 ms)**: o clique envia a ação e espera até 800 ms a vista reconstruída depois de executá-la, para que o `GetLayout` seguinte já veja o estado real (bateria, DPI, Hz). Sem resposta no prazo, fica o estado otimista e o refresh normal corrige depois. Durante essa espera a thread do `ksni` fica bloqueada.
3. **Marcador de layout invisível**: um submenu oculto no fim do menu alterna entre 0 e 1 filho oculto a cada mudança visível, o que força o `ksni` a emitir `LayoutUpdated` e o host a reler o menu inteiro. Risco: um host que ignore `visible:false` mostraria uma linha vazia no fim do menu (no Quickshell do usuário não aparece).

## Verificado no hardware

Com o G703 LIGHTSPEED (receptor `046d:c539`) em 2026-10-12, commit `7696a89` (detalhes em `docs/hardware/daemon-verification.md`):

- Detecção do mouse, nome lido do dispositivo e bateria igual à do sysfs.
- Primeira execução semeia o perfil a partir do DPI/polling atuais sem escrever no mouse.
- `SetActiveDpiLevel` escreve o DPI de verdade (confirmado pela releitura do dispositivo).
- DPI fora da grade, polling não anunciado e nível ativo fora da lista são rejeitados sem mudar o mouse.
- Mouse desligado e religado: o daemon marca offline, redescobre e reaplica o perfil (1600 DPI voltou, embora o mouse volte em 800 sozinho).
- Mouse dormindo por inatividade aparece offline e é redetectado ao acordar; escrita num dispositivo offline falha com `o dispositivo <id> está offline`.

A bandeja, as notificações, o autostart e a ativação sob demanda foram verificados no desktop (Hyprland + Quickshell) em 2026-10-12: `docs/hardware/tray-verification.md`.

## Limitações da v1

- **Um perfil global**: o perfil ativo vale para todos os dispositivos; não há perfil por dispositivo.
- **Níveis de DPI são do host**: `dpi_levels` são presets guardados no `profiles.toml`; só o DPI ativo é escrito no mouse, e nada vai para a memória onboard. O botão de DPI do mouse muda o DPI por conta própria, sem passar pelo daemon (na verificação ele levou o DPI a 3200, valor fora do perfil). O daemon só volta ao DPI do perfil quando o aplica de novo: ao reconectar, em `ActivateProfile` ou numa chamada `Set*`.
- **Bateria só em receptor com exatamente um dispositivo e uma bateria** no sysfs; caso contrário a bateria fica desconhecida (`-1`/`unknown`). A associação dispositivo↔bateria é mantida entre varreduras (uma varredura que falha não a apaga). Se o arquivo da bateria no sysfs não puder ser lido (por exemplo, com o dispositivo offline), a bateria aparece desconhecida.
- **Dispositivos online não são sondados de novo**: a varredura pula os slots de quem já está conectado (as respostas da sonda chegariam também ao descritor do dispositivo). Antes de cada requisição HID++ o daemon descarta respostas velhas da fila.
- **Só mouses com DPI e polling ajustáveis** (features `0x2201` e `0x8060`) são gerenciados; teclados e outros dispositivos do receptor são ignorados. Um dispositivo cujas capacidades não puderam ser lidas não é listado; ele é tentado de novo a cada ciclo e o erro vai para o log uma vez só.
- **`profiles.toml` que não pôde ser copiado desativa o salvamento**: se o arquivo não pôde ser lido, ou se era inválido e não pôde ser movido para `profiles.toml.bak`, o daemon usa o perfil padrão em memória e recusa salvar até ser reiniciado (cada tentativa vai para o log).
- **Mouse com fio** (sem receptor) ainda não é tratado; só os slots 1–6 de receptores são sondados.
- **O índice `0xFF` do receptor não é sondado** (o próprio receptor não é gerenciado).
- **Mouse dormindo aparece offline**: só é redetectado no ciclo de 5 s; depois de acordar pode levar até um ciclo mais o tempo da varredura para voltar. Se o daemon iniciar com o mouse dormindo, ele fica com 0 dispositivos até o mouse acordar.
- **`ProfileChanged` em criar/apagar** traz o nome do perfil ativo (que não muda); clientes devem chamar `ListProfiles` de novo ao receber o sinal.
- **Timeout não cancela**: um comando que estoura os 5 s devolve `Failed`, mas continua na fila do gerente e ainda pode ser aplicado depois.
- O limite de bateria baixa é fixo na v1 (aviso em 15%, rearma em 20%).

## Pendências para os Planos 3 e 4

1. **Mouse cochilando**: quando o receptor responde `0x09`, o mouse aparece offline e perde a leitura da bateria. Criar um estado "dormindo" que mantém capacidades e bateria, sem `DevicesChanged` oscilando.
2. **Reaplicação a cada despertar**: hoje todo despertar reaplica o perfil e desfaz o DPI escolhido pelo botão. Decidir entre reaplicar só depois de uma reconexão real ou mostrar "DPI do dispositivo difere do perfil".
3. **Métodos D-Bus síncronos**: um comando lento bloqueia os outros clientes. Torná-los assíncronos; um cliente que recebe `Failed` por timeout deve considerar o estado desconhecido e reler em `DeviceChanged`/`ProfileChanged`.
4. **Diagnóstico pelo D-Bus** (`Warnings()` + `WarningsChanged`): `profiles.toml` inválido, falta de permissão udev e falhas ao salvar.
5. **Declarar os sinais no `#[interface]`** para que apareçam na introspecção.
6. **Limite de bateria baixa configurável** (spec 5.3), ou calculado na bandeja.
7. **Teste opcional somente leitura no hardware** (`--features hardware`, spec seção 9) ainda falta.
8. **GUI**: validar o DPI contra `dpi_min`/`dpi_max`/`dpi_step`/`dpi_list`; tratar `dpi_current == 0` com o dispositivo conectado; esconder ou marcar fantasmas com `connected=false` (dispositivos nunca são removidos); reler a lista de perfis em `ProfileChanged` (criar/apagar trazem o nome do ativo).
9. ~~**Abrir a GUI pela bandeja**~~: resolvido no Plano 4 (lançamento por `systemd-run --user --scope`, falhas iniciais reportadas).
10. **Varredura bloqueia o gerente**: a thread única fica presa durante uma varredura lenta e um comando D-Bus nesse meio-tempo pode estourar os 5 s (visto na primeira ativação sob demanda).
11. **Tooltip**: não desenhado pelo Quickshell do usuário (limitação do host); conferir em outros hosts.
12. **Bandeja, pendências dos reviews**: o refresher sobrevive ao fim da bandeja; `Shutdown` redundante antes do `exit(0)`; a drenagem do refresher pode atrasar sob fluxo contínuo de eventos (GUI com slider); rádios habilitados dentro de submenu desabilitado; corrida rara (≈1 % ou menos) em que o refresher aplica uma vista calculada antes da ação; pânico em `build_view`/`execute` encerra só a thread de ações; o valor de 800 ms (`SETTLE_TIMEOUT`) não tem teste de constante; o clique bloqueia toda a thread do `ksni` por até 800 ms; um comentário a ajustar.
13. **Instalador**: o `sed` usa `|` como separador e quebra se o `HOME` contiver `|`.
14. **Pendências menores conhecidas**: a primeira execução com 2+ dispositivos semeia só a partir do primeiro; o `.bak` é sobrescrito por uma segunda corrupção; o arquivo temporário fica para trás depois de um save que falhou e não há `fsync` do diretório; o monitor udev só termina depois do próximo evento udev; permissão negada vai para o log a cada varredura (registrar uma vez por nó e tipo de erro).
