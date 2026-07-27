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

const BUS_NAME = 'io.github.pasteharbor';
const OBJECT_PATH = '/io/github/pasteharbor/Clipboard1';
const INTERFACE = 'io.github.pasteharbor.Clipboard1';

const POLL_SECONDS = 1;
const POPUP_WIDTH = 560;
const HISTORY_HEIGHT = 340;
// The daemon clamps ListRecent/Search to 250 rows, so never ask for more.
const LIST_LIMIT_CAP = 250;
const TRAY_KEYBINDING = 'toggle-message-tray';

const METHOD = Object.freeze({
    CAPTURE_TEXT: 'CaptureText',
    CAPTURE_IMAGE: 'CaptureImage',
    LIST_RECENT: 'ListRecent',
    SEARCH: 'Search',
    GET_TEXT: 'GetText',
    GET_IMAGE: 'GetImage',
    GET_THUMBNAIL: 'GetThumbnail',
    DELETE_ITEM: 'DeleteItem',
    CLEAR: 'Clear',
    GET_SETTINGS: 'GetSettings',
    SET_MAX_HISTORY: 'SetMaxHistory',
    SHOW_APP: 'ShowApp',
});

// Clipboard image formats we try to read, best first.
const IMAGE_MIME_TYPES = ['image/png', 'image/jpeg', 'image/webp', 'image/gif', 'image/bmp'];

function callDaemon(method, parameters, callback = null) {
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
            let value = null;
            try {
                value = Gio.DBus.session.call_finish(result);
            } catch (error) {
                console.error(`PasteHarbor ${method} D-Bus call failed: ${error.message}`);
            }

            if (!callback)
                return;

            try {
                callback(value);
            } catch (error) {
                logError(error, `PasteHarbor ${method} callback failed`);
            }
        }
    );
}

function renderHistory(rows, items, status, createRow) {
    if (!rows)
        return;

    rows.destroy_all_children();
    if (status) {
        rows.add_child(new St.Label({text: status, style: 'padding: 10px;'}));
        return;
    }

    const seen = new Set();
    for (const item of items ?? []) {
        if (seen.has(item.id))
            continue;
        seen.add(item.id);
        rows.add_child(createRow(item));
    }
}

function menuLabel(text) {
    const compact = String(text).replace(/\s+/g, ' ').trim();
    if (compact.length <= 72)
        return compact;
    return `${compact.slice(0, 69)}...`;
}

