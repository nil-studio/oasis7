#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT_DIR/scripts/cargo-dev-lib.sh"

if [[ -n "${OASIS7_PUBLIC_TESTNET_NODE_BASE_URL:-}" ]]; then
  NODE_BASE_URL="$OASIS7_PUBLIC_TESTNET_NODE_BASE_URL"
  NODE_BASE_URL_EXPLICIT="1"
else
  NODE_BASE_URL="http://127.0.0.1:19083"
  NODE_BASE_URL_EXPLICIT="0"
fi
CHAIN_EXECUTION_WORLD_DIR="${OASIS7_CHAIN_EXECUTION_WORLD_DIR:-}"
CHAIN_SUBMIT_BIND="${OASIS7_PUBLIC_TESTNET_CHAIN_SUBMIT_BIND:-}"
CHAIN_SUBMIT_BASE_URL="${OASIS7_PUBLIC_TESTNET_CHAIN_SUBMIT_BASE_URL:-}"
MANIFEST_PATH="${OASIS7_TESTNET_MANIFEST:-}"
LETAI_CONFIG_PATH="${LETAI_TOKEN_FILE:-${OASIS7_LETAI_CONFIG_PATH:-}}"
LETAI_PLATFORM_ENV="${LETAI_PLATFORM_ENV:-}"
MERGED_LETAI_CONFIG="${OASIS7_LETAI_MERGED_CONFIG:-/tmp/oasis7-letai-merged-local-bridge.env}"
OUTPUT_DIR="${OASIS7_LOCAL_PUBLIC_TESTNET_OUTPUT_DIR:-$ROOT_DIR/output/local-public-testnet-letai}"
NEWAPI_BIND="${OASIS7_NEWAPI_BRIDGE_BIND_ADDR:-127.0.0.1:5852}"
NEWAPI_STATE_PATH="${OASIS7_NEWAPI_BRIDGE_STATE_PATH:-/tmp/oasis7-newapi-bridge-state.json}"
NEWAPI_LETAI_BASE_URL="${OASIS7_NEWAPI_BRIDGE_LETAI_BASE_URL:-https://api.letai.run}"
NEWAPI_ROUTE_TTL_SECONDS="${OASIS7_NEWAPI_BRIDGE_ROUTE_TTL_SECONDS:-900}"
NEWAPI_CONFIRMATIONS_REQUIRED="${OASIS7_NEWAPI_BRIDGE_CHAIN_CONFIRMATIONS_REQUIRED:-1}"
PRICING_RULES_FILE="${OASIS7_NEWAPI_BRIDGE_PRICING_RULES_FILE:-$ROOT_DIR/scripts/newapi-bridge-service/pricing-rules.example.env}"
PRICING_RULES="${OASIS7_NEWAPI_BRIDGE_PRICING_RULES:-}"
PROVIDER_BIND="${OASIS7_LOCAL_LETAI_PROVIDER_BIND:-127.0.0.1:5841}"
PROVIDER_AGENT="${OASIS7_LOCAL_LETAI_PROVIDER_AGENT:-letai-local-token-file}"
PROVIDER_MODEL="${OASIS7_LETAI_CHAT_MODEL:-gpt-5.4}"
PROVIDER_MAX_OUTPUT_TOKENS="${OASIS7_REMOTE_LLM_MAX_OUTPUT_TOKENS:-64}"
PROXY_URL="${OASIS7_LOCAL_TEST_PROXY_URL:-http://127.0.0.1:7897}"
SOCKS_PROXY_URL="${OASIS7_LOCAL_TEST_SOCKS_PROXY_URL:-socks5://127.0.0.1:7897}"
USE_DEFAULT_PROXY="1"
VIEWER_API_BIND="${OASIS7_VIEWER_LIVE_API_BIND:-127.0.0.1:5023}"
VIEWER_WS_BIND="${OASIS7_VIEWER_LIVE_WS_BIND:-127.0.0.1:5011}"
VIEWER_GENERATED_WORLD_DIR="${OASIS7_VIEWER_GENERATED_WORLD_DIR:-}"
STATIC_BIND="${OASIS7_VIEWER_STATIC_BIND:-127.0.0.1}"
STATIC_PORT="${OASIS7_VIEWER_STATIC_PORT:-4173}"
BUILD="1"
PREFLIGHT_ONLY="0"
REUSE_EXISTING="0"
SKIP_NEWAPI="0"
SKIP_PROVIDER="0"
SKIP_VIEWER_LIVE="0"
SKIP_STATIC_VIEWER="0"
RUN_VIEWER_FIRST_USER_SMOKE="${OASIS7_VIEWER_FIRST_USER_SMOKE:-1}"
PRINT_OPERATOR_COMMANDS="1"
STARTED_PIDS=()
STARTED_LABELS=()
SERVICES_READY="0"

