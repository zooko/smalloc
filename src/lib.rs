#![feature(test)]
extern crate test;

// This algorithm generates the following slot sizes for slabs:

// sc    slotsize   slabsize   l     eaws  flhs  perslab    allslabs             vbu                 
// --    --------   --------   -     ----  ----  -------    --------             ---                 

#[inline(always)]
pub fn layout_to_sizeclass(size: usize, alignment: usize) -> u8 {
    assert!(alignment > 0 && (alignment & (alignment - 1)) == 0, "alignment must be a power of two"); // benchmarks show this bittwiddling expression is a teeeny bit more efficient than the builtin power-of-two function (on some x86-64 systems but not others, and on Apple M4 Max).

    // Round up size to the nearest multiple of alignment:
    let alignedsize: usize = ((size - 1) | (alignment - 1)) + 1;

    if alignedsize <= 2 {
        return 0;
    } else if alignedsize == 3 {
        return 1;
    } else if alignedsize == 4 {
        return 2;
    } else if alignedsize == 5 {
        return 3;
    } else if alignedsize == 6 {
        return 4;
    } else if alignedsize == 7 {
        return 5;
    } else if alignedsize == 8 {
        return 6;
    } else if alignedsize == 9 {
        return 7;
    } else if alignedsize == 10 {
        return 8;
    } else if alignedsize <= 12 {
        return 9;
    } else if alignedsize <= 16 {
        return 10;
    } else if alignedsize <= 21 {
        return 11;
    } else if alignedsize <= 32 {
        return 12;
    } else if alignedsize <= 64 {
        return 13;
    } else {
        return (11+((alignedsize-1).ilog2()+1)/2) as u8;
    }
}

pub fn sizeclass_to_slotsize(scn: u8) -> usize {
    //XXX what's the max sizeclass?
    if scn == 0 {
        return 2;
    } else if scn == 1 {
        return 3;
    } else if scn == 2 {
        return 4;
    } else if scn == 3 {
        return 5;
    } else if scn == 4 {
        return 6;
    } else if scn == 5 {
        return 7;
    } else if scn == 6 {
        return 8;
    } else if scn == 7 {
        return 9;
    } else if scn == 8 {
        return 10;
    } else if scn == 9 {
        return 12;
    } else if scn == 10 {
        return 16;
    } else if scn == 11 {
        return 21;
    } else if scn == 12 {
        return 32;
    } else if scn == 13 {
        return 64;
    } else if scn == 14 {
        return 128;
    } else {
        return 4usize.pow((scn-11).into())*2;
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_l2s() {
	let testvecs: Vec<(usize, usize, u8)> = vec![
	    (3, 1, 1),
	    (3, 2, 2),
	    (3, 4, 2),
	    (6, 1, 4), // 6 byte slots
	    (6, 2, 4), // 6 byte slots
	    (6, 4, 6), // 8 byte slots
	    (6, 8, 6), // 8 byte slots
	    (6, 16, 10), // 16 byte slots
	    (9, 1, 7), // 9 byte slots
	    (9, 2, 8), // 10 byte slots
	    (9, 4, 9), // 12 byte slots
	    (9, 8, 10), // 16 byte slots
	    (9, 16, 10), // 16 byte slots
	    (9, 32, 12), // 32 byte slots
	    (9, 64, 13), // 64 byte slots
	    (9, 128, 14), // 128 byte slots
	    (9, 256, 15), // 512 byte slots
	    (9, 512, 15), // 512 byte slots
	    (9, 1024, 16), // 2048 byte slots
	    (9, 2048, 16), // 2048 byte slots
	    (10, 1, 8),
	    (10, 2, 8),
	    (10, 4, 9), // 12 byte slots
	    (10, 8, 10),
	    (32, 1, 12),
	    (64, 1, 13),
	    (65, 1, 14),
	    (127, 1, 14),
	    (128, 1, 14),
	    (129, 1, 15), // 512 byte slots
	    (256, 1, 15), // 512 byte slots
	    (257, 1, 15),
	    (2047, 1, 16), // 2 KiB slots
	    (2048, 1, 16), // 2 KiB slots
	    (2049, 1, 17), // 8 KiB slots
	    (4095, 1, 17), // 8 KiB slots
	    (4096, 1, 17), // 8 KiB slots
	    (4097, 1, 17), // 8 KiB slots
	    (8191, 1, 17), // 8 KiB slots
	    (8192, 1, 17), // 8 KiB slots
	    (8193, 1, 18), // 32 KiB slots
	    (16384, 1, 18) // 32 KiB slots
	];

	
	for (reqsiz, ali, sc) in testvecs.iter() {
            assert_eq!(*sc, layout_to_sizeclass(*reqsiz, *ali), "reqsize: {}, ali: {}, sc: {}, l2sc: {}", *reqsiz, *ali, *sc, layout_to_sizeclass(*reqsiz, *ali));
	}
    }

    #[test]
    fn test_roundtrip_sc2ss2sc() {
	for sc in 0..30 {
	    let ss = sizeclass_to_slotsize(sc);
	    let rtsc = layout_to_sizeclass(ss, 1);
	    assert_eq!(sc, rtsc, "{}", ss);
	}
    }

    #[test]
    fn test_many_args() {
        for reqalign in vec![1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048] {
            for reqsiz in 1..10000 {
                let sc: u8 = layout_to_sizeclass(reqsiz, reqalign);
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
