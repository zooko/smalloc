//! # smalloc
//!
//! A simple, fast memory allocator.
//!
//! ## Usage
//!
//! ```rust
//! use smalloc::Smalloc;
//! #[global_allocator]
//! static ALLOC: Smalloc = Smalloc::new();
//! ```

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

use core::sync::atomic::{AtomicU64, AtomicUsize, AtomicBool};
use core::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use core::cell::UnsafeCell;
use core::alloc::{GlobalAlloc, Layout};
use core::ptr::{copy_nonoverlapping, null_mut};
use plat::p::sys_alloc;

#[cfg(target_os = "windows")]
use plat::p::sys_commit;


// --- Public structs and methods ---

/// A simple, fast memory allocator.
///
/// # Example
///
/// ```rust
/// use smalloc::Smalloc;
/// #[global_allocator]
/// static ALLOC: Smalloc = Smalloc::new();
/// ```
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

        if sc >= NUM_SCS {
            // This request exceeds the size of our largest sizeclass, so return null pointer.
            null_mut()
        } else {
            let inner = self.inner();
            inner.idempotent_init();
            inner.alloc(sc, false)
        }
    }

    #[inline(always)]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        debug_assert!(layout.align().is_power_of_two());

        self.inner().dealloc(ptr.addr());
    }

    #[inline(always)]
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let reqsiz = layout.size();
        let reqalign = layout.align();
        debug_assert!(reqsiz > 0);
        debug_assert!(reqalign > 0);
        debug_assert!(reqalign.is_power_of_two());

        let sc = reqali_to_sc(reqsiz, reqalign);

        if sc >= NUM_SCS {
            // This request exceeds the size of our largest sizeclass, so return null pointer.
            null_mut()
        } else {
            let inner = self.inner();
            inner.idempotent_init();
            inner.alloc(sc, true)
        }
    }

    #[inline(always)]
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, reqsize: usize) -> *mut u8 {
        let inner = self.inner();
        let p_addr = ptr.addr();
        debug_assert!(inner.is_smalloc_ptr(p_addr));

        let oldsize = layout.size();
        debug_assert!(oldsize > 0);
        let oldalignment = layout.align();
        debug_assert!(oldalignment > 0);
        debug_assert!(oldalignment.is_power_of_two());
        debug_assert!(reqsize > 0);

        debug_assert!(reqali_to_sc(oldsize, oldalignment) >= NUM_UNUSED_SCS);
        debug_assert!(reqali_to_sc(oldsize, oldalignment) < NUM_SCS);

        let oldsc = ptr_to_sc(p_addr);

        debug_assert!(oldsc >= NUM_UNUSED_SCS);
        debug_assert!(oldsc < NUM_SCS);

        // It's possible that the slot `ptr` is currently in is larger than the slot size necessary
        // to hold the size that the user requested when originally allocating (or re-allocating)
        // `ptr`.
        debug_assert!(oldsc >= reqali_to_sc(oldsize, oldalignment));

        let reqsc = reqali_to_sc(reqsize, oldalignment);
        debug_assert!(reqsc >= NUM_UNUSED_SCS);

        // If the requested slot is <= the original slot, just return the pointer and we're done.
        if reqsc <= oldsc {
            return ptr;
        }

        if reqsc >= NUM_SCS {
            // This request exceeds the size of our largest sizeclass, so return null pointer.
            null_mut()
        } else {
            // The "Growers" strategy.
            let reqsc = if (plat::p::SC_FOR_PAGE..GROWERS_SC).contains(&reqsc) { GROWERS_SC } else { reqsc };

            let newp = inner.alloc(reqsc, false);
            if newp.is_null() {
                // smalloc slots must be exhausted
                return newp;
            }

            // Copy the contents from the old location.
            unsafe { copy_nonoverlapping(ptr, newp, oldsize); }

            // Free the old slot.
            inner.dealloc(p_addr);

            newp
        }
    }
}


// --- Private implementation code ---