usage() {
  cat <<'USAGE'
Usage: ./scripts/run-local-public-testnet-letai-test-environment.sh [options]

Start the local public_testnet + NewAPI quota bridge + LetAI provider bridge
test environment described by:
  doc/testing/manual/local-public-testnet-letai-test-environment-2026-06-23.manual.md

This script starts the environment services only. It never submits a signed OC
transfer automatically. For the OC -> NewAPI/LetAI recharge step, it prints the
operator command template so the operator can choose persona, nonce, amount,
and memo explicitly.

Options:
  --node-base-url <url>          public_testnet status/read node base URL (default: http://127.0.0.1:19083, or --chain-submit-base-url when only that is set)
  --public-testnet-base-url <url> Use one remote public_testnet node for both status/read and submit
  --chain-submit-bind <host:port> Submit-capable public_testnet endpoint for gameplay/transfer transactions
  --chain-submit-base-url <url>  HTTP submit-capable endpoint; converted to host:port for viewer live and transfer submit
  --chain-execution-world-dir <dir>
                                  Trusted local node execution root required by Viewer observer
  --manifest <path>              Formal public_testnet manifest path (or OASIS7_TESTNET_MANIFEST)
  --letai-config <path>          LetAI token/config file (or LETAI_TOKEN_FILE/OASIS7_LETAI_CONFIG_PATH)
  --letai-platform-env <path>    Optional env file with platform fields to merge into temp config
  --merged-letai-config <path>   Temp merged LetAI config path (default: /tmp/oasis7-letai-merged-local-bridge.env)
  --output-dir <path>            Logs and pid files directory
  --newapi-bind <host:port>      NewAPI quota bridge bind (default: 127.0.0.1:5852)
  --newapi-state-path <path>     NewAPI bridge state path (default: /tmp/oasis7-newapi-bridge-state.json)
  --pricing-rules-file <path>    Pricing rules env file
  --pricing-rules <rules>        Comma-separated pricing rules, e.g. pv-1:100:100000:0
  --provider-bind <host:port>    LetAI provider bridge bind (default: 127.0.0.1:5841)
  --provider-model <id>          LetAI model (default: gpt-5.4)
  --provider-agent <id>          Provider agent id
  --max-output-tokens <n>        Provider max output tokens (default: 64)
  --viewer-api-bind <host:port>  Viewer live API bind (default: 127.0.0.1:5023)
  --viewer-ws-bind <host:port>   Viewer live websocket bind (default: 127.0.0.1:5011)
  --viewer-generated-world-dir <dir>
                                  Generated-world root for viewer live sidecar map bootstrap
                                  Provider lineage checkpoint is stored at <output-dir>/viewer-provider-lineage.json
  --static-bind <host>           Static viewer bind host (default: 127.0.0.1)
  --static-port <port>           Static viewer port (default: 4173)
  --proxy <url>                  HTTP/HTTPS proxy exported for provider calls
  --socks-proxy <url>            all_proxy value exported for provider calls
  --no-default-proxy             Do not export proxy defaults
  --reuse-existing               Treat already-listening service ports as reusable
  --no-build                     Reuse existing debug binaries
  --preflight-only               Validate config/ports/status and print plan without starting services
  --skip-newapi-bridge           Do not start 127.0.0.1:5852 NewAPI quota bridge
  --skip-provider-bridge         Do not start 127.0.0.1:5841 LetAI provider bridge
  --skip-viewer-live             Do not start oasis7_viewer_live
  --skip-static-viewer           Do not start static viewer HTTP server
  --skip-viewer-first-user-smoke Do not run the viewer first-user smoke gate
  --no-operator-commands         Do not print bind/deposit/transfer/reconcile command templates
  -h, --help                     Show this help

Required for full OC -> NewAPI/LetAI recharge testing:
  OASIS7_NEWAPI_BRIDGE_LETAI_PLATFORM_KEY or LETAI_PLATFORM_KEY
  OASIS7_TEST_KEYS_FILE for the later operator-signed transfer command
USAGE
}

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

log() {
  printf '[local-public-testnet] %s\n' "$*" >&2
}

cleanup_started() {
  local exit_code=$?
  if [[ "$exit_code" -eq 0 || "$SERVICES_READY" == "1" ]]; then
    return 0
  fi
  local pid
  for pid in "${STARTED_PIDS[@]:-}"; do
    if [[ -n "$pid" ]] && kill -0 "$pid" >/dev/null 2>&1; then
      kill "$pid" >/dev/null 2>&1 || true
      wait "$pid" >/dev/null 2>&1 || true
    fi
  done
  local label
  for label in "${STARTED_LABELS[@]:-}"; do
    if [[ -n "$label" ]]; then
      launchctl remove "$label" >/dev/null 2>&1 || true
    fi
  done
}
trap cleanup_started EXIT

addr_port() {
  python3 - "$1" <<'PY'
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

validate_bind() {
  local bind="$1"
  local label="$2"
  local port
  port="$(addr_port "$bind" 2>/dev/null || true)"
  if [[ ! "$port" =~ ^[0-9]+$ || "$port" -lt 1 || "$port" -gt 65535 ]]; then
    die "$label must be in <host:port> format"
  fi
}

origin_from_url() {
  python3 - "$1" <<'PY'
import sys
from urllib.parse import urlparse

raw = sys.argv[1].strip()
parsed = urlparse(raw)
if parsed.scheme and parsed.netloc:
    print(f"{parsed.scheme}://{parsed.netloc}")
else:
    print(raw.rstrip("/"))
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

require_file() {
  local path="$1"
  local label="$2"
  [[ -f "$path" ]] || die "$label does not exist: $path"
}

require_command() {
  local command_name="$1"
  command -v "$command_name" >/dev/null 2>&1 || die "missing command: $command_name"
}

submit_service() {
  local label="$1"
  local stdout_path="$2"
  shift 2
  if command -v launchctl >/dev/null 2>&1; then
    launchctl remove "$label" >/dev/null 2>&1 || true
    launchctl submit \
      -l "$label" \
      -o "$stdout_path" \
      -e "$stdout_path.err" \
      -- "$@"
    STARTED_LABELS+=("$label")
  else
    nohup "$@" >"$stdout_path" 2>"$stdout_path.err" &
    local pid="$!"
    STARTED_PIDS+=("$pid")
    echo "$pid"
  fi
}

curl_json() {
  local url="$1"
  curl -fsS "$url"
}

wait_for_http() {
  local url="$1"
  local label="$2"
  local attempts="${3:-30}"
  local delay_s="${4:-1}"
  local index
  for ((index = 1; index <= attempts; index += 1)); do
    if curl -fsS "$url" >/dev/null 2>&1; then
      log "$label is reachable: $url"
      return 0
    fi
    sleep "$delay_s"
  done
  die "$label did not become reachable: $url"
}

wait_for_port() {
  local bind_or_port="$1"
  local label="$2"
  local attempts="${3:-30}"
  local delay_s="${4:-1}"
  local port="$bind_or_port"
  if [[ "$bind_or_port" == *:* ]]; then
    port="$(addr_port "$bind_or_port")" || die "invalid bind for $label: $bind_or_port"
  fi
  local index listeners
  for ((index = 1; index <= attempts; index += 1)); do
    listeners="$(port_listeners "$port" || true)"
    if [[ -n "$listeners" ]]; then
      log "$label is listening on port $port"
      return 0
    fi
    sleep "$delay_s"
  done
  die "$label did not start listening on port $port"
}

check_or_reuse_port() {
  local label="$1"
  local bind_or_port="$2"
  local port="$bind_or_port"
  if [[ "$bind_or_port" == *:* ]]; then
    port="$(addr_port "$bind_or_port")" || die "invalid bind for $label: $bind_or_port"
  fi
  local listeners
  listeners="$(port_listeners "$port" || true)"
  if [[ -n "$listeners" ]]; then
    if [[ "$REUSE_EXISTING" == "1" ]]; then
      log "$label already listening on port $port; reusing existing service"
      return 1
    fi
    printf '%s\n' "$listeners" >&2
    die "$label port is already in use: $port (pass --reuse-existing to reuse)"
  fi
  return 0
}

remove_launchd_label_if_present() {
  local label="$1"
  if ! command -v launchctl >/dev/null 2>&1; then
    return 0
  fi
  if launchctl list "$label" >/dev/null 2>&1; then
    log "stopping stale launchd service: $label"
    launchctl remove "$label" >/dev/null 2>&1 || true
  fi
}

stop_stale_viewer_live_services() {
  if [[ "$REUSE_EXISTING" == "1" ]]; then
    return 0
  fi
  remove_launchd_label_if_present "oasis7.local-public-testnet.viewer-live"
  remove_launchd_label_if_present "oasis7.local-public-testnet.viewer-live-clean"
}

load_pricing_rules() {
  if [[ -n "$PRICING_RULES" ]]; then
    printf '%s' "$PRICING_RULES"
    return 0
  fi
  require_file "$PRICING_RULES_FILE" "pricing rules file"
  sed -n 's/^OASIS7_NEWAPI_BRIDGE_PRICING_RULES="\([^"]*\)"$/\1/p' "$PRICING_RULES_FILE"
}

first_pricing_amount() {
  python3 - "$1" <<'PY'
import sys
rules = sys.argv[1].split(",")
for rule in rules:
    parts = rule.strip().split(":")
    if len(parts) >= 3 and parts[1].isdigit():
        print(parts[1])
        raise SystemExit(0)
raise SystemExit(1)
PY
}

node_status_bind() {
  python3 - "$NODE_BASE_URL" <<'PY'
import sys
from urllib.parse import urlparse

raw = sys.argv[1]
parsed = urlparse(raw)
if parsed.scheme and parsed.netloc:
    print(parsed.netloc)
else:
    print(raw.removeprefix("http://").removeprefix("https://"))
PY
}

base_url_to_bind() {
  python3 - "$1" <<'PY'
import sys
from urllib.parse import urlparse

raw = sys.argv[1].strip()
parsed = urlparse(raw)
if parsed.scheme == "https":
    raise SystemExit("https submit endpoints require a local HTTP relay or viewer TLS support")
if parsed.scheme and parsed.scheme != "http":
    raise SystemExit(f"unsupported submit endpoint scheme: {parsed.scheme}")
if parsed.scheme and parsed.netloc:
    host = parsed.hostname or ""
    port = parsed.port or 80
    if ":" in host and not host.startswith("["):
        host = f"[{host}]"
    print(f"{host}:{port}")
else:
    print(raw.removeprefix("http://"))
PY
}

chain_submit_bind() {
  if [[ -n "$CHAIN_SUBMIT_BIND" ]]; then
    printf '%s' "$CHAIN_SUBMIT_BIND"
    return 0
  fi
  if [[ -n "$CHAIN_SUBMIT_BASE_URL" ]]; then
    base_url_to_bind "$CHAIN_SUBMIT_BASE_URL"
    return $?
  fi
  if [[ -n "$MANIFEST_PATH" ]]; then
    local manifest_rpc_ref
    manifest_rpc_ref="$(python3 - "$MANIFEST_PATH" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
payload = json.loads(path.read_text())
rpc_ref = (payload.get("endpoint_policy") or {}).get("rpc_ref") or ""
print(rpc_ref)
PY
)"
    if [[ -n "$manifest_rpc_ref" ]]; then
      base_url_to_bind "$manifest_rpc_ref"
      return $?
    fi
  fi
  node_status_bind
}

chain_submit_base_url() {
  if [[ -n "$CHAIN_SUBMIT_BASE_URL" ]]; then
    origin_from_url "$CHAIN_SUBMIT_BASE_URL"
    return $?
  fi
  if [[ -n "$CHAIN_SUBMIT_BIND" ]]; then
    printf 'http://%s' "$CHAIN_SUBMIT_BIND"
    return 0
  fi
  if [[ -n "$MANIFEST_PATH" ]]; then
    local manifest_rpc_ref
    manifest_rpc_ref="$(python3 - "$MANIFEST_PATH" <<'PY'
import json
import sys
from pathlib import Path

payload = json.loads(Path(sys.argv[1]).read_text())
print((payload.get("endpoint_policy") or {}).get("rpc_ref") or "")
PY
)"
    if [[ -n "$manifest_rpc_ref" ]]; then
      origin_from_url "$manifest_rpc_ref"
      return $?
    fi
  fi
  origin_from_url "$NODE_BASE_URL"
}

chain_submit_source_label() {
  if [[ -n "$CHAIN_SUBMIT_BIND" ]]; then
    printf 'explicit bind'
  elif [[ -n "$CHAIN_SUBMIT_BASE_URL" ]]; then
    printf 'explicit base URL'
  elif [[ -n "$MANIFEST_PATH" ]]; then
    python3 - "$MANIFEST_PATH" <<'PY'
import json
import sys
from pathlib import Path

payload = json.loads(Path(sys.argv[1]).read_text())
if (payload.get("endpoint_policy") or {}).get("rpc_ref"):
    print("manifest endpoint_policy.rpc_ref")
else:
    print("local node fallback")
PY
  else
    printf 'local node fallback'
  fi
}

check_chain_submit_endpoint() {
  local submit_base_url="$1"
  local status_json
  local output
  local curl_error
  local attempt
  log "checking public_testnet submit endpoint: $submit_base_url"
  for attempt in 1 2 3 4 5; do
    curl_error=""
    if ! status_json="$(curl_json "$submit_base_url/v1/chain/status" 2>&1)"; then
      curl_error="$status_json"
      if [[ "$attempt" -eq 5 ]]; then
        printf '%s\n' "$curl_error" >&2
        return 1
      fi
      log "public_testnet submit endpoint not reachable yet (attempt $attempt/5); retrying"
      sleep 2
      continue
    fi
    if output="$(STATUS_JSON="$status_json" python3 - <<'PY' 2>&1
import json
import os

payload = json.loads(os.environ["STATUS_JSON"])
network = payload.get("network_tier") or {}
readiness = payload.get("readiness") or {}
consensus = payload.get("consensus") or {}
observability = payload.get("observability") or {}
world_resource = payload.get("world_resource")
role = (payload.get("role") or payload.get("node_role") or "").strip()
node_id = (payload.get("node_id") or "").strip()
world_id = payload.get("world_id")
chain_id = network.get("chain_id")
network_id = network.get("network_id")
failed = readiness.get("failed_gates") or []
world_resource_failed = []

errors = []
if network.get("tier") != "public_testnet":
    errors.append(f"tier={network.get('tier')!r}")
if not world_id or world_id != chain_id or world_id != network_id:
    errors.append(f"identity mismatch world_id={world_id!r} chain_id={chain_id!r} network_id={network_id!r}")
if readiness.get("status") != "ready":
    errors.append(f"readiness={readiness.get('status')!r}")
if failed:
    errors.append(f"failed_gates={failed!r}")
if isinstance(world_resource, dict):
    world_resource_failed = world_resource.get("failed_gates") or []
    if world_resource.get("readiness_status") != "ready":
        errors.append(f"world_resource_readiness={world_resource.get('readiness_status')!r}")
    if world_resource_failed:
        errors.append(f"world_resource_failed_gates={world_resource_failed!r}")
if role and role != "sequencer":
    errors.append(f"role={role!r} is not sequencer")
if not role and "sequencer" not in node_id:
    errors.append(f"role missing and node_id={node_id!r} is not clearly sequencer")

print(json.dumps({
    "node_id": node_id,
    "role": role or None,
    "world_id": world_id,
    "tier": network.get("tier"),
    "chain_id": chain_id,
    "network_id": network_id,
    "readiness": readiness.get("status"),
    "failed_gates": failed,
    "world_resource_readiness": world_resource.get("readiness_status") if isinstance(world_resource, dict) else None,
    "world_resource_failed_gates": world_resource_failed,
    "committed_height": consensus.get("committed_height"),
    "network_committed_height": consensus.get("network_committed_height"),
    "lag": observability.get("network_height_lag"),
}, ensure_ascii=True))

if errors:
    raise SystemExit("public_testnet submit endpoint is not ready/sequencer-capable: " + "; ".join(errors))
PY
    )"; then
      printf '%s\n' "$output"
      return 0
    fi
    if [[ "$attempt" -eq 5 ]]; then
      printf '%s\n' "$output" >&2
      return 1
    fi
    log "public_testnet submit endpoint not ready yet (attempt $attempt/5); retrying"
    sleep 2
  done
}

