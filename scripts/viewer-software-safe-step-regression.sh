#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"
source "$repo_root/scripts/cargo-dev-lib.sh"
source "$repo_root/scripts/release-web-fixture-lib.sh"
source "$repo_root/scripts/agent-browser-lib.sh"

prepare_release_web_stack() {
  release_web_fixture_build_viewer "$out_dir/viewer-build.log" || return $?
  release_web_fixture_build "$out_dir/fixture-build.json" || return $?
  if [[ "$BOOTSTRAP_USES_BUNDLE" -ne 1 ]]; then
    log_note build_release_web_stack
    # Match run-launcher-stack --chain-disable's complete source build before
    # starting the bounded fixture, rather than compiling inside its deadline.
    oasis7_cargo_dev build -p oasis7_provider_tools -p oasis7_launcher_backend -p oasis7_viewer_runtime \
      --bin oasis7_llm_provider_probe --bin oasis7_game_launcher --bin oasis7_viewer_live >>"$ab_log" 2>&1 || return $?
  fi
  release_web_fixture_start "$out_dir" || return $?
  if [[ "$BOOTSTRAP_USES_BUNDLE" -ne 1 ]]; then
    export OASIS7_RUN_LAUNCHER_STACK_SKIP_SOURCE_BUILD=1
  fi
}

usage() {
  cat <<'USAGE'
Usage: ./scripts/viewer-software-safe-step-regression.sh [options] [run-launcher-stack options...]

Run a Web-first QA regression for the canonical Viewer realtime-only closure.
The script forces `render_mode=viewer` and still accepts the `software_safe` compatibility alias, then verifies:
- page load + `__AW_TEST__` availability
- runtime connection reaches `connected`
- target agent selection is reflected in state + DOM
- realtime surfaces remain readable after selection
- world state advances naturally through live snapshot/metrics updates, or the page explicitly surfaces a gameplay blocker without exposing playback controls

Options:
  --url <url>               Use an existing viewer URL; skip stack bootstrap
  --out-dir <path>          Artifact root (default: output/playwright/viewer-software-safe-step)
  --startup-timeout <secs>  Wait timeout for stack URL (default: 240)
  --release-web-fixture     Use isolated signed public-scope QA backend (release gate only)
  --agent-id <id>           Target agent id (default: starter-agent-0)
  --progress-timeout-ms <ms> Wait timeout for natural live progress (default: 15000)
  --headed                  Open browser in headed mode
  --headless                Open browser in headless mode (default)
  -h, --help                Show this help

If --url is omitted, the script starts:
  ./scripts/run-launcher-stack.sh [remaining args...]

Artifacts:
  <out-dir>/<run-id>/launcher-stack.log
  <out-dir>/<run-id>/agent-browser.log
  <out-dir>/<run-id>/browser_env.json
  <out-dir>/<run-id>/initial_state.json
  <out-dir>/<run-id>/after_select_state.json
  <out-dir>/<run-id>/after_progress_state.json
  <out-dir>/<run-id>/final_state.json
  <out-dir>/<run-id>/software-safe-step-summary.json
  <out-dir>/<run-id>/software-safe-step-summary.md
USAGE
}

sleep_ms() {
  python3 - "$1" <<'PY'
import sys, time
time.sleep(int(sys.argv[1]) / 1000.0)
PY
}

append_query_params() {
  python3 - "$1" <<'PY'
from urllib.parse import urlparse, parse_qsl, urlencode, urlunparse
import sys
raw = sys.argv[1]
parts = urlparse(raw)
query = dict(parse_qsl(parts.query, keep_blank_values=True))
query["render_mode"] = "viewer"
query["test_api"] = "1"
print(urlunparse(parts._replace(query=urlencode(query))))
PY
}

extract_ws_host_port() {
  python3 - "$1" <<'PY'
from urllib.parse import urlparse, parse_qs
import sys
raw = sys.argv[1]
parts = urlparse(raw)
ws_values = parse_qs(parts.query).get("ws", [])
if not ws_values:
    raise SystemExit(1)
ws = urlparse(ws_values[0])
host = ws.hostname or ""
port = ws.port or 0
if not host or not port:
    raise SystemExit(1)
print(f"{host} {port}")
PY
}

