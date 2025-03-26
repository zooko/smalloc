pub const MAX_SLABNUM_TO_PACK_INTO_CACHELINE: usize = 10;
pub const MAX_SLABNUM_TO_FIT_INTO_CACHELINE: usize = 11;
pub const MAX_SLABNUM_TO_PACK_INTO_PAGE: usize = 16;
pub const LARGE_SLOTS_SLABNUM: usize = 17;
pub const OVERSIZE_SLABNUM: usize = 18;
pub const NUM_SLABS: usize = 19;

//pub const SIZE_OF_LARGE_SLOTS: usize = 16_777_216; // 16 mebibytes
//pub const SIZE_OF_LARGE_SLOTS: usize = 12_582_912; // 12 mebibytes
//pub const SIZE_OF_LARGE_SLOTS: usize = 10_485_760; // 10 mebibytes
//pub const SIZE_OF_LARGE_SLOTS: usize = 8_388_608; // 8 mebibytes
//pub const SIZE_OF_LARGE_SLOTS: usize = 6_291_456; // 6 mebibytes
pub const SIZE_OF_LARGE_SLOTS: usize = 5_242_880; // 5 mebibytes
//pub const SIZE_OF_LARGE_SLOTS: usize = 4_194_304; // 4 mebibytes

pub const NUM_SLOTS: usize = 16_777_215; // 8 * 2^20
// pub const NUM_SLOTS: usize = 35_000_000;
// pub const NUM_SLOTS: usize = 5_000_000;
//pub const NUM_SLOTS: usize = 8_388_608; // 8 * 2^20
//pub const NUM_SLOTS: usize = 4_194_304; // 4 * 2^20

// Total virtual memory space that we were able to allocate in testing on MacOS on Apple M4, rounded down to the nearest 5 trillion bytes. :-)
// When testing on a linux machine (AMD EPYC 3151) and 32,711,276 bytes RAM, with overcommit=1, the amount I was able to mmap() varied. :-( One time I could mmap() only 95,175,252,639,744 bytes.
pub const TOT_VM_ALLOCATABLE: usize = 95_000_000_000_000;

// These sizes were chosen by calculating how many objects of this size would fit into the least-well-packed 64-byte cache line when we lay out objects of these size end-to-end over many successive 64-byte cache lines. If that makes sense. (Taking the biggest object if there is a tie for how many can fit into the least-well-packaged cache line.)

// size: worst fit into a 64-byte cache line:
// 32      2
// 16      4
// 10      5
//  9      6
//  8      8
//  6     10
//  5     12
//  4     16
//  3     20
//  2     32
//  1     64

#[inline(always)]
pub fn slabnum_to_slotsize(slabnum: usize) -> usize {
    // Sizes where we can fit more slots into a 64-byte cache line. (And kinda maybe 128-byte cache-areas in certain ways...)
    if slabnum == 0 { 1 }
    else if slabnum == 1 { 2 }
    else if slabnum == 2 { 3 }
    else if slabnum == 3 { 4 }
    else if slabnum == 4 { 5 }
    else if slabnum == 5 { 6 }
    else if slabnum == 6 { 8 }
    else if slabnum == 7 { 9 }
    else if slabnum == 8 { 10 }
    else if slabnum == 9 { 16 }
    else if slabnum == 10 { 32 } // MAX_SLABNUM_TO_PACK_INTO_CACHELINE // two or more

    // Debatable whether 64-byte allocations can benefit from sharing cachelines. Definitely not for 64B cachlines, but new Apple chips have 128B cachelines (in some cores) and cacheline pre-fetching on at least some modern Intel and AMD CPUs might give a caching advantage to having 64B slots. In any case, we're including a slot for 64B slots because of that, and because 64B slots pack nicely into 4096-byte memory pages. But the grower-promotion strategy will treat 32B slots (slab num 13) as the largest that can pack multiple objects into cachelines, ie it will promote any growers to at least slab num 15.
    else if slabnum == 11 { 64 } // MAX_SLABNUM_TO_FIT_INTO_CACHELINE // by itself

    // Sizes where we can fit more slots into a 4096-byte memory page.

    else if slabnum == 12 { 128 }
    else if slabnum == 13 { 256 }
    else if slabnum == 14 { 512 }
    else if slabnum == 15 { 1024 }
    else if slabnum == 16 { 2048 } // MAX_SLABNUM_TO_PACK_INTO_PAGE

    // Large slots.
    else { SIZE_OF_LARGE_SLOTS } // LARGE_SLOTS_SLABNUM
}

//pub const NUM_AREAS: usize = 256;
//pub const NUM_AREAS: usize = 32;
pub const NUM_AREAS: usize = 64;

#[inline(always)]
pub fn slabnum_to_numareas(slabnum: usize) -> usize {
    if slabnum <= MAX_SLABNUM_TO_PACK_INTO_CACHELINE {
	NUM_AREAS
    } else {
	1
    }
}

#[inline(always)]
pub fn layout_to_slabnum(layout: Layout) -> usize {
    let size = layout.size();
    let alignment = layout.align();

    assert!(alignment > 0 && (alignment & (alignment - 1)) == 0, "alignment must be a power of two"); // benchmarks show this bittwiddling expression is a teeeny bit more efficient than the builtin power-of-two function (on some x86-64 systems but not others, and on Apple M4 Max).

    // Round up size to the nearest multiple of alignment:
    let alignedsize: usize = ((size - 1) | (alignment - 1)) + 1;

    for slabnum in 0..NUM_SLABS {
	if alignedsize <= slabnum_to_slotsize(slabnum) {
	    return slabnum;
	}
    }

    OVERSIZE_SLABNUM
}

