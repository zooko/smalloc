use crate::*; 
use std::alloc::Layout;
use std::alloc::GlobalAlloc;

/// If we've allocated all the slots from a slab, the next allocation of that sizeclass comes
/// from a different slab of the same sizeclass. This test doesn't work for the largest
/// sizeclass simply because the test assumes you can allocate 2 slots...
fn help_test_overflow_to_other_slab(sc: u8) {
    debug_assert!(sc >= NUM_UNUSED_SCS, "{sc}");
    debug_assert!(sc <= (NUM_SCS - 2)); // This test code needs at least 3 slots.

    let sm = get_testsmalloc!();

    let siz = help_slotsize(sc);
    let l = Layout::from_size_align(siz, 1).unwrap();

    let slabnum = const_shr_usize_u8(get_slab_num(), SLABNUM_FLH_SHIFT_BITS);

    let numslots = help_numslots(sc);
    debug_assert!(numslots >= 3);

    // Step 0: reach into the slab's `flh` and set it to almost the max slot number.
    let first_i = numslots - 3;
    debug_assert!(first_i <= u32::MAX as usize);

    let mut i = first_i;
    sm.help_set_flh_singlethreaded(sc, i as u32, slabnum);

    // Step 1: allocate a slot and store it in local variables:
    let p1 = unsafe { sm.alloc(l) };
    assert!(!p1.is_null());

    let (sc1, slabnum1, slotnum1) = sm.help_ptr_to_loc(p1);
    assert_eq!(sc1, sc);
    assert_eq!(slotnum1 as usize, i);

    i += 1;
    
    // Step 2: allocate all the rest of the slots in this slab except the last one:
    while i < numslots - 2 {
        let pt = unsafe { sm.alloc(l) };
        assert!(!pt.is_null());

        let (scn, _slabnumn, slotnumn) = sm.help_ptr_to_loc(pt);
        assert_eq!(scn, sc);
        assert_eq!(slotnumn as usize, i);

        i += 1;
    }

    // Step 3: allocate the last slot in this slab and store it in local variables:
    let p2 = unsafe { sm.alloc(l) };
    assert!(!p2.is_null());

    let (sc2, slabnum2, slotnum2) = sm.help_ptr_to_loc(p2);
    // Assert some things about the two stored slot locations:
    assert_eq!(sc2, sc, "numslots: {numslots}, i: {i}");
    assert_eq!(slabnum1, slabnum2);
    assert_eq!(slotnum2 as usize, numslots - 2);

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
    let p4 = unsafe { get_testsmalloc!().alloc(l) };
    assert!(!p4.is_null(), "sc3: {sc3}, sc: {sc}, slabnum3: {slabnum3}, slabnum1: {slabnum1}, p3: {p3:?}, p2: {p2:?}, slotnum3: {slotnum3}");

    let (sc4, slabnum4, slotnum4) = sm.help_ptr_to_loc(p4);

    assert_eq!(sc4, sc3);
    assert_eq!(slabnum4, slabnum3);
    assert_eq!(slotnum4, 1);
}

const NUM_SLABS: u8 = const_one_shl_u8(NUM_SLABS_BITS);

