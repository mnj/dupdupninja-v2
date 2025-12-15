#[cfg(all(target_os = "linux", feature = "gtk"))]
fn main() {
    use gtk4 as gtk;
    use gtk::gio;
    use gtk::glib;
    use gtk::prelude::*;

    const APP_ID: &str = "com.dupdup.app";

    let app = gtk::Application::new(Some(APP_ID), gio::ApplicationFlags::empty());

    let quit = gio::SimpleAction::new("quit", None);
    quit.connect_activate(glib::clone!(
        #[weak]
        app,
        move |_, _| app.quit()
    ));
    app.add_action(&quit);
    app.set_accels_for_action("app.quit", &["<primary>q"]);

    let menu = gio::Menu::new();
    menu.append(Some("Exit"), Some("app.quit"));
    app.set_menubar(Some(&menu));

    app.connect_activate(|app| {
        let window = gtk::ApplicationWindow::new(app);
        window.set_title(Some("dupdup"));
        window.set_default_size(1100, 720);
        window.present();
        window.maximize();
    });

    app.run();
}

#[cfg(not(all(target_os = "linux", feature = "gtk")))]
fn main() {
    println!("dupdup-ui-gtk stub. On Ubuntu: install GTK4 dev packages and build with `--features gtk`.");
}

