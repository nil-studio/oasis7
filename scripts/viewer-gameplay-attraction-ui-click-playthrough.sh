#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"
source "$repo_root/scripts/cargo-dev-lib.sh"
source "$repo_root/scripts/agent-browser-lib.sh"

usage() {
  cat <<'USAGE'
Usage: ./scripts/viewer-gameplay-attraction-ui-click-playthrough.sh [options] [run-launcher-stack options...]

Run the TASK-GAME-076 first-30-minute attraction playthrough through actual
player-visible UI clicks. The test API is allowed for readiness, assertions,
and state artifacts only; it must not drive gameplay progression.

Options:
  --url <url>               Use an existing viewer URL; skip stack bootstrap
  --out-dir <path>          Artifact root (default: output/playwright/gameplay-attraction-ui-click-playthrough)
  --startup-timeout <secs>  Wait timeout for stack URL (default: 240)
  --agent-id <id>           Target agent id (default: starter-agent-0)
  --beat-timeout-ms <ms>    Per-beat wait timeout (default: 15000)
  --headed                  Open browser in headed mode
  --headless                Open browser in headless mode (default)
  -h, --help                Show help
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
parts = urlparse(sys.argv[1])
ws_values = parse_qs(parts.query).get("ws", [])
if not ws_values:
    raise SystemExit(1)
ws = urlparse(ws_values[0])
if not ws.hostname or not ws.port:
    raise SystemExit(1)
print(f"{ws.hostname} {ws.port}")
PY
}

wait_for_tcp_listener() {
  local host=$1
  local port=$2
  local timeout_secs=${3:-20}
  local step
  for step in $(seq 1 "$timeout_secs"); do
    if python3 - "$host" "$port" <<'PY'
import socket, sys
try:
    with socket.create_connection((sys.argv[1], int(sys.argv[2])), timeout=1):
        pass
except OSError:
    raise SystemExit(1)
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

json_path() {
  python3 - "$1" "$2" <<'PY'
import json, sys
try:
    value = json.loads(sys.argv[1])
except Exception:
    print("")
    raise SystemExit(0)
for part in sys.argv[2].split("."):
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

write_json_file() {
  json_to_file "$1" "$2"
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

find_testid_click() {
  local testid=$1
  local selector="[data-testid=\"${testid}\"]"
  ab_cmd "$session" scrollintoview "$selector" >>"$ab_log" 2>&1
  ab_cmd "$session" click "$selector" >>"$ab_log" 2>&1
  sleep_ms 500
}

find_selector_click() {
  local selector=$1
  ab_cmd "$session" scrollintoview "$selector" >>"$ab_log" 2>&1
  ab_cmd "$session" click "$selector" >>"$ab_log" 2>&1
  sleep_ms 500
}

recommended_action_disabled() {
  normalize_eval_token "$(ab_eval "$session" 'Boolean(document.querySelector("[data-testid=\"viewer-playthrough-action-recommended\"]")?.disabled)')"
}

state_connection() { json_path "$1" connectionStatus; }
state_last_error() { json_path "$1" lastError; }
state_logical_time() { json_path "$1" logicalTime; }
state_event_seq() { json_path "$1" eventSeq; }

GAME_URL=""
OUT_ROOT="output/playwright/gameplay-attraction-ui-click-playthrough"
STARTUP_TIMEOUT_SECS=240
AGENT_ID="starter-agent-0"
BEAT_TIMEOUT_MS=15000
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
    --agent-id)
      AGENT_ID="${2:-}"
      shift 2
      ;;
    --beat-timeout-ms|--progress-timeout-ms)
      BEAT_TIMEOUT_MS="${2:-}"
      shift 2
      ;;
    --headed)
      HEADED=1
      shift
      ;;
    --headless)
      HEADED=0
      shift
      ;;
    --bundle-dir)
      BOOTSTRAP_USES_BUNDLE=1
      STACK_ARGS+=("$1" "${2:-}")
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      STACK_ARGS+=("$1")
      shift
      ;;
  esac
