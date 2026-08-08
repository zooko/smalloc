#!/bin/bash
set -e

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)

cd "$REPO_ROOT"
source "$SCRIPT_DIR/tools.sh"

BNAME="smalloc"

# Output files
RESF="${OUTPUT_DIR}/${BNAME}.result.txt"
GRAPH_BASE="${OUTPUT_DIR}/${BNAME}.graph-"

mkdir -p ${OUTPUT_DIR}
rm -f $RESF "${GRAPH_BASE}*"

echo "TIMESTAMP: ${TIMESTAMP}" 2>&1 | tee -a $RESF
gather_and_print_git_metadata 2>&1 | tee -a $RESF
print_machine_metadata 2>&1 | tee -a $RESF
echo "smalloc version: $(get_smalloc_dep_version .)" 2>&1 | tee -a $RESF

ALLOCATORS=$(IFS=,; echo "${ALLOCATOR_LIST[*]}")

cargo --locked --offline build --release --package bench --features=$ALLOCATORS

./target/release/bench "${SMALLOC_ONLY}" "${@}" 2>&1 | tee -a $RESF

# For comparative benchmarking, generate comaprative graphs with sumstats.py . For differential
# benchmarking (i.e. called by run-paired-benchmarks.sh), this doesn't make sense so skip it.
if [[ -z "$SMALLOC_ONLY" ]]; then
    ./tools/sumstats.py "$RESF" --graph "$GRAPH_BASE" "${METADATA_ARGS_TO_PASS_TO_PYTHON_SCRIPT[@]}" 2>&1 | tee -a $RESF
fi

echo "# Data results (text) are in \"${RESF}\" ."
