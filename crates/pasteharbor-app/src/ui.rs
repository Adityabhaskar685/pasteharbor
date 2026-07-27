use adw::prelude::*;
use gtk::{gio, glib};
use std::cell::RefCell;
use std::rc::Rc;

use crate::dbus::AppState;
use crate::format::{format_bytes, format_timestamp};
use crate::models::{ClipboardItem, UiSettings};
use pasteharbor_core::{kind, DEFAULT_MAX_HISTORY, MAX_MAX_HISTORY, MIN_MAX_HISTORY};

const APP_CSS: &str = "
.ph-thumb { border-radius: 10px; background: alpha(currentColor, 0.08); }
.ph-avatar {
    border-radius: 10px;
    background: alpha(@accent_bg_color, 0.18);
    color: @accent_color;
}
.ph-badge {
    background: @card_bg_color;
    border-radius: 8px;
    padding: 1px;
    box-shadow: 0 1px 2px alpha(black, 0.3);
    margin: 2px;
}
";

/// Shared handles the closures and helpers need to update the window.
struct Ui {
    list: gtk::ListBox,
    stack: gtk::Stack,
    empty_page: adw::StatusPage,
    title: adw::WindowTitle,
    toasts: adw::ToastOverlay,
}

impl Ui {
    fn set_status(&self, text: &str) {
        self.title.set_subtitle(text);
    }

    fn toast(&self, text: &str) {
        self.toasts.add_toast(adw::Toast::new(text));
    }

    fn show_empty(&self, icon: &str, title: &str, description: &str) {
        self.empty_page.set_icon_name(Some(icon));
        self.empty_page.set_title(title);
        self.empty_page.set_description(Some(description));
        self.stack.set_visible_child_name("empty");
    }

    /// Fall back to the empty page when the last row is removed.
    fn sync_view(&self) {
        if self.list.first_child().is_none() {
            self.show_empty(
                "edit-copy-symbolic",
                "No clipboard history yet",
                "Copy text or an image to get started.",
            );
            self.set_status("Empty");
        }
    }
}

fn install_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(APP_CSS);
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

pub fn build_ui(app: &adw::Application) {
    if let Some(window) = app.active_window() {
        window.present();
        return;
    }

    install_css();

    let state = Rc::new(AppState::new());
    let settings = Rc::new(UiSettings::default());
    let cache = Rc::new(RefCell::new(String::new()));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("PasteHarbor")
        .default_width(820)
        .default_height(680)
        .build();

    let title = adw::WindowTitle::new("PasteHarbor", "Clipboard history");
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&title));

    let refresh_button = gtk::Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text("Refresh")
        .build();
    let settings_button = gtk::Button::builder()
        .icon_name("emblem-system-symbolic")
        .tooltip_text("Settings")
        .build();
    let clear_button = gtk::Button::builder()
        .icon_name("user-trash-symbolic")
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
        .valign(gtk::Align::Start)
        .build();

    let scroller = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&list)
        .build();

    let empty_page = adw::StatusPage::builder()
        .icon_name("edit-copy-symbolic")
        .title("No clipboard history yet")
        .description("Copy text or an image to get started.")
        .vexpand(true)
        .build();

    let stack = gtk::Stack::builder().vexpand(true).build();
    stack.add_named(&scroller, Some("list"));
    stack.add_named(&empty_page, Some("empty"));

    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_top(16);
    content.set_margin_bottom(16);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.append(&search);
    content.append(&stack);

    let clamp = adw::Clamp::builder()
        .maximum_size(760)
        .tightening_threshold(600)
        .child(&content)
        .build();

    let toasts = adw::ToastOverlay::new();
    toasts.set_child(Some(&clamp));

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&toasts));
    window.set_content(Some(&toolbar));

    let ui = Rc::new(Ui {
        list: list.clone(),
        stack: stack.clone(),
        empty_page: empty_page.clone(),
        title: title.clone(),
        toasts: toasts.clone(),
    });

    {
        let state = Rc::clone(&state);
        let ui = Rc::clone(&ui);
        let cache = Rc::clone(&cache);
        let settings = Rc::clone(&settings);
        let search = search.clone();
        refresh_button.connect_clicked(move |_| {
            cache.borrow_mut().clear();
            refresh_history(
                &state,
                &ui,
                &cache,
                search.text().as_str(),
                settings.row_limit.get(),
            );
        });
    }

    {
        let state = Rc::clone(&state);
        let ui = Rc::clone(&ui);
        let cache = Rc::clone(&cache);
        let settings = Rc::clone(&settings);
        search.connect_search_changed(move |entry| {
            cache.borrow_mut().clear();
            refresh_history(
                &state,
                &ui,
                &cache,
                entry.text().as_str(),
                settings.row_limit.get(),
            );
        });
    }

    {
        let state = Rc::clone(&state);
        let ui = Rc::clone(&ui);
        let cache = Rc::clone(&cache);
        let settings = Rc::clone(&settings);
        let search = search.clone();
        let parent = window.clone();
        clear_button.connect_clicked(move |_| {
            show_clear_confirmation(&parent, &state, &ui, &cache, &search, &settings);
        });
    }

    {
        let parent = window.clone();
        let state = Rc::clone(&state);
        let settings = Rc::clone(&settings);
        settings_button.connect_clicked(move |_| show_settings(&parent, &state, &settings));
    }

    {
        let state = Rc::clone(&state);
        let ui = Rc::clone(&ui);
        let cache = Rc::clone(&cache);
        let settings = Rc::clone(&settings);
        let search = search.clone();
        glib::timeout_add_seconds_local(1, move || {
            if settings.auto_refresh.get() {
                refresh_history(
                    &state,
                    &ui,
                    &cache,
                    search.text().as_str(),
                    settings.row_limit.get(),
                );
            }
            glib::ControlFlow::Continue
        });
    }

    refresh_history(&state, &ui, &cache, "", settings.row_limit.get());
    window.present();
}

