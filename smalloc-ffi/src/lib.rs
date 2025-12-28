#![feature(likely_unlikely)]

// Thanks to Claude (Opus 4.5) for help with defining an FFI and diagnosing and fixing handling of
// foreign pointers, and interposition of symbols on macOS.

static SMALLOC: Smalloc = Smalloc::new();

/// # Safety
///
/// This has the same safety requirements as any implementation of `malloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn smalloc_malloc(size: usize) -> *mut c_void {
    if size == 0 {
        return null_mut();
    }

    let sc = req_to_sc(size);
    if sc >= NUM_SCS {
        return null_mut();
    }

    SMALLOC.idempotent_init();
    SMALLOC.inner_alloc(sc) as *mut c_void
}

/// # Safety
///
/// This has the same safety requirements as any implementation of `free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn smalloc_free(ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }

    let smbp = SMALLOC.inner().smbp.load(Acquire);
    if smbp != 0 {
        let p_addr = ptr.addr();

        if p_addr >= smbp + LOWEST_SMALLOC_SLOT_ADDR && p_addr <= smbp + HIGHEST_SMALLOC_SLOT_ADDR {
            SMALLOC.inner_dealloc(ptr as usize);
            return;
        }
    }

    // Foreign pointer - allocated before smalloc was loaded
    unsafe { platform::call_prev_free(ptr) }
}

/// # Safety
///
/// This has the same safety requirements as any implementation of `realloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn smalloc_realloc(ptr: *mut c_void, new_size: usize) -> *mut c_void {
    let reqsc = req_to_sc(new_size);

    if ptr.is_null() {
        if reqsc >= NUM_SCS {
            return null_mut();
        }
        return SMALLOC.inner_alloc(reqsc) as *mut c_void;
    }

    let p_addr = ptr.addr();
    let smbp = SMALLOC.inner().smbp.load(Acquire);

    if p_addr >= smbp + LOWEST_SMALLOC_SLOT_ADDR && p_addr <= smbp + HIGHEST_SMALLOC_SLOT_ADDR {
        let oldsc = ((p_addr & SC_BITS_ADDR_MASK) >> NUM_SLOTNUM_AND_DATA_BITS) as u8;
        debug_assert!(oldsc >= NUM_UNUSED_SCS);
        debug_assert!(oldsc < NUM_SCS);
        debug_assert!(p_addr.trailing_zeros() >= oldsc as u32);

        if unlikely(reqsc <= oldsc) {
            return ptr;
        }

        if unlikely(reqsc >= NUM_SCS) {
            null_mut()
        } else {
            let newp = SMALLOC.inner_alloc(reqsc) as *mut c_void;

            if !newp.is_null() {
                let oldsize = 1 << oldsc;
                unsafe { copy_nonoverlapping(ptr, newp, oldsize); }
                SMALLOC.inner_dealloc(p_addr);
            }

            newp
        }
    } else {
        // Foreign pointer
        unsafe { platform::call_prev_realloc(ptr, new_size) }
    }
}

#[inline(always)]
fn req_to_sc(siz: usize) -> u8 {
    debug_assert!(siz > 0);
    (((siz - 1) | UNUSED_SC_MASK).ilog2() + 1) as u8
}

// Linux: use dlsym(RTLD_NEXT, ...) and export malloc/free/realloc directly
#[cfg(target_os = "linux")]
mod platform {
    use super::*;

    const RTLD_NEXT: *mut c_void = -1isize as *mut c_void;

    unsafe extern "C" {
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    }

    type FreeFn = unsafe extern "C" fn(*mut c_void);
    type ReallocFn = unsafe extern "C" fn(*mut c_void, usize) -> *mut c_void;

    const NOT_LOOKED_UP: *mut c_void = std::ptr::dangling_mut::<c_void>();

    static PREV_FREE: AtomicPtr<c_void> = AtomicPtr::new(NOT_LOOKED_UP);
    static PREV_REALLOC: AtomicPtr<c_void> = AtomicPtr::new(NOT_LOOKED_UP);

    pub unsafe fn call_prev_free(ptr: *mut c_void) {
        let mut f = PREV_FREE.load(Acquire);

        if f == NOT_LOOKED_UP {
            f = unsafe { dlsym(RTLD_NEXT, c"free".as_ptr()) };
            PREV_FREE.store(f, Release);
        }

        if !f.is_null() {
            let f: FreeFn = unsafe { std::mem::transmute(f) };
            unsafe { f(ptr) };
        }
    }

    pub unsafe fn call_prev_realloc(ptr: *mut c_void, new_size: usize) -> *mut c_void {
        let mut f = PREV_REALLOC.load(Acquire);

        if f == NOT_LOOKED_UP {
            f = unsafe { dlsym(RTLD_NEXT, c"realloc".as_ptr()) };
            PREV_REALLOC.store(f, Release);
        }

        if f.is_null() {
            panic!("dlsym failed to find realloc");
        }

        let f: ReallocFn = unsafe { std::mem::transmute(f) };
        unsafe { f(ptr, new_size) }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn malloc(size: usize) -> *mut c_void {
        unsafe { super::smalloc_malloc(size) }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn free(ptr: *mut c_void) {
        unsafe { super::smalloc_free(ptr) }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn realloc(ptr: *mut c_void, new_size: usize) -> *mut c_void {
        unsafe { super::smalloc_realloc(ptr, new_size) }
    }

    use std::sync::atomic::AtomicPtr;
    use std::sync::atomic::Ordering::Release;
    use core::ffi::c_char;
}

// macOS: use malloc_zone_* APIs directly (DYLD_INTERPOSE makes dlsym unusable)
#[cfg(target_os = "macos")]
mod platform {
    use super::*;

    unsafe extern "C" {
        fn malloc_default_zone() -> *mut c_void;
        fn malloc_zone_free(zone: *mut c_void, ptr: *mut c_void);
        fn malloc_zone_realloc(zone: *mut c_void, ptr: *mut c_void, size: usize) -> *mut c_void;
    }

    pub unsafe fn call_prev_free(ptr: *mut c_void) {
        let zone = unsafe { malloc_default_zone() };
        unsafe { malloc_zone_free(zone, ptr) };
    }

    pub unsafe fn call_prev_realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
        let zone = unsafe { malloc_default_zone() };
        unsafe { malloc_zone_realloc(zone, ptr, size) }
    }
}

use std::sync::atomic::Ordering::Acquire;
use core::ffi::c_void;
use std::hint::unlikely;
use std::ptr::{null_mut, copy_nonoverlapping};
use smalloc::i::*;
use smalloc::Smalloc;
