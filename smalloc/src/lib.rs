#![doc = include_str!("../../README.md")]
#![feature(stmt_expr_attributes)]
#![feature(likely_unlikely)]
#![feature(pointer_is_aligned_to)]
#![feature(unchecked_shifts)]


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
const NUM_SLABS_BITS: u8 = 5;

// The first three size classes (which would hold 1-byte, 2-byte, and 4-byte slots) are not used. In
// fact, we re-use the unused space in size class 0 to hold flh's.
const NUM_UNUSED_SCS: u8 = 3;


// --- Constant values determined by the constants above ---

// See the ASCII-art map in `README.md` for where these bits fit into addresses.

const UNUSED_SC_MASK: usize = const_one_shl_usize(NUM_UNUSED_SCS - 1);

const NUM_SCS: u8 = const_one_shl_u8(NUM_SC_BITS); // 32

// This is how many bits hold the data and the slotnum:
const NUM_SLOTNUM_AND_DATA_BITS: u8 = NUM_UNUSED_SCS + NUM_SCS; // 35

// This is how many bits to shift a sizeclass to fit the sizeclass into an address:
const SC_ADDR_SHIFT_BITS: u8 = NUM_SLOTNUM_AND_DATA_BITS; // 35

const SLABNUM_ALONE_MASK: u8 = const_gen_mask_u8(NUM_SLABS_BITS); // 0b11111
const SLABNUM_ADDR_MASK: usize = const_shl_u8_usize(SLABNUM_ALONE_MASK, NUM_SLOTNUM_AND_DATA_BITS + NUM_SC_BITS); // 0b111110000000000000000000000000000000000000000

const SC_BITS_ADDR_MASK: usize = const_shl_u8_usize(const_gen_mask_u8(NUM_SC_BITS), SC_ADDR_SHIFT_BITS); // 0b1111100000000000000000000000000000000000

const SLABNUM_AND_SC_ADDR_MASK: usize = const_shl_u16_usize(const_gen_mask_u16(NUM_SLABS_BITS + NUM_SC_BITS), SC_ADDR_SHIFT_BITS); // 0b111111111100000000000000000000000000000000000

const SLOTNUM_AND_DATA_ADDR_MASK: u64 = const_gen_mask_u64(NUM_SLOTNUM_AND_DATA_BITS); // 0b11111111111111111111111111111111111

const NUM_SLOTS_IN_HIGHEST_SC: u64 = const_one_shl_u64(NUM_UNUSED_SCS + 1); // 16
const HIGHEST_SLOTNUM_IN_HIGHEST_SC: u64 = NUM_SLOTS_IN_HIGHEST_SC - 2; // 14; The -1 is because the last slot isn't used since its slotnum is the sentinel slotnum.
const DATA_ADDR_BITS_IN_HIGHEST_SC: u8 = NUM_SCS - 1; // 31

// The smalloc address of the slot with the lowest address is:
const LOWEST_SMALLOC_SLOT_ADDR: usize = const_shl_u8_usize(NUM_UNUSED_SCS, SC_ADDR_SHIFT_BITS); // 0b1100000000000000000000000000000000000

// The smalloc address of the slot with the highest address is:
const HIGHEST_SMALLOC_SLOT_ADDR: usize = SLABNUM_ADDR_MASK | SC_BITS_ADDR_MASK | const_shl_u64_usize(HIGHEST_SLOTNUM_IN_HIGHEST_SC, DATA_ADDR_BITS_IN_HIGHEST_SC); // 0b111111111111100000000000000000000000000000000

// The smalloc address of the highest-addressed byte of a smalloc slot is:
const HIGHEST_SMALLOC_SLOT_BYTE_ADDR: usize = HIGHEST_SMALLOC_SLOT_ADDR | const_gen_mask_usize(NUM_SCS - 1); // 0b111111111111101111111111111111111111111111111

