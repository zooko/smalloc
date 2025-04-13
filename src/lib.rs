//#![doc=include_str!("../README.md")]
#![feature(pointer_is_aligned_to)]
#![allow(clippy::needless_range_loop)] // I like using needless range loops more than I like using enumerate.

//XXX consider refactoring it so the bigger-than-cacheliney slabs are not "in" "area 0".

// These slot sizes were chosen by calculating how many objects of this size would fit into the least-well-packed 64-byte cache line when we lay out objects of these size end-to-end over many successive 64-byte cache lines. If that makes sense. The worst-case number of objects that can be packed into a cache line can be up 2 fewer than the best-case, since the first object in this cache line might cross the cache line boundary and only the last part of the object is in this cache line, and the last object in this cache line might similarly be unable to fit entirely in and only the first part of it might be in this cache line. So this "how many fit" number below counts only the ones that entirely fit in, even when we are laying out objects of this size one after another (with no padding) across many cache lines. So it can be 0, 1, or 2 fewer than you might think. (Excluding any sizes which are smaller and can't fit more -- in the worst case -- than a larger size.)

// slabnum:      size:    number:
//        0          1         64
//        1          2         32
//        2          3         20
//        3          4         16
//        4          5         12
//        5          6         10
//        6          8          8
//        7          9          6
//        8         10          5
//        9         16          4
//       10         32          2

pub const NUM_SLABS: usize = 18;
// How many slabs have slots that we can pack more than one into a 64-byte cache line?
pub const NUM_SLABS_CACHELINEY: usize = 11;
pub const SLABNUM_TO_SLOTSIZE: [usize; NUM_SLABS] = [ 1, 2, 3, 4, 5, 6, 8, 9, 10, 16, 32, 64, 128, 256, 512, 1024, 2048, SIZE_OF_LARGE_SLOTS];

const SIZE_OF_LARGE_SLOTS: usize = 4_194_304; // 4 mebibytes

pub const NUM_SLOTS: usize = 20_971_520; // 20 * 2^20

const CACHELINE_SIZE: usize = 64;

// The per-slab variables and the free list entries have this size in bytes.
const WORDSIZE: usize = 4;

const NUM_SEPARATE_FREELISTS: usize = 3; // Number of separate free lists for slabs whose slots are too small to hold a 4-byte word (slab numbers 0, 1, and 2)
// The next_multiple_of() is to start the *next* thing on a cacheline boundary.
const SEPARATE_FREELIST_SPACE: usize = (NUM_SLOTS * WORDSIZE).next_multiple_of(CACHELINE_SIZE); // Size of each of the separate free lists

pub const NUM_AREAS: usize = 64;

// Total number of slabs in all areas.
const TOTAL_SLABS: usize = NUM_SLABS + (NUM_AREAS - 1) * NUM_SLABS_CACHELINEY;

// The next_multiple_of() is to start the *next* thing on a cacheline boundary.
const VARIABLES_SPACE: usize = (TOTAL_SLABS * WORDSIZE * 2).next_multiple_of(CACHELINE_SIZE);

// XXX Make sure each of the separate free lists starts at 64-byte alignment.
const SEPARATE_FREELISTS_BASE_OFFSET: usize = VARIABLES_SPACE;

// The next_multiple_of() is to start the *next* thing on a cacheline boundary.
const SEPARATE_FREELISTS_SPACE: usize = (NUM_SEPARATE_FREELISTS * SEPARATE_FREELIST_SPACE * NUM_AREAS).next_multiple_of(CACHELINE_SIZE);

pub const DATA_SLABS_BASE_OFFSET: usize = VARIABLES_SPACE + SEPARATE_FREELISTS_SPACE;

// The next_multiple_of() is to start the *next* thing on a cacheline boundary.
const DATA_SLABS_AREA_0_SIZE: usize = (sum_column_sizes(NUM_SLABS) * NUM_SLOTS).next_multiple_of(CACHELINE_SIZE);
// The next_multiple_of() is to start the *next* thing on a cacheline boundary.
const DATA_SLABS_AREA_NOT0_SIZE: usize = (sum_column_sizes(NUM_SLABS_CACHELINEY) * NUM_SLOTS).next_multiple_of(CACHELINE_SIZE);

/// The sum of the sizes of the first `uptoslabnum` rows of a column.
const fn sum_column_sizes(uptoslabnum: usize) -> usize {
    let mut index = 0;
    let mut sum = 0;
    while index < uptoslabnum {
	sum += SLABNUM_TO_SLOTSIZE[index];
	index += 1;
    }
    sum
}

