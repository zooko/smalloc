# Builtin bench tool

smalloc comes with a "micro-benchmarking" tool, used to measure smalloc's performance at a low level, which can also compare to low-level measurements of other allocators. Build it with

```
cargo build --release --package bench
```

Run it with 

```
./target/release/bench
```

You can optionally add the `--compare` or `--thorough` flags or both.

# Code size

This is the one of the main measurements that I was optimizing for!

```text
% echo smalloc
smalloc
% cd smalloc
% for F in src/lib.rs src/plat/mod.rs ; do
% for F in src/lib.rs src/plat/mod.rs; do F2="${F%.*}-noda.${F##*.}" ; grep -v debug_assert ${F} > ${F2} ; done
% tokei `find . -name '*-noda.*'`
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 Language              Files        Lines         Code     Comments       Blanks
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 Rust                      2          547          292          123          132
 |- Markdown               1            8            0            7            1
 (Total)                              555          292          130          133
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 Total                     2          555          292          130          133
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
% echo smalloc-ffi
smalloc-ffi
% cd smalloc-ffi
% for F in `find . -name '*.rs' -o -name '*.c' -o -name '*.h'`; do F2="${F%.*}-noa.${F##*.}" ; grep -v -i assert ${F} > ${F2} ; done
% tokei `find . -name "*-noa.*"`
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 Language              Files        Lines         Code     Comments       Blanks
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 C                         1           26           21            0            5
─────────────────────────────────────────────────────────────────────────────────
 Rust                      2          431          322           32           77
 |- Markdown               1           30            0           21            9
 (Total)                              461          322           53           86
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 Total                     3          487          343           53           91
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
% echo rpmalloc
rpmalloc
% cd rpmalloc
% for F in `find . -name '*.c' -o -name '*.h'`; do F2="${F%.*}-noa.${F##*.}" ; grep -v -i assert ${F} > ${F2} ; done
% tokei `find . -name '*-noa.*'`
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 Language              Files        Lines         Code     Comments       Blanks
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 C                         2         2793         2226          292          275
 C Header                  2          520          283          158           79
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 Total                     4         3313         2509          450          354
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
% echo glibc
glibc
% cd malloc
% for F in `find . -name '*.c' -o -name '*.h'`; do F2="${F%.*}-noa.${F##*.}" ; grep -v -i assert ${F} > ${F2} ; done
% tokei `find . -name "*-noa.*" ! -name "tst-*"`
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 Language              Files        Lines         Code     Comments       Blanks
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 C                        32        11773         6935         3242         1596
 C Header                  5          954          449          363          142
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 Total                    37        12727         7384         3605         1738
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
% echo mimalloc
mimalloc
% cd src
% for F in `find . -name '*.c' -o -name '*.h'`; do F2="${F%.*}-noa.${F##*.}" ; grep -v -i assert ${F} > ${F2} ; done
% tokei `find . -name '*-noa.*'`
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 Language              Files        Lines         Code     Comments       Blanks
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 C                        27        13636         9487         2343         1806
 C Header                  2         1022          462          431          129
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 Total                    29        14658         9949         2774         1935
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
% echo snmalloc
snmalloc
% cd src
% for F in `find . -name '*.c' -o -name '*.h'`; do F2="${F%.*}-noa.${F##*.}" ; grep -v -i assert ${F} > ${F2} ; done
% tokei `find . -name "*-noa.*"`
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 Language              Files        Lines         Code     Comments       Blanks
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 C Header                130        20703        12728         5452         2523
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 Total                   130        20703        12728         5452         2523
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
% echo jemalloc
jemalloc
% cd src
% for F in `find . -name '*.c' -o -name '*.h'`; do F2="${F%.*}-noa.${F##*.}" ; grep -v -i assert ${F} > ${F2} ; done
% tokei `find . -name "*-noa.*"`
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 Language              Files        Lines         Code     Comments       Blanks
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 C                        67        34702        25713         4793         4196
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 Total                    67        34702        25713         4793         4196
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

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

