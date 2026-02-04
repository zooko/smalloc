
use crate::Smalloc;
use std::sync::atomic::Ordering::Relaxed;
use crate::*;
use std::alloc::{Layout, GlobalAlloc};

/// If we've allocated all the slots from a slab, the next allocation of that sizeclass comes
/// from a different slab of the same sizeclass. This test doesn't work for the largest
/// sizeclass simply because the test assumes you can allocate 2 slots...
fn help_test_overflow_to_other_slab(sc: u8) {
    debug_assert!(sc >= NUM_UNUSED_SCS, "{sc}");
    debug_assert!(sc <= (NUM_SCS - 2)); // This test code needs at least 3 slots.

    let sm = get_testsmalloc();
    sm.inner().idempotent_init();

    let siz = help_slotsize(sc);
    let l = Layout::from_size_align(siz, 1).unwrap();

    let slabnum = rustlevel::get_slabnum();
    debug_assert!(slabnum < NUM_SLABS, "slabnum: {slabnum}, NUM_SLABS: {NUM_SLABS}");

    let numslots = help_numslots(sc);
    debug_assert!(numslots >= 3);

    // Step 0: reach into the slab's `flh` and set it to almost the max slot number.
    let first_i = numslots - 3;

    let mut i = first_i;
    #[cfg(target_os = "windows")]
    {
        sm.help_commit_slots(slabnum, sc, i, 2); // last slot is sentinel so we aren't going to actually touch its memory
    }
    sm.help_set_flh_singlethreaded(sc, i, slabnum);

    // Step 1: allocate a slot and store it in local variables:
    let p1 = unsafe { sm.alloc(l) };
    assert!(!p1.is_null());

    let (sc1, slabnum1, slotnum1) = sm.help_ptr_to_loc(p1);
    assert_eq!(sc1, sc, "p1: {p1:?}, sc: {sc}, sc1: {sc1}, slabnum: {slabnum}, slabnum1: {slabnum1}, SLABNUM_ALONE_MASK: {SLABNUM_BITS_ALONE_MASK:b}");
    assert_eq!(slotnum1, i, "p1: {p1:?}, sc: {sc}, sc1: {sc1}, slabnum: {slabnum}, slabnum1: {slabnum1}");

    i += 1;

    // Step 2: allocate all the rest of the slots in this slab except the last one:
    while i < numslots - 2 {
        let pt = unsafe { sm.alloc(l) };
        assert!(!pt.is_null());

        let (scn, _slabnumn, slotnumn) = sm.help_ptr_to_loc(pt);
        assert_eq!(scn, sc);
        assert_eq!(slotnumn, i);

        i += 1;
    }

    // Step 3: allocate the last slot in this slab and store it in local variables:
    let p2 = unsafe { sm.alloc(l) };
    assert!(!p2.is_null());

    let (sc2, slabnum2, slotnum2) = sm.help_ptr_to_loc(p2);
    // Assert some things about the two stored slot locations:
    assert_eq!(sc2, sc, "numslots: {numslots}, i: {i}");
    assert_eq!(slabnum1, slabnum2);
    assert_eq!(slotnum2, numslots - 2);

    // Step 4: allocate another slot and store it in local variables:
    let p3 = unsafe { sm.alloc(l) };
    assert!(!p3.is_null());

    let (sc3, slabnum3, slotnum3) = sm.help_ptr_to_loc(p3);

    // The raison d'etre for this test: Assert that the newly allocated slot is in the same size
    // class but a different slab.
    assert_eq!(sc3, sc, "sc3: {sc3}, sc: {sc}, slabnum3: {slabnum3}, slabnum1: {slabnum1}, p3: {p3:?}, p2: {p2:?}, siz: {siz}");
    assert_ne!(slabnum3, slabnum1);
    assert_eq!(slotnum3, 0);

    // Step 5: If we alloc_slot() again on this thread, it will come from this new slab:
    let p4 = unsafe { get_testsmalloc().alloc(l) };
    assert!(!p4.is_null(), "sc3: {sc3}, sc: {sc}, slabnum3: {slabnum3}, slabnum1: {slabnum1}, p3: {p3:?}, p2: {p2:?}, slotnum3: {slotnum3}");

    let (sc4, slabnum4, slotnum4) = sm.help_ptr_to_loc(p4);

    assert_eq!(sc4, sc3);
    assert_eq!(slabnum4, slabnum3);
    assert_eq!(slotnum4, 1);
}

