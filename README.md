# dupdup

Duplicate/near-duplicate media finder with a shared Rust core and native UIs per platform.

## Workspace crates
- `dupdup-core`: scanning, hashing, SQLite scan storage, and comparison primitives.
- `dupdup-cli`: early entrypoint for scanning and inspecting scan DBs.
- `dupdup-ui-gtk`: Linux GTK4 UI (stubbed behind a feature until GTK4 deps are wired).
- `dupdup-ui-windows`: Windows native UI (stubbed).
- `dupdup-ui-macos`: macOS native UI (stubbed).

## Quick start
Build everything:

```bash
cargo build
```

Create a scan DB from a folder:

```bash
cargo run -p dupdup-cli -- scan --root /path/to/media --db scan1.sqlite3
```

