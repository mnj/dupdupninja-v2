use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::SystemTime;

use walkdir::WalkDir;

use crate::db::SqliteScanStore;
use crate::error::{Error, Result};
use crate::drive;
use crate::hash::blake3_file;
use crate::models::{DriveMetadata, FilesetMetadata, MediaFileRecord, ScanResult, ScanRootKind, ScanStats};

#[derive(Debug, Clone)]
pub struct ScanConfig {
    pub root: PathBuf,
    pub root_kind: ScanRootKind,
    pub hash_files: bool,
}

impl ScanConfig {
    pub fn for_folder(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            root_kind: ScanRootKind::Folder,
            hash_files: true,
        }
    }
}

pub fn scan_to_sqlite(config: &ScanConfig, store: &SqliteScanStore) -> Result<ScanResult> {
    scan_to_sqlite_with_progress(config, store, None, |_| {})
}

#[derive(Clone, Debug)]
pub struct ScanCancelToken {
    cancelled: Arc<AtomicBool>,
}

impl ScanCancelToken {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone)]
pub struct ScanProgress {
    pub files_seen: u64,
    pub files_hashed: u64,
    pub files_skipped: u64,
    pub bytes_seen: u64,
    pub total_files: u64,
    pub total_bytes: u64,
    pub current_path: PathBuf,
}

