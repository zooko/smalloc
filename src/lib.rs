#![feature(pointer_is_aligned_to)]
#![feature(test)]

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
pub const HUGE_SLABNUM: usize = 6;

pub const SIZE_OF_BIGGEST_SMALL_SLOT: usize = 32;
pub const SIZE_OF_HUGE_SLOTS: usize = 4194304; // 4 * 2^20
pub const SMALL_SLABNUM_TO_SLOTSIZE: [usize; NUM_SMALL_SLABS] =
    [1, 2, 3, 4, 5, 6, 8, 9, 10, 16, SIZE_OF_BIGGEST_SMALL_SLOT];
pub const LARGE_SLABNUM_TO_SLOTSIZE: [usize; NUM_LARGE_SLABS] =
    [64, 128, 256, 512, 1024, 2048, SIZE_OF_HUGE_SLOTS];

pub const fn small_slabnum_to_slotsize(smallslabnum: usize) -> usize {
    assert!(smallslabnum < NUM_SMALL_SLABS);
    SMALL_SLABNUM_TO_SLOTSIZE[smallslabnum]
}

pub const fn large_slabnum_to_slotsize(largeslabnum: usize) -> usize {
    assert!(largeslabnum < NUM_LARGE_SLABS);
    LARGE_SLABNUM_TO_SLOTSIZE[largeslabnum]
}

// For slabs other than the largest slab:
pub const NUM_SLOTS_O: usize = 440_000_000;

// For the largest slab:
pub const NUM_SLOTS_HUGE: usize = 20_000_000;

pub const fn num_large_slots(largeslabnum: usize) -> usize {
    if largeslabnum == HUGE_SLABNUM {
        NUM_SLOTS_HUGE
    } else {
        NUM_SLOTS_O
    }
}

// The per-slab flhs and eacs have this size in bytes.
const DOUBLEWORDSIZE: usize = 8;

// The free list entries have this size in bytes.
const SINGLEWORDSIZE: usize = 4;

// One eac plus one flh
const VARSSIZE: usize = DOUBLEWORDSIZE * 2;

// There are 64 areas each with a full complements of small slabs.
// (Large slabs live in a separate region that is not one of those 64 areas.)
pub const NUM_SMALL_SLAB_AREAS: usize = 64;

// Aligning this to DOUBLEWORDSIZE for the sake of the flh's.
const LARGE_SLABS_VARS_BASE_OFFSET: usize =
    (NUM_SMALL_SLAB_AREAS * NUM_SMALL_SLABS * VARSSIZE).next_multiple_of(DOUBLEWORDSIZE);

pub const VARIABLES_SPACE: usize = LARGE_SLABS_VARS_BASE_OFFSET + NUM_LARGE_SLABS * VARSSIZE;

fn offset_of_small_flh(areanum: usize, smallslabnum: usize) -> usize {
    (areanum * NUM_SMALL_SLABS + smallslabnum) * VARSSIZE
}

fn offset_of_large_flh(largeslabnum: usize) -> usize {
    LARGE_SLABS_VARS_BASE_OFFSET + largeslabnum * VARSSIZE
}

fn offset_of_small_eac(areanum: usize, smallslabnum: usize) -> usize {
    (areanum * NUM_SMALL_SLABS + smallslabnum) * VARSSIZE + DOUBLEWORDSIZE
}

fn offset_of_large_eac(largeslabnum: usize) -> usize {
    LARGE_SLABS_VARS_BASE_OFFSET + largeslabnum * VARSSIZE + DOUBLEWORDSIZE
}

const CACHELINE_SIZE: usize = 64;

// Align the beginning of the separate free lists region to CACHELINE_SIZE.
pub const SEPARATE_FREELISTS_BASE_OFFSET: usize = VARIABLES_SPACE.next_multiple_of(CACHELINE_SIZE);

// The calls to next_multiple_of() on a space are to start the *next* thing on a cacheline boundary.
const SEPARATE_FREELIST_SPACE: usize = (NUM_SLOTS_O * SINGLEWORDSIZE).next_multiple_of(CACHELINE_SIZE); // Size of each of the separate free lists
const NUM_SEPARATE_FREELISTS: usize = 6; // Number of separate free lists for slabs whose slots are too small to hold a 4-byte-aligned 4-byte word (slab numbers 0, 1, 2, 3, 4, and 5)

pub const SEPARATE_FREELISTS_SPACE_REGION: usize =
    NUM_SEPARATE_FREELISTS * SEPARATE_FREELIST_SPACE * NUM_SMALL_SLAB_AREAS;

// Align the beginning of the data slabs to MAX_ALIGNMENT. This is just to fit the maximum (4096) of smallest slots (1 byte) into a (4096-byte) memory page.
pub const DATA_SLABS_BASE_OFFSET: usize =
    (SEPARATE_FREELISTS_BASE_OFFSET + SEPARATE_FREELISTS_SPACE_REGION)
    .next_multiple_of(MAX_ALIGNMENT);

const fn gen_lut_sum_small_slab_sizes() -> [usize; NUM_SMALL_SLABS + 1] {
    let mut lut: [usize; NUM_SMALL_SLABS + 1] = [0; NUM_SMALL_SLABS + 1];

    let mut slabnum = 0;
    let mut sum: usize = 0;
    while slabnum < NUM_SMALL_SLABS {
        // Make the beginning of this slab start on a cache line boundary.
        sum = sum.next_multiple_of(CACHELINE_SIZE);
        sum += small_slabnum_to_slotsize(slabnum) * NUM_SLOTS_O;
        slabnum += 1;
        lut[slabnum] = sum;
    }
    lut
}

const SUM_SMALL_SLAB_SIZES: [usize; NUM_SMALL_SLABS + 1] = gen_lut_sum_small_slab_sizes();

/// The sum of the sizes of the small slabs.
pub const fn sum_small_slab_sizes(numslabs: usize) -> usize {
    assert!(numslabs <= NUM_SMALL_SLABS);
    SUM_SMALL_SLAB_SIZES[numslabs]
}

const SMALL_SLAB_AREA_SPACE: usize =
    sum_small_slab_sizes(NUM_SMALL_SLABS).next_multiple_of(CACHELINE_SIZE);
pub const SMALL_SLAB_AREAS_REGION_SPACE: usize = SMALL_SLAB_AREA_SPACE * NUM_SMALL_SLAB_AREAS;

// Start the large slab region aligned to MAX_ALIGNMENT.
const LARGE_SLAB_REGION_BASE_OFFSET: usize =
    (DATA_SLABS_BASE_OFFSET + SMALL_SLAB_AREAS_REGION_SPACE).next_multiple_of(MAX_ALIGNMENT);

const fn gen_lut_sum_large_slab_sizes() -> [usize; NUM_LARGE_SLABS + 1] {
    let mut lut: [usize; NUM_LARGE_SLABS + 1] = [0; NUM_LARGE_SLABS + 1];

    let mut index = 0;
    let mut sum: usize = 0;
    while index < NUM_LARGE_SLABS {
        let slotsize = large_slabnum_to_slotsize(index);
        // Padding to make the beginning of this slab start on a multiple of this slot size, or of MAX_ALIGNMENT.
        sum = sum.next_multiple_of(if slotsize < MAX_ALIGNMENT {
            slotsize
        } else {
            MAX_ALIGNMENT
        });
        sum += slotsize * num_large_slots(index);
        index += 1;
        lut[index] = sum;
    }
    lut
}

const SUM_LARGE_SLAB_SIZES: [usize; NUM_LARGE_SLABS + 1] = gen_lut_sum_large_slab_sizes();

/// The sum of the sizes of the large slabs.
pub const fn sum_large_slab_sizes(numslabs: usize) -> usize {
    assert!(numslabs <= NUM_LARGE_SLABS);
    SUM_LARGE_SLAB_SIZES[numslabs]
}

pub const LARGE_SLAB_REGION_SPACE: usize = sum_large_slab_sizes(NUM_LARGE_SLABS);

pub const TOTAL_VIRTUAL_MEMORY: usize = LARGE_SLAB_REGION_BASE_OFFSET + LARGE_SLAB_REGION_SPACE;

use std::cmp::PartialEq;

// XXX Look into this new unstable Rust trait NonZero in std::num :-)

#[derive(PartialEq, Debug)]
enum SlotLocation {
    SmallSlot {
        areanum: usize,
        smallslabnum: usize,
        slotnum: usize,
    },
    LargeSlot {
        largeslabnum: usize,
        slotnum: usize,
    },
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
            SlotLocation::SmallSlot {
                areanum,
                smallslabnum,
                slotnum,
            } => offset_of_small_slot(*areanum, *smallslabnum, *slotnum),
            SlotLocation::LargeSlot {
                largeslabnum,
                slotnum,
            } => offset_of_large_slot(*largeslabnum, *slotnum),
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