const DATA_SLABS_SPACE: usize = DATA_SLABS_AREA_0_SIZE + DATA_SLABS_AREA_NOT0_SIZE * (NUM_AREAS-1);

pub const TOTAL_VIRTUAL_MEMORY: usize = VARIABLES_SPACE + SEPARATE_FREELISTS_SPACE + DATA_SLABS_SPACE;

const fn generate_offset_of_var() -> [[usize; NUM_SLABS]; NUM_AREAS] {
    let mut locallut = [[0; NUM_SLABS]; NUM_AREAS];

    let mut offset_of_vars = 0;
    let mut areanum = 0;

    // First generate the offsets of vars for area 0
    let mut slabnum = 0;
    while slabnum < NUM_SLABS {
	locallut[areanum][slabnum] = offset_of_vars;
	offset_of_vars += 2 * WORDSIZE;
	slabnum += 1;
    }

    // Then areas 1 through 63 (inclusive)
    areanum = 1;
    while areanum < NUM_AREAS {
	let mut slabnum = 0;
	while slabnum < NUM_SLABS_CACHELINEY {
	    locallut[areanum][slabnum] = offset_of_vars;
	    offset_of_vars += 2 * WORDSIZE;
	    slabnum += 1;
	}
	areanum += 1;
    }

    locallut
}

/// The offset from the base pointer, not from the beginning of the data region.
pub const OFFSET_OF_VAR: [[usize; NUM_SLABS]; NUM_AREAS] = generate_offset_of_var();

#[derive(Default, PartialEq, Eq, Debug, Copy, Clone)]
pub struct SlotLocation {
    pub areanum: usize,
    pub slabnum: usize,
    pub slotnum: usize
}

/// Return the offset (in units of a byte) from the base pointer (self.baseptr), not from the beginning of the data region.
// Free list entries can live at non-aligned locations, such as when we re-use a 5-byte-wide slot to hold a free list entry in its first 4 bytes, and then likewise with next next 5-byte-wide slot and so forth. Therefore, we count the offset to find a free list entry (from the base ptr) in bytes, not in 4-byte words (even though each free list entry is itself 4 bytes).
const fn offset_of_free_list_entry(sl: SlotLocation) -> usize {
    let SlotLocation { areanum, slabnum, slotnum } = sl;
    
    assert!(areanum == 0 || slabnum < NUM_SLABS_CACHELINEY);

    if slabnum < NUM_SEPARATE_FREELISTS {
	// Separate free list spaces.

	// area-first then slab then slot...
	let pastslots = areanum * NUM_SEPARATE_FREELISTS * NUM_SLOTS + slabnum * NUM_SLOTS + slotnum;

	// The separate free lists are laid out after the variables...
	SEPARATE_FREELISTS_BASE_OFFSET + pastslots * WORDSIZE
    } else {
	// Intrusive free list -- the location of the next-slot space is also the location of the data slot.
	sl.offset_of_slot()
    }
}

/// Return the number of the smallest slab that can hold items with `layout`.
pub const fn layout_to_slabnum(layout: Layout) -> usize {
    let size = layout.size();
    let alignment = layout.align();

    assert!(alignment > 0 && (alignment & (alignment - 1)) == 0, "alignment must be a power of two"); // benchmarks show this bittwiddling expression is a teeeny bit more efficient than the builtin power-of-two function (on some x86-64 systems but not others, and on Apple M4 Max).

    // Round up size to the nearest multiple of alignment:
    let alignedsize: usize = ((size - 1) | (alignment - 1)) + 1;

    let mut slabnum = 0;
    while slabnum < NUM_SLABS {
	if alignedsize <= SLABNUM_TO_SLOTSIZE[slabnum] {
	    return slabnum;
	}
	slabnum += 1;
    }

    NUM_SLABS
}

