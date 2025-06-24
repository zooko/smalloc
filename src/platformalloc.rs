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

pub fn sys_realloc(ptr: *mut u8, oldlayout: Layout, newsize: usize) -> *mut u8 {
    // xxx add tests?
    debug_assert!(newsize > 0);
    let oldsize = oldlayout.size();
    debug_assert!(oldsize > 0);
    let oldalignment = oldlayout.align();
    debug_assert!(oldalignment > 0);
    debug_assert!((oldalignment & (oldalignment - 1)) == 0); // alignment must be a power of two

    let new_ptr = vendor::sys_realloc(ptr, oldsize, newsize);
    debug_assert!(new_ptr.is_aligned_to(oldalignment));

    new_ptr
}

#[cfg(target_os = "linux")]
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
    pub fn sys_realloc(p: *mut u8, oldsize: usize, newsize: usize) -> *mut u8 {
        unsafe {
            mremap(p as *mut c_void, oldsize, newsize, MremapFlags::MAYMOVE)
                .ok()
                .unwrap() as *mut u8
        }
    }

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

#[cfg(target_vendor = "apple")]
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
    use mach_sys::vm::{mach_vm_allocate, mach_vm_deallocate, mach_vm_remap};
    use mach_sys::vm_inherit::VM_INHERIT_NONE;
    use mach_sys::vm_prot::vm_prot_t;
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

    use std::ptr::copy_nonoverlapping;
    /// Because I get `KERN_INVALID_ADDRESS` from `sys_realloc_if_vm_remap_did_what_i_want()`, I'm
    /// instead just allocating another mapping and copying the data into it:
    pub fn sys_realloc(p: *mut u8, oldsize: usize, newsize: usize) -> *mut u8 {
        debug_assert!(p.is_aligned_to(PAGE_SIZE));

        // `smalloc`'s uses `sys_realloc()` only when it needs to grow the space.
        debug_assert!(newsize > oldsize);

        let newp = sys_alloc(newsize.next_multiple_of(PAGE_SIZE)).unwrap();
        unsafe { copy_nonoverlapping(p, newp, oldsize); }
        sys_dealloc(p, oldsize);

        newp
    }
            
// inline
    pub fn _sys_realloc_if_vm_remap_did_what_i_want(p: *mut u8, _oldsize: usize, newsize: usize) -> *mut u8 {
        debug_assert!(p.is_aligned_to(PAGE_SIZE));

        let mut newaddress: mach_vm_address_t = 0;
        let task: mach_port_t = unsafe { mach_task_self() };
        let mut cur_prot: vm_prot_t = 0;
        let mut max_prot: vm_prot_t = 0;
        unsafe {
            let retval = mach_vm_remap(
                task,
                &mut newaddress,
                newsize as u64,
                0, // mask
                VM_FLAGS_ANYWHERE,
                task,
                p.addr() as u64,
                0, // copy = False
                &mut cur_prot,
                &mut max_prot,
                VM_INHERIT_NONE,
            );
            debug_assert!(retval == KERN_SUCCESS, "retval: {retval}, newsize: {newsize}, newaddress: {newaddress:?}, p: {p:?}");
        }

        newaddress as *mut u8
    }
}

#[cfg(test)]
mod platformtests {
    const BYTES1: [u8; 8] = [1, 2, 4, 3, 5, 6, 7, 8];

    //#[test]
    fn _realloc_16kib_down_to_8kib_realloc_back_up_to_16kib_pages_1_1_1() {
        const SIZE: usize = 2usize.pow(13);

        let p1 = crate::platformalloc::vendor::sys_alloc(SIZE * 2).ok().unwrap();
        assert!(!p1.is_null());
        let p2 = crate::platformalloc::vendor::sys_realloc(p1, SIZE * 2, SIZE);
        assert!(!p2.is_null());
        let p3 = crate::platformalloc::vendor::sys_realloc(p2, SIZE, SIZE * 2);
        assert!(!p3.is_null());
    }