pub fn scan_to_sqlite_with_progress<F>(
    config: &ScanConfig,
    store: &SqliteScanStore,
    cancel: Option<&ScanCancelToken>,
    on_progress: F,
) -> Result<ScanResult>
where
    F: FnMut(&ScanProgress),
{
    scan_to_sqlite_with_progress_and_totals(config, store, cancel, None, on_progress)
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ScanTotals {
    pub files: u64,
    pub bytes: u64,
}

pub fn scan_to_sqlite_with_progress_and_totals<F>(
    config: &ScanConfig,
    store: &SqliteScanStore,
    cancel: Option<&ScanCancelToken>,
    totals: Option<ScanTotals>,
    mut on_progress: F,
) -> Result<ScanResult>
where
    F: FnMut(&ScanProgress),
{
    if !config.root.exists() {
        return Err(Error::InvalidArgument(format!(
            "root does not exist: {}",
            config.root.to_string_lossy()
        )));
    }

    let drive = drive::probe_for_path(&config.root).unwrap_or(DriveMetadata {
        id: None,
        label: None,
        fs_type: None,
    });
    let root_parent_path = if config.root_kind == ScanRootKind::Folder {
        config.root.parent().map(|p| p.to_path_buf())
    } else {
        None
    };
    let fileset_meta = FilesetMetadata {
        created_at: SystemTime::now(),
        root_kind: config.root_kind,
        root_path: config.root.clone(),
        root_parent_path,
        drive,
        host_os: std::env::consts::OS.to_string(),
        host_os_version: host_os_version(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        status: String::new(),
        name: fileset_name_from_root(&config.root),
        description: String::new(),
        notes: String::new(),
    };
    store.set_fileset_metadata(&fileset_meta)?;

    let mut stats = ScanStats::default();
    let mut bytes_seen = 0u64;
    let totals = totals.unwrap_or_default();
    for entry in WalkDir::new(&config.root).follow_links(false).into_iter() {
        if let Some(cancel) = cancel {
            if cancel.is_cancelled() {
                update_fileset_status(store, config, "incomplete");
                return Err(Error::Cancelled);
            }
        }

        let entry = match entry {
            Ok(v) => v,
            Err(_) => {
                stats.files_skipped += 1;
                continue;
            }
        };

        if !entry.file_type().is_file() {
            continue;
        }

        stats.files_seen += 1;
        let path = entry.path().to_path_buf();
        let md = match entry.metadata() {
            Ok(v) => v,
            Err(_) => {
                stats.files_skipped += 1;
                continue;
            }
        };

        bytes_seen = bytes_seen.saturating_add(md.len());
        let mut rec = MediaFileRecord {
            path: relative_to_root(&config.root, &path).unwrap_or(path.clone()),
            size_bytes: md.len(),
            modified_at: md.modified().ok(),
            blake3: None,
        };

        if config.hash_files {
            match blake3_file(&path) {
                Ok(hash) => {
                    rec.blake3 = Some(hash);
                    stats.files_hashed += 1;
                }
                Err(_) => {
                    stats.files_skipped += 1;
                }
            }
        }

        store.upsert_file(&rec)?;

        on_progress(&ScanProgress {
            files_seen: stats.files_seen,
            files_hashed: stats.files_hashed,
            files_skipped: stats.files_skipped,
            bytes_seen,
            total_files: totals.files,
            total_bytes: totals.bytes,
            current_path: path,
        });
    }

    update_fileset_status(store, config, "completed");
    Ok(ScanResult { stats })
}

#[derive(Debug, Clone)]
pub struct PrescanProgress {
    pub files_seen: u64,
    pub bytes_seen: u64,
    pub dirs_seen: u64,
    pub current_path: PathBuf,
}

pub fn prescan<F>(
    config: &ScanConfig,
    cancel: Option<&ScanCancelToken>,
    mut on_progress: F,
) -> Result<ScanTotals>
where
    F: FnMut(&PrescanProgress),
{
    if !config.root.exists() {
        return Err(Error::InvalidArgument(format!(
            "root does not exist: {}",
            config.root.to_string_lossy()
        )));
    }

    let mut files = 0u64;
    let mut bytes = 0u64;
    let mut dirs = 0u64;

    for entry in WalkDir::new(&config.root).follow_links(false).into_iter() {
        if let Some(cancel) = cancel {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
        }

        let entry = match entry {
            Ok(v) => v,
            Err(_) => {
                continue;
            }
        };

        if entry.file_type().is_dir() {
            dirs += 1;
            on_progress(&PrescanProgress {
                files_seen: files,
                bytes_seen: bytes,
                dirs_seen: dirs,
                current_path: entry.path().to_path_buf(),
            });
            continue;
        }

        if !entry.file_type().is_file() {
            continue;
        }

        files += 1;
        if let Ok(md) = entry.metadata() {
            bytes = bytes.saturating_add(md.len());
        }

        on_progress(&PrescanProgress {
            files_seen: files,
            bytes_seen: bytes,
            dirs_seen: dirs,
            current_path: entry.path().to_path_buf(),
        });
    }

    Ok(ScanTotals { files, bytes })
}

fn relative_to_root(root: &Path, path: &Path) -> Option<PathBuf> {
    path.strip_prefix(root).ok().map(|p| p.to_path_buf())
}

fn fileset_name_from_root(root: &Path) -> String {
    root.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(|name| name.to_string())
        .unwrap_or_else(|| root.to_string_lossy().to_string())
}

fn host_os_version() -> String {
    #[cfg(target_os = "linux")]
    {
        if let Ok(contents) = std::fs::read_to_string("/etc/os-release") {
            for line in contents.lines() {
                if let Some(value) = line.strip_prefix("PRETTY_NAME=") {
                    return value.trim_matches('"').to_string();
                }
            }
        }
    }

    String::new()
}

fn update_fileset_status(store: &SqliteScanStore, config: &ScanConfig, status: &str) {
    let meta = store
        .get_fileset_metadata()
        .ok()
        .flatten()
        .unwrap_or_else(|| FilesetMetadata {
            created_at: SystemTime::now(),
            root_kind: config.root_kind,
            root_path: config.root.clone(),
            root_parent_path: if config.root_kind == ScanRootKind::Folder {
                config.root.parent().map(|p| p.to_path_buf())
            } else {
                None
            },
            drive: DriveMetadata {
                id: None,
                label: None,
                fs_type: None,
            },
            host_os: std::env::consts::OS.to_string(),
            host_os_version: host_os_version(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            status: String::new(),
            name: fileset_name_from_root(&config.root),
            description: String::new(),
            notes: String::new(),
        });
    let mut updated = meta;
    updated.status = status.to_string();
    let _ = store.set_fileset_metadata(&updated);
}
