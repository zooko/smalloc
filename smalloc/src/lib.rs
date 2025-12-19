#![doc = include_str!("../../README.md")]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::assertions_on_constants)]
#![feature(stmt_expr_attributes)]
#![feature(likely_unlikely)]
#![feature(pointer_is_aligned_to)]
#![feature(unchecked_shifts)]
#![feature(test)]
#![feature(atomic_from_mut)]


// Table of contents:
//
// src/lib.rs (this file):
// * Fixed constants chosen for the design (see README.md)
// * Constant values computed at compile time from the fixed constants
// * Implementation code
//
// src/dev.rs:
// * Code for development (benchmarks, tests, development tools)


// --- Fixed constants chosen for the design ---


// NUM_SC_BITS is the main constant determining the rest of smalloc's layout. It is equal to 5
// because that means there are 32 size classes, and the first one (that is used -- see below) has
// 2^32 slots. This is the largest number of slots that we can encode their slot numbers into a
// 4-byte slot, which means that our smallest slots can be 4 bytes and we can pack more allocations
// of 1, 2, 3, or 4 bytes into each cache line.
const NUM_SC_BITS: u8 = 5;

// NUM_SLABS_BITS is the other constant. There are 2^NUM_SLABS_BITS slabs in each size class.
const NUM_SLABS_BITS: u8 = 6;

// The first two size classes (which would hold 1-byte and 2-byte slots) are not used. In fact, we
// re-use that unused space to hold flh's.
const NUM_UNUSED_SCS: u8 = 2;//xxx visit all uses and try to simplify


// --- Constant values determined by the constants above ---

const UNUSED_SC_MASK: usize = const_one_shl_usize(NUM_UNUSED_SCS - 1);

// See the ASCII-art map in `README.md` for where these bit masks fit in.
const NUM_SCS: u8 = const_one_shl_u8(NUM_SC_BITS); // 32
const NUM_SLOTNUM_AND_DATA_BITS: u8 = NUM_SCS + NUM_UNUSED_SCS; // 34
const NUM_SLABNUM_AND_SLOTNUM_AND_DATA_BITS: u8 = NUM_SLOTNUM_AND_DATA_BITS + NUM_SLABS_BITS; // 40
const SLABNUM_ALONE_MASK: u8 = const_gen_mask_u8(NUM_SLABS_BITS); // 0b111111
const SLABNUM_ADDR_MASK: usize = const_shl_u8_usize(SLABNUM_ALONE_MASK, NUM_SLOTNUM_AND_DATA_BITS); // 0b1111110000000000000000000000000000000000
const SC_BITS_MASK: usize = const_shl_u8_usize(const_gen_mask_u8(NUM_SC_BITS), NUM_SLABNUM_AND_SLOTNUM_AND_DATA_BITS); // 0b111110000000000000000000000000000000000000000
const SLOTNUM_AND_DATA_MASK: usize = const_gen_mask_usize(NUM_SLOTNUM_AND_DATA_BITS); // 0b1111111111111111111111111111111111

const NUM_SLOTS_IN_HIGHEST_SC: u64 = const_one_shl_u64(NUM_UNUSED_SCS + 1);
const HIGHEST_SLOTNUM_IN_HIGHEST_SC: u64 = NUM_SLOTS_IN_HIGHEST_SC - 2; // 6; The -1 because the last slot isn't used since its slotnum is the sentinel slotnum.
const DATA_ADDR_BITS_IN_HIGHEST_SC: u8 = NUM_SCS - 1; // 31

// The smalloc address of the slot with the highest address is:
const HIGHEST_SMALLOC_SLOT_ADDR: usize = const_shl_u8_usize(NUM_SCS - 1, NUM_SLABNUM_AND_SLOTNUM_AND_DATA_BITS) | SLABNUM_ADDR_MASK | const_shl_u64_usize(HIGHEST_SLOTNUM_IN_HIGHEST_SC, DATA_ADDR_BITS_IN_HIGHEST_SC); // 0b111111111111100000000000000000000000000000000

