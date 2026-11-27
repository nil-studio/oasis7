#!/usr/bin/env bash

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/viewer-web-dist-contract.sh"

require_cmd() {
  local cmd=$1
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "error: missing required command: $cmd" >&2
    exit 1
  fi
}

ab_npx_package() {
  printf '%s\n' "${AGENT_BROWSER_NPX_PACKAGE:-agent-browser@0.37.1}"
}

ab_has_cli() {
  command -v agent-browser >/dev/null 2>&1 || command -v npx >/dev/null 2>&1
}

# Native agent-browser derives a Unix socket name from the supplied prefix.
# Keep the final scoped prefix below the platform's conservative 103-byte
# socket-name limit while retaining a readable head and a deterministic hash
# of the complete input (including the per-process suffix).
AB_SESSION_PREFIX_MAX_LENGTH=48
AB_SESSION_PREFIX_HASH_LENGTH=10

ab_compact_session_prefix() {
  local prefix="$1"
  python3 - "$prefix" "$AB_SESSION_PREFIX_MAX_LENGTH" "$AB_SESSION_PREFIX_HASH_LENGTH" <<'PY'
import hashlib
import sys

prefix = sys.argv[1]
max_length = int(sys.argv[2])
hash_length = int(sys.argv[3])
encoded = prefix.encode("utf-8")
if len(encoded) <= max_length:
    print(prefix)
    raise SystemExit(0)

digest = hashlib.sha256(encoded).hexdigest()[:hash_length]
head_budget = max_length - hash_length - 1
head = encoded[:head_budget].decode("utf-8", errors="ignore")
while len(head.encode("utf-8")) + 1 + len(digest) > max_length:
    head = head[:-1]
print(f"{head}-{digest}")
PY
}

ab_require() {
  if ! ab_has_cli; then
    echo "error: missing required command: agent-browser (or npx fallback)" >&2
    exit 1
  fi
  require_cmd python3
}

# Derive a run-scoped session through the version-matched CLI.  The process id
# is part of the prefix so two concurrent runners cannot close or navigate one
# another's browser, while the returned id remains stable for the lifetime of
# the caller's run.
ab_session_id() {
  local prefix=${1:-oasis7-viewer}
  local scoped_prefix
  local output
  scoped_prefix="$(ab_compact_session_prefix "${prefix}-$$")" || return $?
  if command -v agent-browser >/dev/null 2>&1; then
    output=$(agent-browser session id --scope worktree --prefix "$scoped_prefix" 2>/dev/null) || return $?
  else
    output=$(npx --yes "$(ab_npx_package)" session id --scope worktree --prefix "$scoped_prefix" 2>/dev/null) || return $?
  fi
  printf '%s\n' "$output"
}

