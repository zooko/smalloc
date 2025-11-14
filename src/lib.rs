#![doc = include_str!("../README.md")]
#![feature(likely_unlikely)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::assertions_on_constants)]
#![feature(pointer_is_aligned_to)]
#![feature(unchecked_shifts)]
#![feature(test)]
#![feature(atomic_from_mut)]


// Layout of this file:
// * Fixed constants chosen for the design (see README.md)
// * Constant values computed at compile time from the fixed constants
// * Implementation code
// * Code for development (e.g benchmarks, tests, utility functions, development tools)


// --- Fixed constants chosen for the design ---

const NUM_SMALLEST_SLOT_SIZE_BITS: u8 = 2;
const NUM_SLABS_BITS: u8 = 5;
const NUM_SCS: u8 = 31; // This is also NUM_MOST_SLOTS_BITS.


// --- Constant values determined by the constants above ---

// See the ASCII-art map in `README.md` for where these bit masks fit in.
const NUM_SLABS: u8 = 2u8.pow(NUM_SLABS_BITS as u32);
const NUM_FLHS: u16 = NUM_SLABS as u16 * NUM_SCS as u16; // 992
const NUM_MOST_SLOTS_BITS: u8 = NUM_SCS;
const NUM_SLOTNUM_AND_DATA_BITS: u8 = NUM_MOST_SLOTS_BITS + NUM_SMALLEST_SLOT_SIZE_BITS; // 33
const NUM_SLABNUM_AND_SLOTNUM_AND_DATA_BITS: u8 = NUM_SLOTNUM_AND_DATA_BITS + NUM_SLABS_BITS; // 38
const SLABNUM_ALONE_MASK: u8 = const_gen_mask_u8(NUM_SLABS_BITS); // 0b11111
const SLABNUM_ADDR_MASK: usize = const_shl_u8_usize(SLABNUM_ALONE_MASK, NUM_SLOTNUM_AND_DATA_BITS); // 0b11111000000000000000000000000000000000
const NUM_SC_BITS: u8 = NUM_SCS.next_power_of_two().trailing_zeros() as u8; // 5
const SC_BITS_MASK: usize = const_shl_u8_usize(const_gen_mask_u8(NUM_SC_BITS), NUM_SLABNUM_AND_SLOTNUM_AND_DATA_BITS); // 0b1111100000000000000000000000000000000000000
const SLOTNUM_AND_DATA_MASK: usize = const_gen_mask_usize(NUM_SLOTNUM_AND_DATA_BITS); // 0b111111111111111111111111111111111

// The smalloc address of the slot with the highest address is:
const HIGHEST_SMALLOC_SLOT_ADDR: usize = const_shl_u8_usize(NUM_SCS - 1, NUM_SLABNUM_AND_SLOTNUM_AND_DATA_BITS) | SLABNUM_ADDR_MASK; // 0b1111011111000000000000000000000000000000000

// The smalloc address of the highest-addressed byte of a smalloc slot is:
const HIGHEST_SMALLOC_SLOT_BYTE_ADDR: usize = HIGHEST_SMALLOC_SLOT_ADDR | const_gen_mask_usize(NUM_SCS - 1 + NUM_SMALLEST_SLOT_SIZE_BITS); // 0b1111011111011111111111111111111111111111111

// The flh's are laid out after the slabs, and the beginning of the array of flh's is aligned to a
// power of 2 so that we can compute flh addresses with bitwise arithmetic.

const SIZE_OF_FLHS: usize = NUM_FLHS as usize * 8; // Each flh is 8 bytes, so this is 7936.
const FLHS_BASE: usize = (HIGHEST_SMALLOC_SLOT_BYTE_ADDR + 1).next_multiple_of(SIZE_OF_FLHS.next_power_of_two()); // 0b1111011111100000000000000000000000000000000

// The total memory needed for slabs and flh's is:
pub const SIZE_OF_SLABS_AND_FLHS: usize = FLHS_BASE + SIZE_OF_FLHS; // 0b1111011111100000000000000000001111100000000

// We need to allocate extra bytes so that we can align the smalloc base pointer so that all of the
// trailing bits (all of the bits covered by the SMALLOC_ADDRESS_BITS_MASK) of the smalloc base
// pointer are zeros.
const BASEPTR_ALIGN: usize = SIZE_OF_SLABS_AND_FLHS.next_power_of_two(); // 0b10000000000000000000000000000000000000000000 
const SMALLOC_ADDRESS_BITS_MASK: usize = BASEPTR_ALIGN - 1; // 0b1111111111111111111111111111111111111111111 
pub const TOTAL_VIRTUAL_MEMORY: usize = SIZE_OF_SLABS_AND_FLHS + SMALLOC_ADDRESS_BITS_MASK; // 0b11111011111100000000000000000001111011111111 == 17_313_013_178_111

// --- Lookup tables of constant values determined by the constants above ---

const fn gen_lut_scbits() -> [usize; NUM_SCS as usize] {
    let mut result = [0; NUM_SCS as usize];
    let mut i: usize = 0;
    while i < NUM_SCS as usize {
        result[i] = const_shl_u8_usize(i as u8, NUM_SLABNUM_AND_SLOTNUM_AND_DATA_BITS);
        i += 1;
    }
    result
}

const SCBITS_LUT: [usize; NUM_SCS as usize] = gen_lut_scbits();

const fn gen_lut_slabnumbits() -> [usize; NUM_SLABS as usize] {
    let mut result = [0; NUM_SLABS as usize];
    let mut i: usize = 0;
    while i < NUM_SLABS as usize {
        result[i] = const_shl_u8_usize(i as u8, NUM_SLOTNUM_AND_DATA_BITS);
        i += 1;
    }
    result
}

const SLABNUMBITS_LUT: [usize; NUM_SLABS as usize] = gen_lut_slabnumbits();

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
// xxx oops would that trigger a Rust overflow panic instead of wrapping? Check that...
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
/// `get_thread_num() % 32`.
fn get_slab_num() -> u8 {
    SLAB_NUM.with(|cell| {
        cell.get().map_or_else(
            || get_thread_num() as u8 & SLABNUM_ALONE_MASK,
            |value| value,
        )
    })
}

fn set_slab_num(slabnum: u8) {
    SLAB_NUM.set(Some(slabnum));
}

