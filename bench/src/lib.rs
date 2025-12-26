// Thanks to Claude Opus 4.5 for refactoring this entire file with me.

mod platform;
use platform::ClockType;
use std::mem::MaybeUninit;

use std::alloc::GlobalAlloc;
pub struct GlobalAllocWrap;

pub use thousands::Separable;

pub const NUM_BATCHES: u64 = 20;

pub fn clock(clocktype: ClockType) -> u64 {
    let mut tp: MaybeUninit<libc::timespec> = MaybeUninit::uninit();
    let retval = unsafe { libc::clock_gettime(clocktype, tp.as_mut_ptr()) };
    debug_assert_eq!(retval, 0);
    let instsec = unsafe { (*tp.as_ptr()).tv_sec };
    let instnsec = unsafe { (*tp.as_ptr()).tv_nsec };
    debug_assert!(instsec >= 0);
    debug_assert!(instnsec >= 0);
    instsec as u64 * 1_000_000_000 + instnsec as u64
}

use std::alloc::{System, Layout};

unsafe impl GlobalAlloc for GlobalAllocWrap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, reqsize: usize) -> *mut u8 {
        unsafe { System.realloc(ptr, layout, reqsize) }
    }
}

use std::sync::Arc;
pub fn alloc_and_free(al: &Arc<impl GlobalAlloc>) {
    let l = unsafe { Layout::from_size_align_unchecked(32, 1) };
    let p = unsafe { al.alloc(l) };
    unsafe { *p = 0 };
    unsafe { al.dealloc(p, l) };
}

use devutils::*;

/// Returns elapsed nanoseconds (best of NUM_BATCHES batches)
pub fn singlethread_bench<T, F>(bf: F, iters_per_batch: u64, name: &str, al: &T, seed: u64) -> u64
where
    T: GlobalAlloc,
    F: Fn(&T, &mut TestState) + Sync + Send + Copy + 'static
{
    let mut best_ns = u64::MAX;

    for _batch in 0..NUM_BATCHES {
        let mut s = TestState::new(iters_per_batch, seed);
        let start = clock(libc::CLOCK_THREAD_CPUTIME_ID);
        for _i in 0..iters_per_batch {
            bf(al, &mut s);
        }
        let end = clock(libc::CLOCK_THREAD_CPUTIME_ID);
        s.clean_up(al);

        let batch_ns = end - start;
        if batch_ns < best_ns {
            best_ns = batch_ns;
        }
    }

    let nspi = best_ns / iters_per_batch;
    let hundredpses_per_iter = ((best_ns * 10) / iters_per_batch) % 10;
    println!("name: {name:>16}, threads:     1, iters: {:>10}, ns: {:>14}, ns/i: {:>8}.{hundredpses_per_iter}", iters_per_batch.separate_with_commas(), best_ns.separate_with_commas(), nspi.separate_with_commas());

    best_ns
}

pub fn multithread_bench<T, F>(bf: F, threads: u32, iters_per_batch: u64, name: &str, al: &T, seed: u64) -> u64
where
    T: GlobalAlloc + Send + Sync,
    F: Fn(&T, &mut TestState) + Sync + Send + Copy + 'static
{
    let mut best_ns = u64::MAX;

    let mut tses: Vec<TestState> = Vec::with_capacity(threads as usize);
    for _i in 0..threads {
        tses.push(TestState::new(iters_per_batch, seed));
    }

    for _batch in 0..NUM_BATCHES {
        let start = clock(libc::CLOCK_MONOTONIC_RAW);
        help_test_multithreaded_with_allocator(bf, threads, iters_per_batch, al, &mut tses);
        let end = clock(libc::CLOCK_MONOTONIC_RAW);
        let batch_ns = end - start;
        if batch_ns < best_ns {
            best_ns = batch_ns;
        }
    }

    let nspi = best_ns / iters_per_batch;
    let hundredpses_per_iter = ((best_ns * 10) / iters_per_batch) % 10;
    println!("name: {name:>16}, threads: {:>5}, iters: {:>10}, ns: {:>14}, ns/i: {:>8}.{hundredpses_per_iter}", threads.separate_with_commas(), iters_per_batch.separate_with_commas(), best_ns.separate_with_commas(), nspi.separate_with_commas());

    // Dealloc all allocations so that we don't run out of space.
    for mut ts in tses {
        ts.clean_up(al);
    }

    best_ns
}