impl SlotLocation {
    /// Return the offset (in units of a byte) from the base pointer (self.baseptr), not from the beginning of the data slabs region.
    // Data slots can live at locations that aren't nice multiples of a certain number of bytes, since data slots are of various sizes and are packed in next to each other. Therefore, we count the offset to find a data slot (from the data slabs base ptr) in bytes.
    pub const fn offset_of_slot(&self) -> usize {
        assert!(self.areanum < NUM_AREAS);
        assert!(self.slabnum < NUM_SLABS);
        assert!(self.slotnum < NUM_SLOTS);

        assert!(self.areanum == 0 || self.slabnum < NUM_SLABS_CACHELINEY);

        let mut offset = DATA_SLABS_BASE_OFFSET;

        let slotsize = SLABNUM_TO_SLOTSIZE[self.slabnum];

        if self.areanum == 0 {
	    // Count past any preceding slabs in area 0
	    let past_slabs_area_0_size = sum_column_sizes(self.slabnum) * NUM_SLOTS;

	    offset += past_slabs_area_0_size;

	    // Count past any preceding slots in our slab in area 0
	    let past_slab_size = self.slotnum * slotsize;

	    offset += past_slab_size;
        } else {
	    // Count past area 0
	    let past_area_0_size = DATA_SLABS_AREA_0_SIZE;

	    offset += past_area_0_size;

	    // Count past other areas
	    let num_other_areas = self.areanum - 1;
	    let past_other_areas_size = num_other_areas * DATA_SLABS_AREA_NOT0_SIZE;

	    offset += past_other_areas_size;

	    // Count past other slabs in this area
	    let past_slabs_size = sum_column_sizes(self.slabnum) * NUM_SLOTS;
	    
	    offset += past_slabs_size;

	    // Count past other slots in this slab.
	    let past_slots_size = self.slotnum * slotsize;

	    offset += past_slots_size;
        }

        offset
    }
}

use core::alloc::{GlobalAlloc, Layout};

use std::sync::atomic::{AtomicU32, AtomicU8, AtomicPtr, Ordering};
mod platformalloc;
use platformalloc::vendor::{sys_alloc, sys_dealloc, sys_realloc};
use std::ptr::{null_mut, copy_nonoverlapping};

pub struct Smalloc {
    initlock: spin::Mutex<()>,
    baseptr: AtomicPtr<u8>,
}

impl Default for Smalloc {
    fn default() -> Self {
	Self::new()
    }
}

/// Returns Some(SlotLocation) if the ptr pointed to a slot, else None (meaning that the pointer must have been allocated with sys_alloc() instead.
pub fn slotlocation_of_ptr(baseptr: *mut u8, p: *mut u8) -> Option<SlotLocation> {
    // If the pointer is before our base pointer or after the end of our allocated space, then it must have come from an oversized alloc where we fell back to `sys_alloc()`. (Assuming that the user code never passes anything other a pointer that it previous got from our `alloc()`, to `dealloc().)

    // Now there is no well-specified way to compare two pointers if they aren't part of the same allocation, which this p and our baseptr might not be.
    // .addr() is our way of promising the Rust compiler that we won't round-trip these values back into pointers from usizes and use them, below. See https://doc.rust-lang.org/nightly/std/ptr/index.html#strict-provenance
    
    let p_as_usize = p.addr();
    let baseptr_as_usize = baseptr.addr();
    if p_as_usize < baseptr_as_usize {
	return None;
    }
    if p_as_usize >= baseptr_as_usize + TOTAL_VIRTUAL_MEMORY {
	return None;
    }

    // If it wasn't a pointer from a system allocation, then it must be a pointer into one of our slots.
    assert!(p_as_usize >= baseptr_as_usize + DATA_SLABS_BASE_OFFSET);

    // Okay now we know that it is pointer into our allocation, so it is safe to subtract baseptr from it.
    let offset = unsafe { p.offset_from(baseptr) };
    assert!(offset >= DATA_SLABS_BASE_OFFSET as isize);
    let data_offset = offset as usize - DATA_SLABS_BASE_OFFSET;

    let (areanum, within_area_offset) = if data_offset < DATA_SLABS_AREA_0_SIZE {
	(0, data_offset)
    } else {
	let residual_offset = data_offset - DATA_SLABS_AREA_0_SIZE;
	let num_other_areas = residual_offset / DATA_SLABS_AREA_NOT0_SIZE;
	let within_area_offset = residual_offset - num_other_areas * DATA_SLABS_AREA_NOT0_SIZE;

	(num_other_areas+1, within_area_offset)
    };

    let mut slabnum = 0;
    let mut within_slab_offset = within_area_offset;
    let mut slotnum = 0;

    while slabnum < NUM_SLABS {
	let slotsize = SLABNUM_TO_SLOTSIZE[slabnum];
	let slabsize = slotsize * NUM_SLOTS;

	if within_slab_offset < slabsize {
	    // This offset is within this slab.
	    assert!(!slotsize.is_power_of_two() || p.is_aligned_to(slotsize), "slotsize: {}, p: {:?}", slotsize, p);
	    slotnum = within_slab_offset / slotsize;
	    assert!(slotnum * slotsize == within_slab_offset);
            assert!(p.is_aligned_to(CACHELINE_SIZE) || slotnum > 0, "p: {:?}, areanum: {}, slabnum: {}, slotnum: {}", p, areanum, slabnum, slotnum);

	    break;
	}

	slabnum += 1;
	assert!(within_slab_offset >= slabsize);
	within_slab_offset -= slabsize;
    }

    Some(SlotLocation {
	areanum,
	slabnum,
	slotnum
    })
}

