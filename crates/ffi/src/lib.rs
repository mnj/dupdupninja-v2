#![allow(unsafe_code)]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::PathBuf;
use std::slice;

use dupdupninja_core::db::SqliteScanStore;
use dupdupninja_core::models::{DriveMetadata, FilesetMetadata, ScanRootKind};
use dupdupninja_core::scan::{
    prescan, scan_to_sqlite, scan_to_sqlite_with_progress, scan_to_sqlite_with_progress_and_totals,
    PrescanProgress, ScanCancelToken, ScanConfig, ScanTotals,
};

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

fn set_last_error(msg: impl Into<String>) {
    let msg = msg.into();
    let cmsg = CString::new(msg).unwrap_or_else(|_| CString::new("error").unwrap());
    LAST_ERROR.with(|slot| {
        *slot.borrow_mut() = Some(cmsg);
    });
}

fn ok_last_error() {
    LAST_ERROR.with(|slot| {
        *slot.borrow_mut() = None;
    });
}

#[repr(C)]
pub struct DupdupEngine {
    _private: [u8; 0],
}

struct Engine;

#[repr(C)]
pub struct DupdupCancelToken {
    token: ScanCancelToken,
}

#[repr(C)]
pub struct DupdupProgress {
    pub files_seen: u64,
    pub files_hashed: u64,
    pub files_skipped: u64,
    pub bytes_seen: u64,
    pub total_files: u64,
    pub total_bytes: u64,
    pub current_path: *const c_char,
    pub current_step: *const c_char,
}

pub type DupdupProgressCallback =
    Option<extern "C" fn(progress: *const DupdupProgress, user_data: *mut libc::c_void)>;

#[repr(C)]
pub struct DupdupPrescanTotals {
    pub total_files: u64,
    pub total_bytes: u64,
}

#[repr(C)]
pub struct DupdupPrescanProgress {
    pub files_seen: u64,
    pub bytes_seen: u64,
    pub dirs_seen: u64,
    pub current_path: *const c_char,
}

pub type DupdupPrescanCallback =
    Option<extern "C" fn(progress: *const DupdupPrescanProgress, user_data: *mut libc::c_void)>;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DupdupScanOptions {
    pub capture_snapshots: bool,
    pub snapshots_per_video: u32,
    pub snapshot_max_dim: u32,
}

#[repr(C)]
pub struct DupdupFilesetRow {
    pub id: i64,
    pub path: *mut c_char,
    pub size_bytes: u64,
    pub file_type: *mut c_char,
    pub blake3_hex: *mut c_char,
    pub sha256_hex: *mut c_char,
}

#[repr(C)]
pub struct DupdupExactGroup {
    pub label: *mut c_char,
    pub rows_start: usize,
    pub rows_len: usize,
}

#[repr(C)]
pub struct DupdupSimilarGroup {
    pub label: *mut c_char,
    pub rows_start: usize,
    pub rows_len: usize,
}

#[repr(C)]
pub struct DupdupSimilarRow {
    pub id: i64,
    pub path: *mut c_char,
    pub size_bytes: u64,
    pub file_type: *mut c_char,
    pub blake3_hex: *mut c_char,
    pub sha256_hex: *mut c_char,
    pub phash_distance: u8,
    pub dhash_distance: u8,
    pub ahash_distance: u8,
    pub confidence_percent: f32,
}

#[repr(C)]
pub struct DupdupFilesetMetadataView {
    pub name: *mut c_char,
    pub description: *mut c_char,
    pub notes: *mut c_char,
    pub status: *mut c_char,
}

#[repr(C)]
pub struct DupdupSnapshotInfo {
    pub snapshot_index: u32,
    pub snapshot_count: u32,
    pub at_ms: i64,
    pub has_duration: u8,
    pub duration_ms: i64,
    pub has_ahash: u8,
    pub ahash: u64,
    pub has_dhash: u8,
    pub dhash: u64,
    pub has_phash: u8,
    pub phash: u64,
}

const FFI_ABI_MAJOR: u32 = 1;
const FFI_ABI_MINOR: u32 = 3;
const FFI_ABI_PATCH: u32 = 0;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DupdupVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DupdupStatus {
    Ok = 0,
    Error = 1,
    InvalidArgument = 2,
    NullPointer = 3,
}

#[no_mangle]
pub extern "C" fn dupdupninja_engine_new() -> *mut DupdupEngine {
    ok_last_error();
    let engine = Box::new(Engine);
    Box::into_raw(engine) as *mut DupdupEngine
}

#[no_mangle]
pub extern "C" fn dupdupninja_ffi_version() -> DupdupVersion {
    DupdupVersion {
        major: FFI_ABI_MAJOR,
        minor: FFI_ABI_MINOR,
        patch: FFI_ABI_PATCH,
    }
}

#[no_mangle]
pub extern "C" fn dupdupninja_ffi_abi_major() -> u32 {
    FFI_ABI_MAJOR
}

#[no_mangle]
pub unsafe extern "C" fn dupdupninja_engine_free(engine: *mut DupdupEngine) {
    ok_last_error();
    if engine.is_null() {
        return;
    }
    drop(Box::from_raw(engine as *mut Engine));
}

