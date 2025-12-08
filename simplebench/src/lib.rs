mod platform;
use platform::ClockType;
use std::mem::MaybeUninit;

pub use mimalloc::MiMalloc;
use std::alloc::GlobalAlloc;
pub use smalloc::Smalloc;
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
pub fn alloc_and_free(allocator: &Arc<impl GlobalAlloc>) {
    let l = unsafe { Layout::from_size_align_unchecked(32, 1) };
    let p = unsafe { allocator.alloc(l) };
    unsafe { *p = 0 };
    unsafe { allocator.dealloc(p, l) };
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
    let fstr = format!("{:.1}", elap_ns as f64 / iters as f64);
    let nspi_sub_str = &fstr[fstr.find('.').unwrap()..];
    println!("name: {name:>13}, iters: {:>11}, ns: {:>15}, ns/i: {:>8}{}", iters.separate_with_commas(), elap_ns.separate_with_commas(), nspi.separate_with_commas(), nspi_sub_str);

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

        let bi = $crate::GlobalAllocWrap;

        let mm = $crate::MiMalloc;

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
                let f = |al: &$crate::Smalloc, s: &mut TestState| {
                    $func(al, s)
                };
                let sm_name = format!("sm {func_name}");
                candidat_ns = $crate::singlethread_bench(f, $iters, &sm_name, &sm, $seed); 
            });
        });

	//sm.dump_map_of_slabs();

        let mmbidiffperc = 100.0 * (mm_ns as f64 - baseline_ns as f64) / (baseline_ns as f64);
        println!("mimalloc diff from builtin: {mmbidiffperc:.0}%");
        let smbidiffperc = 100.0 * (candidat_ns as f64 - baseline_ns as f64) / (baseline_ns as f64);
        println!("smalloc diff from builtin: {smbidiffperc:.0}%");
        let smmmdiffperc = 100.0 * (candidat_ns as f64 - mm_ns as f64) / (mm_ns as f64);
        println!("smalloc diff from mimalloc: {smmmdiffperc:.0}%");
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

        
        let sm = devutils::get_devsmalloc!();
        devutils::dev_instance::setup();

        // create a closure that specifies the type
        let fsm = |al: &$crate::Smalloc, s: &mut TestState| {
            $func(al, s)
        };

        let sm_name = format!("sm {} {func_name}", $threads);
        let candidat_ns = $crate::multithread_bench(fsm, $threads, $iters, sm_name.as_str(), &sm, $seed);



        let mmbidiffperc = 100.0 * (mm_ns as f64 - baseline_ns as f64) / (baseline_ns as f64);
        println!("mimalloc diff from builtin: {mmbidiffperc:.0}%");
        let smbidiffperc = 100.0 * (candidat_ns as f64 - baseline_ns as f64) / (baseline_ns as f64);
        println!("smalloc diff from builtin: {smbidiffperc:.0}%");
        let smmmdiffperc = 100.0 * (candidat_ns as f64 - mm_ns as f64) / (mm_ns as f64);
        println!("smalloc diff from mimalloc: {smmmdiffperc:.0}%");
        println!("");
        //sm.dump_map_of_slabs();
    }}
}
