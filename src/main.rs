use smalloc::{slabnum_to_slotsize, slabnum_to_numareas, slabnum_to_l, slabnum_to_numslots, OVERSIZE_SLABNUM, NUM_AREAS, MAX_SLABNUM_TO_PACK_INTO_CACHELINE, NUM_SLABS};

use bytesize::ByteSize;

fn conv(size: usize) -> String {
    ByteSize::b(size as u64).to_string_as(true) // true for binary units (KiB, MiB, GiB, etc.)
}

fn convsum(size: usize) -> String {
    let logtwo = size.ilog2();
    format!("{} ({:.3}b)", conv(size), logtwo)
}

use thousands::Separable;
fn virtual_bytes_map() {
    let mut vbu: usize = 0; // virtual bytes used

    // See the README.md to understand this layout.
    
    // First count up the space needed for the variables.
    for slabnum in 0..OVERSIZE_SLABNUM {
	let wordsize = slabnum_to_l(slabnum) as usize;
	let numslabs = slabnum_to_numareas(slabnum);

	vbu += (wordsize*2) * numslabs;
    }

    println!("The total virtual memory space for all the variables is {} ({})", vbu.separate_with_commas(), convsum(vbu));

    let mut totalpaddingneeded = 0;
    
    // Then the space needed for the data slabs.

    // First layout the areas...
    // Do area 0 separately since it is a different shape than all the other areas:
    for slabnum in 0..OVERSIZE_SLABNUM {
	    
	// If this slab's slot size is a power of 2 then we need to
	// pad it before laying it out, to align its first byte onto
	// an address which is an integer multiple of its slot size.
	let slotsize = slabnum_to_slotsize(slabnum);
	if slotsize.is_power_of_two() {
	    let unalignedbytes = vbu % slotsize;
	    if unalignedbytes > 0 {
		let paddingneeded = slotsize - unalignedbytes;
		totalpaddingneeded += paddingneeded;
		//println!("needed {} padding for area {}, slabnum {}, slotsize {}", paddingneeded, 0, slabnum, slotsize);
		vbu += paddingneeded;
		assert!(vbu % slotsize == 0);
	    }
	}
	// Okay the total space needed for this slab is
	let totspaceperslab = slotsize * slabnum_to_numslots(slabnum);

	vbu += totspaceperslab;
    }
    
    // For areas 1 and up, we only have space for the first
    // MAX_SLABNUM_TO_PACK_INTO_CACHELINE slabs:
    for _area in 1..NUM_AREAS {
	// For each area, the slabs...
	for slabnum in 0..MAX_SLABNUM_TO_PACK_INTO_CACHELINE+1 {
	    // If this slab's slot size is a power of 2 then we need
	    // to pad it before laying it out, to align its first byte
	    // onto an address which is an integer multiple of its
	    // slot size.
	    let slotsize = slabnum_to_slotsize(slabnum);
	    if slotsize.is_power_of_two() {
		let unalignedbytes = vbu % slotsize;
		if unalignedbytes > 0 {
		    let paddingneeded = slotsize - unalignedbytes;
		    totalpaddingneeded += paddingneeded;
		    //println!("needed {} padding for area {}, slabnum {}, slotsize {}", paddingneeded, area, slabnum, slotsize);
		    vbu += paddingneeded;
		    assert!(vbu % slotsize == 0);
		}
	    }

	    // Okay the total space needed for this slab is
	    assert!(slabnum < NUM_SLABS);
	    assert!(slabnum < OVERSIZE_SLABNUM, "{}", slabnum);
	    let totspaceperslab = slotsize * slabnum_to_numslots(slabnum);
	    
	    vbu += totspaceperslab;
	}
    }
    
    println!("The total virtual memory space for all the variables and slots is {} ({})", vbu.separate_with_commas(), convsum(vbu));
    println!("Total padding needed was {}", totalpaddingneeded);

//XXX    println!("{:>4} {:>11} {:>8} {:>4} {:>4} {:>10} {:>4} {:>18} {:>18}", "slabnum", "slots", "slotsize", "eaws", "flhs", "slabsize", "slabs", "totpersc", "sum");
//XXX    println!("{:>4} {:>11} {:>8} {:>4} {:>4} {:>10} {:>4} {:>18} {:>18}", "-------", "-----", "--------", "----", "----", "--------", "-----", "--------", "---");
//XXX    for slabnum in 0..OVERSIZE_SLABNUM {
//XXX	let numslots = slabnum_to_numslots(slabnum);
//XXX
//XXX	let slotsize = slabnum_to_slotsize(slabnum);
//XXX
//XXX	let everallocatedwordsize = slabnum_to_l(slabnum) as usize;
//XXX	let freelistheadsize = slabnum_to_l(slabnum) as usize;
//XXX	
//XXX	// The slab takes up `numslots * slotsize + eaws + flhs` virtual bytes
    //XXX        // Pad it to align the next slab to PAGE_ALIGNMENT.
//XXX	let slabsize = ((numslots * slotsize + everallocatedwordsize + freelistheadsize - 1) | (PAGE_ALIGNMENT - 1)) + 1;
//XXX
//XXX	let numslabs = slabnum_to_percpuslabs(sc); xxx
//XXX	
//XXX	let totpersc = slabsize * numslabs;
//XXX
//XXX        vbu += totpersc;
//XXX        
//XXX	println!("{:>4} {:>11} {:>8} {:>4} {:>4} {:>10} {:>4} {:>19} {:>18}", sc, numslots.separate_with_commas(), conv(slotsize), everallocatedwordsize, freelistheadsize, conv(slabsize), numslabs, convsum(totpersc), convsum(vbu));
//XXX    }
//XXX
    println!("About to try to allocate {} ({}) ({}) bytes...", vbu, vbu.separate_with_commas(), convsum(vbu));
    let mmap = mm(vbu);
    println!("{:?}", mmap);

    //XXXlet maxvbu = 2usize.pow(47);
    //XXXlet remainder = maxvbu-vbu;
    //XXXprintln!("Okay this vmmap takes up {}, out of {}, leaving {}...", convsum(vbu), convsum(maxvbu), convsum(remainder));

    // How big of large slots could we fit in here if we have 2^24-1 large slots?
    //XXXlet mut largeslotsize = remainder / (2usize.pow(24)-1);
    //XXXlet mut largeslotsize = (remainder / (2usize.pow(24)-1)) - 1;
    //XXXlet mut newalloc = largeslotsize * (2usize.pow(24)-1);
    //XXXprintln!("We could have 2^24-1 ({}) large slots of size {}, which should add back up to {}+{}=={}", (2u32.pow(24)-1), largeslotsize, vbu, newalloc, vbu+newalloc);

    //XXX// How big of large slots could we fit in here if we have 2^16-1 large slots?
    //XXXlet largeslotsize = remainder / (2usize.pow(16)-1);
    //XXXprintln!("We could have 2^16-1 ({}) large slots of size {}", (2u32.pow(16)-1), largeslotsize);

    //XXX// How big of large slots could we fit in here if we have 2^8-1 large slots?
    //XXXlet largeslotsize = remainder / (2usize.pow(8)-1);
    //XXXprintln!("We could have 2^8-1 ({}) large slots of size {}", (2u32.pow(8)-1), largeslotsize);
}

