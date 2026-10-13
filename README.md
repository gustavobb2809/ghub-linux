# ghub-linux

Configure mouses Logitech no Linux sem o G HUB: DPI, taxa de polling, bateria, perfis, bandeja do sistema e iluminação do LED. Feito e testado com o **G703 Lightspeed**; outros dispositivos não foram verificados.

- `ghubd`: serviço de usuário que fala HID++ 2.0 com o mouse (hidraw) e expõe uma API D-Bus. Também mantém o ícone da bandeja.
- `ghub-gui`: janela Qt 6/QML (cxx-qt) com Visão geral, Sensibilidade, Iluminação e Configurações.
- `ghubctl`: linha de comando de diagnóstico (`probe`, `led get|set|restore`).
- `hidpp`: biblioteca Rust do protocolo HID++ 2.0.

## Instalar (usuário)

```bash
packaging/install-user.sh          # compila e instala sob ~/.local
systemctl --user start ghubd       # ou use "Iniciar com o sistema" no menu da bandeja
```

Para o `ghub-gui`: Qt 6 (`qt6-qtbase-devel`, `qt6-qtdeclarative-devel` no Fedora), `cmake` e um compilador C++. Para acessar o hidraw sem root, instale a regra udev de `docs/udev/`. Detalhes em [docs/daemon.md](docs/daemon.md) e [docs/gui.md](docs/gui.md).

## Aviso sobre o LED

O efeito e a cor do LED são **gravados na memória (flash) do mouse**, no perfil onboard ativo. O programa confere o CRC, salva uma cópia do setor em `~/.local/share/ghub-linux/onboard-backup/` e confere o que gravou, mas gravar na flash sempre tem algum risco. Veja [docs/onboard-profiles.md](docs/onboard-profiles.md).

## Licença

Copyright (C) 2026 Gustavo Barbosa.

Este programa é software livre: você pode redistribuí-lo e/ou modificá-lo sob os termos da **GNU General Public License**, versão 3 ou (a seu critério) qualquer versão posterior, publicada pela Free Software Foundation. Distribuído na esperança de ser útil, mas **sem nenhuma garantia**. O texto completo está em [LICENSE](LICENSE).

## Marcas

Logitech, G HUB, G703 e Lightspeed são marcas de seus respectivos titulares. Este projeto é independente e **não é afiliado, patrocinado nem endossado pela Logitech**.
