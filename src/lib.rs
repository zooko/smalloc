#![doc = include_str!("../README.md")]

#![feature(pointer_is_aligned_to)]
#![allow(clippy::assertions_on_constants)]


// Layout of this file:
// * Fixed constants chosen for the design
// * Constant values computed from the constants above
// * Implementation
// * Code for development (e.g benchmarks, tests, development utilities)


// --- Fixed constants chosen for the design ---

const NUM_SMALL_SLABS: usize = 5;
const NUM_MEDIUM_SLABS: usize = 8;
const NUM_LARGE_SLABS: usize = 1;

const SMALLEST_SLOT_SIZE: usize = 4;
const LARGE_SLOT_SIZE: usize = 2usize.pow(26); // 64 MiB

const NUM_SMALL_SLOTS: usize = 2usize.pow(24);
const NUM_MEDIUM_SLOTS: usize = 2usize.pow(29);
const NUM_LARGE_SLOTS: usize = 2usize.pow(20);

// There are 32 areas each with a full complements of small slabs.
const NUM_SMALL_SLAB_AREAS: usize = 32;

// This is the largest alignment we can guarantee for data slots.
const MAX_ALIGNMENT: usize = 2usize.pow(26);


// --- Constant values computed from the constants above ---

const NUM_SMALL_PLUS_MEDIUM_SLABS: usize = NUM_SMALL_SLABS + NUM_MEDIUM_SLABS;

const MEDIUM_SLABS_REGION_BASE: usize = (2usize.pow(NUM_SMALL_SLABS as u32) - 1) * NUM_MEDIUM_SLOTS * SMALLEST_SLOT_SIZE;

const LARGE_SLAB_REGION_BASE: usize = (2usize.pow(NUM_SMALL_PLUS_MEDIUM_SLABS as u32) - 1) * NUM_MEDIUM_SLOTS * SMALLEST_SLOT_SIZE;

const LARGE_SLAB_REGION_SIZE: usize = LARGE_SLOT_SIZE * NUM_LARGE_SLOTS * NUM_LARGE_SLABS;

// The per-slab flhs and eacs have this size in bytes.
const DOUBLEWORDSIZE: usize = 8;

// The free list entries have this size in bytes.
const SINGLEWORDSIZE: usize = 4;

// One eac plus one flh
const VARSSIZE: usize = DOUBLEWORDSIZE * 2;

const SMALL_SLABS_VARS_REGION_BASE: usize = LARGE_SLAB_REGION_BASE + LARGE_SLAB_REGION_SIZE;

const MEDIUM_SLABS_VARS_REGION_BASE: usize = SMALL_SLABS_VARS_REGION_BASE + NUM_SMALL_SLAB_AREAS * NUM_SMALL_SLABS * VARSSIZE ;

const LARGE_SLAB_VARS_REGION_BASE: usize = MEDIUM_SLABS_VARS_REGION_BASE + NUM_MEDIUM_SLABS * VARSSIZE;

const LARGE_SLAB_VARS_REGION_SIZE: usize = VARSSIZE;

// Pad the total memory to allocate with an added MAX_ALIGNMENT - 1
// bytes so that we can "slide forward" the smalloc base pointer to
// the first 64 MiB boundary in order to align the smalloc base
// pointer to 64 MiB.
const TOTAL_VIRTUAL_MEMORY: usize = LARGE_SLAB_VARS_REGION_BASE + LARGE_SLAB_VARS_REGION_SIZE + MAX_ALIGNMENT - 1;


// --- Implementation ---

use std::primitive::usize;

// xxx remove
// S is NUM_MEDIUM_SLABS
//     slabnum  offset (* S * 4) slotsize/4    slotsize
//           0                 0          1           4
//           1                 1          2           8
//           2                 3          4          16
//           3                 7          8          32
//           4                15         16          64

//           5                31         32         128
//           6                63         64         256
//           7               127        128         512
//           8               255        256        1024
//           9               511        512        2048
//          10              1023       1024        4096
//          11              2047       2048       16384

//          12              4095       4096     special
//              ---------------------------------------
//                      2^(sn)-1     2^(sn)  2^(sn) * 4

#[inline(always)]
fn slotsize(allslabnum: usize) -> usize {
    debug_assert!(allslabnum < NUM_SMALL_PLUS_MEDIUM_SLABS, "{allslabnum} < {NUM_SMALL_PLUS_MEDIUM_SLABS}");

    2usize.pow(allslabnum as u32) * SMALLEST_SLOT_SIZE
}

#[inline(always)]
fn small_slab_area_base_offset(smallslabnum: usize) -> usize {
    debug_assert!(smallslabnum < NUM_SMALL_SLABS);

    (2usize.pow(smallslabnum as u32) - 1) * NUM_SMALL_SLAB_AREAS * NUM_SMALL_SLOTS * SMALLEST_SLOT_SIZE
}

#[inline(always)]
fn small_slab_base_offset(smallslabnum: usize, areanum: usize) -> usize {
    debug_assert!(smallslabnum < NUM_SMALL_SLABS);
    debug_assert!(areanum < NUM_SMALL_SLAB_AREAS);

    small_slab_area_base_offset(smallslabnum) + areanum * NUM_SMALL_SLOTS * slotsize(smallslabnum)
}

#[inline(always)]
fn small_slot_offset(smallslabnum: usize, areanum: usize, slotnum: usize) -> usize {
    // xxx unit tests
    debug_assert!(smallslabnum < NUM_SMALL_SLABS);
    debug_assert!(areanum < NUM_SMALL_SLAB_AREAS);
    debug_assert!(slotnum < NUM_SMALL_SLOTS);

    small_slab_base_offset(smallslabnum, areanum) + slotnum * slotsize(smallslabnum)
}

/// Returns (areanum, slotnum)
#[inline(always)]
fn small_slot(offset: usize, smallslabnum: usize, slotsize: usize) -> (usize, usize) {
    debug_assert!(offset < MEDIUM_SLABS_REGION_BASE);
    debug_assert!(smallslabnum < NUM_SMALL_SLABS);
    debug_assert_eq!(slotsize, crate::slotsize(smallslabnum));
    
    let slab_area_base_o = small_slab_area_base_offset(smallslabnum);
    let within_slab_area_o = offset - slab_area_base_o;
    let areasize = slotsize * NUM_SMALL_SLOTS;
    let areanum = within_slab_area_o / areasize;
    let within_area_o = within_slab_area_o % areasize;
    let slotnum = within_area_o / slotsize;

    (areanum, slotnum)
}

#[inline(always)]
fn medium_slab_base_offset(mediumslabnum: usize) -> usize {
    debug_assert!(mediumslabnum < NUM_MEDIUM_SLABS);

    let allslabnum = NUM_SMALL_SLABS + mediumslabnum;

    // Note that this works because NUM_MEDIUM_SLOTS ==
    // NUM_SMALL_SLOTS * NUM_SMALL_SLAB_AREAS.
    (2usize.pow(allslabnum as u32) - 1) * NUM_MEDIUM_SLOTS * SMALLEST_SLOT_SIZE
}

#[inline(always)]
fn medium_slot_offset(mediumslabnum: usize, slotnum: usize) -> usize {
    // xxx unit tests
    debug_assert!(mediumslabnum < NUM_MEDIUM_SLABS);
    debug_assert!(slotnum < NUM_MEDIUM_SLOTS);

    let allslabnum = NUM_SMALL_SLABS + mediumslabnum;

    medium_slab_base_offset(mediumslabnum) + slotnum * slotsize(allslabnum)
}

/// Returns slotnum
#[inline(always)]
fn medium_slot(offset: usize, mediumslabnum: usize, slotsize: usize) -> usize {
    debug_assert!(offset < LARGE_SLAB_REGION_BASE);
    debug_assert!(mediumslabnum < NUM_MEDIUM_SLABS);
    debug_assert_eq!(slotsize, crate::slotsize(NUM_SMALL_SLABS + mediumslabnum));
    debug_assert!(offset.is_multiple_of(slotsize));
    
    let slabbase_o = medium_slab_base_offset(mediumslabnum);
    let within_slab_o = offset - slabbase_o;

    within_slab_o / slotsize
}

#[inline(always)]
fn large_slab_base_offset() -> usize {
    LARGE_SLAB_REGION_BASE
    // Note that this works because NUM_MEDIUM_SLOTS ==
    // NUM_SMALL_SLOTS * NUM_SMALL_SLAB_AREAS.
//BUG BUG BUG    (2usize.pow(NUM_TOT_SLABS as u32) - 1) * NUM_MEDIUM_SLOTS * SMALLEST_SLOT_SIZE
}

#[inline(always)]
fn large_slot_offset(slotnum: usize) -> usize {
    // xxx unit tests
    debug_assert!(slotnum < NUM_LARGE_SLOTS);

    large_slab_base_offset() + slotnum * LARGE_SLOT_SIZE
}

#[inline(always)]
fn large_slot(offset: usize) -> usize {
    debug_assert!(offset >= LARGE_SLAB_REGION_BASE);
    debug_assert!(offset < SMALL_SLABS_VARS_REGION_BASE);

    let within_slab_o = offset - LARGE_SLAB_REGION_BASE;
    within_slab_o / LARGE_SLOT_SIZE
}

use std::cmp::min;

// xyz are these two ..._to_allslabnum()'s basically doing the same computation?
// xyz document that it can return  > large slab num
// xyz replace this whole thing with something that returns a combination of sizeclass+size?
/// xyz ? This works only for sizes up to 2^14 * 4
#[inline(always)]
pub fn size_to_allslabnum_log_branch(size: usize) -> usize {
    if size <= SMALLEST_SLOT_SIZE {
        0
    } else {
        ((size - 1) / SMALLEST_SLOT_SIZE).ilog2() as usize + 1
    }
}

#[inline(always)]
pub fn size_to_allslabnum_lzcnt_branch(size: usize) -> usize {
    if size <= SMALLEST_SLOT_SIZE {
        0
    } else {
        // constify
        (62 - (size-1).leading_zeros()) as usize
    }
}

#[inline(always)]
pub fn size_to_allslabnum_lzcnt_min(size: usize) -> usize {
    // constify
    (62 - min(62, (size-1).leading_zeros())) as usize
}

// bench min vs branch
#[inline(always)]
fn size_to_allslabnum(size: usize) -> usize {
    debug_assert!(size > 0);
    debug_assert!(size < 32768);
    size_to_allslabnum_lzcnt_branch(size)
    // size_to_allslabnum_lzcnt_min(size)
    //    size_to_allslabnum_log_branch(size)
}

#[inline(always)]
fn offset_to_allslabnum_lzcnt(offset: usize) -> usize {
    // xyz constify the max num of zeroes
    32 - (offset + NUM_MEDIUM_SLOTS * SMALLEST_SLOT_SIZE).leading_zeros() as usize
}

#[cfg(test)]
#[inline(always)]
fn offset_to_allslabnum_log(offset: usize) -> usize {
    // xyz constify the subtraction?
    ((offset + NUM_MEDIUM_SLOTS * SMALLEST_SLOT_SIZE).ilog2() - 31) as usize
}

/// This works only for offsets in the small-slabs or medium-slabs regions, not for the large-slabs
/// region.
#[inline(always)]
fn offset_to_allslabnum(offset: usize) -> usize {
    debug_assert!(offset < LARGE_SLAB_REGION_BASE);

    debug_assert_eq!(std::primitive::usize::BITS, 64);
    // xxx unit tests!!!
    debug_assert!(offset.is_multiple_of(SMALLEST_SLOT_SIZE));

    offset_to_allslabnum_lzcnt(offset)
}

#[inline(always)]
fn small_flh_offset(smallslabnum: usize, areanum: usize) -> usize {
    SMALL_SLABS_VARS_REGION_BASE + (areanum * NUM_SMALL_SLABS + smallslabnum) * VARSSIZE
}

#[inline(always)]
fn small_eac_offset(smallslabnum: usize, areanum: usize) -> usize {
    SMALL_SLABS_VARS_REGION_BASE + (areanum * NUM_SMALL_SLABS + smallslabnum) * VARSSIZE + DOUBLEWORDSIZE
}

#[inline(always)]
fn medium_flh_offset(mediumslabnum: usize) -> usize {
    debug_assert!(mediumslabnum < NUM_MEDIUM_SLABS);

    MEDIUM_SLABS_VARS_REGION_BASE + mediumslabnum * VARSSIZE
}

#[inline(always)]
fn medium_eac_offset(mediumslabnum: usize) -> usize {
    debug_assert!(mediumslabnum < NUM_MEDIUM_SLABS);

    MEDIUM_SLABS_VARS_REGION_BASE + mediumslabnum * VARSSIZE + DOUBLEWORDSIZE
}

#[inline(always)]
fn large_flh_offset() -> usize {
    LARGE_SLAB_VARS_REGION_BASE
}

#[inline(always)]
fn large_eac_offset() -> usize {
    LARGE_SLAB_VARS_REGION_BASE + DOUBLEWORDSIZE
}


use core::alloc::{GlobalAlloc, Layout};

use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU64, Ordering};
mod platformalloc;
use platformalloc::{sys_alloc, sys_dealloc, sys_realloc};
use platformalloc::vendor::{PAGE_SIZE, CACHE_LINE_SIZE};
use std::ptr::{copy_nonoverlapping, null_mut};
use std::cell::Cell;

static GLOBAL_THREAD_AREANUM: AtomicU32 = AtomicU32::new(0);

thread_local! {
    static THREAD_AREANUM: Cell<Option<u32>> = const { Cell::new(None) };
}

/// Get this thread's areanum, or initialize it to the first unused areanum if this is the first
/// time `get_thread_areanum()` has been called.
#[inline(always)]
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

/// Return the offset from the smalloc base pointer, or None if the pointer does not point to the
/// beginning of one of our slots.
#[inline(always)]
fn offset_of_ptr(sys_baseptr: *const u8, sm_baseptr: *const u8, ptr: *const u8) -> Option<usize> {
    // There is no well-specified way to compare two pointers if they aren't part of the same
    // allocation, which this `ptr` and our system baseptr might not be.

    // .addr() is our way of promising the Rust compiler that we won't round-trip these values back
    // into pointers from usizes and use them. See
    // https://doc.rust-lang.org/nightly/std/ptr/index.html#strict-provenance
    let p_addr = ptr.addr();
    let sys_baseptr_addr = sys_baseptr.addr();

    // If the pointer is before the system base pointer or after the end of our allocated space,
    // then it must have come from an alloc where we fell back to `sys_alloc()`.
    if (p_addr < sys_baseptr_addr) || (p_addr > sys_baseptr_addr + TOTAL_VIRTUAL_MEMORY) {
        // xxx add unit test of this case
        return None;
    }

    // If the pointer is before the smalloc base pointer (while being after the system base
    // pointer), this is due to corruption.
    let sm_baseptr_addr = sm_baseptr.addr();
    assert!(p_addr >= sm_baseptr_addr); // This is a security boundary.

    // If the pointer is in the variables region, this is due to corruption.
    assert!(p_addr < sm_baseptr_addr + SMALL_SLABS_VARS_REGION_BASE); // This is a security boundary.

    // If the pointer is not a multiple of SMALLEST_SLOT_SIZE, this is due to corruption.
    assert!(p_addr.is_multiple_of(SMALLEST_SLOT_SIZE)); // This is a security boundary.

    // Okay now we know that it is pointer into our allocation, so it is safe to subtract baseptr
    // from it.
    let ioffset = unsafe { ptr.offset_from(sm_baseptr) };
    debug_assert!(ioffset >= 0);
    Some(ioffset as usize)
}