#[no_mangle]
pub extern "C" fn dupdupninja_cancel_token_new() -> *mut DupdupCancelToken {
    ok_last_error();
    let token = DupdupCancelToken {
        token: ScanCancelToken::new(),
    };
    Box::into_raw(Box::new(token))
}

#[no_mangle]
pub unsafe extern "C" fn dupdupninja_cancel_token_free(token: *mut DupdupCancelToken) {
    ok_last_error();
    if token.is_null() {
        return;
    }
    drop(Box::from_raw(token));
}

#[no_mangle]
pub unsafe extern "C" fn dupdupninja_cancel_token_cancel(token: *mut DupdupCancelToken) {
    ok_last_error();
    if token.is_null() {
        return;
    }
    (*token).token.cancel();
}

#[no_mangle]
pub extern "C" fn dupdupninja_last_error_message() -> *const c_char {
    LAST_ERROR.with(|slot| match &*slot.borrow() {
        Some(msg) => msg.as_ptr(),
        None => std::ptr::null(),
    })
}

#[no_mangle]
pub unsafe extern "C" fn dupdupninja_scan_folder_to_sqlite(
    engine: *mut DupdupEngine,
    root_path: *const c_char,
    db_path: *const c_char,
) -> DupdupStatus {
    ok_last_error();

    if engine.is_null() {
        set_last_error("engine is null");
        return DupdupStatus::NullPointer;
    }
    if root_path.is_null() {
        set_last_error("root_path is null");
        return DupdupStatus::NullPointer;
    }
    if db_path.is_null() {
        set_last_error("db_path is null");
        return DupdupStatus::NullPointer;
    }

    let root_path = match c_path(root_path) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e);
            return DupdupStatus::InvalidArgument;
        }
    };
    let db_path = match c_path(db_path) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e);
            return DupdupStatus::InvalidArgument;
        }
    };

    let store = match SqliteScanStore::open(&db_path) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e.to_string());
            return DupdupStatus::Error;
        }
    };

    let cfg = scan_config_from_options(root_path, default_scan_options(), true);

    match scan_to_sqlite(&cfg, &store) {
        Ok(_) => DupdupStatus::Ok,
        Err(e) => {
            set_last_error(e.to_string());
            DupdupStatus::Error
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn dupdupninja_scan_folder_to_sqlite_with_progress(
    engine: *mut DupdupEngine,
    root_path: *const c_char,
    db_path: *const c_char,
    cancel_token: *mut DupdupCancelToken,
    progress_cb: DupdupProgressCallback,
    user_data: *mut libc::c_void,
) -> DupdupStatus {
    ok_last_error();

    if engine.is_null() {
        set_last_error("engine is null");
        return DupdupStatus::NullPointer;
    }
    if root_path.is_null() {
        set_last_error("root_path is null");
        return DupdupStatus::NullPointer;
    }
    if db_path.is_null() {
        set_last_error("db_path is null");
        return DupdupStatus::NullPointer;
    }

    let root_path = match c_path(root_path) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e);
            return DupdupStatus::InvalidArgument;
        }
    };
    let db_path = match c_path(db_path) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e);
            return DupdupStatus::InvalidArgument;
        }
    };

    let store = match SqliteScanStore::open(&db_path) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e.to_string());
            return DupdupStatus::Error;
        }
    };

    let cfg = scan_config_from_options(root_path, default_scan_options(), true);

    let cancel_ref = if cancel_token.is_null() {
        None
    } else {
        Some(&(*cancel_token).token)
    };

    let result = scan_to_sqlite_with_progress(&cfg, &store, cancel_ref, |progress| {
        if let Some(cb) = progress_cb {
            let path = progress.current_path.to_string_lossy();
            let c_path = CString::new(path.as_ref()).unwrap_or_else(|_| CString::new("").unwrap());
            let c_step = progress
                .current_step
                .as_deref()
                .and_then(|step| CString::new(step).ok());
            let payload = DupdupProgress {
                files_seen: progress.files_seen,
                files_hashed: progress.files_hashed,
                files_skipped: progress.files_skipped,
                bytes_seen: progress.bytes_seen,
                total_files: progress.total_files,
                total_bytes: progress.total_bytes,
                current_path: c_path.as_ptr(),
                current_step: c_step
                    .as_ref()
                    .map(|s| s.as_ptr())
                    .unwrap_or(std::ptr::null()),
            };
            cb(&payload, user_data);
        }
    });

    match result {
        Ok(_) => DupdupStatus::Ok,
        Err(e) => {
            set_last_error(e.to_string());
            DupdupStatus::Error
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn dupdupninja_prescan_folder(
    root_path: *const c_char,
    cancel_token: *mut DupdupCancelToken,
    progress_cb: DupdupPrescanCallback,
    user_data: *mut libc::c_void,
    out_totals: *mut DupdupPrescanTotals,
) -> DupdupStatus {
    ok_last_error();

    if root_path.is_null() {
        set_last_error("root_path is null");
        return DupdupStatus::NullPointer;
    }
    if out_totals.is_null() {
        set_last_error("out_totals is null");
        return DupdupStatus::NullPointer;
    }

    let root_path = match c_path(root_path) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e);
            return DupdupStatus::InvalidArgument;
        }
    };

    let cfg = scan_config_from_options(root_path, default_scan_options(), false);

    let cancel_ref = if cancel_token.is_null() {
        None
    } else {
        Some(&(*cancel_token).token)
    };

    let result = prescan(&cfg, cancel_ref, |progress: &PrescanProgress| {
        if let Some(cb) = progress_cb {
            let path = progress.current_path.to_string_lossy();
            let c_path = CString::new(path.as_ref()).unwrap_or_else(|_| CString::new("").unwrap());
            let payload = DupdupPrescanProgress {
                files_seen: progress.files_seen,
                bytes_seen: progress.bytes_seen,
                dirs_seen: progress.dirs_seen,
                current_path: c_path.as_ptr(),
            };
            cb(&payload, user_data);
        }
    });

    match result {
        Ok(totals) => {
            (*out_totals) = DupdupPrescanTotals {
                total_files: totals.files,
                total_bytes: totals.bytes,
            };
            DupdupStatus::Ok
        }
        Err(e) => {
            set_last_error(e.to_string());
            DupdupStatus::Error
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn dupdupninja_scan_folder_to_sqlite_with_progress_and_totals(
    engine: *mut DupdupEngine,
    root_path: *const c_char,
    db_path: *const c_char,
    cancel_token: *mut DupdupCancelToken,
    total_files: u64,
    total_bytes: u64,
    progress_cb: DupdupProgressCallback,
    user_data: *mut libc::c_void,
) -> DupdupStatus {
    ok_last_error();

    if engine.is_null() {
        set_last_error("engine is null");
        return DupdupStatus::NullPointer;
    }
    if root_path.is_null() {
        set_last_error("root_path is null");
        return DupdupStatus::NullPointer;
    }
    if db_path.is_null() {
        set_last_error("db_path is null");
        return DupdupStatus::NullPointer;
    }

    let root_path = match c_path(root_path) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e);
            return DupdupStatus::InvalidArgument;
        }
    };
    let db_path = match c_path(db_path) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e);
            return DupdupStatus::InvalidArgument;
        }
    };

    let store = match SqliteScanStore::open(&db_path) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e.to_string());
            return DupdupStatus::Error;
        }
    };

    let cfg = scan_config_from_options(root_path, default_scan_options(), true);

    let cancel_ref = if cancel_token.is_null() {
        None
    } else {
        Some(&(*cancel_token).token)
    };

    let totals = ScanTotals {
        files: total_files,
        bytes: total_bytes,
    };

    let result = scan_to_sqlite_with_progress_and_totals(
        &cfg,
        &store,
        cancel_ref,
        Some(totals),
        |progress| {
            if let Some(cb) = progress_cb {
                let path = progress.current_path.to_string_lossy();
                let c_path =
                    CString::new(path.as_ref()).unwrap_or_else(|_| CString::new("").unwrap());
                let c_step = progress
                    .current_step
                    .as_deref()
                    .and_then(|step| CString::new(step).ok());
                let payload = DupdupProgress {
                    files_seen: progress.files_seen,
                    files_hashed: progress.files_hashed,
                    files_skipped: progress.files_skipped,
                    bytes_seen: progress.bytes_seen,
                    total_files: progress.total_files,
                    total_bytes: progress.total_bytes,
                    current_path: c_path.as_ptr(),
                    current_step: c_step
                        .as_ref()
                        .map(|s| s.as_ptr())
                        .unwrap_or(std::ptr::null()),
                };
                cb(&payload, user_data);
            }
        },
    );

    match result {
        Ok(_) => DupdupStatus::Ok,
        Err(e) => {
            set_last_error(e.to_string());
            DupdupStatus::Error
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn dupdupninja_scan_folder_to_sqlite_with_progress_totals_and_options(
    engine: *mut DupdupEngine,
    root_path: *const c_char,
    db_path: *const c_char,
    cancel_token: *mut DupdupCancelToken,
    total_files: u64,
    total_bytes: u64,
    options: *const DupdupScanOptions,
    progress_cb: DupdupProgressCallback,
    user_data: *mut libc::c_void,
) -> DupdupStatus {
    ok_last_error();

    if engine.is_null() {
        set_last_error("engine is null");
        return DupdupStatus::NullPointer;
    }
    if root_path.is_null() {
        set_last_error("root_path is null");
        return DupdupStatus::NullPointer;
    }
    if db_path.is_null() {
        set_last_error("db_path is null");
        return DupdupStatus::NullPointer;
    }
    if options.is_null() {
        set_last_error("options is null");
        return DupdupStatus::NullPointer;
    }

    let root_path = match c_path(root_path) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e);
            return DupdupStatus::InvalidArgument;
        }
    };
    let db_path = match c_path(db_path) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e);
            return DupdupStatus::InvalidArgument;
        }
    };

    let store = match SqliteScanStore::open(&db_path) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e.to_string());
            return DupdupStatus::Error;
        }
    };

    let cfg = scan_config_from_options(root_path, *options, true);

    let cancel_ref = if cancel_token.is_null() {
        None
    } else {
        Some(&(*cancel_token).token)
    };

    let totals = ScanTotals {
        files: total_files,
        bytes: total_bytes,
    };

    let result = scan_to_sqlite_with_progress_and_totals(
        &cfg,
        &store,
        cancel_ref,
        Some(totals),
        |progress| {
            if let Some(cb) = progress_cb {
                let path = progress.current_path.to_string_lossy();
                let c_path =
                    CString::new(path.as_ref()).unwrap_or_else(|_| CString::new("").unwrap());
                let c_step = progress
                    .current_step
                    .as_deref()
                    .and_then(|step| CString::new(step).ok());
                let payload = DupdupProgress {
                    files_seen: progress.files_seen,
                    files_hashed: progress.files_hashed,
                    files_skipped: progress.files_skipped,
                    bytes_seen: progress.bytes_seen,
                    total_files: progress.total_files,
                    total_bytes: progress.total_bytes,
                    current_path: c_path.as_ptr(),
                    current_step: c_step
                        .as_ref()
                        .map(|s| s.as_ptr())
                        .unwrap_or(std::ptr::null()),
                };
                cb(&payload, user_data);
            }
        },
    );

    match result {
        Ok(_) => DupdupStatus::Ok,
        Err(e) => {
            set_last_error(e.to_string());
            DupdupStatus::Error
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn dupdupninja_fileset_list_rows(
    db_path: *const c_char,
    duplicates_only: bool,
    limit: u64,
    offset: u64,
    out_rows: *mut *mut DupdupFilesetRow,
    out_len: *mut usize,
) -> DupdupStatus {
    ok_last_error();

    if db_path.is_null() {
        set_last_error("db_path is null");
        return DupdupStatus::NullPointer;
    }
    if out_rows.is_null() {
        set_last_error("out_rows is null");
        return DupdupStatus::NullPointer;
    }
    if out_len.is_null() {
        set_last_error("out_len is null");
        return DupdupStatus::NullPointer;
    }

    let db_path = match c_path(db_path) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e);
            return DupdupStatus::InvalidArgument;
        }
    };
    let store = match SqliteScanStore::open(&db_path) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e.to_string());
            return DupdupStatus::Error;
        }
    };

    let limit = (limit.min(10_000)) as usize;
    let offset = (offset.min(10_000_000)) as usize;
    let rows = match if duplicates_only {
        store.list_files_with_duplicates(limit, offset)
    } else {
        store.list_files(limit, offset)
    } {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e.to_string());
            return DupdupStatus::Error;
        }
    };

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(DupdupFilesetRow {
            id: row.id,
            path: string_to_c_owned(row.path.to_string_lossy().as_ref()),
            size_bytes: row.size_bytes,
            file_type: string_to_c_owned(row.file_type.as_deref().unwrap_or("")),
            blake3_hex: string_to_c_owned(&hash_to_hex_opt(row.blake3.as_ref())),
            sha256_hex: string_to_c_owned(&hash_to_hex_opt(row.sha256.as_ref())),
        });
    }

    if out.is_empty() {
        *out_rows = std::ptr::null_mut();
        *out_len = 0;
        return DupdupStatus::Ok;
    }

    let mut boxed = out.into_boxed_slice();
    *out_len = boxed.len();
    *out_rows = boxed.as_mut_ptr();
    std::mem::forget(boxed);
    DupdupStatus::Ok
}

