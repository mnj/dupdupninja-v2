# Building dupdup

This repo contains:
- A Rust workspace (core logic, CLI, and some UI stubs/skeletons).
- A macOS Swift Package stub to call the Rust core via C ABI.
- A Windows WinUI 3 (Windows App SDK) C# app skeleton.

## Common (all platforms)

### Rust toolchain

- Install Rust via `rustup` (stable).
- Build everything (Rust workspace):

```bash
cargo build
```

### Useful Rust commands

- Run the CLI:

```bash
cargo run -p dupdup-cli -- --help
```

- Format / lint:

```bash
cargo fmt
cargo clippy
```

## Linux (Ubuntu 24.04, GTK4)

### Prereqs

```bash
sudo apt update
sudo apt install -y build-essential pkg-config libgtk-4-dev
```

### Build/run GTK UI

GTK is behind a feature so the workspace builds without GTK dev packages.

```bash
cargo run -p dupdup-ui-gtk --features gtk
```

## Windows 11 (x64)

There are two Windows UI paths in this repo:

1. Rust `dupdup-ui-windows` (Win32 skeleton, quick native shell).
2. `windows/DupdupWinUI` (WinUI 3 / Windows App SDK, modern UI direction).

### Prereqs (Rust)

- Rust toolchain (stable) for `x86_64-pc-windows-msvc`
- Visual Studio 2022 Build Tools (MSVC) + Windows 10/11 SDK

### Build/run Rust Win32 skeleton

On Windows:

```powershell
cargo run -p dupdup-ui-windows --features winui
```

### Prereqs (WinUI 3 app)

- Windows 11 x64
- Visual Studio 2022 and/or .NET SDK (project targets `net8.0-windows10.0.19041.0`)
- Windows App SDK runtime installed (unpackaged apps rely on it being present)

### Build/run WinUI 3 app

From `windows/DupdupWinUI`:

```powershell
dotnet restore
dotnet run
```

## macOS (SwiftUI + Rust core via C ABI)

### Prereqs

- Xcode (for Swift/SwiftUI)
- Rust toolchain (stable)

### Rust FFI library

The C ABI wrapper crate is `dupdup-ffi`:

```bash
cargo build -p dupdup-ffi
```

The exported header is:
- `crates/ffi/include/dupdup.h`

### Swift Package stub

`macos/DupdupCore` is a SwiftPM package that:
- Vendors the C header as target `CDupdup`
- Provides a tiny Swift wrapper (`DupdupCore.Engine`)

From `macos/DupdupCore`:

```bash
swift build
```

Linking the built Rust library into an actual SwiftUI app target is the next step (Xcode project setup), and will depend on whether you want `staticlib` or `cdylib` linkage and how you want to ship the Rust artifacts.

