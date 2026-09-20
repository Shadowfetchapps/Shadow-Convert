use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use adw::prelude::*;
use gtk::gdk;
use gtk::gio;
use gtk::glib;

use shadow_convert::detect::MediaKind;
use shadow_convert::paths;
use shadow_convert::plan::{
    estimate_size_bytes, AudioFormat, ImageFormat, JobKind, JobRequest, ResolutionTarget,
    VideoContainer,
};
use shadow_convert::probe::{self, Probe};
use shadow_convert::settings::{Quality, Settings};
use shadow_convert::{convert, Error};

use crate::ui::{self, dialogs};

enum Page {
    Drop,
    Inspect,
    Progress,
    Done,
}

struct Widgets {
    window: adw::ApplicationWindow,
    toast: adw::ToastOverlay,
    stack: gtk::Stack,
    drop_zone: gtk::Box,
    file_name: gtk::Label,
    kind_badge: gtk::Label,
    meta_box: gtk::Box,
    action_drop: gtk::DropDown,
    format_drop: gtk::DropDown,
    resolution_drop: gtk::DropDown,
    normalize: gtk::Switch,
    compress: gtk::Switch,
    remove_audio: gtk::Switch,
    trim_start: gtk::Entry,
    trim_end: gtk::Entry,
    trim_row: gtk::Box,
    quality_drop: gtk::DropDown,
    encoder_drop: gtk::DropDown,
    bitrate: gtk::Entry,
    advanced: gtk::Expander,
    output_label: gtk::Label,
    estimate_label: gtk::Label,
    convert_btn: gtk::Button,
    progress: gtk::ProgressBar,
    progress_label: gtk::Label,
    cancel_btn: gtk::Button,
    done_title: gtk::Label,
    done_path: gtk::Label,
    hw_label: gtk::Label,
}

struct State {
    settings: RefCell<Settings>,
    probe: RefCell<Option<Probe>>,
    last_output: RefCell<Option<PathBuf>>,
    cancel: RefCell<Option<Arc<AtomicBool>>>,
    busy: Cell<bool>,
}

enum UiMsg {
    Progress(shadow_convert::ffmpeg::Progress),
    Finished(std::result::Result<shadow_convert::ffmpeg::RunResult, ErrorMsg>),
}

struct ErrorMsg {
    human: String,
    technical: Option<String>,
}

pub fn present(app: &adw::Application, initial: Vec<PathBuf>) {
    ui::load_css();
    let settings = Settings::load();
    ui::apply_theme(settings.theme);

    if let Some(win) = app.active_window() {
        win.present();
        return;
    }

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title(paths::APP_NAME)
        .default_width(720)
        .default_height(860)
        .build();
    window.set_icon_name(Some(paths::APP_ICON));

    let toast = adw::ToastOverlay::new();
    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();

    let open_btn = gtk::Button::from_icon_name("document-open-symbolic");
    open_btn.set_tooltip_text(Some("Open a file"));
    let settings_btn = gtk::Button::from_icon_name("emblem-system-symbolic");
    settings_btn.set_tooltip_text(Some("Settings"));
    let about_btn = gtk::Button::from_icon_name("help-about-symbolic");
    about_btn.set_tooltip_text(Some("About Shadow Convert"));
    header.pack_start(&open_btn);
    header.pack_end(&settings_btn);
    header.pack_end(&about_btn);
    toolbar.add_top_bar(&header);

    let stack = gtk::Stack::new();
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);

    let drop_page = build_drop_page();
    let inspect = build_inspect_page();
    let progress_page = build_progress_page();
    let done_page = build_done_page();

    stack.add_named(&drop_page.0, Some("drop"));
    stack.add_named(&inspect.page, Some("inspect"));
    stack.add_named(&progress_page.0, Some("progress"));
    stack.add_named(&done_page.0, Some("done"));

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&brand_row());
    content.append(&stack);
    toolbar.set_content(Some(&content));
    toast.set_child(Some(&toolbar));
    window.set_content(Some(&toast));

    let widgets = Rc::new(Widgets {
        window: window.clone(),
        toast,
        stack,
        drop_zone: drop_page.1,
        file_name: inspect.file_name,
        kind_badge: inspect.kind_badge,
        meta_box: inspect.meta_box,
        action_drop: inspect.action_drop,
        format_drop: inspect.format_drop,
        resolution_drop: inspect.resolution_drop,
        normalize: inspect.normalize,
        compress: inspect.compress,
        remove_audio: inspect.remove_audio,
        trim_start: inspect.trim_start,
        trim_end: inspect.trim_end,
        trim_row: inspect.trim_row,
        quality_drop: inspect.quality_drop,
        encoder_drop: inspect.encoder_drop,
        bitrate: inspect.bitrate,
        advanced: inspect.advanced,
        output_label: inspect.output_label,
        estimate_label: inspect.estimate_label,
        convert_btn: inspect.convert_btn,
        progress: progress_page.1,
        progress_label: progress_page.2,
        cancel_btn: progress_page.3,
        done_title: done_page.1,
        done_path: done_page.2,
        hw_label: inspect.hw_label,
    });

    let state = Rc::new(State {
        settings: RefCell::new(settings),
        probe: RefCell::new(None),
        last_output: RefCell::new(None),
        cancel: RefCell::new(None),
        busy: Cell::new(false),
    });

    widgets
        .quality_drop
        .set_selected(state.settings.borrow().quality.index());
    widgets.hw_label.set_text(&format!(
        "Encoders: {}",
        shadow_convert::hw::detect().summary()
    ));

    setup_drop_target(&widgets, &state);
    bind_actions(&widgets, &state, &open_btn, &settings_btn, &about_btn, &drop_page.2);
    bind_inspect_updates(&widgets, &state);
    bind_done_buttons(&widgets, &state, &done_page.3, &done_page.4, &done_page.5);

    let open_action = gio::SimpleAction::new("open", None);
    let widgets_c = widgets.clone();
    let state_c = state.clone();
    open_action.connect_activate(move |_, _| choose_file(&widgets_c, &state_c));
    window.add_action(&open_action);
    app.set_accels_for_action("win.open", &["<Ctrl>o"]);

    let settings_action = gio::SimpleAction::new("settings", None);
    let widgets_c = widgets.clone();
    let state_c = state.clone();
    settings_action.connect_activate(move |_, _| open_settings(&widgets_c, &state_c));
    window.add_action(&settings_action);
    app.set_accels_for_action("win.settings", &["<Ctrl>comma"]);

    window.present();

    if let Some(path) = initial.into_iter().next() {
        load_path(&widgets, &state, &path);
    }
}

