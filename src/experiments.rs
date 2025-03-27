mod lib;
use lib::{MAX_SLABNUM_TO_PACK_INTO_CACHELINE, NUM_AREAS, LARGE_SLOTS_SLABNUM, NUM_SLOTS, OVERSIZE_SLABNUM, slabnum_to_slotsize, layout_to_slabnum, TOT_VM_ALLOCATABLE, slabnum_to_numareas, NUM_SLABS};
    
use bytesize::ByteSize;

const WORDSIZE: usize = 4;

fn conv(size: usize) -> String {
    ByteSize::b(size as u64).to_string_as(true) // true for binary units (KiB, MiB, GiB, etc.)
}

fn convsum(size: usize) -> String {
    let logtwo = size.ilog2();
    format!("{} ({:.3}b)", conv(size), logtwo)
}

use memmapix::{MmapOptions, MmapMut, Advice};

fn mm(reqsize: usize) -> Result<MmapMut, std::io::Error> {
    let mm = MmapOptions::new().len(reqsize).map_anon();
    if mm.is_err() {
	return mm;
    }
    let x: MmapMut = mm.unwrap();
    x.advise(Advice::Random).ok();
    Ok(x)

    // XXX We'll have to use https://docs.rs/rustix/latest/rustix/mm/fn.madvise.html to madvise more flags...
	
    //XXX for Linux: MapFlags::UNINITIALIZED . doesn't really optimize much even when it works and it only works on very limited platforms (because it is potentially exposing other process's information to our process
    //XXX | MapFlags::MADV_RANDOM | MapFlags::MADV_DONTDUMP
    //XXX Look into purgable memory on Mach https://developer.apple.com/library/archive/documentation/Performance/Conceptual/ManagingMemory/Articles/CachingandPurgeableMemory.html
    //XXX Look into MADV_FREE on MacOS (and maybe iOS?) (compared to MADV_DONTNEED on Linux)
}

//XXXmod cpuid;
//xxxuse cpuid::{get_vendor_info, Vendor};

//XXXXmod cpu;
//XXXXuse cpu::mpidr;

mod cpu;
use cpu::get_thread_id;

fn _find_max_vm_space_allocatable() {
    let mut trysize: usize = 2usize.pow(63);
    let mut lastsuccess = 0;
    let mut lastfailure = trysize;
    let mut bestsuccess = 0;

    loop {
	if lastfailure - lastsuccess < 2usize.pow(20) {
	    // close enough
	    println!("Done. best success was: {}", bestsuccess);
	    break;
	}
	println!("trysize: {}", trysize);
	let m = mm(trysize);
	if m.is_ok() {
	    println!("It worked! mm: {:?}, lastsuccess: {}, trysize: {}, lastfailure: {}", {}, lastsuccess, trysize, lastfailure);
	    if trysize > bestsuccess {
		bestsuccess = trysize;
	    }
	    lastsuccess = trysize;
	    trysize = (trysize + lastfailure) / 2;
	} else {
	    println!("It failed! mm: {:?}, lastsuccess: {}, trysize: {}, lastfailure: {}", {}, lastsuccess, trysize, lastfailure);
	    lastfailure = trysize;
	    trysize = (trysize + lastsuccess) / 2;
	}
    }
}

use thousands::Separable;

