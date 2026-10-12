# Verificação da bandeja, notificações e autostart

- **Data:** 2026-10-12
- **Ambiente:** Hyprland + Quickshell (host da bandeja), dbus-broker, mouse G703 LIGHTSPEED via receptor Lightspeed.
- **Quem:** controlador + usuário, no desktop real.
- **Commits:** base do plano `a1152f9`; correções finais `22add9a..f4e1f56`; correções achadas nesta verificação: `e4b69f7`, `d8004ef`, `7165198`.

## O que foi verificado

### Passo 1 — daemon à mão, só leitura

- `ghubd` iniciado à mão (sem instalar). O item registrou no `StatusNotifierWatcher` (`org.kde.StatusNotifierItem-<pid>-1`).
- O usuário viu o ícone do pinguim e o menu com: linha com nome e bateria; linha "`<dpi>` DPI · `<hz>` Hz"; submenu "DPI"; submenu "Perfil: `<nome>`"; "Iniciar com o sistema" (desabilitado enquanto o serviço não estava instalado); "Abrir G HUB Linux"; "Sair e parar o serviço".
- O tooltip **não** aparece no Quickshell do usuário. A propriedade SNI `ToolTip` foi lida pelo D-Bus e estava correta: título "G703 LIGHTSPEED Wireless Gaming Mouse w/ HERO", descrição "88 % · 1200 DPI". Conclusão: limitação do host (a barra do usuário não desenha tooltips de itens da bandeja).

### Passo 2 — estados do ícone

- Mouse dormindo/desligado: ícone esmaecido, menu com "Inativo ou dormindo" e o submenu de DPI desabilitado; a bateria continua a última conhecida. Ao religar, volta ao normal. Confirmado pelo usuário.

### Passo 3 — notificação

- `ghubd --notify-test` imprimiu "notificação de teste enviada" e o usuário viu a notificação no desktop.
- `--help` imprime o uso; argumento desconhecido imprime o uso e falha.

### Passo 4 — DPI e perfil pelo menu (com autorização; restaurado no fim)

- Criados pelo D-Bus os níveis [800, 1200, 1600] (ativo 1200) e o perfil "Trabalho". O usuário trocou DPI e perfil pelo menu: o DPI do mouse mudou de verdade e o perfil trocou.
- Restauração ao fim: níveis [1200], perfil "Padrão" ativo, perfil "Trabalho" apagado, DPI 1200.

### Passo 5 — instalação e autostart (com autorização; só sob `$HOME`, sem sudo)

- `bash packaging/install-user.sh` instalou `~/.local/bin/ghubd`, `~/.config/systemd/user/ghubd.service` e `~/.local/share/dbus-1/services/org.ghublinux.Daemon.service` (com o `Exec=` já apontando para o caminho absoluto).
- `systemctl --user start ghubd`: serviço `active`, `Type=dbus`, `BusName=org.ghublinux.Daemon`, mouse detectado, item na bandeja; "Iniciar com o sistema" passou a habilitado.
- Clique em "Iniciar com o sistema": `systemctl --user is-enabled ghubd` passou a `enabled` (link `default.target.wants/ghubd.service`); o clique seguinte volta a `disabled`.

## Bugs encontrados e corrigidos