use std::sync::Barrier;
use std::thread;

#[macro_export]
    macro_rules! multithread_hotspot {
    ($f:expr, $threads:expr, $iters:expr, $al:expr, $l:expr) => {{
        let name = concat!("hs-", stringify!($f));
        $crate::multithread_hotspot_inner($f, $threads, $iters, name, $al, $l)
    }};
}

#[macro_export]
    macro_rules! multithread_free_hotspot {
    ($threads:expr, $iters:expr, $al:expr, $l:expr) => {{
        let name = format!("fh-{}", $threads);
        $crate::multithread_free_hotspot_inner($threads, $iters, &name, $al, $l)
    }};
}

/// Wrapper to mark raw pointers as Send for cross-thread deallocation
#[derive(Clone, Copy)]
struct SendPtr(*mut u8);

unsafe impl Send for SendPtr {}

/// This is to stress test the case that many threads are simultaneously free'ing slots in the same
/// slab as each other.
///
/// Returns picoseconds per free. Not picoseconds per (free per thread). Picoseconds per free. The
/// number of frees is equal to `numthreads` * `iters_per_thread`.
///
/// Thanks to Claude Opus 4.5 for writing 90% of this function for me.
pub fn multithread_free_hotspot_inner<A>(numthreads: u32, iters_per_thread: u64, name: &str, al: &A, l: Layout) -> u64
where
    A: GlobalAlloc + Send + Sync
{
    let iters_per_thread = iters_per_thread as usize;
    let numthreads = numthreads as usize;
    let tot_iters = iters_per_thread * numthreads;

    let mut best_ps = u64::MAX;

    for _batch in 0..NUM_BATCHES {
        // Allocate all pointers upfront
        let mut pointers: Vec<SendPtr> = Vec::with_capacity(tot_iters);
        for _ in 0..tot_iters {
            let p = unsafe { al.alloc(l) };
            assert!(!p.is_null());
            pointers.push(SendPtr(p));
        }

        // Split pointers among threads
        let chunks: Vec<Vec<SendPtr>> = pointers
            .chunks(iters_per_thread)
            .map(|c| c.to_vec())
            .collect();

        let start_barrier = Barrier::new(numthreads + 1);
        let end_barrier = Barrier::new(numthreads + 1);

        let elap_ps = thread::scope(|s| {
            for chunk in &chunks {
                let chunk = chunk.clone();
                s.spawn(|| {
                    start_barrier.wait();

                    for SendPtr(p) in chunk {
                        unsafe { al.dealloc(p, l) };
                    }

                    end_barrier.wait();
                });
            }

            start_barrier.wait();
            // Start timing right before releasing threads
            let start = clock(libc::CLOCK_MONOTONIC_RAW);

            // Wait for all threads to finish
            end_barrier.wait();
            let end = clock(libc::CLOCK_MONOTONIC_RAW);

            assert!(end > start);
            (end - start) * 1000
        });

        if elap_ps < best_ps {
            best_ps = elap_ps;
        }
    }

    let pspi = best_ps / tot_iters as u64;
    let hundredpses = (pspi / 100) % 10;
    let nspi = pspi / 1000;
    let best_ns = best_ps / 1000;
    println!("name: {name:>16}, threads: {:>5}, iters: {:>10}, ns: {:>14}, ns/i: {:>8}.{hundredpses:1}", numthreads.separate_with_commas(), tot_iters.separate_with_commas(), best_ns.separate_with_commas(), nspi.separate_with_commas());

    best_ps / tot_iters as u64
}

