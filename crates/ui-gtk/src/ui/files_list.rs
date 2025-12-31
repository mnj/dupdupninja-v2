use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;
use gtk::glib::prelude::Cast;
use gtk::prelude::GtkWindowExt;

use dupdupninja_core::models::{FileListRow, FileSnapshotRecord};
use dupdupninja_core::MediaFileRecord;
use image::ImageFormat;

use crate::ui::state::{FileActionButtons, SelectedFile, UiState};

pub(crate) struct FileActionBar {
    pub(crate) label: gtk::Label,
    pub(crate) buttons: FileActionButtons,
    pub(crate) container: gtk::Box,
}

#[derive(Clone)]
pub(crate) struct RowItem {
    pub(crate) kind: RowKind,
}

#[derive(Clone)]
pub(crate) enum RowKind {
    File(FileRow),
    MatchGroup { label: String, matches: Vec<FileRow> },
    MatchItem(FileRow),
}

#[derive(Clone)]
pub(crate) struct FileRow {
    pub(crate) id: i64,
    path: PathBuf,
    size_bytes: u64,
    blake3: Option<[u8; 32]>,
    sha256: Option<[u8; 32]>,
    file_type: Option<String>,
}

impl RowItem {
    pub(crate) fn from_file(row: FileListRow) -> Self {
        Self {
            kind: RowKind::File(FileRow::from(row)),
        }
    }

    pub(crate) fn match_group(label: String, matches: Vec<FileListRow>) -> Self {
        Self {
            kind: RowKind::MatchGroup {
                label,
                matches: matches.into_iter().map(FileRow::from).collect(),
            },
        }
    }

    pub(crate) fn match_item(row: FileRow) -> Self {
        Self {
            kind: RowKind::MatchItem(row),
        }
    }

    fn label(&self) -> String {
        match &self.kind {
            RowKind::File(file) | RowKind::MatchItem(file) => file
                .path
                .file_name()
                .and_then(|p| p.to_str())
                .unwrap_or("(unknown)")
                .to_string(),
            RowKind::MatchGroup { label, .. } => label.clone(),
        }
    }

    pub(crate) fn file_ref(&self) -> Option<&FileRow> {
        match &self.kind {
            RowKind::File(file) | RowKind::MatchItem(file) => Some(file),
            RowKind::MatchGroup { .. } => None,
        }
    }

    fn is_group(&self) -> bool {
        matches!(self.kind, RowKind::MatchGroup { .. })
    }

    fn is_match_item(&self) -> bool {
        matches!(self.kind, RowKind::MatchItem(_))
    }
}

impl From<FileListRow> for FileRow {
    fn from(row: FileListRow) -> Self {
        Self {
            id: row.id,
            path: row.path,
            size_bytes: row.size_bytes,
            blake3: row.blake3,
            sha256: row.sha256,
            file_type: row.file_type,
        }
    }
}

