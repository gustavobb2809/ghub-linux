# ghub-linux v1.1 — Iluminação (LED do G703 LS)

Data: 2026-10-13. Complementa `2026-10-12-ghub-linux-design.md` (fase v1.1). Base técnica: `docs/onboard-profiles.md` (spike no hardware) e `docs/onboard-g703ls-backup.txt` (backup bruto da memória onboard).

## 1. Objetivo

Escolher o efeito e a cor do LED do G703 LS pela aba Iluminação e tê-los aplicados de fato no mouse.

**Fora do escopo:** sincronizar o LED continuamente com as cores do sistema (fase posterior, reaproveitando a mesma escrita), apagar o LED ao suspender, botões/macros (v1.2), edição de DPI/taxa do perfil onboard, perfis onboard além do ativo.

## 2. Fatos do hardware que moldam o desenho

- O controle em tempo real (`0x8071`) não alterou o LED em nenhuma combinação testada. O LED é definido no **perfil onboard** (`0x8100`), em flash.
- Cada setor de perfil tem 255 bytes e termina em CRC-16/CCITT-FALSE (poli `0x1021`, init `0xFFFF`, big-endian) dos 253 bytes anteriores.
- O perfil tem 4 entradas de LED de 11 bytes nos offsets `0xd0`, `0xdb`, `0xe6`, `0xf1`. Efeito fixo vermelho (`01 ff 00 00 00…`) gravado nas 4 e recarregado com `fn3 [00 01]` mudou o LED na hora (validado). Ciclo original: `03 00×5 1f 40 00 00 00` (período 8000 ms).
- Leitura: `fn5 [setor(2), offset(2)]`, 16 bytes por chamada (última em offset 239). Escrita: `fn6 [setor(2), 0000, 00ff]`, `fn7` (16 bytes por chamada, último bloco completado com `ff`), `fn8`.
- Não confirmados no hardware: respiração (`0a`) e desligado (`00`).

## 3. Decisões

- A gravação vale só para o **perfil ativo** (`0x8100 fn4`).
- Gravar é explícito (botão **Aplicar**): cada gravação usa a flash do mouse.
- O LED vive no mouse; o host não guarda nem reaplica nada (`profiles.toml` fica intocado).
- Abordagem A: setor tratado como bytes opacos com uma "visão" de LED. Só os bytes das 4 entradas de LED (e o CRC) mudam.

## 4. `hidpp` — módulo `features::onboard`

- `read_sector(dev, s)`, `write_sector(dev, s, bytes)` com o protocolo acima; `active_sector(dev)` via `fn4`.
- `crc16(&[u8])`, `verify(&[u8; 255])`, `seal(&mut [u8; 255])`.
- `Led`: `Off`, `Fixed { rgb }`, `Cycle { period_ms, intensity }`, `Breathing { rgb, period_ms, waveform, intensity }`. `Led::parse(&[u8; 11])` e `Led::encode() -> [u8; 11]`. Layouts: fixo = `01, R, G, B, 00…`; ciclo = `03, 00×5, período(2, BE), intensidade, 00, 00` (intensidade 0 = 100%); respiração = `0a, R, G, B, período(2, BE), forma de onda, intensidade, 00`; desligado = `00, 00…`.
- `read_led(dev) -> Led` (lê o setor ativo, valida o CRC, interpreta a primeira entrada) e `write_led(dev, led)`.
- Erros tipados novos em `HidppError`: setor com CRC inválido, leitura de volta divergente, perfil sem entradas de LED reconhecíveis.

**Salvaguardas de escrita (no módulo, nunca na GUI):**
1. Setor lido com CRC inválido → recusa gravar.
2. Na primeira gravação de uma sessão do daemon, copia o setor para `~/.local/share/ghub-linux/onboard-backup/<id>-<setor>-<timestamp>.bin`.
3. Após gravar, relê e compara; se divergir, regrava a cópia e devolve erro.
4. Só altera os bytes das 4 entradas de LED e o CRC. Nada mais do setor muda.

## 5. `ghubd` e D-Bus

- `Mouse` ganha `led() -> Result<Led, ManagerError>` e `set_led(Led)`. Backend real chama o módulo; `FakeMouse` guarda em memória. Sem `0x8100` anunciado: "não suportado".
- `DeviceState` ganha `led: Option<LedInfo>` (efeito atual e efeitos suportados); `None` quando o mouse não suporta. A GUI esconde a aba nesse caso.
- `Command::GetLed { id, reply }` e `Command::SetLed { id, led, reply }`, tratados na thread gerente como `SetPolling`.
- D-Bus (`org.ghublinux.Daemon1`): `GetLed(s id) -> (s effect, s color, u period_ms, u intensity)` e `SetLed(s id, s effect, s color, u period_ms, u intensity)`; `InvalidArgs` para efeito não suportado ou cor inválida; `DeviceChanged(id)` após gravar. Referência final em `docs/daemon.md`.
- A gravação é lenta; medir se cabe no timeout de 5 s do `Failed` e, se não couber, ampliar o timeout só dessa chamada.
- Falha de gravação: notificação pelo caminho existente (`org.freedesktop.Notifications`) e mensagem na GUI; a cópia de segurança permanece em disco.

## 6. GUI — aba Iluminação

A partir do mock `Iluminacao.dc.html`:
- Prévia do mouse à esquerda (efeito e cor antes de aplicar; animada para ciclo e respiração); Efeito, Cor e Intensidade à direita; botão **Aplicar**.
- Efeito: Fixo, Respiração, Ciclo de cores, Desligado, limitado ao que o mouse suporta.
- Cor (paleta Gruvbox + hex) só em Fixo e Respiração. Intensidade e período só em Respiração e Ciclo (o Fixo não tem campo de brilho).
- Sem o toggle "apagar ao suspender"; sem o selo "v1.1" (aba habilitada).
- Aplicar fica desabilitado sem alteração; mostra "Gravando no mouse…" e depois confirmação ou erro; aviso curto de que a mudança é gravada no mouse.

## 7. Testes

- `hidpp`: memória onboard simulada no `FakeTransport`; CRC, codificação/decodificação de cada efeito, recusa com CRC inválido, regravação do backup quando a leitura de volta diverge.
- `ghubd`: `FakeMouse` com `Led`, os dois comandos, `InvalidArgs`.
- GUI: cenário `--smoke lighting` e item em `docs/gui-verification.md`.
- Hardware (com aprovação a cada gravação): Fixo, Ciclo, Respiração e Desligado, conferidos olhando o mouse; restauração do ciclo original pelo backup ao final.

## 8. Ordem de entrega

1. `hidpp::features::onboard` (+ testes). 2. `ghubctl led get|set` (teste no mouse sem GUI). 3. `ghubd` + D-Bus. 4. Aba na GUI. 5. Docs e verificação manual.

## 9. Riscos

- Respiração e Desligado sem confirmação de layout: validar no passo 2 antes de expor na GUI; se um falhar, o efeito sai da lista de suportados.
- Desgaste da flash: gravação apenas por clique em Aplicar; sem gravações automáticas.
- Perfis ativos diferentes no mouse e no host: o LED aplicado fica no perfil ativo no momento da gravação.
