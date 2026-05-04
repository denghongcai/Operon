#[cfg(windows)]
fn main() {
    winfsp_wrs_build::build();
}

#[cfg(not(windows))]
fn main() {}