fn brand_row() -> gtk::Box {
    let kicker = gtk::Label::new(Some("SHADOW CONVERT"));
    kicker.add_css_class("brand-kicker");
    kicker.set_xalign(0.0);
    let title = gtk::Label::new(Some("Convert any media file"));
    title.add_css_class("brand-title");
    title.set_xalign(0.0);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.append(&kicker);
    text.append(&title);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.set_margin_start(22);
    row.set_margin_end(22);
    row.set_margin_top(10);
    row.set_margin_bottom(4);
    if let Some(tex) = ui::icon_paintable() {
        let image = gtk::Image::from_paintable(Some(&tex));
        image.set_pixel_size(48);
        image.add_css_class("app-icon");
        row.append(&image);
    }
    row.append(&text);
    row
}

fn build_drop_page() -> (gtk::Box, gtk::Box, gtk::Button) {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 16);
    page.set_margin_start(22);
    page.set_margin_end(22);
    page.set_margin_top(12);
    page.set_margin_bottom(24);

    let zone = gtk::Box::new(gtk::Orientation::Vertical, 10);
    zone.add_css_class("drop-zone");
    zone.set_halign(gtk::Align::Fill);
    zone.set_valign(gtk::Align::Fill);
    zone.set_hexpand(true);
    zone.set_vexpand(true);
    let title = gtk::Label::new(Some("Drop a file here"));
    title.add_css_class("drop-title");
    title.set_wrap(true);
    let hint = gtk::Label::new(Some(
        "Video, audio, or image. Spaces and Unicode names are fine.",
    ));
    hint.add_css_class("dim-label");
    hint.set_wrap(true);
    hint.set_justify(gtk::Justification::Center);
    let choose = gtk::Button::with_label("Choose File");
    choose.add_css_class("suggested-action");
    choose.add_css_class("pill");
    choose.set_halign(gtk::Align::Center);
    choose.set_tooltip_text(Some("Open a file picker"));
    zone.append(&title);
    zone.append(&hint);
    zone.append(&choose);
    zone.set_halign(gtk::Align::Fill);
    title.set_halign(gtk::Align::Center);
    hint.set_halign(gtk::Align::Center);
    choose.set_halign(gtk::Align::Center);
    page.append(&zone);
    (page, zone, choose)
}

struct InspectPage {
    page: gtk::ScrolledWindow,
    file_name: gtk::Label,
    kind_badge: gtk::Label,
    meta_box: gtk::Box,
    action_drop: gtk::DropDown,
    format_drop: gtk::DropDown,
    resolution_drop: gtk::DropDown,
    normalize: gtk::Switch,
    compress: gtk::Switch,
    remove_audio: gtk::Switch,
    trim_start: gtk::Entry,
    trim_end: gtk::Entry,
    trim_row: gtk::Box,
    quality_drop: gtk::DropDown,
    encoder_drop: gtk::DropDown,
    bitrate: gtk::Entry,
    advanced: gtk::Expander,
    output_label: gtk::Label,
    estimate_label: gtk::Label,
    convert_btn: gtk::Button,
    hw_label: gtk::Label,
}

