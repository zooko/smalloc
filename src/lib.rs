#![feature(pointer_is_aligned_to)]
#![allow(clippy::not_unsafe_ptr_arg_deref)]

#![doc = include_str!("../README.md")]

// Map of the following offsets and sizes:

// 1. Base Pointer -- 4 MiB alignment
// 
// 2. Vars -- 8 B alignment for each var (and automatic page alignment
//    for all vars)
//   a. NUM_SMALL_SLAB_AREAS x Small Slab Vars
//   b. Large Slab Vars
//
// 3. Separate Free Lists -- 16 KiB alignment per free list (and
//    automatic 4 B alignment per entry)
//
// 4. Data slabs region -- 4 MiB alignment
//
//   a. Small Slab Areas Region
//     i. NUM_SMALL_SLAB_AREAS x Small Slab Area -- 16 KiB alignment per area
//       * Small Slabs -- 16 KiB alignment per slab
//
// 5. Large Slabs Region -- 4 MiB alignment
//   a. Large Slabs excluding the huge-slots slab -- 16 KiB alignment
//   b. Large Slabs, the huge-slots slab -- 4 MiB alignment

// This is the largest alignment we can guarantee for data slots.
const MAX_ALIGNMENT: usize = 2usize.pow(22);

const NUM_SMALL_SLABS: usize = 11;
const NUM_LARGE_SLABS: usize = 10;

const SIZE_OF_HUGE_SLOTS: u32 = 4194304; // 4 * 2^20
const SMALL_SLABNUM_TO_SLOTSIZE: [u8; NUM_SMALL_SLABS] = [1, 2, 3, 4, 5, 6, 8, 9, 10, 16, 32];
const LARGE_SLABNUM_TO_SLOTSIZE: [u32; NUM_LARGE_SLABS] = [64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, SIZE_OF_HUGE_SLOTS];

const fn small_slabnum_to_slotsize(smallslabnum: usize) -> usize {
    debug_assert!(smallslabnum < NUM_SMALL_SLABS);
    SMALL_SLABNUM_TO_SLOTSIZE[smallslabnum] as usize
}

const fn large_slabnum_to_slotsize(largeslabnum: usize) -> usize {
    debug_assert!(largeslabnum < NUM_LARGE_SLABS);
    LARGE_SLABNUM_TO_SLOTSIZE[largeslabnum] as usize
}

// For slabs other than the largest slab:
const NUM_SLOTS_O: usize = 220_000_000;

// For the largest slab:
const NUM_SLOTS_HUGE: usize = 20_000_000;

const fn num_large_slots(largeslabnum: usize) -> usize {
    if largeslabnum == NUM_LARGE_SLABS-1 {
        NUM_SLOTS_HUGE
    } else {
        NUM_SLOTS_O
    }
}

