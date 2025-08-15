#![doc = include_str!("../README.md")]

#![allow(clippy::missing_safety_doc)]

#![feature(pointer_is_aligned_to)]
#![feature(unchecked_shifts)]
#![feature(test)]


// Layout of this file:
// * Fixed constants chosen for the design (see README.md)
// * Constant values computed at compile time from the fixed constants
// * Implementation code
// * Code for development (e.g benchmarks, tests, utility functions, development tools)


// --- Fixed constants chosen for the design ---

const NUM_SMALL_SCS: u8 = 5; // xxx newtype SC? // "SC": "size class
const NUM_MEDIUM_SCS: u8 = 5;
const NUM_LARGE_SCS: u8 = 26;

const SMALLEST_SLOT_SIZE_BITS: u8 = 2;
const LARGE_SLOT_SIZE_BITS_PLUS_NUM_SLOTS_BITS: u8 = 38;

const NUM_SMALL_SLABS_BITS: u8 = 8;

const NUM_SMALL_SLOTS_BITS: u8 = 29;
const NUM_MEDIUM_SLOTS_BITS: u8 = 27;


// --- Fixed constants for supported machine architecture

// The per-slab flhs have this size in bytes.
const DOUBLEWORDSIZE: usize = 8;

// The free list entries have this size in bytes.
const SINGLEWORDSIZE: usize = 4;


// --- Constant values determined by the constants above ---

// xxx symbolify these?
const SMALL_SLABNUM_MASK: u32 = const_gen_mask_u32(NUM_SMALL_SLABS_BITS); // 0b11111111
const LARGE_SLAB_SC_MASK: usize = const_shl_u8_usize(NUM_LARGE_SCS.next_power_of_two() - 1, LARGE_SLOT_SIZE_BITS_PLUS_NUM_SLOTS_BITS); // 0b0b1111100000000000000000000000000000000000000

const NUM_SMALL_SLOTS: u32 = const_one_shl_u32(NUM_SMALL_SLOTS_BITS);
const NUM_MEDIUM_SLOTS: u32 = const_one_shl_u32(NUM_MEDIUM_SLOTS_BITS);

const HIGHEST_SMALL_SLOTNUM: u32 = NUM_SMALL_SLOTS - 1;
const HIGHEST_MEDIUM_SLOTNUM: u32 = NUM_MEDIUM_SLOTS - 1;

const SIZECLASS_0_SC_INDICATOR_MASK: usize =  0b1000000000000000000000000000000000000000;
const SIZECLASS_0_SLOTNUM_MASK: usize = const_shl_u32_usize(HIGHEST_SMALL_SLOTNUM, SMALLEST_SLOT_SIZE_BITS); // 0b01111111111111111111111111111100;
const SIZECLASS_0_SLOTNUM_LSB_MASK: usize = const_one_shl_usize(SMALLEST_SLOT_SIZE_BITS); // 0b100;

const SIZECLASS_5_SC_INDICATOR_MASK: usize = 0b10000000000000000000000000000000000;
const SIZECLASS_5_SLOTNUM_MASK: usize = const_shl_u32_usize(HIGHEST_MEDIUM_SLOTNUM, 5 + SMALLEST_SLOT_SIZE_BITS); // 0b1111111111111111111111111110000000

const LARGE_SC_INDICATOR_MASK: usize = 0b100000000000000000000000000000000000000000000;

// The address of the slot with the highest address is:
const HIGHEST_SMALLOC_SLOT_ADDR: usize = LARGE_SC_INDICATOR_MASK | const_shl_u8_usize(NUM_LARGE_SCS - 1, LARGE_SLOT_SIZE_BITS_PLUS_NUM_SLOTS_BITS); // 0b101100100000000000000000000000000000000000000

// The address of the byte of the slot with the highest address is:
// const HIGHEST_SMALLOC_ADDR: usize = HIGHEST_SMALLOC_SLOT_ADDR | const_gen_mask_usize(LARGE_SLOT_SIZE_BITS_PLUS_NUM_SLOTS_BITS - 1);

// But currently we also have an flh in there:
const HIGHEST_SMALLOC_ADDR: usize = (HIGHEST_SMALLOC_SLOT_ADDR | const_one_shl_usize(LARGE_SLOT_SIZE_BITS_PLUS_NUM_SLOTS_BITS - 1)) + DOUBLEWORDSIZE - 1; // 0b101100110000000000000000000000000000000000111

const SMALLOC_ADDRESS_BITS_MASK: usize = HIGHEST_SMALLOC_ADDR.next_power_of_two() - 1; // 0b111111111111111111111111111111111111111111111 

const UNUSED_MSB_ADDRESS_BITS: u8 = help_leading_zeros_usize(SMALLOC_ADDRESS_BITS_MASK);

// We need to allocate an extra 2^45 - 1 bytes so that we can align the smalloc base pointer to have
// 45 trailing zeros.
const BASEPTR_ALIGN: usize = const_one_shl_usize(45);
const TOTAL_VIRTUAL_MEMORY: usize = HIGHEST_SMALLOC_ADDR + BASEPTR_ALIGN - 1;


// --- Implementation ---

static GLOBAL_THREAD_NUM: AtomicU32 = AtomicU32::new(0);

thread_local! {
    static THREAD_NUM: Cell<Option<u32>> = const { Cell::new(None) };
}

/// Get this thread's unique, incrementing number.
// It is okay if more than 4 billion threads are spawned and this wraps, since the only thing we
// currently use it for is to & it with SMALL_SLABNUM_MASK anyway.
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

pub struct Smalloc {
    initlock: AtomicBool,
    sys_baseptr: AtomicUsize,
    sm_baseptr: AtomicUsize
}

impl Default for Smalloc {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Smalloc {
    fn drop(&mut self) {
        let layout = unsafe { Layout::from_size_align_unchecked(TOTAL_VIRTUAL_MEMORY, PAGE_SIZE) };

        sys_dealloc(self.sys_baseptr.load(Acquire) as *mut u8, layout);//xxx can we use weaker ordering constraints?
    }
}

impl Smalloc {
    pub const fn new() -> Self {
        Self {
            initlock: AtomicBool::new(false),
            sys_baseptr: AtomicUsize::new(0),
            sm_baseptr: AtomicUsize::new(0)
        }
    }

    fn idempotent_init(&self) -> Result<usize, AllocFailed> {
        let mut p: usize;

        p = self.sm_baseptr.load(Acquire);
        if p != 0 {
            return Ok(p);
        }

        //eprintln!("TOTAL_VIRTUAL_MEMORY: {TOTAL_VIRTUAL_MEMORY}");

        let layout = unsafe { Layout::from_size_align_unchecked(TOTAL_VIRTUAL_MEMORY, PAGE_SIZE) };

        // acquire spin lock
        loop {
            if self.initlock.compare_exchange_weak(false, true, AcqRel, Acquire).is_ok() {
                break;
            }
        }

        let mut smbp: usize = 0;

        p = self.sm_baseptr.load(Acquire);
        if p == 0 {
            let sysbp = sys_alloc(layout)?;
            assert!(!sysbp.is_null());
            self.sys_baseptr.store(sysbp.addr(), Release);//xxx can we use weaker ordering constraints?
            smbp = sysbp.addr().next_multiple_of(BASEPTR_ALIGN);
            debug_assert!(smbp + HIGHEST_SMALLOC_ADDR <= sysbp.addr() + layout.size(), "sysbp: {sysbp:?}, smbp: {smbp:?}, H slot: {HIGHEST_SMALLOC_SLOT_ADDR:?}, H: {HIGHEST_SMALLOC_ADDR:?}, smbp+H: {:?}, size: {:?}, BASEPTR_ALIGN: {BASEPTR_ALIGN:?}", smbp + HIGHEST_SMALLOC_ADDR, layout.size());
            self.sm_baseptr.store(smbp, Release); //xxx can we use weaker ordering constraints?
        }

        // Release the spin lock
        self.initlock.store(false, Release);

        debug_assert!(smbp != 0);
        Ok(smbp)
    }

    fn get_sm_baseptr(&self) -> usize {
        let p = self.sm_baseptr.load(Acquire);
        debug_assert!(p != 0);

        p
    }

    fn push_slot_onto_freelist(&self, slabbp: usize, flh_addr: usize, newslotnum: u32, numslotsbits: u8, slotsizebits: u8) {
        debug_assert!(help_trailing_zeros_usize(slabbp) >= slotsizebits);
        debug_assert!(flh_addr % DOUBLEWORDSIZE == 0, "{flh_addr}");
        debug_assert!(newslotnum < const_one_shl_u32(numslotsbits));
        debug_assert!(numslotsbits <= NUM_SMALL_SLOTS_BITS); // the most slots
        debug_assert!(slotsizebits < LARGE_SLOT_SIZE_BITS_PLUS_NUM_SLOTS_BITS); // the biggest slot

        // xxx use smbp or sysbp and with_metadata_of() and/or with_addr() ?
        let flha = unsafe { AtomicU64::from_ptr(flh_addr as *mut u64) };

        loop {
            // Load the value (current first entry slot num) from the flh
            let flhdword: u64 = flha.load(Acquire);
            let curfirstentryslotnum: u32 = (flhdword & u32::MAX as u64) as u32;
            debug_assert!(curfirstentryslotnum < const_one_shl_u32(numslotsbits));

            let counter: u32 = (flhdword >> 32) as u32;

            // Encode it as the next-entry link for the new entry
            // xxx lookup table instead of gen_mask?
            let next_entry_link = Self::encode_next_entry_link(newslotnum, curfirstentryslotnum, numslotsbits);
            debug_assert_eq!(curfirstentryslotnum, Self::decode_next_entry_link(newslotnum, next_entry_link, numslotsbits), "newslotnum: {newslotnum}, next_entry_link: {next_entry_link}, curfirstentryslotnum: {curfirstentryslotnum}, numslotsbits: {numslotsbits}, const_ge_mask_u32(numslotsbits): {}", const_gen_mask_u32(numslotsbits));

            // Write it into the new slot
            let new_slot_p = (slabbp | const_shl_u32_usize(newslotnum, slotsizebits)) as *mut u32;
            debug_assert!(new_slot_p.is_aligned_to(SINGLEWORDSIZE));
            unsafe { *new_slot_p = next_entry_link };

            // Create a new flh word, pointing to the new entry
            let newflhdword = ((counter as u64 + 1) << 32) | newslotnum as u64;
            //xxxxeprintln!("in push: flh_addr: {flh_addr:064b}, newslotnum: {newslotnum:064b}, newflhdword: {newflhdword:064b}");

            // Compare and exchange
            if flha.compare_exchange_weak(flhdword, newflhdword, AcqRel, Acquire).is_ok() {
                break;
            }
        }
    }

    fn encode_next_entry_link(baseslotnum: u32, targslotnum: u32, numslotsbits: u8) -> u32 {
        targslotnum.wrapping_sub(baseslotnum).wrapping_sub(1) & const_gen_mask_u32(numslotsbits)
    }

    fn decode_next_entry_link(baseslotnum: u32, codeword: u32, numslotsbits: u8) -> u32 {
        // xxx lookup table (28 entries) instead of gen_mask?
        (baseslotnum + codeword + 1) & const_gen_mask_u32(numslotsbits)
    }
        
