#![feature(likely_unlikely)]

// Thanks to Claude Sonnet 4.5 for writing the initial version of this whole file for me, and
// updating it together with me. As well as assisting on ideation and research about how to
// interpose the Windows heap API.

//! Windows Heap API interposition layer for smalloc
//! 
//! This crate provides a kernel32.dll replacement that intercepts Heap API functions
//! (HeapAlloc, HeapFree, etc.) and directs them to smalloc, while forwarding
//! everything else to the original system kernel32.dll.
//!
//! Build instructions:
//! 1. Generate DEF file:
//!    dumpbin /EXPORTS C:\Windows\System32\kernel32.dll | python export_extractor.py > kernel32.def
//! 
//! 2. Make a local copy of the original system DLL:
//!    copy C:\Windows\System32\kernel32.dll kernel32_system.dll
//!
//! 3. Build this crate:
//!    cargo build --release
//!
//! 4. Link the resulting kernel32.lib file with the DEF file:
//!    link /DLL /DEF:kernel32.def target\release\kernel32.lib /OUT:kernel32.dll
//!
//! 5. Deploy for your application:
//!    - Copy kernel32.dll to application directory
//!    - Copy kernel32_system.dll to application directory  
//!    - Create ${YOUR_APPLICATION_NAME}.exe.local (empty file) to enable local DLL override

use core::ffi::c_void;
use core::hint::{likely, unlikely};
use core::ptr::null_mut;
use smalloc::i::*;
use smalloc::Smalloc;

// Type aliases for Windows handles
type HANDLE = *mut c_void;

// Global smalloc instance
static SMALLOC: Smalloc = Smalloc::new();

// Sentinel value to identify smalloc's interposed process heap
const SMALLOC_HEAP_HANDLE: HANDLE = 0x00000001 as HANDLE;

// Heap flags
const HEAP_ZERO_MEMORY: u32 = 0x00000008;
const HEAP_REALLOC_IN_PLACE_ONLY: u32 = 0x00000010;
const HEAP_GENERATE_EXCEPTIONS: u32 = 0x00000004;

// Windows exception status codes
const STATUS_NO_MEMORY: u32 = 0xC0000017;

// Size-0 allocation sentinel. The official [Windows
// docs](https://learn.microsoft.com/en-us/windows/win32/api/heapapi/nf-heapapi-heapalloc) are
// silent on the question of what `HeapAlloc` will do if you request 0 bytes. Experimentation show
// me that `HeapAlloc(..., 0)` on Windows 11 returns a non-null pointer that can be freed but that
// is reported as pointing to an allocation of 0 bytes by `HeapSize`. So, our implementation of
// `HeapAlloc` will do that. (Which, BTW, is also what `smalloc-ffi-c-api` does, because some code
// written to the C-API requires it.)
const SIZE_0_ALLOC_SENTINEL: *mut c_void = core::ptr::dangling_mut::<c_void>();

enum PtrClass {
    Null,
    Sentinel,
    Smalloc,
    Foreign,
}

#[inline(always)]
fn classify_ptr(ptr: *mut c_void) -> PtrClass {
    if unlikely(ptr.is_null()) {
        return PtrClass::Null;
    }

    if ptr == SIZE_0_ALLOC_SENTINEL {
        return PtrClass::Sentinel;
    }

    let p_addr = ptr.addr();
    let smbp = SMALLOC.inner().smbp.load(core::sync::atomic::Ordering::Acquire);
    debug_assert!(smbp != 0);

    if likely(p_addr >= smbp + LOWEST_SMALLOC_SLOT_ADDR && p_addr <= smbp + HIGHEST_SMALLOC_SLOT_ADDR) {
        let sc = ((p_addr & SC_BITS_ADDR_MASK) >> NUM_SLOTNUM_AND_DATA_BITS) as u8;
        debug_assert!(sc >= NUM_UNUSED_SCS);
        debug_assert!(sc < NUM_SCS);
        debug_assert!(p_addr.trailing_zeros() >= sc as u32);
        PtrClass::Smalloc
    } else {
        PtrClass::Foreign
    }
}

