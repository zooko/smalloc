#![no_main]

use smalloc::smalloc_main;

use wyrand::WyRand;

#[smalloc_main]
pub fn main() {
    println!("Hello, world! I'm smalloc. :-)");

    let tvm = SMALLOC.get_total_virtual_memory();

    println!("Allocated total virtual memory: {tvm}");

    let mut r = WyRand::new(0);

    let num_args = r.rand() % 2u64.pow(20);
    println!(
        "num_args: {}, bytes for Vec<u8> of that: {}, bytes for a Vec<u128> of that: {}",
        num_args,
        num_args,
        num_args * 16
    );

    let vu8s: Vec<u8> = (0..num_args).map(|_| r.rand() as u8).collect();
    let vu128s: Vec<u128> = (0..num_args).map(|_| r.rand() as u128).collect();

    let i = (r.rand() % num_args) as usize;

    println!("vu8s[{}] = {}", i, vu8s[i]);
    println!("vu128s[{}] = {}", i, vu128s[i]);
}