wait_for_tcp_listener() {
  local host=$1
  local port=$2
  local timeout_secs=${3:-20}
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

normalize_eval_token() {
  local raw=${1:-}
  raw=$(printf '%s' "$raw" | tr -d '\r\n')
  raw=${raw#\"}
  raw=${raw%\"}
  printf '%s' "$raw"
}

log_note() {
  printf '### [%s] %s\n' "$1" "$(date '+%H:%M:%S')" | tee -a "$ab_log" >/dev/null
}

ab_state() {
  ab_eval "$session" 'window.__AW_TEST__?.getState?.() ?? null'
}

state_connection() { json_get "$1" connectionStatus; }
state_render_mode() { json_get "$1" renderMode; }
state_last_error() { json_get "$1" lastError; }
state_logical_time() { json_get "$1" logicalTime; }
state_event_seq() { json_get "$1" eventSeq; }
state_blocker_kind() { json_get "$1" gameplaySummary.blockerKind; }
state_blocker_detail() { json_get "$1" gameplaySummary.blockerDetail; }
state_stage_status() { json_get "$1" gameplaySummary.stageStatus; }
state_last_control_stage() { json_get "$1" lastControlFeedback.stage; }
state_execution_state() { json_get "$1" gameplaySummary.executionState; }
state_recent_feedback_stage() { json_get "$1" gameplaySummary.recentFeedback.stage; }
state_recent_feedback_reason() { json_get "$1" gameplaySummary.recentFeedback.reason; }
state_narrative_blocker_detail() { json_get "$1" gameplaySummary.narrativeBlockerDetail; }

refresh_explicit_blocker_state() {
  local state=$1
  stage_status=$(state_stage_status "$state")
  blocker_kind=$(state_blocker_kind "$state")
  blocker_detail=$(state_blocker_detail "$state")
  execution_state=$(state_execution_state "$state")
  recent_feedback_stage=$(state_recent_feedback_stage "$state")
  recent_feedback_reason=$(state_recent_feedback_reason "$state")
  narrative_blocker_detail=$(state_narrative_blocker_detail "$state")
  expected_blocker_status=""
  expected_blocker_detail=""
  if [[ -n "${blocker_detail:-}" && "$blocker_detail" != "null" ]]; then
    expected_blocker_detail="$blocker_detail"
  elif [[ -n "${narrative_blocker_detail:-}" && "$narrative_blocker_detail" != "null" ]]; then
    expected_blocker_detail="$narrative_blocker_detail"
  elif [[ -n "${recent_feedback_reason:-}" && "$recent_feedback_reason" != "null" ]]; then
    expected_blocker_detail="$recent_feedback_reason"
  fi
  explicit_blocker_state=false
  if [[ -n "${blocker_kind:-}" && "$blocker_kind" != "null" ]]; then
    explicit_blocker_state=true
  elif [[ "${stage_status:-}" == "blocked" ]]; then
    explicit_blocker_state=true
    expected_blocker_status="blocked"
  elif [[ "${execution_state:-}" == "blocked" ]]; then
    explicit_blocker_state=true
    expected_blocker_status="blocked"
  elif [[ "${recent_feedback_stage:-}" == "blocked" ]]; then
    explicit_blocker_state=true
    expected_blocker_status="blocked"
  elif [[ -n "${narrative_blocker_detail:-}" && "$narrative_blocker_detail" != "null" ]]; then
    explicit_blocker_state=true
    expected_blocker_status="blocked"
  elif [[ -n "${recent_feedback_reason:-}" && "$recent_feedback_reason" != "null" ]]; then
    explicit_blocker_state=true
    expected_blocker_status="blocked"
  fi
}

wait_for_api() {
  local timeout_ms=${1:-20000}
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
  local timeout_ms=${1:-20000}
  local deadline=$((SECONDS * 1000 + timeout_ms))
  local state='null'
  local last_error=''
  while (( SECONDS * 1000 < deadline )); do
    state=$(ab_state)
    last_error=$(state_last_error "$state")
    if [[ -n "$last_error" ]]; then
      printf '%s\n' "$state"
      return 2
    fi
    if [[ "$(state_connection "$state")" == "connected" ]]; then
      printf '%s\n' "$state"
      return 0
    fi
    sleep_ms 250
  done
  printf '%s\n' "$state"
  return 1
}

wait_for_js_true() {
  local script=$1
  local timeout_ms=${2:-10000}
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

write_json_file() {
  local raw_json=$1
  local out_path=$2
  json_to_file "$raw_json" "$out_path"
}

browser_env() {
  ab_eval "$session" '(() => {
    const canvas = document.createElement("canvas");
    let gl = null;
    let renderer = null;
    let vendor = null;
    let webglVersion = null;
    try {
      gl = canvas.getContext("webgl") || canvas.getContext("experimental-webgl");
      if (gl) {
        webglVersion = gl.getParameter(gl.VERSION);
        const dbg = gl.getExtension("WEBGL_debug_renderer_info");
        if (dbg) {
          renderer = gl.getParameter(dbg.UNMASKED_RENDERER_WEBGL);
          vendor = gl.getParameter(dbg.UNMASKED_VENDOR_WEBGL);
        }
      }
    } catch (err) {
      renderer = `error:${String(err)}`;
    }
    return {
      userAgent: navigator.userAgent,
      visibilityState: document.visibilityState,
      hasTestApi: typeof window.__AW_TEST__ === "object",
      state: window.__AW_TEST__?.getState?.() ?? null,
      webglVersion,
      renderer,
      vendor,
    };
  })()'
}

summary_json() {
  python3 - "$@" <<'PY'
import json
import sys
payload = {
    "ok": sys.argv[1] == "true",
    "failCategory": None if sys.argv[2] == "null" else sys.argv[2],
    "runId": sys.argv[3],
    "agentId": sys.argv[4],
    "gameUrl": sys.argv[5],
    "renderMode": sys.argv[6],
    "selectedAgentVisible": sys.argv[7] == "true",
    "playbackControlsVisible": sys.argv[8] == "true",
    "logicalTimeAdvanced": sys.argv[9] == "true",
    "eventSeqAdvanced": sys.argv[10] == "true",
    "autoProgressObserved": sys.argv[11] == "true",
    "blockerDomVisible": None if sys.argv[12] == "null" else sys.argv[12] == "true",
    "stageStatus": None if sys.argv[13] == "null" else sys.argv[13],
    "blockerKind": None if sys.argv[14] == "null" else sys.argv[14],
    "blockerDetail": None if sys.argv[15] == "null" else sys.argv[15],
    "expectedBlockerDetail": None if sys.argv[16] == "null" else sys.argv[16],
}
print(json.dumps(payload, ensure_ascii=False, indent=2))
PY
}

GAME_URL=""
OUT_ROOT="output/playwright/viewer-software-safe-step"
STARTUP_TIMEOUT_SECS=240
RELEASE_WEB_FIXTURE=0
AGENT_ID="starter-agent-0"
PROGRESS_TIMEOUT_MS=15000
HEADED=0
STACK_ARGS=()
BOOTSTRAP_USES_BUNDLE=0
stack_pid=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --url)
      GAME_URL="${2:-}"
      shift 2
      ;;
    --out-dir)
      OUT_ROOT="${2:-}"
      shift 2
      ;;
    --startup-timeout)
      STARTUP_TIMEOUT_SECS="${2:-}"
      shift 2
      ;;
    --release-web-fixture)
      RELEASE_WEB_FIXTURE=1
      shift
      ;;
    --agent-id)
      AGENT_ID="${2:-}"
      shift 2
      ;;
    --progress-timeout-ms)
      PROGRESS_TIMEOUT_MS="${2:-}"
      shift 2
      ;;
    --step-timeout-ms)
      echo "error: unsupported option --step-timeout-ms; use --progress-timeout-ms" >&2
      exit 2
      ;;
    --headed)
      HEADED=1
      shift
      ;;
    --headless)
      HEADED=0
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    --bundle-dir)
      BOOTSTRAP_USES_BUNDLE=1
      STACK_ARGS+=("$1" "${2:-}")
      shift 2
      ;;
    *)
      STACK_ARGS+=("$1")
      shift
      ;;
  esac
