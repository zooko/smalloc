#!/usr/bin/env pypy3

import math
import statistics
import sys
from pathlib import Path
import argparse

parser = argparse.ArgumentParser(
    description=(
        "Compare paired benchmark runs between a baseline smalloc "
        "repository and the repository containing this script."
    ),
    epilog=(
        "Example: %(prog)s --baseline=../tempbenchmain2"
    ),
)

parser.add_argument(
    "--baseline",
    required=True,
    metavar="DIR",
    help="baseline smalloc repository",
)

args = parser.parse_args()

old_directory = Path(args.baseline).resolve()
new_directory = Path(__file__).resolve().parent.parent

if not old_directory.is_dir():
    parser.error(f"baseline directory does not exist: {old_directory}")

if not (old_directory / "tmp/paired-runs").is_dir():
    parser.error(
        f"baseline has no tmp/paired-runs directory: "
        f"{old_directory / 'tmp/paired-runs'}"
    )

if not (new_directory / "tmp/paired-runs").is_dir():
    parser.error(
        f"test repository has no tmp/paired-runs directory: "
        f"{new_directory / 'tmp/paired-runs'}"
    )

METADATA_FIELDS = (
    ("timestamp", "TIMESTAMP"),
    ("git_source", "git source"),
    ("git_commit", "git commit"),
    ("git_tag", "git tag"),
    ("git_clean_status", "git clean status"),
    ("cpu_type", "CPU type"),
    ("cpu_count", "CPU count"),
    ("os_type", "OS type"),
)

# Timestamps should differ between runs. Everything else must remain fixed
# within one candidate.
INVARIANT_METADATA_FIELDS = tuple(
    key
    for key, _ in METADATA_FIELDS
    if key != "timestamp"
)

def parse_integer(text):
    text = text.strip()
    multiplier = 1

    if text.endswith("k"):
        text = text[:-1].strip()
        multiplier = 1_000
    elif text.endswith("M"):
        text = text[:-1].strip()
        multiplier = 1_000_000

    text = text.replace(",", "").replace("_", "")
    return int(text) * multiplier


def parse_file(filename):
    results = {}
    run_metadata = {}

    for line in filename.read_text().splitlines():
        for key, label in METADATA_FIELDS:
            prefix = f"{label}:"

            if line.startswith(prefix):
                value = line[len(prefix):].strip()

                assert key not in run_metadata, (
                    f"{filename}: duplicate metadata field {label!r}"
                )

                run_metadata[key] = value
                break
        else:
            if not line.startswith("name:"):
                continue

            fields = line.split(", ")
            name = fields[0].split(":", 1)[1].strip()
            iters = parse_integer(fields[2].split(":", 1)[1])
            ns = parse_integer(fields[3].split(":", 1)[1])

            assert name not in results, (
                f"{filename}: duplicate benchmark {name!r}"
            )

            results[name] = ns / iters

    return results, run_metadata


def discover_rounds(directory):
    paired_runs = directory / "tmp/paired-runs"

    rounds = sorted(
        int(filename.stem)
        for filename in paired_runs.glob("*.txt")
        if filename.stem.isdigit()
    )

    if not rounds:
        raise SystemExit(f"no numbered benchmark runs found in {paired_runs}")

    return rounds


def load_runs(directory, round_numbers):
    runs = []
    metadata_runs = []
    filenames = []

    for round_number in round_numbers:
        filename = (
            directory
            / "tmp/paired-runs"
            / f"{round_number:02d}.txt"
        )
        results, run_metadata = parse_file(filename)

        assert results, f"no benchmark results in {filename}"

        runs.append(results)
        metadata_runs.append(run_metadata)
        filenames.append(filename)

    return runs, metadata_runs, filenames

def validate_candidate_metadata(
    candidate_name,
    filenames,
    metadata_runs,
):
    errors = []

    for key, label in METADATA_FIELDS:
        missing = [
            filename
            for filename, run_metadata in zip(
                filenames,
                metadata_runs,
            )
            if key not in run_metadata
        ]

        if missing:
            errors.append(
                f"{candidate_name}: metadata field {label!r} "
                "is missing from:"
            )
            errors.extend(f"  {filename}" for filename in missing)

    for key in INVARIANT_METADATA_FIELDS:
        label = dict(METADATA_FIELDS)[key]
        values = {}

        for round_number, run_metadata in enumerate(
            metadata_runs,
            1,
        ):
            value = run_metadata.get(key, "<missing>")
            values.setdefault(value, []).append(round_number)

        if len(values) > 1:
            errors.append(
                f"{candidate_name}: metadata skew in {label!r}:"
            )

            for value, rounds in values.items():
                round_list = ", ".join(
                    f"{round_number:02d}"
                    for round_number in rounds
                )
                errors.append(
                    f"  {value!r}: rounds {round_list}"
                )

    if errors:
        raise SystemExit("\n".join(errors))