impl Smalloc {
    pub const fn new() -> Self {
	Self {
	    initlock: spin::Mutex::new(()),
	    baseptr: AtomicPtr::<u8>::new(null_mut()),
	}
    }
    
    fn idempotent_init(&self) -> *mut u8 {
	let mut p: *mut u8;

	p = self.baseptr.load(Ordering::Acquire); // XXX ?? relaxed ???
	if !p.is_null() {
	    return p;
	}

	// acquire spin lock
	let _guard = self.initlock.lock();

	p = self.baseptr.load(Ordering::Acquire);
	if p.is_null() { // XXX come back and figure out if Relaxed is okay. :-)
	    p = sys_alloc(TOTAL_VIRTUAL_MEMORY);
	    assert!(!p.is_null());
            assert!(p.is_aligned_to(4096), "p: {:?}", p); // This is just testing my understanding that mmap() always returns page-aligned pointers (and that page-alignment is always a multiple of 4096.)
	    self.baseptr.store(p, Ordering::Release); // XXX come back and figure out if Relaxed would be okay. :-)  Jack says never. :-)
	}

	p
    }

    fn get_baseptr(&self) -> *mut u8 {
	// It is not okay to call this in alloc()/idempotent_init()
	let p = self.baseptr.load(Ordering::Relaxed);
	assert!(!p.is_null());
	p
    }

    /// Returns 0 if the free list is empty or one greater than the index of the former head of the free list.
    /// See "Thread-Safe State Changes" in README.md
    fn pop_flh(&self, areanum: usize, slabnum: usize) -> usize {
	assert!(areanum == 0 || slabnum < NUM_SLABS_CACHELINEY);

	let baseptr = self.get_baseptr();
	
	let offset_of_flh = OFFSET_OF_VAR[areanum][slabnum] + 1; // units of 1 byte
	let u8_ptr_to_flh = unsafe { baseptr.add(offset_of_flh) };
	assert!(u8_ptr_to_flh.is_aligned_to(WORDSIZE)); // need 4-byte alignment for atomic ops (on at least some/most platforms)
	let u32_ptr_to_flh = u8_ptr_to_flh.cast::<u32>();

	let flh = unsafe { AtomicU32::from_ptr(u32_ptr_to_flh) };
	loop {
	    let a: u32 = flh.load(Ordering::Relaxed);
	    assert!(a as usize <= NUM_SLOTS);
	    //xxx2 add assertionsbb everywhere about value of flh

	    if a == 0 {
		return 0;
	    }

	    let offset_of_next = offset_of_free_list_entry(SlotLocation { areanum, slabnum, slotnum: (a-1) as usize }); // units of 1 byte
	    let u8_ptr_to_next = unsafe { baseptr.add(offset_of_next) }; // note this isn't necessarily aligned
	    let u32_ptr_to_var = u8_ptr_to_next.cast::<u32>();
	    let b: u32 = unsafe { u32_ptr_to_var.read_unaligned() };
	    assert!(b as usize <= NUM_SLOTS);

	    if flh.compare_exchange_weak(a, b, Ordering::Acquire, Ordering::Relaxed).is_ok() {
		return a as usize;
	    }
	}
    }

