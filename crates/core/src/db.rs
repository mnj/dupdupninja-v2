use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::Result;
use crate::models::{
    DriveMetadata, FileListRow, FileSnapshotRecord, FilesetMetadata, MediaFileRecord, ScanRootKind,
};

pub struct SqliteScanStore {
    conn: Connection,
    has_file_id: bool,
}

impl SqliteScanStore {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        let store = Self {
            conn,
            has_file_id: false,
        };
        store.init_schema()?;
        let has_file_id = store.files_table_has_id()?;
        Ok(Self {
            conn: store.conn,
            has_file_id,
        })
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
              ahash INTEGER,
              dhash INTEGER,
              phash INTEGER,
              ffmpeg_metadata TEXT,
              file_type TEXT,
              UNIQUE(path)
            );

            CREATE INDEX IF NOT EXISTS idx_files_blake3 ON files(blake3);
            CREATE INDEX IF NOT EXISTS idx_files_ahash ON files(ahash);
            CREATE INDEX IF NOT EXISTS idx_files_dhash ON files(dhash);
            CREATE INDEX IF NOT EXISTS idx_files_phash ON files(phash);

            CREATE TABLE IF NOT EXISTS file_snapshots (
              file_id INTEGER NOT NULL,
              snapshot_index INTEGER NOT NULL,
              snapshot_count INTEGER NOT NULL,
              at_ms INTEGER NOT NULL,
              duration_ms INTEGER,
              ahash INTEGER,
              dhash INTEGER,
              phash INTEGER,
              image_avif BLOB NOT NULL,
              PRIMARY KEY (file_id, snapshot_index),
              FOREIGN KEY(file_id) REFERENCES files(id) ON DELETE CASCADE
            ) WITHOUT ROWID;

            CREATE INDEX IF NOT EXISTS idx_file_snapshots_file_id ON file_snapshots(file_id);
            "#,
        )?;
        self.ensure_hash_columns()?;
        Ok(())
    }

    fn files_table_has_id(&self) -> Result<bool> {
        let mut stmt = self.conn.prepare("PRAGMA table_info(files)")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            if name == "id" {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn file_id_column(&self) -> &'static str {
        if self.has_file_id {
            "id"
        } else {
            "rowid"
        }
    }

    fn ensure_hash_columns(&self) -> Result<()> {
        self.ensure_column("files", "ahash", "INTEGER")?;
        self.ensure_column("files", "dhash", "INTEGER")?;
        self.ensure_column("files", "phash", "INTEGER")?;
        self.ensure_column("file_snapshots", "ahash", "INTEGER")?;
        self.ensure_column("file_snapshots", "dhash", "INTEGER")?;
        self.ensure_column("file_snapshots", "phash", "INTEGER")?;
        Ok(())
    }

    fn ensure_column(&self, table: &str, column: &str, col_type: &str) -> Result<()> {
        if self.table_has_column(table, column)? {
            return Ok(());
        }
        let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {col_type}");
        self.conn.execute(&sql, [])?;
        Ok(())
    }

    fn table_has_column(&self, table: &str, column: &str) -> Result<bool> {
        let mut stmt = self.conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            if name == column {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn upsert_file(&self, rec: &MediaFileRecord) -> Result<i64> {
        let modified_at_secs = rec.modified_at.map(system_time_to_secs).map(|v| v as i64);

        let blake3_bytes: Option<Vec<u8>> = rec.blake3.map(|b| b.to_vec());
        let sha256_bytes: Option<Vec<u8>> = rec.sha256.map(|b| b.to_vec());
        let ahash = rec.ahash.map(|v| v as i64);
        let dhash = rec.dhash.map(|v| v as i64);
        let phash = rec.phash.map(|v| v as i64);

        self.conn.execute(
            r#"
            INSERT INTO files (
              path, size_bytes, modified_at_secs, blake3, sha256, ahash, dhash, phash, ffmpeg_metadata, file_type
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(path) DO UPDATE SET
              size_bytes=excluded.size_bytes,
              modified_at_secs=excluded.modified_at_secs,
              blake3=excluded.blake3,
              sha256=excluded.sha256,
              ahash=excluded.ahash,
              dhash=excluded.dhash,
              phash=excluded.phash,
              ffmpeg_metadata=excluded.ffmpeg_metadata,
              file_type=excluded.file_type
            "#,
            params![
                rec.path.to_string_lossy(),
                rec.size_bytes as i64,
                modified_at_secs,
                blake3_bytes,
                sha256_bytes,
                ahash,
                dhash,
                phash,
                rec.ffmpeg_metadata.as_deref(),
                rec.file_type.as_deref(),
            ],
        )?;
        let id_col = self.file_id_column();
        let sql = format!("SELECT {id_col} FROM files WHERE path = ?1");
        let file_id = self
            .conn
            .query_row(&sql, params![rec.path.to_string_lossy()], |r| {
                r.get::<_, i64>(0)
            })?;
        Ok(file_id)
    }

    pub fn replace_file_snapshots(
        &self,
        file_id: i64,
        snapshots: &[FileSnapshotRecord],
    ) -> Result<()> {
        self.conn.execute_batch("BEGIN")?;
        let res: Result<()> = (|| {
            self.conn.execute(
                r#"DELETE FROM file_snapshots WHERE file_id = ?1"#,
                params![file_id],
            )?;

            for snap in snapshots {
                self.conn.execute(
                r#"
                INSERT INTO file_snapshots (
                  file_id, snapshot_index, snapshot_count, at_ms, duration_ms, ahash, dhash, phash, image_avif
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                "#,
                params![
                    file_id,
                    snap.snapshot_index as i64,
                    snap.snapshot_count as i64,
                    snap.at_ms,
                    snap.duration_ms,
                    snap.ahash.map(|v| v as i64),
                    snap.dhash.map(|v| v as i64),
                    snap.phash.map(|v| v as i64),
                    &snap.image_avif,
                ],
                )?;
            }
            Ok(())
        })();

        match res {
            Ok(()) => {
                self.conn.execute_batch("COMMIT")?;
                Ok(())
            }
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
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
                        root_path: root_path.map(std::path::PathBuf::from).unwrap_or_default(),
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

    pub fn list_files(&self, limit: usize, offset: usize) -> Result<Vec<FileListRow>> {
        let id_col = self.file_id_column();
        let sql = format!(
            r#"
            SELECT {id_col} AS id, path, size_bytes, blake3, sha256, ahash, dhash, phash, file_type
            FROM files
            ORDER BY path
            LIMIT ?1 OFFSET ?2
            "#
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![limit as i64, offset as i64], |r| {
            let blake3: Option<Vec<u8>> = r.get(3)?;
            let sha256: Option<Vec<u8>> = r.get(4)?;
            let ahash: Option<i64> = r.get(5)?;
            let dhash: Option<i64> = r.get(6)?;
            let phash: Option<i64> = r.get(7)?;
            Ok(FileListRow {
                id: r.get(0)?,
                path: Path::new(r.get::<_, String>(1)?.as_str()).to_path_buf(),
                size_bytes: r.get::<_, i64>(2)? as u64,
                blake3: blob_to_hash(blake3),
                sha256: blob_to_hash(sha256),
                ahash: ahash.map(|v| v as u64),
                dhash: dhash.map(|v| v as u64),
                phash: phash.map(|v| v as u64),
                file_type: r.get(8)?,
            })
        })?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn list_files_with_duplicates(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<FileListRow>> {
        let id_col = self.file_id_column();
        let sql = format!(
            r#"
            SELECT f1.{id_col} AS id, f1.path, f1.size_bytes, f1.blake3, f1.sha256, f1.ahash, f1.dhash, f1.phash, f1.file_type
            FROM files f1
            WHERE (
                f1.blake3 IS NOT NULL
                AND EXISTS (
                  SELECT 1 FROM files f2
                  WHERE f2.blake3 = f1.blake3 AND f2.{id_col} != f1.{id_col}
                )
              ) OR (
                f1.blake3 IS NULL
                AND f1.sha256 IS NOT NULL
                AND EXISTS (
                  SELECT 1 FROM files f2
                  WHERE f2.sha256 = f1.sha256 AND f2.{id_col} != f1.{id_col}
                )
              )
            ORDER BY f1.path
            LIMIT ?1 OFFSET ?2
            "#
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![limit as i64, offset as i64], |r| {
            let blake3: Option<Vec<u8>> = r.get(3)?;
            let sha256: Option<Vec<u8>> = r.get(4)?;
            let ahash: Option<i64> = r.get(5)?;
            let dhash: Option<i64> = r.get(6)?;
            let phash: Option<i64> = r.get(7)?;
            Ok(FileListRow {
                id: r.get(0)?,
                path: Path::new(r.get::<_, String>(1)?.as_str()).to_path_buf(),
                size_bytes: r.get::<_, i64>(2)? as u64,
                blake3: blob_to_hash(blake3),
                sha256: blob_to_hash(sha256),
                ahash: ahash.map(|v| v as u64),
                dhash: dhash.map(|v| v as u64),
                phash: phash.map(|v| v as u64),
                file_type: r.get(8)?,
            })
        })?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn list_files_with_hashes(&self, limit: usize, offset: usize) -> Result<Vec<FileListRow>> {
        let id_col = self.file_id_column();
        let sql = format!(
            r#"
            SELECT {id_col} AS id, path, size_bytes, blake3, sha256, ahash, dhash, phash, file_type
            FROM files
            WHERE ahash IS NOT NULL OR dhash IS NOT NULL OR phash IS NOT NULL
            ORDER BY path
            LIMIT ?1 OFFSET ?2
            "#
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![limit as i64, offset as i64], |r| {
            let blake3: Option<Vec<u8>> = r.get(3)?;
            let sha256: Option<Vec<u8>> = r.get(4)?;
            let ahash: Option<i64> = r.get(5)?;
            let dhash: Option<i64> = r.get(6)?;
            let phash: Option<i64> = r.get(7)?;
            Ok(FileListRow {
                id: r.get(0)?,
                path: Path::new(r.get::<_, String>(1)?.as_str()).to_path_buf(),
                size_bytes: r.get::<_, i64>(2)? as u64,
                blake3: blob_to_hash(blake3),
                sha256: blob_to_hash(sha256),
                ahash: ahash.map(|v| v as u64),
                dhash: dhash.map(|v| v as u64),
                phash: phash.map(|v| v as u64),
                file_type: r.get(8)?,
            })
        })?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn list_direct_matches_by_blake3(&self, file_id: i64) -> Result<Vec<FileListRow>> {
        let id_col = self.file_id_column();
        let (blake3, sha256): (Option<Vec<u8>>, Option<Vec<u8>>) = match self
            .conn
            .query_row(
                &format!(r#"SELECT blake3, sha256 FROM files WHERE {id_col} = ?1"#),
                params![file_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
        {
            Some(values) => values,
            None => return Ok(Vec::new()),
        };
        let (hash, hash_col) = if let Some(hash) = blake3 {
            (hash, "blake3")
        } else if let Some(hash) = sha256 {
            (hash, "sha256")
        } else {
            return Ok(Vec::new());
        };

        let sql = format!(
            r#"
            SELECT {id_col} AS id, path, size_bytes, blake3, sha256, ahash, dhash, phash, file_type
            FROM files
            WHERE {hash_col} = ?1 AND {id_col} != ?2
            ORDER BY path
            "#
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![hash, file_id], |r| {
            let blake3: Option<Vec<u8>> = r.get(3)?;
            let sha256: Option<Vec<u8>> = r.get(4)?;
            let ahash: Option<i64> = r.get(5)?;
            let dhash: Option<i64> = r.get(6)?;
            let phash: Option<i64> = r.get(7)?;
            Ok(FileListRow {
                id: r.get(0)?,
                path: Path::new(r.get::<_, String>(1)?.as_str()).to_path_buf(),
                size_bytes: r.get::<_, i64>(2)? as u64,
                blake3: blob_to_hash(blake3),
                sha256: blob_to_hash(sha256),
                ahash: ahash.map(|v| v as u64),
                dhash: dhash.map(|v| v as u64),
                phash: phash.map(|v| v as u64),
                file_type: r.get(8)?,
            })
        })?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn get_file_by_id(&self, file_id: i64) -> Result<Option<MediaFileRecord>> {
        let id_col = self.file_id_column();
        let sql = format!(
            r#"
            SELECT path, size_bytes, modified_at_secs, blake3, sha256, ahash, dhash, phash, ffmpeg_metadata, file_type
            FROM files
            WHERE {id_col} = ?1
            "#
        );
        let row = self
            .conn
            .query_row(&sql, params![file_id], |r| {
                let blake3: Option<Vec<u8>> = r.get(3)?;
                let sha256: Option<Vec<u8>> = r.get(4)?;
                let ahash: Option<i64> = r.get(5)?;
                let dhash: Option<i64> = r.get(6)?;
                let phash: Option<i64> = r.get(7)?;
                let modified_at_secs: Option<i64> = r.get(2)?;
                Ok(MediaFileRecord {
                    file_id: Some(file_id),
                    path: Path::new(r.get::<_, String>(0)?.as_str()).to_path_buf(),
                    size_bytes: r.get::<_, i64>(1)? as u64,
                    modified_at: modified_at_secs.map(|v| secs_to_system_time(v.max(0) as u64)),
                    blake3: blob_to_hash(blake3),
                    sha256: blob_to_hash(sha256),
                    ahash: ahash.map(|v| v as u64),
                    dhash: dhash.map(|v| v as u64),
                    phash: phash.map(|v| v as u64),
                    ffmpeg_metadata: r.get(8)?,
                    file_type: r.get(9)?,
                })
            })
            .optional()?;
        Ok(row)
    }

    pub fn get_file_by_path(&self, path: &Path) -> Result<Option<MediaFileRecord>> {
        let id_col = self.file_id_column();
        let sql = format!(
            r#"
            SELECT {id_col} AS id, size_bytes, modified_at_secs, blake3, sha256, ahash, dhash, phash, ffmpeg_metadata, file_type
            FROM files
            WHERE path = ?1
            "#
        );
        let row = self
            .conn
            .query_row(&sql, params![path.to_string_lossy()], |r| {
                let blake3: Option<Vec<u8>> = r.get(3)?;
                let sha256: Option<Vec<u8>> = r.get(4)?;
                let ahash: Option<i64> = r.get(5)?;
                let dhash: Option<i64> = r.get(6)?;
                let phash: Option<i64> = r.get(7)?;
                let modified_at_secs: Option<i64> = r.get(2)?;
                Ok(MediaFileRecord {
                    file_id: Some(r.get(0)?),
                    path: path.to_path_buf(),
                    size_bytes: r.get::<_, i64>(1)? as u64,
                    modified_at: modified_at_secs.map(|v| secs_to_system_time(v.max(0) as u64)),
                    blake3: blob_to_hash(blake3),
                    sha256: blob_to_hash(sha256),
                    ahash: ahash.map(|v| v as u64),
                    dhash: dhash.map(|v| v as u64),
                    phash: phash.map(|v| v as u64),
                    ffmpeg_metadata: r.get(8)?,
                    file_type: r.get(9)?,
                })
            })
            .optional()?;
        Ok(row)
    }

    pub fn list_file_snapshots(&self, file_id: i64) -> Result<Vec<FileSnapshotRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT snapshot_index, snapshot_count, at_ms, duration_ms, ahash, dhash, phash, image_avif
            FROM file_snapshots
            WHERE file_id = ?1
            ORDER BY snapshot_index
            "#,
        )?;
        let rows = stmt.query_map(params![file_id], |r| {
            Ok(FileSnapshotRecord {
                snapshot_index: r.get::<_, i64>(0)? as u32,
                snapshot_count: r.get::<_, i64>(1)? as u32,
                at_ms: r.get::<_, i64>(2)?,
                duration_ms: r.get::<_, Option<i64>>(3)?,
                ahash: r.get::<_, Option<i64>>(4)?.map(|v| v as u64),
                dhash: r.get::<_, Option<i64>>(5)?.map(|v| v as u64),
                phash: r.get::<_, Option<i64>>(6)?.map(|v| v as u64),
                image_avif: r.get(7)?,
            })
        })?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn delete_file_by_path(&self, path: &Path) -> Result<bool> {
        let affected = self.conn.execute(
            "DELETE FROM files WHERE path = ?1",
            params![path.to_string_lossy()],
        )?;
        Ok(affected > 0)
    }
}

fn blob_to_hash(blob: Option<Vec<u8>>) -> Option<[u8; 32]> {
    let bytes = blob?;
    if bytes.len() != 32 {
        return None;
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Some(out)
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
