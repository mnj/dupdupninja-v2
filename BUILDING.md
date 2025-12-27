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

## Linux (Debian 13/Trixie, GTK4)

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

### GNOME / GTK minimum

The GTK UI opts into GTK 4.16 APIs (`v4_16`), which aligns with GNOME 48 as shipped in Debian 13/Trixie.

## Windows 11 (x64)

### Prereqs (WinUI 3 app, C++/WinRT)

- Windows 11 x64
- Visual Studio 2022 with:
  - Desktop development with C++
  - Windows App SDK / WinUI 3 tooling
  - C++/WinRT support
- Windows 10/11 SDK (10.0.19041+)
- Windows App SDK runtime installed (unpackaged apps rely on it being present)

### Build/run WinUI 3 app

From `windows/DupdupNinjaWinUI`, open `DupdupNinjaWinUI.sln` in Visual Studio,
restore NuGet packages, then build and run the `DupdupNinjaWinUI` project.

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

### Swift Package + SwiftUI app skeleton

`macos/DupdupNinjaCore` is a SwiftPM package that:
- Vendors the C header as target `CDupdupNinja`
- Provides a tiny Swift wrapper (`DupdupNinjaCore.Engine`)
- Includes a minimal SwiftUI app target (`DupdupNinjaApp`) with `Scan` menu items (Folder/Disk pickers)

From `macos/DupdupNinjaCore`:

```bash
swift build
```

Run the SwiftUI app (macOS only):

```bash
swift run DupdupNinjaApp
```

Linking the built Rust library into an actual SwiftUI app target is the next step (Xcode project setup), and will depend on whether you want `staticlib` or `cdylib` linkage and how you want to ship the Rust artifacts.
