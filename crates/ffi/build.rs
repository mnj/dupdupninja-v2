use std::env;

fn main() {
    // Keep in sync with FFI_ABI_MAJOR in src/lib.rs and DUPDUPNINJA_FFI_ABI_MAJOR in the header.
    const ABI_MAJOR: &str = "1";

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "linux" {
        println!(
            "cargo:rustc-cdylib-link-arg=-Wl,-soname,libdupdupninja_ffi.so.{}",
            ABI_MAJOR
        );
    } else if target_os == "macos" {
        println!(
            "cargo:rustc-cdylib-link-arg=-Wl,-install_name,libdupdupninja_ffi.{}.dylib",
            ABI_MAJOR
        );
    }
}
