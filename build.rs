fn main() {
    // Only error during test builds
    if std::env::var("CARGO_CFG_TEST").is_ok() {
        println!("cargo:warning=This project requires cargo-nextest for testing.");
        println!("cargo:warning=Run: cargo nextest run");
        // Uncomment to make it a hard error:
        // panic!("Use 'cargo nextest run' instead of 'cargo test'");
    }
}