// gen_mask macro for readability
macro_rules! gen_mask { ($bits:expr, $ty:ty) => { ((!0 as $ty) >> (<$ty>::BITS - ($bits) as u32)) }; }

#[doc(hidden)]
pub mod i {
    use crate::*;
    pub mod plat;

    // Everything in this `i` ("internal") module is for the use of the smalloc core lib (this file)
    // and for the use of the smalloc-ffi package.

    pub struct SmallocInner {
        pub smbp: AtomicUsize,
        pub initlock: AtomicBool
    }

    impl SmallocInner {
        /// Returns true if and only if this is a valid pointer to a smalloc slot.
        #[inline(always)]
        pub fn is_smalloc_ptr(&self, p_addr: usize) -> bool {
            let smbp = self.smbp.load(Relaxed);

            p_addr >= smbp + LOWEST_SMALLOC_SLOT_ADDR && p_addr <= smbp + HIGHEST_SMALLOC_SLOT_ADDR &&
                ptr_to_sc(p_addr) >= NUM_UNUSED_SCS &&
                p_addr.trailing_zeros() >= ptr_to_sc(p_addr) as u32 &&
                ptr_to_slotnum(p_addr) != sc_to_sentinel_slotnum(ptr_to_sc(p_addr)) &&
                p_addr & ADDR_NEXT_TOUCHED_BIT == 0
        }

        #[inline(always)]
        pub fn idempotent_init(&self) {
            let smbpval = self.smbp.load(Relaxed);
            if smbpval == 0 {
                // acquire the spin lock
                loop {
                    if self.initlock.compare_exchange_weak(false, true, Acquire, Relaxed).is_ok() {
                        break;
                    }
                }

                let smbpval = self.smbp.load(Relaxed);
                if smbpval == 0 {
                    let sysbp = sys_alloc(TOTAL_VIRTUAL_MEMORY).unwrap().addr();
                    assert!(sysbp != 0);
                    let smbp = sysbp.next_multiple_of(BASEPTR_ALIGN);

                    #[cfg(any(target_os = "windows", doc))]
                    {
                        let size_of_flh_area = (1usize << FLHWORD_SIZE_BITS) * (1usize << NUM_SLABS_BITS) * NUM_SCS as usize;
                        sys_commit(smbp as *mut u8, size_of_flh_area).unwrap();
                    }

                    self.smbp.store(smbp, Release);
                }

                // release the spin lock
                self.initlock.store(false, Release);
            }
        }

