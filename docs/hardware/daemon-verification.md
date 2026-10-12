# ghubd — verificação no hardware real

- **Data:** 2026-10-12 (horários de sinais em UTC; local = UTC-3)
- **Commit:** `7696a89` (build debug, `cargo run -q -p ghubd`)
- **Hardware:** G703 LIGHTSPEED no slot 1 do receptor LIGHTSPEED `046d:c539`
- **ID do dispositivo:** `046dc539-1`
- **Escritas:** autorizadas pelo usuário (passos 3 e 5). Nenhum passo usou `sudo`.

Todas as chamadas abaixo têm o prefixo
`busctl --user call org.ghublinux.Daemon /org/ghublinux/Daemon org.ghublinux.Daemon1`.

## Passo 1 — subir o daemon (somente leitura)

O daemon foi iniciado com o mouse **dormindo**:

- Log: `ghubd: pronto em org.ghublinux.Daemon`.
- `Devices` → `a(ssbis) 0` (nenhum dispositivo).
- `ghubctl probe` no mesmo momento: o slot 1 do receptor respondia erro HID++ 1.0 `0x09` (dispositivo inalcançável/dormindo); slots 2–6 e `0x00`: `0x08`; `0xff`: `0x01`.

Depois de o usuário mexer o mouse (em no máximo um ciclo de 5 s):

- `Devices` → 1 dispositivo:
  `"046dc539-1" "G703 LIGHTSPEED Wireless Gaming Mouse w/ HERO" true 91 "discharging"`
  (nome lido da feature DeviceName `0x0005`; bateria 91%, igual ao sysfs).
- `GetDeviceState s 046dc539-1`: `connected=true`, `dpi_current=1200`, `dpi_levels=[1200]`, `active_level=0`, `dpi_min=100`, `dpi_max=25600`, `dpi_step=50`, `dpi_list=[]`, `polling_hz=1000`, `polling_supported=[1000,500,250,125]`, bateria `91` `"discharging"`, perfil `"Padrão"`.
- `~/.config/ghub-linux/profiles.toml` criado e semeado:

  ```toml
  active = "Padrão"

  [[profile]]
  name = "Padrão"
  dpi_levels = [1200]
  active_level = 0
  polling_hz = 1000
  ```

  (conteúdo conforme os campos registrados; a formatação exata do arquivo não foi copiada)
- O DPI do mouse permaneceu 1200: nenhuma escrita na primeira execução.

| Esperado | Resultado |
|---|---|
| `Devices` lista `046dc539-1` conectado | OK (depois de o mouse acordar; ver divergência 1) |
| `battery_percent` igual ao `capacity` do sysfs | OK (91) |
| `dpi_current` igual ao DPI real, sem escrita | OK (1200) |
| `dpi_min=100`, `dpi_max=25600`, `dpi_step=50` | OK |
| polling `[125,250,500,1000]` | Mesmos valores, outra ordem: `[1000,500,250,125]` (ver divergência 2) |
| `profiles.toml` semeado com `dpi_levels = [<DPI atual>]` | OK (`[1200]`) |
| DPI do mouse não mudou | OK |

## Passo 2 — bateria e monitor de sinais

- `busctl --user monitor org.ghublinux.Daemon` funcionou e gravou os sinais do passo 5 (rodada B).
- Bateria: `battery_percent` 91 igual ao sysfs. O `battery_state` informado foi `"discharging"`; a comparação explícita com `/sys/class/power_supply/hidpp_battery_0/status` não foi registrada.

## Passo 3 — escrita controlada

| # | Chamada | Resultado |
|---|---|---|
| 1 | `SetDpiLevels sauu 046dc539-1 3 800 1200 1600 1` | OK; `dpi_current` continuou 1200 (o nível ativo 1 é 1200, igual ao atual: nenhuma escrita real). |
| 2 | `SetActiveDpiLevel su 046dc539-1 0` | OK; `GetDeviceState` → `dpi_current=800` (lido do dispositivo): escrita real confirmada. |
| 3 | `SetDpiLevels sauu 046dc539-1 1 125 0` | Erro `DPI 125 não é suportado por este dispositivo`; DPI continuou 800. |
| 4 | `SetPollingRate su 046dc539-1 333` | Erro `taxa de polling 333 Hz não é suportada por este dispositivo`. |
| 5 | `SetActiveDpiLevel su 046dc539-1 9` | Erro `o nível ativo está fora da lista de níveis`; estado inalterado. |

