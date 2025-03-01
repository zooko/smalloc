#![feature(test)]
extern crate test;

use rand::Rng;
use test::Bencher;

use smalloc::layout_to_sizeclass;
use smalloc::sizeclass_to_slotsize;

use std::hint::black_box;

const MAX: usize = 2usize.pow(39);
const NUM_ARGS: usize = 64;

#[bench]
fn bench_layout_to_sizeclass_noalign(b: &mut Bencher) {
    let mut r = rand::rng();
    let reqsizs: Vec<usize> = (0..NUM_ARGS).map(|_| r.random_range(0..MAX)).collect();
    let mut i = 0;

    b.iter(|| {
        let num = reqsizs[i];
        black_box(layout_to_sizeclass(num, 1));
        i = (i + 1) % NUM_ARGS;
    });
}

#[bench]
fn bench_layout_to_sizeclass_align(b: &mut Bencher) {
    let mut r = rand::rng();
    let reqsizs: Vec<usize> = (0..NUM_ARGS).map(|_| r.random_range(0..MAX)).collect();
    let reqalignments: Vec<usize> = (0..NUM_ARGS).map(|_| 2usize.pow(r.random_range(0..7))).collect();
    let mut i = 0;

    b.iter(|| {
        let num = reqsizs[i];
        let align = reqalignments[i];
        black_box(layout_to_sizeclass(num, align));

        i = (i + 1) % NUM_ARGS;
    });
}

#[bench]
fn bench_layout_to_sizeclass_hugealign(b: &mut Bencher) {
    let mut r = rand::rng();
    let reqsizs: Vec<usize> = (0..NUM_ARGS).map(|_| r.random_range(0..MAX)).collect();
    let reqalignments: Vec<usize> = (0..NUM_ARGS).map(|_| 2usize.pow(r.random_range(0..35))).collect();
    let mut i = 0;

    b.iter(|| {
        let num = reqsizs[i];
        let align = reqalignments[i];
        black_box(layout_to_sizeclass(num, align));

        i = (i + 1) % NUM_ARGS;
    });
}

#[bench]
fn bench_sizeclass_to_slotsize(b: &mut Bencher) {
    let mut r = rand::rng();
    let reqscs: Vec<u8> = (0..NUM_ARGS).map(|_| r.random_range(0..35)).collect();
    let mut i = 0;

    b.iter(|| {
        let sc = reqscs[i];
        black_box(sizeclass_to_slotsize(sc));

        i = (i + 1) % NUM_ARGS;
    });
}

#[bench]
fn bench_pot_builtin_randoms(b: &mut Bencher) {
    let mut r = rand::rng();
    let reqalignments: Vec<usize> = (0..NUM_ARGS).map(|_| r.random_range(0..MAX)).collect();
    let mut i = 0;

    b.iter(|| {
        let align = reqalignments[i];
        black_box(align.is_power_of_two());

        i = (i + 1) % NUM_ARGS;
    });
}

#[bench]
fn bench_pot_builtin_powtwos(b: &mut Bencher) {
    let mut r = rand::rng();
    let reqalignments: Vec<usize> = (0..NUM_ARGS).map(|_| 2usize.pow(r.random_range(0..35))).collect();
    let mut i = 0;

    b.iter(|| {
        let align = reqalignments[i];
        black_box(align.is_power_of_two());

        i = (i + 1) % NUM_ARGS;
    });
}

#[bench]
fn bench_pot_bittwiddle_randoms(b: &mut Bencher) {
    let mut r = rand::rng();
    let reqalignments: Vec<usize> = (0..NUM_ARGS).map(|_| r.random_range(0..MAX)).collect();
    let mut i = 0;

    b.iter(|| {
        let align = reqalignments[i];
        black_box(align > 0 && (align & (align - 1)) != 0);

        i = (i + 1) % NUM_ARGS;
    });
}

#[bench]
fn bench_pot_bittwiddle_powtwos(b: &mut Bencher) {
    let mut r = rand::rng();
    let reqalignments: Vec<usize> = (0..NUM_ARGS).map(|_| 2usize.pow(r.random_range(0..35))).collect();
    let mut i = 0;

    b.iter(|| {
        let align = reqalignments[i];
        black_box(align > 0 && (align & (align - 1)) != 0);

        i = (i + 1) % NUM_ARGS;
    });
}

