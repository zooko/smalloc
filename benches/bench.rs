use criterion::{black_box, criterion_group, criterion_main, Criterion};

use std::alloc::Layout;

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use core::time::Duration;
use std::ptr::null_mut;
use std::alloc::GlobalAlloc;

const NUM_ARGS: usize = 50_000;

use smalloc::{Smalloc, sum_large_slab_sizes, sum_small_slab_sizes, NUM_LARGE_SLABS, NUM_SMALL_SLABS, NUM_SMALL_SLAB_AREAS, NUM_SLOTS_O, SlotLocation, num_large_slots};

fn bench_sum_small_slab_sizes(c: &mut Criterion) {
    let mut r = StdRng::seed_from_u64(0);
    let reqslabnums: Vec<usize> = (0..NUM_ARGS)
        .map(|_| r.random_range(0..=NUM_SMALL_SLABS))
        .collect();
    let mut i = 0;

    c.bench_function("sum_small_slab_sizes", |b| b.iter(|| {
        black_box(sum_small_slab_sizes(reqslabnums[i % NUM_ARGS]));

        i += 1;
    }));
}

fn bench_sum_large_slab_sizes(c: &mut Criterion) {
    let mut r = StdRng::seed_from_u64(0);
    let reqslabnums: Vec<usize> = (0..NUM_ARGS)
        .map(|_| r.random_range(0..=NUM_LARGE_SLABS))
        .collect();
    let mut i = 0;

    c.bench_function("sum_large_slab_sizes", |b| b.iter(|| {
        black_box(sum_large_slab_sizes(black_box(reqslabnums[i % NUM_ARGS])));

        i += 1;
    }));
}

use lazy_static::lazy_static;

lazy_static! {
    static ref SM: Smalloc = Smalloc::new();
}

fn bench_pop_small_flh(c: &mut Criterion) {
    SM.idempotent_init();

    const NUM_LINKS: usize = 50_000;
    let mut sls = Vec::with_capacity(NUM_LINKS);
    while sls.len() < NUM_LINKS {
        let osl = SM.inner_small_alloc(0, 0);
        if osl.is_none() {
            break;
        }

        sls.push(osl.unwrap());
    }

    for sl in sls {
        SM.push_flh(sl);
    }

    c.bench_function("pop_small_flh", |b| b.iter(|| {
        black_box(SM.pop_small_flh(0, 0));
    }));
}

fn bench_pop_large_flh(c: &mut Criterion) {
    SM.idempotent_init();

    const NUM_LINKS: usize = 50_000;
    let mut sls = Vec::with_capacity(NUM_LINKS);
    while sls.len() < NUM_LINKS {
        let osl = SM.inner_large_alloc(0);
        if osl.is_none() {
            break;
        }

        sls.push(osl.unwrap());
    }

    for sl in sls {
        SM.push_flh(sl);
    }

    c.bench_function("pop_large_flh", |b| b.iter(|| {
        black_box(SM.pop_large_flh(0));
    }));
}

fn randdist_reqsiz(r: &mut StdRng) -> usize {
    // The following distribution was roughly modelled on smalloclog profiling of Zebra.
    let randnum = r.random::<u8>();

    if randnum < 50 {
        r.random_range(1..16)
    } else if randnum < 150 {
        32
    } else if randnum < 200 {
        64
    } else {
        r.random_range(65..10_000)
    }
}

fn bench_inner_alloc(c: &mut Criterion) {
    SM.idempotent_init();

    let mut r = StdRng::seed_from_u64(0);
    let mut reqs = Vec::with_capacity(NUM_ARGS);

    while reqs.len() < NUM_ARGS {
        reqs.push(Layout::from_size_align(randdist_reqsiz(&mut r), 1).unwrap());
    }

    let mut i = 0;
    c.bench_function("inner_alloc", |b| b.iter(|| {
        black_box(SM.inner_alloc(black_box(reqs[i % reqs.len()])));
        i += 1;
    }));
}

fn bench_inner_large_alloc(c: &mut Criterion) {
    SM.idempotent_init();

    let mut r = StdRng::seed_from_u64(0);
    let mut reqs = Vec::with_capacity(NUM_ARGS);

    while reqs.len() < NUM_ARGS {
        reqs.push(r.random_range(0..NUM_LARGE_SLABS));
    }

    let mut i = 0;
    c.bench_function("inner_large_alloc", |b| b.iter(|| {
        black_box(SM.inner_large_alloc(black_box(reqs[i % reqs.len()])));
        i += 1
    }));
}

