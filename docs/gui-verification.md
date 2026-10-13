# ghub-gui — verificação manual

Checklist para rodar com o `ghubd` real e o mouse. O smoke automatizado (`docs/gui.md`) cobre a lógica; aqui está o que só olho e hardware confirmam. Antes de começar, anote o DPI atual e os perfis existentes.

## Telas

- [ ] Visão geral, Sensibilidade e Configurações batem com os mocks (cores, fontes, espaçamentos, abas).
- [ ] Layout em **900×600**: nada cortado ou sobreposto em nenhuma tela. Este tamanho nunca foi verificado visualmente de forma automática; conferir com os olhos.
- [ ] Clicar na aba ativa a mantém selecionada.

## Janela e bandeja

- [ ] Bandeja: clique esquerdo / "Abrir" abre a janela.
- [ ] Segunda abertura (de novo pela bandeja ou `ghub-gui` no terminal) traz a janela existente, sem abrir outra.
- [ ] "ghub" aparece no menu de aplicativos, com o ícone legível em 16 px e em tamanhos maiores.
- [ ] Um QML que falha ao carregar encerra o processo com código 1 (nada de processo parado sem janela). Automatizado por `--smoke load-failure`; conferir também no terminal se aparecer `falha ao carregar o QML`.

## Daemon e dispositivo

- [ ] Daemon parado (`systemctl --user stop ghubd`): a tela "desconectado" aparece com o botão "Iniciar serviço"; ao clicar, o serviço sobe e a tela volta ao normal.
- [ ] Mouse dormindo: os controles ficam desabilitados e voltam ao normal quando o mouse acorda. O "lendo…" vem do cenário fake; com o daemon real o `dpi_current` pode ficar defasado (valor antigo) em vez de 0.
- [ ] Abrir pela bandeja com o autostart ativado, após um novo login (confere o ambiente gráfico herdado pelo escopo, pendência 9 de `docs/daemon.md`).
- [ ] O slider grava ao soltar e também após a pausa de 150 ms; conferir no mouse que o DPI muda nas duas situações.
- [ ] DPI inválido ou 0 digitado no campo: rejeitado, o campo volta ao valor do modelo, sem mudar o mouse.
- [ ] Slider arrastado rápido: o valor acompanha o arrasto, mas só **grava ao soltar** (`commitDpi`); sem enxurrada de escritas durante o arrasto.
- [ ] Trocar o DPI pelo botão do mouse: aparece o banner de que o DPI difere do perfil; "Aplicar perfil" reaplica o do perfil e o banner some.

## Iluminação

- [ ] A aba Iluminação bate com o mock (prévia do mouse com o logo aceso, efeitos, cor, velocidade/intensidade, Aplicar). Em 900×600 nada fica cortado.
- [ ] Aplicar fica desabilitado enquanto nada mudou; ao mudar o efeito ele habilita.
- [ ] **Fixo** (ex.: vermelho) → Aplicar: o LED do mouse muda na hora, sem reconectar.
- [ ] **Ciclo de cores** e **Respiração** mudam o LED como esperado; **Desligado** apaga.
- [ ] Mouse dormindo: a aba fica desabilitada; um erro de gravação mostra a mensagem e sai de "Gravando no mouse…".
- [ ] A cópia de segurança aparece em `~/.local/share/ghub-linux/onboard-backup/` só na primeira gravação de cada sessão do daemon.
- [ ] **Ao final, restaurar o ciclo de cores original**: Ciclo de cores, período 8000 ms, intensidade 0 (ou `ghubctl led set cycle --period 8000 --yes` com o `ghubd` parado).

## Perfis e sistema

- [ ] Criar um perfil; criar outro com o mesmo nome (sem distinção de maiúsculas) é recusado.
- [ ] Apagar um perfil que não é o ativo; o ativo não pode ser apagado.
- [ ] Autostart ("Iniciar com o sistema") liga e desliga e reflete o estado da bandeja.

## Ao final: restaurar

- [ ] Voltar o DPI para **1200**.
- [ ] Deixar só o perfil `[1200]` (`Padrão`, `dpi_levels = [1200]`) como ativo; apagar os perfis criados no teste.
- [ ] Religar o `ghubd` se ele foi parado.
