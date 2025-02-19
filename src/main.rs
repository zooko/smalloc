use simalloc::size_to_sizeclassnum;
use simalloc::size_to_sizeclass;

fn main() {
   println!("Howdy, world!");

   for i in 1usize..66 {
       let rs: usize = i;
       println!("i: {}, size_to_sizeclassnum(i): {}, size_to_sizeclass(i): {}", rs, size_to_sizeclassnum(rs), size_to_sizeclass(rs));
   }
}

