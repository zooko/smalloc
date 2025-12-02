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

const NUM_SMALLEST_SLOT_SIZE_BITS: u8 = 2;
const NUM_SLABS_BITS: u8 = 6;
const NUM_SCS: u8 = 32;


// --- Constant values determined by the constants above ---

// See the ASCII-art map in `README.md` for where these bit masks fit in.
const NUM_SLABS: u8 = 2u8.pow(NUM_SLABS_BITS as u32);
const NUM_FLHS: u16 = NUM_SLABS as u16 * NUM_SCS as u16; // 992
const NUM_SLOTNUM_AND_DATA_BITS: u8 = NUM_SCS + NUM_SMALLEST_SLOT_SIZE_BITS; // 33
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
const SIZE_OF_SLABS_AND_FLHS: usize = FLHS_BASE + SIZE_OF_FLHS; // 0b1111011111100000000000000000001111100000000

// We need to allocate extra bytes so that we can align the smalloc base pointer so that all of the
// trailing bits (all of the bits covered by the SMALLOC_ADDRESS_BITS_MASK) of the smalloc base
// pointer are zeros.
const BASEPTR_ALIGN: usize = SIZE_OF_SLABS_AND_FLHS.next_power_of_two(); // 0b10000000000000000000000000000000000000000000 
const SMALLOC_ADDRESS_BITS_MASK: usize = BASEPTR_ALIGN - 1; // 0b1111111111111111111111111111111111111111111 
const TOTAL_VIRTUAL_MEMORY: usize = SIZE_OF_SLABS_AND_FLHS + SMALLOC_ADDRESS_BITS_MASK; // 0b11111011111100000000000000000001111011111111 == 17_313_013_178_111

// --- Lookup tables of constant values determined by the constants above ---

//xxx asm-inspect and bench vs const_shl_u8_usize (again)?
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

//xxx asm-inspect and bench vs const_shl_u8_usize (again)?
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

use std::cell::UnsafeCell;

pub struct Smalloc {
    inner: UnsafeCell<SmallocInner>,
}

struct SmallocInner {
    sysbp: usize,
    smbp: usize,
    flhsbp: usize,
}

unsafe impl Sync for Smalloc {}

impl Default for Smalloc {
    fn default() -> Self {
        Self::new()
    }
}

//xxx17use crate::platformalloc::{prefetch_read,prefetch_write};
impl Smalloc {
    pub const fn new() -> Self {
        Self {
            inner: UnsafeCell::new(SmallocInner {
                sysbp: 0,
                smbp: 0,
                flhsbp: 0,
            }),
        }
    }

    fn inner(&self) -> &SmallocInner {
        unsafe { &*self.inner.get() }
    }

    #[allow(clippy::mut_from_ref)]
    fn inner_mut(&self) -> &mut SmallocInner {
        unsafe { &mut *self.inner.get() }
    }

    /// For testing only. Do not use in production code.
    pub fn get_total_virtual_memory(&self) -> usize {
        TOTAL_VIRTUAL_MEMORY
    }