fn build_inspect_page() -> InspectPage {
    let page_box = gtk::Box::new(gtk::Orientation::Vertical, 14);
    page_box.set_margin_start(22);
    page_box.set_margin_end(22);
    page_box.set_margin_top(8);
    page_box.set_margin_bottom(24);

    let name = gtk::Label::new(None);
    name.add_css_class("file-name");
    name.set_xalign(0.0);
    name.set_wrap(true);
    name.set_selectable(true);
    let badge = gtk::Label::new(None);
    badge.add_css_class("kind-badge");
    badge.set_halign(gtk::Align::Start);
    page_box.append(&name);
    page_box.append(&badge);

    let meta = gtk::Box::new(gtk::Orientation::Vertical, 4);
    page_box.append(&section("Details"));
    page_box.append(&meta);

    let action = dropdown(&["Convert"]);
    let format = dropdown(&["MP4"]);
    let resolution = dropdown(&["Original", "1080p", "1440p", "4K"]);
    page_box.append(&section("Action"));
    page_box.append(&labeled("What to do", action.clone()));
    page_box.append(&labeled("Format", format.clone()));
    page_box.append(&labeled("Resolution", resolution.clone()));

    let normalize = gtk::Switch::new();
    normalize.set_valign(gtk::Align::Center);
    page_box.append(&switch_row("Normalize loudness", &normalize));
    let compress = gtk::Switch::new();
    compress.set_valign(gtk::Align::Center);
    page_box.append(&switch_row("Compress", &compress));
    let remove_audio = gtk::Switch::new();
    remove_audio.set_valign(gtk::Align::Center);
    page_box.append(&switch_row("Remove audio", &remove_audio));

    let trim_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let trim_start = gtk::Entry::new();
    trim_start.set_placeholder_text(Some("Start (seconds)"));
    trim_start.set_hexpand(true);
    let trim_end = gtk::Entry::new();
    trim_end.set_placeholder_text(Some("End (seconds)"));
    trim_end.set_hexpand(true);
    trim_row.append(&trim_start);
    trim_row.append(&trim_end);
    page_box.append(&trim_row);

    page_box.append(&section("Quality"));
    let quality = dropdown(&["Small", "Balanced", "High", "Maximum"]);
    quality.set_selected(2);
    page_box.append(&labeled("Quality", quality.clone()));

    let encoder = dropdown(&["Auto"]);
    let bitrate = gtk::Entry::new();
    bitrate.set_placeholder_text(Some("Bitrate kbps (optional)"));
    let adv_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
    adv_box.append(&labeled("Encoder", encoder.clone()));
    adv_box.append(&bitrate);
    let advanced = gtk::Expander::new(Some("Advanced"));
    advanced.set_child(Some(&adv_box));
    page_box.append(&advanced);

    page_box.append(&section("Output"));
    let output = gtk::Label::new(None);
    output.set_xalign(0.0);
    output.set_wrap(true);
    output.set_selectable(true);
    output.add_css_class("dim-label");
    let estimate = gtk::Label::new(None);
    estimate.set_xalign(0.0);
    estimate.add_css_class("dim-label");
    page_box.append(&output);
    page_box.append(&estimate);

    let convert_btn = gtk::Button::with_label("Convert");
    convert_btn.add_css_class("suggested-action");
    convert_btn.add_css_class("convert-button");
    convert_btn.add_css_class("pill");
    convert_btn.set_tooltip_text(Some("Start conversion"));
    page_box.append(&convert_btn);

    let another = gtk::Button::with_label("Choose a different file");
    another.add_css_class("flat");
    another.set_action_name(Some("win.open"));
    page_box.append(&another);

    let hw = gtk::Label::new(None);
    hw.add_css_class("dim-label");
    hw.set_xalign(0.0);
    hw.set_wrap(true);
    page_box.append(&hw);

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_child(Some(&page_box));
    scroll.set_vexpand(true);

    InspectPage {
        page: scroll,
        file_name: name,
        kind_badge: badge,
        meta_box: meta,
        action_drop: action,
        format_drop: format,
        resolution_drop: resolution,
        normalize,
        compress,
        remove_audio,
        trim_start,
        trim_end,
        trim_row,
        quality_drop: quality,
        encoder_drop: encoder,
        bitrate,
        advanced,
        output_label: output,
        estimate_label: estimate,
        convert_btn,
        hw_label: hw,
    }
}

