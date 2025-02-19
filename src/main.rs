#![feature(test)]
extern crate test;

pub fn my_next_power_of_two_usize(n: usize) -> usize {
   if n == 1 {
      return 1;
   } else {
     let leading_zeros = (n-1).leading_zeros();
     let diffbits = usize::BITS - leading_zeros;
     return 1usize << diffbits;
    }
}

fn size_to_sizeclass_fit_usize(requested_size: usize) -> usize {
   if requested_size <= 64 {
      let fitted: usize = 64 / requested_size;
      return 64 / fitted;
   } else {
      return my_next_power_of_two_usize(requested_size);
   }
}

fn size_to_sizeclassnum_fit_usize(requested_size: usize) -> u8 {
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

fn sizeclassnum_to_sizeclass_fit_usize(scn: u8) -> usize {
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


use rand::Rng;
use test::Bencher;

#[bench]
fn bench_size_to_sizeclass_fit_usize(b: &mut Bencher) {
    let mut r = rand::rng();

    let mut x: usize = 0;
    b.iter(|| {
        for _i in 0..1000 {
       	    let exp = r.random_range(1..35);
       	    let num: usize = r.random_range(2_usize.pow(exp)..2_usize.pow(exp+1));
            x += size_to_sizeclass_fit_usize(num);
        }
    });
    println!("{}", x); // to keep the compiler from optimizing out the entire loop!
}

#[bench]
fn bench_size_to_sizeclassnum_fit_usize(b: &mut Bencher) {
    let mut r = rand::rng();

    let mut x: u16 = 0;
    b.iter(|| {
        for _i in 0..1000 {
       	    let exp = r.random_range(1..35);
       	    let num: usize = r.random_range(2_usize.pow(exp)+1..2_usize.pow(exp+1));
            x += size_to_sizeclassnum_fit_usize(num) as u16;
        }
    });
    println!("{}", x); // to keep the compiler from optimizing out the entire loop!
}

#[bench]
fn bench_sizeclassnum_to_sizeclass_fit_usize(b: &mut Bencher) {
    let mut r = rand::rng();

    let mut x: usize = 0;
    b.iter(|| {
        for _i in 0..1000 {
       	    let exp = r.random_range(1..35);
            x += sizeclassnum_to_sizeclass_fit_usize(exp);
        }
    });
    println!("{}", x); // to keep the compiler from optimizing out the entire loop!
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

fn main() {
   println!("Howdy, world!");

   for i in 1usize..66 {
       let rs: usize = i;
       println!("i: {}, log2(i): {}, size_to_sizeclassnum_fit_usize(i): {}", rs, rs.ilog2(), size_to_sizeclassnum_fit_usize(rs));
   }
}

