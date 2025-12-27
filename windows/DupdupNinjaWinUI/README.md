# DupdupNinjaWinUI (WinUI 3, C++/WinRT, unpackaged)

This is a Windows 11 (x64) WinUI 3 UI skeleton using C++/WinRT + Windows App SDK.
It is intended to call into the Rust core via the C ABI in `crates/ffi` (`dupdupninja-ffi`) once wiring is added.

## Prereqs

- Windows 11 x64
- Visual Studio 2022 with:
  - Desktop development with C++
  - Windows App SDK / WinUI 3 tooling
  - C++/WinRT support
- Windows 10/11 SDK (10.0.19041+)

## Build/run

From `windows/DupdupNinjaWinUI`, open `DupdupNinjaWinUI.sln` in Visual Studio,
restore NuGet packages, then build and run the `DupdupNinjaWinUI` project.

## Notes

- This project is **unpackaged**. The Windows App SDK runtime must be present on the machine.
- Rust interop is planned via the C header `crates/ffi/include/dupdupninja.h` and a built `dupdupninja_ffi` library (dll/static).