fn build_progress_page() -> (gtk::Box, gtk::ProgressBar, gtk::Label, gtk::Button) {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 16);
    page.set_margin_start(28);
    page.set_margin_end(28);
    page.set_margin_top(48);
    page.set_valign(gtk::Align::Center);
    let title = gtk::Label::new(Some("Converting"));
    title.add_css_class("brand-title");
    let bar = gtk::ProgressBar::new();
    bar.set_show_text(true);
    bar.set_fraction(0.0);
    let label = gtk::Label::new(Some("Starting…"));
    label.add_css_class("dim-label");
    label.set_wrap(true);
    let cancel = gtk::Button::with_label("Cancel");
    cancel.add_css_class("destructive-action");
    cancel.add_css_class("pill");
    cancel.set_halign(gtk::Align::Center);
    cancel.set_tooltip_text(Some("Stop the conversion"));
    page.append(&title);
    page.append(&bar);
    page.append(&label);
    page.append(&cancel);
    (page, bar, label, cancel)
}

fn build_done_page() -> (
    gtk::Box,
    gtk::Label,
    gtk::Label,
    gtk::Button,
    gtk::Button,
    gtk::Button,
) {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 12);
    page.set_margin_start(28);
    page.set_margin_end(28);
    page.set_margin_top(36);
    page.set_valign(gtk::Align::Center);
    let title = gtk::Label::new(Some("Finished"));
    title.add_css_class("brand-title");
    let path = gtk::Label::new(None);
    path.set_wrap(true);
    path.set_selectable(true);
    path.add_css_class("dim-label");
    let open = gtk::Button::with_label("Open");
    open.add_css_class("suggested-action");
    open.add_css_class("pill");
    let folder = gtk::Button::with_label("Open Folder");
    folder.add_css_class("pill");
    let again = gtk::Button::with_label("Convert Another");
    again.add_css_class("flat");
    open.set_halign(gtk::Align::Center);
    folder.set_halign(gtk::Align::Center);
    again.set_halign(gtk::Align::Center);
    page.append(&title);
    page.append(&path);
    page.append(&open);
    page.append(&folder);
    page.append(&again);
    (page, title, path, open, folder, again)
}

fn section(text: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.add_css_class("heading");
    l.set_xalign(0.0);
    l
}

fn dropdown(items: &[&str]) -> gtk::DropDown {
    gtk::DropDown::from_strings(items)
}

fn labeled(title: &str, child: impl IsA<gtk::Widget>) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let l = gtk::Label::new(Some(title));
    l.set_xalign(0.0);
    l.add_css_class("meta-key");
    row.append(&l);
    row.append(&child);
    row
}

fn switch_row(title: &str, switch: &gtk::Switch) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let l = gtk::Label::new(Some(title));
    l.set_xalign(0.0);
    l.set_hexpand(true);
    row.append(&l);
    row.append(switch);
    row
}

fn setup_drop_target(widgets: &Rc<Widgets>, state: &Rc<State>) {
    let target = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
    let zone_enter = widgets.drop_zone.clone();
    target.connect_enter(move |_, _, _| {
        zone_enter.add_css_class("drop-hover");
        gdk::DragAction::COPY
    });
    let zone_leave = widgets.drop_zone.clone();
    target.connect_leave(move |_| {
        zone_leave.remove_css_class("drop-hover");
    });
    let widgets_drop = widgets.clone();
    let state_drop = state.clone();
    target.connect_drop(move |_, value, _, _| {
        widgets_drop.drop_zone.remove_css_class("drop-hover");
        if let Ok(list) = value.get::<gdk::FileList>() {
            let files: Vec<PathBuf> = list.files().iter().filter_map(|f| f.path()).collect();
            if files.len() > 1 {
                widgets_drop.toast.add_toast(adw::Toast::new(
                    "Opened the first file. Use Shadow Batch Processor for many files.",
                ));
            }
            if let Some(path) = files.into_iter().next() {
                load_path(&widgets_drop, &state_drop, &path);
            }
            return true;
        }
        false
    });
    widgets.drop_zone.add_controller(target);

    let inspect_target = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
    let widgets_c = widgets.clone();
    let state_c = state.clone();
    inspect_target.connect_drop(move |_, value, _, _| {
        if let Ok(list) = value.get::<gdk::FileList>() {
            if let Some(path) = list.files().iter().find_map(|f| f.path()) {
                load_path(&widgets_c, &state_c, &path);
                return true;
            }
        }
        false
    });
    widgets.window.add_controller(inspect_target);
}

