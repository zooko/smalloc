use smalloc::{sizeclass_to_slotsize, sizeclass_to_l, sizeclass_to_slots, OVERSIZE_SC, HUGE_SLOTS_SC};

use bytesize::ByteSize;

fn conv(size: usize) -> String {
    ByteSize::b(size as u64).to_string_as(true) // true for binary units (KiB, MiB, GiB, etc.)
}

fn convsum(size: usize) -> String {
    let logtwo = size.ilog2();
    format!("{} ({:.3}b)", conv(size), logtwo)
}

const NUM_SLABSETS: usize = 256;
const PAGE_ALIGNMENT: usize = 4096;

fn sc_to_slab_vm_space(sc: usize) -> usize {
    let ss = sizeclass_to_slotsize(sc);
    let l = sizeclass_to_l(sc) as usize; // number of bytes in this sizeclass's indexes
    let s = sizeclass_to_slots(sc);

    // We need one words of size `l` for the head pointer to the free list.
    let freelistheadsize = l;

    // We need this many bytes for the `everallocated` word:
    let everallocatedwordsize = l;

    // The slab takes up `s * ss` virtual bytes:
    let slabsize = s * ss;

    let tot = freelistheadsize+everallocatedwordsize+slabsize;
    print!("{:>5} {:>10} {:>12} {:>3} {:>4} {:>4} {:>12} ", sc, conv(ss), conv(slabsize), l, everallocatedwordsize, freelistheadsize, conv(tot));

    // Okay that's all the virtual space we need for this slab!
    freelistheadsize + everallocatedwordsize + slabsize
}

fn virtual_bytes_map() {
    let mut vbu: usize = 0; // virtual bytes used

    println!("NUM_SLABSETS: {}", NUM_SLABSETS);
    println!("{:>5} {:>10} {:>12} {:>3} {:>4} {:>4} {:>12} {:>24} {:>24}", "sc", "slotsize", "slabsize", "l", "eaws", "flhs", "perslab", "allslabs", "vbu");
    println!("{:>5} {:>10} {:>12} {:>3} {:>4} {:>4} {:>12} {:>24} {:>24}", "--", "--------", "--------", "-", "----", "----", "-------", "--------", "---");
    for sc in 0..OVERSIZE_SC {
        let mut space_per_slab = sc_to_slab_vm_space(sc);
        
        // align the next slab to PAGE_ALIGNMENT
	space_per_slab = ((space_per_slab - 1) | (PAGE_ALIGNMENT - 1)) + 1;

        // We have NUM_SLABSETS of these
        let space_per_slabset = space_per_slab * NUM_SLABSETS;

        vbu += space_per_slabset;
        
        println!("{:>24} {:>24}", convsum(space_per_slab*NUM_SLABSETS), convsum(vbu));

    }

    //XXXlet maxvbu = 2usize.pow(47);
    //XXXlet remainder = maxvbu-vbu;
    //XXXprintln!("Okay this vmmap takes up {}, out of {}, leaving {}...", convsum(vbu), convsum(maxvbu), convsum(remainder));
    let mmap = mm(vbu);

    println!("{:?}", mmap);

    //XXX // How big of huge slots could we fit in here if we have 2^16-1 huge slots?
    //XXX let hugeslotsize = remainder / (NUM_SLABSETS * (2usize.pow(16)-1));
    //XXX println!("We could have {} * {} huge slots of size {}", NUM_SLABSETS, (2u32.pow(16)-1), hugeslotsize);
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

