#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

DEFAULT_LETAI_CONFIG_PATH="/Users/scc/Documents/keys/letai.txt"
DEFAULT_TOKEN_CONFIG_PATH="${OASIS7_LETAI_TOKEN_CONFIG_PATH:-/Users/scc/Documents/keys/letai-token-local.txt}"
if [[ -n "${OASIS7_LETAI_CONFIG_PATH:-}" ]]; then
  CONFIG_PATH="$OASIS7_LETAI_CONFIG_PATH"
elif [[ -f "$DEFAULT_LETAI_CONFIG_PATH" ]]; then
  CONFIG_PATH="$DEFAULT_LETAI_CONFIG_PATH"
elif [[ -f "$DEFAULT_TOKEN_CONFIG_PATH" ]]; then
  CONFIG_PATH="$DEFAULT_TOKEN_CONFIG_PATH"
else
  CONFIG_PATH="$DEFAULT_LETAI_CONFIG_PATH"
fi
BIND_ADDR="127.0.0.1:5841"
PROXY_URL="http://127.0.0.1:7897"
SOCKS_PROXY_URL="socks5://127.0.0.1:7897"
USE_DEFAULT_PROXY="1"
CHAT_PROBE_BACKEND="${OASIS7_LETAI_CHAT_PROBE_BACKEND:-rust-bridge}"
SKIP_BRIDGE_SMOKE="0"
ENSURE_TOKEN_CONFIG="1"
CHAT_ECHO="0"
AUTO_PLAY="${OASIS7_LOCAL_LETAI_AUTO_PLAY:-1}"
DEPLOYMENT_MODE="${OASIS7_LOCAL_LETAI_DEPLOYMENT_MODE:-trusted_local_only}"
BRIDGE_SMOKE_ATTEMPTS="2"
BRIDGE_AUTO_TOPUP_USD="${OASIS7_LETAI_AUTO_TOPUP_USD:-0.1}"
CHAT_PROBE_TIMEOUT_MS="${OASIS7_LETAI_CHAT_PROBE_TIMEOUT_MS:-60000}"
CHAT_PROBE_ATTEMPTS="${OASIS7_LETAI_CHAT_PROBE_ATTEMPTS:-3}"
CHAT_PROBE_RETRY_DELAY_MS="${OASIS7_LETAI_CHAT_PROBE_RETRY_DELAY_MS:-2000}"
AGENT_PROVIDER_CONNECT_TIMEOUT_MS="${OASIS7_AGENT_PROVIDER_CONNECT_TIMEOUT_MS:-60000}"
MODEL=""
OUTPUT_DIR=""
BRIDGE_PID=""
DETACH="0"
PREFLIGHT_ONLY="0"
DRY_RUN_LAUNCH="0"
STARTUP_PROFILE="strict"
PROVIDER_SMOKE_MODE=""
PROVIDER_SMOKE_MODE_SET="0"
REUSE_EXISTING_BUILD="0"
LOCAL_WORLD_PLAYTEST_PRESET="0"
STARTUP_PROFILE_SET="0"
SOURCE_BIN_DIR="${OASIS7_LOCAL_LETAI_SOURCE_BIN_DIR:-$ROOT_DIR/target/debug}"
VIEWER_DIST_DIR="${OASIS7_LOCAL_LETAI_VIEWER_DIST_DIR:-$ROOT_DIR/crates/oasis7_viewer/dist}"
LAUNCHER_ARGS=()
BRIDGE_ARGS=()

usage() {
  cat <<'USAGE'
Usage: ./scripts/run-local-letai-game-test.sh [options] [-- launcher args...]

Daily manual local-only world playtest:

  ./scripts/run-local-letai-game-test.sh --local-world-playtest

This starts the canonical local real LetAI gameplay stack: token config
normalization, provider bridge, launcher/runtime/viewer, and a local standalone
chain commit -> snapshot loop. It does not connect to public_testnet/formal
testnet. Direct binary startup is for low-level debugging only and must mirror
these env vars by hand.

For a local environment attached to formal public_testnet world state, use:

  ./scripts/run-local-public-testnet-letai-test-environment.sh

Common options:
  --local-world-playtest    Daily local-only world preset. Implies
                              playtest startup, provider smoke skip, reuse build,
                              detach, auto-play, ports 48420/48421/48422,
                              --json-ready, and local standalone chain mode.
  --config <path>           LetAI config file (default: $OASIS7_LETAI_CONFIG_PATH,
                              /Users/scc/Documents/keys/letai.txt, then token-local fallback)
  --output-dir <path>       Launcher output dir; bridge log goes there too
  --preflight-only          Check local prerequisites and print the startup plan without launching
  --dry-run-launch          Print the resolved launcher plan without starting bridge or launcher
  --help-all                Show advanced/debug/compatibility options
  -h, --help                Show this help

Default startup launches the lower-level stack with chain runtime enabled in
local standalone test mode. That mode keeps the local submit -> commit ->
snapshot loop self-contained for manual playtest, and the path is considered
ready only after the launcher-managed chain runtime also materializes
`reward-runtime-execution-world/snapshot.json` and `journal.json`. If the page
briefly opens and then reports `viewer.ws` errors, inspect
`<output-dir>/launcher/oasis7_viewer_live.log` for that persistence ready gate
before treating the Viewer as the root cause.

Passing `-- --chain-disable` after this wrapper's options is a local page-play
mitigation only. It can keep the Viewer/WebSocket stable for manual inspection,
but it does not prove the local standalone chain-enabled world path is healthy.

Examples:
  ./scripts/run-local-letai-game-test.sh --local-world-playtest
  # Then open GAME_URL from output/local-letai-game-test/<run>/launcher/session.meta
USAGE
}

