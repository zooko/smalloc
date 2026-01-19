#![feature(likely_unlikely)]

// Table of contents of this file:
//
// * Public structs and methods
// * Private implementation code 
//   + Fixed constants chosen for the design
//   + Constants determined by the constants above
//     - Constants having to do with the use of slot (and free list) pointers
//     - Constants having to do with the use of flh pointers
//     - Constants having to do with the use of flh words
//     - Constants for calculating the total virtual address space to reserve
//xxx update this ToC


// --- Public structs and methods ---

pub struct Smalloc {
    inner: UnsafeCell<SmallocInner>,
}

impl Smalloc {
    pub const fn new() -> Self { Self {
        inner: UnsafeCell::new(SmallocInner {
            smbp: AtomicUsize::new(0),
            initlock: AtomicBool::new(false),
        }),
    } }

    #[inline(always)]
    pub fn idempotent_init(&self) {
        let inner = self.inner();
        let smbpval = inner.smbp.load(Relaxed);
        if smbpval == 0 {
            // acquire the spin lock
            loop {
                if inner.initlock.compare_exchange_weak(false, true, Acquire, Relaxed).is_ok() {
                    break;
                }
            }

            let smbpval = inner.smbp.load(Relaxed);
            if smbpval == 0 {
                let sysbp = sys_alloc(TOTAL_VIRTUAL_MEMORY).unwrap().addr();
                assert!(sysbp != 0);
                let smbp = sysbp.next_multiple_of(BASEPTR_ALIGN);

                #[cfg(any(target_os = "windows", doc))]
                {
                    let size_of_flh_area = (1usize << FLHWORD_SIZE_BITS) * (1usize << NUM_SLABS_BITS) * NUM_SCS as usize;
                    sys_commit(smbp as *mut u8, size_of_flh_area).unwrap();
                }

                inner.smbp.store(smbp, Release);
            }

            // release the spin lock
            inner.initlock.store(false, Release);
        }
    }
}

unsafe impl GlobalAlloc for Smalloc {
    #[inline(always)]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let reqsiz = layout.size();
        let reqalign = layout.align();
        debug_assert!(reqsiz > 0);
        debug_assert!(reqalign > 0);
        debug_assert!(reqalign.is_power_of_two());

        let sc = reqali_to_sc(reqsiz, reqalign);

        if unlikely(sc >= NUM_SCS) {
            // This request exceeds the size of our largest sizeclass, so return null pointer.
            null_mut()
        } else {
            self.idempotent_init();
            self.inner_alloc(sc, false)
        }
    }

    #[inline(always)]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        debug_assert!(layout.align().is_power_of_two());

        self.inner_dealloc(ptr.addr());
    }

    #[inline(always)]
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let reqsiz = layout.size();
        let reqalign = layout.align();
        debug_assert!(reqsiz > 0);
        debug_assert!(reqalign > 0);
        debug_assert!(reqalign.is_power_of_two());

        let sc = reqali_to_sc(reqsiz, reqalign);

        if unlikely(sc >= NUM_SCS) {
            // This request exceeds the size of our largest sizeclass, so return null pointer.
            null_mut()
        } else {
            self.idempotent_init();
            self.inner_alloc(sc, true)
        }
    }

    #[inline(always)]
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, reqsize: usize) -> *mut u8 {
        debug_assert!(!ptr.is_null());

        let p_addr = ptr.addr();
        let smbp = self.inner().smbp.load(Relaxed);

        // To be valid, the pointer has to be greater than or equal to the smalloc base pointer and
        // less than or equal to the highest slot pointer.
        assert!(p_addr >= smbp);
        assert!(p_addr - smbp >= LOWEST_SMALLOC_SLOT_ADDR && p_addr - smbp <= HIGHEST_SMALLOC_SLOT_ADDR);

        // Okay now we know that it is a pointer into smalloc's region.
        let oldsize = layout.size();
        debug_assert!(oldsize > 0);
        let oldalignment = layout.align();
        debug_assert!(oldalignment > 0);
        debug_assert!(oldalignment.is_power_of_two());
        debug_assert!(reqsize > 0);

        debug_assert!(reqali_to_sc(oldsize, oldalignment) >= NUM_UNUSED_SCS);
        debug_assert!(reqali_to_sc(oldsize, oldalignment) < NUM_SCS);

        let oldsc = ((p_addr & SC_BITS_ADDR_MASK) >> NUM_SN_D_T_BITS) as u8;
        debug_assert!(oldsc >= NUM_UNUSED_SCS);
        debug_assert!(oldsc < NUM_SCS);
        debug_assert!(p_addr.trailing_zeros() >= oldsc as u32);

        // It's possible that the slot `ptr` is currently in is larger than the slot size necessary
        // to hold the size that the user requested when originally allocating (or re-allocating)
        // `ptr`.
        debug_assert!(oldsc >= reqali_to_sc(oldsize, oldalignment));

        let reqsc = reqali_to_sc(reqsize, oldalignment);
        debug_assert!(reqsc >= NUM_UNUSED_SCS);

        // If the requested slot is <= the original slot, just return the pointer and we're done.
        if unlikely(reqsc <= oldsc) {
            return ptr;
        }

        if unlikely(reqsc >= NUM_SCS) {
            // This request exceeds the size of our largest sizeclass, so return null pointer.
            null_mut()
        } else {
            // The "Growers" strategy.
            let reqsc = if (plat::p::SC_FOR_PAGE..GROWERS_SC).contains(&reqsc) { GROWERS_SC } else { reqsc };

            let newp = self.inner_alloc(reqsc, false);
            if unlikely(newp.is_null()) {
                // smalloc slots must be exhausted
                return newp;
            }

            // Copy the contents from the old location.
            unsafe { copy_nonoverlapping(ptr, newp, oldsize); }

            // Free the old slot.
            self.inner_dealloc(p_addr);

            newp
        }
    }
}


