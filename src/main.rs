use smalloc::{sizeclass_to_slotsize, OVERSIZE_SC};

use bytesize::ByteSize;

fn conv(size: usize) -> String {
    let byte_size = ByteSize::b(size as u64);
    byte_size.to_string_as(true) // true for binary units (KiB, MiB, GiB, etc.)
}

fn convsum(size: usize) -> String {
    let logtwo = size.ilog2();
    format!("{} ({:.3}b)", conv(size), logtwo)
}

const NUM_SLABSETS: usize = 256;
const PAGE_ALIGNMENT: usize = 4096;

use smalloc::sizeclass_to_l;

fn sc_to_slab_vm_space(sc: usize) -> usize {
    let ss = sizeclass_to_slotsize(sc);

    //XXXlet l: usize = if ss <= 2usize.pow(20) { 2 } else { if ss <= 2usize.pow(30) { 1 } else { 0 }};
    //XXXlet l: usize = if ss <= 2usize.pow(20) { 2 } else { if ss <= 2usize.pow(28) { 1 } else { 0 }};
    //XXXlet l: usize = if ss <= 2usize.pow(21) { 2 } else { 1 };
    //XXXlet l: usize = 3;
    //XXXlet l: usize = 2;
    //XXXlet l: usize = if ss == 1 { 1 } else if ss == 2 { 2 } else { 3 };
    //XXXlet l: usize = if sc < NUM_SCS-1 { 2 } else { 1 };
    //XXXlet l: usize = if sc == 0 { 1 } else if sc < NUM_SCS-1 { 2 } else { 1 };
    let l = sizeclass_to_l(sc); // number of bytes in this sizeclass's indexes

    let freelistheadsize;
    let everallocatedwordsize;
    let slabsize;
    if l > 0 {
        let s = 2usize.pow((l as u32)*8 - 1);

        // The slab takes up `s * ss` virtual bytes:
        slabsize = s * ss;

        // We need one words of size `l` for the head pointer to the free list.
        freelistheadsize = l;

        // We need this many bytes for the `everallocated` word:
        everallocatedwordsize = l;
    } else {
        // No free list for 1-slot slabs
        // But we do need a single bit to indicate whether this slot is allocated or not. Let's use the free list head for that.
        freelistheadsize = 1;
        everallocatedwordsize = 0; // No everallocated word for 1-slot slabs

        // The slab takes up `ss` virtual bytes:
        slabsize = ss;
    }

    print!("{:>5} {:>10} {:>12} {:>3} {:>4} {:>4} {:>12} ", sc, conv(ss), conv(slabsize), l, everallocatedwordsize, freelistheadsize, conv(freelistheadsize+everallocatedwordsize+slabsize));

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
        
        print!("{:>24} {:>24}", convsum(space_per_slab*NUM_SLABSETS), convsum(vbu));

        let c_void = mm(vbu);

        println!("{:?}", c_void);
    }

    let maxvbu = 2usize.pow(47);
    let remainder = maxvbu-vbu;
    println!("Okay this vmmap takes up {}, out of {}, leaving {}...", convsum(vbu), convsum(maxvbu), convsum(remainder));
}

use memmapix::{MmapOptions, MmapMut};


fn mm(reqsize: usize) -> MmapMut {
    MmapOptions::new().len(reqsize).map_anon().unwrap()

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

