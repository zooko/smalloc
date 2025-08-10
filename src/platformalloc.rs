// Abstract over system virtual memory functions

use std::alloc::Layout;

#[derive(Debug)]
pub struct AllocFailed;

impl std::error::Error for AllocFailed {}

use std::fmt;
impl fmt::Display for AllocFailed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Alloc failed")
    }
}

pub fn sys_alloc(layout: Layout) -> Result<*mut u8, AllocFailed> {
    // xxx add unit tests?
    let size = layout.size();
    debug_assert!(size > 0);
    let alignment = layout.align();
    debug_assert!(alignment > 0);
    debug_assert!((alignment & (alignment - 1)) == 0); // alignment must be a power of two

    let ptr = vendor::sys_alloc(size)?;
    debug_assert!(ptr.is_aligned_to(alignment));

    Ok(ptr)
}

pub fn sys_dealloc(ptr: *mut u8, layout: Layout) {
    // xxx add tests?
    let size = layout.size();
    debug_assert!(size > 0);
    let alignment = layout.align();
    debug_assert!(alignment > 0);
    debug_assert!((alignment & (alignment - 1)) == 0); // alignment must be a power of two

    vendor::sys_dealloc(ptr, size);
}

#[cfg(any(target_os = "linux", doc))]
pub mod vendor {
    // Okay these constants are sometimes incorrect or at least an "over-simplification", but they
    // are probably only wrong by being smaller than they should be (never larger), and being
    // smaller than they should be is probably just a very small loss of performance. The other
    // thing these constants are used for is to run the biggest benchmarks we can without incurring
    // the wrath of the linux OOM-killer, so again it's not a huge problem if the constants are too
    // small for your actual machine.

    // These constants are set for my Linux Intel(R) Xeon(R) CPU E5-2698 v4. They're not *really*
    // true of all linux products. 😂 But the default page size on all linux's that I know of is
    // 4096. The most common exceptions are huge pages (which constitute a design issue for the
    // entire design of `smalloc`, really.)
    pub const PAGE_SIZE: usize = 4096;

    // But the cache line size is pretty much universally true of Intel, AMD, and non-Apple ARM chips.
    pub const CACHE_LINE_SIZE: usize = 64;

    // This const is set for my Linux Intel(R) Xeon(R) CPU E5-2698 v4.
    pub const CACHE_SIZE: usize = 2usize.pow(24);

    use crate::platformalloc::AllocFailed;
    use rustix::mm::{MapFlags, MremapFlags, ProtFlags, mmap_anonymous, mremap, munmap};
    use std::ffi::c_void;
    use std::ptr;

// inline
    pub fn sys_alloc(reqsize: usize) -> Result<*mut u8, AllocFailed> {
        match unsafe {
            mmap_anonymous(
                ptr::null_mut(),
                reqsize,
                ProtFlags::READ | ProtFlags::WRITE,
                MapFlags::PRIVATE | MapFlags::NORESERVE,
            )
        } {
            Ok(p) => Ok(p as *mut u8),
            Err(_) => Err(AllocFailed),
        }
    }

// inline
    pub fn sys_dealloc(p: *mut u8, size: usize) {
        unsafe {
            munmap(p as *mut c_void, size).ok();
        }
    }

// inline
    // Investigating the effects of MADV_RANDOM:
    // mm/madvise.c: MADV_RANDOM -> VM_RAND_READ
    // Documentation/mm/multigen_lru.rst: VM_RAND_READ means do not assume that accesses through page tables will exhibit temporal locality. 🤔

    // filemap.c: it looks like VM_RAND_READ disables readahead, although I don't know if that's true only for file-backed mmaps or also for anonymous mmaps.

    // vma_has_recency() always returns false if VM_RAND_READ
    // memory.c: if !vma_has_recency() then in_lru_fault = false
    // workingset.c, skip LRU
    // vmscan.c: should_skip_vma(). It always "skips" if !vma_has_recency(). What does "skipping" do? should_skip_vma() is used in get_next_vma(). What does get_next_vma() do? It's used from walk_pte_range(), walk_pmd_range(), and walk_pud_range()... and there are functions of the same names but different arguments in pagewalk.c. 🤔 I'm guessing vmscan.c is for evicting pages to repurpose physical memory and pagewalk.c is for something else... let's see if I can confirm that... Well, I couldn't confirm it by looking at the linux source code, but I asked the Brave AI what was the difference and it said what I thought -- vmscan is for reclamation of memory.
    // Okay I'm going to give up on this research project and just conclude that *default* behavior (not MADV_RANDOM) is probably good for smalloc's purposes. I have no idea what it would mean to exclude certain pages from an LRU policy, nor what it means to skip these vma's when walking page tables. But neither of them sound like something we really want for smalloc's allocations.
}

