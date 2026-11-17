#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

tmp_dir=$(mktemp -d)
trap 'rm -rf "$tmp_dir"' EXIT

run_report_parser() {
  local dir="$1"
  python3 - "$dir" <<'PY'
from __future__ import annotations

from collections import Counter
from pathlib import Path
import json
import re
import sys

out_dir = Path(sys.argv[1])

def read_rc(name: str) -> int:
    path = out_dir / f"{name}.rc"
    try:
        return int(path.read_text(encoding="utf-8").strip())
    except FileNotFoundError:
        return 127

unsafe_rc = read_rc("unsafe-usage.log")
unsafe_counts: Counter[str] = Counter()
unsafe_total: int | None = None
unsafe_log = out_dir / "unsafe-usage.log"
unsafe_hit_re = re.compile(r"^([^:\r\n]+?\.rs):(\d+):(.*)$")

if unsafe_rc in (0, 1):
    unsafe_total = 0
    if unsafe_rc == 0 and unsafe_log.exists():
        for line in unsafe_log.read_text(encoding="utf-8", errors="replace").splitlines():
            match = unsafe_hit_re.match(line)
            if not match:
                continue
            path = match.group(1)
            if path.startswith("./"):
                path = path[2:]
            parts = Path(path).parts
            bucket = "/".join(parts[:3]) if len(parts) >= 3 else path
            unsafe_counts[bucket] += 1
        unsafe_total = sum(unsafe_counts.values())
else:
    unsafe_total = None

duplicate_warning_re = re.compile(
    r"warning\[duplicate\]\s*:\s*found (\d+) duplicate entries for crate '([^']+)'"
)
ansi_escape_re = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")
duplicate_warning_entries: list[tuple[str, int]] = []
duplicate_crate_counts: Counter[str] = Counter()
cargo_deny_rc = read_rc("cargo-deny.log")
cargo_deny_log = out_dir / "cargo-deny.log"
if cargo_deny_log.exists():
    for raw_line in cargo_deny_log.read_text(encoding="utf-8", errors="replace").splitlines():
        line = ansi_escape_re.sub("", raw_line)
        match = duplicate_warning_re.search(line)
        if not match:
            continue
        duplicate_count = int(match.group(1))
        crate_name = match.group(2)
        duplicate_warning_entries.append((crate_name, duplicate_count))
        duplicate_crate_counts[crate_name] = max(
            duplicate_crate_counts[crate_name],
            duplicate_count,
        )

duplicate_tree_output_lines = 0
duplicate_tree_log = out_dir / "cargo-tree-duplicates.log"
if duplicate_tree_log.exists():
    duplicate_tree_output_lines = sum(
        1
        for line in duplicate_tree_log.read_text(
            encoding="utf-8",
            errors="replace",
        ).splitlines()
        if line.strip()
    )

duplicate_top_crates = [
    {"crate": crate_name, "duplicate_entries": duplicate_count}
    for crate_name, duplicate_count in sorted(
        duplicate_crate_counts.items(),
        key=lambda item: (-item[1], item[0]),
    )[:20]
]
duplicate_crates = [
    {"crate": crate_name, "duplicate_entries": duplicate_count}
    for crate_name, duplicate_count in sorted(duplicate_crate_counts.items())
]

summary = {
    "rustsec_ignore_baseline_rc": read_rc("rustsec-ignore-baseline.log"),
    "standalone_tool_lockfiles_rc": read_rc("standalone-tool-lockfiles.log"),
    "launcher_p2p_dependency_surface_rc": read_rc("launcher-p2p-dependency-surface.log"),
    "cargo_deny_rc": cargo_deny_rc,
    "duplicate_dependencies_rc": read_rc("cargo-tree-duplicates.log"),
    "duplicate_dependency_tree_output_lines": duplicate_tree_output_lines,
    "duplicate_dependency_cluster_count": len(duplicate_warning_entries),
    "duplicate_dependency_unique_crates": len(duplicate_crate_counts),
    "duplicate_dependency_entry_total": sum(
        duplicate_count for _, duplicate_count in duplicate_warning_entries
    ),
    "duplicate_dependency_crates": duplicate_crates,
    "duplicate_dependency_top_crates": duplicate_top_crates,
    "unsafe_usage_rc": unsafe_rc,
    "unsafe_usage_total": unsafe_total,
    "unsafe_usage_top_buckets": unsafe_counts.most_common(20),
}

(out_dir / "summary.json").write_text(
    json.dumps(summary, ensure_ascii=False, indent=2) + "\n",
    encoding="utf-8",
)

unsafe_matches_display = (
    "unknown" if summary["unsafe_usage_total"] is None else str(summary["unsafe_usage_total"])
)
duplicate_clusters_display = (
    "unknown" if summary["cargo_deny_rc"] == 127 else str(summary["duplicate_dependency_cluster_count"])
)
duplicate_crates_display = (
    "unknown" if summary["cargo_deny_rc"] == 127 else str(summary["duplicate_dependency_unique_crates"])
)
duplicate_entries_display = (
    "unknown" if summary["cargo_deny_rc"] == 127 else str(summary["duplicate_dependency_entry_total"])
)

lines = [
    "# Rust Governance Report",
    "",
    "Report-only checks; non-zero statuses below are findings, not CI failures for this job.",
    "",
    "| Check | Status | Artifact |",
    "| --- | ---: | --- |",
    f"| RustSec ignore baseline | {summary['rustsec_ignore_baseline_rc']} | `rustsec-ignore-baseline.log` |",
    f"| standalone tool lockfiles | {summary['standalone_tool_lockfiles_rc']} | `standalone-tool-lockfiles.log` |",
    f"| launcher p2p dependency surface | {summary['launcher_p2p_dependency_surface_rc']} | `launcher-p2p-dependency-surface.log` |",
    f"| cargo deny check | {summary['cargo_deny_rc']} | `cargo-deny.log` |",
    f"| cargo tree -d | {summary['duplicate_dependencies_rc']} | `cargo-tree-duplicates.log` |",
    f"| unsafe usage scan | {summary['unsafe_usage_rc']} | `unsafe-usage.log` |",
    "",
    f"- Duplicate dependency clusters: `{duplicate_clusters_display}`",
    f"- Duplicate dependency unique crates: `{duplicate_crates_display}`",
    f"- Duplicate dependency entries: `{duplicate_entries_display}`",
    f"- Duplicate dependency tree output lines: `{summary['duplicate_dependency_tree_output_lines']}`",
    f"- Unsafe usage matches: `{unsafe_matches_display}`",
]

if summary["duplicate_dependency_top_crates"]:
    lines.append("")
    lines.append("## Top Duplicate Dependency Crates")
    lines.append("")
    for entry in summary["duplicate_dependency_top_crates"]:
        lines.append(f"- `{entry['crate']}`: {entry['duplicate_entries']} entries")

if summary["unsafe_usage_top_buckets"]:
    lines.append("")
    lines.append("## Top Unsafe Usage Buckets")
    lines.append("")
    for bucket, count in summary["unsafe_usage_top_buckets"]:
        lines.append(f"- `{bucket}`: {count}")

(out_dir / "summary.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
PY
}

# -------------------------------------------------------------------
# Test 1: Missing scanner tool (rg: command not found, exit code 127)
# -------------------------------------------------------------------
case1="$tmp_dir/case1_missing_rg"
mkdir -p "$case1"
echo "./scripts/ci-rust-governance-report.sh: line 53: rg: command not found" >"$case1/unsafe-usage.log"
echo "127" >"$case1/unsafe-usage.log.rc"
echo "0" >"$case1/cargo-deny.log.rc"
: >"$case1/cargo-deny.log"

run_report_parser "$case1"

python3 - "$case1/summary.json" "$case1/summary.md" <<'PY'
from pathlib import Path
import json
import sys

summary = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
md = Path(sys.argv[2]).read_text(encoding="utf-8")

assert summary["unsafe_usage_rc"] == 127, f"expected rc 127, got {summary['unsafe_usage_rc']}"
assert summary["unsafe_usage_total"] is None, f"expected total None on failure, got {summary['unsafe_usage_total']}"
assert summary["unsafe_usage_top_buckets"] == [], f"expected empty buckets on failure, got {summary['unsafe_usage_top_buckets']}"
assert "- Unsafe usage matches: `unknown`" in md, f"expected unknown in markdown, got:\n{md}"
assert "Top Unsafe Usage Buckets" not in md, f"expected no buckets in markdown, got:\n{md}"
assert "ci-rust-governance-report.sh" not in md, f"error line was mistakenly reported as bucket in markdown"
PY

# -------------------------------------------------------------------
# Test 2: Unsafe scan with genuine 0 matches (rg exit code 1)
# -------------------------------------------------------------------
case2="$tmp_dir/case2_zero_unsafe"
mkdir -p "$case2"
: >"$case2/unsafe-usage.log"
echo "1" >"$case2/unsafe-usage.log.rc"

run_report_parser "$case2"

python3 - "$case2/summary.json" "$case2/summary.md" <<'PY'
from pathlib import Path
import json
import sys

summary = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
md = Path(sys.argv[2]).read_text(encoding="utf-8")

assert summary["unsafe_usage_rc"] == 1, f"expected rc 1, got {summary['unsafe_usage_rc']}"
assert summary["unsafe_usage_total"] == 0, f"expected total 0, got {summary['unsafe_usage_total']}"
assert summary["unsafe_usage_top_buckets"] == []
assert "- Unsafe usage matches: `0`" in md, f"expected 0 in markdown, got:\n{md}"
PY

# -------------------------------------------------------------------
# Test 3: Unsafe scan with genuine matches (exit code 0) & non-.rs noise
# -------------------------------------------------------------------
case3="$tmp_dir/case3_valid_unsafe"
mkdir -p "$case3"
cat >"$case3/unsafe-usage.log" <<'EOF'
./crates/oasis7/examples/archive_memory.rs:21:unsafe impl GlobalAlloc for Meter {
crates/oasis7/src/main.rs:10:unsafe fn run() {}
./vendor-foo/src/lib.rs:5:unsafe { do_something(); }
warning: cannot access /tmp/restricted
EOF
echo "0" >"$case3/unsafe-usage.log.rc"

run_report_parser "$case3"

python3 - "$case3/summary.json" "$case3/summary.md" <<'PY'
from pathlib import Path
import json
import sys

summary = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
md = Path(sys.argv[2]).read_text(encoding="utf-8")

assert summary["unsafe_usage_rc"] == 0
assert summary["unsafe_usage_total"] == 3, f"expected 3 valid hits, got {summary['unsafe_usage_total']}"
buckets = dict(summary["unsafe_usage_top_buckets"])
assert buckets.get("crates/oasis7/examples") == 1
assert buckets.get("crates/oasis7/src") == 1
assert buckets.get("vendor-foo/src/lib.rs") == 1
assert "- Unsafe usage matches: `3`" in md
PY

# -------------------------------------------------------------------
# Test 4: Cargo-deny ANSI colored log vs uncolored log parity
# -------------------------------------------------------------------
case4_colored="$tmp_dir/case4_colored"
case4_plain="$tmp_dir/case4_plain"
mkdir -p "$case4_colored" "$case4_plain"

printf 'warning[duplicate]\x1b[0m: found 4 duplicate entries for crate '"'hashbrown'"'\n\x1b[33mwarning[duplicate]\x1b[0m: found 6 duplicate entries for crate '"'windows-sys'"'\nwarning[duplicate]\x1b[0m\x1b[1m: found 2 duplicate entries for crate '"'rand'"'\n' >"$case4_colored/cargo-deny.log"
echo "0" >"$case4_colored/cargo-deny.log.rc"

cat >"$case4_plain/cargo-deny.log" <<'EOF'
warning[duplicate]: found 4 duplicate entries for crate 'hashbrown'
warning[duplicate]: found 6 duplicate entries for crate 'windows-sys'
warning[duplicate]: found 2 duplicate entries for crate 'rand'
EOF
echo "0" >"$case4_plain/cargo-deny.log.rc"

run_report_parser "$case4_colored"
run_report_parser "$case4_plain"

python3 - "$case4_colored/summary.json" "$case4_plain/summary.json" <<'PY'
from pathlib import Path
import json
import sys

colored = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
plain = json.loads(Path(sys.argv[2]).read_text(encoding="utf-8"))

for key in (
    "duplicate_dependency_cluster_count",
    "duplicate_dependency_unique_crates",
    "duplicate_dependency_entry_total",
    "duplicate_dependency_crates",
    "duplicate_dependency_top_crates",
):
    assert colored[key] == plain[key], f"mismatch on {key}: colored={colored[key]} vs plain={plain[key]}"

assert colored["duplicate_dependency_cluster_count"] == 3
assert colored["duplicate_dependency_unique_crates"] == 3
assert colored["duplicate_dependency_entry_total"] == 12
PY

# -------------------------------------------------------------------
# Test 5: Cargo-deny licenses error (rc=4) coexists with duplicate data
# -------------------------------------------------------------------
case5="$tmp_dir/case5_rc4"
mkdir -p "$case5"
printf 'warning[duplicate]\x1b[0m: found 2 duplicate entries for crate '"'foo'"'\nerror[licenses]: failed license check\n' >"$case5/cargo-deny.log"
echo "4" >"$case5/cargo-deny.log.rc"

run_report_parser "$case5"

python3 - "$case5/summary.json" <<'PY'
from pathlib import Path
import json
import sys

summary = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))

assert summary["cargo_deny_rc"] == 4, f"expected rc 4 preserved, got {summary['cargo_deny_rc']}"
assert summary["duplicate_dependency_cluster_count"] == 1
assert summary["duplicate_dependency_unique_crates"] == 1
assert summary["duplicate_dependency_entry_total"] == 2
PY

# -------------------------------------------------------------------
# Test 6: Cargo-deny skipped (rc=127) displays unknown in markdown
# -------------------------------------------------------------------
case6="$tmp_dir/case6_skipped"
mkdir -p "$case6"
echo "cargo-deny unavailable; see cargo-deny-install.log" >"$case6/cargo-deny.log"
echo "127" >"$case6/cargo-deny.log.rc"

run_report_parser "$case6"

python3 - "$case6/summary.json" "$case6/summary.md" <<'PY'
from pathlib import Path
import json
import sys

summary = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
md = Path(sys.argv[2]).read_text(encoding="utf-8")

assert summary["cargo_deny_rc"] == 127
assert "- Duplicate dependency clusters: `unknown`" in md
assert "- Duplicate dependency unique crates: `unknown`" in md
assert "- Duplicate dependency entries: `unknown`" in md
PY

echo "ci-rust-governance-report.test: OK"
