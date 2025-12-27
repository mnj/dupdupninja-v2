#![allow(unsafe_code)]

use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::PathBuf;

use dupdupninja_core::db::SqliteScanStore;
use dupdupninja_core::models::ScanRootKind;
use dupdupninja_core::scan::{scan_to_sqlite, scan_to_sqlite_with_progress, ScanCancelToken, ScanConfig};

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
    pub current_path: *const c_char,
}

pub type DupdupProgressCallback = Option<extern "C" fn(progress: *const DupdupProgress, user_data: *mut libc::c_void)>;

const FFI_ABI_MAJOR: u32 = 1;
const FFI_ABI_MINOR: u32 = 0;
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

    let cfg = ScanConfig {
        root: root_path,
        root_kind: ScanRootKind::Folder,
        hash_files: true,
    };

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

    let cfg = ScanConfig {
        root: root_path,
        root_kind: ScanRootKind::Folder,
        hash_files: true,
    };

    let cancel_ref = if cancel_token.is_null() {
        None
    } else {
        Some(&(*cancel_token).token)
    };

    let result = scan_to_sqlite_with_progress(&cfg, &store, cancel_ref, |progress| {
        if let Some(cb) = progress_cb {
            let path = progress.current_path.to_string_lossy();
            let c_path = CString::new(path.as_ref()).unwrap_or_else(|_| CString::new("").unwrap());
            let payload = DupdupProgress {
                files_seen: progress.files_seen,
                files_hashed: progress.files_hashed,
                files_skipped: progress.files_skipped,
                bytes_seen: progress.bytes_seen,
                current_path: c_path.as_ptr(),
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

unsafe fn c_path(ptr: *const c_char) -> Result<PathBuf, String> {
    let s = CStr::from_ptr(ptr)
        .to_str()
        .map_err(|_| "string is not valid UTF-8".to_string())?;
    if s.is_empty() {
        return Err("string is empty".to_string());
    }
    Ok(PathBuf::from(s))
}