done

if [[ "$RELEASE_WEB_FIXTURE" -eq 1 && -n "$GAME_URL" ]]; then
  echo "error: --release-web-fixture cannot be used with --url" >&2
  exit 2
fi

if [[ "$RELEASE_WEB_FIXTURE" -eq 1 ]]; then
    for stack_arg in "${STACK_ARGS[@]}"; do
      case "$stack_arg" in
        --chain-enable|--chain-status-bind|--chain-execution-world-dir|--chain-local-standalone-test|--scenario|--allow-debug-scenario)
          echo "error: $stack_arg is incompatible with the isolated signed Web QA fixture" >&2
          exit 2
          ;;
      esac
    done
fi

[[ -n "$OUT_ROOT" ]] || { echo "error: --out-dir cannot be empty" >&2; exit 2; }
[[ "$STARTUP_TIMEOUT_SECS" =~ ^[0-9]+$ ]] && [[ "$STARTUP_TIMEOUT_SECS" -gt 0 ]] || { echo "error: --startup-timeout must be positive" >&2; exit 2; }
[[ "$PROGRESS_TIMEOUT_MS" =~ ^[0-9]+$ ]] && [[ "$PROGRESS_TIMEOUT_MS" -gt 0 ]] || { echo "error: --progress-timeout-ms must be positive" >&2; exit 2; }

