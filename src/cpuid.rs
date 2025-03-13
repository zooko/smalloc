// Thanks to the authors of rust-cpuid:
// https://github.com/gz/rust-cpuid/blob/master/src/lib.rs and Brendan
// on this site: https://forum.osdev.org/viewtopic.php?t=23445
// (https://web.archive.org/web/20250223222749/https://forum.osdev.org/viewtopic.php?t=23445)
// ...and the Brave AI. 8-)

//xxxuse std::arch::x86_64::{__cpuid_count};
//xxx
//xxx#[derive(Debug)]
//xxxpub enum Vendor {
//xxx    Intel,
//xxx    Amd,
//xxx    Unknown,
//xxx}
//xxx
//xxxpub fn get_vendor_info() -> Vendor {
//xxx    let cpuid = unsafe { __cpuid_count(0x0, 0x0) };
//xxx    if cpuid.ebx == 0x68747541 && cpuid.edx == 0x69746e65 && cpuid.ecx == 0x444d4163 {
//xxx	Vendor::Amd
//xxx    } else if cpuid.ebx == 0x756e6547 && cpuid.edx == 0x49656e69 && cpuid.ecx == 0x6c65746e {
//xxx	Vendor::Intel
//xxx    } else {
//xxx        Vendor::Unknown
//xxx    }
//xxx}

