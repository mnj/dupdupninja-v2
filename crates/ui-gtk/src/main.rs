#[cfg(all(target_os = "linux", feature = "gtk"))]
fn main() {
    use adw::prelude::*;
    use gtk4 as gtk;
    use gtk::gio;
    use gtk::glib;
    const APP_ID: &str = "com.dupdupninja.app";

    if let Err(err) = adw::init() {
        eprintln!("libadwaita init failed: {err}");
    }

    let app = adw::Application::new(Some(APP_ID), gio::ApplicationFlags::empty());

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
        move |_, _| {
            if let Some(window) = app.active_window() {
                glib::MainContext::ref_thread_default().spawn_local(async move {
                    let dialog = gtk::FileDialog::new();
                    dialog.set_title("Select a folder to scan");
                    match dialog.select_folder_future(Some(&window)).await {
                        Ok(folder) => {
                            if let Some(path) = folder.path() {
                                println!("scan folder: {}", path.to_string_lossy());
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
        move |_, _| {
            if let Some(window) = app.active_window() {
                glib::MainContext::ref_thread_default().spawn_local(async move {
                    if let Some(path) = select_mount_path(&window).await {
                        println!("scan disk path: {}", path.to_string_lossy());
                        match dupdupninja_core::drive::probe_for_path(&path) {
                            Ok(meta) => {
                                println!("disk id: {:?}", meta.id);
                                println!("disk label: {:?}", meta.label);
                                println!("disk fs_type: {:?}", meta.fs_type);
                            }
                            Err(err) => {
                                eprintln!("disk metadata error: {err}");
                            }
                        }
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
        window.set_content(Some(&toolbar));

        window.present();
        window.maximize();
    });

    app.run();
}

#[cfg(all(target_os = "linux", feature = "gtk"))]
async fn select_mount_path(window: &gtk4::Window) -> Option<std::path::PathBuf> {
    use gtk4 as gtk;
    use gtk::gio;
    use gtk::prelude::*;

    let dialog = gtk::Dialog::builder()
        .title("Select a disk/mount to scan")
        .transient_for(window)
        .modal(true)
        .default_width(520)
        .default_height(360)
        .build();
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    dialog.add_button("Select", gtk::ResponseType::Accept);

    let content = dialog.content_area();
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_spacing(8);

    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.add_css_class("boxed-list");
    content.append(&list);

    let monitor = gio::VolumeMonitor::get();
    for mount in monitor.mounts() {
        let root = mount.root();
        let path = match root.path() {
            Some(path) => path,
            None => continue,
        };
        let label = format!("{}  ({})", mount.name(), path.display());
        let row = gtk::ListBoxRow::new();
        let text = gtk::Label::new(Some(&label));
        text.set_xalign(0.0);
        text.set_margin_top(6);
        text.set_margin_bottom(6);
        text.set_margin_start(10);
        text.set_margin_end(10);
        row.set_child(Some(&text));
        row.set_activatable(true);
        row.set_selectable(true);
        unsafe {
            row.set_data("mount-path", path);
        }
        list.append(&row);
    }

    let response = dialog.run_future().await;
    let selection = if response == gtk::ResponseType::Accept {
        list.selected_row().and_then(|row| unsafe {
            row.data::<std::path::PathBuf>("mount-path")
                .map(|p| p.as_ref().clone())
        })
    } else {
        None
    };

    dialog.close();
    selection
}

#[cfg(not(all(target_os = "linux", feature = "gtk")))]
fn main() {
    println!("dupdupninja-ui-gtk stub. On Ubuntu: install GTK4 dev packages and build with `--features gtk`.");
}
