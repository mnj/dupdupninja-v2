# dupdupninja agent instructions

## Goal
Build `dupdupninja`, a cross-platform duplicate/near-duplicate media finder with:
- A shared Rust core (scanning, hashing, comparison, storage).
- Native UIs per platform (Linux: GTK4; Windows: WinUI 3; macOS: SwiftUI).

This repo starts as a Rust workspace with a compiling skeleton; platform UIs are stubbed behind features until their native toolchains/deps are wired up.

## Structure
- `crates/core`: Shared library (`dupdupninja-core`). No UI code.
- `crates/cli`: CLI wrapper over the core (`dupdupninja-cli`). This is the default runnable entry early on.
- `crates/ffi`: C-ABI wrapper (`dupdupninja-ffi`) for non-Rust UIs (Swift/SwiftUI, etc.).
- `crates/ui-gtk`: Linux GTK4 UI stub (real GTK4 code guarded by feature/cfg).
- `macos/DupdupNinjaCore`: Swift Package stub that imports the C header and provides a small Swift wrapper for SwiftUI apps.
- `windows/DupdupNinjaWinUI`: WinUI 3 (Windows App SDK) unpackaged C# UI skeleton (planned to call Rust via `dupdupninja-ffi`).

## Conventions
- Keep platform-specific code behind `cfg(target_os = "...")` and/or explicit Cargo features.
- Prefer small modules and clear data types in `dupdupninja-core` (`models`, `scan`, `db`, `hash`, `video`, `compare`).
- Avoid adding heavy dependencies to the core unless they earn their keep.
- For SQLite, prefer a schema that supports multiple scan DBs and efficient cross-scan comparisons.
- Prefer a stable FFI surface:
  - C ABI (`extern "C"`) with opaque handles and explicit free functions.
  - Thread-local `dupdupninja_last_error_message()` (or an explicit error struct) for error reporting.
  - Avoid leaking Rust types across the boundary.
- All code changes must be verified to compile successfully.

## Dev commands
- Build: `cargo build`
- Run CLI: `cargo run -p dupdupninja-cli -- --help`
- Run GTK UI (Linux): `cargo run -p dupdupninja-ui-gtk --features gtk`
- Format: `cargo fmt`
- Lint: `cargo clippy`
