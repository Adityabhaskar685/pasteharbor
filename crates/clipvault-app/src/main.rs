use adw::prelude::*;
use gtk::glib;
use serde::Deserialize;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use zbus::blocking::{Connection, Proxy};

const APP_ID: &str = "io.github.clipvault.App";
const BUS_NAME: &str = "io.github.clipvault";
const OBJECT_PATH: &str = "/io/github/clipvault/Clipboard1";
const INTERFACE: &str = "io.github.clipvault.Clipboard1";

#[derive(Debug, Clone, Deserialize)]
struct ClipboardItem {
    id: i64,
    created_at: String,
    last_used_at: String,
    kind: String,
    preview_text: String,
    source_app: Option<String>,
    size_bytes: i64,
}

fn main() -> glib::ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &adw::Application) {
    if let Some(window) = app.active_window() {
        window.present();
        return;
    }

    let state = Rc::new(AppState::new());
    let ui_settings = Rc::new(UiSettings::default());

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("ClipVault")
        .default_width(720)
        .default_height(520)
        .build();

    let header = adw::HeaderBar::new();
    let refresh_button = gtk::Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text("Refresh")
        .build();
    let settings_button = gtk::Button::builder()
        .icon_name("emblem-system-symbolic")
        .tooltip_text("Settings")
        .build();
    let clear_button = gtk::Button::builder()
        .icon_name("edit-clear-symbolic")
        .tooltip_text("Clear history")
        .build();
    header.pack_start(&refresh_button);
    header.pack_end(&clear_button);
    header.pack_end(&settings_button);

    let search = gtk::SearchEntry::builder()
        .placeholder_text("Search clipboard history")
        .hexpand(true)
        .build();

    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(vec!["boxed-list".to_string()])
        .build();

    let status = gtk::Label::builder()
        .halign(gtk::Align::Start)
        .css_classes(vec!["dim-label".to_string()])
        .label("Start clipvaultd to load history.")
        .build();

    let scroller = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .child(&list)
        .build();

    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_top(16);
    content.set_margin_bottom(16);
    content.set_margin_start(16);
    content.set_margin_end(16);
    content.append(&search);
    content.append(&status);
    content.append(&scroller);

    let toolbar = gtk::Box::new(gtk::Orientation::Vertical, 0);
    toolbar.append(&header);
    toolbar.append(&content);
    window.set_content(Some(&toolbar));

    {
        let state = Rc::clone(&state);
        let list = list.clone();
        let status = status.clone();
        let search = search.clone();
        let ui_settings = Rc::clone(&ui_settings);
        refresh_button.connect_clicked(move |_| {
            refresh_history(
                &state,
                &list,
                &status,
                search.text().as_str(),
                ui_settings.row_limit.get(),
            );
        });
    }

    {
        let state = Rc::clone(&state);
        let list = list.clone();
        let status = status.clone();
        let ui_settings = Rc::clone(&ui_settings);
        search.connect_search_changed(move |entry| {
            refresh_history(
                &state,
                &list,
                &status,
                entry.text().as_str(),
                ui_settings.row_limit.get(),
            );
        });
    }

    {
        let state = Rc::clone(&state);
        let list = list.clone();
        let status = status.clone();
        let search = search.clone();
        let ui_settings = Rc::clone(&ui_settings);
        clear_button.connect_clicked(move |_| match state.clear() {
            Ok(count) => {
                status.set_label(&format!("Cleared {count} history item(s)."));
                refresh_history(
                    &state,
                    &list,
                    &status,
                    search.text().as_str(),
                    ui_settings.row_limit.get(),
                );
            }
            Err(error) => status.set_label(&format!("Could not clear history: {error}")),
        });
    }

    {
        let parent = window.clone();
        let ui_settings = Rc::clone(&ui_settings);
        settings_button.connect_clicked(move |_| show_settings(&parent, &ui_settings));
    }

    {
        let state = Rc::clone(&state);
        let list = list.clone();
        let status = status.clone();
        let search = search.clone();
        let ui_settings = Rc::clone(&ui_settings);
        glib::timeout_add_seconds_local(1, move || {
            if ui_settings.auto_refresh.get() {
                refresh_history(
                    &state,
                    &list,
                    &status,
                    search.text().as_str(),
                    ui_settings.row_limit.get(),
                );
            }
            glib::ControlFlow::Continue
        });
    }

    refresh_history(&state, &list, &status, "", ui_settings.row_limit.get());
    window.present();
}

fn refresh_history(
    state: &AppState,
    list: &gtk::ListBox,
    status: &gtk::Label,
    query: &str,
    limit: u32,
) {
    while let Some(row) = list.first_child() {
        list.remove(&row);
    }

    match state.items(query, limit) {
        Ok(items) if items.is_empty() => {
            status.set_label("No clipboard history yet.");
        }
        Ok(items) => {
            status.set_label(&format!("{} item(s)", items.len()));
            for item in items {
                list.append(&history_row(state, &item, status));
            }
        }
        Err(error) => {
            status.set_label(&format!("clipvaultd is unavailable: {error}"));
        }
    }
}

