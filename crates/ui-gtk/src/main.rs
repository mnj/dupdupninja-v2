#[cfg(all(target_os = "linux", feature = "gtk"))]
fn main() {
    use adw::prelude::*;
    use gtk4 as gtk;
    use gtk::gio;
    use gtk::glib;
    use std::cell::RefCell;
    use std::rc::Rc;
    const APP_ID: &str = "com.dupdupninja.app";

    if let Err(err) = adw::init() {
        eprintln!("libadwaita init failed: {err}");
    }

    let app = adw::Application::new(Some(APP_ID), gio::ApplicationFlags::empty());
    let ui_state: Rc<RefCell<Option<UiState>>> = Rc::new(RefCell::new(None));

    let quit = gio::SimpleAction::new("quit", None);
    quit.connect_activate(glib::clone!(
        #[weak]
        app,
        move |_, _| app.quit()
    ));
    app.add_action(&quit);
    app.set_accels_for_action("app.quit", &["<primary>q"]);

    let scan_folder = gio::SimpleAction::new("scan_folder", None);
    scan_folder.connect_activate(glib::clone!(
        #[weak]
        app,
        #[strong]
        ui_state,
        move |_, _| {
            if let Some(window) = app.active_window() {
                let ui_state = ui_state.clone();
                glib::MainContext::ref_thread_default().spawn_local(async move {
                    let dialog = gtk::FileDialog::new();
                    dialog.set_title("Select a folder to scan");
                    match dialog.select_folder_future(Some(&window)).await {
                        Ok(folder) => {
                            if let Some(path) = folder.path() {
                                start_scan(ui_state.clone(), path, dupdupninja_core::ScanRootKind::Folder);
                            }
                        }
                        Err(err) => {
                            eprintln!("folder selection error: {err}");
                        }
                    }
                });
            }
        }
    ));
    app.add_action(&scan_folder);

    let scan_disk = gio::SimpleAction::new("scan_disk", None);
    scan_disk.connect_activate(glib::clone!(
        #[weak]
        app,
        #[strong]
        ui_state,
        move |_, _| {
            if let Some(window) = app.active_window() {
                let ui_state = ui_state.clone();
                select_mount_path(&window, move |path| {
                    let ui_state = ui_state.clone();
                    if let Some(path) = path {
                        start_scan(ui_state.clone(), path, dupdupninja_core::ScanRootKind::Drive);
                    }
                });
            }
        }
    ));
    app.add_action(&scan_disk);

    let settings = gio::SimpleAction::new("settings", None);
    settings.connect_activate(glib::clone!(
        #[weak]
        app,
        move |_, _| {
            if let Some(window) = app.active_window() {
                let settings_window = gtk::Window::builder()
                    .transient_for(&window)
                    .title("Settings")
                    .default_width(520)
                    .default_height(360)
                    .modal(true)
                    .build();
                let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
                content.set_margin_top(18);
                content.set_margin_bottom(18);
                content.set_margin_start(18);
                content.set_margin_end(18);
                content.append(&gtk::Label::new(Some(
                    "Settings are not implemented yet.",
                )));
                settings_window.set_child(Some(&content));
                settings_window.present();
            }
        }
    ));
    app.add_action(&settings);
    app.set_accels_for_action("app.settings", &["<primary>comma"]);

    let about = gio::SimpleAction::new("about", None);
    about.connect_activate(glib::clone!(
        #[weak]
        app,
        move |_, _| {
            if let Some(window) = app.active_window() {
                let dialog = gtk::AboutDialog::builder()
                    .transient_for(&window)
                    .modal(true)
                    .program_name("dupdupninja")
                    .version(env!("CARGO_PKG_VERSION"))
                    .comments("Cross-platform duplicate/near-duplicate media finder.")
                    .build();
                dialog.present();
            }
        }
    ));
    app.add_action(&about);

    let app_menu = gio::Menu::new();
    app_menu.append(Some("Settings…"), Some("app.settings"));
    app_menu.append(Some("About"), Some("app.about"));
    app_menu.append(Some("Exit"), Some("app.quit"));

    app.connect_activate(move |app| {
        let window = adw::ApplicationWindow::new(app);
        window.set_title(Some("dupdupninja"));
        window.set_default_size(1100, 720);

        let header = adw::HeaderBar::new();
        let new_scan_menu = gio::Menu::new();
        new_scan_menu.append(Some("Scan Folder…"), Some("app.scan_folder"));
        new_scan_menu.append(Some("Scan Disk…"), Some("app.scan_disk"));
        let new_scan_button = gtk::MenuButton::new();
        new_scan_button.set_icon_name("list-add-symbolic");
        new_scan_button.set_tooltip_text(Some("New scan/fileset"));
        let new_scan_popover = gtk::PopoverMenu::from_model(Some(&new_scan_menu));
        new_scan_button.set_popover(Some(&new_scan_popover));
        header.pack_start(&new_scan_button);

        let menu_button = gtk::MenuButton::new();
        menu_button.set_icon_name("open-menu-symbolic");
        let popover = gtk::PopoverMenu::from_model(Some(&app_menu));
        menu_button.set_popover(Some(&popover));
        header.pack_end(&menu_button);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&content));

        let status_bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        status_bar.set_margin_top(6);
        status_bar.set_margin_bottom(6);
        status_bar.set_margin_start(12);
        status_bar.set_margin_end(12);

        let status_label = gtk::Label::new(Some("Status: Idle"));
        status_label.set_xalign(0.0);
        status_label.set_hexpand(true);
        let progress = gtk::ProgressBar::new();
        progress.set_fraction(0.0);
        progress.set_show_text(true);
        progress.set_text(Some("Idle"));
        let cancel_button = gtk::Button::with_label("Cancel");
        cancel_button.set_sensitive(false);
        status_bar.append(&status_label);
        status_bar.append(&progress);
        status_bar.append(&cancel_button);

        toolbar.add_bottom_bar(&status_bar);
        window.set_content(Some(&toolbar));

        let (update_tx, update_rx) = std::sync::mpsc::channel::<UiUpdate>();
        *ui_state.borrow_mut() = Some(UiState {
            status_label: status_label.clone(),
            progress: progress.clone(),
            cancel_button: cancel_button.clone(),
            cancel_token: None,
            update_tx: update_tx.clone(),
            total_files: 0,
            total_bytes: 0,
        });

        let ui_state_for_cancel = ui_state.clone();
        cancel_button.connect_clicked(move |_| {
            if let Some(state) = ui_state_for_cancel.borrow().as_ref() {
                if let Some(token) = state.cancel_token.as_ref() {
                    token.cancel();
                }
            }
        });

        let ui_state_for_updates = ui_state.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            while let Ok(update) = update_rx.try_recv() {
                if let Some(state) = ui_state_for_updates.borrow_mut().as_mut() {
                    match update {
                        UiUpdate::PrescanProgress { text } => {
                            state.status_label.set_text(&text);
                            state.progress.set_text(Some("Preparing..."));
                            state.progress.pulse();
                        }
                        UiUpdate::PrescanDone { totals } => {
                            state.total_files = totals.files;
                            state.total_bytes = totals.bytes;
                            state.status_label.set_text("Status: Scanning...");
                            state.progress.set_fraction(0.0);
                            state.progress.set_text(Some("0%"));
                        }
                        UiUpdate::Progress { text, fraction } => {
                            state.status_label.set_text(&text);
                            if let Some(fraction) = fraction {
                                state.progress.set_fraction(fraction.clamp(0.0, 1.0));
                                let percent = (fraction * 100.0).round() as u32;
                                state.progress.set_text(Some(&format!("{percent}%")));
                            } else {
                                state.progress.set_text(Some("Scanning..."));
                                state.progress.pulse();
                            }
                        }
                        UiUpdate::Done { text } => {
                            state.status_label.set_text(&text);
                            state.progress.set_text(Some("Idle"));
                            state.progress.set_fraction(0.0);
                            state.cancel_button.set_sensitive(false);
                            state.cancel_token = None;
                            state.total_files = 0;
                            state.total_bytes = 0;
                        }
                        UiUpdate::Error { text } => {
                            state.status_label.set_text(&text);
                            state.progress.set_text(Some("Idle"));
                            state.progress.set_fraction(0.0);
                            state.cancel_button.set_sensitive(false);
                            state.cancel_token = None;
                            state.total_files = 0;
                            state.total_bytes = 0;
                        }
                    }
                }
            }
            glib::ControlFlow::Continue
        });

        window.present();
        window.maximize();
    });

    app.run();
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
struct UiState {
    status_label: gtk4::Label,
    progress: gtk4::ProgressBar,
    cancel_button: gtk4::Button,
    cancel_token: Option<dupdupninja_core::scan::ScanCancelToken>,
    update_tx: std::sync::mpsc::Sender<UiUpdate>,
    total_files: u64,
    total_bytes: u64,
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
enum UiUpdate {
    PrescanProgress { text: String },
    PrescanDone { totals: dupdupninja_core::scan::ScanTotals },
    Progress { text: String, fraction: Option<f64> },
    Done { text: String },
    Error { text: String },
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn start_scan(
    ui_state: std::rc::Rc<std::cell::RefCell<Option<UiState>>>,
    root: std::path::PathBuf,
    root_kind: dupdupninja_core::ScanRootKind,
) {
    use gtk4::prelude::WidgetExt;
    let (status_label, progress, cancel_button, update_tx) = {
        let state = ui_state.borrow();
        let Some(state) = state.as_ref() else {
            return;
        };
        (
            state.status_label.clone(),
            state.progress.clone(),
            state.cancel_button.clone(),
            state.update_tx.clone(),
        )
    };

    let cancel_token = dupdupninja_core::scan::ScanCancelToken::new();
    {
        let mut state = ui_state.borrow_mut();
        if let Some(state) = state.as_mut() {
            state.cancel_token = Some(cancel_token.clone());
        }
    }

    status_label.set_text("Status: Scanning...");
    progress.set_text(Some("Scanning..."));
    progress.pulse();
    cancel_button.set_sensitive(true);

    std::thread::spawn(move || {
        let db_path = scan_db_path();
        let store = match dupdupninja_core::db::SqliteScanStore::open(&db_path) {
            Ok(store) => store,
            Err(err) => {
                let msg = format!("Status: DB error: {err}");
                let _ = update_tx.send(UiUpdate::Error { text: msg });
                return;
            }
        };

        let cfg = dupdupninja_core::scan::ScanConfig {
            root: root.clone(),
            root_kind,
            hash_files: true,
        };

    let prescan_result = dupdupninja_core::scan::prescan(&cfg, Some(&cancel_token), |progress| {
            let folder = progress
                .current_path
                .file_name()
                .and_then(|p| p.to_str())
                .unwrap_or("folder");
            let text = format!(
                "Status: Preparing {} ({} files)",
                folder, progress.files_seen
            );
            let _ = update_tx.send(UiUpdate::PrescanProgress { text });
        });

        let totals = match prescan_result {
            Ok(totals) => totals,
            Err(dupdupninja_core::Error::Cancelled) => {
                let _ = update_tx.send(UiUpdate::Done {
                    text: "Status: Scan cancelled".to_string(),
                });
                return;
            }
            Err(err) => {
                let _ = update_tx.send(UiUpdate::Error {
                    text: format!("Status: Prescan error: {err}"),
                });
                return;
            }
        };

        let _ = update_tx.send(UiUpdate::PrescanDone { totals });

        let result = dupdupninja_core::scan::scan_to_sqlite_with_progress_and_totals(
            &cfg,
            &store,
            Some(&cancel_token),
            Some(totals),
            |progress_update| {
                let path = progress_update
                    .current_path
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|p| p.to_str())
                    .unwrap_or("folder");
                let text = format!(
                    "Status: Scanning {} ({} / {} files)",
                    path,
                    progress_update.files_seen,
                    progress_update.total_files
                );
                let fraction = if progress_update.total_files > 0 {
                    Some(progress_update.files_seen as f64 / progress_update.total_files as f64)
                } else {
                    None
                };
                let _ = update_tx.send(UiUpdate::Progress { text, fraction });
            },
        );

        let update = match result {
            Ok(result) => UiUpdate::Done {
                text: format!(
                    "Status: Scan complete ({} files, {} hashed, {} skipped)",
                    result.stats.files_seen,
                    result.stats.files_hashed,
                    result.stats.files_skipped
                ),
            },
            Err(dupdupninja_core::Error::Cancelled) => UiUpdate::Done {
                text: "Status: Scan cancelled".to_string(),
            },
            Err(err) => UiUpdate::Error {
                text: format!("Status: Scan error: {err}"),
            },
        };
        let _ = update_tx.send(update);
    });
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn scan_db_path() -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    path.push(format!("dupdupninja-scan-{ts}.sqlite3"));
    path
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn select_mount_path<F>(window: &gtk4::Window, on_selected: F)
where
    F: Fn(Option<std::path::PathBuf>) + 'static,
{
    use gtk4 as gtk;
    use gtk::gio;
    use gtk::prelude::*;
    use adw::prelude::*;

    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);