require_cmd python3
: "${AGENT_BROWSER_ARGS:=}"
ab_require

run_id="$(date +%Y%m%d-%H%M%S)"
out_dir="$OUT_ROOT/$run_id"
mkdir -p "$out_dir"

ab_log="$out_dir/agent-browser.log"
run_game_test_log="$out_dir/launcher-stack.log"
browser_env_json="$out_dir/browser_env.json"
initial_state_json="$out_dir/initial_state.json"
after_select_state_json="$out_dir/after_select_state.json"
after_progress_state_json="$out_dir/after_progress_state.json"
final_state_json="$out_dir/final_state.json"
summary_json_path="$out_dir/software-safe-step-summary.json"
summary_md_path="$out_dir/software-safe-step-summary.md"
screenshot_path="$out_dir/software-safe-step.png"
session="viewer-softsafe-step-$run_id"

stop_stack() {
  if [[ -n "$stack_pid" ]] && kill -0 "$stack_pid" >/dev/null 2>&1; then
    kill "$stack_pid" >/dev/null 2>&1 || true
    wait "$stack_pid" >/dev/null 2>&1 || true
  fi
  stack_pid=""
}

cleanup() {
  local exit_code=$?
  trap - EXIT INT TERM
  ab_cmd "$session" close >/dev/null 2>&1 || true
  stop_stack
  release_web_fixture_stop
  exit "$exit_code"
}
trap cleanup EXIT INT TERM

if [[ -z "$GAME_URL" ]]; then
  if [[ "$RELEASE_WEB_FIXTURE" -eq 1 ]]; then
    prepare_release_web_stack
    STACK_ARGS+=(--chain-disable)
    if [[ "$AGENT_ID" == "starter-agent-0" ]]; then AGENT_ID="agent-a"; fi
  fi
  {
    echo "### [bootstrap_stack] $(date '+%H:%M:%S')"
    if [[ "${#STACK_ARGS[@]}" -gt 0 ]]; then
      echo "./scripts/run-launcher-stack.sh ${STACK_ARGS[*]}"
    else
      echo "./scripts/run-launcher-stack.sh"
    fi
    echo
  } | tee -a "$ab_log" >/dev/null

  if [[ "$BOOTSTRAP_USES_BUNDLE" -ne 1 && "$RELEASE_WEB_FIXTURE" -ne 1 ]]; then
    log_note build_oasis7_viewer_live
    oasis7_cargo_dev build -p oasis7_viewer_runtime --bin oasis7_viewer_live >>"$ab_log" 2>&1
  fi

  if command -v stdbuf >/dev/null 2>&1; then
    if [[ "${#STACK_ARGS[@]}" -gt 0 ]]; then
      stdbuf -oL -eL ./scripts/run-launcher-stack.sh --viewer-static-dir crates/oasis7_viewer/.viewer-test-dist "${STACK_ARGS[@]}" >"$run_game_test_log" 2>&1 &
    else
      stdbuf -oL -eL ./scripts/run-launcher-stack.sh --viewer-static-dir crates/oasis7_viewer/.viewer-test-dist >"$run_game_test_log" 2>&1 &
    fi
  else
    if [[ "${#STACK_ARGS[@]}" -gt 0 ]]; then
      ./scripts/run-launcher-stack.sh --viewer-static-dir crates/oasis7_viewer/.viewer-test-dist "${STACK_ARGS[@]}" >"$run_game_test_log" 2>&1 &
    else
      ./scripts/run-launcher-stack.sh --viewer-static-dir crates/oasis7_viewer/.viewer-test-dist >"$run_game_test_log" 2>&1 &
    fi
  fi
  stack_pid=$!

  for ((i = 0; i < STARTUP_TIMEOUT_SECS; i++)); do
    release_web_check_startup || exit 1
    if ! kill -0 "$stack_pid" >/dev/null 2>&1; then
      echo "error: launcher stack exited unexpectedly" >&2
      tail -n 120 "$run_game_test_log" >&2 || true
      exit 1
    fi
    GAME_URL="$(sed -n 's/^- URL: \(http[^[:space:]]*\)$/\1/p' "$run_game_test_log" | tail -n 1)"
    [[ -n "$GAME_URL" ]] && break
    sleep 1
  done

  if [[ -z "$GAME_URL" ]]; then
    echo "error: timeout waiting for game URL from run-launcher-stack.sh" >&2
    tail -n 120 "$run_game_test_log" >&2 || true
    exit 1
  fi