    //#[test]
    fn _realloc_32kib_plus1_down_to_16kib_plus1_then_realloc_back_up_to_32kib_pages_3_2_2() {
        const PAGE_SIZE: usize = 2usize.pow(14);

        let p1 = crate::platformalloc::vendor::sys_alloc(PAGE_SIZE * 2 + 1).ok().unwrap();
        assert!(!p1.is_null());
        let p2 = crate::platformalloc::vendor::sys_realloc(p1, PAGE_SIZE * 2, PAGE_SIZE + 1);
        assert!(!p2.is_null());
        let p3 = crate::platformalloc::vendor::sys_realloc(p2, PAGE_SIZE + 1, PAGE_SIZE * 2);
        assert!(!p3.is_null());
    }

    //#[test]
    fn _realloc_32kib_plus1_down_to_16kib_plus1_then_realloc_back_up_to_32kib_plus1_pages_3_2_3() {
        const PAGE_SIZE: usize = 2usize.pow(14);

        let p1 = crate::platformalloc::vendor::sys_alloc(PAGE_SIZE * 2 + 1).ok().unwrap();
        assert!(!p1.is_null());
        let p2 = crate::platformalloc::vendor::sys_realloc(p1, PAGE_SIZE * 2, PAGE_SIZE + 1);
        assert!(!p2.is_null());
        let p3 = crate::platformalloc::vendor::sys_realloc(p2, PAGE_SIZE + 1, PAGE_SIZE * 2 + 1);
        assert!(!p3.is_null());
    }

    //#[test]
    fn _realloc_48kib_down_to_32kib_then_realloc_back_up_to_48kib_pages_3_2_3() {
        const PAGE_SIZE: usize = 2usize.pow(14);

        let p1 = crate::platformalloc::vendor::sys_alloc(PAGE_SIZE * 3).ok().unwrap();
        assert!(!p1.is_null());
        let p2 = crate::platformalloc::vendor::sys_realloc(p1, PAGE_SIZE * 3, PAGE_SIZE * 2);
        assert!(!p2.is_null());
        let p3 = crate::platformalloc::vendor::sys_realloc(p2, PAGE_SIZE * 2, PAGE_SIZE * 3);
        assert!(!p3.is_null());
    }

    //#[test]
    fn _realloc_32kib_down_to_16kib_plus1_then_realloc_back_up_to_32kib_plus1_pages_2_2_3() {
        const PAGE_SIZE: usize = 2usize.pow(14);

        let p1 = crate::platformalloc::vendor::sys_alloc(PAGE_SIZE * 2).ok().unwrap();
        assert!(!p1.is_null());
        let p2 = crate::platformalloc::vendor::sys_realloc(p1, PAGE_SIZE * 2, PAGE_SIZE + 1);
        assert!(!p2.is_null());
        let p3 = crate::platformalloc::vendor::sys_realloc(p2, PAGE_SIZE + 1, PAGE_SIZE * 2 + 1);
        assert!(!p3.is_null());
    }

    //#[test]
    fn _realloc_32kib_down_to_16kib_then_realloc_back_up_to_48kib_pages_2_1_3() {
        const PAGE_SIZE: usize = 2usize.pow(14);

        let p1 = crate::platformalloc::vendor::sys_alloc(PAGE_SIZE * 2).ok().unwrap();
        assert!(!p1.is_null());
        let p2 = crate::platformalloc::vendor::sys_realloc(p1, PAGE_SIZE * 2, PAGE_SIZE);
        assert!(!p2.is_null());
        let p3 = crate::platformalloc::vendor::sys_realloc(p2, PAGE_SIZE, PAGE_SIZE * 3);
        assert!(!p3.is_null());
    }

    //#[test]
    fn _realloc_8kib_down_to_4kib_then_realloc_back_up_to_48kib_pages_1_1_3() {
        const PAGE_SIZE: usize = 2usize.pow(14);

        let p1 = crate::platformalloc::vendor::sys_alloc(PAGE_SIZE / 2).ok().unwrap();
        assert!(!p1.is_null());
        let p2 = crate::platformalloc::vendor::sys_realloc(p1, PAGE_SIZE / 2, PAGE_SIZE / 4);
        assert!(!p2.is_null());
        let p3 = crate::platformalloc::vendor::sys_realloc(p2, PAGE_SIZE / 4, PAGE_SIZE * 3);
        assert!(!p3.is_null());
    }

