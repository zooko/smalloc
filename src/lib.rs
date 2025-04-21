#![feature(pointer_is_aligned_to)]
#![feature(assert_matches)]
#![feature(unsigned_is_multiple_of)]
// #![allow(clippy::needless_range_loop)] // I like using needless range loops more than I like using enumerate.

// These slot sizes were chosen by calculating how many objects of this size would fit into the least-well-packed 64-byte cache line when we lay out objects of these size end-to-end over many successive 64-byte cache lines. If that makes sense. The worst-case number of objects that can be packed into a cache line can be up 2 fewer than the best-case, since the first object in this cache line might cross the cache line boundary and only the last part of the object is in this cache line, and the last object in this cache line might similarly be unable to fit entirely in and only the first part of it might be in this cache line. So this "how many fit" number below counts only the ones that entirely fit in, even when we are laying out objects of this size one after another (with no padding) across many cache lines. So it can be 0, 1, or 2 fewer than you might think. (Excluding any sizes which are smaller and can't fit more -- in the worst case -- than a larger size.)

// small slots:
//              worst-case number
//              that fit into one
//                      cacheline
// slabnum:      size:     (64B):
// --------   --------   --------
//        0       1  B         64
//        1       2  B         32
//        2       3  B         20
//        3       4  B         16
//        4       5  B         12
//        5       6  B         10
//        6       8  B          8
//        7       9  B          6
//        8      10  B          5
//        9      16  B          4
//       10      32  B          2

// large slots:
//                number that fit
//                       into one
//            virtual memory page
// slabnum:      size:    (4KiB):
// --------   --------   --------
//        0     64   B         64
//        1    128   B         32
//        2    256   B         16
//        3    512   B          8
//        4   1024   B          4
//        5   2048   B          2
//        6      4 MiB          0

// This is the largest alignment we can conveniently guarantee, based on Linux mmap() returning pointers aligned to at least this (in common configurations of linux).
pub const MAX_ALIGNMENT: usize = 4096;

pub const NUM_SMALL_SLABS: usize = 11;
pub const NUM_LARGE_SLABS: usize = 7;
const SIZE_OF_BIGGEST_SMALL_SLOT: usize = 32;
const SIZE_OF_LARGEST_SLOTS: usize = 4194304; // 4 * 2^20
pub const SMALL_SLABNUM_TO_SLOTSIZE: [usize; NUM_SMALL_SLABS] = [1, 2, 3, 4, 5, 6, 8, 9, 10, 16, SIZE_OF_BIGGEST_SMALL_SLOT];
pub const LARGE_SLABNUM_TO_SLOTSIZE: [usize; NUM_LARGE_SLABS] = [64, 128, 256, 512, 1024, 2048, SIZE_OF_LARGEST_SLOTS];

pub const fn small_slabnum_to_slotsize(smallslabnum: usize) -> usize {
    assert!(smallslabnum < NUM_SMALL_SLABS);
    SMALL_SLABNUM_TO_SLOTSIZE[smallslabnum]
}

pub const fn large_slabnum_to_slotsize(largeslabnum: usize) -> usize {
    assert!(largeslabnum < NUM_LARGE_SLABS);
    LARGE_SLABNUM_TO_SLOTSIZE[largeslabnum]
}
    
pub const NUM_SLOTS: usize = 20_971_520; // 20 * 2^20

// The per-slab variables and the free list entries have this size in bytes.
const WORDSIZE: usize = 4;

// There are 64 areas each with a full complements of small slabs.
// (Large slabs live in a separate region that is not one of those 64 areas.)
pub const NUM_SMALL_SLAB_AREAS: usize = 64;

// Intentionally not aligning this to anything bigger than WORDSIZE. (Which it will be anyway, so the next_multiple_of() here is a no-op.
const LARGE_SLABS_VARS_BASE_OFFSET: usize = (NUM_SMALL_SLAB_AREAS * NUM_SMALL_SLABS * 2 * WORDSIZE).next_multiple_of(WORDSIZE);

pub const VARIABLES_SPACE: usize = LARGE_SLABS_VARS_BASE_OFFSET + NUM_LARGE_SLABS * 2 * WORDSIZE;

fn offset_of_small_eac(areanum: usize, smallslabnum: usize) -> usize {
    ( areanum * NUM_SMALL_SLABS * 2 + smallslabnum * 2 ) * WORDSIZE
}
    
fn offset_of_large_eac(largeslabnum: usize) -> usize {
    LARGE_SLABS_VARS_BASE_OFFSET + largeslabnum * 2 * WORDSIZE
}
    
fn offset_of_small_flh(areanum: usize, smallslabnum: usize) -> usize {
    ( areanum * NUM_SMALL_SLABS * 2 + smallslabnum * 2 + 1) * WORDSIZE
}
    
fn offset_of_large_flh(largeslabnum: usize) -> usize {
    LARGE_SLABS_VARS_BASE_OFFSET + (largeslabnum * 2 + 1) * WORDSIZE
}

const CACHELINE_SIZE: usize = 64;

// Align the beginning of the separate free lists region to CACHELINE_SIZE.
pub const SEPARATE_FREELISTS_BASE_OFFSET: usize = VARIABLES_SPACE.next_multiple_of(CACHELINE_SIZE);

// The calls to next_multiple_of() on a space are to start the *next* thing on a cacheline boundary.
const SEPARATE_FREELIST_SPACE: usize = (NUM_SLOTS * WORDSIZE).next_multiple_of(CACHELINE_SIZE); // Size of each of the separate free lists
const NUM_SEPARATE_FREELISTS: usize = 3; // Number of separate free lists for slabs whose slots are too small to hold a 4-byte word (slab numbers 0, 1, and 2)

pub const SEPARATE_FREELISTS_SPACE_REGION: usize = NUM_SEPARATE_FREELISTS * SEPARATE_FREELIST_SPACE * NUM_SMALL_SLAB_AREAS;