fi

GAME_URL="$(append_query_params "$GAME_URL")"
if ws_host_port=$(extract_ws_host_port "$GAME_URL" 2>/dev/null); then
  read -r ws_host ws_port <<<"$ws_host_port"
  wait_for_tcp_listener "$ws_host" "$ws_port" 20 || {
    echo "error: websocket bridge did not become ready: ${ws_host}:${ws_port}" >&2
    exit 1
  }
fi
sleep 4

log_note open
ab_open "$session" "$HEADED" "$GAME_URL" >>"$ab_log" 2>&1
ab_cmd "$session" wait --load networkidle >>"$ab_log" 2>&1 || true
sleep_ms 2500

wait_for_api 20000 || { echo "error: __AW_TEST__ unavailable" >&2; exit 1; }
initial_state=$(wait_for_connected 30000) || {
  echo "error: viewer failed to connect (lastError=$(state_last_error "$initial_state"))" >&2
  exit 1
}
write_json_file "$initial_state" "$initial_state_json"
write_json_file "$(browser_env)" "$browser_env_json"

render_mode="$(state_render_mode "$initial_state")"
[[ "$render_mode" == "viewer" || "$render_mode" == "software_safe" ]] || { echo "error: expected renderMode=viewer-compatible, got $render_mode" >&2; exit 1; }

log_note select_agent
agent_id_json=$(python3 - "$AGENT_ID" <<'PY'
import json
import sys
print(json.dumps(sys.argv[1]))
PY
)
ab_eval "$session" "window.__AW_TEST__.select('agent:${AGENT_ID}')" >>"$ab_log" 2>&1
wait_for_js_true "(() => window.__AW_TEST__?.getState?.()?.selectedId === ${agent_id_json})()" 6000 || {
  echo "error: failed to select agent ${AGENT_ID}" >&2
  exit 1
}
selected_agent_visible=true
if [[ "$RELEASE_WEB_FIXTURE" -eq 1 ]]; then
  # Hosted guests see the signed public projection, not the private control tree.
  wait_for_js_true "(() => {
    const snapshot = window.__AW_TEST__?.getState?.();
    return snapshot?.selectedKind === 'agent' && snapshot?.selectedId === ${agent_id_json}
      && snapshot?.selectedAgentDebug?.environment_class === 'runtime_live'
      && snapshot?.selectedAgentDebug?.provider_config_ref === 'provider://loopback-http/runtime-live/' + ${agent_id_json}
      && snapshot?.pixelWorldRuntimeStatus === 'ready'
      && snapshot?.pixelWorldRuntimeSource === 'wasm_bindgen_runtime';
  })()" 6000 || { echo 'error: release fixture signed Agent projection or actual renderer unavailable' >&2; exit 1; }
  selected_agent_visible=false
