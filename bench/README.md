# Smalloc's bench tool

`smalloc` comes with a "micro-benchmarking" tool, used to measure `smalloc`'s performance at a low
level, which can also compare to low-level measurements of other allocators. Run it with

```
./runbench.sh
```

It produces output that looks like this:

```text
name:     de_mt_aww-64, threads:    64, iters:      2000, ns:      1,619,875, ns/i:       809.9
name:     je_mt_aww-64, threads:    64, iters:      2000, ns:     27,746,750, ns/i:    13,873.3
name:     sn_mt_aww-64, threads:    64, iters:      2000, ns:      2,705,875, ns/i:     1,352.9
name:     mi_mt_aww-64, threads:    64, iters:      2000, ns:      3,795,125, ns/i:     1,897.5
name:     rp_mt_aww-64, threads:    64, iters:      2000, ns:      1,626,084, ns/i:       813.0
name:     sm_mt_aww-64, threads:    64, iters:      2000, ns:        736,500, ns/i:       368.2
smalloc diff from  default:  -55%
smalloc diff from jemalloc:  -97%
smalloc diff from snmalloc:  -73%
smalloc diff from mimalloc:  -81%
smalloc diff from rpmalloc:  -55%
```

You can also pass `--thorough` on the command-line to exercise more cases, including "worst-case
scenario" cases that stress-test specific parts of `smalloc`'s design.

To see benchmarks of real-world Rust code with different allocators, complete with pretty graphs,
see
https://github.com/zooko/bench-allocators/blob/main/benchmark-results/AppleM4Max.darwin25/COMBINED-REPORT.md
.

## Your Code Here

Make a script that runs benchmarks against your codebase, possibly following the examples of
https://github.com/zooko/rebar/blob/master/bench-allocators.sh and
https://github.com/zooko/simd-json/blob/master/bench-allocators.sh
(if your code is in Rust) or
https://github.com/zooko/mimalloc-bench/blob/master/bench-allocators.sh
(if your code is in C/C++/Zig/etc) and publish them and let me know by opening an issue or a
pull-request!