pub(crate) fn build_files_column_view(
    selection: &gtk::NoSelection,
    ui_state: Rc<RefCell<Option<UiState>>>,
) -> gtk::ColumnView {
    let column_view = gtk::ColumnView::new(Some(selection.clone()));
    column_view.set_hexpand(true);
    column_view.set_vexpand(true);

    let check_factory = gtk::SignalListItemFactory::new();
    let ui_state_for_check = ui_state.clone();
    check_factory.connect_setup(move |_, item| {
        let check = gtk::CheckButton::new();
        check.set_halign(gtk::Align::Start);
        let setting = Rc::new(Cell::new(false));
        unsafe {
            check.set_data("ddn-setting", setting.clone());
        }
        let ui_state_for_toggle = ui_state_for_check.clone();
        check.connect_toggled(move |cb| {
            let setting = unsafe {
                cb.data::<Rc<Cell<bool>>>("ddn-setting")
                    .map(|v| v.as_ref().clone())
            };
            if setting.as_ref().map(|s| s.get()).unwrap_or(false) {
                return;
            }
            let file_id = unsafe { cb.data::<i64>("ddn-file-id").map(|v| *v.as_ref()) };
            let rel_path = unsafe {
                cb.data::<PathBuf>("ddn-path").map(|p| p.as_ref().clone())
            };
            let parent_path = unsafe {
                cb.data::<PathBuf>("ddn-parent-path")
                    .map(|p| p.as_ref().clone())
            };
            let mut state = ui_state_for_toggle.borrow_mut();
            let Some(state) = state.as_mut() else {
                return;
            };
            if let (Some(file_id), Some(rel_path), Some(parent_path)) =
                (file_id, rel_path, parent_path)
            {
                if cb.is_active() {
                    state.selected_files.insert(
                        file_id,
                        SelectedFile {
                            rel_path,
                            parent_rel_path: parent_path,
                        },
                    );
                } else {
                    state.selected_files.remove(&file_id);
                }
                update_action_bar_state(state);
            }
        });
        item.downcast_ref::<gtk::ListItem>()
            .unwrap()
            .set_child(Some(&check));
    });
    let ui_state_for_bind = ui_state.clone();
    check_factory.connect_bind(move |_, item| {
        let list_item = item.downcast_ref::<gtk::ListItem>().unwrap();
        let check = list_item
            .child()
            .and_then(|c| c.downcast::<gtk::CheckButton>().ok())
            .unwrap();
        let tree_row = list_item
            .item()
            .and_then(|o| o.downcast::<gtk::TreeListRow>().ok());
        let row_item: Option<RowItem> = tree_row
            .as_ref()
            .and_then(|row| row.item())
            .and_then(|o| o.downcast::<gtk::glib::BoxedAnyObject>().ok())
            .and_then(|o| o.try_borrow::<RowItem>().ok().map(|r| r.clone()));

        if let Some(row_item) = row_item {
            if row_item.is_match_item() {
                if let Some(file) = row_item.file_ref() {
                    check.set_visible(true);
                    check.set_sensitive(true);
                    unsafe {
                        check.set_data("ddn-file-id", file.id);
                        check.set_data("ddn-path", file.path.clone());
                    }
                    if let Some(parent_path) =
                        tree_row.as_ref().and_then(find_parent_file_path)
                    {
                        unsafe {
                            check.set_data("ddn-parent-path", parent_path);
                        }
                    } else {
                        check.set_sensitive(false);
                    }
                    let selected = ui_state_for_bind
                        .try_borrow()
                        .ok()
                        .and_then(|s| {
                            s.as_ref()
                                .map(|s| s.selected_files.contains_key(&file.id))
                        })
                        .unwrap_or(false);
                    if let Some(setting) = unsafe {
                        check
                            .data::<Rc<Cell<bool>>>("ddn-setting")
                            .map(|v| v.as_ref().clone())
                    } {
                        setting.set(true);
                        check.set_active(selected);
                        setting.set(false);
                    } else {
                        check.set_active(selected);
                    }
                    return;
                }
            }
        }

        check.set_visible(false);
        check.set_sensitive(false);
        check.set_active(false);
    });
    let check_column = gtk::ColumnViewColumn::new(Some(""), Some(check_factory));
    check_column.set_fixed_width(36);
    check_column.set_resizable(false);
    column_view.append_column(&check_column);

    let name_factory = gtk::SignalListItemFactory::new();
    name_factory.connect_setup(|_, item| {
        let expander = gtk::TreeExpander::new();
        let label = gtk::Label::new(None);
        label.set_xalign(0.0);
        label.set_hexpand(true);
        expander.set_child(Some(&label));
        item.downcast_ref::<gtk::ListItem>()
            .unwrap()
            .set_child(Some(&expander));
    });
    name_factory.connect_bind(|_, item| {
        let list_item = item.downcast_ref::<gtk::ListItem>().unwrap();
        let tree_row = list_item
            .item()
            .and_then(|o| o.downcast::<gtk::TreeListRow>().ok());
        let Some(tree_row) = tree_row else {
            return;
        };
        let expander = list_item
            .child()
            .and_then(|c| c.downcast::<gtk::TreeExpander>().ok())
            .unwrap();
        expander.set_list_row(Some(&tree_row));

        let label = expander
            .child()
            .and_then(|c| c.downcast::<gtk::Label>().ok())
            .unwrap();
        let row_item: Option<RowItem> = tree_row
            .item()
            .and_then(|o| o.downcast::<gtk::glib::BoxedAnyObject>().ok())
            .and_then(|o| o.try_borrow::<RowItem>().ok().map(|r| r.clone()));
        if let Some(row_item) = row_item {
            label.set_text(&row_item.label());
            if row_item.is_group() {
                label.add_css_class("dim-label");
            } else {
                label.remove_css_class("dim-label");
            }
        } else {
            label.set_text("");
        }
    });

    let name_column = gtk::ColumnViewColumn::new(Some("Filename"), Some(name_factory));
    name_column.set_resizable(true);
    name_column.set_expand(true);
    column_view.append_column(&name_column);

    let size_column = make_text_column("Size", move |row| {
        row.file_ref()
            .map(|f| format_bytes(f.size_bytes))
            .unwrap_or_default()
    });
    let type_column = make_text_column("File Type", move |row| {
        row.file_ref()
            .and_then(|f| f.file_type.clone())
            .unwrap_or_default()
    });
    let blake3_column = make_text_column("Blake3", move |row| {
        row.file_ref()
            .and_then(|f| f.blake3.as_ref())
            .map(hash_to_hex)
            .unwrap_or_default()
    });
    let sha256_column = make_text_column("SHA-256", move |row| {
        row.file_ref()
            .and_then(|f| f.sha256.as_ref())
            .map(hash_to_hex)
            .unwrap_or_default()
    });

    column_view.append_column(&size_column);
    column_view.append_column(&type_column);
    column_view.append_column(&blake3_column);
    column_view.append_column(&sha256_column);

    attach_column_menu(
        &column_view,
        &[
            ("Filename", name_column),
            ("Size", size_column),
            ("File Type", type_column),
            ("Blake3", blake3_column),
            ("SHA-256", sha256_column),
        ],
    );

    column_view
}

