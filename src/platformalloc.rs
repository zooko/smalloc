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

    use crate::platformalloc::AllocFailed;
    use rustix::mm::{MapFlags, ProtFlags, mmap_anonymous, munmap};
    use std::ffi::c_void;
    use std::ptr;

    pub type ClockType = i32;

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

    pub type ClockType = u32;

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

// for Windows, check out VirtualAllocEx with MEM_RESERVE flag: https://learn.microsoft.com/en-us/windows/win32/memory/page-state
// https://stackoverflow.com/questions/15261527/how-can-i-reserve-virtual-memory-in-linux?rq=1
//xxx look into VirtualAlloc on windows and the difference between "reserve" and "commit"...

// mimalloc by default madvise's transparent hugepage support *think*
