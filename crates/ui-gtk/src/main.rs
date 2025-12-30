#[cfg(all(target_os = "linux", feature = "gtk"))]
mod ui;

#[cfg(all(target_os = "linux", feature = "gtk"))]
fn main() {
    ui::run();
}

#[cfg(not(all(target_os = "linux", feature = "gtk")))]
fn main() {}
