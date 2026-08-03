#!/usr/bin/env bash
set -euo pipefail

usage() {
	cat >&2 <<EOF
This is for "differential benchmarking" -- comparing the performance of previous
"baseline" version of smalloc against this version of smalloc. See
"bench-allocators.sh" for "comparative benchmarking" -- comparing the
performance of this version smalloc against other allocators.

Usage: $0 --baseline=DIR

Runs 20 paired benchmark rounds comparing:
  baseline: DIR
  test:     the repository containing this script

DIR must be a smalloc repository.

Example:
  $0 --baseline=../tempbenchmain2
EOF
	exit 2
}

BASELINE=
BENCHMARK_ARGS=()
BENCHMARK_ARG_COUNT=0

for arg in "$@"; do
	if [[ "$arg" == --baseline=* ]]; then
		BASELINE=${arg#--baseline=}
	else
		BENCHMARK_ARGS+=("$arg")
		BENCHMARK_ARG_COUNT=$((BENCHMARK_ARG_COUNT + 1))
	fi
done

[[ -n "$BASELINE" ]] || usage

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
TEST_DIR=$(cd "$SCRIPT_DIR/.." && pwd)
BASELINE_DIR=$(cd "$BASELINE" && pwd)

mkdir -p \
	"$BASELINE_DIR/tmp/paired-runs" \
	"$TEST_DIR/tmp/paired-runs"

run_one() {
	local directory="$1"
	local candidate="$2"
	local round="$3"

	echo "===== round $round: $candidate ($directory) ====="

	(
		cd "$directory"

		if ((BENCHMARK_ARG_COUNT == 0)); then
			./tools/bench-allocators.sh \
				--smalloc-only \
				--thorough
		else
			./tools/bench-allocators.sh \
				--smalloc-only \
				--thorough \
				"${BENCHMARK_ARGS[@]}"
		fi
	) 2>&1 | tee "$directory/tmp/paired-runs/$round.txt"
}

for round_number in {1..20}; do
	printf -v round '%02d' "$round_number"

	if ((round_number % 2)); then
            run_one "$BASELINE_DIR" baseline "$round"
            run_one "$TEST_DIR" test "$round"
	else
            run_one "$TEST_DIR" test "$round"
            run_one "$BASELINE_DIR" baseline "$round"
	fi
done