// Align the beginning of the data slabs to MAX_ALIGNMENT. This is just to fit the maximum (4096) of smallest slots (1 byte) into a memory page.
pub const DATA_SLABS_BASE_OFFSET: usize = (SEPARATE_FREELISTS_BASE_OFFSET + SEPARATE_FREELISTS_SPACE_REGION).next_multiple_of(MAX_ALIGNMENT);

//XXX add benchmarking of the lookup-table version of this:
/// The sum of the sizes of the small slabs for one area up to numslabs (exclusive).
pub const fn sum_small_slab_sizes(numslabs: usize) -> usize {
    assert!(numslabs <= NUM_SMALL_SLABS);
    let mut slabnum = 0;
    let mut sum: usize = 0;
    while slabnum < numslabs {
        // Make the beginning of this slab start on a cache line boundary.
        sum = sum.next_multiple_of(CACHELINE_SIZE);
        sum += small_slabnum_to_slotsize(slabnum) * NUM_SLOTS;
	slabnum += 1;
    }
    sum
}

const SMALL_SLAB_AREA_SPACE: usize = sum_small_slab_sizes(NUM_SMALL_SLABS).next_multiple_of(CACHELINE_SIZE);
pub const SMALL_SLAB_AREAS_REGION_SPACE: usize = SMALL_SLAB_AREA_SPACE * NUM_SMALL_SLAB_AREAS;

// Start the large slab region aligned to MAX_ALIGNMENT.
const LARGE_SLAB_REGION_BASE_OFFSET: usize = (DATA_SLABS_BASE_OFFSET + SMALL_SLAB_AREAS_REGION_SPACE).next_multiple_of(MAX_ALIGNMENT);

//XXX add benchmarking of the lookup-table version of this:
/// The sum of the sizes of the large slabs.
const fn sum_large_slab_sizes(numslabs: usize) -> usize {
    assert!(numslabs <= NUM_LARGE_SLABS);
    let mut index = 0;
    let mut sum: usize = 0;
    while index < numslabs {
        let slotsize = large_slabnum_to_slotsize(index);
        // Padding to make the beginning of this slab start on a multiple of this slot size, or of MAX_ALIGNMENT.
        sum = sum.next_multiple_of(
            if slotsize < MAX_ALIGNMENT {
                slotsize
            } else {
                MAX_ALIGNMENT
            }
        );
        sum += slotsize * NUM_SLOTS;
	index += 1;
    }
    sum
}

pub const LARGE_SLAB_REGION_SPACE: usize = sum_large_slab_sizes(NUM_LARGE_SLABS);

pub const TOTAL_VIRTUAL_MEMORY: usize = LARGE_SLAB_REGION_BASE_OFFSET + LARGE_SLAB_REGION_SPACE;


use std::cmp::PartialEq;

#[derive(PartialEq)]
#[derive(Debug)]
enum SlotLocation {
    SmallSlot { areanum: usize, smallslabnum: usize, slotnum: usize },
    LargeSlot { largeslabnum: usize, slotnum: usize }
}

impl SlotLocation {
    fn slotsize(&self) -> usize {
        match self {
            SlotLocation::SmallSlot { smallslabnum, .. } => {
                SMALL_SLABNUM_TO_SLOTSIZE[*smallslabnum]
            }
            SlotLocation::LargeSlot { largeslabnum, .. } => {
                LARGE_SLABNUM_TO_SLOTSIZE[*largeslabnum]
            }
        }
    }

    fn offset(&self) -> usize {
        match self {
            SlotLocation::SmallSlot { areanum, smallslabnum, slotnum } => {
                offset_of_small_slot(*areanum, *smallslabnum, *slotnum)
            }
            SlotLocation::LargeSlot { largeslabnum, slotnum } => {
                offset_of_large_slot(*largeslabnum, *slotnum)
            }
        }
    }
    
    /// Returns Some(SlotLocation) if the ptr pointed to a slot, else None (meaning that the pointer must have been allocated with `sys_alloc()` instead.
    fn new_from_ptr(baseptr: *mut u8, ptr: *mut u8) -> Option<SlotLocation> {
        // If the pointer is before our base pointer or after the end of our allocated space, then it must have come from an oversized alloc where we fell back to `sys_alloc()`. (Assuming that the user code never passes anything other a pointer that it previous got from our `alloc()`, to our `dealloc().)
        
        // Now there is no well-specified way to compare two pointers if they aren't part of the same allocation, which this p and our baseptr might not be.
        // .addr() is our way of promising the Rust compiler that we won't round-trip these values back into pointers from usizes and use them, below. See https://doc.rust-lang.org/nightly/std/ptr/index.html#strict-provenance
        
        let p_as_usize = ptr.addr();
        let baseptr_as_usize = baseptr.addr();
        if p_as_usize < baseptr_as_usize {
            // xxx add unit test of this case
	    return None;
        }
        if p_as_usize >= baseptr_as_usize + TOTAL_VIRTUAL_MEMORY {
            // xxx add unit test of this case
	    return None;
        }
        
        // If it wasn't a pointer from a system allocation, then it must be a pointer into one of our slots.
        assert!(p_as_usize >= baseptr_as_usize + DATA_SLABS_BASE_OFFSET);

        // Okay now we know that it is pointer into our allocation, so it is safe to subtract baseptr from it.
        let ioffset = unsafe { ptr.offset_from(baseptr) };
        assert!(ioffset >= 0);
        let offset = ioffset as usize;
        assert!(offset < isize::MAX as usize);
        assert!(offset >= DATA_SLABS_BASE_OFFSET);

        if offset < LARGE_SLAB_REGION_BASE_OFFSET {
            // This points into the "small-slabs-areas-region".
            let withinregionoffset = offset - DATA_SLABS_BASE_OFFSET;
            let areanum = withinregionoffset / SMALL_SLAB_AREA_SPACE;
            let pastareas = areanum * SMALL_SLAB_AREA_SPACE;
            let withinareaoffset = withinregionoffset - pastareas;
            assert!(withinareaoffset < sum_small_slab_sizes(NUM_SMALL_SLABS));

            let mut smallslabnum = NUM_SMALL_SLABS-1;
            while withinareaoffset < sum_small_slab_sizes(smallslabnum) {
                smallslabnum -= 1;
            }

            // This ptr is within this slab.
            let withinslaboffset = withinareaoffset - sum_small_slab_sizes(smallslabnum);
            let slotsize = small_slabnum_to_slotsize(smallslabnum);
            assert!(withinslaboffset.is_multiple_of(slotsize)); // ptr must point to the beginning of a slot.
            assert!(if slotsize.is_power_of_two() { ptr.is_aligned_to(slotsize) } else { true });
            let slotnum = withinslaboffset / slotsize;
            assert!(if slotnum == 0 { ptr.is_aligned_to(CACHELINE_SIZE) } else { true });
            assert!(if slotsize.is_power_of_two() { ptr.is_aligned_to(slotsize) } else { true });

            Some(Self::SmallSlot { areanum, smallslabnum, slotnum })
        } else {
            // This points into the "large-slabs-region".
            assert!(LARGE_SLAB_REGION_BASE_OFFSET.is_multiple_of(CACHELINE_SIZE));
            assert!(LARGE_SLAB_REGION_BASE_OFFSET.is_multiple_of(MAX_ALIGNMENT));

            let withinregionoffset = offset - LARGE_SLAB_REGION_BASE_OFFSET;

            let mut largeslabnum = 0;
            while largeslabnum < NUM_LARGE_SLABS-1 && withinregionoffset >= within_region_offset_of_large_slot_slab(largeslabnum+1) {
                largeslabnum += 1;
            }
            assert!(largeslabnum < NUM_LARGE_SLABS);
            let slotsize = large_slabnum_to_slotsize(largeslabnum);
            assert!(if slotsize.is_power_of_two() { ptr.is_aligned_to(min(slotsize, MAX_ALIGNMENT)) } else { true });

            // This ptr is within this slab.
            // XXX replace without using offset_of_large_slot () ? Table from largeslabnum to offset!
            let withinslaboffset = withinregionoffset - within_region_offset_of_large_slot_slab(largeslabnum);
            assert!(withinslaboffset.is_multiple_of(slotsize)); // ptr must point to the beginning of a slot.
            let slotnum = withinslaboffset / slotsize;
            assert!(if slotnum == 0 { ptr.is_aligned_to(CACHELINE_SIZE) } else { true });

            Some(Self::LargeSlot { largeslabnum, slotnum })
        }
    }
}