    let title = gtk::Label::new(Some("Select a disk/mount to scan"));
    title.add_css_class("title-4");
    title.set_xalign(0.0);
    content.append(&title);

    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.add_css_class("boxed-list");
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_child(Some(&list));
    scroller.set_vexpand(true);
    content.append(&scroller);

    let mut entries = mount_entries_from_proc();
    if entries.is_empty() {
        let monitor = gio::VolumeMonitor::get();
        for mount in monitor.mounts() {
            let root = mount.root();
            let path = match root.path() {
                Some(path) => path,
                None => continue,
            };
            let title = mount.name().to_string();
            let subtitle = format!("{}", path.display());
            let detail = mount_detail("", &path.display().to_string());
            entries.push(MountEntry {
                title,
                subtitle,
                detail,
                icon_name: "drive-harddisk-symbolic",
                path,
            });
        }
    }

    for entry in entries {
        let row = gtk::ListBoxRow::new();
        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row_box.set_margin_top(6);
        row_box.set_margin_bottom(6);
        row_box.set_margin_start(10);
        row_box.set_margin_end(10);

        let icon = gtk::Image::from_icon_name(entry.icon_name);
        icon.set_pixel_size(20);
        row_box.append(&icon);

        let text_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let title = gtk::Label::new(Some(&entry.title));
        title.set_xalign(0.0);
        let subtitle = gtk::Label::new(Some(&entry.subtitle));
        subtitle.set_xalign(0.0);
        subtitle.add_css_class("dim-label");
        let detail = gtk::Label::new(Some(&entry.detail));
        detail.set_xalign(0.0);
        detail.add_css_class("dim-label");
        text_box.append(&title);
        text_box.append(&subtitle);
        text_box.append(&detail);
        row_box.append(&text_box);

        row.set_child(Some(&row_box));
        row.set_activatable(true);
        row.set_selectable(true);
        unsafe {
            row.set_data("mount-path", entry.path);
        }
        list.append(&row);
    }