const NUM_SLABS: u8 = 1 << NUM_SLABS_BITS;

/// This test doesn't work on the biggest sizeclass (sc 31).
fn help_test_overflow_to_other_sizeclass_once(sc: u8) {
    debug_assert!(sc >= NUM_UNUSED_SCS, "{sc}");
    debug_assert!(sc < NUM_SCS - 1);

    let sm = get_testsmalloc();

    let siz = help_slotsize(sc);
    let l = Layout::from_size_align(siz, 1).unwrap();
    let numslots = help_numslots(sc);
    let slabnum = rustlevel::get_slabnum();

    // Step 0: allocate a slot and store information about it in local variables:
    let p1 = unsafe { sm.alloc(l) };
    assert!(!p1.is_null());

    let (sc1, slabnum1, _slotnum1) = sm.help_ptr_to_loc(p1);

    assert_eq!(sc1, sc);
    assert_eq!(slabnum1, slabnum);

    // Step 1: reach into each slab's `flh` and set it to the max slot number (which means the
    // free list is empty).
    for slabnum in 0..NUM_SLABS {
        sm.help_set_flh_singlethreaded(sc, numslots - 1, slabnum);
    }

    // Step 3: Allocate another slot and store it in local variables:
    let p2 = unsafe { sm.alloc(l) };
    assert!(!p2.is_null());

    let (sc2, slabnum2, slotnum2) = sm.help_ptr_to_loc(p2);

    // The raison d'etre for this test: Assert that the newly allocated slot is in a bigger
    // size class, same areanum.
    assert_eq!(sc2, sc + 1, "sc2: {sc2}, sc: {sc}, slabnum2: {slabnum2}, slabnum1: {slabnum1}, p2: {p2:?}, p1: {p1:?}");
    assert_eq!(slabnum2, slabnum1);
    assert_eq!(slotnum2, 0, "sc2: {sc2}, sc: {sc}, slabnum2: {slabnum2}, slabnum1: {slabnum1}, p2: {p2:?}, p1: {p1:?}");

    // Step 5: If we alloc_slot() again on this thread, it will come from this new slab:
    let p3 = unsafe { sm.alloc(l) };
    assert!(!p3.is_null(), "sc2: {sc2}, sc: {sc}, slabnum2: {slabnum2}, slabnum1: {slabnum1}, p1: {p1:?}, p2: {p2:?}, slotnum2: {slotnum2}");

    let (sc3, slabnum3, slotnum3) = sm.help_ptr_to_loc(p3);

    assert_eq!(sc3, sc2);
    assert_eq!(slabnum3, slabnum2);
    assert_eq!(slotnum3, 1);
}

