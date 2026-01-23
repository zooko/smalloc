#!/usr/bin/env python3
# Thanks to Claude (Opus 4.5 & Sonnet 4.5) for writing this to my specifications.

import sys
import re
import argparse
import matplotlib.pyplot as plt
import matplotlib.patches as mpatches
import numpy as np

# Allocator colors (matching simd-json benchmark colors)
ALLOCATOR_COLORS = {
    'default': '#ab47bc',    # purple
    'glibc': '#5c6bc0',      # indigo
    'jemalloc': '#42a5f5',   # blue
    'snmalloc': '#26a69a',   # teal
    'mimalloc': '#ffca28',   # amber
    'rpmalloc': '#ff7043',   # deep orange
    'smalloc': '#66bb6a',    # green
}
UNKNOWN_ALLOCATOR_COLOR = '#9e9e9e'  # gray

# Allocator ordering
ALLOCATOR_ORDER = ['default', 'jemalloc', 'snmalloc', 'mimalloc', 'rpmalloc', 'smalloc']

def get_color(name):
    return ALLOCATOR_COLORS.get(name, UNKNOWN_ALLOCATOR_COLOR)

def parse_benchmark_output(filename):
    """Parse the benchmark output file and return structured data."""
    results = {}  # {allocator: {test_name: time_ns}}

    with open(filename, 'r') as f:
        content = f.read()

    # Pattern to match benchmark lines like:
    # name:   de_mt_adrww-64, threads:    64, iters:     20000, ns:     15,778,375, ns/i:       788.9

    pattern = r'name:\s+(\w+)_(\w+)_([^,]+),\s+threads:\s+([\d,]+),\s+iters:\s+[\d,]+,\s+ns:\s+[\d,]+,\s+ns/i:\s+([\d.,]+)'

    for match in re.finditer(pattern, content):
        allocator_prefix = match.group(1)  # e.g., "mi" for mimalloc
        thread_type = match.group(2)        # e.g., "mt" or "st"
        test_suffix = match.group(3)        # e.g., "a-64" or "adrww"
        threads = int(match.group(4).replace(',', ''))
        ns_per_iter = float(match.group(5).replace(',', ''))

        # Map allocator prefixes to names
        allocator_map = {
            'mi': 'mimalloc',
            'je': 'jemalloc',
            'sn': 'snmalloc',
            'rp': 'rpmalloc',
            'sm': 'smalloc',
            'de': 'default',
        }

        allocator = allocator_map.get(allocator_prefix, allocator_prefix)

        # Create test name: thread_type + test_suffix (e.g., "st_a" or "mt_adrww-64")
        test_name = f"{thread_type}_{test_suffix}"

        if allocator not in results:
            results[allocator] = {}

        results[allocator][test_name] = ns_per_iter

    return results

def compute_ratios(results, baseline='default'):
    """Compute time ratios relative to baseline for each test."""
    if baseline not in results:
        print(f"Warning: Baseline '{baseline}' not found in results", file=sys.stderr)
        return {}

    baseline_times = results[baseline]
    ratios = {}  # {allocator: {test_name: ratio}}

    for allocator, tests in results.items():
        ratios[allocator] = {}
        for test_name, time_ns in tests.items():
            if test_name in baseline_times and baseline_times[test_name] > 0:
                ratios[allocator][test_name] = time_ns / baseline_times[test_name]

    return ratios

def sort_allocators(allocators):
    """Sort allocators in canonical order."""
    def sort_key(name):
        if name in ALLOCATOR_ORDER:
            return (0, ALLOCATOR_ORDER.index(name))
        return (1, name)
    return sorted(allocators, key=sort_key)

def format_ns_truncated(ns_value):
    """Format nanoseconds as truncated whole number with appropriate unit."""
    if ns_value >= 1_000_000:
        # Milliseconds
        return f"{int(ns_value / 1_000_000)}ms"
    elif ns_value >= 1_000:
        # Microseconds
        return f"{int(ns_value / 1_000)}μs"
    else:
        # Nanoseconds
        return f"{int(ns_value)}ns"

def format_pct_diff(ratio):
    """Format percentage difference from baseline."""
    pct_diff = (ratio - 1.0) * 100
    if abs(pct_diff) < 0.5:
        return "0%"
    elif pct_diff > 0:
        return f"+{int(round(pct_diff))}%"
    else:
        return f"{int(round(pct_diff))}%"