usage_all() {
  usage
  cat <<'USAGE'

Advanced/debug/compatibility options:
  --config <path>             LetAI config file (default: $OASIS7_LETAI_CONFIG_PATH,
                              /Users/scc/Documents/keys/letai.txt, then token-local fallback)
  --model <id>                LetAI model override (default: gpt-5.4 via helper)
  --bind <host:port>          Local provider bind address (default: 127.0.0.1:5841)
  --proxy <url>               HTTP/HTTPS proxy to export if unset (default: http://127.0.0.1:7897)
  --socks-proxy <url>         all_proxy value to export if unset (default: socks5://127.0.0.1:7897)
  --no-default-proxy          Do not set proxy environment defaults
  --auto-topup-usd <amount>   Auto top up on insufficient quota (default: 0.1)
  --no-ensure-token-config    Use --config directly; do not generate token config from platform key
  --chat-echo                 Enable local receipt-only chat echo for low-level debugging
  --no-chat-echo              Keep provider-backed agent chat disabled (default)
  --auto-play                 Start gameplay/world progression on viewer connection (default)
  --no-auto-play              Require manual Play before gameplay/world progression
  --deployment-mode <mode>    Launcher deployment mode (default: trusted_local_only)
  --hosted-public-join        Legacy compatibility only; hosted-public-join/testnet attach uses a different runbook
  --chat-probe-backend <name> rust-bridge|legacy-cli|none (default: rust-bridge)
  --skip-chat-probe           Alias for --chat-probe-backend none
  --startup-profile <mode>    strict|playtest (playtest keeps page startup usable when provider smoke degrades)
  --provider-smoke-mode <mode>
                              strict|degraded|skip (default strict; playtest default degraded)
  --skip-bridge-smoke         Alias for --provider-smoke-mode skip
  --reuse-existing-build      Reuse existing source-mode binaries instead of rebuilding them
  --preflight-only            Check local prerequisites and print the startup plan without launching
  --dry-run-launch            Print the resolved launcher plan without starting bridge or launcher
  --bridge-smoke-attempts <n> Retry provider bridge smoke up to <n> times (default: 2)
  --chat-probe-timeout-ms <ms>
                              LetAI chat probe timeout (default: 60000)
  --chat-probe-attempts <n>    Retry the full chat probe up to <n> times (default: 3)
  --chat-probe-retry-delay-ms <ms>
                              Delay between chat probe attempts (default: 2000)
  --agent-provider-connect-timeout-ms <ms>
                              Runtime provider decision timeout (default: 60000)
  --output-dir <path>         Launcher output dir; bridge log goes there too
  --detach                    Start the stack in the background and return after spawning

Examples:
  ./scripts/run-local-letai-game-test.sh --local-world-playtest
  ./scripts/run-local-letai-game-test.sh --local-world-playtest -- --viewer-port 4174 --json-ready

Legacy strict-provider diagnostic shape:
  ./scripts/run-local-letai-game-test.sh -- --viewer-port 4174 --json-ready
USAGE
}

cleanup() {
  if [[ -n "$BRIDGE_PID" ]] && kill -0 "$BRIDGE_PID" >/dev/null 2>&1; then
    kill "$BRIDGE_PID" >/dev/null 2>&1 || true
    wait "$BRIDGE_PID" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT INT TERM

command_available() {
  local command_name="$1"
  if [[ "$command_name" == "npm" && "${OASIS7_LOCAL_LETAI_TEST_MISSING_NPM:-0}" == "1" ]]; then
    return 1
  fi
  if [[ "$command_name" == "node" && "${OASIS7_LOCAL_LETAI_TEST_MISSING_NODE:-0}" == "1" ]]; then
    return 1
  fi
  command -v "$command_name" >/dev/null 2>&1
}

viewer_dist_ready() {
  [[ -f "$VIEWER_DIST_DIR/index.html" || -f "$VIEWER_DIST_DIR/viewer.html" ]]
}

bind_port() {
  addr_port "$BIND_ADDR"
}

addr_port() {
  python3 - "$1" <<'PY'
from __future__ import annotations

import sys

raw = sys.argv[1].strip()
if raw.startswith("["):
    _, _, tail = raw.rpartition("]:")
else:
    _, _, tail = raw.rpartition(":")
if not tail.isdigit():
    raise SystemExit(1)
print(tail)
PY
}

port_listeners() {
  local port="$1"
  if ! command -v lsof >/dev/null 2>&1; then
    return 2
  fi
  local output
  output="$(lsof -nP -iTCP:"$port" -sTCP:LISTEN 2>/dev/null || true)"
  printf '%s\n' "$output" | awk 'NR > 1 { print }'
}

provider_bind_listeners() {
  local port
  port="$(bind_port)" || return 1
  port_listeners "$port"
}

planned_launcher_ports() {
  local viewer_port="4173"
  local live_bind="127.0.0.1:5023"
  local web_bind="127.0.0.1:5011"
  local index=0
  while [[ "$index" -lt "${#LAUNCHER_ARGS[@]}" ]]; do
    case "${LAUNCHER_ARGS[$index]}" in
      --viewer-port)
        index=$((index + 1))
        viewer_port="${LAUNCHER_ARGS[$index]:-$viewer_port}"
        ;;
      --live-bind)
        index=$((index + 1))
        live_bind="${LAUNCHER_ARGS[$index]:-$live_bind}"
        ;;
      --web-bind)
        index=$((index + 1))
        web_bind="${LAUNCHER_ARGS[$index]:-$web_bind}"
        ;;
    esac
    index=$((index + 1))
  done
  printf 'viewer HTTP\t%s\n' "$viewer_port"
  printf 'live TCP\t%s\n' "$(addr_port "$live_bind")"
  printf 'web bridge\t%s\n' "$(addr_port "$web_bind")"
}

