mod dbus;
mod format;
mod models;
mod ui;

use adw::prelude::*;
use gtk::glib;
use pasteharbor_core::APP_ID;
use ui::build_ui;

fn main() -> glib::ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run()
}
