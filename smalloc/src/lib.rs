#![doc = include_str!("../../README.md")]
#![feature(stmt_expr_attributes)]
#![feature(likely_unlikely)]
#![feature(pointer_is_aligned_to)]
#![feature(unchecked_shifts)]

// Table of contents of this file:
//
// * Fixed constants chosen for the design
// * Constants determined by the constants above
// * Implementation


// macro for readability
macro_rules! gen_mask {
    ($bits:expr) => {
        ((1 << $bits) - 1)
    };
}


// --- Fixed constants chosen for the design ---

// NUM_SC_BITS is the main constant determining the rest of smalloc's layout. It is equal to 5
// because that means there are 32 size classes, and the first one (that is used -- see below) has
// 2^32 slots. This is the largest number of slots that we can encode their slot numbers into a
// 4-byte slot, which means that our smallest slots can be 4 bytes and we can pack more allocations
// of 1, 2, 3, or 4 bytes into each cache line.
const NUM_SC_BITS: u8 = 5;

// NUM_SLABS_BITS is the other constant. There are 2^NUM_SLABS_BITS slabs in each size class.
pub(crate) const NUM_SLABS_BITS: u8 = 5;

// The first three size classes (which would hold 1-byte, 2-byte, and 4-byte slots) are not used. In
// fact, we re-use the unused space in size class 0 to hold flh's.
const NUM_UNUSED_SCS: u8 = 3;


// --- Constants determined by the constants above ---

// See the ASCII-art map in `README.md` for where these bits fit into addresses.

const NUM_SCS: u8 = 1 << NUM_SC_BITS; // 32

// ---- Constants used to calculate contents of slot (and free list) addresses ----

// This is how many bits hold the data and the slotnum:
const NUM_SLOTNUM_AND_DATA_BITS: u8 = NUM_UNUSED_SCS + NUM_SCS; // 35

// This is how many bits to shift a sizeclass to fit the sizeclass into a slot address:
const SC_ADDR_SHIFT_BITS: u8 = NUM_SLOTNUM_AND_DATA_BITS; // 35

const SLABNUM_ALONE_MASK: u8 = gen_mask!(NUM_SLABS_BITS); // 0b11111
const SLABNUM_ADDR_MASK: usize = (SLABNUM_ALONE_MASK as usize) << (NUM_SLOTNUM_AND_DATA_BITS + NUM_SC_BITS); // 0b111110000000000000000000000000000000000000000

const SC_BITS_ADDR_MASK: usize = gen_mask!(NUM_SC_BITS) << SC_ADDR_SHIFT_BITS; // 0b1111100000000000000000000000000000000000

const SLABNUM_AND_SC_ADDR_MASK: usize = gen_mask!(NUM_SLABS_BITS + NUM_SC_BITS) << SC_ADDR_SHIFT_BITS; // 0b111111111100000000000000000000000000000000000

const SLOTNUM_AND_DATA_ADDR_MASK: u64 = gen_mask!(NUM_SLOTNUM_AND_DATA_BITS); // 0b11111111111111111111111111111111111

// ---- Constants having to do with the use of flh pointers ----

const FLHDWORD_SIZE_BITS: u8 = 3; // 3 bits ie 8-byte sized flh dwords
const SLABNUM_FLH_SHIFT_BITS: u8 = FLHDWORD_SIZE_BITS + NUM_SC_BITS;
const SLABNUM_FLH_ADDR_MASK: usize = (SLABNUM_ALONE_MASK as usize) << SLABNUM_FLH_SHIFT_BITS; // 0b1111100000000
const SC_FLH_ADDR_MASK: usize = gen_mask!(NUM_SC_BITS) << FLHDWORD_SIZE_BITS; // 0b11111000

// One, left-shifted to the position of the units (ones) value of the sizeclass in flh addrs.
const SC_FLH_ADDR_UNIT: usize = 1 << FLHDWORD_SIZE_BITS; // 0b1000

// ---- Constants having to do with the use of flh doublewords ----
const FLHDWORD_PUSH_COUNTER_BITS: u8 = u64::BITS as u8 - NUM_SLOTNUM_AND_DATA_BITS;
const FLHDWORD_PUSH_COUNTER_MASK: u64 = gen_mask!(FLHDWORD_PUSH_COUNTER_BITS) << NUM_SLOTNUM_AND_DATA_BITS;
const FLHDWORD_PUSH_COUNTER_INCR: u64 = 1 << NUM_SLOTNUM_AND_DATA_BITS;

