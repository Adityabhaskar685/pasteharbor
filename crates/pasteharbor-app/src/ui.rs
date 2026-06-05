use adw::prelude::*;
use gtk::glib;
use std::cell::RefCell;
use std::rc::Rc;

use crate::dbus::AppState;
use crate::format::{format_bytes, format_timestamp};
use crate::models::{ClipboardItem, UiSettings};
use pasteharbor_core::{DEFAULT_MAX_HISTORY, MAX_MAX_HISTORY, MIN_MAX_HISTORY};

pub fn build_ui(app: &adw::Application) {
    if let Some(window) = app.active_window() {
        window.present();
        return;
    }

    let state = Rc::new(AppState::new());
    let ui_settings = Rc::new(UiSettings::default());
    let cache = Rc::new(RefCell::new(String::new()));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("PasteHarbor")
        .default_width(780)
        .default_height(620)
        .build();

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new(
        "PasteHarbor",
        "Clipboard history",
    )));

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
        .label("Connecting to pasteharbord...")
        .build();

    let scroller = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
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
        let cache = Rc::clone(&cache);
        let list = list.clone();
        let status = status.clone();
        let search = search.clone();
        let ui_settings = Rc::clone(&ui_settings);
        refresh_button.connect_clicked(move |_| {
            cache.borrow_mut().clear();
            refresh_history(
                &state,
                &cache,
                &list,
                &status,
                search.text().as_str(),
                ui_settings.row_limit.get(),
            );
        });
    }

    {
        let state = Rc::clone(&state);
        let cache = Rc::clone(&cache);
        let list = list.clone();
        let status = status.clone();
        let ui_settings = Rc::clone(&ui_settings);
        search.connect_search_changed(move |entry| {
            cache.borrow_mut().clear();
            refresh_history(
                &state,
                &cache,
                &list,
                &status,
                entry.text().as_str(),
                ui_settings.row_limit.get(),
            );
        });
    }

    {
        let state = Rc::clone(&state);
        let cache = Rc::clone(&cache);
        let list = list.clone();
        let status = status.clone();
        let search = search.clone();
        let ui_settings = Rc::clone(&ui_settings);
        let parent = window.clone();
        clear_button.connect_clicked(move |_| {
            show_clear_confirmation(
                &parent,
                &state,
                &cache,
                &list,
                &status,
                &search,
                &ui_settings,
            );
        });
    }

    {
        let parent = window.clone();
        let state = Rc::clone(&state);
        let ui_settings = Rc::clone(&ui_settings);
        settings_button.connect_clicked(move |_| show_settings(&parent, &state, &ui_settings));
    }

    {
        let state = Rc::clone(&state);
        let cache = Rc::clone(&cache);
        let list = list.clone();
        let status = status.clone();
        let search = search.clone();
        let ui_settings = Rc::clone(&ui_settings);
        glib::timeout_add_seconds_local(1, move || {
            if ui_settings.auto_refresh.get() {
                refresh_history(
                    &state,
                    &cache,
                    &list,
                    &status,
                    search.text().as_str(),
                    ui_settings.row_limit.get(),
                );
            }
            glib::ControlFlow::Continue
        });
    }

    refresh_history(
        &state,
        &cache,
        &list,
        &status,
        "",
        ui_settings.row_limit.get(),
    );
    window.present();
}

fn refresh_history(
    state: &AppState,
    cache: &RefCell<String>,
    list: &gtk::ListBox,
    status: &gtk::Label,
    query: &str,
    limit: u32,
) {
    let json = match state.items_json(query, limit) {
        Ok(json) => json,
        Err(error) => {
            status.set_label(&format!("pasteharbord is unavailable: {error}"));
            return;
        }
    };

    if cache.borrow().as_str() == json {
        return;
    }
    *cache.borrow_mut() = json.clone();

    while let Some(row) = list.first_child() {
        list.remove(&row);
    }

    match serde_json::from_str::<Vec<ClipboardItem>>(&json) {
        Ok(items) if items.is_empty() => {
            status.set_label("No clipboard history yet. Copy text to get started.");
        }
        Ok(items) => {
            status.set_label(&format!("{} recent item(s)", items.len()));
            for item in items {
                list.append(&history_row(state, &item, status));
            }
        }
        Err(error) => status.set_label(&format!("Could not read clipboard history: {error}")),
    }
}

