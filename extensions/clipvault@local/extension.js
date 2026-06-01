import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import GObject from 'gi://GObject';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import St from 'gi://St';

import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';

const BUS_NAME = 'io.github.clipvault';
const OBJECT_PATH = '/io/github/clipvault/Clipboard1';
const INTERFACE = 'io.github.clipvault.Clipboard1';
const POLL_SECONDS = 1;

const ClipVaultIndicator = GObject.registerClass(
class ClipVaultIndicator extends PanelMenu.Button {
    _init() {
        super._init(0.0, 'ClipVault');

        this._lastText = null;
        this._pollId = 0;
        this._reloadToken = 0;
        this._query = '';
        this._limit = 12;
        this._capturePaused = false;
        this._settingsOpen = false;
        this._icon = new St.Icon({
            icon_name: 'edit-copy-symbolic',
            style_class: 'system-status-icon',
        });

        this.add_child(this._icon);
        this.menu.connect('open-state-changed', (_menu, open) => {
            if (open)
                this._reloadMenu();
        });
    }

    start() {
        this._pollId = GLib.timeout_add_seconds(
            GLib.PRIORITY_DEFAULT,
            POLL_SECONDS,
            () => {
                this._captureCurrentText();
                return GLib.SOURCE_CONTINUE;
            }
        );
        this._captureCurrentText();
    }

    stop() {
        if (this._pollId) {
            GLib.Source.remove(this._pollId);
            this._pollId = 0;
        }
    }

    toggleHistory() {
        if (!this.menu.isOpen)
            this._reloadMenu();
        this.menu.toggle();
    }

    openApp() {
        this._call('ShowApp', new GLib.Variant('()', []), result => {
            if (!result) {
                Main.notify('ClipVault', 'Start clipvaultd, then try again.');
                return;
            }

            const [ok] = result.deep_unpack();
            if (!ok)
                Main.notify('ClipVault', 'Could not open the ClipVault app.');
        });
    }

    _captureCurrentText() {
        if (this._capturePaused)
            return;

        const clipboard = St.Clipboard.get_default();
        clipboard.get_text(St.ClipboardType.CLIPBOARD, (_clipboard, text) => {
            if (!text || text === this._lastText)
                return;

            this._lastText = text;
            this._call('CaptureText', new GLib.Variant('(ssb)', [text, 'gnome-shell', false]));
        });
    }

    _reloadMenu() {
        const token = ++this._reloadToken;
        this._renderMenu(token, null, 'Loading...');

        const query = this._query.trim();
        const method = query ? 'Search' : 'ListRecent';
        const parameters = query
            ? new GLib.Variant('(su)', [query, this._limit])
            : new GLib.Variant('(u)', [this._limit]);

        this._call(method, parameters, result => {
            if (token !== this._reloadToken)
                return;

            if (!result) {
                this._renderMenu(token, [], 'clipvaultd is not running');
                return;
            }

            const [json] = result.deep_unpack();
            let items = [];
            try {
                items = JSON.parse(json);
            } catch (_error) {
                this._renderMenu(token, [], 'Could not read history');
                return;
            }

            this._renderMenu(token, items, items.length ? null : 'No clipboard history yet');
        });
    }

    _renderMenu(token, items, status) {
        if (token !== this._reloadToken)
            return;

        this.menu.removeAll();
        this._addSearchRow();
        this._addToolbarRow();

        if (this._settingsOpen)
            this._addSettingsRows();

        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());

        if (status)
            this._addStatusItem(status);

        if (items) {
            const seen = new Set();
            for (const item of items) {
                if (seen.has(item.id))
                    continue;
                seen.add(item.id);
                this._addHistoryRow(item);
            }
        }
    }

    _addSearchRow() {
        const row = new PopupMenu.PopupBaseMenuItem({reactive: false, can_focus: false});
        const entry = new St.Entry({
            hint_text: 'Search history',
            text: this._query,
            can_focus: true,
            x_expand: true,
            track_hover: true,
        });
        const clutterText = entry.get_clutter_text();
        clutterText.connect('activate', () => {
            this._query = clutterText.get_text();
            this._reloadMenu();
        });

        const searchButton = this._iconButton('system-search-symbolic', 'Search');
        searchButton.connect('clicked', () => {
            this._query = clutterText.get_text();
            this._reloadMenu();
        });

        row.add_child(entry);
        row.add_child(searchButton);

        if (this._query) {
            const clearButton = this._iconButton('edit-clear-symbolic', 'Clear search');
            clearButton.connect('clicked', () => {
                this._query = '';
                this._reloadMenu();
            });
            row.add_child(clearButton);
        }

        this.menu.addMenuItem(row);
    }

    _addToolbarRow() {
        const row = new PopupMenu.PopupBaseMenuItem({reactive: false, can_focus: false});
        row.add_child(this._textButton('Refresh', () => this._reloadMenu()));
        row.add_child(this._textButton('Open Window', () => this.openApp()));
        row.add_child(this._textButton('Settings', () => {
            this._settingsOpen = !this._settingsOpen;
            this._reloadMenu();
        }));
        row.add_child(this._textButton('Clear All', () => this._clearHistory()));
        this.menu.addMenuItem(row);
    }

    _addSettingsRows() {
        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());

        const captureItem = new PopupMenu.PopupSwitchMenuItem('Capture new text', !this._capturePaused);
        captureItem.connect('toggled', (_item, state) => {
            this._capturePaused = !state;
        });
        this.menu.addMenuItem(captureItem);

        const limitItem = new PopupMenu.PopupMenuItem(`Rows shown: ${this._limit}`);
        limitItem.connect('activate', () => {
            this._limit = this._limit >= 30 ? 12 : this._limit + 6;
            this._reloadMenu();
        });
        this.menu.addMenuItem(limitItem);
    }

    _addHistoryRow(item) {
        const row = new PopupMenu.PopupBaseMenuItem({reactive: true, can_focus: true});
        const label = new St.Label({
            text: this._menuLabel(item.preview_text),
            x_expand: true,
            y_align: Clutter.ActorAlign.CENTER,
        });
        row.add_child(label);

        const copyButton = this._iconButton('edit-copy-symbolic', 'Copy');
        copyButton.connect('clicked', () => {
            this._restoreText(item.id);
            this.menu.close();
        });
        row.add_child(copyButton);

        const deleteButton = this._iconButton('user-trash-symbolic', 'Delete');
        deleteButton.connect('clicked', () => this._deleteItem(item.id));
        row.add_child(deleteButton);

        row.connect('activate', () => {
            this._restoreText(item.id);
            this.menu.close();
        });
        this.menu.addMenuItem(row);
    }

    _restoreText(id) {
        this._call('GetText', new GLib.Variant('(x)', [id]), result => {
            if (!result)
                return;

            const [text] = result.deep_unpack();
            if (!text)
                return;

            St.Clipboard.get_default().set_text(St.ClipboardType.CLIPBOARD, text);
            this._lastText = text;
        });
    }

    _deleteItem(id) {
        this._call('DeleteItem', new GLib.Variant('(x)', [id]), result => {
            if (!result)
                return;
            this._reloadMenu();
        });
    }

    _clearHistory() {
        this._call('Clear', new GLib.Variant('()', []), result => {
            if (!result)
                return;
            this._reloadMenu();
        });
    }

    _addStatusItem(label) {
        const item = new PopupMenu.PopupMenuItem(label);
        item.reactive = false;
        this.menu.addMenuItem(item);
    }

    _menuLabel(text) {
        const compact = String(text).replace(/\s+/g, ' ').trim();
        if (compact.length <= 72)
            return compact;
        return `${compact.slice(0, 69)}...`;
    }

    _iconButton(iconName, tooltip) {
        const button = new St.Button({
            style_class: 'button',
            can_focus: true,
            child: new St.Icon({icon_name: iconName, style_class: 'popup-menu-icon'}),
        });
        return button;
    }

    _textButton(label, callback) {
        const button = new St.Button({
            style_class: 'button',
            label,
            can_focus: true,
            x_expand: true,
        });
        button.connect('clicked', callback);
        return button;
    }

    _call(method, parameters, callback = null) {
        Gio.DBus.session.call(
            BUS_NAME,
            OBJECT_PATH,
            INTERFACE,
            method,
            parameters,
            null,
            Gio.DBusCallFlags.NONE,
            2000,
            null,
            (_connection, result) => {
                try {
                    const value = Gio.DBus.session.call_finish(result);
                    if (callback)
                        callback(value);
                } catch (_error) {
                    if (callback)
                        callback(null);
                }
            }
        );
    }
});

export default class ClipVaultExtension extends Extension {
    enable() {
        this._indicator = new ClipVaultIndicator();
        Main.panel.addToStatusArea(this.uuid, this._indicator);
        this._indicator.start();

        this._settings = this.getSettings();
        Main.wm.addKeybinding(
            'show-history',
            this._settings,
            Meta.KeyBindingFlags.NONE,
            Shell.ActionMode.NORMAL | Shell.ActionMode.OVERVIEW,
            () => this._indicator.toggleHistory()
        );
    }

    disable() {
        Main.wm.removeKeybinding('show-history');

        if (this._indicator) {
            this._indicator.stop();
            this._indicator.destroy();
            this._indicator = null;
        }

        this._settings = null;
    }
}
