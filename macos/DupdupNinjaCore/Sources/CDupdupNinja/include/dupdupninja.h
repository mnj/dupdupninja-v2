// Generated interface (initial skeleton). Keep in sync with `crates/ffi/src/lib.rs`.
// Recommended: use a header generator (e.g. cbindgen) once the API stabilizes.

#pragma once

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct DupdupEngine DupdupEngine;
typedef struct DupdupCancelToken DupdupCancelToken;

enum {
  DUPDUPNINJA_FFI_ABI_MAJOR = 1,
  DUPDUPNINJA_FFI_ABI_MINOR = 3,
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

typedef struct DupdupProgress {
  uint64_t files_seen;
  uint64_t files_hashed;
  uint64_t files_skipped;
  uint64_t bytes_seen;
  uint64_t total_files;
  uint64_t total_bytes;
  const char* current_path;
  const char* current_step;
} DupdupProgress;

typedef void (*DupdupProgressCallback)(const DupdupProgress* progress, void* user_data);

typedef struct DupdupPrescanTotals {
  uint64_t total_files;
  uint64_t total_bytes;
} DupdupPrescanTotals;

typedef struct DupdupPrescanProgress {
  uint64_t files_seen;
  uint64_t bytes_seen;
  uint64_t dirs_seen;
  const char* current_path;
} DupdupPrescanProgress;

typedef void (*DupdupPrescanCallback)(const DupdupPrescanProgress* progress, void* user_data);

typedef struct DupdupScanOptions {
  uint8_t capture_snapshots;
  uint32_t snapshots_per_video;
  uint32_t snapshot_max_dim;
} DupdupScanOptions;

typedef struct DupdupFilesetRow {
  int64_t id;
  char* path;
  uint64_t size_bytes;
  char* file_type;
  char* blake3_hex;
  char* sha256_hex;
} DupdupFilesetRow;

typedef struct DupdupExactGroup {
  char* label;
  uintptr_t rows_start;
  uintptr_t rows_len;
} DupdupExactGroup;

typedef struct DupdupSimilarGroup {
  char* label;
  uintptr_t rows_start;
  uintptr_t rows_len;
} DupdupSimilarGroup;

typedef struct DupdupSimilarRow {
  int64_t id;
  char* path;
  uint64_t size_bytes;
  char* file_type;
  char* blake3_hex;
  char* sha256_hex;
  uint8_t phash_distance;
  uint8_t dhash_distance;
  uint8_t ahash_distance;
  float confidence_percent;
} DupdupSimilarRow;

typedef struct DupdupFilesetMetadataView {
  char* name;
  char* description;
  char* notes;
  char* status;
} DupdupFilesetMetadataView;

typedef struct DupdupSnapshotInfo {
  uint32_t snapshot_index;
  uint32_t snapshot_count;
  int64_t at_ms;
  uint8_t has_duration;
  int64_t duration_ms;
  uint8_t has_ahash;
  uint64_t ahash;
  uint8_t has_dhash;
  uint64_t dhash;
  uint8_t has_phash;
  uint64_t phash;
} DupdupSnapshotInfo;

// Returns the FFI library version (semantic version).
DupdupNinjaVersion dupdupninja_ffi_version(void);

// Returns the ABI major version used for compatibility checks.
uint32_t dupdupninja_ffi_abi_major(void);

DupdupEngine* dupdupninja_engine_new(void);
void dupdupninja_engine_free(DupdupEngine* engine);

DupdupCancelToken* dupdupninja_cancel_token_new(void);
void dupdupninja_cancel_token_free(DupdupCancelToken* token);
void dupdupninja_cancel_token_cancel(DupdupCancelToken* token);

// Returns a pointer to a thread-local, nul-terminated error message string for the last error.
// The pointer becomes invalid after the next dupdupninja call on the same thread.
const char* dupdupninja_last_error_message(void);

DupdupStatus dupdupninja_scan_folder_to_sqlite(
  DupdupEngine* engine,
  const char* root_path,
  const char* db_path
);

// Progress callback is invoked from the scanning thread. current_path is only valid
// for the duration of the callback.
DupdupStatus dupdupninja_scan_folder_to_sqlite_with_progress(
  DupdupEngine* engine,
  const char* root_path,
  const char* db_path,
  DupdupCancelToken* cancel_token,
  DupdupProgressCallback progress_cb,
  void* user_data
);

DupdupStatus dupdupninja_prescan_folder(
  const char* root_path,
  DupdupCancelToken* cancel_token,
  DupdupPrescanCallback progress_cb,
  void* user_data,
  DupdupPrescanTotals* out_totals
);

DupdupStatus dupdupninja_scan_folder_to_sqlite_with_progress_and_totals(
  DupdupEngine* engine,
  const char* root_path,
  const char* db_path,
  DupdupCancelToken* cancel_token,
  uint64_t total_files,
  uint64_t total_bytes,
  DupdupProgressCallback progress_cb,
  void* user_data
);

DupdupStatus dupdupninja_scan_folder_to_sqlite_with_progress_totals_and_options(
  DupdupEngine* engine,
  const char* root_path,
  const char* db_path,
  DupdupCancelToken* cancel_token,
  uint64_t total_files,
  uint64_t total_bytes,
  const DupdupScanOptions* options,
  DupdupProgressCallback progress_cb,
  void* user_data
);

DupdupStatus dupdupninja_fileset_list_rows(
  const char* db_path,
  uint8_t duplicates_only,
  uint64_t limit,
  uint64_t offset,
  DupdupFilesetRow** out_rows,
  uintptr_t* out_len
);

DupdupStatus dupdupninja_fileset_list_similar_groups(
  const char* db_path,
  uint64_t limit,
  uint64_t offset,
  uint8_t phash_max_distance,
  uint8_t dhash_max_distance,
  uint8_t ahash_max_distance,
  DupdupSimilarGroup** out_groups,
  uintptr_t* out_groups_len,
  DupdupSimilarRow** out_rows,
  uintptr_t* out_rows_len
);

void dupdupninja_fileset_rows_free(DupdupFilesetRow* rows, uintptr_t len);
void dupdupninja_similar_rows_free(DupdupSimilarRow* rows, uintptr_t len);

DupdupStatus dupdupninja_fileset_list_exact_groups(
  const char* db_path,
  uint64_t limit,
  uint64_t offset,
  DupdupExactGroup** out_groups,
  uintptr_t* out_groups_len,
  DupdupFilesetRow** out_rows,
  uintptr_t* out_rows_len
);

void dupdupninja_exact_groups_free(DupdupExactGroup* groups, uintptr_t len);
void dupdupninja_similar_groups_free(DupdupSimilarGroup* groups, uintptr_t len);

DupdupStatus dupdupninja_fileset_get_metadata(
  const char* db_path,
  DupdupFilesetMetadataView* out_meta
);

DupdupStatus dupdupninja_fileset_set_metadata(
  const char* db_path,
  const char* name,
  const char* description,
  const char* notes,
  const char* status
);

void dupdupninja_fileset_metadata_free(DupdupFilesetMetadataView* meta);

DupdupStatus dupdupninja_fileset_delete_file_by_path(
  const char* db_path,
  const char* file_path
);

DupdupStatus dupdupninja_fileset_list_snapshots_by_path(
  const char* db_path,
  const char* file_path,
  DupdupSnapshotInfo** out_rows,
  uintptr_t* out_len
);

void dupdupninja_snapshots_info_free(DupdupSnapshotInfo* rows, uintptr_t len);

#ifdef __cplusplus
} // extern "C"
#endif