fn history_row(state: &AppState, item: &ClipboardItem, status: &gtk::Label) -> gtk::ListBoxRow {
    let title = gtk::Label::builder()
        .label(&item.preview_text)
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .lines(2)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .build();

    let details = gtk::Label::builder()
        .label(format!(
            "{} | {} | {} | last used {}",
            item.source_app.as_deref().unwrap_or("unknown source"),
            item.kind,
            format_bytes(item.size_bytes),
            format_timestamp(&item.last_used_at)
        ))
        .xalign(0.0)
        .css_classes(vec!["dim-label".to_string()])
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .tooltip_text(format!("Captured {}", format_timestamp(&item.created_at)))
        .build();

    let copy_button = gtk::Button::builder()
        .icon_name("edit-copy-symbolic")
        .tooltip_text("Copy again")
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
                    status.set_label("Copied selected history item.");
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
            Ok(false) => status.set_label("History item was already deleted."),
            Err(error) => status.set_label(&format!("Could not delete item: {error}")),
        });
    }

    row
}

fn show_clear_confirmation(
    parent: &adw::ApplicationWindow,
    state: &Rc<AppState>,
    cache: &Rc<RefCell<String>>,
    list: &gtk::ListBox,
    status: &gtk::Label,
    search: &gtk::SearchEntry,
    ui_settings: &Rc<UiSettings>,
) {
    let dialog = gtk::MessageDialog::builder()
        .transient_for(parent)
        .modal(true)
        .message_type(gtk::MessageType::Warning)
        .text("Clear clipboard history?")
        .secondary_text("This permanently deletes every stored clipboard item.")
        .build();
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    dialog.add_button("Clear", gtk::ResponseType::Accept);

    let state = Rc::clone(state);
    let cache = Rc::clone(cache);
    let list = list.clone();
    let status = status.clone();
    let search = search.clone();
    let ui_settings = Rc::clone(ui_settings);
    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept {
            match state.clear() {
                Ok(count) => {
                    cache.borrow_mut().clear();
                    status.set_label(&format!("Cleared {count} history item(s)."));
                    refresh_history(
                        &state,
                        &cache,
                        &list,
                        &status,
                        search.text().as_str(),
                        ui_settings.row_limit.get(),
                    );
                }
                Err(error) => status.set_label(&format!("Could not clear history: {error}")),
            }
        }
        dialog.close();
    });
    dialog.present();
}

fn show_settings(parent: &adw::ApplicationWindow, state: &Rc<AppState>, settings: &Rc<UiSettings>) {
    let window = gtk::Window::builder()
        .title("PasteHarbor Settings")
        .transient_for(parent)
        .modal(true)
        .resizable(false)
        .default_width(420)
        .build();

    let status = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .css_classes(vec!["dim-label".to_string()])
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

    let row_limit_adjustment = gtk::Adjustment::new(
        settings.row_limit.get() as f64,
        10.0,
        250.0,
        10.0,
        25.0,
        0.0,
    );
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

    let max_history = state
        .settings()
        .map(|settings| settings.max_history)
        .unwrap_or(DEFAULT_MAX_HISTORY);
    let max_history_adjustment = gtk::Adjustment::new(
        max_history as f64,
        f64::from(MIN_MAX_HISTORY),
        f64::from(MAX_MAX_HISTORY),
        10.0,
        100.0,
        0.0,
    );
    let max_history_spin = gtk::SpinButton::builder()
        .adjustment(&max_history_adjustment)
        .numeric(true)
        .valign(gtk::Align::Center)
        .build();

    let content = gtk::Box::new(gtk::Orientation::Vertical, 14);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    content.set_margin_start(18);
    content.set_margin_end(18);
    content.append(&settings_row(
        "Auto-refresh open window",
        &auto_refresh_switch,
    ));
    content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    content.append(&settings_row("Rows shown in window", &row_limit_spin));
    content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    content.append(&settings_row("Maximum stored history", &max_history_spin));
    content.append(&status);

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    let cancel_button = gtk::Button::with_label("Cancel");
    let save_button = gtk::Button::with_label("Save");
    save_button.add_css_class("suggested-action");
    buttons.append(&cancel_button);
    buttons.append(&save_button);
    content.append(&buttons);

    {
        let window = window.clone();
        cancel_button.connect_clicked(move |_| window.close());
    }
    {
        let state = Rc::clone(state);
        let window = window.clone();
        let status = status.clone();
        save_button.connect_clicked(move |_| {
            match state.set_max_history(max_history_spin.value_as_int() as u32) {
                Ok(saved) => {
                    status.set_label(&format!("Saved maximum history: {saved}"));
                    window.close();
                }
                Err(error) => status.set_label(&format!("Could not save settings: {error}")),
            }
        });
    }

    window.set_child(Some(&content));
    window.present();
}

fn settings_row(label: &str, control: &impl IsA<gtk::Widget>) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let label = gtk::Label::builder()
        .label(label)
        .xalign(0.0)
        .hexpand(true)
        .build();
    row.append(&label);
    row.append(control);
    row
}
