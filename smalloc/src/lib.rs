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
// * Public type aliases, structs and methods
// * Private implementation code 
//   + Fixed constants chosen for the design
//   + Constants determined by the constants above

use core::sync::atomic::{AtomicU64, AtomicUsize, AtomicBool};
use core::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use core::cell::UnsafeCell;
use core::alloc::{GlobalAlloc, Layout};
use core::ptr::{copy_nonoverlapping, null_mut};
use plat::p::sys_alloc;

#[cfg(target_os = "windows")]
use plat::p::sys_commit;


// --- Public type aliases, structs and methods ---

type SizeClass = u8;
type SlabNum = u8;
type SlotNum = u32;
type NextLinkEntry = u32; // includes the next-entry-touched bit
type Flh = u64;

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

/// Everything in this `i` ("internal") module is for the use of the shmalloc core lib (this file)
/// and for the use of the shmalloc-ffi-c-api and shmalloc-ffi-windows-heap-api packages.
#[doc(hidden)]
pub mod i {
    use crate::*;
    use crate::plat::p::sys_random_bytes;
    use crate::tagmac::Tag;
    pub mod plat;

    pub struct SmallocInner {
        pub smbp: AtomicUsize,
        pub initlock: AtomicBool
    }

    impl SmallocInner {
        /// Returns true if and only if this is a valid pointer to a smalloc slot.
        #[inline(always)]
        pub fn is_smalloc_ptr(&self, p_addr: usize) -> bool {
            let smbp = self.smbp.load(Relaxed);

            p_addr >= smbp + LOWEST_SHMALLOC_SLOT_ADDR && p_addr <= smbp + HIGHEST_SHMALLOC_SLOT_ADDR &&
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

                    let mut key = 0u128;
                    sys_random_bytes((&mut key as *mut u128).cast(), size_of::<u128>());

                    unsafe { ((smbp + TAGMAC_KEY_ADDR) as *mut tagmac::TagMACKey).write(key); }

                    self.smbp.store(smbp, Release);
                }

