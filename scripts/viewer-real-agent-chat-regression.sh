#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

usage() {
  cat <<'USAGE'
Usage: ./scripts/viewer-real-agent-chat-regression.sh [options] [-- launcher args...]

Run the local real-provider Agent chat browser regression:
1. optionally start the canonical local LetAI game stack
2. open the Viewer in a Playwright browser automation session
3. send a direct chat message to agent-0
4. require a real inbound Agent reply and reject local mock reply markers

Options:
  --url <url>                 Use an existing Viewer URL; skip stack bootstrap
  --out-dir <path>            Artifact root (default: output/playwright/viewer-real-agent-chat)
  --startup-timeout <secs>    Wait timeout for a bootstrapped stack (default: 300)
  --agent-id <id>             Target agent id (default: agent-0)
  --chat-message <text>       Chat message to send
                              (default: 你在哪里？身边有什么资源？请直接回答。)
  --expect-contains <text>    Required Agent reply fragment; repeatable
                              (default: runtime:, data 8, electricity 32)
  --forbid-contains <text>    Forbidden Agent reply fragment; repeatable
                              (default: [local-mock-receipt], [local-mock-chat])
  --keep-stack                Leave a stack started by this script running
  --headed                    Open browser in headed mode
  --headless                  Open browser in headless mode (default)
  -h, --help                  Show help

Examples:
  ./scripts/viewer-real-agent-chat-regression.sh
  ./scripts/viewer-real-agent-chat-regression.sh --url "http://127.0.0.1:4173/?ws=ws://127.0.0.1:5011&test_api=1&locale=zh"
USAGE
}

sleep_ms() {
  python3 - "$1" <<'PY'
import sys, time
time.sleep(int(sys.argv[1]) / 1000.0)
PY
}

free_bind_addr() {
  python3 - <<'PY'
from __future__ import annotations

import socket

with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
    sock.bind(("127.0.0.1", 0))
    print(f"127.0.0.1:{sock.getsockname()[1]}")
PY
}

free_port() {
  free_bind_addr | sed 's/^.*://'
}

meta_value() {
  local key=$1
  local file=$2
  sed -n "s/^${key}=//p" "$file" | tail -n 1
}

GAME_URL=""
OUT_ROOT="output/playwright/viewer-real-agent-chat"
STARTUP_TIMEOUT_SECS=300
AGENT_ID="agent-0"
CHAT_MESSAGE="你在哪里？身边有什么资源？请直接回答。"
HEADED=0
KEEP_STACK=0
EXPECT_CONTAINS=()
FORBID_CONTAINS=()
EXPECT_CONTAINS_SET=0
FORBID_CONTAINS_SET=0
LAUNCHER_ARGS=()

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
    --chat-message)
      CHAT_MESSAGE="${2:-}"
      shift 2
      ;;
    --expect-contains)
      EXPECT_CONTAINS+=("${2:-}")
      EXPECT_CONTAINS_SET=1
      shift 2
      ;;
    --forbid-contains)
      FORBID_CONTAINS+=("${2:-}")
      FORBID_CONTAINS_SET=1
      shift 2
      ;;
    --keep-stack)
      KEEP_STACK=1
      shift
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
    --)
      shift
      LAUNCHER_ARGS+=("$@")
      break
      ;;
    *)
      echo "error: unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

[[ -n "$OUT_ROOT" ]] || { echo "error: --out-dir cannot be empty" >&2; exit 2; }
[[ "$STARTUP_TIMEOUT_SECS" =~ ^[0-9]+$ ]] && [[ "$STARTUP_TIMEOUT_SECS" -gt 0 ]] || { echo "error: --startup-timeout must be positive" >&2; exit 2; }
[[ -n "$AGENT_ID" ]] || { echo "error: --agent-id cannot be empty" >&2; exit 2; }
[[ -n "$CHAT_MESSAGE" ]] || { echo "error: --chat-message cannot be empty" >&2; exit 2; }

if [[ "$EXPECT_CONTAINS_SET" -eq 0 ]]; then
  EXPECT_CONTAINS=("runtime:" "data 8" "electricity 32")