fn find_parent_file_path(row: &gtk::TreeListRow) -> Option<PathBuf> {
    let mut current = row.parent();
    while let Some(parent) = current {
        let row_item: Option<RowItem> = parent
            .item()
            .and_then(|o| o.downcast::<gtk::glib::BoxedAnyObject>().ok())
            .and_then(|o| o.try_borrow::<RowItem>().ok().map(|r| r.clone()));
        if let Some(item) = row_item {
            if let Some(file) = item.file_ref() {
                return Some(file.path.clone());
            }
        }
        current = parent.parent();
    }
    None
}

fn make_text_column<F>(title: &str, value: F) -> gtk::ColumnViewColumn
where
    F: Fn(&RowItem) -> String + 'static,
{
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let label = gtk::Label::new(None);
        label.set_xalign(0.0);
        label.set_hexpand(true);
        item.downcast_ref::<gtk::ListItem>()
            .unwrap()
            .set_child(Some(&label));
    });
    factory.connect_bind(move |_, item| {
        let list_item = item.downcast_ref::<gtk::ListItem>().unwrap();
        let label = list_item
            .child()
            .and_then(|c| c.downcast::<gtk::Label>().ok())
            .unwrap();
        let tree_row = list_item
            .item()
            .and_then(|o| o.downcast::<gtk::TreeListRow>().ok());
        let row_item: Option<RowItem> = tree_row
            .and_then(|row| row.item())
            .and_then(|o| o.downcast::<gtk::glib::BoxedAnyObject>().ok())
            .and_then(|o| o.try_borrow::<RowItem>().ok().map(|r| r.clone()));

        if let Some(row_item) = row_item {
            label.set_text(&value(&row_item));
        } else {
            label.set_text("");
        }
    });

    let column = gtk::ColumnViewColumn::new(Some(title), Some(factory));
    column.set_resizable(true);
    column
}

fn attach_column_menu(column_view: &gtk::ColumnView, columns: &[(&str, gtk::ColumnViewColumn)]) {
    let popover = gtk::Popover::new();
    popover.set_autohide(true);
    popover.set_has_arrow(false);
    popover.set_parent(column_view);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    content.set_margin_top(8);
    content.set_margin_bottom(8);
    content.set_margin_start(8);
    content.set_margin_end(8);

    for (label, column) in columns {
        let checkbox = gtk::CheckButton::with_label(label);
        checkbox.set_active(column.is_visible());
        let column = column.clone();
        checkbox.connect_toggled(move |cb| {
            column.set_visible(cb.is_active());
        });
        content.append(&checkbox);
    }

    popover.set_child(Some(&content));

    let click = gtk::GestureClick::builder().button(3).build();
    let popover_for_click = popover.clone();
    click.connect_pressed(move |gesture, _, x, y| {
        let rect = gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1);
        popover_for_click.set_pointing_to(Some(&rect));
        popover_for_click.popup();
        gesture.set_state(gtk::EventSequenceState::Claimed);
    });
    column_view.add_controller(click);
}