    /// Allocate a slot from this slab by popping the free-list-head. Return the resulting pointer
    /// as a usize, or null pointer (0) if this slab is full.
    fn pop_slot_from_freelist(&self, slabbp: usize, flh_addr: usize, numslotsbits: u8, slotsizebits: u8) -> usize {
        debug_assert!(slabbp != 0);
        debug_assert!((slabbp % const_one_shl_usize(slotsizebits)) == 0);
        debug_assert!(flh_addr % DOUBLEWORDSIZE == 0);
        debug_assert!(numslotsbits <= NUM_SMALL_SLOTS_BITS, "{numslotsbits} <= {NUM_SMALL_SLOTS_BITS}"); // the most slots
        debug_assert!(slotsizebits < LARGE_SLOT_SIZE_BITS_PLUS_NUM_SLOTS_BITS); // the biggest slot

        // xxx use smbp or sysbp and with_metadata_of() and/or with_addr() ?
        let flha = unsafe { AtomicU64::from_ptr(flh_addr as *mut u64) };

        loop {
            // Load the value from the flh
            let flhdword = flha.load(Acquire);
            let curfirstentryslotnum = (flhdword & (u32::MAX as u64)) as u32;

            debug_assert!(curfirstentryslotnum < const_one_shl_u32(numslotsbits));

            let curfirstentry_p = slabbp | const_shl_u32_usize(curfirstentryslotnum, slotsizebits);

            if curfirstentry_p == flh_addr {
                // meaning no next entry, meaning the free list is empty
                break 0
            };

            // Read the bits from the first entry's slot and decode them into a slot number. These
            // bits might be "invalid" in the sense of not encoding a slot number, if the flh has
            // changed since we read it above and another thread has started using these bits for
            // something else (e.g. user data or another linked list update). That's okay because in
            // that case our attempt to update the flh below with information derived from these
            // bits will fail.
            let curfirstentryval = unsafe { *(curfirstentry_p as *mut u32) };

            let newnextentryslotnum: u32 = Self::decode_next_entry_link(curfirstentryslotnum, curfirstentryval, numslotsbits);
            //xxxeprintln!("in pop: flhdword: {flhdword}, curfirstentryslotnum: {curfirstentryslotnum}, curfirstentryval: {curfirstentryval}, newnextentryslotnum: {newnextentryslotnum}");

            // Create a new flh word, with the new slotnum, pointing to the new first slot
            let counter: u32 = (flhdword >> 32) as u32;
            let newflhdword = ((counter as u64 + 1) << 32) | newnextentryslotnum as u64;

            // Compare and exchange
            if flha.compare_exchange_weak(flhdword, newflhdword, AcqRel, Acquire).is_ok() {
                break curfirstentry_p;
            }
        }
    }
}

use std::cmp::max;
unsafe impl GlobalAlloc for Smalloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match self.idempotent_init() {
            Err(error) => {
                eprintln!("Failed to alloc; underlying error: {error}"); // xxx can't use without heap allocation?
                null_mut()
            }
            Ok(smbp) => {
                let reqsiz = layout.size();
                let reqalign = layout.align();
                debug_assert!(reqsiz > 0);
                debug_assert!(reqalign > 0);
                debug_assert!((reqalign & (reqalign - 1)) == 0); // alignment must be a power of two

                let reqsizbits = alignedsize_minus1_bits_lzcnt(reqsiz, reqalign);
                let sc = max(SMALLEST_SLOT_SIZE_BITS, reqsizbits) - SMALLEST_SLOT_SIZE_BITS;
                //xxxeprintln!("in alloc(), reqsizbits: {reqsizbits}, sc: {sc}");

                if sc >= (NUM_SMALL_SCS + NUM_MEDIUM_SCS + NUM_LARGE_SCS) {
                    // This request exceeds our largest slot size, so we return null ptr.
                    null_mut()
                } else {
                    let ptr = if sc == 0 {
                        // Size class 0.

                        let slabnum = get_thread_num() & SMALL_SLABNUM_MASK;

                        // The slabbp is the smbp with the "size class 0" indicator bit, and the
                        // slabnum bits.
                        let slabbp = smbp | SIZECLASS_0_SC_INDICATOR_MASK | const_shl_u32_usize(slabnum, SMALLEST_SLOT_SIZE_BITS + NUM_SMALL_SLOTS_BITS);

                        // The flh's address is the second-to-last slot:
                        let flh_addr = (slabbp | SIZECLASS_0_SLOTNUM_MASK) & !SIZECLASS_0_SLOTNUM_LSB_MASK;

                        self.pop_slot_from_freelist(slabbp, flh_addr, NUM_SMALL_SLOTS_BITS, SMALLEST_SLOT_SIZE_BITS)
                    } else if sc < NUM_SMALL_SCS {
                        // Small size class (but not 0 since that was handled in the previous case).

                        let slabnum = get_thread_num() & SMALL_SLABNUM_MASK;

                        let slotsizebits = sc + SMALLEST_SLOT_SIZE_BITS;

                        // The slabbp is the smbp with the size class indicator bit and the slabnum
                        // bits set.
                        let slabbp = smbp | const_shl_usize_usize(SIZECLASS_0_SC_INDICATOR_MASK, sc) | const_shl_u32_usize(slabnum, slotsizebits + NUM_SMALL_SLOTS_BITS); // xxx remove the as usize

                        // The flh's address is the last slot, so turn on all of the slotnum bits:
                        // xxx ? lookup table (4 entries) instead of shl?
                        let flh_addr = slabbp | const_shl_usize_usize(SIZECLASS_0_SLOTNUM_MASK, sc);

                        self.pop_slot_from_freelist(slabbp, flh_addr, NUM_SMALL_SLOTS_BITS, slotsizebits)
                    } else if sc < NUM_SMALL_SCS + NUM_MEDIUM_SCS {
                        // Medium size class.

                        // The slabbp is the smbp with the size class indicator bit.
                        // xxx lookup table (5 entries) instead of shl?
                       let slabbp = smbp | const_shl_usize_usize(SIZECLASS_5_SC_INDICATOR_MASK, sc - 5);

                        // The flh's address is the last slot, so turn on all of the slotnum bits:
                        // xxx ? lookup table (5 entries) instead of shl?
                        let flh_addr = slabbp | const_shl_usize_usize(SIZECLASS_5_SLOTNUM_MASK, sc - 5);

                        self.pop_slot_from_freelist(slabbp, flh_addr, NUM_MEDIUM_SLOTS_BITS, sc + SMALLEST_SLOT_SIZE_BITS)
                    } else {
                        // Large size class.

                        // The slabbp is the smbp with the size class indicator bits (large sc means
                        // the top two bits are 0b10), and the size class bits (next 5 bits) encode
                        // the sizeclass.
                        
                        // xxx lookup table (26 entries) instead of substract then shl?
                        let largesc = sc - NUM_SMALL_SCS - NUM_MEDIUM_SCS;
                        let slabbp = smbp | LARGE_SC_INDICATOR_MASK | const_shl_u8_usize(largesc, LARGE_SLOT_SIZE_BITS_PLUS_NUM_SLOTS_BITS);

                        let slotsizebits = sc + SMALLEST_SLOT_SIZE_BITS;

                        let slotnumbits = LARGE_SLOT_SIZE_BITS_PLUS_NUM_SLOTS_BITS - slotsizebits;

                        // The flh's address is the last slot:
                        // xxx lookup table (26 entries) instead of gen_mask then shl?
                        let flh_addr = slabbp | const_shl_u32_usize(const_gen_mask_u32(slotnumbits), slotsizebits);

                        //xxxlet res = self.pop_slot_from_freelist(slabbp, flh_addr, slotnumbits, slotsizebits);
                        //xxxeprintln!("in alloc() in large case, reqsizbits: {reqsizbits}, sc: {sc}, largesc: {largesc}, slabbp: {slabbp:064b}, slotsizebits: {slotsizebits}, slotnumbits: {slotnumbits}, res: {res:064b}");
                        //xxxres
                        self.pop_slot_from_freelist(slabbp, flh_addr, slotnumbits, slotsizebits)
                    };

                    if ptr == 0 {
                        // The slab was full. Overflow to a slab with larger slots, by recursively
                        // calling `.alloc()` with a doubled requested size. (Doubling the requested
                        // size guarantees that the new recursive request will use the next larger
                        // sc.)

                        let doublesize_layout = Layout::from_size_align(reqsiz * 2, reqalign).unwrap();//xxx use the unsafe version
                        unsafe { self.alloc(doublesize_layout) }
                    } else {
                        ptr as *mut u8
                    }
                }
            }
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        debug_assert!(!ptr.is_null());
        debug_assert!((layout.align() & (layout.align() - 1)) == 0); // alignment must be a power of two

        let alignedsizebits: u8 = alignedsize_minus1_bits_lzcnt(layout.size(), layout.align());

        // xxx compare asm 
        let p_addr = ptr.addr();
        let smbp = self.get_sm_baseptr();

        // If the pointer is before the smalloc base pointer or after the end of all of our slots,
        // then it is invalid.
        let highest_addr = smbp + HIGHEST_SMALLOC_SLOT_ADDR;

        debug_assert!((p_addr >= smbp) && (p_addr <= highest_addr));

        // Okay now we know that it is a pointer into smalloc's region.

        // Turn off all the bits of the address that aren't smalloc address bits. "s_addr" is short
        // for "smallocaddr". It's the part of the address of ptr that is within smalloc's allocated
        // region. (i.e. it is the least-significant 45 bits of ptr.)
        let s_addr = p_addr & SMALLOC_ADDRESS_BITS_MASK;
        
        // The number of leading zeros tells us if it is a small, medium, or large sizeclass, and if
        // it is small or medium then the number of leading zeros also tells us exactly which
        // sizeclass it is.
        let lzc = help_leading_zeros_usize(s_addr);

        debug_assert!(lzc >= UNUSED_MSB_ADDRESS_BITS); // the 19 significant address bits that we masked off
        //xxxdebug_assert!(UNUSED_MSB_ADDRESS_BITS == 19); // 19 ? 19 !

        // The lowest address (size class 5) has 29 leading zeros (including the 10 zeros of its
        // address within smalloc's entire region.
        debug_assert!(lzc <= UNUSED_MSB_ADDRESS_BITS + NUM_SMALL_SCS + NUM_MEDIUM_SCS);
        //xxxdebug_assert!(UNUSED_MSB_ADDRESS_BITS + NUM_SMALL_SCS + NUM_MEDIUM_SCS == 29); // 29 ? 29 !
        //xxxdebug_assert!(UNUSED_MSB_ADDRESS_BITS + NUM_SMALL_SCS == 24); // 24 ? 24 !

