# ClipVault

ClipVault is an Ubuntu/GNOME/Wayland clipboard manager prototype with three parts:

- `clipvaultd`: Rust daemon that stores clipboard history in SQLite through SQLx and exposes a D-Bus API.
- `clipvault-app`: GTK4/libadwaita Rust app for browsing and clearing history.
- `extensions/clipvault@local`: GNOME Shell extension that reads/restores clipboard text and talks to the daemon.

The first implementation supports text clipboard history. The daemon database and API are intentionally shaped so image, HTML, and file URI support can be added without replacing the architecture.

## Requirements

Ubuntu/GNOME packages:

```bash
sudo apt install build-essential pkg-config libgtk-4-dev libadwaita-1-dev libsqlite3-dev
```

Rust:

```bash
rustup default stable
```

## Build

```bash
cargo build
```

## Run The Daemon

```bash
cargo run -p clipvaultd
```

The daemon creates its database at:

```text
$XDG_DATA_HOME/clipvault/clipvault.db
```

or:

```text
~/.local/share/clipvault/clipvault.db
```

## Run The GTK App

Start the daemon first, then run:

```bash
cargo run -p clipvault-app
```

The app refreshes history automatically while it is open. The panel popup also supports search, restore, delete, clear all, basic settings, and opening the full GTK window.

## Install The GNOME Extension For Development

GNOME Shell discovers manually installed extensions when the shell session starts. On Wayland, install the files first, then log out and log back in before enabling the extension.

Package and install the extension:

```bash
gnome-extensions pack extensions/clipvault@local --schema=schemas/org.gnome.shell.extensions.clipvault.gschema.xml --out-dir=/tmp -f
gnome-extensions install --force /tmp/clipvault@local.shell-extension.zip
```

After logging back in:

```bash
gnome-extensions list | grep clipvault
gnome-extensions enable clipvault@local
```

For symlink-based development, create the symlink before logging out and back in:

```bash
mkdir -p ~/.local/share/gnome-shell/extensions
ln -sfnT "$PWD/extensions/clipvault@local" ~/.local/share/gnome-shell/extensions/clipvault@local
glib-compile-schemas ~/.local/share/gnome-shell/extensions/clipvault@local/schemas
```

If a copied install already exists at that path, move it aside before creating the symlink. The extension adds a panel indicator and a `<Super>V` keybinding. It polls the shell clipboard for text, sends new text to the Rust daemon, and shows a searchable clipboard popup with copy/delete controls.