fn refresh_history(
    state: &Rc<AppState>,
    ui: &Rc<Ui>,
    cache: &Rc<RefCell<String>>,
    query: &str,
    limit: u32,
) {
    let json = match state.items_json(query, limit) {
        Ok(json) => json,
        Err(error) => {
            ui.show_empty(
                "network-error-symbolic",
                "pasteharbord is unavailable",
                &error.to_string(),
            );
            ui.set_status("Disconnected");
            return;
        }
    };

    if cache.borrow().as_str() == json {
        return;
    }
    *cache.borrow_mut() = json.clone();

    while let Some(row) = ui.list.first_child() {
        ui.list.remove(&row);
    }

    match serde_json::from_str::<Vec<ClipboardItem>>(&json) {
        Ok(items) if items.is_empty() => {
            ui.show_empty(
                "edit-copy-symbolic",
                "No clipboard history yet",
                "Copy text or an image to get started.",
            );
            ui.set_status("Empty");
        }
        Ok(items) => {
            ui.set_status(&format!("{} item(s)", items.len()));
            for item in &items {
                ui.list.append(&history_row(state, ui, item));
            }
            ui.stack.set_visible_child_name("list");
        }
        Err(error) => {
            ui.show_empty(
                "dialog-error-symbolic",
                "Could not read clipboard history",
                &error.to_string(),
            );
        }
    }
}

fn history_row(state: &Rc<AppState>, ui: &Rc<Ui>, item: &ClipboardItem) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(glib::markup_escape_text(&item.preview_text))
        .subtitle(glib::markup_escape_text(&subtitle(item)))
        .activatable(true)
        .build();

    row.add_prefix(&leading_widget(state, item));

    let copy_button = icon_button("edit-copy-symbolic", "Copy again");
    let delete_button = icon_button("user-trash-symbolic", "Delete");
    row.add_suffix(&copy_button);
    row.add_suffix(&delete_button);

    let is_image = item.kind == kind::IMAGE;

    {
        let state = Rc::clone(state);
        let ui = Rc::clone(ui);
        let id = item.id;
        copy_button.connect_clicked(move |_| copy_item(&state, &ui, id, is_image));
    }

    {
        let state = Rc::clone(state);
        let ui = Rc::clone(ui);
        let id = item.id;
        row.connect_activated(move |_| copy_item(&state, &ui, id, is_image));
    }

    {
        let state = Rc::clone(state);
        let ui = Rc::clone(ui);
        let id = item.id;
        let row_weak = row.downgrade();
        delete_button.connect_clicked(move |_| match state.delete_item(id) {
            Ok(true) => {
                if let Some(row) = row_weak.upgrade() {
                    ui.list.remove(&row);
                }
                ui.toast("Deleted item");
                ui.sync_view();
            }
            Ok(false) => ui.toast("Item was already deleted"),
            Err(error) => ui.toast(&format!("Could not delete item: {error}")),
        });
    }

    row
}

