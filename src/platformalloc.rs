
// Abstract over system virtual memory functions

#[cfg(target_os = "linux")]
pub mod vendor {
    use rustix::mm::{mmap_anonymous, madvise, mremap, munmap, ProtFlags, MapFlags, MremapFlags, Advice};

    pub fn sys_alloc(reqsize: usize) -> *mut u8 {
	assert!(reqsize > 0);
	
	unsafe {
	    mmap_anonymous(
		ptr::null_mut(),
		reqsize,
		ProtFlags::READ | ProtFlags::WRITE,
		MapFlags::PRIVATE | MapFlags::NO_RESERVE
	    )
	};
    }

    pub fn sys_dealloc(ptr: *mut u8, size: usize) -> () {
	unsafe {
	    munmap(ptr, size).ok()
	}
    }

    pub fn sys_realloc(ptr: *mut u8, oldsize: usize, newsize: usize) {
	unsafe {
	    mremap(ptr, oldsize, newsize, MremapFlags::MAYMOVE).ok()
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
    use std::mem::transmute;
    use std::mem::size_of;
    use mach_sys::vm::{mach_vm_allocate, mach_vm_deallocate, mach_vm_remap};
    use mach_sys::vm_types::{mach_vm_address_t, mach_vm_size_t};
    use mach_sys::port::mach_port_t;
    use mach_sys::kern_return::KERN_SUCCESS;
    use mach_sys::traps::mach_task_self;
    use mach_sys::vm_statistics::VM_FLAGS_ANYWHERE;
    use mach_sys::vm_inherit::VM_INHERIT_NONE;
    use mach_sys::vm_prot::vm_prot_t;

    pub fn sys_alloc(size: usize) -> *mut u8 {
	let task: mach_port_t = unsafe { mach_task_self() };
	let mut address: mach_vm_address_t = 0;
	let size: mach_vm_size_t = size as mach_vm_size_t;

	unsafe {
	    let retval = mach_vm_allocate(task, &mut address, size, VM_FLAGS_ANYWHERE);
	    assert!(retval == KERN_SUCCESS);
	}

	address as *mut u8
    }

    pub fn sys_dealloc(ptr: *mut u8, size: usize) {
	assert!(size_of::<usize>() == size_of::<u64>());
	assert!(size_of::<*mut u8>() == size_of::<u64>());
	
	unsafe {
	    let retval = mach_vm_deallocate(mach_task_self(), transmute::<*mut u8, u64>(ptr), size as u64);
	    assert!(retval == KERN_SUCCESS);
	}
    }

    pub fn sys_realloc(ptr: *mut u8, _oldsize: usize, newsize: usize) -> *mut u8 {
	assert!(size_of::<*mut u8>() == size_of::<u64>());

	let mut newaddress: mach_vm_address_t = 0;
	let task: mach_port_t = unsafe { mach_task_self() };
	let mut cur_prot: vm_prot_t = 0;
	let mut max_prot: vm_prot_t = 0;
	unsafe {
	    let retval = mach_vm_remap(task,
		   &mut newaddress,
		   u64::try_from(newsize).unwrap(),
		   0, // mask
		   VM_FLAGS_ANYWHERE,
		   task,
		   transmute::<*mut u8, u64>(ptr),
		   0, // copy = False
		   &mut cur_prot, &mut max_prot, 
		   VM_INHERIT_NONE);
	    assert!(retval == KERN_SUCCESS);
	}

	newaddress as *mut u8
    }

    // Hm, looking at https://github.com/apple-oss-distributions/xnu/blob/main/osfmk/vm/vm_map.c and ./bsd/kern/kern_mman.c and vm_map.c ...
    // madvise -> mach_vm_behavior_set -> vm_map_behavior_set
    // * MADV_RANDOM -> VM_BEHAVIOR_RANDOM
    // * MADV_NORMAL -> VM_BEHAVIOR_DEFAULT
    // * MADV_WILLNEED -> VM_BEHAVIOR_WILLNEED
    // * MADV_DONTNEED -> VM_BEHAVIOR_DONTNEED
    // * MADV_FREE -> VM_BEHAVIOR_FREE
    // * MADV_FREE_REUSABLE -> VM_BEHAVIOR_REUSABLE
    // * MADV_FREE_REUSE -> VM_BEHAVIOR_REUSE
    // * MADV_CAN_REUSE -> VM_BEHAVIOR_CAN_REUSE
    // * MADV_PAGEOUT -> VM_BEHAVIOR_PAGEOUT

    // vm_map_behavior_set:
    // If VM_BEHAVIOR_DEFAULT or VM_BEHAVIOR_RANDOM then set a state for future behavior
    // Looks like VM_BEHAVIOR_RANDOM just turns off a couple of read-ahead and deactivate-behind features. I'm guessing VM_BEHAVIOR_DEFAULT is good enough for smalloc's purposes.

    // else (if VM_BEHAVIOR_WILLNEED, VM_BEHAVIOR_DONTNEED, VM_BEHAVIOR_FREE, VM_BEHAVIOR_REUSABLE, VM_BEHAVIOR_REUSE, VM_BEHAVIOR_CAN_REUSE, VM_BEHAVIOR_PAGEOUT, do something:

    // * VM_BEHAVIOR_DONTNEED -> vm_map_msync(VM_SYNC_DEACTIVATE)
    // * VM_BEHAVIOR_FREE -> vm_map_msync(VM_SYNC_KILLPAGES)
    // * VM_BEHAVIOR_PAGEOUT -> vm_map_pageout() -> vm_object_pageout()
    //   ^-- smalloc doesn't need to do any of these, and trigger immediate action on the part of the kernel, in order to achieve its goals.

    // * VM_BEHAVIOR_WILLNEED -> vm_map_willneed()
    //   If an anonymous mapping, -> vm_pre_fault() -> vm_fault() -> vm_fault_internal()
    //   That seems to heavyweight for smalloc's needs. smalloc won't use WILLNEED. Anyway, in the common case the memory page in question will already have been in use to hold the free list entry and so it will already be in cache by the time smalloc does anything! :-)
    
    // * VM_BEHAVIOR_REUSABLE -> vm_map_reusable_pages()
    //   This checks each vm_map_entry_t in the range by calling "vm_map_entry_is_reusable()" on it, and errors out if any one is not. So... I guess this means that the action the caller intends by "VM_BEHAVIOR_REUSABLE" is not simply the fact that it is already "vm_map_entry_is_reusable()", since the latter is required to always be true... Update: okay, vm_map_entry_is_reusable() is always gonna be true for smalloc's vm map entries, and that state isn't the same as the "reusable" flag on *objects*, see below...
    //   ... and then (for the private, anonymous memory that smalloc uses), -> vm_object_deactivate_pages(kill_page==1, reusable_page==1, reusable_no_write==0)
    //   ... It looks like if we've called vm_object_deactivate_pages() before with reusable_page==1, that it will detect this and just return. There is a comment saying calling it more than once with reusable is "illegal", but it looks idempotent to me... (line 2557 of vm_object.c)
    //   ... And, based on the comments/docs plus my incomplete reading of the code, it just marks the virtual memory pages as reusable and (line 2589) **deactivates** them. So this is perfect for memory pages that smalloc is no longer currently using. The "marking them as reusable" part -- as opposed to deactivating them without marking them as reusable -- is an optimization: if smalloc ends up using them again before the physical RAM backing them has gotten repurposed, then the kernel will let smalloc start using that physical RAM again without going to the effort of zeroing it. That effort might actually involved mapping a "zero page" to the newly re-used virtual memory page, so it could actually be less than the amount of work of writing 0's over the whole page... Or maybe it would end up being more work than simply remapping the physical frame to this page and writing 0's over the entire page? In any case, by using the mark-as-reusable-and-forget feature, we can optimize out that work in the case that smalloc ends up reusing that page before its backing gets repurposed.

    // * VM_BEHAVIOR_REUSE -> vm_map_reuse_pages() -> vm_object_reuse_pages()
    //   It looks like this just clears the flag on the pages which lets the kernel know that it can repurpose their memory-backing first (ahead of other pages that don't have this flag). They are already mapped in so it doesn't have to do anything else! :-)

    // Okay! So this is the pair of behavior flags that smalloc wants to use: call `mach_vm_behavior_set(VM_BEHAVIOR_REUSABLE)` to deactivate a virtual page while marking it to be kept around and reused if it comes to that. Then call `mach_vm_behavior_set(VM_BEHAVIOR_REUSE)` when it comes to that.

//"    There is no MADV_REUSABLE flag for madvise(). The madvise() flag that leads (through VM_BEHAVIOR_REUSABLE) to vm_map_reusable_pages() is MADV_FREE_REUSABLE, and the madvise() flag that leads (through VM_BEHAVIOR_CAN_REUSE) to vm_map_can_reuse() is MADV_CAN_REUSE.
	
    // * VM_BEHAVIOR_CAN_REUSE -> vm_map_can_reuse()
    //   This seems to have no side effects (other than incrementing `vm_page_stats_reusable.can_reuse_success`, which a quick grep suggests has no effects).

    // useful blog post explaining the mach_vm_* functions: https://yoursubtitle.blogspot.com/2009/11/section-86-mach-vm-user-space-interface.html
    // Yep, reading that makes me think default behavior will be best for smalloc's purposes.
	
}

// for Windows, check out VirtualAllocEx with MEM_RESERVE flag: https://learn.microsoft.com/en-us/windows/win32/memory/page-state
// https://stackoverflow.com/questions/15261527/how-can-i-reserve-virtual-memory-in-linux?rq=1
// Use MADV_REUSE rather than MADV_FREE, on Linux, based on Brave AI's explanations, and on Mach(iOS/MacOS), use MADV_FREE_REUSABLE and when done with a page and MADV_FREE_REUSE before re-using the page. The latter is to avoid the overhead of the kernel zero'ing the page's contents.
	//xxx look into VirtualAlloc on windows and the difference between "reserve" and "commit"...
// xxx check if we're using the rustix linux-raw or the rustix libc backend, on linux

