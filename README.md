# PasteHarbor

PasteHarbor is a clipboard-history manager for Ubuntu GNOME on Wayland. A GNOME Shell extension captures copied text and images, a Rust daemon stores them in SQLite through SQLx, and a GTK4/libadwaita app provides a larger history view.

## Features

- Captures both **text and images** copied to the clipboard; click any entry to copy it back
- Image entries store a generated thumbnail, shown in both the popup and the app alongside pixel dimensions
- **Source-app attribution**: each entry shows the icon and name of the app it was copied from
- Modern GTK app (libadwaita rows, thumbnails, empty state, toast feedback) with live refresh, search, clear confirmation, and settings
- Searchable Super+V popup that **scales to the monitor** and shows history up to the configured maximum
- Copy, delete, clear-all, pause-capture, and maximum-history controls
- SQLx + SQLite storage with duplicate detection and basic secret filtering
- User systemd service that starts at login, restarts after failures, and self-heals if it loses its D-Bus name (e.g. after logout/login)

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

Click the panel icon or press `<Super>V` to open clipboard history. The extension takes `<Super>V` over from GNOME's default notification-tray shortcut while enabled and restores it on disable (`<Super>M` still toggles the tray). The daemon starts automatically on future logins.

## Service

```bash
systemctl --user status pasteharbord.service
systemctl --user restart pasteharbord.service
journalctl --user -u pasteharbord.service -f
```

History is stored at `~/.local/share/pasteharbor/pasteharbor.db`, including image bytes and thumbnails as blobs. The database migrates in place on startup when new columns are introduced.

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

