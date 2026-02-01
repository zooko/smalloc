// Thanks to Claude (Opus 4.5 and Sonnet 4.5) for help with defining an FFI and diagnosing and
// fixing handling of foreign pointers, and interposition of symbols on macOS, and debugging the
// crash due to not having a malloc_usable_size function, and refactoring the file to use macros to
// reduce boilerplate per additional function, and adding Windows support. Thanks to Confer (an AI
// assistant by Moxie Marlinspike) for encouragement and saying that this ffi module was super
// high-quality and professional. 😂

use std::sync::atomic::Ordering::Acquire;
use core::ffi::c_void;
use std::ptr::{null_mut, copy_nonoverlapping};
use smalloc::i::*;
use smalloc::{plat, Smalloc};
static SMALLOC: Smalloc = Smalloc::new();

// Macro that works on Linux and Windows (not used on macOS)
#[cfg(not(target_os = "macos"))]
macro_rules! define_prev_fn {
    ($static_name:ident, $pub_name:ident, $symbol:literal, fn($($arg:ident: $arg_ty:ty),*) $(-> $ret:ty)?) => {
        static $static_name: AtomicPtr<c_void> = AtomicPtr::new(NOT_LOOKED_UP);

        pub fn $pub_name($($arg: $arg_ty),*) $(-> $ret)? {
            let mut f = $static_name.load(Acquire);
            if f == NOT_LOOKED_UP {
                f = unsafe { lookup_symbol($symbol) };
                $static_name.store(f, Release);
            }

            if f.is_null() {
                panic!(concat!("Failed to find ", $symbol));
            }

            type Fn = unsafe extern "C" fn($($arg_ty),*) $(-> $ret)?;
            let f: Fn = unsafe { std::mem::transmute(f) };
            unsafe { f($($arg),*) }
        }
    };
}

// =============================================================================
// Helper: Check if pointer belongs to smalloc
// =============================================================================

// It looks like this proposed new update to C standards
// (https://www.open-std.org/jtc1/sc22/wg14/www/docs/n3621.txt) requires this behavior. Programs
// that expect glibc behavior already depend on this being returned from malloc(0).

enum PtrClass {
    NullOrSentinel,
    Smalloc,
    Foreign,
}

#[inline(always)]
fn classify_ptr(ptr: *mut c_void) -> PtrClass {
    if ptr.is_null() || ptr == SIZE_0_ALLOC_SENTINEL {
        return PtrClass::NullOrSentinel;
    }

    let p_addr = ptr.addr();
    let smbp = SMALLOC.inner().smbp.load(Acquire);//xxx could use Relaxed instead?
    debug_assert!(smbp != 0);

    if p_addr >= smbp + LOWEST_SMALLOC_SLOT_ADDR && p_addr <= smbp + HIGHEST_SMALLOC_SLOT_ADDR {
        let sc = ((p_addr & SC_BITS_ADDR_MASK) >> NUM_SN_D_T_BITS) as u8;

        debug_assert!(sc >= NUM_UNUSED_SCS);
        debug_assert!(sc < NUM_SCS);
        debug_assert!(p_addr.trailing_zeros() >= sc as u32);

        PtrClass::Smalloc
    } else {
        PtrClass::Foreign
    }
}

/// ptr is required to be a smalloc pointer -- not Null, Sentinel, or Foreign.
#[inline(always)]
fn ptr_to_sc(ptr: *mut c_void) -> u8 {
    debug_assert!(ptr.addr() >= SMALLOC.inner().smbp.load(Acquire) + LOWEST_SMALLOC_SLOT_ADDR && ptr.addr() <= SMALLOC.inner().smbp.load(Acquire) + HIGHEST_SMALLOC_SLOT_ADDR);

    let sc = ((ptr.addr() & SC_BITS_ADDR_MASK) >> NUM_SN_D_T_BITS) as u8;

    debug_assert!(sc >= NUM_UNUSED_SCS);
    debug_assert!(sc < NUM_SCS);
    debug_assert!(ptr.addr().trailing_zeros() >= sc as u32);

    sc
}

