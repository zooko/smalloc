#![allow(unused_imports)]

use tango_bench::{benchmark_fn, tango_benchmarks, tango_main, IntoBenchmarks};
use std::sync::Arc;

use smalloc::Smalloc;
use smalloc::benches::{dummy_func, alloc_and_free};

use smalloc::benches::GlobalAllocWrap;

use std::alloc::Layout;

use std::cmp::max;

use rand::rngs::StdRng;
use ahash::HashSet;
use ahash::RandomState;
use smalloc::{help_test_one_alloc_dealloc_realloc_with_writes, help_test_one_alloc_dealloc_realloc};
use rand::SeedableRng;
use rand::Rng;

use tango_bench::Bencher;

fn smallocb_benchmarks() -> impl IntoBenchmarks {
    let mut s = GlobalAllocWrap; // comment-in for tango baseline
    //let mut s = Smalloc::new(); // comment-in to compare to smalloc

    let l = Layout::from_size_align(35, 32).unwrap();

    let l1 = l;
    let mut ls = Vec::new();
    ls.push(l1);
    let l2 = Layout::from_size_align(l1.size() + 10, l1.align()).unwrap();
    ls.push(l2);
    let l3 = Layout::from_size_align(max(11, l1.size()) - 10, l1.align()).unwrap();
    ls.push(l3);
    let l4 = Layout::from_size_align(l1.size() * 2 + 10, l1.align()).unwrap();
    ls.push(l4);

    let mut r = StdRng::seed_from_u64(0);

    let mut m: HashSet<(usize, Layout)> = HashSet::with_capacity_and_hasher(100_000_000, RandomState::with_seed(r.random::<u64>() as usize));

    let mut ps = Vec::new();
    
    let bbb = move |b: Bencher| {
        let s_ptr = &mut s as *mut _;
        let r_ptr = &mut r as *mut _;
        let ps_ptr = &mut ps as *mut _;
        let m_ptr = &mut m as *mut _;
        let ls_ptr = ls.as_slice() as *const [Layout];

        b.iter(move || unsafe {
            for _i in 0..1000 {
                help_test_one_alloc_dealloc_realloc_with_writes(
                    &*s_ptr,
                    &mut *r_ptr,
                    &mut *ps_ptr,
                    &mut *m_ptr,
                    &*ls_ptr
                )
            }
        })
    };

    [
        benchmark_fn("smallocb", bbb)
    ]
}

tango_benchmarks!(smallocb_benchmarks());
tango_main!();
