# DupdupNinjaWinUI (WinUI 3, .NET, unpackaged)

This is a Windows 11 (x64) WinUI 3 UI skeleton using the Windows App SDK on .NET.
It is intended to call into the Rust core via the C ABI in `crates/ffi` (`dupdupninja-ffi`) once wiring is added.

## Prereqs

- Windows 11 x64
- Visual Studio 2022 with Windows App SDK / WinUI 3 tooling
- .NET SDK (project targets `net8.0-windows10.0.19041.0`)
- Windows 10/11 SDK (10.0.19041+)

## Build/run

From `windows/DupdupNinjaWinUI`, open `DupdupNinjaWinUI.slnx` in Visual Studio,
restore NuGet packages, then build and run the `DupdupNinjaWinUI` project.

CLI run (unpackaged, x64):

```powershell
dotnet run --project .\DupdupNinjaWinUI\DupdupNinjaWinUI.csproj -p:Platform=x64 --launch-profile "DupdupNinjaWinUI (Unpackaged)"
```

If startup crashes, check:

- `%LOCALAPPDATA%\dupdupninja\winui-startup.log`

## Notes

- This project is **unpackaged**. The Windows App SDK runtime must be present on the machine.
- Rust interop is planned via the C header `crates/ffi/include/dupdupninja.h` and a built `dupdupninja_ffi` library (dll/static).
