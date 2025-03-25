use smalloc::{slabnum_to_slotsize, OVERSIZE_SLABNUM, NUM_AREAS, MAX_SLABNUM_TO_PACK_INTO_CACHELINE, NUM_SLABS, LARGE_SLOTS_SLABNUM, NUM_SLOTS, TOT_VM_ALLOCATABLE};

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
    
    // First count the space needed for the variables.

    // The slabs up to and including MAX_SLABNUM_TO_PACK_INTO_CACHELINE have one slab in each area, the slabs above that have one slab total.
    let totslabs = (MAX_SLABNUM_TO_PACK_INTO_CACHELINE + 1) * NUM_AREAS + LARGE_SLOTS_SLABNUM - MAX_SLABNUM_TO_PACK_INTO_CACHELINE;
    println!("totslabs: {}", totslabs);
    let variablessize = 8;
    let variablesvbu = totslabs * NUM_SLOTS * variablessize;

    println!("The virtual memory space for all the variables is {} ({})", variablesvbu.separate_with_commas(), convsum(variablesvbu));

    vbu += variablesvbu;

    // The free lists for slabs 0 and 1
    let freelistslotsize = 4;
    let freelistspace = 2 * NUM_AREAS * NUM_SLOTS * freelistslotsize;

    println!("The virtual memory space for the free lists is {} ({})", freelistspace.separate_with_commas(), convsum(freelistspace));

    vbu += freelistspace;

    let mut totalpaddingneeded = 0;
    let mut paddingneeded;
    
    // Then the space needed for the data slabs.

    // First layout the areas...
    println!("{:>7} {:>8} {:>4} {:>19} {:>19}", "slabnum", "slotsize", "pad", "sum", "tot");
    println!("{:>7} {:>8} {:>4} {:>19} {:>19}", "-------", "--------", "---", "---", "---");
    for slabnum in 0..OVERSIZE_SLABNUM {
	
	// If this slab's slot size is a power of 2 then we need to
	// pad it before laying it out, to align its first byte onto
	// an address which is an integer multiple of its slot size.
	let slotsize = slabnum_to_slotsize(slabnum);
	paddingneeded = 0;
	if slotsize.is_power_of_two() {
	    let unalignedbytes = vbu % slotsize;
	    if unalignedbytes > 0 {
		paddingneeded = slotsize - unalignedbytes;
		totalpaddingneeded += paddingneeded;
		println!("needed {} padding for area {}, slabnum {}, slotsize {}", paddingneeded, 0, slabnum, slotsize);
		vbu += paddingneeded;
		assert!(vbu % slotsize == 0);
	    }
	} 
	// Okay the total space needed for this slab is
	let spaceperslab = slotsize * 2usize.pow(24)-1;

	// There are this many slots of this size:
	let numslabs= if slabnum <= MAX_SLABNUM_TO_PACK_INTO_CACHELINE {
	    NUM_AREAS
	} else {
	    1
	};

	
	let totslabspace = spaceperslab * numslabs;

	vbu += totslabspace;

	println!("{:>7} {:>8} {:>4} {:>19} {:>19}", slabnum, conv(slotsize), paddingneeded, convsum(totslabspace), convsum(vbu));
    }
    
    println!("XXX The total virtual memory space for all the variables and slots is {} ({})", vbu.separate_with_commas(), convsum(vbu));
    println!("Total padding needed was {}", totalpaddingneeded);

    let remaining = TOT_VM_ALLOCATABLE - vbu;
    println!("Extra vm space we can use! {}", remaining);
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

fn main() {
    println!("Howdy, world!");
    get_thread_id();
    //find_max_vm_space_allocatable();

    //xxx    let v: Vendor = get_vendor_info();
    //xxx    println!("get_vendor_info(): {:?}", v);
    //xxx    if let Vendor::Intel = v {
    //xxx        println!("it is Intel");
    //xxx    }
    
    virtual_bytes_map();
}