merge_letai_config_if_needed() {
  require_file "$LETAI_CONFIG_PATH" "LetAI config"
  if [[ -z "$LETAI_PLATFORM_ENV" ]]; then
    printf '%s' "$LETAI_CONFIG_PATH"
    return 0
  fi
  require_file "$LETAI_PLATFORM_ENV" "LetAI platform env"
  mkdir -p "$(dirname "$MERGED_LETAI_CONFIG")"
  LETAI_CONFIG_PATH="$LETAI_CONFIG_PATH" \
  LETAI_PLATFORM_ENV="$LETAI_PLATFORM_ENV" \
  MERGED_LETAI_CONFIG="$MERGED_LETAI_CONFIG" \
    python3 - <<'PY'
from pathlib import Path
import os
import sys

new = Path(os.environ["LETAI_CONFIG_PATH"])
old = Path(os.environ["LETAI_PLATFORM_ENV"])
merged = Path(os.environ["MERGED_LETAI_CONFIG"])
values = {}

for line in new.read_text(errors="replace").splitlines():
    raw = line.strip()
    if not raw or raw.startswith("#"):
        continue
    sep = "=" if "=" in raw else ":" if ":" in raw else None
    if not sep:
        continue
    key, value = raw.split(sep, 1)
    key = key.strip().lower()
    value = value.strip().strip('"').strip("'")
    if key == "key":
        values["token_key"] = value
    elif key in {"base_url", "token_key", "platform_key", "platform_user_id", "platform_project_id", "model"}:
        values[key] = value

for line in old.read_text(errors="replace").splitlines():
    raw = line.strip()
    if not raw or raw.startswith("#"):
        continue
    sep = "=" if "=" in raw else ":" if ":" in raw else None
    if not sep:
        continue
    key, value = raw.split(sep, 1)
    key = key.strip()
    value = value.strip().strip('"').strip("'")
    if key in {"platform_key", "platform_user_id", "platform_project_id"} and key not in values:
        values[key] = value

if "model" not in values:
    values["model"] = os.environ.get("OASIS7_LETAI_CHAT_MODEL", "gpt-5.4")

required = ["token_key", "base_url", "model"]
missing = [key for key in required if not values.get(key)]
if missing:
    raise SystemExit("missing LetAI config keys: " + ",".join(missing))

ordered = ["token_key", "base_url", "platform_key", "platform_user_id", "platform_project_id", "model"]
merged.write_text("".join(f"{key}={values[key]}\n" for key in ordered if values.get(key)))
merged.chmod(0o600)
print({"path": str(merged), "keys": [key for key in ordered if values.get(key)], "value_lengths": {key: len(values[key]) for key in ordered if values.get(key)}}, file=sys.stderr)
PY
  printf '%s' "$MERGED_LETAI_CONFIG"
}

