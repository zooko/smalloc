// Thanks to Claude Opus 4.5 for refactoring this entire file with me.

// ============================================================================
// ALLOCATOR REGISTRY - Add new allocators here!
// ============================================================================
//
// Each allocator entry is: (short_name, display_name, constructor, setup_block)
//
// - short_name: Used for benchmark output prefixes (e.g., "mi" -> "mi_st_funcname-1")  
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
                de, "default",  $crate::GlobalAllocWrap,       {}; // This causes crashes on Macos+M4Max with --thorough
            mi, "mimalloc", mimalloc::MiMalloc,            {};
            je, "jemalloc", tikv_jemallocator::Jemalloc,   {};
            sn, "snmalloc", snmalloc_rs::SnMalloc,         {};
            rp, "rpmalloc", rpmalloc::RpMalloc,            {};
            @candidate
                s, "smalloc",  devutils::get_devsmalloc!(),   {};
        }
    };
}

pub fn clock(clocktype: ClockType) -> Nanoseconds {
    let mut tp: MaybeUninit<libc::timespec> = MaybeUninit::uninit();
    let retval = unsafe { libc::clock_gettime(clocktype, tp.as_mut_ptr()) };
    debug_assert_eq!(retval, 0);
    let instsec = unsafe { (*tp.as_ptr()).tv_sec };
    let instnsec = unsafe { (*tp.as_ptr()).tv_nsec };
    debug_assert!(instsec >= 0);
    debug_assert!(instnsec >= 0);
    Nanoseconds(instsec as u64 * 1_000_000_000 + instnsec as u64)
}

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

pub fn singlethread_bench<T, F>(bf: F, iters_per_batch: u64, num_batches: u16, name: &str, al: &T, seed: u64)
where
    T: GlobalAlloc,
    F: Fn(&T, &mut TestState) + Sync + Send + Copy + 'static
{
    let mut results_ns = Vec::with_capacity(iters_per_batch as usize);

    for _batch in 0..num_batches {
        let mut s = TestState::new(iters_per_batch, seed);
        let start = clock(libc::CLOCK_THREAD_CPUTIME_ID);
        for _i in 0..iters_per_batch {
            bf(al, &mut s);
        }
        let end = clock(libc::CLOCK_THREAD_CPUTIME_ID);
        s.clean_up(al);

        let batch_ns = end - start;
        results_ns.push(batch_ns);
    }

    results_ns.sort_unstable();
    let median_ns = results_ns[results_ns.len() / 2];
    let nspi = median_ns.per_iter(iters_per_batch);
    println!("name: {name:>16}, threads:     1, iters: {iters_per_batch:>10}, ns: {median_ns:>14}, ns/i: {nspi:>10}");
}

/// Returns median nanoseconds per batch (not per iter -- all the time taken for all the iters in that batch)
pub fn multithread_bench<T, F>(bf: F, threads: u32, iters_per_batch: u64, num_batches: u16, name: &str, al: &T, seed: u64) -> Nanoseconds
where
    T: GlobalAlloc + Send + Sync,
    F: Fn(&T, &mut TestState) + Sync + Send + Copy + 'static
{
    let mut results_ns = Vec::with_capacity(iters_per_batch as usize);

    for _batch in 0..num_batches {
        let mut tses: Vec<TestState> = Vec::with_capacity(threads as usize);
        for _i in 0..threads {
            tses.push(TestState::new(iters_per_batch, seed));
        }

        let start = clock(libc::CLOCK_MONOTONIC_RAW);
        help_test_multithreaded_with_allocator(bf, threads, iters_per_batch, al, &mut tses);
        let end = clock(libc::CLOCK_MONOTONIC_RAW);
        let batch_ns = end - start;
        results_ns.push(batch_ns);

        // Dealloc all allocations so that we don't run out of space.
        for mut ts in tses {
            ts.clean_up(al);
        }
    }

    results_ns.sort_unstable();
    let median_ns = results_ns[results_ns.len() / 2];
    let nspi = median_ns.per_iter(iters_per_batch);
    println!("name: {name:>16}, threads: {threads:>5}, iters: {iters_per_batch:>10}, ns: {median_ns:>14}, ns/i: {nspi:>10}");

    median_ns
}

