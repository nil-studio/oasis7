#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT_DIR/scripts/worktree-harness-lib.sh"
source "$ROOT_DIR/scripts/agent-browser-lib.sh"
source "$ROOT_DIR/scripts/bundle-freshness-lib.sh"
source "$ROOT_DIR/scripts/cargo-dev-lib.sh"
source "$ROOT_DIR/scripts/hosted-login-gate-env-lib.sh"

# Test-only seam used by the repository-owned lifecycle acceptance lane.  It
# is intentionally an explicit environment variable and exits before any
# production provider/build path; normal callers retain the real launcher
# default because the variable is unset.
TEST_ONLY_LAUNCHER_COMMAND="${OASIS7_HARNESS_TEST_LAUNCHER_COMMAND:-}"
ORIGINAL_ARGS=("$@")
if [[ -n "$TEST_ONLY_LAUNCHER_COMMAND" ]]; then
  if [[ "$TEST_ONLY_LAUNCHER_COMMAND" != /* || ! -x "$TEST_ONLY_LAUNCHER_COMMAND" ]]; then
    echo "error: OASIS7_HARNESS_TEST_LAUNCHER_COMMAND must be an executable absolute path" >&2
    exit 1
  fi
  exec "$TEST_ONLY_LAUNCHER_COMMAND" "${ORIGINAL_ARGS[@]}"
fi

VIEWER_HOST="127.0.0.1"
VIEWER_PORT="4173"
LIVE_BIND_ADDR="127.0.0.1:5023"
WEB_BRIDGE_ADDR="127.0.0.1:5011"
ENABLE_LLM="1"
AUTO_PLAY="${OASIS7_RUNTIME_AUTO_PLAY:-1}"
SCENARIO=""
ALLOW_DEBUG_SCENARIO="0"
VIEWER_STATIC_DIR="web"
CHAIN_ENABLED="1"
DEPLOYMENT_MODE="${OASIS7_DEPLOYMENT_MODE:-hosted_public_join}"
ALLOW_TRUSTED_LOCAL_PLAYTEST="${OASIS7_ALLOW_TRUSTED_LOCAL_PLAYTEST:-0}"
CHAIN_LINK_POLICY="${OASIS7_CHAIN_LINK_POLICY:-}"
CHAIN_NODE_ID=""
CHAIN_STATUS_BIND_ADDR=""
CHAIN_EXECUTION_WORLD_DIR="${OASIS7_CHAIN_EXECUTION_WORLD_DIR:-}"
CHAIN_LOCAL_STANDALONE_TEST="${OASIS7_CHAIN_LOCAL_STANDALONE_TEST:-0}"
CHAIN_NODE_AUTO_ATTEST_ALL="${OASIS7_CHAIN_NODE_AUTO_ATTEST_ALL:-0}"
CHAIN_NODE_VALIDATORS=()
MAJOR_WORLD_EVENT_VISIBILITY="unknown"
BUNDLE_DIR=""
BUNDLE_PROFILE="packaging"
BUNDLE_TARGET_TRIPLE="native"
VIEWER_STATIC_DIR_EXPLICIT="0"
ALLOW_STALE_BUNDLE="0"
OUTPUT_DIR=""
RUN_ID=""
META_FILE=""
JSON_READY="0"
SKIP_LLM_PROVIDER_PREFLIGHT="0"
AGENT_DECISION_SOURCE="${OASIS7_AGENT_DECISION_SOURCE:-provider_backed}"
AGENT_PROVIDER_LANE="${OASIS7_AGENT_PROVIDER_LANE:-local}"
AGENT_PROVIDER_BACKEND="${OASIS7_AGENT_PROVIDER_BACKEND:-provider_local_bridge}"
AGENT_PROVIDER_CONTRACT="${OASIS7_AGENT_PROVIDER_CONTRACT:-worldsim_provider_v1}"
AGENT_PROVIDER_TRANSPORT="${OASIS7_AGENT_PROVIDER_TRANSPORT:-}"
AGENT_PROVIDER_URL="${OASIS7_AGENT_PROVIDER_URL:-}"
AGENT_PROVIDER_AUTH_TOKEN="${OASIS7_AGENT_PROVIDER_AUTH_TOKEN:-}"
AGENT_PROVIDER_CONNECT_TIMEOUT_MS="${OASIS7_AGENT_PROVIDER_CONNECT_TIMEOUT_MS:-15000}"
AGENT_PROVIDER_PROFILE="${OASIS7_AGENT_PROVIDER_PROFILE:-oasis7_p0_low_freq_npc}"
AGENT_EXECUTION_LANE="${OASIS7_AGENT_EXECUTION_LANE:-headless_agent}"
PROVIDER_BOOTSTRAP_AUTHORITY_PATHS=()
LOCAL_TEST_PROVIDER_AUTHORITY_PATH="${OASIS7_LOCAL_TEST_PROVIDER_AUTHORITY_PATH:-}"
LOCAL_TEST_PROVIDER_WASM_PATH="${OASIS7_LOCAL_TEST_PROVIDER_WASM_PATH:-}"
LOCAL_TEST_PROVIDER_METADATA_PATH="${OASIS7_LOCAL_TEST_PROVIDER_METADATA_PATH:-}"
LOCAL_TEST_PROVIDER_AGENT_ID="${OASIS7_LOCAL_TEST_PROVIDER_AGENT_ID:-starter-agent-0}"
LOCAL_TEST_PROVIDER_OWNER_BINDING="${OASIS7_LOCAL_TEST_PROVIDER_OWNER_BINDING:-local-test-owner-0}"
LOCAL_TEST_PROVIDER_FINALITY_BLOCK_HASH="${OASIS7_LOCAL_TEST_PROVIDER_FINALITY_BLOCK_HASH:-}"
LOCAL_TEST_PROVIDER_SESSION_MODE="${OASIS7_LOCAL_TEST_PROVIDER_SESSION_MODE:-hosted_public_join}"
LOCAL_TEST_PROVIDER_ADMISSION="none"
AGENT_PROVIDER_PROD_URL="${OASIS7_AGENT_PROVIDER_PROD_URL:-https://t2t.oasis7.tech}"
AGENT_PROVIDER_TEST_URL="${OASIS7_AGENT_PROVIDER_TEST_URL:-}"
PRINT_AGENT_PROVIDER_CONFIG="0"

usage() {
  cat <<'USAGE'
Usage: ./scripts/run-launcher-stack.sh [options]

Start the lower-level launcher/runtime bootstrap used by wrappers and targeted
regressions.

Operator-facing playtest entries:
- Producer / release manual playtest:
  ./scripts/run-producer-playtest.sh --open-headed

Advanced bundle bootstrap:
- ./scripts/build-game-launcher-bundle.sh --out-dir output/release/game-launcher-local
- ./scripts/run-launcher-stack.sh --bundle-dir output/release/game-launcher-local
- stale or manifest-less bundles fail fast unless `--allow-stale-bundle` is passed

Development / debugging fallback:
- source oasis7_game_launcher via cargo run with the same runtime defaults

Chain-enabled ready boundary:
- `--chain-enable` is the default and starts launcher-managed chain runtime.
- The launcher does not treat that path as healthy until
  `output/chain-runtime/<node-id>/reward-runtime-execution-world/snapshot.json`
  and `journal.json` exist. If they do not appear before the ready timeout, the
  launcher exits; browsers that already loaded the page may surface this as
  `viewer.ws` / WebSocket errors.
- `--chain-disable` is useful only as a local Viewer/page-play mitigation. It
  bypasses the chain persistence ready gate and must not be reported as a
  chain-enabled local standalone world pass.

Options:
  --scenario <name>        Optional debug scenario; default uses formal release fixed world
  --allow-debug-scenario   Allow seeded debug scenarios such as llm_bootstrap
  --bundle-dir <path>      Use packaged bundle <path>/run-game.sh (advanced / wrapper path;
                           producer manual play should prefer run-producer-playtest.sh)
  --bundle-profile <name>  Expected bundle build profile: packaging|dev (default: packaging)
  --bundle-target-triple <id>
                           Expected bundle target triple (default: native)
  --viewer-host <host>     Viewer HTTP host (default: 127.0.0.1)
  --viewer-port <port>     Viewer HTTP port (default: 4173)
  --live-bind <addr:port>  oasis7_game_launcher live TCP bind (default: 127.0.0.1:5023)
  --web-bind <addr:port>   WebSocket bridge bind (default: 127.0.0.1:5011)
  --deployment-mode <mode> hosted_public_join (default) or trusted_local_only with --allow-trusted-local-playtest
  --allow-trusted-local-playtest
                           Allow trusted_local_only for local-only playtest stacks
  --viewer-static-dir <p>  Override viewer static dir; source mode defaults to fresh `web`, bundle mode only uses this as an advanced override
                           Provider lineage checkpoint is stored at <output-dir>/viewer-provider-lineage.json
  --allow-stale-bundle    Skip workspace freshness guard for --bundle-dir (advanced / explicit override)
  --chain-enable           Enable chain runtime (default; requires execution-world persistence ready)
  --chain-disable          Disable chain runtime (Viewer/page-play mitigation only)
  --chain-node-id <id>     Override chain node id (default: fresh per run)
  --chain-execution-world-dir <path> Shared local execution root (required for external chain)
  --chain-status-bind <a:p> Override chain status HTTP bind (default: web-bind port + 110)
  --chain-link-policy <p>  enforcing or shadow (default: shadow for trusted local playtest, otherwise enforcing)
  --major-world-event-visibility <policy>
                           explicit audience policy: unknown|public|restricted|denied (default: unknown)
  --chain-local-standalone-test
                           Start a single-node local chain profile for submit/commit/snapshot testing
  --chain-node-validator <v:s>
                           Add a local validator for oasis7_chain_runtime (repeatable)
  --chain-node-auto-attest-all
                           Enable local auto-attestation for configured validators
  --chain-node-no-auto-attest-all
                           Disable local auto-attestation
  --output-dir <path>      Override runtime log/artifact output directory
  --run-id <id>            Override logical run id used for output dir / chain node id defaults
  --meta-file <path>       Override metadata file path (default: <output-dir>/session.meta)
  --json-ready             Emit one-line JSON ready payload after the stack becomes ready
  --skip-llm-provider-preflight
                           Skip the active LLM/provider probe before launcher startup
  --agent-decision-source <s>
                           provider_backed (default) or builtin_llm
  --agent-provider-lane <lane>
                           local (default), local-mock (smoke/CI only), test, or prod
  --agent-provider-url <u> Provider bridge URL override for provider_backed mode
  --agent-provider-auth-token <t>
                           Optional provider bridge bearer token (if configured on the bridge)
  --agent-provider-transport <t>
                           loopback_http (default) or remote_https
  --agent-execution-lane <lane>
                           headless_agent (default) or player_parity
  --provider-bootstrap-authority <path>
                           Explicit JSON Runtime authority bundle; repeat per ProviderBacked agent
  --local-test-provider-authority <path>
                           Explicit DevLocal authority output bundle (requires local standalone + builtin_llm, or the explicit test-tier Hosted local-mock tuple)
  --local-test-provider-wasm <path>
                           Real WASM artifact for the explicit DevLocal provider setup
  --local-test-provider-metadata <path>
                           Canonical build-suite metadata JSON for the WASM artifact
  --local-test-provider-agent-id <id>
                           Starter agent to provision (default: starter-agent-0)
  --local-test-provider-owner-binding <id>
                           Stable local session owner binding (default: local-test-owner-0)
  --local-test-provider-finality-block-hash <hash>
                           Explicit local finality marker, blake3:<64 lowercase hex>
  --local-test-provider-session-mode <mode>
                           hosted_public_join|loopback (default: hosted_public_join)
  --with-llm               Enable LLM mode (default: enabled; required for gameplay)
  --no-llm                 Negative-path only; this launcher stack now fails fast without LLM
  --auto-play              Start gameplay/world progression on viewer connection (default)
  --no-auto-play           Keep gameplay/world paused until explicit Play actions
  --print-agent-provider-config
                           Print resolved provider config as JSON and exit
  -h, --help               Show this help
USAGE
  "$ROOT_DIR/scripts/launcher-help-contract.sh" shared
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --scenario)
      if [[ $# -lt 2 || -z "${2:-}" ]]; then
        echo "error: --scenario requires a value" >&2
        usage >&2
        exit 1
      fi
      SCENARIO="$2"
      shift 2
      ;;
    --allow-debug-scenario)
      ALLOW_DEBUG_SCENARIO="1"
      shift
      ;;
    --bundle-dir)
      BUNDLE_DIR="${2:-}"
      shift 2
      ;;
    --bundle-profile)
      BUNDLE_PROFILE="${2:-}"
      shift 2
      ;;
    --bundle-target-triple)
      BUNDLE_TARGET_TRIPLE="${2:-}"
      shift 2
      ;;
    --viewer-host)
      VIEWER_HOST="${2:-}"
      shift 2
      ;;
    --viewer-port)
      VIEWER_PORT="${2:-}"
      shift 2
      ;;
    --live-bind)
      LIVE_BIND_ADDR="${2:-}"
      shift 2
      ;;
    --web-bind)
      WEB_BRIDGE_ADDR="${2:-}"
      shift 2
      ;;
    --deployment-mode)
      DEPLOYMENT_MODE="${2:-}"
      shift 2
      ;;
    --allow-trusted-local-playtest)
      ALLOW_TRUSTED_LOCAL_PLAYTEST="1"
      shift
      ;;
    --viewer-static-dir)
      VIEWER_STATIC_DIR="${2:-}"
      VIEWER_STATIC_DIR_EXPLICIT="1"
      shift 2
      ;;
    --allow-stale-bundle)
      ALLOW_STALE_BUNDLE="1"
      shift
      ;;
    --output-dir)
      OUTPUT_DIR="${2:-}"
      shift 2
      ;;
    --run-id)
      RUN_ID="${2:-}"
      shift 2
      ;;
    --meta-file)
      META_FILE="${2:-}"
      shift 2
      ;;
    --json-ready)
      JSON_READY="1"
      shift
      ;;
    --skip-llm-provider-preflight)
      SKIP_LLM_PROVIDER_PREFLIGHT="1"
      shift
      ;;
    --print-agent-provider-config)
      PRINT_AGENT_PROVIDER_CONFIG="1"
      shift
      ;;
    --agent-decision-source)
      AGENT_DECISION_SOURCE="${2:-}"
      shift 2
      ;;
    --agent-provider-lane|--agent-provider-env)
      AGENT_PROVIDER_LANE="${2:-}"
      shift 2
      ;;
    --agent-provider-url)
      AGENT_PROVIDER_URL="${2:-}"
      shift 2
      ;;
    --agent-provider-auth-token)
      AGENT_PROVIDER_AUTH_TOKEN="${2:-}"
      shift 2
      ;;
    --agent-provider-transport)
      AGENT_PROVIDER_TRANSPORT="${2:-}"
      shift 2
      ;;
    --agent-execution-lane)
      AGENT_EXECUTION_LANE="${2:-}"
      shift 2
      ;;
    --provider-bootstrap-authority)
      if [[ $# -lt 2 || -z "${2:-}" ]]; then
        echo "error: --provider-bootstrap-authority requires a path" >&2
        usage >&2
        exit 1
      fi
      PROVIDER_BOOTSTRAP_AUTHORITY_PATHS+=("$2")
      shift 2
      ;;
    --local-test-provider-authority)
      LOCAL_TEST_PROVIDER_AUTHORITY_PATH="${2:-}"
      shift 2
      ;;
    --local-test-provider-wasm)
      LOCAL_TEST_PROVIDER_WASM_PATH="${2:-}"
      shift 2
      ;;
    --local-test-provider-metadata)
      LOCAL_TEST_PROVIDER_METADATA_PATH="${2:-}"
      shift 2
      ;;
    --local-test-provider-agent-id)
      LOCAL_TEST_PROVIDER_AGENT_ID="${2:-}"
      shift 2
      ;;
    --local-test-provider-owner-binding)
      LOCAL_TEST_PROVIDER_OWNER_BINDING="${2:-}"
      shift 2
      ;;
    --local-test-provider-finality-block-hash)
      LOCAL_TEST_PROVIDER_FINALITY_BLOCK_HASH="${2:-}"
      shift 2
      ;;
    --local-test-provider-session-mode)
      LOCAL_TEST_PROVIDER_SESSION_MODE="${2:-}"
      shift 2
      ;;
    --chain-enable)
      CHAIN_ENABLED="1"
      shift
      ;;
    --chain-disable)
      CHAIN_ENABLED="0"
      shift
      ;;
    --chain-node-id)
      CHAIN_NODE_ID="${2:-}"
      shift 2
      ;;
    --chain-execution-world-dir)
      CHAIN_EXECUTION_WORLD_DIR="${2:-}"
      shift 2
      ;;
    --chain-status-bind)
      CHAIN_STATUS_BIND_ADDR="${2:-}"
      shift 2
      ;;
    --chain-link-policy)
      CHAIN_LINK_POLICY="${2:-}"
      shift 2
      ;;
    --major-world-event-visibility)
      MAJOR_WORLD_EVENT_VISIBILITY="${2:-}"
      shift 2
      ;;
    --chain-local-standalone-test)
      CHAIN_LOCAL_STANDALONE_TEST="1"
      CHAIN_NODE_AUTO_ATTEST_ALL="1"
      shift
      ;;
    --chain-node-validator)
      CHAIN_NODE_VALIDATORS+=("${2:-}")
      shift 2
      ;;
    --chain-node-auto-attest-all)
      CHAIN_NODE_AUTO_ATTEST_ALL="1"
      shift
      ;;
    --chain-node-no-auto-attest-all)
      CHAIN_NODE_AUTO_ATTEST_ALL="0"
      shift
      ;;
    --with-llm)
      ENABLE_LLM="1"
      shift
      ;;
    --no-llm)
      ENABLE_LLM="0"
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
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown option: $1" >&2
      usage
      exit 1
      ;;
  esac
done

if [[ -z "$VIEWER_HOST" || -z "$VIEWER_PORT" || -z "$LIVE_BIND_ADDR" || -z "$WEB_BRIDGE_ADDR" || -z "$VIEWER_STATIC_DIR" || -z "$DEPLOYMENT_MODE" ]]; then
  echo "error: empty argument is not allowed" >&2
  exit 1
fi

if [[ "$BUNDLE_PROFILE" != "packaging" && "$BUNDLE_PROFILE" != "dev" ]]; then
  echo "error: --bundle-profile must be packaging or dev" >&2
  exit 1
fi
if [[ -z "$BUNDLE_TARGET_TRIPLE" ]]; then
  echo "error: --bundle-target-triple must not be empty" >&2
  exit 1
fi

if ! [[ "$VIEWER_PORT" =~ ^[0-9]+$ ]]; then
  echo "error: --viewer-port must be numeric" >&2
  exit 1
fi

if [[ "$LIVE_BIND_ADDR" != *:* || "$WEB_BRIDGE_ADDR" != *:* ]]; then
  echo "error: --live-bind/--web-bind must be in <host:port> format" >&2
  exit 1
fi

LIVE_BIND_HOST="${LIVE_BIND_ADDR%:*}"
LIVE_BIND_PORT="${LIVE_BIND_ADDR##*:}"
WEB_BRIDGE_HOST="${WEB_BRIDGE_ADDR%:*}"
WEB_BRIDGE_PORT="${WEB_BRIDGE_ADDR##*:}"

if [[ -z "$LIVE_BIND_HOST" || -z "$LIVE_BIND_PORT" || -z "$WEB_BRIDGE_HOST" || -z "$WEB_BRIDGE_PORT" ]]; then
  echo "error: invalid bind address" >&2
  exit 1
fi

if ! [[ "$LIVE_BIND_PORT" =~ ^[0-9]+$ && "$WEB_BRIDGE_PORT" =~ ^[0-9]+$ ]]; then
  echo "error: bind ports must be numeric" >&2
  exit 1
fi

if [[ -n "$BUNDLE_DIR" ]]; then
  if [[ ! -d "$BUNDLE_DIR" ]]; then
    echo "error: --bundle-dir path does not exist: $BUNDLE_DIR" >&2
    exit 1
  fi
  BUNDLE_DIR="$(cd "$BUNDLE_DIR" && pwd)"
  if [[ ! -f "$BUNDLE_DIR/run-game.sh" ]]; then
    echo "error: bundle is missing run-game.sh: $BUNDLE_DIR" >&2
    exit 1
  fi
  if [[ "$ALLOW_STALE_BUNDLE" != "1" ]]; then
    if ! freshness_note=$(bundle_check_freshness "$ROOT_DIR" "$BUNDLE_DIR" "$BUNDLE_PROFILE" "$BUNDLE_TARGET_TRIPLE" 2>&1); then
      echo "error: $freshness_note" >&2
      echo "hint: rebuild via ./scripts/build-game-launcher-bundle.sh --out-dir $BUNDLE_DIR or rerun producer entry with --rebuild; use --allow-stale-bundle only when intentionally validating an older artifact" >&2
      exit 1
    fi
  fi
fi

if [[ "$ENABLE_LLM" != "1" ]]; then
  echo "error: ./scripts/run-launcher-stack.sh now wraps oasis7_game_launcher, which requires active LLM access" >&2
  echo "hint: use env -u RUSTC_WRAPPER cargo run -p oasis7_viewer_runtime --bin oasis7_viewer_live -- llm_bootstrap --allow-debug-scenario --no-llm ... only for direct observer/debug diagnostics" >&2
  exit 1
fi

resolve_agent_provider_lane_defaults() {
  if [[ "$AGENT_DECISION_SOURCE" == "builtin_llm" ]]; then
    return 0
  fi

  case "$AGENT_PROVIDER_LANE" in
    local)
      AGENT_PROVIDER_TRANSPORT="${AGENT_PROVIDER_TRANSPORT:-loopback_http}"
      AGENT_PROVIDER_URL="${AGENT_PROVIDER_URL:-http://127.0.0.1:5841}"
      ;;
    local-mock|mock)
      AGENT_PROVIDER_LANE="local-mock"
      if [[ -z "${OASIS7_AGENT_PROVIDER_BACKEND:-}" || "$AGENT_PROVIDER_BACKEND" == "provider_local_bridge" ]]; then
        AGENT_PROVIDER_BACKEND="provider_local_mock"
      fi
      AGENT_PROVIDER_TRANSPORT="${AGENT_PROVIDER_TRANSPORT:-loopback_http}"
      AGENT_PROVIDER_URL="${AGENT_PROVIDER_URL:-http://127.0.0.1:5841}"
      ;;
    test)
      AGENT_PROVIDER_TRANSPORT="${AGENT_PROVIDER_TRANSPORT:-remote_https}"
      AGENT_PROVIDER_URL="${AGENT_PROVIDER_URL:-$AGENT_PROVIDER_TEST_URL}"
      if [[ -z "$AGENT_PROVIDER_URL" ]]; then
        echo "error: --agent-provider-lane test requires --agent-provider-url or OASIS7_AGENT_PROVIDER_TEST_URL" >&2
        exit 1
      fi
      ;;
    prod|production|online)
      AGENT_PROVIDER_LANE="prod"
      AGENT_PROVIDER_TRANSPORT="${AGENT_PROVIDER_TRANSPORT:-remote_https}"
      AGENT_PROVIDER_URL="${AGENT_PROVIDER_URL:-$AGENT_PROVIDER_PROD_URL}"
      ;;
    *)
      echo "error: --agent-provider-lane must be one of local, local-mock, test, prod; got $AGENT_PROVIDER_LANE" >&2
      exit 1
      ;;
  esac
}

resolve_agent_provider_lane_defaults

provider_url_is_loopback() {
  local provider_url="$1"
  python3 - "$provider_url" <<'PY'
import ipaddress
import sys
from urllib.parse import urlsplit

parts = urlsplit(sys.argv[1])
if parts.scheme != "http" or not parts.hostname:
    raise SystemExit(1)
host = parts.hostname.strip().lower()
if host == "localhost":
    raise SystemExit(0)
try:
    is_loopback = ipaddress.ip_address(host).is_loopback
except ValueError:
    is_loopback = False
raise SystemExit(0 if is_loopback else 1)
PY
}

AGENT_CHAT_ECHO="${OASIS7_RUNTIME_AGENT_CHAT_ECHO:-}"
if [[ -z "$AGENT_CHAT_ECHO" ]]; then
  AGENT_CHAT_ECHO="0"
fi
AGENT_CHAT_ECHO_NORMALIZED="$(printf '%s' "$AGENT_CHAT_ECHO" | tr '[:upper:]' '[:lower:]')"
case "$AGENT_CHAT_ECHO_NORMALIZED" in
  1|true|yes|on)
    AGENT_CHAT_ECHO="1"
    ;;
  0|false|no|off)
    AGENT_CHAT_ECHO="0"
    ;;
  *)
    echo "error: OASIS7_RUNTIME_AGENT_CHAT_ECHO must be boolean-like; got $AGENT_CHAT_ECHO" >&2
    exit 1
    ;;
esac

if [[ -z "$CHAIN_LINK_POLICY" ]]; then
  if [[ "$CHAIN_LOCAL_STANDALONE_TEST" == "1" ]]; then
    CHAIN_LINK_POLICY="enforcing"
  elif [[ "$DEPLOYMENT_MODE" == "trusted_local_only" && "$ALLOW_TRUSTED_LOCAL_PLAYTEST" == "1" ]]; then
    CHAIN_LINK_POLICY="shadow"
  else
    CHAIN_LINK_POLICY="enforcing"
  fi
fi
case "$CHAIN_LINK_POLICY" in
  enforcing|shadow)
    ;;
  *)
    echo "error: --chain-link-policy must be one of enforcing, shadow; got $CHAIN_LINK_POLICY" >&2
    exit 1
    ;;
esac

case "$MAJOR_WORLD_EVENT_VISIBILITY" in
  unknown|public|restricted|denied)
    ;;
  *)
    echo "error: --major-world-event-visibility must be one of unknown|public|restricted|denied; got $MAJOR_WORLD_EVENT_VISIBILITY" >&2
    exit 1
    ;;
esac

LOCAL_TEST_PROVIDER_SETUP_ENABLED="0"
if [[ -n "$LOCAL_TEST_PROVIDER_AUTHORITY_PATH" || -n "$LOCAL_TEST_PROVIDER_WASM_PATH" || -n "$LOCAL_TEST_PROVIDER_METADATA_PATH" || -n "$LOCAL_TEST_PROVIDER_FINALITY_BLOCK_HASH" ]]; then
  LOCAL_TEST_PROVIDER_SETUP_ENABLED="1"
  for local_provider_path_name in LOCAL_TEST_PROVIDER_AUTHORITY_PATH LOCAL_TEST_PROVIDER_WASM_PATH LOCAL_TEST_PROVIDER_METADATA_PATH; do
    local_provider_path_value="${!local_provider_path_name}"
    if [[ -n "$local_provider_path_value" && "$local_provider_path_value" != /* ]]; then
      printf -v "$local_provider_path_name" '%s/%s' "$ROOT_DIR" "$local_provider_path_value"
    fi
  done
  if [[ "$CHAIN_ENABLED" != "1" ]]; then
    echo "error: local test authority setup requires --chain-enable" >&2
    exit 1
  fi
  if [[ "$CHAIN_LOCAL_STANDALONE_TEST" != "1" ]]; then
    echo "error: local test authority setup requires --chain-local-standalone-test" >&2
    exit 1
  fi
  if [[ "$AGENT_DECISION_SOURCE" == "builtin_llm" \
    && "$DEPLOYMENT_MODE" == "trusted_local_only" \
    && "$ALLOW_TRUSTED_LOCAL_PLAYTEST" == "1" ]]; then
    LOCAL_TEST_PROVIDER_ADMISSION="builtin_llm_dev_local"
  elif [[ "$DEPLOYMENT_MODE" == "hosted_public_join" \
    && "$AGENT_PROVIDER_LANE" == "local-mock" \
    && "$AGENT_DECISION_SOURCE" == "provider_backed" \
    && "$AGENT_PROVIDER_BACKEND" == "provider_local_mock" \
    && "$AGENT_PROVIDER_CONTRACT" == "worldsim_provider_v1" \
    && "$AGENT_PROVIDER_TRANSPORT" == "loopback_http" \
    && "$AGENT_EXECUTION_LANE" == "player_parity" \
    && "$LOCAL_TEST_PROVIDER_SESSION_MODE" == "hosted_public_join" \
    && "$CHAIN_ENABLED" == "1" \
    && "$CHAIN_LOCAL_STANDALONE_TEST" == "1" ]] \
    && provider_url_is_loopback "$AGENT_PROVIDER_URL"; then
    LOCAL_TEST_PROVIDER_ADMISSION="hosted_local_mock_test_tier"
  else
    echo "error: local test provider authority setup requires either builtin_llm DevLocal or the exact test-tier HostedPublicJoin provider_local_mock/worldsim_provider_v1/loopback_http/player_parity tuple" >&2
    exit 1
  fi
  if [[ -z "$LOCAL_TEST_PROVIDER_AUTHORITY_PATH" || -z "$LOCAL_TEST_PROVIDER_WASM_PATH" || -z "$LOCAL_TEST_PROVIDER_METADATA_PATH" || -z "$LOCAL_TEST_PROVIDER_FINALITY_BLOCK_HASH" ]]; then
    echo "error: local test authority setup requires authority, wasm, metadata, and finality marker paths" >&2
    exit 1
  fi
  if [[ ! -f "$LOCAL_TEST_PROVIDER_WASM_PATH" ]]; then
    echo "error: local test WASM artifact not found: $LOCAL_TEST_PROVIDER_WASM_PATH" >&2
    exit 1
  fi
  if [[ ! -f "$LOCAL_TEST_PROVIDER_METADATA_PATH" ]]; then
    echo "error: local test build metadata not found: $LOCAL_TEST_PROVIDER_METADATA_PATH" >&2
    exit 1
  fi
  if [[ ! "$LOCAL_TEST_PROVIDER_FINALITY_BLOCK_HASH" =~ ^blake3:[0-9a-f]{64}$ ]]; then
    echo "error: --local-test-provider-finality-block-hash must be blake3:<64 lowercase hex characters>" >&2
    exit 1
  fi
  case "$LOCAL_TEST_PROVIDER_SESSION_MODE" in
    hosted_public_join|loopback)
      ;;
    *)
      echo "error: --local-test-provider-session-mode must be hosted_public_join or loopback" >&2
      exit 1
      ;;
  esac
  if [[ -z "$LOCAL_TEST_PROVIDER_AGENT_ID" || -z "$LOCAL_TEST_PROVIDER_OWNER_BINDING" ]]; then
    echo "error: local test agent and owner binding must be non-empty" >&2
    exit 1
  fi
fi

if [[ "$PRINT_AGENT_PROVIDER_CONFIG" == "1" ]]; then
  python3 - <<PY
import json

payload = {
    "deployment_mode": "$DEPLOYMENT_MODE",
    "allow_trusted_local_playtest": "$ALLOW_TRUSTED_LOCAL_PLAYTEST",
    "auto_play": "$AUTO_PLAY",
    "agent_decision_source": "$AGENT_DECISION_SOURCE",
    "agent_provider_lane": "$AGENT_PROVIDER_LANE",
    "agent_provider_backend": "$AGENT_PROVIDER_BACKEND",
    "agent_provider_contract": "$AGENT_PROVIDER_CONTRACT",
    "agent_provider_transport": "$AGENT_PROVIDER_TRANSPORT",
    "agent_provider_url": "$AGENT_PROVIDER_URL",
    "agent_provider_connect_timeout_ms": "$AGENT_PROVIDER_CONNECT_TIMEOUT_MS",
    "agent_provider_profile": "$AGENT_PROVIDER_PROFILE",
    "agent_execution_lane": "$AGENT_EXECUTION_LANE",
    "provider_bootstrap_authority_count": "${#PROVIDER_BOOTSTRAP_AUTHORITY_PATHS[@]}",
    "local_test_provider_setup_enabled": "$LOCAL_TEST_PROVIDER_SETUP_ENABLED",
    "local_test_provider_admission": "$LOCAL_TEST_PROVIDER_ADMISSION",
    "local_test_provider_session_mode": "$LOCAL_TEST_PROVIDER_SESSION_MODE",
    "major_world_event_visibility": "$MAJOR_WORLD_EVENT_VISIBILITY",
    "chain_link_policy": "$CHAIN_LINK_POLICY",
    "agent_chat_echo": "$AGENT_CHAT_ECHO",
}
print(json.dumps(payload, ensure_ascii=False, sort_keys=True))
PY
  exit 0
fi

if [[ -n "$CHAIN_STATUS_BIND_ADDR" ]]; then
  if [[ "$CHAIN_STATUS_BIND_ADDR" != *:* ]]; then
    echo "error: --chain-status-bind must be in <host:port> format" >&2
    exit 1
  fi
  CHAIN_STATUS_BIND_HOST="${CHAIN_STATUS_BIND_ADDR%:*}"
  CHAIN_STATUS_BIND_PORT="${CHAIN_STATUS_BIND_ADDR##*:}"
  if [[ -z "$CHAIN_STATUS_BIND_HOST" || -z "$CHAIN_STATUS_BIND_PORT" ]]; then
    echo "error: invalid --chain-status-bind" >&2
    exit 1
  fi
  if ! [[ "$CHAIN_STATUS_BIND_PORT" =~ ^[0-9]+$ ]]; then
    echo "error: --chain-status-bind port must be numeric" >&2
    exit 1
  fi
else
  CHAIN_STATUS_BIND_HOST=""
  CHAIN_STATUS_BIND_PORT=""
fi

port_in_use() {
  local port="$1"
  if command -v lsof >/dev/null 2>&1; then
    lsof -iTCP:"$port" -sTCP:LISTEN -n -P >/dev/null 2>&1
    return $?
  fi

  if command -v ss >/dev/null 2>&1; then
    ss -ltn | grep -Eq "[:.]${port}[[:space:]]"
    return $?
  fi

  return 1
}

print_port_owner() {
  local port="$1"
  if command -v lsof >/dev/null 2>&1; then
    lsof -iTCP:"$port" -sTCP:LISTEN -n -P || true
  elif command -v ss >/dev/null 2>&1; then
    ss -ltnp | grep -E "[:.]${port}[[:space:]]" || true
  fi
}

check_port_free() {
  local port="$1"
  if port_in_use "$port"; then
    echo "error: port ${port} is already in use" >&2
    print_port_owner "$port" >&2
    exit 1
  fi
}

resolve_source_mode_target_dir() {
  oasis7_cargo_dev_debug_bin_dir "$ROOT_DIR"
}

ensure_launcher_alive() {
  local pid="${1:-}"
  local pgid="${2:-}"
  local identity="${3:-}"
  local refresh_identity refresh_base_identity
  refresh_base_identity="$identity"
  if [[ "$pid" == "$LAUNCHER_PID" && "$pgid" == "$LAUNCHER_PGID" && -n "$LAUNCHER_IDENTITY" ]]; then
    refresh_base_identity="$LAUNCHER_IDENTITY"
  fi
  wh_process_record_alive "$pid" "$pgid" "$refresh_base_identity" || return 1
  if [[ "$pid" == "$LAUNCHER_PID" && "$pgid" == "$LAUNCHER_PGID" ]]; then
    if refresh_identity="$(wh_process_group_refresh_identity "$pid" "$pgid" "$refresh_base_identity")"; then
      LAUNCHER_IDENTITY="$refresh_identity"
    fi
  fi
}

wait_for_http_ready() {
  local url="$1"
  local timeout_secs="$2"
  local launcher_pid="${3:-}"
  local launcher_pgid="${4:-}"
  local launcher_identity="${5:-}"
  local i
  for ((i = 0; i < timeout_secs; i++)); do
    if ! ensure_launcher_alive "$launcher_pid" "$launcher_pgid" "$launcher_identity"; then
      return 2
    fi
    if curl -fsS "$url" >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  return 1
}

wait_for_tcp_listener_ready() {
  local port="$1"
  local timeout_secs="$2"
  local launcher_pid="${3:-}"
  local launcher_pgid="${4:-}"
  local launcher_identity="${5:-}"
  local i
  if ! command -v lsof >/dev/null 2>&1 && ! command -v ss >/dev/null 2>&1; then
    echo "warning: neither lsof nor ss found; skip passive listener probe for port ${port}" >&2
    return 0
  fi
  for ((i = 0; i < timeout_secs; i++)); do
    if ! ensure_launcher_alive "$launcher_pid" "$launcher_pgid" "$launcher_identity"; then
      return 2
    fi
    if port_in_use "$port"; then
      return 0
    fi
    sleep 1
  done
  return 1
}

tail_logs_on_error() {
  echo "--- oasis7_viewer_live.log (tail) ---" >&2
  tail -n 80 "$WORLD_LOG" >&2 || true
  if [[ -s "$WEB_LOG" ]]; then
    echo "--- web_viewer.log (tail) ---" >&2
    tail -n 80 "$WEB_LOG" >&2 || true
  fi
}

tail_probe_logs_on_error() {
  local probe_json="$1"
  local probe_log="$2"
  if [[ -f "$probe_json" ]]; then
    echo "--- oasis7_llm_provider_probe.json ---" >&2
    cat "$probe_json" >&2 || true
  fi
  if [[ -s "$probe_log" ]]; then
    echo "--- oasis7_llm_provider_probe.log ---" >&2
    tail -n 80 "$probe_log" >&2 || true
  fi
}

provider_curl_timeout_seconds() {
  local timeout_ms="$1"
  python3 - "$timeout_ms" <<'PY'
import sys

raw = sys.argv[1].strip()
try:
    timeout_ms = int(raw)
except ValueError:
    raise SystemExit(f"invalid provider connect timeout `{raw}`")
if timeout_ms <= 0:
    raise SystemExit("provider connect timeout must be positive")
seconds = timeout_ms / 1000
print(f"{seconds:.3f}".rstrip("0").rstrip("."))
PY
}

run_provider_bridge_preflight() {
  local provider_url="${AGENT_PROVIDER_URL%/}"
  if [[ -z "$provider_url" ]]; then
    echo "provider bridge URL is required for provider_backed mode" >"$LLM_PROVIDER_PROBE_LOG"
    return 1
  fi

  local info_file health_file
  info_file="$OUTPUT_DIR/provider-info.json"
  health_file="$OUTPUT_DIR/provider-health.json"
  local curl_timeout_seconds
  if ! curl_timeout_seconds="$(provider_curl_timeout_seconds "$AGENT_PROVIDER_CONNECT_TIMEOUT_MS" 2>"$LLM_PROVIDER_PROBE_LOG")"; then
    return 1
  fi
  local curl_args=(-fsS --connect-timeout "$curl_timeout_seconds" --max-time "$curl_timeout_seconds")
  if [[ -n "$AGENT_PROVIDER_AUTH_TOKEN" ]]; then
    curl_args+=(-H "Authorization: Bearer $AGENT_PROVIDER_AUTH_TOKEN")
  fi

  if ! curl "${curl_args[@]}" "$provider_url/v1/provider/info" >"$info_file" 2>"$LLM_PROVIDER_PROBE_LOG"; then
    return 1
  fi
  if ! curl "${curl_args[@]}" "$provider_url/v1/provider/health" >"$health_file" 2>>"$LLM_PROVIDER_PROBE_LOG"; then
    return 1
  fi

  python3 - "$provider_url" "$AGENT_PROVIDER_TRANSPORT" "$info_file" "$health_file" >"$LLM_PROVIDER_PROBE_JSON" <<'PY'
import json
import sys
from pathlib import Path

provider_url, transport, info_path, health_path = sys.argv[1:5]
payload = {
    "status": "ok",
    "config_source": "provider_backed",
    "provider_url": provider_url,
    "provider_transport": transport,
    "provider_info": json.loads(Path(info_path).read_text()),
    "provider_health": json.loads(Path(health_path).read_text()),
}
print(json.dumps(payload, ensure_ascii=False))
PY
}

run_builtin_llm_preflight() {
  local probe_bin="${1:-}"
  if [[ -n "$probe_bin" ]]; then
    "$probe_bin" >"$LLM_PROVIDER_PROBE_JSON" 2>"$LLM_PROVIDER_PROBE_LOG"
  else
    (
      cd "$BUNDLE_DIR"
      "$ROOT_DIR/scripts/check-active-llm-provider.sh"
    ) >"$LLM_PROVIDER_PROBE_JSON" 2>"$LLM_PROVIDER_PROBE_LOG"
  fi
}

run_active_llm_preflight() {
  local probe_bin="${1:-}"
  case "$AGENT_DECISION_SOURCE" in
    provider_backed|provider_loopback_http|agent_direct_connect)
      run_provider_bridge_preflight
      ;;
    builtin_llm)
      run_builtin_llm_preflight "$probe_bin"
      ;;
    *)
      echo "unsupported agent decision source for preflight: $AGENT_DECISION_SOURCE" >"$LLM_PROVIDER_PROBE_LOG"
      return 1
      ;;
  esac
}

check_port_free "$VIEWER_PORT"
check_port_free "$WEB_BRIDGE_PORT"

if [[ -z "$RUN_ID" ]]; then
  RUN_ID="$(date +%Y%m%d-%H%M%S)"
fi
if [[ "$CHAIN_ENABLED" == "1" ]]; then
  if [[ -z "$CHAIN_STATUS_BIND_ADDR" ]]; then
    CHAIN_STATUS_BIND_PORT=$((WEB_BRIDGE_PORT + 110))
    if (( CHAIN_STATUS_BIND_PORT > 65535 )); then
      echo "error: derived --chain-status-bind port exceeds 65535" >&2
      exit 1
    fi
    CHAIN_STATUS_BIND_HOST="127.0.0.1"
    CHAIN_STATUS_BIND_ADDR="${CHAIN_STATUS_BIND_HOST}:${CHAIN_STATUS_BIND_PORT}"
  fi
  check_port_free "$CHAIN_STATUS_BIND_PORT"
  if [[ -z "$CHAIN_NODE_ID" ]]; then
    CHAIN_NODE_ID="viewer-live-node-playtest-${RUN_ID}"
  fi
fi
if [[ -n "$OUTPUT_DIR" ]]; then
  if [[ "$OUTPUT_DIR" != /* ]]; then
    OUTPUT_DIR="$ROOT_DIR/$OUTPUT_DIR"
  fi
else
  OUTPUT_DIR="$ROOT_DIR/output/playwright/playability/startup-${RUN_ID}"
fi
mkdir -p "$OUTPUT_DIR"

if [[ -n "$BUNDLE_DIR" ]]; then
  if [[ "$VIEWER_STATIC_DIR_EXPLICIT" == "1" ]]; then
    if [[ "$VIEWER_STATIC_DIR" == /* ]]; then
      RESOLVED_VIEWER_STATIC_DIR="$VIEWER_STATIC_DIR"
    else
      RESOLVED_VIEWER_STATIC_DIR="$ROOT_DIR/$VIEWER_STATIC_DIR"
    fi
  else
    RESOLVED_VIEWER_STATIC_DIR=""
  fi
else
  RESOLVED_VIEWER_STATIC_DIR=$(resolve_viewer_static_dir_for_web_closure "$ROOT_DIR" "$VIEWER_STATIC_DIR" "$OUTPUT_DIR")
fi

WORLD_LOG="$OUTPUT_DIR/oasis7_viewer_live.log"
WEB_LOG="$OUTPUT_DIR/web_viewer.log"
LLM_PROVIDER_PROBE_JSON="$OUTPUT_DIR/oasis7_llm_provider_probe.json"
LLM_PROVIDER_PROBE_LOG="$OUTPUT_DIR/oasis7_llm_provider_probe.log"
HOSTED_ACCOUNT_STORE_PATH="$OUTPUT_DIR/hosted-account-store.json"
HOSTED_PLAYER_SESSION_LEDGER_PATH="$OUTPUT_DIR/hosted-player-sessions.json"
HOSTED_REGISTRATION_REPLAY_LEDGER_PATH="$OUTPUT_DIR/hosted-registration-replay.json"
LAUNCHER_ENV_DEFAULTS=()
while IFS= read -r env_default; do
  LAUNCHER_ENV_DEFAULTS+=("$env_default")
done < <(oasis7_hosted_login_gate_env_defaults "$HOSTED_ACCOUNT_STORE_PATH")
LAUNCHER_ENV_DEFAULTS+=(
  "OASIS7_HOSTED_PLAYER_SESSION_LEDGER_PATH=$HOSTED_PLAYER_SESSION_LEDGER_PATH"
  "OASIS7_HOSTED_REGISTRATION_REPLAY_LEDGER_PATH=$HOSTED_REGISTRATION_REPLAY_LEDGER_PATH"
)
LAUNCHER_ENV_DEFAULTS+=("OASIS7_RUNTIME_AGENT_CHAT_ECHO=$AGENT_CHAT_ECHO")
if [[ -n "$META_FILE" ]]; then
  if [[ "$META_FILE" != /* ]]; then
    META_FILE="$ROOT_DIR/$META_FILE"
  fi
else
  META_FILE="$OUTPUT_DIR/session.meta"
fi
mkdir -p "$(dirname "$META_FILE")"

LAUNCHER_PID=""
LAUNCHER_PGID=""
LAUNCHER_IDENTITY=""

write_session_meta() {
  local stack_ready=$1
  local meta_payload
  meta_payload="$({
    printf 'RUN_ID=%s\n' "$RUN_ID"
    printf 'OUTPUT_DIR=%s\n' "$OUTPUT_DIR"
    printf 'WORLD_PID=%s\n' "$LAUNCHER_PID"
    printf 'WEB_PID=\n'
    printf 'LAUNCHER_PID=%s\n' "$LAUNCHER_PID"
    printf 'LAUNCHER_PGID=%s\n' "$LAUNCHER_PGID"
    printf 'LAUNCHER_IDENTITY=%s\n' "$LAUNCHER_IDENTITY"
    printf 'LIVE_BIND_ADDR=%s\n' "$LIVE_BIND_ADDR"
    printf 'WEB_BRIDGE_ADDR=%s\n' "$WEB_BRIDGE_ADDR"
    printf 'VIEWER_HOST=%s\n' "$VIEWER_HOST"
    printf 'VIEWER_PORT=%s\n' "$VIEWER_PORT"
    printf 'CHAIN_ENABLED=%s\n' "$CHAIN_ENABLED"
    printf 'DEPLOYMENT_MODE=%s\n' "$DEPLOYMENT_MODE"
    printf 'CHAIN_NODE_ID=%s\n' "$CHAIN_NODE_ID"
    printf 'CHAIN_STATUS_BIND_ADDR=%s\n' "$CHAIN_STATUS_BIND_ADDR"
    printf 'LAUNCH_MODE=%s\n' "$LAUNCH_MODE"
    printf 'LAUNCH_CMD=%s\n' "$LAUNCH_CMD"
    printf 'BUNDLE_DIR=%s\n' "$BUNDLE_DIR"
    printf 'LLM_PROVIDER_PREFLIGHT_SKIPPED=%s\n' "$SKIP_LLM_PROVIDER_PREFLIGHT"
    printf 'LLM_PROVIDER_PROBE_JSON=%s\n' "$LLM_PROVIDER_PROBE_JSON"
    printf 'LLM_PROVIDER_PROBE_LOG=%s\n' "$LLM_PROVIDER_PROBE_LOG"
    printf 'PROVIDER_LINEAGE_STORE_PATH=%s\n' "$OUTPUT_DIR/viewer-provider-lineage.json"
    printf 'PROVIDER_BOOTSTRAP_AUTHORITY_COUNT=%s\n' "${#PROVIDER_BOOTSTRAP_AUTHORITY_PATHS[@]}"
    printf 'LOCAL_TEST_PROVIDER_SETUP_ENABLED=%s\n' "$LOCAL_TEST_PROVIDER_SETUP_ENABLED"
    printf 'LOCAL_TEST_PROVIDER_ADMISSION=%s\n' "$LOCAL_TEST_PROVIDER_ADMISSION"
    printf 'LOCAL_TEST_PROVIDER_AUTHORITY_PATH=%s\n' "$LOCAL_TEST_PROVIDER_AUTHORITY_PATH"
    printf 'LOCAL_TEST_PROVIDER_SESSION_MODE=%s\n' "$LOCAL_TEST_PROVIDER_SESSION_MODE"
    printf 'HOSTED_ACCOUNT_STORE_PATH=%s\n' "$HOSTED_ACCOUNT_STORE_PATH"
    printf 'STACK_READY=%s\n' "$stack_ready"
    if [[ "$stack_ready" == "1" ]]; then
      printf 'GAME_URL=%s\n' "$GAME_URL"
      printf 'VIEWER_URL_ZH=%s\n' "$VIEWER_URL_ZH"
      printf 'VIEWER_URL_EN=%s\n' "$VIEWER_URL_EN"
      printf 'SOFTWARE_SAFE_VIEWER_URL_ZH=%s\n' "$SOFTWARE_SAFE_VIEWER_URL_ZH"
      printf 'SOFTWARE_SAFE_VIEWER_URL_EN=%s\n' "$SOFTWARE_SAFE_VIEWER_URL_EN"
    fi
  })"
  # Command substitution strips trailing newlines; keep session.meta a
  # complete line-oriented artifact for concurrent readers.
  meta_payload+=$'\n'
  wh_atomic_write "$META_FILE" "$meta_payload"
}

cleanup() {
  local exit_code=$?
  trap - EXIT INT TERM

  if [[ -n "$LAUNCHER_PID" ]]; then
    if ! wh_terminate_process_group "$LAUNCHER_PID" "$LAUNCHER_PGID" 2000 "$LAUNCHER_IDENTITY"; then
      echo "error: unable to prove launcher process-group quiescence" >&2
      exit_code=1
    fi
  fi

  exit "$exit_code"
}
trap cleanup EXIT INT TERM

WORLD_ARGS=(
  --deployment-mode "$DEPLOYMENT_MODE"
  --live-bind "$LIVE_BIND_ADDR"
  --web-bind "$WEB_BRIDGE_ADDR"
  --viewer-host "$VIEWER_HOST"
  --viewer-port "$VIEWER_PORT"
  --provider-lineage-store "$OUTPUT_DIR/viewer-provider-lineage.json"
  --major-world-event-visibility "$MAJOR_WORLD_EVENT_VISIBILITY"
  --no-open-browser
)
if [[ -n "$SCENARIO" ]]; then
  WORLD_ARGS+=(--scenario "$SCENARIO")
fi
if [[ "$ALLOW_DEBUG_SCENARIO" == "1" ]]; then
  WORLD_ARGS+=(--allow-debug-scenario)
fi
if [[ "$ALLOW_TRUSTED_LOCAL_PLAYTEST" == "1" ]]; then
  WORLD_ARGS+=(--allow-trusted-local-playtest)
fi
if [[ -n "$RESOLVED_VIEWER_STATIC_DIR" ]]; then
  WORLD_ARGS+=(--viewer-static-dir "$RESOLVED_VIEWER_STATIC_DIR")
fi
if [[ "$CHAIN_ENABLED" == "1" ]]; then
  WORLD_ARGS+=(
    --chain-enable
    --chain-node-id "$CHAIN_NODE_ID"
    --chain-status-bind "$CHAIN_STATUS_BIND_ADDR"
    --chain-link-policy "$CHAIN_LINK_POLICY"
  )
  if [[ "$CHAIN_LOCAL_STANDALONE_TEST" == "1" ]]; then
    WORLD_ARGS+=(--chain-local-standalone-test)
  fi
  if [[ "$CHAIN_NODE_AUTO_ATTEST_ALL" == "1" ]]; then
    WORLD_ARGS+=(--chain-node-auto-attest-all)
  else
    WORLD_ARGS+=(--chain-node-no-auto-attest-all)
  fi
  if [[ "${#CHAIN_NODE_VALIDATORS[@]}" -gt 0 ]]; then
    for validator in "${CHAIN_NODE_VALIDATORS[@]}"; do
      WORLD_ARGS+=(--chain-node-validator "$validator")
    done
  fi
else
  WORLD_ARGS+=(--chain-disable)
fi
if [[ -n "$CHAIN_EXECUTION_WORLD_DIR" ]]; then
  WORLD_ARGS+=(--chain-execution-world-dir "$CHAIN_EXECUTION_WORLD_DIR")
fi
WORLD_ARGS+=(--with-llm)
if ((${#PROVIDER_BOOTSTRAP_AUTHORITY_PATHS[@]} > 0)); then
  for authority_path in "${PROVIDER_BOOTSTRAP_AUTHORITY_PATHS[@]}"; do
    WORLD_ARGS+=(--provider-bootstrap-authority "$authority_path")
  done
fi
if [[ "$LOCAL_TEST_PROVIDER_SETUP_ENABLED" == "1" ]]; then
  WORLD_ARGS+=(
    --local-test-provider-authority "$LOCAL_TEST_PROVIDER_AUTHORITY_PATH"
    --local-test-provider-wasm "$LOCAL_TEST_PROVIDER_WASM_PATH"
    --local-test-provider-metadata "$LOCAL_TEST_PROVIDER_METADATA_PATH"
    --local-test-provider-agent-id "$LOCAL_TEST_PROVIDER_AGENT_ID"
    --local-test-provider-owner-binding "$LOCAL_TEST_PROVIDER_OWNER_BINDING"
    --local-test-provider-finality-block-hash "$LOCAL_TEST_PROVIDER_FINALITY_BLOCK_HASH"
    --local-test-provider-session-mode "$LOCAL_TEST_PROVIDER_SESSION_MODE"
  )
fi
if [[ "$AUTO_PLAY" == "1" ]]; then
  WORLD_ARGS+=(--auto-play)
else
  WORLD_ARGS+=(--no-auto-play)
fi
WORLD_ARGS+=(
  --agent-decision-source "$AGENT_DECISION_SOURCE"
)
if [[ "$AGENT_DECISION_SOURCE" != "builtin_llm" ]]; then
  WORLD_ARGS+=(
    --agent-provider-backend "$AGENT_PROVIDER_BACKEND"
    --agent-provider-contract "$AGENT_PROVIDER_CONTRACT"
    --agent-provider-transport "$AGENT_PROVIDER_TRANSPORT"
    --agent-provider-url "$AGENT_PROVIDER_URL"
    --agent-provider-connect-timeout-ms "$AGENT_PROVIDER_CONNECT_TIMEOUT_MS"
    --agent-provider-profile "$AGENT_PROVIDER_PROFILE"
    --agent-execution-lane "$AGENT_EXECUTION_LANE"
  )
  if [[ -n "$AGENT_PROVIDER_AUTH_TOKEN" ]]; then
    WORLD_ARGS+=(--agent-provider-auth-token "$AGENT_PROVIDER_AUTH_TOKEN")
  fi
fi

if [[ -n "$BUNDLE_DIR" ]]; then
  LAUNCH_MODE="bundle"
  LAUNCH_CMD="$BUNDLE_DIR/run-game.sh"
  if [[ "$SKIP_LLM_PROVIDER_PREFLIGHT" != "1" ]]; then
    if ! run_active_llm_preflight; then
      echo "error: active provider preflight failed before launcher startup" >&2
      echo "hint: provider_backed mode expects a reachable provider bridge; builtin_llm mode expects direct LLM env/config" >&2
      tail_probe_logs_on_error "$LLM_PROVIDER_PROBE_JSON" "$LLM_PROVIDER_PROBE_LOG"
      exit 1
    fi
  fi
  launch_bundle() {
    cd "$BUNDLE_DIR"
    env "${LAUNCHER_ENV_DEFAULTS[@]}" "$BUNDLE_DIR/run-game.sh" "${WORLD_ARGS[@]}" >"$WORLD_LOG" 2>&1
  }
  wh_start_managed launch_bundle
else
  LAUNCH_MODE="source"
  SOURCE_MODE_TARGET_DIR="$(resolve_source_mode_target_dir)"
  SOURCE_MODE_PROBE_BIN="$SOURCE_MODE_TARGET_DIR/oasis7_llm_provider_probe"
  SOURCE_MODE_LAUNCHER_BIN="$SOURCE_MODE_TARGET_DIR/oasis7_game_launcher"
  SOURCE_MODE_VIEWER_LIVE_BIN="$SOURCE_MODE_TARGET_DIR/oasis7_viewer_live"
  SOURCE_MODE_CHAIN_RUNTIME_BIN="$SOURCE_MODE_TARGET_DIR/oasis7_chain_runtime"
  SOURCE_BUILD_ARGS=(
    build
    -p oasis7_provider_tools
    -p oasis7_launcher_backend
    -p oasis7_viewer_runtime
    --bin
    oasis7_llm_provider_probe
    --bin
    oasis7_game_launcher
    --bin
    oasis7_viewer_live
  )
  if [[ "$CHAIN_ENABLED" == "1" ]]; then
    SOURCE_BUILD_ARGS+=(-p oasis7_chain_runtime --bin oasis7_chain_runtime)
  fi
  if [[ "$LOCAL_TEST_PROVIDER_ADMISSION" == "hosted_local_mock_test_tier" ]]; then
    SOURCE_BUILD_ARGS+=(--features test_tier_required)
  fi
  if [[ "${OASIS7_RUN_LAUNCHER_STACK_SKIP_SOURCE_BUILD:-0}" == "1" ]]; then
    echo "warning: skipping source-mode cargo build; using existing binaries from $SOURCE_MODE_TARGET_DIR" >&2
  else
    oasis7_cargo_dev "${SOURCE_BUILD_ARGS[@]}"
  fi
  [[ -x "$SOURCE_MODE_PROBE_BIN" ]] || { echo "error: built probe binary missing: $SOURCE_MODE_PROBE_BIN" >&2; exit 1; }
  [[ -x "$SOURCE_MODE_LAUNCHER_BIN" ]] || { echo "error: built launcher binary missing: $SOURCE_MODE_LAUNCHER_BIN" >&2; exit 1; }
  [[ -x "$SOURCE_MODE_VIEWER_LIVE_BIN" ]] || { echo "error: built viewer live binary missing: $SOURCE_MODE_VIEWER_LIVE_BIN" >&2; exit 1; }
  if [[ "$CHAIN_ENABLED" == "1" ]]; then
    [[ -x "$SOURCE_MODE_CHAIN_RUNTIME_BIN" ]] || { echo "error: built chain runtime binary missing: $SOURCE_MODE_CHAIN_RUNTIME_BIN" >&2; exit 1; }
  fi
  if [[ "$SKIP_LLM_PROVIDER_PREFLIGHT" != "1" ]]; then
    if ! run_active_llm_preflight "$SOURCE_MODE_PROBE_BIN"; then
      echo "error: active provider preflight failed before launcher startup" >&2
      echo "hint: provider_backed mode expects a reachable provider bridge; builtin_llm mode expects direct LLM env/config" >&2
      tail_probe_logs_on_error "$LLM_PROVIDER_PROBE_JSON" "$LLM_PROVIDER_PROBE_LOG"
      exit 1
    fi
  fi
  LAUNCH_CMD="$SOURCE_MODE_LAUNCHER_BIN"
  launch_source() {
    cd "$ROOT_DIR"
    env "${LAUNCHER_ENV_DEFAULTS[@]}" \
    OASIS7_VIEWER_LIVE_BIN="$SOURCE_MODE_VIEWER_LIVE_BIN" \
    OASIS7_CHAIN_RUNTIME_BIN="$SOURCE_MODE_CHAIN_RUNTIME_BIN" \
    "$SOURCE_MODE_LAUNCHER_BIN" "${WORLD_ARGS[@]}" >"$WORLD_LOG" 2>&1
  }
  wh_start_managed launch_source
fi
LAUNCHER_PID="$WH_MANAGED_PID"
LAUNCHER_PGID="$WH_MANAGED_PGID"
LAUNCHER_IDENTITY="$WH_MANAGED_IDENTITY"
cat <<'INFO' >"$WEB_LOG"
run-viewer-web.sh no longer runs as a standalone process in this stack.
web viewer is served by oasis7_game_launcher built-in static server.
INFO

write_session_meta 0

if ! wait_for_http_ready "http://${VIEWER_HOST}:${VIEWER_PORT}/" 180 "$LAUNCHER_PID" "$LAUNCHER_PGID" "$LAUNCHER_IDENTITY"; then
  if ensure_launcher_alive "$LAUNCHER_PID" "$LAUNCHER_PGID" "$LAUNCHER_IDENTITY"; then
    echo "error: viewer HTTP did not become ready in time" >&2
  else
    echo "error: launcher exited before viewer HTTP became ready" >&2
  fi
  tail_logs_on_error
  exit 1
fi

if ! wait_for_tcp_listener_ready "$WEB_BRIDGE_PORT" 60 "$LAUNCHER_PID" "$LAUNCHER_PGID" "$LAUNCHER_IDENTITY"; then
  if ensure_launcher_alive "$LAUNCHER_PID" "$LAUNCHER_PGID" "$LAUNCHER_IDENTITY"; then
    echo "error: web bridge port ${WEB_BRIDGE_PORT} did not become ready in time" >&2
  else
    echo "error: launcher exited before web bridge port ${WEB_BRIDGE_PORT} became ready" >&2
  fi
  tail_logs_on_error
  exit 1
fi

URL_VIEWER_HOST="$VIEWER_HOST"
if [[ "$URL_VIEWER_HOST" == "0.0.0.0" ]]; then
  URL_VIEWER_HOST="127.0.0.1"
fi
URL_WS_HOST="$WEB_BRIDGE_HOST"
if [[ "$URL_WS_HOST" == "0.0.0.0" ]]; then
  URL_WS_HOST="127.0.0.1"
fi

GAME_URL="http://${URL_VIEWER_HOST}:${VIEWER_PORT}/?ws=ws://${URL_WS_HOST}:${WEB_BRIDGE_PORT}&test_api=1&locale=zh"
VIEWER_URL_ZH="http://${URL_VIEWER_HOST}:${VIEWER_PORT}/?render_mode=viewer&ws=ws://${URL_WS_HOST}:${WEB_BRIDGE_PORT}&test_api=1&locale=zh"
VIEWER_URL_EN="http://${URL_VIEWER_HOST}:${VIEWER_PORT}/?render_mode=viewer&ws=ws://${URL_WS_HOST}:${WEB_BRIDGE_PORT}&test_api=1&locale=en"
SOFTWARE_SAFE_VIEWER_URL_ZH="http://${URL_VIEWER_HOST}:${VIEWER_PORT}/?render_mode=software_safe&ws=ws://${URL_WS_HOST}:${WEB_BRIDGE_PORT}&test_api=1&locale=zh"
SOFTWARE_SAFE_VIEWER_URL_EN="http://${URL_VIEWER_HOST}:${VIEWER_PORT}/?render_mode=software_safe&ws=ws://${URL_WS_HOST}:${WEB_BRIDGE_PORT}&test_api=1&locale=en"

write_session_meta 1

if [[ "$JSON_READY" == "1" ]]; then
  python3 - "$RUN_ID" "$OUTPUT_DIR" "$LAUNCHER_PID" "$LAUNCHER_PGID" "$LIVE_BIND_ADDR" "$WEB_BRIDGE_ADDR" "$VIEWER_HOST" "$VIEWER_PORT" "$CHAIN_ENABLED" "$CHAIN_NODE_ID" "$CHAIN_STATUS_BIND_ADDR" "$LAUNCH_MODE" "$LAUNCH_CMD" "$BUNDLE_DIR" "$GAME_URL" "$VIEWER_URL_ZH" "$VIEWER_URL_EN" "$SOFTWARE_SAFE_VIEWER_URL_ZH" "$SOFTWARE_SAFE_VIEWER_URL_EN" "$META_FILE" <<'PY'
from __future__ import annotations

import json
import sys

payload = {
    "run_id": sys.argv[1],
    "output_dir": sys.argv[2],
    "launcher_pid": int(sys.argv[3]),
    "launcher_pgid": int(sys.argv[4]),
    "live_bind_addr": sys.argv[5],
    "web_bridge_addr": sys.argv[6],
    "viewer_host": sys.argv[7],
    "viewer_port": int(sys.argv[8]),
    "chain_enabled": sys.argv[9] == "1",
    "chain_node_id": sys.argv[10],
    "chain_status_bind_addr": sys.argv[11],
    "launch_mode": sys.argv[12],
    "launch_cmd": sys.argv[13],
    "bundle_dir": sys.argv[14],
    "game_url": sys.argv[15],
    "viewer_url_zh": sys.argv[16],
    "viewer_url_en": sys.argv[17],
    "software_safe_compat_viewer_url_zh": sys.argv[18],
    "software_safe_compat_viewer_url_en": sys.argv[19],
    "meta_file": sys.argv[20],
}
print(json.dumps(payload, ensure_ascii=False))
PY
fi

cat <<INFO
Game test stack is ready.
- Mode: $LAUNCH_MODE
- Launcher: $LAUNCH_CMD
- Bundle dir: ${BUNDLE_DIR:-disabled}
- URL: $GAME_URL
- Viewer URL (zh): $VIEWER_URL_ZH
- Viewer URL (en): $VIEWER_URL_EN
- Software-safe compat URL (zh): $SOFTWARE_SAFE_VIEWER_URL_ZH
- Software-safe compat URL (en): $SOFTWARE_SAFE_VIEWER_URL_EN
- Logs: $OUTPUT_DIR
- Chain enabled: $CHAIN_ENABLED
- Chain node id: ${CHAIN_NODE_ID:-disabled}
- Chain status bind: ${CHAIN_STATUS_BIND_ADDR:-disabled}
- LLM provider preflight: $LLM_PROVIDER_PROBE_JSON

Recommended use:
- producer/release playtests: pass --bundle-dir <bundle>
- source mode remains for development/debug only

agent-browser example:
  AB_SESSION="\$(agent-browser session id --scope worktree --prefix launcher)"
  export AB_SESSION
  agent-browser --session "\$AB_SESSION" --headed open "\$GAME_URL"
  agent-browser --session "\$AB_SESSION" wait --load domcontentloaded
  agent-browser --session "\$AB_SESSION" wait --fn "typeof window.__AW_TEST__ === 'object'"
  agent-browser --session "\$AB_SESSION" session info --json
  agent-browser --session "\$AB_SESSION" close

Press Ctrl+C to stop launcher process.
INFO

while true; do
  if ! ensure_launcher_alive "$LAUNCHER_PID" "$LAUNCHER_PGID" "$LAUNCHER_IDENTITY"; then
    set +e
    wait "$LAUNCHER_PID"
    launcher_status=$?
    set -e
    echo "error: oasis7_game_launcher exited unexpectedly with status $launcher_status" >&2
    tail_logs_on_error
    exit "$launcher_status"
  fi
  sleep 1
done
