#![feature(rustc_private)]
#![allow(unused_imports)]

use bench::{st_bench, mt_bench, compare_st_bench, compare_mt_bench, compare_fh_bench, multithread_hotspot, multithread_free_hotspot, compare_hs_bench};
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
    let thorough = std::env::args().any(|arg| arg == "--thorough");

    println!("Using seed: {}", seed);
    
    const THREADS_THAT_CAN_FIT_INTO_SLABS: u32 = 32;
    const THREADS_WAY_TOO_MANY: u32 = 1024;

    // for benchmarks that are going to re-use space
    let iters_many = if thorough { 100_000 } else { 50_000 };

    // for benchmarks that are not going to re-use space, so they'll run out of space if we do
    // iters_many
    const ITERS_FEW: u64 = 2_000;

    if compare {
        if thorough {
            // hs_bench simulates a somewhat plausible scenario, which is a worst-case-scenario for
            // smalloc before v7.2, when a bunch of threads are all trying to alloc/dealloc from the
            // same slab. This benchmark is structured specifically to exerise smalloc's hotspot:
            // every 64'th thread is active and the intervening 63 are quiescent, because every
            // 64'th thread will get mapped to the same slab by smalloc. So it doesn't make a whole
            // lot of sense to compare smalloc's performance on this particular benchmark to the
            // performance of other allocators, which presumably have different
            // hotspots/worst-case-scenarios.
            compare_hs_bench!(one_ad, THREADS_THAT_CAN_FIT_INTO_SLABS, iters_many);
            compare_hs_bench!(a, THREADS_THAT_CAN_FIT_INTO_SLABS, ITERS_FEW);

            // multithread_free_hotspot simulates a somewhat plausible worst-case-scenario, which is
            // that many threads are trying to free slots in the same slab as each other.
            const TOT_ITERS: u64 = 10_000_000;
            for numthreads in [1u32, 10, 100, 1000, 10_000] {
                let iters_per_thread = TOT_ITERS / numthreads as u64;
                let l = Layout::from_size_align(8, 1).unwrap();
                compare_fh_bench!(numthreads, iters_per_thread, l);
            }

            println!();

            // These benchmarks with 1024 threads are worst-case-scenarios. This is the case that there
            // are more threads than cores *and* every thread is hammering on the allocator as fast as
            // it can. This is not something to optimize for at the cost of performance in other cases,
            // because the user code shouldn't do that. However, we do want to benchmark it, partially
            // just in order to look for pathological behavior in smalloc, and also in order to optimize
            // it if we can do so without penalizing other cases. In particular smalloc v7.2 made it so
            // on flh-collision, alloc fails over to another slab.
            compare_mt_bench!(adrww, THREADS_WAY_TOO_MANY, iters_many, seed);
            compare_mt_bench!(adr, THREADS_WAY_TOO_MANY, iters_many, seed);
            compare_mt_bench!(adww, THREADS_WAY_TOO_MANY, iters_many, seed);
            compare_mt_bench!(ad, THREADS_WAY_TOO_MANY, iters_many, seed);
            compare_mt_bench!(aww, THREADS_WAY_TOO_MANY, ITERS_FEW, seed);
            compare_mt_bench!(a, THREADS_WAY_TOO_MANY, ITERS_FEW, seed);
        }
        
        compare_mt_bench!(adrww, THREADS_THAT_CAN_FIT_INTO_SLABS, iters_many, seed);
        compare_st_bench!(adrww, iters_many, seed);

        compare_mt_bench!(adr, THREADS_THAT_CAN_FIT_INTO_SLABS, iters_many, seed);
        compare_st_bench!(adr, iters_many, seed);

        compare_mt_bench!(adww, THREADS_THAT_CAN_FIT_INTO_SLABS, iters_many, seed);
        compare_st_bench!(adww, iters_many, seed);

        compare_mt_bench!(ad, THREADS_THAT_CAN_FIT_INTO_SLABS, iters_many, seed);
        compare_st_bench!(ad, iters_many, seed);

        compare_mt_bench!(aww, THREADS_THAT_CAN_FIT_INTO_SLABS, ITERS_FEW, seed);
        compare_st_bench!(aww, ITERS_FEW, seed);

        compare_mt_bench!(a, THREADS_THAT_CAN_FIT_INTO_SLABS, ITERS_FEW, seed);
        compare_st_bench!(a, ITERS_FEW, seed);
    } else {
        if thorough {
            let l = Layout::from_size_align(32, 1).unwrap();
            let sm = devutils::get_devsmalloc!();
            devutils::dev_instance::setup();
            multithread_hotspot!(one_ad, THREADS_THAT_CAN_FIT_INTO_SLABS, iters_many, sm, l);
            multithread_hotspot!(a, THREADS_THAT_CAN_FIT_INTO_SLABS, ITERS_FEW, sm, l);

            const TOT_ITERS: u64 = 10_000_000;
            for numthreads in [1u32, 10, 100, 1000, 10_000] {
                let iters_per_thread = TOT_ITERS / numthreads as u64;
                let l = Layout::from_size_align(8, 1).unwrap();
                multithread_free_hotspot!(numthreads, iters_per_thread, sm, l);
            }

            println!();

            mt_bench!(adrww, THREADS_WAY_TOO_MANY, iters_many, seed);
            mt_bench!(adr, THREADS_WAY_TOO_MANY, iters_many, seed);
            mt_bench!(adww, THREADS_WAY_TOO_MANY, iters_many, seed);
            mt_bench!(ad, THREADS_WAY_TOO_MANY, iters_many, seed);
            mt_bench!(aww, THREADS_WAY_TOO_MANY, ITERS_FEW, seed);
            mt_bench!(a, THREADS_WAY_TOO_MANY, ITERS_FEW, seed);
        }
        
        mt_bench!(adrww, THREADS_THAT_CAN_FIT_INTO_SLABS, iters_many, seed);
        st_bench!(adrww, iters_many, seed);

        mt_bench!(adr, THREADS_THAT_CAN_FIT_INTO_SLABS, iters_many, seed);
        st_bench!(adr, iters_many, seed);

        mt_bench!(adww, THREADS_THAT_CAN_FIT_INTO_SLABS, iters_many, seed);
        st_bench!(adww, iters_many, seed);

        mt_bench!(ad, THREADS_THAT_CAN_FIT_INTO_SLABS, iters_many, seed);
        st_bench!(ad, iters_many, seed);

        mt_bench!(aww, THREADS_THAT_CAN_FIT_INTO_SLABS, ITERS_FEW, seed);
        st_bench!(aww, ITERS_FEW, seed);

        mt_bench!(a, THREADS_THAT_CAN_FIT_INTO_SLABS, ITERS_FEW, seed);
        st_bench!(a, ITERS_FEW, seed);
    }
}