    fn push_flh(&self, sl: SlotLocation) {
	let SlotLocation { areanum, slabnum, slotnum } = sl;
	assert!(areanum == 0 || slabnum < NUM_SLABS_CACHELINEY);
	assert!(slabnum < NUM_SLABS);
	assert!(slotnum < NUM_SLOTS);

	let baseptr = self.get_baseptr();
	
	let i = slotnum as u32;

	let offset_of_flh = OFFSET_OF_VAR[areanum][slabnum] + 1; // units of 1 byte
	let u8_ptr_to_flh = unsafe { baseptr.add(offset_of_flh) };
	assert!(u8_ptr_to_flh.is_aligned_to(WORDSIZE)); // need 4-byte alignment for atomic ops (on at least some/most platforms)
	let u32_ptr_to_flh = u8_ptr_to_flh.cast::<u32>();
	let flh = unsafe { AtomicU32::from_ptr(u32_ptr_to_flh) };

	let offset_of_next = offset_of_free_list_entry(sl); // units of 1 byte
	let u8_ptr_to_next = unsafe { baseptr.add(offset_of_next) }; // note this isn't necessarily aligned
	let u32_ptr_to_next: *mut u32 = u8_ptr_to_next.cast::<u32>();

	loop {
	    let a: u32 = flh.load(Ordering::Relaxed);
	    assert!(a as usize <= NUM_SLOTS);
	    unsafe { u32_ptr_to_next.write_unaligned(a) };
	    if flh.compare_exchange_weak(a, i, Ordering::Acquire, Ordering::Relaxed).is_ok() {
		return;
	    }
	}
    }

    /// Returns the index of the next never-before-allocated slot. Returns NUM_SLOTS in the case that all slots have been allocated.
    fn increment_eac(&self, areanum: usize, slabnum: usize) -> usize {
	assert!(areanum == 0 || slabnum < NUM_SLABS_CACHELINEY);
	assert!(slabnum < NUM_SLABS);

	let baseptr = self.get_baseptr();
	
	let offset_of_eac = OFFSET_OF_VAR[areanum][slabnum]; // units of 1 byte
	let u8_ptr_to_eac = unsafe { baseptr.add(offset_of_eac) };
	assert!(u8_ptr_to_eac.is_aligned_to(WORDSIZE)); // need 4-byte alignment for atomic ops (on at least some/most platforms)
	let u32_ptr_to_eac = u8_ptr_to_eac.cast::<u32>();
	let eac = unsafe { AtomicU32::from_ptr(u32_ptr_to_eac) };

	// If eac is maxed out -- at NUM_SLOTS -- another thread has incremented past NUM_SLOTS but not yet decremented it, then this could exceed NUM_SLOTS. However, if this has happened more than a few times simultaneously, such that eac is more than a small number higher than NUM_SLOTS, then something is very wrong and we should panic to prevent some kind of failure case or exploitation. If eac reached 2^32 then it would wrap, and we want to panic long before that.
	assert!((eac.load(Ordering::Relaxed) as usize) < NUM_SLOTS + 64); // keep this assert for runtime!

	let nexteac = eac.fetch_add(1, Ordering::Acquire); // reconsider whether this can be Relaxed (meaning it would be okay if some other memory access got reordered to happen before this fetch_add??
	if nexteac as usize > NUM_SLOTS {
	    eac.fetch_sub(1, Ordering::Acquire); // reconsider whether this can be Relaxed (meaning it would be okay if some other memory access got reordered to happen before this fetch_add??
	}

	eac.load(Ordering::Relaxed) as usize
    }

    /// Returns Some(SlotLocation) if it was able to allocate a slot, else returns None (in which case alloc/realloc needs to satisfy the request by falling back to sys_alloc()
    fn inner_alloc(&self, initslabnum: usize) -> Option<SlotLocation> {
	assert!(initslabnum < NUM_SLABS);

	let mut slabnum = initslabnum;

	loop {
	    let areanum = if slabnum < NUM_SLABS_CACHELINEY { thread_area_num() } else { 0 };

	    let flh = self.pop_flh(areanum, slabnum);
	    if flh != 0 {
		// xxx add unit test of this case
		return Some(SlotLocation { areanum, slabnum, slotnum: flh-1 });
	    }
	    
	    let eac: usize = self.increment_eac(areanum, slabnum);
	    assert!(eac <= NUM_SLOTS);
	    if eac < NUM_SLOTS {
		// xxx add unit test of this case
		return Some(SlotLocation { areanum, slabnum, slotnum: eac });
	    }

	    // xxx add unit test of this case
	    
	    // This slab is exhausted!
	    // xxx4 very unlikely. Investigate adding unlikely/cold annotations for Rust/LLVM...
	    slabnum += 1;

	    if slabnum >= NUM_SLOTS {
		// xxx add unit test of this case
		// xxx4 very very unlikely. Investigate adding unlikely/cold annotations for Rust/LLVM...
		return None;
	    }
	}
    }

}

