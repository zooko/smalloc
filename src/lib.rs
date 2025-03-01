#![feature(test)]
extern crate test;

// This algorithm generates the following slot sizes for slabs:

// sc    slotsize   slabsize   l     eaws  flhs  perslab    allslabs             vbu                 
// --    --------   --------   -     ----  ----  -------    --------             ---                 
// 0     2 B        64.0 kiB   2     2     2     64.0 kiB   16.0 MiB (24.000b)   16.0 MiB (24.000b)  
// 1     3 B        96.0 kiB   2     2     2     96.0 kiB   24.0 MiB (24.585b)   40.0 MiB (25.322b)  
// 2     4 B        128.0 kiB  2     2     2     128.0 kiB  32.0 MiB (25.000b)   72.0 MiB (26.170b)  
// 3     5 B        160.0 kiB  2     2     2     160.0 kiB  40.0 MiB (25.322b)   112.0 MiB (26.807b) 
// 4     6 B        192.0 kiB  2     2     2     192.0 kiB  48.0 MiB (25.585b)   160.0 MiB (27.322b) 
// 5     7 B        224.0 kiB  2     2     2     224.0 kiB  56.0 MiB (25.807b)   216.0 MiB (27.755b) 
// 6     8 B        256.0 kiB  2     2     2     256.0 kiB  64.0 MiB (26.000b)   280.0 MiB (28.129b) 
// 7     9 B        288.0 kiB  2     2     2     288.0 kiB  72.0 MiB (26.170b)   352.0 MiB (28.459b) 
// 8     10 B       320.0 kiB  2     2     2     320.0 kiB  80.0 MiB (26.322b)   432.0 MiB (28.755b) 
// 9     12 B       384.0 kiB  2     2     2     384.0 kiB  96.0 MiB (26.585b)   528.0 MiB (29.044b) 
// 10    16 B       512.0 kiB  2     2     2     512.0 kiB  128.0 MiB (27.000b)  656.0 MiB (29.358b) 
// 11    21 B       672.0 kiB  2     2     2     672.0 kiB  168.0 MiB (27.392b)  824.0 MiB (29.687b) 
// 12    32 B       1.0 MiB    2     2     2     1.0 MiB    256.0 MiB (28.000b)  1.1 GiB (30.077b)   
// 13    64 B       2.0 MiB    2     2     2     2.0 MiB    512.0 MiB (29.000b)  1.6 GiB (30.637b)   
// 14    128 B      4.0 MiB    2     2     2     4.0 MiB    1.0 GiB (30.000b)    2.6 GiB (31.353b)   
// 15    256 B      8.0 MiB    2     2     2     8.0 MiB    2.0 GiB (31.000b)    4.6 GiB (32.187b)   
// 16    512 B      16.0 MiB   2     2     2     16.0 MiB   4.0 GiB (32.000b)    8.6 GiB (33.097b)   
// 17    1.0 kiB    32.0 MiB   2     2     2     32.0 MiB   8.0 GiB (33.000b)    16.6 GiB (34.049b)  
// 18    2.0 kiB    64.0 MiB   2     2     2     64.0 MiB   16.0 GiB (34.000b)   32.6 GiB (35.025b)  
// 19    4.0 kiB    128.0 MiB  2     2     2     128.0 MiB  32.0 GiB (35.000b)   64.6 GiB (36.012b)  
// 20    8.0 kiB    256.0 MiB  2     2     2     256.0 MiB  64.0 GiB (36.000b)   128.6 GiB (37.006b) 
// 21    16.0 kiB   512.0 MiB  2     2     2     512.0 MiB  128.0 GiB (37.000b)  256.6 GiB (38.003b) 
// 22    32.0 kiB   1.0 GiB    2     2     2     1.0 GiB    256.0 GiB (38.000b)  512.6 GiB (39.002b) 
// 23    64.0 kiB   2.0 GiB    2     2     2     2.0 GiB    512.0 GiB (39.000b)  1.0 TiB (40.001b)   
// 24    128.0 kiB  4.0 GiB    2     2     2     4.0 GiB    1.0 TiB (40.000b)    2.0 TiB (41.000b)   
// 25    256.0 kiB  8.0 GiB    2     2     2     8.0 GiB    2.0 TiB (41.000b)    4.0 TiB (42.000b)   
// 26    512.0 kiB  16.0 GiB   2     2     2     16.0 GiB   4.0 TiB (42.000b)    8.0 TiB (43.000b)   
// 27    1.0 MiB    32.0 GiB   2     2     2     32.0 GiB   8.0 TiB (43.000b)    16.0 TiB (44.000b)  
// 28    2.0 MiB    64.0 GiB   2     2     2     64.0 GiB   16.0 TiB (44.000b)   32.0 TiB (45.000b)  
// 29    4.0 MiB    128.0 GiB  2     2     2     128.0 GiB  32.0 TiB (45.000b)   64.0 TiB (46.000b)  