#[no_mangle]
pub unsafe extern "C" fn dupdupninja_fileset_list_similar_groups(
    db_path: *const c_char,
    limit: u64,
    offset: u64,
    phash_max_distance: u8,
    dhash_max_distance: u8,
    ahash_max_distance: u8,
    out_groups: *mut *mut DupdupSimilarGroup,
    out_groups_len: *mut usize,
    out_rows: *mut *mut DupdupSimilarRow,
    out_rows_len: *mut usize,
) -> DupdupStatus {
    ok_last_error();

    if db_path.is_null() {
        set_last_error("db_path is null");
        return DupdupStatus::NullPointer;
    }
    if out_groups.is_null() {
        set_last_error("out_groups is null");
        return DupdupStatus::NullPointer;
    }
    if out_groups_len.is_null() {
        set_last_error("out_groups_len is null");
        return DupdupStatus::NullPointer;
    }
    if out_rows.is_null() {
        set_last_error("out_rows is null");
        return DupdupStatus::NullPointer;
    }
    if out_rows_len.is_null() {
        set_last_error("out_rows_len is null");
        return DupdupStatus::NullPointer;
    }

    let db_path = match c_path(db_path) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e);
            return DupdupStatus::InvalidArgument;
        }
    };
    let store = match SqliteScanStore::open(&db_path) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e.to_string());
            return DupdupStatus::Error;
        }
    };

    let limit = (limit.min(2_000)) as usize;
    let offset = (offset.min(10_000_000)) as usize;
    let rows = match store.list_files_with_hashes(limit, offset) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e.to_string());
            return DupdupStatus::Error;
        }
    };

    let phash_threshold = phash_max_distance.clamp(1, 32) as u32;
    let dhash_threshold = dhash_max_distance.clamp(1, 32) as u32;
    let ahash_threshold = ahash_max_distance.clamp(1, 32) as u32;
    let mut used = vec![false; rows.len()];
    let mut groups = Vec::new();
    let mut members = Vec::new();

    for i in 0..rows.len() {
        if used[i] {
            continue;
        }
        let Some(base_phash) = rows[i].phash else {
            continue;
        };

        let mut indices = Vec::new();
        indices.push(i);
        for j in (i + 1)..rows.len() {
            if used[j] {
                continue;
            }
            let Some(other_phash) = rows[j].phash else {
                continue;
            };
            let dist = hamming64(base_phash, other_phash);
            let dhash_ok = match (rows[i].dhash, rows[j].dhash) {
                (Some(a), Some(b)) => hamming64(a, b) <= dhash_threshold,
                _ => true,
            };
            let ahash_ok = match (rows[i].ahash, rows[j].ahash) {
                (Some(a), Some(b)) => hamming64(a, b) <= ahash_threshold,
                _ => true,
            };
            if dist <= phash_threshold && dhash_ok && ahash_ok {
                indices.push(j);
            }
        }

        if indices.len() < 2 {
            continue;
        }

        let rows_start = members.len();
        let base = &rows[i];
        for idx in indices {
            used[idx] = true;
            let row = &rows[idx];
            let phash_distance = row
                .phash
                .map(|v| hamming64(base_phash, v) as u8)
                .unwrap_or(64);
            let dhash_distance = match (base.dhash, row.dhash) {
                (Some(a), Some(b)) => hamming64(a, b) as u8,
                _ => 64,
            };
            let ahash_distance = match (base.ahash, row.ahash) {
                (Some(a), Some(b)) => hamming64(a, b) as u8,
                _ => 64,
            };
            let confidence = similar_confidence_percent(phash_distance as u32);
            members.push(file_list_row_to_similar_ffi(
                row,
                phash_distance,
                dhash_distance,
                ahash_distance,
                confidence,
            ));
        }

        let rows_len = members.len() - rows_start;
        let base_name = base
            .path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("(unknown)");
        let label = format!("Similar group ({rows_len} files) - {base_name}");
        groups.push(DupdupSimilarGroup {
            label: string_to_c_owned(&label),
            rows_start,
            rows_len,
        });
    }

    if groups.is_empty() {
        *out_groups = std::ptr::null_mut();
        *out_groups_len = 0;
        *out_rows = std::ptr::null_mut();
        *out_rows_len = 0;
        return DupdupStatus::Ok;
    }

    let mut groups_boxed = groups.into_boxed_slice();
    let mut rows_boxed = members.into_boxed_slice();
    *out_groups_len = groups_boxed.len();
    *out_groups = groups_boxed.as_mut_ptr();
    *out_rows_len = rows_boxed.len();
    *out_rows = rows_boxed.as_mut_ptr();
    std::mem::forget(groups_boxed);
    std::mem::forget(rows_boxed);
    DupdupStatus::Ok
}

