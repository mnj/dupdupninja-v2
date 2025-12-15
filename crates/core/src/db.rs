use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::Result;
use crate::models::{DriveMetadata, MediaFileRecord, ScanId, ScanMetadata, ScanRootKind};

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

            CREATE TABLE IF NOT EXISTS scans (
              id TEXT PRIMARY KEY NOT NULL,
              created_at_secs INTEGER NOT NULL,
              root_kind TEXT NOT NULL,
              root_path TEXT NOT NULL,
              drive_id TEXT,
              drive_label TEXT,
              drive_fs_type TEXT
            );

            CREATE TABLE IF NOT EXISTS files (
              scan_id TEXT NOT NULL,
              path TEXT NOT NULL,
              size_bytes INTEGER NOT NULL,
              modified_at_secs INTEGER,
              blake3 BLOB,
              PRIMARY KEY (scan_id, path),
              FOREIGN KEY (scan_id) REFERENCES scans(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_files_blake3 ON files(blake3);
            "#,
        )?;
        Ok(())
    }

    pub fn insert_scan(&self, meta: &ScanMetadata) -> Result<()> {
        let created_at_secs = system_time_to_secs(meta.created_at);
        self.conn.execute(
            r#"
            INSERT INTO scans (
              id, created_at_secs, root_kind, root_path,
              drive_id, drive_label, drive_fs_type
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                meta.id.to_string(),
                created_at_secs,
                root_kind_to_str(meta.root_kind),
                meta.root_path.to_string_lossy(),
                meta.drive.id,
                meta.drive.label,
                meta.drive.fs_type,
            ],
        )?;
        Ok(())
    }

    pub fn upsert_file(&self, rec: &MediaFileRecord) -> Result<()> {
        let modified_at_secs = rec
            .modified_at
            .map(system_time_to_secs)
            .map(|v| v as i64);

        let blake3_bytes: Option<Vec<u8>> = rec.blake3.map(|b| b.to_vec());

        self.conn.execute(
            r#"
            INSERT INTO files (
              scan_id, path, size_bytes, modified_at_secs, blake3
            ) VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(scan_id, path) DO UPDATE SET
              size_bytes=excluded.size_bytes,
              modified_at_secs=excluded.modified_at_secs,
              blake3=excluded.blake3
            "#,
            params![
                rec.scan_id.to_string(),
                rec.path.to_string_lossy(),
                rec.size_bytes as i64,
                modified_at_secs,
                blake3_bytes,
            ],
        )?;
        Ok(())
    }

    pub fn get_scan(&self, id: ScanId) -> Result<Option<ScanMetadata>> {
        let row = self
            .conn
            .query_row(
                r#"
                SELECT
                  id, created_at_secs, root_kind, root_path,
                  drive_id, drive_label, drive_fs_type
                FROM scans
                WHERE id = ?1
                "#,
                params![id.to_string()],
                |r| {
                    let id: String = r.get(0)?;
                    let created_at_secs: i64 = r.get(1)?;
                    let root_kind: String = r.get(2)?;
                    let root_path: String = r.get(3)?;
                    let drive_id: Option<String> = r.get(4)?;
                    let drive_label: Option<String> = r.get(5)?;
                    let drive_fs_type: Option<String> = r.get(6)?;

                    Ok(ScanMetadata {
                        id: id.parse().map_err(|_| {
                            rusqlite::Error::FromSqlConversionFailure(
                                0,
                                rusqlite::types::Type::Text,
                                Box::new(std::fmt::Error),
                            )
                        })?,
                        created_at: secs_to_system_time(created_at_secs as u64),
                        root_kind: str_to_root_kind(&root_kind),
                        root_path: root_path.into(),
                        drive: DriveMetadata {
                            id: drive_id,
                            label: drive_label,
                            fs_type: drive_fs_type,
                        },
                    })
                },
            )
            .optional()?;

        Ok(row)
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