static GLOBAL_THREAD_ID: AtomicU8 = AtomicU8::new(0);

thread_local!(static THREAD_AREA_NUM: usize = GLOBAL_THREAD_ID.fetch_add(1, Ordering::Relaxed).into());

fn thread_area_num() -> usize {
    THREAD_AREA_NUM.with(|&id| id)
}

// xxx can i get the Rust typechecker to tell me if I'm accidentally adding a slot number to an offset ithout multiplying it by a slot size first?

unsafe impl GlobalAlloc for Smalloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
	let baseptr = self.idempotent_init();

	// Allocate a slot
	match self.inner_alloc(layout_to_slabnum(layout)) {
	    Some(sl) => {
		unsafe { baseptr.add(sl.offset_of_slot()) }
	    }
	    None => {
		// Couldn't allocate a slot -- fall back to `sys_alloc()`.
		sys_alloc(layout.size())
	    }
	    
	}
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
	match slotlocation_of_ptr(self.get_baseptr(), ptr) {
	    Some(sl) => {
		// Push the freed slot onto its free list.
		self.push_flh(sl);
	    }
	    None => {
		// No slot -- this allocation must have come from falling back to `sys_alloc()`.
		fallback_to_sys_dealloc(ptr, layout);
	    }
	}
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
	let baseptr = self.get_baseptr();

	let newlayout = Layout::from_size_align(new_size, layout.align()).unwrap();

	let cursl_or_sysa = slotlocation_of_ptr(self.get_baseptr(), ptr);
	if cursl_or_sysa.is_none() {
	    // This must have been allocated by falling back to sys_alloc().
	    return fallback_to_sys_realloc(ptr, layout, newlayout);
	}

	let cursl = cursl_or_sysa.unwrap();
	let mut newslabnum = layout_to_slabnum(newlayout);

	// If the new size fits into the current slot (or would fit into any smaller one), then leave the ptr in place and we're done.
	if newslabnum <= cursl.slabnum {
	    return ptr;
	}

	// The "growers" rule: if the new size would fit into one 64-byte cache line, use a 64-byte slot, else use one of the largest slots.
	if newslabnum <= NUM_SLABS_CACHELINEY { // xxx check for off by one
	    newslabnum = NUM_SLABS_CACHELINEY;
	} else {
	    newslabnum = NUM_SLABS-1;
	}

	// Allocate a new slot...
	let newptr: *mut u8 = match self.inner_alloc(newslabnum) {
	    Some(newsl) => {
		let newslotnum = newsl.slotnum;
		let addr_of_new_slot = unsafe { baseptr.add(newsl.offset_of_slot()) };
		assert!(addr_of_new_slot.is_aligned_to(CACHELINE_SIZE) || newslotnum > 0);
		assert!(addr_of_new_slot.is_aligned_to(layout.align()));
		addr_of_new_slot
	    }
	    None => {
		// Couldn't allocate a slot -- fall back to `sys_alloc()`.
		fallback_to_sys_alloc(layout)
	    }
	};

	// Copy the contents from the old ptr.
	unsafe {
	    copy_nonoverlapping(ptr, newptr, layout.size());
	}

	// Free the old slot
	self.push_flh(cursl);

	newptr
    }
}

// XXX make the first thread have num 0


// I read in the "The Linux Programming Interface" book that glibc's malloc's default size to fall back to system allocation (mmap) -- MMAP_THRESHOLD -- is 128 KiB. But according to https://sourceware.org/glibc/wiki/MallocInternals the threshold is dynamic unless overridden.

/// For allocations that won't fit into even our largest slots, we fall back to a system memory allocation call (such as `mmap()` on Linux, `mach_vm_allocate()` on iOS and Macos, and `VirtualAlloc()` on Windows.
fn fallback_to_sys_alloc(layout: Layout) -> *mut u8 {
    // xxx add tests
    let size = layout.size();
    assert!(size > 0);
    let alignment = layout.align();
    assert!(alignment > 0);

    assert!(alignment > 0 && (alignment & (alignment - 1)) == 0, "alignment must be a power of two"); // benchmarks show this bittwiddling expression is a teeeny bit more efficient than the builtin power-of-two function (on some x86-64 systems but not others, and on Apple M4 Max).

    // Round up size to the nearest multiple of alignment:
    let alignedsize: usize = ((size - 1) | (alignment - 1)) + 1;

    sys_alloc(alignedsize)
}