fn copy_item(state: &Rc<AppState>, ui: &Rc<Ui>, id: i64, is_image: bool) {
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    let clipboard = display.clipboard();

    if is_image {
        match state.get_image(id) {
            Ok((bytes, _mime)) => {
                let gbytes = glib::Bytes::from(&bytes[..]);
                match gtk::gdk::Texture::from_bytes(&gbytes) {
                    Ok(texture) => {
                        clipboard.set_texture(&texture);
                        ui.toast("Copied image to clipboard");
                    }
                    Err(error) => ui.toast(&format!("Could not decode image: {error}")),
                }
            }
            Err(error) => ui.toast(&format!("Could not restore image: {error}")),
        }
    } else {
        match state.get_text(id) {
            Ok(text) => {
                clipboard.set_text(&text);
                ui.toast("Copied to clipboard");
            }
            Err(error) => ui.toast(&format!("Could not restore item: {error}")),
        }
    }
}

/// Leading visual: a thumbnail (images) or a typed avatar (text), badged with
/// the icon of the app the content was copied from when we can resolve it.
fn leading_widget(state: &Rc<AppState>, item: &ClipboardItem) -> gtk::Widget {
    let base: gtk::Widget = if item.kind == kind::IMAGE && item.has_thumbnail {
        let picture = gtk::Picture::new();
        picture.set_size_request(44, 44);
        picture.set_content_fit(gtk::ContentFit::Cover);
        picture.set_overflow(gtk::Overflow::Hidden);
        picture.add_css_class("ph-thumb");
        if let Ok(bytes) = state.get_thumbnail(item.id) {
            let gbytes = glib::Bytes::from(&bytes[..]);
            if let Ok(texture) = gtk::gdk::Texture::from_bytes(&gbytes) {
                picture.set_paintable(Some(&texture));
            }
        }
        picture.upcast()
    } else {
        let icon = gtk::Image::from_icon_name(type_icon(item));
        icon.set_pixel_size(22);
        icon.set_size_request(44, 44);
        icon.add_css_class("ph-avatar");
        icon.upcast()
    };

    match app_gicon(item.source_app.as_deref()) {
        Some(gicon) => {
            let overlay = gtk::Overlay::new();
            overlay.set_child(Some(&base));
            let badge = gtk::Image::from_gicon(&gicon);
            badge.set_pixel_size(18);
            badge.set_halign(gtk::Align::End);
            badge.set_valign(gtk::Align::End);
            badge.add_css_class("ph-badge");
            overlay.add_overlay(&badge);
            overlay.upcast()
        }
        None => base,
    }
}

fn subtitle(item: &ClipboardItem) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(name) = app_display_name(item.source_app.as_deref()) {
        parts.push(name);
    }
    if item.kind == kind::IMAGE {
        if let (Some(width), Some(height)) = (item.width, item.height) {
            parts.push(format!("{width}×{height}"));
        }
    }
    parts.push(format_bytes(item.size_bytes));
    parts.push(format_timestamp(&item.last_used_at));
    parts.join("  ·  ")
}

fn type_icon(item: &ClipboardItem) -> &'static str {
    if item.kind == kind::IMAGE {
        "image-x-generic-symbolic"
    } else {
        "text-x-generic-symbolic"
    }
}

fn desktop_info(source_app: &str) -> Option<gio::DesktopAppInfo> {
    if let Some(info) = gio::DesktopAppInfo::new(source_app) {
        return Some(info);
    }
    if !source_app.ends_with(".desktop") {
        return gio::DesktopAppInfo::new(&format!("{source_app}.desktop"));
    }
    None
}