pub struct Smalloc {
    initlock: AtomicBool,
    sys_baseptr: AtomicPtr<u8>,
    sm_baseptr: AtomicPtr<u8>
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
            sys_baseptr: AtomicPtr::<u8>::new(null_mut()),
            sm_baseptr: AtomicPtr::<u8>::new(null_mut())
        }
    }

    fn idempotent_init(&self) -> Result<*mut u8, AllocFailed> {
        let mut p: *mut u8;

        p = self.sm_baseptr.load(Ordering::Acquire);
        if !p.is_null() {
            return Ok(p);
        }

        //eprintln!("TOTAL_VIRTUAL_MEMORY: {TOTAL_VIRTUAL_MEMORY}");

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

        let mut smbp = null_mut();

        p = self.sys_baseptr.load(Ordering::Acquire);
        if p.is_null() {
            let sysbp = sys_alloc(layout)?;
            debug_assert!(!sysbp.is_null());
            self.sys_baseptr.store(sysbp, Ordering::Release);//xxx can we use weaker ordering constraints?

            let sysbpaddr = sysbp.addr();
            let pad = if sysbpaddr.is_multiple_of(MAX_ALIGNMENT) {
                0
            } else {
                MAX_ALIGNMENT - (sysbpaddr % MAX_ALIGNMENT)
            };

            smbp = unsafe { sysbp.add(pad) };
            debug_assert!(smbp.is_aligned_to(MAX_ALIGNMENT));
            self.sm_baseptr.store(smbp, Ordering::Release);
        }

        // Release the spin lock
        self.initlock.store(false, Ordering::Release);

        debug_assert!(!smbp.is_null());
        Ok(smbp)
    }

    #[inline(always)]
    fn get_sys_baseptr(&self) -> *mut u8 {
        let p = self.sys_baseptr.load(Ordering::Acquire);
        debug_assert!(!p.is_null());

        p
    }

    #[inline(always)]
    fn get_sm_baseptr(&self) -> *mut u8 {
        let p = self.sm_baseptr.load(Ordering::Acquire);
        debug_assert!(!p.is_null());

        p
    }

    #[inline(always)]
    fn get_atomicu64(&self, offset: usize) -> &AtomicU64 {
        debug_assert!(offset.is_multiple_of(DOUBLEWORDSIZE));
        debug_assert_eq!(DOUBLEWORDSIZE*8, std::primitive::u64::BITS as usize);
        unsafe { AtomicU64::from_ptr(self.get_sm_baseptr().add(offset).cast::<u64>()) }
    }
    
    #[inline(always)]
    fn get_atomicu32(&self, offset: usize) -> &AtomicU32 {
        debug_assert!(offset.is_multiple_of(SINGLEWORDSIZE));
        debug_assert_eq!(SINGLEWORDSIZE*8, std::primitive::u32::BITS as usize);
        unsafe { AtomicU32::from_ptr(self.get_sm_baseptr().add(offset).cast::<u32>()) }
    }
    
    #[inline(always)]
    fn push_onto_small_slab_freelist(&self, smallslabnum: usize, areanum: usize, slotnum: usize) {
        debug_assert!(smallslabnum < NUM_SMALL_SLABS);
        debug_assert!(areanum < NUM_SMALL_SLAB_AREAS);
        debug_assert!(slotnum < NUM_SMALL_SLOTS);

        let flha = self.get_atomicu64(small_flh_offset(smallslabnum, areanum));
        let eaca = self.get_atomicu64(small_eac_offset(smallslabnum, areanum));
        let slota = self.get_atomicu32(small_slot_offset(smallslabnum, areanum, slotnum));

        Self::inner_push_onto_freelist(flha, eaca, slota, slotnum)
    }
    
    #[inline(always)]
    fn push_onto_medium_slab_freelist(&self, mediumslabnum: usize, slotnum: usize) {
        debug_assert!(mediumslabnum < NUM_MEDIUM_SLABS);

        let flha = self.get_atomicu64(medium_flh_offset(mediumslabnum));
        let eaca = self.get_atomicu64(medium_eac_offset(mediumslabnum));
        let slota = self.get_atomicu32(medium_slot_offset(mediumslabnum, slotnum));

        Self::inner_push_onto_freelist(flha, eaca, slota, slotnum);
    }

    #[inline(always)]
    fn push_onto_large_slab_freelist(&self, slotnum: usize) {
        let flha = self.get_atomicu64(large_flh_offset());
        let eaca = self.get_atomicu64(large_eac_offset());
        let slota = self.get_atomicu32(large_slot_offset(slotnum));

        Self::inner_push_onto_freelist(flha, eaca, slota, slotnum);
    }

    #[inline(always)]
    fn inner_push_onto_freelist(flha: &AtomicU64, eaca: &AtomicU64, slota: &AtomicU32, slotnum: usize) {
        debug_assert_eq!(flha.as_ptr().addr() + DOUBLEWORDSIZE, eaca.as_ptr().addr());
        debug_assert!(flha.as_ptr().is_aligned_to(DOUBLEWORDSIZE));
        debug_assert!(eaca.as_ptr().is_aligned_to(DOUBLEWORDSIZE));

        // These tests of `slotnum` and `firstindexplus1` against `eac` could be a security
        // boundary, if an attacker is attempting to exploit this process, and their exploit relies
        // on pushing a slot to the free list which has never been allocated. It could also, of
        // course, help find bugs in smalloc or in the code of smalloc's user. Presumably it imposes
        // very little computational cost at runtime, but maybe benchmark that... XXX
        let eacu = eaca.load(Ordering::Acquire);
        assert!(slotnum < eacu as usize, "{slotnum} < {eacu}"); // This is a security boundary.

        loop {
            let flhdword: u64 = flha.load(Ordering::Acquire);
            let firstindexplus1: u32 = (flhdword & u32::MAX as u64) as u32;
            let counter: u32 = (flhdword >> 32) as u32;

            slota.store(firstindexplus1, Ordering::Release);

            let newflhdword = ((counter as u64 + 1) << 32) | (slotnum+1) as u64;

            if flha.compare_exchange_weak(
                flhdword,
                newflhdword,
                Ordering::AcqRel, // AcqRel
                Ordering::Acquire, // Acquire
            ).is_ok() {
                let eacu = eaca.load(Ordering::Acquire);
                assert!((firstindexplus1 as u64) < eacu + 1, "firstindexplus1: {firstindexplus1}, eacu: {eacu}"); // This is a security boundary.
                break;
            }
        }
    }

    /// Allocate a slot from this slab by popping the free-list-head, if possible, else incrementing
    /// the ever-allocated-counter. Return the resulting pointer, or null pointer if this slab is
    /// full.
    #[inline(always)]
    fn inner_alloc(&self, flha: &AtomicU64, slab_bp: *mut u8, eaca: &AtomicU64, slotsize: usize, numslots: usize) -> *mut u8 {
        debug_assert_eq!(flha.as_ptr().addr() + DOUBLEWORDSIZE, eaca.as_ptr().addr());
        debug_assert!(slab_bp.is_aligned_to(PAGE_SIZE));
        debug_assert!(flha.as_ptr().is_aligned_to(DOUBLEWORDSIZE));
        debug_assert!(eaca.as_ptr().is_aligned_to(DOUBLEWORDSIZE));
        debug_assert!(slotsize <= LARGE_SLOT_SIZE);
        debug_assert!(numslots <= NUM_MEDIUM_SLOTS);

        let slotp1;

        // These tests of `firstindexplus1` and `nextindexplus1` against `eac` *could* be a security
        // boundary, but probably not. They would be, only if an exploit relies on popping from the
        // free list when either the previous head or the new head point to a
        // previously-never-allocated slot. It could also, of course, help find bugs in smalloc, but
        // probably never in smalloc's user code. Presumably it imposes very little computational
        // cost at runtime, but maybe benchmark that... XXX
        loop {
            let flhdword: u64 = flha.load(Ordering::Acquire);
            let firstindexplus1: u32 = (flhdword & (u32::MAX as u64)) as u32;
            debug_assert!(firstindexplus1 as u64 <= eaca.load(Ordering::Acquire));

            let counter: u32 = (flhdword >> 32) as u32;
            if firstindexplus1 == 0 {
                // 0 is the sentinel value meaning no next entry, meaning the free list is empty
                slotp1 = 0;
                break;
            };

            let next_p = unsafe { slab_bp.add((firstindexplus1 - 1) as usize * slotsize) };
            debug_assert!(next_p.is_aligned_to(SINGLEWORDSIZE)); // need 4-byte alignment for atomic ops (on at least some/most platforms)
            let nexta = unsafe { AtomicU32::from_ptr(next_p.cast::<u32>()) };
            let nextindexplus1: u32 = nexta.load(Ordering::Acquire);

            let newflhdword = ((counter as u64 + 1) << 32) | nextindexplus1 as u64;

            if flha.compare_exchange_weak(
                flhdword,
                newflhdword,
                Ordering::AcqRel,
                Ordering::Acquire
            ).is_ok() {
                // This constraint must be true, since the compare-exchange succeeded.
                debug_assert!(nextindexplus1 as u64 <= eaca.load(Ordering::Acquire));

                slotp1 = firstindexplus1;
                break;
            }
        };

        if slotp1 > 0 {
            // Return the first slot from the free list.
            unsafe { slab_bp.add((slotp1 - 1) as usize * slotsize) }
        } else {
            // The free list was empty, so allocate another slot and increment `eac`.
            let nextslot = eaca.fetch_add(1, Ordering::Relaxed);
            if nextslot < numslots as u64 {
                unsafe { slab_bp.add(nextslot as usize * slotsize) }
            } else {
                // The slab was full.
                // xxx add unit test that eac gets correctly decremented when the thing is full
                eaca.fetch_sub(1, Ordering::Relaxed);
                null_mut()
            }
        }
    }
}

unsafe impl GlobalAlloc for Smalloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        //eprint!("alloc({layout:?}) -> ");
        match self.idempotent_init() {
            Err(error) => {
                eprintln!("Failed to alloc; underlying error: {error}");
                null_mut()
            }
            Ok(smbp) => {
                let size = layout.size();
                assert!(size > 0);
                let alignment = layout.align();
                assert!(alignment > 0);
                assert!((alignment & (alignment - 1)) == 0); // alignment must be a power of two

                // Round up size to the nearest multiple of alignment in order to get a slot that is
                // aligned on that size.
                let alignedsize: usize = ((size - 1) | (alignment - 1)) + 1;

                if alignedsize <= slotsize(NUM_SMALL_SLABS - 1) {
                    let areanum = get_thread_areanum();
                    let allslabnum = size_to_allslabnum(alignedsize);
                    let slab_bp = unsafe { smbp.add(small_slab_base_offset(allslabnum, areanum)) };

                    let flha = self.get_atomicu64(small_flh_offset(allslabnum, areanum));
                    let eaca = self.get_atomicu64(small_eac_offset(allslabnum, areanum));

                    let slotsize = slotsize(allslabnum);
                    let ptr = self.inner_alloc(flha, slab_bp, eaca, slotsize, NUM_SMALL_SLOTS);
                    if !ptr.is_null() {
                        return ptr;
                    }

                    // The slab was full. Go ahead and overflow to a larger slot, by recursively
                    // calling `.alloc()` with a doubled requested size. (Doubling the requested
                    // size guarantees that the new recursive request will use the next larger
                    // slabnum.)
                    let enlarged_layout = Layout::from_size_align(size * 2, alignment).unwrap();
                    return unsafe { self.alloc(enlarged_layout) };
                }

                if alignedsize <= slotsize(NUM_SMALL_PLUS_MEDIUM_SLABS - 1) {
                    let allslabnum = size_to_allslabnum(alignedsize);
                    let mediumslabnum = allslabnum - NUM_SMALL_SLABS;
                    let slab_bp = unsafe { smbp.add(medium_slab_base_offset(mediumslabnum)) };
                    let flha = self.get_atomicu64(medium_flh_offset(mediumslabnum));
                    let eaca = self.get_atomicu64(medium_eac_offset(mediumslabnum));

                    let slotsize = slotsize(allslabnum);
                    let ptr = self.inner_alloc(flha, slab_bp, eaca, slotsize, NUM_MEDIUM_SLOTS);
                    if !ptr.is_null() {
                        return ptr;
                    }

                    // This slab is totally full! This seems really unlikely/rare. But we go ahead
                    // and overflow to a larger slot, by recursively calling `.alloc()` with a
                    // doubled requested size. (Doubling the requested size guarantees that the new
                    // recursive request will use the next larger slabnum.)
                    let enlarged_layout = Layout::from_size_align(size * 2, alignment).unwrap();
                    return unsafe { self.alloc(enlarged_layout) };
                }

                if alignedsize <= LARGE_SLOT_SIZE {
                    let slab_bp = unsafe { smbp.add(large_slab_base_offset()) };
                    let flha = self.get_atomicu64(large_flh_offset());
                    let eaca = self.get_atomicu64(large_eac_offset());

                    let ptr = self.inner_alloc(flha, slab_bp, eaca, LARGE_SLOT_SIZE, NUM_LARGE_SLOTS);
                    debug_assert!(ptr < unsafe { smbp.add(SMALL_SLABS_VARS_REGION_BASE) } );
                    if !ptr.is_null() {
                        return ptr;
                    }

                    // All the slots in the large-slots slab are full. Fall-through (to fall back to
                    // the system allocator).
                }

                // Either all the slots in the large-slots slab are full, or this request is too
                // large for even smalloc's largest slots, so fall back to system allocator.
                let ptr = sys_alloc(layout).unwrap();

                // This is a correctness requirement, which relies on the underlying system
                // allocator's behavior.
                debug_assert!(ptr.is_aligned_to(layout.align()));

                // This is not a security boundary because `p` was provided to us by the underlying
                // system. But the counterpart of this assertion--asserting before we
                // `sys_dealloc()` that the pointer is aligned, is a security boundary because at
                // that point the pointer will be provided to us by the user code, and that pointer
                // could be attacker-controlled.
                debug_assert!(ptr.is_aligned_to(PAGE_SIZE));

                //eprintln!("{ptr:?}");

                ptr
            }
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        //eprintln!("dealloc({ptr:?}, {layout:?})");
        let optoffset = offset_of_ptr(self.get_sys_baseptr(), self.get_sm_baseptr(), ptr);
        match optoffset {
            None => {
                // This ptr doesn't point to the beginning of one of our slabs. It must have come
                // from an allocation that we satisfied by falling back to the system allocator.
                assert!(ptr.is_aligned_to(PAGE_SIZE)); // This is a security boundary.
                sys_dealloc(ptr, layout);
            }
            Some(offset) => {
                if offset < MEDIUM_SLABS_REGION_BASE {
                    // This points into the "small-slabs-areas-region".

                    let allslabnum = offset_to_allslabnum(offset);
                    let slotsize = slotsize(allslabnum);

                    // This is a security boundary since `offset` could be attacker-controlled (as
                    // well as necessary for alignment reasons).
                    assert!(offset.is_multiple_of(slotsize));

                    let (areanum, slotnum) = small_slot(offset, allslabnum, slotsize);

                    self.push_onto_small_slab_freelist(allslabnum, areanum, slotnum);
                } else if offset < LARGE_SLAB_REGION_BASE {
                    // This points into the "medium-slabs-region".

                    let allslabnum = offset_to_allslabnum(offset);
                    let slotsize = slotsize(allslabnum);

                    // This is a security boundary since `offset` could be attacker-controlled (as
                    // well as necessary for alignment reasons).
                    assert!(offset.is_multiple_of(slotsize));

                    let mediumslabnum = allslabnum - NUM_SMALL_SLABS;

                    let slotnum = medium_slot(offset, mediumslabnum, slotsize);
                    
                    self.push_onto_medium_slab_freelist(mediumslabnum, slotnum);
                } else {
                    // This points into the "large-slab".
                    self.push_onto_large_slab_freelist(large_slot(offset));
                }
            }
        }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, reqsize: usize) -> *mut u8 {
        //eprint!("realloc({ptr:?}, {layout:?}, {reqsize}) -> ");
        assert!(!ptr.is_null());
        let oldsize = layout.size();
        assert!(oldsize > 0);
        let oldalignment = layout.align();
        assert!(oldalignment > 0);
        assert!((oldalignment & (oldalignment - 1)) == 0, "alignment must be a power of two");
        assert!(reqsize > 0);

        // If the requested new size is <= the original size, just return the pointer and we're
        // done.
        if reqsize <= oldsize {
            //eprintln!("{ptr:?}");
            return ptr;
        }

        // The "growers" rule: use the smallest of the following sizes that will fit:
        // CACHE_LINE_SIZE, PAGE_SIZE, or LARGE_SLOT_SIZE. Or if it too large for even the large
        // slots, then double the current size.
        let optoffset = offset_of_ptr(self.get_sys_baseptr(), self.get_sm_baseptr(), ptr);
        match optoffset {
            None => {
                // This ptr doesn't point to the beginning of one of our slabs. It must have come
                // from an allocation that we satisfied by falling back to the system allocator.
                assert!(ptr.is_aligned_to(PAGE_SIZE)); // This is a security boundary.

                let alignedreqsize: usize = ((reqsize - 1) | (oldalignment - 1)) + 1;
                let newsize = if alignedreqsize <= CACHE_LINE_SIZE {
                    CACHE_LINE_SIZE
                } else if alignedreqsize <= PAGE_SIZE {
                    PAGE_SIZE
                } else if alignedreqsize <= LARGE_SLOT_SIZE {
                    LARGE_SLOT_SIZE
                } else {
                    alignedreqsize * 2
                };
                let alignednewsize: usize = ((newsize - 1) | (oldalignment - 1)) + 1;

                let newp = sys_realloc(ptr, layout, alignednewsize);
                debug_assert!(!newp.is_null());
                debug_assert!(newp.is_aligned_to(oldalignment));
                //eprintln!("{newp:?}");
                newp
            }
            Some(offset) => {
                if offset < MEDIUM_SLABS_REGION_BASE {
                    // This is a small slot.

                    // If the requested size fits into the current slot, just return the current
                    // pointer and we're done.
                    let alignedreqsize: usize = ((reqsize - 1) | (oldalignment - 1)) + 1;
                    let allslabnum = offset_to_allslabnum(offset);
                    let slotsize = slotsize(allslabnum);
                    if alignedreqsize <= slotsize {
                        //eprintln!("{ptr:?}");
                        return ptr;
                    }

                    let newsize = if alignedreqsize <= CACHE_LINE_SIZE {
                        CACHE_LINE_SIZE
                    } else if alignedreqsize <= PAGE_SIZE {
                        PAGE_SIZE
                    } else if alignedreqsize <= LARGE_SLOT_SIZE {
                        LARGE_SLOT_SIZE
                    } else {
                        alignedreqsize * 2
                    };
                    let alignednewsize: usize = ((newsize - 1) | (oldalignment - 1)) + 1;

                    let l = unsafe { Layout::from_size_align_unchecked(alignednewsize, oldalignment) };
                    let newp = unsafe { self.alloc(l) };
                    debug_assert!(!newp.is_null());
                    debug_assert!(newp.is_aligned_to(oldalignment));

                    // Copy the contents from the old location.
                    unsafe { copy_nonoverlapping(ptr, newp, oldsize); }

                    // Free the old slot.
                    let (areanum, slotnum) = small_slot(offset, allslabnum, slotsize);

                    self.push_onto_small_slab_freelist(allslabnum, areanum, slotnum);

                    //eprintln!("{newp:?}");
                    return newp;
                }

                if offset < LARGE_SLAB_REGION_BASE {
                    // This is a medium slot.

                    // If the requested size fits into the current slot, just return the current
                    // pointer and we're done.
                    let alignedreqsize: usize = ((reqsize - 1) | (oldalignment - 1)) + 1;
                    let allslabnum = offset_to_allslabnum(offset);
                    let slotsize = slotsize(allslabnum);
                    if alignedreqsize <= slotsize {
                        //eprintln!("{ptr:?}");
                        return ptr;
                    }

                    debug_assert!(alignedreqsize > CACHE_LINE_SIZE);
                    // ... because that would have fit into a small slot, in the code above.

                    let newsize = if alignedreqsize <= PAGE_SIZE {
                        PAGE_SIZE
                    } else if alignedreqsize <= LARGE_SLOT_SIZE {
                        LARGE_SLOT_SIZE
                    } else {
                        alignedreqsize * 2
                    };
                    let alignednewsize: usize = ((newsize - 1) | (oldalignment - 1)) + 1;

                    let l = unsafe { Layout::from_size_align_unchecked(alignednewsize, oldalignment) };
                    let newp = unsafe { self.alloc(l) };
                    debug_assert!(!newp.is_null());
                    debug_assert!(newp.is_aligned_to(oldalignment));

                    // Copy the contents from the old location.
                    unsafe { copy_nonoverlapping(ptr, newp, oldsize); }

                    // Free the old slot.
                    let mediumslabnum = allslabnum - NUM_SMALL_SLABS;

                    let slotnum = medium_slot(offset, mediumslabnum, slotsize);
                    
                    self.push_onto_medium_slab_freelist(mediumslabnum, slotnum);

                    //eprintln!("{newp:?}");
                    return newp;
                }

                // This is a large slot.

                // If the requested size fits into the current slot, just return the current pointer
                // and we're done.
                let alignedreqsize: usize = ((reqsize - 1) | (oldalignment - 1)) + 1;
                if alignedreqsize <= LARGE_SLOT_SIZE {
                    //eprintln!("{ptr:?}");
                    return ptr;
                }

                let newsize = alignedreqsize * 2;
                let alignednewsize: usize = ((newsize - 1) | (oldalignment - 1)) + 1;

                let l = unsafe { Layout::from_size_align_unchecked(alignednewsize, oldalignment) };
                let newp = unsafe { self.alloc(l) };
                debug_assert!(!newp.is_null());
                debug_assert!(newp.is_aligned_to(oldalignment));

                // Copy the contents from the old location.
                unsafe { copy_nonoverlapping(ptr, newp, oldsize); }

                // Free the old slot.
                let slotnum = large_slot(offset);
                
                self.push_onto_large_slab_freelist(slotnum);

                //eprintln!("{newp:?}");

                newp
            }
        }
    }
}


