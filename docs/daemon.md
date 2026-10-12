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

O script compila em modo release, instala o binário em `~/.local/bin/ghubd` e a unit em `~/.config/systemd/user/ghubd.service` e roda `systemctl --user daemon-reload`. Ele **não** habilita nem inicia o serviço:

```bash
systemctl --user start ghubd    # inicia agora
systemctl --user enable ghubd   # inicia com a sessão (opcional)
```

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
- A escrita é atômica (arquivo temporário, `fsync`, `rename`). Um arquivo inválido é renomeado para `profiles.toml.bak` e o daemon começa com o perfil padrão (avisando no stderr).

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
- A bateria vem do sysfs do kernel (`/sys/class/power_supply/hidpp_battery_*`, criado pelo driver `hid-logitech-hidpp`), não do HID++.

## Verificado no hardware

Com o G703 LIGHTSPEED (receptor `046d:c539`) em 2026-10-12, commit `7696a89` (detalhes em `docs/hardware/daemon-verification.md`):

- Detecção do mouse, nome lido do dispositivo e bateria igual à do sysfs.
- Primeira execução semeia o perfil a partir do DPI/polling atuais sem escrever no mouse.
- `SetActiveDpiLevel` escreve o DPI de verdade (confirmado pela releitura do dispositivo).
- DPI fora da grade, polling não anunciado e nível ativo fora da lista são rejeitados sem mudar o mouse.
- Mouse desligado e religado: o daemon marca offline, redescobre e reaplica o perfil (1600 DPI voltou, embora o mouse volte em 800 sozinho).
- Mouse dormindo por inatividade aparece offline e é redetectado ao acordar; escrita num dispositivo offline falha com `o dispositivo <id> está offline`.

## Limitações da v1

- **Um perfil global**: o perfil ativo vale para todos os dispositivos; não há perfil por dispositivo.
- **Níveis de DPI são do host**: `dpi_levels` são presets guardados no `profiles.toml`; só o DPI ativo é escrito no mouse. Nada vai para a memória onboard do mouse; o botão de DPI do mouse continua alternando entre os níveis guardados no próprio mouse (na verificação, o botão levou o DPI a 3200, valor fora do perfil), e o daemon não desfaz essa mudança até reaplicar o perfil (por exemplo, ao reconectar).
- **Bateria só em receptor com exatamente um dispositivo e uma bateria** no sysfs; caso contrário a bateria fica desconhecida (`-1`/`unknown`). Quando o dispositivo fica offline, a associação com a bateria só volta na próxima varredura bem-sucedida.
- **Mouse com fio** (sem receptor) ainda não é tratado; só os slots 1–6 de receptores são sondados.
- **O índice `0xFF` do receptor não é sondado** (o próprio receptor não é gerenciado).
- **Mouse dormindo aparece offline**: só é redetectado no ciclo de 5 s; depois de acordar pode levar até um ciclo mais o tempo da varredura para voltar. Se o daemon iniciar com o mouse dormindo, ele fica com 0 dispositivos até o mouse acordar.
- **`ProfileChanged` em criar/apagar** traz o nome do perfil ativo (que não muda); clientes devem chamar `ListProfiles` de novo ao receber o sinal.
- **Timeout não cancela**: um comando que estoura os 5 s devolve `Failed`, mas continua na fila do gerente e ainda pode ser aplicado depois.
- Sem bandeja, notificação visual ou GUI ainda (planos seguintes); o limite de bateria baixa (15%) é fixo.
