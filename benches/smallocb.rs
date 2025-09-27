#![allow(unused_imports)]

use tango_bench::{benchmark_fn, tango_benchmarks, tango_main, IntoBenchmarks};
use std::sync::Arc;

use smalloc::Smalloc;
use smalloc::benchmarks::{dummy_func, alloc_and_free, GlobalAllocWrap, TestState};

use std::alloc::Layout;

use std::cmp::max;

use rand::rngs::StdRng;
use ahash::HashSet;
use ahash::RandomState;
use smalloc::{help_test_multithreaded_with_allocator, help_test_alloc_dealloc_realloc_with_writes, help_test_alloc_dealloc_realloc, gen_layouts};
use rand::SeedableRng;
use rand::Rng;

use tango_bench::{Bencher, ErasedSampler};

use std::alloc::GlobalAlloc;
use std::cell::RefCell;

mod smallocb_allocator_config;
use smallocb_allocator_config::gen_allocator;

use std::rc::Rc;

fn gen_mt_bencher<T, F>(f: F, num_threads: u32, num_iters: u32, al: Arc<T>, ls: Arc<Vec<Layout>>) -> impl FnMut(Bencher) -> Box<dyn ErasedSampler>
where
      T: GlobalAlloc + ?Sized + Sync + Send + 'static,
      F: Fn(&Arc<T>, u32, &mut TestState, &Arc<Vec<Layout>>) + Sync + Send + 'static + Copy
{
    let al_inter = Arc::clone(&al);
    let ls_inter = Arc::clone(&ls);

    move |b: Bencher| {
        let local_al = Arc::clone(&al_inter);
        let local_ls = Arc::clone(&ls_inter);

        b.iter(move || {
            help_test_multithreaded_with_allocator(f, num_threads, num_iters, &local_al, &local_ls);
        })
    }
}

fn smallocb_benchmarks() -> impl IntoBenchmarks {
    let al = gen_allocator();
    let ls = Arc::new(gen_layouts());

    const NUM_ITERS: u32 = 100_000;

    [
        benchmark_fn("madrww1",
                     gen_mt_bencher(help_test_alloc_dealloc_realloc_with_writes, 1, NUM_ITERS, Arc::clone(&al), Arc::clone(&ls))
        ),
        benchmark_fn("madrww32",
                     gen_mt_bencher(help_test_alloc_dealloc_realloc_with_writes, 32, NUM_ITERS, Arc::clone(&al), Arc::clone(&ls))
        ),
        benchmark_fn("madrww2048",
                     gen_mt_bencher(help_test_alloc_dealloc_realloc_with_writes, 2048, NUM_ITERS, Arc::clone(&al), Arc::clone(&ls))
        ),
    ]
}

tango_benchmarks!(smallocb_benchmarks());

use tango_bench::MeasurementSettings;
use tango_bench::SampleLengthKind::Flat;

tango_main!(
    MeasurementSettings {
        sampler_type: Flat,
        cache_firewall: Some(36864), // For my Apple M4 Max
        max_iterations_per_sample: 1,

        ..Default::default()
    }
);