done

[[ -n "$OUT_ROOT" ]] || { echo "error: --out-dir cannot be empty" >&2; exit 2; }
[[ "$STARTUP_TIMEOUT_SECS" =~ ^[0-9]+$ ]] && [[ "$STARTUP_TIMEOUT_SECS" -gt 0 ]] || { echo "error: --startup-timeout must be positive" >&2; exit 2; }
[[ "$BEAT_TIMEOUT_MS" =~ ^[0-9]+$ ]] && [[ "$BEAT_TIMEOUT_MS" -gt 0 ]] || { echo "error: --beat-timeout-ms must be positive" >&2; exit 2; }

require_cmd python3
: "${AGENT_BROWSER_ARGS:=}"
ab_require

run_id="$(date +%Y%m%d-%H%M%S)"
out_dir="$OUT_ROOT/$run_id"
states_dir="$out_dir/states"
mkdir -p "$states_dir"

ab_log="$out_dir/agent-browser.log"
run_game_test_log="$out_dir/launcher-stack.log"
summary_json_path="$out_dir/gameplay-attraction-ui-click-playthrough-summary.json"
summary_md_path="$out_dir/gameplay-attraction-ui-click-playthrough-summary.md"
screenshot_path="$out_dir/gameplay-attraction-ui-click-playthrough.png"
session="gameplay-attraction-click-$run_id"
summary_items_jsonl="$out_dir/beat-results.jsonl"
: >"$summary_items_jsonl"

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
  exit "$exit_code"
}
trap cleanup EXIT INT TERM

record_beat() {
  local beat_id=$1
  local time_label=$2
  local action_label=$3
  local assertion_label=$4
  local status=$5
  local reason=${6:-}
  local state_file=${7:-}
  local clicked_testids=${8:-}
  python3 - "$summary_items_jsonl" "$beat_id" "$time_label" "$action_label" "$assertion_label" "$status" "$reason" "$state_file" "$clicked_testids" <<'PY'
import json, sys
path, beat_id, time_label, action, assertion, status, reason, state_file, clicked = sys.argv[1:]
with open(path, "a", encoding="utf-8") as handle:
    handle.write(json.dumps({
        "beat": beat_id,
        "time": time_label,
        "action": action,
        "assertion": assertion,
        "status": status,
        "reason": reason or None,
        "state_file": state_file or None,
        "clicked_testids": [item for item in clicked.split(",") if item],
    }, ensure_ascii=False) + "\n")
PY
}

capture_state() {
  local label=$1
  local raw
  local path="$states_dir/${label}.json"
  raw=$(ab_state)
  write_json_file "$raw" "$path"
  printf '%s\n' "$path"
}

click_targets() {
  local csv=$1
  local IFS=,
  local target
  for target in $csv; do
    [[ -n "$target" ]] || continue
    case "$target" in
      agent:$AGENT_ID)
        find_selector_click "[data-select-kind=\"agent\"][data-select-id=\"${AGENT_ID}\"]"
        ;;
      maybe:viewer-playthrough-action-recommended)
        if [[ "$(recommended_action_disabled)" == "true" ]]; then
          sleep_ms 500
        else
          find_testid_click "viewer-playthrough-action-recommended"
        fi
        ;;
      testid:*)
        find_testid_click "${target#testid:}"
        ;;
      *)
        find_testid_click "$target"
        ;;
    esac
  done
}