def generate_detailed_graph(ratios, results, test_type, output_file, metadata):
    """Generate detailed bar chart showing each test's performance."""

    # Filter tests for this type (st or mt)
    allocators = sort_allocators([a for a in ratios.keys() if ratios[a]])

    # Get all test names for this type
    all_tests = set()
    for alloc_ratios in ratios.values():
        for test_name in alloc_ratios.keys():
            if test_name.startswith(f"{test_type}_"):
                all_tests.add(test_name)

    tests = sorted(all_tests)
    if not tests:
        print(f"No {test_type} tests found, skipping detailed graph", file=sys.stderr)
        return

    # Create figure
    fig, ax = plt.subplots(figsize=(14, 8))
    plt.subplots_adjust(bottom=0.18, top=0.88)

    # Bar positioning
    n_allocators = len(allocators)
    n_tests = len(tests)
    bar_width = 0.8 / n_allocators

    # Store bar data for labeling
    bar_data = []  # [(x_pos, height, ns_value, ratio, allocator), ...]

    for i, allocator in enumerate(allocators):
        alloc_ratios = ratios.get(allocator, {})
        alloc_results = results.get(allocator, {})
        values = []
        ns_values = []
        ratio_values = []
        for test in tests:
            ratio = alloc_ratios.get(test, 1.0)
            ns_val = alloc_results.get(test, 0)
            # Convert ratio to percentage (baseline = 100%)
            pct = ratio * 100
            values.append(pct)
            ns_values.append(ns_val)
            ratio_values.append(ratio)

        x = np.arange(n_tests) + i * bar_width
        bars = ax.bar(x, values, bar_width, label=allocator, color=get_color(allocator), edgecolor='none')

        # Store data for labeling
        for j, (bar, ns_val, ratio) in enumerate(zip(bars, ns_values, ratio_values)):
            bar_data.append((bar.get_x() + bar.get_width()/2, bar.get_height(), ns_val, ratio, allocator))

    # Set y-axis to logarithmic scale
    ax.set_yscale('log')

    # Styling
    ax.set_xticks(np.arange(n_tests) + bar_width * (n_allocators - 1) / 2)
    # Simplify test labels (remove "st_" or "mt_" prefix)
    test_labels = [t.split('_', 1)[1] if '_' in t else t for t in tests]
    ax.set_xticklabels(test_labels, fontsize=10)
    ax.set_ylabel('Time vs Baseline (%, log scale)', fontsize=11)

    # Calculate y-axis limits for log scale
    all_values = []
    for allocator in allocators:
        alloc_ratios = ratios.get(allocator, {})
        for test in tests:
            ratio = alloc_ratios.get(test, 1.0)
            all_values.append(ratio * 100)

    min_pct = min(all_values) if all_values else 100
    max_pct = max(all_values) if all_values else 100

    # Set limits with some padding in log space
    ax.set_ylim(min_pct * 0.7, max_pct * 2.5)

    # Add horizontal line at 100% for reference (baseline)
    ax.axhline(y=100, color='#333333', linewidth=1.5, linestyle='--', alpha=0.7, label='_nolegend_')

    # Grid (works well with log scale)
    ax.yaxis.grid(True, linestyle='--', alpha=0.3, which='both')
    ax.set_axisbelow(True)

    # Add labels: absolute time above bar, percentage diff inside bar
    for x_pos, bar_height, ns_val, ratio, allocator in bar_data:
        # Absolute time label above the bar
        if ns_val > 0:
            label = format_ns_truncated(ns_val)
            ax.annotate(label,
                        xy=(x_pos, bar_height),
                        xytext=(0, 3),
                        textcoords='offset points',
                        ha='center', va='bottom',
                        fontsize=7, fontweight='bold',
                        color='#333333')

        # Percentage diff inside the bar (near the top)
        # Skip for baseline (default) since it's always 0%
        if allocator != 'default':
            pct_label = format_pct_diff(ratio)
            # Position inside the bar, near the top
            # Use a position that's 85% of the bar height (in log space)
            inner_y = bar_height * 0.92
            ax.annotate(pct_label,
                        xy=(x_pos, inner_y),
                        ha='center', va='top',
                        fontsize=6, fontweight='bold',
                        color='white')

    # Title
    type_label = "Single-Threaded" if test_type == "st" else "Multi-Threaded"
    ax.set_title(f'{type_label} Performance by Test\n(Time vs baseline, lower is better, log scale)',
                 fontsize=14, fontweight='bold', pad=15)

    # Legend
    ax.legend(loc='upper right', fontsize=9)

    # Key for test abbreviations
    key_text = "Tests: adrww=alloc/dealloc/realloc and write, adww=alloc/dealloc and write, aww=alloc and write"

    # Metadata
    meta_lines = []
    if metadata.get('source'):
        meta_lines.append(f"Source: {metadata['source']}")

    line1_parts = []
    if metadata.get('commit'):
        line1_parts.append(f"Commit: {metadata['commit'][:12]}")
    if metadata.get('git_status'):
        line1_parts.append(f"Git status: {metadata['git_status']}")
    if line1_parts:
        meta_lines.append(" · ".join(line1_parts))

    line2_parts = []
    if metadata.get('cpu'):
        line2_parts.append(f"CPU: {metadata['cpu']}")
    if metadata.get('os'):
        line2_parts.append(f"OS: {metadata['os']}")
    if line2_parts:
        meta_lines.append(" · ".join(line2_parts))

    fig.text(0.5, 0.11, key_text, ha='center', fontsize=9, color='#666666', style='italic')

    y_pos = 0.07
    for line in meta_lines:
        fig.text(0.5, y_pos, line, ha='center', fontsize=9, color='#666666', family='monospace')
        y_pos -= 0.03

    # Remove top and right spines
    ax.spines['top'].set_visible(False)
    ax.spines['right'].set_visible(False)

    plt.savefig(output_file, format='svg', bbox_inches='tight', dpi=150)
    plt.close()
    print(f"📊 Detailed graph saved to: {output_file}")