// We need to allocate extra bytes so that we can align the smalloc base pointer so that all of the
// trailing bits (all of the bits covered by the SMALLOC_ADDRESS_BITS_MASK) of the smalloc base
// pointer are zeros.

const BASEPTR_ALIGN: usize = (HIGHEST_SMALLOC_SLOT_BYTE_ADDR + 1).next_power_of_two(); // 0b1000000000000000000000000000000000000000000000
const SMALLOC_ADDRESS_BITS_MASK: usize = BASEPTR_ALIGN - 1; // 0b111111111111111111111111111111111111111111111
const TOTAL_VIRTUAL_MEMORY: usize = HIGHEST_SMALLOC_SLOT_BYTE_ADDR + SMALLOC_ADDRESS_BITS_MASK; // 0b1111111111111101111111111111111111111111111110 == 70_366_596_694_014

// Constants having to do with the use of flh pointers
const FLHDWORD_SIZE_BITS: u8 = 3; // 3 bits ie 8-byte sized flh dwords
const SLABNUM_FLH_SHIFT_BITS: u8 = FLHDWORD_SIZE_BITS + NUM_SC_BITS;
const SLABNUM_FLH_ADDR_MASK: usize = const_shl_u8_usize(SLABNUM_ALONE_MASK, SLABNUM_FLH_SHIFT_BITS); // 0b1111100000000
const SC_FLH_ADDR_MASK: usize = const_shl_u8_usize(NUM_SCS - 1, FLHDWORD_SIZE_BITS); // 0b11111000
const SLNSC_FLH_ADDR_MASK: usize = SLABNUM_FLH_ADDR_MASK | SC_FLH_ADDR_MASK;//xxx still used?

// One, left-shifted to the position of the ones value of the sc in flh addrs.
const SC_FLH_ADDR_UNIT: usize = const_one_shl_usize(FLHDWORD_SIZE_BITS); // 0b1000

// Constants having to do with the use of flh doublewords:
const FLHDWORD_PUSH_COUNTER_BITS: u8 = u64::BITS as u8 - NUM_SLOTNUM_AND_DATA_BITS;
const FLHDWORD_PUSH_COUNTER_MASK: u64 = const_shl_u32_u64(const_gen_mask_u32(FLHDWORD_PUSH_COUNTER_BITS), NUM_SLOTNUM_AND_DATA_BITS);
const FLHDWORD_PUSH_COUNTER_INCR: u64 = const_one_shl_u64(NUM_SLOTNUM_AND_DATA_BITS);

// How many positions to shift the slabnum-and-sizeclass from their position in the flh addr to
// their position in the data addr?
const SLNSC_SHIFT_FLH_TO_DATA: u8 = SC_ADDR_SHIFT_BITS - FLHDWORD_SIZE_BITS;

// --- Implementation ---

use std::sync::atomic::{AtomicU8, AtomicU64};
use std::cell::Cell;
use std::sync::atomic::Ordering::Relaxed;

static GLOBAL_THREAD_NUM: AtomicU8 = AtomicU8::new(0);

thread_local! {
    static THREAD_NUM: Cell<Option<u8>> = const { Cell::new(None) };
    static SLAB_NUM: Cell<Option<usize>> = const { Cell::new(None) };
}
// xxx try to get rid of this Option

