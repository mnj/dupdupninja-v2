#[cfg(all(target_os = "linux", feature = "gtk"))]
fn main() {
    use gtk4 as gtk;
    use gtk::gio;
    use gtk::glib;
    use gtk::prelude::*;

    const APP_ID: &str = "com.dupdupninja.app";

    let app = gtk::Application::new(Some(APP_ID), gio::ApplicationFlags::empty());

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
                    let dialog = gtk::FileDialog::new();
                    dialog.set_title("Select a disk/mount to scan");
                    match dialog.select_folder_future(Some(&window)).await {
                        Ok(folder) => {
                            if let Some(path) = folder.path() {
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
                        }
                        Err(err) => {
                            eprintln!("disk selection error: {err}");
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

    let menubar = gio::Menu::new();
    let file_menu = gio::Menu::new();
    file_menu.append(Some("Settings…"), Some("app.settings"));
    file_menu.append(Some("Exit"), Some("app.quit"));
    menubar.append_submenu(Some("File"), &file_menu);

    let scan_menu = gio::Menu::new();
    scan_menu.append(Some("Folder…"), Some("app.scan_folder"));
    scan_menu.append(Some("Disk…"), Some("app.scan_disk"));
    menubar.append_submenu(Some("Scan"), &scan_menu);

    let help_menu = gio::Menu::new();
    help_menu.append(Some("About"), Some("app.about"));
    menubar.append_submenu(Some("Help"), &help_menu);

    app.set_menubar(Some(&menubar));

    let app_menu = gio::Menu::new();
    app_menu.append(Some("Settings…"), Some("app.settings"));
    app_menu.append(Some("About"), Some("app.about"));
    app_menu.append(Some("Exit"), Some("app.quit"));

    app.connect_activate(move |app| {
        let window = gtk::ApplicationWindow::new(app);
        window.set_title(Some("dupdupninja"));
        window.set_default_size(1100, 720);

        let header = gtk::HeaderBar::new();
        let menu_button = gtk::MenuButton::new();
        menu_button.set_icon_name("open-menu-symbolic");
        let popover = gtk::PopoverMenu::from_model(Some(&app_menu));
        menu_button.set_popover(Some(&popover));
        header.pack_end(&menu_button);
        window.set_titlebar(Some(&header));

        window.present();
        window.maximize();
    });

    app.run();
}

#[cfg(not(all(target_os = "linux", feature = "gtk")))]
fn main() {
    println!("dupdupninja-ui-gtk stub. On Ubuntu: install GTK4 dev packages and build with `--features gtk`.");
}