#[inline(always)]
fn ptr_to_sc(ptr: *mut c_void) -> u8 {
    let p_addr = ptr.addr();
    let smbp = SMALLOC.inner().smbp.load(core::sync::atomic::Ordering::Acquire);
    debug_assert!(p_addr >= smbp + LOWEST_SMALLOC_SLOT_ADDR);
    debug_assert!(p_addr <= smbp + HIGHEST_SMALLOC_SLOT_ADDR);

    let sc = ((p_addr & SC_BITS_ADDR_MASK) >> NUM_SLOTNUM_AND_DATA_BITS) as u8;
    debug_assert!(sc >= NUM_UNUSED_SCS);
    debug_assert!(sc < NUM_SCS);
    debug_assert!(p_addr.trailing_zeros() >= sc as u32);

    sc
}

#[inline(always)]
fn is_smalloc_heap(h_heap: HANDLE) -> bool {
    h_heap == SMALLOC_HEAP_HANDLE
}

// Windows HeapAlloc guarantees 8-byte alignment
const UNUSED_SC_MASK: usize = 0b111;

#[inline(always)]
fn req_to_sc(siz: usize) -> u8 {
    debug_assert!(siz > 0);
    (((siz - 1) | UNUSED_SC_MASK).ilog2() + 1) as u8
}

#[inline(always)]
fn smalloc_inner_alloc(sc: u8, zeromem: bool) -> *mut c_void {
    SMALLOC.idempotent_init();
    SMALLOC.inner_alloc(sc, zeromem) as *mut c_void
}

#[inline(always)]
unsafe fn raise_oom_if_needed(dw_flags: u32) {
    if unlikely(dw_flags & HEAP_GENERATE_EXCEPTIONS != 0) {
        extern "system" {
            fn RaiseException(
                dwExceptionCode: u32,
                dwExceptionFlags: u32,
                nNumberOfArguments: u32,
                lpArguments: *const usize,
            );
        }
        unsafe {
            RaiseException(
                STATUS_NO_MEMORY,
                0, // EXCEPTION_NONCONTINUABLE
                0,
                null_mut(),
            );
        }
    }
}

mod platform {
    use super::*;

    // Import original system Heap API functions that smalloc needs to use. These are exported by
    // our DEF file as "System<FunctionName>"
    extern "system" {
        fn SystemGetProcessHeap() -> HANDLE;
        fn SystemGetProcessHeaps(NumberOfHeaps: u32, ProcessHeaps: *mut HANDLE) -> u32;
        fn SystemHeapAlloc(hHeap: HANDLE, dwFlags: u32, dwBytes: usize) -> *mut c_void;
        fn SystemHeapFree(hHeap: HANDLE, dwFlags: u32, lpMem: *mut c_void) -> i32;
        fn SystemHeapReAlloc(hHeap: HANDLE, dwFlags: u32, lpMem: *mut c_void, dwBytes: usize) -> *mut c_void;
        fn SystemHeapSize(hHeap: HANDLE, dwFlags: u32, lpMem: *const c_void) -> usize;
        fn SetLastError(dwErrCode: u32);
    }

    #[inline(always)]
    pub(crate) fn call_system_GetProcessHeap() -> HANDLE {
        unsafe { SystemGetProcessHeap() }
    }

    #[inline(always)]
    pub(crate) fn call_system_GetProcessHeaps(number_of_heaps: u32, process_heaps: *mut HANDLE) -> u32 {
        unsafe { SystemGetProcessHeaps(number_of_heaps, process_heaps) }
    }

    #[inline(always)]
    pub(crate) fn call_system_HeapAlloc(h_heap: HANDLE, dw_flags: u32, dw_bytes: usize) -> *mut c_void {
        unsafe { SystemHeapAlloc(h_heap, dw_flags, dw_bytes) }
    }

    #[inline(always)]
    pub(crate) fn call_system_HeapFree(h_heap: HANDLE, dw_flags: u32, lp_mem: *mut c_void) -> i32 {
        unsafe { SystemHeapFree(h_heap, dw_flags, lp_mem) }
    }

