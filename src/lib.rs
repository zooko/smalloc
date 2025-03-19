#![feature(test)]
extern crate test;

// "SC" is short for "size class"
pub const MAX_SC_TO_PACK_INTO_CACHELINE: usize = 6;
pub const MAX_SC_TO_PACK_INTO_PAGE: usize = 11;
pub const HUGE_SLOTS_SC: usize = 12;
pub const OVERSIZE_SC: usize = 13;
pub const NUM_SCS: usize = 14;

#[inline(always)]
pub fn sizeclass_to_slotsize(scn: usize) -> usize {
    // Sizes where we can fit more slots into a 64-byte cache line. (And kinda maybe 128-byte cache-areas in certain ways...)
    if scn == 0 { 1 }
    else if scn == 1 { 2 }
    else if scn == 2 { 4 }
    else if scn == 3 { 8 }
    else if scn == 4 { 16 }
    else if scn == 5 { 21 }
    else if scn == 6 { 32 } // MAX_SC_TO_PACK_INTO_CACHELINE

    // Debatable whether 64-byte allocations can benefit from sharing cachelines. Definitely not for 64B cachlines, but new Apple chips have 128B cachelines (in some cores) and cacheline pre-fetching on at least some modern Intel and AMD CPUs might give a caching advantage to having 64B slots. In any case, we're including a sizeclass for 64B slots because of that, and because 64B slots pack nicely into 4096-byte memory pages. But the grower-promotion strategy will treat 32B slots (SC 13) as the largest that can pack multiple objects into cachelines, ie it will promote any growers to at least SC 8.
    else if scn == 7 { 64 }

    // Sizes where we can fit more slots into a 4096-byte memory page.
    else if scn == 8 { 512 }
    else if scn == 9 { 1024 }
    else if scn == 10 { 1365 }
    else if scn == 11 { 2048 } // MAX_SC_TO_PACK_INTO_PAGE

    // Huge slots.
    else { 2usize.pow(23) } // HUGE_SLOTS_SC
}

pub fn sizeclass_to_slots(sc: usize) -> usize {
    2usize.pow(sizeclass_to_l(sc)*8 - 1)
}

pub fn sizeclass_to_l(sc: usize) -> u32 {
    // For the 1-byte slots, we can only fit a 1-byte index into the intrusive linked list.
    if sc == 0 { 1 }

    // For most slabs, we use 2-byte indexes instead of 3-byte, so we can have more size classes before we run out of virtual memory space.
    else if sc < HUGE_SLOTS_SC { 2 }
    else if sc == HUGE_SLOTS_SC { 2 }

    // This isn't actually a slab, it's really the "oversized" category which we're going to fall back to mmap() to satisfy, so let's just say we have 4-byte indexes so that our slab-overflow analyzer in smalloclog won't ever think we've filled it up when analyzing memory usage operations from programs.
    else{ 8 }
}

#[inline(always)]
pub fn layout_to_sizeclass(size: usize, alignment: usize) -> usize {
    assert!(alignment > 0 && (alignment & (alignment - 1)) == 0, "alignment must be a power of two"); // benchmarks show this bittwiddling expression is a teeeny bit more efficient than the builtin power-of-two function (on some x86-64 systems but not others, and on Apple M4 Max).

    // Round up size to the nearest multiple of alignment:
    let alignedsize: usize = ((size - 1) | (alignment - 1)) + 1;

    for sc in 0..NUM_SCS {
	if alignedsize <= sizeclass_to_slotsize(sc) {
	    return sc;
	}
    }

    OVERSIZE_SC
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_sc2ss2sc() {
	for sc in 0..OVERSIZE_SC {
	    let ss = sizeclass_to_slotsize(sc);
	    let rtsc = layout_to_sizeclass(ss, 1);
	    assert_eq!(sc, rtsc, "{}", ss);
	}
    }

    #[test]
    fn test_many_args() {
        for reqalign in [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048] {
            for reqsiz in 1..10000 {
                let sc = layout_to_sizeclass(reqsiz, reqalign);
                let ss: usize = sizeclass_to_slotsize(sc);
                assert!(ss >= reqsiz, "{} >= {}", ss, reqsiz);

                // Is there any *smaller* size class which still
                // could have held the requested size *and* whose
                // slotsize is a multiple of the requested alignment?
                // If so then we failed to find a valid optimization.
                if sc > 0 {
                    let mut trysc = sc-1;
                    loop {
                        let tryss: usize = sizeclass_to_slotsize(trysc);
                        if tryss < reqsiz {
                            break;
                        }
                        assert!(tryss % reqalign != 0, "If tryss % reqalign == 0, then there was a smaller size class whose slot size was a multiple of the requested alignment. Therefore, we failed to find a valid optimization. reqsiz: {}, sc: {}, ss: {}, trysc: {}, tryss: {}", reqsiz, sc, ss, trysc, tryss);
                        
                        if trysc == 0 {
                            break;
                        }
                        trysc -= 1;
                    }
                }
            }
        }
    }

    //XXX    #[test]
    //XXX    fn test_overflow_sc_to_ss() {
    //XXX        let ss: usize = sizeclass_to_slotsize(255);
    //XXX        println!("ss: {}", ss);
    //XXX        panic!("WhheeeE");
    //XXX    }


}