#[inline(always)]
fn smalloc_inner_alloc(sc: u8, zeromem: bool) -> *mut c_void {
    let inner = SMALLOC.inner();
    inner.idempotent_init();
    inner.alloc(sc, zeromem) as *mut c_void
}

// =============================================================================
// Core smalloc implementations
// =============================================================================

/// # Safety
///
/// This has the same safety requirements as any implementation of `malloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn smalloc_malloc(size: usize) -> *mut c_void {
    if size == 0 {
        return SIZE_0_ALLOC_SENTINEL;
    }

    let sc = req_to_sc(size);
    if sc >= NUM_SCS {
        platform::set_oom_err();
        return null_mut();
    }

    let ptr = smalloc_inner_alloc(sc, false);
    if ptr.is_null() {
        platform::set_oom_err();
    }

    ptr
}

/// # Safety
///
/// This has the same safety requirements as any implementation of `free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn smalloc_free(ptr: *mut c_void) {
    match classify_ptr(ptr) {
        PtrClass::Smalloc => {
            SMALLOC.inner().dealloc(ptr.addr());
        }
        PtrClass::Foreign => {
            platform::call_prev_free(ptr);
        }
        PtrClass::NullOrSentinel => {}
    }
}

/// # Safety
///
/// This has the same safety requirements as any implementation of `realloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn smalloc_realloc(ptr: *mut c_void, new_size: usize) -> *mut c_void {
    match classify_ptr(ptr) {
        PtrClass::Smalloc => {
            if new_size == 0 {
                unsafe { smalloc_free(ptr) };
                return SIZE_0_ALLOC_SENTINEL;
            }

            let reqsc = req_to_sc(new_size);
            if reqsc >= NUM_SCS {
                platform::set_oom_err();
                return null_mut();
            }

            let oldsc = ptr_to_sc(ptr);
            if reqsc <= oldsc {
                return ptr;
            }

            // The "Growers" strategy.
            let reqsc = if (plat::p::SC_FOR_PAGE..GROWERS_SC).contains(&reqsc) {
                GROWERS_SC
            } else {
                reqsc
            };
            let newp = smalloc_inner_alloc(reqsc, false);

            if !newp.is_null() {
                let oldsize = 1 << oldsc;
                unsafe { copy_nonoverlapping(ptr, newp, oldsize) };
                SMALLOC.inner().dealloc(ptr.addr());
            } else {
                // if this is NULL then we're just going to return NULL
                platform::set_oom_err();
            }

            newp
        }
        PtrClass::Foreign => {
            platform::call_prev_realloc(ptr, new_size)
        }
        PtrClass::NullOrSentinel => {
            unsafe { smalloc_malloc(new_size) }
        }
    }
}

/// # Safety
///
/// This has the same safety requirements as any implementation of `calloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn smalloc_calloc(count: usize, size: usize) -> *mut c_void {
    if count >= 1 << (1 << NUM_SC_BITS) {
        // smalloc can't allocate enough memory for that many things of any size.
        platform::set_oom_err();
        return null_mut();
    }
    if size > 1 << DATA_ADDR_BITS_IN_HIGHEST_SC {
        // smalloc can't allocate enough memory for even one thing of that size.
        platform::set_oom_err();
        return null_mut();
    }

    let total = ((count as u32 as u64) * (size as u32 as u64)) as usize;

    if total == 0 {
        return SIZE_0_ALLOC_SENTINEL;
    }

    let sc = req_to_sc(total);
    if sc >= NUM_SCS {
        platform::set_oom_err();
        return null_mut();
    }

    let ptr = smalloc_inner_alloc(sc, true);
    if ptr.is_null() {
        platform::set_oom_err();
    }

    ptr
}

