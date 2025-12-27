// Generated interface (initial skeleton). Keep in sync with `crates/ffi/src/lib.rs`.
// Recommended: use a header generator (e.g. cbindgen) once the API stabilizes.

#pragma once

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct DupdupEngine DupdupEngine;

enum {
  DUPDUPNINJA_FFI_ABI_MAJOR = 1,
  DUPDUPNINJA_FFI_ABI_MINOR = 0,
  DUPDUPNINJA_FFI_ABI_PATCH = 0,
};

typedef struct DupdupNinjaVersion {
  uint32_t major;
  uint32_t minor;
  uint32_t patch;
} DupdupNinjaVersion;

typedef enum DupdupStatus {
  DUPDUP_STATUS_OK = 0,
  DUPDUP_STATUS_ERROR = 1,
  DUPDUP_STATUS_INVALID_ARGUMENT = 2,
  DUPDUP_STATUS_NULL_POINTER = 3,
} DupdupStatus;

// Returns the FFI library version (semantic version).
DupdupNinjaVersion dupdupninja_ffi_version(void);

// Returns the ABI major version used for compatibility checks.
uint32_t dupdupninja_ffi_abi_major(void);

DupdupEngine* dupdupninja_engine_new(void);
void dupdupninja_engine_free(DupdupEngine* engine);

// Returns a pointer to a thread-local, nul-terminated error message string for the last error.
// The pointer becomes invalid after the next dupdupninja call on the same thread.
const char* dupdupninja_last_error_message(void);

DupdupStatus dupdupninja_scan_folder_to_sqlite(
  DupdupEngine* engine,
  const char* root_path,
  const char* db_path
);

#ifdef __cplusplus
} // extern "C"
#endif