pub(crate) fn build_file_action_bar(ui_state: Rc<RefCell<Option<UiState>>>) -> FileActionBar {
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    bar.set_margin_top(6);
    bar.set_margin_bottom(6);
    bar.set_margin_start(6);
    bar.set_margin_end(6);

    let label = gtk::Label::new(Some("0 selected"));
    label.set_xalign(0.0);
    label.set_hexpand(true);

    let show_duplicates = gtk::CheckButton::with_label("Show only duplicates");

    let trash = gtk::Button::with_label("Move to Trash");
    let delete = gtk::Button::with_label("Delete Permanently");
    let copy = gtk::Button::with_label("Copy to...");
    let move_to = gtk::Button::with_label("Move to...");
    let replace_symlink = gtk::Button::with_label("Replace with Symlink");
    let compare = gtk::Button::with_label("Compare Selected");

    bar.append(&label);
    bar.append(&show_duplicates);
    bar.append(&trash);
    bar.append(&delete);
    bar.append(&copy);
    bar.append(&move_to);
    bar.append(&replace_symlink);
    bar.append(&compare);

    let buttons = FileActionButtons {
        trash: trash.clone(),
        delete: delete.clone(),
        copy: copy.clone(),
        move_to: move_to.clone(),
        replace_symlink: replace_symlink.clone(),
        compare: compare.clone(),
    };

    let ui_state_for_actions = ui_state.clone();
    trash.connect_clicked(move |_| {
        apply_to_selected(&ui_state_for_actions, |path| {
            let file = gtk::gio::File::for_path(path);
            file.trash(None::<&gtk::gio::Cancellable>)
                .map(|_| "Moved to Trash".to_string())
                .map_err(|e| e.to_string())
        });
    });

    let ui_state_for_actions = ui_state.clone();
    delete.connect_clicked(move |_| {
        apply_to_selected(&ui_state_for_actions, |path| {
            std::fs::remove_file(path)
                .map(|_| "Deleted permanently".to_string())
                .map_err(|e| e.to_string())
        });
    });

    let ui_state_for_actions = ui_state.clone();
    copy.connect_clicked(move |_| {
        let ui_state_for_dialog = ui_state_for_actions.clone();
        if let Some(window) = active_window(&ui_state_for_dialog) {
            let dialog = gtk::FileDialog::new();
            dialog.set_title("Copy to folder");
            dialog.select_folder(Some(&window), None::<&gtk::gio::Cancellable>, move |res| {
                if let Ok(dest) = res {
                    if let Some(folder) = dest.path() {
                        apply_to_selected(&ui_state_for_dialog, |path| {
                            let file_name = path.file_name().unwrap_or_default().to_os_string();
                            let target = folder.join(file_name);
                            std::fs::copy(path, &target)
                                .map(|_| "Copied file".to_string())
                                .map_err(|e| e.to_string())
                        });
                    }
                }
            });
        }
    });

    let ui_state_for_actions = ui_state.clone();
    move_to.connect_clicked(move |_| {
        let ui_state_for_dialog = ui_state_for_actions.clone();
        if let Some(window) = active_window(&ui_state_for_dialog) {
            let dialog = gtk::FileDialog::new();
            dialog.set_title("Move to folder");
            dialog.select_folder(Some(&window), None::<&gtk::gio::Cancellable>, move |res| {
                if let Ok(dest) = res {
                    if let Some(folder) = dest.path() {
                        apply_to_selected(&ui_state_for_dialog, |path| {
                            let file_name = path.file_name().unwrap_or_default().to_os_string();
                            let target = folder.join(file_name);
                            std::fs::rename(path, &target)
                                .map(|_| "Moved file".to_string())
                                .map_err(|e| e.to_string())
                        });
                    }
                }
            });
        }
    });

    let ui_state_for_actions = ui_state.clone();
    replace_symlink.connect_clicked(move |_| {
        apply_to_selected_with_parent(&ui_state_for_actions, |path, parent_path| {
            if path == parent_path {
                return Ok("Skipped parent file".to_string());
            }
            std::fs::remove_file(path).map_err(|e| e.to_string())?;
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(parent_path, path)
                    .map(|_| "Replaced with symlink".to_string())
                    .map_err(|e| e.to_string())
            }
            #[cfg(not(unix))]
            {
                Err("Symlink replacement is only supported on Unix".to_string())
            }
        });
    });

    let ui_state_for_actions = ui_state.clone();
    compare.connect_clicked(move |_| {
        open_compare_window(&ui_state_for_actions);
    });

    let ui_state_for_toggle = ui_state.clone();
    show_duplicates.connect_toggled(move |cb| {
        let mut state = ui_state_for_toggle.borrow_mut();
        let Some(state) = state.as_mut() else {
            return;
        };
        state.show_only_duplicates = cb.is_active();
        if let Some(active_id) = state.active_fileset_id {
            if let Some(entry) = state.filesets.iter().find(|entry| entry.id == active_id) {
                let db_path = entry.db_path.clone();
                crate::ui::load_fileset_rows(state, &db_path);
            }
        }
    });

    FileActionBar {
        label,
        buttons,
        container: bar,
    }
}

