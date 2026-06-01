#!/usr/bin/env bash
set -euo pipefail

repo_dir="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
bin_dir="$HOME/.local/bin"
extension_dir="$HOME/.local/share/gnome-shell/extensions/pasteharbor@local"
service_dir="$HOME/.config/systemd/user"

cargo build --release --manifest-path "$repo_dir/Cargo.toml"
install -Dm755 "$repo_dir/target/release/pasteharbord" "$bin_dir/pasteharbord"
install -Dm755 "$repo_dir/target/release/pasteharbor-app" "$bin_dir/pasteharbor-app"
install -Dm644 "$repo_dir/systemd/pasteharbord.service" "$service_dir/pasteharbord.service"

rm -rf "$extension_dir"
mkdir -p "$(dirname "$extension_dir")"
cp -R "$repo_dir/extensions/pasteharbor@local" "$extension_dir"
rm -f "$extension_dir/schemas/gschemas.compiled"
glib-compile-schemas "$extension_dir/schemas"

systemctl --user daemon-reload
systemctl --user reenable pasteharbord.service
systemctl --user restart pasteharbord.service

printf '\nPasteHarbor installed. Log out and log back in once, then run:\n'
printf '  gnome-extensions enable pasteharbor@local\n'