            let mut smallslabnum = NUM_SMALL_SLABS - 1;
            while withinareaoffset < sum_small_slab_sizes(smallslabnum) {
                smallslabnum -= 1;
            }

            // This ptr is within this slab.
            let withinslaboffset = withinareaoffset - sum_small_slab_sizes(smallslabnum);
            let slotsize = small_slabnum_to_slotsize(smallslabnum);
            assert!(withinslaboffset.is_multiple_of(slotsize)); // ptr must point to the beginning of a slot.
            assert!(if slotsize.is_power_of_two() {
                ptr.is_aligned_to(slotsize)
            } else {
                true
            });
            let slotnum = withinslaboffset / slotsize;
            assert!(if slotnum == 0 {
                ptr.is_aligned_to(CACHELINE_SIZE)
            } else {
                true
            });
            assert!(if slotsize.is_power_of_two() {
                ptr.is_aligned_to(slotsize)
            } else {
                true
            });

            Some(Self::SmallSlot {
                areanum,
                smallslabnum,
                slotnum,
            })
        } else {
            // This points into the "large-slabs-region".
            assert!(LARGE_SLAB_REGION_BASE_OFFSET.is_multiple_of(CACHELINE_SIZE));
            assert!(LARGE_SLAB_REGION_BASE_OFFSET.is_multiple_of(MAX_ALIGNMENT));

            let withinregionoffset = offset - LARGE_SLAB_REGION_BASE_OFFSET;

            let mut largeslabnum = 0;
            while largeslabnum < NUM_LARGE_SLABS - 1
                && withinregionoffset >= within_region_offset_of_large_slot_slab(largeslabnum + 1)
            {
                largeslabnum += 1;
            }
            assert!(largeslabnum < NUM_LARGE_SLABS);
            let slotsize = large_slabnum_to_slotsize(largeslabnum);
            assert!(if slotsize.is_power_of_two() {
                ptr.is_aligned_to(min(slotsize, MAX_ALIGNMENT))
            } else {
                true
            });

            // This ptr is within this slab.
            // XXX replace without using offset_of_large_slot () ? Table from largeslabnum to offset!
            let withinslaboffset =
                withinregionoffset - within_region_offset_of_large_slot_slab(largeslabnum);
            assert!(withinslaboffset.is_multiple_of(slotsize)); // ptr must point to the beginning of a slot.
            let slotnum = withinslaboffset / slotsize;
            assert!(if slotnum == 0 {
                ptr.is_aligned_to(CACHELINE_SIZE)
            } else {
                true
            });

            Some(Self::LargeSlot {
                largeslabnum,
                slotnum,
            })
        }
    }
}

fn offset_of_small_slot(areanum: usize, slabnum: usize, slotnum: usize) -> usize {
    assert!(areanum < NUM_SMALL_SLAB_AREAS);
    assert!(slabnum < NUM_SMALL_SLABS);
    assert!(slotnum < NUM_SLOTS_O);

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
    // assert!(largeslabnum < NUM_LARGE_SLABS, "largeslabnum: {}, slotnum: {}", largeslabnum, slotnum); // alloc
    assert!(largeslabnum < NUM_LARGE_SLABS); // no alloc
    // assert!(slotnum < num_large_slots(largeslabnum), "slotnum: {}", slotnum);
    assert!(slotnum < num_large_slots(largeslabnum)); // noalloc

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
        let pastslots =
            areanum * NUM_SEPARATE_FREELISTS * NUM_SLOTS_O + smallslabnum * NUM_SLOTS_O + slotnum;
        // The separate free lists are laid out after the variables...
        SEPARATE_FREELISTS_BASE_OFFSET + pastslots * SINGLEWORDSIZE
    } else {
        // Intrusive free list -- the location of the next-slot space is the first 4-byte-aligned location in the data slot.
        offset_of_small_slot(areanum, smallslabnum, slotnum).next_multiple_of(SINGLEWORDSIZE)
    }
}

use core::alloc::{GlobalAlloc, Layout};

use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU64, Ordering};
pub mod platformalloc;
use platformalloc::{sys_alloc, sys_dealloc, sys_realloc};
use std::ptr::{copy_nonoverlapping, null_mut};

pub struct Smalloc {
    initlock: AtomicBool,
    baseptr: AtomicPtr<u8>
}

impl Default for Smalloc {
    fn default() -> Self {
        Self::new()
    }
}

// use thousands::Separable;
// use std::time::Instant;
// use lazy_static::lazy_static;

// lazy_static! {
//     static ref START_TIME: Instant = Instant::now();
// }

// macro_rules! debugln {
//     ($($arg:tt)*) => {{
//         let mut frmt = String::new();
//         let tim_str = format!("{:>12} ", START_TIME.elapsed().as_nanos().separate_with_commas());
//         frmt.push_str(&tim_str);
//         let pid_str = format!("thread: {:>3}, ", get_thread_areanum());
//         frmt.push_str(&pid_str);
//         frmt.push_str(&format!($($arg)*));
//         atomic_dbg::eprintln!("{}", frmt);
//     }};
// }

impl Smalloc {
    pub const fn new() -> Self {
        Self {
            initlock: AtomicBool::new(false),
            baseptr: AtomicPtr::<u8>::new(null_mut())
        }
    }

    fn idempotent_init(&self) -> *mut u8 {
        let mut p: *mut u8;

        p = self.baseptr.load(Ordering::Acquire); // YYY Acquire
        if !p.is_null() {
            return p;
        }

        //debugln!("TOTAL_VIRTUAL_MEMORY: {}", TOTAL_VIRTUAL_MEMORY);

        let layout =
            unsafe { Layout::from_size_align_unchecked(TOTAL_VIRTUAL_MEMORY, MAX_ALIGNMENT) };

        // acquire spin lock
        loop {
            if self.initlock.compare_exchange_weak(
                false,
                true,
                Ordering::AcqRel, // YYY AcqRel
                Ordering::Acquire // YYY Acquire
            ).is_ok() {
                break;
            }
        }

        p = self.baseptr.load(Ordering::Acquire); // YYY Acquire
        if p.is_null() {
            p = sys_alloc(layout).unwrap();
            assert!(!p.is_null());
            assert!(p.is_aligned_to(MAX_ALIGNMENT));
            self.baseptr.store(p, Ordering::Release); // YYY Release
        }

        // Release the spin lock
        self.initlock.store(false, Ordering::Release); // YYY Release


        p
    }

    fn get_baseptr(&self) -> *mut u8 {
        let p = self.baseptr.load(Ordering::Acquire); // YYY ??? Acquire ???
        assert!(!p.is_null());

        p
    }

    // /// For testing/debugging.
    // fn sl_to_ptr(&self, sl: &SlotLocation) -> *mut u8 {
    //     unsafe { self.get_baseptr().add(sl.offset()) }
    // }

    // /// For testing/debugging.
    // fn lssp1_p(&self, largeslabnum: usize, slotnump1u32: u32) -> *mut u8 {
    //     if slotnump1u32 > 0 {
    //         let slotnum = (slotnump1u32-1) as usize;
    //         unsafe { self.get_baseptr().add(SlotLocation::LargeSlot{ largeslabnum, slotnum }.offset()) }
    //     } else {
    //         null_mut()
    //     }
    // }