// --- Code for development (e.g benchmarks, tests, development utilities) ---

#[cfg(target_vendor = "apple")]
pub mod plat {
    use mach_sys::mach_time::{mach_absolute_time, mach_timebase_info};
    use mach_sys::kern_return::KERN_SUCCESS;
    use std::mem::MaybeUninit;
    use thousands::Separable;
    use rand::rngs::StdRng;
    use rand::SeedableRng;
    use rand::Rng;
    use crate::platformalloc::vendor::{CACHE_SIZE, CACHE_LINE_SIZE};

    pub fn dev_measure_cache_behavior() {
        let mut mmtt: MaybeUninit<mach_timebase_info> = MaybeUninit::uninit();
        let retval = unsafe { mach_timebase_info(mmtt.as_mut_ptr()) };
        assert_eq!(retval, KERN_SUCCESS);
        let mtt = unsafe { mmtt.assume_init() };

        const BUFSIZ: usize = 1_000_000_000;
        let mut bs: Box<Vec<u8>> = Box::default();
        bs.resize(BUFSIZ, 0);

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
                // copy a byte from bs to blowoutarea
                blowoutarea[i % MEM_TO_USE] = bs[i];

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

        const BUFSIZ: usize = 1_000_000_000;
        let mut bs: Box<Vec<u8>> = Box::new(Vec::new());
        bs.resize(BUFSIZ, 0);

        let mut stride = 1;
        while stride < 50_000 {
            // Okay now we need to blow out the cache! To do that, we need
            // to touch at least NUM_CACHE_LINES different cache lines
            // that aren't the ones we want to benchmark.

            const MEM_TO_USE: usize = 100_000_000;

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
// On a linux machine (Intel(R) Xeon(R) CPU E5-2698 v4 @ 2.20GHz) with 4,608,000,000 bytes RAM with overcommit = 0, the amount I was able to mmap() varied. :-( One time I could mmap() only 93,971,598,389,248 Bytes.
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

// use bytesize::ByteSize;
// 
// fn conv(size: usize) -> String {
//     ByteSize::b(size as u64).to_string_as(true) // true for binary units (KiB, MiB, GiB, etc.)
// }
// 
// fn convsum(size: usize) -> String {
//     let logtwo = size.ilog2();
//     format!("{} ({:.3}b)", conv(size), logtwo)
// }

// xyz0 add benchmarks of high thread contention

#[cfg(test)]
mod benches {
    use crate::*;

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
    fn bench_size_to_allslabnum_lzcnt_min() {
        let mut c = plat::make_criterion();

        const NUM_ARGS: usize = 1_000_000;

        let mut r = StdRng::seed_from_u64(0);

        let mut reqs = Vec::with_capacity(NUM_ARGS);

        // Generate a distribution of sizes that is similar to realistic usages of smalloc (so that
        // our benchmarks are more representative than if we just generated some kind of even
        // distribution or something).
        while reqs.len() < NUM_ARGS {
            reqs.push(randdist_reqsiz(&mut r));
        }

        let mut i = 0;
        let mut a = 0; // to prevent compiler from optimizing stuff away
        c.bench_function("size_to_allslabnum_lzcnt_min", |b| b.iter(|| {
            a ^= black_box(crate::size_to_allslabnum_lzcnt_min(reqs[i % NUM_ARGS]));
            i += 1;
        }));
    }

    #[test]
    fn bench_size_to_allslabnum_lzcnt_branch() {
        let mut c = plat::make_criterion();

        const NUM_ARGS: usize = 1_000_000;

        let mut r = StdRng::seed_from_u64(0);

        let mut reqs = Vec::with_capacity(NUM_ARGS);

        // Generate a distribution of sizes that is similar to realistic usages of smalloc (so that
        // our benchmarks are more representative than if we just generated some kind of even
        // distribution or something).
        while reqs.len() < NUM_ARGS {
            reqs.push(randdist_reqsiz(&mut r));
        }

        let mut i = 0;
        let mut a = 0; // to prevent compiler from optimizing stuff away
        c.bench_function("size_to_allslabnum_lzcnt_branch", |b| b.iter(|| {
            a ^= black_box(crate::size_to_allslabnum_lzcnt_branch(reqs[i % NUM_ARGS]));
            i += 1;
        }));
    }

    #[test]
    fn bench_size_to_allslabnum_log_branch() {
        let mut c = plat::make_criterion();

        const NUM_ARGS: usize = 1_000_000;

        let mut r = StdRng::seed_from_u64(0);

        let mut reqs = Vec::with_capacity(NUM_ARGS);

        // Generate a distribution of sizes that is similar to realistic usages of smalloc (so that
        // our benchmarks are more representative than if we just generated some kind of even
        // distribution or something).
        while reqs.len() < NUM_ARGS {
            reqs.push(randdist_reqsiz(&mut r));
        }

        let mut i = 0;
        let mut a = 0; // to prevent compiler from optimizing stuff away
        c.bench_function("size_to_allslabnum_log_branch", |b| b.iter(|| {
            a ^= black_box(crate::size_to_allslabnum_log_branch(reqs[i % NUM_ARGS]));
            i += 1;
        }));
    }

    #[test]
    fn bench_offset_to_allslabnum_lzcnt() {
        let mut c = plat::make_criterion();

        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();
        let sybp = sm.get_sys_baseptr();
        let smbp = sm.get_sm_baseptr();

        const NUM_ARGS: usize = 1_000_000;

        let mut r = StdRng::seed_from_u64(0);

        let mut reqs = Vec::with_capacity(NUM_ARGS);

        // Generate a distribution of offsets that is similar to realistic usages of smalloc (so
        // that our benchmarks are more representative than if we just generated some kind of even
        // distribution or something).
        while reqs.len() < NUM_ARGS {
            let mut s = randdist_reqsiz(&mut r);
            if s > slotsize(NUM_SMALL_PLUS_MEDIUM_SLABS - 1) {
                s = slotsize(NUM_SMALL_PLUS_MEDIUM_SLABS - 1);
            }
            let l = Layout::from_size_align(s, 1).unwrap();
            let p = unsafe { sm.alloc(l) };
            let o = crate::offset_of_ptr(sybp, smbp, p);
            reqs.push(o.unwrap());
        }

        let mut i = 0;
        let mut a = 0; // to prevent compiler from optimizing stuff away
        c.bench_function("offset_to_allslabnum_lzcnt", |b| b.iter(|| {
            a ^= black_box(crate::offset_to_allslabnum_lzcnt(reqs[i % NUM_ARGS]));
            i += 1;
        }));
    }

    #[test]
    fn bench_offset_to_allslabnum_log() {
        let mut c = plat::make_criterion();

        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();
        let sybp = sm.get_sys_baseptr();
        let smbp = sm.get_sm_baseptr();

        const NUM_ARGS: usize = 1_000_000;

        let mut r = StdRng::seed_from_u64(0);

        let mut reqs = Vec::with_capacity(NUM_ARGS);

        // Generate a distribution of offsets that is similar to realistic usages of smalloc (so
        // that our benchmarks are more representative than if we just generated some kind of even
        // distribution or something).
        while reqs.len() < NUM_ARGS {
            let mut s = randdist_reqsiz(&mut r);
            if s > slotsize(NUM_SMALL_PLUS_MEDIUM_SLABS - 1) {
                s = slotsize(NUM_SMALL_PLUS_MEDIUM_SLABS - 1);
            }
            let l = Layout::from_size_align(s, 1).unwrap();
            let p = unsafe { sm.alloc(l) };
            let o = crate::offset_of_ptr(sybp, smbp, p);
            reqs.push(o.unwrap());
        }

        let mut i = 0;
        let mut a = 0; // to prevent compiler from optimizing stuff away
        c.bench_function("offset_to_allslabnum_log", |b| b.iter(|| {
            a ^= black_box(crate::offset_to_allslabnum_log(reqs[i % NUM_ARGS]));
            i += 1;
        }));
    }

    #[test]
    fn bench_pop_flh_small_sn_0_empty() {
        let mut c = plat::make_criterion();

        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        c.bench_function("pop_small_flh_sep_sn_0_empty", |b| b.iter(|| { // xxx temp name for comparison to prev version
            let smbp = sm.get_sm_baseptr();
            let slab_bp = unsafe { smbp.add(small_slab_area_base_offset(0)) };
// xxx include these lookups inside bench ? For comparability with smalloc v2's `pop_small_flh()`? Or not, for modeling of smalloc v3's runtime behavior ? *thinky face*
            let areanum = get_thread_areanum();
            let flha = sm.get_atomicu64(small_flh_offset(0, areanum));
            let eaca = sm.get_atomicu64(small_eac_offset(0, areanum));
            black_box(sm.inner_alloc(flha, slab_bp, eaca, 4, NUM_SMALL_SLOTS));
        }));
    }

    #[test]
    fn bench_pop_flh_small_sn_4_empty() {
        let mut c = plat::make_criterion();

        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();
        let smbp = sm.get_sm_baseptr();
        let slab_bp = unsafe { smbp.add(small_slab_base_offset(4, get_thread_areanum())) };

        c.bench_function("pop_small_flh_sn_0_empty", |b| b.iter(|| { // xxx temp name for comparison to prev version
            let areanum = get_thread_areanum();
            let flha = sm.get_atomicu64(small_flh_offset(4, areanum));
            let eaca = sm.get_atomicu64(small_eac_offset(4, areanum));
            black_box(sm.inner_alloc(flha, slab_bp, eaca, 64, NUM_SMALL_SLOTS));
        }));
    }

    use criterion::BatchSize;
    use std::sync::atomic::Ordering;
    use rand::seq::SliceRandom;

    #[derive(PartialEq)]
    enum DataOrder {
        Sequential, Random
    }
    
    fn help_bench_pop_small_slab_freelist_wdata(fnname: &str, smallslabnum: usize, ord: DataOrder, thenwrite: bool) {
        let mut c = plat::make_criterion();

        let gtan1 = get_thread_areanum();

        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        // To prime the pump for the assertion inside setup() that the free list isn't empty.
        let l = Layout::from_size_align(slotsize(smallslabnum), 1).unwrap();
        unsafe { sm.dealloc(sm.alloc(l), l) };

        let router = RefCell::new(StdRng::seed_from_u64(0));

        const NUM_ARGS: usize = 16_000;
        let setup = || {
            let mut rinner = router.borrow_mut();

            let gtan2 = get_thread_areanum();
            assert_eq!(gtan1, gtan2);

            // reset the free list and eac
            let eaca = sm.get_atomicu64(small_eac_offset(smallslabnum, gtan2));
            eaca.store(0, Ordering::Release);
            let flha = sm.get_atomicu64(small_flh_offset(smallslabnum, gtan2));

            // assert that the free list hasnt't been emptied out, which would mean that during the
            // previous batch of benchmarking, the free list ran dry and we started benchmarking the
            // "pop from empty free list" case instead of what we're trying to benchmark here.
            assert_ne!(flha.load(Ordering::Acquire) & u32::MAX as u64, 0);

            flha.store(0, Ordering::Release);
            
            let mut ps = Vec::with_capacity(NUM_ARGS);

            while ps.len() < NUM_ARGS {
                ps.push(unsafe { sm.alloc(l) })
            }

            match ord {
                DataOrder::Sequential => { }
                DataOrder::Random => {
                    ps.shuffle(&mut rinner)
                }
            }

            for p in ps.iter() {
                unsafe { sm.dealloc(*p, l) };
            }
        };

        let smbp = sm.get_sm_baseptr();

        let f = |()| {
            let gtan3 = get_thread_areanum();
            assert_eq!(gtan1, gtan3);

            let slab_bp = unsafe { smbp.add(small_slab_base_offset(smallslabnum, gtan3)) };
            let flha = sm.get_atomicu64(small_flh_offset(smallslabnum, gtan3));
            let eaca = sm.get_atomicu64(small_eac_offset(smallslabnum, gtan3));
            
            let p2 = black_box(sm.inner_alloc(flha, slab_bp, eaca, slotsize(smallslabnum), NUM_SMALL_SLOTS));
            assert!(!p2.is_null());

            if thenwrite {
                // Okay now write into the newly allocated space.
                unsafe { std::ptr::copy_nonoverlapping(&99_u8, p2, 1) };
            }
        };

        c.bench_function(fnname, move |b| b.iter_batched(setup, f, BatchSize::SmallInput));
    }

    #[test]
    fn bench_pop_small_sn_0_wdata_sequential() {
        help_bench_pop_small_slab_freelist_wdata("pop_small_flh_sep_sn_0_wdata_sequential", 0, DataOrder::Sequential, false) // xxx temp name for comparison to prev version
    }

    #[test]
    fn bench_pop_small_sn_0_wdata_sequential_then_write() {
        help_bench_pop_small_slab_freelist_wdata("pop_small_flh_sep_sn_0_wdata_sequential_then_write", 0, DataOrder::Sequential, true) // xxx temp name for comparison to prev version
    }

    #[test]
    fn bench_pop_small_sn_0_wdata_random() {
        help_bench_pop_small_slab_freelist_wdata("pop_small_flh_sep_sn_0_wdata_random", 0, DataOrder::Random, false) // xxx temp name for comparison to prev version
    }

    #[test]
    fn bench_pop_small_sn_0_wdata_random_then_write() {
        help_bench_pop_small_slab_freelist_wdata("pop_small_flh_sep_sn_0_wdata_random_then_write", 0, DataOrder::Random, true) // xxx temp name for comparison to prev version
    }

    #[test]
    fn bench_pop_small_sn_1_wdata_sequential_n() {
        help_bench_pop_small_slab_freelist_wdata("pop_small_flh_sn_4_wdata_sequential", 1, DataOrder::Sequential, false) // xxx temp name for comparison to prev version
    }

    #[test]
    fn bench_pop_small_sn_1_wdata_sequential_then_write() {
        help_bench_pop_small_slab_freelist_wdata("pop_small_flh_sn_4_wdata_sequential_then_write", 1, DataOrder::Sequential, true) // xxx temp name for comparison to prev version
    }

    #[test]
    fn bench_pop_small_sn_1_wdata_random() {
        help_bench_pop_small_slab_freelist_wdata("pop_small_flh_sn_4_wdata_random", 1, DataOrder::Random, false) // xxx temp name for comparison to prev version
    }

    #[test]
    fn bench_pop_small_sn_1_wdata_random_then_write() {
        help_bench_pop_small_slab_freelist_wdata("pop_small_flh_sn_4_wdata_random_then_write", 1, DataOrder::Random, true) // xxx temp name for comparison to prev version
    }

    #[test]
    fn bench_pop_small_sn_4_wdata_random() {
        help_bench_pop_small_slab_freelist_wdata("pop_medium_flh_sn_0_wdata_random", 4, DataOrder::Random, false); // xxx temp name for comparison to prev version
    }

    #[test]
    fn bench_pop_small_sn_4_wdata_random_then_write() {
        help_bench_pop_medium_slab_freelist_wdata("pop_medium_flh_sn_0_wdata_random_then_write", 4, DataOrder::Random, true); // xxx temp name for comparison to prev version
    }

    #[test]
    fn bench_pop_small_sn_4_wdata_sequential() {
        help_bench_pop_medium_slab_freelist_wdata("pop_medium_flh_sn_0_wdata_sequential", 4, DataOrder::Sequential, false); // xxx temp name for comparison to prev version
    }

    #[test]
    fn bench_pop_small_sn_4_wdata_sequential_then_write() {
        help_bench_pop_medium_slab_freelist_wdata("pop_medium_flh_sn_0_wdata_sequential_then_write", 4, DataOrder::Sequential, true); // xxx temp name for comparison to prev version
    }

    fn help_bench_pop_medium_slab_freelist_wdata(fnname: &str, mediumslabnum: usize, ord: DataOrder, thenwrite: bool) {
        let mut c = plat::make_criterion();

        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        // To prime the pump for the assertion inside setup() that the free list isn't empty.
        let allslabnum = mediumslabnum + NUM_SMALL_SLABS;
        let l = Layout::from_size_align(slotsize(allslabnum), 1).unwrap();
        unsafe { sm.dealloc(sm.alloc(l), l) };

        let router = RefCell::new(StdRng::seed_from_u64(0));

        const NUM_ARGS: usize = 16_000;
        let setup = || {
            let mut rinner = router.borrow_mut();

            // reset the free list and eac
            let eaca = sm.get_atomicu64(medium_eac_offset(mediumslabnum));
            eaca.store(0, Ordering::Release);
            let flha = sm.get_atomicu64(medium_flh_offset(mediumslabnum));

            // assert that the free list hasnt't been emptied out,
            // which would mean that during the previous batch of
            // benchmarking, the free list ran dry and we started
            // benchmarking the "pop from empty free list" case
            // instead of what we're trying to benchmark here.
            assert_ne!(flha.load(Ordering::Acquire) & u32::MAX as u64, 0);

            flha.store(0, Ordering::Release);
            
            let mut ps = Vec::with_capacity(NUM_ARGS);

            while ps.len() < NUM_ARGS {
                ps.push(unsafe { sm.alloc(l) })
            }

            match ord {
                DataOrder::Sequential => { }
                DataOrder::Random => {
                    ps.shuffle(&mut rinner)
                }
            }

            for p in ps.iter() {
                unsafe { sm.dealloc(*p, l) };
            }
        };

        let smbp = sm.get_sm_baseptr();

        let f = |()| {
            let slab_bp = unsafe { smbp.add(medium_slab_base_offset(mediumslabnum)) };
            let flha = sm.get_atomicu64(medium_flh_offset(mediumslabnum));
            let eaca = sm.get_atomicu64(medium_eac_offset(mediumslabnum));

            let p2 = black_box(sm.inner_alloc(flha, slab_bp, eaca, slotsize(allslabnum), NUM_MEDIUM_SLOTS));
            assert!(!p2.is_null());

            if thenwrite {
                // Okay now write into the newly allocated space.
                unsafe { std::ptr::copy_nonoverlapping(&99_u8, p2, 1) };
            }
        };

        c.bench_function(fnname, |b| b.iter_batched(setup, f, BatchSize::SmallInput));
    }

    #[test]
    fn bench_pop_medium_sn_5_wdata_random() {
        help_bench_pop_medium_slab_freelist_wdata("pop_medium_flh_sn_6_wdata_random", 5, DataOrder::Random, false); // xxx temp name for comparison to prev version
    }

    #[test]
    fn bench_pop_medium_sn_5_wdata_random_then_write() {
        help_bench_pop_medium_slab_freelist_wdata("pop_medium_flh_sn_6_wdata_random_then_write", 5, DataOrder::Random, true); // xxx temp name for comparison to prev version
    }

    #[test]
    fn bench_pop_medium_sn_5_wdata_sequential() {
        help_bench_pop_medium_slab_freelist_wdata("pop_medium_flh_sn_6_wdata_sequential", 5, DataOrder::Sequential, false); // xxx temp name for comparison to prev version
    }

    #[test]
    fn bench_pop_medium_sn_5_wdata_sequential_then_write() {
        help_bench_pop_medium_slab_freelist_wdata("pop_medium_flh_sn_6_wdata_sequential_then_write", 5, DataOrder::Sequential, true); // xxx temp name for comparison to prev version
    }

    #[test]
    fn bench_pop_medium_sn_6_wdata_random() {
        help_bench_pop_medium_slab_freelist_wdata("pop_medium_flh_sn_7_wdata_random", 6, DataOrder::Random, false); // xxx temp name for comparison to prev version
    }

    #[test]
    fn bench_pop_medium_sn_6_wdata_random_then_write() {
        help_bench_pop_medium_slab_freelist_wdata("pop_medium_flh_sn_7_wdata_random_then_write", 6, DataOrder::Random, true); // xxx temp name for comparison to prev version
    }

    #[test]
    fn bench_pop_medium_sn_6_wdata_sequential() {
        help_bench_pop_medium_slab_freelist_wdata("pop_medium_flh_sn_7_wdata_sequential", 6, DataOrder::Sequential, false); // xxx temp name for comparison to prev version
    }

    #[test]
    fn bench_pop_medium_sn_6_wdata_sequential_then_write() {
        help_bench_pop_medium_slab_freelist_wdata("pop_medium_flh_sn_7_wdata_sequential_then_write", 6, DataOrder::Sequential, true); // xxx temp name for comparison to prev version
    }

    #[test]
    fn bench_small_alloc() {
        let mut c = Criterion::default();

        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        const NUM_ARGS: usize = 50_000;

        let mut r = StdRng::seed_from_u64(0);
        let mut reqs = Vec::with_capacity(NUM_ARGS);

        while reqs.len() < NUM_ARGS {
            reqs.push(slotsize(r.random_range(0..NUM_SMALL_SLABS)));
        }

        let mut accum = 0; // to prevent compiler optimizing things away
        let mut i = 0;
        c.bench_function("small_alloc_with_overflow", |b| b.iter(|| { // xxx temp name for comparison to prev version
            let l = unsafe { Layout::from_size_align_unchecked(reqs[i % reqs.len()], 1) };
            accum ^= black_box(unsafe { sm.alloc(l) }) as u64;
            i += 1;
        }));
    }

    #[test]
    fn bench_medium_alloc() {
        let mut c = Criterion::default();

        const NUM_ARGS: usize = 50_000;

        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let mut r = StdRng::seed_from_u64(0);
        let mut reqs = Vec::with_capacity(NUM_ARGS);

        while reqs.len() < NUM_ARGS {
            reqs.push(slotsize(NUM_SMALL_SLABS + r.random_range(0..NUM_MEDIUM_SLABS)));
        }

        let mut accum = 0; // to prevent compiler optimizing things away
        let mut i = 0;
        c.bench_function("inner_medium_alloc", |b| b.iter(|| { // xxx temp name for comparison to prev version
            let l = unsafe { Layout::from_size_align_unchecked(reqs[i % reqs.len()], 1) };
            accum ^= black_box(unsafe { sm.alloc(l) }) as u64;
            i += 1
        }));
    }

    #[test]
    fn bench_ptr_to_slot() {
        let mut c = Criterion::default();

        const NUM_ARGS: usize = 50_000_000;

        let mut r = StdRng::seed_from_u64(0);
        let baseptr_for_testing: *mut u8 = null_mut();
        let mut reqptrs = Box::new(Vec::new());
        reqptrs.reserve(NUM_ARGS);
        
        while reqptrs.len() < NUM_ARGS {
            // generate a random slot
            let o = if r.random::<bool>() {
                // SmallSlot
                let areanum = r.random_range(0..NUM_SMALL_SLAB_AREAS);
                let smallslabnum = r.random_range(0..NUM_SMALL_SLABS);
                let slotnum = r.random_range(0..NUM_SMALL_SLOTS);

                small_slot_offset(smallslabnum, areanum, slotnum)
            } else {
                // medium or large slot
                let mediumslabnum = r.random_range(0..NUM_MEDIUM_SLABS + NUM_LARGE_SLABS);
                if mediumslabnum < NUM_MEDIUM_SLABS {
                    // medium slot
                    let slotnum = r.random_range(0..NUM_MEDIUM_SLOTS);
                    medium_slot_offset(mediumslabnum, slotnum)
                } else {
                    // large slot
                    let slotnum = r.random_range(0..NUM_LARGE_SLABS);
                    large_slot_offset(slotnum)
                }
            };

            // put the random slot's pointer into the test set
            reqptrs.push(unsafe { baseptr_for_testing.add(o) });
        }

        let mut accum = 0; // This is to prevent the compiler from optimizing away some of these calculations.
        let mut i = 0;
        c.bench_function("ptr_to_slot", |b| b.iter(|| { // xxx temp name for comparison to prev version
            let ptr = reqptrs[i % NUM_ARGS];

            let opto = crate::offset_of_ptr(baseptr_for_testing, baseptr_for_testing, ptr);
            let res = match opto {
                None => {
                    panic!("wrong");
                }
                Some(o) => {
                    if o < MEDIUM_SLABS_REGION_BASE {
                        // This points into the "small-slabs-areas-region".

                        let allslabnum = offset_to_allslabnum(o);
                        let slotsize = slotsize(allslabnum);

                        assert!(o.is_multiple_of(slotsize));

                        let (areanum2, slotnum2) = small_slot(o, allslabnum, slotsize);

                        black_box((allslabnum, areanum2, slotnum2))
                    } else if o < LARGE_SLAB_REGION_BASE {
                        // This points into the "medium-slabs-region".

                        let allslabnum = offset_to_allslabnum(o);
                        let slotsize = slotsize(allslabnum);

                        assert!(o.is_multiple_of(slotsize));

                        let slotnum2 = medium_slot(o, allslabnum - NUM_SMALL_SLABS, slotsize);
                        
                        black_box((allslabnum, 0, slotnum2))
                    } else {
                        // This points into the "large-slab".
                        let slotnum2 = large_slot(o);
                        
                        black_box((0, 0, slotnum2))
                    }
                }
            };

            accum += res.2;

            i += 1;
        }));
    }

    use std::sync::Arc;
    fn dummy_func() -> i64 {
        let mut a = Arc::new(0);
        for i in 0..3 {
            for j in 0..3 {
                *Arc::make_mut(&mut a) ^= black_box(i * j);
            }
        }

        *a
    }

    #[test]
    fn bench_alloc_rand() {
        let mut c = plat::make_criterion();

        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let saved_thread_areanum = get_thread_areanum();
        let r = RefCell::new(StdRng::seed_from_u64(0));

        const NUM_ARGS: usize = 1_000_000;
        let reqsouter = RefCell::new(Vec::with_capacity(NUM_ARGS));

        let setup = || {
            let areanum = get_thread_areanum();
            assert_eq!(areanum, saved_thread_areanum);
            let mut reqsinnersetup = reqsouter.borrow_mut();
            
            let mut rinner = r.borrow_mut();

            // reset the reqs vec
            reqsinnersetup.clear();

            // reset the free lists and eacs for all three size classes
            for smallslabnum in 0..NUM_SMALL_SLABS {
                let flha = sm.get_atomicu64(small_flh_offset(smallslabnum, areanum));
                let eaca = sm.get_atomicu64(small_eac_offset(smallslabnum, areanum));
                flha.store(0, Ordering::Release);
                eaca.store(0, Ordering::Release);
            }

            for mediumslabnum in 0..NUM_MEDIUM_SLABS {
                let flha = sm.get_atomicu64(medium_flh_offset(mediumslabnum));
                let eaca = sm.get_atomicu64(medium_eac_offset(mediumslabnum));
                flha.store(0, Ordering::Release);
                eaca.store(0, Ordering::Release);
            }

            let flha = sm.get_atomicu64(large_flh_offset());
            let eaca = sm.get_atomicu64(large_eac_offset());
            flha.store(0, Ordering::Release);
            eaca.store(0, Ordering::Release);
            
            while reqsinnersetup.len() < NUM_ARGS {
                let l = Layout::from_size_align(randdist_reqsiz(&mut rinner), 1).unwrap();
                reqsinnersetup.push(l);
            }
        };

        let f = |()| {
            dummy_func()
            // let mut reqsinnerf = reqsouter.borrow_mut();
            // let _l = black_box(reqsinnerf.pop().unwrap());
            //unsafe { sm.alloc(l) };
        };

        let mut g = c.benchmark_group("g");
        g.sampling_mode(criterion::SamplingMode::Linear);
        g.bench_function("alloc_rand", |b| b.iter_batched(setup, f, BatchSize::SmallInput));
    }

    fn help_bench_alloc_x_bytes(bytes: usize, fnname: &str) {
        let mut c = plat::make_criterion();

        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let saved_thread_areanum = get_thread_areanum();

        const NUM_ARGS: usize = 100_000;
        let reqsouter = RefCell::new(Vec::with_capacity(NUM_ARGS));

        let setup = || {
            let areanum = get_thread_areanum();
            assert_eq!(areanum, saved_thread_areanum);
            let mut reqsinnersetup = reqsouter.borrow_mut();
            
            // reset the reqs vec
            reqsinnersetup.clear();

            // reset the free lists and eacs for all three size classes
            for smallslabnum in 0..NUM_SMALL_SLABS {
                let flha = sm.get_atomicu64(small_flh_offset(smallslabnum, areanum));
                let eaca = sm.get_atomicu64(small_eac_offset(smallslabnum, areanum));
                flha.store(0, Ordering::Release);
                eaca.store(0, Ordering::Release);
            }

            for mediumslabnum in 0..NUM_MEDIUM_SLABS {
                let flha = sm.get_atomicu64(medium_flh_offset(mediumslabnum));
                let eaca = sm.get_atomicu64(medium_eac_offset(mediumslabnum));
                flha.store(0, Ordering::Release);
                eaca.store(0, Ordering::Release);
            }

            let flha = sm.get_atomicu64(large_flh_offset());
            let eaca = sm.get_atomicu64(large_eac_offset());
            flha.store(0, Ordering::Release);
            eaca.store(0, Ordering::Release);
            
            let l: Layout = Layout::from_size_align(bytes, 1).unwrap();
            while reqsinnersetup.len() < NUM_ARGS {
                reqsinnersetup.push(l);
            }
        };

        let f = |()| {
            let mut reqsinnerf = reqsouter.borrow_mut();
            let l = reqsinnerf.pop().unwrap();
            unsafe { sm.alloc(l) };
        };

        c.bench_function(fnname, |b| b.iter_batched(setup, f, BatchSize::SmallInput));
    }

    #[test]
    fn bench_alloc_1_byte() {
        help_bench_alloc_x_bytes(1, "alloc_1_byte");
    }
    
    #[test]
    fn bench_alloc_2_bytes() {
        help_bench_alloc_x_bytes(2, "alloc_2_bytes");
    }
    
    #[test]
    fn bench_alloc_3_bytes() {
        help_bench_alloc_x_bytes(2, "alloc_3_bytes");
    }
    
    #[test]
    fn bench_alloc_4_bytes() {
        help_bench_alloc_x_bytes(2, "alloc_4_bytes");
    }
    
    #[test]
    fn bench_alloc_5_bytes() {
        help_bench_alloc_x_bytes(2, "alloc_5_bytes");
    }
    
    #[test]
    fn bench_alloc_6_bytes() {
        help_bench_alloc_x_bytes(2, "alloc_6_bytes");
    }
    
    #[test]
    fn bench_alloc_7_bytes() {
        help_bench_alloc_x_bytes(2, "alloc_7_bytes");
    }
    
    #[test]
    fn bench_alloc_8_bytes() {
        help_bench_alloc_x_bytes(2, "alloc_8_bytes");
    }
    
    #[test]
    fn bench_alloc_9_bytes() {
        help_bench_alloc_x_bytes(2, "alloc_9_bytes");
    }
    
    #[test]
    fn bench_alloc_10_bytes() {
        help_bench_alloc_x_bytes(2, "alloc_10_bytes");
    }
    
    #[test]
    fn bench_alloc_16_bytes() {
        help_bench_alloc_x_bytes(2, "alloc_16_bytes");
    }
    
    #[test]
    fn bench_alloc_32_bytes() {
        help_bench_alloc_x_bytes(2, "alloc_32_bytes");
    }
    
    #[test]
    fn bench_alloc_64_bytes() {
        help_bench_alloc_x_bytes(2, "alloc_64_bytes");
    }
    
    #[test]
    fn bench_alloc_128_bytes() {
        help_bench_alloc_x_bytes(2, "alloc_128_bytes");
    }
    
    #[test]
    fn bench_alloc_256_bytes() {
        help_bench_alloc_x_bytes(2, "alloc_256_bytes");
    }
    
    #[test]
    fn bench_alloc_512_bytes() {
        help_bench_alloc_x_bytes(2, "alloc_512_bytes");
    }
    
    #[test]
    fn bench_alloc_1024_bytes() {
        help_bench_alloc_x_bytes(2, "alloc_1024_bytes");
    }
    
    #[test]
    fn bench_alloc_2048_bytes() {
        help_bench_alloc_x_bytes(2, "alloc_2048_bytes");
    }
    
    #[test]
    fn bench_alloc_4096_bytes() {
        help_bench_alloc_x_bytes(2, "alloc_4096_bytes");
    }
    
    #[test]
    fn bench_alloc_8192_bytes() {
        help_bench_alloc_x_bytes(2, "alloc_8192_bytes");
    }
    
    #[test]
    fn bench_alloc_16384_bytes() {
        help_bench_alloc_x_bytes(2, "alloc_16384_bytes");
    }
    
    #[test]
    fn bench_alloc_32768_bytes() {
        help_bench_alloc_x_bytes(2, "alloc_32768_bytes");
    }
    
    #[test]
    fn bench_alloc_65536_bytes() {
        help_bench_alloc_x_bytes(2, "alloc_65536_bytes");
    }
    
    #[test]
    fn bench_alloc_131072_bytes() {
        help_bench_alloc_x_bytes(2, "alloc_131072_bytes");
    }
    
    use std::cell::RefCell;
    #[test]
    fn bench_dealloc() {
        let mut c = plat::make_criterion();

        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let saved_thread_areanum = get_thread_areanum();
        let router = RefCell::new(StdRng::seed_from_u64(0));

        const NUM_ARGS: usize = 15_000;
        let allocsouter = RefCell::new(Vec::with_capacity(NUM_ARGS));

        let setup = || {
            let areanum = get_thread_areanum();
            assert_eq!(areanum, saved_thread_areanum);
            let mut rinner = router.borrow_mut();
            let mut allocsinnersetup = allocsouter.borrow_mut();

            // reset the allocs vec
            allocsinnersetup.clear();

            // reset the free lists and eacs for all three size classes

            for smallslabnum in 0..NUM_SMALL_SLABS {
                let flha = sm.get_atomicu64(small_flh_offset(smallslabnum, areanum));
                flha.store(0, Ordering::Release);
                let eaca = sm.get_atomicu64(small_eac_offset(smallslabnum, areanum));
                eaca.store(0, Ordering::Release);
            }

            for mediumslabnum in 0..NUM_MEDIUM_SLABS {
                let flha = sm.get_atomicu64(medium_flh_offset(mediumslabnum));
                flha.store(0, Ordering::Release);
                let eaca = sm.get_atomicu64(medium_eac_offset(mediumslabnum));
                eaca.store(0, Ordering::Release);
            }
            
            let flha = sm.get_atomicu64(large_flh_offset());
            flha.store(0, Ordering::Release);
            let eaca = sm.get_atomicu64(large_eac_offset());
            eaca.store(0, Ordering::Release);
            
            while allocsinnersetup.len() < NUM_ARGS {
                let l = Layout::from_size_align(randdist_reqsiz(&mut rinner), 1).unwrap();
                allocsinnersetup.push((unsafe { sm.alloc(l) }, l));
            }

            allocsinnersetup.shuffle(&mut rinner);
        };

        let f = |()| {
            let mut allocsinnerf = allocsouter.borrow_mut();
            let (p, l) = allocsinnerf.pop().unwrap();
            unsafe { sm.dealloc(p, l) };
        };

        let mut g = c.benchmark_group("g");
        g.sampling_mode(criterion::SamplingMode::Linear);
        g.bench_function("dealloc", |b| b.iter_batched(setup, f, BatchSize::SmallInput));
    }

    #[test]
    fn cache_behavior_1_1() {
        help_bench_many_accesses("bench_1_1", 1);
    }

    #[test]
    fn cache_behavior_1_2() {
        help_bench_many_accesses("bench_1_2", 2);
    }

    #[test]
    fn cache_behavior_1_3() {
        help_bench_many_accesses("bench_1_3", 3);
    }

    #[test]
    fn cache_behavior_1_4() {
        help_bench_many_accesses("bench_1_4", 4);
    }

    #[test]
    fn cache_behavior_1_5() {
        help_bench_many_accesses("bench_1_5", 5);
    }

    #[test]
    fn cache_behavior_1_6() {
        help_bench_many_accesses("bench_1_6", 6);
    }

    #[test]
    fn cache_behavior_1_8() {
        help_bench_many_accesses("bench_1_8", 8);
    }

    #[test]
    fn cache_behavior_1_9() {
        help_bench_many_accesses("bench_1_9", 9);
    }

    #[test]
    fn cache_behavior_1_10() {
        help_bench_many_accesses("bench_1_10", 10);
    }

    #[test]
    fn cache_behavior_1_16() {
        help_bench_many_accesses("bench_1_16", 16);
    }

    #[test]
    fn cache_behavior_1_32() {
        help_bench_many_accesses("bench_1_32", 32);
    }

    #[test]
    fn cache_behavior_1_64() {
        help_bench_many_accesses("bench_1_64", 64);
    }

    #[test]
    fn cache_behavior_1_128() {
        help_bench_many_accesses("bench_1_128", 128);
    }

    #[test]
    fn cache_behavior_1_256() {
        help_bench_many_accesses("bench_1_256", 256);
    }

    #[test]
    fn cache_behavior_1_512() {
        help_bench_many_accesses("bench_1_512", 512);
    }

    #[test]
    fn cache_behavior_1_1024() {
        help_bench_many_accesses("bench_1_1024", 1024);
    }

    #[test]
    fn cache_behavior_1_2048() {
        help_bench_many_accesses("bench_1_2048", 2048);
    }

    #[test]
    fn cache_behavior_1_4096() {
        help_bench_many_accesses("bench_1_4096", 4096);
    }

    #[test]
    fn cache_behavior_1_8192() {
        help_bench_many_accesses("bench_1_8192", 8192);
    }

    #[test]
    fn cache_behavior_1_16384() {
        help_bench_many_accesses("bench_1_16384", 16384);
    }

    #[test]
    fn cache_behavior_1_32768() {
        help_bench_many_accesses("bench_1_32768", 32768);
    }

    use gcd::Gcd;
    use std::cmp::min;

    /// This is intended to measure the effect of packing many allocations into few cache lines.
    fn help_bench_many_accesses(fnname: &str, alloc_size: usize) {
        let mut c = plat::make_criterion();

        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        const MEM_TO_USE: usize = CACHE_SIZE * 127 + 1_000_000;
        let max_num_args = (MEM_TO_USE / alloc_size).next_multiple_of(CACHE_LINE_SIZE);
        let max_num_slots = if alloc_size <= slotsize(NUM_SMALL_PLUS_MEDIUM_SLABS - 1) {
            NUM_MEDIUM_SLOTS
        } else {
            NUM_LARGE_SLOTS
        };
        let num_args = min(max_num_args, max_num_slots);
        
        assert!(num_args <= NUM_MEDIUM_SLOTS, "{num_args} <= {NUM_MEDIUM_SLOTS}, MEM_TO_USE: {MEM_TO_USE}, CACHE_SIZE: {CACHE_SIZE}, CACHE_LINE_SIZE: {CACHE_LINE_SIZE}, alloc_size: {alloc_size}");

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

// xyz0 teach criterion config that these take more threads
    // #[test]
    // fn bench_threads_1_large_alloc_dealloc_x() {
    //     let mut c = plat::make_criterion();

    //     let mut i = 0;
    //     c.bench_function("size_to_allslabnum_lzcnt_min", |b| b.iter(|| {
    //         crate::tests::help_test_multithreaded(1, 100, SizeClass::Large, true, true, false);
    //         i += 1;
    //     }));

    // }

    // #[test]
    // fn bench_threads_2_large_alloc_dealloc_x() {
    //     let mut c = plat::make_criterion();

    //     let mut i = 0;
    //     c.bench_function("size_to_allslabnum_lzcnt_min", |b| b.iter(|| {
    //         crate::tests::help_test_multithreaded(2, 100, SizeClass::Large, true, true, false);
    //         i += 1;
    //     }));

    // }

    // #[test]
    // fn bench_threads_10_large_alloc_dealloc_x() {
    //     let mut c = plat::make_criterion();

    //     let mut i = 0;
    //     c.bench_function("size_to_allslabnum_lzcnt_min", |b| b.iter(|| {
    //         crate::tests::help_test_multithreaded(10, 100, SizeClass::Large, true, true, false);
    //         i += 1;
    //     }));

    // }

    // #[test]
    // fn bench_threads_100_large_alloc_dealloc_x() {
    //     let mut c = plat::make_criterion();

    //     let mut i = 0;
    //     c.bench_function("size_to_allslabnum_lzcnt_min", |b| b.iter(|| {
    //         crate::tests::help_test_multithreaded(100, 100, SizeClass::Large, true, true, false);
    //         i += 1;
    //     }));

    // }

    // #[test]
    // fn bench_threads_1000_large_alloc_dealloc_x() {
    //     let mut c = plat::make_criterion();

    //     let mut i = 0;
    //     c.bench_function("size_to_allslabnum_lzcnt_min", |b| b.iter(|| {
    //         crate::tests::help_test_multithreaded(1000, 100, SizeClass::Large, true, true, false);
    //         i += 1;
    //     }));

    // }

    // use std::sync::Arc;
    // use std::thread;
    // pub fn help_bench_multithreaded(numthreads: u32, numiters: u32, sc: SizeClass, dealloc: bool, realloc: bool, writes: bool) {
    //     let sm = Arc::new(Smalloc::new());
    //     sm.idempotent_init().unwrap();

    //     let mut handles = Vec::new();
    //     for _i in 0..numthreads {
    //         let smc = Arc::clone(&sm);
    //         handles.push(thread::spawn(move || {
    //             let r = StdRng::seed_from_u64(0);
    //             help_test(&smc, numiters, sc, r, dealloc, realloc, writes);
    //         }));
    //     }

    //     for handle in handles {
    //         handle.join().unwrap();
    //     }
    // }

}

#[cfg(test)]
mod tests {
    // xxx add tests for realloc?
    use super::*;
    use std::cmp::min;
    use std::sync::Arc;

    pub const BYTES1: [u8; 8] = [1, 2, 4, 3, 5, 6, 7, 8];
    const BYTES2: [u8; 8] = [9, 8, 7, 6, 5, 4, 3, 2];
    const BYTES3: [u8; 8] = [0xA, 0xB, 0xC, 0xD, 0xE, 0xF, 0x10, 0x11];
    const BYTES4: [u8; 8] = [0x12, 0x11, 0x10, 0xF, 0xE, 0xD, 0xC, 0xB];
    const BYTES5: [u8; 8] = [0xFE, 0xFD, 0xFC, 0xFB, 0xFA, 0xF9, 0xF8, 0xF7];
    const BYTES6: [u8; 8] = [0xFE, 0xFD, 0xF6, 0xF5, 0xFA, 0xF9, 0xF8, 0xF7];

    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};

    #[derive(Copy, Clone, Debug)]
    enum SizeClass {
        Small,
        Medium,
        Large,
    }

    #[test]
    fn offset_to_allslabnum_log() {
        let mut testvecs = Vec::new();

        let s = SMALLEST_SLOT_SIZE;
        let nsslabas = NUM_SMALL_SLAB_AREAS;
        let nsslots = NUM_SMALL_SLOTS;

        testvecs.push((0, 0));
        testvecs.push((s, 0));
        testvecs.push((s * 2, 0));

        let mut boundary = 0;
        let mut allslabnum = 0;
        let mut exp = 0;

        while allslabnum < NUM_SMALL_SLABS + NUM_MEDIUM_SLABS {
            boundary += nsslabas * nsslots * s * 2usize.pow(exp);
            exp += 1;
            
            testvecs.push((boundary - s * 2, allslabnum));
            testvecs.push((boundary - s, allslabnum));

            allslabnum += 1;

            if boundary < LARGE_SLAB_REGION_BASE {
                testvecs.push((boundary, allslabnum));
                testvecs.push((boundary + s, allslabnum));
            }
        }

        for (o, s) in testvecs.iter() {
            assert_eq!(crate::offset_to_allslabnum_log(*o), *s, "{o} {s}");
        }
    }

    #[test]
    fn offset_to_allslabnum_lzcnt() {
        let mut testvecs = Vec::new();

        let s = SMALLEST_SLOT_SIZE;
        let nsslabas = NUM_SMALL_SLAB_AREAS;
        let nsslots = NUM_SMALL_SLOTS;

        testvecs.push((0, 0));
        testvecs.push((s, 0));
        testvecs.push((s * 2, 0));

        let mut boundary = 0;
        let mut allslabnum = 0;
        let mut exp = 0;

        while allslabnum < NUM_SMALL_SLABS + NUM_MEDIUM_SLABS {
            boundary += nsslabas * nsslots * s * 2usize.pow(exp);
            exp += 1;
            
            testvecs.push((boundary - s * 2, allslabnum));
            testvecs.push((boundary - s, allslabnum));

            allslabnum += 1;

            if boundary < LARGE_SLAB_REGION_BASE {
                testvecs.push((boundary, allslabnum));
                testvecs.push((boundary + s, allslabnum));
            }
        }

        for (o, s) in testvecs.iter() {
            assert_eq!(crate::offset_to_allslabnum_lzcnt(*o), *s, "{o} {s}");
        }
    }

    #[test]
    fn size_to_allslabnum() {
        let mut testvecs = Vec::new();

        testvecs.push((1, 0));
        testvecs.push((2, 0));
        testvecs.push((3, 0));
        testvecs.push((4, 0));

        testvecs.push((5, 1));
        testvecs.push((6, 1));
        testvecs.push((7, 1));
        testvecs.push((8, 1));

        testvecs.push((9, 2));
        testvecs.push((10, 2));
        testvecs.push((11, 2));
        testvecs.push((12, 2));
        testvecs.push((13, 2));
        testvecs.push((14, 2));
        testvecs.push((15, 2));
        testvecs.push((16, 2));
        
        testvecs.push((17, 3));
        testvecs.push((31, 3));
        testvecs.push((32, 3));

        testvecs.push((33, 4));

        testvecs.push((63, 4));
        testvecs.push((64, 4));

        testvecs.push((65, 5));

        testvecs.push((127, 5));
        testvecs.push((128, 5));

        testvecs.push((129, 6));

        testvecs.push((16383, 12));
        testvecs.push((16384, 12));

        testvecs.push((16385, 13));

        testvecs.push((32767, 13));

        for (si, sn) in testvecs.iter() {
            assert_eq!(crate::size_to_allslabnum(*si), *sn, "{si}");
        }
    }

    #[test]
    fn test_large_slot() {
        assert_eq!(large_slot(LARGE_SLAB_REGION_BASE), 0);
        assert_eq!(large_slot(LARGE_SLAB_REGION_BASE + LARGE_SLOT_SIZE), 1);
        assert_eq!(large_slot(LARGE_SLAB_REGION_BASE + LARGE_SLOT_SIZE * 2), 2);
        assert_eq!(large_slot(LARGE_SLAB_REGION_BASE + LARGE_SLOT_SIZE * (NUM_LARGE_SLOTS - 1)), (NUM_LARGE_SLOTS - 1));
    }

    #[test]
    fn test_large_slot_offset() {
        assert_eq!(0, large_slot(LARGE_SLAB_REGION_BASE));
        assert_eq!(1, large_slot(LARGE_SLAB_REGION_BASE + LARGE_SLOT_SIZE));
        assert_eq!(2, large_slot(LARGE_SLAB_REGION_BASE + LARGE_SLOT_SIZE * 2));
        assert_eq!((NUM_LARGE_SLOTS - 1), large_slot(LARGE_SLAB_REGION_BASE + LARGE_SLOT_SIZE * (NUM_LARGE_SLOTS - 1)));
    }

    #[test]
    fn test_offset_to_ptr() {
        let sybp: *mut u8 = 1_000_000 as *mut u8;
        let smbp: *mut u8 = 1_000_000 as *mut u8;
        let ptr: *mut u8 = 1_000_000 as *mut u8;

        assert_eq!(0, offset_of_ptr(sybp, smbp, ptr).unwrap());
    }

    #[test]
    fn one_alloc_and_dealloc_small() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let l = Layout::from_size_align(6, 1).unwrap();
        let p = unsafe { sm.alloc(l) };
        unsafe { sm.dealloc(p, l) };
    }

    #[test]
    fn one_alloc_and_dealloc_medium() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let l = Layout::from_size_align(120, 4).unwrap();
        let p = unsafe { sm.alloc(l) };
        unsafe { sm.dealloc(p, l) };
    }

    #[test]
    fn one_alloc_and_dealloc_large() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let l = Layout::from_size_align(1_000_000, 8).unwrap();
        let p = unsafe { sm.alloc(l) };
        unsafe { sm.dealloc(p, l) };
    }