fn bench_inner_small_alloc(c: &mut Criterion) {
    SM.idempotent_init();

    let mut r = StdRng::seed_from_u64(0);
    let mut reqs = Vec::with_capacity(NUM_ARGS);

    while reqs.len() < NUM_ARGS {
        reqs.push(r.random_range(0..NUM_SMALL_SLABS));
    }

    let mut i = 0;
    c.bench_function("inner_small_alloc", |b| b.iter(|| {
        black_box(SM.inner_small_alloc(0, black_box(reqs[i % reqs.len()])));
        i += 1
    }));
}

fn bench_new_from_ptr(c: &mut Criterion) {
    let mut r = StdRng::seed_from_u64(0);
    let baseptr_for_testing: *mut u8 = null_mut();
    let mut reqptrs = [null_mut(); NUM_ARGS];
    let mut i = 0;
    while i < NUM_ARGS {
        // generate a random slot
        let sl = if r.random::<bool>() {
            // SmallSlot
            let areanum = r.random_range(0..NUM_SMALL_SLAB_AREAS);
            let smallslabnum = r.random_range(0..NUM_SMALL_SLABS);
            let slotnum = r.random_range(0..NUM_SLOTS_O);
            
            SlotLocation::SmallSlot { areanum, smallslabnum, slotnum }
        } else {
            // LargeSlot
            let largeslabnum = r.random_range(0..NUM_LARGE_SLABS);
            let slotnum = r.random_range(0..num_large_slots(largeslabnum));

            SlotLocation::LargeSlot { largeslabnum, slotnum }
        };
        
        // put the random slot's pointer into the test set
        reqptrs[i] = unsafe { baseptr_for_testing.add(sl.offset()) };

        i += 1;
    }

    c.bench_function("new_from_ptr", |b| b.iter(|| {
        let ptr = reqptrs[i % NUM_ARGS];
        black_box(SlotLocation::new_from_ptr(black_box(baseptr_for_testing), black_box(ptr)));
        i += 1;
    }));
}

fn bench_alloc(c: &mut Criterion) {
    let mut r = StdRng::seed_from_u64(0);
    let mut reqs = Vec::with_capacity(NUM_ARGS);
    while reqs.len() < NUM_ARGS {
        reqs.push(Layout::from_size_align(randdist_reqsiz(&mut r), 1).unwrap());
    }

    let mut i = 0;
    c.bench_function("alloc", |b| b.iter(|| {
        let l = reqs[i % reqs.len()];
        black_box(unsafe { SM.alloc(l) });
        i += 1
    }));
}

fn bench_free(c: &mut Criterion) {
    let mut r = StdRng::seed_from_u64(0);
    let mut reqs = Vec::with_capacity(NUM_ARGS);
    while reqs.len() < NUM_ARGS {
        let l = Layout::from_size_align(randdist_reqsiz(&mut r), 1).unwrap();
        reqs.push((unsafe { SM.alloc(l) }, l));
    }

    let mut i = 0;
    c.bench_function("free", |b| b.iter(|| {
        let (p, l) = reqs[i % reqs.len()];
        unsafe { SM.dealloc(p, l) };
        i += 1
    }));
}

    // const MAX: usize = 2usize.pow(39);
    // const NUM_ARGS: usize = 128;

    // // #[bench]
    // // fn bench_alloc_and_free_32_threads(b: &mut Bencher) {
    // //     let l = Layout::from_size_align(64, 1).unwrap();

    // //     let mut r = StdRng::seed_from_u64(0);
    // //     let mut ps = Vec::new();

    // //     b.iter(|| {
    // //         if r.random::<bool>() {
    // //             // Free
    // //             if !ps.is_empty() {
    // //                 let i = r.random_range(0..ps.len());
    // //                 let (p, l2) = ps.remove(i);
    // //                 unsafe { SM.dealloc(p, l2) };
    // //             }
    // //         } else {
    // //             // Malloc
    // //             let p = unsafe { SM.alloc(l) };
    // //             ps.push((p, l));
    // //         }
    // //     });
    // // }

    // use std::ptr::null_mut;

criterion_group!{
    name = smalloc;
    config = Criterion::default().warm_up_time(Duration::from_millis(100)).measurement_time(Duration::from_millis(1000));
    targets = bench_sum_large_slab_sizes, bench_sum_small_slab_sizes, bench_pop_large_flh, bench_inner_alloc, bench_inner_large_alloc, bench_inner_small_alloc, bench_pop_small_flh, bench_new_from_ptr, bench_alloc, bench_free
}

criterion_main!(smalloc);

