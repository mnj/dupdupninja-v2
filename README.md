# dupdupninja

Duplicate/near-duplicate media finder with a shared Rust core and native UIs per platform.

## Building

See `BUILDING.md`.

## Workspace crates
- `dupdupninja-core`: scanning, hashing, SQLite scan storage, and comparison primitives.
- `dupdupninja-cli`: early entrypoint for scanning and inspecting scan DBs.
- `dupdupninja-ui-gtk`: Linux GTK4 UI (stubbed behind a feature until GTK4 deps are wired).
- `dupdupninja-ffi`: C-ABI wrapper for Swift/WinUI and other non-Rust hosts.

## Platform UI projects (non-Rust)
- macOS SwiftUI wrapper: `macos/DupdupNinjaCore`
- Windows WinUI 3 (unpackaged) app skeleton: `windows/DupdupNinjaWinUI`

## Quick start
Build everything:

```bash
cargo build
```

Create a scan DB from a folder:

```bash
cargo run -p dupdupninja-cli -- scan --root /path/to/media --db scan1.sqlite3
```
