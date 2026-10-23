#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"
source "$repo_root/scripts/agent-browser-lib.sh"
source "$repo_root/scripts/cargo-dev-lib.sh"
source "$repo_root/scripts/hosted-login-gate-env-lib.sh"
source "$repo_root/scripts/release-web-fixture-lib.sh"

usage() {
  cat <<'USAGE'
Usage: ./scripts/viewer-primary-web-entry-regression.sh [options]

Run the shipped Hosted Web entry against an isolated signed WorldService QA fixture:
- default browser entry (`/`) must land in `viewer`
- `render_mode=auto` must still land in `viewer`

Options:
  --live-bind <host:port>    live tcp bind (default: 127.0.0.1:5023)
  --web-bind <host:port>     web bridge bind (default: 127.0.0.1:5011)
  --viewer-host <host>       web viewer host (default: 127.0.0.1)
  --viewer-port <port>       web viewer port (default: 4173)
  --viewer-static-dir <dir>  viewer test artifact dir (default: crates/oasis7_viewer/.viewer-test-dist)
  --out-dir <path>           artifact root (default: output/playwright/viewer-primary-web-entry)
  --headed                   open browser in headed mode
  --headless                 open browser in headless mode (default)
  -h, --help                 show this help

Artifacts:
  <out-dir>/<run-id>/launcher.log
  <out-dir>/<run-id>/default_state.json
  <out-dir>/<run-id>/auto_state.json
  <out-dir>/<run-id>/*.png
  <out-dir>/<run-id>/summary.json
  <out-dir>/<run-id>/summary.md
USAGE
}

sleep_ms() {
  python3 - "$1" <<'PY'
import sys
import time
time.sleep(int(sys.argv[1]) / 1000.0)
PY
}

wait_for_port() {
  local host=$1
  local port=$2
  local timeout_secs=${3:-120}
  local step
  for step in $(seq 1 "$timeout_secs"); do
    release_web_check_startup || return 1
    if python3 - "$host" "$port" <<'PY'
import socket
import sys
host = sys.argv[1]
port = int(sys.argv[2])
try:
    with socket.create_connection((host, port), timeout=1):
        pass
except OSError:
    raise SystemExit(1)
raise SystemExit(0)
PY
    then
      return 0
    fi
    sleep 1
  done
  return 1
}

wait_for_http() {
  local url=$1
  local timeout_secs=${2:-120}
  local step
  for step in $(seq 1 "$timeout_secs"); do
    release_web_check_startup || return 1
    local status
    status=$(curl --max-time 3 --silent --output /dev/null --write-out '%{http_code}' "$url") || status=000
    case "$status" in
      2*|3*) return 0 ;;
      404)
        echo "error: owned viewer entry returned HTTP 404: $url" >&2
        return 1
        ;;
    esac
    sleep 1
  done
  return 1
}

print_launcher_failure_context() {
  local reason=$1
  echo "error: $reason" >&2
  if [[ -n "${launcher_pid:-}" ]]; then
    if kill -0 "$launcher_pid" >/dev/null 2>&1; then
      echo "launcher process is still running: pid=$launcher_pid" >&2
    else
      set +e
      wait "$launcher_pid" >/dev/null 2>&1
      local launcher_status=$?
      set -e
      echo "launcher process exited: status=$launcher_status" >&2
      launcher_pid=""
    fi
  fi
  if [[ -f "${launcher_log:-}" ]]; then
    echo "launcher log tail (${launcher_log}):" >&2
    tail -n 160 "$launcher_log" >&2 || true
  else
    echo "launcher log not found: ${launcher_log:-unset}" >&2
  fi
}

normalize_eval_token() {
  local raw=${1:-}
  raw=$(printf '%s' "$raw" | tr -d '\r\n')
  raw=${raw#\"}
  raw=${raw%\"}
  printf '%s' "$raw"
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
    payload = json.loads(raw)
except Exception:
    out.write_text(raw + "\n", encoding="utf-8")
else:
    out.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
PY
}

ab_state() {
  ab_eval "$1" 'window.__AW_TEST__?.getState?.() ?? null'
}

state_connection() { json_get "$1" connectionStatus; }
state_render_mode() { json_get "$1" renderMode; }
state_reason() { json_get "$1" viewerReason; }
state_selected_kind() { json_get "$1" selectedKind; }
state_last_error() { json_get "$1" lastError; }

wait_for_api() {
  local session=$1
  local timeout_ms=${2:-30000}
  local deadline=$((SECONDS * 1000 + timeout_ms))
  while (( SECONDS * 1000 < deadline )); do
    local ready
    ready=$(normalize_eval_token "$(ab_eval "$session" 'typeof window.__AW_TEST__ === "object" ? "ready" : "missing"')")
    if [[ "$ready" == "ready" || "$ready" == "true" ]]; then
      return 0
    fi
    sleep_ms 200
  done
  return 1
}

wait_for_connected() {
  local session=$1
  local timeout_ms=${2:-30000}
  local deadline=$((SECONDS * 1000 + timeout_ms))
  local state='null'
  while (( SECONDS * 1000 < deadline )); do
    state=$(ab_state "$session")
    if [[ "$(state_connection "$state")" == "connected" ]]; then
      printf '%s\n' "$state"
      return 0
    fi
    sleep_ms 250
  done
  printf '%s\n' "$state"
  return 1
}

wait_for_selected_kind() {
  local session=$1
  local expected_kind=$2
  local timeout_ms=${3:-10000}
  local deadline=$((SECONDS * 1000 + timeout_ms))
  local state='null'
  while (( SECONDS * 1000 < deadline )); do
    state=$(ab_state "$session")
    if [[ "$(state_selected_kind "$state")" == "$expected_kind" ]]; then
      printf '%s\n' "$state"
      return 0
    fi
    sleep_ms 250
  done
  printf '%s\n' "$state"
  return 1
}

assert_eval_true() {
  local session=$1
  local script=$2
  local timeout_ms=${3:-8000}
  local deadline=$((SECONDS * 1000 + timeout_ms))
  while (( SECONDS * 1000 < deadline )); do
    local value
    value=$(normalize_eval_token "$(ab_eval "$session" "$script")")
    if [[ "$value" == "true" ]]; then
      return 0
    fi
    sleep_ms 250
  done
  return 1
}

# Validate the current Player HUD, rather than a retired introductory heading.
primary_entry_surface_predicate() {
  cat <<'JS'
(() => {
  const visible = element => Boolean(element && element.getClientRects().length
    && getComputedStyle(element).display !== "none"
    && getComputedStyle(element).visibility !== "hidden");
  const area = document.querySelector('[data-viewer-decision-area="true"]');
  const actions = area?.querySelectorAll('[data-primary-action="true"]');
  const action = actions?.length === 1 ? actions[0] : null;
  const context = area?.querySelector('[data-shell-region="supporting-context"]');
  const receipt = area?.querySelector('[data-viewer-overlay="receipt"]');
  return Boolean([area, action, context, receipt].every(visible)
    && [action, context, receipt].every(element => element.textContent.trim())
    && action.getAttribute("href")?.trim());
})()
JS
}

enter_primary_player_surface() {
  local session=$1 label=$2
  assert_eval_true "$session" '(() => {
    const gate = document.querySelector("[data-viewer-fixture-state=hosted_login_gate]");
    return window.__AW_TEST__.getState().authReady === false && Boolean(gate?.getClientRects().length);
  })()' 15000 || { echo "error: $label missing guest login gate" >&2; return 1; }
  capture_screenshot_best_effort "$session" "$out_dir/$label-login.png" "$out_dir/$label-login.screenshot.log" "$label guest login" >/dev/null
  # Exercise the existing loopback-only backend issuer through its visible UI.
  ab_cmd "$session" click '[data-auth-action="test-login"]' >>"$out_dir/test-login.log" 2>&1 || return 1
  assert_eval_true "$session" '(() => {
    const state = window.__AW_TEST__.getState();
    return state.authReady === true && state.authSource === "hosted_player_issue+browser_local_device_session"
      && state.authTier === "player_session" && Boolean(state.authPlayerId)
      && !document.querySelector("[data-viewer-fixture-state=hosted_login_gate]");
  })()' 15000 || { json_to_file "$(ab_state "$session")" "$out_dir/$label-login-failed-state.json"; echo "error: $label QA session was not issued through test login" >&2; return 1; }
}

assert_body_contains() {
  local body_path=$1
  local expected=$2
  local label=$3
  grep -Fqi "$expected" "$body_path" || {
    echo "error: ${label} body missing ${expected}" >&2
    exit 1
  }
}

capture_screenshot_best_effort() {
  local session=$1
  local screenshot_path=$2
  local log_path=$3
  local label=$4
  local status=0
  local timeout_secs=${VIEWER_PRIMARY_ENTRY_SCREENSHOT_TIMEOUT_SECS:-20}
  local waited_ms=0
  local timeout_ms
  local screenshot_pid

  if [[ ! "$timeout_secs" =~ ^[0-9]+$ || "$timeout_secs" -lt 1 ]]; then
    timeout_secs=20
  fi
  timeout_ms=$((timeout_secs * 1000))

  ab_screenshot "$session" "$screenshot_path" >"$log_path" 2>&1 &
  screenshot_pid=$!
  while kill -0 "$screenshot_pid" >/dev/null 2>&1; do
    if (( waited_ms >= timeout_ms )); then
      status=124
      echo "warning: ${label} screenshot timed out after ${timeout_secs}s; continuing after hard state/body assertions" >&2
      {
        echo
        echo "warning: screenshot timed out after ${timeout_secs}s"
      } >>"$log_path"
      kill "$screenshot_pid" >/dev/null 2>&1 || true
      sleep_ms 200
      kill -9 "$screenshot_pid" >/dev/null 2>&1 || true
      wait "$screenshot_pid" >/dev/null 2>&1 || true
      break
    fi
    sleep_ms 200
    waited_ms=$((waited_ms + 200))
  done

  if [[ "$status" -eq 0 ]]; then
    if wait "$screenshot_pid"; then
      status=0
    else
      status=$?
      echo "warning: ${label} screenshot failed with status ${status}; continuing after hard state/body assertions" >&2
    fi
  fi

  if [[ -f "$screenshot_path" ]]; then
    printf 'present'
  elif [[ "$status" -eq 0 ]]; then
    echo "warning: ${label} screenshot command completed but no file was written at ${screenshot_path}; continuing" >&2
    printf 'missing'
  else
    printf 'failed'
  fi
}

live_bind="127.0.0.1:5023"
web_bind="127.0.0.1:5011"
viewer_host="127.0.0.1"
viewer_port="4173"
viewer_static_dir="$repo_root/crates/oasis7_viewer/.viewer-test-dist"
out_root="output/playwright/viewer-primary-web-entry"
headed=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --live-bind)
      live_bind=${2:-}
      shift 2
      ;;
    --web-bind)
      web_bind=${2:-}
      shift 2
      ;;
    --viewer-host)
      viewer_host=${2:-}
      shift 2
      ;;
    --viewer-port)
      viewer_port=${2:-}
      shift 2
      ;;
    --viewer-static-dir)
      viewer_static_dir=${2:-}
      shift 2
      ;;
    --out-dir)
      out_root=${2:-}
      shift 2
      ;;
    --headed)
      headed=1
      shift
      ;;
    --headless)
      headed=0
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

ab_require

case "$viewer_host" in
  127.0.0.1|localhost|::1|'[::1]') ;;
  *) echo "error: primary entry QA test login requires a loopback viewer host" >&2; exit 2 ;;
esac

if [[ "$web_bind" != *:* ]]; then
  echo "error: --web-bind must be in <host:port> format" >&2
  exit 2
fi

web_bind_host=${web_bind%:*}
web_bind_port=${web_bind##*:}
if [[ -z "$web_bind_host" || -z "$web_bind_port" || ! "$web_bind_port" =~ ^[0-9]+$ ]]; then
  echo "error: invalid --web-bind: $web_bind" >&2
  exit 2
fi

mkdir -p "$out_root"
stamp=$(date '+%Y%m%d-%H%M%S')
run_id="viewer-primary-web-entry-${stamp}"
out_dir="$out_root/$run_id"
mkdir -p "$out_dir"

launcher_log="$out_dir/launcher.log"
default_state_path="$out_dir/default_state.json"
auto_state_path="$out_dir/auto_state.json"
default_body_path="$out_dir/default_body.txt"
auto_body_path="$out_dir/auto_body.txt"
default_screenshot_path="$out_dir/default-entry.png"
auto_screenshot_path="$out_dir/auto-entry.png"
default_screenshot_log_path="$out_dir/default-entry.screenshot.log"
auto_screenshot_log_path="$out_dir/auto-entry.screenshot.log"
summary_json_path="$out_dir/summary.json"
summary_md_path="$out_dir/summary.md"
hosted_account_store_path="$out_dir/hosted-account-store.json"
launcher_env_defaults=()
while IFS= read -r env_default; do
  launcher_env_defaults+=("$env_default")
done < <(oasis7_hosted_login_gate_env_defaults "$hosted_account_store_path")
launcher_env_defaults+=(
  "OASIS7_HOSTED_TEST_LOGIN_ENABLED=1"
  "OASIS7_HOSTED_PLAYER_SESSION_LEDGER_PATH=$out_dir/hosted-player-sessions.json"
  "OASIS7_HOSTED_REGISTRATION_REPLAY_LEDGER_PATH=$out_dir/hosted-registration-replay.json"
)

if [[ "$viewer_static_dir" == "$repo_root/crates/oasis7_viewer/.viewer-test-dist" || "$viewer_static_dir" == "crates/oasis7_viewer/.viewer-test-dist" ]]; then
  release_web_fixture_build_viewer "$out_dir/viewer-build.log"
fi

resolved_viewer_static_dir=$(resolve_viewer_static_dir_for_web_closure \
  "$repo_root" \
  "$viewer_static_dir" \
  "$out_dir")
viewer_static_dir="$resolved_viewer_static_dir"

live_args=(
  "--live-bind" "$live_bind"
  "--web-bind" "$web_bind"
  "--viewer-host" "$viewer_host"
  "--viewer-port" "$viewer_port"
  "--viewer-static-dir" "$viewer_static_dir"
  "--no-open-browser"
)
root_config_path="$repo_root/config.toml"
root_config_existed=0
if [[ -f "$root_config_path" ]]; then
  root_config_existed=1
fi

launcher_pid=""
cleanup() {
  set +e
  if [[ -n "$launcher_pid" ]]; then
    kill "$launcher_pid" >/dev/null 2>&1 || true
    wait "$launcher_pid" >/dev/null 2>&1 || true
  fi
  if [[ "$root_config_existed" -eq 0 && -f "$root_config_path" ]]; then
    rm -f "$root_config_path" >/dev/null 2>&1 || true
  fi
  release_web_fixture_stop
  ab_cmd "${run_id}-default" close >/dev/null 2>&1 || true
  ab_cmd "${run_id}-auto" close >/dev/null 2>&1 || true
}
trap cleanup EXIT

release_web_fixture_build "$out_dir/fixture-build.json"

echo "+ oasis7_cargo_dev build -p oasis7_launcher_backend -p oasis7_viewer_runtime --bin oasis7_viewer_live --bin oasis7_game_launcher"
if ! OASIS7_CARGO_DEV_REPO_ROOT="$repo_root" oasis7_cargo_dev build -p oasis7_launcher_backend -p oasis7_viewer_runtime --bin oasis7_viewer_live --bin oasis7_game_launcher >>"$launcher_log" 2>&1; then
  print_launcher_failure_context "launcher prebuild failed"
  exit 1
fi

launcher_binary="$(oasis7_cargo_dev_debug_bin_dir "$repo_root")/oasis7_game_launcher"
if [[ ! -x "$launcher_binary" && -x "$launcher_binary.exe" ]]; then launcher_binary+=".exe"; fi
[[ -x "$launcher_binary" ]] || { echo "error: actual built launcher executable missing: $launcher_binary" >&2; exit 1; }
release_web_fixture_start "$out_dir"

echo "+ $launcher_binary ${live_args[*]}"
{
  echo
  echo "+ $launcher_binary ${live_args[*]}"
} >>"$launcher_log"
for launcher_env_default in "${launcher_env_defaults[@]}"; do
  export "$launcher_env_default"
done
"$launcher_binary" "${live_args[@]}" >>"$launcher_log" 2>&1 &
launcher_pid=$!

if ! wait_for_port "$web_bind_host" "$web_bind_port" 240; then
  print_launcher_failure_context "web bridge did not come up on $web_bind"
  exit 1
fi
if ! wait_for_http "http://${viewer_host}:${viewer_port}/" 240; then
  print_launcher_failure_context "viewer server did not become ready"
  exit 1
fi

base_query="ws=$(python3 -c 'import sys, urllib.parse; print(urllib.parse.quote(sys.argv[1], safe=""))' "ws://${web_bind}")&test_api=1&hosted_test_login=1"
default_url="http://${viewer_host}:${viewer_port}/?${base_query}"
auto_url="http://${viewer_host}:${viewer_port}/?${base_query}&render_mode=auto"
default_session="${run_id}-default"
auto_session="${run_id}-auto"

ab_open "$default_session" "$headed" "$default_url" >/dev/null
ab_cmd "$default_session" wait --load networkidle >/dev/null 2>&1 || true
wait_for_api "$default_session" 60000 || { echo "error: default route missing __AW_TEST__" >&2; exit 1; }
default_state=$(wait_for_connected "$default_session" 30000) || { json_to_file "$default_state" "$default_state_path"; ab_cmd "$default_session" get text body >"$default_body_path"; echo "error: default route did not connect; inspect $default_state_path" >&2; exit 1; }
enter_primary_player_surface "$default_session" default
# Preserve the post-login public state alongside the visible HUD proof.
default_state=$(ab_state "$default_session")
json_to_file "$default_state" "$default_state_path"
ab_cmd "$default_session" get text body >"$default_body_path"
default_screenshot_status=$(capture_screenshot_best_effort "$default_session" "$default_screenshot_path" "$default_screenshot_log_path" "default entry")
default_url_final=$(normalize_eval_token "$(ab_cmd "$default_session" get url)")

[[ "$(state_render_mode "$default_state")" == "viewer" ]] || { echo "error: default route did not land in viewer" >&2; exit 1; }
[[ "$(state_reason "$default_state")" == "direct_viewer_entry" ]] || { echo "error: default route reason mismatch: $(state_reason "$default_state")" >&2; exit 1; }
[[ "$default_url_final" == "http://${viewer_host}:${viewer_port}/"* ]] || { echo "error: default route escaped viewer host: $default_url_final" >&2; exit 1; }
assert_eval_true "$default_session" "$(primary_entry_surface_predicate)" 15000 || {
  echo "error: default route missing visible Player HUD action/context/receipt" >&2
  exit 1
}
assert_body_contains "$default_body_path" "NEXT MOVE" "default route"
assert_body_contains "$default_body_path" "PLAYER LEVERAGE" "default route"
assert_body_contains "$default_body_path" "OBJECTIVE" "default route"
assert_body_contains "$default_body_path" "ACTION RECEIPT" "default route"

ab_open "$auto_session" "$headed" "$auto_url" >/dev/null
ab_cmd "$auto_session" wait --load networkidle >/dev/null 2>&1 || true
wait_for_api "$auto_session" 60000 || { echo "error: auto route missing __AW_TEST__" >&2; exit 1; }
auto_state=$(wait_for_connected "$auto_session" 30000) || { json_to_file "$auto_state" "$auto_state_path"; ab_cmd "$auto_session" get text body >"$auto_body_path"; echo "error: auto route did not connect; inspect $auto_state_path" >&2; exit 1; }
enter_primary_player_surface "$auto_session" auto
auto_state=$(ab_state "$auto_session")
json_to_file "$auto_state" "$auto_state_path"
ab_cmd "$auto_session" get text body >"$auto_body_path"
auto_screenshot_status=$(capture_screenshot_best_effort "$auto_session" "$auto_screenshot_path" "$auto_screenshot_log_path" "auto entry")
auto_url_final=$(normalize_eval_token "$(ab_cmd "$auto_session" get url)")

[[ "$(state_render_mode "$auto_state")" == "viewer" ]] || { echo "error: auto route did not land in viewer" >&2; exit 1; }
[[ "$(state_reason "$auto_state")" == "direct_viewer_entry" ]] || { echo "error: auto route reason mismatch: $(state_reason "$auto_state")" >&2; exit 1; }
[[ "$auto_url_final" == "http://${viewer_host}:${viewer_port}/"* ]] || { echo "error: auto route escaped viewer host: $auto_url_final" >&2; exit 1; }
assert_eval_true "$auto_session" "$(primary_entry_surface_predicate)" 15000 || {
  echo "error: auto route missing visible Player HUD action/context/receipt" >&2
  exit 1
}

python3 - "$summary_json_path" <<'PY' "$default_state_path" "$auto_state_path" "$default_url_final" "$auto_url_final" "$default_screenshot_path" "$auto_screenshot_path" "$default_screenshot_status" "$auto_screenshot_status" "$default_screenshot_log_path" "$auto_screenshot_log_path"
import json
import pathlib
import sys

def load(path):
    return json.loads(pathlib.Path(path).read_text(encoding="utf-8"))

def screenshot_record(path, status, log_path):
    screenshot_path = pathlib.Path(path)
    return {
        "status": status,
        "path": str(screenshot_path) if screenshot_path.is_file() else None,
        "requested_path": path,
        "log": log_path,
    }

summary = {
    "ok": True,
    "default_entry": {
        "final_url": sys.argv[4],
        "state": load(sys.argv[2]),
        "screenshot": screenshot_record(sys.argv[6], sys.argv[8], sys.argv[10]),
    },
    "auto_entry": {
        "final_url": sys.argv[5],
        "state": load(sys.argv[3]),
        "screenshot": screenshot_record(sys.argv[7], sys.argv[9], sys.argv[11]),
    },
}
pathlib.Path(sys.argv[1]).write_text(json.dumps(summary, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
PY

python3 - "$summary_json_path" "$summary_md_path" <<'PY'
import json
import pathlib
import sys

summary = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
default_state = summary["default_entry"]["state"]
auto_state = summary["auto_entry"]["state"]
default_screenshot = summary["default_entry"]["screenshot"]
auto_screenshot = summary["auto_entry"]["screenshot"]
lines = [
    "# Viewer primary Web entry regression summary",
    "",
    "## Verdict",
    "- Overall: `pass`",
    "- Formal gameplay entry (`/`): `viewer`",
    "- Auto entry (`?render_mode=auto`): `viewer`",
    "",
    "## Default entry",
    f"- Final URL: `{summary['default_entry']['final_url']}`",
    f"- Render mode: `{default_state.get('renderMode')}`",
    f"- Entry reason: `{default_state.get('viewerReason') or default_state.get('softwareSafeReason')}`",
    f"- Screenshot: `{default_screenshot.get('status')}`",
    f"- Screenshot path: `{default_screenshot.get('path') or default_screenshot.get('requested_path')}`",
    f"- Screenshot log: `{default_screenshot.get('log')}`",
    "",
    "## Auto entry",
    f"- Final URL: `{summary['auto_entry']['final_url']}`",
    f"- Render mode: `{auto_state.get('renderMode')}`",
    f"- Entry reason: `{auto_state.get('viewerReason') or auto_state.get('softwareSafeReason')}`",
    f"- Screenshot: `{auto_screenshot.get('status')}`",
    f"- Screenshot path: `{auto_screenshot.get('path') or auto_screenshot.get('requested_path')}`",
    f"- Screenshot log: `{auto_screenshot.get('log')}`",
]
pathlib.Path(sys.argv[2]).write_text("\n".join(lines) + "\n", encoding="utf-8")
PY

printf 'ok: artifacts written to %s\n' "$out_dir"
