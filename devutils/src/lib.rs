// This file contains things used by both tests and benchmarks.

// xxx consider removing some of these "helper"/utility functions in favor of standard/idiomatic things...

const BYTES1: [u8; 8] = [1, 2, 4, 3, 5, 6, 7, 8];
const BYTES2: [u8; 8] = [9, 8, 7, 6, 5, 4, 3, 2];
const BYTES3: [u8; 8] = [0xA, 0xB, 0xC, 0xD, 0xE, 0xF, 0x10, 0x11];
const BYTES4: [u8; 8] = [0x12, 0x11, 0x10, 0xF, 0xE, 0xD, 0xC, 0xB];
const BYTES5: [u8; 8] = [0xFE, 0xFD, 0xFC, 0xFB, 0xFA, 0xF9, 0xF8, 0xF7];
//xxxconst BYTES6: [u8; 8] = [0xFE, 0xFD, 0xF6, 0xF5, 0xFA, 0xF9, 0xF8, 0xF7];

use std::alloc::GlobalAlloc;
use std::cmp::min;
use std::thread;
use std::alloc::Layout;

use smalloc::*;

// For testing and benchmarking only.
pub mod dev_instance {
    use crate::*;

    pub static mut DEV_SMALLOC: Smalloc = Smalloc::new();

    #[macro_export]
    macro_rules! get_devsmalloc {
        () => {
            unsafe { &*std::ptr::addr_of!($crate::dev_instance::DEV_SMALLOC) }
        };
    }
}

#[inline(always)]
pub fn adrww<T: GlobalAlloc>(al: &T, s: &mut TestState) {
    // random coin
    let coin = s.next_coin() % 3;
    if unlikely(s.ps.is_empty()) || coin == 0 {
        // Malloc
        let lt = s.next_layout();
        let p = unsafe { al.alloc(lt) };
        debug_assert!(!p.is_null(), "{lt:?}");
        // Write to the allocation
        unsafe { std::ptr::copy_nonoverlapping(BYTES1.as_ptr(), p, min(BYTES1.len(), lt.size())) };

        #[cfg(debug_assertions)] {
            debug_assert!(!s.m.contains(&(p as usize, lt)), "{:?} {}-{}", p, lt.size(), lt.align()); // This line is the only reason s.m exists.
            s.m.insert((p as usize, lt));

            // Write to a random (other) allocation...
            if !s.ps.is_empty() {
                let (po, lto) = s.get_p_biased();
                unsafe { std::ptr::copy_nonoverlapping(BYTES2.as_ptr(), po as *mut u8, min(BYTES2.len(), lto.size())) };
            }
        }

        s.ps.push((p as usize, lt));
    } else if coin == 1 {
        // Free
        debug_assert!(!s.ps.is_empty());

        let (p, lt) = s.remove_next_p();

        #[cfg(debug_assertions)] {
            // Write to a random (other) allocation...
            if !s.ps.is_empty() {
                let (po, lto) = s.get_p_biased();
                unsafe { std::ptr::copy_nonoverlapping(BYTES3.as_ptr(), po as *mut u8, min(BYTES3.len(), lto.size())) };
            }

            debug_assert!(s.m.contains(&(p, lt)), "{:?} {}-{}", p, lt.size(), lt.align()); // This line is the only reason s.m exists.
            s.m.remove(&(p, lt));
        }

        // Read from this location before dealloc'ing it.
        unsafe {
            let ln = min(BYTES1.len(), lt.size());
            let data = std::slice::from_raw_parts(p as *const u8, ln);
            // Useful assertion for looking for bugs when testing, plus when this is used for
            // benchmarking, this prevents the compiler from optimizing away the read.

            assert!(*data == BYTES1[..ln] || *data == BYTES2[..ln] || *data == BYTES3[..ln] || *data == BYTES4[..ln] || *data == BYTES5[..ln], "data: {data:?}, BYTES1: {BYTES1:?}, BYTES2: {BYTES2:?}, BYTES3: {BYTES3:?}, BYTES4: {BYTES4:?}, BYTES5: {BYTES5:?}");
        }
        unsafe { al.dealloc(p as *mut u8, lt) };
    } else {
        // Realloc
        debug_assert!(!s.ps.is_empty());

        let (p, lt) = s.remove_next_p();

        debug_assert!(s.m.contains(&(p, lt)), "{:?} {}-{}", p, lt.size(), lt.align());
        #[cfg(debug_assertions)] { s.m.remove(&(p, lt)); }

        let newlt = s.next_layout();

        // Read from this location before realloc'ing it.
        unsafe {
            let ln = min(BYTES1.len(), lt.size());
            let data = std::slice::from_raw_parts(p as *const u8, ln);
            // Useful assertion for looking for bugs when testing, plus when this is used for
            // benchmarking, this prevents the compiler from optimizing away the read.

	    assert!(*data == BYTES1[..ln] || *data == BYTES2[..ln] || *data == BYTES3[..ln] || *data == BYTES4[..ln] || *data == BYTES5[..ln]);
        }
        let newp = unsafe { al.realloc(p as *mut u8, lt, newlt.size()) };
        // Write to the (possibly new) location after realloc'ing it.
        unsafe { std::ptr::copy_nonoverlapping(BYTES4.as_ptr(), newp, min(BYTES4.len(), newlt.size())) };

        debug_assert!(!s.m.contains(&(newp as usize, newlt)), "{:?} {}-{}", newp, newlt.size(), newlt.align()); // This line is the only reason s.m exists.
        #[cfg(debug_assertions)] { s.m.insert((newp as usize, newlt)); }

        #[cfg(debug_assertions)] {
            // Write to a random (other) allocation...
            if !s.ps.is_empty() {
                let (po, lto) = s.get_p_biased();
                unsafe { std::ptr::copy_nonoverlapping(BYTES5.as_ptr(), po as *mut u8, min(BYTES5.len(), lto.size())) };
            }
        }

        s.ps.push((newp as usize, newlt));
    }
}

