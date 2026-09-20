use std::process::ExitCode;

use gtk::gio;
use gtk::prelude::*;

use shadow_convert::paths;

pub fn run() -> ExitCode {
    adw::init().expect("libadwaita init");
    let app = adw::Application::builder()
        .application_id(paths::APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();
    app.connect_startup(|_| {
        crate::ui::register_icons();
    });
    app.connect_activate(|app| {
        if let Some(win) = app.active_window() {
            win.present();
        } else {
            crate::ui::window::present(app, Vec::new());
        }
    });
    app.connect_open(|app, files, _| {
        let paths = files
            .iter()
            .filter_map(|f| f.path())
            .collect::<Vec<_>>();
        crate::ui::window::present(app, paths);
    });
    let code = app.run();
    if code == gtk::glib::ExitCode::SUCCESS {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
