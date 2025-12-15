// Generated interface (initial skeleton). Keep in sync with `crates/ffi/src/lib.rs`.
// Recommended: use a header generator (e.g. cbindgen) once the API stabilizes.

#pragma once

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct DupdupEngine DupdupEngine;

typedef enum DupdupStatus {
  DUPDUP_STATUS_OK = 0,
  DUPDUP_STATUS_ERROR = 1,
  DUPDUP_STATUS_INVALID_ARGUMENT = 2,
  DUPDUP_STATUS_NULL_POINTER = 3,
} DupdupStatus;

DupdupEngine* dupdup_engine_new(void);
void dupdup_engine_free(DupdupEngine* engine);

// Returns a pointer to a thread-local, nul-terminated error message string for the last error.
// The pointer becomes invalid after the next dupdup call on the same thread.
const char* dupdup_last_error_message(void);

DupdupStatus dupdup_scan_folder_to_sqlite(
  DupdupEngine* engine,
  const char* root_path,
  const char* db_path
);

#ifdef __cplusplus
} // extern "C"
#endif