    /// For testing only. Do not use in production code.
    fn dump_map_of_slabs(&self) {
        let inner = self.inner();

        // Dump a map of the slabs
        let mut fullslots = 0;
        let mut fulltotsize = 0;
        for sc in 0..NUM_SCS {
            let mut scfullslots = 0;
            let mut scfulltotsize = 0;

            print!("{sc:2} ");

            let highestslotnum = const_gen_mask_u32(NUM_SCS - sc);
            let slotsize = 2u64.pow((sc + NUM_SMALLEST_SLOT_SIZE_BITS) as u32);
            print!("slots: {}, slotsize: {}", highestslotnum, slotsize);

            for slabnum in 0..NUM_SLABS {
//                print!(" {slabnum}");
                
                let headelement = help_get_flh(inner.flhsbp, sc, slabnum);
                if headelement == highestslotnum {
                    // full
                    print!("X");
                    scfullslots += highestslotnum;
                    scfulltotsize += (highestslotnum as u64) * slotsize;
                } else {
                    print!(".");
                }
            }
            println!(" slots: {scfullslots} size: {scfulltotsize}");
            fullslots += scfullslots;
            fulltotsize += scfulltotsize;
        }
        println!(" totslots: {fullslots}, totsize: {fulltotsize}");
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
        let inner = self.inner_mut();
        assert!(inner.sysbp == 0);
        inner.sysbp = sys_alloc(TOTAL_VIRTUAL_MEMORY).unwrap().addr();
        assert!(inner.sysbp != 0);
        inner.smbp = inner.sysbp.next_multiple_of(BASEPTR_ALIGN);
        inner.flhsbp = inner.smbp + FLHS_BASE;
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

            // Write it into the new slot's link
            let new_slot_p = Self::linkptr(slabbp, newslotnum, slotsizebits);
            unsafe { *new_slot_p = next_entry_link };

            // Create a new flh word, pointing to the new entry
            let newflhdword = ((counter as u64 + 1) << 32) | newslotnum as u64;

            // Compare and exchange
            if flh.compare_exchange(flhdword, newflhdword, AcqRel, Acquire).is_ok() { // xxx weaker ordering constraints okay?
	        // prefetch the next link so we can quickly write to it next time
		//xxx14prefetch_write(Self::linkptr(slabbp, newslotnum, slotsizebits));
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

    #[inline(always)]//xxx try removing inline-always
    fn decode_next_entry_link(baseslotnum: u32, codeword: u32, highestslotnum: u32) -> u32 {
        // The baseslotnum cannot be the sentinel slot num.
        debug_assert!(baseslotnum < highestslotnum);

        baseslotnum.wrapping_add(codeword).wrapping_add(1) & highestslotnum
    }
        
    fn linkptr(slabbp: usize, slotnum: u32, slotsizebits: u8) -> *mut u32 {
//xxx10        // This thing about or'ing in a few bits from the least significant bits of the slotnum is to utilize more of the sets from the associative cache in the L1...
//xxx10
//xxx10	// symbolify 6 (it's cache line size)
//xxx10        let num_bits_of_slotnum = max(6, slotsizebits) - 6;
//xxx10        let mask = const_gen_mask_u32(num_bits_of_slotnum);
//xxx10        let slotnum_bits = const_shl_u32_usize(slotnum & mask, 6);
//xxx10        (slabbp | const_shl_u32_usize(slotnum, slotsizebits) | slotnum_bits) as *mut u32
        Self::slotptr(slabbp, slotnum, slotsizebits) as *mut u32
    }

    fn slotptr(slabbp: usize, slotnum: u32, slotsizebits: u8) -> *mut u8 {
        (slabbp | const_shl_u32_usize(slotnum, slotsizebits)) as *mut u8
    }

 
    /// Allocate a slot from this slab by popping the free-list-head. Return the resulting pointer
    /// as a usize, or null pointer (0) if this slab is full.
    ///
    /// `highestslotnum` is the slotnum of the sentinel slot (`numslots - 1`). It is also used to
    /// compute numbers modulo `numslots` with `& highestslotnum` instead of with `% numslots`, and
    /// it is used in `debug_asserts`.
    fn pop_slot_from_freelist(&self, slabbp: usize, flh: &AtomicU64, highestslotnum: u32, slotsizebits: u8) -> usize {
        debug_assert!(slabbp != 0);
        debug_assert!((slabbp >= self.inner().smbp) && (slabbp <= (self.inner().smbp + HIGHEST_SMALLOC_SLOT_ADDR)), "slabbp: {slabbp:x}, smbp: {:x}, highest_addr: {:x}", self.inner().smbp, self.inner().smbp + HIGHEST_SMALLOC_SLOT_ADDR);
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

            let curfirstentrylink_p = Self::linkptr(slabbp, curfirstentryslotnum, slotsizebits);

            // Read the bits from the first entry's link and decode them into a slot number. These
            // bits might be "invalid" in the sense of not encoding a slot number, if the flh has
            // changed since we read it above and another thread has started using these bits for
            // something else (e.g. user data or another linked list update). That's okay because in
            // that case our attempt to update the flh below with information derived from these
            // bits will fail.
            let curfirstentrylink_v = unsafe { *curfirstentrylink_p };

            let newfirstentryslotnum: u32 = Self::decode_next_entry_link(curfirstentryslotnum, curfirstentrylink_v, highestslotnum);

            // Create a new flh word, with the new slotnum, pointing to the new first slot
            let counter: u32 = (flhdword >> 32) as u32;
            let newflhdword = ((counter as u64 + 1) << 32) | newfirstentryslotnum as u64;

            // Compare and exchange
            if flh.compare_exchange(flhdword, newflhdword, AcqRel, Acquire).is_ok() { // xxx weaker ordering constraints okay?
	        // prefetch the next link (which is now the first link) in the free list
		//xxx14prefetch_read(Self::linkptr(slabbp, newfirstentryslotnum, slotsizebits));
	        //xxx6 maybe compute this from curfirstentrylink_p?
	        let curfirstentry_p = Self::slotptr(slabbp, curfirstentryslotnum, slotsizebits) as usize;
                debug_assert!((curfirstentry_p >= self.inner().smbp) && (curfirstentry_p <= (self.inner().smbp + HIGHEST_SMALLOC_SLOT_ADDR)), "curfirstentry_p: {curfirstentry_p:x}, smbp: {:x}, slabbp: {slabbp:x}, highest_addr: {:x}", self.inner().smbp, self.inner().smbp + HIGHEST_SMALLOC_SLOT_ADDR);

                break curfirstentry_p;
            }
        }
    }
}

use std::cmp::max;
//xxx18use std::thread;

fn help_get_flh(flhbp: usize, sc: u8, slabnum: u8) -> u32 {
    let flhi = NUM_SCS as u16 * slabnum as u16 + sc as u16;
    let flhptr = flhbp | const_shl_u16_usize(flhi, 3);
    let flha = unsafe { AtomicU64::from_ptr(flhptr as *mut u64) };
    let flhdword = flha.load(Relaxed);
    (flhdword & u32::MAX as u64) as u32
}

unsafe impl GlobalAlloc for Smalloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let inner = self.inner();

