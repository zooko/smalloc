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

Example output:

```text
name:     de_mt_aww-32, threads:    32, iters:       2000, ns:        814,375, ns/i:      407.1
name:     mi_mt_aww-32, threads:    32, iters:       2000, ns:      1,826,500, ns/i:      913.2
name:     je_mt_aww-32, threads:    32, iters:       2000, ns:      9,878,000, ns/i:    4,939.0
name:     sn_mt_aww-32, threads:    32, iters:       2000, ns:      1,277,959, ns/i:      638.9
name:     rp_mt_aww-32, threads:    32, iters:       2000, ns:        756,750, ns/i:      378.3
name:      s_mt_aww-32, threads:    32, iters:       2000, ns:        346,541, ns/i:      173.2
smalloc diff from  default:  -57%
smalloc diff from mimalloc:  -81%
smalloc diff from jemalloc:  -96%
smalloc diff from snmalloc:  -73%
smalloc diff from rpmalloc:  -54%
```

# Lines of code

This is the one of the main measurements that I was optimizing for!

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

# Benchmarking smalloc in real code

Here are some ways I've benchmarked smalloc to see the effect it has on performance of other code,
and also to compare it to the default allocator, mimalloc, rpmalloc, snmalloc, and jemalloc.

* Rust simd-json (https://github.com/zooko/simd-json)

```text
cargo bench -- --save-baseline default 2>&1 | tee default
for AL in jemalloc mimalloc rpmalloc snmalloc smalloc; do BLNAME=${AL}; cargo bench --features=${AL} -- --save-baseline ${BLNAME} 2>&1 | tee ${BLNAME} ; done
./critcmp.py default jemalloc mimalloc rpmalloc snmalloc smalloc
```

* Rust regex as benchmarked by rebar (https://github.com/zooko/rebar)

```code
cargo build --release
./target/release/rebar build -e '^rust/regex(-(s|mi|sn|je|rp)malloc)?$'
./target/release/rebar measure -e '^rust/regex(-(s|mi|sn|je|rp)malloc)?$' -f curated | tee res.csv
./target/release/rebar rank res.csv
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