const fn large_slab_alignment(largeslabnum: usize) -> usize {
    if largeslabnum == NUM_LARGE_SLABS-1 {
        SIZE_OF_HUGE_SLOTS as usize
    } else {
        PAGE_SIZE
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
const NUM_SMALL_SLAB_AREAS: usize = 64;

const LARGE_SLABS_VARS_BASE_OFFSET: usize = NUM_SMALL_SLAB_AREAS * NUM_SMALL_SLABS * VARSSIZE;

const VARIABLES_SPACE: usize = LARGE_SLABS_VARS_BASE_OFFSET + NUM_LARGE_SLABS * VARSSIZE;

const fn offset_of_small_flh(areanum: usize, smallslabnum: usize) -> usize {
    (areanum * NUM_SMALL_SLABS + smallslabnum) * VARSSIZE
}

const fn offset_of_large_flh(largeslabnum: usize) -> usize {
    LARGE_SLABS_VARS_BASE_OFFSET + largeslabnum * VARSSIZE
}

const fn offset_of_small_eac(areanum: usize, smallslabnum: usize) -> usize {
    (areanum * NUM_SMALL_SLABS + smallslabnum) * VARSSIZE + DOUBLEWORDSIZE
}

const fn offset_of_large_eac(largeslabnum: usize) -> usize {
    LARGE_SLABS_VARS_BASE_OFFSET + largeslabnum * VARSSIZE + DOUBLEWORDSIZE
}

const CACHELINE_SIZE: usize = 64;

// Align the beginning of the separate free lists region, and the
// beginning of each individual separate free list, to PAGE_SIZE
// alignment in order to minimize having (the in-use part of) the free
// list span more memory pages than necessary. (As well as to make the
// items in the free list, starting with the first one, pack nicely
// into cachelines.)
const SEPARATE_FREELISTS_BASE_OFFSET: usize = VARIABLES_SPACE.next_multiple_of(PAGE_SIZE);

// The calls to next_multiple_of() on a SEPARATE_FREELIST_SPACE are to
// start the *next* separate free list on an alignment boundary.
const SEPARATE_FREELIST_SPACE: usize = (NUM_SLOTS_O * SINGLEWORDSIZE).next_multiple_of(PAGE_SIZE); // Size of each of the separate free lists
const NUM_SEPARATE_FREELISTS: usize = 6; // Number of separate free lists for slabs whose slots are too small to hold a 4-byte-aligned 4-byte word (slab numbers 0, 1, 2, 3, 4, and 5)

const SEPARATE_FREELISTS_SPACE_REGION: usize =
    NUM_SEPARATE_FREELISTS * SEPARATE_FREELIST_SPACE * NUM_SMALL_SLAB_AREAS;

// The beginning of the data slabs region (DATA_SLABS_BASE_OFFSET) is
// aligned to MAX_ALIGNMENT, so that we can conveniently calculate
// alignments, including alignments up to MAX_ALIGNMENT, with offsets
// from the DATA_SLABS_BASE_OFFSET.
const DATA_SLABS_BASE_OFFSET: usize = (SEPARATE_FREELISTS_BASE_OFFSET + SEPARATE_FREELISTS_SPACE_REGION).next_multiple_of(MAX_ALIGNMENT);

const fn gen_lut_sum_small_slab_sizes() -> [usize; NUM_SMALL_SLABS + 1] {
    let mut lut: [usize; NUM_SMALL_SLABS + 1] = [0; NUM_SMALL_SLABS + 1];
    
    let mut slabnum = 0;
    let mut sum: usize = 0;
    while slabnum < NUM_SMALL_SLABS {
        sum += small_slabnum_to_slotsize(slabnum) * NUM_SLOTS_O;
        // Add padding to align the beginning of the next small data
        // slab to PAGE_SIZE alignment, so that the first few slots
        // will touch only one page.
        sum = sum.next_multiple_of(PAGE_SIZE);
        slabnum += 1;
        lut[slabnum] = sum;
    }
    lut
}

const SUM_SMALL_SLAB_SIZES: [usize; NUM_SMALL_SLABS + 1] = gen_lut_sum_small_slab_sizes();

/// The sum of the sizes of the first `numslabs` small slabs. The
/// argument `numslabs` is how many slabs to count the aggregate size
/// of, not the index of the biggest slab. So if `numslabs` is 0 the
/// return value is 0. If `numslabs` is 4, then the return value is
/// the sum of the sizes of slabs 0, 1, 2, and 3. If `numslabs` is 11,
/// then it is the sum of all the small slabs. This also means that
/// the return value is the offset within the data slabs region of the
/// small slab with small slab number `numslabs`.
const fn sum_small_slab_sizes(numslabs: usize) -> usize {
    debug_assert!(numslabs <= NUM_SMALL_SLABS);
    SUM_SMALL_SLAB_SIZES[numslabs]
}

// To account for the fact that the other small slab areas, which
// themselves will consist of small slabs that are aligned, is the
// same size as the first small slab area, we need to pad the
// SMALL_SLAB_AREA_SPACE to the alignment.
const SMALL_SLAB_AREA_SPACE: usize = sum_small_slab_sizes(NUM_SMALL_SLABS).next_multiple_of(PAGE_SIZE);
const SMALL_SLAB_AREAS_REGION_SPACE: usize = SMALL_SLAB_AREA_SPACE * NUM_SMALL_SLAB_AREAS;

// Aligning LARGE_SLAB_REGION_BASE_OFFSET to 4 MiB so that it is easy
// to calculate alignments within the large slab region.
const LARGE_SLAB_REGION_BASE_OFFSET: usize =
    (DATA_SLABS_BASE_OFFSET + SMALL_SLAB_AREAS_REGION_SPACE).next_multiple_of(MAX_ALIGNMENT);

const fn gen_lut_sum_large_slab_sizes() -> [usize; NUM_LARGE_SLABS + 1] {
    let mut lut: [usize; NUM_LARGE_SLABS + 1] = [0; NUM_LARGE_SLABS + 1];

    let mut index = 0;
    let mut sum: usize = 0;
    while index < NUM_LARGE_SLABS {
        sum += large_slabnum_to_slotsize(index) * num_large_slots(index);

        // Add padding to align the beginning of the next large data
        // slab. The non-huge-slots one to PAGE_SIZE alignment, so
        // that the first few slots will touch only one page, and so
        // that each slot will be aligned to its own size. The
        // huge-slots slab is aligned to MAX_ALIGNMENT so that the
        // slots themselves can be aligned to their size.
        if index+1 == NUM_LARGE_SLABS-1 {
            sum = sum.next_multiple_of(SIZE_OF_HUGE_SLOTS as usize);
        } else if index < NUM_LARGE_SLABS-1 {
            sum = sum.next_multiple_of(PAGE_SIZE);
        }

        index += 1;
        lut[index] = sum;
    }
    lut
}

const SUM_LARGE_SLAB_SIZES: [usize; NUM_LARGE_SLABS + 1] = gen_lut_sum_large_slab_sizes();

/// The sum of the sizes of the first `numslabs` large slabs. The
/// argument `numslabs` is how many slabs to count the aggregate size
/// of, not the index of the biggest slab. So if `numslabs` is 0 the
/// return value is 0. If `numslabs` is 4, then the return value is
/// the sum of the sizes of slabs 0, 1, 2, and 3. If `numslabs` is 10,
/// then it is the sum of all the slabs, including the huge-slots
/// slab. This also means that the return value is the offset within
/// the large slabs region of the large slab with large slab number
/// `numslabs`.
const fn sum_large_slab_sizes(numslabs: usize) -> usize {
    debug_assert!(numslabs <= NUM_LARGE_SLABS);
    SUM_LARGE_SLAB_SIZES[numslabs]
}

const LARGE_SLAB_REGION_SPACE: usize = sum_large_slab_sizes(NUM_LARGE_SLABS);

// Pad with an added MAX_ALIGNMENT - 1 bytes so that we can "slide
// forward" the base pointer to the first 4 MiB boundary in order to
// align the base pointer to 4 MiB.
const TOTAL_VIRTUAL_MEMORY: usize =
    LARGE_SLAB_REGION_BASE_OFFSET + LARGE_SLAB_REGION_SPACE + MAX_ALIGNMENT - 1;

use std::cmp::PartialEq;

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
                small_slabnum_to_slotsize(*smallslabnum)
            }
            SlotLocation::LargeSlot { largeslabnum, .. } => {
                large_slabnum_to_slotsize(*largeslabnum)
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
        // If the pointer is before our base pointer or after the end of our allocated space, then it must have come from an oversized alloc where we fell back to `sys_alloc()`. (Assuming that the user code never passes anything to our `dealloc() other a pointer that it previous got from our `alloc()`.)

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
        debug_assert!(p_as_usize >= baseptr_as_usize + DATA_SLABS_BASE_OFFSET);

        // Okay now we know that it is pointer into our allocation, so it is safe to subtract baseptr from it.
        let ioffset = unsafe { ptr.offset_from(baseptr) };
        debug_assert!(ioffset >= 0);
        let offset = ioffset as usize;
        debug_assert!(offset < isize::MAX as usize);
        debug_assert!(offset >= DATA_SLABS_BASE_OFFSET);

        if offset < LARGE_SLAB_REGION_BASE_OFFSET {
            // This points into the "small-slabs-areas-region".
            let withinregionoffset = offset - DATA_SLABS_BASE_OFFSET;
            let areanum = withinregionoffset / SMALL_SLAB_AREA_SPACE;
            let pastareas = areanum * SMALL_SLAB_AREA_SPACE;
            let withinareaoffset = withinregionoffset - pastareas;
            debug_assert!(withinareaoffset < sum_small_slab_sizes(NUM_SMALL_SLABS));

            let mut smallslabnum = NUM_SMALL_SLABS - 1;
            while withinareaoffset < sum_small_slab_sizes(smallslabnum) {
                smallslabnum -= 1;
            }

            // This ptr is within this slab.
            let withinslaboffset = withinareaoffset - sum_small_slab_sizes(smallslabnum);
            let slotsize = small_slabnum_to_slotsize(smallslabnum);
            debug_assert!(withinslaboffset.is_multiple_of(slotsize)); // ptr must point to the beginning of a slot.
            debug_assert!(if slotsize.is_power_of_two() {
                ptr.is_aligned_to(slotsize)
            } else {
                true
            });
            let slotnum = withinslaboffset / slotsize;
            debug_assert!(if slotnum == 0 {
                ioffset as usize % PAGE_SIZE == 0
            } else {
                true
            }, "ioffset: {ioffset}, ptr: {ptr:?}, baseptr: {baseptr:?}");

            debug_assert!(if slotnum == 0 {
                ptr.is_aligned_to(PAGE_SIZE)
            } else {
                true
            }, "ptr: {ptr:?}");

            Some(Self::SmallSlot {
                areanum,
                smallslabnum,
                slotnum,
            })
        } else {
            // This points into the "large-slabs-region".
            let withinregionoffset = offset - LARGE_SLAB_REGION_BASE_OFFSET;

            let mut largeslabnum = 0;
            while largeslabnum < NUM_LARGE_SLABS - 1
                && withinregionoffset >= sum_large_slab_sizes(largeslabnum + 1)
            {
                largeslabnum += 1;
            }
            debug_assert!(largeslabnum < NUM_LARGE_SLABS);
            let slotsize = large_slabnum_to_slotsize(largeslabnum);
            debug_assert!(if slotsize.is_power_of_two() {
                ptr.is_aligned_to(slotsize)
            } else {
                true
            });

            // This ptr is within this slab.
            let withinslaboffset = withinregionoffset - sum_large_slab_sizes(largeslabnum);
            debug_assert!(withinslaboffset.is_multiple_of(slotsize)); // ptr must point to the beginning of a slot.
            let slotnum = withinslaboffset / slotsize;
            debug_assert!(if slotnum == 0 {
                ptr.is_aligned_to(PAGE_SIZE)
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

const fn offset_of_small_slot(areanum: usize, slabnum: usize, slotnum: usize) -> usize {
    debug_assert!(areanum < NUM_SMALL_SLAB_AREAS);
    debug_assert!(slabnum < NUM_SMALL_SLABS);
    debug_assert!(slotnum < NUM_SLOTS_O);

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

const fn offset_of_large_slot(largeslabnum: usize, slotnum: usize) -> usize {
    debug_assert!(largeslabnum < NUM_LARGE_SLABS);
    debug_assert!(slotnum < num_large_slots(largeslabnum));

    let slotsize = large_slabnum_to_slotsize(largeslabnum);

    let mut offset = LARGE_SLAB_REGION_BASE_OFFSET;

    // The beginning of this slab within the large slabs region:
    offset += sum_large_slab_sizes(largeslabnum);

    // The beginning of each large slab is aligned.
    debug_assert!(offset.is_multiple_of(large_slab_alignment(largeslabnum)));

    // Count past the bytes of any earlier slots in this slab:
    offset += slotnum * slotsize;

    // The beginning of each large slot is aligned with its slotsize.
    debug_assert!(offset.is_multiple_of(slotsize));

    offset
}

const fn offset_of_small_free_list_entry(areanum: usize, smallslabnum: usize, slotnum: usize) -> usize {
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
mod platformalloc;
use platformalloc::{sys_alloc, sys_dealloc, sys_realloc};
use platformalloc::vendor::PAGE_SIZE;
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

use thousands::Separable;
use platformalloc::AllocFailed;

impl Smalloc {
    pub const fn new() -> Self {
        Self {
            initlock: AtomicBool::new(false),
            baseptr: AtomicPtr::<u8>::new(null_mut())
        }
    }

    fn idempotent_init(&self) -> Result<*mut u8, AllocFailed> {
        let mut p: *mut u8;

        p = self.baseptr.load(Ordering::Acquire);
        if !p.is_null() {
            return Ok(p);
        }

        //eprintln!("TOTAL_VIRTUAL_MEMORY: {}", TOTAL_VIRTUAL_MEMORY);

        let layout =
            unsafe { Layout::from_size_align_unchecked(TOTAL_VIRTUAL_MEMORY, PAGE_SIZE) };

        // acquire spin lock
        loop {
            if self.initlock.compare_exchange_weak(
                false,
                true,
                Ordering::AcqRel,
                Ordering::Acquire
            ).is_ok() {
                break;
            }
        }

        p = self.baseptr.load(Ordering::Acquire);
        if p.is_null() {
            let sysbaseptr = sys_alloc(layout)?;
            debug_assert!(!sysbaseptr.is_null());

            let addrp = sysbaseptr.addr();
            let alipad = if addrp.is_multiple_of(MAX_ALIGNMENT) {
                0
            } else {
                MAX_ALIGNMENT - (addrp % MAX_ALIGNMENT)
            };

            p = unsafe { sysbaseptr.add(alipad) };
            debug_assert!(p.is_aligned_to(MAX_ALIGNMENT));

            self.baseptr.store(p, Ordering::Release);
        }

        // Release the spin lock
        self.initlock.store(false, Ordering::Release);

        Ok(p)
    }

    fn get_baseptr(&self) -> *mut u8 {
        let p = self.baseptr.load(Ordering::Acquire);
        debug_assert!(!p.is_null());

        p
    }

    /// Pop the head of the free list and return it. Returns 0 if the
    /// free list is empty, or returns the one greater than the index
    /// of the former head of the free list. See "Thread-Safe State
    /// Changes" in README.md
    fn pop_small_flh(&self, areanum: usize, smallslabnum: usize) -> u32 {
        let baseptr = self.get_baseptr();

        let flh = self.get_small_flh(areanum, smallslabnum);
        loop {
            let flhdword: u64 = flh.load(Ordering::Acquire);
            let firstindexplus1: u32 = (flhdword & (u32::MAX as u64)) as u32;
            debug_assert!(firstindexplus1 <= NUM_SLOTS_O as u32);
            debug_assert!(firstindexplus1 as u64 <= self.get_small_eac(areanum, smallslabnum).load(Ordering::Relaxed));

            let counter: u32 = (flhdword >> 32) as u32;
            if firstindexplus1 == 0 {
                // 0 is the sentinel value meaning no next entry, meaning the free list is empty
                return 0;
            };

            let offset_of_next = offset_of_small_free_list_entry(
                areanum,
                smallslabnum,
                (firstindexplus1 - 1) as usize,
            );
            let u8_ptr_to_next = unsafe { baseptr.add(offset_of_next) };
            debug_assert!(u8_ptr_to_next.is_aligned_to(SINGLEWORDSIZE)); // need 4-byte alignment for atomic ops (on at least some/most platforms)
            let u32_ptr_to_next = u8_ptr_to_next.cast::<u32>();
            let nextentry = unsafe { AtomicU32::from_ptr(u32_ptr_to_next) };
            let nextindexplus1: u32 = nextentry.load(Ordering::Acquire);

            let newflhdword = ((counter as u64 + 1) << 32) | nextindexplus1 as u64;

            if flh.compare_exchange_weak(
                flhdword,
                newflhdword,
                Ordering::AcqRel,
                Ordering::Acquire
            ).is_ok() {
                // These constraints must be true considering that the POP succeeded.
                debug_assert!(nextindexplus1 <= NUM_SLOTS_O as u32);
                debug_assert!(nextindexplus1 as u64 <= self.get_small_eac(areanum, smallslabnum).load(Ordering::Relaxed));

                break firstindexplus1
            }
        }
    }

    fn get_small_flh(&self, areanum: usize, smallslabnum: usize) -> &AtomicU64 {
        debug_assert!(areanum < NUM_SMALL_SLAB_AREAS);
        debug_assert!(smallslabnum < NUM_SMALL_SLABS);

        let baseptr = self.get_baseptr();
        let offset_of_flh = offset_of_small_flh(areanum, smallslabnum);
        let u8_ptr_to_flh = unsafe { baseptr.add(offset_of_flh) };
        debug_assert!(u8_ptr_to_flh.is_aligned_to(DOUBLEWORDSIZE)); // need 8-byte alignment for atomic ops (on at least some/most platforms)
        let u64_ptr_to_flh = u8_ptr_to_flh.cast::<u64>();
        unsafe { AtomicU64::from_ptr(u64_ptr_to_flh) }
    }

    fn get_large_flh(&self, largeslabnum: usize) -> &AtomicU64 {
        debug_assert!(largeslabnum < NUM_LARGE_SLABS);

        let baseptr = self.get_baseptr();
        let offset_of_flh = offset_of_large_flh(largeslabnum);
        let u8_ptr_to_flh = unsafe { baseptr.add(offset_of_flh) };
        debug_assert!(u8_ptr_to_flh.is_aligned_to(DOUBLEWORDSIZE)); // need 8-byte alignment for atomic ops (on at least some/most platforms)
        let u64_ptr_to_flh = u8_ptr_to_flh.cast::<u64>();
        let flh = unsafe { AtomicU64::from_ptr(u64_ptr_to_flh) };
        debug_assert!(flh.load(Ordering::Relaxed) & (u32::MAX as u64) <= num_large_slots(largeslabnum) as u64, "{}", flh.load(Ordering::Relaxed));
        flh
    }

    /// Pop the head of the free list and return it. Returns 0 if the
    /// free list is empty, or returns the one greater than the index
    /// of the former head of the free list. See "Thread-Safe State
    /// Changes" in README.md
    fn pop_large_flh(&self, largeslabnum: usize) -> u32 {
        let baseptr = self.get_baseptr();
        let flh = self.get_large_flh(largeslabnum);

        loop {
            let flhdword: u64 = flh.load(Ordering::Acquire);
            let firstindexplus1: u32 = (flhdword & (u32::MAX as u64)) as u32;
            debug_assert!(firstindexplus1 <= num_large_slots(largeslabnum) as u32);
            debug_assert!(firstindexplus1 as u64 <= self.get_large_eac(largeslabnum).load(Ordering::Relaxed));

            let counter: u32 = (flhdword >> 32) as u32;

            if firstindexplus1 == 0 {
                // 0 is the sentinel value meaning no next entry, meaning the free list is empty
                return 0;
            }

            // Intrusive free list -- free list entries are stored in data slots (when they are not in use for data).
            let offset_of_next = offset_of_large_slot(largeslabnum, (firstindexplus1 - 1) as usize);
            let u8_ptr_to_next = unsafe { baseptr.add(offset_of_next) };
            debug_assert!(u8_ptr_to_next.is_aligned_to(SINGLEWORDSIZE)); // need 4-byte alignment for atomic ops (on at least some/most platforms)
            let u32_ptr_to_next = u8_ptr_to_next.cast::<u32>();
            let nextentry = unsafe { AtomicU32::from_ptr(u32_ptr_to_next) };
            let nextindexplus1: u32 = nextentry.load(Ordering::Acquire);

            let newflhdword = ((counter as u64 + 1) << 32) | nextindexplus1 as u64;

            if flh.compare_exchange_weak(
                flhdword,
                newflhdword,
                Ordering::AcqRel, // AcqRel
                Ordering::Acquire, // Acquire
            ).is_ok() {
                // These constraints must be true considering that the POP succeeded.
                debug_assert!(nextindexplus1 <= num_large_slots(largeslabnum) as u32);
                debug_assert!(nextindexplus1 as u64 <= self.get_large_eac(largeslabnum).load(Ordering::Relaxed));

                break firstindexplus1
            }
        }
    }

    // xxx maxindex is just for assertion checks
    fn inner_push_flh(
        &self,
        flh: &AtomicU64,
        offset_of_new: usize,
        new_index: u32,
        maxindex: u32
    ) {
        let baseptr = self.get_baseptr();

        let u8_ptr_to_new = unsafe { baseptr.add(offset_of_new) };
        debug_assert!(u8_ptr_to_new.is_aligned_to(SINGLEWORDSIZE)); // need 4-byte alignment for atomic ops (on at least some/most platforms)
        let u32_ptr_to_new: *mut u32 = u8_ptr_to_new.cast::<u32>();
        let newentry = unsafe { AtomicU32::from_ptr(u32_ptr_to_new) };

        loop {
            let flhdword: u64 = flh.load(Ordering::Acquire);
            let firstindexplus1: u32 = (flhdword & u32::MAX as u64) as u32;
            debug_assert!(firstindexplus1 < maxindex + 1);
            let counter: u32 = (flhdword >> 32) as u32;

            newentry.store(firstindexplus1, Ordering::Release);

            let newflhdword = ((counter as u64 + 1) << 32) | (new_index+1) as u64;

            if flh.compare_exchange_weak(
                flhdword,
                newflhdword,
                Ordering::AcqRel, // AcqRel
                Ordering::Acquire, // Acquire
            ).is_ok() {
                break;
            }
        }
    }

    fn push_flh(&self, newsl: &SlotLocation) {
        match newsl {
            SlotLocation::SmallSlot {
                areanum,
                smallslabnum,
                slotnum,
            } => {
                debug_assert!(*slotnum < NUM_SLOTS_O);

                self.inner_push_flh(
                    self.get_small_flh(*areanum, *smallslabnum),
                    offset_of_small_free_list_entry(*areanum, *smallslabnum, *slotnum),
                    *slotnum as u32,
                    NUM_SLOTS_O as u32
                );
            }
            SlotLocation::LargeSlot {
                largeslabnum,
                slotnum,
            } => {
                debug_assert!(*slotnum < num_large_slots(*largeslabnum));

                // Intrusive free list -- the free list entry is stored in the data slot.
                self.inner_push_flh(
                    self.get_large_flh(*largeslabnum),
                    offset_of_large_slot(*largeslabnum, *slotnum),
                    *slotnum as u32,
                    num_large_slots(*largeslabnum) as u32
                );
            }
        }
    }

    fn get_small_eac(&self, areanum: usize, smallslabnum: usize) -> &AtomicU64 {
        debug_assert!(areanum < NUM_SMALL_SLAB_AREAS);
        debug_assert!(smallslabnum < NUM_SMALL_SLABS);

        let baseptr = self.get_baseptr();
        let offset_of_eac = offset_of_small_eac(areanum, smallslabnum);
        let u8_ptr_to_eac = unsafe { baseptr.add(offset_of_eac) };
        debug_assert!(u8_ptr_to_eac.is_aligned_to(DOUBLEWORDSIZE)); // need 8-byte alignment for atomic ops (on at least some/most platforms)
        let u64_ptr_to_eac = u8_ptr_to_eac.cast::<u64>();
        unsafe { AtomicU64::from_ptr(u64_ptr_to_eac) }
    }

    fn get_large_eac(&self, largeslabnum: usize) -> &AtomicU64 {
        debug_assert!(largeslabnum < NUM_LARGE_SLABS);

        let baseptr = self.get_baseptr();
        let offset_of_eac = offset_of_large_eac(largeslabnum);
        let u8_ptr_to_eac = unsafe { baseptr.add(offset_of_eac) };
        debug_assert!(u8_ptr_to_eac.is_aligned_to(DOUBLEWORDSIZE)); // need 8-byte alignment for atomic ops (on at least some/most platforms)
        let u64_ptr_to_eac = u8_ptr_to_eac.cast::<u64>();
        unsafe { AtomicU64::from_ptr(u64_ptr_to_eac) }
    }

    /// Increment the count of ever-allocated-slots (which is the same
    /// as the index of the next never-before-allocated slot). Return
    /// the number before the increment, which is the index of the
    /// next slot you should use. In the case that all slots have been
    /// allocated, return the max number of slots (which is 1 greater
    /// than the maximum slot number).
    fn increment_eac(&self, eac: &AtomicU64, maxnumslots: usize) -> usize {
        let nexteac = eac.fetch_add(1, Ordering::Relaxed);
        if nexteac as usize <= maxnumslots {
            nexteac as usize
        } else {
            if nexteac as usize > maxnumslots + 100000 {
                // If eac is maxed out -- at maxnumslots -- another thread has incremented past NUM_SLOTS but not yet decremented it, then this could exceed maxnumslots. However, if this has happened many, many times simultaneously, such that eac is more than a small number higher than maxnuslots, then something is wrong and we should panic to prevent some kind of unknown failure case or exploitation.
                panic!("the Ever-Allocated-Counter exceeded max slots + 100000");
            }
            
            // xxx add unit test that eac gets correctly decremented when the thing is full
            eac.fetch_sub(1, Ordering::Relaxed);
            
            maxnumslots
        }
    }

    /// Overflowers algorithm -- see README.md for details and rationale.
    fn search_for_small_overflow_anywhere(&self, startareanum: usize, startsmallslabnum: usize) -> Option<SlotLocation> {
        let mut smallslabnum = startsmallslabnum;

        while smallslabnum < NUM_SMALL_SLABS {
            let osl = self.search_for_small_overflow_area_this_slabnum(startareanum, smallslabnum);
            
            if osl.is_some() {
                return osl;
            }

            smallslabnum += 1;
        }

        // All small slabs in all areas (that could fit this
        // allocation) are full!?!?! Overflow to the large slabs:
        self.large_alloc_with_overflow(0)
    }

    /// Allocate a large slot starting at `startlargeslabnum` and
    /// overflowing to larger-slot slabs if necessary.
    fn large_alloc_with_overflow(&self, startlargeslabnum: usize) -> Option<SlotLocation> {
        let mut largeslabnum = startlargeslabnum;

        while largeslabnum < NUM_LARGE_SLABS {
            let osl = self.inner_large_alloc(largeslabnum);
            if osl.is_some() {
                return osl;
            }

            largeslabnum += 1
        }

        // All the large slabs are full !?!
        None
    }

    /// Search for another area that has a good slot in the same slab number.
    fn search_for_small_overflow_area_this_slabnum(&self, startareanum: usize, smallslabnum: usize) -> Option<SlotLocation> {
        //  These two local variables hold the best candidate we've
        //  found so far (or default values if we haven't yet found
        //  one).
        let mut besteacnum = NUM_SLOTS_O as u64;
        let mut bestareanum = 0;

        let mut nextareanum = (startareanum + 31) % NUM_SMALL_SLAB_AREAS;

        while nextareanum != startareanum {
            let next_eac = self.get_small_eac(nextareanum, smallslabnum);
            let loaded_eacnum = next_eac.load(Ordering::Relaxed);

            if loaded_eacnum < besteacnum {
                // Okay this area is a candidate for our new home.

                // Increment this eac... (which reserves the slot and
                // returns the reserved slot number)
                let inced_eacnum = self.increment_eac(next_eac, NUM_SLOTS_O);
                if inced_eacnum != loaded_eacnum as usize {
                    // Whoops nevermind. Some other thread incremented
                    // the eac between when we loaded loaded_eacnum,
                    // above, and when we incremented the eac. So this
                    // is definitely not a good candidate for our
                    // thread's new home since another thread is
                    // currently using it. Now we have to push this
                    // slot onto this slab's free list and then move
                    // on with our search.
                    self.inner_push_flh(
                        self.get_small_flh(nextareanum, smallslabnum),
                        offset_of_small_free_list_entry(nextareanum, smallslabnum, inced_eacnum),
                        inced_eacnum as u32,
                        NUM_SLOTS_O as u32
                    );
                } else {
                    // Now if inced_eacnum is 0, then we're done with this
                    // search, successfully.
                    if inced_eacnum == 0 {
                        // If we had reserved a previous best
                        // candidate then we need to push it onto that
                        // slabs free list before we proceed.
                        if besteacnum != NUM_SLOTS_O as u64 {
                            self.inner_push_flh(
                                self.get_small_flh(bestareanum, smallslabnum),
                                offset_of_small_free_list_entry(bestareanum, smallslabnum, besteacnum as usize),
                                besteacnum as u32,
                                NUM_SLOTS_O as u32
                            );
                        }
                        
                        // Update our THREAD_AREANUM to point to this
                        // area from now on.
                        set_thread_areanum(nextareanum);

                        return Some(SlotLocation::SmallSlot {
                            areanum: nextareanum,
                            smallslabnum,
                            slotnum: 0
                        });
                    } else {
                        // Okay this isn't necessarily our new home,
                        // because its eac wasn't 0, but at least its
                        // eac was smaller than any other that we've
                        // seen so far, so remember it (by keeping it
                        // in the bestareanum and besteacnum local
                        // variables) and continue with the search.

                        // If we previously had a best candidate, then
                        // we need to push the slot that we thus
                        // reserved onto its free list before we
                        // continue the search, because now this slot
                        // is now replacing it as our current best
                        // candidate.
                        if besteacnum != NUM_SLOTS_O as u64 {
                            self.inner_push_flh(
                                self.get_small_flh(bestareanum, smallslabnum),
                                offset_of_small_free_list_entry(bestareanum, smallslabnum, besteacnum as usize),
                                besteacnum as u32,
                                NUM_SLOTS_O as u32
                            );
                        }

                        bestareanum = nextareanum;
                        besteacnum = inced_eacnum as u64;
                    }
                }
            }

            nextareanum = (nextareanum + 31) % NUM_SMALL_SLAB_AREAS;
        }

        // Okay, since the while loop exited, this means we've visited
        // all areas without finding a slot 0 and returning it. If
        // besteacnum is not NUM_SLOTS_O, then bestareanum contains
        // the best candidate area that we found, and besteacnum
        // contains the slot that we already reserved in that area.
        if besteacnum != NUM_SLOTS_O as u64 {
            // Update our THREAD_AREANUM to point to this area from
            // now on.
            set_thread_areanum(bestareanum);

            Some(SlotLocation::SmallSlot {
                areanum: bestareanum,
                smallslabnum,
                slotnum: besteacnum as usize
            })
        } else {
            None // all slots of this slabnum in all areas are full or in-use!
        }
    }

    /// Allocate a slot by popping the free-list-head, if possible,
    /// else incrementing the ever-allocated-counter. Overflows to
    /// other slabs until it finds one that can satisfy the
    /// request. Returns the resulting SlotLocation or None if none
    /// can be found (meaning all the possible slots are full).
    fn small_alloc_with_overflow(&self, areanum: usize, smallslabnum: usize) -> Option<SlotLocation> {
        let flhplus1 = self.pop_small_flh(areanum, smallslabnum);
        if flhplus1 != 0 {
            let sl = SlotLocation::SmallSlot {
                areanum,
                smallslabnum,
                slotnum: (flhplus1 - 1) as usize,
            };
            Some(sl)
        } else {
            let eac: usize = self.increment_eac(self.get_small_eac(areanum, smallslabnum), NUM_SLOTS_O);
            if eac < NUM_SLOTS_O {
                let sl = SlotLocation::SmallSlot {
                    areanum,
                    smallslabnum,
                    slotnum: eac,
                };
                Some(sl)
            } else {
                // This slab is full!?
                self.search_for_small_overflow_anywhere(areanum, smallslabnum)
            }
        }
    }

    fn inner_large_alloc(&self, largeslabnum: usize) -> Option<SlotLocation> {
        let flhplus1 = self.pop_large_flh(largeslabnum);
        if flhplus1 != 0 {
            let sl = SlotLocation::LargeSlot {
                largeslabnum,
                slotnum: (flhplus1 - 1) as usize,
            };
            Some(sl)
        } else {
            let eac: usize = self.increment_eac(
                self.get_large_eac(largeslabnum),
                num_large_slots(largeslabnum)
            );
            if eac < num_large_slots(largeslabnum) {
                let sl = SlotLocation::LargeSlot {
                    largeslabnum,
                    slotnum: eac,
                };
                Some(sl)
            } else {
                // The slab is full!
                None
            }
        }
    }

    /// Returns the newly allocated SlotLocation. if it was able to
    /// allocate a slot, else returns None (in which case
    /// alloc/realloc needs to satisfy the request by falling back to
    /// sys_alloc())
    fn alloc_slot(&self, layout: Layout) -> Option<SlotLocation> {
        let size = layout.size();
        let alignment = layout.align();
        assert!(alignment > 0);
        assert!(
            (alignment & (alignment - 1)) == 0,
            "alignment must be a power of two"
        );

        // Round up size to the nearest multiple of alignment in order to get a slot that is aligned on that size.
        let alignedsize: usize = ((size - 1) | (alignment - 1)) + 1;

        // This way of doing this branch+loop was informed by:
        // 1. Let's branch on small-slot vs large-slot just once and then have two (similar) code paths instead of branching on small-slot vs large-slot multiple times in one code path, and
        // 2. I profiled zebra, which showed that 32B was the most common slot size, and that < 32B was more common than > 32B, and that among > 32B slot sizes, 64B was the most common one...
        if alignedsize <= small_slabnum_to_slotsize(NUM_SMALL_SLABS-1) {
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

            self.small_alloc_with_overflow(get_thread_areanum(), smallslabnum)
        } else if alignedsize <= SIZE_OF_HUGE_SLOTS as usize {
            let mut largeslabnum = 0;
            while large_slabnum_to_slotsize(largeslabnum) < alignedsize {
                largeslabnum += 1;
            }
            assert!(largeslabnum < NUM_LARGE_SLABS);
            
            self.large_alloc_with_overflow(largeslabnum)
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

/// Set this thread's areanum.
fn set_thread_areanum(newareanum: usize) {
    THREAD_AREANUM.with(|cell| {
        cell.set(Some(newareanum as u32))
    })
}

unsafe impl GlobalAlloc for Smalloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match self.idempotent_init() {
            Err(error) => {
                eprintln!("Failed to alloc; underlying error: {error}");
                null_mut()
            }
            Ok(baseptr) => {
                let size = layout.size();
                assert!(size > 0);
                let alignment = layout.align();
                assert!(alignment > 0);
                assert!((alignment & (alignment - 1)) == 0); // alignment must be a power of two

                // Allocate a slot
                match self.alloc_slot(layout) {
                    Some(sl) => {
                        // xxx consider unwrapping this in order to avoid redundantly branching ??
                        let offset = sl.offset();
                        let p = unsafe { baseptr.add(offset) };
                        assert!(if sl.slotsize().is_power_of_two() {
                            p.is_aligned_to(sl.slotsize())
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
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        match SlotLocation::new_from_ptr(self.get_baseptr(), ptr) {
            Some(sl) => {
                self.push_flh(&sl);
            }
            None => {
                // No slot -- this allocation must have come from falling back to `sys_alloc()`.
                sys_dealloc(ptr, layout);
            }
        }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, newsize: usize) -> *mut u8 {
        let oldsize = layout.size();
        debug_assert!(oldsize > 0);
        let oldalignment = layout.align();
        debug_assert!(oldalignment > 0);
        debug_assert!(
            (oldalignment & (oldalignment - 1)) == 0,
            "alignment must be a power of two"
        );
        debug_assert!(newsize > 0);

        let baseptr = self.get_baseptr();

        match SlotLocation::new_from_ptr(baseptr, ptr) {
            Some(cursl) => {
                if newsize <= cursl.slotsize() {
                    // If the new size fits into the current slot, just return the current pointer and we're done.
                    ptr
                } else {
                    // Round up size to the nearest multiple of alignment in order to get a slot that is aligned on that size.
                    let alignednewsize: usize = ((newsize - 1) | (oldalignment - 1)) + 1;

                    debug_assert_eq!(large_slabnum_to_slotsize(0), CACHELINE_SIZE); // The first (0-indexed) slab in the large slots region has slots just big enough to hold one 64-byte cacheline.

                    // The "growers" rule: use the smallest of the following sizes that will fit: 64 B, 4096 B, 16384 B, 4 MiB.
                    let mut new_large_slabnum = NUM_LARGE_SLABS; // invalid value
                    for candidate_new_slabnum in [0, 6, 8, 9] {
                        if alignednewsize <= large_slabnum_to_slotsize(candidate_new_slabnum) {
                            new_large_slabnum = candidate_new_slabnum;
                        }
                    };

                    let mut newptr = null_mut();
                    if new_large_slabnum < NUM_LARGE_SLABS {
                        // Allocate a new slot...
                        if let Some(newsl) = self.large_alloc_with_overflow(new_large_slabnum) {
                            let offset = newsl.offset();
                            let slotsize = newsl.slotsize();
                            let p = unsafe { baseptr.add(offset) };
                            debug_assert!(if slotsize.is_power_of_two() {
                                p.is_aligned_to(newsl.slotsize())
                            } else {
                                true
                            });
                            newptr = p;
                        }
                    }

                    if newptr.is_null() {
                        // Either the request was too large for even the huge slabs, or large_alloc_with_overflow() returned None, meaning that all slabs were full. In either case, fall back to the system allocator.
                        let layout = unsafe { Layout::from_size_align_unchecked(newsize, oldalignment) };
                        newptr = sys_alloc(layout).unwrap();
                    };
                    debug_assert!(newptr.is_aligned_to(oldalignment));

                    // Copy the contents from the old ptr.
                    unsafe {
                        copy_nonoverlapping(ptr, newptr, oldsize);
                    }

                    // Free the old slot
                    self.push_flh(&cursl);

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

#[cfg(target_vendor = "apple")]
pub mod plat {
    use mach_sys::mach_time::{mach_absolute_time, mach_timebase_info};
    use mach_sys::kern_return::KERN_SUCCESS;
    use std::mem::MaybeUninit;
    use thousands::Separable;
    use rand::rngs::StdRng;
    use rand::SeedableRng;
    use rand::Rng;

    pub fn dev_measure_cache_behavior() {
        let mut mmtt: MaybeUninit<mach_timebase_info> = MaybeUninit::uninit();
        let retval = unsafe { mach_timebase_info(mmtt.as_mut_ptr()) };
        assert_eq!(retval, KERN_SUCCESS);
        let mtt = unsafe { mmtt.assume_init() };

        eprintln!("hello world3");
        const BUFSIZ: usize = 1_000_000_000;
        let mut bs: Box<Vec<u8>> = Box::new(Vec::new());
        bs.resize(BUFSIZ, 0);
        eprintln!("hello world4");

        let mut r = StdRng::seed_from_u64(0);
        let mut i = 0;
        while i < bs.len() {
            bs[i] = r.random();
            i += 1;
        }

        let mut stride = 1;
        while stride < 50_000 {
            // Okay now we need to blow out the cache! To do that, we need
            // to touch at least NUM_CACHE_LINES different cache lines
            // that aren't the ones we want to benchmark.

            // These two consts are set for my Apple M4 Max
            const CACHE_LINE_SIZE: usize = 128;
            const CACHE_SIZE: usize = 20 * 2usize.pow(20);
            const MEM_TO_USE: usize = CACHE_SIZE * 127 + 1_000_000;

            let mut blowoutarea: Vec<u8> = vec![0; MEM_TO_USE];
            let mut i = 0;
            while i < MEM_TO_USE {
                blowoutarea[i] = b'9';
                i += CACHE_LINE_SIZE;
            }

            i = 0;
            let start_ticks = unsafe { mach_absolute_time() };
            while i < BUFSIZ {
                let old = bs[i];
                let offset: usize = ((old as usize) << 16) ^ i;
                bs[offset % BUFSIZ] = old + offset as u8;

                i += stride;
            }
            let stop_ticks = unsafe { mach_absolute_time() };

            let steps = BUFSIZ / stride;
            let nanos = (stop_ticks - start_ticks) * (mtt.numer as u64) / (mtt.denom as u64);
            let nanos_per_step = nanos as f64 / steps as f64;

            eprintln!("stride: {:>6}: steps: {:>13}, ticks: {:>9}, nanos: {:>11}, nanos/step: {nanos_per_step}", stride.separate_with_commas(), steps.separate_with_commas(), (stop_ticks - start_ticks).separate_with_commas(), nanos.separate_with_commas());

            stride += 1;
        }
    }
}

#[cfg(target_arch = "x86_64")]
pub mod plat {
    use cpuid;
    use core::arch::x86_64;
    use crate::platformalloc::vendor::{CACHE_SIZE, CACHE_LINE_SIZE};
    use thousands::Separable;

    pub fn dev_measure_cache_behavior() {
        let ofreq = cpuid::clock_frequency();
        assert!(ofreq.is_some());
        let freq_mhz = ofreq.unwrap();

        eprintln!("hello world3");
        const BUFSIZ: usize = 1_000_000_000;
        let mut bs: Box<Vec<u8>> = Box::new(Vec::new());
        bs.resize(BUFSIZ, 0);
        eprintln!("hello world4");

        let mut stride = 1;
        while stride < 50_000 {
            // Okay now we need to blow out the cache! To do that, we need
            // to touch at least NUM_CACHE_LINES different cache lines
            // that aren't the ones we want to benchmark.

            // These two consts are set for my Apple M4 Max
            const CACHE_LINE_SIZE: usize = 128;
            const CACHE_SIZE: usize = 20 * 2usize.pow(20);
            const MEM_TO_USE: usize = CACHE_SIZE * 127 + 1_000_000;

            let mut blowoutarea: Vec<u8> = vec![0; MEM_TO_USE];
            let mut i = 0;
            while i < MEM_TO_USE {
                blowoutarea[i] = b'9';
                i += CACHE_LINE_SIZE;
            }

            i = 0;
            let mut start_aux = 0;
            let start_cycs = unsafe { x86_64::__rdtscp(&mut start_aux) };
            while i < BUFSIZ {
                bs[i] = b'0';

                i += stride;
            }
            let mut stop_aux = 0;
            let stop_cycs = unsafe { x86_64::__rdtscp(&mut stop_aux) };
            assert!(stop_cycs > start_cycs);

            let steps = BUFSIZ / stride;
            let nanos = (stop_cycs - start_cycs) * 1000 / freq_mhz as u64;
            let nanos_per_step = nanos / steps as u64;

            eprintln!("stride: {:>6}: steps: {:>13}, ticks: {:>9}, nanos: {:>11}, nanos/step: {nanos_per_step}", stride.separate_with_commas(), steps.separate_with_commas(), (stop_cycs - start_cycs).separate_with_commas(), nanos.separate_with_commas());

            stride += 1;
        }
    }
}


// I read in the "The Linux Programming Interface" book that glibc's malloc's default size to fall back to system allocation (mmap) -- MMAP_THRESHOLD -- is 128 KiB. But according to https://sourceware.org/glibc/wiki/MallocInternals the threshold is dynamic unless overridden.

// The following are tools I used during development of smalloc, which
// should probably be rm'ed once smalloc is finished. :-)

// On MacOS on Apple M4, I could allocate more than 105 trillion bytes (105,072,079,929,344).
//
// On a linux machine (AMD EPYC 3151) with 32,711,276 bytes RAM, with overcommit=1, the amount I was able to mmap() varied. :-( One time I could mmap() only 95,175,252,639,744 bytes.
// On a linux machine (Intel(R) Xeon(R) CPU E5-2698 v4 @ 2.20GHz) with 4,608,000,000 bytes RAM with overcommit = 0, the amount I was able to mmap() varied. :-( One time I could mmap() only 93,971,598,389,248 bytes.
//
// On a Windows 11 machine in Ubuntu in Windows Subsystem for Linux 2, the amount I was able to mmap() varied. One time I could mmap() only 93,979,814,301,696
// According to https://learn.microsoft.com/en-us/windows/win32/memory/memory-limits-for-windows-releases a 64-bit process can access 128 TiB.
//
// The current settings of smalloc require 92,770,572,173,312 bytes of virtual address space

pub fn dev_find_max_vm_space_allocatable() {
    let mut trysize: usize = 2usize.pow(62);
    let mut lastsuccess = 0;
    let mut lastfailure = trysize;
    let mut bestsuccess = 0;

    loop {
        if lastfailure - lastsuccess <= 1 {
            println!("Done. best success was: {}", bestsuccess.separate_with_commas());
            break;
        }
        //println!("trysize: {}", trysize.separate_with_commas());
        let res_layout = Layout::from_size_align(trysize, PAGE_SIZE);
        match res_layout {
            Ok(layout) => {
                let res_m = sys_alloc(layout);
                match res_m {
                    Ok(m) => {
                        //println!("It worked! m: {:?}, lastsuccess: {}, trysize: {}, lastfailure: {}", m, lastsuccess, trysize, lastfailure);
                        if trysize > bestsuccess {
                            bestsuccess = trysize;
                        }
                        lastsuccess = trysize;
                        trysize = (trysize + lastfailure) / 2;
                        sys_dealloc(m, res_layout.unwrap());
                    }
                    Err(_) => {
                        //println!("It failed! e: {:?}, lastsuccess: {}, trysize: {}, lastfailure: {}", e, lastsuccess, trysize, lastfailure);
                        lastfailure = trysize;
                        trysize = (trysize + lastsuccess) / 2;
                    }
                }
            }
            Err(error) => {
                panic!("Err: {error:?}");
            }
        }
    }
}

use bytesize::ByteSize;

fn conv(size: usize) -> String {
    ByteSize::b(size as u64).to_string_as(true) // true for binary units (KiB, MiB, GiB, etc.)
}

fn convsum(size: usize) -> String {
    let logtwo = size.ilog2();
    format!("{} ({:.3}b)", conv(size), logtwo)
}

pub fn dev_print_virtual_bytes_map() -> usize {
    // See the README.md to understand this layout.

    // See the top of lib.rs for the *real* implementation. This here is just for running cheap experiments and printing out details.

    let totslabs = NUM_SMALL_SLABS * NUM_SMALL_SLAB_AREAS + NUM_LARGE_SLABS;
    println!("totslabs: {totslabs}");

    println!(
        "The virtual memory space for all the variables is {} ({})",
        VARIABLES_SPACE.separate_with_commas(),
        convsum(VARIABLES_SPACE)
    );

    println!(
        "The virtual memory space for the free lists is {} ({})",
        SEPARATE_FREELISTS_SPACE_REGION.separate_with_commas(),
        convsum(SEPARATE_FREELISTS_SPACE_REGION)
    );

    println!("small slabs space");
    println!("{:>5} {:>8} {:>13} {:>16} {:>17}", "slab#", "size", "slots", "space", "areaspace");
    println!("{:>5} {:>8} {:>13} {:>16} {:>17}", "-----", "----", "-----", "-----", "---------");
    // Then the space needed for the data slabs.
    for smallslabnum in 0..NUM_SMALL_SLABS {
        let slotsize = small_slabnum_to_slotsize(smallslabnum);
        println!("{:>5} {:>8} {:>13} {:>16} {:>17}",
                 smallslabnum,
                 slotsize,
                 NUM_SLOTS_O.separate_with_commas(),
                 (slotsize * NUM_SLOTS_O).separate_with_commas(),
                 (slotsize * NUM_SLOTS_O * NUM_SMALL_SLAB_AREAS).separate_with_commas()
        );
    }
    println!(
        "small slabs space: {} ({})",
        SMALL_SLAB_AREAS_REGION_SPACE.separate_with_commas(),
        convsum(SMALL_SLAB_AREAS_REGION_SPACE)
    );

    println!("large slabs space");
    println!("{:>5} {:>8} {:>13} {:>20}", "slab#", "size", "slots", "space");
    println!("{:>5} {:>8} {:>13} {:>20}", "-----", "----", "-----", "-----");
    // Then the space needed for the data slabs.
    for largeslabnum in 0..NUM_LARGE_SLABS-1 {
        let slotsize = large_slabnum_to_slotsize(largeslabnum);
        println!("{:>5} {:>8} {:>13} {:>20}",
                 largeslabnum,
                 slotsize,
                 NUM_SLOTS_O.separate_with_commas(),
                 (slotsize * NUM_SLOTS_O).separate_with_commas()
        );
    }
    let largeslabnum = NUM_LARGE_SLABS-1;
    let slotsize = large_slabnum_to_slotsize(largeslabnum);
    println!("{:>5} {:>8} {:>13} {:>20}",
             largeslabnum,
             slotsize,
             NUM_SLOTS_HUGE.separate_with_commas(),
             (slotsize * NUM_SLOTS_HUGE).separate_with_commas()
    );

    println!(
        "large slabs space: {} ({})",
        LARGE_SLAB_REGION_SPACE.separate_with_commas(),
        convsum(LARGE_SLAB_REGION_SPACE)
    );

    println!(
        "About to try to allocate {} ({}) ({}) bytes...",
        TOTAL_VIRTUAL_MEMORY,
        TOTAL_VIRTUAL_MEMORY.separate_with_commas(),
        convsum(TOTAL_VIRTUAL_MEMORY)
    );
    let res_layout = Layout::from_size_align(TOTAL_VIRTUAL_MEMORY, PAGE_SIZE);
    match res_layout {
        Ok(layout) => {
            let res_m = sys_alloc(layout);
            match res_m {
                Ok(m) => {
                    println!("It worked! m: {m:?}");
                    //println!("ok");
                    1
                }
                Err(e) => {
                    println!("It failed! e: {e:?}");
                    //println!("err");
                    0
                }
            }
        }
        Err(error) => {
            eprintln!("Err: {error:?}");
            2
        }
    }
}

#[cfg(test)]
mod benches {
    use crate::{Smalloc, NUM_SMALL_SLABS, NUM_LARGE_SLABS, NUM_SMALL_SLAB_AREAS, NUM_SLOTS_O, SlotLocation, num_large_slots, get_thread_areanum};

    use rand::{Rng, SeedableRng};
    use rand::rngs::StdRng;
    use std::ptr::null_mut;
    use std::alloc::{GlobalAlloc, Layout};
    use std::hint::black_box;
    use std::time::Duration;

    use criterion::Criterion;
    use crate::platformalloc::vendor::{CACHE_SIZE, CACHE_LINE_SIZE};

    // #[cfg(target_vendor = "apple")]
    // pub mod plat {
    //     use crate::benches::{Criterion, Duration};
    //     use criterion::measurement::plat_apple::MachAbsoluteTimeMeasurement;
    //     pub fn make_criterion() -> Criterion<MachAbsoluteTimeMeasurement> {
    //         Criterion::default().with_measurement(MachAbsoluteTimeMeasurement::default()).sample_size(300).warm_up_time(Duration::new(10, 0)).significance_level(0.0001).confidence_level(0.9999)
    //     }
    // }

    // #[cfg(target_arch = "x86_64")]
    // pub mod plat {
    //     use criterion::measurement::plat_x86_64::RDTSCPMeasurement;
    //     use crate::benches::{Criterion, Duration};
    //     pub fn make_criterion() -> Criterion<RDTSCPMeasurement> {
    //         Criterion::default().with_measurement(RDTSCPMeasurement::default()).sample_size(300).warm_up_time(Duration::new(10, 0)).significance_level(0.0001).confidence_level(0.9999)
    //     }
    // }

    // #[cfg(not(any(target_vendor = "apple", target_arch = "x86_64")))]
    pub mod plat {
        use criterion::Criterion;
        use crate::benches::Duration;
        pub fn make_criterion() -> Criterion {
            Criterion::default().sample_size(300).warm_up_time(Duration::new(10, 0)).significance_level(0.0001).confidence_level(0.9999)
        }
    }
    
    fn randdist_reqsiz(r: &mut StdRng) -> usize {
        // The following distribution was roughly modelled on smalloclog profiling of Zebra.
        let randnum = r.random::<u8>();

        if randnum < 50 {
            r.random_range(1..16)
        } else if randnum < 150 {
            32
        } else if randnum < 200 {
            64
        } else if randnum < 250 {
            r.random_range(65..16384)
        } else {
            4_000_000
        }
    }

    #[test]
    fn pop_small_flh_sep_sn_0_empty() {
        let mut c = plat::make_criterion();

        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        c.bench_function("pop_small_flh_sep_sn_0_empty", |b| b.iter(|| {
            black_box(sm.pop_small_flh(black_box(0), black_box(0)));
        }));
    }

    #[test]
    fn pop_small_flh_intru_sn_0_empty() {
        let mut c = plat::make_criterion();

        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        c.bench_function("pop_small_flh_intru_sn_0_empty", |b| b.iter(|| {
            black_box(sm.pop_small_flh(black_box(0), black_box(6)));
        }));
    }

    #[test]
    fn pop_large_flh_intru_sn_0_empty() {
        let mut c = plat::make_criterion();

        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        c.bench_function("pop_large_flh_intru_sn_0_empty", |b| b.iter(|| {
            black_box(sm.pop_large_flh(black_box(0)));
        }));
    }

    use criterion::BatchSize;
    use std::sync::atomic::Ordering;
    use rand::seq::SliceRandom;

    #[derive(PartialEq)]
    enum DataOrder {
        Sequential, Random
    }
    
    fn help_bench_pop_small_flh_wdata(fnname: &str, smallslabnum: usize, ord: DataOrder, thenwrite: bool) {
        let mut c = plat::make_criterion();

        let gtan1 = get_thread_areanum();

        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        // To prime the pump for the assertion inside setup() that the free list isn't empty.
        sm.push_flh(&sm.small_alloc_with_overflow(gtan1, smallslabnum).unwrap());

        const NUM_ARGS: usize = 16_000;
        let setup = || {
            let gtan2 = get_thread_areanum();
            assert_eq!(gtan1, gtan2);

            // reset the free list and eac
            let eac = sm.get_small_eac(gtan2, smallslabnum);
            eac.store(0, Ordering::Relaxed);
            let flh = sm.get_small_flh(gtan2, smallslabnum);

            // assert that the free list hasnt't been emptied out,
            // which would mean that during the previous batch of
            // benchmarking, the free list ran dry and we started
            // benchmarking the "pop from empty free list" case
            // instead of what we're trying to benchmark here.
            assert_ne!(flh.load(Ordering::Relaxed) & u32::MAX as u64, 0);

            flh.store(0, Ordering::Relaxed);
            
            let mut sls = Vec::with_capacity(NUM_ARGS);

            while sls.len() < NUM_ARGS {
                let sl = sm.small_alloc_with_overflow(gtan2, smallslabnum).unwrap();
                sls.push(sl)
            }

            match ord {
                DataOrder::Sequential => { }
                DataOrder::Random => {
                    let mut r = StdRng::seed_from_u64(0);
                    sls.shuffle(&mut r)
                }
            }

            for sl in sls.iter() {
                sm.push_flh(sl);
            }
        };

        let f = |()| {
            let gtan3 = get_thread_areanum();
            assert_eq!(gtan1, gtan3);
            let slotnump1 = black_box(sm.pop_small_flh(gtan3, black_box(smallslabnum)));
            assert_ne!(slotnump1, 0);

            // Okay now write into the newly allocated space.
            let slotnum = (slotnump1 - 1) as usize;
            let sl = SlotLocation::SmallSlot{ areanum: gtan3, smallslabnum, slotnum };
            let p = unsafe { sm.get_baseptr().add(sl.offset()) };

            if thenwrite {
                unsafe { std::ptr::copy_nonoverlapping(&99_u8, p, 1) };
            }
        };

        c.bench_function(fnname, move |b| b.iter_batched(setup, f, BatchSize::SmallInput));
    }

    #[test]
    fn pop_small_flh_sep_wdata_sequential() {
        help_bench_pop_small_flh_wdata("pop_small_flh_sep_wdata_sequential", 0, DataOrder::Sequential, false)
    }

    #[test]
    fn pop_small_flh_sep_wdata_sequential_then_write() {
        help_bench_pop_small_flh_wdata("pop_small_flh_sep_wdata_sequential_then_write", 0, DataOrder::Sequential, true)
    }

    #[test]
    fn pop_small_flh_sep_wdata_random() {
        help_bench_pop_small_flh_wdata("pop_small_flh_sep_wdata_random", 0, DataOrder::Random, false)
    }

    #[test]
    fn pop_small_flh_sep_wdata_random_then_write() {
        help_bench_pop_small_flh_wdata("pop_small_flh_sep_wdata_random_then_write", 0, DataOrder::Random, true)
    }

    #[test]
    fn pop_small_flh_intru_sn_6_wdata_sequential() {
        help_bench_pop_small_flh_wdata("pop_small_flh_intru_sn_6_wdata_sequential", 6, DataOrder::Sequential, false)
    }

    #[test]
    fn pop_small_flh_intru_sn_6_wdata_sequential_then_write() {
        help_bench_pop_small_flh_wdata("pop_small_flh_intru_sn_6_wdata_sequential_then_write", 6, DataOrder::Sequential, true)
    }

    #[test]
    fn pop_small_flh_intru_sn_6_wdata_random() {
        help_bench_pop_small_flh_wdata("pop_small_flh_intru_sn_6_wdata_random", 6, DataOrder::Random, false)
    }

    #[test]
    fn pop_small_flh_intru_sn_6_wdata_random_then_write() {
        help_bench_pop_small_flh_wdata("pop_small_flh_intru_sn_6_wdata_random_then_write", 6, DataOrder::Random, true)
    }

    #[test]
    fn pop_small_flh_intru_sn_10_wdata_sequential() {
        help_bench_pop_small_flh_wdata("pop_small_flh_intru_sn_10_wdata_sequential", 10, DataOrder::Sequential, false)
    }

    #[test]
    fn pop_small_flh_intru_sn_10_wdata_sequential_then_write() {
        help_bench_pop_small_flh_wdata("pop_small_flh_intru_sn_10_wdata_sequential_then_write", 10, DataOrder::Sequential, true)
    }

    #[test]
    fn pop_small_flh_intru_sn_10_wdata_random() {
        help_bench_pop_small_flh_wdata("pop_small_flh_intru_sn_10_wdata_random", 10, DataOrder::Random, false)
    }

    #[test]
    fn pop_small_flh_intru_sn_10_wdata_random_then_write() {
        help_bench_pop_small_flh_wdata("pop_small_flh_intru_sn_10_wdata_random_then_write", 10, DataOrder::Random, true)
    }

    fn help_bench_pop_large_flh_wdata(fnname: &str, largeslabnum: usize, ord: DataOrder, thenwrite: bool) {
        let mut c = plat::make_criterion();

        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        // To prime the pump for the assertion inside setup() that the free list isn't empty.
        sm.push_flh(&sm.large_alloc_with_overflow(largeslabnum).unwrap());

        let router = RefCell::new(StdRng::seed_from_u64(0));

        const NUM_ARGS: usize = 16_000;
        let setup = || {
            let mut rinner = router.borrow_mut();

            // reset the free list and eac
            let eac = sm.get_large_eac(largeslabnum);
            eac.store(0, Ordering::Relaxed);
            let flh = sm.get_large_flh(largeslabnum);

            // assert that the free list hasnt't been emptied out,
            // which would mean that during the previous batch of
            // benchmarking, the free list ran dry and we started
            // benchmarking the "pop from empty free list" case
            // instead of what we're trying to benchmark here.
            assert_ne!(flh.load(Ordering::Relaxed) & u32::MAX as u64, 0);

            flh.store(0, Ordering::Relaxed);
            
            let mut sls = Vec::with_capacity(NUM_ARGS);

            while sls.len() < NUM_ARGS {
                let sl = sm.large_alloc_with_overflow(largeslabnum).unwrap();
                sls.push(sl)
            }

            match ord {
                DataOrder::Sequential => { }
                DataOrder::Random => {
                    sls.shuffle(&mut rinner)
                }
            }

            for sl in sls.iter() {
                sm.push_flh(sl);
            }
        };

        let f = |()| {
            let slotnump1 = black_box(sm.pop_large_flh(black_box(largeslabnum)));
            assert_ne!(slotnump1, 0);

            // Okay now write into the newly allocated space.
            let slotnum = (slotnump1 - 1) as usize;
            let sl = SlotLocation::LargeSlot{ largeslabnum, slotnum };
            let p = unsafe { sm.get_baseptr().add(sl.offset()) };

            if thenwrite {
                unsafe { std::ptr::copy_nonoverlapping(&99_u8, p, 1) };
            }
        };

        c.bench_function(fnname, |b| b.iter_batched(setup, f, BatchSize::SmallInput));
    }

    #[test]
    fn pop_large_flh_intru_sn_0_wdata_random() {
        help_bench_pop_large_flh_wdata("pop_large_flh_intru_sn_0_wdata_random", 0, DataOrder::Random, false);
    }

    #[test]
    fn pop_large_flh_intru_sn_0_wdata_random_then_write() {
        help_bench_pop_large_flh_wdata("pop_large_flh_intru_sn_0_wdata_random_then_write", 0, DataOrder::Random, true);
    }

    #[test]
    fn pop_large_flh_intru_sn_0_wdata_sequential() {
        help_bench_pop_large_flh_wdata("pop_large_flh_intru_sn_0_wdata_sequential", 0, DataOrder::Sequential, false);
    }

    #[test]
    fn pop_large_flh_intru_sn_0_wdata_sequential_then_write() {
        help_bench_pop_large_flh_wdata("pop_large_flh_intru_sn_0_wdata_sequential_then_write", 0, DataOrder::Sequential, true);
    }

    #[test]
    fn pop_large_flh_intru_sn_8_wdata_random() {
        help_bench_pop_large_flh_wdata("pop_large_flh_intru_sn_8_wdata_random", 8, DataOrder::Random, false);
    }

    #[test]
    fn pop_large_flh_intru_sn_8_wdata_random_then_write() {
        help_bench_pop_large_flh_wdata("pop_large_flh_intru_sn_8_wdata_random_then_write", 8, DataOrder::Random, true);
    }

    #[test]
    fn pop_large_flh_intru_sn_8_wdata_sequential() {
        help_bench_pop_large_flh_wdata("pop_large_flh_intru_sn_8_wdata_sequential", 8, DataOrder::Sequential, false);
    }

    #[test]
    fn pop_large_flh_intru_sn_8_wdata_sequential_then_write() {
        help_bench_pop_large_flh_wdata("pop_large_flh_intru_sn_8_wdata_sequential_then_write", 8, DataOrder::Sequential, true);
    }

    #[test]
    fn pop_large_flh_intru_sn_9_wdata_random() {
        help_bench_pop_large_flh_wdata("pop_large_flh_intru_sn_9_wdata_random", 9, DataOrder::Random, false);
    }

    #[test]
    fn pop_large_flh_intru_sn_9_wdata_random_then_write() {
        help_bench_pop_large_flh_wdata("pop_large_flh_intru_sn_9_wdata_random_then_write", 9, DataOrder::Random, true);
    }

    #[test]
    fn pop_large_flh_intru_sn_9_wdata_sequential() {
        help_bench_pop_large_flh_wdata("pop_large_flh_intru_sn_9_wdata_sequential", 9, DataOrder::Sequential, false);
    }

    #[test]
    fn pop_large_flh_intru_sn_9_wdata_sequential_then_write() {
        help_bench_pop_large_flh_wdata("pop_large_flh_intru_sn_9_wdata_sequential_then_write", 9, DataOrder::Sequential, true);
    }

    #[test]
    fn small_alloc_with_overflow() {
        let mut c = Criterion::default();

        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        const NUM_ARGS: usize = 50_000;

        let mut r = StdRng::seed_from_u64(0);
        let mut reqs = Vec::with_capacity(NUM_ARGS);

        while reqs.len() < NUM_ARGS {
            reqs.push(r.random_range(0..NUM_SMALL_SLABS));
        }

        let mut i = 0;
        c.bench_function("small_alloc_with_overflow", |b| b.iter(|| {
            black_box(sm.small_alloc_with_overflow(0, black_box(reqs[i % reqs.len()])));
            i += 1;
        }));
    }

    #[test]
    fn inner_large_alloc() {
        let mut c = Criterion::default();

        const NUM_ARGS: usize = 50_000;

        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let mut r = StdRng::seed_from_u64(0);
        let mut reqs = Vec::with_capacity(NUM_ARGS);

        while reqs.len() < NUM_ARGS {
            reqs.push(r.random_range(0..NUM_LARGE_SLABS));
        }

        let mut i = 0;
        c.bench_function("inner_large_alloc", |b| b.iter(|| {
            black_box(sm.inner_large_alloc(black_box(reqs[i % reqs.len()])));
            i += 1
        }));
    }

    #[test]
    fn new_from_ptr() {
        let mut c = Criterion::default();

        const NUM_ARGS: usize = 50_000_000;

        let mut r = StdRng::seed_from_u64(0);
        let baseptr_for_testing: *mut u8 = null_mut();
        let mut reqptrs = Box::new(Vec::new());
        reqptrs.reserve(NUM_ARGS);
        
        while reqptrs.len() < NUM_ARGS {
            // generate a random slot
            let sl = if r.random::<bool>() {
                // SmallSlot
                let areanum = r.random_range(0..NUM_SMALL_SLAB_AREAS);
                let smallslabnum = r.random_range(0..NUM_SMALL_SLABS);
                let slotnum = r.random_range(0..NUM_SLOTS_O);

                SlotLocation::SmallSlot { areanum, smallslabnum, slotnum }
            } else {
                // LargeSlot
                let largeslabnum = r.random_range(0..NUM_LARGE_SLABS);
                let slotnum = r.random_range(0..num_large_slots(largeslabnum));

                SlotLocation::LargeSlot { largeslabnum, slotnum }
            };

            // put the random slot's pointer into the test set
            reqptrs.push(unsafe { baseptr_for_testing.add(sl.offset()) });
        }

        let mut i = 0;
        c.bench_function("new_from_ptr", |b| b.iter(|| {
            let ptr = reqptrs[i % NUM_ARGS];
            black_box(SlotLocation::new_from_ptr(baseptr_for_testing, black_box(ptr)));
            i += 1;
        }));
    }

    #[test]
    pub fn alloc() {
        let mut c = plat::make_criterion();

        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let saved_thread_areanum = get_thread_areanum();
        let r = RefCell::new(StdRng::seed_from_u64(0));

        const NUM_ARGS: usize = 20_000_000;
        let reqsouter = RefCell::new(Vec::with_capacity(NUM_ARGS));

        let setup = || {
            assert_eq!(get_thread_areanum(), saved_thread_areanum);
            let mut reqsinnersetup = reqsouter.borrow_mut();
            
            let mut rinner = r.borrow_mut();

            // reset the free lists and eacs
            for smallslabnum in 0..NUM_SMALL_SLABS {
                let eac = sm.get_small_eac(get_thread_areanum(), smallslabnum);
                eac.store(0, Ordering::Relaxed);
                let flh = sm.get_small_flh(get_thread_areanum(), smallslabnum);
                flh.store(0, Ordering::Relaxed);
            }

            for largeslabnum in 0..NUM_LARGE_SLABS {
                let eac = sm.get_large_eac(largeslabnum);
                eac.store(0, Ordering::Relaxed);
                let flh = sm.get_large_flh(largeslabnum);
                flh.store(0, Ordering::Relaxed);
            }
            
            while reqsinnersetup.len() < NUM_ARGS {
                let l: Layout = Layout::from_size_align(randdist_reqsiz(&mut rinner), 1).unwrap();
                reqsinnersetup.push(l);
            }
        };

        let f = |()| {
            let mut reqsinnerf = reqsouter.borrow_mut();
            let l = reqsinnerf.pop().unwrap();
            unsafe { sm.alloc(l) };
        };

        c.bench_function("alloc", |b| b.iter_batched(setup, f, BatchSize::SmallInput));
    }

    use std::cell::RefCell;
    #[test]
    fn dealloc() {
        let mut c = plat::make_criterion();

        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let saved_thread_areanum = get_thread_areanum();
        let router = RefCell::new(StdRng::seed_from_u64(0));

        const NUM_ARGS: usize = 15_000;
        let allocsouter = RefCell::new(Vec::with_capacity(NUM_ARGS));

        let setup = || {
            let mut rinner = router.borrow_mut();
            let mut allocsinnersetup = allocsouter.borrow_mut();

            assert_eq!(get_thread_areanum(), saved_thread_areanum);

            // reset the free lists and eacs
            for smallslabnum in 0..NUM_SMALL_SLABS {
                let eac = sm.get_small_eac(get_thread_areanum(), smallslabnum);
                eac.store(0, Ordering::Relaxed);
                let flh = sm.get_small_flh(get_thread_areanum(), smallslabnum);
                flh.store(0, Ordering::Relaxed);
            }

            for largeslabnum in 0..NUM_LARGE_SLABS {
                let eac = sm.get_large_eac(largeslabnum);
                eac.store(0, Ordering::Relaxed);
                let flh = sm.get_large_flh(largeslabnum);
                flh.store(0, Ordering::Relaxed);
            }
            
            while allocsinnersetup.len() < NUM_ARGS {
                let l: Layout = Layout::from_size_align(randdist_reqsiz(&mut rinner), 1).unwrap();
                allocsinnersetup.push((unsafe { sm.alloc(l) }, l));
            }

            allocsinnersetup.shuffle(&mut rinner);
        };

        let f = |()| {
            let mut allocsinnerf = allocsouter.borrow_mut();
            let (p, l) = allocsinnerf.pop().unwrap();
            unsafe { sm.dealloc(p, l) };
        };

        c.bench_function("dealloc", |b| b.iter_batched(setup, f, BatchSize::SmallInput));
    }

    #[test]
    fn bench_1_1() {
        help_bench_many_accesses("bench_1_1", 1);
    }

    #[test]
    fn bench_1_2() {
        help_bench_many_accesses("bench_1_2", 2);
    }

    #[test]
    fn bench_1_3() {
        help_bench_many_accesses("bench_1_3", 3);
    }

    #[test]
    fn bench_1_4() {
        help_bench_many_accesses("bench_1_4", 4);
    }

    #[test]
    fn bench_1_5() {
        help_bench_many_accesses("bench_1_5", 5);
    }

    #[test]
    fn bench_1_6() {
        help_bench_many_accesses("bench_1_6", 6);
    }

    #[test]
    fn bench_1_8() {
        help_bench_many_accesses("bench_1_8", 8);
    }

    #[test]
    fn bench_1_9() {
        help_bench_many_accesses("bench_1_9", 9);
    }

    #[test]
    fn bench_1_10() {
        help_bench_many_accesses("bench_1_10", 10);
    }

    #[test]
    fn bench_1_16() {
        help_bench_many_accesses("bench_1_16", 16);
    }

    #[test]
    fn bench_1_32() {
        help_bench_many_accesses("bench_1_32", 32);
    }

    #[test]
    fn bench_1_64() {
        help_bench_many_accesses("bench_1_64", 64);
    }

    #[test]
    fn bench_1_128() {
        help_bench_many_accesses("bench_1_128", 128);
    }

    #[test]
    fn bench_1_256() {
        help_bench_many_accesses("bench_1_256", 256);
    }

    #[test]
    fn bench_1_512() {
        help_bench_many_accesses("bench_1_512", 512);
    }

    #[test]
    fn bench_1_1024() {
        help_bench_many_accesses("bench_1_1024", 1024);
    }

    #[test]
    fn bench_1_2048() {
        help_bench_many_accesses("bench_1_2048", 2048);
    }

    #[test]
    fn bench_1_4096() {
        help_bench_many_accesses("bench_1_4096", 4096);
    }

    #[test]
    fn bench_1_8192() {
        help_bench_many_accesses("bench_1_8192", 8192);
    }

    #[test]
    fn bench_1_16384() {
        help_bench_many_accesses("bench_1_16384", 16384);
    }

    #[test]
    fn bench_1_32768() {
        help_bench_many_accesses("bench_1_32768", 32768);
    }

    use std::slice;
    use gcd::Gcd;
    use std::cmp::min;

    /// This is intended to measure the effect of packing many allocations into few cache lines.
    fn help_bench_many_accesses(fnname: &str, alloc_size: usize) {
        let mut c = plat::make_criterion();

        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        const MEM_TO_USE: usize = CACHE_SIZE * 127 + 1_000_000;
        let max_num_args = (MEM_TO_USE / alloc_size).next_multiple_of(CACHE_LINE_SIZE);
        let max_num_slots = 220_000;
        let num_args = min(max_num_args, max_num_slots);
        
        // Okay now we need a jump which is relatively prime to num_args / CACHE_LINE_SIZE (so that
        // we visit all the allocations in a permutation) and >= 1/2 of (num_args / CACHE_LINE_SIZE)
        // (so that we get away from any linear pre-fetching).
        let x = num_args / CACHE_LINE_SIZE;
        let mut jump = x / 2;
        while x.gcd(jump) != 1 {
            jump += 1;
        }

        let mut r = StdRng::seed_from_u64(0);

        let mut allocs = Vec::with_capacity(num_args);

        let l = Layout::from_size_align(alloc_size, 1).unwrap();
        while allocs.len() < num_args {
            // Allocate CACHE_LINE_SIZE allocations, take their pointers, shuffle the pointers, and
            // append them to allocs.
            let mut batch_of_allocs = Vec::new();
            for _x in 0..CACHE_LINE_SIZE {
                batch_of_allocs.push(unsafe { sm.alloc(l) });
            }
            batch_of_allocs.shuffle(&mut r);
            allocs.extend(batch_of_allocs);
        };
        //        eprintln!("num_args: {}, alloc_size: {}, total alloced: {}, jump: {}", num_args.separate_with_commas(), alloc_size.separate_with_commas(), (alloc_size * num_args).separate_with_commas(), jump.separate_with_commas());

        let mut a = 0;
        let mut i = 0;
        c.bench_function(fnname, |b| b.iter(|| {
            // Now CACHE_LINE_SIZE times in a row we're going to read one byte from the allocation
            // pointed to by each successive pointer. The theory is that when those successive
            // allocations are packed into cache lines, we should be able to do these
            // CACHE_LINE_SIZE reads more quickly than when those successive allocations are spread
            // out over many cache lines.
            
            // get the next pointer
            let x = allocs[i % allocs.len()];

            // read a byte from it
            let b = unsafe { *x };

            // accumulate its value
            a ^= b as usize;

            // go to the next pointer
            i += 1;
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::min;
    use std::sync::Arc;

    const BYTES1: [u8; 8] = [1, 2, 4, 3, 5, 6, 7, 8];
    const BYTES2: [u8; 8] = [9, 8, 7, 6, 5, 4, 3, 2];
    const BYTES3: [u8; 8] = [0xA, 0xB, 0xC, 0xD, 0xE, 0xF, 0x10, 0x11];
    const BYTES4: [u8; 8] = [0x12, 0x11, 0x10, 0xF, 0xE, 0xD, 0xC, 0xB];
    const BYTES5: [u8; 8] = [0xFE, 0xFD, 0xFC, 0xFB, 0xFA, 0xF9, 0xF8, 0xF7];

    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};

    #[test]
    fn offset_of_vars() {
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

    /// Simply generate a Layout and call `alloc_slot()`.
    fn help_alloc_slot(sm: &Smalloc, size: usize, alignment: usize) -> SlotLocation {
        let layout = Layout::from_size_align(size, alignment).unwrap();
        sm.alloc_slot(layout).unwrap()
    }

    #[test]
    fn one_alloc_slot_small() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let layout = Layout::from_size_align(6, 1).unwrap();
        sm.alloc_slot(layout).unwrap();
    }

    #[test]
    fn one_alloc_slot_large() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let layout = Layout::from_size_align(120, 4).unwrap();
        sm.alloc_slot(layout).unwrap();
    }

    /// Make the next test fast and less non-deterministic by poking the eac directly.
    fn _help_set_large_slab_eac(sm: &Smalloc, largeslabnum: usize, new_eac: usize) {
        let eac = sm.get_large_eac(largeslabnum); // slab NUM_LARGE_SLABS-1 holds the biggest (4 MiB slots)
        eac.store(new_eac as u64, Ordering::Relaxed);
    }

    #[test] // commented-out because it takes too long to run
    fn dont_buffer_overrun() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        // Allocate NUM_SLOTS_HUGE of the huge slots, then figure out
        // if the highest-addressed byte in that slot would exceed the
        // TOTAL_VIRTUAL_MEMORY.
        
        let mut i = NUM_SLOTS_HUGE - 4;
        _help_set_large_slab_eac(&sm, NUM_LARGE_SLABS-1, i);

        let siz = 2usize.pow(22);
        let layout = Layout::from_size_align(siz, 1).unwrap();
        let mut highestp: *mut u8 = unsafe { sm.alloc(layout) };
        i += 1;
        while i < NUM_SLOTS_HUGE {
            let p = unsafe { sm.alloc(layout) };
            assert!(p > highestp, "p: {p:?}, highestp: {highestp:?}");
            highestp = p;
            i += 1;
        }

        let highest_addr = highestp.addr() + siz - 1;

        let delta = highest_addr - sm.get_baseptr().addr();
        
        eprintln!("highest_addr: {}, delta: {}, TOTAL_VIRTUAL_MEMORY: {}, TOTAL_VIRTUAL_MEMORY-delta: {}", highest_addr, delta, TOTAL_VIRTUAL_MEMORY, TOTAL_VIRTUAL_MEMORY-delta);
        assert!(delta < TOTAL_VIRTUAL_MEMORY);
    }

    #[test]
    fn one_alloc_slot_huge() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

       let layout = Layout::from_size_align(1000000, 8).unwrap();
        sm.alloc_slot(layout).unwrap();
    }

    #[test]
    fn a_few_allocs_and_a_dealloc_for_each_small_slab() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        for smallslabnum in 0..NUM_SMALL_SLABS {
            help_alloc_slot_small(&sm, smallslabnum);
        }
    }

    #[test]
    fn a_few_allocs_and_a_dealloc_for_each_large_slab() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        for largeslabnum in 0..NUM_LARGE_SLABS {
            help_alloc_slot_large(&sm, largeslabnum);
        }
    }

    /// Generate a number of requests (size+alignment) that fit into
    /// the given large slab and for each request call
    /// help_alloc_slot_four_times_large()
    fn help_alloc_slot_large(sm: &Smalloc, largeslabnum: usize) {
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
                help_alloc_slot_four_times_large(sm, reqsize, reqalign);
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
    /// help_alloc_slot_four_times_small()
    fn help_alloc_slot_small(sm: &Smalloc, smallslabnum: usize) {
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
                help_alloc_slot_four_times_small(sm, reqsize, reqalign);
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
    fn help_alloc_slot_four_times_large(sm: &Smalloc, reqsize: usize, reqalign: usize) {
        let sl1 = help_alloc_slot(sm, reqsize, reqalign);
        let SlotLocation::LargeSlot { largeslabnum: _, slotnum: _, } = sl1 else {
            panic!("should have returned a large slot");
        };

        let sl2 = help_alloc_slot(sm, reqsize, reqalign);
        let SlotLocation::LargeSlot { largeslabnum: _, slotnum: _, } = sl2 else {
            panic!("should have returned a large slot");
        };

        let sl3 = help_alloc_slot(sm, reqsize, reqalign);
        let SlotLocation::LargeSlot { largeslabnum: _, slotnum: _, } = sl3 else {
            panic!("should have returned a large slot");
        };

        // Now free the middle one.
        sm.push_flh(&sl2);

        // And allocate another one.
        let sl4 = help_alloc_slot(sm, reqsize, reqalign);
        let SlotLocation::LargeSlot { largeslabnum: _, slotnum: _, } = sl4 else {
            panic!("should have returned a large slot");
        };
    }

    /// Allocate this size+align three times, then free the middle
    /// one, then allocate a fourth time.
    fn help_alloc_slot_four_times_small(sm: &Smalloc, reqsize: usize, reqalign: usize) {
        let sl1 = help_alloc_slot(sm, reqsize, reqalign);
        let SlotLocation::SmallSlot { areanum: _, smallslabnum: _, slotnum: _, } = sl1 else {
            panic!("should have returned a small slot");
        };

        let sl2 = help_alloc_slot(sm, reqsize, reqalign);
        let SlotLocation::SmallSlot { areanum: _, smallslabnum: _, slotnum: _, } = sl2 else {
            panic!("should have returned a small slot");
        };

        let sl3 = help_alloc_slot(sm, reqsize, reqalign);
        let SlotLocation::SmallSlot { areanum: _, smallslabnum: _, slotnum: _, } = sl3 else {
            panic!("should have returned a small slot");
        };

        // Now free the middle one.
        sm.push_flh(&sl2);

        // And allocate another one.
        let sl4 = help_alloc_slot(sm, reqsize, reqalign);
        let SlotLocation::SmallSlot { areanum: _, smallslabnum: _, slotnum: _, } = sl4 else {
            panic!("should have returned a small slot");
        };
    }

    #[test]
    fn alloc_1_byte_then_dealloc() {
        let sm = Smalloc::new();
        let layout = Layout::from_size_align(1, 1).unwrap();
        let p = unsafe { sm.alloc(layout) };
        unsafe { sm.dealloc(p, layout) };
    }

    #[test]
    fn roundtrip_slot_to_ptr_to_slot() {
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
        for largeslabnum in 0..NUM_LARGE_SLABS-1 {
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
        let largeslabnum = NUM_LARGE_SLABS-1;
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
    fn main_thread_init() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();
    }

    #[test]
    fn one_thread_simple() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let handle1 = thread::spawn(move || {
            for _j in 0..1000 {
                help_alloc_slot_small(&sm, 0);
            }
        });
        
        handle1.join().unwrap();
    }

    #[test]
    fn threads_2_simple() {
        let sm = Arc::new(Smalloc::new());
        sm.idempotent_init().unwrap();

        let sm1 = Arc::clone(&sm);
        let handle1 = thread::spawn(move || {
            for _j in 0..1000 {
                help_alloc_slot_small(&sm1, 0);
            }
        });
        
        let sm2 = Arc::clone(&sm);
        let handle2 = thread::spawn(move || {
            for _j in 0..1000 {
                help_alloc_slot_small(&sm2, 0);
            }
        });

        handle1.join().unwrap();
        handle2.join().unwrap();
    }

    #[test]
    fn threads_12_small() {
        let sm = Arc::new(Smalloc::new());
        sm.idempotent_init().unwrap();

        let mut handles = Vec::new();
        for _i in 0..12 {
            let smc = Arc::clone(&sm);
            handles.push(thread::spawn(move || {
                for _j in 0..1000 {
                    help_alloc_slot_small(&smc, 0);
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn threads_12_large_alloc_dealloc() {
        let sm = Arc::new(Smalloc::new());
        sm.idempotent_init().unwrap();

        let l = Layout::from_size_align(64, 1).unwrap();

        let mut handles = Vec::new();
        for _i in 0..12 {
            let smc = Arc::clone(&sm);
            handles.push(thread::spawn(move || {
                help_many_random_alloc_dealloc(&smc, 1000, l, 0);
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn threads_1000_small() {
        let sm = Arc::new(Smalloc::new());
        sm.idempotent_init().unwrap();

        let mut handles = Vec::new();
        for _i in 0..1000 {
            let smc = Arc::clone(&sm);
            handles.push(thread::spawn(move || {
                for _j in 0..12 {
                    help_alloc_slot_small(&smc, 0);
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    fn __help_n_threads_malloc_dealloc(n: u32, iters: usize, layout: Layout, seed: u64) {
        let sm = Arc::new(Smalloc::new());
        sm.idempotent_init().unwrap();

        let mut handles = Vec::new();
        for _i in 0..n {
            let smc = Arc::clone(&sm);
            handles.push(thread::spawn(move || {
                help_many_random_alloc_dealloc(&smc, iters, layout, seed);
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    fn __help_n_threads_malloc_dealloc_with_writes(n: u32, iters: usize, layout: Layout, seed: u64) {
        let sm = Arc::new(Smalloc::new());
        sm.idempotent_init().unwrap();

        let mut handles = Vec::new();
        for _i in 0..n {
            let smc = Arc::clone(&sm);
            handles.push(thread::spawn(move || {
                help_many_random_alloc_dealloc_with_writes(&smc, iters, layout, seed);
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    fn __help_n_threads_malloc_dealloc_realloc_no_writes(n: u32, iters: usize, layout: Layout, seed: u64) {
        let sm = Arc::new(Smalloc::new());
        sm.idempotent_init().unwrap();

        let mut handles = Vec::new();
        for _i in 0..n {
            let smc = Arc::clone(&sm);
            handles.push(thread::spawn(move || {
                help_many_random_alloc_dealloc_realloc_no_writes(&smc, iters, layout, seed);
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    fn __help_n_threads_malloc_dealloc_realloc_with_writes(n: u32, iters: usize, layout: Layout, seed: u64) {
        let sm = Arc::new(Smalloc::new());
        sm.idempotent_init().unwrap();

        let mut handles = Vec::new();
        for _i in 0..n {
            let smc = Arc::clone(&sm);
            handles.push(thread::spawn(move || {
                help_many_random_alloc_dealloc_realloc_with_writes(&smc, iters, layout, seed);
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn threads_32_large_malloc_dealloc_no_writes() {
        let seed = 0;

        let l = Layout::from_size_align(64, 1).unwrap();

        __help_n_threads_malloc_dealloc(32, 1000, l, seed);
    }

    #[test]
    fn threads_32_large_malloc_dealloc_with_writes() {
        //let mut r = rand::rng();
        //let seed = r.random::<u64>();
        //debugln!("rand seed: {}", seed);
        let seed = 0;
        //debugln!("hardcoded seed: {}", seed);

        let l = Layout::from_size_align(64, 1).unwrap();

        __help_n_threads_malloc_dealloc_with_writes(32, 1000, l, seed);
    }

    #[test]
    fn threads_32_large_malloc_dealloc_realloc_with_writes() {
        //let mut r = rand::rng();
        //let seed = r.random::<u64>();
        //debugln!("rand seed: {}", seed);
        let seed = 0;
        //debugln!("hardcoded seed: {}", seed);

        let l = Layout::from_size_align(64, 1).unwrap();

        __help_n_threads_malloc_dealloc_realloc_with_writes(32, 1000, l, seed);
    }

    #[test]
    fn threads_32_large_malloc_dealloc_realloc_no_writes() {
        //let mut r = rand::rng();
        //let seed = r.random::<u64>();
        //debugln!("rand seed: {}", seed);
        let seed = 0;
        //debugln!("hardcoded seed: {}", seed);

        let l = Layout::from_size_align(64, 1).unwrap();

        __help_n_threads_malloc_dealloc_realloc_no_writes(32, 1000, l, seed);
    }

    #[test]
    fn threads_32_small_malloc_dealloc() {
        //let mut r = rand::rng();
        //let seed = r.random::<u64>();
        //debugln!("rand seed: {}", seed);
        let seed = 0;
        //debugln!("hardcoded seed: {}", seed);

        let l = Layout::from_size_align(4, 1).unwrap();

        __help_n_threads_malloc_dealloc(32, 1000, l, seed);
    }

    #[test]
    fn threads_32_small_malloc_dealloc_with_writes() {
        //let mut r = rand::rng();
        //let seed = r.random::<u64>();
        //debugln!("rand seed: {}", seed);
        let seed = 0;
        //debugln!("hardcoded seed: {}", seed);

        let l = Layout::from_size_align(4, 1).unwrap();

        __help_n_threads_malloc_dealloc_with_writes(32, 1000, l, seed);
    }


    #[test]
    fn threads_1000_large_malloc_dealloc() {
        let l = Layout::from_size_align(64, 1).unwrap();

        __help_n_threads_malloc_dealloc(1000, 1000, l, 0);
    }

    use ahash::HashSet;
    use ahash::RandomState;
    
    fn help_many_random_alloc_dealloc(sm: &Smalloc, iters: usize, layout: Layout, seed: u64) {
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
                    unsafe { sm.dealloc(p, lt) };
                }
            } else {
                // Malloc
                let p = unsafe { sm.alloc(l) };
                assert!(!m.contains(&(p, l)), "thread: {:>3}, {:?} {}-{}", get_thread_areanum(), p, l.size(), l.align());
                m.insert((p, l));
                ps.push((p, l));
            }
        }
    }
    
    fn help_many_random_alloc_dealloc_with_writes(sm: &Smalloc, iters: usize, layout: Layout, seed: u64) {
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
                unsafe { sm.dealloc(p, lt) };

                // Write to a random (other) allocation...
                if !ps.is_empty() {
                    let (po, lto) = ps[r.random_range(0..ps.len())];
                    unsafe { std::ptr::copy_nonoverlapping(BYTES2.as_ptr(), po, min(BYTES2.len(), lto.size())) };
                }
            } else {
                // Malloc
                let p = unsafe { sm.alloc(l) };
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
    fn help_many_random_alloc_dealloc_realloc_no_writes(sm: &Smalloc, iters: usize, layout: Layout, seed: u64) {
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
                    unsafe { sm.dealloc(p, lt) };
                }
            } else if coin == 1 {
                // Malloc
                let lt = ls.choose(&mut r).unwrap();
                let p = unsafe { sm.alloc(*lt) };
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
                    let newp = unsafe { sm.realloc(p, lt, newlt.size()) };

                    assert!(!m.contains(&(newp, *newlt)), "thread: {:>3}, {:?} {}-{}", get_thread_areanum(), newp, newlt.size(), newlt.align());
                    m.insert((newp, *newlt));
                    ps.push((newp, *newlt));
                }
            }
        }
    }
    
    fn help_many_random_alloc_dealloc_realloc_with_writes(sm: &Smalloc, iters: usize, layout: Layout, seed: u64) {
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
                    unsafe { sm.dealloc(p, lt) };

                    // Write to a random (other) allocation...xo
                    if !ps.is_empty() {
                        let (po, lto) = ps[r.random_range(0..ps.len())];
                        unsafe { std::ptr::copy_nonoverlapping(BYTES2.as_ptr(), po, min(BYTES2.len(), lto.size())) };
                    }
                }
            } else if coin == 1 {
                // Malloc
                let lt = ls.choose(&mut r).unwrap();
                let p = unsafe { sm.alloc(*lt) };
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
                    let newp = unsafe { sm.realloc(p, lt, newlt.size()) };

                    assert!(!m.contains(&(newp, *newlt)), "thread: {:>3}, {:?} {}-{}", get_thread_areanum(), newp, newlt.size(), newlt.align());
                    m.insert((newp, *newlt));
                    ps.push((newp, *newlt));

                    // Write to a random allocation...
                    let (po, lto) = ps.choose(&mut r).unwrap();
                    unsafe { std::ptr::copy_nonoverlapping(BYTES5.as_ptr(), *po, min(BYTES5.len(), lto.size())) };
                }
            }
        }
    }
    
    #[test]
    fn large_allocs_deallocs_reallocs_with_writes() {
        let sm = Smalloc::new();
        let l = Layout::from_size_align(64, 1).unwrap();

        help_many_random_alloc_dealloc_realloc_with_writes(&sm, 100_000, l, 0);
    }

    #[test]
    fn large_allocs_deallocs_no_reallocs_no_writes() {
        let sm = Smalloc::new();
        let l = Layout::from_size_align(64, 1).unwrap();

        help_many_random_alloc_dealloc(&sm, 100_000, l, 0);
    }

    #[test]
    fn large_allocs_deallocs_no_reallocs_with_writes() {
        let sm = Smalloc::new();
        let l = Layout::from_size_align(64, 1).unwrap();

        help_many_random_alloc_dealloc_with_writes(&sm, 100_000, l, 0);
    }

    #[test]
    /// If we've allocated all of the slots from a small-slots slab,
    /// the subsequent allocations come from different areas.
    fn overflowers_small() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let l = Layout::from_size_align(8, 1).unwrap();

        // Step 0: reach into the slab's `eac` and set it to almost the max slot number.
        let orig_this_thread_areanum = get_thread_areanum();
        let orig_i = NUM_SLOTS_O - 3;
        let mut i = orig_i;
        let eac = sm.get_small_eac(orig_this_thread_areanum, 6); // slab 6 holds 8-byte things
        eac.store(i as u64, Ordering::Relaxed);

        // Step 1: allocate a slot and store it in a local variable:
        let sl1 = sm.alloc_slot(l).unwrap();
        let areanum1: usize;
        let smallslabnum1: usize;
        let slotnum1: usize;
        
        if let SlotLocation::SmallSlot { areanum, smallslabnum, slotnum } = sl1 {
            areanum1 = areanum;
            smallslabnum1 = smallslabnum;
            slotnum1 = slotnum;
        } else {
            panic!("Should have been a small slot.");
        }
        assert_eq!(areanum1, orig_this_thread_areanum);
        assert_eq!(slotnum1, i);
        i += 1;
        
        // Step 2: allocate all the rest of the slots in this slab except the last one:
        while i < NUM_SLOTS_O - 1 {
            sm.alloc_slot(l).unwrap();

            i += 1
        }

        // Step 3: allocate the last slot in this slab and store it in a local variable:
        let sl2 = sm.alloc_slot(l).unwrap();
        let areanum2: usize;
        let smallslabnum2: usize;
        let slotnum2: usize;

        if let SlotLocation::SmallSlot { areanum, smallslabnum, slotnum } = sl2 {
            areanum2 = areanum;
            smallslabnum2 = smallslabnum;
            slotnum2 = slotnum;
        } else {
            panic!("Should have been a small slot.");
        }

        // Assert some things about the two stored slot locations:
        assert_eq!(areanum1, areanum2);
        assert_eq!(smallslabnum1, smallslabnum2);
        assert_eq!(slotnum1, orig_i);
        assert_eq!(slotnum2, NUM_SLOTS_O - 1);

        // Step 4: Allocate another slot and store it in a local variable:
        let sl3 = sm.alloc_slot(l).unwrap();
        let areanum3: usize;
        let smallslabnum3: usize;

        if let SlotLocation::SmallSlot { areanum, smallslabnum, .. } = sl3 {
            areanum3 = areanum;
            smallslabnum3 = smallslabnum;
        } else {
            panic!("Should have been a small slot.");
        }

        // The reason for this test: Assert that the newly allocated
        // slot is in a different area, same slab:
        assert_ne!(areanum3, areanum1);
        assert_eq!(smallslabnum3, smallslabnum3);

        // Okay now this thread should be pointing at the new thread area num.
        let new_this_thread_areanum = get_thread_areanum();
        assert!(orig_this_thread_areanum != new_this_thread_areanum);
        assert_eq!(new_this_thread_areanum, areanum3);

        // Step 5: If we alloc_slot() again on this thread, it will come from this new area:
        let sl4 = sm.alloc_slot(l).unwrap();
        let areanum4: usize;
        let smallslabnum4: usize;

        if let SlotLocation::SmallSlot { areanum, smallslabnum, .. } = sl4 {
            areanum4 = areanum;
            smallslabnum4 = smallslabnum;
        } else {
            panic!("Should have been a small slot.");
        }

        assert_eq!(smallslabnum4, smallslabnum1);
        assert_eq!(areanum4, new_this_thread_areanum);

        // We've now allocated two slots from this new area:
        let second_area_eac = sm.get_small_eac(new_this_thread_areanum, 6); // slab 6 holds 8-byte things
        let second_area_eac_orig_val = second_area_eac.load(Ordering::Relaxed);
        assert_eq!(second_area_eac_orig_val, 2);

        // Step 6: If we allocate a slot from the *original* area --
        // the full one -- it will overflow but this time exercise
        // different code paths in the overflow logic and different
        // results. We'll assert that the end state is as expected...
        let sl5 = sm.small_alloc_with_overflow(areanum1, 6).unwrap();
        let areanum5: usize;
        let smallslabnum5: usize;

        if let SlotLocation::SmallSlot { areanum, smallslabnum, .. } = sl5 {
            areanum5 = areanum;
            smallslabnum5 = smallslabnum;
        } else {
            panic!("Should have been a small slot.");
        }

        assert_eq!(smallslabnum5, smallslabnum1); // same slabnum
        assert!(areanum5 != areanum1); // It didn't go into the full area.
        assert!(areanum5 != areanum4); // It didn't overflow to the same area the previous overflow did.

        // It landed in the third area.
        // But along the way, it incremented the `eac` of the second area:
        assert_eq!(second_area_eac_orig_val + 1, second_area_eac.load(Ordering::Relaxed));

        // And it pushed that slot onto that slab's free list, so now if we alloc from that slab, this will not increment its eac:
        let sl6 = sm.small_alloc_with_overflow(areanum4, 6).unwrap();
        let areanum6: usize;
        let smallslabnum6: usize;

        if let SlotLocation::SmallSlot { areanum, smallslabnum, .. } = sl6 {
            areanum6 = areanum;
            smallslabnum6 = smallslabnum;
        } else {
            panic!("Should have been a small slot.");
        }

        assert_eq!(areanum6, areanum4);
        assert_eq!(smallslabnum6, smallslabnum1);
        assert_eq!(second_area_eac_orig_val + 1, second_area_eac.load(Ordering::Relaxed));
    }

    #[test]
    /// If we've allocated at least one slot in every small slot slab
    /// area and then we overflow one of them, the overflower
    /// algorithm is going to visit every one of the other slab areas
    /// before picking the one with the lowest eac. (There was a bug
    /// in the code handling this edge case that I just noticed, which
    /// reminded me that there was no test of this edge case.)
    fn overflowers_small_edge_case() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let l = Layout::from_size_align(1, 1).unwrap();

        // Step 0: set each slab area's `eac` to 4, then set our
        // thread's area's `eac` to max and a different area's `eac`
        // to 2.
        for areanum in 0..NUM_SMALL_SLAB_AREAS {
            sm.get_small_eac(areanum, 0).store(4, Ordering::Relaxed);
        }
        let this_tan = get_thread_areanum();
        let target_tan = this_tan + 1;
        sm.get_small_eac(this_tan, 0).store(NUM_SLOTS_O as u64, Ordering::Relaxed);
        sm.get_small_eac(target_tan, 0).store(2, Ordering::Relaxed);

        // Step 1: try to allocate a slot from this area:
        let sl1 = sm.alloc_slot(l).unwrap();
        let areanum1: usize;
        let smallslabnum1: usize;
        let slotnum1: usize;
        
        if let SlotLocation::SmallSlot { areanum, smallslabnum, slotnum } = sl1 {
            areanum1 = areanum;
            smallslabnum1 = smallslabnum;
            slotnum1 = slotnum;
        } else {
            panic!("Should have been a small slot.");
        }
        assert_eq!(areanum1, target_tan, "tan: {}, targ tan: {target_tan}, area: {areanum1}, slab: {smallslabnum1}, slot: {slotnum1}", get_thread_areanum());
        assert_eq!(slotnum1, 2);
        assert_eq!(smallslabnum1, 0);
        assert_eq!(get_thread_areanum(), target_tan);
    }

    #[test]
    /// If we've allocated all of the slots from all of the
    /// small-slots slabs, the subsequent allocations come from a
    /// large-slot slab.
    fn overflowers_small_to_large() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let l = Layout::from_size_align(32, 1).unwrap();

        // Step 0: reach into each area's slab's `eac` and set it to the max slot number.
        for slabareanum in 0..NUM_SMALL_SLAB_AREAS {
            let eac = sm.get_small_eac(slabareanum, 10); // slab 10 holds 32-byte things
            eac.store(NUM_SLOTS_O as u64, Ordering::Relaxed);
        }

        // Step 1: Allocate another slot and store it in a local variable:
        let sl1 = sm.alloc_slot(l).unwrap();
        let largeslabnum1: usize;

        if let SlotLocation::LargeSlot { largeslabnum, .. } = sl1 {
            largeslabnum1 = largeslabnum;
        } else {
            panic!("Should have been a large slot.");
        }

        assert_eq!(largeslabnum1, 0);
    }

    // and for small slots overflowing to large slots
    // and for small slots overflowing to large slots and then overflowing again

    fn help_test_overflowers_large(largeslabnum: usize) {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let l = Layout::from_size_align(large_slabnum_to_slotsize(largeslabnum), 1).unwrap();

        let orig_i = NUM_SLOTS_O - 3;
        let mut i = orig_i;
        _help_set_large_slab_eac(&sm, largeslabnum, i);

        // Step 1: allocate a slot and store it in a local variable:
        let sl1 = sm.alloc_slot(l).unwrap();
        let largeslabnum1: usize;
        let slotnum1: usize;
        
        if let SlotLocation::LargeSlot { largeslabnum, slotnum } = sl1 {
            largeslabnum1 = largeslabnum;
            slotnum1 = slotnum;
        } else {
            panic!("Should have been a large slot.");
        }
        assert_eq!(largeslabnum1, largeslabnum);
        assert_eq!(slotnum1, i);
        i += 1;
        
        // Step 2: allocate all the rest of the slots in this slab except the last one:
        while i < NUM_SLOTS_O - 1 {
            sm.alloc_slot(l).unwrap();
            i += 1
        }

        // Step 3: allocate the last slot in this slab and store it in a local variable:
        let sl2 = sm.alloc_slot(l).unwrap();
        let largeslabnum2: usize;
        let slotnum2: usize;

        if let SlotLocation::LargeSlot { largeslabnum, slotnum } = sl2 {
            largeslabnum2 = largeslabnum;
            slotnum2 = slotnum;
        } else {
            panic!("Should have been a large slot.");
        }

        // Assert some things about the two stored slot locations:
        assert_eq!(largeslabnum1, largeslabnum2);
        assert_eq!(slotnum1, orig_i);
        assert_eq!(slotnum2, NUM_SLOTS_O - 1);

        // Step 4: allocate another slot from this slab and store it in a local variable:
        let sl3 = sm.alloc_slot(l).unwrap();
        let largeslabnum3: usize;

        if let SlotLocation::LargeSlot { largeslabnum, .. } = sl3 {
            largeslabnum3 = largeslabnum;
        } else {
            panic!("Should have been a large slot.");
        }

        // Assert that this alloc overflowed to a different slab.
        assert_ne!(largeslabnum1, largeslabnum3);
        assert_eq!(largeslabnum3, largeslabnum+1);
    }

    #[test]
    /// If the slab you're overflowing to is itself full, overflow to the next slab.
    fn multiple_overflows_large() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let largeslabnum = 0;

        let l = Layout::from_size_align(large_slabnum_to_slotsize(largeslabnum), 1).unwrap();

        let orig_i = NUM_SLOTS_O - 3;
        let mut i = orig_i;
        _help_set_large_slab_eac(&sm, largeslabnum, i);

        // Step 1: allocate a slot and store it in a local variable:
        let sl1 = sm.alloc_slot(l).unwrap();
        let largeslabnum1: usize;
        let slotnum1: usize;
        
        if let SlotLocation::LargeSlot { largeslabnum, slotnum } = sl1 {
            largeslabnum1 = largeslabnum;
            slotnum1 = slotnum;
        } else {
            panic!("Should have been a large slot.");
        }
        assert_eq!(largeslabnum1, largeslabnum);
        assert_eq!(slotnum1, i);
        i += 1;

        // Step 2: allocate all the rest of the slots in this slab except the last one:
        while i < NUM_SLOTS_O - 1 {
            sm.alloc_slot(l).unwrap();
            i += 1
        }

        // Step 3: allocate the last slot in this slab and store it in a local variable:
        let sl2 = sm.alloc_slot(l).unwrap();
        let largeslabnum2: usize;
        let slotnum2: usize;

        if let SlotLocation::LargeSlot { largeslabnum, slotnum } = sl2 {
            largeslabnum2 = largeslabnum;
            slotnum2 = slotnum;
        } else {
            panic!("Should have been a large slot.");
        }

        // Assert some things about the two stored slot locations:
        assert_eq!(largeslabnum1, largeslabnum2);
        assert_eq!(slotnum1, orig_i);
        assert_eq!(slotnum2, NUM_SLOTS_O - 1);

        // Step 4: allocate another slot from this slab and store it in a local variable:
        let sl3 = sm.alloc_slot(l).unwrap();
        let largeslabnum3: usize;

        if let SlotLocation::LargeSlot { largeslabnum, .. } = sl3 {
            largeslabnum3 = largeslabnum;
        } else {
            panic!("Should have been a large slot.");
        }

        // Assert that this alloc overflowed to a different slab.
        assert_ne!(largeslabnum1, largeslabnum3);
        assert_eq!(largeslabnum3, largeslabnum+1);

        // Step 5: set the overflow slab to be full
        _help_set_large_slab_eac(&sm, largeslabnum+1, NUM_SLOTS_O);

        let sl4 = sm.alloc_slot(l).unwrap();
        let largeslabnum4: usize;

        if let SlotLocation::LargeSlot { largeslabnum, .. } = sl4 {
            largeslabnum4 = largeslabnum;
        } else {
            panic!("Should have been a large slot.");
        }

        // Assert that this alloc overflowed to a different slab from the first slab.
        assert_ne!(largeslabnum4, largeslabnum1);

        // Assert that this alloc overflowed to a different slab from the second slab.
        assert_ne!(largeslabnum4, largeslabnum3);

        // Assert that this alloc overflowed to next next slab.
        assert_eq!(largeslabnum4, largeslabnum3+1);
    }

    #[test]
    /// If we've allocated all of the slots from large-slots slab 0,
    /// the subsequent allocations come from large-slots slab 1.
    fn overflowers_large_slab_0() {
        help_test_overflowers_large(0);
    }

    #[test]
    /// If we've allocated all of the slots from large-slots slab 8,
    /// the subsequent allocations come from large-slots slab 9.
    fn overflowers_large_slab_8() {
        help_test_overflowers_large(8);
    }

    #[test]
    /// If we've allocated all of the slots from large-slots slab 9 --
    /// the huge-slots slab -- the subsequent allocations come from
    /// falling back to the system allocator.
    fn overflowers_huge_slots_slab() {
        const LARGESLABNUM: usize = NUM_LARGE_SLABS - 1;

        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let l = Layout::from_size_align(large_slabnum_to_slotsize(LARGESLABNUM), 1).unwrap();

        let orig_i = NUM_SLOTS_HUGE - 3;
        let mut i = orig_i;
        _help_set_large_slab_eac(&sm, LARGESLABNUM, i);

        // Step 1: allocate a slot and store it in a local variable:
        let sl1 = sm.alloc_slot(l).unwrap();
        let largeslabnum1: usize;
        let slotnum1: usize;
        
        if let SlotLocation::LargeSlot { largeslabnum, slotnum } = sl1 {
            largeslabnum1 = largeslabnum;
            slotnum1 = slotnum;
        } else {
            panic!("Should have been a large slot.");
        }
        assert_eq!(largeslabnum1, LARGESLABNUM);
        assert_eq!(slotnum1, i);
        i += 1;
        
        // Step 2: allocate all the rest of the slots in this slab except the last one:
        while i < NUM_SLOTS_HUGE - 1 {
            sm.alloc_slot(l).unwrap();
            i += 1
        }

        // Step 3: allocate the last slot in this slab and store it in a local variable:
        let sl2 = sm.alloc_slot(l).unwrap();
        let largeslabnum2: usize;
        let slotnum2: usize;

        if let SlotLocation::LargeSlot { largeslabnum, slotnum } = sl2 {
            largeslabnum2 = largeslabnum;
            slotnum2 = slotnum;
        } else {
            panic!("Should have been a large slot.");
        }

        // Assert some things about the two stored slot locations:
        assert_eq!(largeslabnum1, largeslabnum2);
        assert_eq!(slotnum1, orig_i);
        assert_eq!(slotnum2, NUM_SLOTS_HUGE - 1);

        // Step 4: allocate another slot from this slab and store it in a local variable:
        let sl3 = sm.alloc_slot(l);
        assert!(sl3.is_none()); // no slots available

        // Step 5: invoke the global `alloc()`
        let alloced_ptr = unsafe { sm.alloc(l) };
        assert!(!alloced_ptr.is_null());

        let osl = SlotLocation::new_from_ptr(sm.get_baseptr(), alloced_ptr);
        assert!(osl.is_none()); // it's not pointing to one of our slots

        // I don't believe in sweeping the floors right before razing
        // the house. This call to `sys_dealloc()` is just to exercise
        // more code in case something (like valgrind for example)
        // could find a bug in smalloc this way.
        sys_dealloc(alloced_ptr, l);
    }
}