#[cfg(any(target_vendor = "apple", doc))]
pub mod vendor {
    // Okay these constants are sometimes incorrect or at least an "over-simplification", but they
    // are probably only wrong by being smaller than they should be (never larger), and being
    // smaller than they should be is probably just a very small loss of performance. The other
    // thing these constants are used for is to run the biggest benchmarks we can without incurring
    // the wrath of the linux OOM-killer, so again it's not a huge problem if the constants are too
    // small for your actual machine.

    // These consts are set for my Apple M4 Max -- they're not *really* true of all Apple products.
    pub const PAGE_SIZE: usize = 16384;
    pub const CACHE_LINE_SIZE: usize = 128;

    // This const is set for my Apple M4 Max -- it's not *really* true of all Apple products.
    pub const CACHE_SIZE: usize = 20 * 2usize.pow(20);

    use crate::platformalloc::AllocFailed;
    use mach_sys::kern_return::KERN_SUCCESS;
    use mach_sys::port::mach_port_t;
    use mach_sys::traps::mach_task_self;
    use mach_sys::vm::{mach_vm_allocate, mach_vm_deallocate};
    use mach_sys::vm_statistics::VM_FLAGS_ANYWHERE;
    use mach_sys::vm_types::{mach_vm_address_t, mach_vm_size_t};
    use std::mem::size_of;

// inline
    pub fn sys_alloc(size: usize) -> Result<*mut u8, AllocFailed> {
        let task: mach_port_t = unsafe { mach_task_self() };
        let mut address: mach_vm_address_t = 0;
        let size: mach_vm_size_t = size as mach_vm_size_t;

        let retval;
        unsafe {
            retval = mach_vm_allocate(task, &mut address, size, VM_FLAGS_ANYWHERE);
        }
        if retval == KERN_SUCCESS {
            Ok(address as *mut u8)
        } else {
            Err(AllocFailed)
        }
    }

// inline
    pub fn sys_dealloc(p: *mut u8, size: usize) {
        debug_assert!(size_of::<usize>() == size_of::<u64>());
        debug_assert!(size_of::<*mut u8>() == size_of::<u64>());

        unsafe {
            let retval = mach_vm_deallocate(mach_task_self(), p as u64, size as u64);
            debug_assert!(retval == KERN_SUCCESS);
        }
    }
}

// -> mach_vm_remap(target_task, newaddress-out, newsize, anywhere, src_task, memory_address_u, ...);
// -> mach_vm_remap_external(target_map, address, size, mask, flags, src_map, memory_address, ...);
//     ... (target_map, address, size, mask, flags, src_map memory_address, ...)
//       -> vm_map_remap(target_map, address, size, mask, ..., src_map, memory_address, ...);
// vm_map.c @ 18927 vm_map_remap(target_map, address_u, size_u, ..., ..., src_map, memory_address_u, ...) {
// vm_map_remap_sanitize() -> vm_sanitize_addr_size()
//    src_map -> pgmask
// results in:
//       *addr (6th arg to vm_sanitize_addr_size which is memory_address in vm_map_remap_sanitize, which is memory_address_u in vm_map_remap) <= truncated copy of addr_u (1st argument of vm_sanitize_addr_size which is memory_address_u in vm_map_remap_sanitize, which is memory_address_u in vm_map_remap)
//             ===> so the only effect of this part is to truncate the value in memory_address_u which doesn't matter to us because it was already page-aligned
//       *end (7th arg of vm_sanitize_addr_size which is memory_end (arg 14) in vm_map_remap_sanitize, which is &memory_end in vm_map_remap) <- rounded-up *addr (6th arg which is memory_address in vm_map_remap_sanitize, which is vm_map_remap's memory_address_u) + size_u (2nd arg to vm_sanitize_addr_size, which is size_u (4th arg) of vm_map_remap_sanitize, which is size_u (3rd arg) of vm_map_remap)
//             ===> so the effect of this part is the value in vm_map_remap's memory_end is set to vm_map_remap's memory_address_u rounded-up + size_u
//             ===> and then it set size = memory_end - memory_address_u
// vm_map_remap_sanitize() -> vm_sanitize_addr()
//    target_map -> map
//    address_u -> addr_u
// results in *target_addr (11th arg of vm_map_remap_sanitize) being truncated copy of address_u (3rd arg of vm_map_remap_sanitize)
// 
// memory_address, memory_size <-
// vm_sanitize_addr_size(memory_address_u, size_u
// 
//  -> vm_map_copy_extract(src_map, memory_address, memory_size, ...)

// for Windows, check out VirtualAllocEx with MEM_RESERVE flag: https://learn.microsoft.com/en-us/windows/win32/memory/page-state
// https://stackoverflow.com/questions/15261527/how-can-i-reserve-virtual-memory-in-linux?rq=1
//xxx look into VirtualAlloc on windows and the difference between "reserve" and "commit"...

// mimalloc by default madvise's transparent hugepage support *think*
