use std::path::PathBuf;
use std::time::SystemTime;

use uuid::Uuid;

pub type ScanId = Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanRootKind {
    Folder,
    Drive,
}

#[derive(Debug, Clone)]
pub struct DriveMetadata {
    pub id: Option<String>,
    pub label: Option<String>,
    pub fs_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ScanMetadata {
    pub id: ScanId,
    pub created_at: SystemTime,
    pub root_kind: ScanRootKind,
    pub root_path: PathBuf,
    pub drive: DriveMetadata,
}

#[derive(Debug, Clone)]
pub struct MediaFileRecord {
    pub scan_id: ScanId,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub modified_at: Option<SystemTime>,
    pub blake3: Option<[u8; 32]>,
}

#[derive(Debug, Default, Clone)]
pub struct FilesetMetadata {
    pub name: String,
    pub description: String,
    pub notes: String,
}

#[derive(Debug, Default, Clone)]
pub struct ScanStats {
    pub files_seen: u64,
    pub files_hashed: u64,
    pub files_skipped: u64,
}

#[derive(Debug, Clone)]
pub struct ScanResult {
    pub scan_id: ScanId,
    pub stats: ScanStats,
}