check_public_testnet_node() {
  log "checking public_testnet node: $NODE_BASE_URL"
  local status_json
  local output
  local curl_error
  local attempt
  for attempt in 1 2 3 4 5; do
    curl_error=""
    if ! status_json="$(curl_json "$NODE_BASE_URL/v1/chain/status" 2>&1)"; then
      curl_error="$status_json"
      if [[ "$attempt" -eq 5 ]]; then
        printf '%s\n' "$curl_error" >&2
        return 1
      fi
      log "public_testnet node not reachable yet (attempt $attempt/5); retrying"
      sleep 2
      continue
    fi
    if output="$(STATUS_JSON="$status_json" python3 - <<'PY' 2>&1
import json
import os
import sys

payload = json.loads(os.environ["STATUS_JSON"])
network = payload.get("network_tier") or {}
readiness = payload.get("readiness") or {}
consensus = payload.get("consensus") or {}
observability = payload.get("observability") or {}
world_resource = payload.get("world_resource")
world_id = payload.get("world_id")
chain_id = network.get("chain_id")
network_id = network.get("network_id")
failed = readiness.get("failed_gates") or []
world_resource_failed = []

errors = []
if network.get("tier") != "public_testnet":
    errors.append(f"tier={network.get('tier')!r}")
if not world_id or world_id != chain_id or world_id != network_id:
    errors.append(f"identity mismatch world_id={world_id!r} chain_id={chain_id!r} network_id={network_id!r}")
if readiness.get("status") != "ready":
    errors.append(f"readiness={readiness.get('status')!r}")
if failed:
    errors.append(f"failed_gates={failed!r}")
if isinstance(world_resource, dict):
    world_resource_failed = world_resource.get("failed_gates") or []
    if world_resource.get("readiness_status") != "ready":
        errors.append(f"world_resource_readiness={world_resource.get('readiness_status')!r}")
    if world_resource_failed:
        errors.append(f"world_resource_failed_gates={world_resource_failed!r}")
if consensus.get("committed_height") != consensus.get("network_committed_height"):
    errors.append("committed height does not match network height")
if observability.get("network_height_lag") not in (0, None):
    errors.append(f"lag={observability.get('network_height_lag')!r}")

print(json.dumps({
    "world_id": world_id,
    "tier": network.get("tier"),
    "chain_id": chain_id,
    "network_id": network_id,
    "readiness": readiness.get("status"),
    "failed_gates": failed,
    "world_resource_readiness": world_resource.get("readiness_status") if isinstance(world_resource, dict) else None,
    "world_resource_failed_gates": world_resource_failed,
    "committed_height": consensus.get("committed_height"),
    "network_committed_height": consensus.get("network_committed_height"),
    "lag": observability.get("network_height_lag"),
    "peers": observability.get("connected_peer_count"),
}, ensure_ascii=True))

if errors:
    raise SystemExit("public_testnet node is not ready: " + "; ".join(errors))
PY
    )"; then
      printf '%s\n' "$output"
      return 0
    fi
    if [[ "$attempt" -eq 5 ]]; then
      printf '%s\n' "$output" >&2
      return 1
    fi
    log "public_testnet node not ready yet (attempt $attempt/5); retrying"
    sleep 2
  done
}