fn bind_actions(
    widgets: &Rc<Widgets>,
    state: &Rc<State>,
    open_btn: &gtk::Button,
    settings_btn: &gtk::Button,
    about_btn: &gtk::Button,
    drop_choose: &gtk::Button,
) {
    let w = widgets.clone();
    let s = state.clone();
    open_btn.connect_clicked(move |_| choose_file(&w, &s));
    let w = widgets.clone();
    let s = state.clone();
    drop_choose.connect_clicked(move |_| choose_file(&w, &s));
    let w = widgets.clone();
    let s = state.clone();
    settings_btn.connect_clicked(move |_| open_settings(&w, &s));
    let window = widgets.window.clone();
    about_btn.connect_clicked(move |_| dialogs::show_about(&window));

    let w = widgets.clone();
    let s = state.clone();
    widgets.convert_btn.connect_clicked(move |_| start_convert(&w, &s));

    let s = state.clone();
    let w = widgets.clone();
    widgets.cancel_btn.connect_clicked(move |_| {
        if let Some(flag) = s.cancel.borrow().as_ref() {
            flag.store(true, Ordering::SeqCst);
            w.progress_label.set_text("Cancelling…");
        }
    });
}

fn bind_inspect_updates(widgets: &Rc<Widgets>, state: &Rc<State>) {
    let refresh = {
        let widgets = widgets.clone();
        let state = state.clone();
        move || refresh_inspect(&widgets, &state)
    };
    let r = refresh.clone();
    widgets.action_drop.connect_selected_notify(move |_| r());
    let r = refresh.clone();
    widgets.format_drop.connect_selected_notify(move |_| r());
    let r = refresh.clone();
    widgets.resolution_drop.connect_selected_notify(move |_| r());
    let r = refresh.clone();
    widgets.quality_drop.connect_selected_notify(move |_| r());
    let r = refresh.clone();
    widgets.compress.connect_active_notify(move |_| r());
    let r = refresh.clone();
    widgets.remove_audio.connect_active_notify(move |_| r());
    let r = refresh;
    widgets.normalize.connect_active_notify(move |_| r());
}

fn bind_done_buttons(
    widgets: &Rc<Widgets>,
    state: &Rc<State>,
    open: &gtk::Button,
    folder: &gtk::Button,
    again: &gtk::Button,
) {
    let state_c = state.clone();
    open.connect_clicked(move |_| {
        if let Some(path) = state_c.last_output.borrow().as_ref() {
            let _ = open::that_detached(path);
        }
    });
    let state_c = state.clone();
    folder.connect_clicked(move |_| {
        if let Some(path) = state_c.last_output.borrow().as_ref() {
            if let Some(parent) = path.parent() {
                let _ = open::that_detached(parent);
            }
        }
    });
    let widgets = widgets.clone();
    let state = state.clone();
    again.connect_clicked(move |_| {
        state.probe.replace(None);
        show_page(&widgets, Page::Drop);
    });
}

fn choose_file(widgets: &Rc<Widgets>, state: &Rc<State>) {
    let dialog = gtk::FileDialog::new();
    dialog.set_title("Choose a file to convert");
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("Media files"));
    filter.add_mime_type("video/*");
    filter.add_mime_type("audio/*");
    filter.add_mime_type("image/*");
    let all = gtk::FileFilter::new();
    all.set_name(Some("All files"));
    all.add_pattern("*");
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    filters.append(&all);
    dialog.set_filters(Some(&filters));
    let widgets_cb = widgets.clone();
    let state_cb = state.clone();
    let window = widgets.window.clone();
    dialog.open(Some(&window), gio::Cancellable::NONE, move |result| {
        if let Ok(file) = result {
            if let Some(path) = file.path() {
                load_path(&widgets_cb, &state_cb, &path);
            }
        }
    });
}

fn open_settings(widgets: &Rc<Widgets>, state: &Rc<State>) {
    let current = state.settings.borrow().clone();
    let state_cb = state.clone();
    let widgets_cb = widgets.clone();
    let window = widgets.window.clone();
    dialogs::show_settings(&window, &current, move |updated| {
        ui::apply_theme(updated.theme);
        state_cb.settings.replace(updated);
        refresh_inspect(&widgets_cb, &state_cb);
    });
}

fn load_path(widgets: &Rc<Widgets>, state: &Rc<State>, path: &Path) {
    if state.busy.get() {
        widgets
            .toast
            .add_toast(adw::Toast::new("Wait for the current conversion to finish."));
        return;
    }
    if path.is_dir() {
        dialogs::show_message(
            &widgets.window,
            "That is a folder",
            "Drop a video, audio, or image file. Use Shadow Batch Processor for folders of files.",
        );
        return;
    }
    match shadow_convert::probe_file(path) {
        Ok(probe) => {
            populate_inspect(widgets, state, probe);
            show_page(widgets, Page::Inspect);
        }
        Err(err) => dialogs::show_error(&widgets.window, &err),
    }
}

