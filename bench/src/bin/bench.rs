use bench::{compare_st_bench, compare_mt_bench, compare_fh_bench, compare_hs_bench};
use std::alloc::Layout;
use smalloc::i::NUM_SLABS_BITS;

use devutils::*;

#[cfg(target_os = "macos")]
fn is_macos() -> bool { true }
#[cfg(not(target_os = "macos"))]
fn is_macos() -> bool { false }

#[cfg(target_os = "windows")]
fn is_windows() -> bool { true }
#[cfg(not(target_os = "windows"))]
fn is_windows() -> bool { false }

// The three "main" operations (alloc/dealloc/realloc+write families) run in every
// mode; the three "extra" operations (no-write families) run only with --thorough.
// `$im` = iters for ops that re-use space (have a dealloc, so won't run out of room),
// `$if` = iters for ops that don't re-use space (alloc-only / alloc+write).

macro_rules! main_ops_st {
    ($im:expr, $if:expr, $nb:expr, $seed:expr, $so:expr) => {
        compare_st_bench!(adrww, $im, $nb, $seed, $so);
        compare_st_bench!(adww,  $im, $nb, $seed, $so);
        compare_st_bench!(aww,   $if, $nb, $seed, $so);
    };
}
macro_rules! extra_ops_st {
    ($im:expr, $if:expr, $nb:expr, $seed:expr, $so:expr) => {
        compare_st_bench!(adr, $im, $nb, $seed, $so);
        compare_st_bench!(ad,  $im, $nb, $seed, $so);
        compare_st_bench!(a,   $if, $nb, $seed, $so);
    };
}
macro_rules! main_ops_mt {
    ($nt:expr, $im:expr, $if:expr, $nb:expr, $seed:expr, $so:expr) => {
        compare_mt_bench!(adrww, $nt, $im, $nb, $seed, $so);
        compare_mt_bench!(adww,  $nt, $im, $nb, $seed, $so);
        compare_mt_bench!(aww,   $nt, $if, $nb, $seed, $so);
    };
}
macro_rules! extra_ops_mt {
    ($nt:expr, $im:expr, $if:expr, $nb:expr, $seed:expr, $so:expr) => {
        compare_mt_bench!(adr, $nt, $im, $nb, $seed, $so);
        compare_mt_bench!(ad,  $nt, $im, $nb, $seed, $so);
        compare_mt_bench!(a,   $nt, $if, $nb, $seed, $so);
    };
}

