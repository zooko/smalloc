#!/bin/bash

get_timestamp() {
    date -u +"%Y-%m-%d %H:%M:%S UTC"
}
TIMESTAMP=$(get_timestamp)

get_git_source() {
    local directory="${1:-.}"
    local result

    result=$(git -C "$directory" remote get-url origin 2>/dev/null || true)
    result="${result:-unknown}"

    if [[ "$result" == git@* ]]; then
        result=$(
            printf '%s\n' "$result" |
                sed 's|^git@$$[^:]*$$:$$.*$$|https://\1/\2|'
        )
    fi

    result="${result%.git}"
    printf '%s\n' "$result"
}

get_git_commit() {
    local directory="${1:-.}"
    git -C "$directory" rev-parse HEAD 2>/dev/null || echo unknown
}

get_git_tag() {
    local directory="${1:-.}"
    git -C "$directory" describe --tags --abbrev=0 2>/dev/null || true
}

get_git_clean_status() {
    local directory="${1:-.}"

    if ! git -C "$directory" rev-parse --git-dir >/dev/null 2>&1; then
        echo unknown
    elif [[ -z "$(git -C "$directory" status --porcelain)" ]]; then
        echo clean
    else
        echo "dirty-$(
            git -C "$directory" diff --binary HEAD |
                b3sum --no-names
        )"
    fi
}

GIT_SOURCE=$(get_git_source)
GIT_COMMIT=$(get_git_commit)
GIT_TAG=$(get_git_tag)

if [[ -n "${BENCHMARK_GIT_CLEAN_STATUS_OVERRIDE:-}" ]]; then
    GIT_CLEAN_STATUS="$BENCHMARK_GIT_CLEAN_STATUS_OVERRIDE"
else
    GIT_CLEAN_STATUS=$(get_git_clean_status)
fi

# Print metadata captured when tools.sh was sourced.
print_git_metadata() {
    echo "git source: $GIT_SOURCE"
    echo "git commit: $GIT_COMMIT"
    echo "git tag: $GIT_TAG"
    echo "git clean status: $GIT_CLEAN_STATUS"
}

# Dynamically inspect the repository at DIRECTORY.
print_current_git_metadata() {
    local directory="${1:-.}"

    echo "git source: $(get_git_source "$directory")"
    echo "git commit: $(get_git_commit "$directory")"
    echo "git tag: $(get_git_tag "$directory")"
    echo "git clean status: $(get_git_clean_status "$directory")"
}

get_cpu_type_str() {
    if command -v lscpu >/dev/null 2>&1; then
        # Linux, but John's little raspbi has better information in lscpu than in /proc/cpuinfo
        CPU_TYPE=$(lscpu 2>/dev/null | grep -i "model name" | cut -d':' -f2-)
    elif command -v sysctl >/dev/null 2>&1; then
        # macOS
        CPU_TYPE=$(sysctl -n machdep.cpu.brand_string 2>/dev/null)
    elif [ -f /proc/cpuinfo ]; then
        # Linux in case it didn't have lscpu, and also mingw64 on Windows provides /proc/cpuifo
        CPU_TYPE=$(grep -m1 "model name" /proc/cpuinfo | cut -d':' -f2-)
    fi
    CPU_TYPE=${CPU_TYPE:-Unknown}
    CPU_TYPE=${CPU_TYPE## }  # Trim leading space

    echo "${CPU_TYPE//[^[:alnum:]]/}"
}
CPU_TYPE_STR=$(get_cpu_type_str)

get_cpu_count() {
    nproc 2>/dev/null || sysctl -n hw.logicalcpu 2>/dev/null || echo "${NUMBER_OF_PROCESSORS:-unknown}"
}
CPU_COUNT=$(get_cpu_count)

get_os_type_str() {
    echo "${OSTYPE//[^[:alnum:]]/}"
}
OS_TYPE_STR=$(get_os_type_str)

print_machine_metadata() {
    echo "CPU type: $CPU_TYPE_STR"
    echo "CPU count: $CPU_COUNT"
    echo "OS type: $OS_TYPE_STR"
}

get_smalloc_dep_version() {
    local subdir=$1
    pushd $subdir >/dev/null
    RESULT=$(cargo --offline metadata --format-version 1 --features smalloc 2>/dev/null | jq -r '.packages[] | select(.name == "smmalloc") | .version' 2>/dev/null)
    if [[ -z "${RESULT}" ]]; then
        RESULT=$(cargo --offline metadata --format-version 1 2>/dev/null | jq -r '.packages[] | select(.name == "smmalloc") | .version' 2>/dev/null)
    fi
    popd >/dev/null
    echo "${RESULT}"
}

CPUSTR_DOT_OSSTR="${CPU_TYPE_STR}.${OS_TYPE_STR}"

OUTPUT_DIR="${OUTPUT_DIR:-./benchmark-results}/${CPUSTR_DOT_OSSTR}"

METADATA_ARGS_TO_PASS_TO_PYTHON_SCRIPT=(
  --timestamp "$TIMESTAMP"
  --git-source "$GIT_SOURCE"
  --git-commit "$GIT_COMMIT"
  --git-clean-status "$GIT_CLEAN_STATUS"
  --cpu "$CPU_TYPE_STR"
  --os "$OSTYPE"
  --cpu-count "$CPU_COUNT"
)
[[ -n ${GIT_TAG//[[:space:]]/} ]] && METADATA_ARGS_TO_PASS_TO_PYTHON_SCRIPT+=(--git-tag "${GIT_TAG//[[:space:]]/}")

SMALLOC_ONLY=""
BENCHMARK_ARGS=()

for arg in "$@"; do
    if [[ "$arg" == "--smalloc-only" ]]; then
        SMALLOC_ONLY="--smalloc-only"
    else
        BENCHMARK_ARGS+=("$arg")
    fi
done

ALLOCATOR_LIST=()

if [ -z "$SMALLOC_ONLY" ]; then
    if [ "x${OSTYPE}" = "xmsys" ]; then
        # no jemalloc or snmalloc on windows
        ALLOCATOR_LIST=(mimalloc rpmalloc)
    else
        ALLOCATOR_LIST=(jemalloc snmalloc mimalloc rpmalloc)
    fi
fi