pub struct Smalloc {
    initlock: AtomicBool,
    sys_baseptr: AtomicUsize,
    sm_baseptr: AtomicUsize,
    flhs_baseptr: AtomicUsize,
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
            sm_baseptr: AtomicUsize::new(0),
            flhs_baseptr: AtomicUsize::new(0),
        }
    }

    pub fn dump_map_of_slabs(&self) {
        // Dump a map of the slabs
        let mut fullslots = 0;
        let mut fulltotsize = 0;
        let flhbp = self.get_flhs_baseptr();
        for sc in 0..NUM_SCS {
            let mut scfullslots = 0;
            let mut scfulltotsize = 0;

            print!("sc: {sc} ");

            let highestslotnum = const_gen_mask_u32(NUM_SCS - sc);
            let slotsize = 2u64.pow((sc + NUM_SMALLEST_SLOT_SIZE_BITS) as u32);
            println!("slots: {}, slotsize: {}", highestslotnum, slotsize);

            for slabnum in 0..NUM_SLABS {
                print!("sn: {slabnum} ");
                
                let headelement = help_get_flh(flhbp, sc, slabnum);
                if headelement == highestslotnum {
                    // full
                    print!("X ");
                    scfullslots += highestslotnum;
                    scfulltotsize += (highestslotnum as u64) * slotsize;
                } else {
                    print!(". ");
                }
            }
            println!(" slots: {scfullslots} size: {scfulltotsize}");
            fullslots += scfullslots;
            fulltotsize += scfulltotsize;
        }
        println!(" totslots: {fullslots}, totsize: {fulltotsize}");
    }

    pub fn idempotent_init(&self) -> Result<usize, AllocFailed> {
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

        p = self.sm_baseptr.load(Acquire);
        if p != 0 {
            // Release the spin lock
            self.initlock.store(false, Release);

            Ok(self.sm_baseptr.load(Relaxed))
        } else {
            let sysbp = sys_alloc(layout)?;
            assert!(!sysbp.is_null());
            self.sys_baseptr.store(sysbp.addr(), Release);//xxx can we use weaker ordering constraints?
            let smbp = sysbp.addr().next_multiple_of(BASEPTR_ALIGN);
            debug_assert!(smbp + SIZE_OF_SLABS_AND_FLHS <= sysbp.addr() + layout.size(), "sysbp: {sysbp:?}, smbp: {smbp:?}, slot: {HIGHEST_SMALLOC_SLOT_ADDR:?}, sosaf: {SIZE_OF_SLABS_AND_FLHS:?}, smbp+S: {:?}, size: {:?}, BASEPTR_ALIGN: {BASEPTR_ALIGN:?}", smbp + SIZE_OF_SLABS_AND_FLHS, layout.size());
            self.sm_baseptr.store(smbp, Release); //xxx can we use weaker ordering constraints?
            self.flhs_baseptr.store(smbp + FLHS_BASE, Release); //xxx weaker ordering constraints pls

            // Release the spin lock
            self.initlock.store(false, Release);

            Ok(smbp)
        }
    }

    fn get_sm_baseptr(&self) -> usize {
        let p = self.sm_baseptr.load(Acquire);
        debug_assert!(p != 0);

        p
    }

    fn get_flhs_baseptr(&self) -> usize {
        self.flhs_baseptr.load(Acquire)
    }

    /// `highestslotnum` is for using `& highestslotnum` instead of `% numslots` to compute a number
    /// modulo numslots (where `numslots` here counts the sentinel slot). So `highestslotnum` is
    /// equal to `numslots - 1`, which is also the slotnum of the sentinel slot. (It is also used in
    /// `debug_asserts`.) `newslotnum` cannot be the sentinel slotnum.
    fn push_slot_onto_freelist(&self, slabbp: usize, flh: &AtomicU64, newslotnum: u32, highestslotnum: u32, slotsizebits: u8) {
        debug_assert!(slabbp != 0);
        debug_assert!(help_trailing_zeros_usize(slabbp) >= NUM_SLOTNUM_AND_DATA_BITS);
        debug_assert!(newslotnum < highestslotnum);

        loop {
            // Load the value (current first entry slot num) from the flh
            let flhdword: u64 = flh.load(Acquire);
            let curfirstentryslotnum: u32 = (flhdword & u32::MAX as u64) as u32;
            // The curfirstentryslotnum can be the sentinel slotnum.
            debug_assert!(curfirstentryslotnum <= highestslotnum);

            let counter: u32 = (flhdword >> 32) as u32;

            // Encode it as the next-entry link for the new entry
            let next_entry_link = Self::encode_next_entry_link(newslotnum, curfirstentryslotnum, highestslotnum);
            debug_assert_eq!(curfirstentryslotnum, Self::decode_next_entry_link(newslotnum, next_entry_link, highestslotnum), "newslotnum: {newslotnum}, next_entry_link: {next_entry_link}, curfirstentryslotnum: {curfirstentryslotnum}, highestslotnum: {highestslotnum}");

            // Write it into the new slot
            let new_slot_p = (slabbp | const_shl_u32_usize(newslotnum, slotsizebits)) as *mut u32;
            unsafe { *new_slot_p = next_entry_link };

            // Create a new flh word, pointing to the new entry
            let newflhdword = ((counter as u64 + 1) << 32) | newslotnum as u64;

            // Compare and exchange
            if flh.compare_exchange_weak(flhdword, newflhdword, AcqRel, Acquire).is_ok() {
                break;
            }
        }
    }

    #[inline(always)]
    fn encode_next_entry_link(baseslotnum: u32, targslotnum: u32, highestslotnum: u32) -> u32 {
        debug_assert_ne!(baseslotnum, targslotnum);
        // The baseslotnum cannot be the sentinel slotnum.
        debug_assert!(baseslotnum < highestslotnum);
        // The targslotnum can be the sentinel slotnum.
        debug_assert!(targslotnum <= highestslotnum, "targslotnum: {targslotnum}, highestslotnum: {highestslotnum}");

        targslotnum.wrapping_sub(baseslotnum).wrapping_sub(1) & highestslotnum
    }

    #[inline(always)]
    fn decode_next_entry_link(baseslotnum: u32, codeword: u32, highestslotnum: u32) -> u32 {
        // The baseslotnum cannot be the sentinel slot num.
        debug_assert!(baseslotnum < highestslotnum);

        (baseslotnum + codeword + 1) & highestslotnum
    }
        
    /// Allocate a slot from this slab by popping the free-list-head. Return the resulting pointer
    /// as a usize, or null pointer (0) if this slab is full.
    ///
    /// `highestslotnum` is the slotnum of the sentinel slot (`numslots - 1`). It is also used to
    /// compute numbers modulo `numslots` with `& highestslotnum` instead of with `% numslots`, and
    /// it is used in `debug_asserts`.
    fn pop_slot_from_freelist(&self, slabbp: usize, flh: &AtomicU64, highestslotnum: u32, slotsizebits: u8) -> usize {
        debug_assert!(slabbp != 0);
        debug_assert!((slabbp >= self.get_sm_baseptr()) && (slabbp <= (self.get_sm_baseptr() + HIGHEST_SMALLOC_SLOT_ADDR)), "slabbp: {slabbp:x}, smbp: {:x}, highest_addr: {:x}", self.get_sm_baseptr(), self.get_sm_baseptr() + HIGHEST_SMALLOC_SLOT_ADDR);
        debug_assert!(help_trailing_zeros_usize(slabbp) >= NUM_SLOTNUM_AND_DATA_BITS);

        loop {
            // Load the value from the flh
            let flhdword = flh.load(Acquire);
            let curfirstentryslotnum = (flhdword & (u32::MAX as u64)) as u32;

            // curfirstentryslotnum can be the sentinel value.
            debug_assert!(curfirstentryslotnum <= highestslotnum);
            if curfirstentryslotnum == highestslotnum {
                // meaning no next entry, meaning the free list is empty
                break 0
            }

            let curfirstentry_p = slabbp | const_shl_u32_usize(curfirstentryslotnum, slotsizebits);

            debug_assert!((curfirstentry_p >= self.get_sm_baseptr()) && (curfirstentry_p <= (self.get_sm_baseptr() + HIGHEST_SMALLOC_SLOT_ADDR)), "curfirstentry_p: {curfirstentry_p:x}, smbp: {:x}, slabbp: {slabbp:x}, highest_addr: {:x}", self.get_sm_baseptr(), self.get_sm_baseptr() + HIGHEST_SMALLOC_SLOT_ADDR);


            // Read the bits from the first entry's slot and decode them into a slot number. These
            // bits might be "invalid" in the sense of not encoding a slot number, if the flh has
            // changed since we read it above and another thread has started using these bits for
            // something else (e.g. user data or another linked list update). That's okay because in
            // that case our attempt to update the flh below with information derived from these
            // bits will fail.
            let curfirstentryval = unsafe { *(curfirstentry_p as *mut u32) };

            let newnextentryslotnum: u32 = Self::decode_next_entry_link(curfirstentryslotnum, curfirstentryval, highestslotnum);

            // Create a new flh word, with the new slotnum, pointing to the new first slot
            let counter: u32 = (flhdword >> 32) as u32;
            let newflhdword = ((counter as u64 + 1) << 32) | newnextentryslotnum as u64;

            // Compare and exchange
            if flh.compare_exchange_weak(flhdword, newflhdword, AcqRel, Acquire).is_ok() {
                break curfirstentry_p;
            }
        }
    }
}

use std::cmp::max;
use std::thread;

pub fn help_get_flh(flhbp: usize, sc: u8, slabnum: u8) -> u32 {
    let flhi = NUM_SCS as u16 * slabnum as u16 + sc as u16;
    let flhptr = flhbp | const_shl_u16_usize(flhi, 3);
    let flha = unsafe { AtomicU64::from_ptr(flhptr as *mut u64) };
    let flhdword = flha.load(Relaxed);
    (flhdword & u32::MAX as u64) as u32
}