        #[inline(always)]
        /// zeromem says whether to ensure that the allocated memory is all zeroed out or not
        pub fn alloc(&self, orig_sc: u8, zeromem: bool) -> *mut u8 {
            debug_assert!(orig_sc >= NUM_UNUSED_SCS);
            debug_assert!(orig_sc < NUM_SCS);

            // If the slab is full, or if there is a collision when updating the flh, we'll switch to
            // another slab in this same sizeclass.
            let orig_slabnum = get_slabnum();
            let mut slabnum = orig_slabnum;
            let mut a_slab_was_full = false;

            // If all slabs in the sizeclass are full, we'll switch to the next sizeclass.
            let mut sc = orig_sc;

            let smbp = self.smbp.load(Acquire);

            loop {
                // The flhptr for this sizeclass and slabnum is at this location:
                let slabnum_and_sc = (slabnum as usize) << NUM_SC_BITS | sc as usize;
                let flhptr = smbp | slabnum_and_sc << FLHWORD_SIZE_BITS;

                // Load the value from the flh
                let flh = unsafe { AtomicU64::from_ptr(flhptr as *mut u64) };
                let flhword = flh.load(Acquire);

                // The low-order 4-byte word is the slotnum and touched-bit of the first entry.
                let curfirstentry = flhword as u32;
                let curfirstentryslotnum = curfirstentry & ENTRY_SLOTNUM_MASK;
                let curfirstentrynexttouchedbit = curfirstentry & ENTRY_NEXT_TOUCHED_BIT;

                // The sentinel slotnum for this sizeclass:
                let sentinel_slotnum = sc_to_sentinel_slotnum(sc);

                // The curfirstentry slotnum can be the sentinel slotnum, but not larger.
                debug_assert!(curfirstentryslotnum <= sentinel_slotnum);

                // If the curfirstentry next-slotnum is the sentinel slotnum, then the
                // next-has-been-touched bit must be false. (You can't ever touch—read or
                // write—the memory of the sentinel slot.)
                debug_assert!(if curfirstentryslotnum == sentinel_slotnum { curfirstentrynexttouchedbit == 0 } else { true });

                if curfirstentry != sentinel_slotnum {
                    // There is a slot available in the free list.

                    // Here's the pointer to the current first entry:
                    let curfirstentry_p = smbp | (slabnum_and_sc << NUM_SN_D_T_BITS) | (curfirstentryslotnum as usize) << sc;
                    debug_assert!(self.is_smalloc_ptr(curfirstentry_p), "curfirstentry_p: {curfirstentry_p}/{curfirstentry_p:b}");

                    let next_entry = if curfirstentrynexttouchedbit != 0 {
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
                            if curfirstentry_p.trailing_zeros() >= cbits as u32 {
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
                    if flh.compare_exchange_weak(flhword, newflhword, Acquire, Relaxed).is_ok() { 
                        debug_assert!(next_entry & ENTRY_SLOTNUM_MASK != curfirstentryslotnum);
                        debug_assert!(next_entry & ENTRY_SLOTNUM_MASK <= sentinel_slotnum);

                        // Okay we've successfully allocated a slot! If `zeromem` is requested and
                        // this slot has previously been touched then we have to zero its contents.
                        if zeromem && curfirstentrynexttouchedbit != 0 {
                            unsafe { core::ptr::write_bytes(curfirstentry_p as *mut u8, 0, 1 << sc) };
                        }

                        if slabnum != orig_slabnum {
                            // The slabnum changed. Save the new slabnum for next time.
                            set_slab_num(slabnum);
                        }

                        break curfirstentry_p as *mut u8;
                    } else {
                        // We encountered an update collision on the flh. Fail over to a different
                        // slab in the same size class.
                        slabnum = failover_slabnum(slabnum);
                    }
                } else {
                    // If we got here then curfirstentryslotnum == sentinelslotnum, meaning no next
                    // entry, meaning the free list is empty, meaning this slab is full. Overflow to a
                    // different slab in the same size class.

                    slabnum = failover_slabnum(slabnum);

                    if slabnum != orig_slabnum {
                        // We have not necessarily cycled through all slabs in this sizeclass yet,
                        // so keep trying, but make a note that at least one of the slabs in this
                        // sizeclass was full. (Note that if orig_slabnum is the only one that is
                        // full then we'll cycle through all of them *twice* before failing over to
                        // a bigger sizeclass. That's fine.)
                        a_slab_was_full = true;
                    } else {
                        // ... meaning we've tried each slab in this size class at least once and
                        // each one was either full or gave us an flh update collision (that we
                        // lost). If at least one slab in this size class was full, then overflow to
                        // the next larger size class. (Else, keep trying different slabs in this
                        // size class.)
                        if a_slab_was_full {
                            if sc == NUM_SCS - 1 {
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
        pub fn dealloc(&self, p_addr: usize) {
            debug_assert!(self.is_smalloc_ptr(p_addr));

            // Okay now we know that it is a pointer into smalloc's region.

            // The sizeclass is encoded into these bits of the address:
            let sc = ptr_to_sc(p_addr);
            debug_assert!(sc >= NUM_UNUSED_SCS);
            debug_assert!(sc < NUM_SCS);

            let flhptr = self.smbp.load(Relaxed) | ptr_to_flhaddr(p_addr);
            let flh = unsafe { AtomicU64::from_ptr(flhptr as *mut u64) };

            let newslotnum = ptr_to_slotnum(p_addr);
            let sentinel_slotnum = sc_to_sentinel_slotnum(sc);
            debug_assert!(newslotnum < sentinel_slotnum);

            loop {
                // Load the value (current first entry slotnum and next-entry-touched bit) from the
                // flh
                let flhword = flh.load(Relaxed);

                // The low-order 4-byte word is the slotnum and the touched-bit of the first entry
                let curfirstentry = flhword as u32;
                let curfirstentryslotnum = curfirstentry & ENTRY_SLOTNUM_MASK;

                debug_assert!(newslotnum != curfirstentryslotnum);
                // The curfirstentryslotnum can be the sentinel slotnum but not greater.
                debug_assert!(curfirstentryslotnum <= sentinel_slotnum);

                // Write it into the new slot's link
                unsafe { *(p_addr as *mut u32) = curfirstentry };

                // The high-order 4-byte word is the push counter. Increment it.
                let push_counter = (flhword & FLHWORD_PUSH_COUNTER_MASK).wrapping_add(FLHWORD_PUSH_COUNTER_INCR);

                // The new flh word is the push counter, the next-entry-touched-bit (set), and the
                // next-entry slotnum.
                let newflhword = push_counter | ENTRY_NEXT_TOUCHED_BIT as u64 | newslotnum as u64;

                // Compare and exchange
                if flh.compare_exchange_weak(flhword, newflhword, Release, Relaxed).is_ok() {
                    break;
                }
            }
        }
    }

    impl Smalloc {
        #[doc(hidden)]
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

    pub const TOTAL_VIRTUAL_MEMORY: usize = HIGHEST_SMALLOC_SLOT_BYTE_ADDR + BASEPTR_ALIGN - 1; // 0b1111111111111101111111111111111111111111111110 == 70_366_596_694_014

    /// Return the size class of the given pointer.
    #[inline(always)]
    pub fn ptr_to_sc(p_addr: usize) -> u8 {
        ((p_addr & SC_BITS_ADDR_MASK) >> NUM_SN_D_T_BITS) as u8
    }
}

pub use i::*;


// ---- Constant having to do with slab failover ----

const SLABNUM_ALONE_MASK: u8 = gen_mask!(NUM_SLABS_BITS, u8); // 0b111111

// ---- Constant having to do with slot (and free list) pointers ----

const SLOTNUM_AND_DATA_ADDR_MASK: u64 = gen_mask!(NUM_SN_D_T_BITS, u64); // 0b1111111111111111111111111111111111

// ---- Constant having to do with flh pointers ----

const FLHWORD_SIZE_BITS: u8 = 3; // 3 bits ie 8-byte sized flh words

// ---- Constants having to do with flh words and free list entries ----

const FLHWORD_PUSH_COUNTER_MASK: u64 = gen_mask!(32, u64) << 32;
const FLHWORD_PUSH_COUNTER_INCR: u64 = 1 << 32;

// This is how many bits hold the slotnum for the size class with the most slots (size class 2):
const NUM_SN_BITS: u8 = NUM_SCS - 1; // 31 // We reserve 1 bit to indicate next-touched

const ENTRY_NEXT_TOUCHED_BIT: u32 = 1 << NUM_SN_BITS;

const ADDR_NEXT_TOUCHED_BIT: usize = 1 << (NUM_SN_BITS + NUM_UNUSED_SCS);

const ENTRY_SLOTNUM_MASK: u32 = gen_mask!(NUM_SN_BITS, u32);

// ---- Constants for calculating the total virtual address space to reserve ----

// The smalloc address of the highest-addressed byte of a smalloc slot is:
const HIGHEST_SMALLOC_SLOT_BYTE_ADDR: usize = HIGHEST_SMALLOC_SLOT_ADDR | gen_mask!(DATA_ADDR_BITS_IN_HIGHEST_SC, usize); // 0b111111111111101111111111111111111111111111111

// We need to allocate extra bytes so that we can align the smalloc base pointer so that all of the
// trailing bits of the smalloc base pointer are zeros.

const BASEPTR_ALIGN: usize = (HIGHEST_SMALLOC_SLOT_BYTE_ADDR + 1).next_power_of_two(); // 0b1000000000000000000000000000000000000000000000

// ---- Constants for deciding when to unmap pages ---

// For correctness, this has to be >= plat:::p::SC_FOR_PAGE. Since we use the first 4 bytes of the
// slot for the next-entry-link it also has to be at least 2X that.
//
// For performance, there's no clear way to decide this. The cost of swapping is somewhere between
// 10_000 X and 1_000_000 X the cost of making a few syscalls, but on the other hand swapping may be
// very rare, be dependent on specific usage patterns and the behavior of other processes, and of
// the kernel, and it may not happen at all in a lot of common deployments. So... how do you make a
// fixed balance of that tradeoff? Oh, and worse, swapping could trigger swap thrash — drastically
// slowing down the entire system and probably leading to other failures in this or other processes.
//
// So I guess this isn't really a performance trade-off as much as "trying to heuristically avoid a
// worst-case-scenario at an acceptable performance cost in normal times".
//
// And then there's the question of memsetting memory to 0 ourselves vs having the kernel do it (using tricks like `rep stosb` with microcode optimizations, or by just mapping the zero page CoW, or viadc zva on ARM).
//xxx

use core::sync::atomic::AtomicU8;
use core::cell::Cell;

static GLOBAL_THREAD_NUM: AtomicU8 = AtomicU8::new(0);
const SLAB_NUM_SENTINEL: u8 = u8::MAX;
thread_local! {
    // "Slab And Step NUMbers" xxx
    static SAS_NUMS: Cell<u8> = const { Cell::new(SLAB_NUM_SENTINEL) };
}

/// Get the slab number for this thread. On first call, initializes xxx
#[inline(always)]
fn get_slabnum() -> u8 {

    SAS_NUMS.with(|cell| {
        let slabnum = cell.get();
        if slabnum == SLAB_NUM_SENTINEL {
            let threadnum = GLOBAL_THREAD_NUM.fetch_add(1, Relaxed);
            let slabnum = threadnum & SLABNUM_ALONE_MASK;
            cell.set(slabnum);
            slabnum
        } else {
            slabnum
        }
    })
}

#[inline(always)]
fn set_slab_num(slabnum: u8) {
    SAS_NUMS.with(|cell| {
        cell.set(slabnum);
    });
}

/// Pick a new slab to fail over to. This is used in two cases in `inner_alloc()`: a. when a slab is
/// full, and b. when there is a multithreading collision on the flh.
///
/// xxx made it simpler update docs xxx; Which new slabnumber shall we fail over to? A certain number, d, added to the current slab
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
fn failover_slabnum(slabnum: u8) -> u8 {
    const SLAB_FAILOVER_NUM: u8 = (1u8 << NUM_SLABS_BITS) / 3; // 21
    (slabnum.wrapping_add(SLAB_FAILOVER_NUM)) & SLABNUM_ALONE_MASK
}

#[doc(hidden)]
unsafe impl Sync for Smalloc {}

#[doc(hidden)]
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

/// Return the slotnum of the given pointer.
#[inline(always)]
fn ptr_to_slotnum(p_addr: usize) -> u32 {
    let sc = ptr_to_sc(p_addr);
    ((p_addr as u64 & SLOTNUM_AND_DATA_ADDR_MASK) >> sc) as u32
}

#[inline(always)]
/// Return the sentinel slotnum for this size class.
fn sc_to_sentinel_slotnum(sc: u8) -> u32 {
    gen_mask!(NUM_SN_BITS - (sc - NUM_UNUSED_SCS), u32)
}

#[inline(always)]
/// Return the address of the flh for this slab.
fn ptr_to_flhaddr(p_addr: usize) -> usize {
    // The flhptr for this sizeclass and slabnum is at this location, which we can calculate by
    // masking in the slabnum and sizeclass bits from the address and shifting them right:
    const SLABNUM_AND_SC_ADDR_MASK: usize = SLABNUM_BITS_ADDR_MASK | SC_BITS_ADDR_MASK;
    (p_addr & SLABNUM_AND_SC_ADDR_MASK) >> (NUM_SN_D_T_BITS - FLHWORD_SIZE_BITS)
}

#[cfg(test)]
mod tests;
