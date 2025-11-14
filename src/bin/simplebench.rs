#![feature(rustc_private)]
#![allow(unused_imports)]

#[cfg(not(test))]
mod notests {
    // use smalloc::benchmarks::{clock, bench_itered, bench_once, alloc_and_free, GlobalAllocWrap, TestState, multithread_bench, singlethread_bench};
    use smalloc::benchmarks::{clock, bench_itered, bench_once, alloc_and_free, GlobalAllocWrap, TestState, singlethread_bench};
    use smalloc::{compare_bench, dummy_func, help_test_alloc_dealloc_realloc_with_writes, help_test_alloc_dealloc_realloc, help_test_alloc_dealloc_with_writes, help_test_alloc_dealloc, help_test_alloc_with_writes, help_test_alloc};
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
        compare_bench!(help_test_alloc_dealloc_realloc_with_writes, 1_000_000, "adrww");
        compare_bench!(help_test_alloc_dealloc_realloc, 1_000_000, "adr");
        compare_bench!(help_test_alloc_dealloc_with_writes, 1_000_000, "adww");
        compare_bench!(help_test_alloc_dealloc, 1_000_000, "ad");
        compare_bench!(help_test_alloc_with_writes, 1_000_000, "aww");
        compare_bench!(help_test_alloc, 1_000_000, "a");
    }
}

#[cfg(not(test))]
fn main() {
    notests::main();
}
