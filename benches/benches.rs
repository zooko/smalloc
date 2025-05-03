#![feature(test)]
extern crate test;

use rand::Rng;
use rand::SeedableRng;
use rand::rngs::StdRng;

use test::Bencher;

use std::alloc::Layout;

use smalloc::{
    NUM_LARGE_SLABS,
    NUM_SMALL_SLABS,
    Smalloc,
    sum_large_slab_sizes,
    sum_small_slab_sizes
};

use std::hint::black_box;

const MAX: usize = 2usize.pow(39);
const NUM_ARGS: usize = 128;

//XXX use std::alloc::Layout;

//XXX add cache-flushing to try to suss out cached vs uncached performance
//XXX try Leo's suggestion of criterion bench_function

use lazy_static::lazy_static;

lazy_static! {
    static ref SM: Smalloc = Smalloc::new();
}

use std::alloc::GlobalAlloc;

// #[bench]
// fn bench_alloc_and_free_32_threads(b: &mut Bencher) {
//     let l = Layout::from_size_align(64, 1).unwrap();

//     let mut r = StdRng::seed_from_u64(0);
//     let mut ps = Vec::new();

//     b.iter(|| {
//         if r.random::<bool>() {
//             // Free
//             if !ps.is_empty() {
//                 let i = r.random_range(0..ps.len());
//                 let (p, l2) = ps.remove(i);
//                 unsafe { SM.dealloc(p, l2) };
//             }
//         } else {
//             // Malloc
//             let p = unsafe { SM.alloc(l) };
//             ps.push((p, l));
//         }
//     });
// }

#[bench]
fn bench_alloc_and_free(b: &mut Bencher) {
    let layout = Layout::from_size_align(1, 1).unwrap();

    b.iter(|| {
        let p = black_box(unsafe { SM.alloc(layout) });
        unsafe { SM.dealloc(p, layout) };
    });
}

#[bench]
fn bench_sum_small_slab_sizes(b: &mut Bencher) {
    let mut r = StdRng::seed_from_u64(0);
    let reqslabnums: Vec<usize> = (0..NUM_ARGS)
        .map(|_| r.random_range(0..=NUM_SMALL_SLABS))
        .collect();
    let mut i = 0;

    b.iter(|| {
        black_box(sum_small_slab_sizes(reqslabnums[i % NUM_ARGS]));

        i += 1;
    });
}

#[bench]
fn bench_sum_large_slab_sizes(b: &mut Bencher) {
    let mut r = StdRng::seed_from_u64(0);
    let reqslabnums: Vec<usize> = (0..NUM_ARGS)
        .map(|_| r.random_range(0..=NUM_LARGE_SLABS))
        .collect();
    let mut i = 0;

    b.iter(|| {
        black_box(sum_large_slab_sizes(reqslabnums[i % NUM_ARGS]));

        i += 1;
    });
}

#[inline(always)]
fn pot_builtin(x: usize) -> bool {
    x.is_power_of_two()
}

#[inline(always)]
fn pot_bittwiddle(x: usize) -> bool {
    x > 0 && (x & (x - 1)) != 0
}

#[bench]
fn bench_pot_builtin_randoms(b: &mut Bencher) {
    let mut r = StdRng::seed_from_u64(0);
    let reqalignments: Vec<usize> = (0..NUM_ARGS).map(|_| r.random_range(0..MAX)).collect();
    let mut i = 0;

    b.iter(|| {
        let align = reqalignments[i % NUM_ARGS];
        black_box(pot_builtin(align));

        i += 1;
    });
}

#[bench]
fn bench_pot_builtin_powtwos(b: &mut Bencher) {
    let mut r = StdRng::seed_from_u64(0);
    let reqalignments: Vec<usize> = (0..NUM_ARGS)
        .map(|_| 2usize.pow(r.random_range(0..35)))
        .collect();
    let mut i = 0;

    b.iter(|| {
        let align = reqalignments[i % NUM_ARGS];
        black_box(pot_builtin(align));

        i += 1;
    });
}

#[bench]
fn bench_pot_bittwiddle_randoms(b: &mut Bencher) {
    let mut r = StdRng::seed_from_u64(0);
    let reqalignments: Vec<usize> = (0..NUM_ARGS).map(|_| r.random_range(0..MAX)).collect();
    let mut i = 0;

    b.iter(|| {
        let align = reqalignments[i % NUM_ARGS];
        black_box(pot_bittwiddle(align));

        i += 1;
    });
}

#[bench]
fn bench_pot_bittwiddle_powtwos(b: &mut Bencher) {
    let mut r = StdRng::seed_from_u64(0);
    let reqalignments: Vec<usize> = (0..NUM_ARGS)
        .map(|_| 2usize.pow(r.random_range(0..35)))
        .collect();
    let mut i = 0;

    b.iter(|| {
        let align = reqalignments[i % NUM_ARGS];
        black_box(pot_bittwiddle(align));

        i += 1;
    });
}

//use std::ptr::null_mut;

// #[bench]
// fn bench_slotlocation_of_ptr(b: &mut Bencher) {
//     let mut r = StdRng::seed_from_u64(0);
//     let baseptr_for_testing: *mut u8 = null_mut();
//     let mut reqptrs = [null_mut(); NUM_ARGS];
//     let mut i = 0;
//     while i < NUM_ARGS {
//         // generate a random slot
//         let areanum = r.random_range(0..NUM_AREAS);
//         let slabnum;
//         if areanum == 0 {
//             slabnum = r.random_range(0..NUM_SLABS);
//         } else {
//             slabnum = r.random_range(0..NUM_SLABS_CACHELINEY);
//         }
//         let slotnum = r.random_range(0..NUM_SLOTS);
//         let sl: SlotLocation = SlotLocation {
//             areanum,
//             slabnum,
//             slotnum,
//         };

//         // put the random slot's pointer into the test set
//         reqptrs[i] = unsafe { baseptr_for_testing.add(sl.offset_of_slot()) };

//         i += 1;
//     }

//     b.iter(|| {
//         let ptr = reqptrs[i % NUM_ARGS];
//         black_box(slotlocation_of_ptr(baseptr_for_testing, ptr));

//         i += 1;
//     });
// }
