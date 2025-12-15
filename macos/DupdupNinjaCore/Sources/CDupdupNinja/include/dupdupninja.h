// SwiftPM vendored header for the Rust C ABI.
// Keep this file in sync with `crates/ffi/include/dupdupninja.h`.

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

DupdupEngine* dupdupninja_engine_new(void);
void dupdupninja_engine_free(DupdupEngine* engine);

const char* dupdupninja_last_error_message(void);

DupdupStatus dupdupninja_scan_folder_to_sqlite(
  DupdupEngine* engine,
  const char* root_path,
  const char* db_path
);

#ifdef __cplusplus
} // extern "C"
#endif