run_click_beat() {
  local time_label=$1
  local beat_id=$2
  local action_label=$3
  local clicked_testids=$4
  local assertion_label=$5
  local assertion_js=$6
  log_note "beat_${time_label}_${beat_id}"
  if ! click_targets "$clicked_testids"; then
    local failed_state_file
    failed_state_file=$(capture_state "${time_label}-${beat_id}")
    record_beat "$beat_id" "$time_label" "$action_label" "$assertion_label" "fail" "ui click failed" "$failed_state_file" "$clicked_testids"
    return 1
  fi
  local ok="false"
  if wait_for_js_true "$assertion_js" "$BEAT_TIMEOUT_MS"; then
    ok="true"
  fi
  local state_file
  state_file=$(capture_state "${time_label}-${beat_id}")
  if [[ "$ok" == "true" ]]; then
    record_beat "$beat_id" "$time_label" "$action_label" "$assertion_label" "pass" "" "$state_file" "$clicked_testids"
    return 0
  fi
  record_beat "$beat_id" "$time_label" "$action_label" "$assertion_label" "fail" "assertion timed out" "$state_file" "$clicked_testids"
  return 1
}

if [[ -z "$GAME_URL" ]]; then
  {
    echo "### [bootstrap_stack] $(date '+%H:%M:%S')"
    if [[ "${#STACK_ARGS[@]}" -gt 0 ]]; then
      echo "./scripts/run-launcher-stack.sh ${STACK_ARGS[*]}"
    else
      echo "./scripts/run-launcher-stack.sh"
    fi
    echo
  } | tee -a "$ab_log" >/dev/null

  if [[ "$BOOTSTRAP_USES_BUNDLE" -ne 1 ]]; then
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
    if ! kill -0 "$stack_pid" >/dev/null 2>&1; then
      echo "error: launcher stack exited unexpectedly" >&2
      tail -n 120 "$run_game_test_log" >&2 || true
      exit 1
    fi
    GAME_URL="$(sed -n 's/^- URL: \(http[^[:space:]]*\)$/\1/p' "$run_game_test_log" | tail -n 1)"
    [[ -n "$GAME_URL" ]] && break
    sleep 1
  done
  [[ -n "$GAME_URL" ]] || { echo "error: timeout waiting for game URL" >&2; exit 1; }
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
initial_state='null'
for _ in $(seq 1 120); do
  initial_state=$(ab_state)
  if [[ "$(state_connection "$initial_state")" == "connected" ]]; then
    break
  fi
  if [[ -n "$(state_last_error "$initial_state")" ]]; then
    echo "error: viewer failed to connect (lastError=$(state_last_error "$initial_state"))" >&2
    exit 1
  fi
  sleep_ms 250
done
[[ "$(state_connection "$initial_state")" == "connected" ]] || { echo "error: viewer did not connect" >&2; exit 1; }
write_json_file "$initial_state" "$states_dir/initial.json"

agent_id_json=$(json_quote "$AGENT_ID")
before_time=$(state_logical_time "$initial_state")
before_seq=$(state_event_seq "$initial_state")
before_time=${before_time:-0}
before_seq=${before_seq:-0}

overall_status="pass"
run_click_beat "0-1m" "identity_goal_landing" \
  "click target agent and visible refresh-snapshot control" \
  "agent:${AGENT_ID},viewer-playthrough-action-request-snapshot" \
  "identity, goal, blocker/next-step, and available actions are visible in live state" \
  "(() => { const s = window.__AW_TEST__?.getState?.(); const g = s?.gameplaySummary || {}; return s?.selectedId === ${agent_id_json} && !!g.goalTitle && !!g.objective && !!g.nextStepHint && Array.isArray(g.availableActions) && g.availableActions.length >= 2; })()" \
  || overall_status="fail"

run_click_beat "1-3m" "first_control_proof" \
  "click visible advance-one-step control" \
  "viewer-playthrough-action-step" \
  "live control is accepted and produces progress or explicit completed feedback" \
  "(() => { const s = window.__AW_TEST__?.getState?.(); const f = s?.lastControlFeedback || {}; return Number(s?.logicalTime || 0) > ${before_time} || Number(s?.eventSeq || 0) > ${before_seq} || ['completed_advanced','completed_no_progress','blocked'].includes(String(f.stage || '')); })()" \
  || overall_status="fail"

