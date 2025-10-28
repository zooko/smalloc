use tango_bench::{benchmark_fn, tango_benchmarks, tango_main, IntoBenchmarks};
use std::sync::Arc;

use smalloc::benchmarks::TestState;

use std::alloc::Layout;

use smalloc::{help_test_multithreaded_with_allocator, help_test_alloc_dealloc_realloc_with_writes, gen_layouts};

use tango_bench::{Bencher, ErasedSampler};

use smalloc::smallocb_allocator_config::{gen_allocator, AllocatorType};

fn gen_mt_bencher<F>(f: F, num_threads: u32, num_iters: u32, ls: Arc<Vec<Layout>>, al: Arc<AllocatorType>) -> impl FnMut(Bencher) -> Box<dyn ErasedSampler>
where
    F: Fn(&Arc<AllocatorType>, u32, &mut TestState, &Arc<Vec<Layout>>) + Sync + Send + 'static + Copy
{
    let inter_ls = Arc::clone(&ls);
    let inter_al = Arc::clone(&al);

    move |b: Bencher| {
        let local_al = Arc::clone(&inter_al);
        let local_ls = Arc::clone(&inter_ls);

        b.iter(move || {
            help_test_multithreaded_with_allocator(f, num_threads, num_iters, &local_al, &local_ls);
        })
    }
}

fn gen_st_bencher<F>(f: F, num_iters: u32, ls: Arc<Vec<Layout>>, al: Arc<AllocatorType>) -> impl FnMut(Bencher) -> Box<dyn ErasedSampler>
where
    F: Fn(&Arc<AllocatorType>, u32, &mut TestState, &Arc<Vec<Layout>>) + Sync + Send + 'static + Copy
{
    let inter_ls = Arc::clone(&ls);
    let inter_al = Arc::clone(&al);

    move |b: Bencher| {
        let local_al = Arc::clone(&inter_al);
        let local_ls = Arc::clone(&inter_ls);

        b.iter(move || {
            let mut s = TestState::new(num_iters);
            f(&local_al, num_iters, &mut s, &local_ls);
        })
    }
}

