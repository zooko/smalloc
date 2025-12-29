#![feature(likely_unlikely)]

// Thanks to Claude (Opus 4.5) for help with defining an FFI and diagnosing and fixing handling of
// foreign pointers, and interposition of symbols on macOS, and debugging the crash due to not
// having a malloc_usable_size function, and refactoring the file to use macros to reduce
// boilerplate per additional function.

static SMALLOC: Smalloc = Smalloc::new();

// =============================================================================
// Helper: Check if pointer belongs to smalloc
// =============================================================================

/// Returns Some(sc) if this is a smalloc pointer, None if foreign
#[inline(always)]
fn classify_ptr(ptr: *mut c_void) -> Option<u8> {
    let p_addr = ptr.addr();
    let smbp = SMALLOC.inner().smbp.load(Acquire);
    debug_assert!(smbp != 0);

    if likely(p_addr >= smbp + LOWEST_SMALLOC_SLOT_ADDR && p_addr <= smbp + HIGHEST_SMALLOC_SLOT_ADDR) {
        let sc = ((p_addr & SC_BITS_ADDR_MASK) >> NUM_SLOTNUM_AND_DATA_BITS) as u8;
        debug_assert!(sc >= NUM_UNUSED_SCS);
        debug_assert!(sc < NUM_SCS);
        debug_assert!(p_addr.trailing_zeros() >= sc as u32);
        Some(sc)
    } else {
        None
    }
}

#[inline(always)]
fn req_to_sc(siz: usize) -> u8 {
    debug_assert!(siz > 0);
    (((siz - 1) | UNUSED_SC_MASK).ilog2() + 1) as u8
}

// =============================================================================
// Core smalloc implementations
// =============================================================================

/// # Safety
///
/// This has the same safety requirements as any implementation of `malloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn smalloc_malloc(size: usize) -> *mut c_void {
    if unlikely(size == 0) {
        return null_mut();
    }

    let sc = req_to_sc(size);
    // xxx fall back to prev alloc in this case
    if unlikely(sc >= NUM_SCS) {
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
    if unlikely(ptr.is_null()) {
        return;
    }

    if likely(classify_ptr(ptr).is_some()) {
        SMALLOC.inner_dealloc(ptr.addr());
    } else {
        platform::call_prev_free(ptr);
    }
}

/// # Safety
///
/// This has the same safety requirements as any implementation of `realloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn smalloc_realloc(ptr: *mut c_void, new_size: usize) -> *mut c_void {
    let reqsc = req_to_sc(new_size);

    if unlikely(ptr.is_null()) {
        if unlikely(reqsc >= NUM_SCS) {
            return null_mut();
        }
        return SMALLOC.inner_alloc(reqsc) as *mut c_void;
    }

    if let Some(oldsc) = classify_ptr(ptr) {
        if unlikely(reqsc <= oldsc) {
            return ptr;
        }

        if unlikely(reqsc >= NUM_SCS) {
            // xxx fall back to prev alloc
            return null_mut();
        }

        // The "Growers" strategy. Promote the new sizeclass to the next one up in this schedule:
        #[allow(clippy::suspicious_else_formatting)]
        #[cfg(feature = "growers")]
        let reqsc =
            if reqsc <= 6 { 6 } else
            if reqsc <= 7 { 7 } else
            if reqsc <= 12 { 12 } else
            if reqsc <= 14 { 14 } else
            if reqsc <= 16 { 16 } else
            if reqsc <= 18 { 18 } else
            if reqsc <= 21 { 21 }
        else { reqsc };

        let newp = SMALLOC.inner_alloc(reqsc) as *mut c_void;
        // if this is NULL then we're just going to return NULL
        // xxx fall back to prev alloc

        if likely(!newp.is_null()) {
            let oldsize = 1 << oldsc;
            unsafe { copy_nonoverlapping(ptr, newp, oldsize) };
            SMALLOC.inner_dealloc(ptr.addr());
        }

        newp
    } else {
        platform::call_prev_realloc(ptr, new_size)
    }
}

/// # Safety
///
/// This has the same safety requirements as any implementation of `malloc_usable_size`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn smalloc_malloc_usable_size(ptr: *mut c_void) -> usize {
    if unlikely(ptr.is_null()) {
        return 0;
    }

    if let Some(sc) = classify_ptr(ptr) {
        1 << sc
    } else {
        platform::call_prev_malloc_usable_size(ptr)
    }
}