#[no_mangle]
pub unsafe extern "C" fn dupdupninja_fileset_rows_free(rows: *mut DupdupFilesetRow, len: usize) {
    ok_last_error();
    if rows.is_null() {
        return;
    }

    let rows_slice = slice::from_raw_parts_mut(rows, len);
    for row in rows_slice.iter_mut() {
        free_owned_c_string(row.path);
        row.path = std::ptr::null_mut();
        free_owned_c_string(row.file_type);
        row.file_type = std::ptr::null_mut();
        free_owned_c_string(row.blake3_hex);
        row.blake3_hex = std::ptr::null_mut();
        free_owned_c_string(row.sha256_hex);
        row.sha256_hex = std::ptr::null_mut();
    }

    let _ = Box::from_raw(rows_slice as *mut [DupdupFilesetRow]);
}

#[no_mangle]
pub unsafe extern "C" fn dupdupninja_similar_rows_free(rows: *mut DupdupSimilarRow, len: usize) {
    ok_last_error();
    if rows.is_null() {
        return;
    }

    let rows_slice = slice::from_raw_parts_mut(rows, len);
    for row in rows_slice.iter_mut() {
        free_owned_c_string(row.path);
        row.path = std::ptr::null_mut();
        free_owned_c_string(row.file_type);
        row.file_type = std::ptr::null_mut();
        free_owned_c_string(row.blake3_hex);
        row.blake3_hex = std::ptr::null_mut();
        free_owned_c_string(row.sha256_hex);
        row.sha256_hex = std::ptr::null_mut();
    }

    let _ = Box::from_raw(rows_slice as *mut [DupdupSimilarRow]);
}

