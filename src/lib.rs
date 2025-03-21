#![feature(test)]
extern crate test;

pub const MAX_SLABNUM_TO_PACK_INTO_CACHELINE: usize = 13;
pub const MAX_SLABNUM_TO_FIT_INTO_CACHELINE: usize = 14;
pub const MAX_SLABNUM_TO_PACK_INTO_PAGE: usize = 25;
pub const LARGE_SLOTS_SLABNUM: usize = 26;
pub const OVERSIZE_SLABNUM: usize = 27;
pub const NUM_SLABS: usize = 28;
pub const SIZE_OF_LARGE_SLOTS: usize = 6200000; // 6.2 million bytes

#[inline(always)]
pub fn slabnum_to_slotsize(slabnum: usize) -> usize {
    // Sizes where we can fit more slots into a 64-byte cache line. (And kinda maybe 128-byte cache-areas in certain ways...)
    if slabnum == 0 { 1 }
    else if slabnum == 1 { 2 }
    else if slabnum == 2 { 3 }
    else if slabnum == 3 { 4 }
    else if slabnum == 4 { 5 }
    else if slabnum == 5 { 6 }
    else if slabnum == 6 { 7 }
    else if slabnum == 7 { 8 }
    else if slabnum == 8 { 9 }
    else if slabnum == 9 { 10 }
    else if slabnum == 10 { 12 }
    else if slabnum == 11 { 16 }
    else if slabnum == 12 { 21 }
    else if slabnum == 13 { 32 } // MAX_SLABNUM_TO_PACK_INTO_CACHELINE // two or more

    // Debatable whether 64-byte allocations can benefit from sharing cachelines. Definitely not for 64B cachlines, but new Apple chips have 128B cachelines (in some cores) and cacheline pre-fetching on at least some modern Intel and AMD CPUs might give a caching advantage to having 64B slots. In any case, we're including a slot for 64B slots because of that, and because 64B slots pack nicely into 4096-byte memory pages. But the grower-promotion strategy will treat 32B slots (slab num 13) as the largest that can pack multiple objects into cachelines, ie it will promote any growers to at least slab num 15.
    else if slabnum == 14 { 64 } // MAX_SLABNUM_TO_FIT_INTO_CACHELINE // by itself

    // Sizes where we can fit more slots into a 4096-byte memory page.

    else if slabnum == 15 { 85 }
    else if slabnum == 16 { 113 }
    else if slabnum == 17 { 151 }
    else if slabnum == 18 { 204 }
    else if slabnum == 19 { 273 }
    else if slabnum == 20 { 372 }
    else if slabnum == 21 { 512 }
    else if slabnum == 22 { 682 }
    else if slabnum == 23 { 1024 }
    else if slabnum == 24 { 1365 }
    else if slabnum == 25 { 2048 } // MAX_SLABNUM_TO_PACK_INTO_PAGE

    // Large slots.
    else { SIZE_OF_LARGE_SLOTS } // LARGE_SLOTS_SLABNUM
}

pub const NUM_AREAS: usize = 256;

pub fn slabnum_to_numareas(slabnum: usize) -> usize {
    if slabnum <= MAX_SLABNUM_TO_PACK_INTO_CACHELINE {
	1
    } else {
	NUM_AREAS
    }
}

pub fn slabnum_to_l(slabnum: usize) -> u32 {
    // For the 1-byte slots, we can only fit a 1-byte index into the intrusive linked list.
    if slabnum == 0 { 1 }

    // For the 2-byte slots, we can only fit a 2-byte index into the intrusive linked list.
    else if slabnum == 1 { 2 }

    // For most slabs, we use 3-byte indexes so that our slabs won't get filled up
    else if slabnum <= LARGE_SLOTS_SLABNUM { 3 }
    
    // This isn't actually a slab, it's really the "oversized" category which we're going to fall back to mmap() to satisfy, so let's just say we have 7-byte indexes so that our slab-overflow analyzer in smalloclog won't ever think we've filled it up when analyzing memory usage operations from programs.
    else { 7 }
}

pub fn slabnum_to_numslots(slabnum: usize) -> usize {
    assert!(slabnum < NUM_SLABS);
    let l = slabnum_to_l(slabnum);
    assert!(l <= 7, "{} {}", l, slabnum);
    
    2usize.pow(l*8) - 1
}

#[inline(always)]
pub fn layout_to_slabnum(size: usize, alignment: usize) -> usize {
    assert!(alignment > 0 && (alignment & (alignment - 1)) == 0, "alignment must be a power of two"); // benchmarks show this bittwiddling expression is a teeeny bit more efficient than the builtin power-of-two function (on some x86-64 systems but not others, and on Apple M4 Max).

    // Round up size to the nearest multiple of alignment:
    let alignedsize: usize = ((size - 1) | (alignment - 1)) + 1;

    for slabnum in 0..NUM_SLABS {
	if alignedsize <= slabnum_to_slotsize(slabnum) {
	    return slabnum;
	}
    }

    OVERSIZE_SLABNUM
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_slabnum2ss2slabnum() {
	for slabnum in 0..OVERSIZE_SLABNUM {
	    let ss = slabnum_to_slotsize(slabnum);
	    let rtslabnum = layout_to_slabnum(ss, 1);
	    assert_eq!(slabnum, rtslabnum, "{}", ss);
	}
    }

    #[test]
    fn test_many_args() {
        for reqalign in [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048] {
            for reqsiz in 1..10000 {
                let slabnum = layout_to_slabnum(reqsiz, reqalign);
                let ss: usize = slabnum_to_slotsize(slabnum);
                assert!(ss >= reqsiz, "{} >= {}", ss, reqsiz);

                // Is there any *smaller* slab which still could have
                // held the requested size *and* whose slotsize is a
                // multiple of the requested alignment?  If so then we
                // failed to find a valid optimization.
                if slabnum > 0 {
                    let mut tryslabnum = slabnum-1;
                    loop {
                        let tryss: usize = slabnum_to_slotsize(tryslabnum);
                        if tryss < reqsiz {
                            break;
                        }
                        assert!(tryss % reqalign != 0, "If tryss % reqalign == 0, then there was a smaller slab whose slot size was a multiple of the requested alignment. Therefore, we failed to find a valid optimization. reqsiz: {}, slabnum: {}, ss: {}, tryslabnum: {}, tryss: {}", reqsiz, slabnum, ss, tryslabnum, tryss);
                        
                        if tryslabnum == 0 {
                            break;
                        }
                        tryslabnum -= 1;
                    }
                }
            }
        }
    }
}