fn offset_of_small_slot(areanum: usize, slabnum: usize, slotnum: usize) -> usize {
    assert!(areanum < NUM_SMALL_SLAB_AREAS);
    assert!(slabnum < NUM_SMALL_SLABS);
    assert!(slotnum < NUM_SLOTS);

    // This is in the small-slot slabs region.
    let mut offset = DATA_SLABS_BASE_OFFSET;

    // Count past the bytes of any earlier areas before this area:
    offset += areanum * SMALL_SLAB_AREA_SPACE;
                
    // Count past the bytes of any earlier slabs before this slab:
    offset += sum_small_slab_sizes(slabnum);

    let slotsize = small_slabnum_to_slotsize(slabnum);

    // Count past the bytes of any earlier slots in this slab:
    offset += slotnum * slotsize;

    offset
}

use std::cmp::min;

fn within_region_offset_of_large_slot_slab(largeslabnum: usize) -> usize {
//XXX replace with table
    assert!(largeslabnum < NUM_LARGE_SLABS, "{largeslabnum}");

    let mut offset = 0;

    // Count past the bytes of any earlier slabs before this slab:
    offset += sum_large_slab_sizes(largeslabnum);

    let slotsize = large_slabnum_to_slotsize(largeslabnum);

    // The beginning of each large slab is aligned with its slotsize, or MAX_ALIGNMENT.
    assert!(offset.is_multiple_of(min(slotsize, MAX_ALIGNMENT)));

    offset
}

fn offset_of_large_slot(largeslabnum: usize, slotnum: usize) -> usize {
//xxx replace part of this with table from largeslabnum to offset
    assert!(largeslabnum < NUM_LARGE_SLABS);
    assert!(slotnum < NUM_SLOTS);

    let slotsize = large_slabnum_to_slotsize(largeslabnum);

    let mut offset = LARGE_SLAB_REGION_BASE_OFFSET;

    // The beginning of this slab within the large slabs region:
    offset += within_region_offset_of_large_slot_slab(largeslabnum);

    // The beginning of each large slab is aligned with its slotsize, or MAX_ALIGNMENT.
    assert!(offset.is_multiple_of(min(slotsize, MAX_ALIGNMENT)));

    // Count past the bytes of any earlier slots in this slab:
    offset += slotnum * slotsize;

    // The beginning of each large slot is aligned with its slotsize, or MAX_ALIGNMENT.
    assert!(offset.is_multiple_of(min(slotsize, MAX_ALIGNMENT)));

    offset
}

fn offset_of_small_free_list_entry(areanum: usize, smallslabnum: usize, slotnum: usize) -> usize {
    if smallslabnum < NUM_SEPARATE_FREELISTS {
	// count past previous separate-free-list slots, area-first then slab then slot...
	let pastslots = areanum * NUM_SEPARATE_FREELISTS * NUM_SLOTS + smallslabnum * NUM_SLOTS + slotnum;
	// The separate free lists are laid out after the variables...
	SEPARATE_FREELISTS_BASE_OFFSET + pastslots * WORDSIZE
    } else {
        // Intrusive free list -- the location of the next-slot space is also the location of the data slot.
        offset_of_small_slot(areanum, smallslabnum, slotnum)
    }
}

use core::alloc::{GlobalAlloc, Layout};

use std::sync::atomic::{AtomicU32, AtomicPtr, AtomicBool, Ordering};
pub mod platformalloc;
use platformalloc::{sys_alloc, sys_dealloc, sys_realloc};
use std::ptr::{null_mut, copy_nonoverlapping};

pub struct Smalloc {
    initlock: AtomicBool,
    baseptr: AtomicPtr<u8>,
}

impl Default for Smalloc {
    fn default() -> Self {
	Self::new()
    }
}