#[inline(always)]
pub fn adr<T: GlobalAlloc>(al: &T, s: &mut TestState) {
    // random coin
    let coin = s.next_coin() % 3;
    if unlikely(s.ps.is_empty()) || coin == 0 {
        // Malloc
        let lt = s.next_layout();
        let p = unsafe { al.alloc(lt) };
        debug_assert!(!p.is_null(), "{lt:?}");

        #[cfg(debug_assertions)] {
            debug_assert!(!s.m.contains(&(p as usize, lt)), "{:?} {}-{}", p, lt.size(), lt.align()); // This line is the only reason s.m exists.
            s.m.insert((p as usize, lt));
        }

        s.ps.push((p as usize, lt));
    } else if coin == 1 {
        // Free
        debug_assert!(!s.ps.is_empty());

        let (p, lt) = s.remove_next_p();
	
        #[cfg(debug_assertions)] {
            debug_assert!(s.m.contains(&(p, lt)), "{:?} {}-{}", p, lt.size(), lt.align()); // This line is the only reason s.m exists.
	    s.m.remove(&(p, lt));
	}

        unsafe { al.dealloc(p as *mut u8, lt) };
    } else {
        // Realloc
        debug_assert!(!s.ps.is_empty());

	let (p, lt) = s.remove_next_p();

        debug_assert!(s.m.contains(&(p, lt)), "{:?} {}-{}, m.len(): {}", p, lt.size(), lt.align(), s.m.len());
        #[cfg(debug_assertions)] { s.m.remove(&(p, lt)); }

        let newlt = s.next_layout();

        let newp = unsafe { al.realloc(p as *mut u8, lt, newlt.size()) };

        debug_assert!(!s.m.contains(&(newp as usize, newlt)), "{:?} {}-{}", newp, newlt.size(), newlt.align()); // This line is the only reason s.m exists.
        #[cfg(debug_assertions)] { s.m.insert((newp as usize, newlt)); }

        s.ps.push((newp as usize, newlt));
    }
}