// --- Private implementation code ---

// gen_mask macro for readability
macro_rules! gen_mask { ($bits:expr, $ty:ty) => { ((!0 as $ty) >> (<$ty>::BITS - ($bits) as u32)) }; }

#[doc(hidden)]
pub mod i {
    // Everything in this `i` ("internal") module is for the use of the smalloc core lib (this file)
    // and for the use of the smalloc-ffi package.

    pub struct SmallocInner {
        pub smbp: AtomicUsize,
        pub initlock: AtomicBool
    }

    impl Smalloc {
        #[inline(always)]
        pub fn inner_dealloc(&self, p_addr: usize) {
            // To be valid, the pointer has to be greater than or equal to the smalloc base pointer and
            // less than or equal to the highest slot pointer.
            let smbp = self.inner().smbp.load(Relaxed);
            debug_assert!(p_addr >= smbp);
            debug_assert!(p_addr - smbp >= LOWEST_SMALLOC_SLOT_ADDR && p_addr - smbp <= HIGHEST_SMALLOC_SLOT_ADDR);

            // Okay now we know that it is a pointer into smalloc's region.

            // The sizeclass is encoded into the most-significant bits of the address:
            let sc = ((p_addr & SC_BITS_ADDR_MASK) >> NUM_SN_D_T_BITS) as u8;
            debug_assert!(sc >= NUM_UNUSED_SCS);
            debug_assert!(sc < NUM_SCS);

            // The flhptr for this sizeclass and slabnum is at this location, which we can calculate
            // by masking in the slabnum and sizeclass bits from the address and shifting them
            // right:
            const SLABNUM_AND_SC_ADDR_MASK: usize = SLABNUM_BITS_ADDR_MASK | SC_BITS_ADDR_MASK;
            let flhptr = smbp | (p_addr & SLABNUM_AND_SC_ADDR_MASK) >> (NUM_SN_D_T_BITS - FLHWORD_SIZE_BITS);
            let flh = unsafe { AtomicU64::from_ptr(flhptr as *mut u64) };

            let newslotnum = ((p_addr as u64 & SLOTNUM_AND_DATA_ADDR_MASK) >> sc) as u32;
            let sentinel_slotnum = gen_mask!(NUM_SN_BITS - (sc - NUM_UNUSED_SCS), u32);
            debug_assert!(newslotnum < sentinel_slotnum);

            loop {
                // Load the value (current first entry slotnum and next-entry-touched bit) from the flh
                let flhword = flh.load(Relaxed);

                // The low-order 4-byte word is the slotnum and the touched-bit of the first entry
                let curfirstentry = flhword as u32;

                debug_assert!(newslotnum != curfirstentry & ENTRY_SLOTNUM_MASK);
                // The curfirstentryslotnum can be the sentinel slotnum.
                debug_assert!(curfirstentry & ENTRY_SLOTNUM_MASK <= sentinel_slotnum);

                // Write it into the new slot's link
                unsafe { *(p_addr as *mut u32) = curfirstentry };

                // The high-order 4-byte word is the push counter. Increment it.
                let push_counter = (flhword & FLHWORD_PUSH_COUNTER_MASK).wrapping_add(FLHWORD_PUSH_COUNTER_INCR);

                // The new flh word is the push counter, the next-entry-touched-bit (set), and the next-entry slotnum.
                let newflhword = push_counter | ENTRY_NEXT_TOUCHED_BIT as u64 | newslotnum as u64;

                // Compare and exchange
                if flh.compare_exchange_weak(flhword, newflhword, Release, Relaxed).is_ok() {
                    break;
                }
            }
        }