build_bins() {
  if [[ "$BUILD" != "1" ]]; then
    return 0
  fi
  log "building local binaries"
  "$ROOT_DIR/scripts/cargo-dev.sh" build \
    -p oasis7_protocol_tools -p oasis7_newapi_bridge_service -p oasis7_provider_local_bridge -p oasis7_viewer_runtime \
    --bin oasis7_newapi_bridge_service \
    --bin oasis7_chain_transfer_submit_client \
    --bin oasis7_provider_local_bridge \
    --bin oasis7_viewer_live
}

start_newapi_bridge() {
  if [[ "$SKIP_NEWAPI" == "1" ]]; then
    log "skipping NewAPI quota bridge"
    return 0
  fi
  if ! check_or_reuse_port "NewAPI quota bridge" "$NEWAPI_BIND"; then
    return 0
  fi
  local platform_key_env=""
  if [[ -n "${OASIS7_NEWAPI_BRIDGE_LETAI_PLATFORM_KEY:-}" ]]; then
    platform_key_env="OASIS7_NEWAPI_BRIDGE_LETAI_PLATFORM_KEY"
  elif [[ -n "${LETAI_PLATFORM_KEY:-}" ]]; then
    platform_key_env="LETAI_PLATFORM_KEY"
  fi
  [[ -n "$platform_key_env" ]] || die "OASIS7_NEWAPI_BRIDGE_LETAI_PLATFORM_KEY or LETAI_PLATFORM_KEY is required to start NewAPI bridge"
  local target_dir
  target_dir="$(oasis7_cargo_dev_target_dir "$ROOT_DIR")"
  local pricing
  pricing="$(load_pricing_rules)"
  [[ -n "$pricing" ]] || die "no pricing rules configured"
  mkdir -p "$(dirname "$NEWAPI_STATE_PATH")" "$OUTPUT_DIR"
  local log_path="$OUTPUT_DIR/newapi-bridge.log"
  IFS=',' read -r -a pricing_array <<< "$pricing"
  local cmd=(
    "$target_dir/debug/oasis7_newapi_bridge_service"
    --bind-addr "$NEWAPI_BIND"
    --state-path "$NEWAPI_STATE_PATH"
    --route-ttl-seconds "$NEWAPI_ROUTE_TTL_SECONDS"
    --deposit-account-prefix oc:bridge:
    --chain-base-url "$NODE_BASE_URL"
    --chain-confirmations-required "$NEWAPI_CONFIRMATIONS_REQUIRED"
    --letai-base-url "$NEWAPI_LETAI_BASE_URL"
    --letai-platform-key-env "$platform_key_env"
    --reconcile-interval-seconds 0
  )
  local rule
  for rule in "${pricing_array[@]}"; do
    rule="${rule//[[:space:]]/}"
    [[ -n "$rule" ]] && cmd+=(--pricing-rule "$rule")
  done
  log "starting NewAPI quota bridge on $NEWAPI_BIND; log=$log_path"
  submit_service "oasis7.local-public-testnet.newapi-bridge" "$log_path" "${cmd[@]}" \
    >"$OUTPUT_DIR/newapi-bridge.pid"
  wait_for_http "http://$NEWAPI_BIND/v1/bridge/health" "NewAPI quota bridge"
}

start_provider_bridge() {
  if [[ "$SKIP_PROVIDER" == "1" ]]; then
    log "skipping LetAI provider bridge"
    return 0
  fi
  if ! check_or_reuse_port "LetAI provider bridge" "$PROVIDER_BIND"; then
    return 0
  fi
  local config_path
  config_path="$(merge_letai_config_if_needed)"
  mkdir -p "$OUTPUT_DIR"
  local log_path="$OUTPUT_DIR/provider-bridge.log"
  local env_cmd=(
    env
    "OASIS7_REMOTE_LLM_NEWAPI_BRIDGE_STATE_PATH=$NEWAPI_STATE_PATH"
    "OASIS7_REMOTE_LLM_MAX_OUTPUT_TOKENS=$PROVIDER_MAX_OUTPUT_TOKENS"
  )
  if [[ "$USE_DEFAULT_PROXY" == "1" ]]; then
    env_cmd+=(
      "http_proxy=${http_proxy:-$PROXY_URL}"
      "https_proxy=${https_proxy:-$PROXY_URL}"
      "all_proxy=${all_proxy:-$SOCKS_PROXY_URL}"
    )
  fi
  log "starting LetAI provider bridge on $PROVIDER_BIND; log=$log_path"
  submit_service "oasis7.local-public-testnet.provider-bridge" "$log_path" \
    "${env_cmd[@]}" \
    "$ROOT_DIR/scripts/run-local-letai-provider-bridge.sh" \
      --config "$config_path" \
      --model "$PROVIDER_MODEL" \
      --bind "$PROVIDER_BIND" \
      --provider-agent "$PROVIDER_AGENT" \
    >"$OUTPUT_DIR/provider-bridge.pid"
  wait_for_http "http://$PROVIDER_BIND/v1/provider/info" "LetAI provider bridge"
}

