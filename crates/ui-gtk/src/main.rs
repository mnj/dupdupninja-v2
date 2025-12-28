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
                                let db_path = scan_db_path(&path);
                                let name = fileset_name_from_path(&path);
                                let fileset_id =
                                    add_fileset(ui_state.clone(), name, db_path.clone());
                                start_scan(
                                    ui_state.clone(),
                                    path,
                                    dupdupninja_core::ScanRootKind::Folder,
                                    db_path,
                                    fileset_id,
                                );
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
                        let db_path = scan_db_path(&path);
                        let name = fileset_name_from_path(&path);
                        let fileset_id =
                            add_fileset(ui_state.clone(), name, db_path.clone());
                        start_scan(
                            ui_state.clone(),
                            path,
                            dupdupninja_core::ScanRootKind::Drive,
                            db_path,
                            fileset_id,
                        );
                    }
                });
            }
        }
    ));
    app.add_action(&scan_disk);

    let open_fileset = gio::SimpleAction::new("open_fileset", None);
    open_fileset.connect_activate(glib::clone!(
        #[weak]
        app,
        #[strong]
        ui_state,
        move |_, _| {
            if let Some(window) = app.active_window() {
                let ui_state = ui_state.clone();
                glib::MainContext::ref_thread_default().spawn_local(async move {
                    let dialog = gtk::FileDialog::new();
                    dialog.set_title("Open fileset database");
                    let filter = gtk::FileFilter::new();
                    filter.set_name(Some("DupdupNinja filesets (*.ddn)"));
                    filter.add_pattern("*.ddn");
                    dialog.set_default_filter(Some(&filter));
                    let filters = gio::ListStore::new::<gtk::FileFilter>();
                    filters.append(&filter);
                    dialog.set_filters(Some(&filters));
                    let fileset_dir = default_fileset_dir();
                    if fileset_dir.is_dir() {
                        dialog.set_initial_folder(Some(&gio::File::for_path(fileset_dir)));
                    } else {
                        let home_dir = gtk4::glib::home_dir();
                        dialog.set_initial_folder(Some(&gio::File::for_path(home_dir)));
                    }
                    if let Ok(file) = dialog.open_future(Some(&window)).await {
                        if let Some(path) = file.path() {
                            let name = fileset_name_from_db(&path);
                            add_fileset(ui_state.clone(), name, path);
                        }
                    }
                });
            }
        }
    ));
    app.add_action(&open_fileset);

    let settings = gio::SimpleAction::new("settings", None);
    settings.connect_activate(glib::clone!(
        #[weak]
        app,
        #[strong]
        ui_state,
        move |_, _| {
            if let Some(window) = app.active_window() {
                let (initial_capture, initial_count, initial_max_dim) = ui_state
                    .borrow()
                    .as_ref()
                    .map(|s| (s.capture_snapshots, s.snapshots_per_video, s.snapshot_max_dim))
                    .unwrap_or((true, 3, 1024));

                let settings_window = gtk::Window::builder()
                    .transient_for(&window)
                    .title("Settings")
                    .default_width(520)
                    .default_height(240)
                    .modal(true)
                    .build();

                let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
                content.set_margin_top(18);
                content.set_margin_bottom(18);
                content.set_margin_start(18);
                content.set_margin_end(18);

                let title = gtk::Label::new(Some("Scanning"));
                title.add_css_class("title-3");
                title.set_xalign(0.0);
                content.append(&title);

                let row1 = gtk::Box::new(gtk::Orientation::Horizontal, 12);
                row1.set_hexpand(true);
                let label1 = gtk::Label::new(Some("Capture video snapshots"));
                label1.set_xalign(0.0);
                label1.set_hexpand(true);
                let capture_switch = gtk::Switch::builder().active(initial_capture).build();
                row1.append(&label1);
                row1.append(&capture_switch);
                content.append(&row1);

                let row2 = gtk::Box::new(gtk::Orientation::Horizontal, 12);
                row2.set_hexpand(true);
                let label2 = gtk::Label::new(Some("Snapshots per video"));
                label2.set_xalign(0.0);
                label2.set_hexpand(true);
                let adjustment = gtk::Adjustment::new(
                    initial_count.clamp(1, 10) as f64,
                    1.0,
                    10.0,
                    1.0,
                    1.0,
                    0.0,
                );
                let snapshots_spin = gtk::SpinButton::new(Some(&adjustment), 1.0, 0);
                snapshots_spin.set_sensitive(initial_capture);
                row2.append(&label2);
                row2.append(&snapshots_spin);
                content.append(&row2);

                let row3 = gtk::Box::new(gtk::Orientation::Horizontal, 12);
                row3.set_hexpand(true);
                let label3 = gtk::Label::new(Some("Snapshot max size"));
                label3.set_xalign(0.0);
                label3.set_hexpand(true);
                let sizes = [128_u32, 256, 512, 768, 1024, 1536, 2048];
                let size_labels = [
                    "128 x 128",
                    "256 x 256",
                    "512 x 512",
                    "768 x 768",
                    "1024 x 1024",
                    "1536 x 1536",
                    "2048 x 2048",
                ];
                let string_list = gtk::StringList::new(&size_labels);
                let size_dropdown =
                    gtk::DropDown::new(Some(string_list.clone()), None::<&gtk::Expression>);
                let selected = sizes
                    .iter()
                    .position(|v| *v == initial_max_dim)
                    .unwrap_or(4);
                size_dropdown.set_selected(selected as u32);
                size_dropdown.set_sensitive(initial_capture);
                row3.append(&label3);
                row3.append(&size_dropdown);
                content.append(&row3);

                capture_switch.connect_notify_local(
                    Some("active"),
                    glib::clone!(
                        #[strong]
                        ui_state,
                        #[weak]
                        snapshots_spin,
                        #[weak]
                        size_dropdown,
                        move |sw, _| {
                            let active = sw.is_active();
                            if let Some(state) = ui_state.borrow_mut().as_mut() {
                                state.capture_snapshots = active;
                            }
                            snapshots_spin.set_sensitive(active);
                            size_dropdown.set_sensitive(active);
                        }
                    ),
                );

                snapshots_spin.connect_value_changed(glib::clone!(
                    #[strong]
                    ui_state,
                    move |spin| {
                        let value = spin.value().round().clamp(1.0, 10.0) as u32;
                        if let Some(state) = ui_state.borrow_mut().as_mut() {
                            state.snapshots_per_video = value;
                        }
                    }
                ));

                size_dropdown.connect_selected_notify(glib::clone!(
                    #[strong]
                    ui_state,
                    move |combo| {
                        let idx = combo.selected() as usize;
                        let sizes = [128_u32, 256, 512, 768, 1024, 1536, 2048];
                        if let Some(state) = ui_state.borrow_mut().as_mut() {
                            state.snapshot_max_dim = sizes[idx];
                        }
                    }
                ));

                let close_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
                close_row.set_halign(gtk::Align::End);
                let close_button = gtk::Button::with_label("Close");
                close_button.connect_clicked(glib::clone!(
                    #[weak]
                    settings_window,
                    move |_| settings_window.close()
                ));
                close_row.append(&close_button);
                content.append(&close_row);

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
                    .logo_icon_name("dupdupninja")
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

    let ui_state_for_activate = ui_state.clone();
    app.connect_activate(move |app| {
        if let Some(display) = gtk::gdk::Display::default() {
            let theme = gtk::IconTheme::for_display(&display);
            theme.add_search_path("icons/linux");
        }

        let window = adw::ApplicationWindow::new(app);
        window.set_title(Some("dupdupninja"));
        window.set_default_size(1100, 720);
        window.set_icon_name(Some("dupdupninja"));

        let header = adw::HeaderBar::new();

        let menu_button = gtk::MenuButton::new();
        menu_button.set_icon_name("open-menu-symbolic");
        let popover = gtk::PopoverMenu::from_model(Some(&app_menu));
        menu_button.set_popover(Some(&popover));
        header.pack_end(&menu_button);

        let content = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&content));

        let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 8);
        sidebar.set_margin_top(12);
        sidebar.set_margin_bottom(12);
        sidebar.set_margin_start(12);
        sidebar.set_margin_end(6);
        sidebar.set_size_request(260, -1);
        sidebar.set_vexpand(true);

        let fileset_header = gtk::Label::new(Some("Filesets"));
        fileset_header.set_xalign(0.0);
        fileset_header.add_css_class("title-4");
        sidebar.append(&fileset_header);

        let fileset_menu = gio::Menu::new();
        let open_section = gio::Menu::new();
        open_section.append(Some("Open Existing…"), Some("app.open_fileset"));
        fileset_menu.append_section(None, &open_section);
        let scan_section = gio::Menu::new();
        scan_section.append(Some("Scan Folder…"), Some("app.scan_folder"));
        scan_section.append(Some("Scan Disk…"), Some("app.scan_disk"));
        fileset_menu.append_section(None, &scan_section);
        let fileset_button = gtk::MenuButton::new();
        fileset_button.set_label("Fileset");
        fileset_button.set_icon_name("list-add-symbolic");
        fileset_button.set_always_show_arrow(true);
        fileset_button.set_tooltip_text(Some("Add or open a fileset"));
        let fileset_popover = gtk::PopoverMenu::from_model(Some(&fileset_menu));
        fileset_button.set_popover(Some(&fileset_popover));
        sidebar.append(&fileset_button);

        let fileset_list = gtk::ListBox::new();
        fileset_list.set_selection_mode(gtk::SelectionMode::Single);
        fileset_list.add_css_class("boxed-list");
        let fileset_scroller = gtk::ScrolledWindow::new();
        fileset_scroller.set_vexpand(true);
        fileset_scroller.set_child(Some(&fileset_list));
        sidebar.append(&fileset_scroller);

        let main_area = gtk::Box::new(gtk::Orientation::Vertical, 0);
        main_area.set_hexpand(true);
        main_area.set_vexpand(true);
        let placeholder = gtk::Label::new(Some("Select a fileset to view results."));
        placeholder.set_margin_top(18);
        placeholder.set_margin_start(18);
        placeholder.set_xalign(0.0);
        main_area.append(&placeholder);

        content.append(&sidebar);
        content.append(&main_area);

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
        progress.set_visible(false);
        let cancel_button = gtk::Button::with_label("Cancel");
        cancel_button.set_sensitive(false);
        cancel_button.set_visible(false);
        status_bar.append(&status_label);
        status_bar.append(&progress);
        status_bar.append(&cancel_button);

        toolbar.add_bottom_bar(&status_bar);
        window.set_content(Some(&toolbar));

        let (update_tx, update_rx) = std::sync::mpsc::channel::<UiUpdate>();
        *ui_state_for_activate.borrow_mut() = Some(UiState {
            status_label: status_label.clone(),
            progress: progress.clone(),
            cancel_button: cancel_button.clone(),
            cancel_token: None,
            update_tx: update_tx.clone(),
            total_files: 0,
            total_bytes: 0,
            fileset_list: fileset_list.clone(),
            filesets: Vec::new(),
            next_fileset_id: 1,
            active_fileset_id: None,
            fileset_placeholder: placeholder.clone(),
            active_scan_fileset_id: None,
            scan_actions_enabled: true,
            capture_snapshots: true,
            snapshots_per_video: 3,
            snapshot_max_dim: 1024,
        });

        restore_open_filesets(ui_state_for_activate.clone());

        let ui_state_for_filesets = ui_state_for_activate.clone();
        fileset_list.connect_row_selected(move |list, row| {
            let Ok(mut state_ref) = ui_state_for_filesets.try_borrow_mut() else {
                return;
            };
            if let Some(state) = state_ref.as_mut() {
                let active_id = row.and_then(|row| unsafe {
                    row.data::<u64>("fileset-id").map(|id| *id.as_ref())
                });
                state.active_fileset_id = active_id;
                update_fileset_placeholder(state);
                if active_id.is_none() && list.row_at_index(0).is_some() {
                    list.select_row(list.row_at_index(0).as_ref());
                }
            }
        });

        let ui_state_for_cancel = ui_state_for_activate.clone();
        cancel_button.connect_clicked(move |_| {
            if let Some(state) = ui_state_for_cancel.borrow().as_ref() {
                if let Some(token) = state.cancel_token.as_ref() {
                    token.cancel();
                }
            }
        });

        let ui_state_for_updates = ui_state_for_activate.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            while let Ok(update) = update_rx.try_recv() {
                if let Some(state) = ui_state_for_updates.borrow_mut().as_mut() {
                    match update {
                        UiUpdate::PrescanProgress { text } => {
                            state.status_label.set_text(&text);
                            state.progress.set_text(Some("Preparing..."));
                            state.progress.pulse();
                            state.progress.set_visible(true);
                            state.cancel_button.set_visible(true);
                            set_scan_actions_enabled(state, false);
                        }
                        UiUpdate::PrescanDone { totals } => {
                            state.total_files = totals.files;
                            state.total_bytes = totals.bytes;
                            state.status_label.set_text("Status: Scanning...");
                            state.progress.set_fraction(0.0);
                            state.progress.set_text(Some("0%"));
                            state.progress.set_visible(true);
                            state.cancel_button.set_visible(true);
                            set_scan_actions_enabled(state, false);
                        }
                        UiUpdate::Progress { text, fraction } => {
                            state.status_label.set_text(&text);
                            state.progress.set_visible(true);
                            state.cancel_button.set_visible(true);
                            set_scan_actions_enabled(state, false);
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
                            state.progress.set_visible(false);
                            state.cancel_button.set_visible(false);
                            state.cancel_token = None;
                            state.total_files = 0;
                            state.total_bytes = 0;
                            set_scan_actions_enabled(state, true);
                            if let Some(fileset_id) = state.active_scan_fileset_id.take() {
                                set_fileset_scanning(state, fileset_id, false);
                                set_fileset_status(state, fileset_id, "completed");
                            }
                        }
                        UiUpdate::Cancelled {
                            text,
                            fileset_id,
                        } => {
                            state.status_label.set_text(&text);
                            state.progress.set_text(Some("Idle"));
                            state.progress.set_fraction(0.0);
                            state.cancel_button.set_sensitive(false);
                            state.progress.set_visible(false);
                            state.cancel_button.set_visible(false);
                            state.cancel_token = None;
                            state.total_files = 0;
                            state.total_bytes = 0;
                            state.active_scan_fileset_id = None;
                            set_fileset_scanning(state, fileset_id, false);
                            set_fileset_status(state, fileset_id, "incomplete");
                            set_scan_actions_enabled(state, true);
                        }
                        UiUpdate::Error { text } => {
                            state.status_label.set_text(&text);
                            state.progress.set_text(Some("Idle"));
                            state.progress.set_fraction(0.0);
                            state.cancel_button.set_sensitive(false);
                            state.progress.set_visible(false);
                            state.cancel_button.set_visible(false);
                            state.cancel_token = None;
                            state.total_files = 0;
                            state.total_bytes = 0;
                            set_scan_actions_enabled(state, true);
                            if let Some(fileset_id) = state.active_scan_fileset_id.take() {
                                set_fileset_scanning(state, fileset_id, false);
                            }
                        }
                    }
                }
            }
            glib::ControlFlow::Continue
        });

        window.present();
        window.maximize();
    });

    let ui_state_for_shutdown = ui_state.clone();
    app.connect_shutdown(move |_| {
        persist_open_filesets(ui_state_for_shutdown.clone());
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
    fileset_list: gtk4::ListBox,
    filesets: Vec<FilesetEntry>,
    next_fileset_id: u64,
    active_fileset_id: Option<u64>,
    fileset_placeholder: gtk4::Label,
    active_scan_fileset_id: Option<u64>,
    scan_actions_enabled: bool,
    capture_snapshots: bool,
    snapshots_per_video: u32,
    snapshot_max_dim: u32,
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
enum UiUpdate {
    PrescanProgress { text: String },
    PrescanDone { totals: dupdupninja_core::scan::ScanTotals },
    Progress { text: String, fraction: Option<f64> },
    Done { text: String },
    Cancelled { text: String, fileset_id: u64 },
    Error { text: String },
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn start_scan(
    ui_state: std::rc::Rc<std::cell::RefCell<Option<UiState>>>,
    root: std::path::PathBuf,
    root_kind: dupdupninja_core::ScanRootKind,
    db_path: std::path::PathBuf,
    fileset_id: u64,
) {
    use gtk4::prelude::WidgetExt;
    let (
        status_label,
        progress,
        cancel_button,
        update_tx,
        capture_snapshots,
        snapshots_per_video,
        snapshot_max_dim,
    ) = {
        let state = ui_state.borrow();
        let Some(state) = state.as_ref() else {
            return;
        };
        (
            state.status_label.clone(),
            state.progress.clone(),
            state.cancel_button.clone(),
            state.update_tx.clone(),
            state.capture_snapshots,
            state.snapshots_per_video,
            state.snapshot_max_dim,
        )
    };

    let cancel_token = dupdupninja_core::scan::ScanCancelToken::new();
    {
        let mut state = ui_state.borrow_mut();
        if let Some(state) = state.as_mut() {
            state.cancel_token = Some(cancel_token.clone());
            state.active_scan_fileset_id = Some(fileset_id);
            set_fileset_scanning(state, fileset_id, true);
        }
    }

    status_label.set_text("Status: Scanning...");
    progress.set_text(Some("Scanning..."));
    progress.pulse();
    progress.set_visible(true);
    cancel_button.set_sensitive(true);
    cancel_button.set_visible(true);

    std::thread::spawn(move || {
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
            capture_snapshots,
            snapshots_per_video,
            snapshot_max_dim,
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
                let _ = update_tx.send(UiUpdate::Cancelled {
                    text: "Status: Scan cancelled".to_string(),
                    fileset_id,
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
            Err(dupdupninja_core::Error::Cancelled) => UiUpdate::Cancelled {
                text: "Status: Scan cancelled".to_string(),
                fileset_id,
            },
            Err(err) => UiUpdate::Error {
                text: format!("Status: Scan error: {err}"),
            },
        };
        let _ = update_tx.send(update);
    });
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn scan_db_path(root: &std::path::Path) -> std::path::PathBuf {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let name = sanitize_fileset_name(root);
    let file_name = format!("{name}-{ts}.ddn");
    let mut base = default_fileset_dir();
    if std::fs::create_dir_all(&base).is_err() {
        let mut fallback = std::env::temp_dir();
        fallback.push(file_name);
        return fallback;
    }
    base.push(file_name);
    base
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn default_fileset_dir() -> std::path::PathBuf {
    let mut base = gtk4::glib::user_data_dir();
    base.push("dupdupninja");
    base.push("filesets");
    base
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn default_config_dir() -> std::path::PathBuf {
    let mut base = gtk4::glib::user_config_dir();
    base.push("dupdupninja");
    base
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn open_filesets_path() -> std::path::PathBuf {
    let mut path = default_config_dir();
    path.push("open-filesets.txt");
    path
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn sanitize_fileset_name(root: &std::path::Path) -> String {
    let raw = fileset_name_from_path(root);
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch.to_ascii_lowercase());
        } else if ch.is_whitespace() || ch == '.' {
            out.push('-');
        }
    }
    if out.is_empty() {
        "fileset".to_string()
    } else {
        out
    }
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
struct FilesetEntry {
    id: u64,
    db_path: std::path::PathBuf,
    normalized_path: std::path::PathBuf,
    action_row: adw::ActionRow,
    row: gtk4::ListBoxRow,
    metadata: dupdupninja_core::FilesetMetadata,
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn add_fileset(
    ui_state: std::rc::Rc<std::cell::RefCell<Option<UiState>>>,
    name: String,
    db_path: std::path::PathBuf,
) -> u64 {
    use adw::prelude::*;
    use gtk4::gio;
    use gtk4::glib;
    use gtk4::prelude::WidgetExt;
    let normalized_path = normalize_fileset_path(&db_path);
    let (list, id, close_handler_state, existing_row) = {
        let mut state = ui_state.borrow_mut();
        let Some(state) = state.as_mut() else {
            return 0;
        };
        if let Some(existing) = state
            .filesets
            .iter()
            .find(|entry| entry.normalized_path == normalized_path)
        {
            let id = existing.id;
            let row = existing.row.clone();
            state.active_fileset_id = Some(id);
            update_fileset_placeholder(state);
            (
                state.fileset_list.clone(),
                id,
                ui_state.clone(),
                Some(row),
            )
        } else {
            let id = state.next_fileset_id;
            state.next_fileset_id += 1;
            (
                state.fileset_list.clone(),
                id,
                ui_state.clone(),
                None,
            )
        }
    };

    if let Some(row) = existing_row {
        list.select_row(Some(&row));
        return id;
    }

    let metadata = load_fileset_metadata(&db_path, &name);
    let row = gtk4::ListBoxRow::new();
    let action_row = adw::ActionRow::new();
    apply_fileset_metadata(&action_row, &metadata);
    let menu_button = gtk4::MenuButton::new();
    menu_button.set_icon_name("open-menu-symbolic");
    menu_button.set_tooltip_text(Some("Fileset actions"));
    menu_button.add_css_class("flat");

    let menu_model = gio::Menu::new();
    menu_model.append(Some("Close"), Some("fileset.close"));
    menu_model.append_section(None, &gio::Menu::new());
    menu_model.append(Some("Properties"), Some("fileset.properties"));
    menu_button.set_menu_model(Some(&menu_model));
    action_row.add_suffix(&menu_button);
    row.set_child(Some(&action_row));
    row.set_activatable(true);
    row.set_selectable(true);
    unsafe {
        row.set_data("fileset-id", id);
    }

    let action_group = gio::SimpleActionGroup::new();
    let close_action = gio::SimpleAction::new("close", None);
    close_action.connect_activate(glib::clone!(
        #[weak]
        row,
        #[strong]
        close_handler_state,
        move |_, _| {
            let id = unsafe { row.data::<u64>("fileset-id").map(|id| *id.as_ref()) };
            if let Some(id) = id {
                remove_fileset_by_id(close_handler_state.clone(), id);
            }
        }
    ));
    let properties_action = gio::SimpleAction::new("properties", None);
    properties_action.connect_activate(glib::clone!(
        #[weak]
        list,
        #[strong]
        close_handler_state,
        move |_, _| {
            if let Some(root) = list.root() {
                if let Ok(window) = root.downcast::<gtk4::Window>() {
                    open_fileset_properties(close_handler_state.clone(), id, &window);
                }
            }
        }
    ));
    action_group.add_action(&close_action);
    action_group.add_action(&properties_action);
    row.insert_action_group("fileset", Some(&action_group));

    let context_menu_button = menu_button.clone();
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(gtk4::gdk::BUTTON_SECONDARY);
    gesture.connect_pressed(move |_, _, _, _| {
        context_menu_button.popup();
    });
    row.add_controller(gesture);

    list.append(&row);
    list.select_row(Some(&row));

    if let Some(state) = ui_state.borrow_mut().as_mut() {
        state.filesets.push(FilesetEntry {
            id,
            db_path,
            normalized_path,
            action_row: action_row.clone(),
            row: row.clone(),
            metadata,
        });
        state.active_fileset_id = Some(id);
        update_fileset_placeholder(state);
    }

    id
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn fileset_name_from_path(path: &std::path::Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(|name| name.to_string())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn fileset_name_from_db(path: &std::path::Path) -> String {
    path.file_stem()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(|name| name.to_string())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn normalize_fileset_path(path: &std::path::Path) -> std::path::PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn load_fileset_metadata(
    db_path: &std::path::Path,
    default_name: &str,
) -> dupdupninja_core::FilesetMetadata {
    let fallback = dupdupninja_core::FilesetMetadata {
        created_at: std::time::SystemTime::now(),
        root_kind: dupdupninja_core::ScanRootKind::Folder,
        root_path: std::path::PathBuf::new(),
        root_parent_path: None,
        drive: dupdupninja_core::DriveMetadata {
            id: None,
            label: None,
            fs_type: None,
        },
        host_os: String::new(),
        host_os_version: String::new(),
        app_version: "1.0.0".to_string(),
        status: String::new(),
        name: default_name.to_string(),
        description: String::new(),
        notes: String::new(),
    };
    let store = match dupdupninja_core::db::SqliteScanStore::open(db_path) {
        Ok(store) => store,
        Err(_) => return fallback,
    };
    match store.get_fileset_metadata() {
        Ok(Some(mut meta)) => {
            if meta.name.trim().is_empty() {
                meta.name = default_name.to_string();
            }
            meta
        }
        _ => fallback,
    }
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn apply_fileset_metadata(row: &adw::ActionRow, meta: &dupdupninja_core::FilesetMetadata) {
    use adw::prelude::*;
    let name = if meta.name.trim().is_empty() {
        "Fileset"
    } else {
        meta.name.trim()
    };
    row.set_title(name);
    let status = meta.status.trim();
    if status.eq_ignore_ascii_case("incomplete") {
        row.set_subtitle("Status: Incomplete");
    } else {
        let subtitle = meta.description.trim();
        row.set_subtitle(subtitle);
    }
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn set_fileset_scanning(state: &mut UiState, fileset_id: u64, scanning: bool) {
    use adw::prelude::*;
    if let Some(entry) = state.filesets.iter().find(|entry| entry.id == fileset_id) {
        if scanning {
            entry.action_row.set_subtitle("Scanning...");
        } else {
            apply_fileset_metadata(&entry.action_row, &entry.metadata);
        }
    }
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn set_fileset_status(state: &mut UiState, fileset_id: u64, status: &str) {
    if let Some(entry) = state.filesets.iter_mut().find(|entry| entry.id == fileset_id) {
        entry.metadata.status = status.to_string();
        if let Ok(store) = dupdupninja_core::db::SqliteScanStore::open(&entry.db_path) {
            let _ = store.set_fileset_metadata(&entry.metadata);
        }
        apply_fileset_metadata(&entry.action_row, &entry.metadata);
        if state.active_fileset_id == Some(fileset_id) {
            update_fileset_placeholder(state);
        }
    }
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn restore_open_filesets(ui_state: std::rc::Rc<std::cell::RefCell<Option<UiState>>>) {
    let path = open_filesets_path();
    let contents = match std::fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(_) => return,
    };
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let db_path = std::path::PathBuf::from(trimmed);
        if db_path.is_file() {
            let name = fileset_name_from_db(&db_path);
            add_fileset(ui_state.clone(), name, db_path);
        }
    }
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn persist_open_filesets(ui_state: std::rc::Rc<std::cell::RefCell<Option<UiState>>>) {
    let mut entries = Vec::new();
    if let Some(state) = ui_state.borrow().as_ref() {
        for entry in &state.filesets {
            entries.push(entry.db_path.display().to_string());
        }
    }
    let path = open_filesets_path();
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    let content = entries.join("\n");
    let _ = std::fs::write(path, content);
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn set_scan_actions_enabled(state: &mut UiState, enabled: bool) {
    use gtk4::prelude::*;
    if state.scan_actions_enabled == enabled {
        return;
    }
    state.scan_actions_enabled = enabled;
    if let Some(app) = gtk4::gio::Application::default() {
        if let Some(action) = app.lookup_action("scan_folder") {
            if let Ok(simple) = action.downcast::<gtk4::gio::SimpleAction>() {
                simple.set_enabled(enabled);
            }
        }
        if let Some(action) = app.lookup_action("scan_disk") {
            if let Ok(simple) = action.downcast::<gtk4::gio::SimpleAction>() {
                simple.set_enabled(enabled);
            }
        }
    }
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn remove_fileset_by_id(
    ui_state: std::rc::Rc<std::cell::RefCell<Option<UiState>>>,
    fileset_id: u64,
) {
    let (list, row) = {
        let mut state = ui_state.borrow_mut();
        let Some(state) = state.as_mut() else {
            return;
        };
        let pos = match state.filesets.iter().position(|entry| entry.id == fileset_id) {
            Some(pos) => pos,
            None => return,
        };
        let entry = state.filesets.remove(pos);
        if state.active_fileset_id == Some(fileset_id) {
            state.active_fileset_id = None;
        }
        (state.fileset_list.clone(), entry.row.clone())
    };

    list.remove(&row);

    if let Some(state) = ui_state.borrow_mut().as_mut() {
        if state.fileset_list.selected_row().is_none() {
            if let Some(first) = state.fileset_list.row_at_index(0) {
                state.fileset_list.select_row(Some(&first));
            } else {
                update_fileset_placeholder(state);
            }
        }
    }
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn update_fileset_placeholder(state: &mut UiState) {
    if let Some(active_id) = state.active_fileset_id {
        if let Some(entry) = state.filesets.iter().find(|entry| entry.id == active_id) {
            let name = if entry.metadata.name.trim().is_empty() {
                "Fileset"
            } else {
                entry.metadata.name.trim()
            };
            state
                .fileset_placeholder
                .set_text(&format!("Active fileset: {}", name));
            return;
        }
    }
    state
        .fileset_placeholder
        .set_text("Select a fileset to view results.");
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn open_fileset_properties(
    ui_state: std::rc::Rc<std::cell::RefCell<Option<UiState>>>,
    fileset_id: u64,
    window: &gtk4::Window,
) {
    use gtk4::prelude::*;
    use adw::prelude::*;

    let (db_path, current_meta, total_files) = {
        let state = ui_state.borrow();
        let Some(state) = state.as_ref() else {
            return;
        };
        let entry = match state.filesets.iter().find(|entry| entry.id == fileset_id) {
            Some(entry) => entry,
            None => return,
        };
        let total_files = dupdupninja_core::db::SqliteScanStore::open(&entry.db_path)
            .ok()
            .and_then(|store| store.count_files().ok())
            .unwrap_or(0);
        (entry.db_path.clone(), entry.metadata.clone(), total_files)
    };

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);

    let title = gtk4::Label::new(Some("Fileset Properties"));
    title.add_css_class("title-3");
    title.set_xalign(0.0);
    content.append(&title);

    let name_label = gtk4::Label::new(Some("Name"));
    name_label.set_xalign(0.0);
    name_label.add_css_class("dim-label");
    content.append(&name_label);
    let name_entry = gtk4::Entry::new();
    name_entry.set_text(&current_meta.name);
    content.append(&name_entry);

    let description_label = gtk4::Label::new(Some("Description"));
    description_label.set_xalign(0.0);
    description_label.add_css_class("dim-label");
    content.append(&description_label);
    let description_entry = gtk4::Entry::new();
    description_entry.set_text(&current_meta.description);
    content.append(&description_entry);

    let notes_label = gtk4::Label::new(Some("Notes"));
    notes_label.set_xalign(0.0);
    notes_label.add_css_class("dim-label");
    content.append(&notes_label);
    let notes_view = gtk4::TextView::new();
    notes_view.set_wrap_mode(gtk4::WrapMode::WordChar);
    let buffer = notes_view.buffer();
    buffer.set_text(&current_meta.notes);
    let notes_scroller = gtk4::ScrolledWindow::new();
    notes_scroller.set_min_content_height(120);
    notes_scroller.set_vexpand(true);
    notes_scroller.set_child(Some(&notes_view));
    content.append(&notes_scroller);

    let total_label = gtk4::Label::new(Some("Total files"));
    total_label.set_xalign(0.0);
    total_label.add_css_class("dim-label");
    content.append(&total_label);
    let total_value = gtk4::Label::new(Some(&total_files.to_string()));
    total_value.set_xalign(0.0);
    content.append(&total_value);

    let button_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    button_row.set_halign(gtk4::Align::End);
    let cancel_button = gtk4::Button::with_label("Cancel");
    let save_button = gtk4::Button::with_label("Save");
    save_button.add_css_class("suggested-action");
    button_row.append(&cancel_button);
    button_row.append(&save_button);
    content.append(&button_row);

    let dialog = adw::Dialog::builder()
        .content_width(520)
        .content_height(360)
        .child(&content)
        .build();

    let ui_state_for_save = ui_state.clone();
    let dialog_for_save = dialog.clone();
    save_button.connect_clicked(move |_| {
        let mut name = name_entry.text().to_string();
        if name.trim().is_empty() {
            name = "Fileset".to_string();
        } else {
            name = name.trim().to_string();
        }
        let description = description_entry.text().trim().to_string();
        let buffer = notes_view.buffer();
        let notes = buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), true)
            .trim()
            .to_string();

        let meta = dupdupninja_core::FilesetMetadata {
            name,
            description,
            notes,
            ..current_meta.clone()
        };

        if let Ok(store) = dupdupninja_core::db::SqliteScanStore::open(&db_path) {
            let _ = store.set_fileset_metadata(&meta);
        }

        if let Some(state) = ui_state_for_save.borrow_mut().as_mut() {
            if let Some(entry) = state.filesets.iter_mut().find(|entry| entry.id == fileset_id) {
                entry.metadata = meta;
                apply_fileset_metadata(&entry.action_row, &entry.metadata);
                if state.active_fileset_id == Some(fileset_id) {
                    update_fileset_placeholder(state);
                }
            }
        }
        let _ = dialog_for_save.close();
    });

    let dialog_for_cancel = dialog.clone();
    cancel_button.connect_clicked(move |_| {
        let _ = dialog_for_cancel.close();
    });

    dialog.present(Some(window));
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