const PasteHarborIndicator = GObject.registerClass(
class PasteHarborIndicator extends PanelMenu.Button {
    _init() {
        super._init(0.0, 'PasteHarbor');

        this._lastText = null;
        this._lastImageSig = null;
        this._pollId = 0;
        this._reloadToken = 0;
        this._query = '';
        this._limit = 30;
        this._maxHistory = 500;
        this._capturePaused = false;
        this._settingsOpen = false;
        this._icon = new St.Icon({
            icon_name: 'edit-copy-symbolic',
            style_class: 'system-status-icon',
        });

        this.add_child(this._icon);
        this.menu.connect('open-state-changed', (_menu, open) => {
            if (open) {
                this._loadSettings();
                this._reloadMenu();
            }
        });
    }

    start() {
        this._buildMenu();
        this._loadSettings();
        this._reloadMenu();
        this._pollId = GLib.timeout_add_seconds(
            GLib.PRIORITY_DEFAULT,
            POLL_SECONDS,
            () => {
                this._captureClipboard();
                return GLib.SOURCE_CONTINUE;
            }
        );
        this._captureClipboard();
    }

    stop() {
        if (this._pollId) {
            GLib.Source.remove(this._pollId);
            this._pollId = 0;
        }
    }

    toggleHistory() {
        this.menu.toggle();
    }

    openApp() {
        this._call(METHOD.SHOW_APP, new GLib.Variant('()', []), result => {
            if (!result) {
                Main.notify('PasteHarbor', 'Start pasteharbord, then try again.');
                return;
            }

            const [ok] = result.deep_unpack();
            if (!ok)
                Main.notify('PasteHarbor', 'Could not open the PasteHarbor app.');
        });
    }

    _captureClipboard() {
        if (this._capturePaused)
            return;

        this._captureCurrentText();
        this._captureCurrentImage();
    }

    _captureCurrentText() {
        const clipboard = St.Clipboard.get_default();
        clipboard.get_text(St.ClipboardType.CLIPBOARD, (_clipboard, text) => {
            if (!text || text === this._lastText)
                return;

            this._lastText = text;
            const source = this._currentSourceApp();
            this._call(METHOD.CAPTURE_TEXT, new GLib.Variant('(ssb)', [text, source, false]), result => {
                if (result && !this.menu.isOpen)
                    this._reloadMenu();
            });
        });
    }

    _captureCurrentImage() {
        const clipboard = St.Clipboard.get_default();
        const mimetype = this._pickImageMime(clipboard.get_mimetypes(St.ClipboardType.CLIPBOARD));
        if (!mimetype)
            return;

        clipboard.get_content(St.ClipboardType.CLIPBOARD, mimetype, (_clipboard, bytes) => {
            const size = bytes ? bytes.get_size() : 0;
            if (!size)
                return;

            const signature = `${mimetype}:${size}`;
            if (signature === this._lastImageSig)
                return;
            this._lastImageSig = signature;

            const data = bytes.get_data();
            const source = this._currentSourceApp();
            this._call(
                METHOD.CAPTURE_IMAGE,
                new GLib.Variant('(ayssb)', [data, mimetype, source, false]),
                result => {
                    if (result && !this.menu.isOpen)
                        this._reloadMenu();
                }
            );
        });
    }

    _pickImageMime(mimetypes) {
        const available = mimetypes ?? [];
        for (const mimetype of IMAGE_MIME_TYPES) {
            if (available.includes(mimetype))
                return mimetype;
        }
        return available.find(mimetype => mimetype.startsWith('image/')) ?? null;
    }

    _currentSourceApp() {
        try {
            const window = global.display.get_focus_window();
            if (!window)
                return 'gnome-shell';
            const app = Shell.WindowTracker.get_default().get_window_app(window);
            return app ? app.get_id() : 'gnome-shell';
        } catch (_error) {
            return 'gnome-shell';
        }
    }

    _loadSettings(callback = null) {
        this._call(METHOD.GET_SETTINGS, new GLib.Variant('()', []), result => {
            if (result) {
                try {
                    const [json] = result.deep_unpack();
                    this._maxHistory = JSON.parse(json).max_history ?? this._maxHistory;
                } catch (_error) {
                    // Keep the last valid value if settings cannot be decoded.
                }
            }
            this._syncLimit();
            this._updateSettingsControls();
            if (callback)
                callback();
        });
    }

    _syncLimit() {
        // Show as much of the stored history as the daemon will return so the
        // popup matches the configured maximum instead of a fixed page size.
        this._limit = Math.min(this._maxHistory, LIST_LIMIT_CAP);
    }

    _saveMaxHistory(text) {
        const value = Number.parseInt(text, 10);
        if (!Number.isInteger(value) || value < 10 || value > 10000) {
            Main.notify('PasteHarbor', 'Maximum history must be between 10 and 10000.');
            return;
        }

        this._call(METHOD.SET_MAX_HISTORY, new GLib.Variant('(u)', [value]), result => {
            if (!result) {
                Main.notify('PasteHarbor', 'Could not save maximum history.');
                return;
            }
            const [saved] = result.deep_unpack();
            this._maxHistory = saved;
            this._syncLimit();
            this._updateSettingsControls();
            this._reloadMenu();
        });
    }

    _reloadMenu() {
        const token = ++this._reloadToken;
        this._renderHistory(null, 'Loading...');

        const query = this._query.trim();
        const method = query ? METHOD.SEARCH : METHOD.LIST_RECENT;
        const parameters = query
            ? new GLib.Variant('(su)', [query, this._limit])
            : new GLib.Variant('(u)', [this._limit]);

        this._call(method, parameters, result => {
            if (token !== this._reloadToken)
                return;

            if (!result) {
                this._renderHistory([], 'pasteharbord is not running');
                return;
            }

            const [json] = result.deep_unpack();
            let items = [];
            try {
                items = JSON.parse(json);
            } catch (_error) {
                this._renderHistory([], 'Could not read history');
                return;
            }

            this._renderHistory(items, items.length ? null : 'No clipboard history yet');
        });
    }

    _buildMenu() {
        this.menu.removeAll();
        this._addHeaderRow();
        this._addSearchRow();
        this._addToolbarRow();
        this._addSettingsRows();
        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        this._addHistoryViewport();
        this._updateSettingsControls();
        this._updateSettingsVisibility();
    }

    _renderHistory(items, status) {
        renderHistory(this._historyRows, items, status, item => this._historyRow(item));
    }

    _updateSettingsControls() {
        if (this._maxHistoryEntry)
            this._maxHistoryEntry.get_clutter_text().set_text(String(this._maxHistory));
    }

    _updateSettingsVisibility() {
        for (const item of this._settingsItems ?? [])
            item.visible = this._settingsOpen;
    }

    _updateCaptureControls() {
        if (this._captureSwitch)
            this._captureSwitch.setToggleState(!this._capturePaused);
        if (this._captureStatus)
            this._captureStatus.text = this._capturePaused ? 'Capture paused' : 'Capturing copied text';
    }

    _addHeaderRow() {
        const row = new PopupMenu.PopupBaseMenuItem({reactive: false, can_focus: false});
        row.set_style('padding-top: 8px; padding-bottom: 6px;');
        const titleBox = new St.BoxLayout({vertical: true, x_expand: true});
        titleBox.add_child(new St.Label({text: 'PasteHarbor', style: 'font-weight: bold;'}));
        this._captureStatus = new St.Label({style: 'font-size: 0.85em; opacity: 0.72;'});
        titleBox.add_child(this._captureStatus);
        row.add_child(titleBox);
        row.add_child(this._iconButton('view-refresh-symbolic', () => this._reloadMenu(), 'Refresh history'));
        row.add_child(this._iconButton('window-new-symbolic', () => this.openApp(), 'Open PasteHarbor window'));
        this.menu.addMenuItem(row);
        this._updateCaptureControls();
    }

    _addSearchRow() {
        const row = new PopupMenu.PopupBaseMenuItem({reactive: false, can_focus: false});
        row.set_style(`width: ${POPUP_WIDTH}px;`);
        const entry = new St.Entry({
            hint_text: 'Search history',
            text: this._query,
            can_focus: true,
            x_expand: true,
            track_hover: true,
        });
        const clutterText = entry.get_clutter_text();
        const applySearch = () => {
            this._query = clutterText.get_text();
            this._searchClearButton.visible = Boolean(this._query);
            this._reloadMenu();
        };
        clutterText.connect('activate', applySearch);

        const searchButton = this._iconButton('system-search-symbolic', applySearch, 'Search history');

        const clearButton = this._iconButton('edit-clear-symbolic', null, 'Clear search');
        clearButton.visible = Boolean(this._query);
        clearButton.connect('clicked', () => {
            this._query = '';
            clutterText.set_text('');
            clearButton.visible = false;
            this._reloadMenu();
        });

        this._searchClearButton = clearButton;
        row.add_child(entry);
        row.add_child(searchButton);
        row.add_child(clearButton);

        this.menu.addMenuItem(row);
    }

    _addToolbarRow() {
        const row = new PopupMenu.PopupBaseMenuItem({reactive: false, can_focus: false});
        row.set_style('padding-top: 4px; padding-bottom: 6px; spacing: 6px;');
        row.add_child(this._textButton('Settings', () => {
            this._settingsOpen = !this._settingsOpen;
            this._updateSettingsVisibility();
            this._loadSettings();
        }));
        row.add_child(this._textButton('Clear history', () => this._clearHistory()));
        this.menu.addMenuItem(row);
    }

    _addSettingsRows() {
        const separator = new PopupMenu.PopupSeparatorMenuItem();
        this.menu.addMenuItem(separator);

        const captureItem = new PopupMenu.PopupBaseMenuItem({reactive: false, can_focus: false});
        captureItem.add_child(new St.Label({
            text: 'Capture copied text',
            x_expand: true,
            y_align: Clutter.ActorAlign.CENTER,
        }));
        const captureSwitch = new PopupMenu.Switch(!this._capturePaused);
        const captureButton = new St.Button({
            child: captureSwitch,
            can_focus: true,
            accessible_name: 'Capture copied text',
            style: 'padding: 0;',
        });
        captureButton.connect('clicked', () => {
            captureSwitch.toggle();
            this._capturePaused = !captureSwitch.state;
            this._updateCaptureControls();
        });
        captureItem.add_child(captureButton);
        this.menu.addMenuItem(captureItem);
        this._captureSwitch = captureSwitch;

        const maxHistoryRow = new PopupMenu.PopupBaseMenuItem({reactive: false, can_focus: false});
        const label = new St.Label({
            text: 'Maximum stored history',
            x_expand: true,
            y_align: Clutter.ActorAlign.CENTER,
        });
        const entry = new St.Entry({
            text: String(this._maxHistory),
            can_focus: true,
            style: 'width: 90px;',
        });
        const saveButton = this._textButton('Save', () => {
            this._saveMaxHistory(entry.get_clutter_text().get_text());
        });
        maxHistoryRow.add_child(label);
        maxHistoryRow.add_child(entry);
        maxHistoryRow.add_child(saveButton);
        this.menu.addMenuItem(maxHistoryRow);

        this._maxHistoryEntry = entry;
        this._settingsItems = [separator, captureItem, maxHistoryRow];
    }

    _addHistoryViewport() {
        const viewportItem = new PopupMenu.PopupBaseMenuItem({reactive: false, can_focus: false});
        const scrollView = new St.ScrollView({
            style: `width: ${POPUP_WIDTH}px; height: ${HISTORY_HEIGHT}px;`,
            hscrollbar_policy: St.PolicyType.NEVER,
            vscrollbar_policy: St.PolicyType.AUTOMATIC,
            overlay_scrollbars: true,
        });
        const rows = new St.BoxLayout({
            vertical: true,
            x_expand: true,
            style: 'spacing: 2px;',
        });
        scrollView.add_child(rows);
        viewportItem.add_child(scrollView);
        this.menu.addMenuItem(viewportItem);

        this._historyRows = rows;
        this._renderHistory(null, 'Loading...');
    }

    _historyRow(item) {
        const isImage = item.kind === 'image';
        const row = new PopupMenu.PopupBaseMenuItem({reactive: true, can_focus: true});
        row.set_style('padding: 6px 8px; border-radius: 8px; spacing: 10px;');

        row.add_child(this._leadingIcon(item, isImage));

        const textBox = new St.BoxLayout({
            vertical: true,
            x_expand: true,
            y_align: Clutter.ActorAlign.CENTER,
            style: 'spacing: 2px;',
        });
        textBox.add_child(new St.Label({text: menuLabel(item.preview_text)}));
        const subtitle = this._subtitle(item, isImage);
        if (subtitle) {
            textBox.add_child(new St.Label({
                text: subtitle,
                style: 'font-size: 0.8em; opacity: 0.6;',
            }));
        }
        row.add_child(textBox);

        const restore = () => {
            if (isImage)
                this._restoreImage(item.id);
            else
                this._restoreText(item.id);
            this.menu.close();
        };

        row.add_child(this._iconButton('edit-copy-symbolic', restore, 'Copy item'));
        row.add_child(this._iconButton('user-trash-symbolic', () => this._deleteItem(item.id), 'Delete item'));

        row.connect('activate', restore);
        return row;
    }

    _leadingIcon(item, isImage) {
        const icon = new St.Icon({
            icon_size: 32,
            y_align: Clutter.ActorAlign.CENTER,
            style: 'border-radius: 6px;',
        });

        if (isImage && item.has_thumbnail) {
            icon.set_icon_name('image-x-generic-symbolic');
            this._applyThumbnail(icon, item.id);
            return icon;
        }

        const appGicon = this._appGicon(item.source_app);
        if (appGicon)
            icon.set_gicon(appGicon);
        else
            icon.set_icon_name(isImage ? 'image-x-generic-symbolic' : 'edit-paste-symbolic');
        return icon;
    }

    _subtitle(item, isImage) {
        const parts = [];
        const appName = this._appName(item.source_app);
        if (appName)
            parts.push(appName);
        if (isImage && item.width && item.height)
            parts.push(`${item.width}×${item.height}`);
        return parts.join('  ·  ');
    }

    _lookupApp(sourceApp) {
        if (!sourceApp || sourceApp === 'gnome-shell')
            return null;
        try {
            const appSystem = Shell.AppSystem.get_default();
            let app = appSystem.lookup_app(sourceApp);
            if (!app && !sourceApp.endsWith('.desktop'))
                app = appSystem.lookup_app(`${sourceApp}.desktop`);
            return app;
        } catch (_error) {
            return null;
        }
    }

    _appGicon(sourceApp) {
        const app = this._lookupApp(sourceApp);
        return app ? app.get_icon() : null;
    }

    _appName(sourceApp) {
        const app = this._lookupApp(sourceApp);
        return app ? app.get_name() : null;
    }

    _applyThumbnail(icon, id) {
        this._call(METHOD.GET_THUMBNAIL, new GLib.Variant('(x)', [id]), result => {
            if (!result)
                return;
            const [bytes] = result.deep_unpack();
            if (!bytes || !bytes.length)
                return;
            try {
                icon.set_gicon(Gio.BytesIcon.new(GLib.Bytes.new(bytes)));
            } catch (error) {
                console.error(`PasteHarbor thumbnail decode failed: ${error.message}`);
            }
        });
    }

    _restoreText(id) {
        this._call(METHOD.GET_TEXT, new GLib.Variant('(x)', [id]), result => {
            if (!result)
                return;

            const [text] = result.deep_unpack();
            if (!text)
                return;

            St.Clipboard.get_default().set_text(St.ClipboardType.CLIPBOARD, text);
            this._lastText = text;
        });
    }

    _restoreImage(id) {
        this._call(METHOD.GET_IMAGE, new GLib.Variant('(x)', [id]), result => {
            if (!result)
                return;

            const [bytes, mimetype] = result.deep_unpack();
            if (!bytes || !bytes.length)
                return;

            const glibBytes = GLib.Bytes.new(bytes);
            this._lastImageSig = `${mimetype}:${glibBytes.get_size()}`;
            St.Clipboard.get_default().set_content(St.ClipboardType.CLIPBOARD, mimetype, glibBytes);
        });
    }

    _deleteItem(id) {
        this._call(METHOD.DELETE_ITEM, new GLib.Variant('(x)', [id]), result => {
            if (result)
                this._reloadMenu();
        });
    }

    _clearHistory() {
        this._call(METHOD.CLEAR, new GLib.Variant('()', []), result => {
            if (result)
                this._reloadMenu();
        });
    }

    _iconButton(iconName, callback = null, accessibleName = null) {
        const button = new St.Button({
            style_class: 'button',
            can_focus: true,
            accessible_name: accessibleName,
            child: new St.Icon({icon_name: iconName, style_class: 'popup-menu-icon'}),
        });
        if (callback)
            button.connect('clicked', callback);
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
        callDaemon(method, parameters, callback);
    }
});