| # | Sintoma | Diagnóstico | Correção |
|---|---|---|---|
| 1 | Depois do primeiro clique num submenu, o menu reaberto mostrava a seleção antiga. | No daemon tudo estava certo (clique simulado pelo `Event` do dbusmenu gerava `ItemsPropertiesUpdated` em < 1 s; `GetLayout` correto). Gravando o barramento durante cliques reais: ao **abrir**, o Quickshell só envia `Event(raiz,"opened")`; ao **clicar** num item de submenu envia `Event(item,"clicked")`, `Event(raiz,"closed")` e, 4 ms depois, `GetLayout`. A atualização real chegava ~200 ms depois e o host a ignorava. | `e4b69f7`: atualização otimista dentro do próprio clique (as bolinhas ficam certas). |
| 2 | As linhas informativas do topo (nome/bateria, DPI/Hz) continuavam velhas. | Mesmo protocolo: o `GetLayout` logo após o clique só via a vista otimista, sem os valores reais. | `d8004ef` ("settle"): o clique envia `Request{action, reply}` e espera até 800 ms o resultado da ação e o menu já reconstruído, que substitui a vista antes de o clique retornar; o `GetLayout` seguinte já vê o estado real. Confirmado pelo usuário. |
| 3 | O ✓ de "Iniciar com o sistema" (item de topo) não mudava no Quickshell. | Ao clicar num item de **topo** o host envia só `Event(clicked)` (sem `closed`, sem `GetLayout`). O daemon enviou o `ItemsPropertiesUpdated(toggle-state)` correto 173 ms depois e o layout estava certo, mas o host manteve o ✓. Mudar o estado pela linha de comando (`systemctl --user disable/enable`, sem clique) também não atualizava: o Quickshell do usuário não aplica `toggle-state` recebido por `ItemsPropertiesUpdated`, mas aplica mudanças estruturais (`LayoutUpdated`). | `7165198`: item invisível no fim do menu que alterna entre 0 e 1 filho a cada mudança visível, forçando o `ksni` a emitir `LayoutUpdated`. O usuário confirmou: sem linha vazia no fim do menu e o ✓ liga e desliga corretamente. |

## Ativação sob demanda

- Com o serviço parado, `busctl --user call org.ghublinux.Daemon ... Devices` falhou com "The name is not activatable" até o barramento reler a configuração: `busctl --user call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus ReloadConfig` (dbus-broker). Depois disso `ListActivatableNames` listou `org.ghublinux.Daemon` e a chamada fez o systemd iniciar o serviço (journal: Starting/Started `ghubd.service`).
- Nessa primeira chamada o cliente recebeu "o gerente de dispositivos não respondeu a tempo": a varredura inicial bloqueou a thread do gerente por mais de 5 s com o mouse meio acordado. Numa segunda medição (serviço parado e iniciado de novo), a primeira resposta do daemon veio 0,34 s após o início.
- Consequência: `packaging/install-user.sh` agora pede `ReloadConfig` ao barramento depois do `daemon-reload` (sem abortar se falhar).

## "Sair e parar o serviço"

- O ícone sumiu e não voltou. `systemctl --user show ghubd`: `ActiveState=inactive`, `SubState=dead`, `Result=success`, `NRestarts=0`, `ExecMainStatus=15` (SIGTERM do `systemctl stop --no-block`); nenhum processo `ghubd`; 0 itens na bandeja.

## Estado final (escolhido pelo usuário)

- Instalado, `systemctl --user enable ghubd` e `systemctl --user start ghubd`: serviço `active`, autostart `enabled`, 1 item na bandeja, mouse conectado (88 %), `profiles.toml` com perfil "Padrão", níveis [1200], polling 1000.

## O que NÃO foi provado

- **Outros hosts de bandeja** (Waybar, KDE etc.): só o Quickshell do usuário foi usado. Que outros hosts desenham o tooltip é uma expectativa, não uma observação.
- **Tooltip**: a propriedade está correta no D-Bus, mas nunca foi vista na tela (limitação do host).
- **Início com a sessão de verdade**: o autostart foi verificado pelo estado da unit (`is-enabled`, link em `default.target.wants`), não por um novo login ou boot. A espera pelo `StatusNotifierWatcher` (login antes do shell) também não foi exercitada.
- **Notificação de bateria baixa**: não disparou de verdade (a bateria estava em 88 %); só o caminho de notificação em geral foi visto com `--notify-test`. O encaminhamento de `BatteryLow` para a notificação é coberto apenas pelos testes de unidade do fan-out.
- **Estados "bateria baixa" e "carregando" do ícone**: não observados no desktop (só "normal" e "desconectado"); cobertos por testes de unidade.
- **"Abrir G HUB Linux" e o clique esquerdo**: a GUI ainda não existe (Plano 4).
- **`ReloadConfig` automático no instalador**: adicionado depois da verificação, com base nela; não foi reexecutado no desktop.
