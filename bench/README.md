# Smalloc's bench tool

`smalloc` comes with a "micro-benchmarking" tool, used to measure `smalloc`'s performance at a low
level, which can also compare to low-level measurements of other allocators. Run it with

```
./runbench.sh
```

It produces output that looks like this:

```text
name:     de_mt_aww-32, threads:    32, iters:     10000, ns:      2,702,083, ns/i:       270.2
name:     je_mt_aww-32, threads:    32, iters:     10000, ns:     19,860,125, ns/i:     1,986.0
name:     sn_mt_aww-32, threads:    32, iters:     10000, ns:      4,779,542, ns/i:       477.9
name:     mi_mt_aww-32, threads:    32, iters:     10000, ns:      3,263,583, ns/i:       326.3
name:     rp_mt_aww-32, threads:    32, iters:     10000, ns:        992,917, ns/i:        99.2
name:     sm_mt_aww-32, threads:    32, iters:     10000, ns:        399,417, ns/i:        39.9
smalloc diff from  default:  -85%
smalloc diff from jemalloc:  -98%
smalloc diff from snmalloc:  -92%
smalloc diff from mimalloc:  -88%
smalloc diff from rpmalloc:  -60%
```

You can pass `--smalloc-only` on the command-line to skip all the other allocators. You can pass
`--thorough` on the command-line to exercise more cases, including "worst-case scenario" cases that
stress-test specific parts of `smalloc`'s design.

To see benchmarks of real-world Rust code with different allocators, complete with pretty graphs,
see
https://github.com/zooko/bench-allocators/blob/main/benchmark-results/AppleM4Max.darwin25/COMBINED-REPORT.md
.
