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
                select_mount_path(&window, move |path| {
                    if let Some(path) = path {
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
            let label = format!("{}  ({})", mount.name(), path.display());
            entries.push(MountEntry { label, path });
        }
    }

    for entry in entries {
        let row = gtk::ListBoxRow::new();
        let text = gtk::Label::new(Some(&entry.label));
        text.set_xalign(0.0);
        text.set_margin_top(6);
        text.set_margin_bottom(6);
        text.set_margin_start(10);
        text.set_margin_end(10);
        row.set_child(Some(&text));
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
    label: String,
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

    let mut entries: BTreeMap<PathBuf, String> = BTreeMap::new();
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
        let label = format!("{source}  ({mount_point}) [{fs_type}]");
        entries.entry(path).or_insert(label);
    }

    entries
        .into_iter()
        .map(|(path, label)| MountEntry { path, label })
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