/// This is to stress test the case that one slab's flh is under heavy multi-threading contention
/// but the other slabs's flh's are not.
///
/// Spawn 64 * `hotspot_threads` threads, each of which allocates one slot then blocks. All of the
/// ones that are the %64'th to first-allocate then get unblocked, and proceed to deallocate,
/// allocate, deallocate, allocate etc. `iters` times.
///
/// Returns picoseconds per iters_pbpht. Not picoseconds per (iters * hotspot_threads), nor yet
/// picoseconds per (iters * total_threads). Picoseconds per iter-per-batch-per-hot-thread.
///
/// iters_pbpht: iters per batch per hot thread
///
/// Thanks to Claude Opus 4.5 for writing 90% of this function for me.
pub fn multithread_hotspot_inner<T, F>(f: F, hotspot_threads: u32, iters_pbpht: u64, name: &str, al: &T, l: Layout) -> u64
where
    T: GlobalAlloc + Send + Sync,
    F: Fn(&T, &mut TestState) + Sync + Send + Copy + 'static
{
    // If you want to stress test smalloc, it is best for this to equal 2^NUM_SLABS_BITS.
    const NUM_SLABS: usize = 32;

    let hotspot_threads_usize = hotspot_threads as usize;
    let cool_threads_per_round: usize = NUM_SLABS - 1;

    // One hot thread per 63 cool threads
    let total_threads: usize = hotspot_threads_usize * (1 + cool_threads_per_round);

    let mut best_ns = u64::MAX;

    for _batch in 0..NUM_BATCHES {
        let hot_done_barriers: Vec<Barrier> = (0..hotspot_threads_usize)
            .map(|_| Barrier::new(2))
            .collect();

        let cool_done_barriers: Vec<Barrier> = (0..hotspot_threads_usize)
            .map(|_| Barrier::new(cool_threads_per_round + 1))
            .collect();

        let setup_complete_barrier = Barrier::new(total_threads + 1);
        let hot_start_barrier = Barrier::new(hotspot_threads_usize + 1);
        let hot_finish_barrier = Barrier::new(hotspot_threads_usize + 1);
        let final_barrier = Barrier::new(total_threads + 1);

        let batch_ns = thread::scope(|s| {
            for round in 0..hotspot_threads_usize {
                // Extract references before spawning
                let hot_barrier = &hot_done_barriers[round];
                let cool_barrier = &cool_done_barriers[round];

                // Spawn hot thread
                s.spawn(|| {
                    let _ptr = unsafe { al.alloc(l) }; // xxx this gets leaked oh well
                    let mut ts = TestState::new(iters_pbpht, 0);

                    hot_barrier.wait();

                    setup_complete_barrier.wait();
                    hot_start_barrier.wait();

                    for _ in 0..iters_pbpht {
                        f(al, &mut ts);
                    }

                    hot_finish_barrier.wait();
                    ts.clean_up(al);
                    final_barrier.wait();
                });

                hot_barrier.wait();

                // Spawn cool threads
                for _ in 0..cool_threads_per_round {
                    s.spawn(|| {
                        let _ptr = unsafe { al.alloc(l) }; // xxx this gets leaked oh well
                        cool_barrier.wait();
                        setup_complete_barrier.wait();
                        final_barrier.wait();
                    });
                }

                cool_barrier.wait();
            }

            setup_complete_barrier.wait();

            // Start timing right before releasing hot threads
            let start = clock(libc::CLOCK_MONOTONIC_RAW);
            hot_start_barrier.wait();
            // Wait for all hot threads to finish
            hot_finish_barrier.wait();
            let end = clock(libc::CLOCK_MONOTONIC_RAW);

            final_barrier.wait();

            assert!(end > start);
            end - start
        });

        if batch_ns < best_ns {
            best_ns = batch_ns;
        }
    }

    let elap_ps = best_ns * 1000;
    let pspi = elap_ps / iters_pbpht;
    let hundredpses = (pspi / 100) % 10;
    let nspi = pspi / 1000;
    println!("name: {name:>16}, threads: {:>5}, iters: {:>10}, ns: {:>14}, ns/i: {:>8}.{hundredpses:1}", hotspot_threads.separate_with_commas(), iters_pbpht.separate_with_commas(), best_ns.separate_with_commas(), nspi.separate_with_commas());

    pspi
}

