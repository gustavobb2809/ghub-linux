#!/usr/bin/env bash
# Instala o ghubd para o usuário atual (sem sudo). Não habilita o serviço.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cargo build --release -p ghubd --manifest-path "$root/Cargo.toml"

install -Dm755 "$root/target/release/ghubd" "$HOME/.local/bin/ghubd"
install -Dm644 "$root/packaging/systemd/ghubd.service" "$HOME/.config/systemd/user/ghubd.service"
systemctl --user daemon-reload

echo "Instalado. Para iniciar agora:        systemctl --user start ghubd"
echo "Para iniciar com a sessão (opcional): systemctl --user enable ghubd"
