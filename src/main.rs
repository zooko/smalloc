#![feature(test)]
extern crate test;

// test bench_size_class_100 ... bench:         309.50 ns/iter (+/- 6.92)
fn size_to_sizeclass_1(requested_size: usize) -> usize {
   assert!(requested_size > 0, "requested_size is not greater than 0");
   if requested_size <= 64 {
      let fitted: usize = 64 / requested_size;
      return 64 / fitted;
   } else {
      return requested_size.next_power_of_two();
   }
}

// {64: {1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 12, 16, 21, 32, 64}}
fn size_to_sizeclassnum_1(requested_size: usize) -> u8 {
   assert!(requested_size > 0, "requested_size is not greater than 0");
   if requested_size == 1 {
      return 0;
   } else if requested_size == 2 {
      return 1;
   } else if requested_size == 3 {
      return 2;
   } else if requested_size == 4 {
      return 3;
   } else if requested_size == 5 {
      return 4;
   } else if requested_size == 6 {
      return 5;
   } else if requested_size == 7 {
      return 6;
   } else if requested_size == 8 {
      return 7;
   } else if requested_size == 9 {
      return 8;
   } else if requested_size == 10 {
      return 9;
   } else if requested_size <= 12 {
      return 10;
   } else if requested_size <= 16 {
      return 11;
   } else if requested_size <= 21 {
      return 12;
   } else if requested_size <= 32 {
      return 13;
   } else if requested_size <= 64 {
      return 14;
   } else {
      println!("rs: {} -> {}", requested_size, ((requested_size-1).ilog2() as u8));
      return 9+(requested_size-1).ilog2() as u8;
   }
}

fn sizeclassnum_to_sizeclass_1(scn: u8) -> usize {
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
      return 2_usize.pow((scn-8).into())
   }
}

// ---


// ---


use rand::Rng;
use test::Bencher;
#[bench]
fn bench_size_to_sizeclass_1_10iters_rng(b: &mut Bencher) {
    let mut r = rand::rng();

    let mut x: usize = 0;
    b.iter(|| {
        for _i in 1..11 {
       	    let exp = r.random_range(1..35);
       	    let num: usize = r.random_range(2_usize.pow(exp)..2_usize.pow(exp+1));
            x += size_to_sizeclass_1(num);
        }
    });
    println!("{}", x); // to keep the compiler from optimizing out the entire loop!
}

#[bench]
fn bench_size_to_sizeclassnum_1_10iters_rng(b: &mut Bencher) {
    let mut r = rand::rng();

    let mut x: u16 = 0;
    b.iter(|| {
        for _i in 1..11 {
       	    let exp = r.random_range(1..35);
       	    let num: usize = r.random_range(2_usize.pow(exp)+1..2_usize.pow(exp+1));
            x += size_to_sizeclassnum_1(num) as u16;
        }
    });
    println!("{}", x); // to keep the compiler from optimizing out the entire loop!
}

#[bench]
fn bench_sizeclassnum_to_sizeclass_1_10iters_rng(b: &mut Bencher) {
    let mut r = rand::rng();

    let mut x: usize = 0;
    b.iter(|| {
        for _i in 1..11 {
       	    let exp = r.random_range(1..35);
            x += sizeclassnum_to_sizeclass_1(exp);
        }
    });
    println!("{}", x); // to keep the compiler from optimizing out the entire loop!
}

fn main() {
   println!("Howdy, world!");
   for i in 2usize..1000 {
       println!("i: {} -> {}", i, (i-1).ilog2());
   }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_s2scn1_arg_32() {
       assert_eq!(size_to_sizeclassnum_1(32), 13);
    }

    #[test]
    fn test_s2scn1_arg_64() {
       assert_eq!(size_to_sizeclassnum_1(64), 14);
    }

    #[test]
    fn test_s2scn1_arg_65() {
       assert_eq!(size_to_sizeclassnum_1(65), 15);
    }

    #[test]
    fn test_s2scn1_arg_127() {
       assert_eq!(size_to_sizeclassnum_1(127), 15);
    }

    #[test]
    fn test_s2scn1_arg_128() {
       assert_eq!(size_to_sizeclassnum_1(128), 15);
    }

    #[test]
    fn test_s2scn1_arg_129() {
       assert_eq!(size_to_sizeclassnum_1(129), 16);
    }

    #[test]
    fn test_s2scn1_arg_256() {
       assert_eq!(size_to_sizeclassnum_1(256), 16);
    }

    #[test]
    fn test_s2scn1_arg_257() {
       assert_eq!(size_to_sizeclassnum_1(257), 17);
    }

    #[test]
    fn test_roundtrip_1() {
        for siz in 1..1001 {
	    let scn_a: u8 = size_to_sizeclassnum_1(siz);

	    let sc: usize = size_to_sizeclass_1(siz);
	    let scn_b: u8 = size_to_sizeclassnum_1(sc);

	    assert_eq!(scn_a, scn_b, "siz: {}, sc: {}", siz, sc);
	}
    }


}

fn conv(mut num_bytes: f64) -> String {
    let units = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
    
    for unit in units.iter() {
        if num_bytes < 1024.0 {
            return format!("{:.2} {}", num_bytes, unit);
        }
        num_bytes /= 1024.0;
    }

    // In case the loop completes without returning, which shouldn't happen for valid input
    format!("{:.2} EiB", num_bytes)
}

