
#[cfg(any(target_os = "linux", doc))]
pub type ClockType = i32;

#[cfg(any(target_vendor = "apple", doc))]
pub type ClockType = u32;