// The smalloc address of the highest-addressed byte of a smalloc slot is:
const HIGHEST_SMALLOC_SLOT_BYTE_ADDR: usize = HIGHEST_SMALLOC_SLOT_ADDR | const_gen_mask_usize(NUM_SCS - 1); // 0b111111111111101111111111111111111111111111111

// We need to allocate extra bytes so that we can align the smalloc base pointer so that all of the
// trailing bits (all of the bits covered by the SMALLOC_ADDRESS_BITS_MASK) of the smalloc base
// pointer are zeros.

const BASEPTR_ALIGN: usize = (HIGHEST_SMALLOC_SLOT_BYTE_ADDR + 1).next_power_of_two(); // 0b1000000000000000000000000000000000000000000000
const SMALLOC_ADDRESS_BITS_MASK: usize = BASEPTR_ALIGN - 1; // 0b111111111111111111111111111111111111111111111
const TOTAL_VIRTUAL_MEMORY: usize = HIGHEST_SMALLOC_SLOT_ADDR + SMALLOC_ADDRESS_BITS_MASK; // 0b1111111111111011111111111111111111111111111111 == 70_364_449_210_367


// -- Constant values determined by the sizes of u32 and u64 --

const FLHDWORD_SIZE_BITS: u8 = 3; // 3 bits ie 8-byte sized flh dwords
const FLHDWORD_SLOTNUM_MASK: u64 = u32::MAX as u64;
const FLHDWORD_PUSH_COUNTER_MASK: u64 = const_shl_u32_u64(u32::MAX, 32);
const FLHDWORD_PUSH_COUNTER_INCR: u64 = 1u64 << 32;


// --- Implementation ---

use std::sync::atomic::{AtomicU32, AtomicU64};
use std::cell::Cell;
use std::sync::atomic::Ordering::Relaxed;

static GLOBAL_THREAD_NUM: AtomicU32 = AtomicU32::new(0);

thread_local! {
    static THREAD_NUM: Cell<Option<u32>> = const { Cell::new(None) };
    static SLAB_NUM: Cell<Option<u8>> = const { Cell::new(None) };
}

/// Get this thread's unique, incrementing number.
// It is okay if more than 4 billion threads are spawned and this wraps, since the only thing we
// currently use it for is to & it with SLABNUM_ALONE_MASK anyway.
#[inline(always)]
fn get_thread_num() -> u32 {
    THREAD_NUM.with(|cell| {
        cell.get().map_or_else(
            || {
                let new_value = GLOBAL_THREAD_NUM.fetch_add(1, Relaxed); // xxx reconsider whether we need stronger ordering constraints
                THREAD_NUM.with(|cell| cell.set(Some(new_value)));
                new_value
            },
            |value| value,
        )
    })
}

/// Get the slab that this thread allocates from. If uninitialized, this is initialized to
/// `get_thread_num() % 64`.
#[inline(always)]
fn get_slab_num() -> u8 {
    SLAB_NUM.with(|cell| {
        cell.get().map_or_else(
            || get_thread_num() as u8 & SLABNUM_ALONE_MASK,
            |value| value,
        )
    })
}

#[inline(always)]
fn set_slab_num(slabnum: u8) {
    SLAB_NUM.set(Some(slabnum));
}

use std::cell::UnsafeCell;

pub struct Smalloc {
    inner: UnsafeCell<SmallocInner>,
}

struct SmallocInner {
    sysbp: usize,
    smbp: usize,
}

/// Pick a new slab to fail over to. This is used in two cases: from `inner_alloc()` when a slab is
/// full, and from `pop_slot_from_freelist()` when there is a multithreading collision on the flh.
///
/// Which new slabnumber shall we fail over to? A certain number, d, added to the current slab
/// number, and d should have these properties:
///
/// 1. It should be relatively prime to NUM_SLABS so that we will try all slabs before returning to
///    the original one.
///
/// 2. It should use the information from the thread number, not just the (strictly lesser)
///    information from the original slab number.
/// 
/// 3. It should be larger than about 1/3 of 64 because the next few threads got subsequent slabs
///    (the first time they allocated).
///
/// 4. It should be relatively prime to each other d used by other threads so that multiple threads
///    stepping at once will minimally "step" on each other (e.g. if one thread increased its slab
///    number by 3 and another by 6, then they'd be more likely to re-collide before trying all
///    possible slab numbers, but if they're relatively prime to each other then they'll be
///    minimally likely to recollide soon). This implies that d needs to be prime, which also
///    satisfies requirement 1 above.
#[inline(always)]
fn failover_slabnum(slabnum: u8, threadnum: u32) -> u8 {
    const STEPS: [u8; 16] = [
        61,
        59,
        53,
        47,
        43,
        41,
        37,
        31,
        29,
        23,
        19,
        17,
        13,
        11,
         7,
         5,
    ];

    const STEPS_MASK: u8 = const_gen_mask_u8(4);
    let ix: usize = (const_shr_u8_u8(threadnum as u8, 4) & STEPS_MASK) as usize;
    (slabnum + STEPS[ix]) & SLABNUM_ALONE_MASK
}

