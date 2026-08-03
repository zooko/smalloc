# Smalloc's bench tool

`smalloc` comes with a "micro-benchmarking" tool, used to measure `smalloc`'s performance at a low
level, which can also compare to low-level measurements of other allocators. Build it with 

```
cargo build --package bench --release
```

and run it with

```
./target/release/bench
```

It produces output that looks like this:

```text
Using seed: 0
Using base iteration count: 10 k
name:    sm_st_adrww-1, threads:     1, iters:        1 M, ns:      8,588,042, ns/i:         8.5
name:    de_st_adrww-1, threads:     1, iters:        1 M, ns:     17,012,042, ns/i:        17.0
smalloc diff from  default:  -50%

name:     sm_st_adww-1, threads:     1, iters:        1 M, ns:      8,896,542, ns/i:         8.8
name:     de_st_adww-1, threads:     1, iters:        1 M, ns:     15,295,125, ns/i:        15.2
smalloc diff from  default:  -42%

name:      sm_st_aww-1, threads:     1, iters:       10 k, ns:         33,541, ns/i:         3.3
name:      de_st_aww-1, threads:     1, iters:       10 k, ns:         72,583, ns/i:         7.2
smalloc diff from  default:  -54%

name:   de_mt_adrww-32, threads:    32, iters:      100 k, ns:     29,051,833, ns/i:       290.5
name:   sm_mt_adrww-32, threads:    32, iters:      100 k, ns:      2,992,000, ns/i:        29.9
smalloc diff from  default:  -90%

name:    de_mt_adww-32, threads:    32, iters:      100 k, ns:     19,334,500, ns/i:       193.3
name:    sm_mt_adww-32, threads:    32, iters:      100 k, ns:      3,237,750, ns/i:        32.3
smalloc diff from  default:  -83%

name:     de_mt_aww-32, threads:    32, iters:       10 k, ns:      2,639,958, ns/i:       263.9
name:     sm_mt_aww-32, threads:    32, iters:       10 k, ns:        404,916, ns/i:        40.4
smalloc diff from  default:  -85%
```

You can optionally pass `--features=mimalloc,snmalloc,jemalloc,rpmalloc` (or any subset of them) on
the build command line, to compare smalloc's performance to that of those other allocators.

You can pass `--smalloc-only` on the command-line to skip all the other allocators. You can pass
`--thorough` on the command-line to exercise more cases, including "worst-case scenario" cases that
stress-test specific parts of `smalloc`'s design.

To see benchmarks of real-world Rust code with different allocators, complete with pretty graphs,
see
https://github.com/zooko/bench-allocators/blob/main/benchmark-results/AppleM4Max.darwin25/COMBINED-REPORT.md
.