pub fn layout_to_sizeclass(size: usize, alignment: usize) -> u8 {
    assert!(alignment > 0 && (alignment & (alignment - 1)) == 0, "alignment must be a power of two"); // benchmarks (on x86-64) show this bittwiddling expression is a little more efficient than the builtin power-of-two function

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
        return 8+(alignedsize-1).ilog2() as u8;
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
    } else {
        return 2usize.pow((scn-7).into());
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_s2scn_arg_3() {
        assert_eq!(layout_to_sizeclass(3, 1), 1);
    }

    #[test]
    fn test_s2scn_arg_3_2() {
        assert_eq!(layout_to_sizeclass(3, 2), 2);
    }

    #[test]
    fn test_s2scn_arg_3_4() {
        assert_eq!(layout_to_sizeclass(3, 4), 2);
    }

    #[test]
    fn test_s2scn_arg_6() {
        assert_eq!(layout_to_sizeclass(6, 1), 4); // 6 byte slots
    }

    #[test]
    fn test_s2scn_arg_6_2() {
        assert_eq!(layout_to_sizeclass(6, 2), 4); // 6 byte slots
    }

    #[test]
    fn test_s2scn_arg_6_4() {
        assert_eq!(layout_to_sizeclass(6, 4), 6); // 8 byte slots
    }

    #[test]
    fn test_s2scn_arg_6_8() {
        assert_eq!(layout_to_sizeclass(6, 8), 6); // 8 byte slots
    }

    #[test]
    fn test_s2scn_arg_6_16() {
        assert_eq!(layout_to_sizeclass(6, 16), 10); // 16 byte slots
    }

    #[test]
    fn test_s2scn_arg_9() {
        assert_eq!(layout_to_sizeclass(9, 1), 7); // 9 byte slots
    }

    #[test]
    fn test_s2scn_arg_9_2() {
        assert_eq!(layout_to_sizeclass(9, 2), 8); // 10 byte slots
    }

    #[test]
    fn test_s2scn_arg_9_4() {
        assert_eq!(layout_to_sizeclass(9, 4), 9); // 12 byte slots
    }

    #[test]
    fn test_s2scn_arg_9_8() {
        assert_eq!(layout_to_sizeclass(9, 8), 10); // 16 byte slots
    }

    #[test]
    fn test_s2scn_arg_9_16() {
        assert_eq!(layout_to_sizeclass(9, 16), 10); // 16 byte slots
    }

    #[test]
    fn test_s2scn_arg_9_32() {
        assert_eq!(layout_to_sizeclass(9, 32), 12); // 32 byte slots
    }

    #[test]
    fn test_s2scn_arg_9_64() {
        assert_eq!(layout_to_sizeclass(9, 64), 13); // 64 byte slots
    }

    #[test]
    fn test_s2scn_arg_9_128() {
        assert_eq!(layout_to_sizeclass(9, 128), 14); // 128 byte slots
    }

    #[test]
    fn test_s2scn_arg_9_256() {
        assert_eq!(layout_to_sizeclass(9, 256), 15); // 256 byte slots
    }

    #[test]
    fn test_s2scn_arg_9_512() {
        assert_eq!(layout_to_sizeclass(9, 512), 16); // 512 byte slots
    }

    #[test]
    fn test_s2scn_arg_9_1024() {
        assert_eq!(layout_to_sizeclass(9, 1024), 17); // 1024 byte slots
    }

    #[test]
    fn test_s2scn_arg_10() {
        assert_eq!(layout_to_sizeclass(10, 1), 8);
    }

    #[test]
    fn test_s2scn_arg_10_2() {
        assert_eq!(layout_to_sizeclass(10, 2), 8);
    }

    #[test]
    fn test_s2scn_arg_10_4() {
        assert_eq!(layout_to_sizeclass(10, 4), 9); // 12 byte slots
    }

    #[test]
    fn test_s2scn_arg_10_8() {
        assert_eq!(layout_to_sizeclass(10, 8), 10);
    }

    #[test]
    fn test_s2scn_arg_32() {
        assert_eq!(layout_to_sizeclass(32, 1), 12);
    }

    #[test]
    fn test_s2scn_arg_64() {
        assert_eq!(layout_to_sizeclass(64, 1), 13);
    }

    #[test]
    fn test_s2scn_arg_65() {
        assert_eq!(layout_to_sizeclass(65, 1), 14);
    }

    #[test]
    fn test_s2scn_arg_127() {
        assert_eq!(layout_to_sizeclass(127, 1), 14);
    }

    #[test]
    fn test_s2scn_arg_128() {
        assert_eq!(layout_to_sizeclass(128, 1), 14);
    }

    #[test]
    fn test_s2scn_arg_129() {
        assert_eq!(layout_to_sizeclass(129, 1), 15);
    }

    #[test]
    fn test_s2scn_arg_256() {
        assert_eq!(layout_to_sizeclass(256, 1), 15);
    }

    #[test]
    fn test_s2scn_arg_257() {
        assert_eq!(layout_to_sizeclass(257, 1), 16);
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