unsafe impl Sync for Smalloc {}

impl Default for Smalloc {
    fn default() -> Self {
        Self::new()
    }
}

use std::hint::likely;
impl Smalloc {
    pub const fn new() -> Self {
        Self {
            inner: UnsafeCell::new(SmallocInner {
                sysbp: 0,
                smbp: 0,
            }),
        }
    }

    #[inline(always)]
    fn inner_alloc(&self, orig_sc: u8) -> *mut u8 {
        debug_assert!(orig_sc >= NUM_UNUSED_SCS, "{orig_sc}");
        debug_assert!(orig_sc < NUM_SCS);

        let smbp = self.inner().smbp;

        // If the slab is full, or if there is a collision when updating the flh, we'll switch to
        // another slab in this same sizeclass.
        let orig_slabnum = get_slab_num();
        let mut slabnum = orig_slabnum;

        // If all slabs in the sizeclass are full, we'll switch to the next sizeclass.
        let mut sc = orig_sc;

        let mut threadnum: u32 = 42; // the 42 will never get used
        let mut loaded_threadnum: bool = false;
        let mut a_slab_was_full = false;

        loop {
            // The flh is at this location:
            let flhptr = smbp | const_shl_u8_usize(slabnum, NUM_SC_BITS + FLHDWORD_SIZE_BITS) | const_shl_u8_usize(sc, FLHDWORD_SIZE_BITS);
            let flh = unsafe { AtomicU64::from_ptr(flhptr as *mut u64) };

            // Load the value from the flh
            let flhdword = flh.load(Acquire); // xxx weaker ordering constraints ok?
            let curfirstentryslotnum = (flhdword & FLHDWORD_SLOTNUM_MASK) as u32;

            // curfirstentryslotnum can be the sentinel value.
            let highestslotnum = highest_slotnum(sc);
            debug_assert!(curfirstentryslotnum <= highestslotnum);
            if likely(curfirstentryslotnum < highestslotnum) {
                // There is a slot available in the free list.
                
                // The slabbp is the smbp with the size class bits and the slabnum bits set.
                let slabbp = smbp | const_shl_u8_usize(sc, NUM_SLABNUM_AND_SLOTNUM_AND_DATA_BITS) | const_shl_u8_usize(slabnum, NUM_SLOTNUM_AND_DATA_BITS);
                debug_assert!((slabbp >= smbp) && (slabbp <= (smbp + HIGHEST_SMALLOC_SLOT_ADDR)));
                debug_assert!(help_trailing_zeros_usize(slabbp) >= sc);

                // Read the bits from the first entry's link and decode them into a slot number. These
                // bits might be invalid, if the flh has changed since we read it above and another
                // thread has started using these bits for something else (e.g. user data or another
                // linked list update). That's okay because in that case our attempt to update the flh
                // below with information derived from these bits will fail.
                let curfirstentrylink_p = Self::linkptr(slabbp, curfirstentryslotnum, sc);
                let curfirstentrylink_v = unsafe { *curfirstentrylink_p };
                let newfirstentryslotnum: u32 = Self::decode_next_entry_link(curfirstentryslotnum, curfirstentrylink_v, highestslotnum);

                // Write the new first entry slot num in place of the old in our local (in a register)
                // copy of flhdword, leaving the push-counter bits unchanged.
                let newflhdword = (flhdword & FLHDWORD_PUSH_COUNTER_MASK) | newfirstentryslotnum as u64;

                // Compare and exchange
                if likely(flh.compare_exchange(flhdword, newflhdword, AcqRel, Acquire).is_ok()) { // xxx weaker ordering constraints okay?
	            let curfirstentry_p = Self::slotptr(slabbp, curfirstentryslotnum, sc) as usize;
                    debug_assert!((curfirstentry_p >= smbp) && (curfirstentry_p <= (smbp + HIGHEST_SMALLOC_SLOT_ADDR)));
                    if unlikely(orig_slabnum != slabnum) {
                        // Save the new slabnum for next time.
                        set_slab_num(slabnum);
                    }

                    break curfirstentry_p as *mut u8;
                } else {
                    // Update collision on the flh. Fail over to a different slab in the same size
                    // class.
                    if likely(!loaded_threadnum) { threadnum = get_thread_num(); loaded_threadnum = true; }

                    slabnum = failover_slabnum(slabnum, threadnum);
                }
            } else {
                // If we got here then curfirstentryslotnum == sentinelslotnum, meaning no next
                // entry, meaning the free list is empty, meaning this slab is full. Overflow to a
                // different slab in the same size class.œ
                if likely(!loaded_threadnum) { threadnum = get_thread_num(); loaded_threadnum = true; }

                slabnum = failover_slabnum(slabnum, threadnum);

                if likely(slabnum != orig_slabnum) {
                    // We have not necessarily cycled through all slabs in this sizeclass yet, so
                    // keep trying, but make a note that at least one of the slabs in this sizeclass
                    // was full.
                    a_slab_was_full = true;
                } else {
                    // ... meaning we've tried each slab in this size class and each one was either
                    // full or had an flh update collision. If at least one slab in this size class
                    // was full, then overflow to the next larger size class. (Else, keep trying
                    // different slabs in this size class.)
                    if unlikely(a_slab_was_full) {
                        if unlikely(sc == NUM_SCS - 1) {
                            // This is the largest size class and we've exhausted at least one slab
                            // in it.
                            eprintln!("smalloc exhausted");
                            //xxxself.dump_map_of_slabs(); // for debugging only -- should probably be removed
                            break null_mut();
                        };

                        // Increment the sc
                        sc += 1;
                    }
                }
            }
        }
    }

