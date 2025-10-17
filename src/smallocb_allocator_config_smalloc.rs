// This file gets cp'ed by "runbench.sh", so if you edit this file you have to go figure out if
// you're editing the original or the copy that is going to get overwritten the next time someone
// runs "runbench.sh". This variant -- the Smalloc variant -- has to be in place to run the smalloc
// unit tests.

use std::sync::Arc;
use crate::Smalloc;

pub type AllocatorType = Smalloc;

pub fn gen_allocator() -> Arc<AllocatorType> {
    Arc::new(Smalloc::new())
}