/// This test doesn't work on the biggest or second-biggest sizeclasses (sc's 31 and 30).
fn help_test_overflow_to_other_sizeclass_twice_at_once(sc: u8) {
    debug_assert!(sc >= NUM_UNUSED_SCS, "{sc}");
    debug_assert!(sc < NUM_SCS - 2);

    let sm = get_testsmalloc();

    let siz = help_slotsize(sc);
    let l = Layout::from_size_align(siz, 1).unwrap();
    let numslots = help_numslots(sc);
    let slabnum = rustlevel::get_slabnum();

    // Step 0: allocate a slot and store information about it in local variables:
    let p1 = unsafe { sm.alloc(l) };
    assert!(!p1.is_null());

    let (sc1, slabnum1, _slotnum1) = sm.help_ptr_to_loc(p1);

    assert_eq!(sc1, sc);
    assert_eq!(slabnum1, slabnum);

    // Step 1: reach into each slab's `flh` and set it to the max slot number (which means the
    // free list is empty).
    for slabnum in 0..NUM_SLABS {
        sm.help_set_flh_singlethreaded(sc, numslots - 1, slabnum);
    }

    // Step 2: reach into each slab's `flh` of the *next* sizeclass and set it to the max slot
    // number (which means the free list is empty).
    let sc_next = sc + 1;
    let numslots_next = help_numslots(sc_next);
    for slabnum in 0..NUM_SLABS {
        sm.help_set_flh_singlethreaded(sc_next, numslots_next - 1, slabnum);
    }

    // Step 3: Allocate another slot and store it in local variables:
    let p2 = unsafe { sm.alloc(l) };
    assert!(!p2.is_null());

    let (sc2, slabnum2, slotnum2) = sm.help_ptr_to_loc(p2);

    // The raison d'etre for this test: Assert that the newly allocated slot is in the *next* next
    // size class, same areanum.
    assert_eq!(sc2, sc + 2, "sc2: {sc2}, sc: {sc}, slabnum2: {slabnum2}, slabnum1: {slabnum1}, p2: {p2:?}, p1: {p1:?}");
    assert_eq!(slabnum2, slabnum1);
    assert_eq!(slotnum2, 0, "sc2: {sc2}, sc: {sc}, slabnum2: {slabnum2}, slabnum1: {slabnum1}, p2: {p2:?}, p1: {p1:?}");

    // Step 4: If we alloc_slot() again on this thread, it will come from this new sizeclass:
    let p3 = unsafe { sm.alloc(l) };
    assert!(!p3.is_null(), "sc2: {sc2}, sc: {sc}, slabnum2: {slabnum2}, slabnum1: {slabnum1}, p1: {p1:?}, p2: {p2:?}, slotnum2: {slotnum2}");

    let (sc3, slabnum3, slotnum3) = sm.help_ptr_to_loc(p3);

    assert_eq!(sc3, sc2);
    assert_eq!(slabnum3, slabnum2);
    assert_eq!(slotnum3, 1);
}