/// For allocations created by the system allocation fallback (above), we need to use the system deallocator.
fn fallback_to_sys_dealloc(ptr: *mut u8, layout: Layout) {
    // xxx add tests
    let size = layout.size();
    assert!(size > 0);
    let alignment = layout.align();
    assert!(alignment > 0);

    assert!(alignment > 0 && (alignment & (alignment - 1)) == 0, "alignment must be a power of two"); // benchmarks show this bittwiddling expression is a teeeny bit more efficient than the builtin power-of-two function (on some x86-64 systems but not others, and on Apple M4 Max).

    // Round up size to the nearest multiple of alignment:
    let alignedsize: usize = ((size - 1) | (alignment - 1)) + 1;

    sys_dealloc(ptr, alignedsize)
}

/// For allocations created by the system allocation fallback (above), to realloc them to a different size we need to use the system reallocator.
fn fallback_to_sys_realloc(ptr: *mut u8, oldlayout: Layout, newlayout: Layout) -> *mut u8 {
    // xxx add tests
    let oldalignment = oldlayout.align();
    let newalignment = newlayout.align();
    assert_eq!(oldalignment, newalignment);

    assert!(oldalignment > 0 && (oldalignment & (oldalignment - 1)) == 0, "alignment must be a power of two"); // benchmarks show this bittwiddling expression is a teeeny bit more efficient than the builtin power-of-two function (on some x86-64 systems but not others, and on Apple M4 Max).

    // Round up size to the nearest multiple of alignment:
    let alignednewsize = ((newlayout.size() - 1) | (newalignment - 1)) + 1;
    let alignedoldsize = ((oldlayout.size() - 1) | (oldalignment - 1)) + 1;
    
    sys_realloc(ptr, alignedoldsize, alignednewsize)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offset_of_vars_by_calculation(areanum: usize, slabnum: usize) -> usize {
        if (areanum != 0) && (slabnum >= NUM_SLABS_CACHELINEY) { return 0 };

	// benchmark whether it is more efficient by lookuptable or by calc
	if areanum == 0 {
	    slabnum * 2 * WORDSIZE
	} else {
	    let mut offset = 0;

	    // First count past the first column...
	    offset += NUM_SLABS * 2 * WORDSIZE;

	    // Then count past any whole columns between that first column and ours...
	    offset += (areanum - 1) * NUM_SLABS_CACHELINEY * 2 * WORDSIZE;

	    // Then count past any other variables in our column...
	    offset += slabnum * 2 * WORDSIZE;

	    offset
	}
    }

    pub const SLABNUM_TO_NUMAREAS: [usize; NUM_SLABS] = [NUM_AREAS, NUM_AREAS, NUM_AREAS, NUM_AREAS, NUM_AREAS, NUM_AREAS, NUM_AREAS, NUM_AREAS, NUM_AREAS, NUM_AREAS, NUM_AREAS, 1, 1, 1, 1, 1, 1, 1];

    #[cfg(test)]
    const fn data_slabs_space_for_testing() -> usize {
	let mut slabnum = 0;
	let mut sizeof_data_slabs = 0;

	while slabnum < NUM_SLABS {
	    let slotsize = SLABNUM_TO_SLOTSIZE[slabnum];

	    // Okay the total space needed for this slab is
	    let spaceperslab = slotsize * NUM_SLOTS;

	    // There are this many slots of this size:
	    let numslabs_for_this_sizeclass = SLABNUM_TO_NUMAREAS[slabnum];
	    
	    let totslabareaspace = spaceperslab * numslabs_for_this_sizeclass;

	    sizeof_data_slabs += totslabareaspace;

	    slabnum += 1;
	}

	sizeof_data_slabs
    }

    #[test]
    fn test_roundtrip_slot_to_ptr_to_slot() {
        let baseptr_for_testing: *mut u8 = 1024 as *mut u8;
	// pick "boundary" ish values to test
        // First test area 0
        let areanum = 0;
	for slabnum in [0, 1, 2, 3, 4, 9, 10, 11, 12, NUM_SLABS-2, NUM_SLABS-1] {
	    for slotnum in [0, 1, 2, 253, 254, 255, 256, 257, 1022, 1023, 1024, 2usize.pow(16)-1, 2usize.pow(16), 2usize.pow(16)+1, NUM_SLOTS-2, NUM_SLOTS-1] {
                //eprintln!("areanum: {}, slabnum: {}, slotnum: {}", areanum, slabnum, slotnum);
		let sl1 = SlotLocation { areanum, slabnum, slotnum };
                //eprintln!("sl1: {:?}", sl1);
		let offset = sl1.offset_of_slot();
		assert!(offset >= DATA_SLABS_BASE_OFFSET);
		assert!(offset < DATA_SLABS_BASE_OFFSET + DATA_SLABS_SPACE);
		let p = unsafe { baseptr_for_testing.add(offset) };
		let sl2 = slotlocation_of_ptr(baseptr_for_testing, p).unwrap();
		assert_eq!(sl1, sl2);
	    }
	}
        
	for areanum in [1, 2, NUM_AREAS-3, NUM_AREAS-2, NUM_AREAS-1] {
	    for slabnum in [0, 1, 2, 3, 4] {
		for slotnum in [0, 1, 2, 253, 254, 255, 256, 257, 1022, 1023, 1024, 2usize.pow(16)-1, 2usize.pow(16), 2usize.pow(16)+1, NUM_SLOTS-2, NUM_SLOTS-1] {
		    let sl1 = SlotLocation { areanum, slabnum, slotnum };
		    let offset = sl1.offset_of_slot();
		    assert!(offset >= DATA_SLABS_BASE_OFFSET);
		    assert!(offset < DATA_SLABS_BASE_OFFSET + DATA_SLABS_SPACE);
		    let p = unsafe { baseptr_for_testing.add(offset) };
		    let sl2 = slotlocation_of_ptr(baseptr_for_testing, p).unwrap();
		    assert_eq!(sl1, sl2);
		}
	    }
	}

    }

    #[test]
    fn test_data_slabs_space() {
	assert_eq!(DATA_SLABS_SPACE, data_slabs_space_for_testing());
    }

    #[test]
    fn test_offset_of_vars() {
	assert_eq!(OFFSET_OF_VAR[0][0], 0);
	assert_eq!(offset_of_vars_by_calculation(0, 0), 0);
	assert_eq!(OFFSET_OF_VAR[1][0], 144);
	assert_eq!(OFFSET_OF_VAR[2][0], 232);

	for slabnum in 0..NUM_SLABS {
	    assert_eq!(OFFSET_OF_VAR[0][slabnum], offset_of_vars_by_calculation(0, slabnum));
	}

	for areanum in 1..NUM_AREAS {
	    for slabnum in 0..NUM_SLABS {
		assert_eq!(OFFSET_OF_VAR[areanum][slabnum], offset_of_vars_by_calculation(areanum, slabnum), "areanum: {}, slabnum: {}", areanum, slabnum);
	    }
	}
    }

    #[test]
    fn test_roundtrip_slabnum2ss2slabnum() {
	for slabnum in 0..NUM_SLABS {
	    let ss = SLABNUM_TO_SLOTSIZE[slabnum];
	    let rtslabnum = layout_to_slabnum(Layout::from_size_align(ss, 1).unwrap());
	    assert_eq!(slabnum, rtslabnum, "{}", ss);
	}
    }

    #[test]
    fn test_many_args() {
        for reqalign in [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048] {
            for reqsiz in 1..10000 {
                let slabnum = layout_to_slabnum(Layout::from_size_align(reqsiz, reqalign).unwrap());
                let ss: usize = SLABNUM_TO_SLOTSIZE[slabnum];
                assert!(ss >= reqsiz, "{} >= {}", ss, reqsiz);

                // Is there any *smaller* slab which still could have
                // held the requested size *and* whose slotsize is a
                // multiple of the requested alignment?  If so then we
                // failed to find a valid optimization.
                if slabnum > 0 {
                    let mut tryslabnum = slabnum-1;
                    loop {
                        let tryss: usize = SLABNUM_TO_SLOTSIZE[tryslabnum];
                        if tryss < reqsiz {
                            break;
                        }
                        assert!(tryss % reqalign != 0, "If tryss % reqalign == 0, then there was a smaller slab whose slot size was a multiple of the requested alignment. Therefore, we failed to find a valid optimization. reqsiz: {}, slabnum: {}, ss: {}, tryslabnum: {}, tryss: {}", reqsiz, slabnum, ss, tryslabnum, tryss);
                        
                        if tryslabnum == 0 {
                            break;
                        }
                        tryslabnum -= 1;
                    }
                }
            }
        }
    }
}