launcher_arg_present() {
  local needle="$1"
  local index=0
  while [[ "$index" -lt "${#LAUNCHER_ARGS[@]}" ]]; do
    if [[ "${LAUNCHER_ARGS[$index]}" == "$needle" ]]; then
      return 0
    fi
    index=$((index + 1))
  done
  return 1
}

append_launcher_arg_default() {
  local flag="$1"
  shift
  if launcher_arg_present "$flag"; then
    return 0
  fi
  LAUNCHER_ARGS+=("$flag" "$@")
}

append_launcher_switch_default() {
  local flag="$1"
  if launcher_arg_present "$flag"; then
    return 0
  fi
  LAUNCHER_ARGS+=("$flag")
}

planned_chain_profile() {
  if launcher_arg_present --chain-disable; then
    echo "disabled"
  else
    echo "local-standalone"
  fi
}

required_source_bins() {
  printf '%s\n' \
    oasis7_llm_provider_probe \
    oasis7_game_launcher \
    oasis7_viewer_live \
    oasis7_chain_runtime
}

run_preflight() {
  local failed="0"
  echo "local LetAI game test preflight"
  echo "startup_profile=$STARTUP_PROFILE"
  echo "provider_smoke_mode=$PROVIDER_SMOKE_MODE"
  echo "config=$CONFIG_PATH"
  echo "viewer_dist=$VIEWER_DIST_DIR"
  echo "source_bin_dir=$SOURCE_BIN_DIR"
  if [[ "$REUSE_EXISTING_BUILD" == "1" ]]; then
    echo "source_build=reuse-existing"
  else
    echo "source_build=build-if-needed"
  fi

  if [[ ! -f "$CONFIG_PATH" ]]; then
    echo "error: local playtest preflight failed: LetAI config not found: $CONFIG_PATH" >&2
    echo "hint: pass --config <path> or set OASIS7_LETAI_CONFIG_PATH." >&2
    failed="1"
  fi

  local bind_listeners=""
  if bind_listeners="$(provider_bind_listeners)"; then
    if [[ -n "$bind_listeners" ]]; then
      echo "error: local playtest preflight failed: provider bind address is already in use: $BIND_ADDR" >&2
      echo "hint: Stop the previous local playtest stack, or pass --bind <free host:port>." >&2
      echo "current listeners:" >&2
      printf '%s\n' "$bind_listeners" >&2
      failed="1"
    fi
  else
    echo "warning: could not inspect provider bind listeners for $BIND_ADDR; lsof may be unavailable." >&2
  fi

  local seen_ports=" "
  local launcher_label launcher_port launcher_listeners
  while IFS=$'\t' read -r launcher_label launcher_port; do
    [[ -n "$launcher_port" ]] || continue
    if [[ "$seen_ports" == *" $launcher_port "* ]]; then
      continue
    fi
    seen_ports="${seen_ports}${launcher_port} "
    if launcher_listeners="$(port_listeners "$launcher_port")"; then
      if [[ -n "$launcher_listeners" ]]; then
        echo "error: local playtest preflight failed: $launcher_label port is already in use: $launcher_port" >&2
        echo "hint: Stop the previous local playtest stack, or pass launcher overrides after --, for example -- --viewer-port <free-port> --live-bind 127.0.0.1:<free-port> --web-bind 127.0.0.1:<free-port>." >&2
        echo "current listeners:" >&2
        printf '%s\n' "$launcher_listeners" >&2
        failed="1"
      fi
    else
      echo "warning: could not inspect $launcher_label port $launcher_port; lsof may be unavailable." >&2
    fi
  done < <(planned_launcher_ports)

  if ! command_available node; then
    echo "error: local playtest preflight failed: missing Node.js runtime" >&2
    echo "hint: Install Node.js and npm, or use a prepared bundle/source build that does not require frontend rebuilds." >&2
    failed="1"
  fi

  if ! viewer_dist_ready && ! command_available npm; then
    echo "error: local playtest preflight failed: missing npm and viewer dist" >&2
    echo "hint: Install npm, or build/copy viewer dist at $VIEWER_DIST_DIR before launching." >&2
    echo "hint: rerun with --reuse-existing-build only after a successful source build." >&2
    failed="1"
  elif ! viewer_dist_ready; then
    echo "warning: viewer dist is missing; npm is available, so startup may rebuild frontend assets." >&2
  elif ! command_available npm; then
    echo "warning: npm is not available; using existing viewer dist at $VIEWER_DIST_DIR." >&2
  fi

  if [[ "$REUSE_EXISTING_BUILD" == "1" ]]; then
    local missing_bins=()
    local bin_name
    while IFS= read -r bin_name; do
      if [[ ! -x "$SOURCE_BIN_DIR/$bin_name" ]]; then
        missing_bins+=("$bin_name")
      fi
    done < <(required_source_bins)
    if [[ "${#missing_bins[@]}" -gt 0 ]]; then
      echo "error: local playtest preflight failed: --reuse-existing-build requested but required binaries are missing" >&2
      echo "missing: ${missing_bins[*]}" >&2
      echo "hint: Run without --reuse-existing-build once, or build the missing binaries under $SOURCE_BIN_DIR." >&2
      failed="1"
    fi
  else
    echo "note: source build may take several minutes on a cold cache; Cargo build locks are normal." >&2
  fi

  case "$PROVIDER_SMOKE_MODE" in
    strict)
      echo "provider_smoke=strict"
      ;;
    degraded)
      echo "provider_smoke=degraded"
      echo "note: provider smoke failures will be reported and startup will continue for page playtest." >&2
      ;;
    skip)
      echo "provider_smoke=skip"
      echo "warning: provider smoke is skipped; LLM/gameplay quality is not validated before startup." >&2
      ;;
  esac

  if [[ "$failed" != "0" ]]; then
    exit 2
  fi
  echo "local LetAI game test preflight passed"
}

