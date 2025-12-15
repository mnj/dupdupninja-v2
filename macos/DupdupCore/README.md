# DupdupCore (Swift Package stub)

This is a stub Swift Package intended to wrap the shared Rust core via a C ABI.

## Intended wiring
1. Build the Rust FFI library from the repo root:
   - `cargo build -p dupdup-ffi`
2. On macOS, link `libdupdup_ffi.a` (or `.dylib`) into an Xcode app target.
3. Add `crates/ffi/include` to the header search path and import `dupdup.h`.

This package currently ships the header as a SwiftPM C target so Swift code can `import CDupdup`.
Linking is left to the host Xcode project (so the package can remain a lightweight stub).