fn smallocb_benchmarks() -> impl IntoBenchmarks {
    let ls = Arc::new(gen_layouts());
    let al = gen_allocator();

    [
        benchmark_fn("alloc-free-re-and-write-0-threads-1-iters",
                     gen_st_bencher(help_test_alloc_dealloc_realloc_with_writes, 1, Arc::clone(&ls), Arc::clone(&al))
        ),

        benchmark_fn("alloc-free-re-and-write-0-threads-10-iters",
                     gen_st_bencher(help_test_alloc_dealloc_realloc_with_writes, 10, Arc::clone(&ls), Arc::clone(&al))
        ),

        benchmark_fn("alloc-free-re-and-write-0-threads-100-iters",
                     gen_st_bencher(help_test_alloc_dealloc_realloc_with_writes, 100, Arc::clone(&ls), Arc::clone(&al))
        ),

        benchmark_fn("alloc-free-re-and-write-0-threads-1K-iters",
                     gen_st_bencher(help_test_alloc_dealloc_realloc_with_writes, 1000, Arc::clone(&ls), Arc::clone(&al))
        ),

        benchmark_fn("alloc-free-re-and-write-0-threads-8K-iters",
                     gen_st_bencher(help_test_alloc_dealloc_realloc_with_writes, 8000, Arc::clone(&ls), Arc::clone(&al))
        ),

        benchmark_fn("alloc-free-re-and-write-0-threads-10K-iters",
                     gen_st_bencher(help_test_alloc_dealloc_realloc_with_writes, 10_000, Arc::clone(&ls), Arc::clone(&al))
        ),

        benchmark_fn("alloc-free-re-and-write-1-threads-1K-iters",
                     gen_mt_bencher(help_test_alloc_dealloc_realloc_with_writes, 1, 1000, Arc::clone(&ls), Arc::clone(&al))
        ),

        benchmark_fn("alloc-free-re-and-write-32-threads-10K-iters",
                     gen_mt_bencher(help_test_alloc_dealloc_realloc_with_writes, 32, 10_000, Arc::clone(&ls), Arc::clone(&al))
        ),

        benchmark_fn("alloc-free-re-and-write-32-threads-100K-iters",
                     gen_mt_bencher(help_test_alloc_dealloc_realloc_with_writes, 32, 100_000, Arc::clone(&ls), Arc::clone(&al))
        ),

        benchmark_fn("alloc-free-re-and-write-512-threads-10K-iters",
                     gen_mt_bencher(help_test_alloc_dealloc_realloc_with_writes, 512, 10_000, Arc::clone(&ls), Arc::clone(&al))
        ),

        benchmark_fn("alloc-free-re-and-write-512-threads-100K-iters",
                     gen_mt_bencher(help_test_alloc_dealloc_realloc_with_writes, 512, 100_000, Arc::clone(&ls), Arc::clone(&al))
        ),

        benchmark_fn("alloc-free-re-and-write-512-threads-1M-iters",
                     gen_mt_bencher(help_test_alloc_dealloc_realloc_with_writes, 512, 1_000_000, Arc::clone(&ls), Arc::clone(&al))
        ),

        // benchmark_fn("alloc-free-re-and-write-1-threads-1-iters",
        //              gen_mt_bencher(help_test_alloc_dealloc_realloc_with_writes, 1, 1, Arc::clone(&ls))
        //              // gen_mt_bencher(help_test_dummy_func, 1, 100_000, /* Arc::clone(&al), */Arc::clone(&ls))
        // ),
        // benchmark_fn("alloc-free-re-and-write-1-threads-10-iters",
        //              gen_mt_bencher(help_test_alloc_dealloc_realloc_with_writes, 1, 10, Arc::clone(&ls))
        //              // gen_mt_bencher(help_test_dummy_func, 1, 100_000, /* Arc::clone(&al), */Arc::clone(&ls))
        // ),
        // benchmark_fn("alloc-free-re-and-write-1-threads-100-iters",
        //              gen_mt_bencher(help_test_alloc_dealloc_realloc_with_writes, 1, 100, Arc::clone(&ls), Arc::clone(&al))
        //              // gen_mt_bencher(help_test_dummy_func, 1, 100_000, /* Arc::clone(&al), */Arc::clone(&ls))
        // ),
        // benchmark_fn("alloc-free-re-and-write-1-threads-10K-iters",
        //              gen_mt_bencher(help_test_alloc_dealloc_realloc_with_writes, 1, 10_000, Arc::clone(&ls), Arc::clone(&al))
        //              // gen_mt_bencher(help_test_dummy_func, 1, 100_000, /* Arc::clone(&al), */Arc::clone(&ls))
        // ),
        // benchmark_fn("alloc-free-re-and-write-1-threads-100K-iters",
        //              gen_mt_bencher(help_test_alloc_dealloc_realloc_with_writes, 1, 100_000, Arc::clone(&ls), Arc::clone(&al))
        //              // gen_mt_bencher(help_test_dummy_func, 1, 100_000, /* Arc::clone(&al), */Arc::clone(&ls))
        // ),
        // benchmark_fn("alloc-free-re-and-write-1-threads-400K-iters",
        //              gen_mt_bencher(help_test_alloc_dealloc_realloc_with_writes, 1, 400_000, Arc::clone(&ls), Arc::clone(&al))
        //              // gen_mt_bencher(help_test_dummy_func, 1, 100_000, /* Arc::clone(&al), */Arc::clone(&ls))
        // ),
        // benchmark_fn("alloc-free-re-and-write-2-threads-400K-iters",
        //              gen_mt_bencher(help_test_alloc_dealloc_realloc_with_writes, 2, 400_000, Arc::clone(&ls), Arc::clone(&al))
        //              // gen_mt_bencher(help_test_dummy_func, 1, 100_000, /* Arc::clone(&al), */Arc::clone(&ls))
        // ),
        // benchmark_fn("alloc-free-re-and-write-4-threads-100K-iters",
        //              gen_mt_bencher(help_test_alloc_dealloc_realloc_with_writes, 4, 100_000, Arc::clone(&ls), Arc::clone(&al))
        //              // gen_mt_bencher(help_test_dummy_func, 1, 100_000, /* Arc::clone(&al), */Arc::clone(&ls))
        // ),
        // benchmark_fn("alloc-free-re-and-write-4-threads-400K-iters",
        //              gen_mt_bencher(help_test_alloc_dealloc_realloc_with_writes, 4, 400_000, Arc::clone(&ls), Arc::clone(&al))
        //              // gen_mt_bencher(help_test_dummy_func, 1, 100_000, /* Arc::clone(&al), */Arc::clone(&ls))
        // ),
        // benchmark_fn("alloc-free-re-and-write-4-threads-1M-iters",
        //              gen_mt_bencher(help_test_alloc_dealloc_realloc_with_writes, 4, 1_000_000, Arc::clone(&ls), Arc::clone(&al))
        //              // gen_mt_bencher(help_test_dummy_func, 1, 100_000, /* Arc::clone(&al), */Arc::clone(&ls))
        // ),
        // benchmark_fn("alloc-free-re-and-write-4-threads-2M-iters",
        //              gen_mt_bencher(help_test_alloc_dealloc_realloc_with_writes, 4, 2_000_000, Arc::clone(&ls), Arc::clone(&al))
        //              // gen_mt_bencher(help_test_dummy_func, 1, 100_000, /* Arc::clone(&al), */Arc::clone(&ls))
        // ),
        // benchmark_fn("alloc-free-re-and-write-8-threads-100K-iters",
        //              gen_mt_bencher(help_test_alloc_dealloc_realloc_with_writes, 8, 100_000, Arc::clone(&ls), Arc::clone(&al))
        //              // gen_mt_bencher(help_test_dummy_func, 1, 100_000, /* Arc::clone(&al), */Arc::clone(&ls))
        // ),
        // benchmark_fn("alloc-free-re-and-write-512-threads-100K-iters",
        //              gen_mt_bencher(help_test_alloc_dealloc_realloc_with_writes, 512, 100_000, Arc::clone(&ls), Arc::clone(&al))
        //              // gen_mt_bencher(help_test_dummy_func, 1, 100_000, /* Arc::clone(&al), */Arc::clone(&ls))
        // ),

        // benchmark_fn("alloc-free-re-and-write-0-threads-100K-iters",
        //              gen_st_bencher(help_test_alloc_dealloc_realloc_with_writes, 100_000, Arc::clone(&ls), Arc::clone(&al))
        //              // gen_st_bencher(help_test_dummy_func, 1, 100_000, /* Arc::clone(&al), */Arc::clone(&ls))
        // ),
        // benchmark_fn("alloc-free-re-and-write-0-threads-1M-iters",
        //              gen_st_bencher(help_test_alloc_dealloc_realloc_with_writes, 1_000_000, Arc::clone(&ls), Arc::clone(&al))
        //              // gen_st_bencher(help_test_dummy_func, 1, 100_000, /* Arc::clone(&al), */Arc::clone(&ls))
        // ),
//xxx        benchmark_fn("alloc-free-re-and-write-12-threads-10M-iters",
//xxx                     gen_mt_bencher(help_test_alloc_dealloc_realloc_with_writes, 12, 10_000_000, /* Arc::clone(&al), */Arc::clone(&ls))
//xxx        ),
//xxx        benchmark_fn("alloc-free-re-and-write-32-threads-1M-iters",
//xxx                     gen_mt_bencher(help_test_alloc_dealloc_realloc_with_writes, 32, 1_000_000, /* Arc::clone(&al), */Arc::clone(&ls))
//xxx                     // gen_mt_bencher(help_test_dummy_func, 32, 1_000_000, /* Arc::clone(&al), */Arc::clone(&ls))
//xxx        ),
        // benchmark_fn("alloc-free-re-and-write-512-threads-10K-iters",
        //              gen_mt_bencher(help_test_alloc_dealloc_realloc_with_writes, 512, 10_000, Arc::clone(&ls), Arc::clone(&al))
        //              // gen_mt_bencher(help_test_dummy_func, 512, 10_000, /* Arc::clone(&al), */Arc::clone(&ls))
        // ),
        // benchmark_fn("alloc-free-re-and-write-512-threads-1M-iters",
        //              gen_mt_bencher(help_test_alloc_dealloc_realloc_with_writes, 512, 1_000_000, Arc::clone(&ls), Arc::clone(&al))
        // ),
        // benchmark_fn("alloc-free-re-and-write-512-threads-10M-iters",
        //              gen_mt_bencher(help_test_alloc_dealloc_realloc_with_writes, 512, 10_000_000, /* Arc::clone(&al), */Arc::clone(&ls))
        // ),
//xxx        benchmark_fn("alloc-free-re-and-write-2048-threads-10K-iters",
//xxx                     gen_mt_bencher(help_test_alloc_dealloc_realloc_with_writes, 2048, 10_000, /* Arc::clone(&al), */Arc::clone(&ls))
//xxx                     // gen_mt_bencher(help_test_dummy_func, 2048, 10_000, /* Arc::clone(&al), */Arc::clone(&ls))
//xxx        ),
    ]
}

tango_benchmarks!(smallocb_benchmarks());

use tango_bench::MeasurementSettings;

tango_main!(
    MeasurementSettings {
        ..Default::default()
    }
);