#[macro_export]
    macro_rules! multithread_hotspot {
    ($f:expr, $threads:expr, $iters:expr, $num_batches:expr, $al:expr, $l:expr) => {{
        let name = format!("hs-{}-{}", stringify!($f), $threads);
        $crate::multithread_hotspot_inner($f, $threads, $iters, $num_batches, &name, $al, $l)
    }};
}

#[macro_export]
    macro_rules! multithread_free_hotspot {
    ($threads:expr, $iters:expr, $num_batches:expr, $al:expr, $l:expr) => {{
        let name = format!("fh-{}", $threads);
        $crate::multithread_free_hotspot_inner($threads, $iters, $num_batches, &name, $al, $l)
    }};
}

/// This is to stress test the case that many threads are simultaneously free'ing slots in the same
/// slab as each other.
///
/// Thanks to Claude Opus 4.5 for writing 90% of this function for me.
///
/// Returns median nanoseconds per batch (not per iter -- all the time taken for all the iters in that batch)
pub fn multithread_free_hotspot_inner<A>(numthreads: u32, iters_per_batch_per_thread: u64, num_batches: u16, name: &str, al: &A, l: Layout) -> Nanoseconds
where
    A: GlobalAlloc + Send + Sync
{
    let iters_pbpt = iters_per_batch_per_thread as usize;
    let numthreads = numthreads as usize;
    let tot_iters_pb = iters_pbpt * numthreads;

    let mut results_ns = Vec::with_capacity(num_batches as usize);

    for _batch in 0..num_batches {
        // Allocate all pointers upfront
        let mut pointers: Vec<SendPtr> = Vec::with_capacity(tot_iters_pb);
        for _ in 0..tot_iters_pb {
            let p = unsafe { al.alloc(l) };
            assert!(!p.is_null());
            pointers.push(SendPtr(p));
        }

        let iters_pt = tot_iters_pb / numthreads;

        // Split pointers among threads
        let chunks: Vec<Vec<SendPtr>> = pointers
            .chunks(iters_pt)
            .map(|c| c.to_vec())
            .collect();

        let start_barrier = Barrier::new(numthreads + 1);
        let end_barrier = Barrier::new(numthreads + 1);

        let elap_ns = thread::scope(|s| {
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
            end - start
        });

        results_ns.push(elap_ns);
    }

    results_ns.sort_unstable();
    let median_ns = results_ns[results_ns.len() / 2];
    let nspi = median_ns.per_iter(tot_iters_pb as u64);
    println!("name: {name:>16}, threads: {numthreads:>5}, iters: {tot_iters_pb:>10}, ns: {median_ns:>14}, ns/i: {nspi:>10}");

    median_ns
}

/// This is to stress test the case that one slab's flh is under heavy multi-threading contention
/// but the other slabs's flh's are not.
///
/// Spawn 64 * `hotspot_threads` threads, each of which allocates one slot then blocks. All of the
/// ones that are the %64'th to first-allocate then get unblocked, and proceed to deallocate,
/// allocate, deallocate, allocate etc. `iters` times.
///
/// Returns median nanoseconds per batch. Not nanoseconds per (iters * hotspot_threads), nor yet
/// nanoseconds per (iters * total_threads). Nor even nanoseconds per
/// iter-per-batch-per-hot-thread. Median nanoseconds per batch for all hot threads and all iters in
/// that batch. (Also the nanoseconds carries with it an extra .1 nanosecond precision.)
///
/// iters_pbpht: iters per batch per hot thread
///
/// Thanks to Claude Opus 4.5 for writing 90% of this function for me.
pub fn multithread_hotspot_inner<T, F>(f: F, hotspot_threads: u32, iters_pbpht: u64, num_batches: u16, name: &str, al: &T, l: Layout) -> Nanoseconds
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

    let mut results_ns = Vec::with_capacity(num_batches as usize);

    for _batch in 0..num_batches {
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
                    let _ptr = unsafe { al.alloc(l) }; // this gets leaked oh well
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
                        let _ptr = unsafe { al.alloc(l) }; // this gets leaked oh well
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

        results_ns.push(batch_ns);
    }

    results_ns.sort_unstable();
    let median_ns = results_ns[results_ns.len() / 2];
    let nspi = median_ns.per_iter(iters_pbpht);
    println!("name: {name:>16}, threads: {hotspot_threads_usize:>5}, iters/batch/hot-thread: {iters_pbpht:>10}, ns: {median_ns:>14}, ns/i: {nspi:>10}");

    median_ns
}