pub(crate) fn update_action_bar_state(state: &mut UiState) {
    let count = state.selected_files.len();
    state
        .action_bar_label
        .set_text(&format!("{count} selected"));
    let enabled = count > 0;
    state.action_bar_buttons.trash.set_sensitive(enabled);
    state.action_bar_buttons.delete.set_sensitive(enabled);
    state.action_bar_buttons.copy.set_sensitive(enabled);
    state.action_bar_buttons.move_to.set_sensitive(enabled);
    state
        .action_bar_buttons
        .replace_symlink
        .set_sensitive(enabled);
    state.action_bar_buttons.compare.set_sensitive(enabled);
}

fn apply_to_selected<F>(ui_state: &Rc<RefCell<Option<UiState>>>, mut action: F)
where
    F: FnMut(&Path) -> std::result::Result<String, String>,
{
    let paths = {
        let state_ref = ui_state.borrow();
        let Some(state) = state_ref.as_ref() else {
            return;
        };
        let active_id = match state.active_fileset_id {
            Some(id) => id,
            None => return,
        };
        let entry = match state.filesets.iter().find(|entry| entry.id == active_id) {
            Some(entry) => entry,
            None => return,
        };
        let mut out = Vec::new();
        for selected in state.selected_files.values() {
            out.push(entry.metadata.root_path.join(&selected.rel_path));
        }
        out
    };

    let mut last_result: Option<std::result::Result<String, String>> = None;
    for path in paths {
        last_result = Some(action(&path));
    }

    if let Some(result) = last_result {
        update_status(ui_state, result);
    }

    let mut state = ui_state.borrow_mut();
    let Some(state) = state.as_mut() else {
        return;
    };
    state.selected_files.clear();
    update_action_bar_state(state);
    if let Some(active_id) = state.active_fileset_id {
        if let Some(entry) = state.filesets.iter().find(|entry| entry.id == active_id) {
            let db_path = entry.db_path.clone();
            crate::ui::load_fileset_rows(state, &db_path);
        }
    }
}

fn apply_to_selected_with_parent<F>(
    ui_state: &Rc<RefCell<Option<UiState>>>,
    mut action: F,
)
where
    F: FnMut(&Path, &Path) -> std::result::Result<String, String>,
{
    let paths = {
        let state_ref = ui_state.borrow();
        let Some(state) = state_ref.as_ref() else {
            return;
        };
        let active_id = match state.active_fileset_id {
            Some(id) => id,
            None => return,
        };
        let entry = match state.filesets.iter().find(|entry| entry.id == active_id) {
            Some(entry) => entry,
            None => return,
        };
        let mut out = Vec::new();
        for selected in state.selected_files.values() {
            out.push((
                entry.metadata.root_path.join(&selected.rel_path),
                entry.metadata.root_path.join(&selected.parent_rel_path),
            ));
        }
        out
    };

    let mut last_result: Option<std::result::Result<String, String>> = None;
    for (path, parent_path) in paths {
        last_result = Some(action(&path, &parent_path));
    }

    if let Some(result) = last_result {
        update_status(ui_state, result);
    }

    let mut state = ui_state.borrow_mut();
    let Some(state) = state.as_mut() else {
        return;
    };
    state.selected_files.clear();
    update_action_bar_state(state);
    if let Some(active_id) = state.active_fileset_id {
        if let Some(entry) = state.filesets.iter().find(|entry| entry.id == active_id) {
            let db_path = entry.db_path.clone();
            crate::ui::load_fileset_rows(state, &db_path);
        }
    }
}

