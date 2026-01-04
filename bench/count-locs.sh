#!/bin/sh

echo smalloc
cd smalloc
cd smalloc
find . -name '*-noda.*' -print0 | xargs -0 rm
for F in src/lib.rs src/plat/mod.rs; do F2="${F%.*}-noda.${F##*.}" ; grep -v debug_assert ${F} > ${F2} ; done
tokei `find . -name '*-noda.*'`
cd ..

echo smalloc-ffi
cd smalloc-ffi
find . -name '*-noa.*' -print0 | xargs -0 rm
for F in `find . -name '*.rs' -o -name '*.c' -o -name '*.h'`; do F2="${F%.*}-noa.${F##*.}" ; grep -v -i assert ${F} > ${F2} ; done
tokei `find . -name "*-noa.*"`
cd ../..

echo rpmalloc
cd rpmalloc
cd rpmalloc
find . -name '*-noa.*' -print0 | xargs -0 rm
for F in `find . -name '*.c' -o -name '*.h'`; do F2="${F%.*}-noa.${F##*.}" ; grep -v -i assert ${F} > ${F2} ; done
tokei `find . -name '*-noa.*'`
cd ../..

echo glibc
cd glibc
cd malloc
find . -name '*-noa.*' -print0 | xargs -0 rm
for F in `find . -name '*.c' -o -name '*.h'`; do F2="${F%.*}-noa.${F##*.}" ; grep -v -i assert ${F} > ${F2} ; done
tokei `find . -name "*-noa.*" ! -name "tst-*"`
cd ../..

echo mimalloc
cd mimalloc
cd src
find . -name '*-noa.*' -print0 | xargs -0 rm
for F in `find . -name '*.c' -o -name '*.h'`; do F2="${F%.*}-noa.${F##*.}" ; grep -v -i assert ${F} > ${F2} ; done
tokei `find . -name '*-noa.*'`
cd ../..

echo snmalloc
cd snmalloc
cd src
find . -name '*-noa.*' -print0 | xargs -0 rm
for F in `find . -name '*.c' -o -name '*.h'`; do F2="${F%.*}-noa.${F##*.}" ; grep -v -i assert ${F} > ${F2} ; done
tokei `find . -name "*-noa.*"`
cd ../..

echo jemalloc
cd jemalloc
cd src
find . -name '*-noa.*' -print0 | xargs -0 rm
for F in `find . -name '*.c' -o -name '*.h'`; do F2="${F%.*}-noa.${F##*.}" ; grep -v -i assert ${F} > ${F2} ; done
tokei `find . -name "*-noa.*"`
cd ../..
