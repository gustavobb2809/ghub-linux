# Perfis onboard do G703 LS (spike de iluminação)

Achados do spike de 2026-10-13, lidos do mouse (somente leitura). Backup bruto em `onboard-g703ls-backup.txt`.

## Feature 0x8071 (RgbEffects) — controle em tempo real **não funcionou**
- 2 zonas (locais 1 e 2), 4 efeitos por zona: `0x0000` desligado, `0x0001` fixo, `0x0003` ciclo, `0x000a` respiração.
- Comandos aceitos sem erro, mas sem mudança visível no LED: `fn5 [01 03 05]` (assumir controle), `fn1 [zona, 01, R,G,B, 0…, 01]` (efeito fixo), com e sem trocar a `0x8100` para modo host (`fn1 [02]`). `fn3` é recusada (erro 0x02).
- Em modo onboard (`0x8100 fn2` = `01`), o mouse roda o efeito gravado no perfil.

## Feature 0x8100 (OnboardProfiles)
- `fn0` (descrição): `01 04 01 05 01 06 10 00 ff 0a 04 00` → memória modelo 1, formato de perfil 4, 5 perfis, 6 botões, 16 setores de 255 bytes.
- `fn5 [setor(2), offset(2)]` lê 16 bytes. Setor 0 é o diretório (perfis nos setores 1–5); `0x0100–0x0101` são ROM.
- Cada setor termina em CRC-16/CCITT-FALSE (poli 0x1021, init 0xFFFF, big-endian) dos 253 bytes anteriores. Verificado em todos os setores 0–5.
- Perfil (setor 1, offsets em hex): `00` taxa (01 = 1000 Hz), `01` DPI padrão, `03..` DPIs LE (400/800/1200/3200), `0d..0f` cor do perfil (ff ff ff), `20..` botões (4 bytes cada: `80 01 00 01` = botão 1 …; `90 05` = ação especial), `d0/db/e6/f1` quatro entradas de LED de 11 bytes.
- LED atual (as 4 entradas iguais): `03 00 00 00 00 00 1f 40 00 00 00` = **ciclo de cores, período 0x1f40 = 8000 ms**, intensidade 0 (100%). Isso é o que o usuário vê.
- Para efeito fixo, o layout da entrada (libratbag) é: `01, R, G, B, 00…`; respiração (`0a`): cor(3), período(2), forma de onda(1), intensidade(1). Ainda **não testado** no hardware.

## Caminho provável para a v1.1 (a validar)
Gravar o LED do perfil: ler o setor, mudar as entradas de LED, recalcular o CRC e regravar, lendo de volta para conferir. Requer backup (este arquivo) e escrita só após aprovação. Não serve para sincronização contínua com as cores do sistema (gravação na flash).