//use atomic_dbg::eprintln;
//use thousands::Separable;

impl Smalloc {
    pub const fn new() -> Self {
	Self {
	    initlock: AtomicBool::new(false),
	    baseptr: AtomicPtr::<u8>::new(null_mut()),
	}
    }
    
    fn idempotent_init(&self) -> *mut u8 {
	let mut p: *mut u8;

	p = self.baseptr.load(Ordering::Acquire); // XXX ?? relaxed ???
	if !p.is_null() {
	    return p;
	}

        //eprintln!("TOTAL_VIRTUAL_MEMORY: {}", TOTAL_VIRTUAL_MEMORY.separate_with_commas());

	let layout = unsafe { Layout::from_size_align_unchecked(TOTAL_VIRTUAL_MEMORY, MAX_ALIGNMENT) };

	// acquire spin lock
        loop {
            if self.initlock.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {
                break;
            }
        }

	p = self.baseptr.load(Ordering::Acquire);
	if p.is_null() { // XXX come back and figure out if Relaxed is okay. :-)
	    p = sys_alloc(layout).unwrap();
	    assert!(!p.is_null());
            assert!(p.is_aligned_to(MAX_ALIGNMENT)); // This is just testing my understanding that mmap() always returns page-aligned pointers (and that page-alignment is always a multiple of 4096.)
	    self.baseptr.store(p, Ordering::Release); // XXX come back and figure out if Relaxed would be okay. :-)  Jack says never. :-)
	}
        self.initlock.store(false, Ordering::Release);

        // Release the spin lock
        
	p
    }

    fn get_baseptr(&self) -> *mut u8 {
	// It is not okay to call this in alloc()/idempotent_init()
	let p = self.baseptr.load(Ordering::Relaxed);
	assert!(!p.is_null());

	p
    }

    /// Pop the head of the free list and return it. Returns 0 if the
    /// free list is empty, or returns the one greater than the index
    /// of the former head of the free list. See "Thread-Safe State
    /// Changes" in README.md
    fn pop_small_flh(&self, areanum: usize, smallslabnum: usize) -> u32 {
	let baseptr = self.get_baseptr();
	
	let offset_of_flh = offset_of_small_flh(areanum, smallslabnum);

	let u8_ptr_to_flh = unsafe { baseptr.add(offset_of_flh) };
	assert!(u8_ptr_to_flh.is_aligned_to(WORDSIZE)); // need 4-byte alignment for atomic ops (on at least some/most platforms)
	let u32_ptr_to_flh = u8_ptr_to_flh.cast::<u32>();

	let flh = unsafe { AtomicU32::from_ptr(u32_ptr_to_flh) };
	loop {
	    let firstindexplus1: u32 = flh.load(Ordering::Relaxed);
	    assert!(firstindexplus1 <= NUM_SLOTS as u32);

	    if firstindexplus1 == 0 {
                // 0 is the sentinel value meaning no next entry, meaning the free list is empty
		return 0
	    };

	    let offset_of_next = offset_of_small_free_list_entry(areanum, smallslabnum, (firstindexplus1-1) as usize);
	    let u8_ptr_to_next = unsafe { baseptr.add(offset_of_next) }; // note this isn't necessarily aligned
	    let u32_ptr_to_var = u8_ptr_to_next.cast::<u32>();
	    let nextindexplus1: u32 = unsafe { u32_ptr_to_var.read_unaligned() };
	    assert!(nextindexplus1 <= NUM_SLOTS as u32);
            
	    if flh.compare_exchange_weak(firstindexplus1, nextindexplus1, Ordering::Acquire, Ordering::Relaxed).is_ok() {
		return firstindexplus1
	    }
	}
    }

    /// Pop the head of the free list and return it. Returns 0 if the
    /// free list is empty, or returns the one greater than the index
    /// of the former head of the free list. See "Thread-Safe State
    /// Changes" in README.md
    fn pop_large_flh(&self, largeslabnum: usize) -> u32 {
	let baseptr = self.get_baseptr();
	
	let offset_of_flh = offset_of_large_flh(largeslabnum);

	let u8_ptr_to_flh = unsafe { baseptr.add(offset_of_flh) };
	assert!(u8_ptr_to_flh.is_aligned_to(WORDSIZE)); // need 4-byte alignment for atomic ops (on at least some/most platforms)
	let u32_ptr_to_flh = u8_ptr_to_flh.cast::<u32>();

	let flh = unsafe { AtomicU32::from_ptr(u32_ptr_to_flh) };
	loop {
	    let firstindexplus1: u32 = flh.load(Ordering::Relaxed);
	    assert!(firstindexplus1 <= NUM_SLOTS as u32);

	    if firstindexplus1 == 0 {
                // 0 is the sentinel value meaning no next entry, meaning the free list is empty
		return 0
	    }

            // Intrusive free list -- free list entries are stored in data slots (when they are not in use for data).
	    let offset_of_next = offset_of_large_slot(largeslabnum, (firstindexplus1-1) as usize);
	    let u8_ptr_to_next = unsafe { baseptr.add(offset_of_next) }; // note this isn't necessarily aligned
	    let u32_ptr_to_var = u8_ptr_to_next.cast::<u32>();
	    let nextindexplus1: u32 = unsafe { u32_ptr_to_var.read_unaligned() };
	    assert!(nextindexplus1 <= NUM_SLOTS as u32);
            
	    if flh.compare_exchange_weak(firstindexplus1, nextindexplus1, Ordering::Acquire, Ordering::Relaxed).is_ok() {
		return firstindexplus1
	    }
	}
    }

