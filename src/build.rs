use std::process::Command;
use std::env::var as getenv;
use std::path::Path;

fn main() {
    let destdir = getenv("OUT_DIR").unwrap();

    let mut cmd = Command::new("make");
    let cargo_manifest = getenv("CARGO_MANIFEST_DIR").unwrap();
    cmd.current_dir(&Path::new(&cargo_manifest));

    cmd.arg(format!("DESTDIR={}/", destdir));
    cmd.arg(format!("SRC={}/src/", cargo_manifest));
    cmd.arg("CFLAGSOPT=-g");

    if let Some(j) = getenv("NUM_JOBS").ok() {
        cmd.arg(format!("-j{}", j));
    }

    cmd.arg(format!("{}/libdssim.a", destdir));

    if !cmd.status().unwrap().success() {
        println!("cmd {:?}", cmd);
        println!("out dir {}", destdir);
        println!("cargo {}", cargo_manifest);
        panic!("Script failed");
    }

    println!("cargo:rustc-link-search=native={}", destdir);
    println!("cargo:root={}", destdir);
    println!("cargo:rustc-link-lib=static=dssim");
    printframework();
}

#[cfg(target_os = "macos")]
fn printframework() {
    println!("cargo:rustc-link-lib=framework=Accelerate");
}

#[cfg(not(target_os = "macos"))]
fn printframework() {
}