    #[inline(always)]
    fn inner(&self) -> &SmallocInner {
        unsafe { &*self.inner.get() }
    }

    #[allow(clippy::mut_from_ref)]
    #[inline(always)]
    fn inner_mut(&self) -> &mut SmallocInner {
        unsafe { &mut *self.inner.get() }
    }

    /// For testing only. Do not use in production code.
    pub fn get_total_virtual_memory(&self) -> usize {
        TOTAL_VIRTUAL_MEMORY
    }

//    /// For testing only. Do not use in production code.
//     fn dump_map_of_slabs(&self) {
//         let inner = self.inner();

//         // Dump a map of the slabs
//         let mut fullslots = 0;
//         let mut fulltotsize = 0;
//         for sc in NUM_UNUSED_SCS..NUM_SCS {
//             let mut scfullslots = 0;
//             let mut scfulltotsize = 0;

//             print!("{sc:2} ");

//             let highestslotnum = highest_slotnum(sc);
//             let slotsize = 2u64.pow(sc as u32);
//             print!("slots: {}, slotsize: {}", highestslotnum, slotsize);

//             for slabnum in 0..NUM_SLABS {
// //                print!(" {slabnum}");
                
//                 let headelement = help_get_flh(inner.smbp, sc, slabnum);
//                 if headelement == highestslotnum {
//                     // full
//                     print!("X");
//                     scfullslots += highestslotnum;
//                     scfulltotsize += (highestslotnum as u64) * slotsize;
//                 } else {
//                     print!(".");
//                 }
//             }
//             println!(" slots: {scfullslots} size: {scfulltotsize}");
//             fullslots += scfullslots;
//             fulltotsize += scfulltotsize;
//         }
//         println!(" totslots: {fullslots}, totsize: {fulltotsize}");
//     }

