#![feature(test)]
extern crate test;

use rand::Rng;
use test::Bencher;

use smalloc::layout_to_sizeclass;
use smalloc::sizeclass_to_slotsize;

use std::hint::black_box;

const MAX: usize = 2usize.pow(39);
const NUM_ARGS: usize = 128;

#[bench]
fn bench_layout_to_sizeclass_noalign(b: &mut Bencher) {
    let mut r = rand::rng();
    let reqsizs: Vec<usize> = (0..NUM_ARGS).map(|_| r.random_range(0..MAX)).collect();
    let mut i = 0;

    b.iter(|| {
        let num = reqsizs[i % NUM_ARGS];
        black_box(layout_to_sizeclass(num, 1));
        i += 1;
    });
}

#[bench]
fn bench_layout_to_sizeclass_align(b: &mut Bencher) {
    let mut r = rand::rng();
    let reqsizs: Vec<usize> = (0..NUM_ARGS).map(|_| r.random_range(0..MAX)).collect();
    let reqalignments: Vec<usize> = (0..NUM_ARGS).map(|_| 2usize.pow(r.random_range(0..7))).collect();
    let mut i = 0;

    b.iter(|| {
        let num = reqsizs[i % NUM_ARGS];
        let align = reqalignments[i % NUM_ARGS];
        black_box(layout_to_sizeclass(num, align));

        i += 1;
    });
}

#[bench]
fn bench_layout_to_sizeclass_hugealign(b: &mut Bencher) {
    let mut r = rand::rng();
    let reqsizs: Vec<usize> = (0..NUM_ARGS).map(|_| r.random_range(0..MAX)).collect();
    let reqalignments: Vec<usize> = (0..NUM_ARGS).map(|_| 2usize.pow(r.random_range(0..35))).collect();
    let mut i = 0;

    b.iter(|| {
        let num = reqsizs[i % NUM_ARGS];
        let align = reqalignments[i % NUM_ARGS];
        black_box(layout_to_sizeclass(num, align));

        i += 1;
    });
}

#[bench]
fn bench_sizeclass_to_slotsize(b: &mut Bencher) {
    let mut r = rand::rng();
    let reqscs: Vec<usize> = (0..NUM_ARGS).map(|_| r.random_range(0..35)).collect();
    let mut i = 0;

    b.iter(|| {
        let sc = reqscs[i % NUM_ARGS];
        black_box(sizeclass_to_slotsize(sc));

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