        /// zeromem says whether to ensure that the allocated memory is all zeroed out or not
        #[inline(always)]
        pub fn inner_alloc(&self, orig_sc: u8, zeromem: bool) -> *mut u8 {
            debug_assert!(orig_sc >= NUM_UNUSED_SCS);
            debug_assert!(orig_sc < NUM_SCS);

            // If the slab is full, or if there is a collision when updating the flh, we'll switch to
            // another slab in this same sizeclass.
            let orig_slabnum = get_slabnum();
            let mut slabnum = orig_slabnum;
            let mut a_slab_was_full = false;

            // If all slabs in the sizeclass are full, we'll switch to the next sizeclass.
            let mut sc = orig_sc;

            let smbp = self.inner().smbp.load(Acquire);

            loop {
                // The flhptr for this sizeclass and slabnum is at this location:
                let slabnum_and_sc = (slabnum as usize) << NUM_SC_BITS | sc as usize;
                let flhptr = smbp | slabnum_and_sc << FLHWORD_SIZE_BITS;

                // Load the value from the flh
                let flh = unsafe { AtomicU64::from_ptr(flhptr as *mut u64) };
                let flhword = flh.load(Acquire);

                // The low-order 4-byte word is the slotnum and touched-bit of the first entry.
                let curfirstentry = flhword as u32;

                // The sentinel slotnum for this sizeclass:
                let sentinel_slotnum = gen_mask!(NUM_SN_BITS - (sc - NUM_UNUSED_SCS), u32);

                // The curfirstentry slotnum can be the sentinel slotnum, but not larger.
                debug_assert!(curfirstentry & ENTRY_SLOTNUM_MASK <= sentinel_slotnum);

                // If the curfirstentry next-slotnum is the sentinel slotnum, then the
                // next-has-been-touched bit must be false. (You can't ever touch—read or
                // write—the memory of the sentinel slot.)
                debug_assert!(if curfirstentry & ENTRY_SLOTNUM_MASK == sentinel_slotnum { curfirstentry & ENTRY_NEXT_TOUCHED_BIT == 0 } else { true });

                if likely(curfirstentry != sentinel_slotnum) {
                    // There is a slot available in the free list.

                    // Here's the pointer to the current first entry:
                    let curfirstentryslotnum = curfirstentry & ENTRY_SLOTNUM_MASK;
                    let curfirstentry_p = smbp | (slabnum_and_sc << NUM_SN_D_T_BITS) | (curfirstentryslotnum as usize) << sc;
                    debug_assert!((curfirstentry_p - smbp >= LOWEST_SMALLOC_SLOT_ADDR) && (curfirstentry_p - smbp <= HIGHEST_SMALLOC_SLOT_ADDR));

                    let next_entry = if likely((curfirstentry & ENTRY_NEXT_TOUCHED_BIT) != 0) {
                        // Read the bits from the first entry's space (which are about the second
                        // entry). These bits might be invalid, if the flh has changed since we read
                        // it above and another thread has started using this memory for something
                        // else (e.g. user data or another linked list update). That's okay because
                        // in that case our attempt to update the flh (since the flh must have
                        // changed) below will fail, so information derived from the invalid bits
                        // will not get stored.
                        unsafe { *(curfirstentry_p as *mut u32) }
                    } else {
                        // If this entry has never been touched (read or written), then its
                        // next-entry link is equal to its slotnum + 1 (with the touched bit unset).

                        #[cfg(any(target_os = "windows", doc))]
                        {
                            // And, we have to commit its page/slot before the CAS update to prepare
                            // for this slot—or any subsequent slot beyond this one in this memory
                            // page—getting accessed. (There was a bug in smalloc v7.5.2 when this
                            // commit was after the CAS succeeded, and a *successor* slot got
                            // accessed before the commit.)

                            // commit the larger of 1 slot and 1 page
                            let cbits = std::cmp::max(plat::p::SC_FOR_PAGE, sc);
                            if unlikely(curfirstentry_p.trailing_zeros() >= cbits as u32) {
                                sys_commit(curfirstentry_p as *mut u8, 1 << cbits).unwrap();
                            }
                        }

                        curfirstentryslotnum + 1
                    };

                    // Put the new first entry (which is the old second entry) in place of the old
                    // first entry (which is going to be the return value) in our local copy of
                    // flhword, leaving the push-counter bits unchanged.
                    let newflhword = (flhword & FLHWORD_PUSH_COUNTER_MASK) | next_entry as u64;

                    // Compare and exchange
                    if likely(flh.compare_exchange_weak(flhword, newflhword, Acquire, Relaxed).is_ok()) { 
                        debug_assert!(next_entry & ENTRY_SLOTNUM_MASK != curfirstentryslotnum);
                        debug_assert!(next_entry & ENTRY_SLOTNUM_MASK <= sentinel_slotnum);

                        // Okay we've successfully allocated a slot! If `zeromem` is requested and
                        // this slot has previously been touched then we have to zero its contents.
                        if unlikely(zeromem) && unlikely((curfirstentry & ENTRY_NEXT_TOUCHED_BIT) != 0) {
                            unsafe { core::ptr::write_bytes(curfirstentry_p as *mut u8, 0, 1 << sc) };
                        }

                        break curfirstentry_p as *mut u8;
                    } else {
                        // We encountered an update collision on the flh. Fail over to a different
                        // slab in the same size class.
                        slabnum = (slabnum + SLAB_FAILOVER_NUM) & SLABNUM_BITS_ALONE_MASK;
                    }
                } else {
                    // If we got here then curfirstentryslotnum == sentinelslotnum, meaning no next
                    // entry, meaning the free list is empty, meaning this slab is full. Overflow to a
                    // different slab in the same size class.

                    slabnum = (slabnum + SLAB_FAILOVER_NUM) & SLABNUM_BITS_ALONE_MASK;

                    if likely(slabnum != orig_slabnum) {
                        // We have not necessarily cycled through all slabs in this sizeclass yet,
                        // so keep trying, but make a note that at least one of the slabs in this
                        // sizeclass was full. (Note that if orig_slabnum is the only one that is
                        // full then we'll cycle through all of them *twice* before failing over to
                        // a bigger sizeclass. That's fine!)
                        a_slab_was_full = true;
                    } else {
                        // ... meaning we've tried each slab in this size class at least once and
                        // each one was either full or gave us an flh update collision (that we
                        // lost). If at least one slab in this size class was full, then overflow to
                        // the next larger size class. (Else, keep trying different slabs in this
                        // size class.)
                        if unlikely(a_slab_was_full) {
                            if unlikely(sc == NUM_SCS - 1) {
                                // This is the largest size class and we've exhausted at least one
                                // slab in it, plus we've tried all other slabs at least once, and
                                // each one was either full or we encountered (and lost) an flh
                                // collision while trying to pop from it.
                                eprintln!("smalloc exhausted");
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
        pub fn inner(&self) -> &SmallocInner {
            unsafe { &*self.inner.get() }
        }
    }

    // --- Fixed constants chosen for the design ---

    // NUM_SC_BITS is the main constant determining the rest of smalloc's layout. It is equal to 5
    // because that means there are 32 size classes, and the first one (that is used -- see below)
    // has 2^31 slots. This is the largest number of slots that we can put their slot numbers into a
    // 4-byte slot, which means that our smallest slots can be 4 bytes and we can pack more
    // allocations of 1, 2, 3, or 4 bytes into each cache line.
    pub const NUM_SC_BITS: u8 = 5;

    // NUM_SLABS_BITS is the other constant. There are 2^NUM_SLABS_BITS slabs in each size class.
    pub const NUM_SLABS_BITS: u8 = 6;

    // The first two size classes (which would hold 1-byte and 2-byte slots) are not used. In fact,
    // we re-use that unused space to hold flh's.
    pub const NUM_UNUSED_SCS: u8 = 2;

    // The size class to move "growers" to when they get reallocated to a size too large to pack
    // more than one of them into a single memory page:
    pub const GROWERS_SC: u8 = 22;


    // --- Constants determined by the constants above ---

    // See the ASCII-art map in `README.md` for where these bits fit into addresses.

    pub const NUM_SCS: u8 = 1 << NUM_SC_BITS; // 32

    pub const UNUSED_SC_MASK: usize = gen_mask!(NUM_UNUSED_SCS, usize); // 0b11

    // This is how many bits hold the data, the slotnum, and the next-touched-bit:
    pub const NUM_SN_D_T_BITS: u8 = NUM_UNUSED_SCS + NUM_SCS; // 34

    pub const SLABNUM_BITS_ALONE_MASK: u8 = gen_mask!(NUM_SLABS_BITS, u8); // 0b11111

    // This is how many bits to shift a slabnum to fit it into a slot/data address:
    pub const SLABNUM_ADDR_SHIFT_BITS: u8 = NUM_SN_D_T_BITS + NUM_SC_BITS; // 39

    // Mask of the bits of the slabnum in a slot's or data byte's address:
    pub const SLABNUM_BITS_ADDR_MASK: usize = (SLABNUM_BITS_ALONE_MASK as usize) << SLABNUM_ADDR_SHIFT_BITS; // 0b11111000000000000000000000000000000000000000

    // Mask of the bits of the sizeclass in a slot's address:
    pub const SC_BITS_ADDR_MASK: usize = gen_mask!(NUM_SC_BITS, usize) << NUM_SN_D_T_BITS; // 0b111110000000000000000000000000000000000

    // The following constants are just for calculating lowest and highest addresses which are used
    // for bounds checking, and also used to calculate the total virtual memory address space we
    // need to reserve.
    
    pub const NUM_SLOTS_IN_HIGHEST_SC: u64 = 1 << (NUM_UNUSED_SCS + 1); // 8
    pub const HIGHEST_SLOTNUM_IN_HIGHEST_SC: u64 = NUM_SLOTS_IN_HIGHEST_SC - 2; // 6; The extra -1 is because the last slot isn't used since its slotnum is the sentinel slotnum.

    pub const DATA_ADDR_BITS_IN_HIGHEST_SC: u8 = NUM_SCS - 1; // 31

    // The smalloc address of the slot with the lowest address is:
    pub const LOWEST_SMALLOC_SLOT_ADDR: usize = (NUM_UNUSED_SCS as usize) << NUM_SN_D_T_BITS; // 0b100000000000000000000000000000000000

    // The smalloc address of the slot with the highest address is:
    pub const HIGHEST_SMALLOC_SLOT_ADDR: usize = SLABNUM_BITS_ADDR_MASK | SC_BITS_ADDR_MASK | (HIGHEST_SLOTNUM_IN_HIGHEST_SC as usize) << DATA_ADDR_BITS_IN_HIGHEST_SC; // 0b11111111111100000000000000000000000000000000

    pub mod plat;
    use crate::*;
}

pub use i::*;


// ---- Constant having to do with slab failover ----

// Which next-slab do we try in case of update collision or slab exhaustion? It needs to be
// relatively prime to 64 so it will cycle through all slabs before returning to the first one, and
// it should be far from 1 since the next few threads will be using the next few slabs.
const SLAB_FAILOVER_NUM: u8 = (1u8 << NUM_SLABS_BITS) / 3; // 21

// ---- Constant having to do with slot (and free list) pointers ----

const SLOTNUM_AND_DATA_ADDR_MASK: u64 = gen_mask!(NUM_SN_D_T_BITS, u64); // 0b1111111111111111111111111111111111

// ---- Constant having to do with flh pointers ----

const FLHWORD_SIZE_BITS: u8 = 3; // 3 bits ie 8-byte sized flh words

// ---- Constants having to do with flh words and free list entries ----

const FLHWORD_PUSH_COUNTER_MASK: u64 = gen_mask!(32, u64) << 32;
const FLHWORD_PUSH_COUNTER_INCR: u64 = 1 << 32;

// This is how many bits hold the slotnum for the size class with the most slots (size class 2):
const NUM_SN_BITS: u8 = NUM_SCS - 1; // 31 // We reserve 1 bit to indicate next-touched

const ENTRY_NEXT_TOUCHED_BIT: u32 = 1 << 31;

const ENTRY_SLOTNUM_MASK: u32 = gen_mask!(NUM_SN_BITS, u32);

// ---- Constants for calculating the total virtual address space to reserve ----

// The smalloc address of the highest-addressed byte of a smalloc slot is:
const HIGHEST_SMALLOC_SLOT_BYTE_ADDR: usize = HIGHEST_SMALLOC_SLOT_ADDR | gen_mask!(DATA_ADDR_BITS_IN_HIGHEST_SC, usize); // 0b111111111111101111111111111111111111111111111

// We need to allocate extra bytes so that we can align the smalloc base pointer so that all of the
// trailing bits of the smalloc base pointer are zeros.

const BASEPTR_ALIGN: usize = (HIGHEST_SMALLOC_SLOT_BYTE_ADDR + 1).next_power_of_two(); // 0b1000000000000000000000000000000000000000000000
const TOTAL_VIRTUAL_MEMORY: usize = HIGHEST_SMALLOC_SLOT_BYTE_ADDR + BASEPTR_ALIGN - 1; // 0b1111111111111101111111111111111111111111111110 == 70_366_596_694_014


// --- Implementation ---

static GLOBAL_THREAD_NUM: AtomicU16 = AtomicU16::new(0);
const SLAB_NUM_SENTINEL: u16 = u16::MAX;
thread_local! {
    static SLABNUM: Cell<u16> = const { Cell::new(SLAB_NUM_SENTINEL) };
}

/// Get the slab number for this thread. On first call, initializes it from GLOBAL_THREAD_NUM.
#[inline(always)]
fn get_slabnum() -> u8 {
    SLABNUM.with(|cell| {
        let slabnum = cell.get();
        if slabnum == SLAB_NUM_SENTINEL {
            let newthreadnum = GLOBAL_THREAD_NUM.fetch_add(1, Relaxed);
            let newslabnum = newthreadnum & SLABNUM_BITS_ALONE_MASK as u16;
            cell.set(newslabnum);
            newslabnum as u8
        } else {
            slabnum as u8
        }
    })
}

unsafe impl Sync for Smalloc {}

impl Default for Smalloc {
    fn default() -> Self {
        Self::new()
    }
}

/// Return the size class for the aligned size.
#[inline(always)]
const fn reqali_to_sc(siz: usize, ali: usize) -> u8 {
    debug_assert!(siz > 0);
    debug_assert!(ali > 0);
    debug_assert!(ali < 1 << NUM_SCS);
    debug_assert!(ali.is_power_of_two());

    (((siz - 1) | (ali - 1) | UNUSED_SC_MASK).ilog2() + 1) as u8
}

#[cfg(test)]
mod tests;

use core::hint::{likely, unlikely};
use core::sync::atomic::{AtomicU16, AtomicU64, AtomicUsize, AtomicBool};
use core::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use core::cell::{Cell, UnsafeCell};
use core::alloc::{GlobalAlloc, Layout};
use core::ptr::{copy_nonoverlapping, null_mut};
use plat::p::sys_alloc;

#[cfg(any(target_os = "windows", doc))]
use plat::p::sys_commit;