    /// Initializes the allocator.
    ///
    /// # Safety
    ///
    /// The caller must ensure:
    /// - This method is called exactly once
    /// - This is called before any dynamic allocation occurs
    /// - No other thread simultaneously accesses this `Smalloc` instance during this call to `init`.
    pub unsafe fn init(&self) {
        let inner = self.inner_mut();
        assert!(inner.sysbp == 0);
        inner.sysbp = sys_alloc(TOTAL_VIRTUAL_MEMORY).unwrap().addr();
        assert!(inner.sysbp != 0);
        inner.smbp = inner.sysbp.next_multiple_of(BASEPTR_ALIGN);
    }

    /// `highestslotnum` is for using `& highestslotnum` instead of `% numslots` to compute a number
    /// modulo numslots (where `numslots` here counts the sentinel slot). So `highestslotnum` is
    /// equal to `numslots - 1`, which is also the slotnum of the sentinel slot. (It is also used in
    /// `debug_asserts`.) `newslotnum` cannot be the sentinel slotnum.
    #[inline(always)]
    fn push_slot_onto_freelist(&self, slabbp: usize, flh: &AtomicU64, newslotnum: u32, highestslotnum: u32, sc: u8) {
        debug_assert!(sc >= NUM_UNUSED_SCS);
        debug_assert!(sc < NUM_SCS);
        debug_assert!(slabbp != 0);
        debug_assert!(help_trailing_zeros_usize(slabbp) >= NUM_SLOTNUM_AND_DATA_BITS);
        debug_assert!(newslotnum < highestslotnum);

        loop {
            // Load the value (current first entry slot num) from the flh
            let flhdword = flh.load(Acquire); // xxx weaker ordering constraints ok?
            let curfirstentryslotnum = (flhdword & FLHDWORD_SLOTNUM_MASK) as u32;

            // The curfirstentryslotnum can be the sentinel slotnum.
            debug_assert!(curfirstentryslotnum <= highestslotnum);

            // Encode the curfirstentryslotnum as the next-entry link for the new entry
            let next_entry_link = Self::encode_next_entry_link(newslotnum, curfirstentryslotnum, highestslotnum);
            debug_assert!(curfirstentryslotnum == Self::decode_next_entry_link(newslotnum, next_entry_link, highestslotnum));

            // Write it into the new slot's link
            let new_slot_p = Self::linkptr(slabbp, newslotnum, sc);
            unsafe { *new_slot_p = next_entry_link };

            // Increment the push counter
            let counter = (flhdword & FLHDWORD_PUSH_COUNTER_MASK).wrapping_add(FLHDWORD_PUSH_COUNTER_INCR);
            let newflhdword = counter | newslotnum as u64;

            // Compare and exchange
            if flh.compare_exchange(flhdword, newflhdword, AcqRel, Acquire).is_ok() { // xxx weaker ordering constraints okay?
                break;
            }
        }
    }

    #[inline(always)]
    fn encode_next_entry_link(baseslotnum: u32, targslotnum: u32, highestslotnum: u32) -> u32 {
        debug_assert!(baseslotnum != targslotnum);
        // The baseslotnum cannot be the sentinel slotnum.
        debug_assert!(baseslotnum < highestslotnum);
        // The targslotnum can be the sentinel slotnum.
        debug_assert!(targslotnum <= highestslotnum);

        targslotnum.wrapping_sub(baseslotnum).wrapping_sub(1) & highestslotnum
    }

    #[inline(always)]
    fn decode_next_entry_link(baseslotnum: u32, codeword: u32, highestslotnum: u32) -> u32 {
        // The baseslotnum cannot be the sentinel slot num.
        debug_assert!(baseslotnum < highestslotnum);

        baseslotnum.wrapping_add(codeword).wrapping_add(1) & highestslotnum
    }
        