def main():
    parser = argparse.ArgumentParser(description='Parse benchmark results and generate graphs')
    parser.add_argument('input_file', help='Benchmark output file to parse')
    parser.add_argument('--graph', help='Base name for output graph files (without extension)')
    parser.add_argument('--commit', help='Git commit hash')
    parser.add_argument('--git-status', help='Git status (Clean or Uncommitted changes)')
    parser.add_argument('--cpu', help='CPU type')
    parser.add_argument('--os', help='OS type')
    parser.add_argument('--source', help='Source URL')

    args = parser.parse_args()

    # Parse benchmark output
    results = parse_benchmark_output(args.input_file)

    if not results:
        print("No benchmark results found in input file", file=sys.stderr)
        sys.exit(1)

    # Compute ratios relative to baseline
    ratios = compute_ratios(results, baseline='default')

    # Compute arithmetic means per allocator, split by ST and MT
    means_st = {}
    means_mt = {}
    for allocator, test_ratios in ratios.items():
        st_ratios = [r for name, r in test_ratios.items() if name.startswith('st_')]
        mt_ratios = [r for name, r in test_ratios.items() if name.startswith('mt_')]

        if st_ratios:
            means_st[allocator] = sum(st_ratios) / len(st_ratios)
        if mt_ratios:
            means_mt[allocator] = sum(mt_ratios) / len(mt_ratios)

    # Print text summary
    print("\n" + "=" * 60)
    print("BENCHMARK SUMMARY")
    print("=" * 60)

    if means_st:
        print("\nSingle-Threaded - Arithmetic mean of time ratios (1.0 = baseline):")
        for alloc in sort_allocators(means_st.keys()):
            mean = means_st[alloc]
            pct = (mean - 1.0) * 100
            print(f"  {alloc:12s}: {mean:.3f}  ({pct:+.1f}%)")

    if means_mt:
        print("\nMulti-Threaded - Arithmetic mean of time ratios (1.0 = baseline):")
        for alloc in sort_allocators(means_mt.keys()):
            mean = means_mt[alloc]
            pct = (mean - 1.0) * 100
            print(f"  {alloc:12s}: {mean:.3f}  ({pct:+.1f}%)")

    # Print smalloc comparison
    if 'smalloc' in means_st:
        print("\nSingle-Threaded smalloc vs others:")
        sm = means_st['smalloc']
        for alloc in sort_allocators(means_st.keys()):
            if alloc != 'smalloc':
                other = means_st[alloc]
                diff = (sm - other) / other * 100
                print(f"  smalloc vs {alloc:12s}: {diff:+.1f}%")

    if 'smalloc' in means_mt:
        print("\nMulti-Threaded smalloc vs others:")
        sm = means_mt['smalloc']
        for alloc in sort_allocators(means_mt.keys()):
            if alloc != 'smalloc':
                other = means_mt[alloc]
                diff = (sm - other) / other * 100
                print(f"  smalloc vs {alloc:12s}: {diff:+.1f}%")

    # Generate graphs if requested
    if args.graph:
        metadata = {
            'commit': args.commit,
            'git_status': args.git_status,
            'cpu': args.cpu,
            'os': args.os,
            'source': args.source,
        }

        base_name = args.graph

        # Detailed graphs (passing results for ns/i values)
        if means_st:
            gfname = f'{base_name}st.svg'
            generate_detailed_graph(ratios, results, 'st', gfname, metadata)
            print("Singlethreaded benchmarks graph is in %s" % gfname)
        if means_mt:
            gfname = f'{base_name}mt.svg'
            generate_detailed_graph(ratios, results, 'mt', gfname, metadata)
            print("Multithreaded benchmarks graph is in %s" % gfname)

if __name__ == '__main__':
    main()
