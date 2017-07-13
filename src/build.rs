extern crate gcc;

fn main() {
    gcc::compile_library("libdssim.a", &["src/dssim.c"]);

    if cfg!(target_os = "macos") {
        println!("cargo:rustc-link-lib=framework=Accelerate");
    }
}