/// Get this thread's unique, incrementing number.
// It is okay if more than 256 threads are spawned and this wraps, since the only things we use it
// for are (a) & with SLABNUM_ALONE_MASK (in `get_slab_num`) or & with STEPS_MASK (in
// `failover_slabnum`).
#[inline(always)]
fn get_thread_num() -> u8 {
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

/// Get the slab that this thread allocates from (as a usize, with the slabnum left-shifted to the
/// right location in an flh address). If uninitialized, this is initialized to `get_thread_num() *
/// SLABNUM_ALONE_MASK` (before left-shifting).
#[inline(always)]
fn get_slab_num() -> usize {
    SLAB_NUM.with(|cell| {
        cell.get().map_or_else(
            || const_shl_u8_usize(get_thread_num() & SLABNUM_ALONE_MASK, FLHDWORD_SIZE_BITS + NUM_SC_BITS),
            |value| value,
        )
    })
}

#[inline(always)]
fn set_slab_num(slabnum: usize) {
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

/// Pick a new slab to fail over to. This is used in two cases in `inner_alloc()`: a. when a slab is
/// full, and b. when there is a multithreading collision on the flh.
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
/// 3. It should be relatively prime to each other d used by other threads so that multiple threads
///    stepping at once will minimally "step" on each other (e.g. if one thread increased its slab
///    number by 3 and another by 6, then they'd be more likely to re-collide before trying all
///    possible slab numbers, but if they're relatively prime to each other then they'll be
///    minimally likely to recollide soon). This implies that d needs to be prime, which also
///    satisfies requirement 1 above.
#[inline(always)]
fn failover_slabnum(slnsc: usize, threadnum: u8) -> usize {
    const STEPS: [usize; 16] = [
        const_shl_u8_usize(31, SLABNUM_FLH_SHIFT_BITS),
        const_shl_u8_usize(29, SLABNUM_FLH_SHIFT_BITS),
        const_shl_u8_usize(23, SLABNUM_FLH_SHIFT_BITS),
        const_shl_u8_usize(19, SLABNUM_FLH_SHIFT_BITS),
        const_shl_u8_usize(17, SLABNUM_FLH_SHIFT_BITS),
        const_shl_u8_usize(13, SLABNUM_FLH_SHIFT_BITS),
        const_shl_u8_usize(11, SLABNUM_FLH_SHIFT_BITS),
        const_shl_u8_usize(7, SLABNUM_FLH_SHIFT_BITS),
        const_shl_u8_usize(5, SLABNUM_FLH_SHIFT_BITS),
        const_shl_u8_usize(3, SLABNUM_FLH_SHIFT_BITS),
        const_shl_u8_usize(1, SLABNUM_FLH_SHIFT_BITS),
        const_shl_u8_usize(31, SLABNUM_FLH_SHIFT_BITS),
        const_shl_u8_usize(29, SLABNUM_FLH_SHIFT_BITS),
        const_shl_u8_usize(23, SLABNUM_FLH_SHIFT_BITS),
        const_shl_u8_usize(19, SLABNUM_FLH_SHIFT_BITS),
        const_shl_u8_usize(17, SLABNUM_FLH_SHIFT_BITS),
    ];
    const STEPS_MASK: u8 = const_gen_mask_u8(4);
    let ix: usize = (const_shr_u8_u8(threadnum, 4) & STEPS_MASK) as usize;
    (slnsc + STEPS[ix]) & SLNSC_FLH_ADDR_MASK
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
    fn inner_dealloc(&self, p_addr: usize, sc: u8) {
        debug_assert!(p_addr != 0);
        debug_assert!(sc >= NUM_UNUSED_SCS);
        debug_assert!(sc < NUM_SCS);
        debug_assert!(sc == const_shr_usize_u8(p_addr & SC_BITS_ADDR_MASK, SC_ADDR_SHIFT_BITS));

        let smbp = self.inner().smbp;
        let flhptr = smbp | const_shr_usize_usize(p_addr & SLABNUM_AND_SC_ADDR_MASK, SLNSC_SHIFT_FLH_TO_DATA);
        let flh = unsafe { AtomicU64::from_ptr(flhptr as *mut u64) };
        let slotnum_unit = const_one_shl_u64(sc);
        let newslotnum = p_addr as u64 & SLOTNUM_AND_DATA_ADDR_MASK;
        debug_assert!(help_trailing_zeros_u64(newslotnum) >= sc);

        let sentinelslotnum = SLOTNUM_AND_DATA_ADDR_MASK & !const_gen_mask_u64(sc); // just for debug asserts
        debug_assert!(newslotnum < sentinelslotnum);

        loop {
            // Load the value (current first entry slot num) from the flh
            let flhdword = flh.load(Acquire);
            let curfirstentryslotnum = flhdword & SLOTNUM_AND_DATA_ADDR_MASK;
            debug_assert!(help_trailing_zeros_u64(curfirstentryslotnum) >= sc);
            debug_assert!(newslotnum != curfirstentryslotnum);
            // The curfirstentryslotnum can be the sentinel slotnum.
            debug_assert!(curfirstentryslotnum <= sentinelslotnum);

            // Encode the curfirstentryslotnum as the next-entry link for the new entry
            let next_entry_link = Self::encode_next_entry_link(newslotnum, curfirstentryslotnum, slotnum_unit);
            debug_assert!(curfirstentryslotnum == Self::decode_next_entry_link(newslotnum, next_entry_link, slotnum_unit));

            // Write it into the new slot's link
            unsafe { *(p_addr as *mut u64) = next_entry_link };

            // Increment the push counter
            let counter = (flhdword & FLHDWORD_PUSH_COUNTER_MASK).wrapping_add(FLHDWORD_PUSH_COUNTER_INCR);
            debug_assert!(help_trailing_zeros_u64(counter) >= NUM_SLOTNUM_AND_DATA_BITS);

            // The new flhdword is made up of the push counter and the newslotnum:
            let newflhdword = counter | newslotnum;

            // Compare and exchange
            if flh.compare_exchange(flhdword, newflhdword, AcqRel, Acquire).is_ok() { // xxx weaker ordering constraints okay?
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
        let orig_slabnum_for_flhp = get_slab_num();
        debug_assert!(help_trailing_zeros_usize(orig_slabnum_for_flhp) >= FLHDWORD_SIZE_BITS + NUM_SC_BITS);

        // The concatenation of slabnum bits and sc bits in their positions for use in an flh
        // address (i.e. left-shifted FLHDWORD_SIZE_BITS).
        let mut slnsc = orig_slabnum_for_flhp | const_shl_u8_usize(orig_sc, FLHDWORD_SIZE_BITS);

        let mut sentinelslotnum = SLOTNUM_AND_DATA_ADDR_MASK & !const_gen_mask_u64(orig_sc);
        let mut slotnum_unit = const_one_shl_u64(orig_sc);

        let mut loaded_threadnum: bool = false;
        let mut threadnum = 42; // the 42 will never get used
        let mut a_slab_was_full = false;

        let smbp = self.inner().smbp;

        loop {
            // Load the value from the flh
            let flhptr = smbp | slnsc;
            let flh = unsafe { AtomicU64::from_ptr(flhptr as *mut u64) };
            let flhdword = flh.load(Acquire); // xxx weaker ordering constraints ok?
            let curfirstentryslotnum = flhdword & SLOTNUM_AND_DATA_ADDR_MASK;
            debug_assert!(help_trailing_zeros_u64(curfirstentryslotnum) >= const_shr_usize_u8(slnsc & SC_FLH_ADDR_MASK, FLHDWORD_SIZE_BITS));

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
                let curfirstentry_p = (smbp | const_shl_usize_usize(slnsc, SLNSC_SHIFT_FLH_TO_DATA) | curfirstentryslotnum as usize) as *mut u64;
                debug_assert!(help_trailing_zeros_usize(curfirstentry_p as usize) >= const_shr_usize_u8(slnsc & SC_FLH_ADDR_MASK, FLHDWORD_SIZE_BITS));
                debug_assert!(const_shr_usize_u8(curfirstentry_p as usize & SC_BITS_ADDR_MASK, SC_ADDR_SHIFT_BITS) == const_shr_usize_u8(slnsc & SC_FLH_ADDR_MASK, FLHDWORD_SIZE_BITS));

                debug_assert!((curfirstentry_p as usize >= smbp + LOWEST_SMALLOC_SLOT_ADDR) && (curfirstentry_p as usize <= (smbp + HIGHEST_SMALLOC_SLOT_ADDR)));
                let curfirstentrylink_v = unsafe { *curfirstentry_p };
                let newfirstentryslotnum = Self::decode_next_entry_link(curfirstentryslotnum, curfirstentrylink_v, slotnum_unit);

                debug_assert!(newfirstentryslotnum != curfirstentryslotnum);
                
                // Write the new first entry slot num in place of the old in our local (in a
                // register) copy of flhdword, leaving the push-counter bits unchanged.
                let newflhdword = (flhdword & FLHDWORD_PUSH_COUNTER_MASK) | newfirstentryslotnum as u64;

                // Compare and exchange
                if likely(flh.compare_exchange(flhdword, newflhdword, AcqRel, Acquire).is_ok()) { // xxx weaker ordering constraints okay?
                    debug_assert!(help_trailing_zeros_u64(newfirstentryslotnum) >= const_shr_usize_u8(slnsc & SC_FLH_ADDR_MASK, FLHDWORD_SIZE_BITS));
                    if unlikely(orig_slabnum_for_flhp != (slnsc & SLABNUM_FLH_ADDR_MASK)) {
                        // The slabnum changed. Save the new slabnum for next time.
                        set_slab_num(slnsc & SLABNUM_FLH_ADDR_MASK);
                    }

                    break curfirstentry_p as *mut u8;
                } else {
                    // Update collision on the flh. Fail over to a different slab in the same size
                    // class.
                    if likely(!loaded_threadnum) { threadnum = get_thread_num(); loaded_threadnum = true; }

                    // Put the bits of the new slabnum into the slnsc.
                    slnsc = failover_slabnum(slnsc, threadnum);
                }
            } else {
                // If we got here then curfirstentryslotnum == sentinelslotnum, meaning no next
                // entry, meaning the free list is empty, meaning this slab is full. Overflow to a
                // different slab in the same size class.œ
                if likely(!loaded_threadnum) { threadnum = get_thread_num(); loaded_threadnum = true; }

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
                        if unlikely(slnsc & SC_FLH_ADDR_MASK == SC_FLH_ADDR_MASK) {
                            // This is the largest size class and we've exhausted at least one slab
                            // in it.
                            eprintln!("smalloc exhausted");
                            //xxxself.dump_map_of_slabs(); // for debugging only -- should probably be removed
                            break null_mut();
                        };

                        // Increment the sc
                        slnsc += SC_FLH_ADDR_UNIT;

                        // The sentinel slot num is different in this new size class.
                        sentinelslotnum = const_shl_u64_u64(sentinelslotnum, 1) & SLOTNUM_AND_DATA_ADDR_MASK;
                        // And so is slotnum_unit.
                        slotnum_unit = const_shl_u64_u64(slotnum_unit, 1);
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

//             let highestslotnum = highest_slotnum(const_shl_u8_usize(sc, SC_ADDR_SHIFT_BITS));
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

        let sc = const_shr_usize_u8(p_addr & SC_BITS_ADDR_MASK, SC_ADDR_SHIFT_BITS);
        debug_assert!(sc >= NUM_UNUSED_SCS);
        debug_assert!(sc < NUM_SCS);
        debug_assert!(help_trailing_zeros_usize(p_addr) >= sc);

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

        let oldsc = const_shr_usize_u8(p_addr & SC_BITS_ADDR_MASK, SC_ADDR_SHIFT_BITS);
        debug_assert!(oldsc >= NUM_UNUSED_SCS);
        debug_assert!(oldsc < NUM_SCS);
        debug_assert!(help_trailing_zeros_usize(p_addr) >= oldsc);

        let oldsc_from_args = req_to_sc(oldsize, oldalignment); // Just for debug_assert
        debug_assert!(oldsc >= NUM_UNUSED_SCS);
        debug_assert!(oldsc < NUM_SCS);

        // It's possible that the slot `ptr` is currently in is larger than the slot size necessary
        // to hold the size that the user requested when originally allocating (or re-allocating)
        // `ptr`.
        debug_assert!(oldsc >= oldsc_from_args);

        let reqsc = req_to_sc(reqsize, oldalignment);
        debug_assert!(reqsc >= NUM_UNUSED_SCS);
        debug_assert!(reqsc < NUM_SCS);

        // If the requested slot is <= the original slot, just return the pointer and we're done.
        if unlikely(reqsc <= oldsc) {
            return ptr;
        }

        if unlikely(reqsc >= NUM_SCS) {
            // This request exceeds the size of our largest sizeclass, so return null pointer.
            null_mut()
        } else {
            // // The "Growers" strategy. Promote the new sizeclass to the next one up in this
            // // schedule:
            // // xxx lookup table vs ifthenelse?
            // let reqsc =
            //     if reqsc < 6 { 6 } else // cache line size on x86 and non-Apple ARM
            //     if reqsc < 7 { 7 } else // cache line size on Apple Silicon
            //     if reqsc < 12 { 12 } else // page size on Linux and Windows
            //     if reqsc < 14 { 14 } else // page size on Apple OS
            //     if reqsc < 21 { 21 } else // huge/large/super-page size on various OSes
            // { reqsc };

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

// utility functions

use core::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::Ordering::{AcqRel, Acquire};
use plat::p::sys_alloc;
use std::ptr::{copy_nonoverlapping, null_mut};

// xxx look at asm and benchmark these vs the builtin alternatives

// xxx benchmark and inspect asm for this vs <<
#[inline(always)]
const fn _const_shl_u32_usize(value: u32, shift: u8) -> usize {
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

// xxx benchmark and inspect asm for this vs <<
#[inline(always)]
const fn const_shl_u64_u64(value: u64, shift: u8) -> u64 {
    debug_assert!((shift as u32) < u64::BITS);
    debug_assert!(help_leading_zeros_u64(value) >= shift); // we never shift off 1 bits currently
    unsafe { value.unchecked_shl(shift as u32) }
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
const fn const_shl_usize_usize(value: usize, shift: u8) -> usize {
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
const fn const_shr_usize_usize(value: usize, shift: u8) -> usize {
    debug_assert!((shift as u32) < usize::BITS);
    let res = unsafe { value.unchecked_shr(shift as u32) };
    // No leaving 1 bits stranded up there
    debug_assert!(hlzu(res) as u32 >= usize::BITS - u32::BITS);
    res
}

#[inline(always)]
const fn _const_shr_usize_u32(value: usize, shift: u8) -> u32 {
    debug_assert!((shift as u32) < usize::BITS);
    let res = unsafe { value.unchecked_shr(shift as u32) };
    // No leaving 1 bits stranded up there
    debug_assert!(hlzu(res) as u32 >= usize::BITS - u32::BITS);
    res as u32
}

#[inline(always)]
const fn _const_shr_u64_u32(value: u64, shift: u8) -> u32 {
    debug_assert!((shift as u32) < u64::BITS);
    let res = unsafe { value.unchecked_shr(shift as u32) };
    // No leaving 1 bits stranded up there
    debug_assert!(help_leading_zeros_u64(res) as u32 >= u64::BITS - u32::BITS);
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

#[inline(always)]
const fn const_gen_mask_u64(numbits: u8) -> u64 {
    debug_assert!((numbits as u32) < u64::BITS);

    unsafe { 1u64.unchecked_shl(numbits as u32) - 1 }
}

// xxx revisit (once again) replacing this with some variant of `<<`
#[inline(always)]
const fn const_gen_mask_u16(numbits: u8) -> u16 {
    debug_assert!((numbits as u64) <= u16::BITS as u64);

    unsafe { (1u32.unchecked_shl(numbits as u32) - 1) as u16 }
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

#[inline(always)]
const fn help_trailing_zeros_u64(x: u64) -> u8 {
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