                // release the spin lock
                self.initlock.store(false, Release);
            }
        }

        #[inline(always)]
        /// zeromem says whether to ensure that the allocated memory is all zeroed out or not
        pub fn alloc(&self, orig_sc: SizeClass, zeromem: bool) -> *mut u8 {
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
                // next-has-been-touched bit must be false. (You can't ever touch—i.e. read or
                // write—the memory of the sentinel slot.)
                debug_assert!(if curfirstentryslotnum == sentinel_slotnum { curfirstentrynexttouchedbit == 0 } else { true });

                if curfirstentry != sentinel_slotnum {
                    // There is a slot available in the free list.

                    // Here's the pointer to the current first entry:
                    let curfirstentry_p = smbp | (slabnum_and_sc << NUM_SN_D_T_BITS) | (curfirstentryslotnum as usize) << sc;
                    debug_assert!(self.is_smalloc_ptr(curfirstentry_p), "curfirstentry_p: {curfirstentry_p}/{curfirstentry_p:b}");

                    if curfirstentrynexttouchedbit != 0 {
                        // Read the bits from the first entry's metadata area (which are about the
                        // second entry). These bits might be invalid, if the flh has changed since
                        // we read it above and another thread has used this metadata area
                        // (i.e. another linked list update). That's okay because in that case our
                        // attempt to update the flh below will fail (since the flh must have
                        // changed), so information derived from the invalidated bits will not get
                        // stored.

                        // The location of this slot's metadata is the final (highest-addressed) 16
                        // bytes of the slot, which we can compute by turning on all the bits of the
                        // address within the slot except for the least-significant 4 bits:
                        let slot_metadata_p = (curfirstentry_p | gen_mask!(sc, usize) & !15usize) as *const Tag;
            
                        // Read the tag from there.
                        let tag = unsafe { *slot_metadata_p };

                        let next_entry = (tag >> 96) as NextLinkEntry; // xxx symbolify
                        //xxxeprintln!("in alloc(), next_entry: 0b{next_entry:032b} read from {slot_metadata_p:p}");

                        // Put the new first entry (which is the old second entry) in place of
                        // the old first entry (which is going to be the return value) in our
                        // local copy of flhword, leaving the push-counter bits unchanged.
                        let newflhword = (flhword & FLHWORD_PUSH_COUNTER_MASK) | next_entry as u64;

                        // Compare and exchange
                        if flh.compare_exchange_weak(flhword, newflhword, Acquire, Relaxed).is_ok() { 
                            debug_assert!(next_entry & ENTRY_SLOTNUM_MASK != curfirstentryslotnum);
                            debug_assert!(next_entry & ENTRY_SLOTNUM_MASK <= sentinel_slotnum, "next_entry: {next_entry:b}, ENTRY_SLOTNUM_MASK: {ENTRY_SLOTNUM_MASK:b}, sentinel_slotnum: {sentinel_slotnum:b}, sc: {sc}");

                            // Check the current tag
                            assert!(tagmac::freed_tag_check(tagmac_key(smbp), slabnum, sc, curfirstentryslotnum, next_entry, tag));

                            // Okay we've successfully allocated a slot!

                            // If `zeromem` is requested and this slot has previously been touched
                            // then we have to zero its contents.
                            // xxx redo this with zero-on-free instead
                            if zeromem && curfirstentrynexttouchedbit != 0 {
                                unsafe { core::ptr::write_bytes(curfirstentry_p as *mut u8, 0, (1<<sc) - FREE_SLOT_METADATA_BYTES); }
                            }

                            // Write the new tag.
                            unsafe {
                                *(slot_metadata_p as *mut Tag) = tagmac::alloced_tag(tagmac_key(smbp), slabnum, sc, curfirstentryslotnum);
                            }

                            if slabnum != orig_slabnum {
                                // The slabnum changed. Save the new slabnum for next time.
                                set_slab_num(slabnum);
                            }

                            break curfirstentry_p as *mut u8;
                        }
                    } else {
                        // This entry has never been touched (read or written), so its next-entry
                        // link is equal to its slotnum + 1 (with the touched bit unset).

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

                        // Put the new first entry (which is the old second entry) in place of the
                        // old first entry (which is going to be the return value) in our local copy
                        // of flhword, leaving the push-counter bits unchanged.
                        let next_entry = curfirstentryslotnum + 1;
                        let newflhword = (flhword & FLHWORD_PUSH_COUNTER_MASK) | next_entry as u64;

                        if flh.compare_exchange_weak(flhword, newflhword, Acquire, Relaxed).is_ok() {
                            debug_assert!(next_entry & ENTRY_SLOTNUM_MASK != curfirstentryslotnum);
                            debug_assert!(next_entry & ENTRY_SLOTNUM_MASK <= sentinel_slotnum);
                            
                            
                            // Okay we've successfully allocated a slot!

                            let slot_size = 1usize << sc;
                            let slot_end = curfirstentry_p + slot_size;

                            // Write the new tag.
                            unsafe {
                                *((slot_end - FREE_SLOT_METADATA_BYTES) as *mut Tag) = tagmac::alloced_tag(tagmac_key(smbp), slabnum, sc, curfirstentryslotnum);
                            }

                            if slabnum != orig_slabnum {
                                // The slabnum changed. Save the new slabnum for next time.
                                set_slab_num(slabnum);
                            }

                            break curfirstentry_p as *mut u8;
                        }
                    };

                    // Since neither of the breaks in the if-else above triggered, this means we
                    // encountered an update collision on the flh. Fail over to a different slab in
                    // the same size class.
                    slabnum = failover_slabnum(slabnum);
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

            let smbp = self.smbp.load(Relaxed);
            let slabnum = ptr_to_slabnum(p_addr);

            // The location of this slot's metadata is the final (highest-addressed) 16 bytes of the
            // slot, which we can compute by turning on all the bits of the address within the slot
            // except for the least-significant 4 bits:
            let slot_metadata_p = p_addr | gen_mask!(sc, usize) & !15usize;
            
            // Read the tag from there.
            let tag = unsafe { *(slot_metadata_p as *const Tag) };

            // Check that the tag is correct (for this slot when it is allocated).
            assert!(tagmac::alloced_tag_check(tagmac_key(smbp), slabnum, sc, ptr_to_slotnum(p_addr), tag));

            let flhptr = smbp | ptr_to_flhaddr(p_addr);
            let flh = unsafe { AtomicU64::from_ptr(flhptr as *mut u64) };

            let newslotnum = ptr_to_slotnum(p_addr);
            let sentinel_slotnum = sc_to_sentinel_slotnum(sc);
            debug_assert!(newslotnum < sentinel_slotnum);

            loop {
                // Load the value (current first entry slotnum and next-entry-touched bit) from the
                // flh.
                let flhword = flh.load(Relaxed);

                // The low-order 4-byte word is the slotnum and the touched-bit of the first entry
                let curfirstentry = flhword as NextLinkEntry;
                let curfirstentryslotnum = curfirstentry & ENTRY_SLOTNUM_MASK;

                debug_assert!(newslotnum != curfirstentryslotnum);
                // The curfirstentryslotnum can be the sentinel slotnum but not greater.
                debug_assert!(curfirstentryslotnum <= sentinel_slotnum);

                unsafe {
                    // xxx replace with STZG on MTE
                    //xxxeprintln!("in dealloc(), next_entry: 0b{curfirstentryslotnum:032b}, tag: 0b{:0128b}, written to {:p}", tagmac::freed_tag(tagmac_key(smbp), slabnum, sc, newslotnum, curfirstentry), slot_metadata_p as *const u8);
                    *(slot_metadata_p as *mut Tag) = tagmac::freed_tag(tagmac_key(smbp), slabnum, sc, newslotnum, curfirstentry);
                }

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

    // We can consistently allocate at least this many bytes from the operating system (see
    // find_max_vm_addresses_reservable.rs for details).
    pub const ALLOCATABLE: usize = 93_000_000_000_000;

    // The smallest slots (which also have to contain the 16-byte metadata) hold 2^5 bytes, i.e. 32
    // bytes.
    pub const SMALLEST_SLOT_SIZE_BITS: u8 = 5;

    // The size class to move "growers" to when they get reallocated to a size too large to pack
    // more than one of them into a single memory page:
    pub const GROWERS_SC: SizeClass = 22;


    // --- Constants determined by the constants above ---

    // See the ASCII-art map in `README.md` for where these bits fit into addresses.

    /// How many bits do we need to encode all numbers [0..n)?
    const fn bits_needed(n: u32) -> u8 {
        assert!(n > 0);
        (u32::BITS - (n - 1).leading_zeros()) as u8
    }

    // NUM_SC_BITS is the constant which mostly determines the rest of shmalloc's layout. It is
    // equal to 5 because that means there are 32 size classes, and the first one (that is used) has
    // 2^31 slots. This means we can fit two slotnums (plus each of their next-is-touched bit) into
    // a 64-bit word so we can do atomic operations on them without having to reach for 128-bit
    // atomics.
    pub const NUM_SC_BITS: u8 = bits_needed(usize::BITS / 2); // 5

    pub const NUM_SCS: SizeClass = 1 << NUM_SC_BITS; // 32

    // This is how many bits hold the data, and the slotnum:
    pub const NUM_SN_D_BITS: u8 = SMALLEST_SLOT_SIZE_BITS + NUM_SN_BITS; // 37

    // This is how many bits hold the data, the slotnum, and the next-touched-bit:
    pub const NUM_SN_D_T_BITS: u8 = NUM_SN_D_BITS + 1; // 38

    /// How many bits of an address can we freely use (each bit can be 0 or 1 as we need) if we have
    /// n addresses reserved?
    const fn bits_holdable(n: usize) -> u8 {
        assert!(n > 0);
        (usize::BITS - (n - 1).leading_zeros() - 1) as u8
    }
    // The - 1 is because we have to allocate up to twice as much space so that we can align the
    // shmalloc region to have all of its trailing bits 0 so that we can do nice bitwise arithmetic
    // on shmalloc pointers.
    pub const SHMALLOC_REGION_BITS: u8 = bits_holdable(ALLOCATABLE) - 1;

    // There are 2^NUM_SLABS_BITS slabs in each size class. Here we calculate the the largest number
    // of slabs we can accomodate within the limits of the virtual memory address space, given the
    // 31 bits of slotnums chosen above by NUM_SC_BITS, the 32-byte smallest slot size chosen above
    // by SMALLEST_SLOT_SIZE_BITS, the 1 bit for next-is-touched, and the 5 bits for sizeclass.
    pub const NUM_SLABS_BITS: u8 = SHMALLOC_REGION_BITS - NUM_SN_D_T_BITS - NUM_SC_BITS; // 3 xxx is this actually 3? Some AI or me or some other human: double-check this

    pub const SMALLEST_SLOT_SIZE_BITS_MASK: usize = gen_mask!(SMALLEST_SLOT_SIZE_BITS, usize); // 0b11111

    // SMALLEST_SLOT_SIZE_BITS is also the number of size classes not used — size classes [0–4] are
    // not used. (The space for size class 0 is re-used for the free list pointers.)
    pub const NUM_UNUSED_SCS: u8 = SMALLEST_SLOT_SIZE_BITS;

    // This is how many bits to shift a slabnum to fit it into a slot/data address:
    pub const SLABNUM_ADDR_SHIFT_BITS: u8 = NUM_SN_D_T_BITS + NUM_SC_BITS; // 41

    // Mask of the bits of the slabnum in a slot's or data byte's address:
    pub const SLABNUM_BITS_ADDR_MASK: usize = (SLABNUM_ALONE_MASK as usize) << SLABNUM_ADDR_SHIFT_BITS; // 0b111100000000000000000000000000000000000000000

    // Mask of the bits of the sizeclass in a slot's address:
    pub const SC_BITS_ADDR_MASK: usize = gen_mask!(NUM_SC_BITS, usize) << NUM_SN_D_T_BITS; // 0b11111000000000000000000000000000000000000

    // The following constants are just for calculating lowest and highest addresses which are used
    // for bounds checking. The highest address is also used to calculate the total virtual memory
    // address space we need to reserve.

    pub const NUM_SLOTS_IN_HIGHEST_SC: u64 = 1 << SMALLEST_SLOT_SIZE_BITS; // 32
    pub const HIGHEST_SLOTNUM_IN_HIGHEST_SC: u64 = NUM_SLOTS_IN_HIGHEST_SC - 2; // 30; The extra -1 is because the last slot isn't used since its slotnum is the sentinel slotnum.

    pub const DATA_ADDR_BITS_IN_HIGHEST_SC: u8 = NUM_SCS - 1; // 31

    // The smalloc address of the slot with the lowest address is:
    pub const LOWEST_SHMALLOC_SLOT_ADDR: usize = (NUM_UNUSED_SCS as usize) << NUM_SN_D_T_BITS; // 0b100000000000000000000000000000000000 // xxx update this comment to reflect the value generated by these const exprs

    // The smalloc address of the slot with the highest address is:
    pub const HIGHEST_SHMALLOC_SLOT_ADDR: usize = SLABNUM_BITS_ADDR_MASK | SC_BITS_ADDR_MASK | (HIGHEST_SLOTNUM_IN_HIGHEST_SC as usize) << DATA_ADDR_BITS_IN_HIGHEST_SC; // 0b111111011111100000000000000000000000000000000

    pub const TOTAL_VIRTUAL_MEMORY: usize = HIGHEST_SHMALLOC_BYTE_ADDR + 1 + EXTRA_ALLOC_FOR_ALIGN;

    /// Return the size class of the given pointer.
    #[inline(always)]
    pub fn ptr_to_sc(p_addr: usize) -> SizeClass {
        ((p_addr & SC_BITS_ADDR_MASK) >> NUM_SN_D_T_BITS) as SizeClass
    }
}

pub use i::*;


// ---- Constant having to do with slab failover ----

const SLABNUM_ALONE_MASK: u8 = gen_mask!(NUM_SLABS_BITS, u8); // 0b111

// ---- Constant having to do with slot (and free list) pointers ----

// Mask of the slotnum and data bits
const SN_D_ADDR_MASK: u64 = gen_mask!(NUM_SN_D_BITS, u64); // 0b111111111111111111111111111111111

// ---- Constant having to do with flh pointers ----

const FLHWORD_SIZE_BITS: u8 = 3; // 3 bits ie 8-byte sized flh words
const PUSH_COUNTER_BITS: u8 = Flh::BITS as u8 - NUM_SCS; // 32 // bits in the push counter

// ---- Constants having to do with flh words and free list entries ----

const FLHWORD_PUSH_COUNTER_MASK: Flh = gen_mask!(PUSH_COUNTER_BITS, Flh) << NUM_SCS;
const FLHWORD_PUSH_COUNTER_INCR: Flh = 1 << NUM_SCS;

// How many bits to encode a next-link entry and next-is-touched bit?
const NUM_LINK_BITS: u8 = NUM_SCS;

// This is how many bits hold the slotnum for the size class with the most slots:
const NUM_SN_BITS: u8 = NUM_LINK_BITS - 1; // 31 // We reserve 1 bit to indicate next-touched

const ENTRY_NEXT_TOUCHED_BIT: NextLinkEntry = 1 << NUM_SN_BITS;

const ADDR_NEXT_TOUCHED_BIT: usize = 1 << (NUM_SN_BITS + NUM_UNUSED_SCS);

// mask of the slotnum bits (but not the next-is-touched bit) in an entry
const ENTRY_SLOTNUM_MASK: NextLinkEntry = gen_mask!(NUM_SN_BITS, NextLinkEntry);

// ---- Constants having to do with the MAC tags

// The 32-bit word of all 1-bits can never be a valid next-entry-link, so we can use it as a
// sentinel to mean that this is not a next-entry-link.
const ENTRY_SENTINEL: NextLinkEntry = gen_mask!(NUM_SN_BITS + 1, NextLinkEntry);

// Bytes for the Message-Authentication-Code tag.
const FREE_SLOT_METADATA_BYTES: usize = 16; // 16


// ---- Constants for calculating the total virtual address space to reserve ----

// The smalloc address of the highest-addressed byte of a smalloc slot is:
const HIGHEST_SHMALLOC_BYTE_ADDR: usize = HIGHEST_SHMALLOC_SLOT_ADDR | gen_mask!(DATA_ADDR_BITS_IN_HIGHEST_SC, usize); // 0b111111011111101111111111111111111111111111111

// We need to allocate this extra bytes so that we can align the shmalloc base pointer so that all
// of the trailing bits of the shmalloc base pointer are zeros.
const BASEPTR_ALIGN: usize = HIGHEST_SHMALLOC_BYTE_ADDR.next_power_of_two(); // 0b10000000000000000000000000000000000000000000000
const MIN_PAGE_SIZE: usize = 1 << 12; // 4096 Pointers returned by system alloc will be aligned to at least this.
const EXTRA_ALLOC_FOR_ALIGN: usize = BASEPTR_ALIGN - MIN_PAGE_SIZE; // 0b1111111111111111111111111111111111000000000000

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
const SLAB_NUM_SENTINEL: SlabNum = SlabNum::MAX;
thread_local! {
    // "Slab And Step NUMbers" xxx
    static SAS_NUMS: Cell<SlabNum> = const { Cell::new(SLAB_NUM_SENTINEL) };
}

/// Get the slab number for this thread. On first call, initializes xxx
#[inline(always)]
fn get_slabnum() -> SlabNum {
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
fn set_slab_num(slabnum: SlabNum) {
    debug_assert!((slabnum & !SLABNUM_ALONE_MASK) == 0);
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
fn failover_slabnum(slabnum: SlabNum) -> SlabNum {
    const SLAB_FAILOVER_NUM: SlabNum = (1u8 << NUM_SLABS_BITS) / 3; // 21
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
const fn reqali_to_sc(siz: usize, ali: usize) -> SizeClass {
    debug_assert!(siz > 0);
    debug_assert!(ali > 0);
    debug_assert!(ali < 1 << NUM_SCS);
    debug_assert!(ali.is_power_of_two());

    // 16 bytes extra space for the metadata (next-link entry and tag)
    (((siz + FREE_SLOT_METADATA_BYTES - 1) | (ali - 1) | SMALLEST_SLOT_SIZE_BITS_MASK).ilog2() + 1) as SizeClass
}

/// Return the slotnum of the given pointer.
#[inline(always)]
fn ptr_to_slotnum(p_addr: usize) -> SlotNum {
    let sc = ptr_to_sc(p_addr);
    ((p_addr as u64 & SN_D_ADDR_MASK) >> sc) as SlotNum
}

#[inline(always)]
/// Return the sentinel slotnum for this size class.
fn sc_to_sentinel_slotnum(sc: SizeClass) -> SlotNum {
    gen_mask!(NUM_SN_BITS - (sc - NUM_UNUSED_SCS), SlotNum)
}

#[inline(always)]
/// Return the address of the flh for this slab.
fn ptr_to_flhaddr(p_addr: usize) -> usize {
    // The flhptr for this sizeclass and slabnum is at this location, which we can calculate by
    // masking in the slabnum and sizeclass bits from the address and shifting them right:
    const SLABNUM_AND_SC_ADDR_MASK: usize = SLABNUM_BITS_ADDR_MASK | SC_BITS_ADDR_MASK;
    (p_addr & SLABNUM_AND_SC_ADDR_MASK) >> (NUM_SN_D_T_BITS - FLHWORD_SIZE_BITS)
}

#[inline(always)]
fn ptr_to_slabnum(p_addr: usize) -> SlabNum {
    ((p_addr & SLABNUM_BITS_ADDR_MASK) >> SLABNUM_ADDR_SHIFT_BITS) as SlabNum
}

use core::mem::align_of;
const TAGMAC_KEY_ADDR: usize = 0b0000_0000_0000_0000_0000_0000_0000_1000_0000_0000;
const _: () = assert!(TAGMAC_KEY_ADDR.is_multiple_of(align_of::<tagmac::TagMACKey>()));
#[inline(always)]
fn tagmac_key(smbp: usize) -> tagmac::TagMACKey {
    unsafe { ((smbp | TAGMAC_KEY_ADDR) as *const tagmac::TagMACKey).read() }
}

mod tagmac {
    //! This is a tiny SipHash-0-3-like MAC specialized for shmalloc's two tag shapes. There is no
    //! notion of a variable-length "message" — instead each of the two shapes take fixed-arity,
    //! fixed-size inputs, and we simply XOR the inputs into the key.
    //!
    //! - allocated-slot tag: `NEXT_LINK_SENTINEL + 0b*32 + ShmipHash(key ⊕ (slabnum, sc, slotnum, NEXT_LINK_SENTINEL))`
    //! - freed-slot tag: `next_link + 0b*32 + ShmipHash(key ⊕ (slabnum, sc, slotnum, next_link))`
    //!
    //! This is intentionally not general-purpose SipHash:
    //!
    //! - it doesn't take messages, only the key — no compression round(s) at all (just XOR)
    //! - domain separation (allocated vs free) is done by a sentinel value
    //! - no SipHash length-finalization word is used;
    //! - copy the 32b next_link/NEXT_LINK_SENTINEL into the top 32b of the tag

    use super::{SizeClass, SlabNum, SlotNum, SLABNUM_ALONE_MASK, NUM_UNUSED_SCS, NUM_SCS, sc_to_sentinel_slotnum, NextLinkEntry, ENTRY_SENTINEL, ENTRY_SLOTNUM_MASK, ENTRY_NEXT_TOUCHED_BIT};

    pub(super) type TagMACKey = u128;

    /// 128-bit metadata authentication tag (top 32 bits are next_link)
    pub(super) type Tag = u128;

    macro_rules! sipround {
        ($v0:ident, $v1:ident, $v2:ident, $v3:ident) => {{
            $v0 = $v0.wrapping_add($v1);
            $v1 = $v1.rotate_left(13);
            $v1 ^= $v0;
            $v0 = $v0.rotate_left(32);

            $v2 = $v2.wrapping_add($v3);
            $v3 = $v3.rotate_left(16);
            $v3 ^= $v2;

            $v0 = $v0.wrapping_add($v3);
            $v3 = $v3.rotate_left(21);
            $v3 ^= $v0;

            $v2 = $v2.wrapping_add($v1);
            $v1 = $v1.rotate_left(17);
            $v1 ^= $v2;
            $v2 = $v2.rotate_left(32);
        }};
    }

    #[inline(always)]
    /// If this is an allocated-slot MAC, then `nextlink` is ENTRY_SENTINEL. If this is a freed-slot
    /// MAC, then `nextlink` is the nextlink (which can never be ENTRY_SENTINEL).
    fn mac(key: TagMACKey, slabnum: SlabNum, sc: SizeClass, slotnum: SlotNum, nextlink: NextLinkEntry) -> Tag {
        debug_assert!(slabnum <= SLABNUM_ALONE_MASK);
        debug_assert!(sc >= NUM_UNUSED_SCS);
        debug_assert!(sc < NUM_SCS);
        debug_assert!(slotnum < sc_to_sentinel_slotnum(sc));
        debug_assert!(
            if nextlink != ENTRY_SENTINEL {
                let next_slotnum = nextlink & ENTRY_SLOTNUM_MASK;
                let next_touched = nextlink & ENTRY_NEXT_TOUCHED_BIT;
                let sentinel_slotnum = sc_to_sentinel_slotnum(sc);

                if next_touched != 0 {
                    next_slotnum < sentinel_slotnum
                } else {
                    next_slotnum <= sentinel_slotnum
                }
            } else {
                true
            }
        );

        // “ShmipHash-0-3”
        let mut k0 = (key >> 64) as u64;
        let mut k1 = key as u64;

        k0 ^= (slabnum as u64) << 32;
        k0 ^= sc as u64;
        k1 ^= (slotnum as u64) << 32;
        k1 ^= nextlink as u64;

        let mut v0 = k0 ^ 0x736f_6d65_7073_6575;
        let mut v1 = k1 ^ 0x646f_7261_6e64_6f6d;
        let mut v2 = k0 ^ 0x6c79_6765_6e65_7261;
        let mut v3 = k1 ^ 0x7465_6462_7974_6573;

        sipround!(v0, v1, v2, v3);
        sipround!(v0, v1, v2, v3);
        sipround!(v0, v1, v2, v3);

        (nextlink as Tag) << 96 | (v0 ^ v1 ^ v2 ^ v3) as Tag // symbolify the 96
    }

    /// Return the tag for a currently allocated slot.
    #[inline(always)]
    pub(super) fn alloced_tag(
        key: TagMACKey,
        slabnum: SlabNum,
        sc: SizeClass,
        slotnum: SlotNum,
    ) -> Tag {
        mac(key, slabnum, sc, slotnum, ENTRY_SENTINEL)
    }

    /// Return the tag for a freed slot containing `next_link`.
    #[inline(always)]
    pub(super) fn freed_tag(
        key: TagMACKey,
        slabnum: SlabNum,
        sc: SizeClass,
        slotnum: SlotNum,
        next_link: NextLinkEntry,
    ) -> Tag {
        mac(key, slabnum, sc, slotnum, next_link)
    }

    /// Check the tag for a currently allocated slot.
    #[inline(always)]
    pub(super) fn alloced_tag_check(
        key: TagMACKey,
        slabnum: SlabNum,
        sc: SizeClass,
        slotnum: SlotNum,
        tag: Tag,
    ) -> bool {
        alloced_tag(key, slabnum, sc, slotnum) == tag
    }

    /// Check the tag for a freed slot containing `next_link`.
    #[inline(always)]
    pub(super) fn freed_tag_check(
        key: TagMACKey,
        slabnum: SlabNum,
        sc: SizeClass,
        slotnum: SlotNum,
        next_link: NextLinkEntry,
        tag: Tag,
    ) -> bool {
        //xxxif freed_tag(key, slabnum, sc, slotnum, next_link) != tag {
        //xxx    eprintln!("key: 0b{key:0128b}, slabnum: {slabnum}, slotnum: {slotnum}, next_link: {next_link}, tag: 0x{tag:x}, computed-tag: 0x{:x}", freed_tag(key, slabnum, sc, slotnum, next_link));
        //xxx}
        freed_tag(key, slabnum, sc, slotnum, next_link) == tag
    }
}

#[cfg(test)]
mod tests;