print_launch_plan() {
  echo "local LetAI game test launch plan"
  echo "startup_profile=$STARTUP_PROFILE"
  echo "provider_smoke_mode=$PROVIDER_SMOKE_MODE"
  echo "provider_bind=$BIND_ADDR"
  echo "source_build=$([[ "$REUSE_EXISTING_BUILD" == "1" ]] && echo reuse-existing || echo build-if-needed)"
  echo "auto_play=$([[ "$AUTO_PLAY" == "1" ]] && echo enabled || echo disabled)"
  echo "chain_profile=$(planned_chain_profile)"
  echo "launcher_ports:"
  planned_launcher_ports | sed 's/^/  /'
  if [[ "${#LAUNCHER_ARGS[@]}" -gt 0 ]]; then
    printf 'launcher_passthrough='
    printf '%q ' "${LAUNCHER_ARGS[@]}"
    printf '\n'
  else
    echo "launcher_passthrough=(none)"
  fi
  case "$PROVIDER_SMOKE_MODE" in
    strict)
      echo "bridge_smoke=required"
      ;;
    degraded)
      echo "bridge_smoke=degrade-on-failure"
      echo "degraded startup will continue after provider smoke failure"
      echo "--skip-llm-provider-preflight"
      ;;
    skip)
      echo "bridge_smoke=skip"
      echo "--skip-bridge-smoke"
      echo "--skip-llm-provider-preflight"
      ;;
  esac
  if [[ "$REUSE_EXISTING_BUILD" == "1" ]]; then
    echo "OASIS7_RUN_LAUNCHER_STACK_SKIP_SOURCE_BUILD=1"
  else
    echo "OASIS7_RUN_LAUNCHER_STACK_SKIP_SOURCE_BUILD=0"
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --local-world-playtest)
      LOCAL_WORLD_PLAYTEST_PRESET="1"
      shift
      ;;
    --config)
      CONFIG_PATH="${2:-}"
      shift 2
      ;;
    --model)
      MODEL="${2:-}"
      shift 2
      ;;
    --bind)
      BIND_ADDR="${2:-}"
      shift 2
      ;;
    --proxy)
      PROXY_URL="${2:-}"
      shift 2
      ;;
    --socks-proxy)
      SOCKS_PROXY_URL="${2:-}"
      shift 2
      ;;
    --no-default-proxy)
      USE_DEFAULT_PROXY="0"
      shift
      ;;
    --auto-topup-usd)
      BRIDGE_AUTO_TOPUP_USD="${2:-}"
      shift 2
      ;;
    --no-ensure-token-config)
      ENSURE_TOKEN_CONFIG="0"
      shift
      ;;
    --chat-echo)
      CHAT_ECHO="1"
      shift
      ;;
    --no-chat-echo)
      CHAT_ECHO="0"
      shift
      ;;
    --auto-play)
      AUTO_PLAY="1"
      shift
      ;;
    --no-auto-play)
      AUTO_PLAY="0"
      shift
      ;;
    --deployment-mode)
      DEPLOYMENT_MODE="${2:-}"
      shift 2
      ;;
    --hosted-public-join)
      DEPLOYMENT_MODE="hosted_public_join"
      shift
      ;;
    --chat-probe-backend)
      CHAT_PROBE_BACKEND="${2:-}"
      shift 2
      ;;
    --skip-chat-probe)
      CHAT_PROBE_BACKEND="none"
      shift
      ;;
    --startup-profile)
      STARTUP_PROFILE="${2:-}"
      STARTUP_PROFILE_SET="1"
      shift 2
      ;;
    --provider-smoke-mode)
      PROVIDER_SMOKE_MODE="${2:-}"
      PROVIDER_SMOKE_MODE_SET="1"
      shift 2
      ;;
    --skip-bridge-smoke)
      PROVIDER_SMOKE_MODE="skip"
      PROVIDER_SMOKE_MODE_SET="1"
      SKIP_BRIDGE_SMOKE="1"
      shift
      ;;
    --reuse-existing-build)
      REUSE_EXISTING_BUILD="1"
      shift
      ;;
    --preflight-only)
      PREFLIGHT_ONLY="1"
      shift
      ;;
    --dry-run-launch)
      DRY_RUN_LAUNCH="1"
      shift
      ;;
    --bridge-smoke-attempts)
      BRIDGE_SMOKE_ATTEMPTS="${2:-}"
      shift 2
      ;;
    --chat-probe-timeout-ms)
      CHAT_PROBE_TIMEOUT_MS="${2:-}"
      shift 2
      ;;
    --chat-probe-attempts)
      CHAT_PROBE_ATTEMPTS="${2:-}"
      shift 2
      ;;
    --chat-probe-retry-delay-ms)
      CHAT_PROBE_RETRY_DELAY_MS="${2:-}"
      shift 2
      ;;
    --agent-provider-connect-timeout-ms)
      AGENT_PROVIDER_CONNECT_TIMEOUT_MS="${2:-}"
      shift 2
      ;;
    --output-dir)
      OUTPUT_DIR="${2:-}"
      shift 2
      ;;
    --detach)
      DETACH="1"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    --help-all)
      usage_all
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

