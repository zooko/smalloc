mod platform;
use platform::ClockType;
use std::mem::MaybeUninit;

pub use mimalloc::MiMalloc;
use std::alloc::GlobalAlloc;
pub use smalloc::Smalloc;
pub use tikv_jemallocator::Jemalloc;
pub struct GlobalAllocWrap;

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

#[inline(never)]
pub fn bench_itered<F: FnMut()>(name: &str, iters: usize, mut f: F, clocktype: ClockType) {
    let start = clock(clocktype);
    for _i in 0..iters {
        f();
    }
    let elap = clock(clocktype) - start;
    println!("name: {name}, iters: {iters}, ns: {elap}, ns/i: {}", elap/iters as u64);
}

use thousands::Separable;
#[inline(never)]
pub fn bench_once<F: FnOnce()>(name: &str, f: F, clocktype: ClockType) {
    let start = clock(clocktype);
    f();
    let elap = clock(clocktype) - start;
    println!("name: {name}, ns: {}", elap.separate_with_commas());
}

use devutils::*;

/// Returns elapsed nanoseconds
pub fn singlethread_bench<T, F>(bf: F, iters: u64, name: &str, al: &T, seed: u64) -> u64
where
    T: GlobalAlloc,
    F: Fn(&T, &mut TestState) + Sync + Send + Copy + 'static
{
    let mut s = TestState::new(iters, seed);

    let start = clock(libc::CLOCK_THREAD_CPUTIME_ID);

    for _i in 0..iters {
        bf(al, &mut s);
    }

    let end = clock(libc::CLOCK_THREAD_CPUTIME_ID);
    assert!(end > start);
    let elap_ns = end - start;
    let nspi = elap_ns / iters;
    let hundredpses_per_iter = ((elap_ns * 10) / iters) % 10;
    println!("name: {name:>13}, iters: {:>11}, ns: {:>15}, ns/i: {:>8}.{hundredpses_per_iter}", iters.separate_with_commas(), elap_ns.separate_with_commas(), nspi.separate_with_commas());

    // println!("num popped out of 8 cache: {}, num popped out of 512 cache: {}", s.num_popped_out_of_8_cache, s.num_popped_out_of_512_cache);

    s.clean_up(al);

    elap_ns
}

pub fn multithread_bench<T, F>(bf: F, threads: u32, iters: u64, name: &str, al: &T, seed: u64) -> u64
where
    T: GlobalAlloc + Send + Sync,
    F: Fn(&T, &mut TestState) + Sync + Send + Copy + 'static
{
    let mut tses: Vec<TestState> = Vec::with_capacity(threads as usize);
    for _i in 0..threads {
        tses.push(TestState::new(iters, seed));
    }

    let start = clock(libc::CLOCK_MONOTONIC_RAW);

    help_test_multithreaded_with_allocator(bf, threads, iters, al, &mut tses);
    
    let end = clock(libc::CLOCK_MONOTONIC_RAW);
    assert!(end > start);
    let elap_ns = end - start;
    let nspi = elap_ns / iters;
    let fstr = format!("{:.1}", elap_ns as f64 / iters as f64);
    let nspi_sub_str = &fstr[fstr.find('.').unwrap()..];
    println!("name: {name:>13}, iters: {:>11}, ns: {:>15}, ns/i: {:>8}{}", iters.separate_with_commas(), elap_ns.separate_with_commas(), nspi.separate_with_commas(), nspi_sub_str);

    // Dealloc all allocations so that we don't run out of space.
    for mut ts in tses {
        ts.clean_up(al);
    }
    
    elap_ns
}

// use std::alloc::{GlobalAlloc, Layout};
// use std::sync::Barrier;
// use std::thread;
// use std::time::Instant;

// /// This is to stress test the case that one slab's flh is under heavy multi-threading contention
// /// but the other slabs's flh's are not.
// ///
// /// Spawn 64 * `hotspot_threads` threads, each of which allocates one slot then blocks. All of the
// /// ones that are the %64'th to first-allocate then get unblocked, and proceed to deallocate,
// /// allocate, deallocate, allocate etc. `iters` times.
// ///
// /// Returns picoseconds per iter (not picoseconds per (iter * hotspot_threads), nor yet picoseconds
// /// per (iter * total_threads)). picoseconds per iter
// ///
// /// Thanks to Claude Opus 4.5 for writing 90% of this function for me.
// pub fn multithread_hotspot<T>(hotspot_threads: u32, iters: u64, name: &str, al: &T, l: Layout) -> u64
// where
//     T: GlobalAlloc + Send + Sync,
// {
//     // If you want to stress test smalloc, it is best for this to equal 2^NUM_SLABS_BITS.
//     const NUM_SLABS: usize = 64;

//     let hotspot_threads = hotspot_threads as usize;
//     let iters = iters as usize;
//     let cool_threads_per_round: usize = NUM_SLABS - 1;

