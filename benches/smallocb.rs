#![allow(unused_imports)]
//#![feature(allocator_api)]

//xxxuse std::hint::black_box;
use tango_bench::{benchmark_fn, tango_benchmarks, tango_main, IntoBenchmarks};
use std::sync::Arc;

use smalloc::Smalloc;
use smalloc::benches::{dummy_func, alloc_and_free};

//xxxuse std::alloc::Global;
use smalloc::benches::GlobalAllocWrap;

fn smallocb_benchmarks() -> impl IntoBenchmarks {
    //let s = Arc::new(Smalloc::new());
    //let s = Arc::new(GlobalAllocWrap);

    [
        benchmark_fn("smallocb", move |b| {
            b.iter(|| dummy_func(31, 30))
        })
        // benchmark_fn("smallocb", move |b| {
        //     let s_clone = s.clone();
        //     b.iter(move || alloc_and_free(&s_clone))
        // })
    ]
}

tango_benchmarks!(smallocb_benchmarks());
tango_main!();
