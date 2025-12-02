#![feature(rustc_private)]
#![allow(unused_imports)]

use smalloc::Smalloc;

use std::hint::black_box;
use std::alloc::Layout;
use std::cmp::max;

use thousands::Separable;

use std::thread;

use simplebench::{st_bench, mt_bench, compare_st_bench, compare_mt_bench};

use devutils::*;

use std::alloc::GlobalAlloc;
use std::thread::JoinHandle;

use simplebench::GlobalAllocWrap;

pub fn main() {
    let seed: u64 = std::env::args()
        .find_map(|arg| arg.strip_prefix("--seed=").map(|s| s.to_string()))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let compare = std::env::args().any(|arg| arg == "--compare");

    println!("Using seed: {}", seed);
    
    if compare {
        compare_mt_bench!(aww, 1024, 10_000, seed);
        compare_mt_bench!(a, 1024, 10_000, seed);
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
    } else {
        mt_bench!(aww, 1024, 10_000, seed);
        mt_bench!(a, 1024, 10_000, seed);
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
    }
}