//     // One hot thread per 63 cool threads
//     let total_threads: usize = hotspot_threads * (1 + cool_threads_per_round);

//     let hot_done_barriers: Vec<Barrier> = (0..hotspot_threads)
//         .map(|_| Barrier::new(2))
//         .collect();

//     let cool_done_barriers: Vec<Barrier> = (0..hotspot_threads)
//         .map(|_| Barrier::new(cool_threads_per_round + 1))
//         .collect();

//     let setup_complete_barrier = Barrier::new(total_threads + 1);
//     let hot_start_barrier = Barrier::new(hotspot_threads + 1);
//     let hot_finish_barrier = Barrier::new(hotspot_threads + 1);
//     let final_barrier = Barrier::new(total_threads + 1);

//     let elap_ns = thread::scope(|s| {
//         for round in 0..hotspot_threads {
//             // Spawn hot thread
//             s.spawn(|| {
//                 // Setup phase: initial allocation
//                 let _ptr = unsafe { al.alloc(l) };
//                 hot_done_barriers[round].wait();

//                 setup_complete_barrier.wait();
//                 hot_start_barrier.wait();

//                 // === Benchmarked operation ===
//                 for _ in 0..iters {
//                     let ptr = unsafe { al.alloc(l) };
//                     unsafe { al.dealloc(ptr, l) };
//                 }
//                 // =============================

//                 hot_finish_barrier.wait();
//                 final_barrier.wait();
//             });

//             // Wait for hot thread's setup alloc
//             hot_done_barriers[round].wait();

//             // Spawn cool threads
//             for _ in 0..cool_threads_per_round {
//                 s.spawn(|| {
//                     // Setup phase: initial allocation
//                     let _ptr = unsafe { al.alloc(l) };
//                     cool_done_barriers[round].wait();
//                     setup_complete_barrier.wait();
//                     final_barrier.wait();
//                 });
//             }

//             // Wait for all cool threads' setup allocs
//             cool_done_barriers[round].wait();
//         }

//         setup_complete_barrier.wait();

//         // Start timing right before releasing hot threads
//         let start = clock(libc::CLOCK_MONOTONIC_RAW);
//         hot_start_barrier.wait();
//         // Wait for all hot threads to finish
//         hot_finish_barrier.wait();
//         let end = clock(libc::CLOCK_MONOTONIC_RAW);
//         assert!(end > start);

//         end - start
//     });

//     let elap_ps = elap_ns * 1000;
//     let pspi = elap_ps / iters;
//     let hundredpses = (pspi / 100) % 10;
//     let nspi = pspi / 1000;
//     println!("name: {name:>13}, hot threads: {:>5,} iters: {:>11}, ns: {:>15}, ns/i: {:>8}.{hundredpses:1}", hotspot_threads.separate_with_commas(), iters.separate_with_commas(), elap_ns.separate_with_commas(), nspi.separate_with_commas());

//     pspi
// }
    
#[macro_export]
    macro_rules! st_bench {
    ($func:path, $iters:expr, $seed:expr) => {{
        let func_name = stringify!($func);

        let sm = devutils::get_devsmalloc!();
        devutils::dev_instance::setup();

        // Create a closure that specifies the type
        let f = |al: &$crate::Smalloc, s: &mut TestState| {
            $func(al, s)
        };
        let sm_name = format!("sm {func_name}");
        $crate::singlethread_bench(f, $iters, &sm_name, &sm, $seed); 

    }}
}

