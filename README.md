# PasteHarbor

PasteHarbor is a clipboard-history manager for Ubuntu GNOME on Wayland. A GNOME Shell extension captures copied text, a Rust daemon stores it in SQLite through SQLx, and a GTK4/libadwaita app provides a larger history view.

## Features

- Searchable clipboard popup with a fixed-size scrollable history area
- Copy, delete, clear-all, pause-capture, and maximum-history controls
- GTK app with live refresh, search, clear confirmation, and settings
- SQLx + SQLite storage with duplicate detection and basic secret filtering
- User systemd service that starts at login and restarts after failures

PasteHarbor currently stores text only. Image and file support are planned separately.

## Install

Ubuntu packages:

```bash
sudo apt install build-essential pkg-config libgtk-4-dev libadwaita-1-dev libsqlite3-dev
```

Install Rust with [rustup](https://rustup.rs/), then run:

```bash
./scripts/install.sh
```

Log out and log back in once so GNOME Shell discovers the extension, then enable it:

```bash
gnome-extensions enable pasteharbor@local
```

Click the panel icon or press `<Super>V` to open clipboard history. The daemon starts automatically on future logins.

## Service

```bash
systemctl --user status pasteharbord.service
systemctl --user restart pasteharbord.service
journalctl --user -u pasteharbord.service -f
```

History is stored at `~/.local/share/pasteharbor/pasteharbor.db`.

## Development

```bash
cargo check
cargo test -p pasteharbord
cargo run -p pasteharbord
cargo run -p pasteharbor-app
```

After changing extension JavaScript, reinstall with `./scripts/install.sh`. On Wayland, log out and back in if GNOME Shell still uses an older extension module.

## Uninstall

```bash
./scripts/uninstall.sh
```

The uninstall script keeps clipboard history data. Remove `~/.local/share/pasteharbor` manually if you also want to delete stored history.

## Architecture

```text
GNOME Shell extension -> D-Bus -> pasteharbord -> SQLite
                                  |
                                  +-> pasteharbor-app
```