    /// Pop the head of the free list and return it. Returns 0 if the
    /// free list is empty, or returns the one greater than the index
    /// of the former head of the free list. See "Thread-Safe State
    /// Changes" in README.md
    fn pop_small_flh(&self, areanum: usize, smallslabnum: usize) -> u32 {
        let baseptr = self.get_baseptr();

        let offset_of_flh = offset_of_small_flh(areanum, smallslabnum);

        let u8_ptr_to_flh = unsafe { baseptr.add(offset_of_flh) };
        assert!(u8_ptr_to_flh.is_aligned_to(DOUBLEWORDSIZE)); // need 8-byte alignment for atomic ops (on at least some/most platforms)
        let u64_ptr_to_flh = u8_ptr_to_flh.cast::<u64>();

        let flh = unsafe { AtomicU64::from_ptr(u64_ptr_to_flh) };
        loop {
            let flhdword: u64 = flh.load(Ordering::Acquire); // YYY Acquire
            let firstindexplus1: u32 = (flhdword & u32::MAX as u64) as u32;
            // assert!(firstindexplus1 <= NUM_SLOTS_O as u32, "firstindexplus1: {}", firstindexplus1); // alloc
            assert!(firstindexplus1 <= NUM_SLOTS_O as u32); // noalloc
            // debug_assert!(firstindexplus1 as u64 <= self.get_small_eac(areanum, smallslabnum).load(Ordering::SeqCst), "areanum: {}, smallslabnum: {}, firstindexplus1: {}, eac: {}", areanum, smallslabnum, firstindexplus1, self.get_small_eac(areanum, smallslabnum).load(Ordering::SeqCst)); // alloc
            debug_assert!(firstindexplus1 as u64 <= self.get_small_eac(areanum, smallslabnum).load(Ordering::SeqCst)); // noalloc

            let counter: u32 = (flhdword >> 32) as u32;
            //debugln!("starting to pop / areanum: {}, smallslabnum: {}, firstindexplus1: {}/---", areanum, smallslabnum, firstindexplus1);

            if firstindexplus1 == 0 {
                // 0 is the sentinel value meaning no next entry, meaning the free list is empty
                return 0;
            };

            let offset_of_next = offset_of_small_free_list_entry(
                areanum,
                smallslabnum,
                (firstindexplus1 - 1) as usize,
            );
            let u8_ptr_to_next = unsafe { baseptr.add(offset_of_next) }; // note this isn't necessarily aligned
            assert!(u8_ptr_to_next.is_aligned_to(SINGLEWORDSIZE)); // need 4-byte alignment for atomic ops (on at least some/most platforms)
            let u32_ptr_to_next = u8_ptr_to_next.cast::<u32>();
            let nextentry = unsafe { AtomicU32::from_ptr(u32_ptr_to_next) };
            let nextindexplus1: u32 = nextentry.load(Ordering::Acquire); // YYY Acquire
            //debugln!("continuing to pop (loaded next) / areanum: {}, smallslabnum: {}, firstindexplus1: {}/---, nextindexplus1: {}/---", areanum, smallslabnum, firstindexplus1, nextindexplus1);

            let newflhdword = ((counter as u64 + 1) << 32) | nextindexplus1 as u64;

            if flh.compare_exchange_weak(
                flhdword,
                newflhdword,
                Ordering::AcqRel, // YYY AcqRel
                Ordering::Acquire // YYY Acquire
            ).is_ok() {
                //debugln!("POPPED / areanum: {}, smallslabnum: {}, firstindexplus1: {}/---, nextindexplus1: {}/---", areanum, smallslabnum, firstindexplus1, nextindexplus1);

                // These constraints must be true considering that the POP succeeded.
                // assert!(nextindexplus1 <= NUM_SLOTS_O as u32, "nextindexplus1: {}", nextindexplus1); // alloc
                assert!(nextindexplus1 <= NUM_SLOTS_O as u32);
                // debug_assert!(nextindexplus1 as u64 <= self.get_small_eac(areanum, smallslabnum).load(Ordering::SeqCst), "areanum: {}, smallslabnum: {}, firstindexplus1: {}, eac: {}", areanum, smallslabnum, firstindexplus1, self.get_small_eac(areanum, smallslabnum).load(Ordering::SeqCst)); // alloc
                debug_assert!(nextindexplus1 as u64 <= self.get_small_eac(areanum, smallslabnum).load(Ordering::SeqCst)); // noalloc

                break firstindexplus1
            } else {
                //debugln!("failed to pop / areanum: {}, smallslabnum: {}, firstindexplus1: {}/---, nextindexplus1: {}/---", areanum, smallslabnum, firstindexplus1, nextindexplus1);
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
        assert!(u8_ptr_to_flh.is_aligned_to(DOUBLEWORDSIZE)); // need 8-byte alignment for atomic ops (on at least some/most platforms)
        let u64_ptr_to_flh = u8_ptr_to_flh.cast::<u64>();

        let flh = unsafe { AtomicU64::from_ptr(u64_ptr_to_flh) };
        loop {
            let flhdword: u64 = flh.load(Ordering::Acquire); // YYY Acquire
            let firstindexplus1: u32 = (flhdword & u32::MAX as u64) as u32;
            // assert!(firstindexplus1 <= num_large_slots(largeslabnum) as u32, "firstindexplus1: {}", firstindexplus1); // alloc
            assert!(firstindexplus1 <= num_large_slots(largeslabnum) as u32); // noalloc
            // debug_assert!(firstindexplus1 as u64 <= self.get_large_eac(largeslabnum).load(Ordering::SeqCst), "largeslabnum: {}, firstindexplus1: {}, eac: {}", largeslabnum, firstindexplus1, self.get_large_eac(largeslabnum).load(Ordering::SeqCst)); // alloc
            debug_assert!(firstindexplus1 as u64 <= self.get_large_eac(largeslabnum).load(Ordering::SeqCst)); // noalloc

            let counter: u32 = (flhdword >> 32) as u32;
            //debugln!("starting to pop / largeslabnum: {}, firstindexplus1: {}/{:?}", largeslabnum, firstindexplus1, self.lssp1_p(largeslabnum, firstindexplus1));

            if firstindexplus1 == 0 {
                // 0 is the sentinel value meaning no next entry, meaning the free list is empty
                return 0;
            }

            // Intrusive free list -- free list entries are stored in data slots (when they are not in use for data).
            let offset_of_next = offset_of_large_slot(largeslabnum, (firstindexplus1 - 1) as usize);
            let u8_ptr_to_next = unsafe { baseptr.add(offset_of_next) };
            assert!(u8_ptr_to_next.is_aligned_to(SINGLEWORDSIZE)); // need 4-byte alignment for atomic ops (on at least some/most platforms)
            let u32_ptr_to_next = u8_ptr_to_next.cast::<u32>();
            let nextentry = unsafe { AtomicU32::from_ptr(u32_ptr_to_next) };
            let nextindexplus1: u32 = nextentry.load(Ordering::Acquire); // YYY Acquire
            //debugln!("continuing to pop (loaded next) / largeslabnum: {}, firstindexplus1: {}/{:?}, nextindexplus1: {}/{:?}", largeslabnum, firstindexplus1, self.lssp1_p(largeslabnum, firstindexplus1), nextindexplus1, self.lssp1_p(largeslabnum, nextindexplus1));

            let newflhdword = ((counter as u64 + 1) << 32) | nextindexplus1 as u64;

            if flh.compare_exchange_weak(
                flhdword,
                newflhdword,
                Ordering::AcqRel, // AcqRel
                Ordering::Acquire, // Acquire
            ).is_ok() {
                //debugln!("POPPED / largeslabnum: {}, firstindexplus1: {}/{:?}, nextindexplus1: {}/{:?}", largeslabnum, firstindexplus1, self.lssp1_p(largeslabnum, firstindexplus1), nextindexplus1, self.lssp1_p(largeslabnum, nextindexplus1));

                // These constraints must be true considering that the POP succeeded.
                // assert!(nextindexplus1 <= num_large_slots(largeslabnum) as u32, "nextindexplus1: {}", nextindexplus1); // alloc
                assert!(nextindexplus1 <= num_large_slots(largeslabnum) as u32); // no alloc
                // debug_assert!(nextindexplus1 as u64 <= self.get_large_eac(largeslabnum).load(Ordering::SeqCst), "thread: {}, largeslabnum: {}, firstindexplus1: {}, nextindexplus1: {}, eac: {}, time: {}", get_thread_areanum(), largeslabnum, firstindexplus1, nextindexplus1, self.get_large_eac(largeslabnum).load(Ordering::SeqCst), START_TIME.elapsed().as_nanos().separate_with_commas()); // alloc
                debug_assert!(nextindexplus1 as u64 <= self.get_large_eac(largeslabnum).load(Ordering::SeqCst)); // no alloc

                break firstindexplus1
            } else {
                //debugln!("failed to pop / largeslabnum: {}, firstindexplus1: {}/{:?}, nextindexplus1: {}/{:?}", largeslabnum, firstindexplus1, self.lssp1_p(largeslabnum, firstindexplus1), nextindexplus1, self.lssp1_p(largeslabnum, nextindexplus1));
            }
        }
    }

    // xxx maxindex is just for assertion checks
    fn inner_push_flh(
        &self,
        offset_of_flh: usize,
        offset_of_new: usize,
        new_index: u32,
        maxindex: u32
    ) {
        let baseptr = self.get_baseptr();

        let u8_ptr_to_flh = unsafe { baseptr.add(offset_of_flh) };
        assert!(u8_ptr_to_flh.is_aligned_to(DOUBLEWORDSIZE)); // need 8-byte alignment for atomic ops (on at least some/most platforms)
        let u64_ptr_to_flh = u8_ptr_to_flh.cast::<u64>();
        let flh = unsafe { AtomicU64::from_ptr(u64_ptr_to_flh) };

        let u8_ptr_to_new = unsafe { baseptr.add(offset_of_new) };
        assert!(u8_ptr_to_new.is_aligned_to(SINGLEWORDSIZE)); // need 4-byte alignment for atomic ops (on at least some/most platforms)
        let u32_ptr_to_new: *mut u32 = u8_ptr_to_new.cast::<u32>();
        let newentry = unsafe { AtomicU32::from_ptr(u32_ptr_to_new) };

        loop {
            let flhdword: u64 = flh.load(Ordering::Acquire); // YYY Acquire
            let firstindexplus1: u32 = (flhdword & u32::MAX as u64) as u32;
            assert!(firstindexplus1 < maxindex + 1);
            let counter: u32 = (flhdword >> 32) as u32;

            newentry.store(firstindexplus1, Ordering::Release); // YYY Release
            //debugln!("trying to push / new_index+1: {}/{:?} ahead of firstindexplus1: {}/--- (stored first into new)", new_index+1, u8_ptr_to_new, firstindexplus1);

            let newflhdword = ((counter as u64 + 1) << 32) | (new_index+1) as u64;

            if flh.compare_exchange_weak(
                flhdword,
                newflhdword,
                Ordering::AcqRel, // AcqRel
                Ordering::Acquire, // Acquire
            ).is_ok() {
                //debugln!("PUSHED / new_index+1: {}/{:?} ahead of firstindexplus1: {}/---", new_index+1, u8_ptr_to_new, firstindexplus1);
                break;
            } else {
                //debugln!("failed to push / new_index+1: {}/{:?} ahead of firstindexplus1: {}/---", new_index+1, u8_ptr_to_new, firstindexplus1);
            }
        }
    }

    fn push_flh(&self, newsl: SlotLocation) {
        match newsl {
            SlotLocation::SmallSlot {
                areanum,
                smallslabnum,
                slotnum,
            } => {
                assert!(slotnum < NUM_SLOTS_O);
                //debugln!("smallslabnum: {}, about to push p: {:?}, slotnum: {}", smallslabnum, self.sl_to_ptr(&newsl), slotnum);

                self.inner_push_flh(
                    offset_of_small_flh(areanum, smallslabnum),
                    offset_of_small_free_list_entry(areanum, smallslabnum, slotnum),
                    slotnum as u32,
                    NUM_SLOTS_O as u32
                );

                //debugln!("smallslabnum: {}, just pushed p: {:?}, slotnum: {}", smallslabnum, self.sl_to_ptr(&newsl), slotnum);
            }
            SlotLocation::LargeSlot {
                largeslabnum,
                slotnum,
            } => {
                assert!(slotnum < num_large_slots(largeslabnum));
                //debugln!("largeslabnum: {}, about to push p: {:?}, slotnum: {}", largeslabnum, self.sl_to_ptr(&newsl), slotnum);

                // Intrusive free list -- the free list entry is stored in the data slot.
                self.inner_push_flh(
                    offset_of_large_flh(largeslabnum),
                    offset_of_large_slot(largeslabnum, slotnum),
                    slotnum as u32,
                    num_large_slots(largeslabnum) as u32
                );

                //debugln!("largeslabnum: {}, just pushed p: {:?}, slotnum: {}", largeslabnum, self.sl_to_ptr(&newsl), slotnum);
            }
        }
    }

    fn get_small_eac(&self, areanum: usize, smallslabnum: usize) -> &AtomicU64 {
        assert!(areanum < NUM_SMALL_SLAB_AREAS);
        assert!(smallslabnum < NUM_SMALL_SLABS);

        let baseptr = self.get_baseptr();
        let offset_of_eac = offset_of_small_eac(areanum, smallslabnum);
        let u8_ptr_to_eac = unsafe { baseptr.add(offset_of_eac) };
        assert!(u8_ptr_to_eac.is_aligned_to(DOUBLEWORDSIZE)); // need 8-byte alignment for atomic ops (on at least some/most platforms)
        let u64_ptr_to_eac = u8_ptr_to_eac.cast::<u64>();
        unsafe { AtomicU64::from_ptr(u64_ptr_to_eac) }
    }

    fn get_large_eac(&self, largeslabnum: usize) -> &AtomicU64 {
        assert!(largeslabnum < NUM_LARGE_SLABS);

        let baseptr = self.get_baseptr();
        let offset_of_eac = offset_of_large_eac(largeslabnum);
        let u8_ptr_to_eac = unsafe { baseptr.add(offset_of_eac) };
        assert!(u8_ptr_to_eac.is_aligned_to(DOUBLEWORDSIZE)); // need 8-byte alignment for atomic ops (on at least some/most platforms)
        let u64_ptr_to_eac = u8_ptr_to_eac.cast::<u64>();
        unsafe { AtomicU64::from_ptr(u64_ptr_to_eac) }
    }

    /// Increment the count of ever-allocated-slots (which is the same as the index of the next never-before-allocated slot). Return the number before the increment, which is the index of the next slot you should use. In the case that all slots have been allocated, return the max number of slots (which is 1 greater than the maximum slot number).
    fn increment_eac(&self, eac: &AtomicU64, maxnumslots: usize) -> usize {
        let nexteac = eac.fetch_add(1, Ordering::Relaxed); // XXX reconsider whether we need stronger ordering constraints
        if nexteac as usize <= maxnumslots {
            nexteac as usize
        } else {
            if nexteac as usize > maxnumslots + 100000 {
                // If eac is maxed out -- at maxnumslots -- another thread has incremented past NUM_SLOTS but not yet decremented it, then this could exceed maxnumslots. However, if this has happened many, many times simultaneously, such that eac is more than a small number higher than maxnuslots, then something is wrong and we should panic to prevent some kind of unknown failure case or exploitation.
                panic!("the Ever-Allocated-Counter exceeded max slots + 100000");
            }
            
            eac.fetch_sub(1, Ordering::Relaxed); // XXX reconsider whether we need stronger ordering constraints
            
            maxnumslots
        }
    }

    fn inner_small_alloc(&self, areanum: usize, smallslabnum: usize) -> Option<SlotLocation> {
        let flhplus1 = self.pop_small_flh(areanum, smallslabnum);
        if flhplus1 != 0 {
            // xxx add unit test of this case
            let sl = SlotLocation::SmallSlot {
                areanum,
                smallslabnum,
                slotnum: (flhplus1 - 1) as usize,
            };
            //debugln!("... in inner_small_alloc(), flhplus1: {}/{:?}", flhplus1, self.sl_to_ptr(&sl));
            Some(sl)
        } else {
            let eac: usize = self.increment_eac(self.get_small_eac(areanum, smallslabnum), NUM_SLOTS_O);
            if eac < NUM_SLOTS_O {
                // xxx add unit test of this case
                let sl = SlotLocation::SmallSlot {
                    areanum,
                    smallslabnum,
                    slotnum: eac,
                };
                //debugln!("in inner_small_alloc(), eac: {}/{:?}", eac, self.sl_to_ptr(&sl));
                Some(sl)
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
            let sl = SlotLocation::LargeSlot {
                largeslabnum,
                slotnum: (flhplus1 - 1) as usize,
            };
            //debugln!("... in inner_large_alloc(), largeslabnum: {}, popped flhplus1: {}/{:?}", largeslabnum, flhplus1, self.sl_to_ptr(&sl));
            Some(sl)
        } else {
            let eac: usize = self.increment_eac(
                self.get_large_eac(largeslabnum),
                if largeslabnum == HUGE_SLABNUM { NUM_SLOTS_HUGE } else { NUM_SLOTS_O }
            );
            if eac < num_large_slots(largeslabnum) {
                // xxx add unit test of this case
                let sl = SlotLocation::LargeSlot {
                    largeslabnum,
                    slotnum: eac,
                };
                //debugln!("in inner_large_alloc(), largeslabnum: {}, eac'ed {}/{:?}", largeslabnum, eac, self.sl_to_ptr(&sl));
                Some(sl)
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
        assert!(
            (alignment & (alignment - 1)) == 0,
            "alignment must be a power of two"
        );
        assert!(alignment <= MAX_ALIGNMENT); // We don't guarantee larger alignments than 4096

        // Round up size to the nearest multiple of alignment in order to get a slot that is aligned on that size.
        let alignedsize: usize = ((size - 1) | (alignment - 1)) + 1;

        // XXX benchmark various ways to do this switch+loop...
        // This way of doing this branch+loop was informed by:
        // 1. Let's branch on small-slot vs large-slot just once and then have two (similar) code paths instead of branching on small-slot vs large-slot multiple times in one code path, and
        // 2. I profiled zebra, which showed that 32B was the most common slot size, and that < 32B was more common than > 32B, and that among > 32B slot sizes, 64B was the most common one...
        if alignedsize <= SIZE_OF_BIGGEST_SMALL_SLOT {
            let mut smallslabnum = NUM_SMALL_SLABS - 1;
            while smallslabnum > 0 && small_slabnum_to_slotsize(smallslabnum - 1) >= alignedsize {
                smallslabnum -= 1;
            }
            assert!(smallslabnum < NUM_SMALL_SLABS);
            assert!(small_slabnum_to_slotsize(smallslabnum) >= alignedsize);
            assert!(if smallslabnum > 0 {
                small_slabnum_to_slotsize(smallslabnum - 1) < alignedsize
            } else {
                true
            });

            self.inner_small_alloc(get_thread_areanum(), smallslabnum)
        } else if alignedsize <= SIZE_OF_HUGE_SLOTS {
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
fn get_thread_areanum() -> usize {
    THREAD_AREANUM.with(|cell| {
        cell.get().map_or_else(
            || {
                let new_value = GLOBAL_THREAD_AREANUM.fetch_add(1, Ordering::Relaxed) % NUM_SMALL_SLAB_AREAS as u32; // reconsider we need stronger ordering constraints
                THREAD_AREANUM.with(|cell| cell.set(Some(new_value)));
                new_value
            },
            |value| value,
        )
    }) as usize
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
                self.push_flh(sl);
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
        assert!(
            (oldalignment & (oldalignment - 1)) == 0,
            "alignment must be a power of two"
        );
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
                        // ... else use the HUGE slots.
                        NUM_LARGE_SLABS - 1
                    };

                    // Allocate a new slot...
                    let optnewsl = self.inner_large_alloc(largeslabnum);
                    let newptr: *mut u8 = match optnewsl {
                        Some(newsl) => {
                            let offset = newsl.offset();
                            let slotsize = newsl.slotsize();
                            let p = unsafe { baseptr.add(offset) };
                            assert!(if slotsize.is_power_of_two() {
                                p.is_aligned_to(min(newsl.slotsize(), MAX_ALIGNMENT))
                            } else {
                                true
                            });
                            p
                        }
                        None => {
                            // Slab was full. Fallback to system allocator.
                            let layout =
                                unsafe { Layout::from_size_align_unchecked(newsize, oldalignment) };
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

    const BYTES1: [u8; 8] = [1, 2, 4, 3, 5, 6, 7, 8];
    const BYTES2: [u8; 8] = [9, 8, 7, 6, 5, 4, 3, 2];
    const BYTES3: [u8; 8] = [0xA, 0xB, 0xC, 0xD, 0xE, 0xF, 0x10, 0x11];
    const BYTES4: [u8; 8] = [0x12, 0x11, 0x10, 0xF, 0xE, 0xD, 0xC, 0xB];
    // const BYTES5: [u8; 8] = [0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A];
    const BYTES6: [u8; 8] = [0xFE, 0xFD, 0xFC, 0xFB, 0xFA, 0xF9, 0xF8, 0xF7];
    //const BYTES7: [u8; 8] = [0xF7, 0xF6, 0xF5, 0xF4, 0xF3, 0xF2, 0xF1, 0xF0];

    #[test]
    fn test_offset_of_vars() {
        assert_eq!(offset_of_small_flh(0, 0), 0);
        assert_eq!(offset_of_small_eac(0, 0), 8);
        assert_eq!(offset_of_small_flh(0, 1), 16);
        assert_eq!(offset_of_small_eac(0, 1), 24);

        // There are 11 slabs in an small-slab-area, 2 variables for each slab, and 8 bytes for each variable, so the first variable in the second slab (slab index 1) should be at offset 176.
        assert_eq!(offset_of_small_flh(1, 0), 176);
        assert_eq!(offset_of_small_eac(1, 0), 184);
        assert_eq!(offset_of_small_flh(1, 1), 192);
        assert_eq!(offset_of_small_eac(1, 1), 200);

        // The large-slab vars start after all the small-slab vars
        let all_small_slab_vars = 11 * 2 * 8 * NUM_SMALL_SLAB_AREAS;
        assert_eq!(offset_of_large_flh(0), all_small_slab_vars);
        assert_eq!(offset_of_large_eac(0), all_small_slab_vars + 8);
        assert_eq!(offset_of_large_flh(1), all_small_slab_vars + 16);
        assert_eq!(offset_of_large_eac(1), all_small_slab_vars + 24);
    }

    use lazy_static::lazy_static;

    lazy_static! {
        static ref SM: Smalloc = Smalloc::new();
    }

    /// Simply generate a Layout and call `inner_alloc()`.
    fn help_inner_alloc(size: usize, alignment: usize) -> SlotLocation {
        let layout = Layout::from_size_align(size, alignment).unwrap();
        SM.inner_alloc(layout).unwrap()
    }

    #[test]
    fn test_one_alloc_small() {
        SM.idempotent_init();

        let layout = Layout::from_size_align(6, 1).unwrap();
        SM.inner_alloc(layout).unwrap();
    }

    #[test]
    fn test_one_alloc_large() {
        SM.idempotent_init();

        let layout = Layout::from_size_align(120, 4).unwrap();
        SM.inner_alloc(layout).unwrap();
    }

    #[test]
    fn test_one_alloc_huge() {
        SM.idempotent_init();

        let layout = Layout::from_size_align(1000000, 8).unwrap();
        SM.inner_alloc(layout).unwrap();
    }

    #[test]
    fn test_a_few_allocs_and_a_dealloc_for_each_small_slab() {
        SM.idempotent_init();

        for smallslabnum in 0..NUM_SMALL_SLABS {
            help_inner_alloc_small(smallslabnum);
        }
    }

    #[test]
    fn test_a_few_allocs_and_a_dealloc_for_each_large_slab() {
        SM.idempotent_init();

        for largeslabnum in 0..NUM_LARGE_SLABS {
            help_inner_alloc_large(largeslabnum);
        }
    }

    /// Generate a number of requests (size+alignment) that fit into
    /// the given large slab and for each request call
    /// help_inner_alloc_four_times_large()
    fn help_inner_alloc_large(largeslabnum: usize) {
        let slotsize = large_slabnum_to_slotsize(largeslabnum);
        let smallest = if largeslabnum == 0 {
            small_slabnum_to_slotsize(NUM_SMALL_SLABS - 1) + 1
        } else {
            large_slabnum_to_slotsize(largeslabnum - 1) + 1
        };
        let largest = slotsize;
        for reqsize in [ smallest, smallest + 1, smallest + 2, largest - 3, largest - 1, largest, ] {
            // Generate alignments
            let mut reqalign = 1;
            loop {
                // Test this size/align combo
                help_inner_alloc_four_times_large(reqsize, reqalign);
                reqalign *= 2;
                let alignedsize: usize = ((reqsize - 1) | (reqalign - 1)) + 1;
                if alignedsize > slotsize || alignedsize > MAX_ALIGNMENT {
                    break;
                };
            }
        }
    }

    /// Generate a number of requests (size+alignment) that fit into
    /// the given small slab and for each request call
    /// help_inner_alloc_four_times_small()
    fn help_inner_alloc_small(smallslabnum: usize) {
        let slotsize = small_slabnum_to_slotsize(smallslabnum);
        let smallest = if smallslabnum == 0 {
            1
        } else {
            small_slabnum_to_slotsize(smallslabnum - 1) + 1
        };
        let largest = slotsize;
        for reqsize in smallest..=largest {
            // Generate alignments
            let mut reqalign = 1;
            loop {
                // Test this size/align combo
                help_inner_alloc_four_times_small(reqsize, reqalign);
                reqalign *= 2;
                let alignedsize: usize = ((reqsize - 1) | (reqalign - 1)) + 1;
                if alignedsize > slotsize {
                    break;
                };
            }
        }
    }

    /// Allocate this size+align three times, then free the middle
    /// one, then allocate a fourth time.
    fn help_inner_alloc_four_times_large(reqsize: usize, reqalign: usize) {
        let sl1 = help_inner_alloc(reqsize, reqalign);
        let SlotLocation::LargeSlot { largeslabnum: _, slotnum: _, } = sl1 else {
            panic!("should have returned a large slot");
        };

        let sl2 = help_inner_alloc(reqsize, reqalign);
        let SlotLocation::LargeSlot { largeslabnum: _, slotnum: _, } = sl2 else {
            panic!("should have returned a large slot");
        };

        let sl3 = help_inner_alloc(reqsize, reqalign);
        let SlotLocation::LargeSlot { largeslabnum: _, slotnum: _, } = sl3 else {
            panic!("should have returned a large slot");
        };

        // Now free the middle one.
        SM.push_flh(sl2);

        // And allocate another one.
        let sl4 = help_inner_alloc(reqsize, reqalign);
        let SlotLocation::LargeSlot { largeslabnum: _, slotnum: _, } = sl4 else {
            panic!("should have returned a large slot");
        };
    }

    /// Allocate this size+align three times, then free the middle
    /// one, then allocate a fourth time.
    fn help_inner_alloc_four_times_small(reqsize: usize, reqalign: usize) {
        let sl1 = help_inner_alloc(reqsize, reqalign);
        let SlotLocation::SmallSlot { areanum: _, smallslabnum: _, slotnum: _, } = sl1 else {
            panic!("should have returned a small slot");
        };

        let sl2 = help_inner_alloc(reqsize, reqalign);
        let SlotLocation::SmallSlot { areanum: _, smallslabnum: _, slotnum: _, } = sl2 else {
            panic!("should have returned a small slot");
        };

        let sl3 = help_inner_alloc(reqsize, reqalign);
        let SlotLocation::SmallSlot { areanum: _, smallslabnum: _, slotnum: _, } = sl3 else {
            panic!("should have returned a small slot");
        };

        // Now free the middle one.
        SM.push_flh(sl2);

        // And allocate another one.
        let sl4 = help_inner_alloc(reqsize, reqalign);
        let SlotLocation::SmallSlot { areanum: _, smallslabnum: _, slotnum: _, } = sl4 else {
            panic!("should have returned a small slot");
        };
    }

    #[test]
    fn test_alloc_1_byte_then_dealloc() {
        let layout = Layout::from_size_align(1, 1).unwrap();
        let p = unsafe { SM.alloc(layout) };
        unsafe { SM.dealloc(p, layout) };
    }

    #[test]
    fn test_roundtrip_slot_to_ptr_to_slot() {
        let baseptr_for_testing: *mut u8 = SIZE_OF_HUGE_SLOTS as *mut u8;

        // First the small-slabs region:
        for areanum in [ 1, 2, 30, 31, 32, 33, NUM_SMALL_SLAB_AREAS - 3, NUM_SMALL_SLAB_AREAS - 2, NUM_SMALL_SLAB_AREAS - 1,
        ] {
            for smallslabnum in 0..NUM_SMALL_SLABS {
                for slotnum in [ 0, 1, 2, 253, 254, 255, 256, 257, 1022, 1023, 1024, 2usize.pow(16) - 1, 2usize.pow(16), 2usize.pow(16) + 1, NUM_SLOTS_O - 2, NUM_SLOTS_O - 1, ] {
                    let sl1 = SlotLocation::SmallSlot { areanum, smallslabnum, slotnum, };
                    let offset = sl1.offset();
                    assert!(offset >= DATA_SLABS_BASE_OFFSET);
                    assert!(
                        offset < DATA_SLABS_BASE_OFFSET + SMALL_SLAB_AREAS_REGION_SPACE,
                        "sl1: {:?}, {} {} {} {}",
                        sl1,
                        offset,
                        DATA_SLABS_BASE_OFFSET,
                        SMALL_SLAB_AREAS_REGION_SPACE,
                        (DATA_SLABS_BASE_OFFSET + SMALL_SLAB_AREAS_REGION_SPACE)
                    );
                    assert!(offset < LARGE_SLAB_REGION_BASE_OFFSET);
                    let p = unsafe { baseptr_for_testing.add(offset) };
                    let sl2 = SlotLocation::new_from_ptr(baseptr_for_testing, p).unwrap();
                    assert_eq!(sl1, sl2);
                }
            }
        }

        // Then the large-slabs region excluding the huge slab:
        for largeslabnum in 0..HUGE_SLABNUM {
            for slotnum in [ 0, 1, 2, 253, 254, 255, 256, 257, 1022, 1023, 1024, 2usize.pow(16) - 1, 2usize.pow(16), 2usize.pow(16) + 1, NUM_SLOTS_O - 2, NUM_SLOTS_O - 1, ] {
                let sl1 = SlotLocation::LargeSlot { largeslabnum, slotnum, };
                let offset = sl1.offset();
                assert!(offset >= DATA_SLABS_BASE_OFFSET);
                let p = unsafe { baseptr_for_testing.add(offset) };
                let sl2 = SlotLocation::new_from_ptr(baseptr_for_testing, p).unwrap();
                assert_eq!(sl1, sl2);
            }
        }

        // Then the huge slab:
        let largeslabnum = HUGE_SLABNUM;
        for slotnum in [ 0, 1, 2, 253, 254, 255, 256, 257, 1022, 1023, 1024, 2usize.pow(16) - 1, 2usize.pow(16), 2usize.pow(16) + 1, NUM_SLOTS_HUGE - 2, NUM_SLOTS_HUGE - 1, ] {
            let sl1 = SlotLocation::LargeSlot { largeslabnum, slotnum, };
            let offset = sl1.offset();
            assert!(offset >= DATA_SLABS_BASE_OFFSET);
            let p = unsafe { baseptr_for_testing.add(offset) };
            let sl2 = SlotLocation::new_from_ptr(baseptr_for_testing, p).unwrap();
            assert_eq!(sl1, sl2);
        }
    }

    use std::thread;

    #[test]
    fn test_main_thread_init() {
        SM.idempotent_init();
    }

    #[test]
    fn test_one_thread_simple() {
        SM.idempotent_init();

        let handle1 = thread::spawn(move || {
            for _j in 0..1000 {
                help_inner_alloc_small(0);
            }
        });
        
        handle1.join().unwrap();
    }

    #[test]
    fn test_two_threads_simple() {
        SM.idempotent_init();

        let handle1 = thread::spawn(move || {
            for _j in 0..1000 {
                help_inner_alloc_small(0);
            }
        });
        
        let handle2 = thread::spawn(move || {
            for _j in 0..1000 {
                help_inner_alloc_small(0);
            }
        });

        handle1.join().unwrap();
        handle2.join().unwrap();
    }

    #[test]
    fn test_twelve_threads_small() {
        SM.idempotent_init();

        let mut handles = Vec::new();
        for _i in 0..12 {
            handles.push(thread::spawn(move || {
                for _j in 0..1000 {
                    help_inner_alloc_small(0);
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_twelve_threads_large_alloc_dealloc() {
        SM.idempotent_init();

        let l = Layout::from_size_align(64, 1).unwrap();

        let mut handles = Vec::new();
        for _i in 0..12 {
            handles.push(thread::spawn(move || {
                help_many_random_alloc_dealloc(1000, l, 0);
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_a_thousand_threads_small() {
        SM.idempotent_init();

        let mut handles = Vec::new();
        for _i in 0..1000 {
            handles.push(thread::spawn(move || {
                for _j in 0..12 {
                    help_inner_alloc_small(0);
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    fn help_n_threads_malloc_dealloc(n: u32, iters: usize, layout: Layout, seed: u64) {
        SM.idempotent_init();

        let mut handles = Vec::new();
        for _i in 0..n {
            handles.push(thread::spawn(move || {
                help_many_random_alloc_dealloc(iters, layout, seed);
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    fn help_n_threads_malloc_dealloc_with_writes(n: u32, iters: usize, layout: Layout, seed: u64) {
        SM.idempotent_init();

        let mut handles = Vec::new();
        for _i in 0..n {
            handles.push(thread::spawn(move || {
                help_many_random_alloc_dealloc_with_writes(iters, layout, seed);
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    fn help_n_threads_malloc_dealloc_realloc_no_writes(n: u32, iters: usize, layout: Layout, seed: u64) {
        SM.idempotent_init();

        let mut handles = Vec::new();
        for _i in 0..n {
            handles.push(thread::spawn(move || {
                help_many_random_alloc_dealloc_realloc_no_writes(iters, layout, seed);
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    fn help_n_threads_malloc_dealloc_realloc_with_writes(n: u32, iters: usize, layout: Layout, seed: u64) {
        SM.idempotent_init();

        let mut handles = Vec::new();
        for _i in 0..n {
            handles.push(thread::spawn(move || {
                help_many_random_alloc_dealloc_realloc_with_writes(iters, layout, seed);
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_32_threads_large_malloc_dealloc_no_writes() {
        let seed = 0;

        let l = Layout::from_size_align(64, 1).unwrap();

        help_n_threads_malloc_dealloc(32, 1000, l, seed);
    }

    #[test]
    fn test_32_threads_large_malloc_dealloc_with_writes() {
        //let mut r = rand::rng();
        //let seed = r.random::<u64>();
        //debugln!("rand seed: {}", seed);
        let seed = 0;
        //debugln!("hardcoded seed: {}", seed);

        let l = Layout::from_size_align(64, 1).unwrap();

        help_n_threads_malloc_dealloc_with_writes(32, 1000, l, seed);
    }

    #[test]
    fn test_32_threads_large_malloc_dealloc_realloc_with_writes() {
        //let mut r = rand::rng();
        //let seed = r.random::<u64>();
        //debugln!("rand seed: {}", seed);
        let seed = 0;
        //debugln!("hardcoded seed: {}", seed);

        let l = Layout::from_size_align(64, 1).unwrap();

        help_n_threads_malloc_dealloc_realloc_with_writes(32, 1000, l, seed);
    }

    #[test]
    fn test_32_threads_large_malloc_dealloc_realloc_no_writes() {
        //let mut r = rand::rng();
        //let seed = r.random::<u64>();
        //debugln!("rand seed: {}", seed);
        let seed = 0;
        //debugln!("hardcoded seed: {}", seed);

        let l = Layout::from_size_align(64, 1).unwrap();

        help_n_threads_malloc_dealloc_realloc_no_writes(32, 1000, l, seed);
    }

    #[test]
    fn test_32_threads_small_malloc_dealloc() {
        //let mut r = rand::rng();
        //let seed = r.random::<u64>();
        //debugln!("rand seed: {}", seed);
        let seed = 0;
        //debugln!("hardcoded seed: {}", seed);

        let l = Layout::from_size_align(4, 1).unwrap();

        help_n_threads_malloc_dealloc(32, 1000, l, seed);
    }

    #[test]
    fn test_32_threads_small_malloc_dealloc_with_writes() {
        //let mut r = rand::rng();
        //let seed = r.random::<u64>();
        //debugln!("rand seed: {}", seed);
        let seed = 0;
        //debugln!("hardcoded seed: {}", seed);

        let l = Layout::from_size_align(4, 1).unwrap();

        help_n_threads_malloc_dealloc_with_writes(32, 1000, l, seed);
    }


    #[test]
    fn test_a_thousand_threads_large_malloc_dealloc() {
        let l = Layout::from_size_align(64, 1).unwrap();

        help_n_threads_malloc_dealloc(1000, 1000, l, 0);
    }

    use rand::rngs::StdRng;
    use rand::SeedableRng;
    use rand::Rng;
    use ahash::HashSet;
    use ahash::RandomState;
    
    fn help_many_random_alloc_dealloc(iters: usize, layout: Layout, seed: u64) {
        let l = layout;
        let mut r = StdRng::seed_from_u64(seed);
        let mut m: HashSet<(*mut u8, Layout)> = HashSet::with_hasher(RandomState::with_seed(seed as usize));
        
        let mut ps = Vec::new();

        for _i in 0..iters {
            // random coin
            if r.random::<bool>() {
                // Free
                if !ps.is_empty() {
                    let i = r.random_range(0..ps.len());
                    let (p, lt) = ps.remove(i);
                    assert!(m.contains(&(p, lt)), "thread: {:>3}, {:?} {}-{}", get_thread_areanum(), p, l.size(), l.align());
                    m.remove(&(p, lt));
                    unsafe { SM.dealloc(p, lt) };
                }
            } else {
                // Malloc
                let p = unsafe { SM.alloc(l) };
                assert!(!m.contains(&(p, l)), "thread: {:>3}, {:?} {}-{}", get_thread_areanum(), p, l.size(), l.align());
                m.insert((p, l));
                ps.push((p, l));
            }
        }
    }
    
    fn help_many_random_alloc_dealloc_with_writes(iters: usize, layout: Layout, seed: u64) {
        let l = layout;
        let mut r = StdRng::seed_from_u64(seed);
        let mut m: HashSet<(*mut u8, Layout)> = HashSet::with_hasher(RandomState::with_seed(seed as usize));
        
        let mut ps = Vec::new();
        
        for _i in 0..iters {
            if r.random::<bool>() && !ps.is_empty() {
                // Free
                let (p, lt) = ps.remove(r.random_range(0..ps.len()));
                assert!(m.contains(&(p, lt)), "thread: {:>3}, {:?} {}-{}", get_thread_areanum(), p, l.size(), l.align());
                m.remove(&(p, lt));
                unsafe { std::ptr::copy_nonoverlapping(BYTES1.as_ptr(), p, min(BYTES1.len(), lt.size())) };
                //debugln!("wrote to {:?} with {:?}", p, BYTES1);
                unsafe { SM.dealloc(p, lt) };

                // Write to a random (other) allocation...
                if !ps.is_empty() {
                    let (po, lto) = ps[r.random_range(0..ps.len())];
                    unsafe { std::ptr::copy_nonoverlapping(BYTES2.as_ptr(), po, min(BYTES2.len(), lto.size())) };
                }
            } else {
                // Malloc
                let p = unsafe { SM.alloc(l) };
                unsafe { std::ptr::copy_nonoverlapping(BYTES3.as_ptr(), p, min(BYTES3.len(), layout.size())) };
                assert!(!m.contains(&(p, l)), "thread: {:>3}, {:?} {}-{}", get_thread_areanum(), p, l.size(), l.align());
                m.insert((p, l));
                ps.push((p, l));

                // Write to a random (other) allocation...
                if !ps.is_empty() {
                    let (po, lto) = ps[r.random_range(0..ps.len())];
                    unsafe { std::ptr::copy_nonoverlapping(BYTES4.as_ptr(), po, min(BYTES4.len(), lto.size())) };
                }
            }
        }
    }
    
    use rand::seq::IndexedRandom;
    fn help_many_random_alloc_dealloc_realloc_no_writes(iters: usize, layout: Layout, seed: u64) {
        let l1 = layout;
        let mut ls = Vec::new();
        ls.push(l1);
        let l2 = Layout::from_size_align(l1.size() + 10, l1.align()).unwrap();
        ls.push(l2);
        let l3 = Layout::from_size_align(l1.size() - 10, l1.align()).unwrap();
        ls.push(l3);
        let l4 = Layout::from_size_align(l1.size() * 2 + 10, l1.align()).unwrap();
        ls.push(l4);
        
        let mut r = StdRng::seed_from_u64(seed);
        let mut m: HashSet<(*mut u8, Layout)> = HashSet::with_hasher(RandomState::with_seed(seed as usize));

        let mut ps = Vec::new();

        for _i in 0..iters {
            // random coin
            let coin = r.random_range(0..3);
            if coin == 0 {
                // Free
                if !ps.is_empty() {
                    let (p, lt) = ps.remove(r.random_range(0..ps.len()));
                    assert!(m.contains(&(p, lt)), "thread: {:>3}, {:?} {}-{}", get_thread_areanum(), p, lt.size(), lt.align());
                    m.remove(&(p, lt));
                    //unsafe { std::ptr::copy_nonoverlapping(BYTES1.as_ptr(), p, min(BYTES1.len(), lt.size())) };
                    unsafe { SM.dealloc(p, lt) };

                    // Write to a random (other) allocation...
                    //if !ps.is_empty() {
                    //    let (po, lto) = ps[r.random_range(0..ps.len())];
                    //    unsafe { std::ptr::copy_nonoverlapping(BYTES2.as_ptr(), po, min(BYTES2.len(), lto.size())) };
                    //}
                }
            } else if coin == 1 {
                // Malloc
                let lt = ls.choose(&mut r).unwrap();
                let p = unsafe { SM.alloc(*lt) };
                //unsafe { std::ptr::copy_nonoverlapping(BYTES3.as_ptr(), p, min(BYTES3.len(), lt.size())) };
                assert!(!m.contains(&(p, *lt)), "thread: {:>3}, {:?} {}-{}", get_thread_areanum(), p, lt.size(), lt.align());
                m.insert((p, *lt));
                ps.push((p, *lt));

                //if !ps.is_empty() {
                //    let (po, lto) = ps[r.random_range(0..ps.len())];
                //    unsafe { std::ptr::copy_nonoverlapping(BYTES4.as_ptr(), po, min(BYTES4.len(), lto.size())) };
                //}
            } else {
                // Realloc
                if !ps.is_empty() {
                    let i = r.random_range(0..ps.len());
                    let (p, lt) = ps.remove(i);
                    assert!(m.contains(&(p, lt)), "thread: {:>3}, {:?} {}-{}", get_thread_areanum(), p, lt.size(), lt.align());
                    m.remove(&(p, lt));

                    let newlt = ls.choose(&mut r).unwrap();
                    let newp = unsafe { SM.realloc(p, lt, newlt.size()) };

                    assert!(!m.contains(&(newp, *newlt)), "thread: {:>3}, {:?} {}-{}", get_thread_areanum(), newp, newlt.size(), newlt.align());
                    m.insert((newp, *newlt));
                    ps.push((newp, *newlt));

                    //// Write to a random allocation...
                    //let (po, lto) = ps.choose(&mut r).unwrap();
                    //unsafe { std::ptr::copy_nonoverlapping(BYTES6.as_ptr(), *po, min(BYTES6.len(), lto.size())) };
                }
            }
        }
    }
    
    fn help_many_random_alloc_dealloc_realloc_with_writes(iters: usize, layout: Layout, seed: u64) {
        let l1 = layout;
        let mut ls = Vec::new();
        ls.push(l1);
        let l2 = Layout::from_size_align(l1.size() + 10, l1.align()).unwrap();
        ls.push(l2);
        let l3 = Layout::from_size_align(l1.size() - 10, l1.align()).unwrap();
        ls.push(l3);
        let l4 = Layout::from_size_align(l1.size() * 2 + 10, l1.align()).unwrap();
        ls.push(l4);
        
        let mut r = StdRng::seed_from_u64(seed);
        let mut m: HashSet<(*mut u8, Layout)> = HashSet::with_hasher(RandomState::with_seed(seed as usize));

        let mut ps = Vec::new();

        for _i in 0..iters {
            // random coin
            let coin = r.random_range(0..3);
            if coin == 0 {
                // Free
                if !ps.is_empty() {
                    let (p, lt) = ps.remove(r.random_range(0..ps.len()));
                    assert!(m.contains(&(p, lt)), "thread: {:>3}, {:?} {}-{}", get_thread_areanum(), p, lt.size(), lt.align());
                    m.remove(&(p, lt));
                    unsafe { std::ptr::copy_nonoverlapping(BYTES1.as_ptr(), p, min(BYTES1.len(), lt.size())) };
                    unsafe { SM.dealloc(p, lt) };

                    // Write to a random (other) allocation...
                    if !ps.is_empty() {
                        let (po, lto) = ps[r.random_range(0..ps.len())];
                        unsafe { std::ptr::copy_nonoverlapping(BYTES2.as_ptr(), po, min(BYTES2.len(), lto.size())) };
                    }
                }
            } else if coin == 1 {
                // Malloc
                let lt = ls.choose(&mut r).unwrap();
                let p = unsafe { SM.alloc(*lt) };
                unsafe { std::ptr::copy_nonoverlapping(BYTES3.as_ptr(), p, min(BYTES3.len(), lt.size())) };
                assert!(!m.contains(&(p, *lt)), "thread: {:>3}, {:?} {}-{}", get_thread_areanum(), p, lt.size(), lt.align());
                m.insert((p, *lt));
                ps.push((p, *lt));

                if !ps.is_empty() {
                    let (po, lto) = ps[r.random_range(0..ps.len())];
                    unsafe { std::ptr::copy_nonoverlapping(BYTES4.as_ptr(), po, min(BYTES4.len(), lto.size())) };
                }
            } else {
                // Realloc
                if !ps.is_empty() {
                    let i = r.random_range(0..ps.len());
                    let (p, lt) = ps.remove(i);
                    assert!(m.contains(&(p, lt)), "thread: {:>3}, {:?} {}-{}", get_thread_areanum(), p, lt.size(), lt.align());
                    m.remove(&(p, lt));

                    let newlt = ls.choose(&mut r).unwrap();
                    let newp = unsafe { SM.realloc(p, lt, newlt.size()) };

                    assert!(!m.contains(&(newp, *newlt)), "thread: {:>3}, {:?} {}-{}", get_thread_areanum(), newp, newlt.size(), newlt.align());
                    m.insert((newp, *newlt));
                    ps.push((newp, *newlt));

                    // Write to a random allocation...
                    let (po, lto) = ps.choose(&mut r).unwrap();
                    unsafe { std::ptr::copy_nonoverlapping(BYTES6.as_ptr(), *po, min(BYTES6.len(), lto.size())) };
                }
            }
        }
    }
    
    #[test]
    fn test_100_000_large_allocs_deallocs_reallocs_with_writes() {
        let l = Layout::from_size_align(64, 1).unwrap();

        help_many_random_alloc_dealloc_realloc_with_writes(100_000, l, 0);
    }

    #[test]
    fn test_100_000_large_allocs_deallocs_no_reallocs_no_writes() {
        let l = Layout::from_size_align(64, 1).unwrap();

        help_many_random_alloc_dealloc(100_000, l, 0);
    }

    #[test]
    fn test_100_000_large_allocs_deallocs_no_reallocs_with_writes() {
        let l = Layout::from_size_align(64, 1).unwrap();

        help_many_random_alloc_dealloc_with_writes(100_000, l, 0);
    }

    extern crate test;
    use test::Bencher;

    const MAX: usize = 2usize.pow(39);
    const NUM_ARGS: usize = 128;

    use std::hint::black_box;

    fn pot_builtin(x: usize) -> bool {
        x.is_power_of_two()
    }

    #[bench]
    fn bench_pot_builtin_randoms(b: &mut Bencher) {
        let mut r = StdRng::seed_from_u64(0);
        let reqalignments: Vec<usize> = (0..NUM_ARGS).map(|_| r.random_range(0..MAX)).collect();
        let mut i = 0;

        b.iter(|| {
            let align = reqalignments[i % NUM_ARGS];
            black_box(pot_builtin(align));

            i += 1;
        });
    }

    // #[bench]
    // fn bench_alloc_and_free_32_threads(b: &mut Bencher) {
    //     let l = Layout::from_size_align(64, 1).unwrap();

    //     let mut r = StdRng::seed_from_u64(0);
    //     let mut ps = Vec::new();

    //     b.iter(|| {
    //         if r.random::<bool>() {
    //             // Free
    //             if !ps.is_empty() {
    //                 let i = r.random_range(0..ps.len());
    //                 let (p, l2) = ps.remove(i);
    //                 unsafe { SM.dealloc(p, l2) };
    //             }
    //         } else {
    //             // Malloc
    //             let p = unsafe { SM.alloc(l) };
    //             ps.push((p, l));
    //         }
    //     });
    // }

    #[bench]
    fn bench_alloc_and_free(b: &mut Bencher) {
        let layout = Layout::from_size_align(1, 1).unwrap();

        b.iter(|| {
            let p = black_box(unsafe { SM.alloc(layout) });
            unsafe { SM.dealloc(p, layout) };
        });
    }

    #[bench]
    fn bench_sum_small_slab_sizes(b: &mut Bencher) {
        let mut r = StdRng::seed_from_u64(0);
        let reqslabnums: Vec<usize> = (0..NUM_ARGS)
            .map(|_| r.random_range(0..=NUM_SMALL_SLABS))
            .collect();
        let mut i = 0;

        b.iter(|| {
            black_box(sum_small_slab_sizes(reqslabnums[i % NUM_ARGS]));

            i += 1;
        });
    }

    #[bench]
    fn bench_sum_large_slab_sizes(b: &mut Bencher) {
        let mut r = StdRng::seed_from_u64(0);
        let reqslabnums: Vec<usize> = (0..NUM_ARGS)
            .map(|_| r.random_range(0..=NUM_LARGE_SLABS))
            .collect();
        let mut i = 0;

        b.iter(|| {
            black_box(sum_large_slab_sizes(reqslabnums[i % NUM_ARGS]));

            i += 1;
        });
    }

    fn pot_bittwiddle(x: usize) -> bool {
        x > 0 && (x & (x - 1)) != 0
    }

    // fn dummy() { }

    // #[bench]
    // fn bench_pop_large_flh(b: &mut Bencher) {
    //     SM.idempotent_init();

    //     let sls = Vec:new();
    //     let mut i = 0;
    //     while i < NUM_SLOTS_O {
    //         let sl = SM.inner_large_alloc(0).unwrap();

    

    //         i += 1;
    //     }

    //     b.iter(|| {
    //         black_box(dummy());

    //         i += 1;
    //     });

    //     eprintln!("i is now {}", i);
    // }

    #[bench]
    fn bench_pot_builtin_powtwos(b: &mut Bencher) {
        let mut r = StdRng::seed_from_u64(0);
        let reqalignments: Vec<usize> = (0..NUM_ARGS)
            .map(|_| 2usize.pow(r.random_range(0..35)))
            .collect();
        let mut i = 0;

        b.iter(|| {
            let align = reqalignments[i % NUM_ARGS];
            black_box(pot_builtin(align));

            i += 1;
        });
    }

    #[bench]
    fn bench_pot_bittwiddle_randoms(b: &mut Bencher) {
        let mut r = StdRng::seed_from_u64(0);
        let reqalignments: Vec<usize> = (0..NUM_ARGS).map(|_| r.random_range(0..MAX)).collect();
        let mut i = 0;

        b.iter(|| {
            let align = reqalignments[i % NUM_ARGS];
            black_box(pot_bittwiddle(align));

            i += 1;
        });
    }

    #[bench]
    fn bench_pot_bittwiddle_powtwos(b: &mut Bencher) {
        let mut r = StdRng::seed_from_u64(0);
        let reqalignments: Vec<usize> = (0..NUM_ARGS)
            .map(|_| 2usize.pow(r.random_range(0..35)))
            .collect();
        let mut i = 0;

        b.iter(|| {
            let align = reqalignments[i % NUM_ARGS];
            black_box(pot_bittwiddle(align));

            i += 1;
        });
    }

    //use std::ptr::null_mut;

    // #[bench]
    // fn bench_slotlocation_of_ptr(b: &mut Bencher) {
    //     let mut r = StdRng::seed_from_u64(0);
    //     let baseptr_for_testing: *mut u8 = null_mut();
    //     let mut reqptrs = [null_mut(); NUM_ARGS];
    //     let mut i = 0;
    //     while i < NUM_ARGS {
    //         // generate a random slot
    //         let areanum = r.random_range(0..NUM_AREAS);
    //         let slabnum;
    //         if areanum == 0 {
    //             slabnum = r.random_range(0..NUM_SLABS);
    //         } else {
    //             slabnum = r.random_range(0..NUM_SLABS_CACHELINEY);
    //         }
    //         let slotnum = r.random_range(0..NUM_SLOTS);
    //         let sl: SlotLocation = SlotLocation {
    //             areanum,
    //             slabnum,
    //             slotnum,
    //         };

    //         // put the random slot's pointer into the test set
    //         reqptrs[i] = unsafe { baseptr_for_testing.add(sl.offset_of_slot()) };

    //         i += 1;
    //     }

    //     b.iter(|| {
    //         let ptr = reqptrs[i % NUM_ARGS];
    //         black_box(slotlocation_of_ptr(baseptr_for_testing, ptr));

    //         i += 1;
    //     });
    // }
}
