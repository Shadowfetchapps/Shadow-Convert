use adw::prelude::*;
use gtk::gio;
use gtk::glib;

use shadow_convert::paths;
use shadow_convert::settings::{Settings, Theme};
use shadow_convert::Error;

use crate::ui;

pub fn show_error(parent: &impl IsA<gtk::Window>, err: &Error) {
    let dialog = adw::MessageDialog::new(
        Some(parent),
        Some("Could not convert"),
        Some(&err.human_message()),
    );
    dialog.add_response("ok", "OK");
    if let Some(details) = err.technical_details() {
        dialog.add_response("details", "Show Technical Details");
        dialog.set_response_appearance("details", adw::ResponseAppearance::Suggested);
        let parent = parent.as_ref().clone();
        dialog.connect_response(None, move |dlg, response| {
            if response == "details" {
                let tech = adw::MessageDialog::new(
                    Some(&parent),
                    Some("Technical details"),
                    Some(&details),
                );
                tech.add_response("ok", "OK");
                tech.present();
            }
            dlg.close();
        });
    }
    dialog.present();
}

pub fn show_message(parent: &impl IsA<gtk::Window>, title: &str, body: &str) {
    let dialog = adw::MessageDialog::new(Some(parent), Some(title), Some(body));
    dialog.add_response("ok", "OK");
    dialog.present();
}

pub fn show_about(parent: &impl IsA<gtk::Window>) {
    let about = adw::AboutWindow::builder()
        .transient_for(parent)
        .modal(true)
        .application_name(paths::APP_NAME)
        .application_icon(paths::APP_ICON)
        .developer_name("ShadowfetchLinux")
        .version(paths::APP_VERSION)
        .comments("Convert video, audio, and images locally. No account, no telemetry, no browser.")
        .license_type(gtk::License::MitX11)
        .website(paths::APP_WEBSITE)
        .issue_url("https://github.com/ShadowfetchLinux/Shadow-Convert/issues")
        .copyright("© 2026 Shadow Convert contributors")
        .build();
    about.present();
}

pub fn show_settings(parent: &impl IsA<gtk::Window>, settings: &Settings, on_save: impl Fn(Settings) + 'static) {
    let window = adw::PreferencesWindow::builder()
        .transient_for(parent)
        .modal(true)
        .title("Settings")
        .search_enabled(false)
        .build();

    let page = adw::PreferencesPage::new();
    page.set_title("General");

    let output = adw::PreferencesGroup::new();
    output.set_title("Output");
    output.set_description(Some(
        "Converted files never overwrite the original. By default they are saved next to the source.",
    ));

    let folder_row = adw::ActionRow::new();
    folder_row.set_title("Save converted files to");
    let folder_label = settings
        .output_dir
        .as_ref()
        .map(|p| paths::display_home_path(p))
        .unwrap_or_else(|| "Same folder as the original".into());
    folder_row.set_subtitle(&folder_label);
    let choose = gtk::Button::from_icon_name("folder-open-symbolic");
    choose.set_valign(gtk::Align::Center);
    choose.set_tooltip_text(Some("Choose output folder"));
    choose.add_css_class("flat");
    let clear = gtk::Button::from_icon_name("edit-clear-symbolic");
    clear.set_valign(gtk::Align::Center);
    clear.set_tooltip_text(Some("Use the original file’s folder"));
    clear.add_css_class("flat");
    folder_row.add_suffix(&clear);
    folder_row.add_suffix(&choose);
    output.add(&folder_row);

    let quality_group = adw::PreferencesGroup::new();
    quality_group.set_title("Quality");
    let quality = adw::ComboRow::new();
    quality.set_title("Default quality");
    quality.set_subtitle("High is the default. Small makes smaller files.");
    quality.set_model(Some(&gtk::StringList::new(&[
        "Small",
        "Balanced",
        "High",
        "Maximum",
    ])));
    quality.set_selected(settings.quality.index());
    quality_group.add(&quality);

    let hw_row = adw::SwitchRow::new();
    hw_row.set_title("Prefer hardware encoding");
    hw_row.set_subtitle("Uses NVENC when it is available and useful. Always falls back to the CPU.");
    hw_row.set_active(settings.prefer_hardware);
    quality_group.add(&hw_row);

    let meta_row = adw::SwitchRow::new();
    meta_row.set_title("Preserve useful metadata");
    meta_row.set_subtitle("Turn off to strip titles, tags, and most sidecar metadata.");
    meta_row.set_active(settings.preserve_metadata);
    quality_group.add(&meta_row);

    let appear = adw::PreferencesGroup::new();
    appear.set_title("Appearance");
    let theme = adw::ComboRow::new();
    theme.set_title("Theme");
    theme.set_model(Some(&gtk::StringList::new(&["System", "Light", "Dark"])));
    theme.set_selected(settings.theme.index());
    appear.add(&theme);

    page.add(&output);
    page.add(&quality_group);
    page.add(&appear);
    window.add(&page);

    let current = std::rc::Rc::new(std::cell::RefCell::new(settings.clone()));
    let folder_row_c = folder_row.clone();
    let current_c = current.clone();
    choose.connect_clicked(glib::clone!(
        #[weak]
        window,
        move |_| {
            let dialog = gtk::FileDialog::new();
            dialog.set_title("Save converted files to");
            let current_c = current_c.clone();
            let folder_row_c = folder_row_c.clone();
            dialog.select_folder(Some(&window), gio::Cancellable::NONE, move |result| {
                if let Ok(file) = result {
                    if let Some(path) = file.path() {
                        folder_row_c.set_subtitle(&paths::display_home_path(&path));
                        current_c.borrow_mut().output_dir = Some(path);
                    }
                }
            });
        }
    ));
    let folder_row_c = folder_row.clone();
    let current_c = current.clone();
    clear.connect_clicked(move |_| {
        current_c.borrow_mut().output_dir = None;
        folder_row_c.set_subtitle("Same folder as the original");
    });

    let current_c = current.clone();
    quality.connect_selected_notify(move |row| {
        current_c.borrow_mut().quality = shadow_convert::Quality::from_index(row.selected());
    });
    let current_c = current.clone();
    hw_row.connect_active_notify(move |row| {
        current_c.borrow_mut().prefer_hardware = row.is_active();
    });
    let current_c = current.clone();
    meta_row.connect_active_notify(move |row| {
        current_c.borrow_mut().preserve_metadata = row.is_active();
    });
    let current_c = current.clone();
    theme.connect_selected_notify(move |row| {
        let t = Theme::from_index(row.selected());
        current_c.borrow_mut().theme = t;
        ui::apply_theme(t);
    });

    window.connect_close_request(move |_| {
        let snapshot = current.borrow().clone();
        let _ = snapshot.save();
        on_save(snapshot);
        glib::Propagation::Proceed
    });

    window.present();
}
