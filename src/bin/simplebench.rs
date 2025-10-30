#![feature(rustc_private)]
#![allow(unused_imports)]

#[cfg(not(test))]
mod notests {
    // use smalloc::benchmarks::{clock, bench_itered, bench_once, alloc_and_free, GlobalAllocWrap, TestState, multithread_bench, singlethread_bench};
    use smalloc::benchmarks::{clock, bench_itered, bench_once, alloc_and_free, GlobalAllocWrap, TestState, singlethread_bench};
    use smalloc::{dummy_func, gen_layouts, help_test_alloc_dealloc_realloc_with_writes, help_test_alloc_dealloc_realloc, help_test_alloc_dealloc_with_writes, help_test_alloc_dealloc, help_test_alloc_with_writes, help_test_alloc};
    use smalloc::Smalloc;

    use smalloc::smallocb_allocator_config::gen_allocator;

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

    extern crate libc;

    use std::alloc::GlobalAlloc;
    use std::thread::JoinHandle;

    pub fn main() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();
        let bi = GlobalAllocWrap;
        let ls = gen_layouts();

        thread::scope(|scope| {
            scope.spawn(|| {
	        singlethread_bench(help_test_alloc_dealloc_realloc_with_writes, 10_000, "builtin adrww    0 thr 10K it", &bi, &ls);
            });

            scope.spawn(|| {
	        singlethread_bench(help_test_alloc_dealloc_realloc_with_writes, 10_000, "smalloc adrww    0 thr 10K it", &sm, &ls);
            });
        });

        println!();
        
        thread::scope(|scope| {
            scope.spawn(|| {
	        singlethread_bench(help_test_alloc_dealloc_realloc, 10_000, "builtin adr      0 thr 10K it", &bi, &ls);
            });

            scope.spawn(|| {
	        singlethread_bench(help_test_alloc_dealloc_realloc, 10_000, "smalloc adr      0 thr 10K it", &sm, &ls);
            });
        });

        println!();

        thread::scope(|scope| {
            scope.spawn(|| {
	        singlethread_bench(help_test_alloc_dealloc_with_writes, 10_000, "builtin adww     0 thr 10K it", &bi, &ls);
            });

            scope.spawn(|| {
	        singlethread_bench(help_test_alloc_dealloc_with_writes, 10_000, "smalloc adww     0 thr 10K it", &sm, &ls);
            });
        });

        println!();

        thread::scope(|scope| {
            scope.spawn(|| {
	        singlethread_bench(help_test_alloc_dealloc, 10_000, "builtin ad       0 thr 10K it", &bi, &ls);
            });

            scope.spawn(|| {
	        singlethread_bench(help_test_alloc_dealloc, 10_000, "smalloc ad       0 thr 10K it", &sm, &ls);
            });
        });

        println!();

        thread::scope(|scope| {
            scope.spawn(|| {
	        singlethread_bench(help_test_alloc_with_writes, 10_000, "builtin aww      0 thr 10K it", &bi, &ls);
            });

            scope.spawn(|| {
	        singlethread_bench(help_test_alloc_with_writes, 10_000, "smalloc aww      0 thr 10K it", &sm, &ls);
            });
        });

        println!();

        thread::scope(|scope| {
            scope.spawn(|| {
	        singlethread_bench(help_test_alloc, 10_000, "builtin a        0 thr 10K it", &bi, &ls);
            });

            scope.spawn(|| {
	        singlethread_bench(help_test_alloc, 10_000, "smalloc a        0 thr 10K it", &sm, &ls);
            });
        });

//         thread::scope(|scope| {
//             scope.spawn(|| {
//                 multithread_bench(help_test_alloc_dealloc_realloc_with_writes, 1, ITERS, "default adrww    1", Arc::clone(&al), Arc::clone(&ls));
//             });

// //xxx            scope.spawn(|| {
// //xxx                multithread_bench(help_test_alloc_dealloc_realloc_with_writes, 1, ITERS, "builtin adrww    1", Arc::clone(&bi), Arc::clone(&ls));
// //xxx            });
//         });

//         let al = gen_allocator();
//         thread::scope(|scope| {
//             scope.spawn(|| {
//                 multithread_bench(help_test_alloc_dealloc_realloc_with_writes, 32, ITERS, "default adrww   32", Arc::clone(&al), Arc::clone(&ls));
//             });

// //xxx            scope.spawn(|| {
// //xxx                multithread_bench(help_test_alloc_dealloc_realloc_with_writes, 32, ITERS, "builtin adrww   32", Arc::clone(&bi), Arc::clone(&ls));
// //xxx            });
//         });

//         let al = gen_allocator();
//         thread::scope(|scope| {
//             scope.spawn(|| {
//                 multithread_bench(help_test_alloc_dealloc_realloc_with_writes, 128, ITERS, "default adrww  128", Arc::clone(&al), Arc::clone(&ls));
//             });

// //xxx            scope.spawn(|| {
// //xxx                multithread_bench(help_test_alloc_dealloc_realloc_with_writes, 128, ITERS, "builtin adrww  128", Arc::clone(&bi), Arc::clone(&ls));
// //xxx            });
//         });

//         let al = gen_allocator();
//         thread::scope(|scope| {
//             scope.spawn(|| {
//                 multithread_bench(help_test_alloc_dealloc_realloc_with_writes, 1024, ITERS, "default adrww 1024", Arc::clone(&al), Arc::clone(&ls));
//             });

// //xxx            scope.spawn(|| {
// //xxx                multithread_bench(help_test_alloc_dealloc_realloc_with_writes, 1024, ITERS, "builtin adrww 1024", Arc::clone(&bi), Arc::clone(&ls));
// //xxx            });
//         });

//         let al = gen_allocator();
//         thread::scope(|scope| {
//             scope.spawn(|| {
//                 multithread_bench(help_test_alloc_dealloc_realloc_with_writes, 1650, ITERS, "default adrww 1650", Arc::clone(&al), Arc::clone(&ls));
//             });

// //xxx            scope.spawn(|| {
// //xxx                multithread_bench(help_test_alloc_dealloc_realloc_with_writes, 1650, ITERS, "builtin adrww 1650", Arc::clone(&bi), Arc::clone(&ls));
// //xxx            });
//         });
    }
}

#[cfg(not(test))]
fn main() {
    notests::main();
}
