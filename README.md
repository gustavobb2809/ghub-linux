# ghub-linux

Alternativa nativa para Linux ao Logitech G HUB. Projeto em desenvolvimento.

Design: `docs/superpowers/specs/2026-10-12-ghub-linux-design.md`.

## Daemon

`ghubd` é o daemon de sessão que detecta os mouses, aplica o perfil ativo e se publica no D-Bus de sessão como `org.ghublinux.Daemon`. Como compilar, instalar, usar e as limitações da v1: `docs/daemon.md`.

O daemon também mostra um ícone na bandeja (DPI, perfil, "Iniciar com o sistema"), envia notificações de bateria baixa e pode iniciar com a sessão ou sob demanda pelo D-Bus: ver `docs/daemon.md`.