// How many positions to shift the slabnum-and-sizeclass from their position in the flh addr to
// their position in the slot addr?
const SLNSC_SHIFT_FLH_ADDR_TO_SLOT_ADDR: u8 = SC_ADDR_SHIFT_BITS - FLHDWORD_SIZE_BITS;

// ---- Constants for calculating the total virtual address space to reserve ----

// The smalloc address of the slot with the lowest address is:
const LOWEST_SMALLOC_SLOT_ADDR: usize = (NUM_UNUSED_SCS as usize) << SC_ADDR_SHIFT_BITS; // 0b1100000000000000000000000000000000000

// The smalloc address of the slot with the highest address is:
const NUM_SLOTS_IN_HIGHEST_SC: u64 = 1 << (NUM_UNUSED_SCS + 1); // 16
const HIGHEST_SLOTNUM_IN_HIGHEST_SC: u64 = NUM_SLOTS_IN_HIGHEST_SC - 2; // 14; The -1 is because the last slot isn't used since its slotnum is the sentinel slotnum.
const DATA_ADDR_BITS_IN_HIGHEST_SC: u8 = NUM_SCS - 1; // 31
const HIGHEST_SMALLOC_SLOT_ADDR: usize = SLABNUM_ADDR_MASK | SC_BITS_ADDR_MASK | (HIGHEST_SLOTNUM_IN_HIGHEST_SC as usize) << DATA_ADDR_BITS_IN_HIGHEST_SC; // 0b111111111111100000000000000000000000000000000

// The smalloc address of the highest-addressed byte of a smalloc slot is:
const HIGHEST_SMALLOC_SLOT_BYTE_ADDR: usize = HIGHEST_SMALLOC_SLOT_ADDR | gen_mask!(DATA_ADDR_BITS_IN_HIGHEST_SC); // 0b111111111111101111111111111111111111111111111

// We need to allocate extra bytes so that we can align the smalloc base pointer so that all of the
// trailing bits of the smalloc base pointer are zeros.

const BASEPTR_ALIGN: usize = (HIGHEST_SMALLOC_SLOT_BYTE_ADDR + 1).next_power_of_two(); // 0b1000000000000000000000000000000000000000000000
const TOTAL_VIRTUAL_MEMORY: usize = HIGHEST_SMALLOC_SLOT_BYTE_ADDR + BASEPTR_ALIGN - 1; // 0b1111111111111101111111111111111111111111111110 == 70_366_596_694_014


// --- Implementation ---

use std::sync::atomic::{AtomicU8, AtomicU64};
use std::cell::Cell;

static GLOBAL_THREAD_NUM: AtomicU8 = AtomicU8::new(0);

const SLAB_NUM_SENTINEL: usize = usize::MAX;

thread_local! {
    static SLABNUM_AND_THREADNUM: Cell<(usize, u8)> = const { Cell::new((SLAB_NUM_SENTINEL, 0)) };
}

/// Get the slab number and thread number for this thread. On first call, initializes both.
/// Returns (slab_num, thread_num).
#[inline(always)]
fn get_slabnum_and_threadnum() -> (usize, u8) {
    SLABNUM_AND_THREADNUM.with(|cell| {
        let (slabnum, threadnum) = cell.get();
        if slabnum == SLAB_NUM_SENTINEL {
            let newthreadnum = GLOBAL_THREAD_NUM.fetch_add(1, Relaxed);
            let newslabnum = ((newthreadnum & SLABNUM_ALONE_MASK) as usize) << (FLHDWORD_SIZE_BITS + NUM_SC_BITS);
            cell.set((newslabnum, newthreadnum));
            (newslabnum, newthreadnum)
        } else {
            (slabnum, threadnum)
        }
    })
}

#[inline(always)]
fn set_slab_num(slabnum: usize) {
    SLABNUM_AND_THREADNUM.with(|cell| {
        let (_, thread_num) = cell.get();
        cell.set((slabnum, thread_num));
    });
}

use std::cell::UnsafeCell;