// =============================================================================
// Linux platform module
// =============================================================================

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use core::ffi::c_char;
    use std::sync::atomic::{AtomicPtr, Ordering::Release};

    const RTLD_NEXT: *mut c_void = -1isize as *mut c_void;
    const NOT_LOOKED_UP: *mut c_void = std::ptr::dangling_mut::<c_void>();

    unsafe extern "C" {
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    }

    // Macro to define a lazily-resolved dlsym wrapper
    macro_rules! define_prev_fn {
        ($static_name:ident, $pub_name:ident, $symbol:literal, fn($($arg:ident: $arg_ty:ty),*) $(-> $ret:ty)?) => {
            static $static_name: AtomicPtr<c_void> = AtomicPtr::new(NOT_LOOKED_UP);

            pub fn $pub_name($($arg: $arg_ty),*) $(-> $ret)? {
                let mut f = $static_name.load(Acquire);

                if f == NOT_LOOKED_UP {
                    f = unsafe { dlsym(RTLD_NEXT, concat!($symbol, "\0").as_ptr() as *const c_char) };
                    $static_name.store(f, Release);
                }

                if f.is_null() {
                    panic!(concat!("dlsym failed to find ", $symbol));
                }

                type Fn = unsafe extern "C" fn($($arg_ty),*) $(-> $ret)?;
                let f: Fn = unsafe { std::mem::transmute(f) };
                unsafe { f($($arg),*) }
            }
        };
    }

    define_prev_fn!(PREV_FREE, call_prev_free, "free", fn(ptr: *mut c_void));
    define_prev_fn!(PREV_REALLOC, call_prev_realloc, "realloc", fn(ptr: *mut c_void, size: usize) -> *mut c_void);
    define_prev_fn!(PREV_MALLOC_USABLE_SIZE, call_prev_malloc_usable_size, "malloc_usable_size", fn(ptr: *mut c_void) -> usize);

    // Macro to export interposed symbols
    macro_rules! export_interpose {
        ($name:ident => $impl:path, fn($($arg:ident: $arg_ty:ty),*) $(-> $ret:ty)?) => {
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name($($arg: $arg_ty),*) $(-> $ret)? {
                unsafe { $impl($($arg),*) }
            }
        };
    }

    export_interpose!(malloc => super::smalloc_malloc, fn(size: usize) -> *mut c_void);
    export_interpose!(free => super::smalloc_free, fn(ptr: *mut c_void));
    export_interpose!(realloc => super::smalloc_realloc, fn(ptr: *mut c_void, new_size: usize) -> *mut c_void);
    export_interpose!(malloc_usable_size => super::smalloc_malloc_usable_size, fn(ptr: *mut c_void) -> usize);
}

// =============================================================================
// macOS platform module
// =============================================================================

#[cfg(target_os = "macos")]
mod platform {
    use super::*;

    unsafe extern "C" {
        fn malloc_default_zone() -> *mut c_void;
        fn malloc_zone_free(zone: *mut c_void, ptr: *mut c_void);
        fn malloc_zone_realloc(zone: *mut c_void, ptr: *mut c_void, size: usize) -> *mut c_void;
        fn malloc_size(ptr: *const c_void) -> usize;
    }

    macro_rules! define_zone_fn {
        ($pub_name:ident => $zone_fn:ident, fn($($arg:ident: $arg_ty:ty),*) $(-> $ret:ty)?) => {
            pub fn $pub_name($($arg: $arg_ty),*) $(-> $ret)? {
                let zone = unsafe { malloc_default_zone() };
                unsafe { $zone_fn(zone, $($arg),*) }
            }
        };
    }

    define_zone_fn!(call_prev_free => malloc_zone_free, fn(ptr: *mut c_void));
    define_zone_fn!(call_prev_realloc => malloc_zone_realloc, fn(ptr: *mut c_void, size: usize) -> *mut c_void);

    // malloc_size doesn't use zones, so define it directly
    pub fn call_prev_malloc_usable_size(ptr: *mut c_void) -> usize {
        unsafe { malloc_size(ptr) }
    }
}

use std::sync::atomic::Ordering::Acquire;
use core::ffi::c_void;
use std::hint::{likely, unlikely};
use std::ptr::{null_mut, copy_nonoverlapping};
use smalloc::i::*;
use smalloc::Smalloc;
