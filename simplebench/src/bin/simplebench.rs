#![feature(rustc_private)]
#![allow(unused_imports)]

use simplebench::{st_bench, mt_bench, compare_st_bench, compare_mt_bench, multithread_hotspot, compare_hs_bench};
use std::alloc::Layout;

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
    
    let l = Layout::from_size_align(32, 1).unwrap();
    let sm = devutils::get_devsmalloc!();
    devutils::dev_instance::setup();
    multithread_hotspot(1, 100_000, "mthssm", sm, l);
    multithread_hotspot(2, 100_000, "mthssm", sm, l);
    multithread_hotspot(4, 100_000, "mthssm", sm, l);
    multithread_hotspot(8, 100_000, "mthssm", sm, l);
    multithread_hotspot(16, 100_000, "mthssm", sm, l);
    multithread_hotspot(32, 100_000, "mthssm", sm, l);

    println!();

    //multithread_hotspot(200, 100_000, "mthssm", sm, l);
    //multithread_hotspot(400, 100_000, "mthssm", sm, l);
    //multithread_hotspot(800, 100_000, "mthssm", sm, l);


    if compare {
        compare_hs_bench!(1, 100_000);
        compare_hs_bench!(2, 100_000);
        compare_hs_bench!(4, 100_000);
        compare_hs_bench!(8, 100_000);
        compare_hs_bench!(16, 100_000);
        compare_hs_bench!(32, 100_000);
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
