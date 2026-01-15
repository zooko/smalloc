#![feature(likely_unlikely)]

// Thanks to Claude Sonnet 4.5 for writing the initial version of this whole file for me, and
// updating it together with me. As well as assisting on ideation about how to do this.

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
// "SMAL" in ASCII hex - chosen to be an invalid pointer
const SMALLOC_HEAP_HANDLE: HANDLE = 0x534D414C as HANDLE;

// Error codes
const ENOMEM: i32 = 12;
const EINVAL: i32 = 22;

// Heap flags
const HEAP_ZERO_MEMORY: u32 = 0x00000008;
const HEAP_REALLOC_IN_PLACE_ONLY: u32 = 0x00000010;

// Size-0 allocation sentinel. (The official Windows
// docs)(https://learn.microsoft.com/en-us/windows/win32/api/heapapi/nf-heapapi-heapalloc) are
// silent on the question of what `HeapAlloc` will do if you request 0 bytes. Experimentation show
// me that `HeapAlloc(..., 0)` on Windows 11 returns a non-null pointer that can be freed but that
// is reported as pointing to an allocation of 0 bytes by `HeapSize`. So, our implementation of
// `HeapAlloc` will do that. (Which, BTW, is also what `smalloc-ffi-c-api` does, because some code
// written to the C-API requires it.)
const SIZE_0_ALLOC_SENTINEL: *mut c_void = core::ptr::dangling_mut::<c_void>();

// =============================================================================
// Pointer classification
// =============================================================================

enum PtrClass {
    NullOrSentinel,
    Smalloc,
    Foreign,
}

#[inline(always)]
fn classify_ptr(ptr: *mut c_void) -> PtrClass {
    if unlikely(ptr.is_null() || ptr == SIZE_0_ALLOC_SENTINEL) {
        return PtrClass::NullOrSentinel;
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

// gen_mask macro for readability
macro_rules! gen_mask { ($bits:expr, $ty:ty) => { ((!0 as $ty) >> (<$ty>::BITS - ($bits) as u32)) }; }

// Windows HeapAlloc guarantees 8-byte alignment of returned pointers, even if the requested size is
// smaller than 8 bytes. So we have to skip using size class 2 (which has 4-byte slots), in addition
// to the way we already skip sizes classes 0 and 1.
const UNUSED_SC_MASK: usize = gen_mask!(3, usize);

/// Windows HeapAlloc guarantees 8-byte alignment minimum
#[inline(always)]
fn req_to_sc(siz: usize) -> u8 {
    debug_assert!(siz > 0);
    (((effective_size - 1) | UNUSED_SC_MASK).ilog2() + 1) as u8
}

#[inline(always)]
fn smalloc_inner_alloc(sc: u8, zeromem: bool) -> *mut c_void {
    SMALLOC.idempotent_init();
    SMALLOC.inner_alloc(sc, zeromem) as *mut c_void
}

mod platform {
    use super::*;

    // Import original system Heap API functions that smalloc needs to use.
    // These are exported by our DEF file as "System<FunctionName>"
    extern "system" {
        fn SystemGetProcessHeap() -> HANDLE;
        fn SystemGetProcessHeaps(NumberOfHeaps: u32, ProcessHeaps: *mut HANDLE) -> u32;
        fn SystemHeapFree(hHeap: HANDLE, dwFlags: u32, lpMem: *mut c_void) -> i32;
        fn SystemHeapReAlloc(hHeap: HANDLE, dwFlags: u32, lpMem: *mut c_void, dwBytes: usize) -> *mut c_void;
        fn SystemHeapSize(hHeap: HANDLE, dwFlags: u32, lpMem: *const c_void) -> usize;
    }

    #[inline(always)]
    pub(crate) fn call_system_GetProcessHeap() -> HANDLE {
        unsafe { SystemGetProcessHeap() }
    }

    #[inline(always)]
    pub(crate) fn SystemGetProcessHeaps(NumberOfHeaps: u32, ProcessHeaps: *mut HANDLE) -> u32 {
        unsafe { SystemGetProcessHeaps(NumberOfHeaps, HANDLE) }
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
}

/// Returns a handle to the default process heap (smalloc)
/// 
/// # Safety
/// Same requirements as Windows API GetProcessHeap
#[no_mangle]
pub unsafe extern "system" fn smalloc_GetProcessHeap() -> HANDLE {
    SMALLOC_HEAP_HANDLE
}

/// This reflects the result from the underlying system's GetProcessHeaps, except that any instance
/// of the System's default heap is replaced by our smalloc default heap sentinel value.
/// 
/// # Safety
/// Same requirements as Windows API GetProcessHeaps
#[no_mangle]
pub unsafe extern "system" fn smalloc_GetProcessHeaps(
    NumberOfHeaps: u32,
    ProcessHeaps: *mut HANDLE,
) -> u32 {
    let res = SystemGetProcessHeaps(NumberOfHeaps, ProcessHeaps);

    // xxx Hey Claude, please iterate this list of heaps and if any one of them is the underlying
    // system default heap (i.e. the handle is the same as is returned from SystemGetProcessHeap())
    // then replace it with our smalloc default heap sentinel.

    res
}

/// Allocates a block of memory from smalloc's heap
/// 
/// # Safety
/// Same requirements as Windows API HeapAlloc
#[no_mangle]
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
            // xxx hey Claude, please check if the HEAP_GENERATE_EXECPTIONS flag is present in the dw_flags and if so raise a C++ exception of the right type here.
            return null_mut();
        }

        let ptr = if unlikely(dw_flags & HEAP_ZERO_MEMORY != 0) {
            smalloc_inner_alloc(sc, true);
        } else {
            smalloc_inner(sc, true);
        };

        // xxx hey Claude, please check if the HEAP_GENERATE_EXECPTIONS flag is present in the dw_flags and if so raise a C++ exception of the right type here, if ptr is null.

        ptr
    } else {
        // Foreign heap - delegate to original system implementation
        platform::call_system_HeapAlloc(h_heap, dw_flags, dw_bytes)
    }
}