fn active_window(ui_state: &Rc<RefCell<Option<UiState>>>) -> Option<gtk::Window> {
    let state = ui_state.borrow();
    let _ = state.as_ref()?;
    gtk::Window::list_toplevels()
        .into_iter()
        .find_map(|w| w.downcast::<gtk::Window>().ok())
}

fn update_status(
    ui_state: &Rc<RefCell<Option<UiState>>>,
    result: std::result::Result<String, String>,
) {
    let mut state = ui_state.borrow_mut();
    let Some(state) = state.as_mut() else {
        return;
    };
    match result {
        Ok(msg) => state.status_label.set_text(&format!("Status: {msg}")),
        Err(err) => state.status_label.set_text(&format!("Status: {err}")),
    }
}

fn open_compare_window(ui_state: &Rc<RefCell<Option<UiState>>>) {
    let (db_path, root_path, selections) = {
        let state_ref = ui_state.borrow();
        let Some(state) = state_ref.as_ref() else {
            return;
        };
        let active_id = match state.active_fileset_id {
            Some(id) => id,
            None => return,
        };
        let entry = match state.filesets.iter().find(|entry| entry.id == active_id) {
            Some(entry) => entry,
            None => return,
        };
        let mut grouped = std::collections::BTreeMap::<PathBuf, Vec<i64>>::new();
        for (id, selected) in &state.selected_files {
            grouped.entry(selected.parent_rel_path.clone()).or_default().push(*id);
        }
        (entry.db_path.clone(), entry.metadata.root_path.clone(), grouped)
    };

    if selections.is_empty() {
        update_status(ui_state, Err("No selected files to compare".to_string()));
        return;
    }

    let store = match dupdupninja_core::db::SqliteScanStore::open(&db_path) {
        Ok(store) => store,
        Err(err) => {
            update_status(
                ui_state,
                Err(format!("Failed to open fileset: {err}")),
            );
            return;
        }
    };

    let notebook = gtk::Notebook::new();
    for (parent_rel, matches) in selections {
        let parent = match store.get_file_by_path(&parent_rel).ok().flatten() {
            Some(parent) => parent,
            None => continue,
        };
        let parent_snapshots = load_snapshots(&store, parent.file_id);
        let mut match_records = Vec::new();
        for id in matches {
            if let Some(rec) = store.get_file_by_id(id).ok().flatten() {
                match_records.push(CompareFile {
                    snapshots: load_snapshots(&store, rec.file_id),
                    record: rec,
                });
            }
        }
        if match_records.is_empty() {
            continue;
        }

        let tab_title = display_name(&parent);
        let parent_file = CompareFile {
            record: parent,
            snapshots: parent_snapshots,
        };
        let max_snapshots = std::iter::once(&parent_file)
            .chain(match_records.iter())
            .map(|f| f.snapshots.len())
            .max()
            .unwrap_or(0);
        let tab = build_compare_tab(&root_path, &parent_file, &match_records, max_snapshots);
        let tab_label = gtk::Label::new(Some(&tab_title));
        notebook.append_page(&tab, Some(&tab_label));
    }

    if notebook.n_pages() == 0 {
        update_status(ui_state, Err("No metadata available to compare".to_string()));
        return;
    }

    let header = adw::HeaderBar::builder()
        .title_widget(&adw::WindowTitle::new("Compare selected files", ""))
        .show_end_title_buttons(true)
        .show_start_title_buttons(true)
        .build();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&notebook));

    let (default_width, default_height) = compare_window_default_size(ui_state);
    let window = adw::Window::builder()
        .title("Compare selected files")
        .default_width(default_width)
        .default_height(default_height)
        .content(&toolbar)
        .build();

    if let Some(parent_window) = active_window(ui_state) {
        let window_as_gtk = window.clone().upcast::<gtk::Window>();
        if !parent_window.eq(&window_as_gtk) {
            window.set_transient_for(Some(&parent_window));
        }
    }

    window.present();
}

