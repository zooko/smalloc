use tango_bench::{benchmark_fn, tango_benchmarks, tango_main, IntoBenchmarks};
use std::sync::Arc;

use smalloc::benchmarks::TestState;

use std::alloc::Layout;

use smalloc::{help_test_multithreaded_with_allocator, help_test_alloc_dealloc_realloc_with_writes, gen_layouts};

use tango_bench::{Bencher, ErasedSampler};

use smalloc::smallocb_allocator_config::{gen_allocator, AllocatorType};

fn gen_mt_bencher<F>(f: F, num_threads: u32, num_iters: u32, ls: Arc<Vec<Layout>>, al: Arc<AllocatorType>) -> impl FnMut(Bencher) -> Box<dyn ErasedSampler>
where
    F: Fn(&Arc<AllocatorType>, &mut TestState, &Arc<Vec<Layout>>) + Sync + Send + 'static + Copy
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

fn gen_st_bencher<F>(f: F, ls: Arc<Vec<Layout>>, al: Arc<AllocatorType>) -> impl FnMut(Bencher) -> Box<dyn ErasedSampler>
where
    F: Fn(&Arc<AllocatorType>, &mut TestState, &Arc<Vec<Layout>>) + Sync + Send + 'static + Copy
{
    let inter_ls = Arc::clone(&ls);
    let inter_al = Arc::clone(&al);
    let inter_s = Arc::new(TestState::new(1_000_000));
    let state_ptr = Arc::into_raw(inter_s);

    move |b: Bencher| {
        let local_al = Arc::clone(&inter_al);
        let local_ls = Arc::clone(&inter_ls);

        b.iter(move || {
            unsafe {
                f(&local_al, &mut *(state_ptr as *mut TestState), &local_ls);
            }
        })
    }
}

fn smallocb_benchmarks() -> impl IntoBenchmarks {
    let ls = Arc::new(gen_layouts());
    let al = gen_allocator();

    [
        benchmark_fn("alloc-free-re-and-write-0-threads",
                     gen_st_bencher(help_test_alloc_dealloc_realloc_with_writes, Arc::clone(&ls), Arc::clone(&al))
        ),

        benchmark_fn("alloc-free-re-and-write-32-threads-10K-iters",
                     gen_mt_bencher(help_test_alloc_dealloc_realloc_with_writes, 32, 10_000, Arc::clone(&ls), Arc::clone(&al))
        ),

        benchmark_fn("alloc-free-re-and-write-512-threads-10K-iters",
                     gen_mt_bencher(help_test_alloc_dealloc_realloc_with_writes, 512, 10_000, Arc::clone(&ls), Arc::clone(&al))
        ),

        benchmark_fn("alloc-free-re-and-write-8192-threads-10K-iters",
                     gen_mt_bencher(help_test_alloc_dealloc_realloc_with_writes, 8192, 10_000, Arc::clone(&ls), Arc::clone(&al))
        ),
    ]
}

tango_benchmarks!(smallocb_benchmarks());

use tango_bench::MeasurementSettings;

tango_main!(
    MeasurementSettings {
        ..Default::default()
    }
);