    #[test]
    fn malloc_32kib_then_realloc_to_48kib_pages_2_3() {
        const PAGE_SIZE: usize = 2usize.pow(14);

        let p1 = crate::platformalloc::vendor::sys_alloc(PAGE_SIZE).ok().unwrap();
        assert!(!p1.is_null());
        let p2 = crate::platformalloc::vendor::sys_realloc(p1, PAGE_SIZE, PAGE_SIZE * 3);
        assert!(!p2.is_null());
    }

    #[test]
    fn malloc_4kib_then_realloc_to_48kib_pages_1_3() {
        const PAGE_SIZE: usize = 2usize.pow(14);

        let p1 = crate::platformalloc::vendor::sys_alloc(PAGE_SIZE / 4).ok().unwrap();
        assert!(!p1.is_null());
        let p2 = crate::platformalloc::vendor::sys_realloc(p1, PAGE_SIZE / 4, PAGE_SIZE * 3);
        assert!(!p2.is_null());
    }

    #[test]
    fn malloc_4kib_then_realloc_to_32kib_pages_1_2() {
        const PAGE_SIZE: usize = 2usize.pow(14);

        let p1 = crate::platformalloc::vendor::sys_alloc(PAGE_SIZE / 4).ok().unwrap();
        assert!(!p1.is_null());
        let p2 = crate::platformalloc::vendor::sys_realloc(p1, PAGE_SIZE / 4, PAGE_SIZE * 2);
        assert!(!p2.is_null());
    }

    #[test]
    fn malloc_4kib_then_realloc_to_32kib_plus1_pages_1_3() {
        const PAGE_SIZE: usize = 2usize.pow(14);

        let p1 = crate::platformalloc::vendor::sys_alloc(PAGE_SIZE / 4).ok().unwrap();
        assert!(!p1.is_null());
        let p2 = crate::platformalloc::vendor::sys_realloc(p1, PAGE_SIZE / 4, PAGE_SIZE * 2 + 1);
        assert!(!p2.is_null());
    }

    //#[test]
    fn _realloc_32kib_down_to_16kib_plus1_then_realloc_back_up_to_48kib_pages_2_2_3() {
        const PAGE_SIZE: usize = 2usize.pow(14);

        let p1 = crate::platformalloc::vendor::sys_alloc(PAGE_SIZE * 2).ok().unwrap();
        assert!(!p1.is_null());
        let p2 = crate::platformalloc::vendor::sys_realloc(p1, PAGE_SIZE * 2, PAGE_SIZE + 1);
        assert!(!p2.is_null());
        let p3 = crate::platformalloc::vendor::sys_realloc(p2, PAGE_SIZE + 1, PAGE_SIZE * 3);
        assert!(!p3.is_null());
    }

    //#[test]
    fn _realloc_32kib_down_to_16kib_plus1_then_realloc_back_up_to_32kib_pages_2_2_2() {
        const PAGE_SIZE: usize = 2usize.pow(14);

        let p1 = crate::platformalloc::vendor::sys_alloc(PAGE_SIZE * 2).ok().unwrap();
        assert!(!p1.is_null());
        let p2 = crate::platformalloc::vendor::sys_realloc(p1, PAGE_SIZE * 2, PAGE_SIZE + 1);
        assert!(!p2.is_null());
        let p3 = crate::platformalloc::vendor::sys_realloc(p2, PAGE_SIZE + 1, PAGE_SIZE * 2);
        assert!(!p3.is_null());
    }

    //#[test]
    fn _realloc_32kib_down_to_16kib_plus1_then_realloc_back_up_to_32kib_with_write_pages_2_2_2() {
        const PAGE_SIZE: usize = 2usize.pow(14);

        let p1 = crate::platformalloc::vendor::sys_alloc(PAGE_SIZE * 2).ok().unwrap();
        assert!(!p1.is_null());
        let p2 = crate::platformalloc::vendor::sys_realloc(p1, PAGE_SIZE * 2, PAGE_SIZE + 1);
        assert!(!p2.is_null());
        unsafe { std::ptr::copy_nonoverlapping(BYTES1.as_ptr(), p2.add(PAGE_SIZE + 1 - BYTES1.len()), min(BYTES1.len(), PAGE_SIZE + 1)) };
        let p3 = crate::platformalloc::vendor::sys_realloc(p2, PAGE_SIZE + 1, PAGE_SIZE * 2);
        assert!(!p3.is_null());
    }

