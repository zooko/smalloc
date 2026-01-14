use devutils::*;

use std::alloc::Layout;
use std::alloc::GlobalAlloc;

use devutils::nextest_integration_tests;
use devutils::get_devsmalloc;

use smmalloc::plat;

fn assert_all_bytes_val(ptr: *mut u8, numbytes: usize, val: u8, skipfirst: usize) {
    unsafe {
        let slice = std::slice::from_raw_parts(ptr, numbytes);
        for (i, &byte) in slice.iter().enumerate() {
            if i >= skipfirst {
                assert_eq!(byte, val, "Byte at offset {} is 0x{:02x}, expected {}", i, byte, val);
            }
        }
    }
}

nextest_integration_tests! {
    fn zeromem_first_time() {
        let sm = get_devsmalloc!();

        // You tell smalloc to zero the mem when allocating a slot that has never been allocated
        // before. The resulting mem is all zeroes.

        // more than one page just in case there's a bug in there somewhere that zeroes out the
        // first page but not the second one.
        let numbytes = 1 << (plat::p::SC_FOR_PAGE + 1);

        let l = Layout::from_size_align(numbytes, 1).unwrap();
        let p = unsafe { sm.alloc_zeroed(l) };

        assert_all_bytes_val(p, numbytes, 0, 0);
    }
    
    fn zeromem_second_time() {
        let sm = get_devsmalloc!();

        // You tell smalloc to zero the mem when allocating a slot that you've previously allocated
        // and wrote into but then freed. The resulting mem is all zeroes.

        // more than one page just in case there's a bug in there somewhere that zeroes out the
        // first page but not the second one.
        let numbytes = 1 << (plat::p::SC_FOR_PAGE + 1);

        let l = Layout::from_size_align(numbytes, 1).unwrap();
        let p1 = unsafe { sm.alloc_zeroed(l) };
        // fill it with 1 bytes
        unsafe { core::ptr::write_bytes(p1, 1, numbytes) };
        let p2 = unsafe { sm.alloc_zeroed(l) };
        // fill it with 2 bytes
        unsafe { core::ptr::write_bytes(p1, 2, numbytes) };

        // dealloc the 2nd one
        unsafe { sm.dealloc(p2, l) };
        // dealloc the 1st one
        unsafe { sm.dealloc(p1, l) };

        // allocate again -- due to smalloc's implementation it'll give us back p1.
        let p3 = unsafe { sm.alloc_zeroed(l) };

        // In current smalloc, this is always going to give you the same pointer as before. If it
        // doesn't that's not indicative a bug in the underlying allocator! It does, however, mean
        // that this test isn't working as intended.
        if p3 != p1 {
            panic!("cannot re-allocate memory previously used - test cannot run");
        }

        assert_all_bytes_val(p3, numbytes, 0, 0);
    }
    
    fn dont_zeromem() {
        let sm = get_devsmalloc!();

        // You don't tell smalloc to zero the mem when allocating a slot that you've previously
        // allocated and wrote into but then freed. The resulting mem is not all zeroes.

        // more than one page just in case there's a bug in there somewhere that zeroes out the
        // first page but not the second one.
        let numbytes = 1 << (plat::p::SC_FOR_PAGE + 1);

        // Note: there's not really anything wrong if you run this test against a different memory
        // allocator than smalloc and this test fails. That just means the memory allocator zeroed
        // the memory unnecessarily -- which is harmless except for possible performance
        // consequences -- or else it gave you different allocation on the second time (which is a
        // normal thing for an allocator to do as a security measure. So maybe we should move this
        // test to the "transparent-box tests" (./smalloc/src/tests.rs) instead of these "opaque-box
        // tests", since the test could give a false alarm if the underlying code under test is not
        // actually the current smalloc implementation...

        // more than one page just in case there's a bug in there somewhere that zeroes out one page
        // but not the other.
        let l = Layout::from_size_align(1 << (plat::p::SC_FOR_PAGE+1), 1).unwrap();
        let p1 = unsafe { sm.alloc(l) };

        // fill it with 1 bytes
        unsafe { core::ptr::write_bytes(p1, 1, numbytes) };

        // dealloc it
        unsafe { sm.dealloc(p1, l) };

        // alloc a space (of the same size) again
        let p2 = unsafe { sm.alloc(l) };

        // In current smalloc, this is always going to give you the same pointer as before. If it
        // doesn't that's not indicative a bug in the underlying allocator! It does, however, mean
        // that this test isn't working as intended.
        if p1 != p2 {
            panic!("cannot re-allocate memory previously used - test cannot run");
        }

        assert_all_bytes_val(p2, numbytes, 1, 4);
    }

    /// This reproduces a bug in the Windows support in which dealloc incorrectly marked a slot as
    /// COMMITted.
    fn pop_push_pop_pop() {
        let sm = get_devsmalloc!();
        let l = Layout::from_size_align(65536, 1).unwrap();
        let p = unsafe { sm.alloc(l) };
        unsafe { sm.dealloc(p, l) };
        unsafe { sm.alloc(l) };
        unsafe { sm.alloc(l) };
    }

    /// This reproduces a bug in `platform::plat::sys_realloc()` /
    /// `_sys_realloc_if_vm_remap_did_what_i_want()` (or possibly in MacOS's `mach_vm_remap()`) that
    /// was uncovered by tests::threads_1_large_alloc_dealloc_realloc_x()
    fn large_realloc_down_realloc_back_up() {
        const LARGE_SLOT_SIZE: usize = 2usize.pow(24);

        let l1 = Layout::from_size_align(LARGE_SLOT_SIZE * 2, 1).unwrap();
        let l2 = Layout::from_size_align(LARGE_SLOT_SIZE, 1).unwrap();

        let p1 = unsafe { get_devsmalloc!().alloc(l1) };
        assert!(!p1.is_null());
        let p2 = unsafe { get_devsmalloc!().realloc(p1, l1, LARGE_SLOT_SIZE) };
        assert!(!p2.is_null());
        let p3 = unsafe { get_devsmalloc!().realloc(p2, l2, LARGE_SLOT_SIZE * 2) };
        assert!(!p3.is_null());
    }

    fn test_alloc_1_byte_then_dealloc() {
        let layout = Layout::from_size_align(1, 1).unwrap();
        let p = unsafe { get_devsmalloc!().alloc(layout) };
        assert!(!p.is_null());
        unsafe { get_devsmalloc!().dealloc(p, layout) };
    }

    fn threads_1_alloc_x() {
        help_test_multithreaded(1, 100, false, false, false);
    }

    fn threads_1_alloc_dealloc_x() {
        help_test_multithreaded(1, 100, true, false, false);
    }

    fn threads_1_alloc_dealloc_realloc_x() {
        help_test_multithreaded(1, 100, true, true, false);
    }

    fn threads_1_alloc_dealloc_with_writes_x() {
        help_test_multithreaded(1, 100, true, false, true);
    }

    fn threads_1_alloc_dealloc_realloc_with_writes_x() {
        help_test_multithreaded(1, 100, true, true, true);
    }

    fn threads_2_alloc_x() {
        help_test_multithreaded(2, 100, false, false, false);
    }

    fn threads_2_alloc_dealloc_x() {
        help_test_multithreaded(2, 100, true, false, false);
    }

    fn threads_2_alloc_dealloc_realloc_x() {
        help_test_multithreaded(2, 100, true, true, false);
    }

    fn threads_2_alloc_dealloc_with_writes_x() {
        help_test_multithreaded(2, 100, true, false, true);
    }

    fn threads_2_alloc_dealloc_realloc_with_writes_x() {
        help_test_multithreaded(2, 100, true, true, true);
    }

    fn threads_4096_alloc_x() {
        help_test_multithreaded(4096, 100, false, false, false);
    }

    fn threads_4096_alloc_dealloc_x() {
        help_test_multithreaded(4096, 100, true, false, false);
    }

    fn threads_4096_alloc_dealloc_realloc_x() {
        help_test_multithreaded(4096, 100, true, true, false);
    }

    fn threads_4096_alloc_dealloc_with_writes_x() {
        help_test_multithreaded(4096, 100, true, false, true);
    }

    fn threads_4096_alloc_dealloc_realloc_with_writes_x() {
        help_test_multithreaded(4096, 100, true, true, true);
    }
}

fn help_test_multithreaded(threads: u32, iters: u64, dealloc: bool, realloc: bool, writes: bool)  {
    let f = match (dealloc, realloc, writes) {
        (true, true, true) => { adrww }
        (true, true, false) => { adr }
        (true, false, true) => { adww }
        (true, false, false) => { ad }
        (false, false, true) => { aww }
        (false, false, false) => { a }
        (false, _, _) => panic!()
    };

    let mut tses: Vec<TestState> = Vec::with_capacity(threads as usize);
    for _i in 0..threads {
        tses.push(TestState::new(iters, 0));
    }

    help_test_multithreaded_with_allocator(f, threads, iters, get_devsmalloc!(), &mut tses);

    //xxx4 could consider cleaning up here -- dealloc'ing all the allocations...
}

// prints error if run under cargo test
#[test]
fn aaa_require_nextest() {
    if std::env::var("NEXTEST").is_ok() {
        return;
    }
    panic!(
        "\n\n\
         \x1b[1;31merror:\x1b[0m This project requires cargo-nextest for testing.\n\n\
         \x20   Run tests with:  \x1b[1;32mcargo nextest run\x1b[0m\n\
         \x20   Install with:    cargo install cargo-nextest\n"
    );
}