    #[inline(always)]
    fn linkptr(slabbp: usize, slotnum: u32, sc: u8) -> *mut u32 {
        debug_assert!(sc >= NUM_UNUSED_SCS);
        debug_assert!(sc < NUM_SCS);
//xxx10        // This thing about or'ing in a few bits from the least significant bits of the slotnum is to utilize more of the sets from the associative cache in the L1...
//xxx10
//xxx10	// symbolify 6 (it's cache line size)
//xxx10        let num_bits_of_slotnum = max(6, sc) - 6; // xxx oops changed sc to new sc (+2)
//xxx10        let mask = const_gen_mask_u32(num_bits_of_slotnum);
//xxx10        let slotnum_bits = const_shl_u32_usize(slotnum & mask, 6);
//xxx10        (slabbp | const_shl_u32_usize(slotnum, sc) | slotnum_bits) as *mut u32
        Self::slotptr(slabbp, slotnum, sc) as *mut u32
    }

    // xxx check in-line-always
    #[inline(always)]
    fn slotptr(slabbp: usize, slotnum: u32, sc: u8) -> *mut u8 {
        debug_assert!(sc >= NUM_UNUSED_SCS);
        debug_assert!(sc < NUM_SCS);
        (slabbp | const_shl_u32_usize(slotnum, sc)) as *mut u8
    }
}

#[inline(always)]
fn highest_slotnum(sc: u8) -> u32 {
    const_gen_mask_u32(NUM_SLOTNUM_AND_DATA_BITS - sc)
}

use std::hint::unlikely;
unsafe impl GlobalAlloc for Smalloc {
    #[inline(always)]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        debug_assert!(self.inner().smbp != 0);
        let reqsiz = layout.size();
        let reqalign = layout.align();
        debug_assert!(reqsiz > 0);
        debug_assert!(reqalign > 0);
        debug_assert!(reqalign.is_power_of_two()); // alignment must be a power of two

        let sc = req_to_sc(reqsiz, reqalign);

        self.inner_alloc(sc)
    }

    #[inline(always)]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        debug_assert!(!ptr.is_null());
        debug_assert!(layout.align().is_power_of_two()); // alignment must be a power of two

        let p_addr = ptr.addr();

        let inner = self.inner();

        // To be valid, the pointer has to be greater than or equal to the smalloc base pointer and
        // less than or equal to the highest slot pointer.
        let highest_addr = inner.smbp + HIGHEST_SMALLOC_SLOT_ADDR;

        assert!((p_addr >= inner.smbp) && (p_addr <= highest_addr));

        // Okay now we know that it is a pointer into smalloc's region.

        let slabbp = p_addr & !SLOTNUM_AND_DATA_MASK;
        let sc = const_shr_usize_u8(p_addr & SC_BITS_MASK, NUM_SLABNUM_AND_SLOTNUM_AND_DATA_BITS);
        let slotnum = const_shr_usize_u32(p_addr & SLOTNUM_AND_DATA_MASK, sc);
        let slabnum = const_shr_usize_u8(p_addr & SLABNUM_ADDR_MASK, NUM_SLOTNUM_AND_DATA_BITS);
        let highestslotnum = const_gen_mask_u32(NUM_SLOTNUM_AND_DATA_BITS - sc);

        let flhi = NUM_SCS as u16 * slabnum as u16 + sc as u16;
        let flhptr = inner.smbp | const_shl_u16_usize(flhi, 3);
        let flh = unsafe { AtomicU64::from_ptr(flhptr as *mut u64) };

        debug_assert!(p_addr.trailing_zeros() >= sc as u32);

        self.push_slot_onto_freelist(slabbp, flh, slotnum, highestslotnum, sc);
    }

    #[inline(always)]
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, reqsize: usize) -> *mut u8 {
        debug_assert!(!ptr.is_null());
        let oldsize = layout.size();
        debug_assert!(oldsize > 0);
        let oldalignment = layout.align();
        debug_assert!(oldalignment > 0);
        debug_assert!(oldalignment.is_power_of_two()); // alignment must be a power of two
        debug_assert!(reqsize > 0);

        let oldsc = req_to_sc(oldsize, oldalignment);
        debug_assert!(oldsc >= NUM_UNUSED_SCS);
        debug_assert!(oldsc < NUM_SCS);
        let reqsc = req_to_sc(reqsize, oldalignment);
        debug_assert!(reqsc >= NUM_UNUSED_SCS);
        debug_assert!(reqsc < NUM_SCS);

        // If the requested slot is <= the original slot, just return the pointer and we're done.
        if unlikely(reqsc <= oldsc) {
            return ptr;
        }

        let newp = self.inner_alloc(reqsc);
        debug_assert!(!newp.is_null());
        debug_assert!(newp.is_aligned_to(oldalignment));

        // Copy the contents from the old location.
        unsafe { copy_nonoverlapping(ptr, newp, oldsize); }

        // Free the old slot.
        unsafe { self.dealloc(ptr, layout) };

        newp
    }
}

