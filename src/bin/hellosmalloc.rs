#[cfg(not(test))]
mod notests {
    const MAX_U128: u128 = 2u128.pow(39);
    const MAX_U8: u8 = 2u8.pow(6);

    use smalloc::Smalloc;

    #[global_allocator]
    static SMALLOC: Smalloc = Smalloc::new();

    use rand::rngs::StdRng;
    use rand::SeedableRng;
    use rand::Rng;
    
    pub fn main() {
        println!("Hello, world!");

        let mut r = StdRng::seed_from_u64(0);

        let num_args: usize = r.random_range(0..2usize.pow(20));
        println!(
            "num_args: {}, bytes for Vec<u8> of that: {}, bytes for a Vec<u128> of that: {}",
            num_args,
            num_args,
            num_args * 16
        );

        let vu8s: Vec<u8> = (0..num_args).map(|_| r.random_range(0..MAX_U8)).collect();
        let vu128s: Vec<u128> = (0..num_args).map(|_| r.random_range(0..MAX_U128)).collect();

        let i = r.random_range(0..num_args);

        println!("vu8s[{}] = {}", i, vu8s[i]);
        println!("vu128s[{}] = {}", i, vu128s[i]);
    }
}

#[cfg(not(test))]
fn main() {
    notests::main();
}