fn populate_inspect(widgets: &Widgets, state: &State, probe: Probe) {
    widgets
        .file_name
        .set_text(&probe.path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
    widgets.kind_badge.set_text(probe.kind.as_label());
    while let Some(child) = widgets.meta_box.first_child() {
        widgets.meta_box.remove(&child);
    }
    for (k, v) in probe.summary_lines() {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let key = gtk::Label::new(Some(&k));
        key.add_css_class("meta-key");
        key.set_xalign(0.0);
        key.set_width_chars(12);
        let val = gtk::Label::new(Some(&v));
        val.set_xalign(0.0);
        val.set_hexpand(true);
        val.set_selectable(true);
        row.append(&key);
        row.append(&val);
        widgets.meta_box.append(&row);
    }

    let actions = actions_for(probe.kind);
    widgets
        .action_drop
        .set_model(Some(&gtk::StringList::new(&actions)));
    widgets.action_drop.set_selected(0);
    fill_formats(widgets, probe.kind, 0);
    fill_encoders(widgets);
    widgets.compress.set_active(false);
    widgets.remove_audio.set_active(false);
    widgets.normalize.set_active(false);
    widgets.trim_start.set_text("");
    widgets.trim_end.set_text("");
    widgets.quality_drop.set_selected(state.settings.borrow().quality.index());
    state.probe.replace(Some(probe));
    refresh_inspect(widgets, state);
}

fn actions_for(kind: MediaKind) -> Vec<&'static str> {
    match kind {
        MediaKind::Video => vec![
            "Convert video",
            "Compress",
            "Extract audio",
            "Remove audio",
            "Trim",
            "Make GIF",
        ],
        MediaKind::Audio => vec!["Convert audio", "Normalize", "Trim"],
        MediaKind::Image => vec!["Convert image", "Resize", "Compress"],
        MediaKind::Unknown => vec!["Convert"],
    }
}

fn fill_formats(widgets: &Widgets, kind: MediaKind, action_idx: u32) {
    let labels: &[&str] = match kind {
        MediaKind::Video if action_idx == 2 => &["MP3", "WAV", "FLAC", "AAC", "Opus"],
        MediaKind::Video if action_idx == 5 => &["GIF"],
        MediaKind::Video => &["MP4", "MKV", "WebM"],
        MediaKind::Audio => &["MP3", "WAV", "FLAC", "AAC", "Opus"],
        MediaKind::Image => &["PNG", "JPEG", "WebP", "AVIF"],
        MediaKind::Unknown => &["MP4"],
    };
    widgets.format_drop.set_model(Some(&gtk::StringList::new(labels)));
    widgets.format_drop.set_selected(0);
}

fn fill_encoders(widgets: &Widgets) {
    let caps = shadow_convert::hw::detect();
    let labels: Vec<&str> = caps.video_encoder_choices().into_iter().map(|(_, l)| l).collect();
    widgets
        .encoder_drop
        .set_model(Some(&gtk::StringList::new(&labels)));
    widgets.encoder_drop.set_selected(0);
}

fn selected_label(drop: &gtk::DropDown) -> String {
    drop.selected_item()
        .and_downcast::<gtk::StringObject>()
        .map(|s| s.string().to_string())
        .unwrap_or_default()
}

fn refresh_inspect(widgets: &Widgets, state: &State) {
    let Some(probe) = state.probe.borrow().clone() else {
        return;
    };
    let action_idx = widgets.action_drop.selected();
    let expected = actions_for(probe.kind);
    if let Some(cur) = expected.get(action_idx as usize) {
        let _ = cur;
    }
    // Rebuild format list when action changes.
    static LAST: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(u32::MAX);
    // Per-window last action stored on quality? Use a simple heuristic: if format model size mismatches, refill.
    let formats_now = match (probe.kind, action_idx) {
        (MediaKind::Video, 2) => 5u32,
        (MediaKind::Video, 5) => 1,
        (MediaKind::Video, _) => 3,
        (MediaKind::Audio, _) => 5,
        (MediaKind::Image, _) => 4,
        _ => 1,
    };
    let model_n = widgets
        .format_drop
        .model()
        .map(|m| m.n_items())
        .unwrap_or(0);
    if model_n != formats_now {
        fill_formats(widgets, probe.kind, action_idx);
    }
    let _ = LAST;

    let is_trim = matches!(
        (probe.kind, action_idx),
        (MediaKind::Video, 4) | (MediaKind::Audio, 2)
    );
    widgets.trim_row.set_visible(is_trim);
    widgets.normalize.set_visible(matches!(
        (probe.kind, action_idx),
        (MediaKind::Audio, 0) | (MediaKind::Audio, 1)
    ));
    if probe.kind == MediaKind::Audio && action_idx == 1 {
        widgets.normalize.set_active(true);
    }
    widgets.compress.set_visible(matches!(
        (probe.kind, action_idx),
        (MediaKind::Video, 0) | (MediaKind::Image, 0) | (MediaKind::Image, 2)
    ));
    widgets.remove_audio.set_visible(probe.kind == MediaKind::Video && action_idx == 0);
    widgets.resolution_drop.set_visible(matches!(
        (probe.kind, action_idx),
        (MediaKind::Video, 0) | (MediaKind::Video, 1) | (MediaKind::Image, 1)
    ));
    widgets.advanced.set_visible(probe.kind == MediaKind::Video && action_idx <= 1);

    if let Ok(req) = build_request(&probe, widgets, &state.settings.borrow()) {
        if let Ok(plan) = shadow_convert::plan_job(&probe, &req) {
            widgets
                .output_label
                .set_text(&format!("Will save to {}", paths::display_home_path(&plan.output)));
            if let Some(est) = estimate_size_bytes(&probe, &req) {
                widgets.estimate_label.set_text(&format!(
                    "Estimated size about {}",
                    probe::format_bytes(est)
                ));
            } else {
                widgets.estimate_label.set_text("");
            }
        }
    }
}