/// Print comparison percentages
pub fn print_comparisons(candidate: Nanoseconds, baselines: &[(&str, Nanoseconds)]) {
    for (name, baseline) in baselines {
        let diff_perc = candidate.diff_percent(*baseline);
        println!("smalloc diff from {name:>8}: {diff_perc:+4.0}%");
    }
    println!();
}

// ============================================================================
// Single-threaded benchmarks
// ============================================================================

#[macro_export]
macro_rules! st_bench {
    ($func:path, $iters_per_batch:expr, $num_batches:expr, $seed:expr) => {{
        let sm = devutils::get_devsmalloc!();
        sm.idempotent_init();

        let func_name = stringify!($func);
        let f = |al: &smalloc::Smalloc, s: &mut TestState| { $func(al, s) };
        let name = format!("s_st_{func_name}-1");
        $crate::singlethread_bench(f, $iters_per_batch, $num_batches, &name, &sm, $seed);
    }};
}

#[macro_export]
macro_rules! compare_st_bench_impl {
    (
        $func:path, $iters_per_batch:expr, $num_batches:expr, $seed:expr ;
        @allocators $( $short:ident, $display:expr, $instance:expr, { $($setup:tt)* } );+ ;
        @candidate $cand_short:ident, $cand_display:expr, $cand_instance:expr, { $($cand_setup:tt)* };
    ) => {{
        use thousands::Separable;
        use std::sync::atomic::{AtomicU64, Ordering};
        let func_name = stringify!($func);

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
                    let mut results_ns: Vec<$crate::Nanoseconds> = Vec::with_capacity(iters_per_batch as usize);

                    for _batch in 0..$num_batches {
                        let mut s = devutils::TestState::new(iters_per_batch, seed);
                        let start = $crate::clock(libc::CLOCK_THREAD_CPUTIME_ID);
                        for _i in 0..iters_per_batch {
                            $func(alloc_ref, &mut s);
                        }
                        let end = $crate::clock(libc::CLOCK_THREAD_CPUTIME_ID);
                        s.clean_up(alloc_ref);
                        let batch_ns = end - start;
                        results_ns.push(batch_ns);
                    }

                    results_ns.sort_unstable();
                    let median_ns = results_ns[results_ns.len() / 2];
                    let nspi = median_ns.per_iter(iters_per_batch);
                    let name = format!("{}_st_{}-1", short_str, stringify!($func));
                    println!("name: {name:>16}, threads:     1, iters: {iters_per_batch:>10}, ns: {median_ns:>14}, ns/i: {nspi:>10}");

                    result_ref.store(median_ns.into(), Ordering::Relaxed);
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
                let mut results_ns = Vec::with_capacity(iters_per_batch as usize);

                for _batch in 0..$num_batches {
                    let mut s = devutils::TestState::new(iters_per_batch, seed);
                    let start = $crate::clock(libc::CLOCK_THREAD_CPUTIME_ID);
                    for _i in 0..iters_per_batch {
                        $func(cand_ref, &mut s);
                    }
                    let end = $crate::clock(libc::CLOCK_THREAD_CPUTIME_ID);
                    s.clean_up(cand_ref);
                    let batch_ns = end - start;
                    results_ns.push(batch_ns);
                }

                results_ns.sort_unstable();
                let median_ns = results_ns[results_ns.len() / 2];
                let nspi = median_ns.per_iter(iters_per_batch);
                let name = format!("{}_st_{}-1", cand_short_str, stringify!($func));
                println!("name: {name:>16}, threads:     1, iters: {iters_per_batch:>10}, ns: {median_ns:>14}, ns/i: {nspi:>10}");

                cand_result_ref.store(median_ns.into(), Ordering::Relaxed);
            });
        });

        // Collect results and print comparisons
        let candidate_ns = candidate_result.load(Ordering::Relaxed).into();
        let comparisons: Vec<(&str, $crate::Nanoseconds)> = results
            .iter()
            .map(|(atomic, name)| (*name, atomic.load(Ordering::Relaxed).into()))
            .collect();

        $crate::print_comparisons(candidate_ns, &comparisons);
    }};
}

