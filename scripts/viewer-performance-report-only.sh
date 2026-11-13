#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
probe_command="${OASIS7_VIEWER_PERFORMANCE_PROBE_COMMAND:-$repo_root/scripts/viewer-performance-probe.sh}"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
artifact_dir="${OASIS7_VIEWER_PERFORMANCE_ARTIFACT_DIR:-$repo_root/output/ci/viewer-performance/$run_id}"
if [[ "${OASIS7_LOCAL_CHECK:-0}" == 1 ]]; then
  probe_command="$repo_root/scripts/viewer-performance-probe.sh"
  artifact_dir="$(node "$repo_root/crates/oasis7_viewer/scripts/viewer-local-output.mjs" report performance)"
fi
summary_json="$artifact_dir/summary.json"

mkdir -p "$artifact_dir"
set +e
"$probe_command" --profile smoke --out-dir "$artifact_dir"
probe_status=$?
set -e

if [[ ! -s "$summary_json" ]]; then
  echo "viewer performance collection failure: missing summary.json artifact_dir=$artifact_dir probe_exit=$probe_status" >&2
  exit 1
fi

summary_status=""
if ! summary_status=$(python3 - "$summary_json" "$artifact_dir" <<'PY'
import json
import pathlib
import sys

summary_path = pathlib.Path(sys.argv[1])
artifact_dir = pathlib.Path(sys.argv[2])
try:
    summary = json.loads(summary_path.read_text(encoding="utf-8"))
except Exception as exc:
    raise SystemExit(f"invalid summary.json: {exc}")

status = summary.get("status")
if status not in {"pass", "fail"}:
    raise SystemExit(f"invalid summary status: {status!r}")
for field in ("scenario", "metrics", "gates", "browser", "viewport", "artifacts"):
    if field not in summary:
        raise SystemExit(f"missing summary field: {field}")
if not isinstance(summary["gates"], list) or not summary["gates"]:
    raise SystemExit("missing measured gate samples")
artifacts = summary["artifacts"]
if not isinstance(artifacts, dict):
    raise SystemExit("invalid artifacts field")
for name in ("summaryJson", "summaryMarkdown", "screenshot"):
    artifact = artifacts.get(name)
    if not isinstance(artifact, str) or not pathlib.Path(artifact).is_file():
        raise SystemExit(f"missing {name} artifact")
if not (artifact_dir / "web-dist").is_dir():
    raise SystemExit("missing web-dist reproduction artifact")
print(status)
PY
); then
  echo "viewer performance collection failure: invalid summary artifact_dir=$artifact_dir probe_exit=$probe_status" >&2
  exit 1
fi

if [[ "$summary_status" == "fail" ]]; then
  echo "viewer performance threshold miss (report-only): artifact_dir=$artifact_dir probe_exit=$probe_status"
  exit 0
fi

if [[ "$probe_status" -ne 0 ]]; then
  echo "viewer performance collection failure: passing summary with probe failure artifact_dir=$artifact_dir probe_exit=$probe_status" >&2
  exit 1
fi

echo "viewer performance report: pass artifact_dir=$artifact_dir"