fn virtual_bytes_map() -> usize {
    let mut vbu: usize = 0; // virtual bytes used

    // See the README.md to understand this layout.
    
    // First count the space needed for the variables.

    let mut totalpaddingneeded = 0;
    
    // The slabs up to and including MAX_SLABNUM_TO_PACK_INTO_CACHELINE have one slab in each area.
    let totslabs = (MAX_SLABNUM_TO_PACK_INTO_CACHELINE + 1) * NUM_AREAS + LARGE_SLOTS_SLABNUM - MAX_SLABNUM_TO_PACK_INTO_CACHELINE;
    println!("totslabs: {}", totslabs);
    let variablessize = 8;
    let variablesvbu = totslabs * NUM_SLOTS * variablessize;

    vbu += variablesvbu;

    println!("The virtual memory space for all the variables is {} ({})", variablesvbu.separate_with_commas(), convsum(variablesvbu));

    // Free lists need to be 4-byte aligned.
    let unalignedbytes = vbu % WORDSIZE;
    if unalignedbytes > 0 {
	let paddingforfreelists = WORDSIZE - unalignedbytes;
	println!("Needed {} padding for free lists.", paddingforfreelists);
	totalpaddingneeded += paddingforfreelists;
	vbu += paddingforfreelists;
    }
    
    // The free lists for slabs 0, 1, and 2 (all the slabs whose slots are < 4 bytes).
    let numsmallslabs = 3;
    let freelistslotsize = WORDSIZE;
    let freelistspace: usize = numsmallslabs * NUM_AREAS * NUM_SLOTS * freelistslotsize;

    vbu += freelistspace;
    
    println!("The virtual memory space for the free lists is {} ({})", freelistspace.separate_with_commas(), convsum(freelistspace));

    // Then the space needed for the data slabs.

    for slabnum in 0..OVERSIZE_SLABNUM {
	
	let mut paddingneeded = 0;
	// If this slab's slot size is a power of 2 then we need to
	// pad it before laying it out, to align its first byte onto
	// an address which is an integer multiple of its slot size.
	let slotsize = slabnum_to_slotsize(slabnum);
	if slotsize.is_power_of_two() {
	    let unalignedbytes = vbu % slotsize;
	    if unalignedbytes > 0 {
		let paddingforpow2 = slotsize - unalignedbytes;
		paddingneeded += paddingforpow2;
		println!("needed {} padding for area {}, slabnum {}, slotsize {}", paddingforpow2, 0, slabnum, slotsize);
		totalpaddingneeded += paddingforpow2;
		vbu += paddingforpow2;
		assert!(vbu % slotsize == 0);
	    }
	} 

	// Also, we want every slab to begin on a 64-byte boundary for cache-line friendliness.
	let unalignedbytes = vbu % 64;
	if unalignedbytes > 0 {
	    let paddingforcacheline = 64 - unalignedbytes;
	    paddingneeded += paddingforcacheline;
	    println!("needed {} padding for cache-line-friendliness", paddingforcacheline);
	    totalpaddingneeded += paddingforcacheline;
	    vbu += paddingforcacheline;
	}
	
	// Okay the total space needed for this slab is
	let spaceperslab = slotsize * NUM_SLOTS;

	// There are this many slots of this size:
	let numslabs_for_this_sizeclass = slabnum_to_numareas(slabnum);
	
	let totslabspace = spaceperslab * numslabs_for_this_sizeclass;

	vbu += totslabspace;

	println!("{:>7} {:>8} {:>4} {:>19} {:>19}", slabnum, conv(slotsize), paddingneeded, convsum(totslabspace), convsum(vbu));

    }
    
    println!("XXX The total virtual memory space for all the variables and slots is {} ({})", vbu.separate_with_commas(), convsum(vbu));
    println!("Total padding needed was {}", totalpaddingneeded);
    for numsmallslabs in 0..NUM_SLABS { if slabnum_to_slotsize(numsmallslabs) >= WORDSIZE { break } }
    
    let remaining = TOT_VM_ALLOCATABLE - vbu;
    println!("Extra vm space we can use! {} ({})", remaining, convsum(remaining));
    println!("About to try to allocate {} ({}) ({}) bytes...", vbu, vbu.separate_with_commas(), convsum(vbu));
    let mmap = mm(vbu);
    println!("{:?}", mmap);

    //// How big of large slots could we fit in here?
    //xxx    let largeslotsize = remaining / (NUM_SLOTS * slabnum_to_numareas(LARGE_SLOTS_SLABNUM));
    //xxx    let newalloc = largeslotsize * NUM_SLOTS;
    //xxx    assert!(newalloc <= remaining, "{} <= {}", newalloc, remaining);
    //xxx    println!("We could have {} * {} large slots of size {}, which should add back up to {} ({}) + {} ({}) == {} ({})", NUM_SLOTS, slabnum_to_numareas(LARGE_SLOTS_SLABNUM), largeslotsize, vbu, convsum(vbu), newalloc, convsum(newalloc), vbu+newalloc, convsum(vbu+newalloc));

    vbu
}

fn main() {
    virtual_bytes_map();
}