    fn inner_push_flh(&self, offset_of_flh: usize, offset_of_new: usize, new_index: u32) {
	let baseptr = self.get_baseptr();

	let u8_ptr_to_flh = unsafe { baseptr.add(offset_of_flh) };
	assert!(u8_ptr_to_flh.is_aligned_to(WORDSIZE)); // need 4-byte alignment for atomic ops (on at least some/most platforms)
	let u32_ptr_to_flh = u8_ptr_to_flh.cast::<u32>();
	let flh = unsafe { AtomicU32::from_ptr(u32_ptr_to_flh) };

	let u8_ptr_to_new = unsafe { baseptr.add(offset_of_new) }; // note this isn't necessarily aligned
	let u32_ptr_to_new: *mut u32 = u8_ptr_to_new.cast::<u32>();

        loop {
	    let firstindexplus1: u32 = flh.load(Ordering::Relaxed);
	    assert!(firstindexplus1 <= NUM_SLOTS as u32);
	    unsafe { u32_ptr_to_new.write_unaligned(firstindexplus1) };
            if flh.compare_exchange_weak(firstindexplus1, new_index+1, Ordering::Acquire, Ordering::Relaxed).is_ok() {
                break
            }
	}
    }

    fn push_flh(&self, newsl: SlotLocation) {
        match newsl {
            SlotLocation::SmallSlot { areanum, smallslabnum, slotnum } => {
                assert!(slotnum < NUM_SLOTS);
                self.inner_push_flh(
                    offset_of_small_flh(areanum, smallslabnum),
                    offset_of_small_free_list_entry(areanum, smallslabnum, slotnum),
                    slotnum as u32
                )
            }
            SlotLocation::LargeSlot { largeslabnum, slotnum } => {
                assert!(slotnum < NUM_SLOTS);
                // Intrusive free list -- the free list entry is stored in the data slot.
                self.inner_push_flh(
                    offset_of_large_flh(largeslabnum),
                    offset_of_large_slot(largeslabnum, slotnum),
                    slotnum as u32
                )
            }
        }
    }

    fn get_small_eac(&self, areanum: usize, smallslabnum: usize) -> &AtomicU32 {
	assert!(areanum < NUM_SMALL_SLAB_AREAS);
	assert!(smallslabnum < NUM_SMALL_SLABS);

	let baseptr = self.get_baseptr();
        let offset_of_eac = offset_of_small_eac(areanum, smallslabnum);
        let u8_ptr_to_eac = unsafe { baseptr.add(offset_of_eac) };
	assert!(u8_ptr_to_eac.is_aligned_to(WORDSIZE)); // need 4-byte alignment for atomic ops (on at least some/most platforms)
	let u32_ptr_to_eac = u8_ptr_to_eac.cast::<u32>();
	unsafe { AtomicU32::from_ptr(u32_ptr_to_eac) }
    }

    fn get_large_eac(&self, largeslabnum: usize) -> &AtomicU32 {
	assert!(largeslabnum < NUM_LARGE_SLABS);

	let baseptr = self.get_baseptr();
        let offset_of_eac = offset_of_large_eac(largeslabnum);
        let u8_ptr_to_eac = unsafe { baseptr.add(offset_of_eac) };
	assert!(u8_ptr_to_eac.is_aligned_to(WORDSIZE)); // need 4-byte alignment for atomic ops (on at least some/most platforms)
	let u32_ptr_to_eac = u8_ptr_to_eac.cast::<u32>();
	unsafe { AtomicU32::from_ptr(u32_ptr_to_eac) }
    }

    /// Returns the index of the next never-before-allocated slot. Returns NUM_SLOTS in the case that all slots have been allocated.
    fn increment_eac(&self, eac: &AtomicU32) -> usize {
	let nexteac = eac.fetch_add(1, Ordering::Acquire); // reconsider whether this can be Relaxed (meaning it would be okay if some other memory access got reordered to happen before this fetch_add??
	if nexteac as usize <= NUM_SLOTS {
	    nexteac as usize
	} else {
            if nexteac as usize > NUM_SLOTS + 100 {
	        // If eac is maxed out -- at NUM_SLOTS -- another thread has incremented past NUM_SLOTS but not yet decremented it, then this could exceed NUM_SLOTS. However, if this has happened more than a few times simultaneously, such that eac is more than a small number higher than NUM_SLOTS, then something is very wrong and we should panic to prevent some kind of failure case or exploitation. If eac reached 2^32 then it would wrap, and we want to panic long before that.
                panic!("the Ever-Allocated-Counter exceeded NUM_SLOTS + 100");
            }

	    eac.fetch_sub(1, Ordering::Acquire); // reconsider whether this can be Relaxed (meaning it would be okay if some other memory access got reordered to happen before this fetch_add??

            NUM_SLOTS
	}
    }

    fn inner_small_alloc(&self, areanum: usize, smallslabnum: usize) -> Option<SlotLocation> {
	let flhplus1 = self.pop_small_flh(areanum, smallslabnum);
	if flhplus1 != 0 {
	    // xxx add unit test of this case
	    Some(SlotLocation::SmallSlot { areanum, smallslabnum, slotnum: (flhplus1-1) as usize })
	} else {
	    let eac: usize = self.increment_eac(self.get_small_eac(areanum, smallslabnum));
	    if eac < NUM_SLOTS {
		// xxx add unit test of this case
		Some(SlotLocation::SmallSlot { areanum, smallslabnum, slotnum: eac })
	    } else {
	        // xxx add unit test of this case
                // The slab is full!
		None
            }
        }
    }

    fn inner_large_alloc(&self, largeslabnum: usize) -> Option<SlotLocation> {
	let flhplus1 = self.pop_large_flh(largeslabnum);
	if flhplus1 != 0 {
	    // xxx add unit test of this case
	    Some(SlotLocation::LargeSlot { largeslabnum, slotnum: (flhplus1-1) as usize })
	} else {
	    let eac: usize = self.increment_eac(self.get_large_eac(largeslabnum));
	    if eac < NUM_SLOTS {
		// xxx add unit test of this case
		Some(SlotLocation::LargeSlot { largeslabnum, slotnum: eac })
	    } else {
		// xxx add unit test of this case
                // The slab is full!
		None
            }
        }
    }
    
