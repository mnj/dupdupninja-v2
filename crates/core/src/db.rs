use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::Result;
use crate::models::{DriveMetadata, FilesetMetadata, MediaFileRecord, ScanRootKind};

pub struct SqliteScanStore {
    conn: Connection,
}

impl SqliteScanStore {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        let store = Self { conn };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS fileset (
              id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
              created_at_secs INTEGER,
              root_kind TEXT,
              root_path TEXT,
              root_parent_path TEXT,
              drive_id TEXT,
              drive_label TEXT,
              drive_fs_type TEXT,
              host_os TEXT,
              host_os_version TEXT,
              app_version TEXT,
              status TEXT,
              name TEXT,
              description TEXT,
              notes TEXT
            );

            CREATE TABLE IF NOT EXISTS files (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              path TEXT NOT NULL,
              size_bytes INTEGER NOT NULL,
              modified_at_secs INTEGER,
              blake3 BLOB,
              sha256 BLOB,
              ffmpeg_metadata TEXT,
              UNIQUE(path)
            );

            CREATE INDEX IF NOT EXISTS idx_files_blake3 ON files(blake3);
            "#,
        )?;
        Ok(())
    }

    pub fn upsert_file(&self, rec: &MediaFileRecord) -> Result<()> {
        let modified_at_secs = rec
            .modified_at
            .map(system_time_to_secs)
            .map(|v| v as i64);

        let blake3_bytes: Option<Vec<u8>> = rec.blake3.map(|b| b.to_vec());
        let sha256_bytes: Option<Vec<u8>> = rec.sha256.map(|b| b.to_vec());

        self.conn.execute(
            r#"
            INSERT INTO files (
              path, size_bytes, modified_at_secs, blake3, sha256, ffmpeg_metadata
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(path) DO UPDATE SET
              size_bytes=excluded.size_bytes,
              modified_at_secs=excluded.modified_at_secs,
              blake3=excluded.blake3,
              sha256=excluded.sha256,
              ffmpeg_metadata=excluded.ffmpeg_metadata
            "#,
            params![
                rec.path.to_string_lossy(),
                rec.size_bytes as i64,
                modified_at_secs,
                blake3_bytes,
                sha256_bytes,
                rec.ffmpeg_metadata.as_deref(),
            ],
        )?;
        Ok(())
    }

    pub fn get_fileset_metadata(&self) -> Result<Option<FilesetMetadata>> {
        let row = self
            .conn
            .query_row(
                r#"
                SELECT
                  created_at_secs, root_kind, root_path, root_parent_path,
                  drive_id, drive_label, drive_fs_type,
                  host_os, host_os_version, app_version, status,
                  name, description, notes
                FROM fileset
                WHERE id = 1
                "#,
                [],
                |r| {
                    let created_at_secs: Option<i64> = r.get(0)?;
                    let root_kind: Option<String> = r.get(1)?;
                    let root_path: Option<String> = r.get(2)?;
                    let root_parent_path: Option<String> = r.get(3)?;
                    let drive_id: Option<String> = r.get(4)?;
                    let drive_label: Option<String> = r.get(5)?;
                    let drive_fs_type: Option<String> = r.get(6)?;
                    let host_os: Option<String> = r.get(7)?;
                    let host_os_version: Option<String> = r.get(8)?;
                    let app_version: Option<String> = r.get(9)?;
                    let status: Option<String> = r.get(10)?;
                    let name: Option<String> = r.get(11)?;
                    let description: Option<String> = r.get(12)?;
                    let notes: Option<String> = r.get(13)?;
                    Ok(FilesetMetadata {
                        created_at: created_at_secs
                            .map(|v| secs_to_system_time(v as u64))
                            .unwrap_or_else(SystemTime::now),
                        root_kind: root_kind
                            .as_deref()
                            .map(str_to_root_kind)
                            .unwrap_or(ScanRootKind::Folder),
                        root_path: root_path
                            .map(std::path::PathBuf::from)
                            .unwrap_or_default(),
                        root_parent_path: root_parent_path.map(std::path::PathBuf::from),
                        drive: DriveMetadata {
                            id: drive_id,
                            label: drive_label,
                            fs_type: drive_fs_type,
                        },
                        host_os: host_os.unwrap_or_default(),
                        host_os_version: host_os_version.unwrap_or_default(),
                        app_version: app_version.unwrap_or_default(),
                        status: status.unwrap_or_default(),
                        name: name.unwrap_or_default(),
                        description: description.unwrap_or_default(),
                        notes: notes.unwrap_or_default(),
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    pub fn set_fileset_metadata(&self, meta: &FilesetMetadata) -> Result<()> {
        let created_at_secs = system_time_to_secs(meta.created_at);
        let root_parent = meta
            .root_parent_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string());
        self.conn.execute(
            r#"
            INSERT INTO fileset (
              id, created_at_secs, root_kind, root_path, root_parent_path,
              drive_id, drive_label, drive_fs_type,
              host_os, host_os_version, app_version, status,
              name, description, notes
            ) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
            ON CONFLICT(id) DO UPDATE SET
              created_at_secs=excluded.created_at_secs,
              root_kind=excluded.root_kind,
              root_path=excluded.root_path,
              root_parent_path=excluded.root_parent_path,
              drive_id=excluded.drive_id,
              drive_label=excluded.drive_label,
              drive_fs_type=excluded.drive_fs_type,
              host_os=excluded.host_os,
              host_os_version=excluded.host_os_version,
              app_version=excluded.app_version,
              status=excluded.status,
              name=excluded.name,
              description=excluded.description,
              notes=excluded.notes
            "#,
            params![
                created_at_secs as i64,
                root_kind_to_str(meta.root_kind),
                meta.root_path.to_string_lossy(),
                root_parent,
                meta.drive.id,
                meta.drive.label,
                meta.drive.fs_type,
                meta.host_os,
                meta.host_os_version,
                meta.app_version,
                meta.status,
                meta.name,
                meta.description,
                meta.notes
            ],
        )?;
        Ok(())
    }

    pub fn count_files(&self) -> Result<u64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))?;
        Ok(count.max(0) as u64)
    }
}

fn system_time_to_secs(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs()
}

fn secs_to_system_time(secs: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

fn root_kind_to_str(k: ScanRootKind) -> &'static str {
    match k {
        ScanRootKind::Folder => "folder",
        ScanRootKind::Drive => "drive",
    }
}

fn str_to_root_kind(s: &str) -> ScanRootKind {
    match s {
        "drive" => ScanRootKind::Drive,
        _ => ScanRootKind::Folder,
    }
}
