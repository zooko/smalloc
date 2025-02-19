#![feature(test)]
extern crate test;

pub fn size_to_sizeclass_fit_usize(requested_size: usize) -> usize {
   if requested_size <= 64 {
      let fitted: usize = 64 / requested_size;
      return 64 / fitted;
   } else {
      return requested_size.next_power_of_two();
   }
}

pub fn size_to_sizeclassnum_fit_usize(requested_size: usize) -> u8 {
   let rs: usize = requested_size;
   if rs == 1 {
      return 0;
   } else if rs == 2 {
      return 1;
   } else if rs == 3 {
      return 2;
   } else if rs == 4 {
      return 3;
   } else if rs == 5 {
      return 4;
   } else if rs == 6 {
      return 5;
   } else if rs == 7 {
      return 6;
   } else if rs == 8 {
      return 7;
   } else if rs == 9 {
      return 8;
   } else if rs == 10 {
      return 9;
   } else if rs <= 12 {
      return 10;
   } else if rs <= 16 {
      return 11;
   } else if rs <= 21 {
      return 12;
   } else if rs <= 32 {
      return 13;
   } else if rs <= 64 {
      return 14;
   } else {
      return 9+(rs-1).ilog2() as u8;
   }
}

pub fn sizeclassnum_to_sizeclass_fit_usize(scn: u8) -> usize {
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

    // fit usize
    #[test]
    fn test_s2scnfu_arg_32() {
       assert_eq!(size_to_sizeclassnum_fit_usize(32), 13);
    }

    #[test]
    fn test_s2scnfu_arg_64() {
       assert_eq!(size_to_sizeclassnum_fit_usize(64), 14);
    }

    #[test]
    fn test_s2scnfu_arg_65() {
       assert_eq!(size_to_sizeclassnum_fit_usize(65), 15);
    }

    #[test]
    fn test_s2scnfu_arg_127() {
       assert_eq!(size_to_sizeclassnum_fit_usize(127), 15);
    }

    #[test]
    fn test_s2scnfu_arg_128() {
       assert_eq!(size_to_sizeclassnum_fit_usize(128), 15);
    }

    #[test]
    fn test_s2scnfu_arg_129() {
       assert_eq!(size_to_sizeclassnum_fit_usize(129), 16);
    }

    #[test]
    fn test_s2scnfu_arg_256() {
       assert_eq!(size_to_sizeclassnum_fit_usize(256), 16);
    }

    #[test]
    fn test_s2scnfu_arg_257() {
       assert_eq!(size_to_sizeclassnum_fit_usize(257), 17);
    }

    #[test]
    fn test_roundtrip_fit_usize() {
        for siz in 1..1001 {
	    let scn_a: u8 = size_to_sizeclassnum_fit_usize(siz);

	    let sc: usize = size_to_sizeclass_fit_usize(siz);
	    let scn_b: u8 = size_to_sizeclassnum_fit_usize(sc);

	    assert_eq!(scn_a, scn_b, "siz: {}, sc: {}", siz, sc);
	}
    }

}