    /// Returns the newly allocated SlotLocation. if it was able to allocate a slot, else returns None (in which case alloc/realloc needs to satisfy the request by falling back to sys_alloc())
    fn inner_alloc(&self, layout: Layout) -> Option<SlotLocation> {
        let size = layout.size();
        let alignment = layout.align();
        assert!(alignment > 0);
        assert!((alignment & (alignment - 1)) == 0, "alignment must be a power of two");
        assert!(alignment <= MAX_ALIGNMENT); // We don't guarantee larger alignments than 4096

        // Round up size to the nearest multiple of alignment in order to get a slot that is aligned on that size.
        let alignedsize: usize = ((size - 1) | (alignment - 1)) + 1;

        // XXX benchmark various ways to do this switch+loop...
        // This way of doing this branch+loop was informed by:
        // 1. Let's branch on small-slot vs large-slot just once and then have two (similar) code paths instead of branching on small-slot vs large-slot multiple times in one code path, and
        // 2. I profiled zebra, which showed that 32B was the most common slot size, and that < 32B was more common than > 32B, and that among > 32B slot sizes, 64B was the most common one...
        if alignedsize <= SIZE_OF_BIGGEST_SMALL_SLOT {
            let mut smallslabnum = NUM_SMALL_SLABS-1;
            while smallslabnum > 0 && small_slabnum_to_slotsize(smallslabnum-1) >= alignedsize {
                smallslabnum -= 1;
            }
            assert!(smallslabnum < NUM_SMALL_SLABS);
            assert!(small_slabnum_to_slotsize(smallslabnum) >= alignedsize);
            assert!(smallslabnum == 0|| small_slabnum_to_slotsize(smallslabnum-1) < alignedsize, "smallslabnum: {}, alignedsize: {}", smallslabnum, alignedsize);

            self.inner_small_alloc(get_thread_areanum() as usize, smallslabnum)
        } else if alignedsize <= SIZE_OF_LARGEST_SLOTS {
            let mut largeslabnum = 0;
            while large_slabnum_to_slotsize(largeslabnum) < alignedsize {
                largeslabnum += 1;
            }
            assert!(largeslabnum < NUM_LARGE_SLABS);

            self.inner_large_alloc(largeslabnum)
        } else {
            // This is too large for even the largest smalloc slots, so fall back to the system allocator.
            None
        }
    }
}

use std::cell::Cell;

static GLOBAL_THREAD_AREANUM: AtomicU32 = AtomicU32::new(0);
thread_local! {
    static THREAD_AREANUM: Cell<Option<u32>> = const { Cell::new(None) };
}

/// Get this thread's areanum, or initialize it to the first unused
/// areanum if this is the first time `get_thread_areanum()` has been called.
fn get_thread_areanum() -> u32 {
    THREAD_AREANUM.with(|cell| {
        cell.get().map_or_else(
            || {
                let new_value = GLOBAL_THREAD_AREANUM.fetch_add(1, Ordering::Relaxed);
                THREAD_AREANUM.with(|cell| cell.set(Some(new_value)));
                new_value
            },
            |value| value,
        )
    })
}

// xxx can i get the Rust typechecker to tell me if I'm accidentally adding a slot number to an offset ithout multiplying it by a slot size first?
//XXX learn about Constant Parameters and consider using them in here


unsafe impl GlobalAlloc for Smalloc {
    /// I require `layout`'s `align` to be <= MAX_ALIGNMENT.
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
	let baseptr = self.idempotent_init();

        let size = layout.size();
        assert!(size > 0);
        let alignment = layout.align();
        assert!(alignment > 0);
        assert!((alignment & (alignment - 1)) == 0); // alignment must be a power of two
        assert!(alignment <= MAX_ALIGNMENT); // We don't guarantee larger alignments than 4096

	// Allocate a slot
	match self.inner_alloc(layout) {
	    Some(sl) => {
                // xxx consider unwrapping this in order to avoid redundantly branching ?? 
                let offset = sl.offset();
		let p = unsafe { baseptr.add(offset) };
                assert!(if sl.slotsize().is_power_of_two() {
                    p.is_aligned_to(min(sl.slotsize(), MAX_ALIGNMENT))
                } else {
                    true
                });

                p
	    }
	    None => {
		// Couldn't allocate a slot -- fall back to `sys_alloc()`.
		sys_alloc(layout).unwrap()
	    }
	}
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
	match SlotLocation::new_from_ptr(self.get_baseptr(), ptr) {
	    Some(sl) => {
                self.push_flh(sl)
	    }
	    None => {
		// No slot -- this allocation must have come from falling back to `sys_alloc()`.
		sys_dealloc(ptr, layout);
	    }
	}
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, newsize: usize) -> *mut u8 {
        let oldsize = layout.size();
        assert!(oldsize > 0);
        let oldalignment = layout.align();
        assert!(oldalignment > 0);
        assert!((oldalignment & (oldalignment - 1)) == 0, "alignment must be a power of two");
        assert!(newsize > 0);
        assert!(oldalignment <= MAX_ALIGNMENT); // We don't guarantee larger alignments than 4096

	let baseptr = self.get_baseptr();

        match SlotLocation::new_from_ptr(baseptr, ptr) {
            Some(cursl) => {
                if newsize <= cursl.slotsize() {
                    // If the new size fits into the current slot, just return the current pointer and we're done.
                    ptr
                } else {
                    // Round up size to the nearest multiple of alignment in order to get a slot that is aligned on that size.
                    let alignednewsize: usize = ((newsize - 1) | (oldalignment - 1)) + 1;

                    // The "growers" rule: if the new size would fit into one 64-byte cache line, use a 64-byte slot...
                    let largeslabnum = if alignednewsize <= CACHELINE_SIZE {
                        assert_eq!(large_slabnum_to_slotsize(0), CACHELINE_SIZE); // The first (0-indexed) slab in the large slots region has slots just big enough to hold one 64-byte cacheline.
                        0
                    } else {
                        // ... else use the LARGEST slots.
                        NUM_LARGE_SLABS - 1
                    };

	            // Allocate a new slot...
                    let optnewsl = self.inner_large_alloc(largeslabnum);
                    let newptr: *mut u8 = match optnewsl {
                        Some(newsl) => {
                            let offset = newsl.offset();
                            let slotsize = newsl.slotsize();
		            let p = unsafe { baseptr.add(offset) };
                            assert!(if slotsize.is_power_of_two()  {
                                p.is_aligned_to(min(newsl.slotsize(), MAX_ALIGNMENT))
                            } else {
                                true
                            });
                            p
                        }
                        None => {
                            // Slab was full. Fallback to system allocator.
	                    let layout = unsafe { Layout::from_size_align_unchecked(newsize, oldalignment) };
                            sys_alloc(layout).unwrap()
                        }
                    };
		    assert!(newptr.is_aligned_to(oldalignment));
                    
	            // Copy the contents from the old ptr.
	            unsafe {
	                copy_nonoverlapping(ptr, newptr, oldsize);
	            }

	            // Free the old slot
	            self.push_flh(cursl);

	            newptr
                }
            }
            None => {
	        // This pointer must have been originally allocated by falling back to sys_alloc(), so we handle it now by falling back to sys_realloc().
	        sys_realloc(ptr, layout, newsize)
            }
        }
    }
}

