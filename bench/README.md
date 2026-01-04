# Count Lines of Code

This is the one of the main measurements that I was optimizing for.

 * smalloc core: 286
 * smalloc core + smalloc-ffi: 634
 * rpmalloc: 2,509
 * glibc: 7,384
 * mimalloc: 9,949
 * snmalloc: 12,728
 * jemalloc: 25,713

To count lines of code in various memory allocators using my methodology (mostly just excluding
debug asserts), run [count-locs.sh](count-locs.sh). See an example output in
[results/count-locs.output.txt](results/count-locs.output.txt).

# Smalloc's bench tool

Smalloc comes with a "micro-benchmarking" tool, used to measure smalloc's performance at a low level, which can also compare to low-level measurements of other allocators. Build it with

```
cargo build --release --package bench
```

Run it with 

```
./target/release/bench
```

You can optionally add the `--compare` or `--thorough` flags or both.

# Benchmarking smalloc in real code

Here are some ways I've benchmarked smalloc to see the effect it has on performance of other code,
and also to compare it to the default allocator, mimalloc, rpmalloc, snmalloc, and jemalloc.

## Rust simd-json

Get this fork of the Rust simd-json repo: https://github.com/zooko/simd-json and run the
[bench-allocators.sh](https://github.com/zooko/simd-json/blob/26a671f60228123cb5b6dd1a8da136dff6523244/bench-allocators.sh)
script. [Example output](results/simd-json.output.txt).

* Rust regex as benchmarked by rebar (https://github.com/zooko/rebar)

Get this fork of the Rust rebar repo: https://github.com/zooko/simd-json and run the
[bench-allocators.sh](https://github.com/zooko/simd-json/blob/26a671f60228123cb5b6dd1a8da136dff6523244/bench-allocators.sh)
script. [Example output](results/simd-json.output.txt).

```code
```

* mimalloc-bench (https://github.com/daanx/mimalloc-bench)

* C++ simdjson (https://github.com/zooko/simdjson)

```code
git clone https://github.com/simdjson/simdjson
cd simdjson
cmake -B build -DCMAKE_BUILD_TYPE=Release -DSIMDJSON_DEVELOPER_MODE=ON
cmake --build build --config Release
TYP=ondemand ; ALLOC=default ; for i in {1..3} ; do ./build/benchmark/bench_${TYP} --benchmark_format=csv --benchmark_filter="simdjson_${TYP}" --benchmark_out=o.${TYP}.${ALLOC}.${i}.csv ; done
TYP=ondemand ; ALLOC=smalloc ; for i in {1..3} ; do LD_PRELOAD=${PATH_TO_DYNLIB}/libsmalloc_ffi.so ./build/benchmark/bench_${TYP} --benchmark_format=csv --benchmark_filter="simdjson_${TYP}" --benchmark_out=o.${TYP}.${ALLOC}.${i}.csv ; done
TYP=ondemand ; ALLOC=mimalloc ; for i in {1..3} ; do LD_PRELOAD=${PATH_TO_DYNLIB}/libmimalloc.so ./build/benchmark/bench_${TYP} --benchmark_format=csv --benchmark_filter="simdjson_${TYP}" --benchmark_out=o.${TYP}.${ALLOC}.${i}.csv ; done
```

