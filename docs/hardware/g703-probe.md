# G703 LS — probe HID++ no hardware real

- **Data:** 2026-10-12
- **Comando:** `cargo run -q -p ghubctl -- probe` (somente leitura, nenhum setter chamado)
- **Hardware:** receptor LIGHTSPEED `046d:c539` (`/dev/hidraw4`) + G703 LS `046d:4086` (`/dev/hidraw8`, nó virtual criado pelo `hid-logitech-dj`)
- **Permissão:** regra `docs/udev/70-ghub-linux.rules` instalada em `/etc/udev/rules.d/`

## Saída completa

```
/dev/hidraw4  046d:c539  Logitech USB Receiver
  índice 0x01: HID++ 4.2
    features:
      0x0001 (índice 1) IFeatureSet
      0x0003 (índice 2) DeviceFwVersion
      0x0005 (índice 3) DeviceName
      0x1d4b (índice 4) ?
      0x0020 (índice 5) ?
      0x1001 (índice 6) ?
      0x8071 (índice 7) RgbEffects
      0x8100 (índice 8) OnboardProfiles
      0x8110 (índice 9) ?
      0x8060 (índice 10) ReportRate
      0x2201 (índice 11) AdjustableDpi
      0x00c2 (índice 12) ?
      0x1802 (índice 13) ?
      0x1803 (índice 14) ?
      0x1805 (índice 15) ?
      0x1806 (índice 16) ?
      0x1811 (índice 17) ?
      0x1830 (índice 18) ?
      0x1890 (índice 19) ?
      0x1891 (índice 20) ?
      0x18a1 (índice 21) ?
      0x1801 (índice 22) ?
      0x18b1 (índice 23) ?
      0x1df3 (índice 24) ?
      0x1e00 (índice 25) ?
      0x1eb0 (índice 26) ?
      0x1863 (índice 27) ?
      0x1e22 (índice 28) ?
    bateria: o dispositivo não suporta a feature 0x1000
    DPI atual: 1200 (padrão 800)
    DPI suportado: Range { min: 100, max: 25600, step: 50 }
    polling atual: 1000 Hz
    polling suportado: [1000, 500, 250, 125] Hz
  índice 0x02: erro HID++ 1.0 0x08 (dispositivo não fala HID++ 2.0)
  índice 0x03: erro HID++ 1.0 0x08 (dispositivo não fala HID++ 2.0)
  índice 0x04: erro HID++ 1.0 0x08 (dispositivo não fala HID++ 2.0)
  índice 0x05: erro HID++ 1.0 0x08 (dispositivo não fala HID++ 2.0)
  índice 0x06: erro HID++ 1.0 0x08 (dispositivo não fala HID++ 2.0)
  índice 0xff: erro HID++ 1.0 0x01 (dispositivo não fala HID++ 2.0)
  índice 0x00: erro HID++ 1.0 0x08 (dispositivo não fala HID++ 2.0)
/dev/hidraw8  046d:4086  Logitech G703 LS
  índice 0x01: HID++ 4.2
    falha ao listar features: o dispositivo não suporta a feature 0x0001
  índice 0x02: tempo esgotado esperando o dispositivo
  índice 0x03: tempo esgotado esperando o dispositivo
  índice 0x04: tempo esgotado esperando o dispositivo
  índice 0x05: tempo esgotado esperando o dispositivo
  índice 0x06: tempo esgotado esperando o dispositivo
  índice 0xff: tempo esgotado esperando o dispositivo
  índice 0x00: tempo esgotado esperando o dispositivo
```

## Resumo

- **Nó que funcionou:** `/dev/hidraw4` (receptor `046d:c539`), **índice de dispositivo `0x01`**.
- **Protocolo:** HID++ **4.2**.
- **Outros índices no receptor (`0x02`–`0x06`, `0x00`):** erro HID++ 1.0 `0x08` (sem dispositivo pareado nesse slot). `0xff` (o próprio receptor): erro HID++ 1.0 `0x01`, ou seja, o receptor fala HID++ 1.0 e não 2.0.
- **Nó do mouse `/dev/hidraw8`:** o índice `0x01` responde ao ping (HID++ 4.2), mas `IRoot.getFeature(0x0001)` não devolve índice, então a listagem de features falha; os demais índices dão timeout. Para este hardware, usar o nó do receptor.

## Hipótese vs. observado

| Feature | Uso no plano | Hipótese | Observado | Índice |
|---|---|---|---|---|
| `0x1000` | bateria (Battery Level Status) | uma de `0x1000`/`0x1004` | **ausente** | — |
| `0x1004` | bateria (Unified Battery) | uma de `0x1000`/`0x1004` | **ausente** | — |
| `0x2201` | DPI ajustável | presente | presente | 11 |
| `0x8060` | taxa de report | presente | presente | 10 |
| `0x8071` | RGB (futuro) | presente | presente | 7 |
| `0x8100` | perfis onboard (futuro) | presente | presente | 8 |
| `0x1B04` | remapeamento de botões (futuro) | presente | **ausente** | — |

## Valores lidos e contradições com as hipóteses

- **Bateria:** nem `0x1000` nem `0x1004` existem. O mouse anuncia **`0x1001`** (Battery Voltage, índice 6, pelo conhecimento público do protocolo). A API de bateria atual (`0x1000`) não funciona neste aparelho; é preciso implementar `0x1001` (tensão em mV + estado de carga, convertida para porcentagem por curva).
- **DPI (`0x2201`):** faixa `100`–`25600`, passo `50` (formato range, não lista). DPI atual `1200`, padrão `800`.
- **Polling (`0x8060`):** atual `1000 Hz`; suportados `1000, 500, 250, 125 Hz`.
- **Botões:** `0x1B04` ausente. O remapeamento terá de passar pelos perfis onboard (`0x8100`) ou outra feature; a investigar na fase v1.2.
- **Outras features anunciadas, sem nome no `ghubctl` (nomes pelo conhecimento público, não verificados):** `0x1d4b` (Wireless Device Status), `0x0020` (Config Change), `0x8110` (Mouse Button Spy), `0x00c2` (DFU Control), `0x1802`–`0x18b1`, `0x1df3`, `0x1e00`, `0x1eb0`, `0x1e22` (features internas/diagnóstico do fabricante, não usar).
