#!/usr/bin/env bash
# Instala o ghubd para o usuário atual (sem sudo). Não habilita o serviço.
#   --dry-run   só imprime o que seria feito
set -euo pipefail

DRY=0
if [ "${1:-}" = "--dry-run" ]; then
    DRY=1
elif [ -n "${1:-}" ]; then
    echo "uso: $0 [--dry-run]" >&2
    exit 2
fi

root="$(cd "$(dirname "$0")/.." && pwd)"
bindir="$HOME/.local/bin"
unitdir="$HOME/.config/systemd/user"
dbusdir="$HOME/.local/share/dbus-1/services"

run() {
    if [ "$DRY" = 1 ]; then
        echo "+ $*"
    else
        "$@"
    fi
}

run cargo build --release -p ghubd --manifest-path "$root/Cargo.toml"
run install -Dm755 "$root/target/release/ghubd" "$bindir/ghubd"
run install -Dm644 "$root/packaging/systemd/ghubd.service" "$unitdir/ghubd.service"

# O arquivo de ativação do D-Bus não expande variáveis: grava o caminho absoluto.
if [ "$DRY" = 1 ]; then
    echo "+ sed 's|@BINDIR@|$bindir|' $root/packaging/dbus/org.ghublinux.Daemon.service > $dbusdir/org.ghublinux.Daemon.service"
else
    mkdir -p "$dbusdir"
    sed "s|@BINDIR@|$bindir|" "$root/packaging/dbus/org.ghublinux.Daemon.service" \
        > "$dbusdir/org.ghublinux.Daemon.service"
fi

run systemctl --user daemon-reload

# O dbus-broker só enxerga um arquivo de ativação novo depois de reler a
# configuração (ou de um novo login). Falhar aqui não é fatal.
reload_bus() {
    if [ "$DRY" = 1 ]; then
        echo "+ $*"
        return 0
    fi
    if ! "$@"; then
        echo "aviso: não foi possível pedir ao barramento da sessão que releia a configuração;" >&2
        echo "       a ativação sob demanda pode só funcionar após um novo login." >&2
    fi
    return 0
}
reload_bus busctl --user call org.freedesktop.DBus /org/freedesktop/DBus \
    org.freedesktop.DBus ReloadConfig

if [ "$DRY" = 1 ]; then
    echo "(simulação: nada foi alterado)"
    exit 0
fi

echo "Instalado. Para iniciar agora:        systemctl --user start ghubd"
echo "Para iniciar com a sessão (opcional): systemctl --user enable ghubd"
echo "(ou use \"Iniciar com o sistema\" no menu da bandeja)"
