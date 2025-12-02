// I read in the "The Linux Programming Interface" book that glibc's malloc's default size to fall back to system allocation (mmap) -- MMAP_THRESHOLD -- is 128 KiB. But according to https://sourceware.org/glibc/wiki/MallocInternals the threshold is dynamic unless overridden.

// The following are tools I used during development of smalloc, which should probably be rm'ed once
// smalloc is finished. :-)

// On MacOS on Apple M4, I could allocate more than 105 trillion bytes (105,072,079,929,344).
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


#[cfg(any(target_os = "linux", doc))]
pub fn sys_dealloc(p: *mut u8, size: usize) {
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

use plat::plat::sys_alloc;
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
        let res_m = sys_alloc(trysize);
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
