#!/bin/bash
set -e

source "$(dirname "$0")/tools.sh"

BNAME="smalloc"

# Output files
RESF="${OUTPUT_DIR}/${BNAME}.result.txt"
GRAPH_BASE="${OUTPUT_DIR}/${BNAME}.graph-"

mkdir -p ${OUTPUT_DIR}
rm -f $RESF

echo "TIMESTAMP: ${TIMESTAMP}" 2>&1 | tee -a $RESF
gather_and_print_git_metadata 2>&1 | tee -a $RESF
print_machine_metadata 2>&1 | tee -a $RESF

ALLOCATORS=$(IFS=,; echo "${ALLOCATOR_LIST[*]}")

cargo --offline build --release --package bench --features=$ALLOCATORS

./target/release/bench "${SMALLOC_ONLY}" ${*} 2>&1 | tee -a $RESF

# Generate graphs with sumstats.py
./bench/sumstats.py "$RESF" --graph "$GRAPH_BASE" "${METADATA_ARGS_TO_PASS_TO_PYTHON_SCRIPT[@]}" 2>&1 | tee -a $RESF

echo "# Data results (text) are in \"${RESF}\" ."