run_click_beat "3-5m" "first_consequence_read" \
  "click visible refresh-snapshot control after first step" \
  "viewer-playthrough-action-request-snapshot" \
  "Control Proof and Attraction Proof expose visible consequence/what-I-caused fields" \
  "(() => { const g = window.__AW_TEST__?.getState?.()?.gameplaySummary || {}; const details = document.querySelector('#viewer-gameplay-details'); const text = details?.innerText || ''; const hasControlProof = text.includes('Control Proof') || text.includes('控制证明'); const hasAttractionProof = text.includes('Attraction Proof') || text.includes('吸引力证明'); return details?.open === true && hasControlProof && hasAttractionProof && !!g.controlProof?.consequence && !!g.attractionProof?.whatICaused && !!g.attractionProof?.whyContinue; })()" \
  || overall_status="fail"

run_click_beat "5-7m" "first_choice_hook" \
  "click visible recommended action when enabled, otherwise verify disabled truth" \
  "maybe:viewer-playthrough-action-recommended" \
  "recommended action has label, protocol, and executable-or-disabled truth" \
  "(() => { const g = window.__AW_TEST__?.getState?.()?.gameplaySummary || {}; const a = g.recommendedAction || {}; return !!a.label && !!a.protocolAction && Object.prototype.hasOwnProperty.call(a, 'disabledReason'); })()" \
  || overall_status="fail"

run_click_beat "7-10m" "first_blocker_recovery" \
  "click visible advance-one-step recovery control" \
  "viewer-playthrough-action-step" \
  "blocked/waiting state has recovery, next step, or explicit completed progress" \
  "(() => { const s = window.__AW_TEST__?.getState?.(); const g = s?.gameplaySummary || {}; const f = s?.lastControlFeedback || {}; return !!g.controlProof?.recovery && !!g.nextStepHint && (!!g.recommendedAction || ['completed_advanced','completed_no_progress','blocked'].includes(String(f.stage || ''))); })()" \
  || overall_status="fail"

run_click_beat "8-12m" "first_visible_output" \
  "click visible refresh-snapshot control for economy read" \
  "viewer-playthrough-action-request-snapshot" \
  "economic surface exposes input, output, next value, and unlocked value" \
  "(() => { const e = window.__AW_TEST__?.getState?.()?.gameplaySummary?.economicSurface || {}; return !!e.input && !!e.output && !!e.nextValue && !!e.unlockedValue; })()" \
  || overall_status="fail"

run_click_beat "12-18m" "persistent_capability" \
  "click visible advance-one-step control and re-read capability state" \
  "viewer-playthrough-action-step" \
  "progress, stage, goal, and available actions persist after later read" \
  "(() => { const g = window.__AW_TEST__?.getState?.()?.gameplaySummary || {}; return Number.isFinite(Number(g.progressPercent)) && !!g.stageStatus && !!g.goalTitle && Array.isArray(g.availableActions); })()" \
  || overall_status="fail"

run_click_beat "18-23m" "first_expansion_tradeoff" \
  "click visible refresh-snapshot control for branch/choice strip" \
  "viewer-playthrough-action-request-snapshot" \
  "choice surface provides at least two player-understandable next actions or a branch hint" \
  "(() => { const g = window.__AW_TEST__?.getState?.()?.gameplaySummary || {}; const actions = Array.isArray(g.availableActions) ? g.availableActions.filter((a) => !a.disabledReason) : []; return actions.length >= 2 || !!g.branchHint || !!g.attractionProof?.newOption; })()" \
  || overall_status="fail"

