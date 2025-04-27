use smalloc::{
    HUGE_SLABNUM, LARGE_SLAB_REGION_SPACE, MAX_ALIGNMENT, NUM_LARGE_SLABS, NUM_SLOTS_HUGE,
    NUM_SLOTS_O, NUM_SMALL_SLAB_AREAS, NUM_SMALL_SLABS, SEPARATE_FREELISTS_SPACE_REGION,
    SMALL_SLAB_AREAS_REGION_SPACE, TOTAL_VIRTUAL_MEMORY, VARIABLES_SPACE,
    large_slabnum_to_slotsize, small_slabnum_to_slotsize,
};

use bytesize::ByteSize;
use smalloc::platformalloc::sys_alloc;
use std::alloc::Layout;

fn conv(size: usize) -> String {
    ByteSize::b(size as u64).to_string_as(true) // true for binary units (KiB, MiB, GiB, etc.)
}

fn convsum(size: usize) -> String {
    let logtwo = size.ilog2();
    format!("{} ({:.3}b)", conv(size), logtwo)
}

use thousands::Separable;

fn print_virtual_bytes_map() -> usize {
    // See the README.md to understand this layout.

    // See the top of lib.rs for the *real* implementation. This here is just for running cheap experiments and printing out details.

    // The slabs up to and including MAX_SLABNUM_TO_PACK_INTO_CACHELINE have one slab in each area.
    let totslabs = NUM_SMALL_SLABS * NUM_SMALL_SLAB_AREAS + NUM_LARGE_SLABS;
    println!("totslabs: {}", totslabs);

    println!(
        "The virtual memory space for all the variables is {} ({})",
        VARIABLES_SPACE.separate_with_commas(),
        convsum(VARIABLES_SPACE)
    );

    println!(
        "The virtual memory space for the free lists is {} ({})",
        SEPARATE_FREELISTS_SPACE_REGION.separate_with_commas(),
        convsum(SEPARATE_FREELISTS_SPACE_REGION)
    );

    println!("small slabs space");
    println!(
        "{:>5} {:>8} {:>13} {:>16} {:>17}",
        "slab#", "size", "slots", "space", "areaspace"
    );
    println!(
        "{:>5} {:>8} {:>13} {:>16} {:>17}",
        "-----", "----", "-----", "-----", "---------"
    );
    // Then the space needed for the data slabs.
    for smallslabnum in 0..NUM_SMALL_SLABS {
        let slotsize = small_slabnum_to_slotsize(smallslabnum);
        println!(
            "{:>5} {:>8} {:>13} {:>16} {:>17}",
            smallslabnum,
            slotsize,
            NUM_SLOTS_O.separate_with_commas(),
            (slotsize * NUM_SLOTS_O).separate_with_commas(),
            (slotsize * NUM_SLOTS_O * NUM_SMALL_SLAB_AREAS).separate_with_commas()
        );
    }
    println!(
        "small slabs space: {} ({})",
        SMALL_SLAB_AREAS_REGION_SPACE.separate_with_commas(),
        convsum(SMALL_SLAB_AREAS_REGION_SPACE)
    );

    println!("large slabs space");
    println!(
        "{:>5} {:>8} {:>13} {:>20}",
        "slab#", "size", "slots", "space"
    );
    println!(
        "{:>5} {:>8} {:>13} {:>20}",
        "-----", "----", "-----", "-----"
    );
    // Then the space needed for the data slabs.
    for largeslabnum in 0..HUGE_SLABNUM {
        let slotsize = large_slabnum_to_slotsize(largeslabnum);
        println!(
            "{:>5} {:>8} {:>13} {:>20}",
            largeslabnum,
            slotsize,
            NUM_SLOTS_O.separate_with_commas(),
            (slotsize * NUM_SLOTS_O).separate_with_commas()
        );
    }
    let largeslabnum = HUGE_SLABNUM;
    let slotsize = large_slabnum_to_slotsize(largeslabnum);
    println!(
        "{:>5} {:>8} {:>13} {:>20}",
        largeslabnum,
        slotsize,
        NUM_SLOTS_HUGE.separate_with_commas(),
        (slotsize * NUM_SLOTS_HUGE).separate_with_commas()
    );

    println!(
        "large slabs space: {} ({})",
        LARGE_SLAB_REGION_SPACE.separate_with_commas(),
        convsum(LARGE_SLAB_REGION_SPACE)
    );

    println!(
        "About to try to allocate {} ({}) ({}) bytes...",
        TOTAL_VIRTUAL_MEMORY,
        TOTAL_VIRTUAL_MEMORY.separate_with_commas(),
        convsum(TOTAL_VIRTUAL_MEMORY)
    );
    let res_layout = Layout::from_size_align(TOTAL_VIRTUAL_MEMORY, MAX_ALIGNMENT);
    match res_layout {
        Ok(layout) => {
            let res_m = sys_alloc(layout);
            match res_m {
                Ok(m) => {
                    println!("It worked! m: {:?}", m);
                    //println!("ok");
                    1
                }
                Err(e) => {
                    println!("It failed! e: {:?}", e);
                    //println!("err");
                    0
                }
            }
        }
        Err(error) => {
            eprintln!("Err: {:?}", error);
            2
        }
    }
}

fn main() {
    print_virtual_bytes_map();
}