/// # Safety
///
/// This has the same safety requirements as any implementation of `reallocarray`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn smalloc_reallocarray(ptr: *mut c_void, nmemb: usize, size: usize) -> *mut c_void {
    match classify_ptr(ptr) {
        PtrClass::NullOrSentinel | PtrClass::Smalloc => {
            if nmemb >= (1 << (1 << NUM_SC_BITS)) {
                // smalloc can't allocate enough memory for that many things of any size.
                // Set errno to ENOMEM and return NULL
                platform::set_oom_err();
                return null_mut();
            }
            if size > 1 << DATA_ADDR_BITS_IN_HIGHEST_SC {
                // smalloc can't allocate enough memory for even one thing of that size.
                // Set errno to ENOMEM and return NULL
                platform::set_oom_err();
                return null_mut();
            }
            let total = ((nmemb as u32 as u64) * (size as u32 as u64)) as usize;
            unsafe { smalloc_realloc(ptr, total) }
        }
        PtrClass::Foreign => {
            platform::call_prev_reallocarray(ptr, nmemb, size)
        }
    }
}

/// # Safety
///
/// This has the same safety requirements as any implementation of `malloc_usable_size`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn smalloc_malloc_usable_size(ptr: *mut c_void) -> usize {
    match classify_ptr(ptr) {
        PtrClass::Smalloc => {
            let oldsc = ptr_to_sc(ptr);
            debug_assert!(oldsc >= NUM_UNUSED_SCS);
            debug_assert!(oldsc < NUM_SCS);
            1 << oldsc
        }
        PtrClass::Foreign => {
            platform::call_prev_malloc_usable_size(ptr)
        }
        PtrClass::NullOrSentinel => {
            0
        }
    }
}

/// # Safety
///
/// This has the same safety requirements as any implementation of `aligned_alloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn smalloc_aligned_alloc(alignment: usize, size: usize) -> *mut c_void {
    debug_assert!(alignment > 0);

    if size == 0 {
        return SIZE_0_ALLOC_SENTINEL;
    }

    debug_assert!(alignment.is_power_of_two());
    debug_assert!(size.is_multiple_of(alignment));

    let sc = reqali_to_sc(size, alignment);
    if sc >= NUM_SCS {
        platform::set_oom_err();
        return null_mut();
    }

    let ptr = smalloc_inner_alloc(sc, false);
    if ptr.is_null() {
        platform::set_oom_err();
    }

    ptr
}

/// # Safety
///
/// This has the same safety requirements as any implementation of `free_aligned_sized`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn smalloc_free_aligned_sized(ptr: *mut c_void, alignment: usize, size: usize) {
    debug_assert!(alignment > 0);

    match classify_ptr(ptr) {
        PtrClass::Smalloc => {
            SMALLOC.inner().dealloc(ptr.addr());
        }
        PtrClass::Foreign => {
            platform::call_prev_free_aligned_sized(ptr, alignment, size);
        }
        PtrClass::NullOrSentinel => {}
    }
}