fi
if [[ "$FORBID_CONTAINS_SET" -eq 0 ]]; then
  FORBID_CONTAINS=("[local-mock-receipt]" "[local-mock-chat]")
fi

run_id="$(date +%Y%m%d-%H%M%S)"
out_dir="$OUT_ROOT/$run_id"
mkdir -p "$out_dir"

node_bin="${OASIS7_NODE_BIN:-}"
if [[ -z "$node_bin" ]]; then
  if command -v node >/dev/null 2>&1; then
    node_bin="$(command -v node)"
  elif [[ -x "${HOME}/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/bin/node" ]]; then
    node_bin="${HOME}/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/bin/node"
  else
    echo "error: missing required command: node; set OASIS7_NODE_BIN to a Node.js executable" >&2
    exit 1
  fi
fi
node_bin_dir="$(cd "$(dirname "$node_bin")" && pwd)"
export PATH="$node_bin_dir:$PATH"

stack_pid=""
stack_meta=""
stack_log="$out_dir/local-letai-stack.log"
managed_ports=()

stack_port_listeners() {
  if ! command -v lsof >/dev/null 2>&1; then
    return 0
  fi
  local args=()
  local port
  for port in "${managed_ports[@]}"; do
    args+=("-iTCP:$port")
  done
  lsof -nP "${args[@]}" -sTCP:LISTEN 2>/dev/null | awk 'NR > 1 { print }'
}

stack_port_listener_pids() {
  stack_port_listeners | awk '{ print $2 }' | sort -u
}

wait_for_stack_ports_clean() {
  local report=${1:-1}
  local listeners=""
  for _ in $(seq 1 80); do
    listeners="$(stack_port_listeners || true)"
    if [[ -z "$listeners" ]]; then
      return 0
    fi
    sleep_ms 250
  done
  if [[ "$report" -eq 1 ]]; then
    echo "error: local LetAI stack cleanup left managed ports listening: ${managed_ports[*]}" >&2
    printf '%s\n' "$listeners" >&2
  fi
  return 1
}

terminate_stack_port_listeners() {
  local pids=""
  pids="$(stack_port_listener_pids || true)"
  if [[ -z "$pids" ]]; then
    return 0
  fi
  # This wrapper owns the canonical local playtest ports while it runs.
  # If polite launcher shutdown leaves children behind, terminate those listeners.
  echo "warning: terminating leftover local LetAI stack listener pids: ${pids//$'\n'/ }" >&2
  while IFS= read -r pid; do
    [[ -n "$pid" ]] || continue
    kill "$pid" >/dev/null 2>&1 || true
  done <<<"$pids"
}

cleanup() {
  local exit_code=$?
  trap - EXIT INT TERM
  if [[ -n "$stack_pid" && "$KEEP_STACK" -ne 1 ]]; then
    if [[ -n "$stack_meta" && -f "$stack_meta" ]]; then
      launcher_pid="$(meta_value LAUNCHER_PID "$stack_meta" 2>/dev/null || true)"
      if [[ -n "${launcher_pid:-}" ]]; then
        kill "$launcher_pid" >/dev/null 2>&1 || true
      fi
    fi
    kill "$stack_pid" >/dev/null 2>&1 || true
    wait "$stack_pid" >/dev/null 2>&1 || true
    if [[ "$exit_code" -eq 0 ]]; then
      if ! wait_for_stack_ports_clean 0; then
        terminate_stack_port_listeners
        wait_for_stack_ports_clean || exit_code=1
      fi
    fi
  fi
  exit "$exit_code"
}
trap cleanup EXIT INT TERM

