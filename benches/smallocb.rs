use tango_bench::{benchmark_fn, tango_benchmarks, tango_main, IntoBenchmarks};
//use std::sync::Arc;

use smalloc::benchmarks::TestState;

// use smalloc::{help_test_multithreaded_with_allocator, adrww, gen_layouts};
use smalloc::{adrww, adr, adww, ad, aww, a};

use tango_bench::{Bencher, ErasedSampler};

//use smalloc::smallocb_allocator_config::{gen_allocator, AllocatorType};
use smalloc::smallocb_allocator_config::AllocatorType;

// fn gen_mt_bencher<F>(f: F, num_threads: u32, num_iters: u32, ls: Arc<Vec<Layout>>, al: Arc<AllocatorType>) -> impl FnMut(Bencher) -> Box<dyn ErasedSampler>
// where
//     F: Fn(&Arc<AllocatorType>, &mut TestState, &Arc<Vec<Layout>>) + Sync + Send + 'static + Copy
// {
//     let inter_ls = Arc::clone(&ls);
//     let inter_al = Arc::clone(&al);

//     move |b: Bencher| {
//         let local_al = Arc::clone(&inter_al);
//         let local_ls = Arc::clone(&inter_ls);

//         b.iter(move || {
//             help_test_multithreaded_with_allocator(f, num_threads, num_iters, &local_al, &local_ls);
//         })
//     }
// }

fn gen_st_bencher<F>(f: F) -> impl FnMut(Bencher) -> Box<dyn ErasedSampler>
where
    F: Fn(&AllocatorType, &mut TestState) + Sync + Send + 'static + Copy
{
    use smalloc::smallocb_allocator_config::gen_allocator;
    let al_ptr = Box::leak(Box::new(gen_allocator())) as *const AllocatorType;

    let ts_ptr = Box::leak(Box::new(TestState::new(1_000_000))) as *mut TestState;

    move |b: Bencher| {
        b.iter(move || {
            unsafe {
                f(&*al_ptr, &mut *ts_ptr);
            }
        })
    }
}

fn smallocb_benchmarks() -> impl IntoBenchmarks {
    [
        benchmark_fn("adrww",
                     gen_st_bencher(adrww)
        ),

        benchmark_fn("adr",
                     gen_st_bencher(adr)
        ),

        benchmark_fn("adww",
                     gen_st_bencher(adww)
        ),

        benchmark_fn("ad",
                     gen_st_bencher(ad)
        ),

        benchmark_fn("aww",
                     gen_st_bencher(aww)
        ),

        benchmark_fn("a",
                     gen_st_bencher(a)
        ),

        // benchmark_fn("alloc-free-re-and-write-32-threads-10K-iters",
        //              gen_mt_bencher(adrww, 32, 10_000, Arc::clone(&ls), Arc::clone(&al))
        // ),

        // benchmark_fn("alloc-free-re-and-write-512-threads-10K-iters",
        //              gen_mt_bencher(adrww, 512, 10_000, Arc::clone(&ls), Arc::clone(&al))
        // ),

        // benchmark_fn("alloc-free-re-and-write-512-threads-100K-iters",
        //              gen_mt_bencher(adrww, 512, 100_000, Arc::clone(&ls), Arc::clone(&al))
        // ),

        // benchmark_fn("alloc-free-re-and-write-8192-threads-10K-iters",
        //              gen_mt_bencher(adrww, 8192, 10_000, Arc::clone(&ls), Arc::clone(&al))
        // ),
    ]
}

tango_benchmarks!(smallocb_benchmarks());

use tango_bench::MeasurementSettings;

tango_main!(
    MeasurementSettings {
        ..Default::default()
    }
);