        if lzc > UNUSED_MSB_ADDRESS_BITS + NUM_SMALL_SCS  {
            // This pointer is in the region for medium size classes.
            let sizeclass = 34 - lzc; // symbolify the 34??

            let slotsizebits = sizeclass + SMALLEST_SLOT_SIZE_BITS;
            debug_assert!(slotsizebits >= alignedsizebits);

            // The pointer is required to point to the first address in its slot
            debug_assert!(help_trailing_zeros_usize(s_addr) >= slotsizebits);

            // After the unused least-significant address bits, the next-least-significant 27 bits
            // encode the slot number. This mask has 1 in each position that encodes the slotnum.
            // xxx lookup table (5 entries) instead of shift ??
            debug_assert!(slotsizebits + NUM_MEDIUM_SLOTS_BITS < usize::BITS as u8, "slotsizebits: {slotsizebits}, NUM_MEDIUM_SLOTS_BITS: {NUM_MEDIUM_SLOTS_BITS}");
            let slotnummask = const_shl_u32_usize(HIGHEST_MEDIUM_SLOTNUM, slotsizebits);

            // The pointer to the beginning of the slab is just the p_addr with all of those
            // slotnum bits turned off:
            let slabbp = p_addr & !slotnummask;

            // The flh is in the last slot in this slab, its address is p_addr with all of those
            // slotnum bits turned on:
            let flh_addr = p_addr | slotnummask;
            //xxxeprintln!("in dealloc(), p_addr: {p_addr:064b}, slotnummask: {slotnummask:064b}, flh_addr: {flh_addr:064b}");

            // The slotnum is just the bits covered by the slotnummask.
            let slotnum = const_shr_usize_u32(s_addr & slotnummask, slotsizebits);

            //xxxxeprintln!("push_slot(..., ..., {slotnum}, ...)");
            self.push_slot_onto_freelist(slabbp, flh_addr, slotnum, NUM_MEDIUM_SLOTS_BITS, slotsizebits);
        } else if lzc == UNUSED_MSB_ADDRESS_BITS + NUM_SMALL_SCS {
            // This pointer is in sizeclass 0.

            // The pointer is required to point to the first address in its slot
            debug_assert!(help_trailing_zeros_usize(s_addr) >= SMALLEST_SLOT_SIZE_BITS);

            // The pointer to the beginning of the slab is just the p_addr with all of the slotnum
            // bits turned off:
            let slabbp = p_addr & !SIZECLASS_0_SLOTNUM_MASK;

            // The flh is in the second-to-last slot in this slab, so compute its address like this:
            // turn on all of the bits of the address that encode the slotnum except turn off the
            // least-significant one:
            let flh_addr = (p_addr | SIZECLASS_0_SLOTNUM_MASK) & !SIZECLASS_0_SLOTNUM_LSB_MASK;

            // The slotnum is just the bits covered by the slotnummask.
            let slotnum = const_shr_usize_u32(s_addr & SIZECLASS_0_SLOTNUM_MASK, SMALLEST_SLOT_SIZE_BITS);

            self.push_slot_onto_freelist(slabbp, flh_addr, slotnum, NUM_SMALL_SLOTS_BITS, SMALLEST_SLOT_SIZE_BITS);
        } else if lzc > UNUSED_MSB_ADDRESS_BITS {
            // This pointer is in the region for small size classes. (But this is not size class 0
            // because that would be handled by the previous case.)
            let sizeclass = UNUSED_MSB_ADDRESS_BITS + NUM_SMALL_SCS - lzc;

            let slotsizebits = sizeclass + SMALLEST_SLOT_SIZE_BITS;
            debug_assert!(slotsizebits >= alignedsizebits);

            // The pointer is required to point to the first address in its slot
            debug_assert!(help_trailing_zeros_usize(s_addr) >= slotsizebits);

            // After the unused least-significant address bits, the next-least-significant 29 bits
            // encode the slot number. This mask has 1 in each position that encodes the slotnum.
            // xxx lookup table (4 entries) instead of shift ??
            let slotnummask = const_shl_u32_usize(HIGHEST_SMALL_SLOTNUM, slotsizebits);

            // The pointer to the beginning of the slab is just the p_addr with all of those
            // slotnum bits turned off:
            let slabbp = p_addr & !slotnummask;

            // The flh is in the last slot in this slab, its address is p_addr with all of those
            // slotnum bits turned on:
            let flh_addr = p_addr | slotnummask;

            // The slotnum is just those bits.
            let smallslotnum = const_shr_usize_u32(s_addr & slotnummask, slotsizebits);

            self.push_slot_onto_freelist(slabbp, flh_addr, smallslotnum, NUM_SMALL_SLOTS_BITS, slotsizebits);
        } else {
            // This pointer is in the region for large size classes.

            let sizeclass = const_shr_usize_u8(s_addr & LARGE_SLAB_SC_MASK, LARGE_SLOT_SIZE_BITS_PLUS_NUM_SLOTS_BITS) + NUM_SMALL_SCS + NUM_MEDIUM_SCS;

            let slotsizebits = sizeclass + SMALLEST_SLOT_SIZE_BITS;
            debug_assert!(slotsizebits >= alignedsizebits);

            // The pointer is required to point to the first address in its slot
            debug_assert!(help_trailing_zeros_usize(s_addr) >= slotsizebits);

            // Each successive large size class has one more slotsizebits and one fewer slotnumbits
            // than the one before.
            let slotnumbits = LARGE_SLOT_SIZE_BITS_PLUS_NUM_SLOTS_BITS - slotsizebits;

            // This mask has 1 in each position that encodes the slotnum.
            // xxx lookup table (26 entries) instead of gen_mask and shift-left ??
            let slotnummask = const_shl_u32_usize(const_gen_mask_u32(slotnumbits), slotsizebits);

            // The pointer to the beginning of the slab is just the p_addr with all of those
            // slotnum bits turned off:
            let slabbp = p_addr & !slotnummask;

            // The flh is in the last slot in this slab, its address is p_addr with all of the bits
            // turned on in the span that encodes the slotnum:
            let flh_addr = p_addr | slotnummask;

            // The slotnum is just those bits.
            let largeslotnum = const_shr_usize_u32(s_addr & slotnummask, slotsizebits);

            self.push_slot_onto_freelist(slabbp, flh_addr, largeslotnum, slotnumbits, slotsizebits);
        }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, reqsize: usize) -> *mut u8 {
        debug_assert!(!ptr.is_null());
        let oldsize = layout.size();
        debug_assert!(oldsize > 0);
        let oldalignment = layout.align();
        debug_assert!(oldalignment > 0);
        debug_assert!((oldalignment & (oldalignment - 1)) == 0, "alignment must be a power of two");
        debug_assert!(reqsize > 0);

        let oldsizbits = alignedsize_minus1_bits_lzcnt(oldsize, oldalignment);
        let reqsizbits = alignedsize_minus1_bits_lzcnt(reqsize, oldalignment);

        // If the requested new size (rounded up to a slot) is <= the original size (rounded up to a
        // slot), just return the pointer and we're done.
        if reqsizbits <= oldsizbits {
            //eprintln!("{ptr:?}");
            return ptr;
        }

        let reqsc = max(SMALLEST_SLOT_SIZE_BITS, reqsizbits) - SMALLEST_SLOT_SIZE_BITS;

        // The "growers" rule: use the smallest of the following size classes that will fit: 64
        // bytes (size class 4), 4096 bytes (size class 10), or double the current size.
        let newsc = if reqsc <= 4 {
            4
        } else if reqsc <= 10 {
            10
        } else {
            reqsc + 1
        };

        let l = unsafe { Layout::from_size_align_unchecked(const_one_shl_usize(newsc + SMALLEST_SLOT_SIZE_BITS), oldalignment) };
        let newp = unsafe { self.alloc(l) };
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

use std::cmp::min;
use core::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize};
use std::sync::atomic::Ordering::{AcqRel, Acquire, Release, Relaxed};
mod platformalloc;
use platformalloc::{sys_alloc, sys_dealloc};
use platformalloc::vendor::PAGE_SIZE;
use std::ptr::{copy_nonoverlapping, null_mut};
use std::cell::Cell;
use thousands::Separable;
use platformalloc::AllocFailed;

// xxx look at asm and benchmark these vs the builtin alternatives

// xxx benchmark and inspect asm for this vs <<
const fn const_shl_u32_usize(value: u32, shift: u8) -> usize {
    debug_assert!((shift as u32) < usize::BITS);
    debug_assert!(help_leading_zeros_usize(value as usize) >= shift); // we never shift off 1 bits currently
    unsafe { (value as usize).unchecked_shl(shift as u32) }
}

// xxx benchmark and inspect asm for this vs <<
const fn const_shl_u8_usize(value: u8, shift: u8) -> usize {
    debug_assert!((shift as u32) < usize::BITS);
    debug_assert!(help_leading_zeros_usize(value as usize) >= shift); // we never shift off 1 bits currently
    unsafe { (value as usize).unchecked_shl(shift as u32) }
}

// xxx benchmark and inspect asm for this vs <<
const fn const_shl_usize_usize(value: usize, shift: u8) -> usize {
    debug_assert!((shift as u32) < usize::BITS);
    debug_assert!(help_leading_zeros_usize(value) >= shift); // we never shift off 1 bits currently
    unsafe { value.unchecked_shl(shift as u32) }
}

#[inline(always)]
const fn const_one_shl_usize(shift: u8) -> usize {
    debug_assert!((shift as u32) < usize::BITS);

    unsafe { 1usize.unchecked_shl(shift as u32) }
}

#[inline(always)]
const fn const_one_shl_u32(shift: u8) -> u32 {
    debug_assert!((shift as u32) < u32::BITS);

    unsafe { 1u32.unchecked_shl(shift as u32) }
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
    debug_assert!(help_leading_zeros_usize(res) as u32 >= usize::BITS - u32::BITS);
    res as u32
}

#[inline(always)]
const fn const_shr_usize_u8(value: usize, shift: u8) -> u8 {
    debug_assert!((shift as u32) < usize::BITS);
    let res = unsafe { value.unchecked_shr(shift as u32) };
    // No leaving 1 bits stranded up there
    debug_assert!(help_leading_zeros_usize(res) as u32 >= usize::BITS - u8::BITS);
    res as u8
}

#[inline(always)]
const fn _const_gen_mask_usize(numbits: u8) -> usize {
    debug_assert!((numbits as u32) < usize::BITS);

    unsafe { 1usize.unchecked_shl(numbits as u32) - 1 }
}

#[inline(always)]
const fn const_gen_mask_u32(numbits: u8) -> u32 {
    debug_assert!((numbits as u32) < u32::BITS);

    unsafe { 1u32.unchecked_shl(numbits as u32) - 1 }
}

/// Returns the number of significant bits in the aligned size. This is the log base 2 of the size
/// of slot required to hold requests of this size and alignment.
// xxx nanobenchmark these two ways to compute alignedsize/alignedsizebits
fn alignedsize_minus1_bits_lzcnt(size: usize, align: usize) -> u8 {
    debug_assert!(size > 0);
    debug_assert!(align > 0);
    usize::BITS as u8 - min(help_leading_zeros_usize(size - 1), help_leading_zeros_usize(align - 1))
}

#[inline(always)]
const fn _help_leading_zeros_u32(x: u32) -> u8 {
    let res = x.leading_zeros();
    debug_assert!(res <= u8::MAX as u32);
    res as u8
}
    
#[inline(always)]
const fn help_leading_zeros_usize(x: usize) -> u8 {
    let res = x.leading_zeros();
    debug_assert!(res <= u8::MAX as u32);
    res as u8
}
    
#[inline(always)]
const fn help_trailing_zeros_usize(x: usize) -> u8 {
    x.trailing_zeros() as u8
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
// xxx let's assume 93 trillion bytes of virtual address space

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

// xxx add benchmarks of high thread contention

// #[cfg(test)]
// mod benches {
//     use crate::*;

//     use rand::{Rng, SeedableRng};
//     use rand::rngs::StdRng;
//     use std::ptr::null_mut;
//     use std::alloc::{GlobalAlloc, Layout};
//     use std::hint::black_box;
//     use std::time::Duration;
//     use crate::platformalloc::vendor::{CACHE_SIZE, CACHE_LINE_SIZE};

//     // #[cfg(target_vendor = "apple")]
//     // pub mod plat {
//     //     use crate::benches::{Criterion, Duration};
//     //     use criterion::measurement::plat_apple::MachAbsoluteTimeMeasurement;
//     //     pub fn make_criterion() -> Criterion<MachAbsoluteTimeMeasurement> {
//     //         Criterion::default().with_measurement(MachAbsoluteTimeMeasurement::default()).sample_size(300).warm_up_time(Duration::new(10, 0)).significance_level(0.0001).confidence_level(0.9999)
//     //     }
//     // }

//     // #[cfg(target_arch = "x86_64")]
//     // pub mod plat {
//     //     use criterion::measurement::plat_x86_64::RDTSCPMeasurement;
//     //     use crate::benches::{Criterion, Duration};
//     //     pub fn make_criterion() -> Criterion<RDTSCPMeasurement> {
//     //         Criterion::default().with_measurement(RDTSCPMeasurement::default()).sample_size(300).warm_up_time(Duration::new(10, 0)).significance_level(0.0001).confidence_level(0.9999)
//     //     }
//     // }

//     // #[cfg(not(any(target_vendor = "apple", target_arch = "x86_64")))]
//     pub mod plat {
//         use crate::benches::Duration;
//        //xxx pub fn make_criterion() -> Criterion {
//        //xxx     Criterion::default().sample_size(300).warm_up_time(Duration::new(10, 0)).significance_level(0.0001).confidence_level(0.9999)
//        //xxx }
//     }
    
//     fn randdist_reqsiz(r: &mut StdRng) -> usize {
//         // The following distribution was roughly modelled on smalloclog profiling of Zebra.
//         let randnum = r.random::<u8>();

//         if randnum < 50 {
//             r.random_range(1..16)
//         } else if randnum < 150 {
//             32
//         } else if randnum < 200 {
//             64
//         } else if randnum < 250 {
//             r.random_range(65..16384)
//         } else {
//             4_000_000
//         }
//     }

//     #[test]
//     fn bench_size_to_sc_lzcnt_min() {
//         let mut c = plat::make_criterion();

//         const NUM_ARGS: usize = 1_000_000;

//         let mut r = StdRng::seed_from_u64(0);

//         let mut reqs = Vec::with_capacity(NUM_ARGS);

//         // Generate a distribution of sizes that is similar to realistic usages of smalloc (so that
//         // our benchmarks are more representative than if we just generated some kind of even
//         // distribution or something).
//         while reqs.len() < NUM_ARGS {
//             reqs.push(randdist_reqsiz(&mut r));
//         }

//         let mut i = 0;
//         let mut a = 0; // to prevent compiler from optimizing stuff away
//         c.bench_function("size_to_sc_lzcnt_min", |b| b.iter(|| {
//             a ^= black_box(crate::size_to_sc_lzcnt_min(reqs[i % NUM_ARGS]));
//             i += 1;
//         }));
//     }

//     #[test]
//     fn bench_size_to_sc_lzcnt_branch() {
//         let mut c = plat::make_criterion();

//         const NUM_ARGS: usize = 1_000_000;

//         let mut r = StdRng::seed_from_u64(0);

//         let mut reqs = Vec::with_capacity(NUM_ARGS);

//         // Generate a distribution of sizes that is similar to realistic usages of smalloc (so that
//         // our benchmarks are more representative than if we just generated some kind of even
//         // distribution or something).
//         while reqs.len() < NUM_ARGS {
//             reqs.push(randdist_reqsiz(&mut r));
//         }

//         let mut i = 0;
//         let mut a = 0; // to prevent compiler from optimizing stuff away
//         c.bench_function("size_to_sc_lzcnt_branch", |b| b.iter(|| {
//             a ^= black_box(crate::size_to_sc_lzcnt_branch(reqs[i % NUM_ARGS]));
//             i += 1;
//         }));
//     }

//     #[test]
//     fn bench_size_to_sc_log_branch() {
//         let mut c = plat::make_criterion();

//         const NUM_ARGS: usize = 1_000_000;

//         let mut r = StdRng::seed_from_u64(0);

//         let mut reqs = Vec::with_capacity(NUM_ARGS);

//         // Generate a distribution of sizes that is similar to realistic usages of smalloc (so that
//         // our benchmarks are more representative than if we just generated some kind of even
//         // distribution or something).
//         while reqs.len() < NUM_ARGS {
//             reqs.push(randdist_reqsiz(&mut r));
//         }

//         let mut i = 0;
//         let mut a = 0; // to prevent compiler from optimizing stuff away
//         c.bench_function("size_to_sc_log_branch", |b| b.iter(|| {
//             a ^= black_box(crate::size_to_sc_log_branch(reqs[i % NUM_ARGS]));
//             i += 1;
//         }));
//     }

//     #[test]
//     fn bench_offset_to_allslabnum_lzcnt() {
//         let mut c = plat::make_criterion();

//         let sm = Smalloc::new();
//         sm.idempotent_init().unwrap();
//         let smbp = sm.get_sm_baseptr();

//         const NUM_ARGS: usize = 1_000_000;

//         let mut r = StdRng::seed_from_u64(0);

//         let mut reqs = Vec::with_capacity(NUM_ARGS);

//         // Generate a distribution of offsets that is similar to realistic usages of smalloc (so
//         // that our benchmarks are more representative than if we just generated some kind of even
//         // distribution or something).
//         while reqs.len() < NUM_ARGS {
//             let mut s = randdist_reqsiz(&mut r);
//             if s > slotsize(NUM_SMALL_PLUS_MEDIUM_SLABS - 1) {
//                 s = slotsize(NUM_SMALL_PLUS_MEDIUM_SLABS - 1);
//             }
//             let l = Layout::from_size_align(s, 1).unwrap();
//             let p = unsafe { sm.alloc(l) };
//             let o = crate::offset_of_ptr(sybp, smbp, p);
//             reqs.push(o.unwrap());
//         }

//         let mut i = 0;
//         let mut a = 0; // to prevent compiler from optimizing stuff away
//         c.bench_function("offset_to_allslabnum_lzcnt", |b| b.iter(|| {
//             a ^= black_box(crate::offset_to_allslabnum_lzcnt(reqs[i % NUM_ARGS]));
//             i += 1;
//         }));
//     }

//     #[test]
//     fn bench_offset_to_allslabnum_log() {
//         let mut c = plat::make_criterion();

//         let sm = Smalloc::new();
//         sm.idempotent_init().unwrap();
//         let smbp = sm.get_sm_baseptr();

//         const NUM_ARGS: usize = 1_000_000;

//         let mut r = StdRng::seed_from_u64(0);

//         let mut reqs = Vec::with_capacity(NUM_ARGS);

//         // Generate a distribution of offsets that is similar to realistic usages of smalloc (so
//         // that our benchmarks are more representative than if we just generated some kind of even
//         // distribution or something).
//         while reqs.len() < NUM_ARGS {
//             let mut s = randdist_reqsiz(&mut r);
//             if s > slotsize(NUM_SMALL_PLUS_MEDIUM_SLABS - 1) {
//                 s = slotsize(NUM_SMALL_PLUS_MEDIUM_SLABS - 1);
//             }
//             let l = Layout::from_size_align(s, 1).unwrap();
//             let p = unsafe { sm.alloc(l) };
//             let o = crate::offset_of_ptr(sybp, smbp, p);
//             reqs.push(o.unwrap());
//         }

//         let mut i = 0;
//         let mut a = 0; // to prevent compiler from optimizing stuff away
//         c.bench_function("offset_to_allslabnum_log", |b| b.iter(|| {
//             a ^= black_box(crate::offset_to_allslabnum_log(reqs[i % NUM_ARGS]));
//             i += 1;
//         }));
//     }

//     #[test]
//     fn bench_pop_flh_small_sn_0_empty() {
//         let mut c = plat::make_criterion();

//         let sm = Smalloc::new();
//         sm.idempotent_init().unwrap();

//         c.bench_function("pop_small_flh_sep_sn_0_empty", |b| b.iter(|| { // xxx temp name for comparison to prev version
//             let smbp = sm.get_sm_baseptr();
//             let slab_bp = unsafe { smbp.add(SizeClass(0).base_offset()) };
// // xxx include these lookups inside bench ? For comparability with smalloc v2's `pop_small_flh()`? Or not, for modeling of smalloc v3's runtime behavior ? *thinky face*
//             let areanum = get_thread_num();
//             let flha = sm.get_atomicu64(small_flh_offset(0, areanum));
//             let eaca = sm.get_atomicu64(small_eac_offset(0, areanum));
//             black_box(sm.inner_alloc(flha, slab_bp, eaca, 4, NUM_SMALL_SLOTS));
//         }));
//     }

//     #[test]
//     fn bench_pop_flh_small_sn_4_empty() {
//         let mut c = plat::make_criterion();

//         let sm = Smalloc::new();
//         sm.idempotent_init().unwrap();
//         let smbp = sm.get_sm_baseptr();
//         let slab_bp = unsafe { smbp.add(small_slab_base_offset(4, get_thread_num())) };

//         c.bench_function("pop_small_flh_sn_0_empty", |b| b.iter(|| { // xxx temp name for comparison to prev version
//             let areanum = get_thread_num();
//             let flha = sm.get_atomicu64(small_flh_offset(4, areanum));
//             let eaca = sm.get_atomicu64(small_eac_offset(4, areanum));
//             black_box(sm.inner_alloc(flha, slab_bp, eaca, 64, NUM_SMALL_SLOTS));
//         }));
//     }

//     use std::sync::atomic::Ordering;
//     use rand::seq::SliceRandom;

//     #[derive(PartialEq)]
//     enum DataOrder {
//         Sequential, Random
//     }
    
//     fn help_bench_pop_small_slab_freelist_wdata(fnname: &str, smallslabnum: usize, ord: DataOrder, thenwrite: bool) {
//         let mut c = plat::make_criterion();

//         let gtan1 = get_thread_num();

//         let sm = Smalloc::new();
//         sm.idempotent_init().unwrap();

//         // To prime the pump for the assertion inside setup() that the free list isn't empty.
//         let l = Layout::from_size_align(slotsize(smallslabnum), 1).unwrap();
//         unsafe { sm.dealloc(sm.alloc(l), l) };

//         let router = RefCell::new(StdRng::seed_from_u64(0));

//         const NUM_ARGS: usize = 16_000;
//         let setup = || {
//             let mut rinner = router.borrow_mut();

//             let gtan2 = get_thread_num();
//             assert_eq!(gtan1, gtan2);

//             // reset the free list and eac
//             let eaca = sm.get_atomicu64(small_eac_offset(smallslabnum, gtan2));
//             eaca.store(0, Ordering::Release);
//             let flha = sm.get_atomicu64(small_flh_offset(smallslabnum, gtan2));

//             // assert that the free list hasnt't been emptied out, which would mean that during the
//             // previous batch of benchmarking, the free list ran dry and we started benchmarking the
//             // "pop from empty free list" case instead of what we're trying to benchmark here.
//             assert_ne!(flha.load(Ordering::Acquire) & u32::MAX as u64, 0);

//             flha.store(0, Ordering::Release);
            
//             let mut ps = Vec::with_capacity(NUM_ARGS);

//             while ps.len() < NUM_ARGS {
//                 ps.push(unsafe { sm.alloc(l) })
//             }

//             match ord {
//                 DataOrder::Sequential => { }
//                 DataOrder::Random => {
//                     ps.shuffle(&mut rinner)
//                 }
//             }

//             for p in ps.iter() {
//                 unsafe { sm.dealloc(*p, l) };
//             }
//         };

//         let smbp = sm.get_sm_baseptr();

//         let f = |()| {
//             let gtan3 = get_thread_num();
//             assert_eq!(gtan1, gtan3);

//             let slab_bp = unsafe { smbp.add(small_slab_base_offset(smallslabnum, gtan3)) };
//             let flha = sm.get_atomicu64(small_flh_offset(smallslabnum, gtan3));
//             let eaca = sm.get_atomicu64(small_eac_offset(smallslabnum, gtan3));
            
//             let p2 = black_box(sm.inner_alloc(flha, slab_bp, eaca, slotsize(smallslabnum), NUM_SMALL_SLOTS));
//             assert!(!p2.is_null());

//             if thenwrite {
//                 // Okay now write into the newly allocated space.
//                 unsafe { std::ptr::copy_nonoverlapping(&99_u8, p2, 1) };
//             }
//         };

//         c.bench_function(fnname, move |b| b.iter_batched(setup, f, BatchSize::SmallInput));
//     }

//     #[test]
//     fn bench_pop_small_sn_0_wdata_sequential() {
//         help_bench_pop_small_slab_freelist_wdata("pop_small_flh_sep_sn_0_wdata_sequential", 0, DataOrder::Sequential, false) // xxx temp name for comparison to prev version
//     }

//     #[test]
//     fn bench_pop_small_sn_0_wdata_sequential_then_write() {
//         help_bench_pop_small_slab_freelist_wdata("pop_small_flh_sep_sn_0_wdata_sequential_then_write", 0, DataOrder::Sequential, true) // xxx temp name for comparison to prev version
//     }

//     #[test]
//     fn bench_pop_small_sn_0_wdata_random() {
//         help_bench_pop_small_slab_freelist_wdata("pop_small_flh_sep_sn_0_wdata_random", 0, DataOrder::Random, false) // xxx temp name for comparison to prev version
//     }

//     #[test]
//     fn bench_pop_small_sn_0_wdata_random_then_write() {
//         help_bench_pop_small_slab_freelist_wdata("pop_small_flh_sep_sn_0_wdata_random_then_write", 0, DataOrder::Random, true) // xxx temp name for comparison to prev version
//     }

//     #[test]
//     fn bench_pop_small_sn_1_wdata_sequential_n() {
//         help_bench_pop_small_slab_freelist_wdata("pop_small_flh_sn_4_wdata_sequential", 1, DataOrder::Sequential, false) // xxx temp name for comparison to prev version
//     }

//     #[test]
//     fn bench_pop_small_sn_1_wdata_sequential_then_write() {
//         help_bench_pop_small_slab_freelist_wdata("pop_small_flh_sn_4_wdata_sequential_then_write", 1, DataOrder::Sequential, true) // xxx temp name for comparison to prev version
//     }

//     #[test]
//     fn bench_pop_small_sn_1_wdata_random() {
//         help_bench_pop_small_slab_freelist_wdata("pop_small_flh_sn_4_wdata_random", 1, DataOrder::Random, false) // xxx temp name for comparison to prev version
//     }

//     #[test]
//     fn bench_pop_small_sn_1_wdata_random_then_write() {
//         help_bench_pop_small_slab_freelist_wdata("pop_small_flh_sn_4_wdata_random_then_write", 1, DataOrder::Random, true) // xxx temp name for comparison to prev version
//     }

//     #[test]
//     fn bench_pop_small_sn_4_wdata_random() {
//         help_bench_pop_small_slab_freelist_wdata("pop_medium_flh_sn_0_wdata_random", 4, DataOrder::Random, false); // xxx temp name for comparison to prev version
//     }

//     #[test]
//     fn bench_pop_small_sn_4_wdata_random_then_write() {
//         help_bench_pop_medium_slab_freelist_wdata("pop_medium_flh_sn_0_wdata_random_then_write", 4, DataOrder::Random, true); // xxx temp name for comparison to prev version
//     }

//     #[test]
//     fn bench_pop_small_sn_4_wdata_sequential() {
//         help_bench_pop_medium_slab_freelist_wdata("pop_medium_flh_sn_0_wdata_sequential", 4, DataOrder::Sequential, false); // xxx temp name for comparison to prev version
//     }

//     #[test]
//     fn bench_pop_small_sn_4_wdata_sequential_then_write() {
//         help_bench_pop_medium_slab_freelist_wdata("pop_medium_flh_sn_0_wdata_sequential_then_write", 4, DataOrder::Sequential, true); // xxx temp name for comparison to prev version
//     }

//     fn help_bench_pop_medium_slab_freelist_wdata(fnname: &str, mediumslabnum: usize, ord: DataOrder, thenwrite: bool) {
//         let mut c = plat::make_criterion();

//         let sm = Smalloc::new();
//         sm.idempotent_init().unwrap();

//         // To prime the pump for the assertion inside setup() that the free list isn't empty.
//         let allslabnum = mediumslabnum + NUM_SMALL_SCS;
//         let l = Layout::from_size_align(slotsize(allslabnum), 1).unwrap();
//         unsafe { sm.dealloc(sm.alloc(l), l) };

//         let router = RefCell::new(StdRng::seed_from_u64(0));

//         const NUM_ARGS: usize = 16_000;
//         let setup = || {
//             let mut rinner = router.borrow_mut();

//             // reset the free list and eac
//             let eaca = sm.get_atomicu64(medium_eac_offset(mediumslabnum));
//             eaca.store(0, Ordering::Release);
//             let flha = sm.get_atomicu64(medium_flh_offset(mediumslabnum));

//             // assert that the free list hasnt't been emptied out,
//             // which would mean that during the previous batch of
//             // benchmarking, the free list ran dry and we started
//             // benchmarking the "pop from empty free list" case
//             // instead of what we're trying to benchmark here.
//             assert_ne!(flha.load(Ordering::Acquire) & u32::MAX as u64, 0);

//             flha.store(0, Ordering::Release);
            
//             let mut ps = Vec::with_capacity(NUM_ARGS);

//             while ps.len() < NUM_ARGS {
//                 ps.push(unsafe { sm.alloc(l) })
//             }

//             match ord {
//                 DataOrder::Sequential => { }
//                 DataOrder::Random => {
//                     ps.shuffle(&mut rinner)
//                 }
//             }

//             for p in ps.iter() {
//                 unsafe { sm.dealloc(*p, l) };
//             }
//         };

//         let smbp = sm.get_sm_baseptr();

//         let f = |()| {
//             let slab_bp = unsafe { smbp.add(medium_slab_base_offset(mediumslabnum)) };
//             let flha = sm.get_atomicu64(medium_flh_offset(mediumslabnum));
//             let eaca = sm.get_atomicu64(medium_eac_offset(mediumslabnum));

//             let p2 = black_box(sm.inner_alloc(flha, slab_bp, eaca, slotsize(allslabnum), NUM_MEDIUM_SLOTS));
//             assert!(!p2.is_null());

//             if thenwrite {
//                 // Okay now write into the newly allocated space.
//                 unsafe { std::ptr::copy_nonoverlapping(&99_u8, p2, 1) };
//             }
//         };

//         c.bench_function(fnname, |b| b.iter_batched(setup, f, BatchSize::SmallInput));
//     }

//     #[test]
//     fn bench_pop_medium_sn_5_wdata_random() {
//         help_bench_pop_medium_slab_freelist_wdata("pop_medium_flh_sn_6_wdata_random", 5, DataOrder::Random, false); // xxx temp name for comparison to prev version
//     }

//     #[test]
//     fn bench_pop_medium_sn_5_wdata_random_then_write() {
//         help_bench_pop_medium_slab_freelist_wdata("pop_medium_flh_sn_6_wdata_random_then_write", 5, DataOrder::Random, true); // xxx temp name for comparison to prev version
//     }

//     #[test]
//     fn bench_pop_medium_sn_5_wdata_sequential() {
//         help_bench_pop_medium_slab_freelist_wdata("pop_medium_flh_sn_6_wdata_sequential", 5, DataOrder::Sequential, false); // xxx temp name for comparison to prev version
//     }

//     #[test]
//     fn bench_pop_medium_sn_5_wdata_sequential_then_write() {
//         help_bench_pop_medium_slab_freelist_wdata("pop_medium_flh_sn_6_wdata_sequential_then_write", 5, DataOrder::Sequential, true); // xxx temp name for comparison to prev version
//     }

//     #[test]
//     fn bench_pop_medium_sn_6_wdata_random() {
//         help_bench_pop_medium_slab_freelist_wdata("pop_medium_flh_sn_7_wdata_random", 6, DataOrder::Random, false); // xxx temp name for comparison to prev version
//     }

//     #[test]
//     fn bench_pop_medium_sn_6_wdata_random_then_write() {
//         help_bench_pop_medium_slab_freelist_wdata("pop_medium_flh_sn_7_wdata_random_then_write", 6, DataOrder::Random, true); // xxx temp name for comparison to prev version
//     }

//     #[test]
//     fn bench_pop_medium_sn_6_wdata_sequential() {
//         help_bench_pop_medium_slab_freelist_wdata("pop_medium_flh_sn_7_wdata_sequential", 6, DataOrder::Sequential, false); // xxx temp name for comparison to prev version
//     }

//     #[test]
//     fn bench_pop_medium_sn_6_wdata_sequential_then_write() {
//         help_bench_pop_medium_slab_freelist_wdata("pop_medium_flh_sn_7_wdata_sequential_then_write", 6, DataOrder::Sequential, true); // xxx temp name for comparison to prev version
//     }

//     #[test]
//     fn bench_small_alloc() {
//         let mut c = Criterion::default();

//         let sm = Smalloc::new();
//         sm.idempotent_init().unwrap();

//         const NUM_ARGS: usize = 50_000;

//         let mut r = StdRng::seed_from_u64(0);
//         let mut reqs = Vec::with_capacity(NUM_ARGS);

//         while reqs.len() < NUM_ARGS {
//             reqs.push(slotsize(r.random_range(0..NUM_SMALL_SCS)));
//         }

//         let mut accum = 0; // to prevent compiler optimizing things away
//         let mut i = 0;
//         c.bench_function("small_alloc_with_overflow", |b| b.iter(|| { // xxx temp name for comparison to prev version
//             let l = unsafe { Layout::from_size_align_unchecked(reqs[i % reqs.len()], 1) };
//             accum ^= black_box(unsafe { sm.alloc(l) }) as u64;
//             i += 1;
//         }));
//     }

//     #[test]
//     fn bench_medium_alloc() {
//         let mut c = Criterion::default();

//         const NUM_ARGS: usize = 50_000;

//         let sm = Smalloc::new();
//         sm.idempotent_init().unwrap();

//         let mut r = StdRng::seed_from_u64(0);
//         let mut reqs = Vec::with_capacity(NUM_ARGS);

//         while reqs.len() < NUM_ARGS {
//             reqs.push(slotsize(NUM_SMALL_SCS + r.random_range(0..NUM_MEDIUM_SCS)));
//         }

//         let mut accum = 0; // to prevent compiler optimizing things away
//         let mut i = 0;
//         c.bench_function("inner_medium_alloc", |b| b.iter(|| { // xxx temp name for comparison to prev version
//             let l = unsafe { Layout::from_size_align_unchecked(reqs[i % reqs.len()], 1) };
//             accum ^= black_box(unsafe { sm.alloc(l) }) as u64;
//             i += 1
//         }));
//     }

//     #[test]
//     fn bench_ptr_to_slot() {
//         let mut c = Criterion::default();

//         const NUM_ARGS: usize = 50_000_000;

//         let mut r = StdRng::seed_from_u64(0);
//         let baseptr_for_testing: *mut u8 = null_mut();
//         let mut reqptrs = Box::new(Vec::new());
//         reqptrs.reserve(NUM_ARGS);
        
//         while reqptrs.len() < NUM_ARGS {
//             // generate a random slot
//             let o = if r.random::<bool>() {
//                 // SmallSlot
//                 let areanum = r.random_range(0..NUM_SMALL_SLAB_AREAS);
//                 let smallslabnum = r.random_range(0..NUM_SMALL_SCS);
//                 let slotnum = r.random_range(0..NUM_SMALL_SLOTS);

//                 small_slot_offset(smallslabnum, areanum, slotnum)
//             } else {
//                 // medium or large slot
//                 let mediumslabnum = r.random_range(0..NUM_MEDIUM_SCS + NUM_LARGE_SCS);
//                 if mediumslabnum < NUM_MEDIUM_SCS {
//                     // medium slot
//                     let slotnum = r.random_range(0..NUM_MEDIUM_SLOTS);
//                     medium_slot_offset(mediumslabnum, slotnum)
//                 } else {
//                     // large slot
//                     let slotnum = r.random_range(0..NUM_LARGE_SCS);
//                     large_slot_offset(slotnum)
//                 }
//             };

//             // put the random slot's pointer into the test set
//             reqptrs.push(unsafe { baseptr_for_testing.add(o) });
//         }

//         let mut accum = 0; // This is to prevent the compiler from optimizing away some of these calculations.
//         let mut i = 0;
//         c.bench_function("ptr_to_slot", |b| b.iter(|| { // xxx temp name for comparison to prev version
//             let ptr = reqptrs[i % NUM_ARGS];

//             let opto = crate::offset_of_ptr(baseptr_for_testing, baseptr_for_testing, ptr);
//             let res = match opto {
//                 None => {
//                     panic!("wrong");
//                 }
//                 Some(o) => {
//                     if o < MEDIUM_SLABS_REGION_BASE {
//                         // This points into the "small-slabs-areas-region".

//                         let allslabnum = offset_to_allslabnum(o);
//                         let slotsize = slotsize(allslabnum);

//                         assert!(o.is_multiple_of(slotsize));

//                         let (areanum2, slotnum2) = small_slot(o, allslabnum, slotsize);

//                         black_box((allslabnum, areanum2, slotnum2))
//                     } else if o < LARGE_SLAB_REGION_BASE {
//                         // This points into the "medium-slabs-region".

//                         let allslabnum = offset_to_allslabnum(o);
//                         let slotsize = slotsize(allslabnum);

//                         assert!(o.is_multiple_of(slotsize));

//                         let slotnum2 = medium_slot(o, allslabnum - NUM_SMALL_SCS, slotsize);
                        
//                         black_box((allslabnum, 0, slotnum2))
//                     } else {
//                         // This points into the "large-slab".
//                         let slotnum2 = large_slot(o);
                        
//                         black_box((0, 0, slotnum2))
//                     }
//                 }
//             };

//             accum += res.2;

//             i += 1;
//         }));
//     }

//     use std::sync::Arc;
//     fn dummy_func() -> i64 {
//         let mut a = Arc::new(0);
//         for i in 0..3 {
//             for j in 0..3 {
//                 *Arc::make_mut(&mut a) ^= black_box(i * j);
//             }
//         }

//         *a
//     }

//     #[test]
//     fn bench_alloc_rand() {
//         let mut c = plat::make_criterion();

//         let sm = Smalloc::new();
//         sm.idempotent_init().unwrap();

//         let saved_thread_areanum = get_thread_num();
//         let r = RefCell::new(StdRng::seed_from_u64(0));

//         const NUM_ARGS: usize = 1_000_000;
//         let reqsouter = RefCell::new(Vec::with_capacity(NUM_ARGS));

//         let setup = || {
//             let areanum = get_thread_num();
//             assert_eq!(areanum, saved_thread_areanum);
//             let mut reqsinnersetup = reqsouter.borrow_mut();
            
//             let mut rinner = r.borrow_mut();

//             // reset the reqs vec
//             reqsinnersetup.clear();

//             // reset the free lists and eacs for all three size classes
//             for smallslabnum in 0..NUM_SMALL_SCS {
//                 let flha = sm.get_atomicu64(small_flh_offset(smallslabnum, areanum));
//                 let eaca = sm.get_atomicu64(small_eac_offset(smallslabnum, areanum));
//                 flha.store(0, Ordering::Release);
//                 eaca.store(0, Ordering::Release);
//             }

//             for mediumslabnum in 0..NUM_MEDIUM_SCS {
//                 let flha = sm.get_atomicu64(medium_flh_offset(mediumslabnum));
//                 let eaca = sm.get_atomicu64(medium_eac_offset(mediumslabnum));
//                 flha.store(0, Ordering::Release);
//                 eaca.store(0, Ordering::Release);
//             }

//             let flha = sm.get_atomicu64(large_flh_offset());
//             let eaca = sm.get_atomicu64(large_eac_offset());
//             flha.store(0, Ordering::Release);
//             eaca.store(0, Ordering::Release);
            
//             while reqsinnersetup.len() < NUM_ARGS {
//                 let l = Layout::from_size_align(randdist_reqsiz(&mut rinner), 1).unwrap();
//                 reqsinnersetup.push(l);
//             }
//         };

//         let f = |()| {
//             dummy_func()
//             // let mut reqsinnerf = reqsouter.borrow_mut();
//             // let _l = black_box(reqsinnerf.pop().unwrap());
//             //unsafe { sm.alloc(l) };
//         };

//         let mut g = c.benchmark_group("g");
//     //xxx    g.sampling_mode(criterion::SamplingMode::Linear);
//         g.bench_function("alloc_rand", |b| b.iter_batched(setup, f, BatchSize::SmallInput));
//     }

//     fn help_bench_alloc_x_bytes(bytes: usize, fnname: &str) {
//         let mut c = plat::make_criterion();

//         let sm = Smalloc::new();
//         sm.idempotent_init().unwrap();

//         let saved_thread_areanum = get_thread_num();

//         const NUM_ARGS: usize = 100_000;
//         let reqsouter = RefCell::new(Vec::with_capacity(NUM_ARGS));

//         let setup = || {
//             let areanum = get_thread_num();
//             assert_eq!(areanum, saved_thread_areanum);
//             let mut reqsinnersetup = reqsouter.borrow_mut();
            
//             // reset the reqs vec
//             reqsinnersetup.clear();

//             // reset the free lists and eacs for all three size classes
//             for smallslabnum in 0..NUM_SMALL_SCS {
//                 let flha = sm.get_atomicu64(small_flh_offset(smallslabnum, areanum));
//                 let eaca = sm.get_atomicu64(small_eac_offset(smallslabnum, areanum));
//                 flha.store(0, Ordering::Release);
//                 eaca.store(0, Ordering::Release);
//             }

//             for mediumslabnum in 0..NUM_MEDIUM_SCS {
//                 let flha = sm.get_atomicu64(medium_flh_offset(mediumslabnum));
//                 let eaca = sm.get_atomicu64(medium_eac_offset(mediumslabnum));
//                 flha.store(0, Ordering::Release);
//                 eaca.store(0, Ordering::Release);
//             }

//             let flha = sm.get_atomicu64(large_flh_offset());
//             let eaca = sm.get_atomicu64(large_eac_offset());
//             flha.store(0, Ordering::Release);
//             eaca.store(0, Ordering::Release);
            
//             let l: Layout = Layout::from_size_align(bytes, 1).unwrap();
//             while reqsinnersetup.len() < NUM_ARGS {
//                 reqsinnersetup.push(l);
//             }
//         };

//         let f = |()| {
//             let mut reqsinnerf = reqsouter.borrow_mut();
//             let l = reqsinnerf.pop().unwrap();
//             unsafe { sm.alloc(l) };
//         };

//         c.bench_function(fnname, |b| b.iter_batched(setup, f, BatchSize::SmallInput));
//     }

//     #[test]
//     fn bench_alloc_1_byte() {
//         help_bench_alloc_x_bytes(1, "alloc_1_byte");
//     }
    
//     #[test]
//     fn bench_alloc_2_bytes() {
//         help_bench_alloc_x_bytes(2, "alloc_2_bytes");
//     }
    
//     #[test]
//     fn bench_alloc_3_bytes() {
//         help_bench_alloc_x_bytes(2, "alloc_3_bytes");
//     }
    
//     #[test]
//     fn bench_alloc_4_bytes() {
//         help_bench_alloc_x_bytes(2, "alloc_4_bytes");
//     }
    
//     #[test]
//     fn bench_alloc_5_bytes() {
//         help_bench_alloc_x_bytes(2, "alloc_5_bytes");
//     }
    
//     #[test]
//     fn bench_alloc_6_bytes() {
//         help_bench_alloc_x_bytes(2, "alloc_6_bytes");
//     }
    
//     #[test]
//     fn bench_alloc_7_bytes() {
//         help_bench_alloc_x_bytes(2, "alloc_7_bytes");
//     }
    
//     #[test]
//     fn bench_alloc_8_bytes() {
//         help_bench_alloc_x_bytes(2, "alloc_8_bytes");
//     }
    
//     #[test]
//     fn bench_alloc_9_bytes() {
//         help_bench_alloc_x_bytes(2, "alloc_9_bytes");
//     }
    
//     #[test]
//     fn bench_alloc_10_bytes() {
//         help_bench_alloc_x_bytes(2, "alloc_10_bytes");
//     }
    
//     #[test]
//     fn bench_alloc_16_bytes() {
//         help_bench_alloc_x_bytes(2, "alloc_16_bytes");
//     }
    
//     #[test]
//     fn bench_alloc_32_bytes() {
//         help_bench_alloc_x_bytes(2, "alloc_32_bytes");
//     }
    
//     #[test]
//     fn bench_alloc_64_bytes() {
//         help_bench_alloc_x_bytes(2, "alloc_64_bytes");
//     }
    
//     #[test]
//     fn bench_alloc_128_bytes() {
//         help_bench_alloc_x_bytes(2, "alloc_128_bytes");
//     }
    
//     #[test]
//     fn bench_alloc_256_bytes() {
//         help_bench_alloc_x_bytes(2, "alloc_256_bytes");
//     }
    
//     #[test]
//     fn bench_alloc_512_bytes() {
//         help_bench_alloc_x_bytes(2, "alloc_512_bytes");
//     }
    
//     #[test]
//     fn bench_alloc_1024_bytes() {
//         help_bench_alloc_x_bytes(2, "alloc_1024_bytes");
//     }
    
//     #[test]
//     fn bench_alloc_2048_bytes() {
//         help_bench_alloc_x_bytes(2, "alloc_2048_bytes");
//     }
    
//     #[test]
//     fn bench_alloc_4096_bytes() {
//         help_bench_alloc_x_bytes(2, "alloc_4096_bytes");
//     }
    
//     #[test]
//     fn bench_alloc_8192_bytes() {
//         help_bench_alloc_x_bytes(2, "alloc_8192_bytes");
//     }
    
//     #[test]
//     fn bench_alloc_16384_bytes() {
//         help_bench_alloc_x_bytes(2, "alloc_16384_bytes");
//     }
    
//     #[test]
//     fn bench_alloc_32768_bytes() {
//         help_bench_alloc_x_bytes(2, "alloc_32768_bytes");
//     }
    
//     #[test]
//     fn bench_alloc_65536_bytes() {
//         help_bench_alloc_x_bytes(2, "alloc_65536_bytes");
//     }
    
//     #[test]
//     fn bench_alloc_131072_bytes() {
//         help_bench_alloc_x_bytes(2, "alloc_131072_bytes");
//     }
    
//     use std::cell::RefCell;
//     #[test]
//     fn bench_dealloc() {
//         let mut c = plat::make_criterion();

//         let sm = Smalloc::new();
//         sm.idempotent_init().unwrap();

//         let saved_thread_areanum = get_thread_num();
//         let router = RefCell::new(StdRng::seed_from_u64(0));

//         const NUM_ARGS: usize = 15_000;
//         let allocsouter = RefCell::new(Vec::with_capacity(NUM_ARGS));

//         let setup = || {
//             let areanum = get_thread_num();
//             assert_eq!(areanum, saved_thread_areanum);
//             let mut rinner = router.borrow_mut();
//             let mut allocsinnersetup = allocsouter.borrow_mut();

//             // reset the allocs vec
//             allocsinnersetup.clear();

//             // reset the free lists and eacs for all three size classes

//             for smallslabnum in 0..NUM_SMALL_SCS {
//                 let flha = sm.get_atomicu64(small_flh_offset(smallslabnum, areanum));
//                 flha.store(0, Ordering::Release);
//                 let eaca = sm.get_atomicu64(small_eac_offset(smallslabnum, areanum));
//                 eaca.store(0, Ordering::Release);
//             }

//             for mediumslabnum in 0..NUM_MEDIUM_SCS {
//                 let flha = sm.get_atomicu64(medium_flh_offset(mediumslabnum));
//                 flha.store(0, Ordering::Release);
//                 let eaca = sm.get_atomicu64(medium_eac_offset(mediumslabnum));
//                 eaca.store(0, Ordering::Release);
//             }
            
//             let flha = sm.get_atomicu64(large_flh_offset());
//             flha.store(0, Ordering::Release);
//             let eaca = sm.get_atomicu64(large_eac_offset());
//             eaca.store(0, Ordering::Release);
            
//             while allocsinnersetup.len() < NUM_ARGS {
//                 let l = Layout::from_size_align(randdist_reqsiz(&mut rinner), 1).unwrap();
//                 allocsinnersetup.push((unsafe { sm.alloc(l) }, l));
//             }

//             allocsinnersetup.shuffle(&mut rinner);
//         };

//         let f = |()| {
//             let mut allocsinnerf = allocsouter.borrow_mut();
//             let (p, l) = allocsinnerf.pop().unwrap();
//             unsafe { sm.dealloc(p, l) };
//         };

//         let mut g = c.benchmark_group("g");
//         //xxxg.sampling_mode(criterion::SamplingMode::Linear);
//         g.bench_function("dealloc", |b| b.iter_batched(setup, f, BatchSize::SmallInput));
//     }

//     #[test]
//     fn cache_behavior_1_1() {
//         help_bench_many_accesses("bench_1_1", 1);
//     }

//     #[test]
//     fn cache_behavior_1_2() {
//         help_bench_many_accesses("bench_1_2", 2);
//     }

//     #[test]
//     fn cache_behavior_1_3() {
//         help_bench_many_accesses("bench_1_3", 3);
//     }

//     #[test]
//     fn cache_behavior_1_4() {
//         help_bench_many_accesses("bench_1_4", 4);
//     }

//     #[test]
//     fn cache_behavior_1_5() {
//         help_bench_many_accesses("bench_1_5", 5);
//     }

//     #[test]
//     fn cache_behavior_1_6() {
//         help_bench_many_accesses("bench_1_6", 6);
//     }

//     #[test]
//     fn cache_behavior_1_8() {
//         help_bench_many_accesses("bench_1_8", 8);
//     }

//     #[test]
//     fn cache_behavior_1_9() {
//         help_bench_many_accesses("bench_1_9", 9);
//     }

//     #[test]
//     fn cache_behavior_1_10() {
//         help_bench_many_accesses("bench_1_10", 10);
//     }

//     #[test]
//     fn cache_behavior_1_16() {
//         help_bench_many_accesses("bench_1_16", 16);
//     }

//     #[test]
//     fn cache_behavior_1_32() {
//         help_bench_many_accesses("bench_1_32", 32);
//     }

//     #[test]
//     fn cache_behavior_1_64() {
//         help_bench_many_accesses("bench_1_64", 64);
//     }

//     #[test]
//     fn cache_behavior_1_128() {
//         help_bench_many_accesses("bench_1_128", 128);
//     }

//     #[test]
//     fn cache_behavior_1_256() {
//         help_bench_many_accesses("bench_1_256", 256);
//     }

//     #[test]
//     fn cache_behavior_1_512() {
//         help_bench_many_accesses("bench_1_512", 512);
//     }

//     #[test]
//     fn cache_behavior_1_1024() {
//         help_bench_many_accesses("bench_1_1024", 1024);
//     }

//     #[test]
//     fn cache_behavior_1_2048() {
//         help_bench_many_accesses("bench_1_2048", 2048);
//     }

//     #[test]
//     fn cache_behavior_1_4096() {
//         help_bench_many_accesses("bench_1_4096", 4096);
//     }

//     #[test]
//     fn cache_behavior_1_8192() {
//         help_bench_many_accesses("bench_1_8192", 8192);
//     }

//     #[test]
//     fn cache_behavior_1_16384() {
//         help_bench_many_accesses("bench_1_16384", 16384);
//     }

//     #[test]
//     fn cache_behavior_1_32768() {
//         help_bench_many_accesses("bench_1_32768", 32768);
//     }

//     use std::cmp::min;

//     /// This is intended to measure the effect of packing many allocations into few cache lines.
//     fn help_bench_many_accesses(fnname: &str, alloc_size: usize) {
//         let mut c = plat::make_criterion();

//         let sm = Smalloc::new();
//         sm.idempotent_init().unwrap();

//         const MEM_TO_USE: usize = CACHE_SIZE * 127 + 1_000_000;
//         let max_num_args = (MEM_TO_USE / alloc_size).next_multiple_of(CACHE_LINE_SIZE);
//         let max_num_slots = if alloc_size <= slotsize(NUM_SMALL_PLUS_MEDIUM_SLABS - 1) {
//             NUM_MEDIUM_SLOTS
//         } else {
//             NUM_LARGE_SLOTS
//         };
//         let num_args = min(max_num_args, max_num_slots);
        
//         assert!(num_args <= NUM_MEDIUM_SLOTS, "{num_args} <= {NUM_MEDIUM_SLOTS}, MEM_TO_USE: {MEM_TO_USE}, CACHE_SIZE: {CACHE_SIZE}, CACHE_LINE_SIZE: {CACHE_LINE_SIZE}, alloc_size: {alloc_size}");

//         // Okay now we need a jump which is relatively prime to num_args / CACHE_LINE_SIZE (so that
//         // we visit all the allocations in a permutation) and >= 1/2 of (num_args / CACHE_LINE_SIZE)
//         // (so that we get away from any linear pre-fetching).
//         let x = num_args / CACHE_LINE_SIZE;
//         let mut jump = x / 2;
//         while x.gcd(jump) != 1 {
//             jump += 1;
//         }

//         let mut r = StdRng::seed_from_u64(0);

//         let mut allocs = Vec::with_capacity(num_args);

//         let l = Layout::from_size_align(alloc_size, 1).unwrap();
//         while allocs.len() < num_args {
//             // Allocate CACHE_LINE_SIZE allocations, take their pointers, shuffle the pointers, and
//             // append them to allocs.
//             let mut batch_of_allocs = Vec::new();
//             for _x in 0..CACHE_LINE_SIZE {
//                 batch_of_allocs.push(unsafe { sm.alloc(l) });
//             }
//             batch_of_allocs.shuffle(&mut r);
//             allocs.extend(batch_of_allocs);
//         };
//         //        eprintln!("num_args: {}, alloc_size: {}, total alloced: {}, jump: {}", num_args.separate_with_commas(), alloc_size.separate_with_commas(), (alloc_size * num_args).separate_with_commas(), jump.separate_with_commas());

//         let mut a = 0;
//         let mut i = 0;
//         c.bench_function(fnname, |b| b.iter(|| {
//             // Now CACHE_LINE_SIZE times in a row we're going to read one byte from the allocation
//             // pointed to by each successive pointer. The theory is that when those successive
//             // allocations are packed into cache lines, we should be able to do these
//             // CACHE_LINE_SIZE reads more quickly than when those successive allocations are spread
//             // out over many cache lines.
            
//             // get the next pointer
//             let x = allocs[i % allocs.len()];

//             // read a byte from it
//             let b = unsafe { *x };

//             // accumulate its value
//             a ^= b as usize;

//             // go to the next pointer
//             i += 1;
//         }));
//     }

// // xxx teach criterion config that these take more threads
//     // #[test]
//     // fn bench_threads_1_large_alloc_dealloc_x() {
//     //     let mut c = plat::make_criterion();

//     //     let mut i = 0;
//     //     c.bench_function("size_to_sc_lzcnt_min", |b| b.iter(|| {
//     //         crate::tests::help_test_multithreaded(1, 100, SizeClass::Large, true, true, false);
//     //         i += 1;
//     //     }));

//     // }

//     // #[test]
//     // fn bench_threads_2_large_alloc_dealloc_x() {
//     //     let mut c = plat::make_criterion();

//     //     let mut i = 0;
//     //     c.bench_function("size_to_sc_lzcnt_min", |b| b.iter(|| {
//     //         crate::tests::help_test_multithreaded(2, 100, SizeClass::Large, true, true, false);
//     //         i += 1;
//     //     }));

//     // }

//     // #[test]
//     // fn bench_threads_10_large_alloc_dealloc_x() {
//     //     let mut c = plat::make_criterion();

//     //     let mut i = 0;
//     //     c.bench_function("size_to_sc_lzcnt_min", |b| b.iter(|| {
//     //         crate::tests::help_test_multithreaded(10, 100, SizeClass::Large, true, true, false);
//     //         i += 1;
//     //     }));

//     // }

//     // #[test]
//     // fn bench_threads_100_large_alloc_dealloc_x() {
//     //     let mut c = plat::make_criterion();

//     //     let mut i = 0;
//     //     c.bench_function("size_to_sc_lzcnt_min", |b| b.iter(|| {
//     //         crate::tests::help_test_multithreaded(100, 100, SizeClass::Large, true, true, false);
//     //         i += 1;
//     //     }));

//     // }

//     // #[test]
//     // fn bench_threads_1000_large_alloc_dealloc_x() {
//     //     let mut c = plat::make_criterion();

//     //     let mut i = 0;
//     //     c.bench_function("size_to_sc_lzcnt_min", |b| b.iter(|| {
//     //         crate::tests::help_test_multithreaded(1000, 100, SizeClass::Large, true, true, false);
//     //         i += 1;
//     //     }));

//     // }

//     // use std::sync::Arc;
//     // use std::thread;
//     // pub fn help_bench_multithreaded(numthreads: u32, numiters: u32, sc: SizeClass, dealloc: bool, realloc: bool, writes: bool) {
//     //     let sm = Arc::new(Smalloc::new());
//     //     sm.idempotent_init().unwrap();

//     //     let mut handles = Vec::new();
//     //     for _i in 0..numthreads {
//     //         let smc = Arc::clone(&sm);
//     //         handles.push(thread::spawn(move || {
//     //             let r = StdRng::seed_from_u64(0);
//     //             help_test(&smc, numiters, sc, r, dealloc, realloc, writes);
//     //         }));
//     //     }

//     //     for handle in handles {
//     //         handle.join().unwrap();
//     //     }
//     // }

// }

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

    const fn help_pow2_usize(bits: u8) -> usize {
        2usize.pow(bits as u32)
    }
    
    const fn help_pow2_u32(bits: u8) -> u32 {
        2u32.pow(bits as u32)
    }
    
    fn alignedsize_or(size: usize, align: usize) -> usize {
        ((size - 1) | (align - 1)) + 1
    }

    const fn extract_bits_usize(x: usize, start: u8, length: u8) -> usize {
        assert!(((length + start) as u32) < usize::BITS);
        unsafe { x.unchecked_shr(start as u32) & _const_gen_mask_usize(length) }
    }

    #[derive(Copy, Clone, Debug)]
    enum SizeClass {
        Small,
        Medium,
        Large,
    }

    #[test]
    fn slotnum_encode_and_decode_roundtrip() {
        let numslotsbitses = [ NUM_MEDIUM_SLOTS_BITS, NUM_SMALL_SLOTS_BITS, LARGE_SLOT_SIZE_BITS_PLUS_NUM_SLOTS_BITS - SMALLEST_SLOT_SIZE_BITS - NUM_SMALL_SCS - NUM_MEDIUM_SCS ];
        for numslotsbits in numslotsbitses {
            let numslots = help_pow2_u32(numslotsbits);
            let slotnums = [ 0, 1, 2, 3, 4, numslots - 4, numslots - 3, numslots - 2, numslots - 1 ];
            for slotnum1 in slotnums {
                for slotnum2 in slotnums {
                    let ence = Smalloc::encode_next_entry_link(slotnum1, slotnum2, numslotsbits);
                    let dece = Smalloc::decode_next_entry_link(slotnum1, ence, numslotsbits);
                    assert_eq!(slotnum2, dece);
                }
            }
        }
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
    fn one_alloc_slot_large() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let l = Layout::from_size_align(1_000_000, 8).unwrap();
        unsafe { sm.alloc(l) };
    }

    #[test]
    fn a_few_allocs_and_a_dealloc_for_each_small_slab() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        for sc in 0..NUM_SMALL_SCS {
            help_small_alloc_diff_size_and_alignment_singlethreaded(&sm, sc);
        }
    }

