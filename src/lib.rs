#![feature(test)]
extern crate test;

// This algorithm generates the following size classes for malloc requests of size i (if alignment can be 1):
// i, layout_to_sizeclass(i), layout_to_slotsize(i)
// ---, ---, ---
// 1, 0, 1
// 2, 1, 2
// 3, 2, 3
// 4, 3, 4
// 5, 4, 5
// 6, 5, 6
// 7, 6, 7
// 8, 7, 8
// 9, 8, 9
// 10, 9, 10
// 11, 10, 12
// 12, 10, 12
// 13, 11, 16
// 14, 11, 16
// 15, 11, 16
// 16, 11, 16
// 17, 12, 21
// 18, 12, 21
// 19, 12, 21
// 20, 12, 21
// 21, 12, 21
// 22, 13, 32
// 23, 13, 32
// 24, 13, 32
// 25, 13, 32
// 26, 13, 32
// 27, 13, 32
// 28, 13, 32
// 29, 13, 32
// 30, 13, 32
// 31, 13, 32
// 32, 13, 32
// 33, 14, 64
// 34, 14, 64
// 35, 14, 64
// 36, 14, 64
// 37, 14, 64
// 38, 14, 64
// 39, 14, 64
// 40, 14, 64
// 41, 14, 64
// 42, 14, 64
// 43, 14, 64
// 44, 14, 64
// 45, 14, 64
// 46, 14, 64
// 47, 14, 64
// 48, 14, 64
// 49, 14, 64
// 50, 14, 64
// 51, 14, 64
// 52, 14, 64
// 53, 14, 64
// 54, 14, 64
// 55, 14, 64
// 56, 14, 64
// 57, 14, 64
// 58, 14, 64
// 59, 14, 64
// 60, 14, 64
// 61, 14, 64
// 62, 14, 64
// 63, 14, 64
// 64, 14, 64
// 65, 15, 128
// 126, 15, 128
// 127, 15, 128
// 128, 15, 128
// 129, 16, 256
// 254, 16, 256
// 255, 16, 256
// 256, 16, 256
// 257, 17, 512

pub fn layout_to_sizeclass(size: usize, alignment: usize) -> u8 {
    assert!(alignment.is_power_of_two(), "alignment must be a power of two");
    assert!(alignment > 0 && (alignment & (alignment - 1)) == 0, "alignment must be a power of two");

    // Round up size to the nearest multiple of alignment:
    let alignedsize: usize = ((size - 1) | (alignment - 1)) + 1;

    if alignedsize == 1 {
        return 0;
    } else if alignedsize == 2 {
        return 1;
    } else if alignedsize == 3 {
        return 2;
    } else if alignedsize == 4 {
        return 3;
    } else if alignedsize == 5 {
        return 4;
    } else if alignedsize == 6 {
        return 5;
    } else if alignedsize == 7 {
        return 6;
    } else if alignedsize == 8 {
        return 7;
    } else if alignedsize == 9 {
        return 8;
    } else if alignedsize == 10 {
        return 9;
    } else if alignedsize <= 12 {
        return 10;
    } else if alignedsize <= 16 {
        return 11;
    } else if alignedsize <= 21 {
        return 12;
    } else if alignedsize <= 32 {
        return 13;
    } else if alignedsize <= 64 {
        return 14;
    } else {
        return 9+(alignedsize-1).ilog2() as u8;
    }
}