/// Frees a memory block allocated from a heap
/// 
/// # Safety
/// Same requirements as Windows API HeapFree
#[no_mangle]
pub unsafe extern "system" fn smalloc_HeapFree(
    h_heap: HANDLE,
    dw_flags: u32,
    lp_mem: *mut c_void,
) -> i32 {
    if likely(is_smalloc_heap(h_heap)) {
        match classify_ptr(lp_mem) {
            PtrClass::Smalloc => {
                SMALLOC.inner_dealloc(lp_mem.addr());
                1 // TRUE
            }
            PtrClass::Foreign => {
                // xxx Hey Claude, there is no way HeapFree can be called passing the smalloc heap handle and a foreign lp_mem. Please add a debug_assert at the beginning of this function that if h_heap is smalloc's heap then lp_mem must be a smalloc pointer or a NullOrSentinel. And then figure out what to do with this branch of the match. Maybe mark it as unreachable? Or, perhaps a little cleaner, use some kind of if else statement instead of the match expression to branch between Smalloc ptr and NullOrSentinel ptr. If you do the later then you can also mark the Smalloc Ptr branch as likely().
                // Pointer from original system heap
                let system_heap = platform::call_system_GetProcessHeap();
                platform::call_system_HeapFree(system_heap, dw_flags, lp_mem)
            }
            PtrClass::NullOrSentinel => 1, // TRUE (freeing null succeeds)
        }
    } else {
        // Foreign heap handle - forward to original system
        platform::call_system_HeapFree(h_heap, dw_flags, lp_mem)
    }
}