fn app_gicon(source_app: Option<&str>) -> Option<gio::Icon> {
    desktop_info(source_app?).and_then(|info| info.icon())
}

fn app_display_name(source_app: Option<&str>) -> Option<String> {
    desktop_info(source_app?).map(|info| info.name().to_string())
}

fn icon_button(icon: &str, tooltip: &str) -> gtk::Button {
    gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(tooltip)
        .valign(gtk::Align::Center)
        .css_classes(vec!["flat".to_string()])
        .build()
}

fn show_clear_confirmation(
    parent: &adw::ApplicationWindow,
    state: &Rc<AppState>,
    ui: &Rc<Ui>,
    cache: &Rc<RefCell<String>>,
    search: &gtk::SearchEntry,
    settings: &Rc<UiSettings>,
) {
    let dialog = adw::MessageDialog::builder()
        .transient_for(parent)
        .modal(true)
        .heading("Clear clipboard history?")
        .body("This permanently deletes every stored clipboard item.")
        .build();
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("clear", "Clear");
    dialog.set_response_appearance("clear", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");

    let state = Rc::clone(state);
    let ui = Rc::clone(ui);
    let cache = Rc::clone(cache);
    let search = search.clone();
    let settings = Rc::clone(settings);
    dialog.connect_response(None, move |_, response| {
        if response == "clear" {
            match state.clear() {
                Ok(count) => {
                    cache.borrow_mut().clear();
                    ui.toast(&format!("Cleared {count} item(s)"));
                    refresh_history(
                        &state,
                        &ui,
                        &cache,
                        search.text().as_str(),
                        settings.row_limit.get(),
                    );
                }
                Err(error) => ui.toast(&format!("Could not clear history: {error}")),
            }
        }
    });
    dialog.present();
}

fn show_settings(parent: &adw::ApplicationWindow, state: &Rc<AppState>, settings: &Rc<UiSettings>) {
    let window = adw::PreferencesWindow::builder()
        .title("PasteHarbor Settings")
        .transient_for(parent)
        .modal(true)
        .default_width(460)
        .default_height(360)
        .build();

    let page = adw::PreferencesPage::new();
    let group = adw::PreferencesGroup::builder().title("Clipboard history").build();

    let auto_refresh_row = adw::SwitchRow::builder()
        .title("Auto-refresh open window")
        .subtitle("Keep this window in sync with new copies")
        .active(settings.auto_refresh.get())
        .build();
    {
        let settings = Rc::clone(settings);
        auto_refresh_row.connect_active_notify(move |row| {
            settings.auto_refresh.set(row.is_active());
        });
    }

    let row_limit_row = adw::SpinRow::builder()
        .title("Rows shown in window")
        .subtitle("How many recent items this window displays")
        .adjustment(&gtk::Adjustment::new(
            settings.row_limit.get() as f64,
            10.0,
            250.0,
            10.0,
            25.0,
            0.0,
        ))
        .build();
    {
        let settings = Rc::clone(settings);
        row_limit_row.connect_value_notify(move |row| {
            settings.row_limit.set(row.value() as u32);
        });
    }

    let max_history = state
        .settings()
        .map(|settings| settings.max_history)
        .unwrap_or(DEFAULT_MAX_HISTORY);
    let max_history_row = adw::SpinRow::builder()
        .title("Maximum stored history")
        .subtitle("Older items beyond this count are pruned")
        .adjustment(&gtk::Adjustment::new(
            max_history as f64,
            f64::from(MIN_MAX_HISTORY),
            f64::from(MAX_MAX_HISTORY),
            10.0,
            100.0,
            0.0,
        ))
        .build();
    {
        let state = Rc::clone(state);
        let window = window.clone();
        max_history_row.connect_value_notify(move |row| {
            if let Err(error) = state.set_max_history(row.value() as u32) {
                let toast = adw::Toast::new(&format!("Could not save: {error}"));
                window.add_toast(toast);
            }
        });
    }

    group.add(&auto_refresh_row);
    group.add(&row_limit_row);
    group.add(&max_history_row);
    page.add(&group);
    window.add(&page);
    window.present();
}