fn history_row(state: &AppState, item: &ClipboardItem, status: &gtk::Label) -> gtk::ListBoxRow {
    let title = gtk::Label::builder()
        .label(&item.preview_text)
        .xalign(0.0)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .build();

    let details = gtk::Label::builder()
        .label(format!(
            "{} | {} bytes | {} | copied {} | used {}",
            item.kind,
            item.size_bytes,
            item.source_app.as_deref().unwrap_or("unknown source"),
            item.created_at,
            item.last_used_at
        ))
        .xalign(0.0)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .css_classes(vec!["dim-label".to_string()])
        .build();

    let copy_button = gtk::Button::builder()
        .icon_name("edit-copy-symbolic")
        .tooltip_text("Put this item back on the clipboard")
        .valign(gtk::Align::Center)
        .build();

    let delete_button = gtk::Button::builder()
        .icon_name("user-trash-symbolic")
        .tooltip_text("Delete")
        .valign(gtk::Align::Center)
        .build();

    let text_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    text_box.set_hexpand(true);
    text_box.append(&title);
    text_box.append(&details);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    actions.append(&copy_button);
    actions.append(&delete_button);

    let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row_box.set_margin_top(10);
    row_box.set_margin_bottom(10);
    row_box.set_margin_start(12);
    row_box.set_margin_end(12);
    row_box.append(&text_box);
    row_box.append(&actions);

    let row = gtk::ListBoxRow::new();
    row.set_child(Some(&row_box));

    {
        let state = state.clone();
        let status = status.clone();
        let item_id = item.id;
        copy_button.connect_clicked(move |_| match state.get_text(item_id) {
            Ok(text) => {
                if let Some(display) = gtk::gdk::Display::default() {
                    display.clipboard().set_text(&text);
                    status.set_label("Copied selected history item to the clipboard.");
                }
            }
            Err(error) => status.set_label(&format!("Could not restore item: {error}")),
        });
    }

    {
        let state = state.clone();
        let status = status.clone();
        let row = row.clone();
        let item_id = item.id;
        delete_button.connect_clicked(move |_| match state.delete_item(item_id) {
            Ok(true) => {
                if let Some(list) = row
                    .parent()
                    .and_then(|parent| parent.downcast::<gtk::ListBox>().ok())
                {
                    list.remove(&row);
                }
                status.set_label("Deleted history item.");
            }
            Ok(false) => status.set_label("History item was already gone."),
            Err(error) => status.set_label(&format!("Could not delete item: {error}")),
        });
    }

    row
}

fn show_settings(parent: &adw::ApplicationWindow, settings: &Rc<UiSettings>) {
    let window = gtk::Window::builder()
        .title("ClipVault Settings")
        .transient_for(parent)
        .modal(true)
        .default_width(380)
        .default_height(180)
        .build();

    let auto_refresh_label = gtk::Label::builder()
        .label("Auto-refresh history")
        .xalign(0.0)
        .hexpand(true)
        .build();
    let auto_refresh_switch = gtk::Switch::builder()
        .active(settings.auto_refresh.get())
        .valign(gtk::Align::Center)
        .build();

    {
        let settings = Rc::clone(settings);
        auto_refresh_switch.connect_active_notify(move |switch| {
            settings.auto_refresh.set(switch.is_active());
        });
    }

    let auto_refresh_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    auto_refresh_row.append(&auto_refresh_label);
    auto_refresh_row.append(&auto_refresh_switch);

    let row_limit_label = gtk::Label::builder()
        .label("History rows shown")
        .xalign(0.0)
        .hexpand(true)
        .build();
    let row_limit_adjustment =
        gtk::Adjustment::new(settings.row_limit.get() as f64, 10.0, 100.0, 5.0, 10.0, 0.0);
    let row_limit_spin = gtk::SpinButton::builder()
        .adjustment(&row_limit_adjustment)
        .numeric(true)
        .valign(gtk::Align::Center)
        .build();

    {
        let settings = Rc::clone(settings);
        row_limit_spin.connect_value_changed(move |spin| {
            settings.row_limit.set(spin.value_as_int() as u32);
        });
    }

    let row_limit_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row_limit_row.append(&row_limit_label);
    row_limit_row.append(&row_limit_spin);

    let close_button = gtk::Button::builder()
        .label("Close")
        .halign(gtk::Align::End)
        .build();
    {
        let window = window.clone();
        close_button.connect_clicked(move |_| window.close());
    }

    let content = gtk::Box::new(gtk::Orientation::Vertical, 14);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    content.set_margin_start(18);
    content.set_margin_end(18);
    content.append(&auto_refresh_row);
    content.append(&row_limit_row);
    content.append(&close_button);

    window.set_child(Some(&content));
    window.present();
}

#[derive(Clone)]
struct AppState {
    connection: Rc<RefCell<Option<Connection>>>,
}

impl AppState {
    fn new() -> Self {
        Self {
            connection: Rc::new(RefCell::new(None)),
        }
    }

    fn items(&self, query: &str, limit: u32) -> anyhow::Result<Vec<ClipboardItem>> {
        let proxy = self.proxy()?;
        let json: String = if query.trim().is_empty() {
            proxy.call("ListRecent", &(limit))?
        } else {
            proxy.call("Search", &(query, limit))?
        };
        Ok(serde_json::from_str(&json)?)
    }

    fn get_text(&self, id: i64) -> anyhow::Result<String> {
        let proxy = self.proxy()?;
        Ok(proxy.call("GetText", &(id))?)
    }

    fn delete_item(&self, id: i64) -> anyhow::Result<bool> {
        let proxy = self.proxy()?;
        Ok(proxy.call("DeleteItem", &(id))?)
    }

    fn clear(&self) -> anyhow::Result<u64> {
        let proxy = self.proxy()?;
        Ok(proxy.call("Clear", &())?)
    }

    fn proxy(&self) -> anyhow::Result<Proxy<'_>> {
        if self.connection.borrow().is_none() {
            *self.connection.borrow_mut() = Some(Connection::session()?);
        }

        let borrow = self.connection.borrow();
        let connection = borrow.as_ref().expect("connection initialized");
        Ok(Proxy::new(connection, BUS_NAME, OBJECT_PATH, INTERFACE)?)
    }
}

struct UiSettings {
    auto_refresh: Cell<bool>,
    row_limit: Cell<u32>,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            auto_refresh: Cell::new(true),
            row_limit: Cell::new(50),
        }
    }
}
