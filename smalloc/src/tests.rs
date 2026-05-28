
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

    let slabnum = get_slabnum();
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
    assert_eq!(sc1, sc, "p1: {p1:?}, sc: {sc}, sc1: {sc1}, slabnum: {slabnum}, slabnum1: {slabnum1}, SLABNUM_ALONE_MASK: {SLABNUM_ALONE_MASK:b}");
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
    let slabnum = get_slabnum();

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
    let slabnum = get_slabnum();

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
    let slabnum = get_slabnum();

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

    let slabnum = get_slabnum();
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
    fn test_sc_to_sentinel_slotnum() {
        let test_cases = [
            (5, 0b1111111111111111111111111111111),
            (6, 0b111111111111111111111111111111),
            (31, 0b11111),
        ];

        for (sc, sentinel) in test_cases {
            assert!(sc_to_sentinel_slotnum(sc) == sentinel, "sc_to_sentinel_slotnum({sc}) should equal {:0b} not {:0b}", sentinel, sc_to_sentinel_slotnum(sc));
        }
    }

    fn test_reqali_to_sc() {
        let test_cases = [
            (1, 1, max(5, NUM_UNUSED_SCS)),
            (2, 1, max(5, NUM_UNUSED_SCS)),
            (3, 1, max(5, NUM_UNUSED_SCS)),
            (4, 1, max(5, NUM_UNUSED_SCS)),
            (5, 1, max(5, NUM_UNUSED_SCS)),
            (8, 1, max(5, NUM_UNUSED_SCS)),
            (9, 1, max(5, NUM_UNUSED_SCS)),
            (15, 1, max(5, NUM_UNUSED_SCS)),
            (16, 1, max(5, NUM_UNUSED_SCS)),
            (17, 1, max(6, NUM_UNUSED_SCS)),
            (31, 1, max(6, NUM_UNUSED_SCS)),
            (32, 1, max(6, NUM_UNUSED_SCS)),
            (33, 1, max(6, NUM_UNUSED_SCS)),
            (48, 1, max(6, NUM_UNUSED_SCS)),
            (49, 1, max(7, NUM_UNUSED_SCS)),
            (112, 1, max(7, NUM_UNUSED_SCS)),
            (113, 1, max(8, NUM_UNUSED_SCS)),

            (1, 2, max(5, NUM_UNUSED_SCS)),
            (2, 2, max(5, NUM_UNUSED_SCS)),
            (3, 2, max(5, NUM_UNUSED_SCS)),
            (4, 2, max(5, NUM_UNUSED_SCS)),
            (5, 2, max(5, NUM_UNUSED_SCS)),
            (8, 2, max(5, NUM_UNUSED_SCS)),
            (9, 2, max(5, NUM_UNUSED_SCS)),
            (15, 2, max(5, NUM_UNUSED_SCS)),
            (16, 2, max(5, NUM_UNUSED_SCS)),
            (17, 2, max(6, NUM_UNUSED_SCS)),
            (31, 2, max(6, NUM_UNUSED_SCS)),
            (32, 2, max(6, NUM_UNUSED_SCS)),
            (33, 2, max(6, NUM_UNUSED_SCS)),
            (48, 2, max(6, NUM_UNUSED_SCS)),
            (49, 2, max(7, NUM_UNUSED_SCS)),
            (112, 2, max(7, NUM_UNUSED_SCS)),
            (113, 2, max(8, NUM_UNUSED_SCS)),

            (1, 4, max(5, NUM_UNUSED_SCS)),
            (2, 4, max(5, NUM_UNUSED_SCS)),
            (3, 4, max(5, NUM_UNUSED_SCS)),
            (4, 4, max(5, NUM_UNUSED_SCS)),
            (5, 4, max(5, NUM_UNUSED_SCS)),
            (8, 4, max(5, NUM_UNUSED_SCS)),
            (9, 4, max(5, NUM_UNUSED_SCS)),
            (15, 4, max(5, NUM_UNUSED_SCS)),
            (16, 4, max(5, NUM_UNUSED_SCS)),
            (17, 4, max(6, NUM_UNUSED_SCS)),
            (31, 4, max(6, NUM_UNUSED_SCS)),
            (32, 4, max(6, NUM_UNUSED_SCS)),
            (33, 4, max(6, NUM_UNUSED_SCS)),
            (48, 4, max(6, NUM_UNUSED_SCS)),
            (49, 4, max(7, NUM_UNUSED_SCS)),
            (112, 4, max(7, NUM_UNUSED_SCS)),
            (113, 4, max(8, NUM_UNUSED_SCS)),

            (1, 32, max(5, NUM_UNUSED_SCS)),
            (2, 32, max(5, NUM_UNUSED_SCS)),
            (3, 32, max(5, NUM_UNUSED_SCS)),
            (4, 32, max(5, NUM_UNUSED_SCS)),
            (5, 32, max(5, NUM_UNUSED_SCS)),
            (8, 32, max(5, NUM_UNUSED_SCS)),
            (9, 32, max(5, NUM_UNUSED_SCS)),
            (15, 32, max(5, NUM_UNUSED_SCS)),
            (16, 32, max(5, NUM_UNUSED_SCS)),
            (17, 32, max(6, NUM_UNUSED_SCS)),
            (31, 32, max(6, NUM_UNUSED_SCS)),
            (32, 32, max(6, NUM_UNUSED_SCS)),
            (33, 32, max(6, NUM_UNUSED_SCS)),
            (48, 32, max(6, NUM_UNUSED_SCS)),
            (49, 32, max(7, NUM_UNUSED_SCS)),
            (112, 32, max(7, NUM_UNUSED_SCS)),
            (113, 32, max(8, NUM_UNUSED_SCS)),

            (1, 64, max(6, NUM_UNUSED_SCS)),
            (2, 64, max(6, NUM_UNUSED_SCS)),
            (3, 64, max(6, NUM_UNUSED_SCS)),
            (4, 64, max(6, NUM_UNUSED_SCS)),
            (5, 64, max(6, NUM_UNUSED_SCS)),
            (8, 64, max(6, NUM_UNUSED_SCS)),
            (9, 64, max(6, NUM_UNUSED_SCS)),
            (15, 64, max(6, NUM_UNUSED_SCS)),
            (16, 64, max(6, NUM_UNUSED_SCS)),
            (17, 64, max(6, NUM_UNUSED_SCS)),
            (31, 64, max(6, NUM_UNUSED_SCS)),
            (32, 64, max(6, NUM_UNUSED_SCS)),
            (33, 64, max(6, NUM_UNUSED_SCS)),
            (48, 64, max(6, NUM_UNUSED_SCS)),
            (49, 64, max(7, NUM_UNUSED_SCS)),
            (112, 64, max(7, NUM_UNUSED_SCS)),
            (113, 64, max(8, NUM_UNUSED_SCS)),

            (2usize.pow(31)-FREE_SLOT_METADATA_BYTES, 4, 31),
            (2usize.pow(32)-FREE_SLOT_METADATA_BYTES, 4, 32),
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
        let slabnum = get_slabnum();
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

    /// Allocate the highest-addressed valid slot in the smalloc region and write the last byte of that
    /// slot. This catches bugs where `HIGHEST_SHMALLOC_SLOT_ADDR` or `HIGHEST_SHMALLOC_BYTE_ADDR` fail
    /// to cover the largest slab, largest sizeclass, and largest non-sentinel slot number.
    fn test_highest_addressed_slot_last_byte_is_writable() {
        let sm = get_testsmalloc();
        sm.inner().idempotent_init();

        let sc = NUM_SCS - 1;
        let siz = help_slotsize(sc);
        let l = Layout::from_size_align(siz, 1).unwrap();

        let slabnum = NUM_SLABS - 1;
        debug_assert!(slabnum & !SLABNUM_ALONE_MASK == 0);

        let numslots = help_numslots(sc);
        let slotnum = numslots - 2;

        set_slab_num(slabnum);
        sm.help_set_flh_singlethreaded(sc, slotnum, slabnum);

        let p = unsafe { sm.alloc(l) };
        assert!(!p.is_null());

        let (sc1, slabnum1, slotnum1) = sm.help_ptr_to_loc(p);
        assert_eq!(sc1, sc, "p: {p:?}, sc: {sc}, sc1: {sc1}");
        assert_eq!(slabnum1, slabnum, "p: {p:?}, slabnum: {slabnum}, slabnum1: {slabnum1}");
        assert_eq!(slotnum1, slotnum, "p: {p:?}, slotnum: {slotnum}, slotnum1: {slotnum1}");

        unsafe {
            p.add(siz - 1).write_volatile(0xa5);
            sm.dealloc(p, l);
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

        assert!((p_addr >= smbp) && (p_addr <= smbp + HIGHEST_SHMALLOC_SLOT_ADDR));

        let slabnum = (p_addr & SLABNUM_BITS_ADDR_MASK) >> SLABNUM_ADDR_SHIFT_BITS;
        let sc = (p_addr & SC_BITS_ADDR_MASK) >> NUM_SN_D_T_BITS;
        let slotnum = (p_addr & SN_D_ADDR_MASK as usize) >> sc;
        debug_assert!(slabnum < NUM_SLABS as usize);

        (sc as u8, slabnum as u8, slotnum as u32)
    }

}

fn help_numslots(sc: u8) -> u32 {
    1 << (NUM_SN_BITS - (sc - NUM_UNUSED_SCS))
}

fn help_slotsize(sc: u8) -> usize {
    help_pow2_usize(sc) - FREE_SLOT_METADATA_BYTES
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