unsafe impl GlobalAlloc for Smalloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match self.idempotent_init() {
            Err(error) => {
                eprintln!("Failed to alloc; underlying error: {error}"); // xxx can't print out error contents without heap allocation
                null_mut()
            }
            Ok(smbp) => {
                debug_assert!(smbp == self.get_sm_baseptr(), "{smbp:x}, {:x}", self.get_sm_baseptr());
                let reqsiz = layout.size();
                let reqalign = layout.align();
                debug_assert!(reqsiz > 0);
                debug_assert!(reqalign > 0);
                debug_assert!(reqalign.is_power_of_two()); // alignment must be a power of two

                let slotsizebits = req_to_slotsizebits(reqsiz, reqalign);
                let sc = slotsizebits - NUM_SMALLEST_SLOT_SIZE_BITS;
                if sc >= NUM_SCS {
                    eprintln!("smalloc exhausted");

                    self.dump_map_of_slabs();
                    
                    // This request exceeds our largest slot size, so we return null ptr.
                    return null_mut();
                };

                // If the slab is full, we'll switch to another slab in this same sizeclass.
                let orig_slabnum = get_slab_num();
                let mut slabnum = orig_slabnum;
                
                loop {
                    // The slabbp is the smbp with the size class bits and the slabnum bits set.
                    let slabbp = smbp | SCBITS_LUT[sc as usize] | SLABNUMBITS_LUT[slabnum as usize];
                    //let slabbp = smbp | const_shl_u8_usize(sc, NUM_SLABNUM_AND_SLOTNUM_AND_DATA_BITS) | const_shl_u8_usize(slabnum, NUM_SLOTNUM_AND_DATA_BITS);

                    debug_assert!((slabbp >= smbp) && (slabbp <= (smbp + HIGHEST_SMALLOC_SLOT_ADDR)), "slabbp: {slabbp:x}, smbp: {smbp:x}, highest_addr: {:x}", smbp + HIGHEST_SMALLOC_SLOT_ADDR);
                    debug_assert!(help_trailing_zeros_usize(slabbp) >= slotsizebits);

                    let flhi = NUM_SCS as u16 * slabnum as u16 + sc as u16;
                    let flhptr = self.get_flhs_baseptr() | const_shl_u16_usize(flhi, 3);
                    let flh = unsafe { AtomicU64::from_ptr(flhptr as *mut u64) };

                    let highestslotnum = const_gen_mask_u32(NUM_SCS - sc);

                    let p_addr = self.pop_slot_from_freelist(slabbp, flh, highestslotnum, slotsizebits);

                    debug_assert!((p_addr == 0) || (p_addr >= self.get_sm_baseptr()) && (p_addr <= (self.get_sm_baseptr() + HIGHEST_SMALLOC_SLOT_ADDR)), "p_addr: {p_addr:x}, smbp: {:x}, highest_addr: {:x}", self.get_sm_baseptr(), self.get_sm_baseptr() + HIGHEST_SMALLOC_SLOT_ADDR);

                    if p_addr != 0 {
                        // Remember the new slab num for next time.
                        set_slab_num(slabnum);

                        return p_addr as *mut u8;
                    }

                    // This slab was full.  Overflow to a different slab in the same size
                    // class. Which slabnumber? 1. It should be relatively prime to NUM_SLABS so
                    // that we will try all slabs before returning to the original one. 2. It should
                    // be larger than 1 or 2 because the next couple threads got those slabs (the
                    // first time they allocated). 3. It should use the information from the thread
                    // number, not just the (strictly lesser) information from the original slab
                    // number. So:
                    const STEPS: [u8; 10] = [3, 5, 7, 11, 13, 17, 19, 23, 29, 31];
                    let ix = (get_thread_num() as usize / NUM_SLABS as usize) % STEPS.len();
                    slabnum = (slabnum + STEPS[ix]) % NUM_SLABS;

                    if slabnum == orig_slabnum {
                        // All slabs in this sizeclass were full. Overflow to a slab with larger
                        // slots, by recursively calling `.alloc()` with a doubled requested
                        // size. (Doubling the requested size guarantees that the new recursive
                        // request will use the next larger sc.)
                        let doublesize_layout = Layout::from_size_align(reqsiz * 2, reqalign).unwrap();//xxx use the unsafe version
                        return unsafe { self.alloc(doublesize_layout) }
                    }
                }
            }
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        debug_assert!(!ptr.is_null());
        debug_assert!(layout.align().is_power_of_two()); // alignment must be a power of two

        let p_addr = ptr.addr();
        let smbp = self.get_sm_baseptr();

        // To be valid, the pointer has to be greater than or equal to the smalloc base pointer and
        // less than or equal to the highest slot pointer.
        let highest_addr = smbp + HIGHEST_SMALLOC_SLOT_ADDR;

        assert!((p_addr >= smbp) && (p_addr <= highest_addr), "p_addr: {p_addr}, smbp: {smbp}, highest_addr: {highest_addr}");

        // Okay now we know that it is a pointer into smalloc's region.

        let slabbp = p_addr & !SLOTNUM_AND_DATA_MASK;
        let sc = const_shr_usize_u8(p_addr & SC_BITS_MASK, NUM_SLABNUM_AND_SLOTNUM_AND_DATA_BITS);
        let slotsizebits = sc + NUM_SMALLEST_SLOT_SIZE_BITS;
        let slotnum = const_shr_usize_u32(p_addr & SLOTNUM_AND_DATA_MASK, slotsizebits);
        let slabnum = const_shr_usize_u8(p_addr & SLABNUM_ADDR_MASK, NUM_SLOTNUM_AND_DATA_BITS);
        let highestslotnum = const_gen_mask_u32(NUM_MOST_SLOTS_BITS - sc);

        let flhi = NUM_SCS as u16 * slabnum as u16 + sc as u16;
        let flhptr = self.get_flhs_baseptr() | const_shl_u16_usize(flhi, 3);
        let flh = unsafe { AtomicU64::from_ptr(flhptr as *mut u64) };

        debug_assert!(p_addr.trailing_zeros() >= slotsizebits as u32);