start_viewer_live() {
  if [[ "$SKIP_VIEWER_LIVE" == "1" ]]; then
    log "skipping viewer live"
    return 0
  fi
  stop_stale_viewer_live_services
  if ! check_or_reuse_port "viewer live API" "$VIEWER_API_BIND"; then
    return 0
  fi
  if ! check_or_reuse_port "viewer live websocket" "$VIEWER_WS_BIND"; then
    return 0
  fi
  local target_dir
  target_dir="$(oasis7_cargo_dev_target_dir "$ROOT_DIR")"
  local chain_status_bind
  chain_status_bind="$(node_status_bind)"
  local chain_submit_bind_value
  chain_submit_bind_value="$(chain_submit_bind)" || die "invalid chain submit endpoint: ${CHAIN_SUBMIT_BASE_URL:-$CHAIN_SUBMIT_BIND}"
  if [[ "$(chain_submit_source_label)" == "local node fallback" ]]; then
    log "warning: no explicit submit endpoint and no manifest endpoint_policy.rpc_ref; gameplay submits will fall back to chain status bind $chain_status_bind"
    log "warning: if $chain_status_bind is an observer, player claims may queue locally instead of broadcasting into public_testnet consensus"
  else
    log "chain gameplay submit bind: $chain_submit_bind_value ($(chain_submit_source_label))"
  fi
  mkdir -p "$OUTPUT_DIR"
  local log_path="$OUTPUT_DIR/viewer-live.log"
  local provider_lineage_store="$OUTPUT_DIR/viewer-provider-lineage.json"
  local generated_world_args=()
  if [[ -n "$VIEWER_GENERATED_WORLD_DIR" ]]; then
    VIEWER_GENERATED_WORLD_DIR="$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).expanduser().resolve())' "$VIEWER_GENERATED_WORLD_DIR")"
    require_file "$VIEWER_GENERATED_WORLD_DIR/generated-scenario-world/snapshot.json" "viewer generated world sidecar snapshot"
    require_file "$VIEWER_GENERATED_WORLD_DIR/generated-scenario-world/journal.json" "viewer generated world sidecar journal"
    require_file "$VIEWER_GENERATED_WORLD_DIR/world-generation-provenance.json" "viewer generated world provenance"
    generated_world_args=(--generated-world-dir "$VIEWER_GENERATED_WORLD_DIR")
    log "viewer live generated world dir: $VIEWER_GENERATED_WORLD_DIR"
  fi
  log "viewer live provider lineage store: $provider_lineage_store"
  if [[ -z "$CHAIN_EXECUTION_WORLD_DIR" ]]; then
    die "Viewer observer requires --chain-execution-world-dir or OASIS7_CHAIN_EXECUTION_WORLD_DIR set to the node's local execution root"
  fi
  log "starting viewer live api=$VIEWER_API_BIND ws=$VIEWER_WS_BIND; log=$log_path"
  submit_service "oasis7.local-public-testnet.viewer-live" "$log_path" \
    env \
    OASIS7_AGENT_DECISION_SOURCE=provider_backed \
    OASIS7_AGENT_PROVIDER_BACKEND=provider_local_bridge \
    OASIS7_AGENT_PROVIDER_CONTRACT=worldsim_provider_v1 \
    OASIS7_AGENT_PROVIDER_TRANSPORT=loopback_http \
    OASIS7_AGENT_PROVIDER_URL="http://$PROVIDER_BIND" \
    OASIS7_AGENT_PROVIDER_CONNECT_TIMEOUT_MS=90000 \
    OASIS7_AGENT_PROVIDER_DECISION_TIMEOUT_MS=90000 \
    OASIS7_AGENT_PROVIDER_PROFILE=oasis7_p0_low_freq_npc \
    OASIS7_AGENT_EXECUTION_LANE=headless_agent \
    "$target_dir/debug/oasis7_viewer_live" \
      --bind "$VIEWER_API_BIND" \
      --web-bind "$VIEWER_WS_BIND" \
      --deployment-mode trusted_local_only \
      --chain-status-bind "$chain_status_bind" \
      --chain-execution-world-dir "$CHAIN_EXECUTION_WORLD_DIR" \
      --chain-submit-bind "$chain_submit_bind_value" \
      --chain-link-policy enforcing \
      --provider-lineage-store "$provider_lineage_store" \
      --no-auto-play \
      --llm \
      "${generated_world_args[@]}" \
    >"$OUTPUT_DIR/viewer-live.pid"
  wait_for_port "$VIEWER_API_BIND" "viewer live API"
  wait_for_port "$VIEWER_WS_BIND" "viewer live websocket"
}

start_static_viewer() {
  if [[ "$SKIP_STATIC_VIEWER" == "1" ]]; then
    log "skipping static viewer"
    return 0
  fi
  if ! check_or_reuse_port "static viewer" "$STATIC_PORT"; then
    return 0
  fi
  mkdir -p "$OUTPUT_DIR"
  local log_path="$OUTPUT_DIR/static-viewer.log"
  log "starting static viewer on $STATIC_BIND:$STATIC_PORT; log=$log_path"
  local static_dir="$ROOT_DIR/crates/oasis7_viewer/dist"
  if [[ ! -f "$static_dir/viewer.html" ]]; then
    die "viewer dist is missing: $static_dir/viewer.html (build crates/oasis7_viewer before starting this test environment)"
  fi
  if [[ ! -f "$static_dir/software_safe.html" ]]; then
    die "viewer dist is missing compatibility alias: $static_dir/software_safe.html (build crates/oasis7_viewer before starting this test environment)"
  fi
  log "serving viewer static directory: $static_dir"
  submit_service "oasis7.local-public-testnet.static-viewer" "$log_path" \
    python3 -m http.server "$STATIC_PORT" \
    --bind "$STATIC_BIND" \
    --directory "$static_dir" \
    >"$OUTPUT_DIR/static-viewer.pid"
  wait_for_http "http://$STATIC_BIND:$STATIC_PORT/viewer.html" "static viewer"
}

