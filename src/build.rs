
#[cfg(target_os = "macos")]
fn main() {
    println!("cargo:rustc-link-lib=framework=Accelerate");
}

#[cfg(not(target_os = "macos"))]
fn main() {
}