    #[test]
    fn a_few_allocs_and_a_dealloc_for_each_medium_slab() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        for sc in NUM_SMALL_SCS .. NUM_SMALL_SCS + NUM_MEDIUM_SCS {
            help_medium_alloc_diff_size_and_alignment_singlethreaded(&sm, sc);
        }
    }

    #[test]
    fn a_few_allocs_and_a_dealloc_for_each_large_slab() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        // Doesn't work for the larger large slab because it overflows them
        for sc in NUM_SMALL_SCS + NUM_MEDIUM_SCS .. NUM_SMALL_SCS + NUM_MEDIUM_SCS + 15 {
            help_large_alloc_diff_size_and_alignment_singlethreaded(&sm, sc);
        }
    }

    #[test]
    fn a_few_allocs_and_a_dealloc_for_the_largest_slab() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let sc = NUM_SMALL_SCS + NUM_MEDIUM_SCS + NUM_LARGE_SCS - 1;
        let smallest = help_pow2_usize(sc + SMALLEST_SLOT_SIZE_BITS - 1) + 1;
        let largest = help_pow2_usize(sc + SMALLEST_SLOT_SIZE_BITS);

        for reqsize in [ smallest, smallest + 1, smallest + 2, largest - 3, largest - 1, largest, ] {
            // Generate alignments
            let mut reqalign = 1;
            loop {
                // Test this size/align combo
                let l = Layout::from_size_align(reqsize, reqalign).unwrap();

                let p1 = unsafe { sm.alloc(l) };

                let (sc1, _, slotnum1) = help_ptr_to_loc(&sm, p1, l);

                assert_eq!(sc1, sc);
                assert_eq!(slotnum1, 0);

                unsafe { sm.dealloc(p1, l) };

                let p2 = unsafe { sm.alloc(l) };

                let (sc2, _, slotnum2) = help_ptr_to_loc(&sm, p2, l);

                assert_eq!(sc2, sc);
                assert_eq!(slotnum2, 0);

                unsafe { sm.dealloc(p2, l) };

                let p3 = unsafe { sm.alloc(l) };

                let (sc3, _, slotnum3) = help_ptr_to_loc(&sm, p3, l);

                assert_eq!(sc3, sc);
                assert_eq!(slotnum3, 0);

                unsafe { sm.dealloc(p3, l) };

                reqalign *= 2;
                if alignedsize_or(reqsize, reqalign) > largest {
                    break;
                };
            }
        }
    }

    /// This reproduces a bug in `platform::vendor::sys_realloc()` /
    /// `_sys_realloc_if_vm_remap_did_what_i_want()` (or possibly in MacOS's `mach_vm_remap()`) that
    /// was uncovered by tests::threads_1_large_alloc_dealloc_realloc_x()
    #[test]
    fn large_realloc_down_realloc_back_up() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        const LARGE_SLOT_SIZE: usize = help_pow2_usize(24);

        let l1 = Layout::from_size_align(LARGE_SLOT_SIZE * 2, 1).unwrap();
        let l2 = Layout::from_size_align(LARGE_SLOT_SIZE, 1).unwrap();

        let p1 = unsafe { sm.alloc(l1) };
        assert!(!p1.is_null());
        let p2 = unsafe { sm.realloc(p1, l1, LARGE_SLOT_SIZE) };
        assert!(!p2.is_null());
        let p3 = unsafe { sm.realloc(p2, l2, LARGE_SLOT_SIZE * 2) };
        assert!(!p3.is_null());
    }

    /// Generate a number of requests (size+alignment) that fit into the given slab and for each
    /// request call help_small_alloc_four_times_singlethreaded()
    fn help_small_alloc_diff_size_and_alignment_singlethreaded(sm: &Smalloc, sc: u8) {
        assert!(sc < NUM_SMALL_SCS);

        let smallest = if sc == 0 {
            1
        } else {
            help_pow2_usize(sc + SMALLEST_SLOT_SIZE_BITS - 1) + 1
        };
        let largest = help_pow2_usize(sc + SMALLEST_SLOT_SIZE_BITS);
        for reqsize in smallest..=largest {
            // Generate alignments
            let mut reqalign = 1;
            loop {
                // Test this size/align combo
                help_small_alloc_four_times_singlethreaded(sm, reqsize, reqalign);
                reqalign *= 2;
                if alignedsize_or(reqsize, reqalign) > largest {
                    break;
                };
            }
        }
    }

    /// Generate a number of requests (size+alignment) that fit into the given medium slab and for
    /// each request call help_medium_alloc_four_times_singlethreaded()
    fn help_medium_alloc_diff_size_and_alignment_singlethreaded(sm: &Smalloc, sc: u8) {
        assert!(sc >= NUM_SMALL_SCS);
        assert!(sc < NUM_SMALL_SCS + NUM_MEDIUM_SCS);

        let smallest = help_pow2_usize(sc + SMALLEST_SLOT_SIZE_BITS - 1) + 1;
        let largest = help_pow2_usize(sc + SMALLEST_SLOT_SIZE_BITS);
        for reqsize in [ smallest, smallest + 1, smallest + 2, largest - 3, largest - 1, largest, ] {
            // Generate alignments
            let mut reqalign = 1;
            loop {
                // Test this size/align combo
                help_medium_alloc_four_times_singlethreaded(sm, reqsize, reqalign);
                reqalign *= 2;
                if alignedsize_or(reqsize, reqalign) > largest {
                    break;
                };
            }
        }
    }

    /// Generate a number of requests (size+alignment) that fit into a large slab and for each
    /// request call help_large_alloc_four_times().
    fn help_large_alloc_diff_size_and_alignment_singlethreaded(sm: &Smalloc, sc: u8) {
        assert!(sc >= NUM_SMALL_SCS + NUM_MEDIUM_SCS);

        const TOTAL_SCS: u8 = NUM_SMALL_SCS + NUM_MEDIUM_SCS + NUM_LARGE_SCS;

        // This doesn't work on sc 35 -- the largest sizeclass -- because there aren't 3 slots you
        // can allocate of that size or larger.
        assert!(sc < TOTAL_SCS - 1);

        let smallest = help_pow2_usize(sc + SMALLEST_SLOT_SIZE_BITS - 1) + 1;
        let largest = help_pow2_usize(sc + SMALLEST_SLOT_SIZE_BITS);
        for reqsize in [ smallest, smallest + 1, smallest + 2, largest - 3, largest - 1, largest, ] {
            // Generate alignments
            let mut reqalign = 1;
            loop {
                // Test this size/align combo
                help_large_alloc_four_times_singlethreaded(sm, reqsize, reqalign);
                reqalign *= 2;
                if alignedsize_or(reqsize, reqalign) > largest {
                    break;
                };
            }
        }
    }

    /// Return the sizeclass, slabnum, and slotnum
    fn help_ptr_to_loc(sm: &Smalloc, ptr: *const u8, layout: Layout) -> (u8, u8, u32) {
        assert!((layout.align() & (layout.align() - 1)) == 0); // alignment must be a power of two
        let alignedsizebits = alignedsize_minus1_bits_lzcnt(layout.size(), layout.align());
        //xxxeprintln!("in help_ptr_to_loc(): alignedsizebits: {alignedsizebits}, size: {}, align: {}", layout.size(), layout.align());
        
        let p_addr = ptr.addr();
        let smbp_addr = sm.get_sm_baseptr();

        assert!((p_addr >= smbp_addr) && (p_addr <= smbp_addr + HIGHEST_SMALLOC_SLOT_ADDR));
        //eprintln!("p_addr: {p_addr:064b}, smbp_addr: {smbp_addr:064b}, HIGHEST_SMALLOC_SLOT_ADDR: {HIGHEST_SMALLOC_SLOT_ADDR:064b}");

        let s_addr = p_addr & SMALLOC_ADDRESS_BITS_MASK;

        let lzc = help_leading_zeros_usize(s_addr);

        assert!(lzc >= UNUSED_MSB_ADDRESS_BITS); // the 19 significant address bits that we masked off
        // assert!(UNUSED_MSB_ADDRESS_BITS == 19); // 19 ? 19 !

        assert!(lzc <= UNUSED_MSB_ADDRESS_BITS + NUM_SMALL_SCS + NUM_MEDIUM_SCS);
        // assert!(UNUSED_MSB_ADDRESS_BITS + NUM_SMALL_SCS + NUM_MEDIUM_SCS == 29); // 29 ? 29 !
        // assert!(UNUSED_MSB_ADDRESS_BITS + NUM_SMALL_SCS == 24); // 24 ? 24 !

        if lzc > UNUSED_MSB_ADDRESS_BITS + NUM_SMALL_SCS {
            let sizeclass = 34 - lzc; // symbolify the 34??

            let slotsizebits = sizeclass + SMALLEST_SLOT_SIZE_BITS;
            assert!(slotsizebits >= alignedsizebits);

            assert!(help_trailing_zeros_usize(s_addr) >= slotsizebits);

            assert!(slotsizebits + NUM_MEDIUM_SCS < usize::BITS as u8);
            let mediumslotnum = extract_bits_usize(s_addr, slotsizebits, NUM_MEDIUM_SLOTS_BITS) as u32;

            (sizeclass, 0, mediumslotnum)
        } else if lzc == UNUSED_MSB_ADDRESS_BITS + NUM_SMALL_SCS {
            assert!(alignedsizebits <= 2);

            assert!(help_trailing_zeros_usize(s_addr) >= SMALLEST_SLOT_SIZE_BITS);

            let smallslotnum = const_shr_usize_u32(s_addr & SIZECLASS_0_SLOTNUM_MASK, SMALLEST_SLOT_SIZE_BITS);

            let slabnum = const_shr_usize_u8(s_addr & !SIZECLASS_0_SC_INDICATOR_MASK, NUM_SMALL_SLOTS_BITS + SMALLEST_SLOT_SIZE_BITS);

            (0, slabnum, smallslotnum)
        } else if lzc > UNUSED_MSB_ADDRESS_BITS {
            let sizeclass = UNUSED_MSB_ADDRESS_BITS + NUM_SMALL_SCS - lzc;

            let slotsizebits = sizeclass + SMALLEST_SLOT_SIZE_BITS;
            assert!(slotsizebits >= alignedsizebits);

            assert!(help_trailing_zeros_usize(s_addr) >= slotsizebits);

            let slotnummask = const_shl_u32_usize(HIGHEST_SMALL_SLOTNUM, slotsizebits);

            let smallslotnum = const_shr_usize_u32(s_addr & slotnummask, slotsizebits);
            
            let slabnummask = const_shl_u32_usize(SMALL_SLABNUM_MASK, SMALLEST_SLOT_SIZE_BITS + sizeclass + NUM_SMALL_SLOTS_BITS);
            let slabnum = const_shr_usize_u8(s_addr & slabnummask, slotsizebits + NUM_SMALL_SLOTS_BITS);

            (sizeclass, slabnum, smallslotnum)
        } else {
            let sizeclass = const_shr_usize_u8(s_addr & LARGE_SLAB_SC_MASK, LARGE_SLOT_SIZE_BITS_PLUS_NUM_SLOTS_BITS) + NUM_SMALL_SCS + NUM_MEDIUM_SCS;

            let slotsizebits = sizeclass + SMALLEST_SLOT_SIZE_BITS;
            assert!(slotsizebits >= alignedsizebits, "{slotsizebits} >= {alignedsizebits}; sizeclass: {sizeclass}");

            assert!(help_trailing_zeros_usize(s_addr) >= slotsizebits);

            let numslotsbits = LARGE_SLOT_SIZE_BITS_PLUS_NUM_SLOTS_BITS - slotsizebits;

            let slotnummask: usize = const_shl_u32_usize(const_gen_mask_u32(numslotsbits), slotsizebits);

            let largeslotnum = const_shr_usize_u32(s_addr & slotnummask, slotsizebits);

            (sizeclass, 0, largeslotnum)
        }
    }
        
    // /// Return the slab base pointer and free list head pointer for this slab.
    // fn help_slab_to_ptrs(sm: &Smalloc, sc: u32, slabnum: usize) -> (*mut u8, *mut u8) {
    //     assert!(sc < NUM_SMALL_SCS + NUM_MEDIUM_SCS + NUM_LARGE_SCS);
    //     assert!(if sc < NUM_SMALL_SCS { slabnum < one_shl(NUM_SMALL_SLABS_BITS) } else { slabnum == 0 });

    //     let smbp = sm.get_sm_baseptr();

    //     if sc == 0 {
    //         let slabbp = smbp | SIZECLASS_0_SC_INDICATOR_MASK | const_shl_usize(slabnum, SMALLEST_SLOT_SIZE_BITS);
    //         let flh_addr = (smbp | SIZECLASS_0_SLOTNUM_MASK) & !SIZECLASS_0_SLOTNUM_LSB_MASK;

    //         (slabbp, flh_addr)
    //     } else if sc < NUM_SMALL_SCS {
    //         let slabbp = smbp | const_shl_usize(SIZECLASS_0_SC_INDICATOR_MASK, sc) | const_shl_usize(slabnum, sc + SMALLEST_SLOT_SIZE_BITS);
    //         let slotnum_mask = const_shl_usize(SIZECLASS_0_SLOTNUM_MASK, sc);
    //         let flh_addr = slabbp | slotnum_mask;

    //         (slabbp, flh_addr)
    //     } else if sc < NUM_SMALL_SCS + NUM_MEDIUM_SCS {
    //         let slabbp = smbp | const_shl_usize(SIZECLASS_5_SC_INDICATOR_MASK, sc - 5);
    //         let slotnum_mask = const_shl_usize(SIZECLASS_5_SLOTNUM_MASK, sc - 5);
    //         let flh_addr = slabbp | slotnum_mask;

    //         (slabbp, flh_addr)
    //     } else {
    //         let slotsizebits = sc + SMALLEST_SLOT_SIZE_BITS;
    //         let slotnumbits = LARGE_SLOT_SIZE_BITS_PLUS_NUM_SLOTS_BITS - slotsizebits;
    //         let largesc = sc - NUM_SMALL_SCS - NUM_MEDIUM_SCS;
    //         let slabbp = smbp | const_shl_usize(largesc, LARGE_SLOT_SIZE_BITS_PLUS_NUM_SLOTS_BITS);
    //         let flh_addr = slabbp | const_shl_usize(gen_mask(slotnumbits), slotsizebits);

    //         (slabbp, flh_addr)
    //     }
    // }

    #[test]
    fn test_alignedsize_minus1_bits_lzcnt() {
        assert_eq!(alignedsize_minus1_bits_lzcnt(33, 63), 6);
        assert_eq!(alignedsize_minus1_bits_lzcnt(33, 64), 6);
        assert_eq!(alignedsize_minus1_bits_lzcnt(33, 65), 7);
    }

    /// Allocate this size+align three times, then free the middle one, then allocate a fourth time,
    /// then assert that the fourth slot is the same as the second slot. Also asserts that the sc is
    /// small and the slabareanum is the same as this thread num.
    fn help_small_alloc_four_times_singlethreaded(sm: &Smalloc, reqsize: usize, reqalign: usize) {
        assert!(reqsize > 0);
        assert!(reqsize <= help_pow2_usize(SMALLEST_SLOT_SIZE_BITS + NUM_SMALL_SCS - 1));
        assert!(reqalign > 0);

        let l = Layout::from_size_align(reqsize, reqalign).unwrap();

        let orig_slabareanum = get_thread_num() as u8;

        let p1 = unsafe { sm.alloc(l) };
        assert!(!p1.is_null());

        let (sc1, slabnum1, slotnum1) = help_ptr_to_loc(sm, p1, l);
        assert!(sc1 < NUM_SMALL_SCS, "should have returned a small slot, returns sc1: {sc1}, l: {l:?}");
        assert_eq!(slabnum1, orig_slabareanum);

        let p2 = unsafe { sm.alloc(l) };
        assert!(!p2.is_null());

        let (sc2, slabnum2, slotnum2) = help_ptr_to_loc(sm, p2, l);
        assert!(sc2 < NUM_SMALL_SCS, "should have returned a small slot");
        assert_eq!(slabnum2, slabnum1, "p1: {p1:?}, p2: {p2:?}, slabnum1: {slabnum1}, slabnum2: {slabnum2}, slotnum1: {slotnum1}, slotnum2: {slotnum2}");
        assert_eq!(slabnum2, orig_slabareanum);
        assert_eq!(slotnum2, slotnum1 + 1);

        let p3 = unsafe { sm.alloc(l) };
        assert!(!p3.is_null());

        let (sc3, slabnum3, slotnum3) = help_ptr_to_loc(sm, p3, l);
        assert!(sc3 < NUM_SMALL_SCS, "should have returned a small slot");
        assert_eq!(slabnum3, slabnum1);
        assert_eq!(slabnum3, orig_slabareanum);
        assert_eq!(slotnum3, slotnum2 + 1);

        // Now free the middle one.
        unsafe { sm.dealloc(p2, l) };

        // And allocate another one.
        let p4 = unsafe { sm.alloc(l) };
        assert!(!p4.is_null());

        let (sc4, slabnum4, slotnum4) = help_ptr_to_loc(sm, p4, l);
        assert!(sc4 < NUM_SMALL_SCS, "should have returned a small slot");
        assert_eq!(slabnum4, slabnum1);
        assert_eq!(slabnum4, orig_slabareanum);

        // It should have allocated slot num 2 again
        assert_eq!(slotnum4, slotnum2);
    }

    // xxx consider reducing the code size of these tests...
    
    /// Allocate this size+align three times, then free the middle one, then allocate a fourth time,
    /// then assert that the fourth slot is the same as the second slot. Also asserts that the sc is
    /// medium.
    fn help_medium_alloc_four_times_singlethreaded(sm: &Smalloc, reqsize: usize, reqalign: usize) {
        assert!(reqsize > help_pow2_usize(SMALLEST_SLOT_SIZE_BITS + NUM_SMALL_SCS - 1), "reqsize: {reqsize}");
        assert!(reqsize <= help_pow2_usize(SMALLEST_SLOT_SIZE_BITS + NUM_SMALL_SCS + NUM_MEDIUM_SCS - 1));
        assert!(reqalign > 0);

        let l = Layout::from_size_align(reqsize, reqalign).unwrap();
        
        let p1 = unsafe { sm.alloc(l) };

        assert!(!p1.is_null());
        let (sc1, _, slotnum1) = help_ptr_to_loc(sm, p1, l);
        assert!(sc1 >= NUM_SMALL_SCS, "should have returned a medium slot");
        assert!(sc1 < NUM_SMALL_SCS + NUM_MEDIUM_SCS, "should have returned a medium slot");

        let p2 = unsafe { sm.alloc(l) };
        assert!(!p2.is_null());

        let (sc2, _, slotnum2) = help_ptr_to_loc(sm, p2, l);
        assert!(sc2 >= NUM_SMALL_SCS, "should have returned a medium slot");
        assert!(sc2 < NUM_SMALL_SCS + NUM_MEDIUM_SCS, "should have returned a medium slot");
        assert_eq!(slotnum2, slotnum1 + 1);

        let p3 = unsafe { sm.alloc(l) };
        assert!(!p3.is_null());

        let (sc3, _, slotnum3) = help_ptr_to_loc(sm, p3, l);
        assert!(sc3 >= NUM_SMALL_SCS, "should have returned a medium slot");
        assert!(sc3 < NUM_SMALL_SCS + NUM_MEDIUM_SCS, "should have returned a medium slot");
        assert_eq!(slotnum3, slotnum2 + 1);

        // Now free the middle one.
        unsafe { sm.dealloc(p2, l) };

        //xxxeprintln!("in test code firstfreeslotnum: {}", help_get_flh_singlehthreaded(sm.get_sm_baseptr(), sc2));

        // And allocate another one.
        let p4 = unsafe { sm.alloc(l) };
        assert!(!p4.is_null());

        let (sc4, _, slotnum4) = help_ptr_to_loc(sm, p4, l);
        assert!(sc4 >= NUM_SMALL_SCS, "should have returned a medium slot");
        assert!(sc4 < NUM_SMALL_SCS + NUM_MEDIUM_SCS, "should have returned a medium slot");

        // It should have allocated slot num 2 again
        assert_eq!(sc4, sc2, "p1: {p1:?}, sc1: {sc1:?}, slotnum1: {slotnum1:?}, p2: {p2:?}, sc2: {sc2:?}, slotnum2: {slotnum2:?}, p3: {p3:?}, sc3: {sc3:?}, slotnum3: {slotnum3:?}, p4: {p4:?}, sc4: {sc4:?}, slotnum4: {slotnum4:?}");
        assert_eq!(slotnum4, slotnum2, "p1: {p1:?}, sc1: {sc1:?}, slotnum1: {slotnum1:?}, p2: {p2:?}, sc2: {sc2:?}, slotnum2: {slotnum2:?}, p3: {p3:?}, sc3: {sc3:?}, slotnum3: {slotnum3:?}, p4: {p4:?}, sc4: {sc4:?}, slotnum4: {slotnum4:?}");
    }

    /// Allocate this size+align three times, then free the middle one, then allocate a fourth time,
    /// then assert that the fourth slot is the same as the second slot. Also asserts that the sc is
    /// large.
    fn help_large_alloc_four_times_singlethreaded(sm: &Smalloc, reqsize: usize, reqalign: usize) {
        assert!(reqsize > help_pow2_usize(SMALLEST_SLOT_SIZE_BITS + NUM_SMALL_SCS + NUM_MEDIUM_SCS - 1));

        // This test can't test sc 35 because you can't allocate 3 slots of that size.
        assert!(reqsize < help_pow2_usize(SMALLEST_SLOT_SIZE_BITS + NUM_SMALL_SCS + NUM_MEDIUM_SCS + NUM_LARGE_SCS - 1));

        let alignedsizebits = alignedsize_minus1_bits_lzcnt(reqsize, reqalign);

        //xxxeprintln!("xxx 0 reqsize: {reqsize}, reqalign: {reqalign}, alignedsizebits: {alignedsizebits}");

        let l = Layout::from_size_align(reqsize, reqalign).unwrap();

        let _sc = alignedsizebits - SMALLEST_SLOT_SIZE_BITS;

        //xxxeprintln!("in help_large_alloc_four_times_singlethreaded() 1 code firstfreeslotnum: {}", help_get_flh_singlehthreaded(sm.get_sm_baseptr(), sc));

        let p1 = unsafe { sm.alloc(l) };
        assert!(!p1.is_null());

        //xxxeprintln!("in help_large_alloc_four_times_singlethreaded() 2 code firstfreeslotnum: {}", help_get_flh_singlehthreaded(sm.get_sm_baseptr(), sc));

        let (sc1, _, slotnum1) = help_ptr_to_loc(sm, p1, l);
        assert_eq!(sc1 + SMALLEST_SLOT_SIZE_BITS, alignedsizebits);
        assert!(sc1 >= NUM_SMALL_SCS + NUM_MEDIUM_SCS, "should have returned a large slot");
        assert!(sc1 < NUM_SMALL_SCS + NUM_MEDIUM_SCS + NUM_LARGE_SCS - 1, "should have returned a large slot");

        let p2 = unsafe { sm.alloc(l) };
        assert!(!p2.is_null());

        let (sc2, _, slotnum2) = help_ptr_to_loc(sm, p2, l);
        assert_eq!(sc2 + SMALLEST_SLOT_SIZE_BITS, alignedsizebits);
        assert!(sc2 >= NUM_SMALL_SCS + NUM_MEDIUM_SCS, "should have returned a large slot");
        assert!(sc2 < NUM_SMALL_SCS + NUM_MEDIUM_SCS + NUM_LARGE_SCS - 1, "should have returned a large slot");
        assert_eq!(slotnum2, slotnum1 + 1, "sc1: {sc1}, sc2: {sc2}, reqsize: {reqsize}, reqalign: {reqalign}, p1: {:064b}, p2: {:064b}", p1 as usize, p2 as usize);

        let p3 = unsafe { sm.alloc(l) };
        assert!(!p3.is_null());

        let (sc3, _, slotnum3) = help_ptr_to_loc(sm, p3, l);
        assert!(sc3 >= NUM_SMALL_SCS + NUM_MEDIUM_SCS, "should have returned a large slot");
        assert!(sc3 < NUM_SMALL_SCS + NUM_MEDIUM_SCS + NUM_LARGE_SCS - 1, "should have returned a large slot");
        assert_eq!(slotnum3, slotnum2 + 1);

        // Now free the middle one.
        unsafe { sm.dealloc(p2, l) };

        // And allocate another one.
        let p4 = unsafe { sm.alloc(l) };
        assert!(!p4.is_null());

        let (sc4, _, slotnum4) = help_ptr_to_loc(sm, p4, l);
        assert!(sc4 >= NUM_SMALL_SCS + NUM_MEDIUM_SCS, "should have returned a large slot");
        assert!(sc4 < NUM_SMALL_SCS + NUM_MEDIUM_SCS + NUM_LARGE_SCS - 1, "should have returned a large slot");

        // It should have allocated slot num 2 again
        assert_eq!(slotnum4, slotnum2);
    }

    #[test]
    fn test_alloc_1_byte_then_dealloc() {
        let sm = Smalloc::new();
        let layout = Layout::from_size_align(1, 1).unwrap();
        let p = unsafe { sm.alloc(layout) };
        assert!(!p.is_null());
        unsafe { sm.dealloc(p, layout) };
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

    //xxx add newtypiness

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
                    assert!(m.contains(&(p, lt)), "thread: {:>3}, {:?} {}-{}", get_thread_num(), p, l.size(), l.align());
                    m.remove(&(p, lt));
                    unsafe { sm.dealloc(p, lt) };
                }
            } else {
                // Malloc
                let p = unsafe { sm.alloc(l) };
                assert!(!p.is_null(), "l: {l:?}");
                assert!(!m.contains(&(p, l)), "thread: {:>3}, {:?} {}-{}", get_thread_num(), p, l.size(), l.align());
                m.insert((p, l));
                ps.push((p, l));
            }
        }
    }

    fn help_test_alloc(sm: &Smalloc, _numiters: u32, l: Layout, _r: &mut StdRng) {
        let p = unsafe { sm.alloc(l) };
        assert!(!p.is_null());
    }

    fn help_slotsize(sc: u8) -> usize {
        help_pow2_usize(sc + SMALLEST_SLOT_SIZE_BITS)
    }

    fn help_test(sm: &Smalloc, numiters: u32, sc: SizeClass, mut r: StdRng,  dealloc: bool, realloc: bool, writes: bool) {
        let l = match sc {
            SizeClass::Small => {
                Layout::from_size_align(help_slotsize(0), 1).unwrap()
            }
            SizeClass::Medium => {
                Layout::from_size_align(help_slotsize(NUM_SMALL_SCS + NUM_MEDIUM_SCS - 1), 1).unwrap()
            }
            SizeClass::Large => {
                Layout::from_size_align(help_slotsize(NUM_SMALL_SCS + NUM_MEDIUM_SCS), 1).unwrap()
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
                assert!(m.contains(&(p, lt)), "thread: {:>3}, {:?} {}-{}", get_thread_num(), p, l.size(), l.align());
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
                assert!(!p.is_null());
                unsafe { std::ptr::copy_nonoverlapping(BYTES3.as_ptr(), p, min(BYTES3.len(), l.size())) };
                assert!(!m.contains(&(p, l)), "thread: {:>3}, {:?} {}-{}", get_thread_num(), p, l.size(), l.align());
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
                    assert!(m.contains(&(p, lt)), "thread: {:>3}, {:?} {}-{}", get_thread_num(), p, lt.size(), lt.align());
                    m.remove(&(p, lt));
                    unsafe { sm.dealloc(p, lt) };
                }
            } else if coin == 1 {
                // Malloc
                let lt = ls.choose(r).unwrap();
                let p = unsafe { sm.alloc(*lt) };
                assert!(!p.is_null());
                assert!(!m.contains(&(p, *lt)), "thread: {:>3}, {:?} {}-{}", get_thread_num(), p, lt.size(), lt.align());
                m.insert((p, *lt));
                ps.push((p, *lt));
            } else {
                // Realloc
                if !ps.is_empty() {
                    let i = r.random_range(0..ps.len());
                    let (p, lt) = ps.remove(i);
                    assert!(!p.is_null());
                    assert!(m.contains(&(p, lt)), "thread: {:>3}, {:?} {}-{}", get_thread_num(), p, lt.size(), lt.align());
                    m.remove(&(p, lt));

                    let newlt = ls.choose(r).unwrap();
                    let newp = unsafe { sm.realloc(p, lt, newlt.size()) };
                    assert!(!newp.is_null());

                    assert!(!m.contains(&(newp, *newlt)), "thread: {:>3}, p: {:?}, newp: {:?} {}", get_thread_num(), p, newp, newlt.size());
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
                    assert!(m.contains(&(p, lt)), "thread: {:>3}, {:?} {}-{}", get_thread_num(), p, lt.size(), lt.align());
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
                assert!(!p.is_null());
                unsafe { std::ptr::copy_nonoverlapping(BYTES3.as_ptr(), p, min(BYTES3.len(), lt.size())) };
                assert!(!m.contains(&(p, *lt)), "thread: {:>3}, {:?} {}-{}", get_thread_num(), p, lt.size(), lt.align());
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
                    assert!(!p.is_null());
                    assert!(m.contains(&(p, lt)), "thread: {:>3}, {:?} {}-{}", get_thread_num(), p, lt.size(), lt.align());
                    m.remove(&(p, lt));

                    let newlt = ls.choose(&mut r).unwrap();
                    let newp = unsafe { sm.realloc(p, lt, newlt.size()) };
                    unsafe { std::ptr::copy_nonoverlapping(BYTES5.as_ptr(), newp, min(BYTES5.len(), lt.size())) };

                    assert!(!m.contains(&(newp, *newlt)), "thread: {:>3}, {:?} {}-{}", get_thread_num(), newp, newlt.size(), newlt.align());
                    m.insert((newp, *newlt));
                    ps.push((newp, *newlt));

                    // Write to a random allocation...
                    let (po, lto) = ps.choose(&mut r).unwrap();
                    unsafe { std::ptr::copy_nonoverlapping(BYTES6.as_ptr(), *po, min(BYTES6.len(), lto.size())) };
                }
            }
        }
    }
    
    fn help_set_flh_singlehthreaded(smbp: usize, sc: u8, slotnum: u32) {
        let flh_addr = if sc == 0 {
            let slabnum = get_thread_num() as u8;
            let slabbp = smbp | SIZECLASS_0_SC_INDICATOR_MASK | const_shl_u8_usize(slabnum, SMALLEST_SLOT_SIZE_BITS) as usize;
            (slabbp | SIZECLASS_0_SLOTNUM_MASK) & !SIZECLASS_0_SLOTNUM_LSB_MASK
        } else if sc < NUM_SMALL_SCS {
            let slabnum = get_thread_num() as u8;
            let slotnum_mask = const_shl_usize_usize(SIZECLASS_0_SLOTNUM_MASK, sc);
            let slabbp = smbp | const_shl_usize_usize(SIZECLASS_0_SC_INDICATOR_MASK, sc) | const_shl_u8_usize(slabnum, sc + SMALLEST_SLOT_SIZE_BITS) as usize;
            slabbp | slotnum_mask
        } else if sc < NUM_SMALL_SCS + NUM_MEDIUM_SCS {
            let slabbp = smbp | const_shl_usize_usize(SIZECLASS_5_SC_INDICATOR_MASK, sc - 5);
            let slotnum_mask = const_shl_usize_usize(SIZECLASS_5_SLOTNUM_MASK, sc - 5);
            slabbp | slotnum_mask
        } else {
            let largesc = sc - NUM_SMALL_SCS - NUM_MEDIUM_SCS;
            let slabbp = smbp | const_shl_u8_usize(largesc, LARGE_SLOT_SIZE_BITS_PLUS_NUM_SLOTS_BITS);
            let slotsizebits = sc + SMALLEST_SLOT_SIZE_BITS;
            let slotnumbits = LARGE_SLOT_SIZE_BITS_PLUS_NUM_SLOTS_BITS - slotsizebits;
            slabbp | LARGE_SC_INDICATOR_MASK | const_shl_u32_usize(const_gen_mask_u32(slotnumbits), slotsizebits)
        };
            
        let flha = unsafe { AtomicU64::from_ptr(flh_addr as *mut u64) };

        // single threaded so don't bother with the counter
        
        flha.store(slotnum as u64, Relaxed);
    }

    fn help_get_flh_singlehthreaded(smbp: usize, sc: u8) -> u32 {
        let flh_addr = if sc == 0 {
            let slabnum = get_thread_num() & SMALL_SLABNUM_MASK;
            let slabbp = smbp | SIZECLASS_0_SC_INDICATOR_MASK | const_shl_u32_usize(slabnum, SMALLEST_SLOT_SIZE_BITS) as usize;
            (slabbp | SIZECLASS_0_SLOTNUM_MASK) & !SIZECLASS_0_SLOTNUM_LSB_MASK
        } else if sc < NUM_SMALL_SCS {
            let slabnum = get_thread_num() & SMALL_SLABNUM_MASK;
            let slotnum_mask = const_shl_usize_usize(SIZECLASS_0_SLOTNUM_MASK, sc);
            let slabbp = smbp | const_shl_usize_usize(SIZECLASS_0_SC_INDICATOR_MASK, sc) | const_shl_u32_usize(slabnum, sc + SMALLEST_SLOT_SIZE_BITS) as usize;
            slabbp | slotnum_mask
        } else if sc < NUM_SMALL_SCS + NUM_MEDIUM_SCS {
            let slabbp = smbp | const_shl_usize_usize(SIZECLASS_5_SC_INDICATOR_MASK, sc - 5);
            let slotnum_mask = const_shl_usize_usize(SIZECLASS_5_SLOTNUM_MASK, sc - 5);
            slabbp | slotnum_mask
        } else {
            let slotsizebits = sc + SMALLEST_SLOT_SIZE_BITS;
            let slotnumbits = LARGE_SLOT_SIZE_BITS_PLUS_NUM_SLOTS_BITS - slotsizebits;
            let largesc = sc - NUM_SMALL_SCS - NUM_MEDIUM_SCS;
            let slabbp = smbp | LARGE_SC_INDICATOR_MASK | const_shl_u8_usize(largesc, LARGE_SLOT_SIZE_BITS_PLUS_NUM_SLOTS_BITS);
            slabbp | const_shl_u32_usize(const_gen_mask_u32(slotnumbits), slotsizebits)
        };
        
        let flha = unsafe { AtomicU64::from_ptr(flh_addr as *mut u64) };

        flha.load(Relaxed) as u32
    }

    /// If we've allocated all of the slots from a slab, then the next allocation comes from the
    /// next-bigger slab.
    fn help_test_overflow(sc: u8, numslots: u32) {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();
        let smbp = sm.get_sm_baseptr();

        let siz = help_slotsize(sc);
        let alignedsizebits = alignedsize_minus1_bits_lzcnt(siz, 1);
        let l = Layout::from_size_align(siz, 1).unwrap();

        // Step 0: reach into the slab's `flh` and set it to almost the max slot number.

        let first_i = numslots - 3;
        let mut i = first_i;
        help_set_flh_singlehthreaded(smbp, sc, i);

        // Step 1: allocate a slot and store it in local variables:
        let p1 = unsafe { sm.alloc(l) };
        assert!(!p1.is_null());

        let (sc1, slabnum1, slotnum1) = help_ptr_to_loc(&sm, p1, l);
        assert_eq!(sc1 + 2, alignedsizebits);
        assert_eq!(sc1, sc);
        assert_eq!(slotnum1, i);

        i += 1;
        
        // Step 2: allocate all the rest of the slots in this slab except the last one:
        while i < numslots - 1 {
            let pt = unsafe { sm.alloc(l) };
            assert!(!pt.is_null());

            i += 1
        }

        // Step 3: allocate the last slot in this slab and store it in local variables:
        let p2 = unsafe { sm.alloc(l) };
        assert!(!p2.is_null());

        let (sc2, slabnum2, slotnum2) = help_ptr_to_loc(&sm, p2, l);
        // Assert some things about the two stored slot locations:
        assert_eq!(sc2, sc, "sc: {sc}, numslots: {numslots}, i: {i}");
        assert_eq!(sc2 + 2, alignedsizebits);
        assert_eq!(slabnum1, slabnum2);
        assert_eq!(slotnum2, numslots - 1);

        // Step 4: Allocate another slot and store it in local variables:
        let p3 = unsafe { sm.alloc(l) };
        assert!(!p3.is_null());

        let (sc3, slabnum3, slotnum3) = help_ptr_to_loc(&sm, p3, l);

        // The raison d'etre for this test: Assert that the newly allocated slot is in a bigger
        // size class, same areanum.
        assert_eq!(slabnum3, slabnum1);
        assert_eq!(sc3, sc + 1);
        assert!(sc3 + 2 > alignedsizebits);
        assert_eq!(slotnum3, 0);
        assert_eq!(help_get_flh_singlehthreaded(smbp, sc3), 1, "sc3: {sc3}");

        // Step 5: If we alloc_slot() again on this thread, it will come from this new slab:
        let p4 = unsafe { sm.alloc(l) };
        assert!(!p4.is_null());

        let (sc4, slabnum4, slotnum4) = help_ptr_to_loc(&sm, p4, l);

        assert_eq!(sc4, sc3);
        assert!(sc4 + 2 > alignedsizebits);
        assert_eq!(slabnum4, slabnum3);
        assert_eq!(slotnum4, 1);

        // We've now allocated two slots from this new area:
        assert_eq!(help_get_flh_singlehthreaded(smbp, sc4), 2);
    }

    #[test]
    /// If we've allocated all of the slots from the smallest small-slots slab, the subsequent
    /// allocations come from a larger small-slots slab.
    fn overflow_smallest_to_bigger_small() {
        // slab 0 has a different number of slots from the other small-slots slabs
        // -2 for the flh slots
        let numslots: u32 = NUM_SMALL_SLOTS - 2;
        help_test_overflow(0, numslots);
    }

    #[test]
    /// If we've allocated all of the slots from the second-smallest small-slots slab, the
    /// subsequent allocations come from a larger small-slots slab.
    fn overflow_small_to_bigger_small() {
        // - 1 for the flh slot
        let numslots: u32 = NUM_SMALL_SLOTS - 1;
        help_test_overflow(1, numslots);
    }

    #[test]
    /// If we've allocated all of the slots from a slab, the subsequent allocations come from a
    /// larger sizeclass.
    fn overflow_x() {
        // This doesn't work for the largest large slab because there is no where to overflow to.
        for sc in 2..NUM_SMALL_SCS + NUM_MEDIUM_SCS + NUM_LARGE_SCS - 2 { 
            // - 1 for the flh slot
            let numslots = if sc < NUM_SMALL_SCS {
                help_pow2_u32(NUM_SMALL_SLOTS_BITS) - 1
            } else if sc < NUM_SMALL_SCS + NUM_MEDIUM_SCS {
                help_pow2_u32(NUM_MEDIUM_SLOTS_BITS) - 1
            } else {
                help_pow2_u32(36 - sc) - 1
            };
            help_test_overflow(sc, numslots);
        }
    }

    #[test]
    /// If we've allocated all of the slots from the largest large-slots slab, the next allocation
    /// fails.
    fn overflow_from_largest_large_slots_slab() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();
        let smbp = sm.get_sm_baseptr();

        let sc = NUM_SMALL_SCS + NUM_MEDIUM_SCS + NUM_LARGE_SCS - 1;
        let siz = help_slotsize(sc);
        let l = Layout::from_size_align(siz, 1).unwrap();

        // Step 0: reach into the slab's `flh` and set it to the max slot number.
        help_set_flh_singlehthreaded(smbp, sc, 1);

        // Step 1: allocate a slot and store it in local variables:
        let p1 = unsafe { sm.alloc(l) };
        assert!(p1.is_null());
    }
}
