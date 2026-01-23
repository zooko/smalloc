#!/bin/bash

BNAME="cargo-bench"

# Collect metadata
GITCOMMIT="$(git log -1 | head -1 | cut -d' ' -f2)"
GITCLEANSTATUS=$( [ -z "$( git status --porcelain )" ] && echo \"Clean\" || echo \"Uncommitted changes\" )
TIMESTAMP=$(date -u +"%Y-%m-%d %H:%M:%S UTC")

# CPU type on linuxy
CPUTYPE=`grep "model name" /proc/cpuinfo 2>/dev/null | uniq | cut -d':' -f2-`
if [ "x${CPUTYPE}" = "x" ] ; then
    # CPU type on macos
    CPUTYPE=`sysctl -n machdep.cpu.brand_string 2>/dev/null`
fi
CPUTYPESTR="${CPUTYPE//[^[:alnum:]]/}"
OSTYPESTR="${OSTYPE//[^[:alnum:]]/}"
ARGS=$*
CPUSTR_DOT_OSSTR="${CPUTYPESTR}.${OSTYPESTR}"
OUTPUT_DIR="${OUTPUT_DIR:-./bench/results}/${CPUSTR_DOT_OSSTR}"

RESF="${OUTPUT_DIR}/${BNAME}.result.txt"
GRAPH_BASE="${OUTPUT_DIR}/${BNAME}.graph-"

mkdir -p ${OUTPUT_DIR}

rm -f $RESF

echo "GITCOMMIT: ${GITCOMMIT}" 2>&1 | tee -a $RESF
echo "GITCLEANSTATUS: ${GITCLEANSTATUS}" 2>&1 | tee -a $RESF
echo "CPUTYPE: ${CPUTYPE}" 2>&1 | tee -a $RESF
echo "OSTYPE: ${OSTYPE}" 2>&1 | tee -a $RESF

mkdir -p ${OUTPUT_DIR}

if [ "x${OSTYPE}" = "xmsys" ]; then
    # no jemalloc or snmalloc on windows
    ALLOCATORS=mimalloc,rpmalloc
else
    ALLOCATORS=mimalloc,rpmalloc,jemalloc,snmalloc
fi

cargo --locked build --release --package bench --features=$ALLOCATORS &&

echo "# ./target/release/bench --compare ${ARGS}" 2>&1 | tee -a $RESF &&
echo 2>&1 | tee -a $RESF &&
./target/release/bench --compare ${ARGS} 2>&1 | tee -a $RESF &&

# Generate graphs with sumstats.py
./bench/sumstats.py "$RESF" \
    --graph "$GRAPH_BASE" \
    --commit "$GITCOMMIT" \
    --git-status "$GITCLEANSTATUS" \
    --cpu "$CPUTYPE" \
    --os "$OSTYPE" &&

echo "# Data results (text) are in \"${RESF}\" ."