export default class PasteHarborExtension extends Extension {
    enable() {
        this._indicator = new PasteHarborIndicator();
        Main.panel.addToStatusArea(this.uuid, this._indicator);
        this._indicator.start();

        // GNOME Shell binds <Super>V to toggle-message-tray by default, which
        // collides with our show-history binding and makes Super+V flaky.
        // Release the conflicting variant while enabled and restore it later.
        this._releaseTrayShortcut();

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
        this._restoreTrayShortcut();

        if (this._indicator) {
            this._indicator.stop();
            this._indicator.destroy();
            this._indicator = null;
        }

        this._settings = null;
    }

    _conflictsWithSuperV(binding) {
        return String(binding).replace(/\s+/g, '').toLowerCase() === '<super>v';
    }

    _releaseTrayShortcut() {
        this._shellKeybindings = new Gio.Settings({
            schema_id: 'org.gnome.shell.keybindings',
        });
        const current = this._shellKeybindings.get_strv(TRAY_KEYBINDING);
        const filtered = current.filter(binding => !this._conflictsWithSuperV(binding));
        if (filtered.length === current.length) {
            this._savedTrayShortcut = null;
            return;
        }

        this._savedTrayShortcut = current;
        this._shellKeybindings.set_strv(TRAY_KEYBINDING, filtered);
    }

    _restoreTrayShortcut() {
        if (this._shellKeybindings && this._savedTrayShortcut)
            this._shellKeybindings.set_strv(TRAY_KEYBINDING, this._savedTrayShortcut);
        this._savedTrayShortcut = null;
        this._shellKeybindings = null;
    }
}