/// Reallocates a memory block from a heap
/// 
/// # Safety
/// Same requirements as Windows API HeapReAlloc
#[no_mangle]
pub unsafe extern "system" fn smalloc_HeapReAlloc(
    h_heap: HANDLE,
    dw_flags: u32,
    lp_mem: *mut c_void,
    dw_bytes: usize,
) -> *mut c_void {
    if is_smalloc_heap(h_heap) {
        match classify_ptr(lp_mem) {
            PtrClass::NullOrSentinel => {
                // Behave like HeapAlloc
                unsafe { smalloc_HeapAlloc(h_heap, dw_flags, dw_bytes) }
            }
            PtrClass::Smalloc => {
                if unlikely(dw_bytes == 0) {
                    SMALLOC.inner_dealloc(lp_mem.addr());
                    return SIZE_0_ALLOC_SENTINEL;
                }

                let reqsc = req_to_sc(dw_bytes);

                if unlikely(reqsc >= NUM_SCS) {
                    platform::set_last_error(8);
                    return null_mut();
                }

                let oldsc = ptr_to_sc(lp_mem);

                if unlikely((dw_flags & HEAP_ZERO_MEMORY != 0) && (reqsc >= oldsc)) {
                    // xxx we cannot zero out the (possible) new bytes as required by the Windows
                    // API because we don't know the exact size of the original allocation -- only
                    // that it fit into the old slot.

                    // xxx Claude, please confirm that you understand this dilemma, and please
                    // search for evidence about whether any code actually depends on the guarantee
                    // of the Windows API that the new bytes will be zeroed. For example, user code
                    // could realloc an existing allocation to a larger size, and then use a
                    // `strcmp` style function that relies on the first byte in the extended space
                    // being zero. Therefore, I see no way around it but we're going to have to
                    // fail-stop here. `smalloc` cannot safely implement `HeapReAlloc` with the
                    // `HEAP_ZERO_MEMORY` flag when the size grows.

                    // xxx Claude, please raise a C++ exception of type `STATUS_NO_MEMORY`, if
                    // `HEAP_GENERATE_EXCEPTIONS` is on, or else return NULL.
                    return null_mut();
                }

                // If fits in current slot, just return it
                if unlikely(reqsc <= oldsc) {
                    return lp_mem;
                }

                // Need larger slot
                if unlikely(dw_flags & HEAP_REALLOC_IN_PLACE_ONLY) {
                    // xxx Claude, please raise a C++ exception of type `STATUS_NO_MEMORY` if `HEAP_GENERATE_EXCEPTIONS` is on.
                    return null_mut();
                }

                let new_ptr = smalloc_inner_alloc(reqsc, dw_flags & HEAP_ZERO_MEMORY != 0);

                if unlikely(new_ptr.is_null()) {
                    // xxx Claude, please raise a C++ exception of type `STATUS_NO_MEMORY` if `HEAP_GENERATE_EXCEPTIONS` is on.
                    return null_mut();
                }

                // Copy the old data
                let old_size = 1 << oldsc;
                unsafe { core::ptr::copy_nonoverlapping(lp_mem, new_ptr, old_size) };

                // Free old slot
                SMALLOC.inner_dealloc(lp_mem.addr());

                new_ptr
            }
            // xxx Claude: please change this to either an "unreachable", or replace this match
            // entirely with an if then else (with a likely annotation). Because there cannot be
            // Foreign pointers in the smalloc heap. Also please add a debug_assert! right after the
            // classify_ptr asserting that it is not a foreign pointer in a smalloc heap.
            PtrClass::Foreign => {
                // Foreign pointer - delegate to original system
                let system_heap = platform::call_system_GetProcessHeap();
                platform::call_system_HeapReAlloc(system_heap, dw_flags, lp_mem, dw_bytes)
            }
        }
    } else {
        // Foreign heap - delegate entirely to original system
        platform::call_system_HeapReAlloc(h_heap, dw_flags, lp_mem, dw_bytes)
    }
}

/// Returns the size of a memory block allocated from a heap
/// 
/// # Safety
/// Same requirements as Windows API HeapSize
#[no_mangle]
pub unsafe extern "system" fn smalloc_HeapSize(
    h_heap: HANDLE,
    dw_flags: u32,
    lp_mem: *const c_void,
) -> usize {
    if is_smalloc_heap(h_heap) {
        match classify_ptr(lp_mem as *mut c_void) {
            PtrClass::Smalloc => {
                let sc = ptr_to_sc(lp_mem as *mut c_void);
                1 << sc
            }
            PtrClass::Foreign => {
                let system_heap = platform::call_system_GetProcessHeap();
                platform::call_system_HeapSize(system_heap, dw_flags, lp_mem)
            }
            // xxx Claude, please do the same transformation -- if it is a smalloc heap and a foreign pointer then this is an assertion failure in debug mode (using debug_assert!) and underfined behavior in release mode. (I don't believe it will ever be the case in practice.) And, either replace this match arm with unreachable or -- probably better -- replace the match expression with an if/then (with likely).
            PtrClass::NullOrSentinel => {
                platform::set_last_error(87); // ERROR_INVALID_PARAMETER
                usize::MAX // Indicates error
            }
        }
    } else {
        platform::call_system_HeapSize(h_heap, dw_flags, lp_mem)
    }
}

/// Creates a private heap (cannot be smalloc - delegate to system)
/// 
/// # Safety
/// Same requirements as Windows API HeapCreate
#[no_mangle]
pub unsafe extern "system" fn smalloc_HeapCreate(
    fl_options: u32,
    dw_initial_size: usize,
    dw_maximum_size: usize,
) -> HANDLE {
    platform::call_system_HeapCreate(fl_options, dw_initial_size, dw_maximum_size)
}

/// Destroys a private heap
/// 
/// # Safety
/// Same requirements as Windows API HeapDestroy
#[no_mangle]
pub unsafe extern "system" fn smalloc_HeapDestroy(h_heap: HANDLE) -> i32 {
    if is_smalloc_heap(h_heap) {
        // Cannot destroy the process default heap
        platform::set_last_error(87); // ERROR_INVALID_PARAMETER
        0 // FALSE
    } else {
        // Foreign heap - delegate to original system
        platform::call_system_HeapDestroy(h_heap)
    }
}
