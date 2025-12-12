#![feature(rustc_private)]
#![allow(unused_imports)]

use simplebench::{st_bench, mt_bench, compare_st_bench, compare_mt_bench};

use devutils::*;

pub fn main() {
    let seed: u64 = std::env::args()
        .find_map(|arg| arg.strip_prefix("--seed=").map(|s| s.to_string()))
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            // No --seed= found; check for erroneous "--seed VALUE" usage
            let args: Vec<String> = std::env::args().collect();
            if let Some(w) = args.windows(2).find(|w| w[0] == "--seed" && w[1].parse::<u64>().is_ok()) {
                eprintln!("Error: use --seed={} instead of --seed {}", w[1], w[1]);
                std::process::exit(1);
            }
            0
        });

    let compare = std::env::args().any(|arg| arg == "--compare");

    println!("Using seed: {}", seed);
    
    if compare {
        compare_mt_bench!(adww, 1024, 10_000, seed);
        compare_mt_bench!(ad, 1024, 10_000, seed);
        compare_mt_bench!(adrww, 1024, 10_000, seed);
        compare_mt_bench!(adrww, 32, 300_000, seed);
        compare_mt_bench!(adrww, 4, 600_000, seed);
        compare_mt_bench!(adr, 1024, 10_000, seed);
        compare_st_bench!(aww, 1_000_000, seed);
        compare_st_bench!(a, 1_000_000, seed);
        compare_st_bench!(adww, 1_000_000, seed);
        compare_st_bench!(ad, 1_000_000, seed);
        compare_st_bench!(adrww, 1_000_000, seed);
        compare_st_bench!(adr, 1_000_000, seed);
        compare_mt_bench!(aww, 512, 10_000, seed);
        compare_mt_bench!(a, 1024, 10_000, seed);
    } else {
        mt_bench!(adww, 1024, 10_000, seed);
        mt_bench!(ad, 1024, 10_000, seed);
        mt_bench!(adrww, 1024, 10_000, seed);
        mt_bench!(adrww, 32, 300_000, seed);
        mt_bench!(adrww, 4, 600_000, seed);
        mt_bench!(adr, 1024, 10_000, seed);
        st_bench!(aww, 1_000_000, seed);
        st_bench!(a, 1_000_000, seed);
        st_bench!(adww, 1_000_000, seed);
        st_bench!(ad, 1_000_000, seed);
        st_bench!(adrww, 1_000_000, seed);
        st_bench!(adr, 1_000_000, seed);
        mt_bench!(aww, 512, 10_000, seed);
        mt_bench!(a, 1024, 10_000, seed);
    }
}