else
wait_for_js_true "(() => { const agentId = ${agent_id_json}; return !!document.querySelector('[data-select-kind=\"agent\"][data-select-id=\"' + agentId + '\"][data-selected=\"true\"]'); })()" 6000 || {
  echo "error: selected agent ${AGENT_ID} is not reflected in DOM" >&2
  exit 1
}
fi
after_select_state=$(ab_state)
write_json_file "$after_select_state" "$after_select_state_json"

before_logical_time=$(state_logical_time "$after_select_state")
before_event_seq=$(state_event_seq "$after_select_state")
before_logical_time=${before_logical_time:-0}
before_event_seq=${before_event_seq:-0}

wait_for_js_true "(() => {
  const snapshot = window.__AW_TEST__?.getState?.();
  const text = document.body?.innerText || '';
  const hasRealtimeSurface =
    text.includes('Recent Events') ||
    text.includes('最近事件') ||
    text.includes('Formal Gameplay Summary') ||
    text.includes('正式玩法摘要');
  const hasGameplayState =
    snapshot?.connectionStatus === 'connected' &&
    snapshot?.controlProfile === 'live' &&
    !!snapshot?.gameplaySummary;
  return hasRealtimeSurface || hasGameplayState;
})()" 4000 || {
  echo "error: realtime gameplay surface missing in DOM" >&2
  exit 1
}

log_note wait_live_progress
wait_for_js_true "(() => {
  const snapshot = window.__AW_TEST__?.getState?.();
  return Number(snapshot?.logicalTime || 0) > ${before_logical_time}
    || Number(snapshot?.eventSeq || 0) > ${before_event_seq};
})()" "$PROGRESS_TIMEOUT_MS" || true

after_progress_state=$(ab_state)

playback_controls_visible=$(normalize_eval_token "$(ab_eval "$session" "(() => {
  const text = document.body?.innerText || '';
  return text.includes('Playback Controls') ||
    text.includes('回放控制') ||
    text.includes('Step x1') ||
    text.includes('单步 x1') ||
    text.includes('Step custom count') ||
    text.includes('按自定义步数推进');
})()" 2>>"$ab_log" || printf 'false')")
after_logical_time=$(state_logical_time "$after_progress_state")
after_event_seq=$(state_event_seq "$after_progress_state")
after_logical_time=${after_logical_time:-0}
after_event_seq=${after_event_seq:-0}

logical_time_advanced=false
event_seq_advanced=false
if (( ${after_logical_time%%.*} > ${before_logical_time%%.*} )); then
  logical_time_advanced=true
fi
if (( ${after_event_seq%%.*} > ${before_event_seq%%.*} )); then
  event_seq_advanced=true
fi
auto_progress_observed=false
if [[ "$logical_time_advanced" == "true" || "$event_seq_advanced" == "true" ]]; then
  auto_progress_observed=true
fi

stage_status=$(state_stage_status "$after_progress_state")
blocker_kind=$(state_blocker_kind "$after_progress_state")
blocker_detail=$(state_blocker_detail "$after_progress_state")
blocker_dom_visible=null
if [[ "$(state_connection "$after_progress_state")" != "connected" ]]; then
  echo "error: viewer disconnected after realtime wait (lastError=$(state_last_error "$after_progress_state"))" >&2
  exit 1
