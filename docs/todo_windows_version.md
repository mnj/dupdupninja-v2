# Windows Version Parity TODO (WinUI 3 vs GTK4)

Goal: make `windows/DupdupNinjaWinUI` feature-complete with the Linux GTK4 app.

This document splits the work into issue lists by priority.

## Scope Notes

- Current WinUI app is scan-oriented and does not yet expose full fileset/match workflows.
- GTK app is the parity reference for behavior and user flows.
- Snapshot functionality should follow existing core/FFI capabilities and settings.

---

## P0 (Core Parity, Must-Have)

### Issue P0-1: Fileset Sidebar + Open Existing `.ddn`

Description:
- Add fileset list/sidebar in main window.
- Support:
  - open existing `.ddn`
  - add fileset from new scan
  - active fileset selection
  - remove/close fileset from UI
- Persist/reload open filesets between app launches.

Acceptance Criteria:
- User can switch between multiple filesets.
- App restores previously opened filesets on startup.
- Active fileset updates the main results panel.

Dependencies:
- none

---

### Issue P0-2: Real Settings + Persistence

Description:
- Replace placeholder settings window with scan-related settings:
  - `capture_snapshots`
  - `snapshots_per_video`
  - `snapshot_max_dim`
  - default fileset folder
- Persist settings to disk and reload at startup.

Acceptance Criteria:
- Settings are reflected in subsequent scans.
- Settings survive app restart.

Dependencies:
- P0-1 (for default fileset folder UX integration)

---

### Issue P0-3: Results Browser (Files + Match Groups)

Description:
- Add main results view with grouped matches and child rows.
- Show core columns:
  - name/path
  - size
  - file type
  - blake3
  - sha256
- Add toggle/filter for "show only duplicates".

Acceptance Criteria:
- Opening a fileset shows match groups and files.
- Filter updates results in-place.

Dependencies:
- P0-1

---

### Issue P0-4: Matching Modes in GUI (Exact + Similar)

Description:
- Add GUI controls for:
  - exact duplicates (blake3/sha256)
  - similar matches (pHash primary, show dHash/aHash scores)
- Show confidence percentages:
  - exact = `100.00%`
  - similar capped at `99.99%`

Acceptance Criteria:
- Exact mode reliably groups byte-identical files.
- Similar mode groups by configured thresholds and shows score details.

Dependencies:
- P0-3

---

### Issue P0-5: Scan Lifecycle Integration

Description:
- Integrate scan actions with fileset model and results refresh:
  - scan folder
  - scan drive
  - cancel scan
  - completed/incomplete status
- Ensure active fileset refreshes after scan completion/cancel.
- Do not refresh match groups during scan progress (no live match updates while scanning).

Acceptance Criteria:
- Cancel updates status correctly.
- Completed scan updates results without app restart.
- Match groups remain unchanged during scan and refresh only once scan finishes.

Dependencies:
- P0-1, P0-2, P0-3

---

### Issue P0-6: Data Access Architecture Decision + Implementation

Description:
- Pick one strategy for WinUI data operations:
  1. Expand Rust FFI APIs for fileset/match/snapshot/actions
  2. Use direct SQLite access in C# for read/write operations
- Implement chosen approach for P0 features.

Acceptance Criteria:
- P0 features work without duplicated business logic drift.
- Error handling is robust and user-facing messages are clear.

Dependencies:
- P0-3, P0-4

---

## P1 (Feature Completeness, High Value)

### Issue P1-1: File Actions Bar (Delete/Trash/Copy/Move)

Description:
- Add selected-file actions:
  - move to trash/recycle bin
  - delete permanently
  - copy to...
  - move to...
- Keep DB/fileset in sync after action.

Acceptance Criteria:
- Actions update filesystem and UI list consistently.
- Failures report actionable errors.

Dependencies:
- P0-3, P0-6

---

### Issue P1-2: Replace with Symlink/Junction Equivalent

Description:
- Implement Windows-safe dedupe action equivalent to GTK "replace with symlink".
- Handle permission/admin constraints with fallback guidance.

Acceptance Criteria:
- Action works for supported scenarios.
- Clear error/fallback messaging for restricted environments.

Dependencies:
- P1-1

---

### Issue P1-3: Compare Selected View

Description:
- Build compare window/pane for selected files.
- Show side-by-side metadata and snapshots.

Acceptance Criteria:
- User can compare selected match members with full metadata.
- Snapshot previews render from `.ddn` data.

Dependencies:
- P0-3, P0-4

---

### Issue P1-4: Fileset Properties Editor

Description:
- Add editable fileset metadata dialog:
  - name
  - description
  - notes
  - status (where relevant)

Acceptance Criteria:
- Metadata edits persist in `.ddn` and reflect in UI.

Dependencies:
- P0-1

---

## P2 (Polish, Reliability, Platform Hardening)

### Issue P2-1: Packaging and RID Output Validation

Description:
- Ensure publish outputs contain required native libs for:
  - `win-x64`
  - `win-arm64`
- Validate runtime loading paths for `dupdupninja_ffi.dll`.

Acceptance Criteria:
- Fresh machine run succeeds without manual DLL placement.
- CI artifacts contain required binaries.

Dependencies:
- P0-6

---

### Issue P2-2: Drive/Mount Selection UX Hardening

Description:
- Refine drive-root selection behavior and messaging for local/network paths.
- Keep semantics aligned with GTK app expectations.

Acceptance Criteria:
- Invalid selections produce clear guidance.
- Valid roots are accepted consistently.

Dependencies:
- P0-5

---

### Issue P2-3: Performance + Incremental Refresh

Description:
- Improve large-fileset responsiveness:
  - pagination/virtualization
  - background loading
  - debounced refreshes

Acceptance Criteria:
- UI remains responsive on large `.ddn` filesets.

Dependencies:
- P0-3

---

### Issue P2-4: End-to-End Regression Test Matrix

Description:
- Add test plan/checklist across:
  - scan success/cancel
  - exact/similar matching
  - file actions
  - compare view
  - settings persistence
- Include x64 + arm64 Windows validation passes.

Acceptance Criteria:
- Repeatable release checklist exists and is used in CI/release process.

Dependencies:
- All P0/P1

---

## Suggested Delivery Order

1. P0-1, P0-2, P0-5
2. P0-3, P0-4, P0-6
3. P1-1, P1-3, P1-4, P1-2
4. P2-1, P2-2, P2-3, P2-4

## Suggested Tracking Metadata (for GitHub Issues)

For each issue create:
- Labels: `windows`, `winui`, `parity`, plus `p0/p1/p2`
- Milestone: `Windows parity`
- Checklist in description:
  - UX implemented
  - core integration done
  - error handling done
  - tested on x64
  - tested on arm64 (if applicable)
