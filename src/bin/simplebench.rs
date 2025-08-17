#![feature(rustc_private)]

#[cfg(not(test))]
mod notests {
    use smalloc::benches::{bench_allocator, GlobalAllocWrap};
    use smalloc::Smalloc;
    use std::sync::Arc;
    use std::thread;

    pub fn main() {
        let mut handles = Vec::new();

        handles.push(thread::spawn(|| {
            bench_allocator("smalloc", &Arc::new(Smalloc::new()));
        }));

        handles.push(thread::spawn(|| {
            bench_allocator("builtin", &Arc::new(GlobalAllocWrap));
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
