fn main() {
    #[cfg(all(target_os = "linux", feature = "gtk"))]
    {
        // Placeholder: wire GTK4 `Application` here.
        // Kept behind a feature to avoid requiring GTK4 dev packages during early bootstrap.
        println!("dupdup-ui-gtk (GTK4) not implemented yet.");
        return;
    }

    #[cfg(not(all(target_os = "linux", feature = "gtk")))]
    {
        println!("dupdup-ui-gtk stub (enable feature `gtk` on Linux to start GTK4 wiring).");
    }
}

