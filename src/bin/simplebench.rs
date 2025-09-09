#![feature(rustc_private)]

#![allow(unused_imports)]

#[cfg(not(test))]
mod notests {
    use smalloc::benches::{clock, dummy_func, bench_itered, bench_once, alloc_and_free, GlobalAllocWrap};
    use smalloc::{help_test_one_alloc_dealloc_realloc_with_writes, help_test_one_alloc_dealloc_realloc};
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

    pub fn main() {
        println!("Hello, world! I'm smalloc. I just mmap()'ed {} bytes of virtual address space. :-)", TOTAL_VIRTUAL_MEMORY.separate_with_commas());

        let mut handles = Vec::new();

        // let df3029 = move || {
        //     dummy_func(30, 29);
        // };

        // handles.push(thread::spawn(move || {
        //     let multithread_dummy3029 = move || {
        //         let mut mt3029handles = Vec::new();
        //         for _t in 0..128 {
        //             mt3029handles.push(thread::spawn(df3029));
        //         }
        //         for mth in mt3029handles {
        //             mth.join().unwrap();
        //         }
        //     };
            
        //     bench_once("mtdummy3029", || {
        //         multithread_dummy3029();
        //     }, libc::CLOCK_THREAD_CPUTIME_ID);
        // }));

        // let df3130 = move || {
        //     dummy_func(31, 30);
        // };

        // handles.push(thread::spawn(move || {
        //     let multithread_dummy3130 = move || {
        //         let mut mt3130handles = Vec::new();
        //         for _t in 0..128 {
        //             mt3130handles.push(thread::spawn(df3130));
        //         }
        //         for mth in mt3130handles {
        //             mth.join().unwrap();
        //         }
        //     };
            
        //     bench_once("mtdummy3130", || {
        //         multithread_dummy3130();
        //     }, libc::CLOCK_THREAD_CPUTIME_ID);
        // }));

        const ITERS: usize = 1_000_000;
        const NUM_THREADS: usize = 512;

        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let bi = GlobalAllocWrap;

        let mut ls = Vec::new();
        for siz in [35, 64, 128, 500, 2000, 10_000, 1_000_000] {
            ls.push(Layout::from_size_align(siz, 1).unwrap());
            ls.push(Layout::from_size_align(siz + 10, 1).unwrap());
            ls.push(Layout::from_size_align(siz - 10, 1).unwrap());
            ls.push(Layout::from_size_align(siz * 2, 1).unwrap());
        }
        let lsa = Arc::new(ls);

        fn multithread_bench<F>(bf: F, iters: usize, name: &str)
        where
            F: FnMut() + Send + 'static
        {
            let bfa = Arc::new(Mutex::new(bf));

            let start = clock(libc::CLOCK_UPTIME_RAW);

            let mut smhandles = Vec::new();

            for _t in 0..NUM_THREADS {
                let bfa_clone = Arc::clone(&bfa);
                let handle = thread::spawn(move || {
                    let mut bfa_guard  = bfa_clone.lock().unwrap();
                    bfa_guard();
                });
                smhandles.push(handle);
            }

            for smh in smhandles {
                smh.join().unwrap();
            }

            let elap = clock(libc::CLOCK_UPTIME_RAW) - start;

            eprintln!("name: {name}, threads: {NUM_THREADS}, iters: {iters}, ns: {}, ns/i: {}", elap.separate_with_commas(), (elap / iters as u64).separate_with_commas());
        }

        fn generate_one_thread_iterated_function<A: GlobalAlloc + 'static>(allocator: A, ls: Arc<Vec<Layout>>) -> impl FnMut() {
            move || {
                let lsai = Arc::clone(&ls);
                let mut r1 = StdRng::seed_from_u64(0);
                let mut m1: HashSet<(usize, Layout)> = HashSet::with_capacity_and_hasher(ITERS, RandomState::with_seed(r1.random::<u64>() as usize));
                let mut ps1 = Vec::new();
                    
                for _i in 0..ITERS {
                    help_test_one_alloc_dealloc_realloc_with_writes(&allocator, &mut r1, &mut ps1, &mut m1, &lsai);
                }
            }
        }

        let lsasm = Arc::clone(&lsa);
        let otsm = generate_one_thread_iterated_function(sm, lsasm);
        handles.push(thread::spawn(move || {
            multithread_bench(otsm, ITERS, "smalloc");
        }));

        let lsabi = Arc::clone(&lsa);
        let otbi = generate_one_thread_iterated_function(bi, lsabi);
        handles.push(thread::spawn(move || {
            multithread_bench(otbi, ITERS, "builtin");
        }));

        //handles.push(thread::spawn(multithread_sm));
        //handles.push(thread::spawn(multithread_bi));
                     
        for handle in handles {
            handle.join().unwrap();
        }
    }
}

#[cfg(not(test))]
fn main() {
    notests::main();
}
