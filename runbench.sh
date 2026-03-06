#!/bin/bash
set -e

source "$(dirname "$0")/gather-metadata.sh"

BNAME="cargo-bench"

ARGS=$*

OUTPUT_DIR="${OUTPUT_DIR:-./bench/results}/${CPUSTR_DOT_OSSTR}"

RESF="${OUTPUT_DIR}/${BNAME}.result.txt"
GRAPH_BASE="${OUTPUT_DIR}/${BNAME}.graph-"

mkdir -p ${OUTPUT_DIR}
rm -f $RESF

echo "TIMESTAMP: ${TIMESTAMP}" 2>&1 | tee -a $RESF
gather_and_print_git_metadata 2>&1 | tee -a $RESF
print_machine_metadata 2>&1 | tee -a $RESF

mkdir -p ${OUTPUT_DIR}

if [ "x${OSTYPE}" = "xmsys" ]; then
    # no jemalloc or snmalloc on windows
    ALLOCATORS=mimalloc,rpmalloc
else
    ALLOCATORS=jemalloc,snmalloc,mimalloc,rpmalloc
fi

cargo --locked build --release --package bench --features=$ALLOCATORS

./target/release/bench --compare ${ARGS} 2>&1 | tee -a $RESF

# Generate graphs with sumstats.py
./bench/sumstats.py "$RESF" --graph "$GRAPH_BASE" "${METADATA_ARGS_TO_PASS_TO_PYTHON_SCRIPT[@]}"

echo "# Data results (text) are in \"${RESF}\" ."