#[inline(always)]
pub fn adww<T: GlobalAlloc>(al: &T, s: &mut TestState) {
    // random coin
    let coin = s.next_coin() % 2;
    if unlikely(s.ps.is_empty()) || coin == 0 {
        // Malloc
        let lt = s.next_layout();
        let p = unsafe { al.alloc(lt) };
        debug_assert!(!p.is_null(), "{lt:?}");
        // Write to the allocation
        unsafe { std::ptr::copy_nonoverlapping(BYTES1.as_ptr(), p, min(BYTES1.len(), lt.size())) };

        #[cfg(debug_assertions)] {
            debug_assert!(!s.m.contains(&(p as usize, lt)), "{:?} {}-{}", p, lt.size(), lt.align()); // This line is the only reason s.m exists.
            s.m.insert((p as usize, lt));

            // Write to a random (other) allocation...
            if !s.ps.is_empty() {
                let (po, lto) = s.get_p_biased();
                unsafe { std::ptr::copy_nonoverlapping(BYTES2.as_ptr(), po as *mut u8, min(BYTES2.len(), lto.size())) };
            }
        }

        s.ps.push((p as usize, lt));

        if s.cached_8 > 0 {
            s.cached_8 -= 1;
        } else {
            s.num_popped_out_of_8_cache += 1;
        }
        if s.cached_512 > 0 {
            s.cached_512 -= 1;
        } else {
            s.num_popped_out_of_512_cache += 1;
        }
    } else {
        // Free
        debug_assert!(!s.ps.is_empty());

        let (p, lt) = s.remove_next_p();

        #[cfg(debug_assertions)] {
            // Write to a random (other) allocation...
            if !s.ps.is_empty() {
                let (po, lto) = s.get_p_biased();
                unsafe { std::ptr::copy_nonoverlapping(BYTES3.as_ptr(), po as *mut u8, min(BYTES3.len(), lto.size())) };
            }

            debug_assert!(s.m.contains(&(p, lt)), "{:?} {}-{}", p, lt.size(), lt.align()); // This line is the only reason s.m exists.
            s.m.remove(&(p, lt));
        }

        // Read from this location before dealloc'ing it.
        unsafe {
            let ln = min(BYTES1.len(), lt.size());
            let data = std::slice::from_raw_parts(p as *const u8, ln);
            // Useful assertion for looking for bugs when testing, plus when this is used for
            // benchmarking, this prevents the compiler from optimizing away the read.

            assert!(*data == BYTES1[..ln] || *data == BYTES2[..ln] || *data == BYTES3[..ln], "data: {data:?}, BYTES1: {BYTES1:?}, BYTES2: {BYTES2:?}, BYTES3: {BYTES3:?}");
        }
        unsafe { al.dealloc(p as *mut u8, lt) };

        s.cached_8 = min(8, s.cached_8 + 1);
        s.cached_512 = min(512, s.cached_512 + 1);
    }
}

#[inline(always)]
pub fn ad<T: GlobalAlloc>(al: &T, s: &mut TestState) {
    // random coin
    let coin = s.next_coin() % 2;
    if unlikely(s.ps.is_empty()) || coin == 0 {
        // Malloc
        let lt = s.next_layout();
        let p = unsafe { al.alloc(lt) };
        debug_assert!(!p.is_null(), "{lt:?}");

        #[cfg(debug_assertions)] {
            debug_assert!(!s.m.contains(&(p as usize, lt)), "{:?} {}-{}", p, lt.size(), lt.align()); // This line is the only reason s.m exists.
            s.m.insert((p as usize, lt));
        }

        s.ps.push((p as usize, lt));

        if s.cached_8 > 0 {
            s.cached_8 -= 1;
        } else {
            s.num_popped_out_of_8_cache += 1;
        }
        if s.cached_512 > 0 {
            s.cached_512 -= 1;
        } else {
            s.num_popped_out_of_512_cache += 1;
        }
    } else {
        // Free
        debug_assert!(!s.ps.is_empty());

        let (p, lt) = s.remove_next_p();

        #[cfg(debug_assertions)] {
            debug_assert!(s.m.contains(&(p, lt)), "{:?} {}-{}", p, lt.size(), lt.align()); // This line is the only reason s.m exists.
            s.m.remove(&(p, lt));
        }

        unsafe { al.dealloc(p as *mut u8, lt) };

        s.cached_8 = min(8, s.cached_8 + 1);
        s.cached_512 = min(512, s.cached_512 + 1);
    }
}

