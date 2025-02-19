use simalloc::size_to_sizeclassnum_fit_usize;
use simalloc::size_to_sizeclass_fit_usize;

fn main() {
   println!("Howdy, world!");

   for i in 1usize..66 {
       let rs: usize = i;
       println!("i: {}, size_to_sizeclassnum_fit_usize(i): {}, size_to_sizeclass_fit_usize(i): {}", rs, size_to_sizeclassnum_fit_usize(rs), size_to_sizeclass_fit_usize(rs));
   }
}

