#!/bin/bash

get_timestamp() {
    date -u +"%Y-%m-%d %H:%M:%S UTC"
}
TIMESTAMP=$(get_timestamp)

get_git_source() {
    RES=$(git remote get-url origin 2>/dev/null)
    RES="${RES:-unknown}"
    [[ "$RES" == git@* ]] && RES=$(echo "$RES" | sed 's|^git@\([^:]*\):\(.*\)|https://\1/\2|')
    RES="${RES%.git}"
    echo "${RES}"
}
GIT_SOURCE=$(get_git_source)

get_git_commit() {
    git rev-parse HEAD
}
GIT_COMMIT=$(get_git_commit)

get_git_tag() {
    git describe --tags --abbrev=0 2>/dev/null || echo
}
GIT_TAG=$(get_git_tag)

get_git_clean_status() {
    [ -z "$(git status --porcelain)" ] && echo Clean || echo Uncommitted changes
}
GIT_CLEAN_STATUS=$(get_git_clean_status)

gather_and_print_git_metadata() {
    echo "git source: $(get_git_source)"
    echo "git commit: $(get_git_commit)"
    echo "git tag: $(get_git_tag)"
    echo "git clean status: $(get_git_clean_status)"
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

CPUSTR_DOT_OSSTR="${CPU_TYPE_STR}.${OS_TYPE_STR}"

METADATA_ARGS_TO_PASS_TO_PYTHON_SCRIPT=(
  --timestamp "$TIMESTAMP"
  --git-source "$GIT_SOURCE"
  --git-commit "$GIT_COMMIT"
  --git-clean-status "$GIT_CLEAN_STATUS"
  --git-tag "$GIT_TAG"
  --cpu "$CPU_TYPE_STR"
  --os "$OSTYPE"
  --cpu-count "$CPU_COUNT"
)
