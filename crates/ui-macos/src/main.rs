fn main() {
    #[cfg(all(target_os = "macos", feature = "cocoa"))]
    {
        println!("dupdup-ui-macos (native) not implemented yet.");
        return;
    }

    #[cfg(not(all(target_os = "macos", feature = "cocoa")))]
    {
        println!("dupdup-ui-macos stub (enable feature `cocoa` on macOS to start wiring).");
    }
}

