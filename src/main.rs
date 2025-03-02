use smalloc::sizeclass_to_slotsize;
use rustix;

extern crate bytesize;

use bytesize::ByteSize;

fn conv(size: u128) -> String {
    let byte_size = ByteSize::b(size as u64);
    byte_size.to_string_as(true) // true for binary units (KiB, MiB, GiB, etc.)
}

fn ceil_log2(n: u128) -> u32 {
    (n - 1).next_power_of_two().trailing_zeros()
}

fn convsum(size: u128) -> String {
    format!("{} ({:.3}b)", conv(size), (size as f64).log2())
}

const NUM_SLABSETS: u128 = 256;
const PAGE_ALIGNMENT: u128 = 4096;

fn sc_to_slab_vm_space(sc: u8) -> u128 {
    let ss: u128 = sizeclass_to_slotsize(sc) as u128;

    //XXXlet l: u128 = if ss <= 2u128.pow(20) { 2 } else { if ss <= 2u128.pow(30) { 1 } else { 0 }};
    //XXXlet l: u128 = if ss <= 2u128.pow(20) { 2 } else { if ss <= 2u128.pow(28) { 1 } else { 0 }};
    //XXXlet l: u128 = if ss <= 2u128.pow(21) { 2 } else { 1 };
    let l: u128 = 2;

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

    print!("{:<5} {:<10} {:<10} {:<5} {:<5} {:<5} {:<10} ", sc, conv(ss), conv(slabsize), l, everallocatedwordsize, freelistheadsize, conv(freelistheadsize+everallocatedwordsize+slabsize));

    // Okay that's all the virtual space we need for this slab!
    return freelistheadsize + everallocatedwordsize + slabsize;
}

fn virtual_bytes_map() {
    let mut vbu: u128 = 0; // virtual bytes used

    let mut sc: u8 = 0; // sizeclass / slab number

    println!("NUM_SLABSETS: {}, Using l = 2", NUM_SLABSETS);
    println!("{:<5} {:<10} {:<10} {:<5} {:<5} {:5} {:<10} {:<20} {:<20}", "sc", "slotsize", "slabsize", "l", "eaws", "flhs", "perslab", "allslabs", "vbu");
    println!("{:<5} {:<10} {:<10} {:<5} {:<5} {:5} {:<10} {:<20} {:<20}", "--", "--------", "--------", "-", "----", "----", "-------", "--------", "---");
    while vbu < 2u128.pow(47) {
        let space_per_slab: u128 = sc_to_slab_vm_space(sc);
        
        // We have NUM_SLABSETS of these (one per possible cpuid -- see the docs for details).
        let mut space_per_slabset = space_per_slab * NUM_SLABSETS;

        // align each slabset to PAGE_ALIGNMENT
        if space_per_slabset % PAGE_ALIGNMENT != 0 {
            space_per_slabset += space_per_slabset % PAGE_ALIGNMENT;
        }
        
        vbu += space_per_slabset;
        
        if vbu <= 2u128.pow(47) {
            println!("{:<20} {:<20}", convsum(space_per_slab*NUM_SLABSETS), convsum(vbu));
        } else {
            println!("{:<20} {:<20} XXXX This exceeds 47 bits of address space", convsum(space_per_slab*NUM_SLABSETS), convsum(vbu));
        }

        sc += 1;
    }
    
}

use rustix::mm::{mmap_anonymous, MapFlags, ProtFlags};
use core::ffi::c_void;


fn mm(reqsize: usize) -> *mut c_void {
    let addr = unsafe { mmap_anonymous(
        std::ptr::null_mut(), // Address hint (None for any address)
        reqsize, // Size of the mapping
        ProtFlags::READ | ProtFlags::WRITE, // Protection flags
        MapFlags::PRIVATE | MapFlags::UNINITIALIZED
    ).expect("Failed to create anonymous mapping") };

    println!("Anonymous mapping created at address: {:?}", addr);
    //XXX | MapFlags::MADV_RANDOM | MapFlags::MADV_DONTDUMP
    return addr;
}

fn mmap() {
    println!("mmap'ing!");
    let mut siz: usize = 1;

    loop { 
        println!("about to try to mmap {}", convsum(siz as u128));

        let c_void = mm(siz);

        println!("{:?}", c_void);

        siz *= 2;
    }

}


mod cpuid;
use cpuid::{get_vendor_info, Vendor};

fn main() {
    println!("Howdy, world!");

    let v: Vendor = get_vendor_info();
    println!("get_vendor_info(): {:?}", v);
    if let Vendor::Intel = v {
        println!("it is Intel");
    }
    
    //XXXrun_gtlp();
    virtual_bytes_map();
    //mmap();
    //try_sbrk();
}