use memmapix::{MmapOptions, MmapMut, Advice};

fn mm(reqsize: usize) -> MmapMut {
    let mm = MmapOptions::new().len(reqsize).map_anon().unwrap();
    mm.advise(Advice::Random).ok();
    mm

    // XXX We'll have to use https://docs.rs/rustix/latest/rustix/mm/fn.madvise.html to madvise more flags...
	
    //XXX for Linux: MapFlags::UNINITIALIZED . doesn't really optimize much even when it works and it only works on very limited platforms (because it is potentially exposing other process's information to our process
    //XXX | MapFlags::MADV_RANDOM | MapFlags::MADV_DONTDUMP
    //XXX Look into purgable memory on Mach https://developer.apple.com/library/archive/documentation/Performance/Conceptual/ManagingMemory/Articles/CachingandPurgeableMemory.html
    //XXX Look into MADV_FREE on MacOS (and maybe iOS?) (compared to MADV_DONTNEED on Linux)
}

//XXXmod cpuid;
//xxxuse cpuid::{get_vendor_info, Vendor};

fn main() {
    println!("Howdy, world!");

//xxx    let v: Vendor = get_vendor_info();
//xxx    println!("get_vendor_info(): {:?}", v);
//xxx    if let Vendor::Intel = v {
//xxx        println!("it is Intel");
//xxx    }
    
    virtual_bytes_map();
}