// utility functions

use core::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::Ordering::{AcqRel, Acquire};
use plat::p::sys_alloc;
use std::ptr::{copy_nonoverlapping, null_mut};
//xxx16use thousands::Separable;

// xxx look at asm and benchmark these vs the builtin alternatives

// xxx benchmark and inspect asm for this vs <<
#[inline(always)]
const fn const_shl_u32_usize(value: u32, shift: u8) -> usize {
    debug_assert!((shift as u32) < usize::BITS);
    debug_assert!(hlzu(value as usize) >= shift); // we never shift off 1 bits currently
    unsafe { (value as usize).unchecked_shl(shift as u32) }
}

// xxx benchmark and inspect asm for this vs <<
#[inline(always)]
const fn const_shl_u64_usize(value: u64, shift: u8) -> usize {
    debug_assert!((shift as u32) < usize::BITS);
    debug_assert!(help_leading_zeros_u64(value) >= shift); // we never shift off 1 bits currently
    unsafe { (value as usize).unchecked_shl(shift as u32) }
}

#[inline(always)]
const fn const_shl_u32_u64(value: u32, shift: u8) -> u64 {
    debug_assert!((shift as u32) < u64::BITS);
    debug_assert!(help_leading_zeros_u64(value as u64) >= shift); // we never shift off 1 bits currently
    unsafe { (value as u64).unchecked_shl(shift as u32) }
}

// xxx benchmark and inspect asm for this vs <<
#[inline(always)]
const fn const_shl_u16_usize(value: u16, shift: u8) -> usize {
    debug_assert!((shift as u32) < usize::BITS);
    debug_assert!(hlzu(value as usize) >= shift); // we never shift off 1 bits currently
    unsafe { (value as usize).unchecked_shl(shift as u32) }
}

// xxx benchmark and inspect asm for this vs <<
#[inline(always)]
const fn const_shl_u8_usize(value: u8, shift: u8) -> usize {
    debug_assert!((shift as u32) < usize::BITS);
    debug_assert!(hlzu(value as usize) >= shift); // we never shift off 1 bits currently
    unsafe { (value as usize).unchecked_shl(shift as u32) }
}

// xxx benchmark and inspect asm for this vs <<
#[inline(always)]
const fn _const_shl_usize_usize(value: usize, shift: u8) -> usize {
    debug_assert!((shift as u32) < usize::BITS);
    debug_assert!(hlzu(value) >= shift); // we never shift off 1 bits currently
    unsafe { value.unchecked_shl(shift as u32) }
}

#[inline(always)]
const fn const_one_shl_usize(shift: u8) -> usize {
    debug_assert!((shift as u32) < usize::BITS);

    unsafe { 1usize.unchecked_shl(shift as u32) }
}

#[inline(always)]
const fn const_one_shl_u64(shift: u8) -> u64 {
    debug_assert!((shift as u32) < u64::BITS);

    unsafe { 1u64.unchecked_shl(shift as u32) }
}

#[inline(always)]
const fn _const_one_shl_u32(shift: u8) -> u32 {
    debug_assert!((shift as u32) < u32::BITS);

    unsafe { 1u32.unchecked_shl(shift as u32) }
}

#[inline(always)]
const fn const_one_shl_u8(shift: u8) -> u8 {
    debug_assert!((shift as u32) < u8::BITS);

    unsafe { 1u8.unchecked_shl(shift as u32) }
}

#[inline(always)]
const fn _const_shr_usize_usize(value: usize, shift: u8) -> usize {
    debug_assert!((shift as u32) < usize::BITS);
    unsafe { value.unchecked_shr(shift as u32) }
}