pub fn sizeclass_to_slotsize(scn: u8) -> usize {
//XXX what's the max sizeclass?
    if scn == 0 {
        return 1;
    } else if scn == 1 {
        return 2;
    } else if scn == 2 {
        return 3;
    } else if scn == 3 {
        return 4;
    } else if scn == 4 {
        return 5;
    } else if scn == 5 {
        return 6;
    } else if scn == 6 {
        return 7;
    } else if scn == 7 {
        return 8;
    } else if scn == 8 {
        return 9;
    } else if scn == 9 {
        return 10;
    } else if scn == 10 {
        return 12;
    } else if scn == 11 {
        return 16;
    } else if scn == 12 {
        return 21;
    } else if scn == 13 {
        return 32;
    } else if scn == 14 {
        return 64;
    } else {
        return 2usize.pow((scn-8).into());
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_s2scn_arg_3() {
        assert_eq!(layout_to_sizeclass(3, 1), 2);
    }

    #[test]
    fn test_s2scn_arg_3_2() {
        assert_eq!(layout_to_sizeclass(3, 2), 3);
    }

    #[test]
    fn test_s2scn_arg_3_4() {
        assert_eq!(layout_to_sizeclass(3, 4), 3);
    }

    #[test]
    fn test_s2scn_arg_6() {
        assert_eq!(layout_to_sizeclass(6, 1), 5); // 6 byte slots
    }

    #[test]
    fn test_s2scn_arg_6_2() {
        assert_eq!(layout_to_sizeclass(6, 2), 5); // 6 byte slots
    }

    #[test]
    fn test_s2scn_arg_6_4() {
        assert_eq!(layout_to_sizeclass(6, 4), 7); // 8 byte slots
    }

    #[test]
    fn test_s2scn_arg_6_8() {
        assert_eq!(layout_to_sizeclass(6, 8), 7); // 8 byte slots
    }

    #[test]
    fn test_s2scn_arg_6_16() {
        assert_eq!(layout_to_sizeclass(6, 16), 11); // 16 byte slots
    }

    #[test]
    fn test_s2scn_arg_9() {
        assert_eq!(layout_to_sizeclass(9, 1), 8); // 9 byte slots
    }

    #[test]
    fn test_s2scn_arg_9_2() {
        assert_eq!(layout_to_sizeclass(9, 2), 9); // 10 byte slots
    }

    #[test]
    fn test_s2scn_arg_9_4() {
        assert_eq!(layout_to_sizeclass(9, 4), 10); // 12 byte slots
    }

    #[test]
    fn test_s2scn_arg_9_8() {
        assert_eq!(layout_to_sizeclass(9, 8), 11); // 16 byte slots
    }

    #[test]
    fn test_s2scn_arg_9_16() {
        assert_eq!(layout_to_sizeclass(9, 16), 11); // 16 byte slots
    }

    #[test]
    fn test_s2scn_arg_9_32() {
        assert_eq!(layout_to_sizeclass(9, 32), 13); // 32 byte slots
    }

    #[test]
    fn test_s2scn_arg_9_64() {
        assert_eq!(layout_to_sizeclass(9, 64), 14); // 64 byte slots
    }

    #[test]
    fn test_s2scn_arg_9_128() {
        assert_eq!(layout_to_sizeclass(9, 128), 15); // 128 byte slots
    }

    #[test]
    fn test_s2scn_arg_9_256() {
        assert_eq!(layout_to_sizeclass(9, 256), 16); // 256 byte slots
    }

    #[test]
    fn test_s2scn_arg_9_512() {
        assert_eq!(layout_to_sizeclass(9, 512), 17); // 512 byte slots
    }

    #[test]
    fn test_s2scn_arg_9_1024() {
        assert_eq!(layout_to_sizeclass(9, 1024), 18); // 1024 byte slots
    }

    #[test]
    fn test_s2scn_arg_10() {
        assert_eq!(layout_to_sizeclass(10, 1), 9);
    }

    #[test]
    fn test_s2scn_arg_10_2() {
        assert_eq!(layout_to_sizeclass(10, 2), 9);
    }

    #[test]
    fn test_s2scn_arg_10_4() {
        assert_eq!(layout_to_sizeclass(10, 4), 10); // 12 byte slots
    }

    #[test]
    fn test_s2scn_arg_10_8() {
        assert_eq!(layout_to_sizeclass(10, 8), 11);
    }

    #[test]
    fn test_s2scn_arg_32() {
        assert_eq!(layout_to_sizeclass(32, 1), 13);
    }

    #[test]
    fn test_s2scn_arg_64() {
        assert_eq!(layout_to_sizeclass(64, 1), 14);
    }

    #[test]
    fn test_s2scn_arg_65() {
        assert_eq!(layout_to_sizeclass(65, 1), 15);
    }

    #[test]
    fn test_s2scn_arg_127() {
        assert_eq!(layout_to_sizeclass(127, 1), 15);
    }

    #[test]
    fn test_s2scn_arg_128() {
        assert_eq!(layout_to_sizeclass(128, 1), 15);
    }

    #[test]
    fn test_s2scn_arg_129() {
        assert_eq!(layout_to_sizeclass(129, 1), 16);
    }

    #[test]
    fn test_s2scn_arg_256() {
        assert_eq!(layout_to_sizeclass(256, 1), 16);
    }

    #[test]
    fn test_s2scn_arg_257() {
        assert_eq!(layout_to_sizeclass(257, 1), 17);
    }

    #[test]
    fn test_many_args() {
        for reqalign in vec![1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048] {
            for reqsiz in 1..10000 {
	        let sc: u8 = layout_to_sizeclass(reqsiz, reqalign);
	        let ss: usize = sizeclass_to_slotsize(sc);
                assert!(ss >= reqsiz);

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
                        assert!(tryss % reqalign != 0, "If tryss % reqalign == 0, then there was a smaller size class whose slot size was a multiple of the requested alignment. Therefore, we failed to find a valid optimization.");
                        
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