if [[ "$LOCAL_WORLD_PLAYTEST_PRESET" == "1" ]]; then
  if [[ "$STARTUP_PROFILE_SET" == "0" ]]; then
    STARTUP_PROFILE="playtest"
  fi
  if [[ "$PROVIDER_SMOKE_MODE_SET" == "0" ]]; then
    PROVIDER_SMOKE_MODE="skip"
    PROVIDER_SMOKE_MODE_SET="1"
  fi
  REUSE_EXISTING_BUILD="1"
  if [[ "$DRY_RUN_LAUNCH" == "0" ]]; then
    DETACH="1"
  fi
  CHAT_ECHO="0"
  append_launcher_arg_default --viewer-port 48420
  append_launcher_arg_default --web-bind 127.0.0.1:48421
  append_launcher_arg_default --live-bind 127.0.0.1:48422
  append_launcher_switch_default --json-ready
fi

if [[ -z "$CONFIG_PATH" || -z "$BIND_ADDR" || -z "$BRIDGE_AUTO_TOPUP_USD" || -z "$BRIDGE_SMOKE_ATTEMPTS" || -z "$DEPLOYMENT_MODE" || -z "$CHAT_PROBE_BACKEND" || -z "$CHAT_PROBE_TIMEOUT_MS" || -z "$CHAT_PROBE_ATTEMPTS" || -z "$CHAT_PROBE_RETRY_DELAY_MS" || -z "$AGENT_PROVIDER_CONNECT_TIMEOUT_MS" ]]; then
  echo "error: empty --config, --bind, --deployment-mode, --auto-topup-usd, --bridge-smoke-attempts, --chat-probe-backend, --chat-probe-timeout-ms, --chat-probe-attempts, --chat-probe-retry-delay-ms, or --agent-provider-connect-timeout-ms is not allowed" >&2
  exit 2
fi
case "$CHAT_PROBE_BACKEND" in
  rust-bridge|legacy-cli|none) ;;
  *)
    echo "error: --chat-probe-backend must be rust-bridge, legacy-cli, or none" >&2
    exit 2
    ;;
esac
case "$STARTUP_PROFILE" in
  strict|playtest) ;;
  *)
    echo "error: --startup-profile must be strict or playtest" >&2
    exit 2
    ;;
esac
if [[ "$PROVIDER_SMOKE_MODE_SET" == "0" ]]; then
  if [[ "$STARTUP_PROFILE" == "playtest" ]]; then
    PROVIDER_SMOKE_MODE="degraded"
  else
    PROVIDER_SMOKE_MODE="strict"
  fi
fi
case "$PROVIDER_SMOKE_MODE" in
  strict|degraded|skip) ;;
  *)
    echo "error: --provider-smoke-mode must be strict, degraded, or skip" >&2
    exit 2
    ;;
esac
if [[ "$PROVIDER_SMOKE_MODE" == "skip" ]]; then
  SKIP_BRIDGE_SMOKE="1"
else
  SKIP_BRIDGE_SMOKE="0"
fi
if ! [[ "$BRIDGE_SMOKE_ATTEMPTS" =~ ^[0-9]+$ ]] || [[ "$BRIDGE_SMOKE_ATTEMPTS" -lt 1 ]]; then
  echo "error: --bridge-smoke-attempts must be a positive integer" >&2
  exit 2
fi
if ! [[ "$CHAT_PROBE_TIMEOUT_MS" =~ ^[0-9]+$ ]] || [[ "$CHAT_PROBE_TIMEOUT_MS" -lt 1000 ]]; then
  echo "error: --chat-probe-timeout-ms must be an integer >= 1000" >&2
  exit 2
fi
if ! [[ "$CHAT_PROBE_ATTEMPTS" =~ ^[0-9]+$ ]] || [[ "$CHAT_PROBE_ATTEMPTS" -lt 1 ]]; then
  echo "error: --chat-probe-attempts must be a positive integer" >&2
  exit 2
fi
if ! [[ "$CHAT_PROBE_RETRY_DELAY_MS" =~ ^[0-9]+$ ]]; then
  echo "error: --chat-probe-retry-delay-ms must be an integer >= 0" >&2
  exit 2
fi
if ! [[ "$AGENT_PROVIDER_CONNECT_TIMEOUT_MS" =~ ^[0-9]+$ ]] || [[ "$AGENT_PROVIDER_CONNECT_TIMEOUT_MS" -lt 1000 ]]; then
  echo "error: --agent-provider-connect-timeout-ms must be an integer >= 1000" >&2
  exit 2
fi

if [[ "$REUSE_EXISTING_BUILD" == "1" ]]; then
  export OASIS7_RUN_LAUNCHER_STACK_SKIP_SOURCE_BUILD=1
fi
if [[ "$PROVIDER_SMOKE_MODE" == "degraded" || "$PROVIDER_SMOKE_MODE" == "skip" ]]; then
  LAUNCHER_ARGS+=(--skip-llm-provider-preflight)
fi

if [[ "$USE_DEFAULT_PROXY" == "1" ]]; then
  export https_proxy="${https_proxy:-$PROXY_URL}"
  export http_proxy="${http_proxy:-$PROXY_URL}"
  export all_proxy="${all_proxy:-$SOCKS_PROXY_URL}"
fi
LOOPBACK_NO_PROXY="127.0.0.1,localhost,::1"
if [[ -n "${no_proxy:-}" ]]; then
  case ",$no_proxy," in
    *",127.0.0.1,"*) ;;
    *) export no_proxy="$LOOPBACK_NO_PROXY,$no_proxy" ;;
  esac