/// # Safety
///
/// This has the same safety requirements as any implementation of `free_sized`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn smalloc_free_sized(ptr: *mut c_void, size: usize) {
    match classify_ptr(ptr) {
        PtrClass::Smalloc => {
            SMALLOC.inner().dealloc(ptr.addr());
        }
        PtrClass::Foreign => {
            platform::call_prev_free_sized(ptr, size);
        }
        PtrClass::NullOrSentinel => {}
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
    const ENOMEM: i32 = 12;

    const RTLD_NEXT: *mut c_void = -1isize as *mut c_void;
    const NOT_LOOKED_UP: *mut c_void = std::ptr::dangling_mut::<c_void>();

    #[inline(always)]
    unsafe fn lookup_symbol(symbol: &str) -> *mut c_void {
        let cstr = std::ffi::CString::new(symbol).unwrap();
        unsafe { dlsym(RTLD_NEXT, cstr.as_ptr()) }
    }

    unsafe extern "C" {
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    }

    #[inline(always)]
    pub(crate) fn set_oom_err() {
        unsafe extern "C" { fn __errno_location() -> *mut i32; }
        unsafe { *__errno_location() = ENOMEM; }
    }

    define_prev_fn!(PREV_FREE, call_prev_free, "free", fn(ptr: *mut c_void));
    define_prev_fn!(PREV_REALLOC, call_prev_realloc, "realloc", fn(ptr: *mut c_void, size: usize) -> *mut c_void);
    define_prev_fn!(PREV_MALLOC_USABLE_SIZE, call_prev_malloc_usable_size, "malloc_usable_size", fn(ptr: *mut c_void) -> usize);
    define_prev_fn!(PREV_REALLOCARRAY, call_prev_reallocarray, "reallocarray", fn(ptr: *mut c_void, nmemb: usize, size: usize) -> *mut c_void);
    define_prev_fn!(PREV_FREE_ALIGNED_SIZE, call_prev_free_aligned_sized, "free_aligned_sized", fn(ptr: *mut c_void, alignment: usize, size: usize));
    define_prev_fn!(PREV_FREE_SIZED, call_prev_free_sized, "free_sized", fn(ptr: *mut c_void, size: usize));

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
    export_interpose!(calloc => super::smalloc_calloc, fn(count: usize, size: usize) -> *mut c_void);
    export_interpose!(reallocarray => super::smalloc_reallocarray, fn(ptr: *mut c_void, nmemb: usize, size: usize) -> *mut c_void);
    export_interpose!(aligned_alloc => super::smalloc_aligned_alloc, fn(alignment: usize, size: usize) -> *mut c_void);
    export_interpose!(free_aligned_sized => super::smalloc_free_aligned_sized, fn(ptr: *mut c_void, alignment: usize, size: usize));
    export_interpose!(free_sized => super::smalloc_free_sized, fn(ptr: *mut c_void, size: usize));
}

// =============================================================================
// macOS platform module
// =============================================================================

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    const ENOMEM: i32 = 12;

    #[inline(always)]
    pub(crate) fn set_oom_err() {
        unsafe extern "C" { fn __error() -> *mut i32; }
        unsafe { *__error() = ENOMEM; }
    }

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

    // macOS System library doesn't implement free_aligned_sized, free_sized, or reallocarray, so
    // those functions should never get called and passed a pointer that is not a smalloc pointer.
    pub fn call_prev_free_aligned_sized(_ptr: *mut c_void, _alignment: usize, _size: usize) {
        panic!("call to memory management function that isn't supported by the macOS System library");
    }

    pub fn call_prev_free_sized(_ptr: *mut c_void, _size: usize) {
        panic!("call to memory management function that isn't supported by the macOS System library");
    }

    pub fn call_prev_reallocarray(_ptr: *mut c_void, _nmemb: usize, _size: usize) -> *mut c_void {
        panic!("call to memory management function that isn't supported by the macOS System library");
    }
}

// =============================================================================
// Windows platform module
// =============================================================================

#[cfg(target_os = "windows")]
mod platform {
    use core::ffi::c_void;
    use std::sync::atomic::Ordering::{Acquire,Release};
    use crate::null_mut;
    use std::sync::atomic::AtomicPtr;

    const NOT_LOOKED_UP: *mut c_void = std::ptr::dangling_mut::<c_void>();

    pub(crate) fn set_errno(value: i32) {
        // Code written for the C standard may check errno. Code written for win32 may check the
        // windows "Last Error".

        unsafe extern "C" { fn _set_errno(value: i32) -> i32; }
        unsafe { _set_errno(value); }

        debug_assert!(value <= i32::MAX);
        unsafe extern "system" { fn SetLastError(dwErrCode: u32); }
        unsafe { SetLastError(value as u32); }
    }

