#!/usr/bin/env python3
# Thanks to Claude (Opus 4.5 & Sonnet 4.5) for writing this to my specifications.

import sys
import re
import argparse
import math

# Allocator colors
ALLOCATOR_COLORS = {
    'default': '#78909c',   # blue-grey (distinct from smalloc green)
    'glibc': '#5c6bc0',     # indigo
    'jemalloc': '#66bb6a',  # green
    'snmalloc': '#ab47bc',  # purple
    'mimalloc': '#ffca28',  # amber
    'rpmalloc': '#ff7043',  # deep orange
    'smalloc': '#42a5f5',   # blue
    'smalloc + ffi': '#93c2f9', # light blue
}
UNKNOWN_ALLOCATOR_COLOR = '#9e9e9e'  # gray

# Allocator ordering
ALLOCATOR_ORDER = ['default', 'jemalloc', 'snmalloc', 'mimalloc', 'rpmalloc', 'smalloc']

def get_color(name):
    return ALLOCATOR_COLORS.get(name, UNKNOWN_ALLOCATOR_COLOR)

def parse_benchmark_output(filename):
    """Parse the benchmark output file and return structured data."""
    results = {}  # {allocator: {test_name: time_ns}}

    with open(filename, 'r', encoding='utf-8') as f:
        content = f.read()

    # Pattern to match benchmark lines like:
    # name: de_mt_adrww-64, threads: 64, iters: 20000, ns: 15,778,375, ns/i: 788.9
    pattern = r'name:\s+(\w+)_(\w+)_([^,]+),\s+threads:\s+([\d,]+),\s+iters:\s+[\d,]+,\s+ns:\s+[\d,]+,\s+ns/i:\s+([\d.,]+)'

    for match in re.finditer(pattern, content):
        allocator_prefix = match.group(1)  # e.g., "mi" for mimalloc
        thread_type = match.group(2)       # e.g., "mt" or "st"
        test_suffix = match.group(3)       # e.g., "a-64" or "adrww"
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

def escape_xml(text):
    """Escape special XML characters."""
    return text.replace('&', '&amp;').replace('<', '&lt;').replace('>', '&gt;').replace('"', '&quot;')

def rounded_rect_path(x, y, width, height, radius):
    """Generate SVG path for rectangle with only top corners rounded."""
    r = min(radius, width / 2, height / 2)

    # Start at bottom-left, go clockwise
    path = f"M {x} {y + height}"
    path += f" L {x + width} {y + height}"
    path += f" L {x + width} {y + r}"
    path += f" A {r} {r} 0 0 0 {x + width - r} {y}"
    path += f" L {x + r} {y}"
    path += f" A {r} {r} 0 0 0 {x} {y + r}"
    path += f" Z"

    return path