pub struct Smalloc {
    inner: UnsafeCell<SmallocInner>,
}

struct SmallocInner {
    smbp: usize,
}

/// Pick a new slab to fail over to. This is used in two cases in `inner_alloc()`: a. when a slab is
/// full, and b. when there is a multithreading collision on the flh.
///
/// Which new slabnumber shall we fail over to? A certain number, d, added to the current slab
/// number, and d should have these properties:
///
/// 1. It should be relatively prime to the number of slabs so that we will try all slabs before
///    returning to the original one.
///
/// 2. It should use the information from the thread number, not just the (strictly lesser)
///    information from the original slab number.
/// 
/// 3. It should be relatively prime to each other d used by other threads so that multiple threads
///    stepping at once will minimally "step" on each other (e.g. if one thread increased its slab
///    number by 3 and another by 6, then they'd be more likely to re-collide before trying all
///    possible slab numbers, but if they're relatively prime to each other then they'll be
///    minimally likely to recollide soon). This implies that d needs to be prime, which also
///    satisfies requirement 1 above.
#[inline(always)]
fn failover_slabnum(slnsc: usize, threadnum: u8) -> usize {
    macro_rules! shifted_steps {
        ($shift:expr; $($step:expr),+ $(,)?) => {
            [$( $step << $shift ),+]
        };
    }

    const STEPS: [usize; 16] = shifted_steps!(SLABNUM_FLH_SHIFT_BITS; 31, 29, 23, 19, 17, 13, 11, 7, 5, 3, 1, 31, 29, 23, 19, 17);
    const STEPS_MASK: u8 = gen_mask!(4);

    let ix: usize = ((threadnum >> 4) & STEPS_MASK) as usize;
    (slnsc + STEPS[ix]) & (SLABNUM_FLH_ADDR_MASK | SC_FLH_ADDR_MASK)
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
                smbp: 0,
            }),
        }
    }

    #[inline(always)]
    fn inner_dealloc(&self, p_addr: usize, sc: u8) {
        debug_assert!(p_addr != 0);
        debug_assert!(sc >= NUM_UNUSED_SCS);
        debug_assert!(sc < NUM_SCS);
        debug_assert!(sc == ((p_addr & SC_BITS_ADDR_MASK) >> SC_ADDR_SHIFT_BITS) as u8);

        let smbp = self.inner().smbp;
        let flhptr = smbp | (p_addr & SLABNUM_AND_SC_ADDR_MASK) >> SLNSC_SHIFT_FLH_ADDR_TO_SLOT_ADDR;
        let flh = unsafe { AtomicU64::from_ptr(flhptr as *mut u64) };
        let newslotnum = p_addr as u64 & SLOTNUM_AND_DATA_ADDR_MASK;
        debug_assert!(newslotnum.trailing_zeros() >= sc as u32);

        let sentinelslotnum = SLOTNUM_AND_DATA_ADDR_MASK & !gen_mask!(sc); // just for debug asserts
        debug_assert!(newslotnum < sentinelslotnum);

        loop {
            // Load the value (current first entry slot num) from the flh
            let flhdword = flh.load(Relaxed);
            let curfirstentryslotnum = flhdword & SLOTNUM_AND_DATA_ADDR_MASK;
            debug_assert!(curfirstentryslotnum.trailing_zeros() >= sc as u32);
            debug_assert!(newslotnum != curfirstentryslotnum);
            // The curfirstentryslotnum can be the sentinel slotnum.
            debug_assert!(curfirstentryslotnum <= sentinelslotnum);

            // Encode the curfirstentryslotnum as the next-entry link for the new entry
            let next_entry_link = Self::encode_next_entry_link(newslotnum, curfirstentryslotnum, 1 << sc);
            debug_assert!(curfirstentryslotnum == Self::decode_next_entry_link(newslotnum, next_entry_link, 1 << sc));

            // Write it into the new slot's link
            unsafe { *(p_addr as *mut u64) = next_entry_link };

            // Increment the push counter
            let counter = (flhdword & FLHDWORD_PUSH_COUNTER_MASK).wrapping_add(FLHDWORD_PUSH_COUNTER_INCR);
            debug_assert!(counter.trailing_zeros() >= NUM_SLOTNUM_AND_DATA_BITS as u32);

            // The new flhdword is made up of the push counter and the newslotnum:
            let newflhdword = counter | newslotnum;

            // Compare and exchange
            if flh.compare_exchange_weak(flhdword, newflhdword, Release, Relaxed).is_ok() {
                break;
            }
        }
    }

    #[inline(always)]
    fn inner_alloc(&self, orig_sc: u8) -> *mut u8 {
        debug_assert!(orig_sc >= NUM_UNUSED_SCS);
        debug_assert!(orig_sc < NUM_SCS);

        // If the slab is full, or if there is a collision when updating the flh, we'll switch to
        // another slab in this same sizeclass.

        // `orig_slabnum_for_flhp` has the slabnum in the bit-position where it fits into an flh
        // address.
        let (orig_slabnum_for_flhp, threadnum) = get_slabnum_and_threadnum();
        debug_assert!(orig_slabnum_for_flhp.trailing_zeros() >= (FLHDWORD_SIZE_BITS + NUM_SC_BITS) as u32);

        // "slnsc" is the concatenation of the slabnum bits and sc bits in their positions for use
        // in an flh address (i.e. left-shifted FLHDWORD_SIZE_BITS).
        let mut slnsc = orig_slabnum_for_flhp | (orig_sc as usize) << FLHDWORD_SIZE_BITS;

        // In this size class, the sentinel slotnum and the unit value of the sc are:
        let mut sentinelslotnum = SLOTNUM_AND_DATA_ADDR_MASK & !gen_mask!(orig_sc);
        let mut slotnum_unit = 1 << orig_sc;

        let mut a_slab_was_full = false;

        let smbp = self.inner().smbp;

        loop {
            // Load the value from the flh
            let flhptr = smbp | slnsc;
            let flh = unsafe { AtomicU64::from_ptr(flhptr as *mut u64) };
            let flhdword = flh.load(Acquire);
            let curfirstentryslotnum = flhdword & SLOTNUM_AND_DATA_ADDR_MASK;
            debug_assert!(curfirstentryslotnum.trailing_zeros() >= ((slnsc & SC_FLH_ADDR_MASK) >> FLHDWORD_SIZE_BITS) as u32);

            // curfirstentryslotnum can be the sentinel value.
            debug_assert!(curfirstentryslotnum <= sentinelslotnum);
            if likely(curfirstentryslotnum < sentinelslotnum) {
                // There is a slot available in the free list.
                
                // Read the bits from the first entry's link (to the second entry) and decode them
                // into a slot number. These bits might be invalid, if the flh has changed since we
                // read it above and another thread has started using these bits for something else
                // (e.g. user data or another linked list update). That's okay because in that case
                // our attempt to update the flh below will fail so the invalid bits will not get
                // stored.
                let curfirstentry_p = (smbp | (slnsc << SLNSC_SHIFT_FLH_ADDR_TO_SLOT_ADDR) | curfirstentryslotnum as usize) as *mut u64;
                debug_assert!((curfirstentry_p as usize).trailing_zeros() >= ((slnsc & SC_FLH_ADDR_MASK) >> FLHDWORD_SIZE_BITS) as u32);
                // xxx use some functions or macros or something to make this more readable? :-}
                debug_assert!(((curfirstentry_p as usize & SC_BITS_ADDR_MASK) >> SC_ADDR_SHIFT_BITS) == ((slnsc & SC_FLH_ADDR_MASK) >> FLHDWORD_SIZE_BITS));

                debug_assert!((curfirstentry_p as usize >= smbp + LOWEST_SMALLOC_SLOT_ADDR) && (curfirstentry_p as usize <= (smbp + HIGHEST_SMALLOC_SLOT_ADDR)));
                let curfirstentrylink_v = unsafe { *curfirstentry_p };
                let newfirstentryslotnum = Self::decode_next_entry_link(curfirstentryslotnum, curfirstentrylink_v, slotnum_unit);

                // Write the new first entry slot num in place of the old in our local (in a
                // register) copy of flhdword, leaving the push-counter bits unchanged.
                let newflhdword = (flhdword & FLHDWORD_PUSH_COUNTER_MASK) | newfirstentryslotnum;

                // Compare and exchange
                if likely(flh.compare_exchange_weak(flhdword, newflhdword, Acquire, Relaxed).is_ok()) { 
                    debug_assert!(newfirstentryslotnum != curfirstentryslotnum);
                    debug_assert!(newfirstentryslotnum.trailing_zeros() as usize >= (slnsc & SC_FLH_ADDR_MASK) >> FLHDWORD_SIZE_BITS);
                    if unlikely(orig_slabnum_for_flhp != (slnsc & SLABNUM_FLH_ADDR_MASK)) {
                        // The slabnum changed. Save the new slabnum for next time.
                        set_slab_num(slnsc & SLABNUM_FLH_ADDR_MASK);
                    }

                    break curfirstentry_p as *mut u8;
                } else {
                    // Update collision on the flh. Fail over to a different slab in the same size
                    // class.

                    // Put the bits of the new slabnum into the slnsc.
                    slnsc = failover_slabnum(slnsc, threadnum);
                }
            } else {
                // If we got here then curfirstentryslotnum == sentinelslotnum, meaning no next
                // entry, meaning the free list is empty, meaning this slab is full. Overflow to a
                // different slab in the same size class.œ

                // Put the bits of the new slabnum into the slnsc.
                slnsc = failover_slabnum(slnsc, threadnum);

                if likely((slnsc & SLABNUM_FLH_ADDR_MASK) != orig_slabnum_for_flhp) {
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
                        if unlikely((slnsc & SC_FLH_ADDR_MASK) == SC_FLH_ADDR_MASK) {
                            // This is the largest size class and we've exhausted at least one slab
                            // in it.
                            eprintln!("smalloc exhausted");
                            //xxxself.dump_map_of_slabs(); // for debugging only -- should probably be removed
                            break null_mut();
                        };

                        // Increment the sc
                        slnsc += SC_FLH_ADDR_UNIT;

                        // The sentinel slot num and the sc unit are different in this new size class.
                        sentinelslotnum = unsafe { sentinelslotnum.unchecked_shl(1) } & SLOTNUM_AND_DATA_ADDR_MASK;
                        slotnum_unit <<= 1;
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

    /// Initializes the allocator.
    ///
    /// # Safety
    ///
    /// The caller must ensure:
    /// - This method is called exactly once
    /// - This is called before any dynamic allocation occurs
    /// - No other thread simultaneously accesses this `Smalloc` instance during this call to `init`.
    pub unsafe fn init(&self) {
        let sysbp = sys_alloc(TOTAL_VIRTUAL_MEMORY).unwrap().addr();
        assert!(sysbp != 0);
        self.inner_mut().smbp = sysbp.next_multiple_of(BASEPTR_ALIGN);
    }

    #[inline(always)]
    fn encode_next_entry_link(baseslotnum: u64, targslotnum: u64, slotnum_unit: u64) -> u64 {
        targslotnum.wrapping_sub(baseslotnum).wrapping_sub(slotnum_unit) & SLOTNUM_AND_DATA_ADDR_MASK
    }

    #[inline(always)]
    fn decode_next_entry_link(baseslotnum: u64, codeword: u64, slotnum_unit: u64) -> u64 {
        baseslotnum.wrapping_add(codeword).wrapping_add(slotnum_unit) & SLOTNUM_AND_DATA_ADDR_MASK
    }
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

        if unlikely(sc >= NUM_SCS) {
            // This request exceeds the size of our largest sizeclass, so return null pointer.
            null_mut()
        } else {
            self.inner_alloc(sc)
        }
    }

    #[inline(always)]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        debug_assert!(!ptr.is_null());
        debug_assert!(layout.align().is_power_of_two()); // alignment must be a power of two

        let p_addr = ptr.addr();

        // To be valid, the pointer has to be greater than or equal to the smalloc base pointer and
        // less than or equal to the highest slot pointer.
        let smbp = self.inner().smbp;
        let lowest_addr = smbp + LOWEST_SMALLOC_SLOT_ADDR;
        let highest_addr = smbp + HIGHEST_SMALLOC_SLOT_ADDR;
        assert!((p_addr >= lowest_addr) && (p_addr <= highest_addr));

        // Okay now we know that it is a pointer into smalloc's region.

        let sc = ((p_addr & SC_BITS_ADDR_MASK) >> SC_ADDR_SHIFT_BITS) as u8;
        debug_assert!(sc >= NUM_UNUSED_SCS);
        debug_assert!(sc < NUM_SCS);
        debug_assert!(p_addr.trailing_zeros() >= sc as u32);

        self.inner_dealloc(p_addr, sc);
    }

    #[inline(always)]
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, reqsize: usize) -> *mut u8 {
        debug_assert!(!ptr.is_null());

        let p_addr = ptr.addr();
        let smbp = self.inner().smbp;

        // To be valid, the pointer has to be greater than or equal to the smalloc base pointer and
        // less than or equal to the highest slot pointer.
        let lowest_addr = smbp + LOWEST_SMALLOC_SLOT_ADDR;
        let highest_addr = smbp + HIGHEST_SMALLOC_SLOT_ADDR;

        assert!((p_addr >= lowest_addr) && (p_addr <= highest_addr));

        let oldsize = layout.size();
        debug_assert!(oldsize > 0);
        let oldalignment = layout.align();
        debug_assert!(oldalignment > 0);
        debug_assert!(oldalignment.is_power_of_two()); // alignment must be a power of two
        debug_assert!(reqsize > 0);

        let oldsc = ((p_addr & SC_BITS_ADDR_MASK) >> SC_ADDR_SHIFT_BITS) as u8;
        debug_assert!(oldsc >= NUM_UNUSED_SCS);
        debug_assert!(oldsc < NUM_SCS);
        debug_assert!(p_addr.trailing_zeros() >= oldsc as u32);

        let oldsc_from_args = req_to_sc(oldsize, oldalignment); // Just for debug_assert
        debug_assert!(oldsc_from_args >= NUM_UNUSED_SCS);
        debug_assert!(oldsc_from_args < NUM_SCS);

        // It's possible that the slot `ptr` is currently in is larger than the slot size necessary
        // to hold the size that the user requested when originally allocating (or re-allocating)
        // `ptr`.
        debug_assert!(oldsc >= oldsc_from_args);

        let reqsc = req_to_sc(reqsize, oldalignment);
        debug_assert!(reqsc >= NUM_UNUSED_SCS);

        // If the requested slot is <= the original slot, just return the pointer and we're done.
        if unlikely(reqsc <= oldsc) {
            return ptr;
        }

        if unlikely(reqsc >= NUM_SCS) {
            // This request exceeds the size of our largest sizeclass, so return null pointer.
            null_mut()
        } else {
            // The "Growers" strategy. Promote the new sizeclass to the next one up in this
            // schedule:
            let reqsc =
                if reqsc <= 6 { 6 } else // cache line size on x86 and non-Apple ARM
                if reqsc <= 7 { 7 } else // cache line size on Apple Silicon
                if reqsc <= 12 { 12 } else // page size on Linux and Windows
                if reqsc <= 14 { 14 } else // page size on Apple OS
                if reqsc <= 18 { 18 } else // this is just so the larger sc's don't get filled up
                if reqsc <= 21 { 21 } else // huge/large/super-page size on various OSes
            { reqsc };

            let newp = self.inner_alloc(reqsc);
            debug_assert!(!newp.is_null());
            debug_assert!(newp.is_aligned_to(oldalignment));

            // Copy the contents from the old location.
            unsafe { copy_nonoverlapping(ptr, newp, oldsize); }

            // Free the old slot.
            self.inner_dealloc(p_addr, oldsc);

            newp
        }
    }
}

/// Return the size class for the aligned size.
#[inline(always)]
fn req_to_sc(siz: usize, ali: usize) -> u8 {
    debug_assert!(siz > 0);
    debug_assert!(siz <= gen_mask!(NUM_SCS));
    debug_assert!(ali > 0);
    debug_assert!(ali < 1 << NUM_SCS);
    debug_assert!(ali.is_power_of_two());

    const UNUSED_SC_MASK: usize = gen_mask!(NUM_UNUSED_SCS);

    let res = ((siz - 1) | (ali - 1) | UNUSED_SC_MASK).ilog2() + 1;
    debug_assert!(res < NUM_SCS as u32);

    res as u8
}

pub use smalloc_macros::smalloc_main;

#[cfg(test)]
mod tests;

use core::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use plat::p::sys_alloc;
use std::ptr::{copy_nonoverlapping, null_mut};
