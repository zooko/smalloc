// I read in the "The Linux Programming Interface" book that glibc's malloc's default size to fall back to system allocation (mmap) -- MMAP_THRESHOLD -- is 128 KiB. But according to https://sourceware.org/glibc/wiki/MallocInternals the threshold is dynamic unless overridden.

// On MacOS on Apple M4, I could allocate more than 105 trillion bytes (105,072,079,929,344).
// 2025-11-29: On MacOS on Apple M4: 140,256,418,463,744 bytes.
//
// On a linux machine (AMD EPYC 3151) with 32,711,276 bytes RAM, with overcommit=1, the amount I was able to mmap() varied. :-( One time I could mmap() only 95,175,252,639,744 bytes.
// On a linux machine (Intel(R) Xeon(R) CPU E5-2698 v4 @ 2.20GHz) with 4,608,000,000 bytes RAM with overcommit = 0, the amount I was able to mmap() varied. :-( One time I could mmap() only 93,971,598,389,248 Bytes.
//
// On a Windows 11 machine in Ubuntu in Windows Subsystem for Linux 2, the amount I was able to mmap() varied. One time I could mmap() only 93,979,814,301,696
// According to https://learn.microsoft.com/en-us/windows/win32/memory/memory-limits-for-windows-releases a 64-bit process can access 128 TiB.
//
// The current settings of smalloc (v4.0.0) require 59,785,944,760,326 bytes of virtual address space.
//
// Now working on smalloc v5.0.0 which requires only 29,824,252,903,423 bytes of virtual address space.
//
// 2025-11-29: The current smalloc (v6.0.4) requires 17,313,013,178,111 bytes.
//
// 2025-11-29: The current smalloc (v6.0.5) requires 35,175,782,162,431 bytes.
//
// 2025-11-30: The current smalloc (v6.1.0) requires 70,360,154,259,455 bytes.
//
// 2025-12-11: The current smalloc (v7.1.0) requires 70,360,449,210,367 bytes.
// 
// 2025-12-16: The current smalloc (v7.2.0) requires 70,366,596,694,014 bytes.



#[cfg(any(target_os = "linux", doc))]
pub fn sys_dealloc(p: *mut u8, size: usize) {
    use rustix::mm::munmap;
    use core::ffi::c_void;

    unsafe {
        munmap(p as *mut c_void, size).ok();
    }
}

#[cfg(any(target_vendor = "apple", doc))]
pub fn sys_dealloc(p: *mut u8, size: usize) {
    use mach_sys::kern_return::KERN_SUCCESS;
    use mach_sys::vm::mach_vm_deallocate;
    use mach_sys::traps::mach_task_self;

    debug_assert!(size_of::<usize>() == size_of::<u64>());
    debug_assert!(size_of::<*mut u8>() == size_of::<u64>());

    unsafe {
        let retval = mach_vm_deallocate(mach_task_self(), p as u64, size as u64);
        debug_assert!(retval == KERN_SUCCESS);
    }
}

use thousands::Separable;
fn dev_find_max_vm_space_allocatable() {
    let mut trysize: usize = 2usize.pow(62);
    let mut lastsuccess = 0;
    let mut lastfailure = trysize;
    let mut bestsuccess = 0;

    loop {
        if lastfailure - lastsuccess <= 1 {
            println!("Done. best success was: {}", bestsuccess.separate_with_commas());
            break;
        }
        //println!("trysize: {}", trysize.separate_with_commas());
        let res_m = p::sys_alloc(trysize);
        match res_m {
            Ok(m) => {
                //println!("It worked! m: {:?}, lastsuccess: {}, trysize: {}, lastfailure: {}", m, lastsuccess, trysize, lastfailure);
                if trysize > bestsuccess {
                    bestsuccess = trysize;
                }
                lastsuccess = trysize;
                sys_dealloc(m, trysize);
                trysize = (trysize + lastfailure) / 2;
            }
            Err(_) => {
                //println!("It failed! e: {:?}, lastsuccess: {}, trysize: {}, lastfailure: {}", e, lastsuccess, trysize, lastfailure);
                lastfailure = trysize;
                trysize = (trysize + lastsuccess) / 2;
            }
        }
    }
}

fn main() {
    dev_find_max_vm_space_allocatable();
}

#[derive(Debug)]
pub struct AllocFailed;

impl std::error::Error for AllocFailed {}

use std::fmt;
impl fmt::Display for AllocFailed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Alloc failed")
    }
}

#[cfg(any(target_os = "linux", doc))]
pub mod p {
    use super::AllocFailed;
    use rustix::mm::{MapFlags, ProtFlags, mmap_anonymous};
    use std::ptr;

    #[allow(unsafe_code)]
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
}

#[cfg(any(target_vendor = "apple", doc))]
pub mod p {
    use super::AllocFailed;
    use mach_sys::kern_return::KERN_SUCCESS;
    use mach_sys::port::mach_port_t;
    use mach_sys::traps::mach_task_self;
    use mach_sys::vm::mach_vm_allocate;
    use mach_sys::vm_statistics::VM_FLAGS_ANYWHERE;
    use mach_sys::vm_types::{mach_vm_address_t, mach_vm_size_t};

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