    #[inline(always)]
    pub(crate) fn call_system_HeapReAlloc(h_heap: HANDLE, dw_flags: u32, lp_mem: *mut c_void, dw_bytes: usize) -> *mut c_void {
        unsafe { SystemHeapReAlloc(h_heap, dw_flags, lp_mem, dw_bytes) }
    }

    #[inline(always)]
    pub(crate) fn call_system_HeapSize(h_heap: HANDLE, dw_flags: u32, lp_mem: *const c_void) -> usize {
        unsafe { SystemHeapSize(h_heap, dw_flags, lp_mem) }
    }

    #[inline(always)]
    pub(crate) fn set_last_error(error_code: u32) {
        unsafe { SetLastError(error_code) };
    }
}

/// Returns a handle to the default process heap (smalloc)
/// 
/// # Safety
/// Same requirements as Windows API GetProcessHeap
#[unsafe(no_mangle)]
pub unsafe extern "system" fn smalloc_GetProcessHeap() -> HANDLE {
    SMALLOC_HEAP_HANDLE
}

/// This reflects the result from the underlying system's GetProcessHeaps, except that any instance
/// of the System's default heap is replaced by our smalloc default heap sentinel value.
/// 
/// # Safety
/// Same requirements as Windows API GetProcessHeaps
#[unsafe(no_mangle)]
pub unsafe extern "system" fn smalloc_GetProcessHeaps(
    number_of_heaps: u32,
    process_heaps: *mut HANDLE,
) -> u32 {
    let res = platform::call_system_GetProcessHeaps(number_of_heaps, process_heaps);

    if !process_heaps.is_null() && res > 0 {
        let system_default = platform::call_system_GetProcessHeap();
        let heaps = unsafe { core::slice::from_raw_parts_mut(process_heaps, res as usize) };

        for heap in heaps.iter_mut() {
            if *heap == system_default {
                *heap = SMALLOC_HEAP_HANDLE;
            }
        }
    }

    res
}

/// Allocates a block of memory from a heap
/// 
/// # Safety
/// Same requirements as Windows API HeapAlloc
#[unsafe(no_mangle)]
pub unsafe extern "system" fn smalloc_HeapAlloc(
    h_heap: HANDLE,
    dw_flags: u32,
    dw_bytes: usize,
) -> *mut c_void {
    if likely(is_smalloc_heap(h_heap)) {
        if unlikely(dw_bytes == 0) {
            return SIZE_0_ALLOC_SENTINEL;
        }

        let sc = req_to_sc(dw_bytes);

        if unlikely(sc >= NUM_SCS) {
            unsafe { raise_oom_if_needed(dw_flags) };
            return null_mut();
        }

        let ptr = smalloc_inner_alloc(sc, dw_flags & HEAP_ZERO_MEMORY != 0);

        if unlikely(ptr.is_null()) {
            unsafe { raise_oom_if_needed(dw_flags) };
        }

        ptr
    } else {
        platform::call_system_HeapAlloc(h_heap, dw_flags, dw_bytes)
    }
}

/// Frees a memory block allocated from a heap
/// 
/// # Safety
/// Same requirements as Windows API HeapFree
#[unsafe(no_mangle)]
pub unsafe extern "system" fn smalloc_HeapFree(
    h_heap: HANDLE,
    dw_flags: u32,
    lp_mem: *mut c_void,
) -> i32 {
    if likely(is_smalloc_heap(h_heap)) {
        let ptr_class = classify_ptr(lp_mem);

        // Since this is the smalloc heap, pointer must be smalloc or null/sentinel (never foreign)
        assert!(matches!(ptr_class, PtrClass::Smalloc | PtrClass::Null | PtrClass::Sentinel));

        if likely(matches!(ptr_class, PtrClass::Smalloc)) {
            // Smalloc pointer
            SMALLOC.inner_dealloc(lp_mem.addr());
        }

        // Null or sentinel - freeing succeeds
        1
    } else {
        // Foreign heap - delegate to original system
        platform::call_system_HeapFree(h_heap, dw_flags, lp_mem)
    }
}