/// Print comparison percentages
pub fn print_comparisons(candidate_ns: u64, baselines: &[(&str, u64)]) {
    for (name, baseline_ns) in baselines {
        let diff_perc = 100.0 * (candidate_ns as f64 - *baseline_ns as f64) / (*baseline_ns as f64);
        println!("smalloc diff from {:>8}: {:+4.0}%", name, diff_perc);
    }
    println!();
}

// ============================================================================
// ALLOCATOR REGISTRY - Add new allocators here!
// ============================================================================
//
// Each allocator entry is: (short_name, display_name, constructor, setup_block)
//
// - short_name: Used for benchmark output prefixes (e.g., "mm" -> "mm_st_funcname-1")  
// - display_name: Used in comparison output (e.g., "smalloc diff from mimalloc: +5%")
// - constructor: Expression that creates the allocator instance
// - setup_block: Code to run after construction (use {} for none)
//
// The LAST entry is treated as the "candidate" (the one being compared against others).

#[macro_export]
macro_rules! with_all_allocators {
    ($($macro_path:tt)::+ ! ( $($args:tt)* )) => {
        $($macro_path)::+! {
            $($args)* ;
            @allocators
                de, "default",  $crate::GlobalAllocWrap,       {}; // This causes a crash on Macos+M4Max
            mm, "mimalloc", mimalloc::MiMalloc,            {};
            jm, "jemalloc", tikv_jemallocator::Jemalloc,   {};
            nm, "snmalloc", snmalloc_rs::SnMalloc,         {};
            rp, "rpmalloc", rpmalloc::RpMalloc,            {};
            @candidate
                sm, "smalloc",  devutils::get_devsmalloc!(),   {};
        }
    };
}

// ============================================================================
// Single-threaded benchmarks
// ============================================================================

#[macro_export]
macro_rules! st_bench {
    ($func:path, $iters_per_batch:expr, $seed:expr) => {{
        let sm = devutils::get_devsmalloc!();
        sm.idempotent_init();

        let func_name = stringify!($func);
        let f = |al: &smalloc::Smalloc, s: &mut TestState| { $func(al, s) };
        let name = format!("sm_st_{func_name}-1");
        $crate::singlethread_bench(f, $iters_per_batch, &name, &sm, $seed);
    }};
}