if [[ -z "$GAME_URL" ]]; then
  stack_out="$out_dir/local-letai-stack"
  mkdir -p "$stack_out"
  stack_meta="$stack_out/launcher/session.meta"
  stack_provider_bind="$(free_bind_addr)"
  stack_viewer_port="$(free_port)"
  stack_live_bind="$(free_bind_addr)"
  stack_web_bind="$(free_bind_addr)"
  managed_ports=(
    "$stack_viewer_port"
    "${stack_live_bind##*:}"
    "${stack_web_bind##*:}"
    "${stack_provider_bind##*:}"
  )
  stack_cmd=(
    ./scripts/run-local-letai-game-test.sh
    --bind "$stack_provider_bind"
    --startup-profile playtest
    --provider-smoke-mode degraded
    --skip-chat-probe
    --reuse-existing-build
    --no-chat-echo
    --auto-play
    --output-dir "$stack_out"
    --
    --chain-disable
    --skip-llm-provider-preflight
    --viewer-port "$stack_viewer_port"
    --live-bind "$stack_live_bind"
    --web-bind "$stack_web_bind"
  )
  if [[ "${#LAUNCHER_ARGS[@]}" -gt 0 ]]; then
    stack_cmd+=("${LAUNCHER_ARGS[@]}")
  fi
  printf 'Starting local LetAI stack:'
  printf ' %q' "${stack_cmd[@]}"
  printf '\n'
  "${stack_cmd[@]}" >"$stack_log" 2>&1 &
  stack_pid=$!

  for ((i = 0; i < STARTUP_TIMEOUT_SECS; i++)); do
    if ! kill -0 "$stack_pid" >/dev/null 2>&1; then
      echo "error: local LetAI stack exited before readiness; log: $stack_log" >&2
      tail -n 120 "$stack_log" >&2 || true
      exit 1
    fi
    if [[ -f "$stack_meta" ]] && [[ "$(meta_value STACK_READY "$stack_meta" 2>/dev/null || true)" == "1" ]]; then
      GAME_URL="$(meta_value GAME_URL "$stack_meta")"
      break
    fi
    sleep 1
  done

  if [[ -z "$GAME_URL" ]]; then
    echo "error: timed out waiting for STACK_READY=1 in $stack_meta; log: $stack_log" >&2
    tail -n 120 "$stack_log" >&2 || true
    exit 1
  fi
fi

playwright_node_modules="${OASIS7_PLAYWRIGHT_NODE_MODULES:-${HOME}/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules}"
if [[ ! -d "$playwright_node_modules" ]]; then
  echo "error: Playwright node_modules not found: $playwright_node_modules" >&2
  echo "hint: set OASIS7_PLAYWRIGHT_NODE_MODULES to a node_modules directory containing playwright" >&2
  exit 1
fi

chat_args=(
  "$node_bin"
  "$repo_root/crates/oasis7_viewer/scripts/real-agent-chat-regression.mjs"
  --url "$GAME_URL"
  --out-dir "$out_dir/playwright"
  --agent-id "$AGENT_ID"
  --chat-message "$CHAT_MESSAGE"
  --timeout-ms 90000
)
if [[ "$HEADED" -eq 1 ]]; then
  chat_args+=(--headed)
fi
for needle in "${EXPECT_CONTAINS[@]}"; do
  chat_args+=(--expect-contains "$needle")
done
for needle in "${FORBID_CONTAINS[@]}"; do
  chat_args+=(--forbid-contains "$needle")
done

NODE_PATH="$playwright_node_modules" "${chat_args[@]}" | tee "$out_dir/viewer-real-agent-chat-regression.log"

cat >"$out_dir/viewer-real-agent-chat-regression-summary.md" <<EOF
# Viewer real Agent chat regression

- gameUrl: \`$GAME_URL\`
- agentId: \`$AGENT_ID\`
- chatMessage: \`$CHAT_MESSAGE\`
- requiredContains: \`${EXPECT_CONTAINS[*]}\`
- forbiddenContains: \`${FORBID_CONTAINS[*]}\`
- stackLog: \`$stack_log\`
EOF

if [[ -n "$stack_pid" && "$KEEP_STACK" -eq 1 ]]; then
  cat <<EOF
ok: artifacts written to $out_dir
ok: stack kept running
stack_log=$stack_log
game_url=$GAME_URL
EOF
else
  echo "ok: artifacts written to $out_dir"
fi