#[no_mangle]
pub unsafe extern "C" fn dupdupninja_fileset_list_exact_groups(
    db_path: *const c_char,
    limit: u64,
    offset: u64,
    out_groups: *mut *mut DupdupExactGroup,
    out_groups_len: *mut usize,
    out_rows: *mut *mut DupdupFilesetRow,
    out_rows_len: *mut usize,
) -> DupdupStatus {
    ok_last_error();

    if db_path.is_null() {
        set_last_error("db_path is null");
        return DupdupStatus::NullPointer;
    }
    if out_groups.is_null() {
        set_last_error("out_groups is null");
        return DupdupStatus::NullPointer;
    }
    if out_groups_len.is_null() {
        set_last_error("out_groups_len is null");
        return DupdupStatus::NullPointer;
    }
    if out_rows.is_null() {
        set_last_error("out_rows is null");
        return DupdupStatus::NullPointer;
    }
    if out_rows_len.is_null() {
        set_last_error("out_rows_len is null");
        return DupdupStatus::NullPointer;
    }

    let db_path = match c_path(db_path) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e);
            return DupdupStatus::InvalidArgument;
        }
    };
    let store = match SqliteScanStore::open(&db_path) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e.to_string());
            return DupdupStatus::Error;
        }
    };

    let limit = (limit.min(10_000)) as usize;
    let offset = (offset.min(10_000_000)) as usize;
    let rows = match store.list_files_with_duplicates(limit, offset) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e.to_string());
            return DupdupStatus::Error;
        }
    };

    let mut grouped: BTreeMap<String, Vec<dupdupninja_core::models::FileListRow>> = BTreeMap::new();
    for row in rows {
        let group_key = if let Some(hash) = row.blake3.as_ref() {
            format!("blake3:{}", hash_to_hex_opt(Some(hash)))
        } else if let Some(hash) = row.sha256.as_ref() {
            format!("sha256:{}", hash_to_hex_opt(Some(hash)))
        } else {
            continue;
        };
        grouped.entry(group_key).or_default().push(row);
    }

    let mut group_records = Vec::new();
    let mut row_records = Vec::new();

    for (key, members) in grouped {
        if members.len() < 2 {
            continue;
        }

        let rows_start = row_records.len();
        for row in members {
            row_records.push(file_list_row_to_ffi(row));
        }
        let rows_len = row_records.len() - rows_start;

        let short_hash = key
            .split_once(':')
            .map(|(_, h)| h)
            .unwrap_or("")
            .chars()
            .take(12)
            .collect::<String>();
        let label = format!("Exact group ({rows_len} files) - {short_hash}");
        group_records.push(DupdupExactGroup {
            label: string_to_c_owned(&label),
            rows_start,
            rows_len,
        });
    }

    if group_records.is_empty() {
        *out_groups = std::ptr::null_mut();
        *out_groups_len = 0;
        *out_rows = std::ptr::null_mut();
        *out_rows_len = 0;
        return DupdupStatus::Ok;
    }

    let mut groups_boxed = group_records.into_boxed_slice();
    let mut rows_boxed = row_records.into_boxed_slice();
    *out_groups_len = groups_boxed.len();
    *out_groups = groups_boxed.as_mut_ptr();
    *out_rows_len = rows_boxed.len();
    *out_rows = rows_boxed.as_mut_ptr();
    std::mem::forget(groups_boxed);
    std::mem::forget(rows_boxed);
    DupdupStatus::Ok
}