#[macro_export]
macro_rules! compare_st_bench_impl {
    (
        $func:path, $iters_per_batch:expr, $seed:expr ;
        @allocators $( $short:ident, $display:expr, $instance:expr, { $($setup:tt)* } );+ ;
        @candidate $cand_short:ident, $cand_display:expr, $cand_instance:expr, { $($cand_setup:tt)* };
    ) => {{
        use $crate::Separable;
        use std::sync::atomic::{AtomicU64, Ordering};
        let func_name = stringify!($func);

        const NUM_BATCHES: u64 = $crate::NUM_BATCHES;

        // Create all allocator instances and run setup
        $(
            let $short = $instance;
            $($setup)*
        )+
            let $cand_short = $cand_instance;
        $($cand_setup)*

        // Create atomic storage for results
            let results: Vec<(AtomicU64, &str)> = vec![
                $( (AtomicU64::new(0), $display), )+
            ];
        let candidate_result = AtomicU64::new(0);

        // Spawn threads for each allocator
        std::thread::scope(|scope| {
            let mut _idx = 0usize;
            $(
                let result_ref = &results[_idx].0;
                let alloc_ref = &$short;
                let short_str = stringify!($short);
                let iters_per_batch = $iters_per_batch;
                let seed = $seed;
                scope.spawn(move || {
                    let mut best_ns = u64::MAX;

                    for _batch in 0..NUM_BATCHES {
                        let mut s = devutils::TestState::new(iters_per_batch, seed);
                        let start = $crate::clock(libc::CLOCK_THREAD_CPUTIME_ID);
                        for _i in 0..iters_per_batch {
                            $func(alloc_ref, &mut s);
                        }
                        let end = $crate::clock(libc::CLOCK_THREAD_CPUTIME_ID);
                        s.clean_up(alloc_ref);
                        let batch_ns = end - start;
                        if batch_ns < best_ns {
                            best_ns = batch_ns;
                        }
                    }

                    let nspi = best_ns / iters_per_batch;
                    let frac = ((best_ns * 10) / iters_per_batch) % 10;
                    let name = format!("{}_st_{}-1", short_str, stringify!($func));
                    println!("name: {:>16}, threads:     1, iters: {:>10}, ns: {:>14}, ns/i: {:>8}.{}", name, iters_per_batch.separate_with_commas(), best_ns.separate_with_commas(), nspi.separate_with_commas(), frac);

                    result_ref.store(best_ns, Ordering::Relaxed);
                });
                _idx += 1;
            )+

            // Candidate allocator
                let cand_result_ref = &candidate_result;
            let cand_ref = $cand_short;
            let cand_short_str = stringify!($cand_short);
            let iters_per_batch = $iters_per_batch;
            let seed = $seed;
            scope.spawn(move || {
                let mut best_ns = u64::MAX;

                for _batch in 0..NUM_BATCHES {
                    let mut s = devutils::TestState::new(iters_per_batch, seed);
                    let start = $crate::clock(libc::CLOCK_THREAD_CPUTIME_ID);
                    for _i in 0..iters_per_batch {
                        $func(cand_ref, &mut s);
                    }
                    let end = $crate::clock(libc::CLOCK_THREAD_CPUTIME_ID);
                    s.clean_up(cand_ref);
                    let batch_ns = end - start;
                    if batch_ns < best_ns {
                        best_ns = batch_ns;
                    }
                }

                let nspi = best_ns / iters_per_batch;
                let frac = ((best_ns * 10) / iters_per_batch) % 10;
                let name = format!("{}_st_{}-1", cand_short_str, stringify!($func));
                println!("name: {:>16}, threads:     1, iters: {:>10}, ns: {:>14}, ns/i: {:>8}.{}", name, iters_per_batch.separate_with_commas(), best_ns.separate_with_commas(), nspi.separate_with_commas(), frac);

                cand_result_ref.store(best_ns, Ordering::Relaxed);
            });
        });

        // Collect results and print comparisons
        let candidate_ns = candidate_result.load(Ordering::Relaxed);
        let comparisons: Vec<(&str, u64)> = results
            .iter()
            .map(|(atomic, name)| (*name, atomic.load(Ordering::Relaxed)))
            .collect();

        $crate::print_comparisons(candidate_ns, &comparisons);
    }};
}

#[macro_export]
macro_rules! compare_st_bench {
    ($func:path, $iters_per_batch:expr, $seed:expr) => {
        $crate::with_all_allocators!($crate::compare_st_bench_impl!($func, $iters_per_batch, $seed))
    };
}

// ============================================================================
// Multi-threaded benchmarks
// ============================================================================

#[macro_export]
macro_rules! mt_bench {
    ($func:path, $threads:expr, $iters_per_batch:expr, $seed:expr) => {{
        let sm = devutils::get_devsmalloc!();
        sm.idempotent_init();

        let func_name = stringify!($func);
        let f = |al: &smalloc::Smalloc, s: &mut TestState| { $func(al, s) };
        let name = format!("sm_mt_{func_name}-{}", $threads);
        $crate::multithread_bench(f, $threads, $iters_per_batch, &name, &sm, $seed);
    }};
}

#[macro_export]
macro_rules! compare_mt_bench_impl {
    (
        $func:path, $threads:expr, $iters_per_batch:expr, $seed:expr ;
        @allocators $( $short:ident, $display:expr, $instance:expr, { $($setup:tt)* } );+ ;
        @candidate $cand_short:ident, $cand_display:expr, $cand_instance:expr, { $($cand_setup:tt)* };
    ) => {{
        let func_name = stringify!($func);

        // Create all allocator instances and run setup
        $(
            let $short = $instance;
            $($setup)*
        )+
            let $cand_short = $cand_instance;
        $($cand_setup)*

        // Run benchmarks sequentially (mt_bench already uses multiple threads internally)
            let mut results: Vec<(&str, u64)> = Vec::new();
        $(
            let f = |al: &_, s: &mut TestState| { $func(al, s) };
            let name = format!("{}_mt_{}-{}", stringify!($short), func_name, $threads);
            let ns = $crate::multithread_bench(f, $threads, $iters_per_batch, &name, &$short, $seed);
            results.push(($display, ns));
        )+

        // Candidate
            let f = |al: &_, s: &mut TestState| { $func(al, s) };
        let name = format!("{}_mt_{}-{}", stringify!($cand_short), func_name, $threads);
        let candidate_ns = $crate::multithread_bench(f, $threads, $iters_per_batch, &name, $cand_short, $seed);

        $crate::print_comparisons(candidate_ns, &results);
    }};
}

