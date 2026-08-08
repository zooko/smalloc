# Allocator colors
ALLOCATOR_COLORS = {
    'default': '#8a969e',        # duller blue-grey
    'glibc': '#6f76a3',          # duller indigo
    'jemalloc': '#7faa82',       # duller green
    'snmalloc': '#a06bab',       # duller purple
    'mimalloc': '#e0bd5e',       # duller amber
    'rpmalloc': '#d98567',       # duller deep orange
    'smalloc': '#42a5f5',        # blue (vivid)
    'smalloc + ffi': '#93c2f9',  # light blue (vivid)
}
UNKNOWN_ALLOCATOR_COLOR = '#9e9e9e'  # gray

# Canonical allocator ordering
ALLOCATOR_ORDER = ['smalloc', 'rpmalloc', 'mimalloc', 'snmalloc', 'jemalloc', 'glibc', 'default']

def get_color(name):
    return ALLOCATOR_COLORS.get(name, UNKNOWN_ALLOCATOR_COLOR)

def sort_allocators(names):
    """Sort allocator names in canonical order: smalloc first, known allocators, unknown
    allocators, default."""
    def sort_key(name):
        if name in ALLOCATOR_ORDER:
            return (0, ALLOCATOR_ORDER.index(name))
        else:
            return (0, ALLOCATOR_ORDER.index('smalloc') - 0.5)
    return sorted(names, key=sort_key)

def allocator_prefix_to_name(name):
    allocator_map = {
        'mi': 'mimalloc',
        'je': 'jemalloc',
        'sn': 'snmalloc',
        'rp': 'rpmalloc',
        'sm': 'smalloc',
        'de': 'default',
    }
    return allocator_map.get(name, name)
    
def add_parse_args(parser):
    parser.add_argument('--timestamp', help='When the benchmarking process started')
    parser.add_argument('--git-source', help='Git source URL')
    parser.add_argument('--git-commit', help='Git commit hash')
    parser.add_argument('--git-tag', help='Git tag')
    parser.add_argument('--git-clean-status', help='Git status (Clean or Uncommitted changes)')
    parser.add_argument('--graph', help='Output SVG graph to this file')
    parser.add_argument('--cpu', help='CPU type')
    parser.add_argument('--os', help='OS type')
    parser.add_argument('--cpu-count', help='Number of CPUs')
    parser.add_argument('--smalloc-dep-version', help='Version number of smalloc dependency (from cargo metadata)')

def escape_xml(text):
    """Escape special XML characters."""
    return text.replace('&', '&amp;').replace('<', '&lt;').replace('>', '&gt;').replace('"', '&quot;')

def escape_xml_comment(text):
    """Make text legal inside an XML comment."""
    text = str(text).replace("--", "- -")

    # XML comment content must not end with a hyphen.
    if text.endswith("-"):
        text += " "

    return text

def add_svg_metadata(args, metadata_y, svg_parts, svg_width):
    # Add complete metadata as a non-rendered XML comment.
    comment_fields = [
        ("timestamp", args.timestamp),
        ("git source", args.git_source),
        ("git commit", args.git_commit),
        ("git tag", args.git_tag),
        ("git clean status", args.git_clean_status),
        ("CPU", args.cpu),
        ("OS", args.os),
        ("CPU count", args.cpu_count),
        ("smalloc version", args.smalloc_dep_version),
    ]

    comment_fields = [
        (name, value)
        for name, value in comment_fields
        if value is not None and str(value).strip()
    ]

    if comment_fields:
        svg_parts.append("  <!--\n")
        svg_parts.append("  Benchmark metadata\n")

        for name, value in comment_fields:
            line = escape_xml_comment(f"{name}: {value}")
            svg_parts.append(f"  {line}\n")

        svg_parts.append("  -->\n")

    # Keep the existing visibly rendered metadata.
    line0_parts = []
    if args.timestamp:
        line0_parts.append(f"Timestamp: {args.timestamp}")

    line1_parts = []
    if args.git_source:
        line1_parts.append(f"Source: {args.git_source}")
    if args.git_commit:
        line1_parts.append(f"Commit: {args.git_commit}")
    if args.git_tag:
        line1_parts.append(f"Tag: {args.git_tag}")

    line2_parts = []
    if args.git_clean_status:
        line2_parts.append(
            f"Git Clean Status: {args.git_clean_status}"
        )

    line3_parts = []
    if args.cpu:
        line3_parts.append(f"CPU: {args.cpu}")
    if args.os:
        line3_parts.append(f"OS: {args.os}")
    if args.cpu_count:
        line3_parts.append(f"CPU count: {args.cpu_count}")

    line4_parts = []
    if args.smalloc_dep_version:
        line4_parts.append(
            f"smalloc version: {args.smalloc_dep_version}"
        )

    visible_lines = [
        (0, line0_parts),
        (14, line1_parts),
        (28, line2_parts),
        (42, line3_parts),
        (56, line4_parts),
    ]

    for offset, parts in visible_lines:
        if not parts:
            continue

        text = escape_xml(" · ".join(parts))

        svg_parts.append(
            f'  <text x="{svg_width / 2}" '
            f'y="{metadata_y + offset}" '
            f'class="metadata" text-anchor="middle">'
            f"{text}</text>\n"
        )
