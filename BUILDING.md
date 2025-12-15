# Building dupdupninja

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
cargo run -p dupdupninja-cli -- --help
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
cargo run -p dupdupninja-ui-gtk --features gtk
```

## Windows 11 (x64)

### Prereqs (WinUI 3 app)

- Windows 11 x64
- Visual Studio 2022 and/or .NET SDK (project targets `net8.0-windows10.0.19041.0`)
- Windows App SDK runtime installed (unpackaged apps rely on it being present)

### Build/run WinUI 3 app

From `windows/DupdupNinjaWinUI`:

```powershell
dotnet restore
dotnet run
```

## macOS (SwiftUI + Rust core via C ABI)

### Prereqs

- Xcode (for Swift/SwiftUI)
- Rust toolchain (stable)

### Rust FFI library

The C ABI wrapper crate is `dupdupninja-ffi`:

```bash
cargo build -p dupdupninja-ffi
```

The exported header is:
- `crates/ffi/include/dupdupninja.h`

### Swift Package stub

`macos/DupdupNinjaCore` is a SwiftPM package that:
- Vendors the C header as target `CDupdupNinja`
- Provides a tiny Swift wrapper (`DupdupNinjaCore.Engine`)

From `macos/DupdupNinjaCore`:

```bash
swift build
```

Linking the built Rust library into an actual SwiftUI app target is the next step (Xcode project setup), and will depend on whether you want `staticlib` or `cdylib` linkage and how you want to ship the Rust artifacts.
