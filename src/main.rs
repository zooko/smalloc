use smalloc::{sizeclass_to_slotsize, NUM_SCSU8};
use rustix;

use bytesize::ByteSize;

fn conv(size: u128) -> String {
    let sizu64: u64 = u64::try_from(size).unwrap();
    let byte_size = ByteSize::b(sizu64);
    byte_size.to_string_as(true) // true for binary units (KiB, MiB, GiB, etc.)
}

fn convsum(size: u128) -> String {
    let logtwo: u32 = size.ilog2();
    format!("{} ({:.3}b)", conv(size), logtwo)
}

const NUM_SLABSETS: u128 = 256;
const PAGE_ALIGNMENT: u128 = 4096;

fn sc_to_slab_vm_space(sc: u8) -> u128 {
    let ss: u128 = sizeclass_to_slotsize(sc) as u128;

    //XXXlet l: u128 = if ss <= 2u128.pow(20) { 2 } else { if ss <= 2u128.pow(30) { 1 } else { 0 }};
    //XXXlet l: u128 = if ss <= 2u128.pow(20) { 2 } else { if ss <= 2u128.pow(28) { 1 } else { 0 }};
    //XXXlet l: u128 = if ss <= 2u128.pow(21) { 2 } else { 1 };
    //XXXlet l: u128 = 3;
    //XXXlet l: u128 = 2;
    //XXXlet l: u128 = if ss == 1 { 1 } else if ss == 2 { 2 } else { 3 };
    //let l: u128 = if sc < NUM_SCSU8-1 { 2 } else { 1 };
    let l: u128 = if sc == 0 { 1 } else if sc < NUM_SCSU8-1 { 2 } else { 1 };

    let freelistheadsize: u128;
    let everallocatedwordsize: u128;
    let slabsize: u128;
    if l > 0 {
        let s = 2u128.pow(l as u32 *8 - 1);

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
    return freelistheadsize + everallocatedwordsize + slabsize;
}

fn virtual_bytes_map() {
    let mut vbu: u128 = 0; // virtual bytes used

    println!("NUM_SLABSETS: {}", NUM_SLABSETS);
    println!("{:>5} {:>10} {:>12} {:>3} {:>4} {:>4} {:>12} {:>24} {:>24}", "sc", "slotsize", "slabsize", "l", "eaws", "flhs", "perslab", "allslabs", "vbu");
    println!("{:>5} {:>10} {:>12} {:>3} {:>4} {:>4} {:>12} {:>24} {:>24}", "--", "--------", "--------", "-", "----", "----", "-------", "--------", "---");
    for sc in 0..NUM_SCSU8 {
        let mut space_per_slab: u128 = sc_to_slab_vm_space(sc);
        
        // align the next slab to PAGE_ALIGNMENT
	space_per_slab = ((space_per_slab - 1) | (PAGE_ALIGNMENT - 1)) + 1;

        // We have NUM_SLABSETS of these
        let space_per_slabset = space_per_slab * NUM_SLABSETS;

        vbu += space_per_slabset;
        
        print!("{:>24} {:>24}", convsum(space_per_slab*NUM_SLABSETS), convsum(vbu));

        let c_void = mm(vbu as usize);

        println!("{:?}", c_void);

	unsafe { munmap(c_void, vbu as usize).expect("munmap!?") };
    }

    let maxvbu: u128 = 2u128.pow(47);
    let remainder: u128 = maxvbu-vbu;
    println!("Okay this vmmap takes up {}, out of {}, leaving {}...", convsum(vbu), convsum(maxvbu), convsum(remainder as u128));

//    let keepfree: u128 = 2u128.pow(46);
//    let alloc: u128 = remainder - keepfree;
//    let allocus: usize = usize::try_from(alloc).unwrap();

//    println!("About to try to reserve {} and allocate remainder {} == {}", convsum(keepfree), alloc.separate_with_commas(), convsum(alloc));

//    let c_void = mm(allocus);
//    println!("{:?}", c_void);
//    unsafe { munmap(c_void, allocus).expect("munmap!?") };
    
//    println!("Okay! I was able to allocate {}.", convsum(alloc));

//    let l1numslots = NUM_SLABSETS * 2u128.pow(8);
//    let l2numslots = NUM_SLABSETS * 2u128.pow(16);
//    let l3numslots = NUM_SLABSETS * 2u128.pow(24);

//    let l1slotsize: u128 = alloc / l1numslots;
//    let l2slotsize: u128 = alloc / l2numslots;
//    let l3slotsize: u128 = alloc / l3numslots;
    
//    println!("In L=1's, that would be {} slots of {} ({}), in L=2's, that would be {} slots of {} ({}), and in L=3's, that would be {} slots of {} ({}).", l1numslots.separate_with_commas(), l1slotsize.separate_with_commas(), convsum(l1slotsize), l2numslots.separate_with_commas(), l2slotsize.separate_with_commas(), convsum(l2slotsize), l3numslots.separate_with_commas(), l3slotsize.separate_with_commas(), convsum(l3slotsize));

    
}

use rustix::mm::{mmap_anonymous, munmap, MapFlags, ProtFlags};
use core::ffi::c_void;


fn mm(reqsize: usize) -> *mut c_void {
    // XXX on MacOSX (and maybe on iOS?) MAP_ANON, MAP_PRIVATE
    let addr = unsafe { mmap_anonymous(
        std::ptr::null_mut(), // Address hint (None for any address)
        reqsize, // Size of the mapping
        ProtFlags::READ | ProtFlags::WRITE, // Protection flags
        MapFlags::PRIVATE
    ).expect("Failed to create anonymous mapping") };

        //XXX for Linux: MapFlags::UNINITIALIZED . doesn't really optimize much even when it works and it only works on very limited platforms (because it is potentially exposing other process's information to our process
//XXX    println!("Anonymous mapping created at address: {:?}", addr);
    //XXX | MapFlags::MADV_RANDOM | MapFlags::MADV_DONTDUMP
    //XXX Look into purgable memory on Mach https://developer.apple.com/library/archive/documentation/Performance/Conceptual/ManagingMemory/Articles/CachingandPurgeableMemory.html
    //XXX Look into MADV_FREE on MacOS (and maybe iOS?) (compared to MADV_DONTNEED on Linux)
    return addr;
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
    
    //XXXrun_gtlp();
    virtual_bytes_map();
    //try_sbrk();
}