use core::{
    alloc::{GlobalAlloc, Layout}
};

use memmapix::{MmapOptions, MmapMut, Advice};

fn mmap(reqsize: usize) -> MmapMut {
    let mm = MmapOptions::new().len(reqsize).map_anon().unwrap();
    mm.advise(Advice::Random).ok();
    mm

    // XXX We'll have to use https://docs.rs/rustix/latest/rustix/mm/fn.madvise.html to madvise more flags...

    //XXX for Linux: MapFlags::UNINITIALIZED . doesn't really optimize much even when it works and it only works on very limited platforms (because it is potentially exposing other process's information to our process
    //XXX | MapFlags::MADV_RANDOM | MapFlags::MADV_DONTDUMP
    //XXX Look into purgable memory on Mach https://developer.apple.com/library/archive/documentation/Performance/Conceptual/ManagingMemory/Articles/CachingandPurgeableMemory.html
    //XXX Look into MADV_FREE on MacOS (and maybe iOS?) (compared to MADV_DONTNEED on Linux)
}

pub struct Smalloc {
    bp: Option<MmapMut>
}

impl Smalloc {
    pub const fn new() -> Self {
	Self {
	    bp: None
	}
    }

    fn init_mmap(&self) {
//xxx	self.bp = Some(mmap(required_virtual_memory()));
    }
}

use std::sync::atomic::{AtomicUsize, AtomicU8, Ordering};
//use std::thread;

// global (static) THREAD_ID
static GLOBAL_THREAD_ID: AtomicU8 = AtomicU8::new(0);

thread_local!(static THREAD_AREA_NUM: u8 = GLOBAL_THREAD_ID.fetch_add(1, Ordering::Relaxed));

fn thread_area_num() -> u8 {
    THREAD_AREA_NUM.with(|&id| id)
}

//xxxfn calc_addr_of_ffs(slabnum: usize, area: usize) {
//xxx    let sizeof_var = 4;
//xxx    let sizeof_vars_for_one_slab = sizeofvar * 2;
//xxx
//xxx    let num_slabs_below_this_one = if slabnum == 0 {
//xxx	0
//xxx    } else 
//xxx//XXXX neeed alignment on cachelines for all slabs!?
//xxx
//xxx    let sizeof_vars_for_all_slabs_of_this_size = numareasforslab * sizeofslabsvars;
//xxx}

//xxx /// Returns 0 if the free list is empty or one greater than the index of the former head of the free list.
//xxxfn pop_ffs_head(slabnum: usize, area: usize) -> u16 {
//xxx    let addr_of_ffs = calc_addr_of_ffs(slabnum, area);
//xxx
//xxx    // Uh, now we need a Rust local variable which is actually the 3 bytes of the `ffs` in order to use Rust's atomic features on it... 🤔
//xxx    let theffs = AtomicUsize::from_ptr(addr_of_ffs);
//xxx
//xxx    let ffs = load_ffs_from_mem(addr_of_ffs);
//xxx    if ffs == 0 {
//xxx	return 0;
//xxx    }
//xxx    let ffsindex = ffs-1;
//xxx
//xxx    let baseptr_of_freelist = calc_addr_of_freelist(slabnum, area);
//xxx    let slot_size_of_freelist = calc_slot_size_of_freelist(slabnum);
//xxx    let freelistnewheadindex = load_free_list_index(baseptr_of_freelist + ffsindex * slot_size_of_freelist);
//xxx
//xxx
//xxx//	.compare_and_exchange_weak(ffs, freelistnewheadindex, Ordering::This, Ordering::That);
//xxx}
   


//xxxunsafe impl GlobalAlloc for Smalloc {
//xxx    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
//xxx	let bp = 0; // XXX This is actually the base pointer of our anonymous mmap.
//xxx
//xxx	// To `malloc()`, we first compute the smallest slab this allocation would fit in:
//xxx	let slabnum = layout_to_slabnum(layout);
//xxx
//xxx	let area = thread_area_num();
//xxx	
//xxx	// Try to pop the head of the free list for this slab in this area.
//xxx	
//xxx	// XXX mark this as very unlikely/cold
//xxx
//xxx//xxx	let bp = &mut self.baseptr;
//xxx//xxx	let mp = bp.as_mut_ptr();
//xxx//xxx	let _slabnum = layout_to_slabnum(layout.size(), layout.align()); // XXX consider passing Layout struct to layout_to_slabnum() instead of breaking it into its parts like this.
//xxx	eprintln!("xxxslabnum: {}", slabnum);
//xxx//xxx	let x = self.baseptr.as_mut_ptr(); //xxx 
//xxx//xxx	return mp;
//xxx    }
//xxx
//xxx    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
//xxx    }
//xxx
//xxx    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
//xxx//xxx	return self.baseptr.as_mut_ptr(); //xxx
//xxx    }
//xxx}

// Water shares not being used. Could sell. Hoshi needs more water. Mutual ditch company, Beetle owns the shares in the company. Beaver Creek Water Irrigation Ditch Company. If its feasible I definitely would buy it in a heartbeat. I don't know how else you could sell it other than to the ditch company.