#[no_mangle]
pub unsafe extern "C" fn dupdupninja_exact_groups_free(groups: *mut DupdupExactGroup, len: usize) {
    ok_last_error();
    if groups.is_null() {
        return;
    }

    let groups_slice = slice::from_raw_parts_mut(groups, len);
    for group in groups_slice.iter_mut() {
        free_owned_c_string(group.label);
        group.label = std::ptr::null_mut();
    }

    let _ = Box::from_raw(groups_slice as *mut [DupdupExactGroup]);
}

#[no_mangle]
pub unsafe extern "C" fn dupdupninja_similar_groups_free(
    groups: *mut DupdupSimilarGroup,
    len: usize,
) {
    ok_last_error();
    if groups.is_null() {
        return;
    }

    let groups_slice = slice::from_raw_parts_mut(groups, len);
    for group in groups_slice.iter_mut() {
        free_owned_c_string(group.label);
        group.label = std::ptr::null_mut();
    }

    let _ = Box::from_raw(groups_slice as *mut [DupdupSimilarGroup]);
}

#[no_mangle]
pub unsafe extern "C" fn dupdupninja_fileset_get_metadata(
    db_path: *const c_char,
    out_meta: *mut DupdupFilesetMetadataView,
) -> DupdupStatus {
    ok_last_error();
    if db_path.is_null() {
        set_last_error("db_path is null");
        return DupdupStatus::NullPointer;
    }
    if out_meta.is_null() {
        set_last_error("out_meta is null");
        return DupdupStatus::NullPointer;
    }

    let db_path = match c_path(db_path) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e);
            return DupdupStatus::InvalidArgument;
        }
    };
    let store = match SqliteScanStore::open(&db_path) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e.to_string());
            return DupdupStatus::Error;
        }
    };

    let meta = match store.get_fileset_metadata() {
        Ok(Some(v)) => v,
        Ok(None) => FilesetMetadata {
            created_at: std::time::SystemTime::now(),
            root_kind: ScanRootKind::Folder,
            root_path: PathBuf::new(),
            root_parent_path: None,
            drive: DriveMetadata {
                id: None,
                label: None,
                fs_type: None,
            },
            host_os: String::new(),
            host_os_version: String::new(),
            app_version: String::new(),
            status: String::new(),
            name: String::new(),
            description: String::new(),
            notes: String::new(),
        },
        Err(e) => {
            set_last_error(e.to_string());
            return DupdupStatus::Error;
        }
    };

    *out_meta = DupdupFilesetMetadataView {
        name: string_to_c_owned(&meta.name),
        description: string_to_c_owned(&meta.description),
        notes: string_to_c_owned(&meta.notes),
        status: string_to_c_owned(&meta.status),
    };
    DupdupStatus::Ok
}