    #[test]
    fn one_large_alloc_and_realloc_to_large() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let l1 = Layout::from_size_align(1_000_000, 8).unwrap();
        let p1 = unsafe { sm.alloc(l1) };
        assert!(!p1.is_null());

        let reqsize: usize = 2_000_000;
        let p2 = unsafe { sm.realloc(p1, l1, reqsize) };
        assert!(!p2.is_null());
    }
    
    #[test]
    fn one_medium_alloc_and_realloc_to_large() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let l1 = Layout::from_size_align(300, 8).unwrap();
        let p1 = unsafe { sm.alloc(l1) };
        assert!(!p1.is_null());

        let reqsize: usize = 2_000_000;
        let p2 = unsafe { sm.realloc(p1, l1, reqsize) };
        assert!(!p2.is_null());
    }
    
    #[test]
    fn one_large_alloc_and_realloc_to_oversize() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let l1 = Layout::from_size_align(1_000_000, 8).unwrap();
        let p1 = unsafe { sm.alloc(l1) };
        assert!(!p1.is_null());

        let reqsize: usize = 100_000_000;
        let p2 = unsafe { sm.realloc(p1, l1, reqsize) };
        assert!(!p2.is_null());
    }
    
    #[test]
    fn dont_buffer_overrun() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        // Allocate NUM_LARGE_SLOTS of the huge slots, then figure out if the highest-addressed byte
        // in that slot would exceed the TOTAL_VIRTUAL_MEMORY.
        
        let mut i = NUM_LARGE_SLOTS - 4;

        let eaca = sm.get_atomicu64(large_eac_offset());
        eaca.store(i as u64, Ordering::Release);

        let siz = LARGE_SLOT_SIZE;
        let layout = Layout::from_size_align(siz, 1).unwrap();
        let mut highestp: *mut u8 = unsafe { sm.alloc(layout) };
        i += 1;
        while i < NUM_LARGE_SLOTS {
            let p = unsafe { sm.alloc(layout) };
            assert!(p > highestp, "p: {p:?}, highestp: {highestp:?}");
            highestp = p;
            i += 1;
        }

        let highest_addr = highestp.addr() + siz - 1;

        let delta = highest_addr - sm.get_sys_baseptr().addr();
        
        eprintln!("highest_addr: {}, delta: {}, TOTAL_VIRTUAL_MEMORY: {}, TOTAL_VIRTUAL_MEMORY-delta: {}", highest_addr, delta, TOTAL_VIRTUAL_MEMORY, TOTAL_VIRTUAL_MEMORY-delta);
        assert!(delta < TOTAL_VIRTUAL_MEMORY);
    }

    #[test]
    fn one_alloc_slot_large() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let l = Layout::from_size_align(1_000_000, 8).unwrap();
        unsafe { sm.alloc(l) };
    }

    #[test]
    fn max_size_roundtrip_to_slot() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let l = Layout::from_size_align(LARGE_SLOT_SIZE, 1).unwrap();
        let p = unsafe { sm.alloc(l) };

        let optoffset = offset_of_ptr(sm.get_sys_baseptr(), sm.get_sm_baseptr(), p);
        assert!(optoffset.is_some());

        let offset = optoffset.unwrap();
        assert!(offset >= LARGE_SLAB_REGION_BASE);

        let slotnum = large_slot(offset);
        assert_eq!(slotnum, 0);
    }

    #[test]
    fn a_few_allocs_and_a_dealloc_for_each_small_slab() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        for smallslabnum in 0..NUM_SMALL_SLABS {
            help_small_alloc_singlethreaded(&sm, smallslabnum);
        }
    }

    #[test]
    fn a_few_allocs_and_a_dealloc_for_each_medium_slab() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        for mediumslabnum in 0..NUM_MEDIUM_SLABS {
            help_medium_alloc_singlethreaded(&sm, mediumslabnum);
        }
    }

    #[test]
    fn a_few_allocs_and_a_dealloc_for_the_large_slab() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        help_large_alloc(&sm);
    }

    /// This reproduces a bug in `platform::vendor::sys_realloc()` /
    /// `_sys_realloc_if_vm_remap_did_what_i_want()` (or possibly in MacOS's `mach_vm_remap()`) that
    /// was uncovered by tests::threads_1_large_alloc_dealloc_realloc_x()
    #[test]
    fn oversize_realloc_down_realloc_back_up() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let l1 = Layout::from_size_align(LARGE_SLOT_SIZE * 2, 1).unwrap();
        let l2 = Layout::from_size_align(LARGE_SLOT_SIZE, 1).unwrap();

        let p1 = unsafe { sm.alloc(l1) };
        assert!(!p1.is_null());
        let p2 = unsafe { sm.realloc(p1, l1, LARGE_SLOT_SIZE) };
        assert!(!p2.is_null());
        let p3 = unsafe { sm.realloc(p2, l2, LARGE_SLOT_SIZE * 2) };
        assert!(!p3.is_null());
    }

    /// Generate a number of requests (size+alignment) that fit into the given small slab and for
    /// each request call help_small_alloc_four_times_singlethreaded()
    fn help_small_alloc_singlethreaded(sm: &Smalloc, smallslabnum: usize) {
        let smallest = if smallslabnum == 0 {
            1
        } else {
            crate::slotsize(smallslabnum - 1) + 1
        };
        let slotsize = crate::slotsize(smallslabnum);
        let largest = slotsize;
        for reqsize in smallest..=largest {
            // Generate alignments
            let mut reqalign = 1;
            loop {
                // Test this size/align combo
                help_small_alloc_four_times_singlethreaded(sm, reqsize, reqalign);
                reqalign *= 2;
                let alignedsize: usize = ((reqsize - 1) | (reqalign - 1)) + 1;
                if alignedsize > slotsize {
                    break;
                };
            }
        }
    }

    /// Generate a number of requests (size+alignment) that fit into the given medium slab and for
    /// each request call help_medium_alloc_four_times_singlethreaded()
    fn help_medium_alloc_singlethreaded(sm: &Smalloc, mediumslabnum: usize) {
        let allslabnum = NUM_SMALL_SLABS + mediumslabnum;
        let smallest = crate::slotsize(allslabnum - 1) + 1;
        let slotsize = crate::slotsize(allslabnum);
        for reqsize in [ smallest, smallest + 1, smallest + 2, slotsize - 3, slotsize - 1, slotsize, ] {
            // Generate alignments
            let mut reqalign = 1;
            loop {
                // Test this size/align combo
                help_medium_alloc_four_times_singlethreaded(sm, reqsize, reqalign);
                reqalign *= 2;
                let alignedsize: usize = ((reqsize - 1) | (reqalign - 1)) + 1;
                if alignedsize > slotsize || alignedsize > MAX_ALIGNMENT {
                    break;
                };
            }
        }
    }

    /// Generate a number of requests (size+alignment) that fit into a large slab and for each
    /// request call help_large_alloc_four_times()
    fn help_large_alloc(sm: &Smalloc) {
        let smallest = crate::slotsize(NUM_SMALL_PLUS_MEDIUM_SLABS - 1) + 1;
        let slotsize = LARGE_SLOT_SIZE;
        for reqsize in [ smallest, smallest + 1, smallest + 2, slotsize - 3, slotsize - 1, slotsize, ] {
            // Generate alignments
            let mut reqalign = 1;
            loop {
                // Test this size/align combo
                help_large_alloc_four_times(sm, reqsize, reqalign);
                reqalign *= 2;
                let alignedsize: usize = ((reqsize - 1) | (reqalign - 1)) + 1;
                if alignedsize > slotsize || alignedsize > MAX_ALIGNMENT {
                    break;
                };
            }
        }
    }

    /// Allocate this size+align three times, then free the middle one, then allocate a fourth time,
    /// then assert that only three more have been ever-allocated. This test code asserts things
    /// about the internal consistency of the smalloc data based on the assumption that this is the
    /// only thread currently updating smalloc.
    fn help_small_alloc_four_times_singlethreaded(sm: &Smalloc, reqsize: usize, reqalign: usize) {
        debug_assert!(reqsize > 0);
        debug_assert!(reqsize <= crate::slotsize(NUM_SMALL_SLABS - 1));

        let sybp = sm.get_sys_baseptr();
        let smbp = sm.get_sm_baseptr();

        let allslabnum = crate::size_to_allslabnum(reqsize);

        let eaca = sm.get_atomicu64(small_eac_offset(allslabnum, get_thread_areanum()));
        let orig_eac_val = eaca.load(Ordering::Acquire);

        let l = Layout::from_size_align(reqsize, reqalign).unwrap();
        
        let orig_this_thread_areanum = get_thread_areanum();

        let p1 = unsafe { sm.alloc(l) };
        let p1o = offset_of_ptr(sybp, smbp, p1).unwrap();
        assert!(p1o < MEDIUM_SLABS_REGION_BASE, "should have returned a small slot");
        let allslabnum1 = crate::offset_to_allslabnum(p1o);
        assert_eq!(allslabnum1, allslabnum);
        let (areanum1, slotnum1) = small_slot(p1o, allslabnum1, crate::slotsize(allslabnum1));

        assert_eq!(areanum1, orig_this_thread_areanum);

        let p2 = unsafe { sm.alloc(l) };
        let p2o = offset_of_ptr(sybp, smbp, p2).unwrap();
        assert!(p2o < MEDIUM_SLABS_REGION_BASE, "should have returned a small slot");
        let allslabnum2 = crate::offset_to_allslabnum(p2o);
        assert_eq!(allslabnum2, allslabnum);
        let (areanum2, slotnum2) = small_slot(p2o, allslabnum2, crate::slotsize(allslabnum2));

        assert_eq!(areanum2, orig_this_thread_areanum);
        assert_eq!(slotnum2, slotnum1 + 1);

        let p3 = unsafe { sm.alloc(l) };
        let p3o = offset_of_ptr(sybp, smbp, p3).unwrap();
        assert!(p3o < MEDIUM_SLABS_REGION_BASE, "should have returned a small slot");
        let allslabnum3 = crate::offset_to_allslabnum(p3o);
        assert_eq!(allslabnum3, allslabnum);
        let (areanum3, slotnum3) = small_slot(p3o, allslabnum3, crate::slotsize(allslabnum3));

        assert_eq!(areanum3, orig_this_thread_areanum);
        assert_eq!(slotnum3, slotnum1 + 2);

        // Now free the middle one.
        unsafe { sm.dealloc(p2, l) };

        // And allocate another one.
        let p4 = unsafe { sm.alloc(l) };
        let p4o = offset_of_ptr(sybp, smbp, p4).unwrap();
        assert!(p4o < MEDIUM_SLABS_REGION_BASE, "should have returned a small slot");
        let allslabnum4 = crate::offset_to_allslabnum(p4o);
        assert_eq!(allslabnum4, allslabnum);
        let (areanum4, slotnum4) = small_slot(p4o, allslabnum4, crate::slotsize(allslabnum4));

        assert_eq!(areanum4, orig_this_thread_areanum);
        assert_eq!(slotnum4, slotnum1 + 1);

        // And assert that only three more than before have been ever-allocated:
        assert_eq!(eaca.load(Ordering::Acquire), orig_eac_val + 3);
    }

    /// Allocate this size+align three times, then free the middle one, then allocate a fourth time,
    /// then assert that only three more have been ever-allocated. This test code asserts things
    /// about the internal consistency of the smalloc data based on the assumption that this is the
    /// only thread currently updating smalloc.
    fn help_medium_alloc_four_times_singlethreaded(sm: &Smalloc, reqsize: usize, reqalign: usize) {
        debug_assert!(reqsize > crate::slotsize(NUM_SMALL_SLABS - 1));
        debug_assert!(reqsize <= crate::slotsize(NUM_SMALL_PLUS_MEDIUM_SLABS - 1));

        let sybp = sm.get_sys_baseptr();
        let smbp = sm.get_sm_baseptr();

        let allslabnum = crate::size_to_allslabnum(reqsize);

        let l = Layout::from_size_align(reqsize, reqalign).unwrap();
        
        let p1 = unsafe { sm.alloc(l) };
        let p1o = offset_of_ptr(sybp, smbp, p1).unwrap();
        assert!(p1o < LARGE_SLAB_REGION_BASE, "should have returned a medium slot");
        assert!(p1o >= MEDIUM_SLABS_REGION_BASE, "should have returned a medium slot");
        let allslabnum1 = crate::offset_to_allslabnum(p1o);
        assert_eq!(allslabnum1, allslabnum);
        let mediumslabnum1 = allslabnum1 - NUM_SMALL_SLABS;
        let _slotnum1 = medium_slot(p1o, mediumslabnum1, crate::slotsize(allslabnum1));

        let p2 = unsafe { sm.alloc(l) };
        let p2o = offset_of_ptr(sybp, smbp, p2).unwrap();
        assert!(p2o < LARGE_SLAB_REGION_BASE, "should have returned a medium slot");
        assert!(p2o >= MEDIUM_SLABS_REGION_BASE, "should have returned a medium slot");
        let allslabnum2 = crate::offset_to_allslabnum(p2o);
        assert_eq!(allslabnum2, allslabnum);
        let mediumslabnum2 = allslabnum2 - NUM_SMALL_SLABS;
        let slotnum2 = medium_slot(p2o, mediumslabnum2, crate::slotsize(allslabnum2));

        let p3 = unsafe { sm.alloc(l) };
        let p3o = offset_of_ptr(sybp, smbp, p3).unwrap();
        assert!(p3o < LARGE_SLAB_REGION_BASE, "should have returned a medium slot");
        assert!(p3o >= MEDIUM_SLABS_REGION_BASE, "should have returned a medium slot");
        let allslabnum3 = crate::offset_to_allslabnum(p3o);
        assert_eq!(allslabnum3, allslabnum);
        let mediumslabnum3 = allslabnum3 - NUM_SMALL_SLABS;
        let _slotnum3 = medium_slot(p3o, mediumslabnum3, crate::slotsize(allslabnum3));

        // Now free the middle one.
        unsafe { sm.dealloc(p2, l) };

        // And allocate another one.
        let p4 = unsafe { sm.alloc(l) };
        let p4o = offset_of_ptr(sybp, smbp, p4).unwrap();
        assert!(p4o < LARGE_SLAB_REGION_BASE, "should have returned a medium slot");
        assert!(p4o >= MEDIUM_SLABS_REGION_BASE, "should have returned a medium slot");
        let allslabnum4 = crate::offset_to_allslabnum(p4o);
        assert_eq!(allslabnum4, allslabnum);
        let mediumslabnum4 = allslabnum4 - NUM_SMALL_SLABS;
        let slotnum4 = medium_slot(p4o, mediumslabnum4, crate::slotsize(allslabnum4));
        assert_eq!(slotnum4, slotnum2); // It reused that slot
    }

    /// Allocate this size+align three times, then free the middle one, then allocate a fourth time,
    /// then assert that only three more have been ever-allocated. This test code asserts things
    /// about the internal consistency of the smalloc data based on the assumption that this is the
    /// only thread currently updating smalloc.
    fn help_large_alloc_four_times(sm: &Smalloc, reqsize: usize, reqalign: usize) {
        debug_assert!(reqsize > crate::slotsize(NUM_SMALL_PLUS_MEDIUM_SLABS - 1));
        debug_assert!(reqsize <= LARGE_SLOT_SIZE);

        let sybp = sm.get_sys_baseptr();
        let smbp = sm.get_sm_baseptr();

        let eaca = sm.get_atomicu64(large_eac_offset());
        let orig_eac_val = eaca.load(Ordering::Acquire);

        let l = Layout::from_size_align(reqsize, reqalign).unwrap();

        let p1 = unsafe { sm.alloc(l) };
        let p1o = offset_of_ptr(sybp, smbp, p1).unwrap();
        assert!(p1o < SMALL_SLABS_VARS_REGION_BASE, "should have returned a large slot");
        assert!(p1o >= LARGE_SLAB_REGION_BASE, "should have returned a large slot");
        let _slotnum1 = large_slot(p1o);

        let p2 = unsafe { sm.alloc(l) };
        let p2o = offset_of_ptr(sybp, smbp, p2).unwrap();
        assert!(p2o < SMALL_SLABS_VARS_REGION_BASE, "should have returned a large slot");
        assert!(p2o >= LARGE_SLAB_REGION_BASE, "should have returned a large slot");
        let slotnum2 = large_slot(p2o);

        let p3 = unsafe { sm.alloc(l) };
        let p3o = offset_of_ptr(sybp, smbp, p3).unwrap();
        assert!(p3o < SMALL_SLABS_VARS_REGION_BASE, "should have returned a large slot");
        assert!(p3o >= LARGE_SLAB_REGION_BASE, "should have returned a large slot");
        let _slotnum3 = large_slot(p3o);

        // Now free the middle one.
        unsafe { sm.dealloc(p2, l) };

        // And allocate another one.
        let p4 = unsafe { sm.alloc(l) };
        let p4o = offset_of_ptr(sybp, smbp, p4).unwrap();
        assert!(p4o < SMALL_SLABS_VARS_REGION_BASE, "should have returned a large slot");
        assert!(p4o >= LARGE_SLAB_REGION_BASE, "should have returned a large slot");
        let slotnum4 = large_slot(p4o);

        assert_eq!(slotnum2, slotnum4);

        // And assert that only three more have been ever-allocated:
        assert_eq!(eaca.load(Ordering::Acquire), orig_eac_val + 3);
    }

    #[test]
    fn test_alloc_1_byte_then_dealloc() {
        let sm = Smalloc::new();
        let layout = Layout::from_size_align(1, 1).unwrap();
        let p = unsafe { sm.alloc(layout) };
        unsafe { sm.dealloc(p, layout) };
    }

    #[test]
    fn roundtrip_slot_to_ptr_to_slot() {
        let baseptr_for_testing: *mut u8 = 2usize.pow(40) as *mut u8;

        // First the small-slabs region:
        for areanum in [ 1, 2, NUM_SMALL_SLAB_AREAS - 3, NUM_SMALL_SLAB_AREAS - 2, NUM_SMALL_SLAB_AREAS - 1,
        ] {
            for smallslabnum in 0..NUM_SMALL_SLABS {
                for slotnum in [ 0, 1, 2, 253, 254, 255, 256, 257, 1022, 1023, 1024, 2usize.pow(16) - 1, 2usize.pow(16), 2usize.pow(16) + 1, NUM_SMALL_SLOTS - 2, NUM_SMALL_SLOTS - 1, ] {
                    let offset = small_slot_offset(smallslabnum, areanum, slotnum);
                    assert!(offset < MEDIUM_SLABS_REGION_BASE);
                    let p = unsafe { baseptr_for_testing.add(offset) };
                    let offset2 = offset_of_ptr(baseptr_for_testing, baseptr_for_testing, p).unwrap();
                    assert_eq!(offset, offset2);

                    let allslabnum = crate::offset_to_allslabnum(offset2);
                    let slotsize = slotsize(allslabnum);

                    assert!(offset.is_multiple_of(slotsize));

                    let (areanum2, slotnum2) = small_slot(offset, allslabnum, slotsize);
                    assert_eq!(areanum, areanum2);
                    assert_eq!(slotnum, slotnum2);
                }
            }
        }

        // Then the medium-slabs region:
        for mediumslabnum in 0..NUM_MEDIUM_SLABS {
            for slotnum in [ 0, 1, 2, 253, 254, 255, 256, 257, 1022, 1023, 1024, 2usize.pow(16) - 1, 2usize.pow(16), 2usize.pow(16) + 1, NUM_MEDIUM_SLOTS - 2, NUM_MEDIUM_SLOTS - 1, ] {
                let offset = medium_slot_offset(mediumslabnum, slotnum);
                assert!(offset < LARGE_SLAB_REGION_BASE);
                let p = unsafe { baseptr_for_testing.add(offset) };
                let offset2 = offset_of_ptr(baseptr_for_testing, baseptr_for_testing, p).unwrap();
                assert_eq!(offset, offset2);

                let allslabnum = crate::offset_to_allslabnum(offset2);
                let slotsize = slotsize(allslabnum);

                assert!(offset.is_multiple_of(slotsize));

                let slotnum2 = medium_slot(offset, allslabnum - NUM_SMALL_SLABS, slotsize);
                assert_eq!(slotnum, slotnum2);
            }
        }

        // Then the large slab:
        for slotnum in [ 0, 1, 2, 253, 254, 255, 256, 257, 1022, 1023, 1024, 2usize.pow(16) - 1, 2usize.pow(16), 2usize.pow(16) + 1, NUM_LARGE_SLOTS - 2, NUM_LARGE_SLOTS - 1, ] {
            let offset = large_slot_offset(slotnum);
            assert!(offset < SMALL_SLABS_VARS_REGION_BASE);
            let p = unsafe { baseptr_for_testing.add(offset) };
            let offset2 = offset_of_ptr(baseptr_for_testing, baseptr_for_testing, p).unwrap();
            assert_eq!(offset, offset2);

            assert!(offset.is_multiple_of(LARGE_SLOT_SIZE));

            let slotnum2 = large_slot(offset);
            assert_eq!(slotnum, slotnum2);
        }
    }

    use std::thread;

    #[test]
    fn main_thread_init() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();
    }

    #[test]
    fn threads_1_small_alloc_x() {
        help_test_multithreaded(1, 100, SizeClass::Small, false, false, false);
    }

    #[test]
    fn threads_1_small_alloc_dealloc_x() {
        help_test_multithreaded(1, 100, SizeClass::Small, true, false, false);
    }

    #[test]
    fn threads_1_small_alloc_dealloc_realloc_x() {
        help_test_multithreaded(1, 100, SizeClass::Small, true, true, false);
    }

    #[test]
    fn threads_1_small_alloc_dealloc_with_writes_x() {
        help_test_multithreaded(1, 100, SizeClass::Small, true, false, true);
    }

    #[test]
    fn threads_1_small_alloc_dealloc_realloc_with_writes_x() {
        help_test_multithreaded(1, 100, SizeClass::Small, true, true, true);
    }

    #[test]
    fn threads_2_small_alloc_x() {
        help_test_multithreaded(2, 100, SizeClass::Small, false, false, false);
    }

    #[test]
    fn threads_2_small_alloc_dealloc_x() {
        help_test_multithreaded(2, 100, SizeClass::Small, true, false, false);
    }

    #[test]
    fn threads_2_small_alloc_dealloc_realloc_x() {
        help_test_multithreaded(2, 100, SizeClass::Small, true, true, false);
    }

    #[test]
    fn threads_2_small_alloc_dealloc_with_writes_x() {
        help_test_multithreaded(2, 100, SizeClass::Small, true, false, true);
    }

    #[test]
    fn threads_2_small_alloc_dealloc_realloc_with_writes_x() {
        help_test_multithreaded(2, 100, SizeClass::Small, true, true, true);
    }

    #[test]
    fn threads_32_small_alloc_x() {
        help_test_multithreaded(32, 100, SizeClass::Small, false, false, false);
    }

    #[test]
    fn threads_32_small_alloc_dealloc_x() {
        help_test_multithreaded(32, 100, SizeClass::Small, true, false, false);
    }

    #[test]
    fn threads_32_small_alloc_dealloc_realloc_x() {
        help_test_multithreaded(32, 100, SizeClass::Small, true, true, false);
    }

    #[test]
    fn threads_32_small_alloc_dealloc_with_writes_x() {
        help_test_multithreaded(32, 100, SizeClass::Small, true, false, true);
    }

    #[test]
    fn threads_32_small_alloc_dealloc_realloc_with_writes_x() {
        help_test_multithreaded(32, 100, SizeClass::Small, true, true, true);
    }

    #[test]
    fn threads_64_small_alloc_x() {
        help_test_multithreaded(64, 100, SizeClass::Small, false, false, false);
    }

    #[test]
    fn threads_64_small_alloc_dealloc_x() {
        help_test_multithreaded(64, 100, SizeClass::Small, true, false, false);
    }

    #[test]
    fn threads_64_small_alloc_dealloc_realloc_x() {
        help_test_multithreaded(64, 100, SizeClass::Small, true, true, false);
    }

    #[test]
    fn threads_64_small_alloc_dealloc_with_writes_x() {
        help_test_multithreaded(64, 100, SizeClass::Small, true, false, true);
    }

    #[test]
    fn threads_64_small_alloc_dealloc_realloc_with_writes_x() {
        help_test_multithreaded(64, 100, SizeClass::Small, true, true, true);
    }

    #[test]
    fn threads_1_medium_alloc_x() {
        help_test_multithreaded(1, 100, SizeClass::Medium, false, false, false);
    }

    #[test]
    fn threads_1_medium_alloc_dealloc_x() {
        help_test_multithreaded(1, 100, SizeClass::Medium, true, false, false);
    }

    #[test]
    fn threads_1_medium_alloc_dealloc_realloc_x() {
        help_test_multithreaded(1, 100, SizeClass::Medium, true, true, false);
    }

    #[test]
    fn threads_1_medium_alloc_dealloc_with_writes_x() {
        help_test_multithreaded(1, 100, SizeClass::Medium, true, false, true);
    }

    #[test]
    fn threads_1_medium_alloc_dealloc_realloc_with_writes_x() {
        help_test_multithreaded(1, 100, SizeClass::Medium, true, true, true);
    }

    #[test]
    fn threads_2_medium_alloc_x() {
        help_test_multithreaded(2, 100, SizeClass::Medium, false, false, false);
    }

    #[test]
    fn threads_2_medium_alloc_dealloc_x() {
        help_test_multithreaded(2, 100, SizeClass::Medium, true, false, false);
    }

    #[test]
    fn threads_2_medium_alloc_dealloc_realloc_x() {
        help_test_multithreaded(2, 100, SizeClass::Medium, true, true, false);
    }

    #[test]
    fn threads_2_medium_alloc_dealloc_with_writes_x() {
        help_test_multithreaded(2, 100, SizeClass::Medium, true, false, true);
    }

    #[test]
    fn threads_2_medium_alloc_dealloc_realloc_with_writes_x() {
        help_test_multithreaded(2, 100, SizeClass::Medium, true, true, true);
    }

    #[test]
    fn threads_32_medium_alloc_x() {
        help_test_multithreaded(32, 100, SizeClass::Medium, false, false, false);
    }

    #[test]
    fn threads_32_medium_alloc_dealloc_x() {
        help_test_multithreaded(32, 100, SizeClass::Medium, true, false, false);
    }

    #[test]
    fn threads_32_medium_alloc_dealloc_realloc_x() {
        help_test_multithreaded(32, 100, SizeClass::Medium, true, true, false);
    }

    #[test]
    fn threads_32_medium_alloc_dealloc_with_writes_x() {
        help_test_multithreaded(32, 100, SizeClass::Medium, true, false, true);
    }

    #[test]
    fn threads_32_medium_alloc_dealloc_realloc_with_writes_x() {
        help_test_multithreaded(32, 100, SizeClass::Medium, true, true, true);
    }

    #[test]
    fn threads_64_medium_alloc_x() {
        help_test_multithreaded(64, 100, SizeClass::Medium, false, false, false);
    }

    #[test]
    fn threads_64_medium_alloc_dealloc_x() {
        help_test_multithreaded(64, 100, SizeClass::Medium, true, false, false);
    }

    #[test]
    fn threads_64_medium_alloc_dealloc_realloc_x() {
        help_test_multithreaded(64, 100, SizeClass::Medium, true, true, false);
    }

    #[test]
    fn threads_64_medium_alloc_dealloc_with_writes_x() {
        help_test_multithreaded(64, 100, SizeClass::Medium, true, false, true);
    }

    #[test]
    fn threads_64_medium_alloc_dealloc_realloc_with_writes_x() {
        help_test_multithreaded(64, 100, SizeClass::Medium, true, true, true);
    }

    #[test]
    fn threads_1_large_alloc_x() {
        help_test_multithreaded(1, 100, SizeClass::Large, false, false, false);
    }

    #[test]
    fn threads_1_large_alloc_dealloc_x() {
        help_test_multithreaded(1, 100, SizeClass::Large, true, false, false);
    }

    #[test]
    fn threads_1_large_alloc_dealloc_realloc_x() {
        help_test_multithreaded(1, 100, SizeClass::Large, true, true, false);
    }

    #[test]
    fn threads_1_large_alloc_dealloc_with_writes_x() {
        help_test_multithreaded(1, 100, SizeClass::Large, true, false, true);
    }

    #[test]
    fn threads_1_large_alloc_dealloc_realloc_with_writes_x() {
        help_test_multithreaded(1, 100, SizeClass::Large, true, true, true);
    }

    #[test]
    fn threads_2_large_alloc_x() {
        help_test_multithreaded(2, 100, SizeClass::Large, false, false, false);
    }

    #[test]
    fn threads_2_large_alloc_dealloc_x() {
        help_test_multithreaded(2, 100, SizeClass::Large, true, false, false);
    }

    #[test]
    fn threads_2_large_alloc_dealloc_realloc_x() {
        help_test_multithreaded(2, 100, SizeClass::Large, true, true, false);
    }

    #[test]
    fn threads_2_large_alloc_dealloc_with_writes_x() {
        help_test_multithreaded(2, 100, SizeClass::Large, true, false, true);
    }

    #[test]
    fn threads_2_large_alloc_dealloc_realloc_with_writes_x() {
        help_test_multithreaded(2, 100, SizeClass::Large, true, true, true);
    }

    #[test]
    fn threads_32_large_alloc_x() {
        help_test_multithreaded(32, 100, SizeClass::Large, false, false, false);
    }

    #[test]
    fn threads_32_large_alloc_dealloc_x() {
        help_test_multithreaded(32, 100, SizeClass::Large, true, false, false);
    }

    #[test]
    fn threads_32_large_alloc_dealloc_realloc_x() {
        help_test_multithreaded(32, 100, SizeClass::Large, true, true, false);
    }

    #[test]
    fn threads_32_large_alloc_dealloc_with_writes_x() {
        help_test_multithreaded(32, 100, SizeClass::Large, true, false, true);
    }

    #[test]
    fn threads_32_large_alloc_dealloc_realloc_with_writes_x() {
        help_test_multithreaded(32, 100, SizeClass::Large, true, true, true);
    }

    #[test]
    fn threads_64_large_alloc_x() {
        help_test_multithreaded(64, 100, SizeClass::Large, false, false, false);
    }

    #[test]
    fn threads_64_large_alloc_dealloc_x() {
        help_test_multithreaded(64, 100, SizeClass::Large, true, false, false);
    }

    #[test]
    fn threads_64_large_alloc_dealloc_realloc_x() {
        help_test_multithreaded(64, 100, SizeClass::Large, true, true, false);
    }

    #[test]
    fn threads_64_large_alloc_dealloc_with_writes_x() {
        help_test_multithreaded(64, 100, SizeClass::Large, true, false, true);
    }

    #[test]
    fn threads_64_large_alloc_dealloc_realloc_with_writes_x() {
        help_test_multithreaded(64, 100, SizeClass::Large, true, true, true);
    }

    //xyz3 add newtypiness
    //xyz3 remove eac!

    fn help_test_multithreaded(numthreads: u32, numiters: u32, sc: SizeClass, dealloc: bool, realloc: bool, writes: bool) {
        let sm = Arc::new(Smalloc::new());
        sm.idempotent_init().unwrap();

        let mut handles = Vec::new();
        for _i in 0..numthreads {
            let smc = Arc::clone(&sm);
            handles.push(thread::spawn(move || {
                let r = StdRng::seed_from_u64(0);
                help_test(&smc, numiters, sc, r, dealloc, realloc, writes);
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    use ahash::HashSet;
    use ahash::RandomState;
    
    fn help_test_alloc_dealloc(sm: &Smalloc, numiters: u32, l: Layout, r: &mut StdRng) {
        let mut m: HashSet<(*mut u8, Layout)> = HashSet::with_hasher(RandomState::with_seed(r.random::<u64>() as usize));
        
        let mut ps = Vec::new();

        for _i in 0..numiters {
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

    fn help_test_alloc(sm: &Smalloc, _numiters: u32, l: Layout, _r: &mut StdRng) {
        unsafe { let _ = sm.alloc(l); }
    }

    fn help_test(sm: &Smalloc, numiters: u32, sc: SizeClass, mut r: StdRng,  dealloc: bool, realloc: bool, writes: bool) {
        let l = match sc {
            SizeClass::Small => {
                Layout::from_size_align(slotsize(0), 1).unwrap()
            }
            SizeClass::Medium => {
                Layout::from_size_align(slotsize(NUM_SMALL_PLUS_MEDIUM_SLABS - 1), 1).unwrap()
            }
            SizeClass::Large => {
                Layout::from_size_align(LARGE_SLOT_SIZE / 3, 1).unwrap()
            }
        };
        
        for _i in 0..numiters {
            match (dealloc, realloc, writes) {
                (true, true, true) => {
                    help_test_alloc_dealloc_realloc_with_writes(sm, numiters, l, &mut r)
                }
                (true, true, false) => {
                    help_test_alloc_dealloc_realloc(sm, numiters, l, &mut r)
                }
                (true, false, true) => {
                    help_test_alloc_dealloc_with_writes(sm, numiters, l, &mut r);
                }
                (true, false, false) => {
                    help_test_alloc_dealloc(sm, numiters, l, &mut r)
                }
                (false, false, false) => {
                    help_test_alloc(sm, numiters, l, &mut r)
                }
                (false, _, _) => todo!()
            }
        }
    }
    
    fn help_test_alloc_dealloc_with_writes(sm: &Smalloc, numiters: u32, l: Layout, r: &mut StdRng) {
        let mut m: HashSet<(*mut u8, Layout)> = HashSet::with_hasher(RandomState::with_seed(r.random::<u64>() as usize));
        
        let mut ps = Vec::new();
        
        for _i in 0..numiters {
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
                unsafe { std::ptr::copy_nonoverlapping(BYTES3.as_ptr(), p, min(BYTES3.len(), l.size())) };
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
    fn help_test_alloc_dealloc_realloc(sm: &Smalloc, numiters: u32, l: Layout, r: &mut StdRng) {
        let l1 = l;
        let mut ls = Vec::new();
        ls.push(l1);
        let l2 = Layout::from_size_align(l1.size() + 10, l1.align()).unwrap();
        ls.push(l2);
        let l3 = Layout::from_size_align(max(11, l1.size()) - 10, l1.align()).unwrap();
        ls.push(l3);
        let l4 = Layout::from_size_align(l1.size() * 2 + 10, l1.align()).unwrap();
        ls.push(l4);
        
        let mut m: HashSet<(*mut u8, Layout)> = HashSet::with_hasher(RandomState::with_seed(r.random::<u64>() as usize));

        let mut ps = Vec::new();

        for _i in 0..numiters {
            // random coin
            let coin = r.random_range(0..3); // a 3-sided coin!
            if coin == 0 {
                // Free
                if !ps.is_empty() {
                    let (p, lt) = ps.remove(r.random_range(0..ps.len()));
                    assert!(m.contains(&(p, lt)), "thread: {:>3}, {:?} {}-{}", get_thread_areanum(), p, lt.size(), lt.align());
                    m.remove(&(p, lt));
                    unsafe { sm.dealloc(p, lt) };
                }
            } else if coin == 1 {
                // Malloc
                let lt = ls.choose(r).unwrap();
                let p = unsafe { sm.alloc(*lt) };
                assert!(!m.contains(&(p, *lt)), "thread: {:>3}, {:?} {}-{}", get_thread_areanum(), p, lt.size(), lt.align());
                m.insert((p, *lt));
                ps.push((p, *lt));
            } else {
                // Realloc
                if !ps.is_empty() {
                    let i = r.random_range(0..ps.len());
                    let (p, lt) = ps.remove(i);
                    assert!(m.contains(&(p, lt)), "thread: {:>3}, {:?} {}-{}", get_thread_areanum(), p, lt.size(), lt.align());
                    m.remove(&(p, lt));

                    let newlt = ls.choose(r).unwrap();
                    let newp = unsafe { sm.realloc(p, lt, newlt.size()) };
                    assert!(!newp.is_null());

                    assert!(!m.contains(&(newp, *newlt)), "thread: {:>3}, p: {:?}, newp: {:?} {}", get_thread_areanum(), p, newp, newlt.size());
//xyz8 realloc to 128 MiB resulted in null ptr
                    m.insert((newp, *newlt));
                    ps.push((newp, *newlt));
                }
            }
        }
    }

    use std::cmp::max;
    fn help_test_alloc_dealloc_realloc_with_writes(sm: &Smalloc, numiters: u32, l: Layout, mut r: &mut StdRng) {
        let l1 = l;
        let mut ls = Vec::new();
        ls.push(l1);
        let l2 = Layout::from_size_align(l1.size() + 10, l1.align()).unwrap();
        ls.push(l2);
        let l3 = Layout::from_size_align(max(11, l1.size()) - 10, l1.align()).unwrap();
        ls.push(l3);
        let l4 = Layout::from_size_align(l1.size() * 2 + 10, l1.align()).unwrap();
        ls.push(l4);
        
        let mut m: HashSet<(*mut u8, Layout)> = HashSet::with_hasher(RandomState::with_seed(r.random::<u64>() as usize));

        let mut ps = Vec::new();

        for _i in 0..numiters {
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
                    unsafe { std::ptr::copy_nonoverlapping(BYTES5.as_ptr(), newp, min(BYTES5.len(), lt.size())) };

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
    /// If we've allocated all of the slots from a small-slots slab, the subsequent allocations come
    /// from a larger-slots slab.
    fn overflowers_small() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();
        let sybp = sm.get_sys_baseptr();
        let smbp = sm.get_sm_baseptr();

        let siz = 8;
        let l = Layout::from_size_align(siz, 1).unwrap();
        let allslabnum = 1; // slab 1 holds 8-byte things

        // Step 0: reach into the slab's `eac` and set it to almost the max slot number.
        let first_this_thread_areanum = get_thread_areanum();
        let first_i = NUM_SMALL_SLOTS - 3;
        let mut i = first_i;
        debug_assert!(crate::slotsize(allslabnum) >= siz);
        let eaca = sm.get_atomicu64(small_eac_offset(allslabnum, first_this_thread_areanum));
        eaca.store(i as u64, Ordering::Release);

        // Step 1: allocate a slot and store it in local variables:
        let p1 = unsafe { sm.alloc(l) };
        let p1o = offset_of_ptr(sybp, smbp, p1).unwrap();
        assert!(p1o < MEDIUM_SLABS_REGION_BASE, "should have returned a small slot");
        let allslabnum1 = crate::offset_to_allslabnum(p1o);
        assert_eq!(allslabnum1, allslabnum);
        let (areanum1, slotnum1) = small_slot(p1o, allslabnum1, crate::slotsize(allslabnum1));

        assert_eq!(areanum1, first_this_thread_areanum);
        assert_eq!(slotnum1, i);

        i += 1;
        
        // Step 2: allocate all the rest of the slots in this slab except the last one:
        while i < NUM_SMALL_SLOTS - 1 {
            unsafe { sm.alloc(l) };

            i += 1
        }

        // Step 3: allocate the last slot in this slab and store it in local variables:
        let p2 = unsafe { sm.alloc(l) };
        let p2o = offset_of_ptr(sybp, smbp, p2).unwrap();
        assert!(p2o < MEDIUM_SLABS_REGION_BASE, "should have returned a small slot");
        let allslabnum2 = crate::offset_to_allslabnum(p2o);
        let (areanum2, slotnum2) = small_slot(p2o, allslabnum2, crate::slotsize(allslabnum2));

        // Assert some things about the two stored slot locations:
        assert_eq!(areanum1, areanum2);
        assert_eq!(allslabnum1, allslabnum2);
        assert_eq!(slotnum1, first_i);
        assert_eq!(slotnum2, NUM_SMALL_SLOTS - 1);

        // Step 4: Allocate another slot and store it in local variables:
        let p3 = unsafe { sm.alloc(l) };
        let p3o = offset_of_ptr(sybp, smbp, p3).unwrap();
        assert!(p3o < MEDIUM_SLABS_REGION_BASE, "should have returned a small slot");
        let allslabnum3 = crate::offset_to_allslabnum(p3o);
        let (areanum3, slotnum3) = small_slot(p3o, allslabnum3, crate::slotsize(allslabnum3));

        // The raison d'etre for this test: Assert that the newly allocated slot is in a bigger
        // slab, same areanum.
        assert_eq!(areanum3, areanum1);
        assert_ne!(allslabnum3, allslabnum);
        assert_eq!(slotnum3, 0);

        // This thread should still be pointing at the same thread area num.
        let second_this_thread_areanum = get_thread_areanum();
        assert_eq!(first_this_thread_areanum, second_this_thread_areanum);

        // Step 5: If we alloc_slot() again on this thread, it will come from this new slab:
        let p4 = unsafe { sm.alloc(l) };
        let p4o = offset_of_ptr(sybp, smbp, p4).unwrap();
        assert!(p4o < MEDIUM_SLABS_REGION_BASE, "should have returned a small slot");
        let allslabnum4 = crate::offset_to_allslabnum(p4o);
        let (areanum4, slotnum4) = small_slot(p4o, allslabnum4, crate::slotsize(allslabnum4));

        assert_eq!(allslabnum4, allslabnum3);
        assert_eq!(areanum4, second_this_thread_areanum);
        assert_eq!(slotnum4, 1);

        // We've now allocated two slots from this new area:
        let second_area_eaca = sm.get_atomicu64(small_eac_offset(allslabnum4, areanum4));
        let second_area_eac_orig_val = second_area_eaca.load(Ordering::Acquire);
        assert_eq!(second_area_eac_orig_val, 2);
    }

    #[test]
    /// If we've allocated all of the slots from all of the areas of a small-slots slab that is not
    /// the largest small-slots slab, then subsequent allocations come from a larger small-slots
    /// slab.
    fn overflowers_small_to_bigger_small() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();
        let sybp = sm.get_sys_baseptr();
        let smbp = sm.get_sm_baseptr();

        let siz = 8;
        let l = Layout::from_size_align(siz, 1).unwrap();

        let allslabnum = 1; // slab 1 holds 8-byte things
        let tan = get_thread_areanum();

        debug_assert!(crate::slotsize(allslabnum) >= siz);

        // Step 0: reach into each area's slab's `eac` and set it to the max slot number.
        for slabareanum in 0..NUM_SMALL_SLAB_AREAS {
            let eaca = sm.get_atomicu64(small_eac_offset(allslabnum, slabareanum));
            eaca.store(NUM_SMALL_SLOTS as u64, Ordering::Release);
        }

        // Step 1: Allocate another slot and store it in local variables:
        let p1 = unsafe { sm.alloc(l) };
        let p1o = offset_of_ptr(sybp, smbp, p1).unwrap();
        assert!(p1o < MEDIUM_SLABS_REGION_BASE, "should have returned a small slot");
        let allslabnum1 = crate::offset_to_allslabnum(p1o);
        assert_eq!(allslabnum1, allslabnum + 1);
        let (areanum, slotnum1) = small_slot(p1o, allslabnum1, crate::slotsize(allslabnum1));
        assert_eq!(areanum, tan);
        assert_eq!(slotnum1, 0);
    }

    #[test]
    /// If we've allocated all of the slots from all of the areas of the largest small-slots slabs,
    /// the subsequent allocations come from a medium-slots slab.
    fn overflowers_small_to_medium() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();
        let sybp = sm.get_sys_baseptr();
        let smbp = sm.get_sm_baseptr();

        let allslabnum = NUM_SMALL_SLABS - 1;

        let siz = slotsize(allslabnum);
        let l = Layout::from_size_align(siz, 1).unwrap();

        // Step 0: reach into each area's slab's `eac` and set it to the max slot number.
        for slabareanum in 0..NUM_SMALL_SLAB_AREAS {
            let eaca = sm.get_atomicu64(small_eac_offset(allslabnum, slabareanum));
            eaca.store(NUM_SMALL_SLOTS as u64, Ordering::Release);
        }

        // Step 1: Allocate another slot and store it in local variables:
        let p1 = unsafe { sm.alloc(l) };
        let p1o = offset_of_ptr(sybp, smbp, p1).unwrap();
        assert!(p1o < LARGE_SLAB_REGION_BASE, "should have returned a medium slot");
        assert!(p1o >= MEDIUM_SLABS_REGION_BASE, "should have returned a medium slot");
        let allslabnum1 = crate::offset_to_allslabnum(p1o);
        assert_eq!(allslabnum1, allslabnum + 1);
        let mediumslabnum = allslabnum1 - NUM_SMALL_SLABS;
        let slotnum1 = medium_slot(p1o, mediumslabnum, crate::slotsize(allslabnum1));
        assert_eq!(slotnum1, 0);
    }


    /// This only works for mediumslabs other than the biggest medium slab.
    fn help_test_overflowers_medium(mediumslabnum: usize) {
        debug_assert!(mediumslabnum < NUM_MEDIUM_SLABS - 1);

        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();
        let sybp = sm.get_sys_baseptr();
        let smbp = sm.get_sm_baseptr();

        let l = Layout::from_size_align(crate::slotsize(NUM_SMALL_SLABS + mediumslabnum), 1).unwrap();

        let allslabnum = NUM_SMALL_SLABS + mediumslabnum;

        let orig_i = NUM_MEDIUM_SLOTS - 3;
        let mut i = orig_i;

        let eaca = sm.get_atomicu64(medium_eac_offset(mediumslabnum));
        eaca.store(i as u64, Ordering::Release);

        // Step 1: allocate a slot and store it in local variables:
        let p1 = unsafe { sm.alloc(l) };
        let p1o = offset_of_ptr(sybp, smbp, p1).unwrap();
        assert!(p1o < LARGE_SLAB_REGION_BASE, "should have returned a medium slot");
        assert!(p1o >= MEDIUM_SLABS_REGION_BASE, "should have returned a medium slot");
        let allslabnum1 = crate::offset_to_allslabnum(p1o);
        assert_eq!(allslabnum1, allslabnum);
        let slotnum1 = medium_slot(p1o, mediumslabnum, crate::slotsize(allslabnum1));
        assert_eq!(slotnum1, i);

        i += 1;
        
        // Step 2: allocate all the rest of the slots in this slab except the last one:
        while i < NUM_MEDIUM_SLOTS - 1 {
            unsafe { sm.alloc(l) };

            i += 1
        }

        // Step 3: allocate the last slot in this slab and store it in local variables:
        let p2 = unsafe { sm.alloc(l) };
        let p2o = offset_of_ptr(sybp, smbp, p2).unwrap();
        assert!(p2o < LARGE_SLAB_REGION_BASE, "should have returned a medium slot");
        assert!(p2o >= MEDIUM_SLABS_REGION_BASE, "should have returned a medium slot");
        let allslabnum2 = crate::offset_to_allslabnum(p2o);
        assert_eq!(allslabnum2, allslabnum);
        let slotnum2 = medium_slot(p2o, mediumslabnum, crate::slotsize(allslabnum2));
        assert_eq!(slotnum2, i);

        // Assert some things about the two stored slot locations:
        assert_eq!(allslabnum1, allslabnum2);
        assert_eq!(slotnum1, orig_i);
        assert_eq!(slotnum2, NUM_MEDIUM_SLOTS - 1);

        // Step 4: allocate another slot and store it in local variables:
        let p3 = unsafe { sm.alloc(l) };
        let p3o = offset_of_ptr(sybp, smbp, p3).unwrap();
        assert!(p3o < LARGE_SLAB_REGION_BASE, "should have returned a medium slot");
        assert!(p3o >= MEDIUM_SLABS_REGION_BASE, "should have returned a medium slot");
        let allslabnum3 = crate::offset_to_allslabnum(p3o);
        assert_ne!(allslabnum3, allslabnum);
        let slotnum3 = medium_slot(p3o, allslabnum3 - NUM_SMALL_SLABS, crate::slotsize(allslabnum3));
        assert_eq!(slotnum3, 0);

        // Assert that this alloc overflowed to a different slab.
        assert_eq!(allslabnum3, allslabnum+1);
    }

    #[test]
    /// If the slab you're overflowing to is itself full, overflow to the next slab.
    fn multiple_overflows_medium() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();
        let sybp = sm.get_sys_baseptr();
        let smbp = sm.get_sm_baseptr();

        let mediumslabnum = 0;
        let allslabnum = NUM_SMALL_SLABS + mediumslabnum;

        let siz = crate::slotsize(allslabnum);
        let l = Layout::from_size_align(siz, 1).unwrap();

        let orig_i = NUM_MEDIUM_SLOTS - 3;
        let mut i = orig_i;

        let eaca = sm.get_atomicu64(medium_eac_offset(mediumslabnum));
        eaca.store(i as u64, Ordering::Release);

        // Step 1: allocate a slot and store it in local variables:
        let p1 = unsafe { sm.alloc(l) };
        let p1o = offset_of_ptr(sybp, smbp, p1).unwrap();
        assert!(p1o < LARGE_SLAB_REGION_BASE, "should have returned a medium slot");
        assert!(p1o >= MEDIUM_SLABS_REGION_BASE, "should have returned a medium slot");
        let allslabnum1 = crate::offset_to_allslabnum(p1o);
        assert_eq!(allslabnum1, allslabnum);
        let slotnum1 = medium_slot(p1o, allslabnum1 - NUM_SMALL_SLABS, crate::slotsize(allslabnum1));
        assert_eq!(slotnum1, i);

        i += 1;
        
        // Step 2: allocate all the rest of the slots in this slab except the last one:
        while i < NUM_MEDIUM_SLOTS - 1 {
            unsafe { sm.alloc(l) };

            i += 1
        }

        // Step 3: allocate the last slot in this slab and store it in local variables:
        let p2 = unsafe { sm.alloc(l) };
        let p2o = offset_of_ptr(sybp, smbp, p2).unwrap();
        assert!(p2o < LARGE_SLAB_REGION_BASE, "should have returned a medium slot");
        assert!(p2o >= MEDIUM_SLABS_REGION_BASE, "should have returned a medium slot");
        let allslabnum2 = crate::offset_to_allslabnum(p2o);
        assert_eq!(allslabnum2, allslabnum);
        let slotnum2 = medium_slot(p2o, allslabnum2 - NUM_SMALL_SLABS, crate::slotsize(allslabnum2));
        assert_eq!(slotnum2, i);

        // Assert some things about the two stored slot locations:
        assert_eq!(allslabnum1, allslabnum2);
        assert_eq!(slotnum1, orig_i);
        assert_eq!(slotnum2, NUM_MEDIUM_SLOTS - 1);

        // Step 4: allocate another slot from this slab and store it in local variables:
        let p3 = unsafe { sm.alloc(l) };
        let p3o = offset_of_ptr(sybp, smbp, p3).unwrap();
        assert!(p3o < LARGE_SLAB_REGION_BASE, "should have returned a medium slot");
        assert!(p3o >= MEDIUM_SLABS_REGION_BASE, "should have returned a medium slot");
        let allslabnum3 = crate::offset_to_allslabnum(p3o);

        // Assert that this alloc overflowed to a different slab.
        assert_ne!(allslabnum1, allslabnum3);
        assert_eq!(allslabnum3, allslabnum+1);

        let slotnum3 = medium_slot(p3o, allslabnum3 - NUM_SMALL_SLABS, crate::slotsize(allslabnum3));
        assert_eq!(slotnum3, 0);

        // Step 5: set the overflow slab to be full, and alloc another slot
        let eaca = sm.get_atomicu64(medium_eac_offset(allslabnum3 - NUM_SMALL_SLABS));
        eaca.store(NUM_MEDIUM_SLOTS as u64, Ordering::Release);
//xxx add set/get _atomicu64() / set/get _atomicu32()...

        let p4 = unsafe { sm.alloc(l) };
        let p4o = offset_of_ptr(sybp, smbp, p4).unwrap();
        assert!(p4o < LARGE_SLAB_REGION_BASE, "should have returned a medium slot");
        assert!(p4o >= MEDIUM_SLABS_REGION_BASE, "should have returned a medium slot");

        // Assert that this alloc overflowed to a different slab from the first slab.
        let allslabnum4 = crate::offset_to_allslabnum(p4o);
        assert_ne!(allslabnum4, allslabnum);

        // Assert that this alloc overflowed to a different slab from the second slab.
        assert_ne!(allslabnum4, allslabnum3);

        // Assert that this alloc overflowed to next next slab.
        assert_eq!(allslabnum4, allslabnum+2);

        let slotnum4 = medium_slot(p4o, allslabnum4 - NUM_SMALL_SLABS, crate::slotsize(allslabnum4));
        assert_eq!(slotnum4, 0);
    }

    #[test]
    /// If we've allocated all of the slots from medium-slots slab 0, the subsequent allocations
    /// come from medium-slots slab 1.
    fn overflowers_medium() {
        help_test_overflowers_medium(0);
    }

    #[test]
    /// If we've allocated all of the slots from medium-slots slab 6, the subsequent allocations
    /// come from large-slots slab 7.
    fn overflowers_medium_slab_6() {
        help_test_overflowers_medium(6);
    }

    #[test]
    /// If we've allocated all of the slots from the largest medium-slots slab the subsequent
    /// allocations come from the large-slots slab.
    fn overflowers_from_medium_to_large() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();
        let sybp = sm.get_sys_baseptr();
        let smbp = sm.get_sm_baseptr();

        let mediumslabnum = NUM_MEDIUM_SLABS - 1;
        let allslabnum = mediumslabnum + NUM_SMALL_SLABS;
        let siz = slotsize(allslabnum);

        let l = Layout::from_size_align(siz, 1).unwrap();

        let orig_i = NUM_MEDIUM_SLOTS - 3;
        let mut i = orig_i;

        let eaca = sm.get_atomicu64(medium_eac_offset(mediumslabnum));
        eaca.store(i as u64, Ordering::Release);

        // Step 1: allocate a slot and store it in local variables:
        let p1 = unsafe { sm.alloc(l) };
        let p1o = offset_of_ptr(sybp, smbp, p1).unwrap();
        assert!(p1o < LARGE_SLAB_REGION_BASE, "should have returned a medium slot");
        assert!(p1o >= MEDIUM_SLABS_REGION_BASE, "should have returned a medium slot");
        let allslabnum1 = crate::offset_to_allslabnum(p1o);
        assert_eq!(allslabnum1, allslabnum);
        let mediumslabnum1 = allslabnum1 - NUM_SMALL_SLABS;
        let slotnum1 = medium_slot(p1o, mediumslabnum1, crate::slotsize(allslabnum1));
        assert_eq!(slotnum1, i);

        i += 1;
        
        // Step 2: allocate all the rest of the slots in this slab except the last one:
        while i < NUM_MEDIUM_SLOTS - 1 {
            unsafe { sm.alloc(l) };

            i += 1
        }

        // Step 3: allocate the last slot in this slab and store it in local variables:
        let p2 = unsafe { sm.alloc(l) };
        let p2o = offset_of_ptr(sybp, smbp, p2).unwrap();
        assert!(p2o < LARGE_SLAB_REGION_BASE, "should have returned a medium slot");
        assert!(p2o >= MEDIUM_SLABS_REGION_BASE, "should have returned a medium slot");
        let allslabnum2 = crate::offset_to_allslabnum(p2o);
        assert_eq!(allslabnum2, allslabnum);
        let mediumslabnum2 = allslabnum2 - NUM_SMALL_SLABS;
        let slotnum2 = medium_slot(p2o, mediumslabnum2, crate::slotsize(allslabnum2));
        assert_eq!(slotnum2, NUM_MEDIUM_SLOTS - 1);

        // Step 4: allocate another slot and store it in local variables:
        let p3 = unsafe { sm.alloc(l) };
        let p3o = offset_of_ptr(sybp, smbp, p3).unwrap();
        assert!(p3o < SMALL_SLABS_VARS_REGION_BASE, "should have returned a large slot");
        assert!(p3o >= LARGE_SLAB_REGION_BASE, "should have returned a large slot");
        let slotnum3 = large_slot(p3o);
        assert_eq!(slotnum3, 0);
    }

    #[test]
    /// If we've allocated all of the slots from the large-slots slab the subsequent allocations
    /// come from falling back to the system allocator.
    fn overflowers_from_large_slots_slab() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();
        let sybp = sm.get_sys_baseptr();
        let smbp = sm.get_sm_baseptr();

        let siz = LARGE_SLOT_SIZE;

        let l = Layout::from_size_align(siz, 1).unwrap();

        let orig_i = NUM_LARGE_SLOTS - 3;
        let mut i = orig_i;

        let eaca = sm.get_atomicu64(large_eac_offset());
        eaca.store(i as u64, Ordering::Release);

        // Step 1: allocate a slot and store it in local variables:
        let p1 = unsafe { sm.alloc(l) };
        let p1o = offset_of_ptr(sybp, smbp, p1).unwrap();
        assert!(p1o < SMALL_SLABS_VARS_REGION_BASE, "should have returned a large slot");
        assert!(p1o >= LARGE_SLAB_REGION_BASE, "should have returned a large slot");
        let slotnum1 = large_slot(p1o);
        assert_eq!(slotnum1, i);

        i += 1;
        
        // Step 2: allocate all the rest of the slots in this slab except the last one:
        while i < NUM_LARGE_SLOTS - 1 {
            unsafe { sm.alloc(l) };

            i += 1
        }

        // Step 3: allocate the last slot in this slab and store it in local variables:
        let p2 = unsafe { sm.alloc(l) };
        let p2o = offset_of_ptr(sybp, smbp, p2).unwrap();
        assert!(p2o < SMALL_SLABS_VARS_REGION_BASE, "should have returned a large slot");
        assert!(p2o >= LARGE_SLAB_REGION_BASE, "should have returned a large slot");
        let slotnum2 = large_slot(p2o);
        assert_eq!(slotnum2, i);

        // Assert some things about the two stored slot locations:
        assert_eq!(slotnum1, orig_i);
        assert_eq!(slotnum2, NUM_LARGE_SLOTS - 1);

        // Step 4: allocate another slot from this slab and store it in local variables:
        let p3 = unsafe { sm.alloc(l) };
        let opt_p3o = offset_of_ptr(sybp, smbp, p3);

        // Assert that it isn't in any of our slots.
        assert!(opt_p3o.is_none());

        // I don't believe in sweeping the floors right before razing the house. This call to
        // `sys_dealloc()` is just to exercise more code in case something (like valgrind for
        // example) could find a bug in smalloc this way.
        sys_dealloc(p3, l);
    }
}