// I read in the "The Linux Programming Interface" book that glibc's malloc's default size to fall back to system allocation (mmap) -- MMAP_THRESHOLD -- is 128 KiB. But according to https://sourceware.org/glibc/wiki/MallocInternals the threshold is dynamic unless overridden.

#[cfg(test)]
mod tests {
    use super::*;

    use thousands::Separable;

    #[test]
    fn test_offset_of_vars() {
        assert_eq!(offset_of_small_eac(0, 0), 0);
        assert_eq!(offset_of_small_flh(0, 0), 4);
        assert_eq!(offset_of_small_eac(0, 1), 8);
        assert_eq!(offset_of_small_flh(0, 1), 12);

        // There are 11 slabs in an small-slab-area, 2 variables for each slab, and 4 bytes for each variable, so the first variable in the second slab (slab index 1) should be at offset 88.
        assert_eq!(offset_of_small_eac(1, 0), 88);
        assert_eq!(offset_of_small_flh(1, 0), 92);
        assert_eq!(offset_of_small_eac(1, 1), 96);
        assert_eq!(offset_of_small_flh(1, 1), 100);

        // The large-slab vars start after all the small-slab vars
        let all_small_slab_vars = 11 * 2 * 4 * NUM_SMALL_SLAB_AREAS;
        assert_eq!(offset_of_large_eac(0), all_small_slab_vars);
        assert_eq!(offset_of_large_flh(0), all_small_slab_vars+4);
        assert_eq!(offset_of_large_eac(1), all_small_slab_vars+8);
        assert_eq!(offset_of_large_flh(1), all_small_slab_vars+12);

        // There are 7 large slabs, 2 variables for each slab, and 4 bytes for each variable.
        assert_eq!(offset_of_large_eac(0), all_small_slab_vars);
        assert_eq!(offset_of_large_flh(0), all_small_slab_vars+4);
        assert_eq!(offset_of_large_eac(1), all_small_slab_vars+8);
        assert_eq!(offset_of_large_flh(1), all_small_slab_vars+12);
    }

    fn help_test_inner_alloc(size: usize, alignment: usize) -> SlotLocation {
	let layout = Layout::from_size_align(size, alignment).unwrap();
        SM.inner_alloc(layout).unwrap()
    }

    use lazy_static::lazy_static;

    lazy_static! {
        static ref SM: Smalloc = Smalloc::new();
    }


    #[test]
    fn test_a_few_allocs_and_a_dealloc_small() {
        SM.idempotent_init();

        for smallslabnum in 0..NUM_SMALL_SLABS {
            help_test_inner_alloc_small(smallslabnum);
        }
    }

    #[test]
    fn test_a_few_allocs_and_a_dealloc_large() {
        SM.idempotent_init();

        for largeslabnum in 0..NUM_LARGE_SLABS {
            help_test_inner_alloc_large(largeslabnum);
        }
    }

    fn help_test_inner_alloc_large(largeslabnum: usize) {
        // Generate requested sizes that fit into this slab:
        let slotsize = large_slabnum_to_slotsize(largeslabnum);
        let smallest = if largeslabnum == 0 {
            small_slabnum_to_slotsize(NUM_SMALL_SLABS-1)+1
        } else {
            large_slabnum_to_slotsize(largeslabnum-1)+1
        };
        let largest = slotsize;
        for reqsize in [smallest, smallest+1, smallest+2, largest-3, largest-1, largest] {
            // Generate alignments
            let mut reqalign = 1;
            loop {
                // Test this size/align combo
                help_test_inner_alloc_large_size_align(largeslabnum, reqsize, reqalign);
                reqalign *= 2;
                let alignedsize: usize = ((reqsize - 1) | (reqalign - 1)) + 1;
                if alignedsize > slotsize || alignedsize > MAX_ALIGNMENT { break };
            }
        }
    }
    
    fn help_test_inner_alloc_small(smallslabnum: usize) {
        // Generate requested sizes that fit into this slab:
        let slotsize = small_slabnum_to_slotsize(smallslabnum);
        let smallest = if smallslabnum == 0 {
            1
        } else {
            small_slabnum_to_slotsize(smallslabnum-1)+1
        };
        let largest = slotsize;
        for reqsize in smallest..=largest {
            // Generate alignments
            let mut reqalign = 1;
            loop {
                // Test this size/align combo
                help_test_inner_alloc_small_size_align(smallslabnum, reqsize, reqalign);
                reqalign *= 2;
                let alignedsize: usize = ((reqsize - 1) | (reqalign - 1)) + 1;
                if alignedsize > slotsize { break };
            }
        }
    }

