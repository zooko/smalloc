#![feature(rustc_private)]

#![allow(unused_imports)]

#[cfg(not(test))]
mod notests {
    use smalloc::benches::{dummy_func, bench, alloc_and_free, GlobalAllocWrap};
    use smalloc::Smalloc;
    use std::sync::Arc;
    use std::thread;
    use std::hint::black_box;

    pub fn main() {
        let mut handles = Vec::new();

        handles.push(thread::spawn(|| {
            bench("dummy3029", 100_000, || {
                dummy_func(30, 29);
            });
        }));

        handles.push(thread::spawn(|| {
            bench("dummy3130", 100_000, || {
                dummy_func(31, 30);
            });
        }));

        //let iters = 1;
        //let iters = 2;
        //let iters = 4;
        //let iters = 8;
        //let iters = 16;
        //let iters = 32;
        //let iters = 64;
        // let iters = 128;
        //let iters = 256;
        //let iters = 1000;
        //let iters = 1_000_000;
        let iters = 1_000_000_000;
        //let iters = 10_000_000_000;
        let sm = Arc::new(Smalloc::new());
        sm.idempotent_init().unwrap();
        handles.push(thread::spawn(move || {
            bench("smalloc", iters, || {
                alloc_and_free(&sm);
            });
        }));

        let bi = Arc::new(GlobalAllocWrap);
        handles.push(thread::spawn(move || {
            bench("builtin", iters, || {
                alloc_and_free(&bi);
            });
        }));

        for handle in handles {
            handle.join().unwrap();
        }
    }
}

#[cfg(not(test))]
fn main() {
    notests::main();
}
