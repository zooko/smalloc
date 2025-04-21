use smalloc::{MAX_ALIGNMENT};

use smalloc::platformalloc::{sys_alloc, sys_dealloc};

// On MacOS on Apple M4, I could allocate more than 105 trillion bytes (105,072,079,929,344).
// On a linux machine (AMD EPYC 3151) with 32,711,276 bytes RAM, with overcommit=1, the amount I was able to mmap() varied. :-( One time I could mmap() only 95,175,252,639,744 bytes.
// On a Windows 11 machine in Ubuntu in Windows Subsystem for Linux 2, the amount I was able to mmap() varied. One time I could mmap() only 93,979,814,301,696
// According to https://learn.microsoft.com/en-us/windows/win32/memory/memory-limits-for-windows-releases a 64-bit process can access 128 TiB.
const _TOT_VM_ALLOCATABLE: usize = 90_000_000_000_000;

//use thousands::Separable;
use core::alloc::{Layout};
fn find_max_vm_space_allocatable() {
    let mut trysize: usize = 2usize.pow(62);
    let mut lastsuccess = 0;
    let mut lastfailure = trysize;
    let mut bestsuccess = 0;

    loop {
	if lastfailure - lastsuccess <= 1 {
	    println!("Done. best success was: {}", bestsuccess);
	    break;
	}
	//println!("trysize: {}", trysize.separate_with_commas());
	let res_layout = Layout::from_size_align(trysize, MAX_ALIGNMENT);
        match res_layout {
            Ok(layout) => {
                //eprintln!("l: {:?}", layout);
	        let res_m = sys_alloc(layout);
                match res_m {
                    Ok(m) => {
	                //println!("It worked! m: {:?}, lastsuccess: {}, trysize: {}, lastfailure: {}", m, lastsuccess, trysize, lastfailure);
	                if trysize > bestsuccess {
		            bestsuccess = trysize;
	                }
	                lastsuccess = trysize;
	                trysize = (trysize + lastfailure) / 2;
                        sys_dealloc(m, res_layout.unwrap());
                    }
                    Err(_) => {
	                //println!("It failed! e: {:?}, lastsuccess: {}, trysize: {}, lastfailure: {}", e, lastsuccess, trysize, lastfailure);
	                lastfailure = trysize;
	                trysize = (trysize + lastsuccess) / 2;
                    }
                }
            }
            Err(error) => {
                eprintln!("Err: {:?}", error);
            }
        }
    }
}

fn main() {
    find_max_vm_space_allocatable();
}
