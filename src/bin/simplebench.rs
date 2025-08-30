#![feature(rustc_private)]

#![allow(unused_imports)]

#[cfg(not(test))]
mod notests {
    use smalloc::benches::{dummy_func, bench, alloc_and_free, GlobalAllocWrap};
    use smalloc::{help_test_one_alloc_dealloc_realloc_with_writes, help_test_one_alloc_dealloc_realloc};
    use smalloc::Smalloc;
    use std::sync::Arc;
    use std::thread;
    use std::hint::black_box;
    use std::alloc::Layout;
    use rand::rngs::StdRng;
    use rand::SeedableRng;
    use std::cmp::max;
    use ahash::HashSet;
    use ahash::RandomState;
    use rand::Rng;

    pub fn main() {
        let mut handles = Vec::new();

        handles.push(thread::spawn(|| {
            bench("dummy3029", 100_000, || {
                dummy_func(30, 29);
            });
        }));

        handles.push(thread::spawn(|| {
            bench("dummy3130", 100_000, || {
                dummy_func(31, 30);
            });
        }));

        let iters = 10_000_000;

        let sm = Arc::new(Smalloc::new());
        sm.idempotent_init().unwrap();

        let mut ls = Vec::new();
        for siz in [35, 64, 128, 500, 2000, 10_000, 1_000_000] {
            ls.push(Layout::from_size_align(siz, 1).unwrap());
            ls.push(Layout::from_size_align(siz + 10, 1).unwrap());
            ls.push(Layout::from_size_align(siz - 10, 1).unwrap());
            ls.push(Layout::from_size_align(siz * 2, 1).unwrap());
        }

        let mut rsm1 = StdRng::seed_from_u64(0);
        let mut msm1: HashSet<(usize, Layout)> = HashSet::with_capacity_and_hasher(iters, RandomState::with_seed(rsm1.random::<u64>() as usize));
        let mut pssm1 = Vec::new();

        let sm1 = Arc::clone(&sm);
        let ls1 = ls.clone();
        let bench_a_d_r_w_w_sm = move || {
            help_test_one_alloc_dealloc_realloc_with_writes(sm1.as_ref(), &mut rsm1, &mut pssm1, &mut msm1, &ls1);
        };

        handles.push(thread::spawn(move || {
            bench("a_d_r_w_w sm", iters, bench_a_d_r_w_w_sm);
        }));

        let mut rsm2 = StdRng::seed_from_u64(0);
        let mut msm2: HashSet<(usize, Layout)> = HashSet::with_capacity_and_hasher(iters, RandomState::with_seed(rsm2.random::<u64>() as usize));
        let mut pssm2 = Vec::new();

        let sm2 = Arc::clone(&sm);
        let ls2 = ls.clone();
        let bench_a_d_r_sm = move || {
            help_test_one_alloc_dealloc_realloc(sm2.as_ref(), &mut rsm2, &mut pssm2, &mut msm2, &ls2);
        };

        handles.push(thread::spawn(move || {
            bench("a_d_r sm", iters, bench_a_d_r_sm);
        }));

        let mut rbi1 = StdRng::seed_from_u64(0);
        let mut mbi1: HashSet<(usize, Layout)> = HashSet::with_capacity_and_hasher(iters, RandomState::with_seed(rbi1.random::<u64>() as usize));
        let mut psbi1 = Vec::new();

        let bi = Arc::new(GlobalAllocWrap);

        let bi1 = Arc::clone(&bi);
        let lsbi1 = ls.clone();
        let bench_a_d_r_w_w_bi = move || {
            help_test_one_alloc_dealloc_realloc_with_writes(bi1.as_ref(), &mut rbi1, &mut psbi1, &mut mbi1, &lsbi1);
        };

        handles.push(thread::spawn(move || {
            bench("a_d_r_w_w bi", iters, bench_a_d_r_w_w_bi);
        }));

        let mut rbi2 = StdRng::seed_from_u64(0);
        let mut mbi2: HashSet<(usize, Layout)> = HashSet::with_capacity_and_hasher(iters, RandomState::with_seed(rbi2.random::<u64>() as usize));
        let mut psbi2 = Vec::new();

        let bi2 = Arc::clone(&bi);
        let lsbi2 = ls.clone();
        let bench_a_d_r_bi = move || {
            help_test_one_alloc_dealloc_realloc(bi2.as_ref(), &mut rbi2, &mut psbi2, &mut mbi2, &lsbi2);
        };

        handles.push(thread::spawn(move || {
            bench("a_d_r bi", iters, bench_a_d_r_bi);
        }));

        for handle in handles {
            handle.join().unwrap();
        }
    }
}

#[cfg(not(test))]
fn main() {
    notests::main();
}