    let button_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    button_row.set_halign(gtk::Align::End);
    let cancel_button = gtk::Button::with_label("Cancel");
    let select_button = gtk::Button::with_label("Select");
    select_button.add_css_class("suggested-action");
    select_button.set_sensitive(false);
    button_row.append(&cancel_button);
    button_row.append(&select_button);
    content.append(&button_row);

    let dialog = adw::Dialog::builder()
        .content_width(520)
        .content_height(360)
        .child(&content)
        .build();

    let callback = std::rc::Rc::new(std::cell::RefCell::new(Some(Box::new(on_selected))));

    let callback_for_select = callback.clone();
    let list_for_select = list.clone();
    let dialog_for_select = dialog.clone();
    select_button.connect_clicked(move |_| {
        let selection = list_for_select.selected_row().and_then(|row| unsafe {
            row.data::<std::path::PathBuf>("mount-path")
                .map(|p| p.as_ref().clone())
        });
        if let Some(callback) = callback_for_select.borrow_mut().take() {
            callback(selection);
        }
        let _ = dialog_for_select.close();
    });

    let callback_for_cancel = callback.clone();
    let dialog_for_cancel = dialog.clone();
    cancel_button.connect_clicked(move |_| {
        if let Some(callback) = callback_for_cancel.borrow_mut().take() {
            callback(None);
        }
        let _ = dialog_for_cancel.close();
    });