viewer_first_user_smoke_url() {
  printf 'http://%s:%s/viewer.html?ws=ws://%s&test_api=1&locale=zh\n' \
    "$STATIC_BIND" \
    "$STATIC_PORT" \
    "$VIEWER_WS_BIND"
}

run_viewer_first_user_smoke() {
  if [[ "$RUN_VIEWER_FIRST_USER_SMOKE" != "1" ]]; then
    log "skipping viewer first-user smoke"
    return 0
  fi
  if [[ "$SKIP_VIEWER_LIVE" == "1" || "$SKIP_STATIC_VIEWER" == "1" ]]; then
    log "skipping viewer first-user smoke because viewer live/static viewer is disabled"
    return 0
  fi
  local node_bin
  node_bin="${OASIS7_NODE_BIN:-}"
  if [[ -z "$node_bin" ]]; then
    node_bin="$(command -v node || true)"
  fi
  [[ -n "$node_bin" ]] || die "node is required for viewer first-user smoke; install node, set OASIS7_NODE_BIN, or pass --skip-viewer-first-user-smoke"

  local smoke_dir
  smoke_dir="$OUTPUT_DIR/viewer-first-user-smoke"
  mkdir -p "$smoke_dir"
  local smoke_url
  smoke_url="$(viewer_first_user_smoke_url)"
  log "running viewer first-user smoke: $smoke_url"
  "$node_bin" "$ROOT_DIR/crates/oasis7_viewer/scripts/viewer-first-user-smoke.mjs" \
    --url "$smoke_url" \
    --out-dir "$smoke_dir" \
    --timeout-ms 60000 \
    >"$OUTPUT_DIR/viewer-first-user-smoke.log" 2>&1 || {
      tail -n 120 "$OUTPUT_DIR/viewer-first-user-smoke.log" >&2 || true
      die "viewer first-user smoke failed; artifacts: $smoke_dir"
    }
  cat "$OUTPUT_DIR/viewer-first-user-smoke.log" >&2
}

print_summary() {
  local pricing
  pricing="$(load_pricing_rules || true)"
  local amount="<pricing-amount>"
  if [[ -n "$pricing" ]]; then
    amount="$(first_pricing_amount "$pricing" 2>/dev/null || printf '<pricing-amount>')"
  fi
  local viewer_url="http://$STATIC_BIND:$STATIC_PORT/viewer.html?ws=ws://$VIEWER_WS_BIND&test_api=1&locale=zh"
  local software_safe_compat_url="http://$STATIC_BIND:$STATIC_PORT/software_safe.html?ws=ws://$VIEWER_WS_BIND&test_api=1&locale=zh"
  local submit_base_url
  submit_base_url="$(chain_submit_base_url 2>/dev/null || printf '<invalid>')"
  cat <<EOF

Local public_testnet test environment plan:
  node: $NODE_BASE_URL
  chain_status_bind: $(node_status_bind)
  chain_submit_bind: $(chain_submit_bind 2>/dev/null || printf '<invalid>') ($(chain_submit_source_label 2>/dev/null || printf 'unknown source'))
  chain_submit_base_url: $submit_base_url
  newapi_bridge: http://$NEWAPI_BIND
  newapi_state_path: $NEWAPI_STATE_PATH
  provider_bridge: http://$PROVIDER_BIND
  viewer_live_api: http://$VIEWER_API_BIND
  viewer_generated_world_dir: ${VIEWER_GENERATED_WORLD_DIR:-none}
  viewer_url: $viewer_url
  viewer_software_safe_compat_url: $software_safe_compat_url
  logs: $OUTPUT_DIR

EOF
  if [[ "$PRINT_OPERATOR_COMMANDS" != "1" || "$SKIP_NEWAPI" == "1" ]]; then
    return 0
  fi
  cat <<EOF
Operator recharge commands are intentionally not executed by this script.
After selecting a test persona and nonce, run the bind/deposit/transfer/reconcile
flow from the runbook. The signed transfer shape is:

  source ./scripts/cargo-dev-lib.sh
  target_dir="\$(oasis7_cargo_dev_target_dir)"
  "\$target_dir/debug/oasis7_chain_transfer_submit_client" submit \\
    --keys-file "\$OASIS7_TEST_KEYS_FILE" \\
    --persona happy_path \\
    --to-account-id "\$TEST_DEPOSIT_ACCOUNT_ID" \\
    --amount $amount \\
    --nonce <operator-selected-next-nonce> \\
    --memo "\$TEST_DEPOSIT_TOKEN" \\
    --chain-base-url "$submit_base_url"

Then reconcile:

  curl -sS -X POST "http://$NEWAPI_BIND/v1/bridge/reconcile"

Provider smoke after reconcile:

  ./scripts/provider-remote-https/provider-bridge-contract-smoke.sh \\
    --base-url "http://$PROVIDER_BIND" \\
    --auth-token "newapi_user_ref:\$TEST_NEWAPI_USER_REF" \\
    --timeout-ms 90000 \\
    --legacy-compatibility-only \\
    --decision-count 1 \\
    --min-successes 1

EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --node-base-url) NODE_BASE_URL="${2:-}"; NODE_BASE_URL_EXPLICIT="1"; shift 2 ;;
    --public-testnet-base-url)
      NODE_BASE_URL="${2:-}"
      CHAIN_SUBMIT_BASE_URL="${2:-}"
      NODE_BASE_URL_EXPLICIT="1"
      shift 2
      ;;
    --chain-execution-world-dir)
      CHAIN_EXECUTION_WORLD_DIR="${2:-}"
      shift 2
      ;;
    --chain-submit-bind) CHAIN_SUBMIT_BIND="${2:-}"; shift 2 ;;
    --chain-submit-base-url) CHAIN_SUBMIT_BASE_URL="${2:-}"; shift 2 ;;
    --manifest) MANIFEST_PATH="${2:-}"; shift 2 ;;
    --letai-config) LETAI_CONFIG_PATH="${2:-}"; shift 2 ;;
    --letai-platform-env) LETAI_PLATFORM_ENV="${2:-}"; shift 2 ;;
    --merged-letai-config) MERGED_LETAI_CONFIG="${2:-}"; shift 2 ;;
    --output-dir) OUTPUT_DIR="${2:-}"; shift 2 ;;
    --newapi-bind) NEWAPI_BIND="${2:-}"; shift 2 ;;
    --newapi-state-path) NEWAPI_STATE_PATH="${2:-}"; shift 2 ;;
    --pricing-rules-file) PRICING_RULES_FILE="${2:-}"; shift 2 ;;
    --pricing-rules) PRICING_RULES="${2:-}"; shift 2 ;;
    --provider-bind) PROVIDER_BIND="${2:-}"; shift 2 ;;
    --provider-model) PROVIDER_MODEL="${2:-}"; shift 2 ;;
    --provider-agent) PROVIDER_AGENT="${2:-}"; shift 2 ;;
    --max-output-tokens) PROVIDER_MAX_OUTPUT_TOKENS="${2:-}"; shift 2 ;;
    --viewer-api-bind) VIEWER_API_BIND="${2:-}"; shift 2 ;;
    --viewer-ws-bind) VIEWER_WS_BIND="${2:-}"; shift 2 ;;
    --viewer-generated-world-dir) VIEWER_GENERATED_WORLD_DIR="${2:-}"; shift 2 ;;
    --static-bind) STATIC_BIND="${2:-}"; shift 2 ;;
    --static-port) STATIC_PORT="${2:-}"; shift 2 ;;
    --proxy) PROXY_URL="${2:-}"; shift 2 ;;
    --socks-proxy) SOCKS_PROXY_URL="${2:-}"; shift 2 ;;
    --no-default-proxy) USE_DEFAULT_PROXY="0"; shift ;;
    --reuse-existing) REUSE_EXISTING="1"; shift ;;
    --no-build) BUILD="0"; shift ;;
    --preflight-only) PREFLIGHT_ONLY="1"; shift ;;
    --skip-newapi-bridge) SKIP_NEWAPI="1"; shift ;;
    --skip-provider-bridge) SKIP_PROVIDER="1"; shift ;;
    --skip-viewer-live) SKIP_VIEWER_LIVE="1"; shift ;;
    --skip-static-viewer) SKIP_STATIC_VIEWER="1"; shift ;;
    --skip-viewer-first-user-smoke) RUN_VIEWER_FIRST_USER_SMOKE="0"; shift ;;
    --no-operator-commands) PRINT_OPERATOR_COMMANDS="0"; shift ;;
    -h|--help) usage; exit 0 ;;
    *) usage >&2; die "unknown option: $1" ;;
  esac