#[inline(always)]
const fn const_shr_usize_u32(value: usize, shift: u8) -> u32 {
    debug_assert!((shift as u32) < usize::BITS);
    let res = unsafe { value.unchecked_shr(shift as u32) };
    // No leaving 1 bits stranded up there
    debug_assert!(hlzu(res) as u32 >= usize::BITS - u32::BITS);
    res as u32
}

#[inline(always)]
const fn const_shr_usize_u8(value: usize, shift: u8) -> u8 {
    debug_assert!((shift as u32) < usize::BITS);
    let res = unsafe { value.unchecked_shr(shift as u32) };
    // No leaving 1 bits stranded up there
    debug_assert!(hlzu(res) as u32 >= usize::BITS - u8::BITS);
    res as u8
}

#[inline(always)]
const fn const_shr_u8_u8(value: u8, shift: u8) -> u8 {
    debug_assert!((shift as u32) < u8::BITS);
    unsafe { value.unchecked_shr(shift as u32) }
}

#[inline(always)]
const fn const_gen_mask_usize(numbits: u8) -> usize {
    debug_assert!((numbits as u32) < usize::BITS);

    unsafe { 1usize.unchecked_shl(numbits as u32) - 1 }
}

// xxx revisit (once again) replacing this with some variant of `<<`
#[inline(always)]
const fn const_gen_mask_u32(numbits: u8) -> u32 {
    debug_assert!((numbits as u64) <= u32::BITS as u64);

    unsafe { (1u64.unchecked_shl(numbits as u32) - 1) as u32 }
}

#[inline(always)]
const fn const_gen_mask_u8(numbits: u8) -> u8 {
    debug_assert!((numbits as u32) < u8::BITS);

    unsafe { 1u8.unchecked_shl(numbits as u32) - 1 }
}

/// Return the size class for the aligned size.
#[inline(always)]
fn req_to_sc(siz: usize, ali: usize) -> u8 {
    debug_assert!(siz > 0);
    debug_assert!(siz <= const_one_shl_usize(NUM_SCS - 1));
    debug_assert!(ali > 0);
    debug_assert!(ali < const_one_shl_usize(NUM_SCS));
    debug_assert!(ali.is_power_of_two());

    let res = ((siz - 1) | (ali - 1) | UNUSED_SC_MASK).ilog2() + 1;
    debug_assert!(res < NUM_SCS as u32);

    res as u8
}

#[inline(always)]
const fn _help_leading_zeros_u32(x: u32) -> u8 {
    let res = x.leading_zeros();
    debug_assert!(res <= u8::MAX as u32);
    res as u8
}
    
#[inline(always)]
const fn hlzu(x: usize) -> u8 {
    let res = x.leading_zeros();
    debug_assert!(res <= u8::MAX as u32);
    res as u8
}
    
#[inline(always)]
const fn help_leading_zeros_u64(x: u64) -> u8 {
    let res = x.leading_zeros();
    debug_assert!(res <= u8::MAX as u32);
    res as u8
}
    
#[inline(always)]
const fn help_trailing_zeros_usize(x: usize) -> u8 {
    x.trailing_zeros() as u8
}

pub use smalloc_macros::smalloc_main;

// xxx could we move this out to src/tests.rs using the ctor hack ? 
// For testing and benchmarking only.
#[cfg(test)]
mod unit_test_instance {
    use super::Smalloc;
    use std::sync::OnceLock;

    pub static mut SMAL: Smalloc = Smalloc::new();
    static INIT: OnceLock<()> = OnceLock::new();

    pub fn setup() {
        INIT.get_or_init(|| {
            unsafe {
                (*std::ptr::addr_of_mut!(SMAL)).init();
            }
        });
    }

    #[macro_export]
    #[cfg(debug_assertions)]
    macro_rules! get_testsmalloc {
        () => {
            #[allow(unused_unsafe)]
            unsafe { &*std::ptr::addr_of!($crate::unit_test_instance::SMAL) }
        };
    }
}

// xxx could we move this out to src/tests.rs and just have less clear output if the user runs `cargo test`?
#[cfg(debug_assertions)]
#[macro_export]
macro_rules! nextest_unit_tests {
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
                    
                unit_test_instance::setup();

                $body
            }
        )*
    };
}



#[cfg(test)]
mod tests;
