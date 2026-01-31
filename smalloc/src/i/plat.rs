// Abstract over system virtual memory functions

#[derive(Debug)]
pub struct AllocFailed;

impl std::error::Error for AllocFailed {}

use std::fmt;
impl fmt::Display for AllocFailed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Alloc failed")
    }
}

#[cfg(target_os = "windows")]
pub mod p {
    use super::AllocFailed;
    use windows_sys::Win32::System::Memory::{VirtualAlloc2, MEM_COMMIT, MEM_RESERVE, PAGE_READWRITE, PAGE_NOACCESS};
    const MEM_64K_PAGES: u32 = 0x20400000;
    use windows_sys::Win32::Foundation::GetLastError;
    use core::ffi::c_void;
    use core::ptr::null_mut;

    // The size class necessary to hold a memory page, since we're using 64 KiB pages:
    pub const SC_FOR_PAGE: u8 = crate::reqali_to_sc(65_536, 65_536);

    #[allow(unsafe_code)]
    pub fn sys_alloc(reqsize: usize) -> Result<*mut u8, AllocFailed> {
        //eprintln!("about to alloc {reqsize}");
        let p = unsafe {
            VirtualAlloc2(null_mut(), std::ptr::null(), reqsize, MEM_RESERVE, PAGE_NOACCESS, null_mut(), 0)
        };

        if !p.is_null() {
            //eprintln!("succeeded to alloc {reqsize}");
            Ok(p as *mut u8)
        } else {
            let error = unsafe { GetLastError() };
            eprintln!("VirtualAlloc reserve failed with error code: {}", error); // xxx cant do as allocator

            //println!("Failed to alloc {reqsize}");
            Err(AllocFailed)
        }
    }

    #[allow(unsafe_code)]
    pub fn sys_commit(pin: *mut u8, reqsize: usize) -> Result<*mut u8, AllocFailed> {
        //eprintln!("about to commit {pin:p}, {reqsize}");
        let pout = unsafe {
            VirtualAlloc2(null_mut(), pin as *mut c_void, reqsize, MEM_COMMIT | MEM_64K_PAGES, PAGE_READWRITE, null_mut(), 0)
        };

        if !pout.is_null() {
            //eprintln!("succeeded to commit {pin:p}, {reqsize}");
            Ok(pout as *mut u8)
        } else {
            let error = unsafe { GetLastError() };
            println!("VirtualAlloc commit failed with error code: {}", error); // xxx cant do as allocator

            //eprintln!("failed to commit {pin:p}, {reqsize}");
            Err(AllocFailed)
        }
    }
}

#[cfg(target_os = "linux")]
pub mod p {
    use super::AllocFailed;
    use rustix::mm::{MapFlags, ProtFlags, mmap_anonymous};
    use std::ptr;

    // The size class necessary to hold a memory page, since memory pages on Linux (except in cases
    // of "huge pages") are 4 KiB.
    pub const SC_FOR_PAGE: u8 = crate::reqali_to_sc(4096, 4096);

    #[allow(unsafe_code)]
    pub fn sys_alloc(reqsize: usize) -> Result<*mut u8, AllocFailed> {
        match unsafe {
            mmap_anonymous(ptr::null_mut(), reqsize, ProtFlags::READ | ProtFlags::WRITE, MapFlags::PRIVATE | MapFlags::NORESERVE)
        } {
            Ok(p) => Ok(p as *mut u8),
            Err(_) => Err(AllocFailed),
        }
    }
}

#[cfg(target_vendor = "apple")]
pub mod p {
    use super::AllocFailed;
    use mach_sys::kern_return::KERN_SUCCESS;
    use mach_sys::port::mach_port_t;
    use mach_sys::traps::mach_task_self;
    use mach_sys::vm::mach_vm_allocate;
    use mach_sys::vm_statistics::VM_FLAGS_ANYWHERE;
    use mach_sys::vm_types::{mach_vm_address_t, mach_vm_size_t};

    // The size class necessary to hold a memory page, since memory pages on Macos are 16 KiB.
    pub const SC_FOR_PAGE: u8 = crate::reqali_to_sc(16_384, 16_384);

    #[allow(unsafe_code)]
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
}

// for Windows, check out VirtualAllocEx with MEM_RESERVE flag: https://learn.microsoft.com/en-us/windows/win32/memory/page-state
// https://stackoverflow.com/questions/15261527/how-can-i-reserve-virtual-memory-in-linux?rq=1
//xxx look into VirtualAlloc on windows and the difference between "reserve" and "commit"...

// mimalloc by default madvise's transparent hugepage support *think*