def print_candidate_metadata(title, metadata_runs):
    run_metadata = metadata_runs[0]
    labels = dict(METADATA_FIELDS)

    print(title)
    print("-" * len(title))

    for key in INVARIANT_METADATA_FIELDS:
        print(f"{labels[key]}: {run_metadata[key]}")

    timestamps = [
        run_metadata["timestamp"]
        for run_metadata in metadata_runs
    ]

    print(f"first timestamp: {timestamps[0]}")
    print(f"last timestamp:  {timestamps[-1]}")
    print()

    
def mean(values):
    return sum(values) / len(values)


def geometric_mean(values):
    return math.exp(mean([math.log(value) for value in values]))


def median_confidence_interval(values, confidence=0.95):
    values = sorted(values)
    n = len(values)
    tail_probability = (1 - confidence) / 2
    cumulative_probability = 0
    k = 0

    for i in range(n):
        cumulative_probability += math.comb(n, i) / 2**n

        if cumulative_probability <= tail_probability:
            k = i + 1
        else:
            break

    if k == 0:
        return values[0], values[-1]

    return values[k - 1], values[n - k]


def paired_summary(old, new):
    ratios = [
        new_value / old_value
        for old_value, new_value in zip(old, new)
    ]

    ratio = statistics.median(ratios)
    lower, upper = median_confidence_interval(ratios)

    return ratio, lower, upper


def benchmark_parts(name):
    parts = name.split("_")

    if len(parts) < 3 or parts[0] != "sm":
        return None, None, None

    kind = parts[1]
    operation_and_threads = "_".join(parts[2:])
    operation, separator, threads = operation_and_threads.rpartition("-")

    if not separator:
        return kind, None, None

    return kind, operation, int(threads)


def category_summary(name, selected):
    differences = sorted(row["difference"] for row in selected)

    faster = sum(difference < 0 for difference in differences)
    slower = sum(difference > 0 for difference in differences)

    significantly_faster = sum(
        row["upper"] < 1
        for row in selected
    )
    significantly_slower = sum(
        row["lower"] > 1
        for row in selected
    )

    return {
        "name": name,
        "count": len(selected),
        "median": statistics.median(differences),
        "minimum": min(differences),
        "maximum": max(differences),
        "faster": faster,
        "slower": slower,
        "significantly_faster": significantly_faster,
        "significantly_slower": significantly_slower,
        "range_excludes_zero": (
            min(differences) > 0
            or max(differences) < 0
        ),
    }


old_round_numbers = discover_rounds(old_directory)
new_round_numbers = discover_rounds(new_directory)

if old_round_numbers != new_round_numbers:
    old_only = sorted(set(old_round_numbers) - set(new_round_numbers))
    new_only = sorted(set(new_round_numbers) - set(old_round_numbers))

    errors = ["baseline and test candidates have different paired rounds"]

    if old_only:
        errors.append(
            "  only in baseline: "
            + ", ".join(map(str, old_only))
        )

    if new_only:
        errors.append(
            "  only in test: "
            + ", ".join(map(str, new_only))
        )

    raise SystemExit("\n".join(errors))

round_numbers = old_round_numbers

old_runs, old_metadata, old_filenames = load_runs(
    old_directory,
    round_numbers,
)
new_runs, new_metadata, new_filenames = load_runs(
    new_directory,
    round_numbers,
)

validate_candidate_metadata(
    "Baseline candidate",
    old_filenames,
    old_metadata,
)
validate_candidate_metadata(
    "Test candidate",
    new_filenames,
    new_metadata,
)

tests = set(old_runs[0])

for run in old_runs + new_runs:
    assert set(run) == tests

rows = []

for test in sorted(tests):
    old = [run[test] for run in old_runs]
    new = [run[test] for run in new_runs]
    ratio, lower, upper = paired_summary(old, new)
    kind, operation, threads = benchmark_parts(test)

    rows.append({
        "name": test,
        "kind": kind,
        "operation": operation,
        "threads": threads,
        "old": old,
        "new": new,
        "ratio": ratio,
        "lower": lower,
        "upper": upper,
        "difference": (ratio - 1) * 100,
        "significant": lower > 1 or upper < 1,
    })

name_width = max(len("benchmark"), max(len(row["name"]) for row in rows))

print(f"Baseline directory: {old_directory}")
print(f"Test directory:     {new_directory}")
print(f"Paired rounds:      {len(round_numbers)}")
print(f"Benchmarks:         {len(rows)}")
print()