else
  export no_proxy="$LOOPBACK_NO_PROXY"
fi
if [[ -n "${NO_PROXY:-}" ]]; then
  case ",$NO_PROXY," in
    *",127.0.0.1,"*) ;;
    *) export NO_PROXY="$LOOPBACK_NO_PROXY,$NO_PROXY" ;;
  esac
else
  export NO_PROXY="$LOOPBACK_NO_PROXY"
fi
if [[ "$CHAT_ECHO" == "1" ]]; then
  export OASIS7_RUNTIME_AGENT_CHAT_ECHO=1
else
  unset OASIS7_RUNTIME_AGENT_CHAT_ECHO
fi
export OASIS7_LETAI_AUTO_TOPUP_USD="$BRIDGE_AUTO_TOPUP_USD"

RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
if [[ -z "$OUTPUT_DIR" ]]; then
  OUTPUT_DIR="$ROOT_DIR/output/local-letai-game-test/$RUN_STAMP"
fi
mkdir -p "$OUTPUT_DIR"

if [[ "$PREFLIGHT_ONLY" == "1" ]]; then
  run_preflight
  exit 0
fi

run_preflight

if [[ "$DRY_RUN_LAUNCH" == "1" ]]; then
  print_launch_plan
  exit 0
fi

if [[ -n "$MODEL" ]]; then
  BRIDGE_ARGS+=(--model "$MODEL")
fi

EFFECTIVE_CONFIG_PATH="$CONFIG_PATH"
if [[ "$ENSURE_TOKEN_CONFIG" == "1" ]]; then
  EFFECTIVE_CONFIG_PATH="$OUTPUT_DIR/letai-local-token.env"
  ENSURE_ARGS=()
  if [[ -n "$MODEL" ]]; then
    ENSURE_ARGS+=(--model "$MODEL")
  fi
  "$ROOT_DIR/scripts/ensure-letai-local-token-config.sh" \
    --config "$CONFIG_PATH" \
    --out "$EFFECTIVE_CONFIG_PATH" \
    ${ENSURE_ARGS[@]+"${ENSURE_ARGS[@]}"}
fi