fn build_request(probe: &Probe, widgets: &Widgets, settings: &Settings) -> Result<JobRequest, Error> {
    let action_idx = widgets.action_drop.selected();
    let format = selected_label(&widgets.format_drop);
    let quality = Quality::from_index(widgets.quality_drop.selected());
    let resolution = match widgets.resolution_drop.selected() {
        1 => ResolutionTarget::P1080,
        2 => ResolutionTarget::P1440,
        3 => ResolutionTarget::P2160,
        _ => ResolutionTarget::Original,
    };
    let encoder_override = encoder_id(widgets.encoder_drop.selected());
    let bitrate_k = widgets
        .bitrate
        .text()
        .as_str()
        .trim()
        .parse::<u32>()
        .ok();
    let kind = match probe.kind {
        MediaKind::Video => match action_idx {
            1 => JobKind::Video {
                container: parse_video_container(&format)?,
                resolution,
                remove_audio: false,
                compress: true,
            },
            2 => JobKind::ExtractAudio {
                format: parse_audio_format(&format)?,
            },
            3 => JobKind::Video {
                container: parse_video_container(&format).unwrap_or(VideoContainer::Mp4),
                resolution: ResolutionTarget::Original,
                remove_audio: true,
                compress: false,
            },
            4 => JobKind::TrimVideo {
                start: parse_secs(&widgets.trim_start.text())?,
                end: parse_secs_opt(&widgets.trim_end.text())?,
                container: parse_video_container(&format).unwrap_or(VideoContainer::Mp4),
            },
            5 => JobKind::Gif { width: Some(480) },
            _ => JobKind::Video {
                container: parse_video_container(&format)?,
                resolution,
                remove_audio: widgets.remove_audio.is_active(),
                compress: widgets.compress.is_active(),
            },
        },
        MediaKind::Audio => match action_idx {
            1 => JobKind::Audio {
                format: parse_audio_format(&format)?,
                normalize: true,
                start: None,
                end: None,
            },
            2 => JobKind::Audio {
                format: parse_audio_format(&format)?,
                normalize: widgets.normalize.is_active(),
                start: Some(parse_secs(&widgets.trim_start.text())?),
                end: parse_secs_opt(&widgets.trim_end.text())?,
            },
            _ => JobKind::Audio {
                format: parse_audio_format(&format)?,
                normalize: widgets.normalize.is_active(),
                start: None,
                end: None,
            },
        },
        MediaKind::Image => {
            let (width, height) = if action_idx == 1 {
                match resolution {
                    ResolutionTarget::P1080 => (Some(1920), Some(1080)),
                    ResolutionTarget::P1440 => (Some(2560), Some(1440)),
                    ResolutionTarget::P2160 => (Some(3840), Some(2160)),
                    ResolutionTarget::Original => (None, None),
                }
            } else {
                (None, None)
            };
            JobKind::Image {
                format: parse_image_format(&format)?,
                width,
                height,
                compress: action_idx == 2 || widgets.compress.is_active(),
            }
        }
        MediaKind::Unknown => {
            return Err(Error::user("This file type is not supported."));
        }
    };
    Ok(JobRequest {
        kind,
        quality,
        prefer_hardware: settings.prefer_hardware,
        preserve_metadata: settings.preserve_metadata,
        encoder_override,
        bitrate_k,
        output_dir: settings.output_dir.clone(),
    })
}