# Mark a known session as owned by this process and optionally persist a small
# ownership record next to the run artifacts.  Explicit sessions are accepted
# only at a caller boundary (for example, a child runner handed a session by
# its parent); ordinary runners should use ab_session_begin instead.
ab_session_adopt() {
  local session=${1:-}
  local artifact_dir=${2:-}
  [[ -n "$session" ]] || return 1
  AB_SESSION_NAME="$session"
  AB_SESSION_OWNED=1
  AB_SESSION_ARTIFACT_DIR="$artifact_dir"
  export AB_SESSION_NAME AB_SESSION_OWNED AB_SESSION_ARTIFACT_DIR
  if [[ -n "$artifact_dir" ]]; then
    mkdir -p "$artifact_dir"
    python3 - "$artifact_dir/browser-session.json" "$session" "$$" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
path.write_text(json.dumps({
    "session": sys.argv[2],
    "ownerPid": int(sys.argv[3]),
    "owned": True,
}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
PY
  fi
  printf '%s\n' "$session"
}

# Begin an owned session and optionally persist its ownership record next to
# the run artifacts.  The returned id is generated with the current worktree
# scope, so parallel runners do not share a fixed session name.
ab_session_begin() {
  local prefix=${1:-oasis7-viewer}
  local artifact_dir=${2:-}
  local session
  session=$(ab_session_id "$prefix") || return $?
  ab_session_adopt "$session" "$artifact_dir" >/dev/null || return $?
  printf '%s\n' "$session"
}

ab_run() {
  local session=$1
  shift
  local namespace=${AGENT_BROWSER_NAMESPACE:-}
  if command -v agent-browser >/dev/null 2>&1; then
    if [[ -n "$namespace" ]]; then
      AGENT_BROWSER_SESSION="$session" AGENT_BROWSER_NAMESPACE="$namespace" agent-browser "$@"
    else
      AGENT_BROWSER_SESSION="$session" agent-browser "$@"
    fi
    return
  fi
  if [[ -n "$namespace" ]]; then
    AGENT_BROWSER_SESSION="$session" AGENT_BROWSER_NAMESPACE="$namespace" \
      npx --yes "$(ab_npx_package)" "$@"
  else
    AGENT_BROWSER_SESSION="$session" npx --yes "$(ab_npx_package)" "$@"
  fi
}

ab_read_retryable_error() {
  local output=${1:-}
  [[ "$output" == *EAGAIN* || \
    "$output" == *"Resource temporarily unavailable"* || \
    "$output" == *"daemon may be busy"* || \
    "$output" == *"daemon ... unresponsive"* || \
    "$output" == *"ECONNRESET"* || \
    "$output" == *"socket"* ]]
}

# Retry only read-oriented commands.  Callers must not use this wrapper for
# click/fill/submit/control actions because an uncertain action may already
# have reached the page and replay would duplicate the side effect.
ab_read_retry() {
  local session=$1
  shift
  local attempts=${AB_READ_RETRY_ATTEMPTS:-3}
  local delay_secs=${AB_READ_RETRY_DELAY_SECS:-0.2}
  local attempt=1
  local output
  local status=1
  while (( attempt <= attempts )); do
    if output=$(ab_run "$session" "$@" 2>&1); then
      printf '%s\n' "$output"
      return 0
    else
      status=$?
    fi
    if ! ab_read_retryable_error "$output" || (( attempt == attempts )); then
      printf '%s\n' "$output" >&2
      return "$status"
    fi
    printf 'warning: agent-browser read retry %d/%d: %s\n' \
      "$attempt" "$attempts" "$output" >&2
    sleep "$delay_secs"
    attempt=$((attempt + 1))
  done
  return "$status"
}

ab_capture_diagnostics() {
  local session=$1
  local artifact_dir=${2:-${AB_SESSION_ARTIFACT_DIR:-}}
  local info tabs status=0
  if [[ -n "$artifact_dir" ]]; then
    mkdir -p "$artifact_dir"
    if ! info=$(ab_read_retry "$session" session info --json); then
      status=1
    fi
    printf '%s\n' "$info" >"$artifact_dir/browser-session-info.json"
    if ! tabs=$(ab_read_retry "$session" tab list --json); then
      status=1
    fi
    printf '%s\n' "$tabs" >"$artifact_dir/browser-tabs.json"
  else
    ab_read_retry "$session" session info --json || status=$?
    ab_read_retry "$session" tab list --json || status=$?
  fi
  return "$status"
}

ab_session_cleanup() {
  local session=${1:-${AB_SESSION_NAME:-}}
  local artifact_dir=${2:-${AB_SESSION_ARTIFACT_DIR:-}}
  local status=0
  [[ -n "$session" ]] || return 0
  if [[ -n "$artifact_dir" ]]; then
    ab_capture_diagnostics "$session" "$artifact_dir" || status=1
  fi
  ab_run "$session" record stop >/dev/null 2>&1 || true
  ab_run "$session" close >/dev/null 2>&1 || status=$?
  if [[ "${AB_SESSION_NAME:-}" == "$session" ]]; then
    AB_SESSION_OWNED=0
    export AB_SESSION_OWNED
  fi
  return "$status"
}

ab_browser_args() {
  if [[ ${AGENT_BROWSER_ARGS+x} ]]; then
    printf '%s\n' "$AGENT_BROWSER_ARGS"
  else
    printf '%s\n' '--use-angle=gl,--ignore-gpu-blocklist'
  fi
}

ab_cmd() {
  local session=$1
  shift
  ab_run "$session" "$@"
}

ab_resolve_output_path() {
  python3 - "$1" <<'PY'
from pathlib import Path
import sys

print(Path(sys.argv[1]).expanduser().resolve(strict=False))
PY
}

ab_screenshot() {
  local session=$1
  local out_path=$2
  local resolved_out_path
  local output
  local status=0

  resolved_out_path=$(ab_resolve_output_path "$out_path")
  mkdir -p "$(dirname "$resolved_out_path")"
  output=$(ab_read_retry "$session" screenshot "$resolved_out_path" 2>&1) || status=$?
  printf '%s\n' "$output"

  if [[ "$status" -eq 0 && ! -f "$out_path" ]]; then
    python3 - "$output" "$out_path" <<'PY2'
import os
import pathlib
import re
import shutil
import sys

raw = sys.argv[1]
out_path = pathlib.Path(sys.argv[2])
ansi = re.compile(r'\[[0-9;]*m')
clean = ansi.sub('', raw)
match = re.search(r'Screenshot saved to\s+(.+)', clean)
if not match:
    raise SystemExit(0)
source = pathlib.Path(match.group(1).strip())
if not source.exists():
    raise SystemExit(0)
out_path.parent.mkdir(parents=True, exist_ok=True)
if source.resolve() != out_path.resolve():
    shutil.copy2(source, out_path)
PY2
  fi

  return "$status"
}

ab_open() {
  local session=$1
  local headed=$2
  local url=$3
  local browser_args
  local cmd=()

  browser_args=$(ab_browser_args)
  if [[ -n "$browser_args" ]]; then
    cmd+=(--args "$browser_args")
  fi
  if [[ "$headed" -eq 1 ]]; then
    cmd+=(--headed)
  fi
  cmd+=(open "$url")

  ab_run "$session" "${cmd[@]}"
}

ab_eval() {
  local session=$1
  local script=$2
  ab_run "$session" eval --stdin <<<"$script"
}

# Read-only variant of ab_eval.  Keep action-bearing eval calls on ab_eval so
# an uncertain command is never replayed by the retry loop.
ab_read_eval() {
  local session=$1
  local script=$2
  ab_read_retry "$session" eval --stdin <<<"$script"
}

json_quote() {
  python3 - "$1" <<'PY'
import json
import sys
print(json.dumps(sys.argv[1]))
PY
}

json_get() {
  python3 - "$1" "$2" <<'PY'
import json
import sys

raw = sys.argv[1]
path = sys.argv[2].split('.') if sys.argv[2] else []
try:
    value = json.loads(raw)
except Exception:
    print("")
    raise SystemExit(0)
for part in path:
    if isinstance(value, dict):
        value = value.get(part)
    else:
        value = None
        break
if value is None:
    print("")
elif isinstance(value, bool):
    print("true" if value else "false")
elif isinstance(value, (dict, list)):
    print(json.dumps(value, ensure_ascii=False))
else:
    print(value)
PY
}

json_to_file() {
  local raw_json=$1
  local out_path=$2
  python3 - "$raw_json" "$out_path" <<'PY'
import json
import pathlib
import sys

raw = sys.argv[1]
out = pathlib.Path(sys.argv[2])
try:
    data = json.loads(raw)
except Exception:
    out.write_text(raw + "\n", encoding="utf-8")
else:
    out.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
PY
}


resolve_viewer_static_dir_for_web_closure() {
  local repo_root=$1
  local requested_dir=$2
  local out_dir=$3

  if [[ "$requested_dir" != "web" ]]; then
    printf '%s
' "$requested_dir"
    return 0
  fi

  local dist_dir="$repo_root/crates/oasis7_viewer/dist"
  local dist_index="$dist_dir/index.html"
  local rebuilt_dir
  local source_metadata_json

  if [[ "$out_dir" = /* ]]; then
    rebuilt_dir="$out_dir/web-dist"
  else
    rebuilt_dir="$repo_root/$out_dir/web-dist"
  fi

  source_metadata_json=$(viewer_web_dist_source_metadata_json "$repo_root")

  if [[ -f "$dist_index" ]] && python3 - "$source_metadata_json" "$dist_dir" "$(viewer_web_dist_manifest_name)" "$(viewer_web_dist_required_files)" <<'PY'
from __future__ import annotations

import json
import sys
from pathlib import Path

current = json.loads(sys.argv[1])
dist_dir = Path(sys.argv[2]).resolve()
manifest_name = sys.argv[3]
required_raw = sys.argv[4]
manifest_path = dist_dir / manifest_name

if not manifest_path.is_file():
    raise SystemExit(1)

manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
if manifest.get("sourceFingerprint") != current.get("sourceFingerprint"):
    raise SystemExit(1)

dist_files = manifest.get("distFiles")
if not isinstance(dist_files, dict) or not dist_files:
    raise SystemExit(1)

for rel, metadata in dist_files.items():
    candidate = dist_dir / rel
    if not candidate.is_file():
        raise SystemExit(1)
    if candidate.stat().st_size != metadata.get("size"):
        raise SystemExit(1)

for rel in [line.strip() for line in required_raw.splitlines() if line.strip()]:
    if not (dist_dir / rel).is_file():
        raise SystemExit(1)
PY
  then
    printf '%s
' "$dist_dir"
    return 0
  fi

  if ! command -v npm >/dev/null 2>&1; then
    if [[ -f "$dist_index" ]]; then
      echo "warning: npm missing; falling back to committed viewer dist: $dist_dir" >&2
      printf '%s
' "$dist_dir"
      return 0
    fi
    echo "error: missing required command: npm" >&2
    return 1
  fi

  mkdir -p "$rebuilt_dir"
  if [[ -x "$repo_root/scripts/build-viewer-software-safe.sh" ]]; then
    echo "+ $repo_root/scripts/build-viewer-software-safe.sh" >&2
    if ! (
      cd "$repo_root"
      ./scripts/build-viewer-software-safe.sh
    ) >&2; then
      echo "error: viewer software-safe build failed" >&2
      return 1
    fi
  else
    echo "+ npm --prefix $repo_root/crates/oasis7_viewer run build:software-safe" >&2
    if ! (
      cd "$repo_root"
      npm --prefix crates/oasis7_viewer run build:software-safe
    ) >&2; then
      echo "error: viewer software-safe build failed" >&2
      return 1
    fi
  fi
  if [[ -x "$repo_root/scripts/copy-viewer-web-dist.sh" ]]; then
    if ! "$repo_root/scripts/copy-viewer-web-dist.sh" --dist-dir "$rebuilt_dir" >&2; then
      echo "error: viewer web dist copy failed" >&2
      return 1
    fi
  else
    cp "$repo_root/crates/oasis7_viewer/viewer.html" "$rebuilt_dir/index.html"
    cp "$repo_root/crates/oasis7_viewer/viewer.html" "$rebuilt_dir/viewer.html"
    cp "$repo_root/crates/oasis7_viewer/dist/software_safe.html" "$rebuilt_dir/software_safe.html"
    cp "$repo_root/crates/oasis7_viewer/dist/viewer.js" "$rebuilt_dir/viewer.js"
    cp "$repo_root/crates/oasis7_viewer/dist/software_safe.js" "$rebuilt_dir/software_safe.js"
    cp "$repo_root/crates/oasis7_viewer/viewer_first_agent_claim_evidence.html" \
      "$rebuilt_dir/viewer_first_agent_claim_evidence.html"
    cp "$repo_root/crates/oasis7_viewer/dist/software_safe_first_agent_claim_evidence.html" \
      "$rebuilt_dir/software_safe_first_agent_claim_evidence.html"
    cp "$repo_root/crates/oasis7_viewer/favicon.ico" "$rebuilt_dir/favicon.ico"
    if [[ -d "$repo_root/crates/oasis7_viewer/dist/pixel-world-bridge" ]]; then
      rm -rf "$rebuilt_dir/pixel-world-bridge"
      cp -R "$repo_root/crates/oasis7_viewer/dist/pixel-world-bridge" "$rebuilt_dir/pixel-world-bridge"
    fi
  fi
  viewer_web_dist_write_manifest "$repo_root" "$rebuilt_dir"
  printf '%s
' "$rebuilt_dir"
}
