#![feature(rustc_private)]
#![allow(unused_imports)]

#[cfg(not(test))]
mod notests {
    use smalloc::benchmarks::{clock, dummy_func, bench_itered, bench_once, alloc_and_free, GlobalAllocWrap, TestState, multithread_bench};
    use smalloc::{gen_layouts, help_test_alloc_dealloc_realloc_with_writes, help_test_alloc_dealloc_realloc};
    use smalloc::Smalloc;

    use std::hint::black_box;
    use std::alloc::Layout;
    use rand::rngs::StdRng;
    use rand::SeedableRng;
    use std::cmp::max;
    use ahash::HashSet;
    use ahash::RandomState;
    use rand::Rng;

    use smalloc::TOTAL_VIRTUAL_MEMORY;
    use thousands::Separable;

    use std::thread;
    use std::sync::{Arc, Mutex};

    extern crate libc;

    use std::alloc::GlobalAlloc;
    use std::thread::JoinHandle;

    pub fn main() {
        // println!("Hello, world! I'm smalloc. I just mmap()'ed {} bytes of virtual address space. :-)", TOTAL_VIRTUAL_MEMORY.separate_with_commas());

        const ITERS: u32 = 1_850_000;

        let sm = Arc::new(Smalloc::new());
        let bi = Arc::new(GlobalAllocWrap);
        let ls = Arc::new(gen_layouts());

        thread::scope(|scope| {
            scope.spawn(|| {
                multithread_bench(help_test_alloc_dealloc_realloc_with_writes, 1, ITERS, "smalloc adrww    1", Arc::clone(&sm), Arc::clone(&ls));
            });

            scope.spawn(|| {
                multithread_bench(help_test_alloc_dealloc_realloc_with_writes, 1, ITERS, "builtin adrww    1", Arc::clone(&bi), Arc::clone(&ls));
            });
        });

        thread::scope(|scope| {
            scope.spawn(|| {
                multithread_bench(help_test_alloc_dealloc_realloc_with_writes, 32, ITERS, "smalloc adrww   32", Arc::clone(&sm), Arc::clone(&ls));
            });

            scope.spawn(|| {
                multithread_bench(help_test_alloc_dealloc_realloc_with_writes, 32, ITERS, "builtin adrww   32", Arc::clone(&bi), Arc::clone(&ls));
            });
        });

        thread::scope(|scope| {
            scope.spawn(|| {
                multithread_bench(help_test_alloc_dealloc_realloc_with_writes, 128, ITERS, "smalloc adrww  128", Arc::clone(&sm), Arc::clone(&ls));
            });

            scope.spawn(|| {
                multithread_bench(help_test_alloc_dealloc_realloc_with_writes, 128, ITERS, "builtin adrww  128", Arc::clone(&bi), Arc::clone(&ls));
            });
        });

        thread::scope(|scope| {
            scope.spawn(|| {
                multithread_bench(help_test_alloc_dealloc_realloc_with_writes, 1024, ITERS, "smalloc adrww 1024", Arc::clone(&sm), Arc::clone(&ls));
            });

            scope.spawn(|| {
                multithread_bench(help_test_alloc_dealloc_realloc_with_writes, 1024, ITERS, "builtin adrww 1024", Arc::clone(&bi), Arc::clone(&ls));
            });
        });

        thread::scope(|scope| {
            scope.spawn(|| {
                multithread_bench(help_test_alloc_dealloc_realloc_with_writes, 1650, ITERS, "smalloc adrww 1650", Arc::clone(&sm), Arc::clone(&ls));
            });

            scope.spawn(|| {
                multithread_bench(help_test_alloc_dealloc_realloc_with_writes, 1650, ITERS, "builtin adrww 1650", Arc::clone(&bi), Arc::clone(&ls));
            });
        });
    }
}

#[cfg(not(test))]
fn main() {
    notests::main();
}
