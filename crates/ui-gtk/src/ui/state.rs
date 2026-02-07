use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use adw::ActionRow;
use gtk4 as gtk;

use dupdupninja_core::scan::{ScanCancelToken, ScanTotals};
use dupdupninja_core::FilesetMetadata;

#[derive(Clone)]
pub(crate) struct FileActionButtons {
    pub(crate) trash: gtk::Button,
    pub(crate) delete: gtk::Button,
    pub(crate) copy: gtk::Button,
    pub(crate) move_to: gtk::Button,
    pub(crate) replace_symlink: gtk::Button,
    pub(crate) compare: gtk::Button,
}

pub(crate) struct FilesetEntry {
    pub(crate) id: u64,
    pub(crate) db_path: PathBuf,
    pub(crate) normalized_path: PathBuf,
    pub(crate) action_row: ActionRow,
    pub(crate) row: gtk::ListBoxRow,
    pub(crate) metadata: FilesetMetadata,
}

pub(crate) struct SelectedFile {
    pub(crate) rel_path: PathBuf,
    pub(crate) parent_rel_path: PathBuf,
}

pub(crate) struct UiState {
    pub(crate) status_label: gtk::Label,
    pub(crate) detail_status_label: gtk::Label,
    pub(crate) progress: gtk::ProgressBar,
    pub(crate) cancel_button: gtk::Button,
    pub(crate) cancel_token: Option<ScanCancelToken>,
    pub(crate) update_tx: std::sync::mpsc::Sender<UiUpdate>,
    pub(crate) total_files: u64,
    pub(crate) total_bytes: u64,
    pub(crate) fileset_list: gtk::ListBox,
    pub(crate) filesets: Vec<FilesetEntry>,
    pub(crate) next_fileset_id: u64,
    pub(crate) active_fileset_id: Option<u64>,
    pub(crate) fileset_placeholder: gtk::Label,
    pub(crate) files_stack: gtk::Stack,
    pub(crate) files_root_store: gtk::gio::ListStore,
    pub(crate) files_db_path: Rc<RefCell<Option<PathBuf>>>,
    pub(crate) active_scan_fileset_id: Option<u64>,
    pub(crate) scan_actions_enabled: bool,
    pub(crate) capture_snapshots: bool,
    pub(crate) snapshots_per_video: u32,
    pub(crate) snapshot_max_dim: u32,
    pub(crate) last_files_refresh: Option<Instant>,
    pub(crate) selected_files: HashMap<i64, SelectedFile>,
    pub(crate) action_bar_label: gtk::Label,
    pub(crate) action_bar_buttons: FileActionButtons,
    pub(crate) show_only_duplicates: bool,
}

pub(crate) enum UiUpdate {
    PrescanProgress {
        text: String,
    },
    PrescanDone {
        totals: ScanTotals,
    },
    Progress {
        text: String,
        detail: Option<String>,
        fraction: Option<f64>,
    },
    Done {
        text: String,
    },
    Cancelled {
        text: String,
        fileset_id: u64,
    },
    Error {
        text: String,
    },
}