| Esperado | Resultado |
|---|---|
| 1ª chamada tem sucesso e `dpi_current` fica 1200 | OK |
| `SetActiveDpiLevel 0` muda para 800 | OK |
| 125 DPI (fora da grade) falha com `InvalidArgs` e o DPI continua 800 | Falhou com a mensagem acima e o DPI continuou 800. O nome do erro D-Bus não foi registrado; pelo código (`service.rs`), essa mensagem vem de `ManagerError::Invalid`, mapeado para `InvalidArgs`. |

As chamadas 4 e 5 não estavam no roteiro e foram acrescentadas.

## Passo 5 — reconexão

O teste precisou de duas rodadas, e em ambas o mouse ficou desligado pela chave **por 15 s ou mais** antes de ser religado. A detecção de queda e de volta roda no ciclo de 5 s do daemon.

### Rodada A — inconclusiva

Perfil em 800; DPI mudado pelo botão do mouse para 3200; mouse desligado ≥ 15 s e religado. Voltou com 800.

Inconclusiva: 800 é também o DPI padrão de fábrica do mouse (o probe mostra "padrão 800"), então não dá para distinguir "perfil reaplicado" de "mouse voltou ao padrão". A rodada mostrou, porém, que o mouse **não** preserva o DPI ao ser desligado (3200 → 800).

### Rodada B — decisiva

Perfil em 1600 (`SetActiveDpiLevel su 046dc539-1 2`), `busctl --user monitor org.ghublinux.Daemon` gravando, mouse desligado e religado cerca de 20 s depois. O mouse voltou com `dpi_current=1600`, diferente do padrão de fábrica (800).

Sinais gravados (UTC):

| Horário | Sinais | Interpretação |
|---|---|---|
| 16:22:26.6 e 16:22:27.2 | `DeviceChanged("046dc539-1")` + `DevicesChanged` | Mouse acordando do sono por inatividade, detectado. |
| 16:22:53.0 | `DeviceChanged` + `DevicesChanged`, e 21 ms depois outro `DeviceChanged` | Mouse desligado: `ping` falhou, marcado offline; bateria passou a desconhecida. |
| 16:23:13.4 | `DeviceChanged` + `DevicesChanged` + `DeviceChanged` | Mouse religado: redescoberto e perfil reaplicado. |

Conclusão: o daemon detectou a queda e a volta e reaplicou o perfil (1600 DPI).

| Esperado | Resultado |
|---|---|
| `Devices` mostra `connected=false` e depois `true` | OK, registrado pelos sinais acima (offline com o mouse desligado, redescoberto ao religar) |
| `GetDeviceState` volta ao DPI do perfil | OK na rodada B (1600). A rodada A (800) é inconclusiva. |
| Em até ~10 s | O roteiro foi ajustado: o mouse precisou ficar desligado ≥ 15 s; a detecção segue o ciclo de 5 s. |

### Observado também

Quando o mouse dormiu por inatividade, o daemon o marcou offline (`connected=false`, `dpi_current=0`, bateria `-1`/`"unknown"`), e `SetActiveDpiLevel` devolveu `o dispositivo 046dc539-1 está offline`.

## Restauração e encerramento

`SetDpiLevels sauu 046dc539-1 1 1200 0` → `dpi_current=1200`, `profiles.toml` com `dpi_levels=[1200]`. Daemon encerrado (nenhum nome `org.ghublinux` restou no barramento).

## Divergências e observações

1. **Mouse dormindo na partida:** o nó do mouse e o receptor devolvem `0x09` no slot 1; o daemon fica com 0 dispositivos e varre a cada 5 s até o mouse acordar. Esperado pelo desenho (ciclo de 5 s), mas o roteiro presumia o mouse acordado.
2. **Ordem de `polling_supported`:** `[1000,500,250,125]`, a ordem em que o dispositivo informa, e não `[125,250,500,1000]` como no roteiro. Mesmos valores.
3. **Bateria ao ficar offline:** a associação com o sysfs é perdida até a próxima varredura bem-sucedida (pendência menor já registrada).
4. **Nome do erro D-Bus** da rejeição de 125 DPI não foi registrado (só a mensagem).
5. **Reconexão:** precisou do mouse desligado ≥ 15 s e de uma segunda rodada com DPI diferente do padrão de fábrica para ser conclusiva.