/// This test doesn't work on the biggest or second-biggest sizeclasses (sc's 31 and 30).
fn help_test_overflow_to_other_sizeclass_twice_in_a_row(sc: u8) {
    debug_assert!(sc >= NUM_UNUSED_SCS, "{sc}");
    debug_assert!(sc < NUM_SCS - 2);

    let sm = get_testsmalloc();

    let siz = help_slotsize(sc);
    let l = Layout::from_size_align(siz, 1).unwrap();
    let numslots = help_numslots(sc);
    let slabnum = rustlevel::get_slabnum();

    // Step 0: allocate a slot and store information about it in local variables:
    let p1 = unsafe { sm.alloc(l) };
    assert!(!p1.is_null());

    let (sc1, slabnum1, _slotnum1) = sm.help_ptr_to_loc(p1);

    assert_eq!(sc1, sc, "sc1: {sc1}, sc: {sc}, slabnum1: {slabnum1}, p1: {p1:?}, siz: {siz}");
    assert_eq!(slabnum1, slabnum);

    // Step 1: reach into each slab's `flh` and set it to the max slot number (which means the
    // free list is empty).
    for slabnum in 0..NUM_SLABS {
        sm.help_set_flh_singlethreaded(sc, numslots - 1, slabnum);
    }

    // Step 2: Allocate another slot and store it in local variables:
    let p2 = unsafe { sm.alloc(l) };
    assert!(!p2.is_null());

    let (sc2, slabnum2, slotnum2) = sm.help_ptr_to_loc(p2);

    // Assert that the newly allocated slot is in a bigger size class, same slab num.
    assert_eq!(sc2, sc + 1, "sc2: {sc2}, sc: {sc}, slabnum2: {slabnum2}, slabnum1: {slabnum1}, p2: {p2:?}, p1: {p1:?}");
    assert_eq!(slabnum2, slabnum1);
    assert_eq!(slotnum2, 0, "sc2: {sc2}, sc: {sc}, slabnum2: {slabnum2}, slabnum1: {slabnum1}, p2: {p2:?}, p1: {p1:?}");

    // Step 3: If we alloc_slot() again on this thread, it will come from this new size class:
    let p3 = unsafe { sm.alloc(l) };
    assert!(!p3.is_null(), "sc2: {sc2}, sc: {sc}, slabnum2: {slabnum2}, slabnum1: {slabnum1}, p1: {p1:?}, p3: {p3:?}, slotnum2: {slotnum2}");

    let (sc3, slabnum3, slotnum3) = sm.help_ptr_to_loc(p3);

    assert_eq!(sc3, sc2);
    assert_eq!(slabnum3, slabnum2);
    assert_eq!(slotnum3, 1);

    // Now to do the same thing again:
    let sc = sc3;
    let siz = help_slotsize(sc);
    let l = Layout::from_size_align(siz, 1).unwrap();
    let numslots = help_numslots(sc);

    // Step 4: reach into each slab's `flh` and set it to the max slot number (which means the
    // free list is empty).
    for slabnum in 0..NUM_SLABS {
        sm.help_set_flh_singlethreaded(sc, numslots - 1, slabnum);
    }

    // Step 5: Allocate another slot and store it in local variables:
    let p4 = unsafe { sm.alloc(l) };
    assert!(!p4.is_null());

    let (sc4, slabnum4, slotnum4) = sm.help_ptr_to_loc(p4);

    // The raison d'etre for this test: Assert that the newly allocated slot is in a bigger
    // size class, same slab num.
    assert_eq!(sc4, sc + 1, "sc4: {sc4}, sc: {sc}, slabnum4: {slabnum4}, slabnum1: {slabnum1}, p4: {p4:?}, p1: {p1:?}");
    assert_eq!(slabnum4, slabnum1);
    assert_eq!(slotnum4, 0, "sc4: {sc4}, sc: {sc}, slabnum4: {slabnum4}, slabnum1: {slabnum1}, p4: {p4:?}, p1: {p1:?}");

    // Step 6: If we alloc_slot() again on this thread, it will come from this new size class:
    let p5 = unsafe { sm.alloc(l) };
    assert!(!p5.is_null(), "sc4: {sc4}, sc: {sc}, slabnum4: {slabnum4}, slabnum1: {slabnum1}, p1: {p1:?}, p5: {p5:?}, slotnum2: {slotnum2}");

    let (sc5, slabnum5, slotnum5) = sm.help_ptr_to_loc(p5);

    assert_eq!(sc5, sc4);
    assert_eq!(slabnum5, slabnum2);
    assert_eq!(slotnum5, 1);

}

