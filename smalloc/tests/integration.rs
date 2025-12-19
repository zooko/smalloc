use devutils::*;

use std::alloc::Layout;
use std::alloc::GlobalAlloc;

use devutils::nextest_integration_tests;
use devutils::get_devsmalloc;

nextest_integration_tests! {
    fn one_alloc_and_dealloc_medium() {
        let l = Layout::from_size_align(120, 4).unwrap();
        let p = unsafe { get_devsmalloc!().alloc(l) };
        assert!(!p.is_null());
        unsafe { get_devsmalloc!().dealloc(p, l) };
    }

    fn one_realloc_to_tiny() {
        let l = Layout::from_size_align(1, 1).unwrap();
        let p = unsafe { get_devsmalloc!().alloc(l) };
        let p2 = unsafe { get_devsmalloc!().realloc(p, l, 3) };
        debug_assert_eq!(p, p2);
    }

    fn one_alloc_and_dealloc_small() {
        let l = Layout::from_size_align(6, 1).unwrap();
        let p = unsafe { get_devsmalloc!().alloc(l) };
        unsafe { get_devsmalloc!().dealloc(p, l) };
    }

    fn one_alloc_and_dealloc_large() {
        let l = Layout::from_size_align(1_000_000, 8).unwrap();
        let p = unsafe { get_devsmalloc!().alloc(l) };
        unsafe { get_devsmalloc!().dealloc(p, l) };
    }

    fn one_large_alloc_and_realloc_to_large() {
        let l1 = Layout::from_size_align(1_000_000, 8).unwrap();
        let p1 = unsafe { get_devsmalloc!().alloc(l1) };
        assert!(!p1.is_null());

        let reqsize: usize = 2_000_000;
        let p2 = unsafe { get_devsmalloc!().realloc(p1, l1, reqsize) };
        assert!(!p2.is_null());
    }

    fn one_medium_alloc_and_realloc_to_large() {
        let l1 = Layout::from_size_align(300, 8).unwrap();
        let p1 = unsafe { get_devsmalloc!().alloc(l1) };
        assert!(!p1.is_null());

        let reqsize: usize = 2_000_000;
        let p2 = unsafe { get_devsmalloc!().realloc(p1, l1, reqsize) };
        assert!(!p2.is_null());
    }

    fn one_large_alloc_and_realloc_to_oversize() {
        let l1 = Layout::from_size_align(1_000_000, 8).unwrap();
        let p1 = unsafe { get_devsmalloc!().alloc(l1) };
        assert!(!p1.is_null());

        let reqsize: usize = 100_000_000;
        let p2 = unsafe { get_devsmalloc!().realloc(p1, l1, reqsize) };
        assert!(!p2.is_null());
    }

    fn one_alloc_slot_large() {
        let l = Layout::from_size_align(1_000_000, 8).unwrap();
        unsafe { get_devsmalloc!().alloc(l) };
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

    // xxx consider reducing the code size of these tests...

    fn test_alloc_1_byte_then_dealloc() {
        let layout = Layout::from_size_align(1, 1).unwrap();
        let p = unsafe { get_devsmalloc!().alloc(layout) };
        assert!(!p.is_null());
        unsafe { get_devsmalloc!().dealloc(p, layout) };
    }

    fn threads_1_small_alloc_x() {
        help_test_multithreaded(1, 100, false, false, false);
    }

    fn threads_1_small_alloc_dealloc_x() {
        help_test_multithreaded(1, 100, true, false, false);
    }

    fn threads_1_small_alloc_dealloc_realloc_x() {
        help_test_multithreaded(1, 100, true, true, false);
    }

    fn threads_1_small_alloc_dealloc_with_writes_x() {
        help_test_multithreaded(1, 100, true, false, true);
    }

    fn threads_1_small_alloc_dealloc_realloc_with_writes_x() {
        help_test_multithreaded(1, 100, true, true, true);
    }

    fn threads_2_small_alloc_x() {
        help_test_multithreaded(2, 100, false, false, false);
    }

    fn threads_2_small_alloc_dealloc_x() {
        help_test_multithreaded(2, 100, true, false, false);
    }

    fn threads_2_small_alloc_dealloc_realloc_x() {
        help_test_multithreaded(2, 100, true, true, false);
    }

    fn threads_2_small_alloc_dealloc_with_writes_x() {
        help_test_multithreaded(2, 100, true, false, true);
    }

    fn threads_2_small_alloc_dealloc_realloc_with_writes_x() {
        help_test_multithreaded(2, 100, true, true, true);
    }

    fn threads_32_small_alloc_x() {
        help_test_multithreaded(32, 100, false, false, false);
    }

    fn threads_32_small_alloc_dealloc_x() {
        help_test_multithreaded(32, 100, true, false, false);
    }

    fn threads_32_small_alloc_dealloc_realloc_x() {
        help_test_multithreaded(32, 100, true, true, false);
    }

    fn threads_32_small_alloc_dealloc_with_writes_x() {
        help_test_multithreaded(32, 100, true, false, true);
    }

    fn threads_32_small_alloc_dealloc_realloc_with_writes_x() {
        help_test_multithreaded(32, 100, true, true, true);
    }

    fn threads_64_small_alloc_x() {
        help_test_multithreaded(64, 100, false, false, false);
    }

    fn threads_64_small_alloc_dealloc_x() {
        help_test_multithreaded(64, 100, true, false, false);
    }

    fn threads_64_small_alloc_dealloc_realloc_x() {
        help_test_multithreaded(64, 100, true, true, false);
    }

    fn threads_64_small_alloc_dealloc_with_writes_x() {
        help_test_multithreaded(64, 100, true, false, true);
    }

    fn threads_64_small_alloc_dealloc_realloc_with_writes_x() {
        help_test_multithreaded(64, 100, true, true, true);
    }

    fn threads_1_medium_alloc_x() {
        help_test_multithreaded(1, 100, false, false, false);
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

// One sentinel test (not ignored) - prints error if run under cargo test
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