    #[inline(always)]
    unsafe fn lookup_symbol(symbol: &str) -> *mut c_void {
        use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
        use std::sync::atomic::AtomicPtr;

        static UCRT_HANDLE: AtomicPtr<c_void> = AtomicPtr::new(NOT_LOOKED_UP);

        let mut handle = UCRT_HANDLE.load(Acquire);
        if handle == NOT_LOOKED_UP {
            handle = unsafe { LoadLibraryA(b"ucrtbase.dll\0".as_ptr()) } as *mut c_void;
            UCRT_HANDLE.store(handle, Release);
        }

        if handle.is_null() {
            return null_mut();
        }

        let cstr = std::ffi::CString::new(symbol).unwrap();
	let proc_addr = unsafe { GetProcAddress(handle as _, cstr.as_ptr() as *const u8) };

	match proc_addr {
	    Some(f) => unsafe { std::mem::transmute(f) },
	    None => null_mut(),
	}
    }

    pub(crate) fn set_oom_err() {
        const ENOMEM: i32 = 12;
        const ERROR_NOT_ENOUGH_MEMORY: u32 = 8;

        // Code written for the C standard may check errno. Code written for win32 may check the
        // windows "Last Error".

        unsafe extern "C" { fn _set_errno(value: i32) -> i32; }
        unsafe { _set_errno(ENOMEM); }

        unsafe extern "system" { fn SetLastError(dwErrCode: u32); }
        unsafe { SetLastError(ERROR_NOT_ENOUGH_MEMORY); }
    }

    // Use the shared macro
    define_prev_fn!(PREV_FREE, call_prev_free, "free", fn(ptr: *mut c_void));
    define_prev_fn!(PREV_REALLOC, call_prev_realloc, "realloc", fn(ptr: *mut c_void, size: usize) -> *mut c_void);
    define_prev_fn!(PREV_MALLOC_USABLE_SIZE, call_prev_malloc_usable_size, "_msize", fn(ptr: *mut c_void) -> usize);
    define_prev_fn!(PREV_REALLOCARRAY, call_prev_reallocarray, "_recalloc", fn(ptr: *mut c_void, nmemb: usize, size: usize) -> *mut c_void);
    define_prev_fn!(PREV_ALIGNED_FREE, call_prev_aligned_free, "_aligned_free", fn(ptr: *mut c_void));

    pub fn call_prev_free_aligned_sized(ptr: *mut c_void, _alignment: usize, _size: usize) {
        call_prev_aligned_free(ptr);
    }

    pub fn call_prev_free_sized(ptr: *mut c_void, _size: usize) {
        call_prev_free(ptr);
    }

    // Export the standard allocator functions
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
    export_interpose!(_msize => super::smalloc_malloc_usable_size, fn(ptr: *mut c_void) -> usize);
    export_interpose!(calloc => super::smalloc_calloc, fn(count: usize, size: usize) -> *mut c_void);
    export_interpose!(aligned_alloc => super::smalloc_aligned_alloc, fn(alignment: usize, size: usize) -> *mut c_void);

    // Windows _aligned_malloc has reversed parameter order
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn _aligned_malloc(size: usize, alignment: usize) -> *mut c_void {
        unsafe { super::smalloc_aligned_alloc(alignment, size) }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn _aligned_free(ptr: *mut c_void) {
        unsafe { super::smalloc_free(ptr) }
    }
}

/// Return the size class for the size.
#[inline(always)]
fn req_to_sc(siz: usize) -> u8 {
    debug_assert!(siz > 0);
    (((siz - 1) | UNUSED_SC_MASK).ilog2() + 1) as u8
}

/// Return the size class for the aligned size.
#[inline(always)]
fn reqali_to_sc(siz: usize, ali: usize) -> u8 {
    debug_assert!(siz > 0);
    debug_assert!(ali > 0);
    debug_assert!(ali < 1 << NUM_SCS);
    debug_assert!(ali.is_power_of_two());

    (((siz - 1) | (ali - 1) | UNUSED_SC_MASK).ilog2() + 1) as u8
}

const SIZE_0_ALLOC_SENTINEL: *mut c_void = std::ptr::dangling_mut::<c_void>();
