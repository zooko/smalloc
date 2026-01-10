// On MacOS on Apple M4, I could allocate at least 105,072,079,929,344.
// On linux (e.g. Intel(R) Xeon(R) CPU E5-2698 v4 @ 2.20GHz) I could allocate at least 93,971,598,389,248 bytes.
// On a Windows 11 machine, I was able to reserve at least 138,072,720,605,184 bytes.
//
// The current smalloc (v7.5.5) requires 70,366,596,694,014 bytes of virtual address space.

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
        let res_m = p::sys_alloc(trysize);
        match res_m {
            Ok(m) => {
                if trysize > bestsuccess {
                    bestsuccess = trysize;
                }
                lastsuccess = trysize;
                p::sys_dealloc(m, trysize);
                trysize = (trysize + lastfailure) / 2;
            }
            Err(_) => {
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

#[cfg(any(target_os = "windows", doc))]
pub mod p {
    use super::AllocFailed;
    use windows_sys::Win32::System::Memory::{VirtualAlloc2, VirtualFree, MEM_RESERVE, PAGE_NOACCESS, MEM_RELEASE};
    use core::ffi::c_void;
    use std::ptr::null_mut;

    #[allow(unsafe_code)]
    pub fn sys_alloc(reqsize: usize) -> Result<*mut u8, AllocFailed> {
        eprintln!("About to alloc {reqsize}");
        let p = unsafe {
            VirtualAlloc2(null_mut(), std::ptr::null(), reqsize, MEM_RESERVE, PAGE_NOACCESS, null_mut(), 0)
        };

        if !p.is_null() {
            eprintln!("Succeeded to alloc {reqsize}");
            Ok(p as *mut u8)
        } else {
            eprintln!("Failed to alloc {reqsize}");
            Err(AllocFailed)
        }
    }

    pub fn sys_dealloc(p: *mut u8, _size: usize) {
        unsafe { VirtualFree(p as *mut c_void, 0, MEM_RELEASE) };
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

    pub fn sys_dealloc(p: *mut u8, size: usize) {
        use rustix::mm::munmap;
        use core::ffi::c_void;

        unsafe {
            munmap(p as *mut c_void, size).ok();
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
}
