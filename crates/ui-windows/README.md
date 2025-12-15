# dupdup-ui-windows

Windows 11 UI (initial native skeleton).

## Build/run

On Windows 11 (x64), with a working Rust toolchain and Windows SDK:

```powershell
cargo run -p dupdup-ui-windows --features winui
```

## Notes

- The current implementation uses Win32 windowing via the `windows` crate to provide a minimal native shell
  (maximized window + File → Exit).
- We can replace the internals with WinUI 3 (Windows App SDK) once the hosting strategy (packaged/unpackaged) is decided.