/// Allocate this size+align three times, then free the middle one, then allocate a fourth time,
/// then assert that the fourth slot is the same as the second slot. Also asserts that the
/// slab num is the same as this thread num.
fn help_alloc_four_times_singlethreaded(sm: &Smalloc, reqsize: usize, reqalign: usize) {
    assert!(reqsize > 0);
    assert!(reqsize <= help_slotsize(NUM_SCS - 1));
    assert!(reqalign > 0);

    let l = Layout::from_size_align(reqsize, reqalign).unwrap();

    let slabnum = rustlevel::get_slabnum();
    let orig_slabnum = slabnum;

    let p1 = unsafe { sm.alloc(l) };
    assert!(!p1.is_null(), "l: {l:?}");

    let (sc1, slabnum1, slotnum1) = sm.help_ptr_to_loc(p1);
    assert!(help_slotsize(sc1) >= reqsize);
    assert_eq!(slabnum1, orig_slabnum);

    let p2 = unsafe { sm.alloc(l) };
    assert!(!p2.is_null());

    let (sc2, slabnum2, slotnum2) = sm.help_ptr_to_loc(p2);
    assert!(help_slotsize(sc2) >= reqsize);
    assert_eq!(slabnum2, slabnum1, "p1: {p1:?}, p2: {p2:?}, slabnum1: {slabnum1}, slabnum2: {slabnum2}, slotnum1: {slotnum1}, slotnum2: {slotnum2}");
    assert_eq!(slabnum2, orig_slabnum);

    let p3 = unsafe { sm.alloc(l) };
    assert!(!p3.is_null());

    let (sc3, slabnum3, _slotnum3) = sm.help_ptr_to_loc(p3);
    assert!(help_slotsize(sc3) >= reqsize);
    assert_eq!(slabnum3, slabnum1);
    assert_eq!(slabnum3, orig_slabnum);

    // Now free the middle one.
    unsafe { sm.dealloc(p2, l) };

    // And allocate another one.
    let p4 = unsafe { sm.alloc(l) };
    assert!(!p4.is_null());

    let (sc4, slabnum4, slotnum4) = sm.help_ptr_to_loc(p4);
    assert!(help_slotsize(sc4) >= reqsize);
    assert_eq!(slabnum4, slabnum1);
    assert_eq!(slabnum4, orig_slabnum);

    // It should have allocated slot num 2 again
    assert_eq!(slotnum4, slotnum2);

    // Clean up so that we don't run out of slots while running these tests.
    unsafe { sm.dealloc(p1, l); }
    unsafe { sm.dealloc(p3, l); }
    unsafe { sm.dealloc(p4, l); }
}

fn highest_slotnum(sc: u8) -> u32 {
    help_numslots(sc) - 1
}