    fn help_test_inner_alloc_large_size_align(largeslabnum: usize, reqsize: usize, reqalign: usize) {
        let origea: u32 = SM.get_large_eac(largeslabnum).load(Ordering::Relaxed);

        let sl1 = help_test_inner_alloc(reqsize, reqalign);
        let SlotLocation::LargeSlot { largeslabnum: _, slotnum: _ } = sl1 else {
            panic!("should have returned a large slot");
        };
        assert_eq!(SM.get_large_eac(largeslabnum).load(Ordering::Relaxed), origea+1);

        let sl2 = help_test_inner_alloc(reqsize, reqalign);
        let SlotLocation::LargeSlot { largeslabnum: _, slotnum } = sl2 else {
            panic!("should have returned a large slot");
        };
        assert_eq!(SM.get_large_eac(largeslabnum).load(Ordering::Relaxed), origea+2);
        let sl2s_slot = slotnum;

        let sl3 = help_test_inner_alloc(reqsize, reqalign);
        let SlotLocation::LargeSlot { largeslabnum: _, slotnum: _ } = sl3 else {
            panic!("should have returned a large slot");
        };
        assert_eq!(SM.get_large_eac(largeslabnum).load(Ordering::Relaxed), origea+3);

        // Now free the middle one.
        SM.push_flh(sl2);

        // And allocate another one. The ever-allocated-count should not go up because it re-uses the freed slot for the subsequent allocation.
        let sl4 = help_test_inner_alloc(reqsize, reqalign);
        let SlotLocation::LargeSlot { largeslabnum: _, slotnum } = sl4 else {
            panic!("should have returned a large slot");
        };
        assert_eq!(slotnum, sl2s_slot);
        assert_eq!(SM.get_large_eac(largeslabnum).load(Ordering::Relaxed), origea+3);

    }

    fn help_test_inner_alloc_small_size_align(smallslabnum: usize, reqsize: usize, reqalign: usize) {
        let origea: u32 = SM.get_small_eac(0, smallslabnum).load(Ordering::Relaxed);

        let sl1 = help_test_inner_alloc(reqsize, reqalign);
        let SlotLocation::SmallSlot { areanum: _, smallslabnum: _, slotnum: _ } = sl1 else {
            panic!("should have returned a small slot");
        };
        assert_eq!(SM.get_small_eac(0, smallslabnum).load(Ordering::Relaxed), origea+1);

        let sl2 = help_test_inner_alloc(reqsize, reqalign);
        let SlotLocation::SmallSlot { areanum: _, smallslabnum: _, slotnum } = sl2 else {
            panic!("should have returned a small slot");
        };
        assert_eq!(SM.get_small_eac(0, smallslabnum).load(Ordering::Relaxed), origea+2);
        let sl2s_slot = slotnum;

        let sl3 = help_test_inner_alloc(reqsize, reqalign);
        let SlotLocation::SmallSlot { areanum: _, smallslabnum: _, slotnum: _ } = sl3 else {
            panic!("should have returned a small slot");
        };
        assert_eq!(SM.get_small_eac(0, smallslabnum).load(Ordering::Relaxed), origea+3);

        // Now free the middle one.
        SM.push_flh(sl2);

        // And allocate another one. The ever-allocated-count should not go up because it re-uses the freed slot for the subsequent allocation.
        let sl4 = help_test_inner_alloc(reqsize, reqalign);
        let SlotLocation::SmallSlot { areanum: _, smallslabnum: _, slotnum } = sl4 else {
            panic!("should have returned a small slot");
        };
        assert_eq!(slotnum, sl2s_slot);
        assert_eq!(SM.get_small_eac(0, smallslabnum).load(Ordering::Relaxed), origea+3);

    }

//XXX    #[test]
    fn _test_alloc_1_byte_then_dealloc() {
        let sm = Smalloc::new();
	let layout = Layout::from_size_align(1, 1).unwrap();
        let p = unsafe { sm.alloc(layout) };
        unsafe { sm.dealloc(p, layout) };
    }

//XXX    #[test]
    fn _test_roundtrip_slot_to_ptr_to_slot() {
        let baseptr_for_testing: *mut u8 = SIZE_OF_LARGEST_SLOTS as *mut u8;

        // First the small-slabs region:
	for areanum in [1, 2, 30, 31, 32, 33, NUM_SMALL_SLAB_AREAS-3, NUM_SMALL_SLAB_AREAS-2, NUM_SMALL_SLAB_AREAS-1] {
	    for smallslabnum in 0..NUM_SMALL_SLABS {
		for slotnum in [0, 1, 2, 253, 254, 255, 256, 257, 1022, 1023, 1024, 2usize.pow(16)-1, 2usize.pow(16), 2usize.pow(16)+1, NUM_SLOTS-2, NUM_SLOTS-1] {
		    let sl1 = SlotLocation::SmallSlot { areanum, smallslabnum, slotnum };
		    let offset = sl1.offset();
		    assert!(offset >= DATA_SLABS_BASE_OFFSET);
		    assert!(offset < DATA_SLABS_BASE_OFFSET + SMALL_SLAB_AREAS_REGION_SPACE,
                            "sl1: {:?}, {} {} {} {}",
                            sl1,
                            offset.separate_with_commas(),
                            DATA_SLABS_BASE_OFFSET.separate_with_commas(),
                            SMALL_SLAB_AREAS_REGION_SPACE.separate_with_commas(),
                            (DATA_SLABS_BASE_OFFSET + SMALL_SLAB_AREAS_REGION_SPACE).separate_with_commas()
                    );
		    assert!(offset < LARGE_SLAB_REGION_BASE_OFFSET);
		    let p = unsafe { baseptr_for_testing.add(offset) };
		    let sl2 = SlotLocation::new_from_ptr(baseptr_for_testing, p).unwrap();
		    assert_eq!(sl1, sl2);
		}
	    }
	}

        // Then the large-slabs region:
	for largeslabnum in 0..NUM_LARGE_SLABS {
	    for slotnum in [0, 1, 2, 253, 254, 255, 256, 257, 1022, 1023, 1024, 2usize.pow(16)-1, 2usize.pow(16), 2usize.pow(16)+1, NUM_SLOTS-2, NUM_SLOTS-1] {
		let sl1 = SlotLocation::LargeSlot { largeslabnum, slotnum };
		let offset = sl1.offset();
		assert!(offset >= DATA_SLABS_BASE_OFFSET);
		let p = unsafe { baseptr_for_testing.add(offset) };
		let sl2 = SlotLocation::new_from_ptr(baseptr_for_testing, p).unwrap();
		assert_eq!(sl1, sl2);
	    }
	}
    }
}

