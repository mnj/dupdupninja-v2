# dupdup agent instructions

## Goal
Build `dupdup`, a cross-platform duplicate/near-duplicate media finder with:
- A shared Rust core (scanning, hashing, comparison, storage).
- Native UIs per platform (Linux: GTK4; Windows: WinUI 3; macOS: SwiftUI).

This repo starts as a Rust workspace with a compiling skeleton; platform UIs are stubbed behind features until their native toolchains/deps are wired up.

## Structure
- `crates/core`: Shared library (`dupdup-core`). No UI code.
- `crates/cli`: CLI wrapper over the core (`dupdup-cli`). This is the default runnable entry early on.
- `crates/ffi`: C-ABI wrapper (`dupdup-ffi`) for non-Rust UIs (Swift/SwiftUI, etc.).
- `crates/ui-gtk`: Linux GTK4 UI stub (real GTK4 code guarded by feature/cfg).
- `macos/DupdupCore`: Swift Package stub that imports the C header and provides a small Swift wrapper for SwiftUI apps.
- `windows/DupdupWinUI`: WinUI 3 (Windows App SDK) unpackaged C# UI skeleton (planned to call Rust via `dupdup-ffi`).

## Conventions
- Keep platform-specific code behind `cfg(target_os = "...")` and/or explicit Cargo features.
- Prefer small modules and clear data types in `dupdup-core` (`models`, `scan`, `db`, `hash`, `video`, `compare`).
- Avoid adding heavy dependencies to the core unless they earn their keep.
- For SQLite, prefer a schema that supports multiple scan DBs and efficient cross-scan comparisons.
- Prefer a stable FFI surface:
  - C ABI (`extern "C"`) with opaque handles and explicit free functions.
  - Thread-local `dupdup_last_error_message()` (or an explicit error struct) for error reporting.
  - Avoid leaking Rust types across the boundary.

## Dev commands
- Build: `cargo build`
- Run CLI: `cargo run -p dupdup-cli -- --help`
- Run GTK UI (Linux): `cargo run -p dupdup-ui-gtk --features gtk`
- Format: `cargo fmt`
- Lint: `cargo clippy`
