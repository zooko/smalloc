// This executable demonstrates how to initialize and use smalloc with the ctor approach. There are
// two things you need to do: declare a static smalloc instance as the `#[global_allocator]`, and
// define an `init_smalloc()` function like this:

use smalloc::Smalloc;
#[global_allocator]
static ALLOC: Smalloc = Smalloc::new();

#[ctor::ctor]
unsafe fn init_smalloc() {
    unsafe { ALLOC.init() };
}

pub fn main() {
    println!("Hello, world! I'm smalloc. :-)");

    const NUM_ELEMS: usize = 9999;

    let vu8s: Vec<u8> = vec![7; NUM_ELEMS];
    let vu128s: Vec<u128> = vec![11; NUM_ELEMS];

    const I: usize = 7777;

    println!("vu8s[{}] = {}", I, vu8s[I]);
    println!("vu128s[{}] = {}", I, vu128s[I]);
}
