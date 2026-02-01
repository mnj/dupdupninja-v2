use std::convert::TryInto;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc,
    Arc,
};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use image_hasher::{HashAlg, HasherConfig};
use walkdir::WalkDir;

use crate::db::SqliteScanStore;
use crate::error::{Error, Result};
use crate::drive;
use crate::hash::{blake3_file, sha256_file};
use crate::models::{
    DriveMetadata, FileSnapshotRecord, FilesetMetadata, MediaFileRecord, ScanResult, ScanRootKind,
    ScanStats,
};
use serde_json::Value;
use wait_timeout::ChildExt;

#[derive(Debug, Clone)]
pub struct ScanConfig {
    pub root: PathBuf,
    pub root_kind: ScanRootKind,
    pub hash_files: bool,
    pub perceptual_hashes: bool,
    pub capture_snapshots: bool,
    pub snapshots_per_video: u32,
    pub snapshot_max_dim: u32,
}

impl ScanConfig {
    pub fn for_folder(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            root_kind: ScanRootKind::Folder,
            hash_files: true,
            perceptual_hashes: true,
            capture_snapshots: true,
            snapshots_per_video: 3,
            snapshot_max_dim: 1024,
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
        let linked_file = is_linked_file(&entry, &md);
        let mut rec = MediaFileRecord {
            file_id: None,
            path: relative_to_root(&config.root, &path).unwrap_or(path.clone()),
            size_bytes: md.len(),
            modified_at: md.modified().ok(),
            blake3: None,
            sha256: None,
            ahash: None,
            dhash: None,
            phash: None,
            ffmpeg_metadata: None,
            file_type: None,
        };

        rec.file_type = match infer::get_from_path(&path) {
            Ok(Some(kind)) => Some(kind.mime_type().to_string()),
            Ok(None) => None,
            Err(_) => None,
        };

        rec.ffmpeg_metadata = ffprobe_metadata(&path);

        if config.perceptual_hashes && !linked_file && is_image_file(&path, rec.file_type.as_deref())
        {
            if let Some((ahash, dhash, phash)) = image_hashes_from_path(&path) {
                rec.ahash = Some(ahash);
                rec.dhash = Some(dhash);
                rec.phash = Some(phash);
            }
        }

        if config.hash_files && !linked_file {
            match blake3_file(&path) {
                Ok(hash) => {
                    rec.blake3 = Some(hash);
                }
                Err(_) => {
                    stats.files_skipped += 1;
                }
            }
            match sha256_file(&path) {
                Ok(hash) => {
                    rec.sha256 = Some(hash);
                    stats.files_hashed += 1;
                }
                Err(_) => {
                    stats.files_skipped += 1;
                }
            }
        }

        let file_id = store.upsert_file(&rec)?;
        rec.file_id = Some(file_id);

        if config.capture_snapshots && config.snapshots_per_video > 0 {
            let is_video = is_video_file(&path, rec.file_type.as_deref());
            let duration_ms = rec
                .ffmpeg_metadata
                .as_deref()
                .and_then(ffprobe_duration_ms);

            if is_video && duration_ms.is_some() {
                let snapshots = video_snapshots_for_file(
                    &path,
                    duration_ms,
                    config.snapshots_per_video,
                    config.snapshot_max_dim,
                    Duration::from_secs(30),
                );
                if let Some(snaps) = snapshots {
                    let _ = store.replace_file_snapshots(file_id, &snaps);
                }
            }
        }

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

fn is_linked_file(entry: &walkdir::DirEntry, md: &std::fs::Metadata) -> bool {
    if entry.file_type().is_symlink() {
        return true;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if md.nlink() > 1 {
            return true;
        }
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if md.number_of_links() > 1 {
            return true;
        }
    }

    false
}

fn ffprobe_metadata(path: &Path) -> Option<String> {
    let (tx, rx) = mpsc::channel();
    let path = path.to_path_buf();
    thread::spawn(move || {
        let result = std::panic::catch_unwind(|| ffprobe_metadata_inner(&path)).ok().flatten();
        let _ = tx.send(result);
    });

    rx.recv_timeout(Duration::from_secs(30)).ok().flatten()
}

fn ffprobe_metadata_inner(path: &Path) -> Option<String> {
    let mut child = Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-print_format")
        .arg("json")
        .arg("-show_format")
        .arg("-show_streams")
        .arg("--")
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;

    let mut stdout = child.stdout.take()?;
    let mut stderr = child.stderr.take()?;
    let timeout = Duration::from_secs(30);

    match child.wait_timeout(timeout).ok()? {
        Some(status) => {
            let mut out = Vec::new();
            let mut err = Vec::new();
            let _ = stdout.read_to_end(&mut out);
            let _ = stderr.read_to_end(&mut err);
            if !status.success() {
                return None;
            }
            let text = String::from_utf8(out).ok()?;
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        None => {
            let _ = child.kill();
            let _ = child.wait();
            None
        }
    }
}

fn ffprobe_duration_ms(json: &str) -> Option<i64> {
    let v: Value = serde_json::from_str(json).ok()?;
    let duration_secs = v
        .get("format")
        .and_then(|f| f.get("duration"))
        .and_then(|d| {
            if let Some(s) = d.as_str() {
                s.parse::<f64>().ok()
            } else {
                d.as_f64()
            }
        })
        .filter(|d| d.is_finite() && *d > 0.0);

    duration_secs.map(|d| (d * 1000.0).round() as i64)
}

fn is_video_file(path: &Path, file_type: Option<&str>) -> bool {
    if let Some(mime) = file_type {
        if mime.starts_with("video/") {
            return true;
        }
    }

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());

    matches!(
        ext.as_deref(),
        Some("mp4")
            | Some("m4v")
            | Some("mov")
            | Some("mkv")
            | Some("webm")
            | Some("avi")
            | Some("mpeg")
            | Some("mpg")
            | Some("mpe")
            | Some("ts")
            | Some("mts")
            | Some("m2ts")
            | Some("3gp")
            | Some("3g2")
            | Some("wmv")
            | Some("flv")
            | Some("f4v")
            | Some("ogv")
            | Some("mxf")
    )
}

fn video_snapshots_for_file(
    path: &Path,
    duration_ms: Option<i64>,
    snapshots_per_video: u32,
    snapshot_max_dim: u32,
    timeout: Duration,
) -> Option<Vec<FileSnapshotRecord>> {
    let duration_ms = duration_ms?;
    let (tx, rx) = mpsc::channel();
    let path = path.to_path_buf();

    let inner_timeout = timeout.saturating_sub(Duration::from_secs(2));
    thread::spawn(move || {
        let result = std::panic::catch_unwind(|| {
            video_snapshots_for_file_inner(
                &path,
                duration_ms,
                snapshots_per_video,
                snapshot_max_dim,
                inner_timeout,
            )
        })
        .ok()
        .flatten();
        let _ = tx.send(result);
    });

    rx.recv_timeout(timeout).ok().flatten()
}

fn video_snapshots_for_file_inner(
    path: &Path,
    duration_ms: i64,
    snapshots_per_video: u32,
    snapshot_max_dim: u32,
    timeout: Duration,
) -> Option<Vec<FileSnapshotRecord>> {
    if snapshots_per_video == 0 || duration_ms <= 0 {
        return Some(Vec::new());
    }

    let deadline = Instant::now() + timeout;
    let duration_secs = (duration_ms as f64) / 1000.0;

    let mut snaps = Vec::with_capacity(snapshots_per_video as usize);
    for idx in 0..snapshots_per_video {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining < Duration::from_millis(250) {
            break;
        }

        let pos = ((idx + 1) as f64) / ((snapshots_per_video + 1) as f64);
        let mut at_secs = duration_secs * pos;
        if duration_secs > 2.0 {
            at_secs = at_secs.clamp(0.5, duration_secs - 0.5);
        } else {
            at_secs = at_secs.clamp(0.0, duration_secs.max(0.0));
        }

        let per_snapshot_timeout = remaining.min(Duration::from_secs(10));
        let image_avif = match ffmpeg_snapshot_avif_inner(
            path,
            at_secs,
            snapshot_max_dim,
            per_snapshot_timeout,
        ) {
            Some(bytes) => bytes,
            None => continue,
        };

        let (ahash, dhash, phash) = image_hashes_from_avif(&image_avif)
            .map(|(a, d, p)| (Some(a), Some(d), Some(p)))
            .unwrap_or((None, None, None));

        snaps.push(FileSnapshotRecord {
            snapshot_index: idx,
            snapshot_count: snapshots_per_video,
            at_ms: (at_secs * 1000.0).round() as i64,
            duration_ms: Some(duration_ms),
            ahash,
            dhash,
            phash,
            image_avif,
        });
    }

    Some(snaps)
}

fn ffmpeg_snapshot_avif_inner(
    path: &Path,
    at_secs: f64,
    snapshot_max_dim: u32,
    timeout: Duration,
) -> Option<Vec<u8>> {
    let ts = format!("{at_secs:.3}");
    let mut out_path = std::env::temp_dir();
    let unique = format!(
        "dupdupninja-snapshot-{}-{}.avif",
        std::process::id(),
        SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_nanos()
    );
    out_path.push(unique);

    let max_dim = snapshot_max_dim.max(1);
    let scale_filter = format!(
        "scale='min(iw,{0})':'min(ih,{0})':force_original_aspect_ratio=decrease,scale=trunc(iw/2)*2:trunc(ih/2)*2",
        max_dim
    );

    let mut child = Command::new("ffmpeg")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-nostdin")
        .arg("-ss")
        .arg(ts)
        .arg("-i")
        .arg(path)
        .arg("-map")
        .arg("0:v:0")
        .arg("-frames:v")
        .arg("1")
        .arg("-an")
        .arg("-sn")
        .arg("-dn")
        .arg("-vf")
        .arg(scale_filter)
        .arg("-pix_fmt")
        .arg("yuv420p")
        .arg("-c:v")
        .arg("libaom-av1")
        .arg("-still-picture")
        .arg("1")
        .arg("-crf")
        .arg("35")
        .arg("-b:v")
        .arg("0")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .arg(&out_path)
        .spawn()
        .ok()?;

    match child.wait_timeout(timeout).ok()? {
        Some(status) => {
            if !status.success() {
                let _ = std::fs::remove_file(&out_path);
                return None;
            }
            let bytes = std::fs::read(&out_path).ok()?;
            let _ = std::fs::remove_file(&out_path);
            Some(bytes)
        }
        None => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(&out_path);
            None
        }
    }
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

fn is_image_file(path: &Path, file_type: Option<&str>) -> bool {
    if let Some(mime) = file_type {
        if mime.starts_with("image/") {
            return true;
        }
    }
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "jpg"
            | "jpeg"
            | "png"
            | "gif"
            | "bmp"
            | "tiff"
            | "webp"
            | "avif"
            | "heic"
            | "heif"
    )
}

fn image_hashes_from_path(path: &Path) -> Option<(u64, u64, u64)> {
    let image = image::open(path).ok()?;
    image_hashes_from_image(&image)
}

fn image_hashes_from_avif(bytes: &[u8]) -> Option<(u64, u64, u64)> {
    let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Avif).ok()?;
    image_hashes_from_image(&image)
}

fn image_hashes_from_image(image: &image::DynamicImage) -> Option<(u64, u64, u64)> {
    let ahash = hash_image_with_alg(image, HashAlg::Mean, false)?;
    let dhash = hash_image_with_alg(image, HashAlg::Gradient, false)?;
    let phash = hash_image_with_alg(image, HashAlg::Mean, true)?;
    Some((ahash, dhash, phash))
}

fn hash_image_with_alg(image: &image::DynamicImage, alg: HashAlg, use_dct: bool) -> Option<u64> {
    let mut config = HasherConfig::new().hash_alg(alg);
    if use_dct {
        config = config.preproc_dct();
    }
    let hasher = config.to_hasher();
    let hash = hasher.hash_image(image);
    hash_to_u64(&hash)
}

fn hash_to_u64(hash: &image_hasher::ImageHash) -> Option<u64> {
    let bytes = hash.as_bytes();
    if bytes.len() != 8 {
        return None;
    }
    let arr: [u8; 8] = bytes.try_into().ok()?;
    Some(u64::from_be_bytes(arr))
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