#[macro_export]
macro_rules! compare_st_bench {
    ($func:path, $iters_per_batch:expr, $num_batches:expr, $seed:expr) => {
        $crate::with_all_allocators!($crate::compare_st_bench_impl!($func, $iters_per_batch, $num_batches, $seed))
    };
}

// ============================================================================
// Multi-threaded benchmarks
// ============================================================================

#[macro_export]
macro_rules! mt_bench {
    ($func:path, $threads:expr, $iters_per_batch:expr, $num_batches:expr, $seed:expr) => {{
        let sm = devutils::get_devsmalloc!();
        sm.idempotent_init();

        let func_name = stringify!($func);
        let f = |al: &smalloc::Smalloc, s: &mut TestState| { $func(al, s) };
        let name = format!("s_mt_{func_name}-{}", $threads);
        $crate::multithread_bench(f, $threads, $iters_per_batch, $num_batches, &name, &sm, $seed);
    }};
}

#[macro_export]
macro_rules! compare_mt_bench_impl {
    (
        $func:path, $threads:expr, $iters_per_batch:expr, $num_batches:expr, $seed:expr ;
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
            let mut results: Vec<(&str, $crate::Nanoseconds)> = Vec::new();
        $(
            let f = |al: &_, s: &mut TestState| { $func(al, s) };
            let name = format!("{}_mt_{}-{}", stringify!($short), func_name, $threads);
            let ns = $crate::multithread_bench(f, $threads, $iters_per_batch, $num_batches, &name, &$short, $seed);
            results.push(($display, ns));
        )+

        // Candidate
            let f = |al: &_, s: &mut TestState| { $func(al, s) };
        let name = format!("{}_mt_{}-{}", stringify!($cand_short), func_name, $threads);
        let candidate_ns = $crate::multithread_bench(f, $threads, $iters_per_batch, $num_batches, &name, $cand_short, $seed);

        $crate::print_comparisons(candidate_ns, &results);
    }};
}

#[macro_export]
macro_rules! compare_fh_bench_impl {
    (
        $threads:expr, $iters:expr, $num_batches:expr, $l:expr ;
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
            let mut results: Vec<(&str, $crate::Nanoseconds)> = Vec::new();
        $(
            let name = format!("{}_fh-{}", stringify!($short), $threads);
            let ns = $crate::multithread_free_hotspot_inner($threads, $iters, $num_batches, &name, &$short, $l);
            results.push(($display, ns));
        )+

        // Candidate
            let name = format!("{}_fh-{}", stringify!($cand_short), $threads);
        let candidate_ps = $crate::multithread_free_hotspot_inner($threads, $iters, $num_batches, &name, $cand_short, $l);
        $crate::print_comparisons(candidate_ps, &results);
    }};
}

