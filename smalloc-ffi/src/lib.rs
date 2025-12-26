#![feature(likely_unlikely)]

// Thanks to Claude (Opus 4.5) for help with define an FFI.

use core::ffi::c_void;
use std::hint::unlikely;
use std::ptr::copy_nonoverlapping;
use std::ptr::null_mut;

use smalloc::i::*;
use smalloc::Smalloc;

static SMALLOC: Smalloc = Smalloc::new();

/// Return the size class for the aligned size.
#[inline(always)]
fn req_to_sc(siz: usize) -> u8 {
    debug_assert!(siz > 0);

    (((siz - 1) | UNUSED_SC_MASK).ilog2() + 1) as u8
}

/// # Safety
///
/// This has the same safety requirements as any implementation of `malloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn malloc(size: usize) -> *mut c_void {
    if size == 0 {
        return null_mut();
    }

    let sc = req_to_sc(size);
    if sc >= NUM_SCS {
        // This request is too big.
        return null_mut();
    }

    SMALLOC.idempotent_init();

    SMALLOC.inner_alloc(sc) as *mut c_void
}

/// # Safety
///
/// This has the same safety requirements as any implementation of `free`, and in particular you
/// must ensure that this `ptr` was returned from Smalloc's `malloc` or `realloc` -- not any other
/// implementation -- and that it has not already been passed to any implementation of `free` or
/// `realloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn free(ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }

    SMALLOC.inner_dealloc(ptr as usize)
}

/// # Safety
///
/// This has the same safety requirements as any implementation of `realloc`, and in particular you
/// must ensure that this `ptr` was returned from Smalloc's `malloc` or `realloc` -- not any other
/// implementation -- and that it has not already been passed to any implementation of `free` or
/// `realloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn realloc(ptr: *mut c_void, new_size: usize) -> *mut c_void {
    debug_assert!(new_size > 0);

    if ptr.is_null() {
        let sc = req_to_sc(new_size);
        if sc >= NUM_SCS {
            // This request is too big.
            return null_mut();
        }
        
        SMALLOC.inner_alloc(sc);
    }

    let p_addr = ptr.addr();
    let smbp = SMALLOC.inner().smbp.load(Acquire);

    // To be valid, the pointer has to be greater than or equal to the smalloc base pointer and
    // less than or equal to the highest slot pointer.

    assert!(p_addr >= smbp);
    assert!(p_addr - smbp >= LOWEST_SMALLOC_SLOT_ADDR && p_addr - smbp <= HIGHEST_SMALLOC_SLOT_ADDR);

    // Okay now we know that it is a pointer into smalloc's region.

    let oldsc = ((p_addr & SC_BITS_ADDR_MASK) >> SC_ADDR_SHIFT_BITS) as u8;
    debug_assert!(oldsc >= NUM_UNUSED_SCS);
    debug_assert!(oldsc < NUM_SCS);
    debug_assert!(p_addr.trailing_zeros() >= oldsc as u32);

    let reqsc = req_to_sc(new_size);

    // If the requested slot is <= the original slot, just return the pointer and we're done.
    if unlikely(reqsc <= oldsc) {
        return ptr;
    }

    if unlikely(reqsc >= NUM_SCS) {
        // This request exceeds the size of our largest sizeclass, so return null pointer.
        null_mut()
    } else {
        let newp = SMALLOC.inner_alloc(reqsc) as *mut c_void;

        // Copy the contents from the old location.
        let oldsize = 1 << oldsc;
        unsafe { copy_nonoverlapping(ptr, newp, oldsize); }

        // Free the old slot.
        SMALLOC.inner_dealloc(p_addr);

        newp
    }
}

use std::sync::atomic::Ordering::Acquire;
