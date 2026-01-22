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
ARGSSTR="${ARGS//[^[:alnum:]]/}"
CPUSTR_DOT_OSSTR="${CPUTYPESTR}.${OSTYPESTR}"
FNAME="${BNAME}.result.${CPUSTR_DOT_OSSTR}.txt"
RESF="tmp/${FNAME}"
GRAPHF="tmp/${BNAME}.graph.${CPUSTR_DOT_OSSTR}.svg"

echo "# Saving result into \"${RESF}\""
echo "# Saving graph into \"${GRAPHF}\""
rm -f $RESF $GRAPHF
mkdir -p tmp

if [ "x${OSTYPE}" = "xmsys" ]; then
    # no jemalloc or snmalloc on windows
    ALLOCATORS="mimalloc rpmalloc smalloc"
else
    ALLOCATORS="jemalloc snmalloc mimalloc rpmalloc smalloc"
fi

set -e

cargo --locked build --release --package bench --features=$ALLOCATORS &&

# Run benchmarks
./target/release/bench --compare ${ARGS} 2>&1 | tee -a $RESF &&

# Generate comparison with metadata passed as arguments
./sumstats.py tmp/default $ALLOCATORS \
    --commit "$GITCOMMIT" \
    --git-status "$GITCLEANSTATUS" \
    --cpu "$CPUTYPE" \
    --os "$OSTYPE" \
    --graph "$GRAPHF" \
    2>&1 | tee -a $RESF

echo "# Results are in \"${RESF}\" ."
echo "# Graph is in \"${GRAPHF}\" ."