    use std::cmp::min;

    use std::thread;
    use std::time::Duration;
    
    //#[test]
    fn _realloc_down_realloc_back_up_16kib_plus1_with_write_and_wait_30() {
        const SIZE: usize = 2usize.pow(13)+1;
        const BYTES1: [u8; 8] = [1, 2, 4, 3, 5, 6, 7, 8];

        let p1 = crate::platformalloc::vendor::sys_alloc(SIZE * 2).ok().unwrap();
        assert!(!p1.is_null());
        let p2 = crate::platformalloc::vendor::sys_realloc(p1, SIZE * 2, SIZE);
        assert!(!p2.is_null());
        unsafe { std::ptr::copy_nonoverlapping(BYTES1.as_ptr(), p2.add(SIZE - BYTES1.len()), min(BYTES1.len(), SIZE)) };
        thread::sleep(Duration::from_secs(30));
        let p3 = crate::platformalloc::vendor::sys_realloc(p2, SIZE, SIZE * 2);
        assert!(!p3.is_null());
    }

    //#[test]
    fn _realloc_down_realloc_back_up_16kib_plus1_with_write_and_wait_300() {
        const SIZE: usize = 2usize.pow(13)+1;
        const BYTES1: [u8; 8] = [1, 2, 4, 3, 5, 6, 7, 8];

        let p1 = crate::platformalloc::vendor::sys_alloc(SIZE * 2).ok().unwrap();
        assert!(!p1.is_null());
        let p2 = crate::platformalloc::vendor::sys_realloc(p1, SIZE * 2, SIZE);
        assert!(!p2.is_null());
        unsafe { std::ptr::copy_nonoverlapping(BYTES1.as_ptr(), p2.add(SIZE - BYTES1.len()), min(BYTES1.len(), SIZE)) };
        thread::sleep(Duration::from_secs(300));
        let p3 = crate::platformalloc::vendor::sys_realloc(p2, SIZE, SIZE * 2);
        assert!(!p3.is_null());
    }

    //#[test]
    fn _realloc_32kib_down_to_16kib_plus1_then_realloc_back_up_to_32kib_with_intervening_alloc_dealloc_and_write_pages_2_2_2() {
        const PAGE_SIZE: usize = 2usize.pow(14);
        const BYTES1: [u8; 8] = [1, 2, 4, 3, 5, 6, 7, 8];

        let p1 = crate::platformalloc::vendor::sys_alloc(PAGE_SIZE * 2).ok().unwrap();
        assert!(!p1.is_null());
        let p2 = crate::platformalloc::vendor::sys_realloc(p1, PAGE_SIZE * 2, PAGE_SIZE + 1);
        assert!(!p2.is_null());
        unsafe { std::ptr::copy_nonoverlapping(BYTES1.as_ptr(), p2.add(PAGE_SIZE + 1 - BYTES1.len()), min(BYTES1.len(), PAGE_SIZE)) };
        let p4 = crate::platformalloc::vendor::sys_alloc(PAGE_SIZE).ok().unwrap();
        assert!(!p4.is_null());
        let p5 = crate::platformalloc::vendor::sys_alloc(PAGE_SIZE * 2).ok().unwrap();
        assert!(!p5.is_null());
        let p6 = crate::platformalloc::vendor::sys_alloc(PAGE_SIZE + 1).ok().unwrap();
        assert!(!p6.is_null());
        crate::platformalloc::vendor::sys_dealloc(p4, PAGE_SIZE);
        crate::platformalloc::vendor::sys_dealloc(p5, PAGE_SIZE * 2);
        crate::platformalloc::vendor::sys_dealloc(p6, PAGE_SIZE + 1);
        let p3 = crate::platformalloc::vendor::sys_realloc(p2, PAGE_SIZE + 1, PAGE_SIZE * 2);
        assert!(!p3.is_null());
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
