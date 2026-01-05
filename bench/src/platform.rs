#[cfg(target_os = "windows")]
pub mod p {
    use windows_sys::Win32::System::Performance::{QueryPerformanceCounter,QueryPerformanceFrequency};
    use crate::Nanoseconds;

    fn qpc() -> i64 {
        let mut counter: i64 = 0;
        let res = unsafe { QueryPerformanceCounter(&mut counter) };
        debug_assert!(res != 0);

        let mut freq: i64 = 0;
        let res = unsafe { QueryPerformanceFrequency(&mut freq) };
        debug_assert!(res != 0);

        ((counter as u128 * 1_000_000_000) / freq as u128) as i64
    }

    pub fn thread_cputime() -> Nanoseconds {
        Nanoseconds(qpc() as u64)
    }

    pub fn clock_monotonic_raw() -> Nanoseconds {
        Nanoseconds(qpc() as u64)
    }
}

#[cfg(not(target_os = "windows"))]
pub mod p {
    use crate::Nanoseconds;

    #[cfg(any(target_os = "linux", doc))]
    pub type ClockType = i32;

    #[cfg(any(target_vendor = "apple", doc))]
    pub type ClockType = u32;

    use std::mem::MaybeUninit;

    fn clock(clocktype: ClockType) -> Nanoseconds {
        let mut tp: MaybeUninit<libc::timespec> = MaybeUninit::uninit();
        let retval = unsafe { libc::clock_gettime(clocktype, tp.as_mut_ptr()) };
        debug_assert_eq!(retval, 0);
        let instsec = unsafe { (*tp.as_ptr()).tv_sec };
        let instnsec = unsafe { (*tp.as_ptr()).tv_nsec };
        debug_assert!(instsec >= 0);
        debug_assert!(instnsec >= 0);
        Nanoseconds(instsec as u64 * 1_000_000_000 + instnsec as u64)
    }

    pub fn thread_cputime() -> Nanoseconds {
        clock(libc::CLOCK_THREAD_CPUTIME_ID)
    }

    pub fn clock_monotonic_raw() -> Nanoseconds {
        clock(libc::CLOCK_MONOTONIC_RAW)
    }



}