#[no_mangle]
pub unsafe extern "C" fn dupdupninja_fileset_set_metadata(
    db_path: *const c_char,
    name: *const c_char,
    description: *const c_char,
    notes: *const c_char,
    status: *const c_char,
) -> DupdupStatus {
    ok_last_error();
    if db_path.is_null() {
        set_last_error("db_path is null");
        return DupdupStatus::NullPointer;
    }

    let db_path = match c_path(db_path) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e);
            return DupdupStatus::InvalidArgument;
        }
    };
    let store = match SqliteScanStore::open(&db_path) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e.to_string());
            return DupdupStatus::Error;
        }
    };
    let mut meta = match store.get_fileset_metadata() {
        Ok(Some(v)) => v,
        Ok(None) => FilesetMetadata {
            created_at: std::time::SystemTime::now(),
            root_kind: ScanRootKind::Folder,
            root_path: PathBuf::new(),
            root_parent_path: None,
            drive: DriveMetadata {
                id: None,
                label: None,
                fs_type: None,
            },
            host_os: String::new(),
            host_os_version: String::new(),
            app_version: String::new(),
            status: String::new(),
            name: String::new(),
            description: String::new(),
            notes: String::new(),
        },
        Err(e) => {
            set_last_error(e.to_string());
            return DupdupStatus::Error;
        }
    };

    meta.name = c_string_opt(name).unwrap_or_default();
    meta.description = c_string_opt(description).unwrap_or_default();
    meta.notes = c_string_opt(notes).unwrap_or_default();
    meta.status = c_string_opt(status).unwrap_or_default();

    match store.set_fileset_metadata(&meta) {
        Ok(()) => DupdupStatus::Ok,
        Err(e) => {
            set_last_error(e.to_string());
            DupdupStatus::Error
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn dupdupninja_fileset_metadata_free(meta: *mut DupdupFilesetMetadataView) {
    ok_last_error();
    if meta.is_null() {
        return;
    }
    free_owned_c_string((*meta).name);
    (*meta).name = std::ptr::null_mut();
    free_owned_c_string((*meta).description);
    (*meta).description = std::ptr::null_mut();
    free_owned_c_string((*meta).notes);
    (*meta).notes = std::ptr::null_mut();
    free_owned_c_string((*meta).status);
    (*meta).status = std::ptr::null_mut();
}

#[no_mangle]
pub unsafe extern "C" fn dupdupninja_fileset_delete_file_by_path(
    db_path: *const c_char,
    file_path: *const c_char,
) -> DupdupStatus {
    ok_last_error();
    if db_path.is_null() {
        set_last_error("db_path is null");
        return DupdupStatus::NullPointer;
    }
    if file_path.is_null() {
        set_last_error("file_path is null");
        return DupdupStatus::NullPointer;
    }

    let db_path = match c_path(db_path) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e);
            return DupdupStatus::InvalidArgument;
        }
    };
    let file_path = match c_path(file_path) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e);
            return DupdupStatus::InvalidArgument;
        }
    };

    let store = match SqliteScanStore::open(&db_path) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e.to_string());
            return DupdupStatus::Error;
        }
    };
    match store.delete_file_by_path(&file_path) {
        Ok(_) => DupdupStatus::Ok,
        Err(e) => {
            set_last_error(e.to_string());
            DupdupStatus::Error
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn dupdupninja_fileset_list_snapshots_by_path(
    db_path: *const c_char,
    file_path: *const c_char,
    out_rows: *mut *mut DupdupSnapshotInfo,
    out_len: *mut usize,
) -> DupdupStatus {
    ok_last_error();
    if db_path.is_null() {
        set_last_error("db_path is null");
        return DupdupStatus::NullPointer;
    }
    if file_path.is_null() {
        set_last_error("file_path is null");
        return DupdupStatus::NullPointer;
    }
    if out_rows.is_null() {
        set_last_error("out_rows is null");
        return DupdupStatus::NullPointer;
    }
    if out_len.is_null() {
        set_last_error("out_len is null");
        return DupdupStatus::NullPointer;
    }

    let db_path = match c_path(db_path) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e);
            return DupdupStatus::InvalidArgument;
        }
    };
    let file_path = match c_path(file_path) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e);
            return DupdupStatus::InvalidArgument;
        }
    };

    let store = match SqliteScanStore::open(&db_path) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e.to_string());
            return DupdupStatus::Error;
        }
    };

    let file = match store.get_file_by_path(&file_path) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e.to_string());
            return DupdupStatus::Error;
        }
    };
    let Some(file) = file else {
        *out_rows = std::ptr::null_mut();
        *out_len = 0;
        return DupdupStatus::Ok;
    };
    let Some(file_id) = file.file_id else {
        *out_rows = std::ptr::null_mut();
        *out_len = 0;
        return DupdupStatus::Ok;
    };

    let snapshots = match store.list_file_snapshots(file_id) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e.to_string());
            return DupdupStatus::Error;
        }
    };

    if snapshots.is_empty() {
        *out_rows = std::ptr::null_mut();
        *out_len = 0;
        return DupdupStatus::Ok;
    }

    let mut out = Vec::with_capacity(snapshots.len());
    for snap in snapshots {
        out.push(DupdupSnapshotInfo {
            snapshot_index: snap.snapshot_index,
            snapshot_count: snap.snapshot_count,
            at_ms: snap.at_ms,
            has_duration: if snap.duration_ms.is_some() { 1 } else { 0 },
            duration_ms: snap.duration_ms.unwrap_or_default(),
            has_ahash: if snap.ahash.is_some() { 1 } else { 0 },
            ahash: snap.ahash.unwrap_or_default(),
            has_dhash: if snap.dhash.is_some() { 1 } else { 0 },
            dhash: snap.dhash.unwrap_or_default(),
            has_phash: if snap.phash.is_some() { 1 } else { 0 },
            phash: snap.phash.unwrap_or_default(),
        });
    }

    let mut boxed = out.into_boxed_slice();
    *out_len = boxed.len();
    *out_rows = boxed.as_mut_ptr();
    std::mem::forget(boxed);
    DupdupStatus::Ok
}

