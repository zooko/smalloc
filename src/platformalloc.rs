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
    // xxx add tests?
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

    vendor::sys_dealloc(ptr, size)
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
    pub const PAGE_SIZE: usize = 4096;

    use crate::platformalloc::AllocFailed;
    use rustix::mm::{MapFlags, MremapFlags, ProtFlags, mmap_anonymous, mremap, munmap};
    use std::ffi::c_void;
    use std::ptr;

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

    pub fn sys_dealloc(p: *mut u8, size: usize) {
        unsafe {
            munmap(p as *mut c_void, size).ok();
        }
    }

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
    pub const PAGE_SIZE: usize = 16384;

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

    pub fn sys_dealloc(p: *mut u8, size: usize) {
        debug_assert!(size_of::<usize>() == size_of::<u64>());
        debug_assert!(size_of::<*mut u8>() == size_of::<u64>());

        unsafe {
            let retval = mach_vm_deallocate(mach_task_self(), p as u64, size as u64);
            debug_assert!(retval == KERN_SUCCESS);
        }
    }

    pub fn sys_realloc(p: *mut u8, _oldsize: usize, newsize: usize) -> *mut u8 {
        debug_assert!(size_of::<*mut u8>() == size_of::<u64>());

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
            debug_assert!(retval == KERN_SUCCESS);
        }

        newaddress as *mut u8
    }
}

// for Windows, check out VirtualAllocEx with MEM_RESERVE flag: https://learn.microsoft.com/en-us/windows/win32/memory/page-state
// https://stackoverflow.com/questions/15261527/how-can-i-reserve-virtual-memory-in-linux?rq=1
//xxx look into VirtualAlloc on windows and the difference between "reserve" and "commit"...
