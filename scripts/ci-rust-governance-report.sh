#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

out_dir="output/rust-governance"

usage() {
  cat <<'USAGE'
Usage: ./scripts/ci-rust-governance-report.sh [--out-dir <dir>]

Produce report-only Rust governance artifacts:
- RustSec ignored-advisory baseline metadata/ratchet report
- standalone tool lockfile consistency report
- cargo-deny full policy report
- duplicate dependency report
- duplicate dependency baseline/budget report
- unsafe usage distribution

The script returns success even when report-only checks find issues. The
required advisory gate lives in ./scripts/ci-tests.sh.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out-dir)
      out_dir="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

[[ -n "$out_dir" ]] || { echo "error: --out-dir cannot be empty" >&2; exit 2; }
mkdir -p "$out_dir"

run_report() {
  local label="$1"
  local log_path="$2"
  shift 2

  local status=0
  "$@" >"$log_path" 2>&1 || status=$?
  printf '%s\n' "$status" >"${log_path}.rc"
}

if python3 - "$out_dir/cargo-deny-install.log" <<'PY'
from __future__ import annotations

from pathlib import Path
import subprocess
import sys

log_path = Path(sys.argv[1])
with log_path.open("w", encoding="utf-8") as log:
    try:
        result = subprocess.run(
            ["./scripts/ensure-cargo-deny.sh"],
            stdout=log,
            stderr=subprocess.STDOUT,
            timeout=120,
            check=False,
        )
    except subprocess.TimeoutExpired:
        log.write("error: cargo-deny install timed out after 120 seconds\n")
        raise SystemExit(124)
raise SystemExit(result.returncode)
PY
then
  run_report "cargo deny" "$out_dir/cargo-deny.log" cargo deny check
else
  {
    echo "cargo-deny unavailable; see cargo-deny-install.log"
    echo "report-only cargo deny check skipped"
  } >"$out_dir/cargo-deny.log"
  printf '%s\n' 127 >"$out_dir/cargo-deny.log.rc"
fi
run_report "RustSec ignore baseline" "$out_dir/rustsec-ignore-baseline.log" ./scripts/check-rustsec-ignore-baseline.sh
run_report "standalone tool lockfiles" "$out_dir/standalone-tool-lockfiles.log" ./scripts/check-standalone-tool-lockfiles.sh
run_report "launcher p2p dependency surface" "$out_dir/launcher-p2p-dependency-surface.log" ./scripts/check-launcher-p2p-dependency-surface.sh
run_report "duplicate dependencies" "$out_dir/cargo-tree-duplicates.log" cargo tree -d
if command -v rg >/dev/null 2>&1; then
  run_report "unsafe usage" "$out_dir/unsafe-usage.log" rg -n --glob '*.rs' '\bunsafe\b' .
elif command -v git >/dev/null 2>&1 && git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  run_report "unsafe usage" "$out_dir/unsafe-usage.log" git grep -n -E '\bunsafe\b' -- '*.rs'
else
  run_report "unsafe usage" "$out_dir/unsafe-usage.log" rg -n --glob '*.rs' '\bunsafe\b' .
fi

python3 - "$out_dir" <<'PY'
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

run_report "duplicate dependency baseline" "$out_dir/duplicate-dependency-baseline.log" ./scripts/check-duplicate-dependency-baseline.sh "$out_dir/summary.json"

python3 - "$out_dir" <<'PY'
from __future__ import annotations

from pathlib import Path
import json
import sys

out_dir = Path(sys.argv[1])
summary_path = out_dir / "summary.json"
summary = json.loads(summary_path.read_text(encoding="utf-8"))

def read_rc(name: str) -> int:
    path = out_dir / f"{name}.rc"
    try:
        return int(path.read_text(encoding="utf-8").strip())
    except FileNotFoundError:
        return 127

summary["duplicate_dependency_baseline_rc"] = read_rc("duplicate-dependency-baseline.log")
summary_path.write_text(
    json.dumps(summary, ensure_ascii=False, indent=2) + "\n",
    encoding="utf-8",
)

summary_md = out_dir / "summary.md"
lines = summary_md.read_text(encoding="utf-8").splitlines()
insert_after = "| cargo tree -d |"
baseline_line = (
    f"| duplicate dependency baseline | "
    f"{summary['duplicate_dependency_baseline_rc']} | "
    "`duplicate-dependency-baseline.log` |"
)
updated: list[str] = []
inserted = False
for line in lines:
    updated.append(line)
    if not inserted and line.startswith(insert_after):
        updated.append(baseline_line)
        inserted = True
if not inserted:
    updated.append(baseline_line)
summary_md.write_text("\n".join(updated) + "\n", encoding="utf-8")
PY

cat "$out_dir/summary.md"