print_candidate_metadata(
    "Baseline candidate metadata",
    old_metadata,
)
print_candidate_metadata(
    "Test candidate metadata",
    new_metadata,
)
print()
print("* = 95% paired interval excludes zero")
print()
print("Per-benchmark paired comparison:")
print(
    f"{'benchmark':<{name_width}}  "
    f"{'old':>12}  "
    f"{'new':>12}  "
    f"{'paired diff':>12}"
)
print(
    f"{'-'*len('benchmark'):<{name_width}}  "
    f"{'-'*len('old'):>12}  "
    f"{'-'*len('new'):>12}  "
    f"{'-'*len('paired diff'):>12}"
)

for row in rows:
    difference = f"{row['difference']:+.3f}%"

    if row["significant"] and abs(row['difference']) >= 1:
        difference += " *"
    else:
        difference += "  "

    print(
        f"{row['name']:<{name_width}}  "
        f"{statistics.median(row['old']):12.4f}  "
        f"{statistics.median(row['new']):12.4f}  "
        f"{difference:>12}"
    )

non_fh = [row for row in rows if row["kind"] != "fh"]

categories = [
    (
        "Free-hotspot tests",
        [row for row in rows if row["kind"] == "fh"],
    ),
    (
        "Single-threaded",
        [row for row in rows if row["kind"] == "st"],
    ),
    (
        "Multithreaded, 32 threads",
        [
            row for row in rows
            if row["kind"] == "mt" and row["threads"] == 32
        ],
    ),
    (
        "Multithreaded, 64 threads",
        [
            row for row in rows
            if row["kind"] == "mt" and row["threads"] == 64
        ],
    ),
    (
        "Multithreaded, 1024 threads",
        [
            row for row in rows
            if row["kind"] == "mt" and row["threads"] == 1024
        ],
    ),
    (
        "Operation a, all thread counts",
        [
            row for row in non_fh
            if row["operation"] == "a"
        ],
    ),
    (
        "Operation aww, all thread counts",
        [
            row for row in non_fh
            if row["operation"] == "aww"
        ],
    ),
    (
        "Operation ad, all thread counts",
        [
            row for row in non_fh
            if row["operation"] == "ad"
        ],
    ),
    (
        "Operation adww, all thread counts",
        [
            row for row in non_fh
            if row["operation"] == "adww"
        ],
    ),
    (
        "Operation adr, all thread counts",
        [
            row for row in non_fh
            if row["operation"] == "adr"
        ],
    ),
    (
        "Operation adrww, all thread counts",
        [
            row for row in non_fh
            if row["operation"] == "adrww"
        ],
    ),
    (
        "Allocation-only family (a + aww)",
        [
            row for row in non_fh
            if row["operation"] in ("a", "aww")
        ],
    ),
    (
        "Alloc/dealloc family (ad + adww)",
        [
            row for row in non_fh
            if row["operation"] in ("ad", "adww")
        ],
    ),
    (
        "Alloc/dealloc/realloc family (adr + adrww)",
        [
            row for row in non_fh
            if row["operation"] in ("adr", "adrww")
        ],
    ),
    (
        "Write variants (aww, adww, adrww)",
        [
            row for row in non_fh
            if row["operation"] is not None
            and row["operation"].endswith("ww")
        ],
    ),
    (
        "No-write variants (a, ad, adr)",
        [
            row for row in non_fh
            if row["operation"] in ("a", "ad", "adr")
        ],
    ),
]

summaries = [
    category_summary(name, selected)
    for name, selected in categories
    if selected
]

category_width = max(
    len("category"),
    max(len(summary["name"]) for summary in summaries),
)

print()
print("Category trends:")
print(
    f"{'category':<{category_width}}  "
    f"{'n':>3}  "
    f"{'median diff':>13}  "
    f"{'range':>21}  "
)
print(
    f"{'-'*len('category'):<{category_width}}  "
    f"{'-'*len('n'):>3}  "
    f"{'-'*len('median diff'):>13}  "
    f"{'-'*len('range'):>21}  "
)

for summary in summaries:
    median_value = f"{summary['median']:+.3f}%"

    if summary["range_excludes_zero"] and abs(summary['median']) >= 1:
        median_value += " †"
    else:
        median_value += "  "

    value_range = (
        f"{summary['minimum']:+.2f}% to "
        f"{summary['maximum']:+.2f}%"
    )

    print(
        f"{summary['name']:<{category_width}}  "
        f"{summary['count']:3}  "
        f"{median_value:>13}  "
        f"{value_range:>21}  "
    )

print()
print("Notes:")
print("  Negative differences mean the new version is faster.")
print("  Categories overlap intentionally.")
print("  Category medians treat each benchmark as one observation.")
print("  F/S means faster/slower; sig. means individually significant.")
print("  * means the benchmark's 95% paired interval excludes zero and its mean ≥ 1%.")
print("  † next to a category median means every benchmark in that category moved in the same direction and their median ≥ 1%.")