/// Reallocates a memory block from a heap
/// 
/// # Safety
/// Same requirements as Windows API HeapReAlloc
#[unsafe(no_mangle)]
pub unsafe extern "system" fn smalloc_HeapReAlloc(
    h_heap: HANDLE,
    dw_flags: u32,
    lp_mem: *mut c_void,
    dw_bytes: usize,
) -> *mut c_void {
    if is_smalloc_heap(h_heap) {
        match classify_ptr(lp_mem) {
            PtrClass::Smalloc => {
                if unlikely(dw_bytes == 0) {
                    SMALLOC.inner_dealloc(lp_mem.addr());
                    return SIZE_0_ALLOC_SENTINEL;
                }

        let reqsc = req_to_sc(dw_bytes);

        if unlikely(reqsc >= NUM_SCS) {
            unsafe { raise_oom_if_needed(dw_flags) };
            return null_mut();
        }

        let oldsc = ptr_to_sc(lp_mem);

        // We cannot implement HEAP_ZERO_MEMORY with a bigger or equal slot size, because we don't
        // know the exact old allocation size, only that it fit in its old slot. The Windows API
        // does not document any way for us to return an error or raise an exception that indicates
        // this problem, either, so we'll have to panic.
        assert!(dw_flags & HEAP_ZERO_MEMORY == 0 || reqsc < oldsc);

        // If fits in current slot, just return it
        if unlikely(reqsc <= oldsc) {
            return lp_mem;
        }

        // Need larger slot - check HEAP_REALLOC_IN_PLACE_ONLY
        if unlikely(dw_flags & HEAP_REALLOC_IN_PLACE_ONLY != 0) {
            unsafe { raise_oom_if_needed(dw_flags) };
            return null_mut();
        }

        // If we reached this line then the HEAP_ZERO_MEMORY flag must be off.
        let new_ptr = smalloc_inner_alloc(reqsc, false);

        if unlikely(new_ptr.is_null()) {
            unsafe { raise_oom_if_needed(dw_flags) };
            return null_mut();
        }

        // Copy the old data
        unsafe { core::ptr::copy_nonoverlapping(lp_mem, new_ptr, 1 << oldsc) };

        // Free old slot
        SMALLOC.inner_dealloc(lp_mem.addr());

        new_ptr
            }

        // Since this is the smalloc heap, pointer must be smalloc or null/sentinel (never foreign)
        assert!(matches!(ptr_class, PtrClass::Smalloc | PtrClass::Null | PtrClass::Sentinel));

        if unlikely(matches!(ptr_class, PtrClass::Null | PtrClass::Sentinel)) {
            return unsafe { smalloc_HeapAlloc(h_heap, dw_flags, dw_bytes) };
        }
    } else {
        platform::call_system_HeapReAlloc(h_heap, dw_flags, lp_mem, dw_bytes)
    }
}

/// Returns the size of a memory block allocated from a heap
/// 
/// # Safety
/// Same requirements as Windows API HeapSize
#[unsafe(no_mangle)]
pub unsafe extern "system" fn smalloc_HeapSize(
    h_heap: HANDLE,
    dw_flags: u32,
    lp_mem: *const c_void,
) -> usize {
    if is_smalloc_heap(h_heap) {
        let ptr_class = classify_ptr(lp_mem);

        // Since this is the smalloc heap, pointer must be smalloc or null/sentinel (never foreign)
        assert!(matches!(ptr_class, PtrClass::Smalloc | PtrClass::NullOrSentinel));

        if likely(matches!(ptr_class, PtrClass::Smalloc)) {
            let sc = ptr_to_sc(lp_mem as *mut c_void);
            1 << sc
        } else {
            // If the user passes a NULL pointer to HeapSize, the system implementation aborts. So
            // we'll do that, too.
            assert!(!lp_mem.is_null());

            // This must be a smalloc sentinel pointer.
            0
        }
    } else {
        platform::call_system_HeapSize(h_heap, dw_flags, lp_mem)
    }
}