#[macro_export]
macro_rules! compare_fh_bench_impl {
    (
        $threads:expr, $iters:expr, $l:expr ;
        @allocators $( $short:ident, $display:expr, $instance:expr, { $($setup:tt)* } );+ ;
        @candidate $cand_short:ident, $cand_display:expr, $cand_instance:expr, { $($cand_setup:tt)* };
    ) => {{
        // Create all allocator instances and run setup
        $(
            let $short = $instance;
            $($setup)*
        )+
            let $cand_short = $cand_instance;
        $($cand_setup)*

        // Run benchmarks sequentially
            let mut results: Vec<(&str, u64)> = Vec::new();
        $(
            let name = format!("{}_fh-{}", stringify!($short), $threads);
            let ns = $crate::multithread_free_hotspot_inner($threads, $iters, &name, &$short, $l);
            results.push(($display, ns));
        )+

        // Candidate
            let name = format!("{}_fh-{}", stringify!($cand_short), $threads);
        let candidate_ns = $crate::multithread_free_hotspot_inner($threads, $iters, &name, $cand_short, $l);
        $crate::print_comparisons(candidate_ns, &results);
    }};
}

#[macro_export]
macro_rules! compare_fh_bench {
    ($threads:expr, $iters:expr, $l:expr) => {
        $crate::with_all_allocators!($crate::compare_fh_bench_impl!($threads, $iters, $l))
    };
}

#[macro_export]
macro_rules! compare_mt_bench {
    ($func:path, $threads:expr, $iters_per_batch:expr, $seed:expr) => {
        $crate::with_all_allocators!($crate::compare_mt_bench_impl!($func, $threads, $iters_per_batch, $seed))
    };
}

// ============================================================================
// Hotspot benchmarks
// ============================================================================

#[macro_export]
macro_rules! compare_hs_bench_impl {
    (
        $func:expr, $threads:expr, $iters_per_batch:expr ;
        @allocators $( $short:ident, $display:expr, $instance:expr, { $($setup:tt)* } );+ ;
        @candidate $cand_short:ident, $cand_display:expr, $cand_instance:expr, { $($cand_setup:tt)* };
    ) => {{
        let func_name = stringify!($func);
        let l = Layout::from_size_align(32, 1).unwrap();

        // Create all allocator instances and run setup
        $(
            let $short = $instance;
            $($setup)*
        )+
            let $cand_short = $cand_instance;
        $($cand_setup)*

        // Run benchmarks and collect results
            let mut results: Vec<(&str, u64)> = Vec::new();
        $(
            let f = |al: &_, s: &mut TestState| { $func(al, s) };
            let name = format!("{}_hs_{}-{}", stringify!($short), func_name, $threads);
            let ns = $crate::multithread_hotspot_inner(f, $threads, $iters_per_batch, &name, &$short, l);
            results.push(($display, ns));
        )+

        // Candidate
            let f = |al: &_, s: &mut TestState| { $func(al, s) };
        let name = format!("{}_hs_{}-{}", stringify!($cand_short), func_name, $threads);
        let candidate_ns = $crate::multithread_hotspot_inner(f, $threads, $iters_per_batch, &name, $cand_short, l);

        $crate::print_comparisons(candidate_ns, &results);
    }};
}

#[macro_export]
macro_rules! compare_hs_bench {
    ($func:expr, $threads:expr, $iters_per_batch:expr) => {
        $crate::with_all_allocators!($crate::compare_hs_bench_impl!($func, $threads, $iters_per_batch))
    };
}