use std::cmp::max;
nextest_unit_tests! {
    fn test_reqali_to_sc() {
        let test_cases = [
            (1, 1, max(0, NUM_UNUSED_SCS)),
            (2, 1, max(1, NUM_UNUSED_SCS)),
            (3, 1, max(2, NUM_UNUSED_SCS)),
            (4, 1, max(2, NUM_UNUSED_SCS)),
            (5, 1, max(3, NUM_UNUSED_SCS)),
            (7, 1, max(3, NUM_UNUSED_SCS)),
            (8, 1, max(3, NUM_UNUSED_SCS)),
            (9, 1, max(4, NUM_UNUSED_SCS)),

            (1, 2, max(1, NUM_UNUSED_SCS)),
            (2, 2, max(1, NUM_UNUSED_SCS)),
            (3, 2, max(2, NUM_UNUSED_SCS)),
            (4, 2, max(2, NUM_UNUSED_SCS)),
            (5, 2, max(3, NUM_UNUSED_SCS)),
            (7, 2, max(3, NUM_UNUSED_SCS)),
            (8, 2, max(3, NUM_UNUSED_SCS)),
            (9, 2, max(4, NUM_UNUSED_SCS)),

            (1, 4, max(2, NUM_UNUSED_SCS)),
            (2, 4, max(2, NUM_UNUSED_SCS)),
            (3, 4, max(2, NUM_UNUSED_SCS)),
            (4, 4, max(2, NUM_UNUSED_SCS)),
            (5, 4, max(3, NUM_UNUSED_SCS)),
            (7, 4, max(3, NUM_UNUSED_SCS)),
            (8, 4, max(3, NUM_UNUSED_SCS)),
            (9, 4, max(4, NUM_UNUSED_SCS)),

            (1, 8, max(3, NUM_UNUSED_SCS)),
            (2, 8, max(3, NUM_UNUSED_SCS)),
            (3, 8, max(3, NUM_UNUSED_SCS)),
            (4, 8, max(3, NUM_UNUSED_SCS)),
            (5, 8, max(3, NUM_UNUSED_SCS)),
            (7, 8, max(3, NUM_UNUSED_SCS)),
            (8, 8, max(3, NUM_UNUSED_SCS)),
            (9, 8, max(4, NUM_UNUSED_SCS)),

            (1, 16, max(4, NUM_UNUSED_SCS)),
            (2, 16, max(4, NUM_UNUSED_SCS)),
            (3, 16, max(4, NUM_UNUSED_SCS)),
            (4, 16, max(4, NUM_UNUSED_SCS)),
            (5, 16, max(4, NUM_UNUSED_SCS)),
            (7, 16, max(4, NUM_UNUSED_SCS)),
            (8, 16, max(4, NUM_UNUSED_SCS)),
            (9, 16, max(4, NUM_UNUSED_SCS)),
            (15, 16, max(4, NUM_UNUSED_SCS)),
            (16, 16, max(4, NUM_UNUSED_SCS)),
            (17, 16, max(5, NUM_UNUSED_SCS)),

            (1, 32, max(5, NUM_UNUSED_SCS)),
            (2, 32, max(5, NUM_UNUSED_SCS)),
            (3, 32, max(5, NUM_UNUSED_SCS)),
            (4, 32, max(5, NUM_UNUSED_SCS)),
            (5, 32, max(5, NUM_UNUSED_SCS)),
            (7, 32, max(5, NUM_UNUSED_SCS)),
            (8, 32, max(5, NUM_UNUSED_SCS)),
            (9, 32, max(5, NUM_UNUSED_SCS)),
            (15, 32, max(5, NUM_UNUSED_SCS)),
            (16, 32, max(5, NUM_UNUSED_SCS)),
            (17, 32, max(5, NUM_UNUSED_SCS)),
            (30, 32, max(5, NUM_UNUSED_SCS)),
            (31, 32, max(5, NUM_UNUSED_SCS)),
            (32, 32, max(5, NUM_UNUSED_SCS)),

            (33, 32, max(6, NUM_UNUSED_SCS)),
            (32, 64, max(6, NUM_UNUSED_SCS)),
            (33, 64, max(6, NUM_UNUSED_SCS)),

            (2usize.pow(31), 4, 31),
            (4, 2usize.pow(31), 31),
        ];

        for (reqsiz, reqali, sc) in test_cases {
            assert_eq!(reqali_to_sc(reqsiz, reqali), sc, "reqali_to_sc({reqsiz}, {reqali}) should equal {sc}");
        }
    }

    fn a_few_allocs_and_a_dealloc_for_the_largest_slab() {
        let sm = get_testsmalloc();

        let sc = NUM_SCS - 1;
        let smallest = help_slotsize(sc - 1) + 1;
        let largest = help_slotsize(sc);

        for reqsize in [ smallest, smallest + 1, smallest + 2, largest - 3, largest - 1, largest, ] {
            // Generate alignments
            let mut reqalign = 1;
            loop {
                // Test this size/align combo
                let l = Layout::from_size_align(reqsize, reqalign).unwrap();

                let p1 = unsafe { sm.alloc(l) };

                let (sc1, _, slotnum1) = sm.help_ptr_to_loc(p1);

                assert_eq!(sc1, sc);
                assert_eq!(slotnum1, 0);

                unsafe { sm.dealloc(p1, l) };

                let p2 = unsafe { sm.alloc(l) };

                let (sc2, _, slotnum2) = sm.help_ptr_to_loc(p2);

                assert_eq!(sc2, sc);
                assert_eq!(slotnum2, 0);

                unsafe { sm.dealloc(p2, l) };

                let p3 = unsafe { sm.alloc(l) };

                let (sc3, _, slotnum3) = sm.help_ptr_to_loc(p3);

                assert_eq!(sc3, sc);
                assert_eq!(slotnum3, 0);

                unsafe { sm.dealloc(p3, l) };

                reqalign *= 2;
                if alignedsize_or(reqsize, reqalign) > largest {
                    break;
                };
            }
        }
    }

    /// If we've allocated all of the slots from a slab, the subsequent allocations come from a
    /// different slab in the same sizeclass.
    fn overflow_to_other_slab() {
        for sc in NUM_UNUSED_SCS..NUM_SCS - 1 {
            help_test_overflow_to_other_slab(sc);
        }
    }

    /// If we've allocated all of the slots from all slabs of this sizeclass, the subsequent
    /// allocations come from a bigger sizeclass.
    fn overflow_to_other_sizeclass_once() {
        for sc in NUM_UNUSED_SCS..NUM_SCS - 2 {
            help_test_overflow_to_other_sizeclass_once(sc);
        }
    }

    /// If we've allocated all of the slots from all slabs of this sizeclass, the subsequent
    /// allocations come from a bigger sizeclass, and then if we do that again it will work again.
    fn overflow_to_other_sizeclass_twice_in_a_row() {
       // Have to skip every other sc since it was used up by the previous iteration of the tests...
        for sc in (NUM_UNUSED_SCS..NUM_SCS - 2).step_by(2) {
            help_test_overflow_to_other_sizeclass_twice_in_a_row(sc);
        }
    }

    /// If we've allocated all of the slots from all slabs of this sizeclass and the next sizeclass,
    /// the subsequent allocations come from *next* next sizeclass
    fn overflow_to_other_sizeclass_twice_at_once() {
       // Have to skip every other sc since it was used up by the previous iteration of the tests...
        for sc in (NUM_UNUSED_SCS..NUM_SCS - 3).step_by(2) {
            help_test_overflow_to_other_sizeclass_twice_at_once(sc);
        }
    }

    /// If we've allocated all of the slots from all of the largest large-slots slabs, the next
    /// allocation will fail.
    fn overflow_from_all_largest_large_slots_slabs() {
        let sm = get_testsmalloc();
        sm.inner().idempotent_init();

        let sc = NUM_SCS - 1;
        let siz = help_slotsize(sc);
        let l = Layout::from_size_align(siz, 1).unwrap();

        let highestslotnum = highest_slotnum(sc);

        // Step 0: reach into each slab's `flh` and set it to the max slot number (which means the
        // free list is empty).
        for slabnum in 0..NUM_SLABS {
            sm.help_set_flh_singlethreaded(sc, highestslotnum, slabnum);
        }

        // Step 1: allocate a slot
        let p1 = unsafe { sm.alloc(l) };
        assert!(p1.is_null(), "p1: {p1:?}, sc: {sc}, l: {l:?}");
    }

    /// If we've allocated all of the slots from one of the largest large-slots slab, the next
    /// allocation will come from another one.
    fn overflow_from_one_largest_large_slots_slab() {
        let sm = get_testsmalloc();
        sm.inner().idempotent_init();

        let sc = NUM_SCS - 1;
        let siz = help_slotsize(sc);
        let l = Layout::from_size_align(siz, 1).unwrap();

        let highestslotnum = highest_slotnum(sc);

        // Step 0: reach into the current slab's `flh` and set it to the max slot number (which
        // means the free list is empty).
        let slabnum = rustlevel::get_slabnum();
        sm.help_set_flh_singlethreaded(sc, highestslotnum, slabnum);

        // Step 1: allocate a slot
        let p1 = unsafe { sm.alloc(l) };
        assert!(!p1.is_null(), "p1: {p1:?}, sc: {sc}, l: {l:?}");
        let (sc1, _slabnum1, _slotnum1) = sm.help_ptr_to_loc(p1);
        assert_eq!(sc1, sc);
    }

    fn a_few_allocs_and_a_dealloc_for_each_slab() {
        // Doesn't work for the largest size class (sc 31) because there aren't 3 slots.
        let sm = get_testsmalloc();

        for sc in NUM_UNUSED_SCS..NUM_SCS - 1 {
            help_alloc_diff_size_and_alignment_singlethreaded(sm, sc);
        }
    }
}

