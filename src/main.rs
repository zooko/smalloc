//use simalloc::layout_to_sizeclass;
use simalloc::sizeclass_to_slotsize;

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

    let mut sc: u8 = 1; // sizeclass

    const NUM_HEAPS: u128 = 256;

    println!("{:<5} {:<10} {:<5} {:<10} {:<5} {:<5} {:<10} {:<16} {:<24}", "sc", "slotsize", "l", "fifoarea", "eaws", "ptrs", "slabsize", "total", "vbu");
    while vbu < 2u128.pow(47) {
        let ss: u128 = sizeclass_to_slotsize(sc) as u128;

        let l: u128 = if ss <= 2u128.pow(20) { 2 } else { if ss <= 2u128.pow(28) { 1 } else { 0 }};

        let fifoqueueareasize: u128;
        let everallocatedwordsize: u128;
        let fifoqueueptrssize: u128;
        let slabsize: u128;
	if l > 0 {
            let s = 2u128.pow(l as u32 *8);

            // The slab takes up `s * ss` virtual bytes:
            slabsize = s * ss;

            // For the "FIFO queue area", we need `s` spots, each of size `l` bytes.
            fifoqueueareasize = s * l;

            // We need this many bytes for the `everallocated` word:
            everallocatedwordsize = l + 1;

            // We need two words of size `l` for the head and tail pointer of the FIFO queue:
            fifoqueueptrssize = 2*l;
	} else {
            fifoqueueareasize = 0; // No FIFO area for 1-slot slabs
            everallocatedwordsize = 0; // No everallocated word for 1-slot slabs

	    // No FIFO ptrs for 1-slot slabs

	    // But we do need a single bit to indicate whether this slot is allocated or not. Let's just add 1 (byte) to the fifoqueueptrssize to account for that...
            fifoqueueptrssize = 1;

            // The slab takes up `ss` virtual bytes:
            slabsize = ss;
	}

        // Okay that's all the virtual space we need for this slab!
        let totalsizeperslab = fifoqueueareasize + everallocatedwordsize + fifoqueueptrssize + slabsize;

        // We have NUM_HEAPS of these (one per possible cpuid -- see the docs for details).
        
        vbu += totalsizeperslab * NUM_HEAPS;
        
	if vbu < 2u128.pow(47) {
            println!("{:<5} {:<10} {:<5} {:<10} {:<5} {:<5} {:<10} {:<16} {:<24}", sc, conv(ss), l, conv(fifoqueueareasize), everallocatedwordsize, fifoqueueptrssize, conv(slabsize), convsum(totalsizeperslab), convsum(vbu));
	}

        sc += 1;
    }
    
}

//XXXuse memmap2::{MmapMut, Mmap};

//XXXfn mmap() {
//XXX    println!("mmap'ing!");
//XXX    let mut siz: usize = 1;
//XXX
//XXX    loop { 
//XXX        println!("{}", convsum(siz as u128));
//XXX
//XXX        let mut m = MmapMut::map_anon(siz).unwrap();
//XXX
//XXX        println!("{}", m);
//XXX
//XXX        siz *= 2;
//XXX    }
//XXX
//XXX}

fn main() {
    println!("Howdy, world!");

    virtual_bytes_map();
    //XXXmmap();
}