fn compare_window_default_size(ui_state: &Rc<RefCell<Option<UiState>>>) -> (i32, i32) {
    let min_width = 480.0;
    let min_height = 360.0;
    let mut width = 900.0;
    let mut height = 600.0;

    if let Some(parent_window) = active_window(ui_state) {
        let (parent_w, parent_h) = parent_window.default_size();
        if parent_w > 0 && parent_h > 0 {
            width = (parent_w as f64 * 0.8).round();
            height = (parent_h as f64 * 0.8).round();
        } else if let Some(surface) = parent_window.surface() {
            if let Some(display) = gtk::gdk::Display::default() {
                if let Some(monitor) = display.monitor_at_surface(&surface) {
                    let geo = monitor.geometry();
                    width = (geo.width() as f64 * 0.7).round();
                    height = (geo.height() as f64 * 0.7).round();
                }
            }
        }
    }

    (
        width.max(min_width) as i32,
        height.max(min_height) as i32,
    )
}

struct CompareFile {
    record: MediaFileRecord,
    snapshots: Vec<FileSnapshotRecord>,
}

fn build_compare_tab(
    root_path: &Path,
    parent: &CompareFile,
    matches: &[CompareFile],
    max_snapshots: usize,
) -> gtk::Widget {
    let rows = metadata_rows(max_snapshots);
    let parent_title = format!("Parent: {}", display_name(&parent.record));
    let parent_column = build_metadata_column(
        &parent_title,
        &rows,
        parent,
        root_path,
        true,
    );

    let matches_box = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    for file in matches {
        let title = display_name(&file.record);
        let column = build_metadata_column(&title, &rows, file, root_path, false);
        matches_box.append(&column);
    }

    let matches_scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .child(&matches_box)
        .build();
    matches_scroller.set_hexpand(true);
    matches_scroller.set_vexpand(true);

    let parent_scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .child(&parent_column)
        .build();
    parent_scroller.set_hexpand(false);
    parent_scroller.set_vexpand(true);
    let adj = matches_scroller.vadjustment();
    parent_scroller.set_vadjustment(Some(&adj));

    let container = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    container.append(&parent_scroller);
    container.append(&matches_scroller);
    container.upcast()
}

fn build_metadata_column(
    title: &str,
    rows: &[CompareRow],
    file: &CompareFile,
    root_path: &Path,
    include_labels: bool,
) -> gtk::Widget {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 8);
    column.set_margin_top(12);
    column.set_margin_bottom(12);
    column.set_margin_start(12);
    column.set_margin_end(12);

    let header = gtk::Label::new(Some(title));
    header.set_xalign(0.0);
    header.add_css_class("title-4");
    column.append(&header);

    for row_def in rows {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let label_text = match row_def {
            CompareRow::Field(label, _) => label.clone(),
            CompareRow::Snapshot(index) => snapshot_label(*index),
        };
        let label_widget = gtk::Label::new(Some(&label_text));
        label_widget.set_xalign(0.0);
        label_widget.set_width_chars(16);
        label_widget.add_css_class("dim-label");
        if !include_labels {
            label_widget.set_visible(false);
        }
        row.append(&label_widget);
        match row_def {
            CompareRow::Field(_, field) => {
                let value = metadata_value(field, &file.record, root_path);
                let value_label = gtk::Label::new(Some(&value));
                value_label.set_xalign(0.0);
                value_label.set_wrap(true);
                value_label.set_selectable(true);
                row.append(&value_label);
            }
            CompareRow::Snapshot(index) => {
                row.append(&snapshot_widget(&file.snapshots, *index));
            }
        }
        column.append(&row);
    }

    column.upcast()
}

#[derive(Clone, Copy)]
enum MetadataField {
    Path,
    Size,
    Modified,
    FileType,
    Blake3,
    Sha256,
    Ffmpeg,
}

enum CompareRow {
    Field(String, MetadataField),
    Snapshot(usize),
}