impl Smalloc {
    fn help_set_flh_singlethreaded(&self, sc: u8, slotnum: u32, slabnum: u8) {
        debug_assert!(sc >= NUM_UNUSED_SCS, "{sc}");
        debug_assert!(sc < NUM_SCS);

        let inner = self.inner();

        let smbp = inner.smbp.load(Acquire);

        let flhi = NUM_SCS as usize * slabnum as usize + sc as usize;
        let flhptr = smbp | flhi << 3;
        let flha = unsafe { AtomicU64::from_ptr(flhptr as *mut u64) };

        // single threaded so don't bother with the counter
        flha.store(slotnum as u64, Relaxed);
    }

    #[cfg(any(target_os = "windows", doc))]
    fn help_commit_slots(&self, slabnum: u8, sc: u8, firstslotnum: u32, numslots: u32) {
        //xxxeprintln!("in help_commit_slots, committing: slabnum: {slabnum}, sc: {sc}, firstslotnum: {firstslotnum}, numslots: {numslots}");
        let inner = self.inner();
        let smbp = inner.smbp.load(Acquire);
        let curfirstentry_p = smbp | ((((slabnum as usize) << NUM_SC_BITS) | sc as usize) << NUM_SN_D_T_BITS) | (firstslotnum as usize) << sc;

        let commit_size = (1usize << sc) * numslots as usize;
        sys_commit(curfirstentry_p as *mut u8, commit_size).unwrap();
    }