/// This test doesn't work on the biggest sizeclass (sc 31).
fn help_test_overflow_to_other_sizeclass_once(sc: u8) {
    debug_assert!(sc >= NUM_UNUSED_SCS, "{sc}");
    debug_assert!(sc < NUM_SCS - 1);

    let sm = get_testsmalloc!();

    let siz = help_slotsize(sc);
    let l = Layout::from_size_align(siz, 1).unwrap();
    let numslots = help_numslots(sc);
    let slabnum = const_shr_usize_u8(get_slab_num(), SLABNUM_FLH_SHIFT_BITS);

    // Step 0: allocate a slot and store information about it in local variables:
    let p1 = unsafe { sm.alloc(l) };
    assert!(!p1.is_null());

    let (sc1, slabnum1, _slotnum1) = sm.help_ptr_to_loc(p1);

    assert_eq!(sc1, sc);
    assert_eq!(slabnum1, slabnum);

    // Step 1: reach into each slab's `flh` and set it to the max slot number (which means the
    // free list is empty).
    for slabnum in 0..NUM_SLABS {
        sm.help_set_flh_singlethreaded(sc, (numslots - 1) as u32, slabnum);
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

    let sm = get_testsmalloc!();

    let siz = help_slotsize(sc);
    let l = Layout::from_size_align(siz, 1).unwrap();
    let numslots = help_numslots(sc);
    let slabnum = (get_slab_num() >> SLABNUM_FLH_SHIFT_BITS) as u8;

    // Step 0: allocate a slot and store information about it in local variables:
    let p1 = unsafe { sm.alloc(l) };
    assert!(!p1.is_null());

    let (sc1, slabnum1, _slotnum1) = sm.help_ptr_to_loc(p1);

    assert_eq!(sc1, sc);
    assert_eq!(slabnum1, slabnum);

    // Step 1: reach into each slab's `flh` and set it to the max slot number (which means the
    // free list is empty).
    for slabnum in 0..NUM_SLABS {
        sm.help_set_flh_singlethreaded(sc, (numslots - 1) as u32, slabnum);
    }

    // Step 2: reach into each slab's `flh` of the *next* sizeclass and set it to the max slot
    // number (which means the free list is empty).
    let sc_next = sc + 1;
    let numslots_next = help_numslots(sc_next);
    for slabnum in 0..NUM_SLABS {
        sm.help_set_flh_singlethreaded(sc_next, (numslots_next - 1) as u32, slabnum);
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

    let sm = get_testsmalloc!();

    let siz = help_slotsize(sc);
    let l = Layout::from_size_align(siz, 1).unwrap();
    let numslots = help_numslots(sc);
    let slabnum = (get_slab_num() >> SLABNUM_FLH_SHIFT_BITS) as u8;

    // Step 0: allocate a slot and store information about it in local variables:
    let p1 = unsafe { sm.alloc(l) };
    assert!(!p1.is_null());

    let (sc1, slabnum1, _slotnum1) = sm.help_ptr_to_loc(p1);

    assert_eq!(sc1, sc, "sc1: {sc1}, sc: {sc}, slabnum1: {slabnum1}, p1: {p1:?}, siz: {siz}");
    assert_eq!(slabnum1, slabnum);

    // Step 1: reach into each slab's `flh` and set it to the max slot number (which means the
    // free list is empty).
    for slabnum in 0..NUM_SLABS {
        sm.help_set_flh_singlethreaded(sc, (numslots - 1) as u32, slabnum);
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
        sm.help_set_flh_singlethreaded(sc, (numslots - 1) as u32, slabnum);
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
/// slabareanum is the same as this thread num.
fn help_alloc_four_times_singlethreaded(sm: &Smalloc, reqsize: usize, reqalign: usize) {
    assert!(reqsize > 0);
    assert!(reqsize <= help_slotsize(NUM_SCS - 1));
    assert!(reqalign > 0);

    let l = Layout::from_size_align(reqsize, reqalign).unwrap();

    let orig_slabareanum = const_shr_usize_u8(get_slab_num(), SLABNUM_FLH_SHIFT_BITS);

    let p1 = unsafe { sm.alloc(l) };
    assert!(!p1.is_null(), "l: {l:?}");

    let (sc1, slabnum1, slotnum1) = sm.help_ptr_to_loc(p1);
    assert!(help_slotsize(sc1) >= reqsize);
    assert_eq!(slabnum1, orig_slabareanum);

    let p2 = unsafe { sm.alloc(l) };
    assert!(!p2.is_null());

    let (sc2, slabnum2, slotnum2) = sm.help_ptr_to_loc(p2);
    assert!(help_slotsize(sc2) >= reqsize);
    assert_eq!(slabnum2, slabnum1, "p1: {p1:?}, p2: {p2:?}, slabnum1: {slabnum1}, slabnum2: {slabnum2}, slotnum1: {slotnum1}, slotnum2: {slotnum2}");
    assert_eq!(slabnum2, orig_slabareanum);

    let p3 = unsafe { sm.alloc(l) };
    assert!(!p3.is_null());

    let (sc3, slabnum3, _slotnum3) = sm.help_ptr_to_loc(p3);
    assert!(help_slotsize(sc3) >= reqsize);
    assert_eq!(slabnum3, slabnum1);
    assert_eq!(slabnum3, orig_slabareanum);

    // Now free the middle one.
    unsafe { sm.dealloc(p2, l) };

    // And allocate another one.
    let p4 = unsafe { sm.alloc(l) };
    assert!(!p4.is_null());

    let (sc4, slabnum4, slotnum4) = sm.help_ptr_to_loc(p4);
    assert!(help_slotsize(sc4) >= reqsize);
    assert_eq!(slabnum4, slabnum1);
    assert_eq!(slabnum4, orig_slabareanum);

    // It should have allocated slot num 2 again
    assert_eq!(slotnum4, slotnum2);

    // Clean up so that we don't run out of slots while running these tests.
    unsafe { sm.dealloc(p1, l); }
    unsafe { sm.dealloc(p3, l); }
    unsafe { sm.dealloc(p4, l); }
}

fn highest_slotnum(sc: u8) -> u32 {
    const_gen_mask_u32(NUM_SLOTNUM_AND_DATA_BITS - sc)
}


nextest_unit_tests! {
    fn test_req_to_sc() {
        let test_cases = [
            (1, 1, 3),
            (2, 1, 3),
            (3, 1, 3),
            (4, 1, 3),
            (5, 1, 3),
            (7, 1, 3),
            (8, 1, 3),
            (9, 1, 4),

            (1, 2, 3),
            (2, 2, 3),
            (3, 2, 3),
            (4, 2, 3),
            (5, 2, 3),
            (7, 2, 3),
            (8, 2, 3),
            (9, 2, 4),

            (1, 4, 3),
            (2, 4, 3),
            (3, 4, 3),
            (4, 4, 3),
            (5, 4, 3),
            (7, 4, 3),
            (8, 4, 3),
            (9, 4, 4),

            (1, 8, 3),
            (2, 8, 3),
            (3, 8, 3),
            (4, 8, 3),
            (5, 8, 3),
            (7, 8, 3),
            (8, 8, 3),
            (9, 8, 4),

            (1, 16, 4),
            (2, 16, 4),
            (3, 16, 4),
            (4, 16, 4),
            (5, 16, 4),
            (7, 16, 4),
            (8, 16, 4),
            (9, 16, 4),
            (15, 16, 4),
            (16, 16, 4),
            (17, 16, 5),

            (2usize.pow(31), 4, 31),
            (4, 2usize.pow(31), 31),
        ];

        for (reqsiz, reqali, sc) in test_cases {
            assert_eq!(req_to_sc(reqsiz, reqali), sc, "req_to_sc({reqsiz}, {reqali}) should equal {sc}");
        }
    }

    fn a_few_allocs_and_a_dealloc_for_the_largest_slab() {
        let sm = get_testsmalloc!();

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
        for sc in [3, 5, 7, 9, 11, 13, 15, 17, 19, 21, 23, 25, 27, 29] {
            help_test_overflow_to_other_sizeclass_twice_in_a_row(sc);
        }
    }

    /// If we've allocated all of the slots from all slabs of this sizeclass and the next sizeclass,
    /// the subsequent allocations come from *next* next sizeclass
    fn overflow_to_other_sizeclass_twice_at_once() {
       // Have to skip every other sc since it was used up by the previous iteration of the tests...
        for sc in [3, 5, 7, 9, 11, 13, 15, 17, 19, 21, 23, 25, 27, 29] {
            help_test_overflow_to_other_sizeclass_twice_at_once(sc);
        }
    }

    /// If we've allocated all of the slots from all of the largest large-slots slabs, the next
    /// allocation will fail.
    fn overflow_from_all_largest_large_slots_slabs() {
        let sm = get_testsmalloc!();

        let sc = NUM_SCS - 1;
        let siz = help_slotsize(sc);
        let l = Layout::from_size_align(siz, 1).unwrap();

        let highestslotnum = highest_slotnum(sc);
        
        // Step 0: reach into each slab's `flh` and set it to the max slot number.
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
        let sm = get_testsmalloc!();

        let sc = NUM_SCS - 1;
        let siz = help_slotsize(sc);
        let l = Layout::from_size_align(siz, 1).unwrap();

        let highestslotnum = highest_slotnum(sc);

        // Step 0: reach into the current slab's `flh` and set it to the max slot number.
        let slabnum = const_shr_usize_u8(get_slab_num(), SLABNUM_FLH_SHIFT_BITS);
        sm.help_set_flh_singlethreaded(sc, highestslotnum, slabnum);

        // Step 1: allocate a slot
        let p1 = unsafe { sm.alloc(l) };
        assert!(!p1.is_null(), "p1: {p1:?}, sc: {sc}, l: {l:?}");
        let (sc1, _slabnum1, _slotnum1) = sm.help_ptr_to_loc(p1);
        assert_eq!(sc1, sc);
    }

    fn slotnum_encode_and_decode_roundtrip() {
        for sc in [ 31, 30, 25, 12, 9, 3 ] {
            let slotnum_unit = const_one_shl_u64(sc);
            let numslots = help_numslots(sc);
            assert!(numslots > 0, "sc: {sc}");
            assert!(numslots <= 2usize.pow(32), "sc: {sc}");
            
            let slotnums = [ 0, 1, 2, 3, 4, numslots.wrapping_sub(4), numslots.wrapping_sub(3), numslots.wrapping_sub(2), numslots.wrapping_sub(1) ];
            for slotnum1 in slotnums {
                assert!(slotnum1 < 2usize.pow(32));
                let slotnum1 = slotnum1 as u32;
                for slotnum2 in slotnums {
                    assert!(slotnum2 < 2usize.pow(32));
                    let sslotnum1 = const_shl_u32_u64(slotnum1, sc);
                    let slotnum2 = slotnum2 as u32;
                    let sslotnum2 = const_shl_u32_u64(slotnum2, sc);
                    if slotnum1 < (numslots - 1) as u32 && (slotnum2 as usize) < numslots && slotnum1 != slotnum2 {
                        let ence = Smalloc::encode_next_entry_link(sslotnum1, sslotnum2, slotnum_unit);
                        let dece = Smalloc::decode_next_entry_link(sslotnum1, ence, slotnum_unit);
                        assert_eq!(sslotnum2, dece, "slotnum1: {slotnum1}, ence: {ence}, slotnum_unit: {slotnum_unit}");
                    }
                }
            }
        }
    }

    fn a_few_allocs_and_a_dealloc_for_each_slab() {
        // Doesn't work for the largest size class (sc 31) because there aren't 3 slots.
        let sm = get_testsmalloc!();

        for sc in NUM_UNUSED_SCS..NUM_SCS - 1 {
            help_alloc_diff_size_and_alignment_singlethreaded(sm, sc);
        }
    }
}

use std::sync::atomic::Ordering::Relaxed;
impl Smalloc {
    fn help_set_flh_singlethreaded(&self, sc: u8, slotnum: u32, slabnum: u8) {
        debug_assert!(sc >= NUM_UNUSED_SCS, "{sc}");
        debug_assert!(sc < NUM_SCS);

        let inner = self.inner();

        let flhi = NUM_SCS as u16 * slabnum as u16 + sc as u16;
        let flhptr = inner.smbp | const_shl_u16_usize(flhi, 3);
        let flha = unsafe { AtomicU64::from_ptr(flhptr as *mut u64) };
        //xxxeprintln!("xxx 4 flhptr: {flhptr:x}, {:x}", const_shl_u32_u64(slotnum, sc));

        // single threaded so don't bother with the counter
        flha.store(const_shl_u32_u64(slotnum, sc), Relaxed);
    }

    /// Return the sizeclass, slabnum, and slotnum
    fn help_ptr_to_loc(&self, ptr: *const u8) -> (u8, u8, u32) {
        let inner = self.inner();

        let p_addr = ptr.addr();

        assert!((p_addr >= inner.smbp) && (p_addr <= inner.smbp + HIGHEST_SMALLOC_SLOT_ADDR));

        let sc = const_shr_usize_u8(p_addr & SC_BITS_ADDR_MASK, SC_ADDR_SHIFT_BITS);
        let slabnum = const_shr_usize_u8(p_addr & SLABNUM_ADDR_MASK, NUM_SLOTNUM_AND_DATA_BITS + NUM_SC_BITS);
        let slotnum = const_shr_u64_u32(p_addr as u64 & SLOTNUM_AND_DATA_ADDR_MASK, sc);

        (sc, slabnum, slotnum)
    }

}

fn help_numslots(sc: u8) -> usize {
    help_pow2_usize(NUM_SLOTNUM_AND_DATA_BITS - sc)
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

// xxx benchmark and inspect asm for this vs <<
#[inline(always)]
const fn _const_shl_u32_usize(value: u32, shift: u8) -> usize {
    debug_assert!((shift as u32) < usize::BITS);
    debug_assert!(hlzu(value as usize) >= shift); // we never shift off 1 bits currently
    unsafe { (value as usize).unchecked_shl(shift as u32) }
}

#[inline(always)]
const fn const_shr_u64_u32(value: u64, shift: u8) -> u32 {
    debug_assert!((shift as u32) < u64::BITS);
    let res = unsafe { value.unchecked_shr(shift as u32) };
    // No leaving 1 bits stranded up there
    debug_assert!(help_leading_zeros_u64(res) as u32 >= u64::BITS - u32::BITS);
    res as u32
}

// xxx benchmark and inspect asm for this vs <<
#[inline(always)]
const fn _const_shl_u8_u64(value: u8, shift: u8) -> u64 {
    debug_assert!((shift as u32) < u64::BITS);
    debug_assert!(help_leading_zeros_u64(value as u64) >= shift); // we never shift off 1 bits currently
    unsafe { (value as u64).unchecked_shl(shift as u32) }
}