fi
fail_category=null
if [[ "$auto_progress_observed" != "true" ]]; then
  refresh_explicit_blocker_state "$after_progress_state"

  if [[ "$explicit_blocker_state" != "true" ]]; then
    log_note step_once
    ab_eval "$session" "window.__AW_TEST__?.sendControl?.('step', null) ?? null" >>"$ab_log" 2>&1 || true
    wait_for_js_true "(() => {
      const snapshot = window.__AW_TEST__?.getState?.();
      const feedback = snapshot?.lastControlFeedback;
      const stage = String(feedback?.stage || '');
      if (feedback?.action !== 'step') {
        return Number(snapshot?.logicalTime || 0) > ${before_logical_time}
          || Number(snapshot?.eventSeq || 0) > ${before_event_seq};
      }
      return Number(snapshot?.logicalTime || 0) > ${before_logical_time}
        || Number(snapshot?.eventSeq || 0) > ${before_event_seq}
        || stage === 'completed_advanced'
        || stage === 'completed_no_progress'
        || stage === 'blocked';
    })()" "$PROGRESS_TIMEOUT_MS" || true

    after_progress_state=$(ab_state)
    after_logical_time=$(state_logical_time "$after_progress_state")
    after_event_seq=$(state_event_seq "$after_progress_state")
    after_logical_time=${after_logical_time:-0}
    after_event_seq=${after_event_seq:-0}
    logical_time_advanced=false
    event_seq_advanced=false
    if (( ${after_logical_time%%.*} > ${before_logical_time%%.*} )); then
      logical_time_advanced=true
    fi
    if (( ${after_event_seq%%.*} > ${before_event_seq%%.*} )); then
      event_seq_advanced=true
    fi
    auto_progress_observed=false
    if [[ "$logical_time_advanced" == "true" || "$event_seq_advanced" == "true" ]]; then
      auto_progress_observed=true
    fi
    refresh_explicit_blocker_state "$after_progress_state"

    if [[ "$auto_progress_observed" != "true" && "$explicit_blocker_state" != "true" && "$(state_last_control_stage "$after_progress_state")" == "queued" ]]; then
      log_note resume_play
      ab_eval "$session" "window.__AW_TEST__?.sendControl?.('play', null) ?? null" >>"$ab_log" 2>&1 || true
      wait_for_js_true "(() => {
        const snapshot = window.__AW_TEST__?.getState?.();
        const feedback = snapshot?.lastControlFeedback;
        const stage = String(feedback?.stage || '');
        if (feedback?.action !== 'play') {
          return Number(snapshot?.logicalTime || 0) > ${before_logical_time}
            || Number(snapshot?.eventSeq || 0) > ${before_event_seq};
        }
        return Number(snapshot?.logicalTime || 0) > ${before_logical_time}
          || Number(snapshot?.eventSeq || 0) > ${before_event_seq}
          || stage === 'completed_advanced'
          || stage === 'completed_no_progress'
          || stage === 'blocked';
      })()" "$PROGRESS_TIMEOUT_MS" || true

      after_progress_state=$(ab_state)
      after_logical_time=$(state_logical_time "$after_progress_state")
      after_event_seq=$(state_event_seq "$after_progress_state")
      after_logical_time=${after_logical_time:-0}
      after_event_seq=${after_event_seq:-0}
      logical_time_advanced=false
      event_seq_advanced=false
      if (( ${after_logical_time%%.*} > ${before_logical_time%%.*} )); then
        logical_time_advanced=true
      fi
      if (( ${after_event_seq%%.*} > ${before_event_seq%%.*} )); then
        event_seq_advanced=true
      fi
      auto_progress_observed=false
      if [[ "$logical_time_advanced" == "true" || "$event_seq_advanced" == "true" ]]; then
        auto_progress_observed=true
      fi
      refresh_explicit_blocker_state "$after_progress_state"
    fi
  fi

  if [[ "$auto_progress_observed" == "true" ]]; then
    :
  elif [[ "$explicit_blocker_state" == "true" ]]; then
    blocker_dom_check_js=$(python3 - "${expected_blocker_status:-null}" "${stage_status:-null}" "${blocker_kind:-null}" "${blocker_detail:-null}" "${expected_blocker_detail:-null}" <<'PY'
import json
import sys

expected_blocker_status = sys.argv[1]
stage_status = sys.argv[2]
blocker_kind = sys.argv[3]
blocker_detail = sys.argv[4]
expected_blocker_detail = sys.argv[5]

print(r"""
(() => {
  const normalize = (value) => String(value || '')
    .toLowerCase()
    .replace(/\s+/g, ' ')
    .trim();
  const text = normalize(document.body && document.body.innerText ? document.body.innerText : '');
  const expectedBlockerStatus = %s;
  const stageStatus = %s;
  const blockerKind = %s;
  const blockerDetail = %s;
  const expectedBlockerDetail = %s;
  const surfaceKeywords = [
    'formal gameplay summary',
    'recent feedback',
    'blocker',
    'blocked',
    'handoff',
    '正式玩法摘要',
    '最近反馈',
    '阻塞',
    '交接',
    '接管',
  ];
  const hasSurface = surfaceKeywords.some((token) => text.includes(normalize(token)));
  const kindOk = !blockerKind || blockerKind === 'null' || text.includes(normalize(blockerKind));
  const detailOk = !blockerDetail || blockerDetail === 'null' || text.includes(normalize(blockerDetail));
  const expectedDetailOk =
    !expectedBlockerDetail ||
    expectedBlockerDetail === 'null' ||
    text.includes(normalize(expectedBlockerDetail));
  const statusOk =
    (stageStatus !== 'blocked' && expectedBlockerStatus !== 'blocked') ||
    text.includes('blocked') ||
    text.includes('阻塞');
  return hasSurface && kindOk && detailOk && expectedDetailOk && statusOk;
})()
""" % (
    json.dumps(expected_blocker_status),
    json.dumps(stage_status),
    json.dumps(blocker_kind),
    json.dumps(blocker_detail),
    json.dumps(expected_blocker_detail),
))
PY
)
    blocker_dom_visible=$(normalize_eval_token "$(ab_eval "$session" "$blocker_dom_check_js" 2>>"$ab_log" || printf 'false')")
    if [[ "$blocker_dom_visible" != "true" ]]; then
      fail_category="blocker_ui_not_visible"
    fi
  else
    fail_category="no_auto_progress_or_explicit_blocker"
  fi
