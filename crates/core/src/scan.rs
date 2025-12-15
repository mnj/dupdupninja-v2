use std::path::{Path, PathBuf};
use std::time::SystemTime;

use uuid::Uuid;
use walkdir::WalkDir;

use crate::db::SqliteScanStore;
use crate::error::{Error, Result};
use crate::hash::blake3_file;
use crate::models::{DriveMetadata, MediaFileRecord, ScanMetadata, ScanResult, ScanRootKind, ScanStats};

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
    if !config.root.exists() {
        return Err(Error::InvalidArgument(format!(
            "root does not exist: {}",
            config.root.to_string_lossy()
        )));
    }

    let scan_id = Uuid::new_v4();
    let meta = ScanMetadata {
        id: scan_id,
        created_at: SystemTime::now(),
        root_kind: config.root_kind,
        root_path: config.root.clone(),
        drive: DriveMetadata {
            id: None,
            label: None,
            fs_type: None,
        },
    };
    store.insert_scan(&meta)?;

    let mut stats = ScanStats::default();
    for entry in WalkDir::new(&config.root).follow_links(false).into_iter() {
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

        let mut rec = MediaFileRecord {
            scan_id,
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
    }

    Ok(ScanResult { scan_id, stats })
}

fn relative_to_root(root: &Path, path: &Path) -> Option<PathBuf> {
    path.strip_prefix(root).ok().map(|p| p.to_path_buf())
}

