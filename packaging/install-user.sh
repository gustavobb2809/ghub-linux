#!/usr/bin/env bash
# Instala o ghubd e a GUI para o usuário atual (sem sudo). Não habilita o serviço.
#   --dry-run   só imprime o que seria feito
#   --no-gui    não compila nem instala o ghub-gui
set -euo pipefail

DRY=0
GUI=1
for arg in "$@"; do
    case "$arg" in
        --dry-run) DRY=1 ;;
        --no-gui) GUI=0 ;;
        *)
            echo "uso: $0 [--dry-run] [--no-gui]" >&2
            exit 2
            ;;
    esac
done

root="$(cd "$(dirname "$0")/.." && pwd)"
target="${CARGO_TARGET_DIR:-$root/target}"
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
run install -Dm755 "$target/release/ghubd" "$bindir/ghubd"
run install -Dm644 "$root/packaging/systemd/ghubd.service" "$unitdir/ghubd.service"

# O arquivo de ativação do D-Bus não expande variáveis: grava o caminho absoluto.
if [ "$DRY" = 1 ]; then
    echo "+ sed 's|@BINDIR@|$bindir|' $root/packaging/dbus/org.ghublinux.Daemon.service > $dbusdir/org.ghublinux.Daemon.service"
else
    mkdir -p "$dbusdir"
    sed "s|@BINDIR@|$bindir|" "$root/packaging/dbus/org.ghublinux.Daemon.service" \
        > "$dbusdir/org.ghublinux.Daemon.service"
fi

if [ "$GUI" = 1 ]; then
    appdir="$HOME/.local/share/applications"
    icondir="$HOME/.local/share/icons/hicolor/scalable/apps"
    run cargo build --release -p ghub-gui --manifest-path "$root/Cargo.toml"
    run install -Dm755 "$target/release/ghub-gui" "$bindir/ghub-gui"
    run install -Dm644 "$root/packaging/icons/org.ghublinux.Gui.svg" "$icondir/org.ghublinux.Gui.svg"
    # O .desktop não expande variáveis: grava o caminho absoluto.
    if [ "$DRY" = 1 ]; then
        echo "+ sed 's|@BINDIR@|$bindir|' $root/packaging/org.ghublinux.Gui.desktop > $appdir/org.ghublinux.Gui.desktop"
    else
        mkdir -p "$appdir"
        sed "s|@BINDIR@|$bindir|" "$root/packaging/org.ghublinux.Gui.desktop" \
            > "$appdir/org.ghublinux.Gui.desktop"
    fi
    # Atualizar os caches é opcional: só se o comando existir, e sem falhar o script.
    if command -v update-desktop-database >/dev/null 2>&1; then
        run update-desktop-database "$appdir" || true
    fi
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        run gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" || true
    fi
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
if [ "$GUI" = 1 ]; then
    echo "Para abrir a interface: \"ghub\" no menu de aplicativos, ou ghub-gui"
fi