pub fn main() {
    let seed: u64 = std::env::args()
        .find_map(|arg| arg.strip_prefix("--seed=").map(|s| s.to_string()))
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            // No --seed= found; catch the easy-to-make "--seed VALUE" mistake.
            let args: Vec<String> = std::env::args().collect();
            if let Some(w) = args.windows(2).find(|w| w[0] == "--seed" && w[1].parse::<u64>().is_ok()) {
                eprintln!("Error: use --seed={} instead of --seed {}", w[1], w[1]);
                std::process::exit(1);
            }
            0
        });

    println!("Using seed: {}", seed);

    let smalloconly = std::env::args().any(|arg| arg == "--smalloc-only");
    let thorough = std::env::args().any(|arg| arg == "--thorough");

    let num_batches: u16 = 30;

    // Single-threaded benchmarks are so fast we can afford many more iters/batches
    // without making the user wait.
    let num_st_batches: u16 = 200;
    let iters_st_many: u64 = 500_000;

    let iters_many: u64 = 100_000;

    // Ops that never free won't re-use space, so they'd exhaust the heap at iters_many.
    const ITERS_FEW: u64 = 10_000;

    const DEFAULT_THREADS: u32 = 32;
    const THREADS_THAT_CAN_FIT_INTO_SLABS: u32 = 1 << NUM_SLABS_BITS;
    const THREADS_WAY_TOO_MANY: u32 = 1024;

    // ---- Single-threaded ----
    main_ops_st!(iters_st_many, ITERS_FEW, num_st_batches, seed, smalloconly);
    if thorough {
        extra_ops_st!(iters_st_many, ITERS_FEW, num_st_batches, seed, smalloconly);
    }

    // ---- Multi-threaded, light concurrency, ALWAYS run ----
    // DEFAULT_THREADS (8) ≈ the median core count of machines in the world,
    // so it's the one multithreaded count we measure even in non-thorough runs.
    main_ops_mt!(DEFAULT_THREADS, iters_many, ITERS_FEW, num_batches, seed, smalloconly);
    if thorough {
        extra_ops_mt!(DEFAULT_THREADS, iters_many, ITERS_FEW, num_batches, seed, smalloconly);
    }

    // ---- Multi-threaded, one thread per slab lane (the standard concurrency target) ----
    // FIT is derived from slab geometry and may change in a smalloc variant; skip it
    // if it collapses to DEFAULT_THREADS so we don't measure the same count twice.
    if thorough && THREADS_THAT_CAN_FIT_INTO_SLABS != DEFAULT_THREADS {
        main_ops_mt!(THREADS_THAT_CAN_FIT_INTO_SLABS, iters_many, ITERS_FEW, num_batches, seed, smalloconly);
        extra_ops_mt!(THREADS_THAT_CAN_FIT_INTO_SLABS, iters_many, ITERS_FEW, num_batches, seed, smalloconly);
    }

    if thorough {
        // ---- Multi-threaded, oversubscribed worst case ----
        //
        // 1024 threads is a worst-case-scenario: more threads than cores *and* every
        // thread is hammering the allocator as fast as it can. This is not something to
        // optimize for at the cost of other cases, because user code shouldn't do that.
        // We benchmark it to look for pathological behavior in smalloc, and to optimize it
        // when we can do so without penalizing other cases. In particular smalloc v7.2 made
        // alloc fail over to another slab on flh-collision.
        //
        // Note: `aww` here makes the *default* allocator on Windows return a NULL pointer.
        main_ops_mt!(THREADS_WAY_TOO_MANY, iters_many, ITERS_FEW, num_batches, seed, smalloconly);
        extra_ops_mt!(THREADS_WAY_TOO_MANY, iters_many, ITERS_FEW, num_batches, seed, smalloconly);

        println!();

        // ---- Hotspot (hs) ----
        //
        // hs simulates a somewhat plausible scenario which is a worst-case-scenario for
        // smalloc before v7.2: a bunch of threads all trying to alloc/dealloc from the same
        // slab. It is structured specifically to exercise smalloc's hotspot: every 64'th
        // thread is active and the intervening 63 are quiescent, because every 64'th thread
        // gets mapped to the same slab by smalloc. So it doesn't make much sense to compare
        // smalloc's performance on this benchmark to other allocators, which presumably have
        // different hotspots/worst-case-scenarios.
        //
        // These consistently crash the OS (!) on macOS Tahoe 26.2–26.5 on Apple M4 Max. :-(
        let cool = THREADS_THAT_CAN_FIT_INTO_SLABS - 1;
        if !is_macos() {
            compare_hs_bench!(one_ad, 100, cool, iters_many, num_batches, smalloconly);
            compare_hs_bench!(a, 100, cool, ITERS_FEW, num_batches, smalloconly);
            // Windows ran out of resources (i.e. threads) running the 200/400 cases:
            if !is_windows() {
                compare_hs_bench!(one_ad, 200, cool, iters_many, num_batches, smalloconly);
                compare_hs_bench!(one_ad, 400, cool, iters_many, num_batches, smalloconly);
                compare_hs_bench!(a, 200, cool, ITERS_FEW, num_batches, smalloconly);
                compare_hs_bench!(a, 400, cool, ITERS_FEW, num_batches, smalloconly);
            }
        }

        // ---- Free hotspot (fh) ----
        //
        // fh simulates a somewhat plausible worst-case-scenario: many threads all trying to
        // free slots in the same slab as each other.
        const TOT_ITERS: u64 = 100_000;
        let l = Layout::from_size_align(8, 1).unwrap();
        let mut counts = [1u32, DEFAULT_THREADS, THREADS_THAT_CAN_FIT_INTO_SLABS, 100, THREADS_WAY_TOO_MANY];
        counts.sort_unstable();
        let mut prev = 0;
        for nt in counts {
            if nt == prev {
                continue;
            }
            prev = nt;
            compare_fh_bench!(nt, TOT_ITERS / nt as u64, num_batches, l, smalloconly);
        }
    }
}
