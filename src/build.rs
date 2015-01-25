use std::process::Command;
use std::env::var as getenv;
use std::path::Path;

fn main() {
    let destdir = getenv("OUT_DIR").unwrap();

    let mut cmd = Command::new("make");
    cmd.current_dir(&Path::new(&getenv("CARGO_MANIFEST_DIR").unwrap()));

    cmd.arg(format!("DESTDIR={}/", destdir));

    if let Some(j) = getenv("NUM_JOBS").ok() {
        cmd.arg(format!("-j{}", j));
    }

    cmd.arg(format!("{}/libdssim.a", destdir));

    if !cmd.status().unwrap().success() {
        panic!("Script failed");
    }

    println!("cargo:rustc-flags=-L {} -l static=dssim", destdir);
    println!("cargo:root={}", destdir);
}