if [[ "$DETACH" == "1" && "${OASIS7_LOCAL_LETAI_DETACHED_CHILD:-0}" != "1" ]]; then
  ABS_OUTPUT_DIR="$(cd "$OUTPUT_DIR" && pwd)"
  if [[ "$EFFECTIVE_CONFIG_PATH" == /* ]]; then
    ABS_EFFECTIVE_CONFIG_PATH="$EFFECTIVE_CONFIG_PATH"
  else
    ABS_EFFECTIVE_CONFIG_PATH="$ROOT_DIR/$EFFECTIVE_CONFIG_PATH"
  fi
  DETACH_LOG="$ABS_OUTPUT_DIR/local-letai-game-test.supervisor.log"
  DETACH_PID_FILE="$ABS_OUTPUT_DIR/local-letai-game-test.supervisor.pid"
  DETACH_LABEL_FILE="$ABS_OUTPUT_DIR/local-letai-game-test.launchctl-label"
  DETACH_SCRIPT="$ABS_OUTPUT_DIR/local-letai-game-test.detached.sh"
  DETACH_LABEL_SUFFIX="$(basename "$ABS_OUTPUT_DIR" | tr -c 'A-Za-z0-9_.-' '-')"
  DETACH_LABEL="oasis7.local-letai.$DETACH_LABEL_SUFFIX"
  DETACH_CMD=(
    "$ROOT_DIR/scripts/run-local-letai-game-test.sh"
    --config "$ABS_EFFECTIVE_CONFIG_PATH"
    --no-ensure-token-config
    --bind "$BIND_ADDR"
    --proxy "$PROXY_URL"
    --socks-proxy "$SOCKS_PROXY_URL"
    --auto-topup-usd "$BRIDGE_AUTO_TOPUP_USD"
    --chat-probe-backend "$CHAT_PROBE_BACKEND"
    --bridge-smoke-attempts "$BRIDGE_SMOKE_ATTEMPTS"
    --chat-probe-timeout-ms "$CHAT_PROBE_TIMEOUT_MS"
    --chat-probe-attempts "$CHAT_PROBE_ATTEMPTS"
    --chat-probe-retry-delay-ms "$CHAT_PROBE_RETRY_DELAY_MS"
    --agent-provider-connect-timeout-ms "$AGENT_PROVIDER_CONNECT_TIMEOUT_MS"
    --deployment-mode "$DEPLOYMENT_MODE"
    --startup-profile "$STARTUP_PROFILE"
    --provider-smoke-mode "$PROVIDER_SMOKE_MODE"
    --output-dir "$ABS_OUTPUT_DIR"
  )
  if [[ "$USE_DEFAULT_PROXY" != "1" ]]; then
    DETACH_CMD+=(--no-default-proxy)
  fi
  if [[ -n "$MODEL" ]]; then
    DETACH_CMD+=(--model "$MODEL")
  fi
  if [[ "$CHAT_ECHO" == "1" ]]; then
    DETACH_CMD+=(--chat-echo)
  else
    DETACH_CMD+=(--no-chat-echo)
  fi
  if [[ "$AUTO_PLAY" == "1" ]]; then
    DETACH_CMD+=(--auto-play)
  else
    DETACH_CMD+=(--no-auto-play)
  fi
  if [[ "$SKIP_BRIDGE_SMOKE" == "1" ]]; then
    DETACH_CMD+=(--skip-bridge-smoke)
  fi
  if [[ "$REUSE_EXISTING_BUILD" == "1" ]]; then
    DETACH_CMD+=(--reuse-existing-build)
  fi
  if [[ "$DRY_RUN_LAUNCH" == "1" ]]; then
    DETACH_CMD+=(--dry-run-launch)
  fi
  if [[ "${#LAUNCHER_ARGS[@]}" -gt 0 ]]; then
    DETACH_CMD+=(-- "${LAUNCHER_ARGS[@]}")
  fi

  {
    echo "#!/usr/bin/env bash"
    echo "set -euo pipefail"
    printf 'export HOME=%q\n' "${HOME:-}"
    printf 'export PATH=%q\n' "${PATH:-/usr/bin:/bin:/usr/sbin:/sbin}"
    printf 'exec >>%q 2>&1\n' "$DETACH_LOG"
    printf 'echo detached child started at "$(date +%%F\\ %%T\\ %%Z)" pid=$$ label=%q\n' "$DETACH_LABEL"
    echo "trap 'status=\$?; echo detached child exiting with status \$status >&2' EXIT"
    echo "trap 'echo detached child received SIGHUP >&2' HUP"
    echo "trap 'echo detached child received SIGTERM >&2' TERM"
    echo "trap 'echo detached child received SIGINT >&2' INT"
    printf 'cd %q\n' "$ROOT_DIR"
    printf 'env OASIS7_LOCAL_LETAI_DETACHED_CHILD=1 OASIS7_RUN_LAUNCHER_STACK_SKIP_SOURCE_BUILD=%q ' "${OASIS7_RUN_LAUNCHER_STACK_SKIP_SOURCE_BUILD:-0}"
    printf '%q ' "${DETACH_CMD[@]}"
    printf '\n'
  } >"$DETACH_SCRIPT"
  chmod +x "$DETACH_SCRIPT"
  : >"$DETACH_LOG"
  if [[ "${OASIS7_LOCAL_LETAI_TEST_DETACH_NO_SUBMIT:-0}" == "1" ]]; then
    echo "test detach submit skipped" >"$DETACH_PID_FILE"
  elif command -v launchctl >/dev/null 2>&1; then
    launchctl remove "$DETACH_LABEL" >/dev/null 2>&1 || true
    launchctl submit -l "$DETACH_LABEL" -- /bin/bash "$DETACH_SCRIPT"
    echo "$DETACH_LABEL" >"$DETACH_LABEL_FILE"
    sleep 0.2
    launchctl list | awk -v label="$DETACH_LABEL" '$3 == label { print $1 }' >"$DETACH_PID_FILE" || true
  else
    nohup /bin/bash "$DETACH_SCRIPT" >/dev/null 2>&1 &
    echo "$!" >"$DETACH_PID_FILE"
  fi
  echo "local LetAI game test detached"
  echo "output_dir=$ABS_OUTPUT_DIR"
  echo "supervisor_log=$DETACH_LOG"
  echo "supervisor_script=$DETACH_SCRIPT"
  if [[ -f "$DETACH_LABEL_FILE" ]]; then
    echo "launchctl_label=$(cat "$DETACH_LABEL_FILE")"
  fi
  if [[ -s "$DETACH_PID_FILE" ]]; then
    echo "supervisor_pid=$(cat "$DETACH_PID_FILE")"
  fi
  echo "wait for STACK_READY=1 in $ABS_OUTPUT_DIR/launcher/session.meta, then open the GAME_URL from that file"
  exit 0
fi

echo "local LetAI game test"
echo "config=$CONFIG_PATH"
echo "effective_config=$EFFECTIVE_CONFIG_PATH"
echo "bridge=http://$BIND_ADDR"
echo "chat_echo=$([[ "$CHAT_ECHO" == "1" ]] && echo enabled-debug-only || echo disabled)"
echo "auto_play=$([[ "$AUTO_PLAY" == "1" ]] && echo enabled || echo disabled)"
echo "deployment_mode=$DEPLOYMENT_MODE"
echo "startup_profile=$STARTUP_PROFILE"
echo "provider_smoke_mode=$PROVIDER_SMOKE_MODE"
echo "chat_probe_backend=$CHAT_PROBE_BACKEND"
echo "chat_probe_timeout_ms=$CHAT_PROBE_TIMEOUT_MS"
echo "chat_probe_attempts=$CHAT_PROBE_ATTEMPTS"
echo "chat_probe_retry_delay_ms=$CHAT_PROBE_RETRY_DELAY_MS"
echo "agent_provider_connect_timeout_ms=$AGENT_PROVIDER_CONNECT_TIMEOUT_MS"
echo "output_dir=$OUTPUT_DIR"
export OASIS7_AGENT_PROVIDER_CONNECT_TIMEOUT_MS="$AGENT_PROVIDER_CONNECT_TIMEOUT_MS"
export OASIS7_AGENT_PROVIDER_DECISION_TIMEOUT_MS="$AGENT_PROVIDER_CONNECT_TIMEOUT_MS"

if [[ "$CHAT_PROBE_BACKEND" == "legacy-cli" ]]; then
  chat_probe_status=1
  for attempt in $(seq 1 "$CHAT_PROBE_ATTEMPTS"); do
    set +e
    "$ROOT_DIR/scripts/check-letai-chat-completions.sh" \
      --config "$EFFECTIVE_CONFIG_PATH" \
      --timeout-ms "$CHAT_PROBE_TIMEOUT_MS" \
      ${BRIDGE_ARGS[@]+"${BRIDGE_ARGS[@]}"}
    chat_probe_status=$?
    set -e
    if [[ "$chat_probe_status" -eq 0 ]]; then
      chat_probe_status=0
      break
    fi
    echo "LetAI chat probe attempt $attempt/$CHAT_PROBE_ATTEMPTS failed" >&2
    if [[ "$attempt" -lt "$CHAT_PROBE_ATTEMPTS" && "$CHAT_PROBE_RETRY_DELAY_MS" -gt 0 ]]; then
      sleep "$(((CHAT_PROBE_RETRY_DELAY_MS + 999) / 1000))"
    fi
  done
  if [[ "$chat_probe_status" -ne 0 ]]; then
    exit "$chat_probe_status"
  fi
fi

"$ROOT_DIR/scripts/run-local-letai-provider-bridge.sh" \
  --config "$EFFECTIVE_CONFIG_PATH" \
  --bind "$BIND_ADDR" \
  --auto-topup-usd "$BRIDGE_AUTO_TOPUP_USD" \
  ${BRIDGE_ARGS[@]+"${BRIDGE_ARGS[@]}"} \
  >"$OUTPUT_DIR/local-letai-provider-bridge.log" 2>&1 &
BRIDGE_PID="$!"

for _ in $(seq 1 90); do
  if ! kill -0 "$BRIDGE_PID" >/dev/null 2>&1; then
    echo "error: local LetAI bridge exited early; log: $OUTPUT_DIR/local-letai-provider-bridge.log" >&2
    tail -n 80 "$OUTPUT_DIR/local-letai-provider-bridge.log" >&2 || true
    exit 1
  fi
  if curl -fsS "http://$BIND_ADDR/v1/provider/info" >/dev/null 2>&1; then
    if ! kill -0 "$BRIDGE_PID" >/dev/null 2>&1; then
      echo "error: local LetAI bridge exited while provider endpoint was becoming ready; log: $OUTPUT_DIR/local-letai-provider-bridge.log" >&2
      tail -n 80 "$OUTPUT_DIR/local-letai-provider-bridge.log" >&2 || true
      exit 1
    fi
    break
  fi
  sleep 1
done

if ! curl -fsS "http://$BIND_ADDR/v1/provider/info" >/dev/null 2>&1; then
  echo "error: local LetAI bridge did not become ready; log: $OUTPUT_DIR/local-letai-provider-bridge.log" >&2
  tail -n 80 "$OUTPUT_DIR/local-letai-provider-bridge.log" >&2 || true
  exit 1
fi

if [[ "$PROVIDER_SMOKE_MODE" == "strict" || "$PROVIDER_SMOKE_MODE" == "degraded" ]]; then
  smoke_status=1
  for attempt in $(seq 1 "$BRIDGE_SMOKE_ATTEMPTS"); do
    if "$ROOT_DIR/scripts/provider-remote-https/provider-bridge-contract-smoke.sh" \
      --base-url "http://$BIND_ADDR" \
      --timeout-ms "$AGENT_PROVIDER_CONNECT_TIMEOUT_MS" \
      --legacy-compatibility-only \
      --decision-count 1 \
      --min-successes 1; then
      smoke_status=0
      break
    fi
    echo "provider bridge smoke attempt $attempt/$BRIDGE_SMOKE_ATTEMPTS failed" >&2
    sleep 2
  done
  if [[ "$smoke_status" -ne 0 ]]; then
    "$ROOT_DIR/scripts/provider-remote-https/provider-bridge-contract-smoke.sh" \
      --base-url "http://$BIND_ADDR" \
      --timeout-ms "$AGENT_PROVIDER_CONNECT_TIMEOUT_MS" \
      --legacy-compatibility-only \
      --decision-count 1 \
      --min-successes 0 >&2 || true
    if [[ "$PROVIDER_SMOKE_MODE" == "degraded" ]]; then
      echo "warning: provider bridge smoke failed; continuing because --provider-smoke-mode degraded is active" >&2
      echo "warning: page startup may succeed, but provider-backed LLM actions may show runtime blockers until the upstream recovers" >&2
    else
      echo "error: provider bridge smoke failed in strict mode" >&2
      echo "hint: retry later, or use --startup-profile playtest / --provider-smoke-mode degraded to open the page while preserving a warning." >&2
      exit 1
    fi
  fi
elif [[ "$CHAT_PROBE_BACKEND" == "rust-bridge" ]]; then
  echo "warning: --skip-bridge-smoke also skips the default Rust bridge chat probe" >&2
fi

set +e
LAUNCHER_MODE_ARGS=(--deployment-mode "$DEPLOYMENT_MODE")
if [[ "$DEPLOYMENT_MODE" == "trusted_local_only" ]]; then
  LAUNCHER_MODE_ARGS+=(--allow-trusted-local-playtest)
fi
if [[ "$AUTO_PLAY" == "1" ]]; then
  LAUNCHER_MODE_ARGS+=(--auto-play)
else
  LAUNCHER_MODE_ARGS+=(--no-auto-play)
fi
LAUNCHER_CMD=(
  "$ROOT_DIR/scripts/run-launcher-stack.sh"
  "${LAUNCHER_MODE_ARGS[@]}"
  --chain-local-standalone-test
  --agent-decision-source provider_backed \
  --agent-provider-url "http://$BIND_ADDR" \
  --output-dir "$OUTPUT_DIR/launcher"
)
if [[ "${#LAUNCHER_ARGS[@]}" -gt 0 ]]; then
  LAUNCHER_CMD+=("${LAUNCHER_ARGS[@]}")
fi
"${LAUNCHER_CMD[@]}"
launcher_status=$?
set -e

echo "local LetAI launcher stack exited with status $launcher_status" >&2

exit "$launcher_status"
