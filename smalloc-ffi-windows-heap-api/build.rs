fn main() {
    // Windows-only build
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() != "windows" {
        panic!("This crate only builds on Windows");
    }

    println!("cargo:rerun-if-changed=export_extractor.py");
    println!("cargo:rerun-if-changed=build.rs");
}
