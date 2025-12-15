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

    let menubar = gio::Menu::new();
    let file_menu = gio::Menu::new();
    file_menu.append(Some("Exit"), Some("app.quit"));
    menubar.append_submenu(Some("File"), &file_menu);

    let scan_menu = gio::Menu::new();
    scan_menu.append(Some("Folder…"), Some("app.scan_folder"));
    scan_menu.append(Some("Disk…"), Some("app.scan_disk"));
    menubar.append_submenu(Some("Scan"), &scan_menu);

    app.set_menubar(Some(&menubar));

    app.connect_activate(|app| {
        let window = gtk::ApplicationWindow::new(app);
        window.set_title(Some("dupdupninja"));
        window.set_default_size(1100, 720);
        window.present();
        window.maximize();
    });

    app.run();
}

#[cfg(not(all(target_os = "linux", feature = "gtk")))]
fn main() {
    println!("dupdupninja-ui-gtk stub. On Ubuntu: install GTK4 dev packages and build with `--features gtk`.");
}