#[no_mangle]
pub unsafe extern "C" fn dupdupninja_snapshots_info_free(
    rows: *mut DupdupSnapshotInfo,
    len: usize,
) {
    ok_last_error();
    if rows.is_null() {
        return;
    }
    let rows_slice = slice::from_raw_parts_mut(rows, len);
    let _ = Box::from_raw(rows_slice as *mut [DupdupSnapshotInfo]);
}

unsafe fn c_path(ptr: *const c_char) -> Result<PathBuf, String> {
    let s = CStr::from_ptr(ptr)
        .to_str()
        .map_err(|_| "string is not valid UTF-8".to_string())?;
    if s.is_empty() {
        return Err("string is empty".to_string());
    }
    Ok(PathBuf::from(s))
}

unsafe fn c_string_opt(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    CStr::from_ptr(ptr).to_str().ok().map(str::to_owned)
}

fn default_scan_options() -> DupdupScanOptions {
    DupdupScanOptions {
        capture_snapshots: true,
        snapshots_per_video: 3,
        snapshot_max_dim: 1024,
    }
}

fn scan_config_from_options(
    root: PathBuf,
    options: DupdupScanOptions,
    hash_files: bool,
) -> ScanConfig {
    ScanConfig {
        root,
        root_kind: ScanRootKind::Folder,
        hash_files,
        perceptual_hashes: true,
        capture_snapshots: options.capture_snapshots,
        snapshots_per_video: options.snapshots_per_video.clamp(1, 10),
        snapshot_max_dim: options.snapshot_max_dim.clamp(128, 4096),
    }
}

fn hash_to_hex_opt(hash: Option<&[u8; 32]>) -> String {
    match hash {
        Some(bytes) => bytes.iter().map(|b| format!("{b:02x}")).collect(),
        None => String::new(),
    }
}

fn file_list_row_to_ffi(row: dupdupninja_core::models::FileListRow) -> DupdupFilesetRow {
    DupdupFilesetRow {
        id: row.id,
        path: string_to_c_owned(row.path.to_string_lossy().as_ref()),
        size_bytes: row.size_bytes,
        file_type: string_to_c_owned(row.file_type.as_deref().unwrap_or("")),
        blake3_hex: string_to_c_owned(&hash_to_hex_opt(row.blake3.as_ref())),
        sha256_hex: string_to_c_owned(&hash_to_hex_opt(row.sha256.as_ref())),
    }
}

fn file_list_row_to_similar_ffi(
    row: &dupdupninja_core::models::FileListRow,
    phash_distance: u8,
    dhash_distance: u8,
    ahash_distance: u8,
    confidence_percent: f32,
) -> DupdupSimilarRow {
    DupdupSimilarRow {
        id: row.id,
        path: string_to_c_owned(row.path.to_string_lossy().as_ref()),
        size_bytes: row.size_bytes,
        file_type: string_to_c_owned(row.file_type.as_deref().unwrap_or("")),
        blake3_hex: string_to_c_owned(&hash_to_hex_opt(row.blake3.as_ref())),
        sha256_hex: string_to_c_owned(&hash_to_hex_opt(row.sha256.as_ref())),
        phash_distance,
        dhash_distance,
        ahash_distance,
        confidence_percent,
    }
}

fn hamming64(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

fn similar_confidence_percent(phash_distance: u32) -> f32 {
    let similarity = ((64_u32.saturating_sub(phash_distance)) as f32 / 64.0) * 100.0;
    similarity.min(99.99).max(0.0)
}

fn string_to_c_owned(text: &str) -> *mut c_char {
    CString::new(text)
        .unwrap_or_else(|_| CString::new("").unwrap())
        .into_raw()
}

unsafe fn free_owned_c_string(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    let _ = CString::from_raw(ptr);
}
