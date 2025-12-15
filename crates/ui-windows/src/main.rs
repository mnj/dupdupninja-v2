fn main() {
    #[cfg(all(target_os = "windows", feature = "winui"))]
    {
        println!("dupdup-ui-windows (native) not implemented yet.");
        return;
    }

    #[cfg(not(all(target_os = "windows", feature = "winui")))]
    {
        println!("dupdup-ui-windows stub (enable feature `winui` on Windows to start wiring).");
    }
}

