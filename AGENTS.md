# dupdup agent instructions

## Goal
Build `dupdup`, a cross-platform duplicate/near-duplicate media finder with:
- A shared Rust core (scanning, hashing, comparison, storage).
- Native UIs per platform (Linux: GTK4; Windows: modern Win UI; macOS: native Cocoa/AppKit).

This repo starts as a Rust workspace with a compiling skeleton; platform UIs are stubbed behind features until their native toolchains/deps are wired up.

## Structure
- `crates/core`: Shared library (`dupdup-core`). No UI code.
- `crates/cli`: CLI wrapper over the core (`dupdup-cli`). This is the default runnable entry early on.
- `crates/ui-gtk`: Linux GTK4 UI stub (real GTK4 code guarded by feature/cfg).
- `crates/ui-windows`: Windows native UI stub (real Windows UI guarded by feature/cfg).
- `crates/ui-macos`: macOS native UI stub (real AppKit code guarded by feature/cfg).

## Conventions
- Keep platform-specific code behind `cfg(target_os = "...")` and/or explicit Cargo features.
- Prefer small modules and clear data types in `dupdup-core` (`models`, `scan`, `db`, `hash`, `video`, `compare`).
- Avoid adding heavy dependencies to the core unless they earn their keep.
- For SQLite, prefer a schema that supports multiple scan DBs and efficient cross-scan comparisons.

## Dev commands
- Build: `cargo build`
- Run CLI: `cargo run -p dupdup-cli -- --help`
- Format: `cargo fmt`
- Lint: `cargo clippy`