def needs_log_scale(ratios, tests, allocators):
    """Check if any test group has a 10x or greater ratio between measurements."""
    for test in tests:
        test_pcts = []
        for allocator in allocators:
            alloc_ratios = ratios.get(allocator, {})
            if test in alloc_ratios:
                test_pcts.append(alloc_ratios[test] * 100)

        if len(test_pcts) >= 2:
            min_pct = min(test_pcts)
            max_pct = max(test_pcts)
            if min_pct > 0 and max_pct / min_pct >= 10:
                return True
    return False

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

    # Determine if we need log scale
    use_log_scale = needs_log_scale(ratios, tests, allocators)

    # SVG dimensions
    svg_width = 1000
    svg_height = 550
    margin_left = 80
    margin_right = 180  # Room for legend
    margin_top = 80
    margin_bottom = 140
    chart_width = svg_width - margin_left - margin_right
    chart_height = svg_height - margin_top - margin_bottom

    n_allocators = len(allocators)
    n_tests = len(tests)

    # Bar layout
    group_width = chart_width / n_tests
    bar_width = (group_width * 0.8) / n_allocators
    group_padding = group_width * 0.1

    # Calculate y-axis range (percentage, baseline = 100%)
    all_pcts = []
    for allocator in allocators:
        alloc_ratios = ratios.get(allocator, {})
        for test in tests:
            if test in alloc_ratios:
                all_pcts.append(alloc_ratios[test] * 100)

    min_pct = min(all_pcts) if all_pcts else 1
    max_pct = max(all_pcts) if all_pcts else 100

    # Chart area boundaries
    chart_top_y = margin_top
    chart_bottom_y = margin_top + chart_height

    if use_log_scale:
        # Log scale: extend range for visual padding
        y_min = max(min_pct * 0.7, 1)  # Don't go below 1%
        y_max = max_pct * 1.3

        log_y_min = math.log10(y_min)
        log_y_max = math.log10(y_max)
        log_range = log_y_max - log_y_min

        # Value-to-Y coordinate conversion for log scale
        def pct_to_y(pct):
            if pct <= 0:
                pct = y_min
            log_val = math.log10(pct)
            # Normalize to 0-1 range, then map to chart coordinates
            normalized = (log_val - log_y_min) / log_range
            # Y increases downward, so invert
            return chart_top_y + chart_height * (1 - normalized)

    else:
        # Linear scale: start at 0
        y_min = 0
        y_max = max_pct * 1.15
        # Round up to nice number
        if y_max <= 120:
            y_max = 120
        elif y_max <= 150:
            y_max = 150
        elif y_max <= 200:
            y_max = 200
        else:
            y_max = math.ceil(y_max / 50) * 50

        # Value-to-Y coordinate conversion for linear scale
        def pct_to_y(pct):
            normalized = pct / y_max
            return chart_top_y + chart_height * (1 - normalized)

    # Start building SVG
    svg_parts = []
    svg_parts.append('<?xml version="1.0" encoding="UTF-8"?>')

    # SVG header
    svg_parts.append(f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {svg_width} {svg_height}" width="{svg_width}" height="{svg_height}">\n')

    # Background
    svg_parts.append(f'  <rect width="{svg_width}" height="{svg_height}" fill="white"/>\n')

    # Styles (using your specified fonts)
    svg_parts.append('''  <style>
    .title { font-family: sans-serif; font-size: 16px; font-weight: bold; fill: #333333; }
    .subtitle { font-family: sans-serif; font-size: 12px; fill: #666666; }
    .axis-label { font-family: sans-serif; font-size: 11px; fill: #666666; }
    .tick-label { font-family: sans-serif; font-size: 10px; fill: #333333; }
    .tick-label-minor { font-family: sans-serif; font-size: 9px; fill: #999999; }
    .test-label { font-family: sans-serif; font-size: 11px; fill: #333333; }
    .bar-label-value { font-family: sans-serif; font-size: 8px; fill: #333333; }
    .bar-label-pct { font-family: sans-serif; font-size: 8px; fill: white; }
    .legend-text { font-family: sans-serif; font-size: 10px; fill: #333333; }
    .metadata { font-family: sans-serif; font-size: 9px; fill: #666666; }
    .key-text { font-family: sans-serif; font-size: 9px; font-style: italic; fill: #666666; }
    .grid-line-major { stroke: #cccccc; stroke-width: 1; }
    .grid-line-minor { stroke: #e0e0e0; stroke-width: 0.5; stroke-dasharray: 4,3; }
  </style>\n''')

    # Title
    type_label = "Single-Threaded" if test_type == "st" else "Multi-Threaded"
    title_y = 30
    svg_parts.append(f'  <text x="{svg_width/2}" y="{title_y}" class="title" text-anchor="middle">{type_label} Performance by Test</text>\n')
    scale_label = "log scale" if use_log_scale else "linear scale"
    svg_parts.append(f'  <text x="{svg_width/2}" y="{title_y + 18}" class="subtitle" text-anchor="middle">(Time vs baseline, lower is better, {scale_label})</text>\n')

    # Y-axis label (rotated)
    y_label_x = 20
    y_label_y = margin_top + chart_height / 2
    svg_parts.append(f'  <text x="{y_label_x}" y="{y_label_y}" class="axis-label" text-anchor="middle" transform="rotate(-90 {y_label_x} {y_label_y})">Time vs Baseline (%, {scale_label})</text>\n')

    # Generate tick values
    if use_log_scale:
        # Major ticks at powers of 10
        major_ticks = []
        minor_ticks = []

        # Find the range of powers of 10
        min_power = math.floor(math.log10(y_min))
        max_power = math.ceil(math.log10(y_max))

        for power in range(min_power, max_power + 1):
            val = 10 ** power
            if y_min <= val <= y_max:
                major_ticks.append(val)

            # Minor ticks at 2x and 5x each power of 10
            for mult in [2, 5]:
                minor_val = mult * (10 ** power)
                if y_min <= minor_val <= y_max and minor_val not in major_ticks:
                    minor_ticks.append(minor_val)

        # Also add ticks at the actual min and max if they're nice numbers
        for val in [y_min, y_max]:
            rounded = round(val)
            if abs(rounded - val) < 0.1 * val:
                if rounded not in major_ticks and rounded not in minor_ticks:
                    minor_ticks.append(rounded)

        # Draw minor grid lines first (so they're behind major ones)
        for tick in sorted(minor_ticks):
            y_pos = pct_to_y(tick)
            if chart_top_y <= y_pos <= chart_bottom_y:
                svg_parts.append(f'  <line x1="{margin_left}" y1="{y_pos}" x2="{margin_left + chart_width}" y2="{y_pos}" class="grid-line-minor"/>\n')
                # Minor tick label
                if tick >= 1000:
                    label = f"{int(tick/1000)}k" if tick % 1000 == 0 else f"{tick/1000:.1f}k"
                else:
                    label = f"{int(tick)}" if tick == int(tick) else f"{tick:.1f}"
                svg_parts.append(f'  <text x="{margin_left - 8}" y="{y_pos + 3}" class="tick-label-minor" text-anchor="end">{label}%</text>\n')

        # Draw major grid lines
        for tick in major_ticks:
            y_pos = pct_to_y(tick)
            if chart_top_y <= y_pos <= chart_bottom_y:
                svg_parts.append(f'  <line x1="{margin_left}" y1="{y_pos}" x2="{margin_left + chart_width}" y2="{y_pos}" class="grid-line-major"/>\n')
                # Major tick label
                if tick >= 1000:
                    label = f"{int(tick/1000)}k"
                else:
                    label = f"{int(tick)}"
                svg_parts.append(f'  <text x="{margin_left - 8}" y="{y_pos + 3}" class="tick-label" text-anchor="end">{label}%</text>\n')

    else:
        # Linear scale ticks
        if y_max <= 150:
            step = 20
        elif y_max <= 300:
            step = 50
        else:
            step = 100

        tick = 0
        while tick <= y_max:
            y_pos = pct_to_y(tick)
            svg_parts.append(f'  <line x1="{margin_left}" y1="{y_pos}" x2="{margin_left + chart_width}" y2="{y_pos}" class="grid-line-major"/>\n')
            svg_parts.append(f'  <text x="{margin_left - 8}" y="{y_pos + 3}" class="tick-label" text-anchor="end">{int(tick)}%</text>\n')
            tick += step

    # Baseline line at 100%
    if y_min <= 100 <= y_max:
        baseline_y = pct_to_y(100)
        svg_parts.append(f'  <line x1="{margin_left}" y1="{baseline_y}" x2="{margin_left + chart_width}" y2="{baseline_y}" class="grid-line-line"/>\n')

    # X-axis line at bottom
    svg_parts.append(f'  <line x1="{margin_left}" y1="{chart_bottom_y}" x2="{margin_left + chart_width}" y2="{chart_bottom_y}" stroke="#333333" stroke-width="1"/>\n')

    # Draw bars
    for test_idx, test in enumerate(tests):
        group_x = margin_left + test_idx * group_width + group_padding

        for alloc_idx, allocator in enumerate(allocators):
            alloc_ratios = ratios.get(allocator, {})
            alloc_results = results.get(allocator, {})

            if test not in alloc_ratios:
                continue

            ratio = alloc_ratios[test]
            pct = ratio * 100
            ns_val = alloc_results.get(test, 0)

            # Bar position - calculate Y from percentage
            bar_x = group_x + alloc_idx * bar_width
            bar_top_y = pct_to_y(pct)
            bar_height = chart_bottom_y - bar_top_y

            # Ensure minimum visible bar height
            if bar_height < 2:
                bar_height = 2
                bar_top_y = chart_bottom_y - bar_height

            color = get_color(allocator)

            # Draw bar with rounded top corners
            corner_radius = 3
            path = rounded_rect_path(bar_x, bar_top_y, bar_width - 1, bar_height, corner_radius)
            svg_parts.append(f'  <path d="{path}" fill="{color}"/>\n')

            # Value label above bar (absolute time)
            bar_center_x = bar_x + (bar_width - 1) / 2
            if ns_val > 0:
                label = format_ns_truncated(ns_val)
                svg_parts.append(f'  <text x="{bar_center_x}" y="{bar_top_y - 5}" class="bar-label-value" text-anchor="middle">{escape_xml(label)}</text>\n')

            # Percentage diff inside bar - skip for baseline
            if allocator != 'default' and bar_height > 20:
                pct_label = format_pct_diff(ratio)
                pct_y = bar_top_y + 14
                svg_parts.append(f'  <text x="{bar_center_x}" y="{pct_y}" class="bar-label-pct" text-anchor="middle">{escape_xml(pct_label)}</text>\n')

        # Test label below group
        group_center_x = group_x + (n_allocators * bar_width) / 2
        test_label = test.split('_', 1)[1] if '_' in test else test
        svg_parts.append(f'  <text x="{group_center_x}" y="{chart_bottom_y + 20}" class="test-label" text-anchor="middle">{escape_xml(test_label)}</text>\n')

    # Legend
    legend_x = margin_left + chart_width + 20
    legend_y = margin_top + 10
    legend_item_height = 20
    legend_box_size = 12

    for i, allocator in enumerate(allocators):
        item_y = legend_y + i * legend_item_height
        color = get_color(allocator)

        # Color box with rounded corners
        svg_parts.append(f'  <rect x="{legend_x}" y="{item_y}" width="{legend_box_size}" height="{legend_box_size}" fill="{color}" rx="2"/>\n')
        # Label
        svg_parts.append(f'  <text x="{legend_x + legend_box_size + 6}" y="{item_y + 10}" class="legend-text">{escape_xml(allocator)}</text>\n')

    # Key for test abbreviations
    key_y = svg_height - 75
    key_text = "Tests: adrww=alloc/dealloc/realloc and write, adww=alloc/dealloc and write, aww=alloc and write"
    svg_parts.append(f'  <text x="{svg_width/2}" y="{key_y}" class="key-text" text-anchor="middle">{escape_xml(key_text)}</text>\n')

    # Metadata
    meta_y = svg_height - 50

    meta_parts = []
    if metadata.get('timestamp'):
        meta_parts.append(f"Timestamp: {metadata['timestamp']}")

    if meta_parts:
        svg_parts.append(f'  <text x="{svg_width/2}" y="{meta_y}" class="metadata" text-anchor="middle">{escape_xml(" · ".join(meta_parts))}</text>\n')

    line2_parts = []
    if metadata.get('source'):
        line2_parts.append(f"Source: {metadata['source']}")
    if metadata.get('commit'):
        line2_parts.append(f"Commit: {metadata['commit'][:12]}")
    if metadata.get('git_status'):
        line2_parts.append(f'Git status: {metadata["git_status"]}')

    if line2_parts:
        svg_parts.append(f'  <text x="{svg_width/2}" y="{meta_y + 15}" class="metadata" text-anchor="middle">{escape_xml(" · ".join(line2_parts))}</text>\n')

    line3_parts = []
    if metadata.get('cpu'):
        line3_parts.append(f"CPU: {metadata['cpu']}")
    if metadata.get('os'):
        line3_parts.append(f"OS: {metadata['os']}")
    if metadata.get('cpucount'):
        line3_parts.append(f"CPU Count: {metadata['cpucount']}")

    if line3_parts:
        svg_parts.append(f'  <text x="{svg_width/2}" y="{meta_y + 30}" class="metadata" text-anchor="middle">{escape_xml(" · ".join(line3_parts))}</text>\n')

    svg_parts.append('</svg>\n')

    with open(output_file, 'w', encoding='utf-8') as f:
        f.write(''.join(svg_parts))

    print(f"Detailed graph saved to: {output_file}")

def main():
    parser = argparse.ArgumentParser(description='Parse benchmark results and generate graphs')
    parser.add_argument('input_file', help='Benchmark output file to parse')
    parser.add_argument('--timestamp', help='When the benchmarking process started')
    parser.add_argument('--source', help='Source URL')
    parser.add_argument('--commit', help='Git commit hash')
    parser.add_argument('--git-status', help='Git status (Clean or Uncommitted changes)')
    parser.add_argument('--cpu', help='CPU type')
    parser.add_argument('--os', help='OS type')
    parser.add_argument('--cpucount', help='Number of CPUs')
    parser.add_argument('--graph', help='Base name for output graph files (without extension)')

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
            print(f"  {alloc:12s}: {mean:.3f} ({pct:+.1f}%)")

    if means_mt:
        print("\nMulti-Threaded - Arithmetic mean of time ratios (1.0 = baseline):")
        for alloc in sort_allocators(means_mt.keys()):
            mean = means_mt[alloc]
            pct = (mean - 1.0) * 100
            print(f"  {alloc:12s}: {mean:.3f} ({pct:+.1f}%)")

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
            'timestamp': args.timestamp,
            'commit': args.commit,
            'git_status': args.git_status,
            'cpu': args.cpu,
            'os': args.os,
            'cpucount': args.cpucount,
            'source': args.source,
        }

        base_name = args.graph

        # Detailed graphs
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
