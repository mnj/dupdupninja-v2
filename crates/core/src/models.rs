use std::path::PathBuf;
use std::time::SystemTime;

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
pub struct MediaFileRecord {
    pub file_id: Option<i64>,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub modified_at: Option<SystemTime>,
    pub blake3: Option<[u8; 32]>,
    pub sha256: Option<[u8; 32]>,
    pub ffmpeg_metadata: Option<String>,
    pub file_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FileSnapshotRecord {
    pub snapshot_index: u32,
    pub snapshot_count: u32,
    pub at_ms: i64,
    pub duration_ms: Option<i64>,
    pub image_avif: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct FileListRow {
    pub id: i64,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub blake3: Option<[u8; 32]>,
    pub sha256: Option<[u8; 32]>,
    pub file_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FilesetMetadata {
    pub created_at: SystemTime,
    pub root_kind: ScanRootKind,
    pub root_path: PathBuf,
    pub root_parent_path: Option<PathBuf>,
    pub drive: DriveMetadata,
    pub host_os: String,
    pub host_os_version: String,
    pub app_version: String,
    pub status: String,
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
    pub stats: ScanStats,
}