done

if [[ "$NODE_BASE_URL_EXPLICIT" != "1" ]]; then
  if [[ -n "$CHAIN_SUBMIT_BASE_URL" ]]; then
    NODE_BASE_URL="$(origin_from_url "$CHAIN_SUBMIT_BASE_URL")"
  elif [[ -n "$CHAIN_SUBMIT_BIND" ]]; then
    NODE_BASE_URL="http://$CHAIN_SUBMIT_BIND"
  fi
fi

[[ -n "$NODE_BASE_URL" ]] || die "--node-base-url cannot be empty"
if [[ -n "$CHAIN_SUBMIT_BIND" && -n "$CHAIN_SUBMIT_BASE_URL" ]]; then
  die "use either --chain-submit-bind or --chain-submit-base-url, not both"
fi
if [[ -n "$CHAIN_SUBMIT_BIND" ]]; then
  validate_bind "$CHAIN_SUBMIT_BIND" "--chain-submit-bind"
fi
if [[ -n "$CHAIN_SUBMIT_BASE_URL" ]]; then
  submit_bind_for_validation="$(chain_submit_bind 2>/dev/null)" || die "--chain-submit-base-url must be an HTTP URL or <host:port>; HTTPS requires a local HTTP relay or viewer TLS support"
  validate_bind "$submit_bind_for_validation" "--chain-submit-base-url"
fi
[[ -n "$OUTPUT_DIR" ]] || die "--output-dir cannot be empty"
[[ -n "$NEWAPI_BIND" ]] || die "--newapi-bind cannot be empty"
[[ -n "$PROVIDER_BIND" ]] || die "--provider-bind cannot be empty"
[[ "$STATIC_PORT" =~ ^[0-9]+$ ]] || die "--static-port must be numeric"

require_command curl
require_command nohup
require_command python3
if [[ -n "$MANIFEST_PATH" ]]; then
  require_file "$MANIFEST_PATH" "manifest"
fi
if [[ "$SKIP_VIEWER_LIVE" != "1" ]]; then
  resolved_chain_submit_bind="$(chain_submit_bind 2>/dev/null)" || die "chain submit endpoint must resolve to an HTTP <host:port>; HTTPS requires a local HTTP relay or viewer TLS support"
  validate_bind "$resolved_chain_submit_bind" "chain submit endpoint"
  resolved_chain_submit_base_url="$(chain_submit_base_url 2>/dev/null)" || die "chain submit endpoint must resolve to an HTTP base URL"
  check_chain_submit_endpoint "$resolved_chain_submit_base_url"
fi
if [[ "$SKIP_PROVIDER" != "1" ]]; then
  [[ -n "$LETAI_CONFIG_PATH" ]] || die "--letai-config or LETAI_TOKEN_FILE/OASIS7_LETAI_CONFIG_PATH is required"
  require_file "$LETAI_CONFIG_PATH" "LetAI config"
fi

mkdir -p "$OUTPUT_DIR"
check_public_testnet_node
if [[ "$PREFLIGHT_ONLY" != "1" && "$SKIP_VIEWER_LIVE" != "1" ]]; then
  stop_stale_viewer_live_services
fi
if [[ "$SKIP_NEWAPI" != "1" ]]; then
  check_or_reuse_port "NewAPI quota bridge" "$NEWAPI_BIND" >/dev/null || true
fi
if [[ "$SKIP_PROVIDER" != "1" ]]; then
  check_or_reuse_port "LetAI provider bridge" "$PROVIDER_BIND" >/dev/null || true
fi
if [[ "$SKIP_VIEWER_LIVE" != "1" ]]; then
  check_or_reuse_port "viewer live API" "$VIEWER_API_BIND" >/dev/null || true
  check_or_reuse_port "viewer live websocket" "$VIEWER_WS_BIND" >/dev/null || true
fi
if [[ "$SKIP_STATIC_VIEWER" != "1" ]]; then
  check_or_reuse_port "static viewer" "$STATIC_PORT" >/dev/null || true
fi

if [[ "$PREFLIGHT_ONLY" == "1" ]]; then
  print_summary
  exit 0
fi

build_bins
start_newapi_bridge
start_provider_bridge
start_viewer_live
start_static_viewer
run_viewer_first_user_smoke
SERVICES_READY="1"
print_summary