fi

write_json_file "$after_progress_state" "$after_progress_state_json"
if [[ "$RELEASE_WEB_FIXTURE" -eq 1 && "$auto_progress_observed" != "true" ]]; then
  echo 'error: release fixture canonical world did not naturally advance' >&2
  exit 1
fi
final_state=$(ab_state)
write_json_file "$final_state" "$final_state_json"
ab_screenshot "$session" "$screenshot_path" >>"$ab_log" 2>&1 || true

summary_raw=$(summary_json \
  "$([[ "$fail_category" == "null" ]] && printf 'true' || printf 'false')" \
  "$fail_category" \
  "$run_id" \
  "$AGENT_ID" \
  "$GAME_URL" \
  "$render_mode" \
  "$selected_agent_visible" \
  "$playback_controls_visible" \
  "$logical_time_advanced" \
  "$event_seq_advanced" \
  "$auto_progress_observed" \
  "${blocker_dom_visible:-null}" \
  "${stage_status:-null}" \
  "${blocker_kind:-null}" \
  "${blocker_detail:-null}" \
  "${expected_blocker_detail:-null}")
printf '%s\n' "$summary_raw" >"$summary_json_path"
python3 - "$summary_json_path" "$summary_md_path" <<'PY'
import json
import pathlib
import sys
src = pathlib.Path(sys.argv[1])
out = pathlib.Path(sys.argv[2])
data = json.loads(src.read_text())
lines = [
    '# Viewer realtime-only regression summary',
    '',
    f"- ok: `{data['ok']}`",
    f"- failCategory: `{data['failCategory']}`",
    f"- runId: `{data['runId']}`",
    f"- agentId: `{data['agentId']}`",
    f"- renderMode: `{data['renderMode']}`",
    f"- selectedAgentVisible: `{data['selectedAgentVisible']}`",
    f"- playbackControlsVisible: `{data['playbackControlsVisible']}`",
    f"- logicalTimeAdvanced: `{data['logicalTimeAdvanced']}`",
    f"- eventSeqAdvanced: `{data['eventSeqAdvanced']}`",
    f"- autoProgressObserved: `{data['autoProgressObserved']}`",
    f"- blockerDomVisible: `{data['blockerDomVisible']}`",
    f"- stageStatus: `{data['stageStatus']}`",
    f"- blockerKind: `{data['blockerKind']}`",
    f"- blockerDetail: `{data['blockerDetail']}`",
    f"- expectedBlockerDetail: `{data['expectedBlockerDetail']}`",
    f"- gameUrl: `{data['gameUrl']}`",
]
out.write_text("\n".join(lines) + "\n", encoding='utf-8')
PY

if [[ "$fail_category" != "null" ]]; then
  echo "error: viewer realtime-only regression saw neither natural progress nor explicit blocker (failCategory=${fail_category}, stageStatus=${stage_status:-null}, blockerKind=${blocker_kind:-null}, logicalTimeAdvanced=${logical_time_advanced}, eventSeqAdvanced=${event_seq_advanced})" >&2
  exit 1
fi

printf 'ok: artifacts written to %s\n' "$out_dir"
