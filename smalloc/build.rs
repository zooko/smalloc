fn main() {
    // Declare that `nightly` is a valid cfg
    println!("cargo::rustc-check-cfg=cfg(nightly)");

    let version = std::process::Command::new(
        std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string())
    )
    .arg("--version")
    .output()
    .expect("failed to run rustc");

    let version_str = String::from_utf8_lossy(&version.stdout);

    if version_str.contains("nightly") {
        println!("cargo:rustc-cfg=nightly");
    }
}
