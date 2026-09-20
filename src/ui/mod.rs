pub mod dialogs;
pub mod window;

use std::path::PathBuf;

use shadow_convert::paths;

pub fn load_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(include_str!("style.css"));
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

pub fn register_icons() {
    gtk::Window::set_default_icon_name(paths::APP_ICON);
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    let theme = gtk::IconTheme::for_display(&display);
    for path in icon_search_paths() {
        theme.add_search_path(path);
    }
}

fn icon_search_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut push = |p: PathBuf| {
        let svg = p.join("hicolor/scalable/apps/shadow-convert.svg");
        let png = p.join("hicolor/256x256/apps/shadow-convert.png");
        if !(svg.is_file() || png.is_file()) {
            return;
        }
        if let Ok(canon) = p.canonicalize() {
            if !out.contains(&canon) {
                out.push(canon);
            }
        } else if !out.contains(&p) {
            out.push(p);
        }
    };

    if let Ok(cwd) = std::env::current_dir() {
        push(cwd.join("data/icons"));
    }
    if let Some(exe) = std::env::current_exe().ok() {
        if let Some(dir) = exe.parent() {
            push(dir.join("../../data/icons"));
            push(dir.join("../share/icons"));
            push(dir.join("share/icons"));
        }
    }
    out
}

pub fn icon_paintable() -> Option<gtk::gdk::Texture> {
    const PNG: &[u8] = include_bytes!("../../data/icons/hicolor/256x256/apps/shadow-convert.png");
    gtk::gdk::Texture::from_bytes(&gtk::glib::Bytes::from_static(PNG)).ok()
}

pub fn apply_theme(theme: shadow_convert::Theme) {
    let manager = adw::StyleManager::default();
    let scheme = match theme {
        shadow_convert::Theme::System => adw::ColorScheme::Default,
        shadow_convert::Theme::Light => adw::ColorScheme::ForceLight,
        shadow_convert::Theme::Dark => adw::ColorScheme::ForceDark,
    };
    manager.set_color_scheme(scheme);
}