#[macro_export]
    macro_rules! compare_st_bench {
    ($func:path, $iters:expr, $seed:expr) => {{
        let func_name = stringify!($func);

        let mut baseline_ns = 42;
        let mut candidat_ns = 42;
        let mut mm_ns = 42;
        let mut jm_ns = 42;

        let bi = $crate::GlobalAllocWrap;

        let mm = $crate::MiMalloc;

        let jm = $crate::Jemalloc;

        let sm = devutils::get_devsmalloc!();
        devutils::dev_instance::setup();

        std::thread::scope(|scope| {
            scope.spawn(|| { 
                // Create a closure that specifies the type
                let f = |al: &$crate::GlobalAllocWrap, s: &mut TestState| {
                    $func(al, s)
                };
                let bi_name = format!("bi {func_name}");
                baseline_ns = $crate::singlethread_bench(f, $iters, &bi_name, &bi, $seed); 
            });
            scope.spawn(|| { 
                // Create a closure that specifies the type
                let f = |al: &$crate::MiMalloc, s: &mut TestState| {
                    $func(al, s)
                };
                let mm_name = format!("mm {func_name}");
                mm_ns = $crate::singlethread_bench(f, $iters, &mm_name, &mm, $seed); 
            });
            scope.spawn(|| { 
                // Create a closure that specifies the type
                let f = |al: &$crate::Jemalloc, s: &mut TestState| {
                    $func(al, s)
                };
                let jm_name = format!("jm {func_name}");
                jm_ns = $crate::singlethread_bench(f, $iters, &jm_name, &jm, $seed); 
            });
            scope.spawn(|| { 
                // Create a closure that specifies the type
                let f = |al: &$crate::Smalloc, s: &mut TestState| {
                    $func(al, s)
                };
                let sm_name = format!("sm {func_name}");
                candidat_ns = $crate::singlethread_bench(f, $iters, &sm_name, &sm, $seed); 
            });
        });

	//sm.dump_map_of_slabs();

//        let mmbidiffperc = 100.0 * (mm_ns as f64 - baseline_ns as f64) / (baseline_ns as f64);
//        println!("mimalloc diff from builtin: {mmbidiffperc:+4.0}%");
        let smbidiffperc = 100.0 * (candidat_ns as f64 - baseline_ns as f64) / (baseline_ns as f64);
        println!("smalloc diff from  builtin: {smbidiffperc:+4.0}%");
        let smmmdiffperc = 100.0 * (candidat_ns as f64 - mm_ns as f64) / (mm_ns as f64);
        println!("smalloc diff from mimalloc: {smmmdiffperc:+4.0}%");
        let smjmdiffperc = 100.0 * (candidat_ns as f64 - jm_ns as f64) / (jm_ns as f64);
        println!("smalloc diff from jemalloc: {smjmdiffperc:+4.0}%");
        println!("");
    }}
}

#[macro_export]
    macro_rules! mt_bench {
    ($func:path, $threads:expr, $iters:expr, $seed:expr) => {{
        let func_name = stringify!($func);

        let sm = devutils::get_devsmalloc!();
        devutils::dev_instance::setup();

        // Create a closure that specifies the type
        let fsm = |al: &$crate::Smalloc, s: &mut TestState| {
            $func(al, s)
        };

        let sm_name = format!("sm {} {func_name}", $threads);
        $crate::multithread_bench(fsm, $threads, $iters, sm_name.as_str(), &sm, $seed);

        // sm.dump_map_of_slabs();
    }}
}

#[macro_export]
    macro_rules! compare_mt_bench {
    ($func:path, $threads:expr, $iters:expr, $seed:expr) => {{
        let func_name = stringify!($func);

        let bi = $crate::GlobalAllocWrap;

        // Create a closure that specifies the type
        let fbi = |al: &$crate::GlobalAllocWrap, s: &mut TestState| {
            $func(al, s)
        };

        let bi_name = format!("bi {} {func_name}", $threads);
        let baseline_ns = $crate::multithread_bench(fbi, $threads, $iters, bi_name.as_str(), &bi, $seed);


        let mm = $crate::MiMalloc;

        // create a closure that specifies the type
        let fmm = |al: &$crate::MiMalloc, s: &mut TestState| {
            $func(al, s)
        };

        let mm_name = format!("mm {} {func_name}", $threads);
        let mm_ns = $crate::multithread_bench(fmm, $threads, $iters, mm_name.as_str(), &mm, $seed);


        let jm = $crate::Jemalloc;

        // create a closure that specifies the type
        let fjm = |al: &$crate::Jemalloc, s: &mut TestState| {
            $func(al, s)
        };

        let jm_name = format!("jm {} {func_name}", $threads);
        let jm_ns = $crate::multithread_bench(fjm, $threads, $iters, jm_name.as_str(), &jm, $seed);

        
        let sm = devutils::get_devsmalloc!();
        devutils::dev_instance::setup();

        // create a closure that specifies the type
        let fsm = |al: &$crate::Smalloc, s: &mut TestState| {
            $func(al, s)
        };

        let sm_name = format!("sm {} {func_name}", $threads);
        let candidat_ns = $crate::multithread_bench(fsm, $threads, $iters, sm_name.as_str(), &sm, $seed);



//        let mmbidiffperc = 100.0 * (mm_ns as f64 - baseline_ns as f64) / (baseline_ns as f64);
//        println!("mimalloc diff from builtin: {mmbidiffperc:+4.0}%");
        let smbidiffperc = 100.0 * (candidat_ns as f64 - baseline_ns as f64) / (baseline_ns as f64);
        println!("smalloc diff from  builtin: {smbidiffperc:+4.0}%");
        let smmmdiffperc = 100.0 * (candidat_ns as f64 - mm_ns as f64) / (mm_ns as f64);
        println!("smalloc diff from mimalloc: {smmmdiffperc:+4.0}%");
        let smjmdiffperc = 100.0 * (candidat_ns as f64 - jm_ns as f64) / (jm_ns as f64);
        println!("smalloc diff from jemalloc: {smjmdiffperc:+4.0}%");
        println!("");
        //sm.dump_map_of_slabs();
    }}
}
