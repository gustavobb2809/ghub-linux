# ghub-linux

Configure mouses Logitech no Linux sem o G HUB: DPI, taxa de polling, distância de levantamento (lift-off), bateria, perfis, bandeja do sistema e iluminação do LED. Feito e testado com o **G703 Lightspeed** (todos os recursos) e o **G Pro X2** (DPI, polling de 125 a 8000 Hz, distância de levantamento (lift-off) e bateria; sem iluminação, pois o mouse não tem LED). Outros dispositivos não foram verificados.

- `ghubd`: serviço de usuário que fala HID++ 2.0 com o mouse (hidraw) e expõe uma API D-Bus. Também mantém o ícone da bandeja (DPI, perfil, "Iniciar com o sistema"), envia notificações de bateria baixa e pode iniciar com a sessão ou sob demanda pelo D-Bus.
- `ghub-gui`: janela Qt 6/QML (cxx-qt) com Visão geral, Sensibilidade, Iluminação e Configurações.
- `ghubctl`: linha de comando de diagnóstico (`probe`, `led get|set|restore`).
- `hidpp`: biblioteca Rust do protocolo HID++ 2.0.

Design: [docs/superpowers/specs/2026-10-12-ghub-linux-design.md](docs/superpowers/specs/2026-10-12-ghub-linux-design.md) (v1) e [2026-10-13-iluminacao-v1.1-design.md](docs/superpowers/specs/2026-10-13-iluminacao-v1.1-design.md) (iluminação).

## Instalar (usuário)

```bash
packaging/install-user.sh          # compila e instala sob ~/.local
systemctl --user start ghubd       # ou use "Iniciar com o sistema" no menu da bandeja
```

Para o `ghub-gui`: Qt 6 (`qt6-qtbase-devel`, `qt6-qtdeclarative-devel` no Fedora), `cmake` e um compilador C++. Para acessar o hidraw sem root, instale a regra udev de `docs/udev/`. Detalhes em [docs/daemon.md](docs/daemon.md) e [docs/gui.md](docs/gui.md).

## Distância de levantamento (lift-off)

Em mouses que permitem ajustar (hoje o G Pro X2), a aba Sensibilidade mostra o card **Distância de levantamento** com três níveis: Baixa, Média e Alta. É a altura em que o sensor para de rastrear ao levantar o mouse. O valor fica no perfil ativo, como a taxa de polling, e o `ghubd` o reaplica sempre que o mouse reconecta. Um perfil sem lift-off definido não mexe no que está no mouse.

Pelo D-Bus: `LiftOffLevels` (vazio quando não há suporte), `GetLiftOff` e `SetLiftOff` com `low`, `medium` ou `high`.

```bash
busctl --user call org.ghublinux.Daemon /org/ghublinux/Daemon org.ghublinux.Daemon1 SetLiftOff ss <id> low
```

## Modo jogo (polling máximo)

Um modo jogo pode subir o mouse para a maior taxa de polling e voltar à configuração normal ao sair, usando só a API D-Bus de perfis. Ao entrar:

```bash
D="busctl --user call org.ghublinux.Daemon /org/ghublinux/Daemon org.ghublinux.Daemon1"
$D GetActiveProfile                   # guarde o nome para restaurar depois
$D CreateProfile s "Modo jogo"        # copia o perfil ativo
$D ActivateProfile s "Modo jogo"
$D SetPollingRate su <id> 8000        # a maior taxa de polling_supported em GetDeviceState
```

Ao sair, reative o perfil guardado e apague o temporário:

```bash
$D ActivateProfile s "<perfil guardado>"
$D DeleteProfile s "Modo jogo"
```

A troca de perfil funciona com o mouse dormindo, porque o `ghubd` aplica quando ele acorda. Assim a volta não depende do mouse estar ativo. Já o `SetPollingRate` exige o mouse conectado. Enquanto o modo estiver ligado, a GUI mostra o perfil "Modo jogo", e mudanças de DPI feitas nele se perdem ao sair. O modo jogo do [ricehyprland](https://github.com/gustavobb2809/ricehyprland) (`gamemode-set on --mouse=1`) segue esse fluxo.

## Aviso sobre o LED

O efeito e a cor do LED são **gravados na memória (flash) do mouse**, no perfil onboard ativo. O programa confere o CRC, salva uma cópia do setor em `~/.local/share/ghub-linux/onboard-backup/` e confere o que gravou, mas gravar na flash sempre tem algum risco. Veja [docs/onboard-profiles.md](docs/onboard-profiles.md).

## Licença

Copyright (C) 2026 Gustavo Barbosa.

Este programa é software livre: você pode redistribuí-lo e/ou modificá-lo sob os termos da **GNU General Public License**, versão 3 ou (a seu critério) qualquer versão posterior, publicada pela Free Software Foundation. Distribuído na esperança de ser útil, mas **sem nenhuma garantia**. O texto completo está em [LICENSE](LICENSE).

## Marcas

Logitech, G HUB, G703, G Pro X2 e Lightspeed são marcas de seus respectivos titulares. Este projeto é independente e **não é afiliado, patrocinado nem endossado pela Logitech**.