        debug_assert!(inner.smbp != 0);
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

        let highestslotnum = const_gen_mask_u32(NUM_SCS - sc);

        // If the slab is full, we'll switch to another slab in this same sizeclass.
        let orig_slabnum = get_slab_num();
        let mut slabnum = orig_slabnum;
                
        loop {
            // The slabbp is the smbp with the size class bits and the slabnum bits set.
	    // xxx benchmark and examine asm diff
            let slabbp = inner.smbp | SCBITS_LUT[sc as usize] | SLABNUMBITS_LUT[slabnum as usize];
            //let slabbp = inner.smbp | const_shl_u8_usize(sc, NUM_SLABNUM_AND_SLOTNUM_AND_DATA_BITS) | const_shl_u8_usize(slabnum, NUM_SLOTNUM_AND_DATA_BITS);//xxx benchmark and asm-compare

            debug_assert!((slabbp >= inner.smbp) && (slabbp <= (inner.smbp + HIGHEST_SMALLOC_SLOT_ADDR)), "slabbp: {slabbp:x}, smbp: {:x}, highest_addr: {:x}", inner.smbp, inner.smbp + HIGHEST_SMALLOC_SLOT_ADDR);
            debug_assert!(help_trailing_zeros_usize(slabbp) >= slotsizebits);

            let flhi = NUM_SCS as u16 * slabnum as u16 + sc as u16;
            let flhptr = inner.flhsbp | const_shl_u16_usize(flhi, 3);
            let flh = unsafe { AtomicU64::from_ptr(flhptr as *mut u64) };

            let p_addr = self.pop_slot_from_freelist(slabbp, flh, highestslotnum, slotsizebits);

            debug_assert!((p_addr == 0) || (p_addr >= inner.smbp) && (p_addr <= (inner.smbp + HIGHEST_SMALLOC_SLOT_ADDR)), "p_addr: {p_addr:x}, smbp: {:x}, highest_addr: {:x}", inner.smbp, inner.smbp + HIGHEST_SMALLOC_SLOT_ADDR);

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
                let doublesize_layout = Layout::from_size_align(reqsiz * 2, reqalign).unwrap();//xxx use the unsafe version and use a shl
                return unsafe { self.alloc(doublesize_layout) }
            }
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        debug_assert!(!ptr.is_null());
        debug_assert!(layout.align().is_power_of_two()); // alignment must be a power of two

        let p_addr = ptr.addr();

        let inner = self.inner();

        // To be valid, the pointer has to be greater than or equal to the smalloc base pointer and
        // less than or equal to the highest slot pointer.
        let highest_addr = inner.smbp + HIGHEST_SMALLOC_SLOT_ADDR;

        assert!((p_addr >= inner.smbp) && (p_addr <= highest_addr), "p_addr: {p_addr}, smbp: {}, highest_addr: {highest_addr}", inner.smbp);

        // Okay now we know that it is a pointer into smalloc's region.

        let slabbp = p_addr & !SLOTNUM_AND_DATA_MASK;
        let sc = const_shr_usize_u8(p_addr & SC_BITS_MASK, NUM_SLABNUM_AND_SLOTNUM_AND_DATA_BITS);
        let slotsizebits = sc + NUM_SMALLEST_SLOT_SIZE_BITS;
        let slotnum = const_shr_usize_u32(p_addr & SLOTNUM_AND_DATA_MASK, slotsizebits);
        let slabnum = const_shr_usize_u8(p_addr & SLABNUM_ADDR_MASK, NUM_SLOTNUM_AND_DATA_BITS);
        let highestslotnum = const_gen_mask_u32(NUM_SCS - sc);

        let flhi = NUM_SCS as u16 * slabnum as u16 + sc as u16;
        let flhptr = inner.flhsbp | const_shl_u16_usize(flhi, 3);
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
use std::sync::atomic::Ordering::{AcqRel, Acquire};
use plat::plat::sys_alloc;
use std::ptr::{copy_nonoverlapping, null_mut};
//xxx16use thousands::Separable;

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

/// Return the number of significant bits in the aligned size. This is the log base 2 of the size of
/// slot required to hold requests of this size and alignment, but a minimum of 2 since that is log
/// base 2 of the slots of our smallest sizeclass.
fn req_to_slotsizebits(size: usize, align: usize) -> u8 {
    debug_assert!(size > 0);
    debug_assert!(align > 0);
    max(NUM_SMALLEST_SLOT_SIZE_BITS, usize::BITS as u8 - min(help_leading_zeros_usize(size - 1), help_leading_zeros_usize(align - 1)))
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

pub use smalloc_macros::smalloc_main;

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