    /// Return the sizeclass, slabnum, and slotnum
    fn help_ptr_to_loc(&self, ptr: *const u8) -> (u8, u8, u32) {
        let smbp = self.inner().smbp.load(Relaxed);

        let p_addr = ptr.addr();

        assert!((p_addr >= smbp) && (p_addr <= smbp + HIGHEST_SMALLOC_SLOT_ADDR));

        let slabnum = (p_addr & SLABNUM_BITS_ADDR_MASK) >> SLABNUM_ADDR_SHIFT_BITS;
        let sc = (p_addr & SC_BITS_ADDR_MASK) >> NUM_SN_D_T_BITS;
        let slotnum = (p_addr & SLOTNUM_AND_DATA_ADDR_MASK as usize) >> sc;
        debug_assert!(slabnum < NUM_SLABS as usize);

        (sc as u8, slabnum as u8, slotnum as u32)
    }

}

fn help_numslots(sc: u8) -> u32 {
    1 << (NUM_SN_BITS - (sc - NUM_UNUSED_SCS))
}

fn help_slotsize(sc: u8) -> usize {
    help_pow2_usize(sc)
}

const fn help_pow2_usize(bits: u8) -> usize {
    2usize.pow(bits as u32)
}

fn alignedsize_or(size: usize, align: usize) -> usize {
    ((size - 1) | (align - 1)) + 1
}

/// Generate a number of requests (size+alignment) that fit into the given slab and for each
/// request call help_alloc_four_times_singlethreaded()
fn help_alloc_diff_size_and_alignment_singlethreaded(sm: &Smalloc, sc: u8) {
    debug_assert!(sc >= NUM_UNUSED_SCS, "{sc}");
    debug_assert!(sc < NUM_SCS);

    let smallest = if sc == 0 {
        1
    } else {
        2usize.pow((sc - 1) as u32) + 1
    };
    let largest = 2usize.pow(sc as u32);
    for reqsize in [smallest, smallest + 1, largest - 2, largest - 1, largest] {
        // Generate alignments
        let mut reqalign = 1;
        loop {
            // Test this size/align combo
            help_alloc_four_times_singlethreaded(sm, reqsize, reqalign);
            reqalign *= 2;
            if alignedsize_or(reqsize, reqalign) > largest {
                break;
            };
        }
    }
}

static mut UNIT_TEST_ALLOC: Smalloc = Smalloc::new();

fn get_testsmalloc() -> &'static Smalloc {
    unsafe { &*std::ptr::addr_of!(UNIT_TEST_ALLOC) }
}

#[macro_export]
macro_rules! nextest_unit_tests {
    (
        $(
            $(#[$attr:meta])*
            fn $name:ident() $body:block
        )*
    ) => {
        $(
            #[test]
            $(#[$attr])*
            fn $name() {
                if std::env::var("NEXTEST").is_err() {
                    panic!("This project requires cargo-nextest to run tests.");
                }
                    
                $body
            }
        )*
    };
}
