#![feature(test)]
extern crate test;

use rand::Rng;
use test::Bencher;

use smalloc::{layout_to_slabnum, slabnum_to_slotsize, slabnum_to_numareas, REAL_NUM_SLABS};

use std::hint::black_box;

const MAX: usize = 2usize.pow(39);
const NUM_ARGS: usize = 128;

#[bench]
fn bench_calc_size_of_data_slabs(b: &mut Bencher) {
    black_box(calc_size_of_data_slabs());
}

#[bench]
fn bench_calc_size_of_data_slabs_with_lookup_table(b: &mut Bencher) {
    black_box(calc_size_of_data_slabs_with_lookup_table());
}

#[bench]
fn bench_calculate_offset_of_vars(b: &mut Bencher) {
    let mut r = rand::rng();
    let reqslabnums: Vec<usize> = (0..NUM_ARGS).map(|_| r.random_range(0..REAL_NUM_SLABS)).collect();
    let reqareanums: Vec<usize> = (0..NUM_ARGS).map(|_| r.random_range(0..NUM_AREAS)).collect();
    let mut i = 0;

    let reqareanum: Vec<usize> = Vec::new();
    for i in 0..NUM_ARGS {
	if r.gen_bool(0.5) {
	    reqareanums.push(0);
	} else {
	    reqareanums.push(r.gen_range(1..NUM_AREAS));
	}
	if r.gen_bool(0.5) {
	    reqslabnums.push(r.gen_range(0..=MAX_SLABNUM_TO_PACK_MULTIPLE_INTO_CACHELINE));
	} else {
	    reqslabnums.push(r.gen_range(MAX_SLABNUM_TO_PACK_MULTIPLE_INTO_CACHELINE, REAL_NUM_SLABS));
	}
    }

    i = 0;
    b.iter(|| {
        let slabnum = reqslabnums[i % NUM_ARGS];
        let areanum = reqareanums[i % NUM_ARGS];
        black_box(calculate_offset_of_vars(areanum. slabnum));
        i += 1;
    });
}


#[bench]
fn bench_calculate_offset_of_vars_with_lookup_table(b: &mut Bencher) {
    let mut r = rand::rng();
    let reqslabnums: Vec<usize> = (0..NUM_ARGS).map(|_| r.random_range(0..REAL_NUM_SLABS)).collect();
    let reqareanums: Vec<usize> = (0..NUM_ARGS).map(|_| r.random_range(0..NUM_AREAS)).collect();
    let mut i = 0;

    let reqareanum: Vec<usize> = Vec::new();
    for i in 0..NUM_ARGS {
	if r.gen_bool(0.5) {
	    reqareanums.push(0);
	} else {
	    reqareanums.push(r.gen_range(1..NUM_AREAS));
	}
	if r.gen_bool(0.5) {
	    reqslabnums.push(r.gen_range(0..=MAX_SLABNUM_TO_PACK_MULTIPLE_INTO_CACHELINE));
	} else {
	    reqslabnums.push(r.gen_range(MAX_SLABNUM_TO_PACK_MULTIPLE_INTO_CACHELINE, REAL_NUM_SLABS));
	}
    }

    i = 0;
    b.iter(|| {
        let slabnum = reqslabnums[i % NUM_ARGS];
        let areanum = reqareanums[i % NUM_ARGS];
        black_box(calculate_offset_of_vars_with_lookup_table(areanum. slabnum));
        i += 1;
    });
}

#[bench]
fn bench_layout_to_slabnum_noalign(b: &mut Bencher) {
    let mut r = rand::rng();
    let reqsizs: Vec<usize> = (0..NUM_ARGS).map(|_| r.random_range(0..MAX)).collect();
    let mut i = 0;

    b.iter(|| {
        let num = reqsizs[i % NUM_ARGS];
        black_box(layout_to_slabnum(Layout::from_size_align(num, 1).unwrap()));
        i += 1;
    });
}

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

#[bench]
fn bench_layout_to_slabnum_hugealign(b: &mut Bencher) {
    let mut r = rand::rng();
    let reqsizs: Vec<usize> = (0..NUM_ARGS).map(|_| r.random_range(0..MAX)).collect();
    let reqalignments: Vec<usize> = (0..NUM_ARGS).map(|_| 2usize.pow(r.random_range(0..35))).collect();
    let mut i = 0;

    b.iter(|| {
        let num = reqsizs[i % NUM_ARGS];
        let align = reqalignments[i % NUM_ARGS];
        black_box(layout_to_slabnum(Layout::from_size_align(num, align).unwrap()));

        i += 1;
    });
}

#[bench]
fn bench_slabnum_to_slotsize(b: &mut Bencher) {
    let mut r = rand::rng();
    let reqscs: Vec<usize> = (0..NUM_ARGS).map(|_| r.random_range(0..REAL_NUM_SLABS)).collect();
    let mut i = 0;

    b.iter(|| {
        let sc = reqscs[i % NUM_ARGS];
        black_box(slabnum_to_slotsize(sc));

	i += 1;
    });
}

#[bench]
fn bench_slabnum_to_numareas(b: &mut Bencher) {
    let mut r = rand::rng();
    let reqscs: Vec<usize> = (0..NUM_ARGS).map(|_| r.random_range(0..REAL_NUM_SLABS)).collect();
    let mut i = 0;

    b.iter(|| {
        let sc = reqscs[i % NUM_ARGS];
        black_box(slabnum_to_numareas(sc));

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
