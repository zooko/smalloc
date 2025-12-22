#![no_main]

// This executable demonstrates how to initialize and use smalloc with the proc-macro
// approach. There are two things you need to do: put `#![no_main] at the top of your lib.rs and put
// `#[smalloc_main]` before your `main()` function.

use smalloc::smalloc_main;

#[smalloc_main]
pub fn main() {
    println!("Hello, world! I'm smalloc. :-)");

    const NUM_ELEMS: usize = 9999;

    let vu8s: Vec<u8> = vec![7; NUM_ELEMS];
    let vu128s: Vec<u128> = vec![11; NUM_ELEMS];

    const I: usize = 7777;

    println!("vu8s[{}] = {}", I, vu8s[I]);
    println!("vu128s[{}] = {}", I, vu128s[I]);
}
