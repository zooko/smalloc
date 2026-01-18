# Count Lines of Code

This is the one of the main measurements that I was optimizing for.

 * smalloc core: 351
 * smalloc core + smalloc-ffi: 785
 * rpmalloc: 2,509
 * glibc: 7,384
 * mimalloc: 9,949
 * snmalloc: 12,728
 * jemalloc: 25,713

To count lines of code in various memory allocators using my methodology (mostly just excluding
debug asserts), run [count-locs.sh](count-locs.sh). See an example output in
[results/count-locs.output.txt](results/count-locs.output.txt).

# Smalloc's bench tool

`smalloc` comes with a "micro-benchmarking" tool, used to measure `smalloc`'s performance at a low
level, which can also compare to low-level measurements of other allocators. Run it with

```
./runbench.sh
```

There is an example output in [bench/results/cargo-bench.result.AppleM4Max.darwin25..txt](bench/results/cargo-bench.result.AppleM4Max.darwin25..txt).

# Benchmarking user code with different allocators

Here are some ways to benchmark smalloc to see the effect it has on performance of various
codebases, and also to compare it to the default allocator, mimalloc, rpmalloc, snmalloc, and
jemalloc.

## Rust simd-json

Get this fork of the Rust simd-json repo: https://github.com/zooko/simd-json and run the
[bench-allocators.sh](https://github.com/zooko/simd-json/blob/main/bench-allocators.sh)
script. [Example output](bench/results/simd-json.result.AppleM4Max.darwin25..txt).

## Rust rebar

Get this fork of the Rust rebar repo: https://github.com/zooko/rebar and run the
[bench-allocators.sh](https://github.com/zooko/rebar/blob/master/bench-allocators.sh)
script. [Example output](bench/results/rebar.bench-allocators.result.AppleM4Max.darwin25..txt).

## mimalloc-bench

Get this fork of the mimalloc-bench repo: https://github.com/zooko/mimalloc-bench and run the
[bench-allocators.sh](https://github.com/zooko/mimalloc-bench/blob/master/bench-allocators.sh)
script. (Only works on Linux.) [Example output](bench/results/mimalloc-bench.output.txt)

## Your Code Here

Make a script that runs benchmarks against your codebase like these and submit a pull request!
