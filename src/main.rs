//use simalloc::layout_to_sizeclass;
use simalloc::sizeclass_to_slotsize;
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
    format!("{} ({}b {}b)", conv(size), ceil_log2(size), (size as f64).log2())
}

fn virtual_bytes_map() {
    let mut vbu: u128 = 0; // virtual bytes used

    let mut sc: u8 = 0; // sizeclass / slab number

    const NUM_HEAPS: u128 = 256;

    println!("NUM_HEAPS: {}, Using l = 2 when <= 1 MiB then l = 1 when <= 1 GiB then 0.", NUM_HEAPS);
    println!("{:<5} {:<10} {:<5} {:<10} {:<10} {:<5} {:<5} {:<5} {:<10} {:<16} {:<24}", "sc", "slotsize", "l", "ffifoarea", "nfifoarea", "eaws", "fptrs", "nptrs", "slabsize", "total", "vbu");
    while vbu < 2u128.pow(47) {
        let ss: u128 = sizeclass_to_slotsize(sc) as u128;

        //XXXlet l: u128 = if ss <= 2u128.pow(20) { 2 } else { if ss <= 2u128.pow(28) { 1 } else { 0 }};
        //XXXlet l: u128 = if ss <= 2u128.pow(21) { 2 } else { 1 };
        let l: u128 = if ss <= 2u128.pow(20) { 2 } else { if ss <= 2u128.pow(30) { 1 } else { 0 }};

        let fastfifoqueueareasize: u128;
        let notasfastfifoqueueareasize: u128;
        let everallocatedwordsize: u128;
        let fastfifoqueueptrssize: u128;
        let notasfastfifoqueueptrssize: u128;
        let slabsize: u128;
	if l > 0 {
            let s = 2u128.pow(l as u32 *8);

            // The slab takes up `s * ss` virtual bytes:
            slabsize = s * ss;

            // For the "fast FIFO queue area", we need `s` spots, each of size `l` bytes.
            fastfifoqueueareasize = s * l;

            // We need two words of size `l` for the head and tail pointer of the fast FIFO queue:
            fastfifoqueueptrssize = 2*l;

            // For the "not-as-fast FIFO queue area", we need `s` spots, each of size `l+1` bytes.
            notasfastfifoqueueareasize = s * (l+1);

            // We need two words of size `l` for the head and tail pointer of the not-as-fast FIFO queue:
            notasfastfifoqueueptrssize = 2*l;

            // We need this many bytes for the `everallocated` word:
            everallocatedwordsize = l + 1;
	} else {
            fastfifoqueueareasize = 0; // No FIFO area for 1-slot slabs
            notasfastfifoqueueareasize = 0; // No FIFO area for 1-slot slabs
            everallocatedwordsize = 0; // No everallocated word for 1-slot slabs

	    // No FIFO ptrs for 1-slot slabs

	    // But we do need a single bit to indicate whether this slot is allocated or not. Let's just add 1 (byte) to the fastfifoqueueptrssize to account for that...
            fastfifoqueueptrssize = 1;
            notasfastfifoqueueptrssize = 0;

            // The slab takes up `ss` virtual bytes:
            slabsize = ss;
	}

        // Okay that's all the virtual space we need for this slab!
        let totalsizeperslab = fastfifoqueueareasize + notasfastfifoqueueptrssize + fastfifoqueueptrssize + notasfastfifoqueueptrssize + everallocatedwordsize + slabsize;

        // We have NUM_HEAPS of these (one per possible cpuid -- see the docs for details).
        
        vbu += totalsizeperslab * NUM_HEAPS;
        
	if vbu < 2u128.pow(47) {
            println!("{:<5} {:<10} {:<5} {:<10} {:<10} {:<5} {:<5} {:<5} {:<10} {:<16} {:<24}", sc, conv(ss), l, conv(fastfifoqueueareasize), conv(notasfastfifoqueueareasize), everallocatedwordsize, fastfifoqueueptrssize, notasfastfifoqueueptrssize, conv(slabsize), convsum(totalsizeperslab), convsum(vbu));
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
    //virtual_bytes_map();
    //mmap();
    //try_sbrk();
}