fn metadata_rows(max_snapshots: usize) -> Vec<CompareRow> {
    let mut rows = vec![
        CompareRow::Field("Path".to_string(), MetadataField::Path),
        CompareRow::Field("Size".to_string(), MetadataField::Size),
        CompareRow::Field("Modified".to_string(), MetadataField::Modified),
        CompareRow::Field("File Type".to_string(), MetadataField::FileType),
        CompareRow::Field("Blake3".to_string(), MetadataField::Blake3),
        CompareRow::Field("SHA-256".to_string(), MetadataField::Sha256),
        CompareRow::Field("FFmpeg metadata".to_string(), MetadataField::Ffmpeg),
    ];
    for idx in 0..max_snapshots {
        rows.push(CompareRow::Snapshot(idx));
    }
    rows
}

fn snapshot_label(index: usize) -> String {
    format!("Snapshot {}", index + 1)
}

fn snapshot_widget(snapshots: &[FileSnapshotRecord], index: usize) -> gtk::Widget {
    if let Some(snapshot) = snapshots.get(index) {
        let bytes = gtk::glib::Bytes::from(&snapshot.image_avif);
        if let Ok(texture) = gtk::gdk::Texture::from_bytes(&bytes) {
            let picture = gtk::Picture::for_paintable(&texture);
            picture.set_can_shrink(true);
            picture.set_content_fit(gtk::ContentFit::Contain);
            picture.set_size_request(160, 90);
            return picture.upcast();
        }
        if let Some(texture) = decode_avif_texture(&snapshot.image_avif) {
            let picture = gtk::Picture::for_paintable(&texture);
            picture.set_can_shrink(true);
            picture.set_content_fit(gtk::ContentFit::Contain);
            picture.set_size_request(160, 90);
            return picture.upcast();
        }
        let label = gtk::Label::new(Some("Snapshot unavailable"));
        label.set_xalign(0.0);
        return label.upcast();
    }
    let label = gtk::Label::new(Some("-"));
    label.set_xalign(0.0);
    label.upcast()
}

fn metadata_value(field: &MetadataField, record: &MediaFileRecord, root_path: &Path) -> String {
    match field {
        MetadataField::Path => {
            let full = root_path.join(&record.path);
            full.to_string_lossy().to_string()
        }
        MetadataField::Size => format_bytes(record.size_bytes),
        MetadataField::Modified => record
            .modified_at
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| format!("{}s", d.as_secs()))
            .unwrap_or_else(|| "Unknown".to_string()),
        MetadataField::FileType => record.file_type.clone().unwrap_or_default(),
        MetadataField::Blake3 => record
            .blake3
            .as_ref()
            .map(hash_to_hex)
            .unwrap_or_default(),
        MetadataField::Sha256 => record
            .sha256
            .as_ref()
            .map(hash_to_hex)
            .unwrap_or_default(),
        MetadataField::Ffmpeg => record.ffmpeg_metadata.clone().unwrap_or_default(),
    }
}

fn display_name(record: &MediaFileRecord) -> String {
    record
        .path
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("(unknown)")
        .to_string()
}

fn load_snapshots(
    store: &dupdupninja_core::db::SqliteScanStore,
    file_id: Option<i64>,
) -> Vec<FileSnapshotRecord> {
    let Some(file_id) = file_id else {
        return Vec::new();
    };
    store.list_file_snapshots(file_id).unwrap_or_default()
}

fn decode_avif_texture(data: &[u8]) -> Option<gtk::gdk::Texture> {
    let img = image::load_from_memory_with_format(data, ImageFormat::Avif).ok()?;
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    let stride = (width as usize).saturating_mul(4);
    let bytes = gtk::glib::Bytes::from_owned(rgba.into_raw());
    let texture = gtk::gdk::MemoryTexture::new(
        width as i32,
        height as i32,
        gtk::gdk::MemoryFormat::R8g8b8a8,
        &bytes,
        stride,
    );
    Some(texture.upcast::<gtk::gdk::Texture>())
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.2} MB", b / MB)
    } else if b >= KB {
        format!("{:.2} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

fn hash_to_hex(hash: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for b in hash {
        use std::fmt::Write;
        let _ = write!(&mut out, "{:02x}", b);
    }
    out
}