run_click_beat "20-25m" "agency_correction" \
  "click visible refresh-snapshot control for agency moves" \
  "viewer-playthrough-action-request-snapshot" \
  "Agency Moves exposes interrupt/reprioritize/correction/handoff truth or explicit unavailable reason" \
  "(() => { const a = window.__AW_TEST__?.getState?.()?.gameplaySummary?.agencyMoves || {}; return !!a.summary && Object.prototype.hasOwnProperty.call(a, 'interrupt') && Object.prototype.hasOwnProperty.call(a, 'reprioritize'); })()" \
  || overall_status="fail"

run_click_beat "25-30m" "return_hook_small_player_value" \
  "click visible advance-one-step control for final return-hook inspection" \
  "viewer-playthrough-action-step" \
  "return hook exposes first win, mature-world continuation, share replay, and leverage/anti-grind fields" \
  "(() => { const g = window.__AW_TEST__?.getState?.()?.gameplaySummary || {}; return !!g.progressionProof?.summary && !!g.matureWorldContinuation?.summary && !!g.shareReplay?.summary && !!g.attractionProof?.recovery && !!g.attractionProof?.waitingCost; })()" \
  || overall_status="fail"

final_state_path=$(capture_state "final")
ab_screenshot "$session" "$screenshot_path" >>"$ab_log" 2>&1 || true

python3 - "$summary_items_jsonl" "$summary_json_path" "$summary_md_path" "$run_id" "$GAME_URL" "$overall_status" "$final_state_path" "$screenshot_path" <<'PY'
import json
import pathlib
import sys

items_path = pathlib.Path(sys.argv[1])
summary_json = pathlib.Path(sys.argv[2])
summary_md = pathlib.Path(sys.argv[3])
run_id = sys.argv[4]
game_url = sys.argv[5]
overall = sys.argv[6]
final_state_path = sys.argv[7]
screenshot_path = sys.argv[8]

beats = [json.loads(line) for line in items_path.read_text(encoding="utf-8").splitlines() if line.strip()]
payload = {
    "task": "TASK-GAME-076",
    "runId": run_id,
    "interactionMode": "actual_ui_click",
    "testApiUsage": "assertions_and_state_artifacts_only",
    "ok": overall == "pass" and all(beat["status"] == "pass" for beat in beats),
    "gameUrl": game_url,
    "beatCount": len(beats),
    "passedBeatCount": sum(1 for beat in beats if beat["status"] == "pass"),
    "failedBeatCount": sum(1 for beat in beats if beat["status"] != "pass"),
    "beats": beats,
    "finalState": final_state_path,
    "screenshot": screenshot_path,
}
summary_json.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
lines = [
    "# TASK-GAME-076 Actual UI-Click 30m Playthrough",
    "",
    f"- ok: `{payload['ok']}`",
    f"- interactionMode: `{payload['interactionMode']}`",
    f"- testApiUsage: `{payload['testApiUsage']}`",
    f"- runId: `{run_id}`",
    f"- beatCount: `{payload['beatCount']}`",
    f"- passedBeatCount: `{payload['passedBeatCount']}`",
    f"- failedBeatCount: `{payload['failedBeatCount']}`",
    f"- gameUrl: `{game_url}`",
    f"- screenshot: `{screenshot_path}`",
    "",
    "## Beats",
    "",
]
for beat in beats:
    lines.append(f"- `{beat['time']}` `{beat['beat']}`: `{beat['status']}`")
    lines.append(f"  - action: {beat['action']}")
    lines.append(f"  - clicked_testids: `{', '.join(beat.get('clicked_testids') or [])}`")
    lines.append(f"  - assertion: {beat['assertion']}")
    if beat.get("reason"):
        lines.append(f"  - reason: {beat['reason']}")
summary_md.write_text("\n".join(lines) + "\n", encoding="utf-8")
PY

printf 'TASK-GAME-076 actual UI-click playthrough summary: %s\n' "$summary_json_path"
printf 'TASK-GAME-076 actual UI-click playthrough report: %s\n' "$summary_md_path"

if [[ "$overall_status" != "pass" ]]; then
  exit 1
fi
