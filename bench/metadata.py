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

# Canonical allocator ordering
ALLOCATOR_ORDER = ['default', 'jemalloc', 'snmalloc', 'mimalloc', 'rpmalloc', 'smalloc']

def get_color(name):
    return ALLOCATOR_COLORS.get(name, UNKNOWN_ALLOCATOR_COLOR)

def sort_allocators(names):
    """Sort allocator names in canonical order: default, known allocators, unknown, smalloc last."""
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

def escape_xml(text):
    """Escape special XML characters."""
    return text.replace('&', '&amp;').replace('<', '&lt;').replace('>', '&gt;').replace('"', '&quot;')

def add_svg_metadata(args, metadata_y, svg_parts, svg_width):
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
        line2_parts.append(f"Git Clean Status: {args.git_clean_status}")

    line3_parts = []
    if args.cpu:
        line3_parts.append(f"CPU: {args.cpu}")
    if args.os:
        line3_parts.append(f"OS: {args.os}")
    if args.cpu_count:
        line3_parts.append(f"CPU count: {args.cpu_count}")

    if line0_parts:
        svg_parts.append(f'  <text x="{svg_width/2}" y="{metadata_y}" class="metadata" text-anchor="middle">{escape_xml(" · ".join(line0_parts))}</text>\n')
    if line1_parts:
        svg_parts.append(f'  <text x="{svg_width/2}" y="{metadata_y + 14}" class="metadata" text-anchor="middle">{" · ".join(line1_parts)}</text>\n')
    if line2_parts:
        svg_parts.append(f'  <text x="{svg_width/2}" y="{metadata_y + 28}" class="metadata" text-anchor="middle">{" · ".join(line2_parts)}</text>\n')
    if line3_parts:
        svg_parts.append(f'  <text x="{svg_width/2}" y="{metadata_y + 42}" class="metadata" text-anchor="middle">{" · ".join(line3_parts)}</text>\n')
