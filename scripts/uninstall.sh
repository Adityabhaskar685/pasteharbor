#!/usr/bin/env bash
set -euo pipefail

systemctl --user disable --now pasteharbord.service >/dev/null 2>&1 || true
gnome-extensions disable pasteharbor@local >/dev/null 2>&1 || true
rm -f "$HOME/.config/systemd/user/pasteharbord.service"
rm -f "$HOME/.local/bin/pasteharbord"
rm -f "$HOME/.local/bin/pasteharbor-app"
rm -rf "$HOME/.local/share/gnome-shell/extensions/pasteharbor@local"
systemctl --user daemon-reload

printf 'PasteHarbor removed. Clipboard history was kept in ~/.local/share/pasteharbor.\n'