    let select_button_for_row = select_button.clone();
    list.connect_row_selected(move |_, row| {
        select_button_for_row.set_sensitive(row.is_some());
    });

    let callback_for_closed = callback.clone();
    dialog.connect_closed(move |_| {
        if let Some(callback) = callback_for_closed.borrow_mut().take() {
            callback(None);
        }
    });

    dialog.present(Some(window));
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
struct MountEntry {
    title: String,
    subtitle: String,
    detail: String,
    icon_name: &'static str,
    path: std::path::PathBuf,
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn mount_entries_from_proc() -> Vec<MountEntry> {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;

    let contents = match fs::read_to_string("/proc/self/mountinfo") {
        Ok(contents) => contents,
        Err(_) => return Vec::new(),
    };

    let mut entries: BTreeMap<PathBuf, (String, String, String, &'static str)> = BTreeMap::new();
    for line in contents.lines() {
        let mut parts = line.split(" - ");
        let left = match parts.next() {
            Some(left) => left,
            None => continue,
        };
        let right = match parts.next() {
            Some(right) => right,
            None => continue,
        };

        let left_fields: Vec<&str> = left.split_whitespace().collect();
        if left_fields.len() < 5 {
            continue;
        }
        let mount_point = unescape_mount_field(left_fields[4]);
        let right_fields: Vec<&str> = right.split_whitespace().collect();
        if right_fields.len() < 2 {
            continue;
        }
        let fs_type = right_fields[0];
        let source = right_fields[1];

        if !should_include_mount(source, &mount_point) {
            continue;
        }

        let path = PathBuf::from(&mount_point);
        let subtitle = format!("{mount_point} [{fs_type}]");
        let detail = mount_detail(source, &mount_point);
        let icon_name = icon_for_mount(source, &mount_point);
        entries
            .entry(path)
            .or_insert((source.to_string(), subtitle, detail, icon_name));
    }

    entries
        .into_iter()
        .map(|(path, (title, subtitle, detail, icon_name))| MountEntry {
            title,
            subtitle,
            detail,
            icon_name,
            path,
        })
        .collect()
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn should_include_mount(source: &str, mount_point: &str) -> bool {
    source.starts_with("/dev/")
        || mount_point == "/"
        || mount_point.starts_with("/run/media/")
        || mount_point.starts_with("/media/")
        || mount_point.starts_with("/mnt/")
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn icon_for_mount(source: &str, mount_point: &str) -> &'static str {
    if mount_point.starts_with("/run/media/") || mount_point.starts_with("/media/") {
        "media-removable-symbolic"
    } else if mount_point == "/" || source.starts_with("/dev/") {
        "drive-harddisk-symbolic"
    } else {
        "drive-harddisk-symbolic"
    }
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn mount_detail(source: &str, mount_point: &str) -> String {
    let fs = filesystem_bytes(mount_point);
    let model = device_model(source);
    match (fs, model) {
        (Some((total, free)), Some(model)) => format!(
            "{} total • {} free • {}",
            human_bytes(total),
            human_bytes(free),
            model
        ),
        (Some((total, free)), None) => format!(
            "{} total • {} free",
            human_bytes(total),
            human_bytes(free)
        ),
        (None, Some(model)) => model,
        (None, None) => "Details unavailable".to_string(),
    }
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn filesystem_bytes(mount_point: &str) -> Option<(u64, u64)> {
    use std::ffi::CString;
    let c_path = CString::new(mount_point).ok()?;
    unsafe {
        let mut st: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c_path.as_ptr(), &mut st) != 0 {
            return None;
        }
        let total = st.f_blocks as u64 * st.f_frsize as u64;
        let free = st.f_bavail as u64 * st.f_frsize as u64;
        Some((total, free))
    }
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn device_model(source: &str) -> Option<String> {
    let dev_name = device_name_from_source(source)?;
    let model_path = format!("/sys/class/block/{}/device/model", dev_name);
    let model = std::fs::read_to_string(model_path).ok()?;
    let model = model.trim();
    if model.is_empty() {
        None
    } else {
        Some(model.to_string())
    }
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn device_name_from_source(source: &str) -> Option<String> {
    if source.starts_with("/dev/") {
        let path = std::path::Path::new(source);
        if source.starts_with("/dev/mapper/") {
            if let Ok(link) = std::fs::read_link(path) {
                if let Some(name) = link.file_name().and_then(|v| v.to_str()) {
                    return Some(name.to_string());
                }
            }
        }
        return path
            .file_name()
            .and_then(|v| v.to_str())
            .map(|v| v.to_string());
    }
    None
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn human_bytes(value: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut size = value as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit + 1 < UNITS.len() {
        size /= 1024.0;
        unit += 1;
    }
    format!("{:.1} {}", size, UNITS[unit])
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn unescape_mount_field(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            let a = chars.next();
            let b = chars.next();
            let c = chars.next();
            if let (Some(a), Some(b), Some(c)) = (a, b, c) {
                if a.is_ascii_digit() && b.is_ascii_digit() && c.is_ascii_digit() {
                    let oct = [a, b, c].iter().collect::<String>();
                    if let Ok(val) = u8::from_str_radix(&oct, 8) {
                        out.push(val as char);
                        continue;
                    }
                }
                out.push('\\');
                out.push(a);
                out.push(b);
                out.push(c);
                continue;
            }
            out.push('\\');
            if let Some(a) = a {
                out.push(a);
            }
            if let Some(b) = b {
                out.push(b);
            }
            if let Some(c) = c {
                out.push(c);
            }
            continue;
        }
        out.push(ch);
    }
    out
}

#[cfg(not(all(target_os = "linux", feature = "gtk")))]
fn main() {
    println!("dupdupninja-ui-gtk stub. On Ubuntu: install GTK4 dev packages and build with `--features gtk`.");
}
