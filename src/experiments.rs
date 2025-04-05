mod lib;
use lib::{MAX_SLABNUM_TO_PACK_INTO_CACHELINE, NUM_AREAS, LARGE_SLOTS_SLABNUM, NUM_SLOTS, OVERSIZE_SLABNUM, slabnum_to_slotsize, layout_to_slabnum, slabnum_to_numareas, NUM_SLABS, WORDSIZE, mmap};
    
// On MacOS on Apple M4, I could allocate more than 105 trillion bytes.
// On a linux machine (AMD EPYC 3151) with 32,711,276 bytes RAM, with overcommit=1, the amount I was able to mmap() varied. :-( One time I could mmap() only 95,175,252,639,744 bytes.
// According to https://learn.microsoft.com/en-us/windows/win32/memory/memory-limits-for-windows-releases a 64-bit process can access 128 TiB.
TOT_VM_ALLOCATABLE = 90_000_000_000_000;

use bytesize::ByteSize;

fn conv(size: usize) -> String {
    ByteSize::b(size as u64).to_string_as(true) // true for binary units (KiB, MiB, GiB, etc.)
}

fn convsum(size: usize) -> String {
    let logtwo = size.ilog2();
    format!("{} ({:.3}b)", conv(size), logtwo)
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

    assert!(variablesvbu == VARIABLES_SPACE);

    vbu += variablesvbu;

    // Free lists need to be 4-byte aligned.
    let unalignedbytes = vbu % WORDSIZE;
    if unalignedbytes > 0 {
	let paddingforfreelists = WORDSIZE - unalignedbytes;
//	println!("Needed {} padding for free lists.", paddingforfreelists);
	vbu += paddingforfreelists;
    }
    
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

    asserr!(freelistspace == SEPARATE_FREELISTS_SPACE);

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
		if paddingforpow2 > 0 {
		    println!("needed {} padding for area {}, slabnum {}, slotsize {}", paddingforpow2, 0, slabnum, slotsize);
		}
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
	    if paddingforcacheline > 0 {
		println!("needed {} padding for cache-line-friendliness", paddingforcacheline);
	    }
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
    for numsmallslabs in 0..REAL_NUM_SLABS { if slabnum_to_slotsize(numsmallslabs) >= WORDSIZE { break } }
    
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

const fn sum_array<const N: usize>(arr: &[u8; N]) -> u8 {
    let mut sum = 0;
    let mut i = 0;
    while i < N {
        sum += arr[i];
        i += 1;
    }
    sum
}

const ARRAY: [u8; 4] = [1, 2, 3, 4];

const fn generate_array2<const N: usize>(arr: &[u8; N]) -> [u8; N] {
    let mut result = [0; N];
    let mut i = 0;
    while i < N {
        result[i] = i as u8 * arr[i];
        i += 1;
    }
    result
}

const ARRAY2: [u8; 4] = generate_array2(&ARRAY);

const SUM: u8 = sum_array(&ARRAY2);

const OFFSET_OF_DATA_SLABS_ARE_0 = xxx

    
fn main() {
    virtual_bytes_map();
    println!("The array is: {:?}", ARRAY);
    println!("The generated array is: {:?}", ARRAY2);
    println!("The sum of the array is: {}", SUM);
}
