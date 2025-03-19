use smalloc::{sizeclass_to_slotsize, sizeclass_to_l, sizeclass_to_numslots, sizeclass_to_percpuslabs, OVERSIZE_SC};

use bytesize::ByteSize;

fn conv(size: usize) -> String {
    ByteSize::b(size as u64).to_string_as(true) // true for binary units (KiB, MiB, GiB, etc.)
}

fn convsum(size: usize) -> String {
    let logtwo = size.ilog2();
    format!("{} ({:.3}b)", conv(size), logtwo)
}

const PAGE_ALIGNMENT: usize = 4096;

use thousands::Separable;
fn virtual_bytes_map() {
    let mut vbu: usize = 0; // virtual bytes used

    println!("{:>4} {:>11} {:>8} {:>4} {:>4} {:>10} {:>4} {:>18} {:>18}", "sc", "slots", "slotsize", "eaws", "flhs", "slabsize", "slabs", "totpersc", "sum");
    println!("{:>4} {:>11} {:>8} {:>4} {:>4} {:>10} {:>4} {:>18} {:>18}", "--", "-----", "--------", "----", "----", "--------", "-----", "--------", "---");
    for sc in 0..OVERSIZE_SC {
	let numslots = sizeclass_to_numslots(sc);

	let slotsize = sizeclass_to_slotsize(sc);

	let everallocatedwordsize = sizeclass_to_l(sc) as usize;
	let freelistheadsize = sizeclass_to_l(sc) as usize;
	
	// The slab takes up `numslots * slotsize + eaws + flhs` virtual bytes
        // Pad it to align the next slab to PAGE_ALIGNMENT.
	let slabsize = ((numslots * slotsize + everallocatedwordsize + freelistheadsize - 1) | (PAGE_ALIGNMENT - 1)) + 1;

	let numslabs = sizeclass_to_percpuslabs(sc);
	
	let totpersc = slabsize * numslabs;

        vbu += totpersc;
        
	println!("{:>4} {:>11} {:>8} {:>4} {:>4} {:>10} {:>4} {:>19} {:>18}", sc, numslots.separate_with_commas(), conv(slotsize), everallocatedwordsize, freelistheadsize, conv(slabsize), numslabs, convsum(totpersc), convsum(vbu));
    }

    println!("About to try to allocate {} ({}) ({}) bytes...", vbu, vbu.separate_with_commas(), convsum(vbu));
    let mmap = mm(vbu);
    println!("{:?}", mmap);

    //XXXlet maxvbu = 2usize.pow(47);
    //XXXlet remainder = maxvbu-vbu;
    //XXXprintln!("Okay this vmmap takes up {}, out of {}, leaving {}...", convsum(vbu), convsum(maxvbu), convsum(remainder));

    // How big of huge slots could we fit in here if we have 2^24-1 huge slots?
    //XXXlet mut hugeslotsize = remainder / (2usize.pow(24)-1);
    //XXXlet mut hugeslotsize = (remainder / (2usize.pow(24)-1)) - 1;
    //XXXlet mut newalloc = hugeslotsize * (2usize.pow(24)-1);
    //XXXprintln!("We could have 2^24-1 ({}) huge slots of size {}, which should add back up to {}+{}=={}", (2u32.pow(24)-1), hugeslotsize, vbu, newalloc, vbu+newalloc);

    //XXX// How big of huge slots could we fit in here if we have 2^16-1 huge slots?
    //XXXlet hugeslotsize = remainder / (2usize.pow(16)-1);
    //XXXprintln!("We could have 2^16-1 ({}) huge slots of size {}", (2u32.pow(16)-1), hugeslotsize);

    //XXX// How big of huge slots could we fit in here if we have 2^8-1 huge slots?
    //XXXlet hugeslotsize = remainder / (2usize.pow(8)-1);
    //XXXprintln!("We could have 2^8-1 ({}) huge slots of size {}", (2u32.pow(8)-1), hugeslotsize);
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