#[inline(always)]
pub fn aww<T: GlobalAlloc>(al: &T, s: &mut TestState) {
    // Malloc
    let lt = s.next_layout();
    let p = unsafe { al.alloc(lt) };
    debug_assert!(!p.is_null(), "lt: {lt:?}");
    debug_assert!(!s.m.contains(&(p as usize, lt)), "{:?} {}-{}", p, lt.size(), lt.align()); // This line is the only reason s.m exists.
    #[cfg(debug_assertions)] { s.m.insert((p as usize, lt)); }
    // Write to the allocation
    unsafe { std::ptr::copy_nonoverlapping(BYTES1.as_ptr(), p, min(BYTES1.len(), lt.size())) };

    #[cfg(debug_assertions)] {
        // Write to a random (other) allocation...
        if !s.ps.is_empty() {
            let (po, lto) = s.get_p_biased();
            unsafe { std::ptr::copy_nonoverlapping(BYTES2.as_ptr(), po as *mut u8, min(BYTES2.len(), lto.size())) };
        }
    }

    s.ps.push((p as usize, lt));
}

#[inline(always)]
pub fn a<T: GlobalAlloc>(al: &T, s: &mut TestState) {
    // Malloc
    let lt = s.next_layout();
    let p = unsafe { al.alloc(lt) };
    debug_assert!(!p.is_null(), "lt: {lt:?}");
    debug_assert!(!s.m.contains(&(p as usize, lt)), "{:?} {}-{}", p, lt.size(), lt.align()); // This line is the only reason s.m exists.
    #[cfg(debug_assertions)] { s.m.insert((p as usize, lt)); }

    s.ps.push((p as usize, lt));
}

#[inline(always)]
pub fn one_ad<T: GlobalAlloc>(al: &T, s: &mut TestState) {
    // Malloc
    let lt = s.next_layout();
    let p = unsafe { al.alloc(lt) };
    debug_assert!(!p.is_null(), "lt: {lt:?}");
    unsafe { al.dealloc(p, lt) };
}

pub fn help_test_multithreaded_with_allocator<T, F>(f: F, threads: u32, iters_per_batch: u64, al: &T, tses: &mut [TestState])
where
    T: GlobalAlloc + Send + Sync,
    F: Fn(&T, &mut TestState) + Sync + Send + Copy + 'static
{
    assert!(tses.len() >= threads as usize, "Need at least {} TestStates", threads);

    thread::scope(|scope| {
        let tses_ptr = tses.as_mut_ptr() as usize;

        for t in 0..threads {
            scope.spawn(move || {
                let s = unsafe { &mut *(tses_ptr as *mut TestState).add(t as usize) };
                for _i in 0..iters_per_batch {
                    f(al, s);
                }
            });
        }
    });
}

use std::cmp::max;
fn gen_layouts() -> [Layout; NUMLAYOUTS] {
    let mut ls = Vec::new();
    //for siz in [4, 4, 4, 8, 8, 32, 32, 32, 32, 35, 64, 128, 2000, 8_000] {
    for siz in [4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 8, 8, 32, 32, 35, 64, 8_000] {
        ls.push(Layout::from_size_align(siz, 1).unwrap());

        ls.push(Layout::from_size_align(siz + 10, 1).unwrap());
        ls.push(Layout::from_size_align(max(siz, 11) - 10, 1).unwrap());
        ls.push(Layout::from_size_align(siz * 2, 1).unwrap());
    }

    ls.push(Layout::from_size_align(1_000_000, 1).unwrap());

    ls.try_into().unwrap()
}

trait VecUncheckedExt<T> {
    unsafe fn swap_remove_unchecked(&mut self, index: usize) -> T;
}

impl<T> VecUncheckedExt<T> for Vec<T> {
    #[inline]
    unsafe fn swap_remove_unchecked(&mut self, index: usize) -> T {
        debug_assert!(index < self.len());

        let len = self.len();
        let hole = unsafe { self.as_mut_ptr().add(index) };
        let value = unsafe { std::ptr::read(hole) };

        let back = len - 1;
        if index != back {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    self.as_ptr().add(back),
                    hole,
                    1
                );
            }
        }

        unsafe { self.set_len(back) };
        value
    }
}


const NUMCOINS: usize = 1024;
const NUMLAYOUTS: usize = 101;
use std::collections::HashSet;
pub struct TestState {
    coins: [u32; NUMCOINS],
    nextcoin: usize,
    layouts: [Layout; NUMLAYOUTS],
    nextlayout: usize,
    ps: Vec<(usize, Layout)>,
    nextp: usize,
    m: HashSet<(usize, Layout), WyHashBuilder>,
    cached_8: u64,
    cached_512: u64,
    pub num_popped_out_of_8_cache: u64,
    pub num_popped_out_of_512_cache: u64,
}

