#![feature(test)]
extern crate test;

use rand::Rng;
use test::Bencher;

use smalloc::{layout_to_slabnum, slotlocation_of_ptr, SLABNUM_TO_SLOTSIZE, NUM_SLABS, NUM_SLABS_CACHELINEY, NUM_AREAS, NUM_SLOTS, SlotLocation};

use std::hint::black_box;

const MAX: usize = 2usize.pow(39);
const NUM_ARGS: usize = 128;

use std::alloc::Layout;

#[bench]
fn bench_layout_to_slabnum_align(b: &mut Bencher) {
    let mut r = rand::rng();
    let reqsizs: Vec<usize> = (0..NUM_ARGS).map(|_| r.random_range(0..MAX)).collect();
    let reqalignments: Vec<usize> = (0..NUM_ARGS).map(|_| 2usize.pow(r.random_range(0..7))).collect();
    let mut i = 0;

    b.iter(|| {
        let num = reqsizs[i % NUM_ARGS];
        let align = reqalignments[i % NUM_ARGS];
        black_box(layout_to_slabnum(Layout::from_size_align(num, align).unwrap()));

        i += 1;
    });
}

// XXX bench table-lookup offset-of-vars vs computation offset-of-vars

#[bench]
fn bench_slabnum_to_slotsize(b: &mut Bencher) {
    let mut r = rand::rng();
    let reqscs: Vec<usize> = (0..NUM_ARGS).map(|_| r.random_range(0..NUM_SLABS)).collect();
    let mut i = 0;

    b.iter(|| {
        let sc = reqscs[i % NUM_ARGS];
        black_box(SLABNUM_TO_SLOTSIZE[sc]);

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
    let mut r = rand::rng();
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
    let mut r = rand::rng();
    let reqalignments: Vec<usize> = (0..NUM_ARGS).map(|_| 2usize.pow(r.random_range(0..35))).collect();
    let mut i = 0;

    b.iter(|| {
        let align = reqalignments[i % NUM_ARGS];
        black_box(pot_builtin(align));

	i += 1;
    });
}

#[bench]
fn bench_pot_bittwiddle_randoms(b: &mut Bencher) {
    let mut r = rand::rng();
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
    let mut r = rand::rng();
    let reqalignments: Vec<usize> = (0..NUM_ARGS).map(|_| 2usize.pow(r.random_range(0..35))).collect();
    let mut i = 0;

    b.iter(|| {
        let align = reqalignments[i % NUM_ARGS];
	black_box(pot_bittwiddle(align));

	i += 1;
    });
}

use std::ptr::null_mut;

#[bench]
fn bench_slotlocation_of_ptr(b: &mut Bencher) {
    let mut r = rand::rng();
    let baseptr_for_testing: *mut u8 = null_mut();
    let mut reqptrs = [null_mut(); NUM_ARGS];
    let mut i = 0;
    while i < NUM_ARGS {
        // generate a random slot
        let areanum = r.random_range(0..NUM_AREAS);
        let slabnum;
        if areanum == 0 {
            slabnum = r.random_range(0..NUM_SLABS);
        } else {
            slabnum = r.random_range(0..NUM_SLABS_CACHELINEY);
        }
        let slotnum = r.random_range(0..NUM_SLOTS);
        let sl: SlotLocation = SlotLocation { areanum, slabnum, slotnum };

        // put the random slot's pointer into the test set
        reqptrs[i] = unsafe { baseptr_for_testing.add(sl.offset_of_slot()) };

        i += 1;
    }

    b.iter(|| {
        let ptr = reqptrs[i % NUM_ARGS];
        black_box(slotlocation_of_ptr(baseptr_for_testing, ptr));

	i += 1;
    });
}