        self.push_slot_onto_freelist(slabbp, flh, slotnum, highestslotnum, slotsizebits);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, reqsize: usize) -> *mut u8 {
        debug_assert!(!ptr.is_null());
        let oldsize = layout.size();
        debug_assert!(oldsize > 0);
        let oldalignment = layout.align();
        debug_assert!(oldalignment > 0);
        debug_assert!(oldalignment.is_power_of_two()); // alignment must be a power of two
        debug_assert!(reqsize > 0);

        let oldsizbits = req_to_slotsizebits(oldsize, oldalignment);
        let reqsizbits = req_to_slotsizebits(reqsize, oldalignment);

        // If the requested new size (rounded up to a slot) is <= the original size (rounded up to a
        // slot), just return the pointer and we're done.
        if reqsizbits <= oldsizbits {
            return ptr;
        }

        let newsc = max(NUM_SMALLEST_SLOT_SIZE_BITS, reqsizbits) - NUM_SMALLEST_SLOT_SIZE_BITS;

        let l = unsafe { Layout::from_size_align_unchecked(const_one_shl_usize(newsc + NUM_SMALLEST_SLOT_SIZE_BITS), oldalignment) };
        let newp = unsafe { self.alloc(l) };
        debug_assert!(!newp.is_null(), "{l:?}");
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
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::atomic::Ordering::{AcqRel, Acquire, Release};
mod platformalloc;
use platformalloc::{sys_alloc, sys_dealloc};
use platformalloc::vendor::PAGE_SIZE;
use std::ptr::{copy_nonoverlapping, null_mut};
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
const fn const_shl_u16_usize(value: u16, shift: u8) -> usize {
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
const fn _const_shl_usize_usize(value: usize, shift: u8) -> usize {
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
const fn _const_one_shl_u32(shift: u8) -> u32 {
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
const fn const_gen_mask_usize(numbits: u8) -> usize {
    debug_assert!((numbits as u32) < usize::BITS);

    unsafe { 1usize.unchecked_shl(numbits as u32) - 1 }
}

#[inline(always)]
const fn const_gen_mask_u32(numbits: u8) -> u32 {
    debug_assert!((numbits as u32) < u32::BITS);

    unsafe { 1u32.unchecked_shl(numbits as u32) - 1 }
}

#[inline(always)]
const fn const_gen_mask_u8(numbits: u8) -> u8 {
    debug_assert!((numbits as u32) < u8::BITS);

    unsafe { 1u8.unchecked_shl(numbits as u32) - 1 }
}

/// Return the number of significant bits in the aligned size. This is the log base 2 of the size of
/// slot required to hold requests of this size and alignment, but a minimum of 2 since that is log
/// base 2 of the slots of our smallest sizeclass.
fn req_to_slotsizebits(size: usize, align: usize) -> u8 {
    debug_assert!(size > 0);
    debug_assert!(align > 0);
    max(2, usize::BITS as u8 - min(help_leading_zeros_usize(size - 1), help_leading_zeros_usize(align - 1)))
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
// When using the Tango benchmarking harness, which loads and executes functions from two executables at once, I can only allocate 35,183,801,663,488 virtual memory. I have no idea why that is. :-(
//
// The current settings of smalloc (v4.0.0) require 59,785,944,760,326 bytes of virtual address space.
//
// Now working on smalloc v5.0.0 which requires only 29,824,252,903,423 bytes of virtual address space.


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


pub mod smallocb_allocator_config;

pub mod benchmarks {
    use std::alloc::GlobalAlloc;

    // use crate::{help_test_multithreaded_with_allocator, help_test_singlethreaded_with_allocator};

    use crate::platformalloc::vendor::ClockType;
    use std::mem::MaybeUninit;

    pub fn clock(clocktype: ClockType) -> u64 {
        let mut tp: MaybeUninit<libc::timespec> = MaybeUninit::uninit();
        let retval = unsafe { libc::clock_gettime(clocktype, tp.as_mut_ptr()) };
        debug_assert_eq!(retval, 0);
        let instsec = unsafe { (*tp.as_ptr()).tv_sec };
        let instnsec = unsafe { (*tp.as_ptr()).tv_nsec };
        debug_assert!(instsec >= 0);
        debug_assert!(instnsec >= 0);
        instsec as u64 * 1_000_000_000 + instnsec as u64
    }

    use std::alloc::{System, Layout};

    pub struct GlobalAllocWrap;

    unsafe impl GlobalAlloc for GlobalAllocWrap {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, reqsize: usize) -> *mut u8 {
            unsafe { System.realloc(ptr, layout, reqsize) }
        }
    }

    use std::sync::Arc;
    pub fn alloc_and_free(allocator: &Arc<impl GlobalAlloc>) {
        let l = unsafe { Layout::from_size_align_unchecked(32, 1) };
        let p = unsafe { allocator.alloc(l) };
        unsafe { *p = 0 };
        unsafe { allocator.dealloc(p, l) };
    }

    #[inline(never)]
    pub fn bench_itered<F: FnMut()>(name: &str, iters: usize, mut f: F, clocktype: ClockType) {
        let start = clock(clocktype);
        for _i in 0..iters {
            f();
        }
        let elap = clock(clocktype) - start;
        println!("name: {name}, iters: {iters}, ns: {elap}, ns/i: {}", elap/iters as u64);
    }

    use thousands::Separable;
    #[inline(never)]
    pub fn bench_once<F: FnOnce()>(name: &str, f: F, clocktype: ClockType) {
        let start = clock(clocktype);
        f();
        let elap = clock(clocktype) - start;
        println!("name: {name}, ns: {}", elap.separate_with_commas());
    }

    use std::time::Instant;
    pub fn singlethread_bench<T, F>(bf: F, iters: u64, name: &str, al: &T) -> u64
    where
        T: GlobalAlloc,
        F: Fn(&T, &mut TestState) + Sync + Send + Copy + 'static
    {
        let mut s = TestState::new(iters);

        let start = Instant::now();

        for _i in 0..iters {
            bf(al, &mut s);
    	}

        let end = Instant::now();
        assert!(end > start);
        let elap_ns = (end - start).as_nanos() as u64;
        let ns_per_iter = elap_ns / iters;
        println!("name: {name:>12}, iters: {:>11}, ns: {:>12}, ns/i: {:>11}", iters.separate_with_commas(), elap_ns.separate_with_commas(), ns_per_iter.separate_with_commas());

        elap_ns
    }

    #[macro_export]
        macro_rules! compare_bench {
        ($func:path, $iters:expr, $name:expr) => {{
            let mut baseline_ns = 42;
            let mut candidat_ns = 42;

            let sm = $crate::Smalloc::new();
            sm.idempotent_init().unwrap();
            let bi = $crate::benchmarks::GlobalAllocWrap;

            std::thread::scope(|scope| {
                scope.spawn(|| { 
                    // Create a closure that specifies the type
                    let f = |al: &$crate::benchmarks::GlobalAllocWrap, s: &mut $crate::benchmarks::TestState| {
                        $func(al, s)
                    };
                    let bi_name = format!("bi {}", $name);
                    baseline_ns = $crate::benchmarks::singlethread_bench(f, $iters, &bi_name, &bi); 
                });
                scope.spawn(|| { 
                    // Create a closure that specifies the type
                    let f = |al: &$crate::Smalloc, s: &mut $crate::benchmarks::TestState| {
                        $func(al, s)
                    };
                    let sm_name = format!("sm {}", $name);
                    candidat_ns = $crate::benchmarks::singlethread_bench(f, $iters, &sm_name, &sm); 
                });
            });

            let diffperc = 100.0 * (candidat_ns as f64 - baseline_ns as f64) / (baseline_ns as f64);
            println!("diff: {diffperc:.0}%");
        }}
    }


    // pub fn multithread_bench<F>(bf: F, threads: u32, iters: u32, name: &str, al: AllocatorType, ls: [Layout; 24])
    // where
    //     F: Fn(&AllocatorType, &mut TestState, [Layout; 24]) + Sync + Send + Copy + 'static
    // {
    //     let start = clock(libc::CLOCK_UPTIME_RAW);

    //     help_test_multithreaded_with_allocator(bf, threads, iters, al, ls);

    //     let elap = clock(libc::CLOCK_UPTIME_RAW) - start;

    //     println!("name: {name:>12}, threads: {threads:>4}, iters: {:>7}, ms: {:>9}, ns/i: {:>10}", iters.separate_with_commas(), (elap/1_000_000).separate_with_commas(), (elap / iters as u64).separate_with_commas());
    // }

    pub const NUMCOINS: usize = 512;
    pub const NUMLAYOUTS: usize = 24;
    use std::collections::HashSet;
    pub struct TestState {
        pub coins: [u32; NUMCOINS],
        pub nextcoin: usize,
        pub layouts: [Layout; NUMLAYOUTS],
        pub nextlayout: usize,
        pub r: WyRand,
        pub ps: Vec<(usize, Layout)>,
        pub m: HashSet<(usize, Layout), WyHashBuilder>,
    }

    use wyhash::WyHash;

    // Simple wrapper
    #[derive(Clone)]
    pub struct WyHashBuilder(u64);

    impl std::hash::BuildHasher for WyHashBuilder {
        type Hasher = WyHash;
        fn build_hasher(&self) -> Self::Hasher {
            WyHash::with_seed(self.0)
        }
    }

    use crate::gen_layouts;
    use wyrand::WyRand;
    use rand::seq::SliceRandom;
    impl TestState {
        pub fn new(iters: u64) -> Self {
            let mut r = WyRand::new(6);
            let m = HashSet::with_capacity_and_hasher(iters as usize, WyHashBuilder(42));
            let coins: [u32; NUMCOINS] = std::array::from_fn(|_| r.rand() as u32);
            let nextcoin = 0;
            let mut layouts = gen_layouts();
            layouts.shuffle(&mut r);
            let nextlayout = 0;

            Self {
                coins,
                nextcoin,
                layouts,
                nextlayout,
                r,
                m,
                ps: Vec::with_capacity(iters as usize),
            }
        }
    }
}

// xxx move tests and benchmarks to a separate file

// These functions are used in both tests and benchmarks.
pub const BYTES1: [u8; 8] = [1, 2, 4, 3, 5, 6, 7, 8];
const BYTES2: [u8; 8] = [9, 8, 7, 6, 5, 4, 3, 2];
const BYTES3: [u8; 8] = [0xA, 0xB, 0xC, 0xD, 0xE, 0xF, 0x10, 0x11];
const BYTES4: [u8; 8] = [0x12, 0x11, 0x10, 0xF, 0xE, 0xD, 0xC, 0xB];
const BYTES5: [u8; 8] = [0xFE, 0xFD, 0xFC, 0xFB, 0xFA, 0xF9, 0xF8, 0xF7];
const BYTES6: [u8; 8] = [0xFE, 0xFD, 0xF6, 0xF5, 0xFA, 0xF9, 0xF8, 0xF7];
use benchmarks::TestState;
use crate::smallocb_allocator_config::AllocatorType;
use std::hint::black_box;

#[inline(never)]
pub fn dummy_func(maxi: u64, maxj: u64) -> u64 {
    let mut a = Arc::new(0);
    for i in 0..maxi {
        for j in 0..maxj {
            *Arc::make_mut(&mut a) ^= black_box(i.wrapping_mul(j));
        }
    }

    *a
}

pub fn help_test_dummy_func(_al: &Arc<AllocatorType>, iters: u32, _s: &mut TestState, _ls: &Arc<Vec<Layout>>) {
    for _i in 0..iters {
        //dummy_func(9, 7); // This crashed with heap corruption twice out of about 10 runs.
        dummy_func(2, 3);
    }
}

use crate::benchmarks::NUMCOINS;
fn next_coin(s: &mut TestState) -> u32 {
    let coin = s.coins[s.nextcoin]; s.nextcoin = (s.nextcoin + 1) % NUMCOINS;
    coin
}

use crate::benchmarks::NUMLAYOUTS;
fn next_layout(s: &mut TestState) -> Layout {
    let layout = s.layouts[s.nextlayout]; s.nextlayout = (s.nextlayout + 1) % NUMLAYOUTS;
    layout
}

use std::hint::unlikely;
pub fn help_test_alloc_dealloc_realloc_with_writes<T: GlobalAlloc>(al: &T, s: &mut TestState) {
    // random coin
    let coin = next_coin(s) % 3;
    if unlikely(s.ps.is_empty()) || coin == 0 {
        // Malloc
        let lt = next_layout(s);
        let p = unsafe { al.alloc(lt) };
        debug_assert!(!p.is_null(), "{lt:?}");
        unsafe { std::ptr::copy_nonoverlapping(BYTES3.as_ptr(), p, min(BYTES3.len(), lt.size())) };
        debug_assert!(!s.m.contains(&(p as usize, lt)), "thread: {:>3}, {:?} {}-{}", get_thread_num(), p, lt.size(), lt.align()); // This line is the only reason s.m exists.
        #[cfg(test)] { s.m.insert((p as usize, lt)); }
        s.ps.push((p as usize, lt));

        // Write to a random allocation...
        let i = next_coin(s) as usize % s.ps.len();
        let (po, lto) = unsafe { s.ps.get_unchecked(i) };
        unsafe { std::ptr::copy_nonoverlapping(BYTES4.as_ptr(), (*po) as *mut u8, min(BYTES4.len(), lto.size())) };
    } else if coin == 1 {
        // Free
        debug_assert!(!s.ps.is_empty());

        // Write to a random allocation...
        let i = next_coin(s) as usize % s.ps.len();
        let (po, lto) = unsafe { s.ps.get_unchecked(i) };
        unsafe { std::ptr::copy_nonoverlapping(BYTES2.as_ptr(), (*po) as *mut u8, min(BYTES2.len(), lto.size())) };

        let i = next_coin(s) as usize % s.ps.len();
        let (p, lt) = s.ps.swap_remove(i);
        debug_assert!(s.m.contains(&(p, lt)), "thread: {:>3}, {:?} {}-{}", get_thread_num(), p, lt.size(), lt.align());
        #[cfg(test)] { s.m.remove(&(p, lt)); }

        unsafe { std::ptr::copy_nonoverlapping(BYTES1.as_ptr(), p as *mut u8, min(BYTES1.len(), lt.size())) };
        unsafe { al.dealloc(p as *mut u8, lt) };
    } else {
        // Realloc
        debug_assert!(!s.ps.is_empty());
        let i = next_coin(s) as usize % s.ps.len();
        let (p, lt) = s.ps.swap_remove(i);
        debug_assert_ne!(p, 0);
        debug_assert!(s.m.contains(&(p, lt)), "thread: {:>3}, {:?} {}-{}", get_thread_num(), p, lt.size(), lt.align());
        #[cfg(test)] { s.m.remove(&(p, lt)); }

        let newlt = next_layout(s);
        let newp = unsafe { al.realloc(p as *mut u8, lt, newlt.size()) };
        unsafe { std::ptr::copy_nonoverlapping(BYTES5.as_ptr(), newp, min(BYTES5.len(), lt.size())) };

        debug_assert!(!s.m.contains(&(newp as usize, newlt)), "thread: {:>3}, {:?} {}-{}", get_thread_num(), newp, newlt.size(), newlt.align()); // This line is the only reason s.m exists.
        #[cfg(test)] { s.m.insert((newp as usize, newlt)); }
        s.ps.push((newp as usize, newlt));

        // Write to a random allocation...
        let i = next_coin(s) as usize % s.ps.len();
        let (po, lto) = unsafe { s.ps.get_unchecked(i) };
        unsafe { std::ptr::copy_nonoverlapping(BYTES6.as_ptr(), (*po) as *mut u8, min(BYTES6.len(), lto.size())) };
    }
}

pub fn help_test_alloc_dealloc_realloc<T: GlobalAlloc>(al: &T, s: &mut TestState) {
    // random coin
    let coin = next_coin(s) % 3;
    if unlikely(s.ps.is_empty()) || coin == 0 {
        // Malloc
        let lt = next_layout(s);
        let p = unsafe { al.alloc(lt) };
        debug_assert!(!p.is_null(), "{lt:?}");
        debug_assert!(!s.m.contains(&(p as usize, lt)), "thread: {:>3}, {:?} {}-{}", get_thread_num(), p, lt.size(), lt.align()); // This line is the only reason s.m exists.
        #[cfg(test)] { s.m.insert((p as usize, lt)); }
        s.ps.push((p as usize, lt));
    } else if coin == 1 {
        // Free
        debug_assert!(!s.ps.is_empty());
        let i = next_coin(s) as usize % s.ps.len();
        let (p, lt) = s.ps.swap_remove(i);
        debug_assert!(s.m.contains(&(p, lt)), "thread: {:>3}, {:?} {}-{}", get_thread_num(), p, lt.size(), lt.align());
        #[cfg(test)] { s.m.remove(&(p, lt)); }
        unsafe { al.dealloc(p as *mut u8, lt) };
    } else {
        // Realloc
        debug_assert!(!s.ps.is_empty());
        let i = next_coin(s) as usize % s.ps.len();
        let (p, lt) = s.ps.swap_remove(i);
        debug_assert_ne!(p, 0);
        debug_assert!(s.m.contains(&(p, lt)), "thread: {:>3}, {:?} {}-{}", get_thread_num(), p, lt.size(), lt.align());
        #[cfg(test)] { s.m.remove(&(p, lt)); }

        let newlt = next_layout(s);
        let newp = unsafe { al.realloc(p as *mut u8, lt, newlt.size()) };

        debug_assert!(!s.m.contains(&(newp as usize, newlt)), "thread: {:>3}, {:?} {}-{}", get_thread_num(), newp, newlt.size(), newlt.align()); // This line is the only reason s.m exists.
        #[cfg(test)] { s.m.insert((newp as usize, newlt)); }
        s.ps.push((newp as usize, newlt));
    }
}

pub fn help_test_alloc_dealloc_with_writes<T: GlobalAlloc>(al: &T, s: &mut TestState) {
    // random coin
    let coin = next_coin(s) % 2;
    if unlikely(s.ps.is_empty()) || coin == 0 {
        // Malloc
        let lt = next_layout(s);
        let p = unsafe { al.alloc(lt) };
        debug_assert!(!p.is_null(), "{lt:?}");
        unsafe { std::ptr::copy_nonoverlapping(BYTES3.as_ptr(), p, min(BYTES3.len(), lt.size())) };
        debug_assert!(!s.m.contains(&(p as usize, lt)), "thread: {:>3}, {:?} {}-{}", get_thread_num(), p, lt.size(), lt.align()); // This line is the only reason s.m exists.
        #[cfg(test)] { s.m.insert((p as usize, lt)); }
        s.ps.push((p as usize, lt));

        // Write to a random allocation...
        let i = next_coin(s) as usize % s.ps.len();
        let (po, lto) = unsafe { s.ps.get_unchecked(i) };
        unsafe { std::ptr::copy_nonoverlapping(BYTES4.as_ptr(), (*po) as *mut u8, min(BYTES4.len(), lto.size())) };
    } else {
        // Free
        debug_assert!(!s.ps.is_empty());

        // Write to a random allocation...
        let i = next_coin(s) as usize % s.ps.len();
        let (po, lto) = unsafe { s.ps.get_unchecked(i) };
        unsafe { std::ptr::copy_nonoverlapping(BYTES2.as_ptr(), (*po) as *mut u8, min(BYTES2.len(), lto.size())) };

        let i = next_coin(s) as usize % s.ps.len();
        let (p, lt) = s.ps.swap_remove(i);
        debug_assert!(s.m.contains(&(p, lt)), "thread: {:>3}, {:?} {}-{}", get_thread_num(), p, lt.size(), lt.align());
        #[cfg(test)] { s.m.remove(&(p, lt)); }

        unsafe { std::ptr::copy_nonoverlapping(BYTES1.as_ptr(), p as *mut u8, min(BYTES1.len(), lt.size())) };
        unsafe { al.dealloc(p as *mut u8, lt) };
    }
}

pub fn help_test_alloc_dealloc<T: GlobalAlloc>(al: &T, s: &mut TestState) {
    // random coin
    let coin = next_coin(s) % 2;
    if unlikely(s.ps.is_empty()) || coin == 0 {
        // Malloc
        let lt = next_layout(s);
        let p = unsafe { al.alloc(lt) };
        debug_assert!(!p.is_null(), "{lt:?}");
        debug_assert!(!s.m.contains(&(p as usize, lt)), "thread: {:>3}, {:?} {}-{}", get_thread_num(), p, lt.size(), lt.align()); // This line is the only reason s.m exists.
        #[cfg(test)] { s.m.insert((p as usize, lt)); }
        s.ps.push((p as usize, lt));
    } else {
        // Free
        debug_assert!(!s.ps.is_empty());

        let i = next_coin(s) as usize % s.ps.len();
        let (p, lt) = s.ps.swap_remove(i);
        debug_assert!(s.m.contains(&(p, lt)), "thread: {:>3}, {:?} {}-{}", get_thread_num(), p, lt.size(), lt.align());
        #[cfg(test)] { s.m.remove(&(p, lt)); }

        unsafe { al.dealloc(p as *mut u8, lt) };
    }
}

pub fn help_test_alloc_with_writes<T: GlobalAlloc>(al: &T, s: &mut TestState) {
    // Malloc
    let l = next_layout(s);
    let p = unsafe { al.alloc(l) };
    debug_assert!(!p.is_null(), "l: {l:?}");
    debug_assert!(!s.m.contains(&(p as usize, l)), "thread: {:>3}, {:?} {}-{}", get_thread_num(), p, l.size(), l.align()); // This line is the only reason s.m exists.
    #[cfg(test)] { s.m.insert((p as usize, l)); }
    unsafe { std::ptr::copy_nonoverlapping(BYTES3.as_ptr(), p, min(BYTES3.len(), l.size())) };
    s.ps.push((p as usize, l));
}

pub fn help_test_alloc<T: GlobalAlloc>(al: &T, s: &mut TestState) {
    // Malloc
    let l = next_layout(s);
    let p = unsafe { al.alloc(l) };
    debug_assert!(!p.is_null(), "l: {l:?}");
    debug_assert!(!s.m.contains(&(p as usize, l)), "thread: {:>3}, {:?} {}-{}", get_thread_num(), p, l.size(), l.align()); // This line is the only reason s.m exists.
    #[cfg(test)] { s.m.insert((p as usize, l)); }
    s.ps.push((p as usize, l));
}

use std::sync::Arc;
pub fn help_test_multithreaded_with_allocator<F>(f: F, threads: u32, iters: u64, al: AllocatorType)
where
    F: Fn(&AllocatorType, &mut TestState) + Sync + Send + Copy + 'static
{
    thread::scope(|scope| {
        for _t in 0..threads {
            scope.spawn(|| {
                let mut s = TestState::new(iters);

                for _i in 0..iters {
                    f(&al, &mut s);
                }
            });
        }
    });
}

pub fn gen_layouts() -> [Layout; 24] {
    let mut ls = Vec::new();
    for siz in [4, 35, 64, 128, 2000, 8_000] {
        ls.push(Layout::from_size_align(siz, 1).unwrap());

        ls.push(Layout::from_size_align(siz + 10, 1).unwrap());
        ls.push(Layout::from_size_align(max(siz, 11) - 10, 1).unwrap());
        ls.push(Layout::from_size_align(siz * 2, 1).unwrap());
    }

    ls.try_into().unwrap()
}

#[cfg(test)]
pub mod tests {
    use super::*;

    const fn help_pow2_usize(bits: u8) -> usize {
        2usize.pow(bits as u32)
    }
    
    pub const fn help_pow2_u32(bits: u8) -> u32 {
        2u32.pow(bits as u32)
    }
    
    fn alignedsize_or(size: usize, align: usize) -> usize {
        ((size - 1) | (align - 1)) + 1
    }

    #[test]
    fn slotnum_encode_and_decode_roundtrip() {
        for numslotsbits in [ 31, 30, 25, 12, 9, 3, 2, 1 ] {
            let highestslotnum = const_gen_mask_u32(numslotsbits);
            let numslots = help_pow2_u32(numslotsbits);
            
            let slotnums = [ 0, 1, 2, 3, 4, numslots.wrapping_sub(4), numslots.wrapping_sub(3), numslots.wrapping_sub(2), numslots.wrapping_sub(1) ];
            for slotnum1 in slotnums {
                for slotnum2 in slotnums {
                    if slotnum1 < numslots - 1 && slotnum2 < numslots && slotnum1 != slotnum2 {
                        let ence = Smalloc::encode_next_entry_link(slotnum1, slotnum2, highestslotnum);
                        let dece = Smalloc::decode_next_entry_link(slotnum1, ence, highestslotnum);
                        assert_eq!(slotnum2, dece);
                    }
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
    fn one_realloc_to_tiny() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let l = Layout::from_size_align(1, 1).unwrap();
        let p = unsafe { sm.alloc(l) };
        let p2 = unsafe { sm.realloc(p, l, 3) };
        debug_assert_eq!(p, p2);
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
    fn a_few_allocs_and_a_dealloc_for_each_slab() {
        // Doesn't work for the largest size class (sc 31) because there aren't 3 slots.
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        for sc in 0..NUM_SCS - 1 {
            help_alloc_diff_size_and_alignment_singlethreaded(&sm, sc);
        }
    }

    #[test]
    fn a_few_allocs_and_a_dealloc_for_the_largest_slab() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let sc = NUM_SCS - 1;
        let smallest = help_pow2_usize(sc + NUM_SMALLEST_SLOT_SIZE_BITS - 1) + 1;
        let largest = help_pow2_usize(sc + NUM_SMALLEST_SLOT_SIZE_BITS);

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
    /// request call help_alloc_four_times_singlethreaded()
    fn help_alloc_diff_size_and_alignment_singlethreaded(sm: &Smalloc, sc: u8) {
        assert!(sc < NUM_SCS);

        let smallest = if sc == 0 {
            1
        } else {
            help_pow2_usize(sc + NUM_SMALLEST_SLOT_SIZE_BITS - 1) + 1
        };
        let largest = help_pow2_usize(sc + NUM_SMALLEST_SLOT_SIZE_BITS);
        for reqsize in [smallest, smallest + 1, largest - 2, largest - 1, largest] {
            // Generate alignments
            let mut reqalign = 1;
            loop {
                // Test this size/align combo
                help_alloc_four_times_singlethreaded(sm, reqsize, reqalign);
                reqalign *= 2;
                if alignedsize_or(reqsize, reqalign) > largest {
                    break;
                };
            }
        }
    }

    /// Return the sizeclass, slabnum, and slotnum
    fn help_ptr_to_loc(sm: &Smalloc, ptr: *const u8, layout: Layout) -> (u8, u8, u32) {
        assert!(layout.align().is_power_of_two()); // alignment must be a power of two
        
        let p_addr = ptr.addr();
        let smbp_addr = sm.get_sm_baseptr();

        assert!((p_addr >= smbp_addr) && (p_addr <= smbp_addr + HIGHEST_SMALLOC_SLOT_ADDR));

        let sc = const_shr_usize_u8(p_addr & SC_BITS_MASK, NUM_SLABNUM_AND_SLOTNUM_AND_DATA_BITS);
        let slabnum = const_shr_usize_u8(p_addr & SLABNUM_ADDR_MASK, NUM_SLOTNUM_AND_DATA_BITS);
        let slotnum = const_shr_usize_u32(p_addr & SLOTNUM_AND_DATA_MASK, sc + NUM_SMALLEST_SLOT_SIZE_BITS);

        (sc, slabnum, slotnum)
    }
        
    // /// Return the slab base pointer and free list head pointer for this slab.
    // fn help_slab_to_ptrs(sm: &Smalloc, sc: u32, slabnum: usize) -> (*mut u8, *mut u8) {
    //     assert!(sc < NUM_SMALL_SCS + NUM_MEDIUM_SCS + NUM_LARGE_SCS);
    //     assert!(if sc < NUM_SMALL_SCS { slabnum < one_shl(NUM_SMALL_SLABS_BITS) } else { slabnum == 0 });

    //     let smbp = sm.get_sm_baseptr();

    //     if sc == 0 {
    //         let slabbp = smbp | SIZECLASS_0_SC_INDICATOR_MASK | const_shl_usize(slabnum, NUM_SMALLEST_SLOT_SIZE_BITS);
    //         let flh_addr = (smbp | SIZECLASS_0_SLOTNUM_MASK) & !SIZECLASS_0_SLOTNUM_LSB_MASK;

    //         (slabbp, flh_addr)
    //     } else if sc < NUM_SMALL_SCS {
    //         let slabbp = smbp | const_shl_usize(SIZECLASS_0_SC_INDICATOR_MASK, sc) | const_shl_usize(slabnum, sc + NUM_SMALLEST_SLOT_SIZE_BITS);
    //         let slotnum_mask = const_shl_usize(SIZECLASS_0_SLOTNUM_MASK, sc);
    //         let flh_addr = slabbp | slotnum_mask;

    //         (slabbp, flh_addr)
    //     } else if sc < NUM_SMALL_SCS + NUM_MEDIUM_SCS {
    //         let slabbp = smbp | const_shl_usize(SIZECLASS_5_SC_INDICATOR_MASK, sc - 5);
    //         let slotnum_mask = const_shl_usize(SIZECLASS_5_SLOTNUM_MASK, sc - 5);
    //         let flh_addr = slabbp | slotnum_mask;

    //         (slabbp, flh_addr)
    //     } else {
    //         let slotsizebits = sc + NUM_SMALLEST_SLOT_SIZE_BITS;
    //         let slotnumbits = LARGE_SLOT_SIZE_BITS_PLUS_NUM_SLOTS_BITS - slotsizebits;
    //         let largesc = sc - NUM_SMALL_SCS - NUM_MEDIUM_SCS;
    //         let slabbp = smbp | const_shl_usize(largesc, LARGE_SLOT_SIZE_BITS_PLUS_NUM_SLOTS_BITS);
    //         let flh_addr = slabbp | const_shl_usize(gen_mask(slotnumbits), slotsizebits);

    //         (slabbp, flh_addr)
    //     }
    // }


    /// Allocate this size+align three times, then free the middle one, then allocate a fourth time,
    /// then assert that the fourth slot is the same as the second slot. Also asserts that the
    /// slabareanum is the same as this thread num.
    fn help_alloc_four_times_singlethreaded(sm: &Smalloc, reqsize: usize, reqalign: usize) {
        assert!(reqsize > 0);
        assert!(reqsize <= help_pow2_usize(NUM_SMALLEST_SLOT_SIZE_BITS + NUM_SCS - 1));
        assert!(reqalign > 0);

        let l = Layout::from_size_align(reqsize, reqalign).unwrap();

        let orig_slabareanum = get_slab_num();

        let p1 = unsafe { sm.alloc(l) };
        assert!(!p1.is_null(), "l: {l:?}");

        let (sc1, slabnum1, slotnum1) = help_ptr_to_loc(sm, p1, l);
        assert!(help_pow2_usize(sc1 + NUM_SMALLEST_SLOT_SIZE_BITS) >= reqsize);
        assert_eq!(slabnum1, orig_slabareanum);

        let p2 = unsafe { sm.alloc(l) };
        assert!(!p2.is_null());

        let (sc2, slabnum2, slotnum2) = help_ptr_to_loc(sm, p2, l);
        assert!(help_pow2_usize(sc2 + NUM_SMALLEST_SLOT_SIZE_BITS) >= reqsize);
        assert_eq!(slabnum2, slabnum1, "p1: {p1:?}, p2: {p2:?}, slabnum1: {slabnum1}, slabnum2: {slabnum2}, slotnum1: {slotnum1}, slotnum2: {slotnum2}");
        assert_eq!(slabnum2, orig_slabareanum);

        let p3 = unsafe { sm.alloc(l) };
        assert!(!p3.is_null());

        let (sc3, slabnum3, _slotnum3) = help_ptr_to_loc(sm, p3, l);
        assert!(help_pow2_usize(sc3 + NUM_SMALLEST_SLOT_SIZE_BITS) >= reqsize);
        assert_eq!(slabnum3, slabnum1);
        assert_eq!(slabnum3, orig_slabareanum);

        // Now free the middle one.
        unsafe { sm.dealloc(p2, l) };

        // And allocate another one.
        let p4 = unsafe { sm.alloc(l) };
        assert!(!p4.is_null());

        let (sc4, slabnum4, slotnum4) = help_ptr_to_loc(sm, p4, l);
        assert!(help_pow2_usize(sc4 + NUM_SMALLEST_SLOT_SIZE_BITS) >= reqsize);
        assert_eq!(slabnum4, slabnum1);
        assert_eq!(slabnum4, orig_slabareanum);

        // It should have allocated slot num 2 again
        assert_eq!(slotnum4, slotnum2);

        // Clean up so that we don't run out of slots while running these tests.
        unsafe { sm.dealloc(p1, l); }
        unsafe { sm.dealloc(p3, l); }
        unsafe { sm.dealloc(p4, l); }
    }

    // xxx consider reducing the code size of these tests...
    
    #[test]
    fn test_alloc_1_byte_then_dealloc() {
        let sm = Smalloc::new();
        let layout = Layout::from_size_align(1, 1).unwrap();
        let p = unsafe { sm.alloc(layout) };
        assert!(!p.is_null());
        unsafe { sm.dealloc(p, layout) };
    }

    #[test]
    fn main_thread_init() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();
    }

    #[test]
    fn threads_1_small_alloc_x() {
        help_test_multithreaded(1, 100, false, false, false);
    }

    #[test]
    fn threads_1_small_alloc_dealloc_x() {
        help_test_multithreaded(1, 100, true, false, false);
    }

    #[test]
    fn threads_1_small_alloc_dealloc_realloc_x() {
        help_test_multithreaded(1, 100, true, true, false);
    }

    #[test]
    fn threads_1_small_alloc_dealloc_with_writes_x() {
        help_test_multithreaded(1, 100, true, false, true);
    }

    #[test]
    fn threads_1_small_alloc_dealloc_realloc_with_writes_x() {
        help_test_multithreaded(1, 100, true, true, true);
    }

    #[test]
    fn threads_2_small_alloc_x() {
        help_test_multithreaded(2, 100, false, false, false);
    }

    #[test]
    fn threads_2_small_alloc_dealloc_x() {
        help_test_multithreaded(2, 100, true, false, false);
    }

    #[test]
    fn threads_2_small_alloc_dealloc_realloc_x() {
        help_test_multithreaded(2, 100, true, true, false);
    }

    #[test]
    fn threads_2_small_alloc_dealloc_with_writes_x() {
        help_test_multithreaded(2, 100, true, false, true);
    }

    #[test]
    fn threads_2_small_alloc_dealloc_realloc_with_writes_x() {
        help_test_multithreaded(2, 100, true, true, true);
    }

    #[test]
    fn threads_32_small_alloc_x() {
        help_test_multithreaded(32, 100, false, false, false);
    }

    #[test]
    fn threads_32_small_alloc_dealloc_x() {
        help_test_multithreaded(32, 100, true, false, false);
    }

    #[test]
    fn threads_32_small_alloc_dealloc_realloc_x() {
        help_test_multithreaded(32, 100, true, true, false);
    }

    #[test]
    fn threads_32_small_alloc_dealloc_with_writes_x() {
        help_test_multithreaded(32, 100, true, false, true);
    }

    #[test]
    fn threads_32_small_alloc_dealloc_realloc_with_writes_x() {
        help_test_multithreaded(32, 100, true, true, true);
    }

    #[test]
    fn threads_64_small_alloc_x() {
        help_test_multithreaded(64, 100, false, false, false);
    }

    #[test]
    fn threads_64_small_alloc_dealloc_x() {
        help_test_multithreaded(64, 100, true, false, false);
    }

    #[test]
    fn threads_64_small_alloc_dealloc_realloc_x() {
        help_test_multithreaded(64, 100, true, true, false);
    }

    #[test]
    fn threads_64_small_alloc_dealloc_with_writes_x() {
        help_test_multithreaded(64, 100, true, false, true);
    }

    #[test]
    fn threads_64_small_alloc_dealloc_realloc_with_writes_x() {
        help_test_multithreaded(64, 100, true, true, true);
    }

    #[test]
    fn threads_1_medium_alloc_x() {
        help_test_multithreaded(1, 100, false, false, false);
    }

    use crate::smallocb_allocator_config::gen_allocator;
    fn help_test_multithreaded(threads: u32, iters: u64, dealloc: bool, realloc: bool, writes: bool)  {
        let al = gen_allocator();

        let f = match (dealloc, realloc, writes) {
            (true, true, true) => { help_test_alloc_dealloc_realloc_with_writes }
            (true, true, false) => { help_test_alloc_dealloc_realloc }
            (true, false, true) => { help_test_alloc_dealloc_with_writes }
            (true, false, false) => { help_test_alloc_dealloc }
            (false, false, false) => { help_test_alloc }
            (false, _, _) => panic!()
        };

        help_test_multithreaded_with_allocator(f, threads, iters, al);
    }

    pub fn help_slotsize(sc: u8) -> usize {
        help_pow2_usize(sc + NUM_SMALLEST_SLOT_SIZE_BITS)
    }

    use std::sync::atomic::Ordering::Relaxed;
    fn help_set_flh_singlethreaded(flhbp: usize, sc: u8, slotnum: u32, slabnum: u8) {
        let flhi = NUM_SCS as u16 * slabnum as u16 + sc as u16;
        let flhptr = flhbp | const_shl_u16_usize(flhi, 3);
        let flha = unsafe { AtomicU64::from_ptr(flhptr as *mut u64) };

        // single threaded so don't bother with the counter
        flha.store(slotnum as u64, Relaxed);
    }

    /// If we've allocated all the slots from a slab, the next allocation of that sizeclass comes
    /// from a different slab of the same sizeclass. This test doesn't work for the largest
    /// sizeclass simply because the test assumes you can allocate 2 slots...
    fn help_test_overflow_to_other_slab(sc: u8) {
        debug_assert!(sc < NUM_SCS);

        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let siz = help_slotsize(sc);
        let l = Layout::from_size_align(siz, 1).unwrap();
        let alignedsizebits = req_to_slotsizebits(siz, 1);

        let slabnum = get_slab_num();

        let numslots = help_pow2_u32(NUM_MOST_SLOTS_BITS - sc);

        // Step 0: reach into the slab's `flh` and set it to almost the max slot number.
        let first_i = numslots - 3;
        let mut i = first_i;
        help_set_flh_singlethreaded(sm.get_flhs_baseptr(), sc, i, slabnum);

        // Step 1: allocate a slot and store it in local variables:
        let p1 = unsafe { sm.alloc(l) };
        assert!(!p1.is_null());

        let (sc1, slabnum1, slotnum1) = help_ptr_to_loc(&sm, p1, l);
        assert_eq!(sc1 + 2, alignedsizebits);
        assert_eq!(sc1, sc);
        assert_eq!(slotnum1, i);

        i += 1;
        
        // Step 2: allocate all the rest of the slots in this slab except the last one:
        while i < numslots - 2 {
            let pt = unsafe { sm.alloc(l) };
            assert!(!pt.is_null());

            let (scn, _slabnumn, slotnumn) = help_ptr_to_loc(&sm, pt, l);
            assert_eq!(scn + 2, alignedsizebits);
            assert_eq!(scn, sc);
            assert_eq!(slotnumn, i);

            i += 1
        }

        // Step 3: allocate the last slot in this slab and store it in local variables:
        let p2 = unsafe { sm.alloc(l) };
        assert!(!p2.is_null());

        let (sc2, slabnum2, slotnum2) = help_ptr_to_loc(&sm, p2, l);
        // Assert some things about the two stored slot locations:
        assert_eq!(sc2, sc, "numslots: {numslots}, i: {i}");
        assert_eq!(sc2 + 2, alignedsizebits);
        assert_eq!(slabnum1, slabnum2);
        assert_eq!(slotnum2, numslots - 2);

        // Step 4: allocate another slot and store it in local variables:
        let p3 = unsafe { sm.alloc(l) };
        assert!(!p3.is_null());

        let (sc3, slabnum3, slotnum3) = help_ptr_to_loc(&sm, p3, l);

        // The raison d'etre for this test: Assert that the newly allocated slot is in the same size
        // class but a different slab.
        assert_eq!(sc3, sc, "sc3: {sc3}, sc: {sc}, slabnum3: {slabnum3}, slabnum1: {slabnum1}, p3: {p3:?}, p2: {p2:?}");
        assert_ne!(slabnum3, slabnum1);
        assert_eq!(slotnum3, 0);

        // Step 5: If we alloc_slot() again on this thread, it will come from this new slab:
        let p4 = unsafe { sm.alloc(l) };
        assert!(!p4.is_null(), "sc3: {sc3}, sc: {sc}, slabnum3: {slabnum3}, slabnum1: {slabnum1}, p3: {p3:?}, p2: {p2:?}, slotnum3: {slotnum3}");

        let (sc4, slabnum4, slotnum4) = help_ptr_to_loc(&sm, p4, l);
        
        assert_eq!(sc4, sc3);
        assert!(sc4 + 2 >= alignedsizebits, "{sc4}, {alignedsizebits}");
        assert_eq!(slabnum4, slabnum3);
        assert_eq!(slotnum4, 1);
    }

    /// If we've allocated all of the slots from a slab, then the next allocation comes from the
    /// next-bigger slab. This test doesn't work on the biggest sizeclass (sc 30).
    fn help_test_overflow_to_other_sizeclass(sc: u8) {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let siz = help_slotsize(sc);
        let l = Layout::from_size_align(siz, 1).unwrap();
        let alignedsizebits = req_to_slotsizebits(siz, 1);
        let numslots = help_pow2_u32(NUM_MOST_SLOTS_BITS - sc);
        let slabnum = get_slab_num();

        // Step 0: allocate a slot and store information about it in local variables:
        let p1 = unsafe { sm.alloc(l) };
        assert!(!p1.is_null());
        
        let (sc1, slabnum1, _slotnum1) = help_ptr_to_loc(&sm, p1, l);

        assert_eq!(sc1, sc);
        assert_eq!(sc1 + 2, alignedsizebits);
        assert_eq!(slabnum1, slabnum);

        // Step 1: reach into each slab's `flh` and set it to the max slot number (which means the
        // free list is empty).
        for slabnum in 0..NUM_SLABS {
            help_set_flh_singlethreaded(sm.get_flhs_baseptr(), sc, numslots - 1, slabnum);
        }

        // Step 3: Allocate another slot and store it in local variables:
        let p2 = unsafe { sm.alloc(l) };
        assert!(!p2.is_null());

        let (sc2, slabnum2, slotnum2) = help_ptr_to_loc(&sm, p2, l);

        // The raison d'etre for this test: Assert that the newly allocated slot is in a bigger
        // size class, same areanum.
        assert_eq!(sc2, sc + 1, "sc2: {sc2}, sc: {sc}, slabnum2: {slabnum2}, slabnum1: {slabnum1}, p2: {p2:?}, p2: {p2:?}");
        assert_eq!(slabnum2, slabnum1);
        assert!(sc2 + 2 > alignedsizebits);
        assert_eq!(slotnum2, 0);

        // Step 5: If we alloc_slot() again on this thread, it will come from this new slab:
        let p3 = unsafe { sm.alloc(l) };
        assert!(!p3.is_null(), "sc2: {sc2}, sc: {sc}, slabnum2: {slabnum2}, slabnum1: {slabnum1}, p1: {p1:?}, p2: {p2:?}, slotnum2: {slotnum2}");

        let (sc3, slabnum3, slotnum3) = help_ptr_to_loc(&sm, p3, l);

        assert_eq!(sc3, sc2);
        assert_eq!(slabnum3, slabnum2);
        assert_eq!(slotnum3, 1);
    }

    /// If we've allocated all of the slots from a slab, the subsequent allocations come from a
    /// different slab in the same sizeclass.
    #[test]
    fn overflow_to_other_slab() {
        for sc in 0..NUM_SCS - 1 { 
            help_test_overflow_to_other_slab(sc);
        }
    }

    /// If we've allocated all of the slots from all slabs of this sizeclass, the subsequent
    /// allocations come from a bigger sizeclass.
    #[test]
    fn overflow_to_other_sizeclass() {
        for sc in 0..NUM_SCS - 2 { 
            help_test_overflow_to_other_sizeclass(sc);
        }
    }

    /// Overflow works with more threads than our internal lookup table has entries.
    #[test]
    fn overflow_with_many_threads() {
        // We need 320 threads to exceed the 10-index internal lookup table, but instead of spawning
        // a bunch of threads here we're just going to reach in and set the thread num .
        THREAD_NUM.set(Some(320));
        help_test_overflow_to_other_slab(0);
    }

    #[test]
    /// If we've allocated all of the slots from the largest large-slots slab, the next allocation
    /// fails.
    fn overflow_from_largest_large_slots_slab() {
        let sm = Smalloc::new();
        sm.idempotent_init().unwrap();

        let sc = NUM_SCS - 1;
        let siz = help_slotsize(sc);
        let l = Layout::from_size_align(siz, 1).unwrap();

        // Step 0: reach into each slab's `flh` and set it to the max slot number.
        for slabnum in 0..NUM_SLABS {
            help_set_flh_singlethreaded(sm.get_flhs_baseptr(), sc, 1, slabnum);
        }

        // Step 1: allocate a slot
        let p1 = unsafe { sm.alloc(l) };
        assert!(p1.is_null(), "p1: {p1:?}, sc: {sc}, l: {l:?}");
    }
}

