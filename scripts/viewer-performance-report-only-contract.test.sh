#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
helper="$repo_root/scripts/viewer-performance-report-only.sh"
probe_source="$repo_root/crates/oasis7_viewer/scripts/viewer-performance-probe.mjs"
workflow="$repo_root/.github/workflows/rust.yml"
tmp_root=$(mktemp -d)
trap 'rm -rf "$tmp_root"' EXIT

grep -Fq 'case "--out-dir": options.outDir = next(); break;' "$probe_source"
grep -Fq 'summary.artifacts = { summaryJson: summaryJsonPath, summaryMarkdown: summaryMdPath, screenshot: screenshotPath };' "$probe_source"
grep -Fq "path: output/ci/viewer-performance/**" "$workflow"
grep -Fq "if-no-files-found: error" "$workflow"

make_probe() {
  local mode="$1"
  local probe="$tmp_root/probe-$mode.sh"
  cat >"$probe" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
out_dir=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --out-dir) out_dir="$2"; shift 2 ;;
    *) shift ;;
  esac
done
case "${PROBE_MODE:?}" in
  threshold_miss)
    mkdir -p "$out_dir/web-dist"
    : >"$out_dir/viewer-performance.png"
    : >"$out_dir/summary.md"
    cat >"$out_dir/summary.json" <<JSON
{"status":"fail","scenario":{"name":"dense"},"metrics":{"fpsAvg":1},"gates":[{"id":"fps_avg","status":"fail"}],"browser":{"userAgent":"fixture"},"viewport":{"width":1,"height":1},"artifacts":{"summaryJson":"$out_dir/summary.json","summaryMarkdown":"$out_dir/summary.md","screenshot":"$out_dir/viewer-performance.png"}}
JSON
    exit 1
    ;;
  pass)
    mkdir -p "$out_dir/web-dist"
    : >"$out_dir/viewer-performance.png"
    : >"$out_dir/summary.md"
    cat >"$out_dir/summary.json" <<JSON
{"status":"pass","scenario":{"name":"dense"},"metrics":{"fpsAvg":60},"gates":[{"id":"fps_avg","status":"pass"}],"browser":{"userAgent":"fixture"},"viewport":{"width":1,"height":1},"artifacts":{"summaryJson":"$out_dir/summary.json","summaryMarkdown":"$out_dir/summary.md","screenshot":"$out_dir/viewer-performance.png"}}
JSON
    exit 0
    ;;
  missing_summary) exit 1 ;;
  invalid_summary)
    mkdir -p "$out_dir"
    printf '{invalid json\n' >"$out_dir/summary.json"
    exit 1
    ;;
esac
EOF
  chmod +x "$probe"
  printf '%s\n' "$probe"
}

run_probe() {
  local mode="$1"
  local out_dir="$tmp_root/$mode"
  local probe
  probe=$(make_probe "$mode")
  PROBE_MODE="$mode" \
  OASIS7_LOCAL_CHECK=0 \
  OASIS7_VIEWER_PERFORMANCE_PROBE_COMMAND="$probe" \
  OASIS7_VIEWER_PERFORMANCE_ARTIFACT_DIR="$out_dir" \
    "$helper" >"$tmp_root/$mode.log" 2>&1
}

if ! run_probe threshold_miss; then
  echo "threshold miss must be a nonblocking report" >&2
  exit 1
fi
grep -Fq 'viewer performance threshold miss (report-only)' "$tmp_root/threshold_miss.log"
grep -Fq "artifact_dir=$tmp_root/threshold_miss" "$tmp_root/threshold_miss.log"

if run_probe pass; then :; else
  echo "valid passing collection must pass" >&2
  exit 1
fi

for mode in missing_summary invalid_summary; do
  if run_probe "$mode"; then
    echo "$mode must block required-gate collection" >&2
    exit 1
  fi
  grep -Fq 'viewer performance collection failure' "$tmp_root/$mode.log"
done

echo "viewer-performance-report-only contract: passed"