use wyhash::WyHash;

// Simple wrapper
#[derive(Clone)]
struct WyHashBuilder(u64);

impl std::hash::BuildHasher for WyHashBuilder {
    type Hasher = WyHash;
    fn build_hasher(&self) -> Self::Hasher {
        WyHash::with_seed(self.0)
    }
}

use wyrand::WyRand;
impl TestState {
    pub fn new(iters: u64, seed: u64) -> Self {
        let cap = iters as usize;
        let mut r = WyRand::new(seed);
        let m = HashSet::with_capacity_and_hasher(cap, WyHashBuilder(seed));
        let coins: [u32; NUMCOINS] = std::array::from_fn(|_| r.rand() as u32);
        let nextcoin = 0;
        let mut layouts = gen_layouts();
        layouts.shuffle(&mut r);
        let nextlayout = 0;

        Self {
            coins,
            nextcoin,
            layouts,
            nextlayout,
            m,
            ps: Vec::with_capacity(cap),
            nextp: 0,
            cached_8: 0,
            cached_512: 0,
            num_popped_out_of_8_cache: 0,
            num_popped_out_of_512_cache: 0,
        }
    }

    pub fn clean_up<T: GlobalAlloc>(&mut self, al: &T) {
        for (p, l) in &self.ps {
            unsafe { al.dealloc(*p as *mut u8, *l) }
        }
    }
    
    fn next_layout(&mut self) -> Layout {
        let layout = self.layouts[self.nextlayout]; self.nextlayout = (self.nextlayout + 1) % NUMLAYOUTS;
        layout
    }

    fn next_coin(&mut self) -> u32 {
        let coin = self.coins[self.nextcoin]; self.nextcoin = (self.nextcoin + 1) % NUMCOINS;
        coin
    }

    fn remove_next_p(&mut self) -> (usize, Layout) {
        debug_assert!(self.nextp < self.ps.len());
        debug_assert!(!self.ps.is_empty());
        let (u, l) = unsafe { self.ps.swap_remove_unchecked(self.nextp) };
        if likely(!self.ps.is_empty()) {
            if self.nextp.is_multiple_of(1000) {
                self.nextp = self.next_coin() as usize % self.ps.len();
            } else {
                self.nextp -= 1;
            }
        } else {
            debug_assert_eq!(self.nextp, 0);
        }

        (u, l)
    }

    fn _remove_p_even_distribution(&mut self) -> (usize, Layout) {
        let i = self.next_coin() as usize % self.ps.len();
        unsafe { self.ps.swap_remove_unchecked(i) }
    }

    fn _remove_p_recent(&mut self, cutoff: usize) -> (usize, Layout) {
        let i = self.ps.len() - 1 - (self.next_coin() as usize % min(cutoff, self.ps.len()));
        unsafe { self.ps.swap_remove_unchecked(i) }
    }

    fn _remove_p_biased(&mut self) -> (usize, Layout) {
        let i1 = self.next_coin() as usize % self.ps.len();
        let i2 = self.next_coin() as usize % self.ps.len();
        unsafe { self.ps.swap_remove_unchecked(min(i1, i2)) }
    }

    #[cfg(debug_assertions)] 
    fn get_p_biased(&mut self) -> (usize, Layout) {
        debug_assert!(!self.ps.is_empty());
        let i1 = self.next_coin() as usize % self.ps.len();
        let i2 = self.next_coin() as usize % self.ps.len();
        unsafe { *self.ps.get_unchecked(min(i1, i2)) }
    }
}

pub trait ShuffleSlice {
    fn shuffle(&mut self, rng: &mut WyRand);
}

impl<T> ShuffleSlice for [T] {
    fn shuffle(&mut self, rng: &mut WyRand) {
        for i in (1..self.len()).rev() {
            let j = rng.rand() as usize % (i + 1);
            self.swap(i, j);
        }
    }
}

#[macro_export]
macro_rules! nextest_integration_tests {
    (
        $(
            $(#[$attr:meta])*
            fn $name:ident() $body:block
        )*
    ) => {
        $(
            #[test]
            $(#[$attr])*
            fn $name() {
                if std::env::var("NEXTEST").is_err() {
                    panic!("This project requires cargo-nextest to run tests.");
                }
                
                $body
            }
        )*
    };
}
