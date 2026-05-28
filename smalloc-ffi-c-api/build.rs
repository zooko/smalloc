fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "macos" {
        let out_dir = std::env::var("OUT_DIR").unwrap();

        std::process::Command::new("cc")
            .args(["-c", "interpose.c", "-o"])
            .arg(format!("{out_dir}/interpose.o"))
            .status()
            .expect("failed to compile interpose.c");

        println!("cargo:rustc-cdylib-link-arg={out_dir}/interpose.o");
        println!("cargo:rustc-cdylib-link-arg=-Wl,-u,_interpose_malloc");
        println!("cargo:rustc-cdylib-link-arg=-Wl,-u,_interpose_free");
        println!("cargo:rustc-cdylib-link-arg=-Wl,-u,_interpose_realloc");
        println!("cargo:rerun-if-changed=interpose.c");
    }
}