fn encoder_id(index: u32) -> Option<String> {
    shadow_convert::hw::detect()
        .video_encoder_choices()
        .get(index as usize)
        .map(|(id, _)| (*id).to_string())
        .filter(|id| id != "auto")
}

fn parse_video_container(label: &str) -> Result<VideoContainer, Error> {
    match label.to_ascii_lowercase().as_str() {
        "mp4" => Ok(VideoContainer::Mp4),
        "mkv" => Ok(VideoContainer::Mkv),
        "webm" => Ok(VideoContainer::Webm),
        other => Err(Error::user(format!("Unknown video format: {other}"))),
    }
}

fn parse_audio_format(label: &str) -> Result<AudioFormat, Error> {
    match label.to_ascii_lowercase().as_str() {
        "mp3" => Ok(AudioFormat::Mp3),
        "wav" => Ok(AudioFormat::Wav),
        "flac" => Ok(AudioFormat::Flac),
        "aac" | "m4a" => Ok(AudioFormat::Aac),
        "opus" => Ok(AudioFormat::Opus),
        other => Err(Error::user(format!("Unknown audio format: {other}"))),
    }
}

fn parse_image_format(label: &str) -> Result<ImageFormat, Error> {
    match label.to_ascii_lowercase().as_str() {
        "png" => Ok(ImageFormat::Png),
        "jpeg" | "jpg" => Ok(ImageFormat::Jpeg),
        "webp" => Ok(ImageFormat::Webp),
        "avif" => Ok(ImageFormat::Avif),
        other => Err(Error::user(format!("Unknown image format: {other}"))),
    }
}

fn parse_secs(text: &str) -> Result<f64, Error> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(0.0);
    }
    t.parse::<f64>()
        .map_err(|_| Error::user("Enter trim times in seconds, for example 3.5."))
}

fn parse_secs_opt(text: &str) -> Result<Option<f64>, Error> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(None);
    }
    Ok(Some(parse_secs(t)?))
}

fn start_convert(widgets: &Rc<Widgets>, state: &Rc<State>) {
    if state.busy.get() {
        return;
    }
    let Some(probe) = state.probe.borrow().clone() else {
        return;
    };
    let request = match build_request(&probe, widgets, &state.settings.borrow()) {
        Ok(r) => r,
        Err(err) => {
            dialogs::show_error(&widgets.window, &err);
            return;
        }
    };
    let cancel = Arc::new(AtomicBool::new(false));
    state.cancel.replace(Some(cancel.clone()));
    state.busy.set(true);
    widgets.progress.set_fraction(0.0);
    widgets.progress.set_text(Some("0%"));
    widgets.progress_label.set_text("Starting…");
    show_page(widgets, Page::Progress);

    let (tx, rx) = async_channel::unbounded::<UiMsg>();
    thread::spawn(move || {
        let result = convert(&probe, &request, cancel, |p| {
            let _ = tx.send_blocking(UiMsg::Progress(p));
        });
        let msg = match result {
            Ok(r) => UiMsg::Finished(Ok(r)),
            Err(err) => UiMsg::Finished(Err(ErrorMsg {
                human: err.human_message(),
                technical: err.technical_details(),
            })),
        };
        let _ = tx.send_blocking(msg);
    });

    let widgets = widgets.clone();
    let state = state.clone();
    glib::spawn_future_local(async move {
        while let Ok(msg) = rx.recv().await {
            match msg {
                UiMsg::Progress(p) => {
                    widgets.progress.set_fraction(p.ratio.clamp(0.0, 1.0));
                    widgets
                        .progress
                        .set_text(Some(&format!("{:.0}%", p.ratio * 100.0)));
                    widgets.progress_label.set_text(&p.message);
                }
                UiMsg::Finished(Ok(result)) => {
                    state.busy.set(false);
                    state.last_output.replace(Some(result.output.clone()));
                    widgets.done_title.set_text("Finished");
                    widgets
                        .done_path
                        .set_text(&paths::display_home_path(&result.output));
                    show_page(&widgets, Page::Done);
                    break;
                }
                UiMsg::Finished(Err(err)) => {
                    state.busy.set(false);
                    show_page(&widgets, Page::Inspect);
                    let e = if let Some(tech) = err.technical {
                        Error::detailed(err.human, tech)
                    } else {
                        Error::user(err.human)
                    };
                    dialogs::show_error(&widgets.window, &e);
                    break;
                }
            }
        }
    });
}

fn show_page(widgets: &Widgets, page: Page) {
    let name = match page {
        Page::Drop => "drop",
        Page::Inspect => "inspect",
        Page::Progress => "progress",
        Page::Done => "done",
    };
    widgets.stack.set_visible_child_name(name);
}
