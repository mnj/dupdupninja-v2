# DupdupNinjaWinUI (WinUI 3, unpackaged)

This is a Windows 11 (x64) WinUI 3 UI skeleton using the Windows App SDK.
It is intended to call into the Rust core via the C ABI in `crates/ffi` (`dupdupninja-ffi`) once wiring is added.

## Prereqs

- Windows 11 x64
- Visual Studio 2022 (or `dotnet` SDK) with Windows App SDK/WinUI tooling
- Windows 10/11 SDK (10.0.19041+)

## Build/run

From `windows/DupdupNinjaWinUI`:

```powershell
dotnet restore
dotnet run
```

## Notes

- This project is **unpackaged** (`WindowsPackageType=None`). The Windows App SDK runtime must be present on the machine.
- Rust interop is planned via the C header `crates/ffi/include/dupdupninja.h` and a built `dupdupninja_ffi` library (dll/static).
