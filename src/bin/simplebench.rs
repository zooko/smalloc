#![feature(rustc_private)]
#![allow(unused_imports)]

#[cfg(not(test))]
mod notests {
    // use smalloc::benchmarks::{clock, bench_itered, bench_once, alloc_and_free, GlobalAllocWrap, TestState, multithread_bench, singlethread_bench};
    use smalloc::benchmarks::{clock, bench_itered, bench_once, alloc_and_free, GlobalAllocWrap, TestState, singlethread_bench};
    use smalloc::{compare_st_bench, compare_mt_bench, adrww, adr, adww, ad, aww, a};
    use smalloc::Smalloc;

    use smalloc::smallocb_allocator_config::gen_allocator;

    use std::hint::black_box;
    use std::alloc::Layout;
    use std::cmp::max;

    use smalloc::TOTAL_VIRTUAL_MEMORY;
    use thousands::Separable;

    use std::thread;

    extern crate libc;

    use std::alloc::GlobalAlloc;
    use std::thread::JoinHandle;
    pub fn main() {
        compare_mt_bench!(adrww, 4, 40_000_000);
        compare_mt_bench!(adrww, 16, 40_000_000);
        compare_mt_bench!(adrww, 32, 40_000_000);
        compare_mt_bench!(adrww, 64, 40_000_000);
        compare_mt_bench!(adrww, 128, 40_000_000);
        compare_mt_bench!(adrww, 256, 40_000_000);
        compare_mt_bench!(adrww, 1024, 40_000_000);

        // This shows the CAS operation as being the hotspot:
        // compare_mt_bench!(adr, 1024, 40_000_000);


        // compare_mt_bench!(adrww, 1024, 10_000_000);
        // compare_mt_bench!(adrww, 2048, 10_000_000);

        // compare_st_bench!(adrww, 1_000_000);
        // compare_st_bench!(adr, 1_000_000);
        // compare_st_bench!(adww, 1_000_000);
        // compare_st_bench!(ad, 1_000_000);
        // compare_st_bench!(aww, 1_000_000);
        // compare_st_bench!(a, 1_000_000);

        // for numthreads in 1..32 {
        //     compare_mt_bench!(adrww, numthreads, 2_000_000);
        //     compare_mt_bench!(adr, numthreads, 2_000_000);
        //     compare_mt_bench!(adww, numthreads, 2_000_000);
        //     compare_mt_bench!(ad, numthreads, 2_000_000);
        //     compare_mt_bench!(aww, numthreads, 2_000_000);
        //     compare_mt_bench!(a, numthreads, 2_000_000);
        // }
    }
}

#[cfg(not(test))]
fn main() {
    notests::main();
}