#[macro_export]
macro_rules! compare_fh_bench {
    ($threads:expr, $iters:expr, $num_batches:expr, $l:expr) => {
        $crate::with_all_allocators!($crate::compare_fh_bench_impl!($threads, $iters, $num_batches, $l))
    };
}

#[macro_export]
macro_rules! compare_mt_bench {
    ($func:path, $threads:expr, $iters_per_batch:expr, $num_batches:expr, $seed:expr) => {
        $crate::with_all_allocators!($crate::compare_mt_bench_impl!($func, $threads, $iters_per_batch, $num_batches, $seed))
    };
}

// ============================================================================
// Hotspot benchmarks
// ============================================================================

#[macro_export]
macro_rules! compare_hs_bench_impl {
    (
        $func:expr, $threads:expr, $iters_per_batch:expr, $num_batches:expr ;
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
            let mut results: Vec<(&str, $crate::Nanoseconds)> = Vec::new();
        $(
            let f = |al: &_, s: &mut TestState| { $func(al, s) };
            let name = format!("{}_hs_{}-{}", stringify!($short), func_name, $threads);
            let ns = $crate::multithread_hotspot_inner(f, $threads, $iters_per_batch, $num_batches, &name, &$short, l);
            results.push(($display, ns));
        )+

        // Candidate
            let f = |al: &_, s: &mut TestState| { $func(al, s) };
        let name = format!("{}_hs_{}-{}", stringify!($cand_short), func_name, $threads);
        let candidate_ns = $crate::multithread_hotspot_inner(f, $threads, $iters_per_batch, $num_batches, &name, $cand_short, l);

        $crate::print_comparisons(candidate_ns, &results);
    }};
}

#[macro_export]
macro_rules! compare_hs_bench {
    ($func:expr, $threads:expr, $iters_per_batch:expr, $num_batches:expr) => {
        $crate::with_all_allocators!($crate::compare_hs_bench_impl!($func, $threads, $iters_per_batch, $num_batches))
    };
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Nanoseconds(pub u64);

#[derive(Clone, Copy, PartialEq, PartialOrd)]
pub struct NanosecondsPerIter(pub f64);

impl Nanoseconds {
    pub fn diff_percent(self, baseline: Nanoseconds) -> f64 {
        100.0 * (self.0 as f64 - baseline.0 as f64) / (baseline.0 as f64)
    }

    pub fn per_iter(self, iters: u64) -> NanosecondsPerIter {
        NanosecondsPerIter(self.0 as f64 / iters as f64)
    }
}

impl fmt::Display for Nanoseconds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = self.0.separate_with_commas();
        if let Some(width) = f.width() {
            write!(f, "{s:>width$}")
        } else {
            write!(f, "{s}")
        }
    }
}

impl fmt::Display for NanosecondsPerIter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let full = format!("{}.{}", (self.0 as u64).separate_with_commas(), (self.0 * 10.0) as u64 % 10);
        write!(f, "{full:>width$}", width = f.width().unwrap_or(0))
    }
}

impl std::ops::Sub for Nanoseconds {
    type Output = Nanoseconds;
    fn sub(self, rhs: Nanoseconds) -> Nanoseconds {
        Nanoseconds(self.0 - rhs.0)
    }
}

impl From<Nanoseconds> for u64 {
    fn from(ns: Nanoseconds) -> u64 {
        ns.0
    }
}

impl From<u64> for Nanoseconds {
    fn from(val: u64) -> Nanoseconds {
        Nanoseconds(val)
    }
}

/// Wrapper to mark raw pointers as Send for cross-thread deallocation
#[derive(Clone, Copy)]
struct SendPtr(*mut u8);

unsafe impl Send for SendPtr {}

pub struct GlobalAllocWrap;

mod platform;
use platform::ClockType;

use devutils::*;

use thousands::Separable;

use std::fmt;
use std::mem::MaybeUninit;
use std::sync::Barrier;
use std::thread;
use std::alloc::{GlobalAlloc, System, Layout};
