#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CASE_ID="PWT-004"
OUT_DIR="output/playwright/prompt-control/${CASE_ID}"
HEADED=0
CONTRACT_ONLY=0
FULL_GAMEPLAY=0
HOSTED_LOCAL_MOCK=0
TEST_TIER_REQUIRED=0
TEST_SIGNER_SEED=""
SOURCE_BASE_REF="${OASIS7_VIEWER_PROVENANCE_BASE:-}"
SOURCE_HEAD=""
SOURCE_BASE=""
SOURCE_TREE_CLEAN=""
GAME_URL=""
AGENT_ID="starter-agent-0"
PROMPT_GOAL="Inspect the selected agent's prompt control state."
STARTUP_TIMEOUT=180
ACTION_TIMEOUT_MS=10000
VIEWPORT_WIDTH=""
VIEWPORT_HEIGHT=""
TEST_LOGIN=0
STACK_ARGS=()
STACK_BOOTSTRAPPED=0
STACK_CHAIN_ARG_EXPLICIT=0
FIRST_AGENT_CLAIM_PERFORMED=0

usage() {
  cat <<'EOF'
Usage: viewer-prompt-control-regression.sh --headed [options]

Runs the PWT-004 prompt-control flow in a headed browser.  --contract-only
produces the deterministic runner contract and manifest without launching a
browser, game process, or model provider.

Options:
  --headed | --headless
  --contract-only
  --full-gameplay         launch the trusted-local chain-backed gameplay lane
  --hosted-local-mock     launch the Hosted local-mock seeded-Agent lane
  --test-tier-required    build/launch oasis7 binaries with test_tier_required
  --test-signer-seed N    deterministic Hosted test signer fixture (currently 42)
  --source-base REF       immutable source comparison/base ref for binary provenance
  --case-id ID
  --out-dir DIR
  --url URL
  --test-login             use the visible local hosted test-login flow
  --agent-id ID
  --prompt-goal TEXT
  --startup-timeout SECONDS
  --action-timeout-ms MILLISECONDS
  --viewport-width PIXELS
  --viewport-height PIXELS
EOF
}

while (($# > 0)); do
  case "$1" in
    --headed) HEADED=1 ;;
    --headless) HEADED=0 ;;
    --contract-only) CONTRACT_ONLY=1 ;;
    --full-gameplay) FULL_GAMEPLAY=1 ;;
    --hosted-local-mock) HOSTED_LOCAL_MOCK=1 ;;
    --test-tier-required) TEST_TIER_REQUIRED=1 ;;
    --test-signer-seed) shift; TEST_SIGNER_SEED="${1:?missing value for --test-signer-seed}" ;;
    --source-base) shift; SOURCE_BASE_REF="${1:?missing value for --source-base}" ;;
    --case-id) shift; CASE_ID="${1:?missing value for --case-id}" ;;
    --out-dir) shift; OUT_DIR="${1:?missing value for --out-dir}" ;;
    --url) shift; GAME_URL="${1:?missing value for --url}" ;;
    --test-login) TEST_LOGIN=1 ;;
    --agent-id) shift; AGENT_ID="${1:?missing value for --agent-id}" ;;
    --prompt-goal) shift; PROMPT_GOAL="${1:?missing value for --prompt-goal}" ;;
    --startup-timeout) shift; STARTUP_TIMEOUT="${1:?missing value for --startup-timeout}" ;;
    --action-timeout-ms) shift; ACTION_TIMEOUT_MS="${1:?missing value for --action-timeout-ms}" ;;
    --viewport-width) shift; VIEWPORT_WIDTH="${1:?missing value for --viewport-width}" ;;
    --viewport-height) shift; VIEWPORT_HEIGHT="${1:?missing value for --viewport-height}" ;;
    -h|--help) usage; exit 0 ;;
    *) STACK_ARGS+=("$1") ;;
  esac
  shift
done

if (( HOSTED_LOCAL_MOCK == 1 )); then
  if (( TEST_TIER_REQUIRED == 0 )); then
    echo "error: --hosted-local-mock requires explicit --test-tier-required" >&2
    exit 2
  fi
  if (( FULL_GAMEPLAY == 1 )); then
    echo "error: --hosted-local-mock cannot be combined with --full-gameplay" >&2
    exit 2
  fi
  if [[ -z "$TEST_SIGNER_SEED" ]]; then
    TEST_SIGNER_SEED="42"
  fi
fi

if [[ -n "$TEST_SIGNER_SEED" && "$TEST_SIGNER_SEED" != "42" ]]; then
  echo "error: --test-signer-seed currently supports only the deterministic seed-42 fixture" >&2
  exit 2
fi
if [[ -n "$TEST_SIGNER_SEED" && "$HOSTED_LOCAL_MOCK" != "1" ]]; then
  echo "error: --test-signer-seed requires --hosted-local-mock" >&2
  exit 2
fi

if ((${#STACK_ARGS[@]} > 0)); then
  for stack_arg in "${STACK_ARGS[@]}"; do
    # An explicit chain argument from the caller is authoritative.
    if [[ "$stack_arg" == --chain-* ]]; then
      STACK_CHAIN_ARG_EXPLICIT=1
      break
    fi
  done
fi

if (( HEADED == 0 )); then
  echo "error: --headed is required for PWT-004; headed evidence is mandatory" >&2
  exit 2
fi
if [[ "$CASE_ID" != "PWT-004" ]]; then
  echo "error: this runner currently supports only --case-id PWT-004" >&2
  exit 2
fi
if [[ ! "$AGENT_ID" =~ ^[A-Za-z0-9_.:-]+$ ]]; then
  echo "error: --agent-id contains unsupported characters" >&2
  exit 2
fi
if ! [[ "$STARTUP_TIMEOUT" =~ ^[1-9][0-9]*$ && "$ACTION_TIMEOUT_MS" =~ ^[1-9][0-9]*$ ]]; then
  echo "error: timeouts must be positive integers" >&2
  exit 2
fi
if [[ -n "$VIEWPORT_WIDTH" || -n "$VIEWPORT_HEIGHT" ]]; then
  if ! [[ "$VIEWPORT_WIDTH" =~ ^[1-9][0-9]*$ && "$VIEWPORT_HEIGHT" =~ ^[1-9][0-9]*$ ]]; then
    echo "error: --viewport-width and --viewport-height must be provided together as positive integers" >&2
    exit 2
  fi
fi

LOCAL_PROVIDER_AUTHORITY="$OUT_DIR/runtime/local-test-provider-authority.json"
LOCAL_PROVIDER_DIR="$ROOT_DIR/.tmp/wasm-build-suite/local-test-provider"
LOCAL_PROVIDER_WASM="$LOCAL_PROVIDER_DIR/module.runtime.local-test-provider.wasm"
LOCAL_PROVIDER_METADATA="$LOCAL_PROVIDER_DIR/module.runtime.local-test-provider.metadata.json"

if (( CONTRACT_ONLY == 0 )); then
  for stale_artifact in binary-provenance.json artifact-manifest.json run-summary.json; do
    if [[ -e "$OUT_DIR/$stale_artifact" ]]; then
      echo "error: refusing to reuse existing provenance output: $OUT_DIR/$stale_artifact" >&2
      exit 2
    fi
  done
fi

if (( CONTRACT_ONLY == 0 && (FULL_GAMEPLAY == 1 || HOSTED_LOCAL_MOCK == 1) )); then
  if [[ ! -f "$LOCAL_PROVIDER_WASM" ]]; then
    echo "error: local test provider artifact WASM is missing: $LOCAL_PROVIDER_WASM" >&2
    exit 2
  fi
  if [[ ! -f "$LOCAL_PROVIDER_METADATA" ]]; then
    echo "error: local test provider artifact metadata is missing: $LOCAL_PROVIDER_METADATA" >&2
    exit 2
  fi
fi

AGENT_SELECTOR="[data-pixel-world-agent-marker=\"true\"][data-agent-id=\"${AGENT_ID}\"]"

capture_source_provenance() {
  local default_base_ref
  local dirty_tree
  SOURCE_HEAD="$(git -C "$ROOT_DIR" rev-parse --verify 'HEAD^{commit}' 2>/dev/null || true)"
  if [[ -z "$SOURCE_HEAD" ]]; then
    echo "error: cannot resolve exact source HEAD for binary provenance" >&2
    return 1
  fi
  if [[ -z "$SOURCE_BASE_REF" ]]; then
    default_base_ref="${OASIS7_VIEWER_PROVENANCE_BASE_REF:-main}"
    SOURCE_BASE_REF="$default_base_ref"
  fi
  SOURCE_BASE="$(git -C "$ROOT_DIR" rev-parse --verify "${SOURCE_BASE_REF}^{commit}" 2>/dev/null || true)"
  if [[ -z "$SOURCE_BASE" ]]; then
    echo "error: cannot resolve exact source base for binary provenance: $SOURCE_BASE_REF" >&2
    return 1
  fi
  if ! git -C "$ROOT_DIR" merge-base --is-ancestor "$SOURCE_BASE" "$SOURCE_HEAD" >/dev/null 2>&1; then
    echo "error: source base is not an ancestor of source HEAD; refusing binary provenance" >&2
    return 1
  fi
  dirty_tree="$(git -C "$ROOT_DIR" status --porcelain --untracked-files=all 2>/dev/null || true)"
  if [[ -n "$dirty_tree" ]]; then
    echo "error: source tree is dirty; refusing exact-head binary provenance" >&2
    return 1
  fi
  SOURCE_TREE_CLEAN="1"
}

write_source_build_command() {
  local command_path="$1"
  shift
  python3 - "$command_path" "$@" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
path.write_text(json.dumps({"argv": sys.argv[2:]}, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
}

write_binary_provenance() {
  local target_dir="$1"
  local build_log="$2"
  local build_command_path="$3"
  local build_started="$4"
  local build_finished="$5"
  local rustc_release="$6"
  local cargo_version="$7"
  local rustup_toolchain="$8"
  local host_triple="$9"
  shift 9
  python3 - "$OUT_DIR/binary-provenance.json" "$target_dir" "$build_log" "$build_command_path" "$build_started" "$build_finished" "$SOURCE_HEAD" "$SOURCE_BASE" "$SOURCE_BASE_REF" "$SOURCE_TREE_CLEAN" "$rustc_release" "$cargo_version" "$rustup_toolchain" "$host_triple" "$@" <<'PY'
import hashlib
import json
import pathlib
import sys

output = pathlib.Path(sys.argv[1])
target_dir = pathlib.Path(sys.argv[2]).resolve()
build_log = pathlib.Path(sys.argv[3]).resolve()
build_command_path = pathlib.Path(sys.argv[4]).resolve()
build_started, build_finished = sys.argv[5], sys.argv[6]
source_head, source_base, source_base_ref = sys.argv[7], sys.argv[8], sys.argv[9]
source_tree_clean = sys.argv[10] == "1"
rustc_release, cargo_version, rustup_toolchain, host_triple = sys.argv[11:15]
binary_names = sys.argv[15:]

if not source_tree_clean or not source_head or not source_base:
    raise SystemExit("binary provenance requires a clean exact source identity")
if not build_log.is_file() or not build_command_path.is_file():
    raise SystemExit("binary provenance is missing the build log or command record")
command_record = json.loads(build_command_path.read_text(encoding="utf-8"))
build_log_bytes = build_log.read_bytes()
build_command = command_record.get("argv")
if not isinstance(build_command, list) or not build_command:
    raise SystemExit("binary provenance build command is invalid")

binaries = {}
for name in binary_names:
    path = target_dir / name
    if not path.is_file() or not path.stat().st_mode & 0o111:
        raise SystemExit(f"binary provenance missing executable: {path}")
    data = path.read_bytes()
    binaries[name] = {
        "path": str(path),
        "sha256": hashlib.sha256(data).hexdigest(),
        "sizeBytes": len(data),
        "usedBySourceMode": True,
        "launched": name != "oasis7_llm_provider_probe",
    }

output.write_text(
    json.dumps(
        {
            "schema": "oasis7.viewer.binary-provenance/v1",
            "status": "verified",
            "source": {
                "head": source_head,
                "base": source_base,
                "baseRef": source_base_ref,
                "treeClean": source_tree_clean,
            },
            "build": {
                "command": build_command,
                "commandRecord": str(build_command_path),
                "features": ["test_tier_required"],
                "profile": "debug",
                "targetDir": str(target_dir),
                "startedAt": build_started,
                "finishedAt": build_finished,
                "log": str(build_log),
                "logSha256": hashlib.sha256(build_log_bytes).hexdigest(),
                "toolchain": {
                    "rustcVersion": rustc_release,
                    "cargoVersion": cargo_version,
                    "activeToolchain": rustup_toolchain,
                    "hostTriple": host_triple,
                },
            },
            "freshness": {
                "outputRootWasFresh": True,
                "targetDirMatchesLauncher": True,
                "reusePolicy": "cargo build validated the clean source identity; pre-existing evidence output is rejected",
            },
            "binaries": binaries,
            "launchedBinaries": [name for name, item in binaries.items() if item["launched"]],
            "notLaunchedBinaries": [name for name, item in binaries.items() if not item["launched"]],
        },
        indent=2,
        sort_keys=True,
    )
    + "\n",
    encoding="utf-8",
)
PY
}

write_runner_config() {
  mkdir -p "$OUT_DIR"
  python3 - "$OUT_DIR/runner-config.json" "$TEST_TIER_REQUIRED" "$HOSTED_LOCAL_MOCK" "$TEST_SIGNER_SEED" "$AGENT_ID" "$VIEWPORT_WIDTH" "$VIEWPORT_HEIGHT" "$SOURCE_HEAD" "$SOURCE_BASE" "$SOURCE_BASE_REF" "$SOURCE_TREE_CLEAN" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
test_tier_required = sys.argv[2] == "1"
hosted_local_mock = sys.argv[3] == "1"
viewport_width = int(sys.argv[6]) if sys.argv[6] else None
viewport_height = int(sys.argv[7]) if sys.argv[7] else None
source_head = sys.argv[8] or None
source_base = sys.argv[9] or None
source_base_ref = sys.argv[10] or None
source_tree_clean = sys.argv[11] == "1" if sys.argv[11] else None
path.write_text(
    json.dumps(
        {
            "agentId": sys.argv[5],
            "browserMode": "headed",
            "source": {
                "head": source_head,
                "base": source_base,
                "baseRef": source_base_ref,
                "treeClean": source_tree_clean,
            },
            "provenance": {
                "path": "binary-provenance.json" if source_head else None,
                "status": "verified" if source_head else None,
            },
            "deploymentMode": "hosted_public_join" if hosted_local_mock else "caller_or_default",
            "evidenceBoundary": {
                "providerCallsDuringVerification": False,
                "realInference": False,
                "worldConsequenceClaim": False,
            },
            "launchRoute": "hosted_local_mock_seeded_agent" if hosted_local_mock else "legacy_prompt_control",
            "chain": {
                "autoPlay": False,
                "enabled": hosted_local_mock,
                "standaloneTest": hosted_local_mock,
                "storageProfile": "dev_local" if hosted_local_mock else None,
            },
            "localAuthority": {
                "authorityArtifact": "runtime/local-test-provider-authority.json" if hosted_local_mock else None,
                "finalityBlockHash": "blake3:" + "0" * 64 if hosted_local_mock else None,
                "ownerBinding": "local-test-owner-0" if hosted_local_mock else None,
                "sessionMode": "hosted_public_join" if hosted_local_mock else None,
                "wasmArtifact": ".tmp/wasm-build-suite/local-test-provider/module.runtime.local-test-provider.wasm" if hosted_local_mock else None,
                "metadataArtifact": ".tmp/wasm-build-suite/local-test-provider/module.runtime.local-test-provider.metadata.json" if hosted_local_mock else None,
            },
            "provider": {
                "backend": "provider_local_mock" if hosted_local_mock else None,
                "contract": "worldsim_provider_v1" if hosted_local_mock else None,
                "executionLane": "player_parity" if hosted_local_mock else None,
                "transport": "loopback_http" if hosted_local_mock else None,
                "url": "http://127.0.0.1:5841" if hosted_local_mock else None,
            },
            "seededAgent": hosted_local_mock,
            "testSigner": {
                "seed": int(sys.argv[4]) if sys.argv[4] else None,
                "privateKeyRecorded": False,
            },
            "testTierRequired": test_tier_required,
            "viewport": {
                "requestedHeight": viewport_height,
                "requestedWidth": viewport_width,
            },
        },
        indent=2,
        sort_keys=True,
    )
    + "\n",
    encoding="utf-8",
)
PY
}

build_test_tier_binaries() {
  source "$ROOT_DIR/scripts/cargo-dev-lib.sh"
  local build_log="$OUT_DIR/test-tier-required-build.log"
  local build_command="$OUT_DIR/test-tier-required-build.command"
  local build_started
  local build_finished
  local target_dir
  local rustc_verbose
  local rustc_release
  local cargo_version
  local rustup_toolchain
  local host_triple
  local -a build_args=(
    build
    -p oasis7_provider_tools
    -p oasis7_launcher_backend
    -p oasis7_viewer_runtime
    --features test_tier_required
    --locked
    --bin oasis7_llm_provider_probe
    --bin oasis7_game_launcher
    --bin oasis7_viewer_live
  )
  if (( FULL_GAMEPLAY == 1 || HOSTED_LOCAL_MOCK == 1 )); then
    build_args+=(-p oasis7_chain_runtime --bin oasis7_chain_runtime)
  fi

  build_started="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  printf 'oasis7_cargo_dev' >"$build_command"
  printf ' %q' "${build_args[@]}" >>"$build_command"
  printf '\n' >>"$build_command"
  write_source_build_command "$OUT_DIR/test-tier-required-build.json" oasis7_cargo_dev "${build_args[@]}"
  if ! oasis7_cargo_dev "${build_args[@]}" >"$build_log" 2>&1; then
    echo "error: test_tier_required launcher/viewer build failed (log: $build_log)" >&2
    return 1
  fi
  build_finished="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

  target_dir="$(oasis7_cargo_dev_debug_bin_dir "$ROOT_DIR")"
  local -a required_binaries=(
    oasis7_llm_provider_probe
    oasis7_game_launcher
    oasis7_viewer_live
  )
  if (( FULL_GAMEPLAY == 1 || HOSTED_LOCAL_MOCK == 1 )); then
    required_binaries+=(oasis7_chain_runtime)
  fi
  for binary in "${required_binaries[@]}"; do
    if [[ ! -x "$target_dir/$binary" ]]; then
      echo "error: test_tier_required binary missing after build: $target_dir/$binary" >&2
      return 1
    fi
  done
  rustc_verbose="$(rustc -vV 2>/dev/null || true)"
  rustc_release="$(printf '%s\n' "$rustc_verbose" | sed -n 's/^release: //p')"
  host_triple="$(printf '%s\n' "$rustc_verbose" | sed -n 's/^host: //p')"
  cargo_version="$(cargo --version 2>/dev/null | sed 's/^cargo //' || true)"
  rustup_toolchain="$(rustup show active-toolchain 2>/dev/null || true)"
  if [[ -z "$rustc_release" || -z "$host_triple" || -z "$cargo_version" ]]; then
    echo "error: cannot resolve Rust toolchain identity for binary provenance" >&2
    return 1
  fi
  printf 'target_dir=%s\nfeatures=test_tier_required\nprofile=debug\nsource_head=%s\nsource_base=%s\nsource_base_ref=%s\nrustc_release=%s\ncargo_version=%s\nactive_toolchain=%s\nhost_triple=%s\n' \
    "$target_dir" "$SOURCE_HEAD" "$SOURCE_BASE" "$SOURCE_BASE_REF" "$rustc_release" "$cargo_version" "$rustup_toolchain" "$host_triple" \
    >"$OUT_DIR/test-tier-required-binaries.meta"
  write_binary_provenance \
    "$target_dir" \
    "$build_log" \
    "$OUT_DIR/test-tier-required-build.json" \
    "$build_started" \
    "$build_finished" \
    "$rustc_release" \
    "$cargo_version" \
    "$rustup_toolchain" \
    "$host_triple" \
    "${required_binaries[@]}"
  export OASIS7_RUN_LAUNCHER_STACK_SKIP_SOURCE_BUILD=1
}

require_nonempty_env() {
  local name="$1"
  if [[ -z "${!name:-}" ]]; then
    echo "error: required strong-auth environment variable ${name} is missing" >&2
    exit 2
  fi
}

configure_test_signer() {
  [[ "$TEST_SIGNER_SEED" == "42" ]] || return 0

  # This is the same deterministic seed-42 fixture used by the Hosted
  # strong-auth unit lane.  It is only installed for the explicit local
  # browser route and never used by production launcher defaults.
  local seed42_private="2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a"
  local seed42_public="197f6b23e16c8532c6abc838facd5ea789be0c76b2920334039bfa8b3d368d61"
  local seed42_approval="correct-code"
  if [[ -n "${OASIS7_HOSTED_STRONG_AUTH_PUBLIC_KEY:-}" && "${OASIS7_HOSTED_STRONG_AUTH_PUBLIC_KEY}" != "$seed42_public" ]] \
    || [[ -n "${OASIS7_HOSTED_STRONG_AUTH_PRIVATE_KEY:-}" && "${OASIS7_HOSTED_STRONG_AUTH_PRIVATE_KEY}" != "$seed42_private" ]] \
    || [[ -n "${OASIS7_HOSTED_STRONG_AUTH_APPROVAL_CODE:-}" && "${OASIS7_HOSTED_STRONG_AUTH_APPROVAL_CODE}" != "$seed42_approval" ]]; then
    echo "error: --test-signer-seed 42 requires the deterministic seed-42 signer fixture; refusing mismatched strong-auth signer environment" >&2
    return 2
  fi
  if [[ -z "${OASIS7_HOSTED_STRONG_AUTH_PUBLIC_KEY:-}" ]]; then
    export OASIS7_HOSTED_STRONG_AUTH_PUBLIC_KEY="$seed42_public"
  fi
  if [[ -z "${OASIS7_HOSTED_STRONG_AUTH_PRIVATE_KEY:-}" ]]; then
    export OASIS7_HOSTED_STRONG_AUTH_PRIVATE_KEY="$seed42_private"
  fi
  if [[ -z "${OASIS7_HOSTED_STRONG_AUTH_APPROVAL_CODE:-}" ]]; then
    export OASIS7_HOSTED_STRONG_AUTH_APPROVAL_CODE="$seed42_approval"
  fi
}

require_loopback_url() {
  local url="$1"
  if ! python3 - "$url" <<'PY'
import ipaddress
import sys
from urllib.parse import urlsplit

parts = urlsplit(sys.argv[1])
host = (parts.hostname or "").strip().lower()
if host == "localhost":
    raise SystemExit(0)
try:
    is_loopback = ipaddress.ip_address(host).is_loopback
except ValueError:
    is_loopback = False
raise SystemExit(0 if is_loopback else 1)
PY
  then
    echo "error: --test-login is restricted to a loopback URL" >&2
    exit 2
  fi
}

read_authoritative_hosted_url() {
  local runtime_log="$OUT_DIR/runtime/oasis7_viewer_live.log"
  python3 - "$runtime_log" <<'PY'
import pathlib
import re
import sys
from urllib.parse import parse_qsl, urlsplit

path = pathlib.Path(sys.argv[1])
if not path.is_file():
    raise SystemExit(0)
for line in path.read_text(errors="replace").splitlines():
    match = re.search(r"^- URL: (https?://\S+)", line)
    if not match:
        continue
    url = match.group(1)
    query = dict(parse_qsl(urlsplit(url).query, keep_blank_values=True))
    if query.get("hosted_access", "").strip():
        print(url)
        raise SystemExit(0)
raise SystemExit(0)
PY
}

wait_for_authoritative_hosted_url() {
  local deadline=$((SECONDS + STARTUP_TIMEOUT))
  local authoritative_url
  while (( SECONDS < deadline )); do
    authoritative_url="$(read_authoritative_hosted_url)"
    if [[ -n "$authoritative_url" ]]; then
      printf '%s\n' "$authoritative_url"
      return 0
    fi
    if [[ -n "$LAUNCH_PID" ]] && ! kill -0 "$LAUNCH_PID" 2>/dev/null; then
      echo "error: hosted_public_join launcher exited before publishing authoritative hosted URL" >&2
      return 1
    fi
    sleep 1
  done
  echo "error: authoritative hosted URL with hosted_access was not published under $OUT_DIR/runtime/oasis7_viewer_live.log" >&2
  return 1
}

meta_value() {
  local key="$1"
  local path="$2"
  sed -n "s/^${key}=//p" "$path" | tail -n 1
}

wait_for_full_gameplay_readiness() {
  local stack_meta="$OUT_DIR/runtime/session.meta"
  local deadline=$((SECONDS + STARTUP_TIMEOUT))
  while (( SECONDS < deadline )); do
    if [[ -n "$LAUNCH_PID" ]] && ! kill -0 "$LAUNCH_PID" 2>/dev/null; then
      echo "error: full-gameplay launcher exited before readiness" >&2
      return 1
    fi
    if [[ -f "$stack_meta" ]] \
      && [[ "$(meta_value STACK_READY "$stack_meta")" == "1" ]] \
      && [[ "$(meta_value LOCAL_TEST_PROVIDER_SETUP_ENABLED "$stack_meta")" == "1" ]] \
      && [[ "$(meta_value CHAIN_ENABLED "$stack_meta")" == "1" ]] \
      && [[ "$(meta_value DEPLOYMENT_MODE "$stack_meta")" == "trusted_local_only" ]] \
      && [[ -f "$LOCAL_PROVIDER_AUTHORITY" ]]; then
      if python3 - "$LOCAL_PROVIDER_AUTHORITY" "$AGENT_ID" <<'PY'
import json
import pathlib
import sys

authority_path = pathlib.Path(sys.argv[1])
agent_id = sys.argv[2]
try:
    authority = json.loads(authority_path.read_text(encoding="utf-8"))
except (OSError, ValueError):
    raise SystemExit(1)
authority_grant = authority.get("grant")
capability_invocation_context = authority.get("invocation_context")
if not isinstance(authority_grant, dict) or not isinstance(capability_invocation_context, dict):
    raise SystemExit(1)
if authority.get("agent_id") != agent_id:
    raise SystemExit(1)
if not authority_grant.get("grant_id") or authority_grant.get("grant_id") != capability_invocation_context.get("grant_id"):
    raise SystemExit(1)
subject = capability_invocation_context.get("subject")
if isinstance(subject, dict) and subject.get("agent_id") not in (None, agent_id):
    raise SystemExit(1)
PY
      then
        return 0
      fi
    fi
    sleep 1
  done
  echo "error: full-gameplay readiness requires STACK_READY=1, LOCAL_TEST_PROVIDER_SETUP_ENABLED=1, CHAIN_ENABLED=1, DEPLOYMENT_MODE=trusted_local_only, and consistent authority_grant/capability_invocation_context" >&2
  return 1
}

configure_loopback_browser_args() {
  local args
  if [[ ${AGENT_BROWSER_ARGS+x} ]]; then
    args="$AGENT_BROWSER_ARGS"
  else
    args="$(ab_browser_args)"
    if [[ "$(uname -s)" == "Darwin" ]]; then
      # The library default is GL, but headed macOS Chrome exposes no WebGL2
      # canvas with that backend. Preserve an explicit caller override.
      args="${args//--use-angle=gl/--use-angle=metal}"
    fi
  fi
  case ",$args," in
    *,--no-proxy-server,*) ;;
    *)
      [[ -n "$args" ]] && args+=","
      args+="--no-proxy-server"
      ;;
  esac
  AGENT_BROWSER_ARGS="$args"
  export AGENT_BROWSER_ARGS
}

write_manifest() {
  local root="$1"
  local tier="$2"
  local eligible="$3"
  local provider_calls="$4"
  local agent_id="${5:-\${AGENT_ID}}"
  python3 - "$root" "$CASE_ID" "$tier" "$eligible" "$provider_calls" "$agent_id" "$TEST_TIER_REQUIRED" "$HOSTED_LOCAL_MOCK" "$TEST_SIGNER_SEED" <<'PY'
import hashlib
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
case_id, tier = sys.argv[2], sys.argv[3]
eligible = sys.argv[4].lower() == "true"
provider_calls = sys.argv[5].lower() == "true"
agent_id = sys.argv[6]
test_tier_required = sys.argv[7] == "1"
hosted_local_mock = sys.argv[8] == "1"
test_signer_seed = int(sys.argv[9]) if sys.argv[9] else None
provenance_path = root / "binary-provenance.json"
binary_provenance = None
if test_tier_required and tier != "contract_only":
    if not provenance_path.is_file():
        raise SystemExit("refusing a test-tier manifest without binary provenance")
    try:
        binary_provenance = json.loads(provenance_path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        raise SystemExit(f"invalid binary provenance: {exc}")
    if binary_provenance.get("status") != "verified":
        raise SystemExit("refusing a test-tier manifest with unverified binary provenance")
browser_diagnostics = None
if tier != "contract_only":
    browser_diagnostics = {
        "console": "browser-console.log",
        "errors": "browser-errors.log",
    }
    for relative in browser_diagnostics.values():
        if not (root / relative).is_file():
            raise SystemExit(f"refusing a headed manifest without browser diagnostics: {relative}")
viewport_path = root / "browser-viewport.json"
viewport = None
if viewport_path.is_file():
    try:
        viewport = json.loads(viewport_path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        viewport = None
layout_path = root / "browser-layout.json"
prompt_layout = None
if layout_path.is_file():
    try:
        prompt_layout = json.loads(layout_path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        prompt_layout = None
visible = {
    "selection": rf'[data-pixel-world-agent-marker=\"true\"][data-agent-id=\"{agent_id}\"]',
    "promptDisclosure": "Advanced Prompt Settings",
    "shortTermGoal": "#prompt-short",
    "testLogin": r'[data-auth-action=\"test-login\"]',
    "approvalCode": "#strong-auth-approval-code",
    "preview": r'button[data-prompt-action=\"preview\"]',
    "apply": r'button[data-prompt-action=\"apply\"]',
    "rollbackTarget": "#prompt-rollback-version",
    "rollback": r'button[data-prompt-action=\"rollback\"]',
}
artifacts = []
for path in sorted(root.rglob("*")):
    if not path.is_file() or path.name == "artifact-manifest.json":
        continue
    data = path.read_bytes()
    artifacts.append({
        "bytes": len(data),
        "path": path.relative_to(root).as_posix(),
        "sha256": hashlib.sha256(data).hexdigest(),
    })
manifest = {
    "acceptanceEligible": eligible,
    "artifacts": artifacts,
    "browserMode": "headed",
    "browserDiagnostics": browser_diagnostics,
    "caseId": case_id,
    "binaryProvenance": binary_provenance,
    "evidenceTier": tier,
    "externalProviderCallsDuringVerification": provider_calls,
    "launchRoute": "hosted_local_mock_seeded_agent" if hosted_local_mock else "legacy_prompt_control",
    "localMockCallsDuringVerification": hosted_local_mock,
    "providerCallsDuringVerification": provider_calls,
    "source": (binary_provenance or {}).get("source"),
    "schema": "oasis7.viewer.prompt-control-artifact-manifest/v1",
    "testSignerSeed": test_signer_seed,
    "testTierRequired": test_tier_required,
    "visibleActionContract": visible,
    "viewport": viewport,
    "promptLayout": prompt_layout,
}
(root / "artifact-manifest.json").write_text(
    json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8"
)
PY
}

write_contract_artifacts() {
  mkdir -p "$OUT_DIR"
  python3 - "$OUT_DIR" "$CASE_ID" <<'PY'
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
case_id = sys.argv[2]
visible = {
    "selection": r'[data-pixel-world-agent-marker=\"true\"][data-agent-id=\"${AGENT_ID}\"]',
    "promptDisclosure": "Advanced Prompt Settings",
    "shortTermGoal": "#prompt-short",
    "testLogin": "[data-auth-action=\"test-login\"]",
    "approvalCode": "#strong-auth-approval-code",
    "preview": r'button[data-prompt-action=\"preview\"]',
    "apply": r'button[data-prompt-action=\"apply\"]',
    "rollbackTarget": "#prompt-rollback-version",
    "rollback": r'button[data-prompt-action=\"rollback\"]',
}
contract = {
    "browserMode": "headed",
    "caseId": case_id,
    "providerCallsDuringVerification": False,
    "visibleActionContract": visible,
}
manifest_input = {
    "acceptanceEligible": False,
    "evidenceTier": "contract_only",
    "required": ["headed_browser", "visible_selection", "preview", "apply", "rollback"],
}
(root / "contract.json").write_text(json.dumps(contract, indent=2, sort_keys=True) + "\n", encoding="utf-8")
(root / "manifest-input.json").write_text(json.dumps(manifest_input, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
  write_manifest "$OUT_DIR" "contract_only" false false
  echo "contract-only artifacts written to $OUT_DIR"
}

# This branch is deliberately before ab_require and the launcher. It is the
# deterministic, no-provider verification surface used by automated tests.
configure_test_signer
if (( CONTRACT_ONLY == 1 )); then
  write_runner_config
  write_contract_artifacts
  exit 0
fi

require_nonempty_env OASIS7_HOSTED_STRONG_AUTH_PUBLIC_KEY
require_nonempty_env OASIS7_HOSTED_STRONG_AUTH_PRIVATE_KEY
require_nonempty_env OASIS7_HOSTED_STRONG_AUTH_APPROVAL_CODE
if (( TEST_LOGIN == 0 )); then
  echo "error: PWT-004 requires explicit --test-login for the local strong-auth evidence lane" >&2
  exit 2
fi
export OASIS7_HOSTED_TEST_LOGIN_ENABLED=1
if [[ -n "$GAME_URL" ]]; then
  require_loopback_url "$GAME_URL"
fi

source "$ROOT_DIR/scripts/agent-browser-lib.sh"
if (( TEST_TIER_REQUIRED == 1 )); then
  capture_source_provenance
fi
mkdir -p "$OUT_DIR"
if (( TEST_TIER_REQUIRED == 1 )); then
  build_test_tier_binaries
fi
write_runner_config
RUN_ID="viewer-prompt-control-${CASE_ID}-$(date -u +%Y%m%dT%H%M%SZ)-$$"
LAUNCH_LOG="$OUT_DIR/launcher.log"
AB_LOG="$OUT_DIR/agent-browser.log"
LAUNCH_PID=""
SESSION=""
cleanup() {
  local exit_code=$?
  trap - EXIT INT TERM
  if [[ -n "$LAUNCH_PID" ]] && kill -0 "$LAUNCH_PID" 2>/dev/null; then
    kill "$LAUNCH_PID" 2>/dev/null || true
    wait "$LAUNCH_PID" >/dev/null 2>&1 || true
  fi
  if [[ -n "$SESSION" ]]; then
    ab_session_cleanup "$SESSION" "$OUT_DIR" || ab_cmd "$SESSION" close >/dev/null 2>&1 || true
  fi
  exit "$exit_code"
}
trap cleanup EXIT

ab_require
SESSION="$(ab_session_begin "viewer-prompt-control-${CASE_ID}-${RUN_ID}" "$OUT_DIR")"

diagnostic_slug() {
  printf '%s' "$1" | tr -cs '[:alnum:]_.-' '_' | sed 's/^_//; s/_$//'
}

capture_failure_diagnostics() {
  local stage="$1"
  local slug
  local base
  local page_summary
  local state
  slug="$(diagnostic_slug "$stage")"
  base="$OUT_DIR/failure-${slug}"

  printf '[failure:%s] collecting browser diagnostics\n' "$stage" >>"$AB_LOG"
  AB_READ_RETRY_ATTEMPTS=1 ab_read_retry "$SESSION" session info --json \
    >"${base}-session-info.json" 2>&1 || true
  AB_READ_RETRY_ATTEMPTS=1 ab_read_retry "$SESSION" tab list --json \
    >"${base}-tabs.json" 2>&1 || true
  AB_READ_RETRY_ATTEMPTS=1 ab_read_retry "$SESSION" console \
    >"${base}-console.log" 2>&1 || true
  AB_READ_RETRY_ATTEMPTS=1 ab_read_retry "$SESSION" errors \
    >"${base}-errors.log" 2>&1 || true
  AB_READ_RETRY_ATTEMPTS=1 ab_read_retry "$SESSION" snapshot -i \
    >"${base}-snapshot.txt" 2>&1 || true
  ab_screenshot "$SESSION" "${base}.png" \
    >"${base}-screenshot.log" 2>&1 || true

  page_summary="$(AB_READ_RETRY_ATTEMPTS=1 ab_read_eval "$SESSION" \
    'JSON.stringify({readyState:document.readyState,title:document.title,url:location.href,awTest:typeof window.__AW_TEST__,testLogin:Boolean(document.querySelector("[data-auth-action=\\"test-login\\"]"))})' \
    2>/dev/null || true)"
  printf '%s\n' "$page_summary" >"${base}-page-summary.json"

  state="$(AB_READ_RETRY_ATTEMPTS=1 ab_read_eval "$SESSION" \
    'window.__AW_TEST__?.getState?.() ?? null' 2>/dev/null || true)"
  if [[ -n "$state" ]]; then
    write_safe_state "$state" "${base}-state.json" || true
  fi
  printf '[failure:%s] diagnostics written under %s\n' "$stage" "$OUT_DIR" >>"$AB_LOG"
}

wait_for_cli_stage() {
  local stage="$1"
  shift
  local defer_failure=0
  local output
  local result
  if [[ "${1:-}" == "--defer-failure" ]]; then
    defer_failure=1
    shift
  fi
  if output="$(ab_read_retry "$SESSION" "$@" 2>&1)"; then
    printf '[%s] %s\n' "$stage" "$output" >>"$AB_LOG"
    return 0
  else
    result=$?
  fi
  printf '[%s] command failed (exit=%s):\n%s\n' "$stage" "$result" "$output" >>"$AB_LOG"
  capture_failure_diagnostics "$stage"
  if (( defer_failure == 0 )); then
    echo "error: ${stage} wait failed (phase: ${stage}; diagnostics: ${OUT_DIR}/failure-$(diagnostic_slug "$stage")-*)" >&2
  fi
  return "$result"
}

run_visible_action() {
  local phase="$1"
  shift
  local output
  local result
  # Visible actions intentionally call ab_cmd directly: an uncertain click or
  # fill must never be replayed because it may already have reached the page.
  if output="$(ab_cmd "$SESSION" "$@" 2>&1)"; then
    printf '[action:%s] %s\n' "$phase" "$output" >>"$AB_LOG"
    return 0
  else
    result=$?
  fi
  printf '[action:%s] command failed (exit=%s):\n%s\n' "$phase" "$result" "$output" >>"$AB_LOG"
  capture_failure_diagnostics "$phase"
  echo "error: visible action ${phase} failed (phase: ${phase}; diagnostics: ${OUT_DIR}/failure-$(diagnostic_slug "$phase")-*)" >&2
  return "$result"
}

capture_browser_viewport() {
  local raw
  local result
  if raw="$(ab_read_eval "$SESSION" 'JSON.stringify({innerWidth:window.innerWidth,innerHeight:window.innerHeight,outerWidth:window.outerWidth,outerHeight:window.outerHeight,devicePixelRatio:window.devicePixelRatio,visualViewportWidth:window.visualViewport?.width ?? null,visualViewportHeight:window.visualViewport?.height ?? null})' 2>&1)"; then
    :
  else
    result=$?
    printf '[viewport] measurement failed (exit=%s):\n%s\n' "$result" "$raw" >>"$AB_LOG"
    echo "error: browser viewport measurement failed (phase: browser viewport measurement; diagnostics: ${OUT_DIR}/agent-browser.log)" >&2
    return "$result"
  fi
  if ! python3 - "$OUT_DIR/browser-viewport.json" "$raw" "$VIEWPORT_WIDTH" "$VIEWPORT_HEIGHT" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
raw = sys.argv[2]
requested_width = int(sys.argv[3]) if sys.argv[3] else None
requested_height = int(sys.argv[4]) if sys.argv[4] else None
try:
    measured = json.loads(raw)
    if isinstance(measured, str):
        measured = json.loads(measured)
except (TypeError, ValueError) as exc:
    raise SystemExit(f"invalid browser viewport measurement: {exc}")
if not isinstance(measured, dict):
    raise SystemExit("invalid browser viewport measurement: expected object")
required = ("innerWidth", "innerHeight", "outerWidth", "outerHeight", "devicePixelRatio")
if any(key not in measured for key in required):
    raise SystemExit("invalid browser viewport measurement: missing required field")
width = int(measured["innerWidth"])
height = int(measured["innerHeight"])
if requested_width is not None and (width != requested_width or height != requested_height):
    raise SystemExit(
        f"requested viewport {requested_width}x{requested_height} measured as {width}x{height}"
    )
payload = {
    "height": height,
    "innerHeight": height,
    "innerWidth": width,
    "outerHeight": int(measured["outerHeight"]),
    "outerWidth": int(measured["outerWidth"]),
    "requestedHeight": requested_height,
    "requestedWidth": requested_width,
    "devicePixelRatio": float(measured["devicePixelRatio"]),
    "visualViewportHeight": measured.get("visualViewportHeight"),
    "visualViewportWidth": measured.get("visualViewportWidth"),
    "width": width,
}
path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
  then
    result=$?
    printf '[viewport] measurement rejected (exit=%s): %s\n' "$result" "$raw" >>"$AB_LOG"
    echo "error: browser viewport measurement did not satisfy the requested dimensions (phase: browser viewport measurement; diagnostics: ${OUT_DIR}/agent-browser.log)" >&2
    return "$result"
  fi
  printf '[viewport] %s\n' "$(tr '\n' ' ' <"$OUT_DIR/browser-viewport.json")" >>"$AB_LOG"
}

capture_prompt_layout() {
  local raw
  local result
  if raw="$(ab_read_eval "$SESSION" 'JSON.stringify((() => { const button = document.querySelector(`button[data-prompt-action="rollback"]`); button?.focus({preventScroll:true}); const rect = button?.getBoundingClientRect(); const center = rect ? { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 } : null; const hit = center ? document.elementFromPoint(center.x, center.y) : null; const target = document.querySelector(`#viewer-details-panel .command-surface__target-row`); const targetRect = target?.getBoundingClientRect(); const buttonStyle = button ? getComputedStyle(button) : null; return { activeElementIsRollback: document.activeElement === button, horizontalOverflowPx: Math.max(0, document.documentElement.scrollWidth - window.innerWidth), rollback: { activeElement: document.activeElement?.getAttribute?.(`data-prompt-action`) || document.activeElement?.tagName || null, center, centerHit: hit ? { dataAction: hit.getAttribute(`data-prompt-action`), className: String(hit.className || ``), tagName: hit.tagName } : null, focusVisible: button?.matches(`:focus-visible`) === true, height: rect?.height || 0, minHitTarget: Boolean(rect && rect.width >= 44 && rect.height >= 44), outlineStyle: buttonStyle?.outlineStyle || null, outlineWidth: buttonStyle?.outlineWidth || null, text: button?.textContent?.trim() || null, uncovered: Boolean(button && hit && (hit === button || button.contains(hit))), visible: Boolean(rect && rect.top >= 0 && rect.left >= 0 && rect.bottom <= window.innerHeight && rect.right <= window.innerWidth), width: rect?.width || 0 }, targetRow: { height: targetRect?.height || 0, position: target ? getComputedStyle(target).position : null, top: targetRect?.top || 0, visible: Boolean(targetRect && targetRect.top >= 0 && targetRect.bottom <= window.innerHeight) }, viewport: { height: window.innerHeight, width: window.innerWidth } }; })())' 2>&1)"; then
    :
  else
    result=$?
    printf '[layout] measurement failed (exit=%s):\n%s\n' "$result" "$raw" >>"$AB_LOG"
    echo "error: prompt layout measurement failed (phase: prompt layout measurement; diagnostics: ${OUT_DIR}/agent-browser.log)" >&2
    return "$result"
  fi
  if python3 - "$OUT_DIR/browser-layout.json" "$raw" <<'PY'
import json
import math
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
raw = sys.argv[2]
try:
    measured = json.loads(raw)
    if isinstance(measured, str):
        measured = json.loads(measured)
except (TypeError, ValueError) as exc:
    raise SystemExit(f"invalid prompt layout measurement: {exc}")
if not isinstance(measured, dict):
    raise SystemExit("invalid prompt layout measurement: expected object")

def finite_number(value, label):
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value):
        raise SystemExit(f"invalid prompt layout measurement: {label} must be a finite number")
    return float(value)

overflow = finite_number(measured.get("horizontalOverflowPx"), "horizontalOverflowPx")
if overflow > 0:
    raise SystemExit(
        f"invalid prompt layout measurement: horizontal overflow is {overflow:g}px"
    )

rollback = measured.get("rollback")
if not isinstance(rollback, dict):
    raise SystemExit("invalid prompt layout measurement: rollback must be an object")
for field in ("visible", "uncovered", "minHitTarget"):
    if rollback.get(field) is not True:
        raise SystemExit(f"invalid prompt layout measurement: rollback.{field} must be true")
for field in ("width", "height"):
    size = finite_number(rollback.get(field), f"rollback.{field}")
    if size < 44:
        raise SystemExit(
            f"invalid prompt layout measurement: rollback.{field} is {size:g}px; minimum is 44px"
        )
path.write_text(json.dumps(measured, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
  then
    :
  else
    result=$?
    printf '[layout] measurement rejected (exit=%s): %s\n' "$result" "$raw" >>"$AB_LOG"
    echo "error: prompt layout measurement rejected (phase: prompt layout measurement; diagnostics: ${OUT_DIR}/agent-browser.log)" >&2
    return "$result"
  fi
  printf '[layout] %s\n' "$(tr '\n' ' ' <"$OUT_DIR/browser-layout.json")" >>"$AB_LOG"
}

register_hosted_player_session() {
  local result
  # This hook only registers the server-side session; it is intentionally an
  # action-bearing eval with no retry because it may already have committed.
  if ab_eval "$SESSION" "window.__AW_TEST__.registerPlayerSessionForTest(null)" >>"$AB_LOG" 2>&1; then
    printf '[action:hosted player session registration action] completed\n' >>"$AB_LOG"
    return 0
  else
    result=$?
  fi
  printf '[action:hosted player session registration action] command failed (exit=%s)\n' "$result" >>"$AB_LOG"
  capture_failure_diagnostics "hosted player session registration action"
  echo "error: hosted player session registration action failed (phase: hosted player session registration action; diagnostics: ${OUT_DIR}/failure-$(diagnostic_slug "hosted player session registration action")-*)" >&2
  return "$result"
}

maybe_rebind_post_onboarding_session() {
  local agent_id_json="$1"
  local rebind_required
  local result
  rebind_required="$(ab_read_eval "$SESSION" "(() => { const s = window.__AW_TEST__.getState(); return s?.authBoundAgentId === ${agent_id_json} && s?.authBindingEpoch == null; })()" 2>/dev/null || true)"
  case "$rebind_required" in
    true|\"true\")
      # This is an action-bearing session setup call.  Never retry it: the
      # first request may have committed even if browser transport failed.
      if ab_eval "$SESSION" "window.__AW_TEST__.registerPlayerSessionForTest(${agent_id_json}, {forceRebind: true})" >>"$AB_LOG" 2>&1; then
        printf '[action:post-onboarding auth binding rebind action] completed\n' >>"$AB_LOG"
        return 0
      else
        result=$?
      fi
      printf '[action:post-onboarding auth binding rebind action] command failed (exit=%s)\n' "$result" >>"$AB_LOG"
      capture_failure_diagnostics "post-onboarding auth binding rebind action"
      echo "error: post-onboarding auth binding rebind action failed (phase: post-onboarding auth binding rebind action; diagnostics: ${OUT_DIR}/failure-$(diagnostic_slug "post-onboarding auth binding rebind action")-*)" >&2
      return "$result"
      ;;
    false|\"false\")
      printf '[action:post-onboarding auth binding rebind action] skipped; binding epoch already present or agent not bound\n' >>"$AB_LOG"
      ;;
    *)
      capture_failure_diagnostics "post-onboarding auth binding rebind decision"
      echo "error: post-onboarding auth binding rebind state was unavailable (phase: post-onboarding auth binding rebind decision; diagnostics: ${OUT_DIR}/failure-$(diagnostic_slug "post-onboarding auth binding rebind decision")-*)" >&2
      return 1
      ;;
  esac
}

maybe_claim_first_agent() {
  local agent_selector_json="$1"
  local empty_world
  local claim_button_xpath
  local claim_button_xpath_json
  empty_world="$(ab_read_eval "$SESSION" "(() => { const s = window.__AW_TEST__.getState(); const g = s?.gameplaySummary || {}; return !document.querySelector(${agent_selector_json}) && Number(g?.entityCounts?.agents) === 0; })()" 2>/dev/null || true)"
  case "$empty_world" in
    true|\"true\")
      FIRST_AGENT_CLAIM_PERFORMED=1
      ;;
    false|\"false\")
      return 0
      ;;
    *)
      capture_failure_diagnostics "empty-world entity gate"
      echo "error: empty-world entity gate state was unavailable (phase: empty-world entity gate)" >&2
      return 1
      ;;
  esac

  run_visible_action "claim first agent panel navigation action" click \
    'a[href="#viewer-targets-panel"]'
  claim_button_xpath='//*[@id="viewer-targets-panel"]//button[contains(normalize-space(.), "Claim First Agent") or contains(normalize-space(.), "认领第一个 Agent")]'
  claim_button_xpath_json="$(json_quote "$claim_button_xpath")"
  wait_for_cli_stage "claim first agent button" wait --fn \
    "Boolean((() => { const node = document.evaluate(${claim_button_xpath_json}, document, null, XPathResult.FIRST_ORDERED_NODE_TYPE, null).singleNodeValue; return node && !node.disabled; })())"
  run_visible_action "claim first agent action" click "xpath=$claim_button_xpath"
  wait_for_js_true \
    '(() => { const s = window.__AW_TEST__.getState(); const f = s?.lastGameplayActionFeedback; return f?.kind === "gameplay_action" && f?.action === "claim_first_agent" && f?.stage === "ack" && f?.accepted === true && f?.response?.action_id === "claim_first_agent"; })()' \
    "claim first agent gameplay authority ack"
  wait_for_js_true \
    '(() => { const s = window.__AW_TEST__.getState(); return Number(s?.gameplaySummary?.entityCounts?.agents) > 0; })()' \
    "claim first agent snapshot entity count"
  wait_for_js_true "(() => window.__AW_TEST__.getState()?.selectedId === ${agent_selector_json})()" \
    "claim first agent authoritative selection"
}

maybe_claim_starter_oc() {
  local agent_id_json="$1"
  local starter_oc_xpath
  local starter_oc_xpath_json
  local onboarding_visible
  # The player-facing overlay is titled Claim Your First OC.
  starter_oc_xpath='//*[@data-viewer-fixture-state="starter_oc_required_gate"]//button[contains(normalize-space(.), "Claim Starter OC") or contains(normalize-space(.), "领取初始 OC")]'
  starter_oc_xpath_json="$(json_quote "$starter_oc_xpath")"
  onboarding_visible="$(ab_read_eval "$SESSION" "Boolean((() => { const node = document.evaluate(${starter_oc_xpath_json}, document, null, XPathResult.FIRST_ORDERED_NODE_TYPE, null).singleNodeValue; return node && !node.disabled && node.getClientRects().length > 0; })())" 2>/dev/null || true)"
  case "$onboarding_visible" in
    false|\"false\")
      return 0
      ;;
    true|\"true\")
      ;;
    *)
      capture_failure_diagnostics "starter OC onboarding visibility"
      echo "error: starter OC onboarding visibility was unavailable (phase: starter OC onboarding visibility)" >&2
      return 1
      ;;
  esac

  run_visible_action "claim starter oc action" click "xpath=$starter_oc_xpath"
  wait_for_js_true \
    '(() => { const s = window.__AW_TEST__.getState(); const f = s?.lastGameplayActionFeedback; return f?.kind === "gameplay_action" && f?.action === "claim_starter_oc" && f?.stage === "ack" && f?.accepted === true && f?.response?.action_id === "claim_starter_oc"; })()' \
    "claim starter oc gameplay authority ack"
  wait_for_js_true "(() => { const s = window.__AW_TEST__.getState(); return !document.querySelector('[data-viewer-fixture-state=\"starter_oc_required_gate\"]') && Boolean(document.querySelector('#prompt-short')) && s?.authBoundAgentId === ${agent_id_json}; })()" \
    "starter OC overlay dismissal and agent binding"
}

wait_for_js_true() {
  local script="$1"
  local label="$2"
  local timeout_ms="${3:-$ACTION_TIMEOUT_MS}"
  local timeout_secs=$(( (timeout_ms + 999) / 1000 ))
  local deadline
  local value
  (( timeout_secs > 0 )) || timeout_secs=1
  deadline=$((SECONDS + timeout_secs))
  while (( SECONDS < deadline )); do
    value="$(ab_read_eval "$SESSION" "$script" 2>/dev/null || true)"
    if [[ "$value" == "true" || "$value" == '"true"' ]]; then
      return 0
    fi
    sleep 0.2
  done
  printf '[%s] JS wait timed out; last value=%s\n' "$label" "${value:-<empty>}" >>"$AB_LOG"
  capture_failure_diagnostics "$label"
  echo "error: timed out waiting for ${label} (phase: ${label}; diagnostics: ${OUT_DIR}/failure-$(diagnostic_slug "$label")-*)" >&2
  return 1
}

wait_for_prompt_surface_continuity() {
  local timeout_ms="${1:-$ACTION_TIMEOUT_MS}"
  local timeout_secs=$(( (timeout_ms + 999) / 1000 ))
  local deadline
  local status=""
  (( timeout_secs > 0 )) || timeout_secs=1
  deadline=$((SECONDS + timeout_secs))
  while (( SECONDS < deadline )); do
    status="$(ab_read_eval "$SESSION" '(() => { const href = String(window.location.href || ""); if (href === "about:blank") return "tab_lost"; const panel = document.querySelector(`section#viewer-details-panel[data-viewer-route-panel="command"]`); const summary = panel?.querySelector("details.command-surface__advanced-details > summary"); return panel && summary ? "ready" : `prompt_surface_missing:${href}`; })()' 2>/dev/null || true)"
    case "$status" in
      ready|\"ready\")
        printf '[prompt surface page continuity] command panel and Advanced summary ready\n' >>"$AB_LOG"
        return 0
        ;;
      tab_lost|\"tab_lost\")
        printf '[prompt surface page continuity] tab lost to about:blank\n' >>"$AB_LOG"
        capture_failure_diagnostics "pre-prompt tab loss"
        echo "error: pre-prompt tab loss detected: active page is about:blank (phase: pre-prompt tab loss; diagnostics: ${OUT_DIR}/failure-$(diagnostic_slug "pre-prompt tab loss")-*)" >&2
        return 1
        ;;
    esac
    sleep 0.2
  done
  printf '[prompt surface page continuity] exact prompt DOM missing; last status=%s\n' "${status:-<empty>}" >>"$AB_LOG"
  capture_failure_diagnostics "prompt surface page continuity"
  echo "error: prompt surface missing while page remained active (phase: prompt surface page continuity; last status: ${status:-<empty>}; diagnostics: ${OUT_DIR}/failure-$(diagnostic_slug "prompt surface page continuity")-*)" >&2
  return 1
}

wait_for_domcontentloaded() {
  local result
  if wait_for_cli_stage "domcontentloaded" --defer-failure wait --load domcontentloaded; then
    return 0
  else
    result=$?
  fi
  if wait_for_js_true \
    'document.readyState === "complete" || document.readyState === "interactive"' \
    "domcontentloaded fallback" "$ACTION_TIMEOUT_MS"; then
    printf '[domcontentloaded fallback] JS readyState accepted after CLI wait failure\n' >>"$AB_LOG"
    echo "warning: domcontentloaded CLI wait failed; readyState fallback passed (phase: domcontentloaded fallback)" >&2
    return 0
  fi
  echo "error: domcontentloaded readiness failed after CLI wait and JS fallback (phase: domcontentloaded)" >&2
  return "$result"
}

wait_for_webgl2() {
  wait_for_js_true \
    'Boolean(document.createElement("canvas").getContext("webgl2"))' \
    "WebGL2 readiness" "$ACTION_TIMEOUT_MS"
}

state_raw() {
  ab_read_eval "$SESSION" 'window.__AW_TEST__.getState()'
}

write_safe_state() {
  local raw_json="$1"
  local out_path="$2"
  python3 - "$raw_json" "$out_path" <<'PY'
import json
import pathlib
import sys

raw, out_path = sys.argv[1], pathlib.Path(sys.argv[2])
sensitive_keys = {
    "authPlayerId",
    "authPublicKey",
    "authRevokedBy",
    "authRecoveryErrorMessage",
    "privateKey",
    "publicKey",
    "releaseToken",
    "registrationGrant",
    "approvalCode",
    "submittedDraft",
    "systemPrompt",
    "shortTermGoal",
    "longTermGoal",
    "system_prompt_override",
    "short_term_goal_override",
    "long_term_goal_override",
}

def redact(value):
    if isinstance(value, dict):
        return {
            key: redact(item)
            for key, item in value.items()
            if key not in sensitive_keys
        }
    if isinstance(value, list):
        return [redact(item) for item in value]
    return value

try:
    data = json.loads(raw)
except Exception:
    data = {"stateReadback": "unavailable"}
out_path.write_text(json.dumps(redact(data), ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
PY
}

capture_success_browser_diagnostics() {
  local status=0
  if ! AB_READ_RETRY_ATTEMPTS=1 ab_read_retry "$SESSION" console \
    >"$OUT_DIR/browser-console.log" 2>&1; then
    status=1
  fi
  if ! AB_READ_RETRY_ATTEMPTS=1 ab_read_retry "$SESSION" errors \
    >"$OUT_DIR/browser-errors.log" 2>&1; then
    status=1
  fi
  if (( status != 0 )); then
    echo "error: successful headed evidence requires browser console/errors capture (phase: browser diagnostics; diagnostics: $OUT_DIR)" >&2
    return "$status"
  fi
}

quiesce_for_manifest() {
  local status=0
  local launch_pid
  if [[ -n "$LAUNCH_PID" ]]; then
    launch_pid="$LAUNCH_PID"
    kill "$LAUNCH_PID" 2>/dev/null || true
    wait "$LAUNCH_PID" >/dev/null 2>&1 || true
    if kill -0 "$launch_pid" 2>/dev/null; then
      status=1
    fi
    if [[ -f "$LAUNCH_LOG" ]] && grep -Fq -- "unable to prove launcher process-group quiescence" "$LAUNCH_LOG"; then
      status=1
    fi
    LAUNCH_PID=""
  fi
  if [[ -n "$SESSION" ]]; then
    if ! ab_session_cleanup "$SESSION" "$OUT_DIR"; then
      status=1
    fi
    SESSION=""
  fi
  if (( status != 0 )); then
    echo "error: headed evidence processes did not reach quiescence before manifest finalization" >&2
    return "$status"
  fi
}

wait_for_prompt_feedback() {
  local mode="$1"
  local rollback_version="${2:-}"
  local expression
  local agent_id_json
  agent_id_json="$(json_quote "$AGENT_ID")"
  case "$mode" in
    preview)
      expression="(() => { const s = window.__AW_TEST__.getState(); const f = s?.lastPromptFeedback; const r = f?.response || {}; return f?.action === 'prompt_preview' && f?.stage === 'accepted' && r.status === 'accepted' && r.preview === true && Number(r.mutation_count) === 0 && r.applied_scope === 'none' && r.persistence_scope === 'none' && r.sync_scope === 'none' && s?.strongAuthLastGrantActionId === 'prompt_control_preview' && !s?.strongAuthLastGrantError && f?.agentId === ${agent_id_json}; })()"
      ;;
    apply)
      expression="(() => { const s = window.__AW_TEST__.getState(); const f = s?.lastPromptFeedback; const r = f?.response || {}; return f?.action === 'prompt_apply' && f?.stage === 'applied' && r.status === 'applied' && r.preview === false && Number(r.mutation_count) === 1 && r.applied_scope === 'runtime_instance' && r.persistence_scope === 'none' && r.sync_scope === 'none' && s?.strongAuthLastGrantActionId === 'prompt_control_apply' && !s?.strongAuthLastGrantError && f?.agentId === ${agent_id_json}; })()"
      ;;
    rollback)
      expression="(() => { const s = window.__AW_TEST__.getState(); const f = s?.lastPromptFeedback; const r = f?.response || {}; return f?.action === 'prompt_rollback' && f?.stage === 'applied' && r.status === 'applied' && r.operation === 'rollback' && Number(r.mutation_count) === 1 && r.applied_scope === 'runtime_instance' && r.persistence_scope === 'none' && r.sync_scope === 'none' && Number(r.rolled_back_to_version) === ${rollback_version} && s?.strongAuthLastGrantActionId === 'prompt_control_rollback' && !s?.strongAuthLastGrantError && f?.agentId === ${agent_id_json}; })()"
      ;;
    *)
      echo "error: unsupported prompt feedback mode ${mode}" >&2
      return 2
      ;;
  esac
  wait_for_js_true "$expression" "${mode} authority feedback" "$ACTION_TIMEOUT_MS"
}

wait_for_apply_version_convergence() {
  local before_version="$1"
  wait_for_js_true \
    "(() => { const s = window.__AW_TEST__.getState(); const f = s?.lastPromptFeedback; const r = f?.response || {}; const beforeVersion = Number(${before_version}); return f?.action === 'prompt_apply' && f?.stage === 'applied' && Number(r.version) > beforeVersion && Number(s?.selectedPromptVersion) === Number(r.version); })()" \
    "apply authoritative version convergence" "$ACTION_TIMEOUT_MS"
}

wait_for_rollback_version_convergence() {
  local rollback_target_version="$1"
  wait_for_js_true \
    "(() => { const s = window.__AW_TEST__.getState(); const f = s?.lastPromptFeedback; const r = f?.response || {}; const rollbackTargetVersion = Number(${rollback_target_version}); return f?.action === 'prompt_rollback' && f?.stage === 'applied' && Number(r.rolled_back_to_version) === rollbackTargetVersion && Number(s?.selectedPromptVersion) === Number(r?.version); })()" \
    "rollback authoritative version convergence" "$ACTION_TIMEOUT_MS"
}

if [[ -z "$GAME_URL" ]]; then
  STACK_BOOTSTRAPPED=1
  if (( HOSTED_LOCAL_MOCK == 1 )); then
    # This is the only headed Hosted local-mock route.  It uses the existing
    # run-scoped W3 DevLocal authority/funding fixture so starter-agent-0 has
    # canonical starter ownership/funding while HostedPublicJoin + strong-auth
    # remain the browser-facing authority surfaces.  No external bridge or
    # inference provider is started.
    if ((${#STACK_ARGS[@]} > 0)); then
      STACK_ARGS=(
        --chain-enable
        --chain-local-standalone-test
        --chain-node-auto-attest-all
        --major-world-event-visibility restricted
        --local-test-provider-authority "$LOCAL_PROVIDER_AUTHORITY"
        --local-test-provider-wasm "$LOCAL_PROVIDER_WASM"
        --local-test-provider-metadata "$LOCAL_PROVIDER_METADATA"
        --local-test-provider-agent-id "$AGENT_ID"
        --local-test-provider-owner-binding local-test-owner-0
        --local-test-provider-finality-block-hash blake3:0000000000000000000000000000000000000000000000000000000000000000
        --local-test-provider-session-mode hosted_public_join
        --skip-llm-provider-preflight
        --agent-decision-source provider_backed
        --agent-provider-lane local-mock
        --agent-provider-transport loopback_http
        --agent-provider-url http://127.0.0.1:5841
        --agent-execution-lane player_parity
        --no-auto-play
        "${STACK_ARGS[@]}"
      )
    else
      STACK_ARGS=(
        --chain-enable
        --chain-local-standalone-test
        --chain-node-auto-attest-all
        --major-world-event-visibility restricted
        --local-test-provider-authority "$LOCAL_PROVIDER_AUTHORITY"
        --local-test-provider-wasm "$LOCAL_PROVIDER_WASM"
        --local-test-provider-metadata "$LOCAL_PROVIDER_METADATA"
        --local-test-provider-agent-id "$AGENT_ID"
        --local-test-provider-owner-binding local-test-owner-0
        --local-test-provider-finality-block-hash blake3:0000000000000000000000000000000000000000000000000000000000000000
        --local-test-provider-session-mode hosted_public_join
        --skip-llm-provider-preflight
        --agent-decision-source provider_backed
        --agent-provider-lane local-mock
        --agent-provider-transport loopback_http
        --agent-provider-url http://127.0.0.1:5841
        --agent-execution-lane player_parity
        --no-auto-play
      )
    fi
  elif (( FULL_GAMEPLAY == 1 )); then
    if ((${#STACK_ARGS[@]} > 0)); then
      STACK_ARGS=(
        --allow-trusted-local-playtest
        --chain-enable
        --chain-local-standalone-test
        --chain-node-auto-attest-all
        --chain-link-policy shadow
        --major-world-event-visibility restricted
        --local-test-provider-authority "$LOCAL_PROVIDER_AUTHORITY"
        --local-test-provider-wasm "$LOCAL_PROVIDER_WASM"
        --local-test-provider-metadata "$LOCAL_PROVIDER_METADATA"
        --local-test-provider-agent-id starter-agent-0
        --local-test-provider-owner-binding local-test-owner-0
        --local-test-provider-finality-block-hash blake3:0000000000000000000000000000000000000000000000000000000000000000
        --local-test-provider-session-mode hosted_public_join
        "${STACK_ARGS[@]}"
      )
    else
      STACK_ARGS=(
        --allow-trusted-local-playtest
        --chain-enable
        --chain-local-standalone-test
        --chain-node-auto-attest-all
        --chain-link-policy shadow
        --major-world-event-visibility restricted
        --local-test-provider-authority "$LOCAL_PROVIDER_AUTHORITY"
        --local-test-provider-wasm "$LOCAL_PROVIDER_WASM"
        --local-test-provider-metadata "$LOCAL_PROVIDER_METADATA"
        --local-test-provider-agent-id starter-agent-0
        --local-test-provider-owner-binding local-test-owner-0
        --local-test-provider-finality-block-hash blake3:0000000000000000000000000000000000000000000000000000000000000000
        --local-test-provider-session-mode hosted_public_join
      )
    fi
  elif (( STACK_CHAIN_ARG_EXPLICIT == 0 )); then
    # Hosted bootstrap defaults to a chain-disabled page-play lane.  Any
    # explicit --chain-* caller argument remains authoritative.
    STACK_ARGS+=(--chain-disable)
  fi
  if ((${#STACK_ARGS[@]} > 0)); then
    "$ROOT_DIR/scripts/run-launcher-stack.sh" --viewer-static-dir "$ROOT_DIR/crates/oasis7_viewer/.viewer-test-dist" \
      --with-llm \
      --agent-decision-source "$([[ "$HOSTED_LOCAL_MOCK" == "1" ]] && printf provider_backed || printf builtin_llm)" \
      --deployment-mode "$([[ "$FULL_GAMEPLAY" == "1" ]] && printf trusted_local_only || printf hosted_public_join)" \
      --json-ready \
      --run-id "$RUN_ID" \
      --output-dir "$OUT_DIR/runtime" \
      "${STACK_ARGS[@]}" >"$LAUNCH_LOG" 2>&1 &
  else
    "$ROOT_DIR/scripts/run-launcher-stack.sh" --viewer-static-dir "$ROOT_DIR/crates/oasis7_viewer/.viewer-test-dist" \
      --with-llm \
      --agent-decision-source "$([[ "$HOSTED_LOCAL_MOCK" == "1" ]] && printf provider_backed || printf builtin_llm)" \
      --deployment-mode "$([[ "$FULL_GAMEPLAY" == "1" ]] && printf trusted_local_only || printf hosted_public_join)" \
      --json-ready \
      --run-id "$RUN_ID" \
      --output-dir "$OUT_DIR/runtime" >"$LAUNCH_LOG" 2>&1 &
  fi
  LAUNCH_PID=$!
  if (( FULL_GAMEPLAY == 1 )); then
    wait_for_full_gameplay_readiness || exit 1
  fi
  deadline=$((SECONDS + STARTUP_TIMEOUT))
  while (( SECONDS < deadline )); do
    GAME_URL="$(python3 - "$LAUNCH_LOG" <<'PY'
import pathlib
import re
import sys
path = pathlib.Path(sys.argv[1])
if path.exists():
    for line in path.read_text(errors="replace").splitlines():
        match = re.search(r"^- URL: (https?://\S+)", line)
        if match:
            print(match.group(1))
            raise SystemExit
PY
)"
    [[ -n "$GAME_URL" ]] && break
    if ! kill -0 "$LAUNCH_PID" 2>/dev/null; then
      echo "error: launcher exited before publishing a URL" >&2
      exit 1
    fi
    sleep 1
  done
  [[ -n "$GAME_URL" ]] || { echo "error: launcher URL timeout" >&2; exit 1; }
  if (( TEST_LOGIN == 1 )); then
    GAME_URL="$(wait_for_authoritative_hosted_url)" || exit 1
    [[ -n "$GAME_URL" ]] || exit 1
  fi
fi

GAME_URL="$(python3 - "$GAME_URL" "$TEST_LOGIN" <<'PY'
from urllib.parse import parse_qsl, urlencode, urlsplit, urlunsplit
import sys
parts = urlsplit(sys.argv[1])
query = dict(parse_qsl(parts.query, keep_blank_values=True))
query.update({"render_mode": "viewer", "test_api": "1"})
if sys.argv[2] == "1":
    # Explicit local test-login URL contract: hosted_test_login=1.
    query["hosted_test_login"] = "1"
print(urlunsplit((parts.scheme, parts.netloc, parts.path, urlencode(query), parts.fragment)))
PY
)"

require_loopback_url "$GAME_URL"
if (( HEADED == 1 )); then
  configure_loopback_browser_args
fi

ab_open "$SESSION" 1 "$GAME_URL"
if [[ -n "$VIEWPORT_WIDTH" ]]; then
  if ! ab_cmd "$SESSION" set viewport "$VIEWPORT_WIDTH" "$VIEWPORT_HEIGHT" >>"$AB_LOG" 2>&1; then
    echo "error: failed to set requested browser viewport ${VIEWPORT_WIDTH}x${VIEWPORT_HEIGHT} (phase: browser viewport setup; diagnostics: ${OUT_DIR}/agent-browser.log)" >&2
    exit 1
  fi
fi
capture_browser_viewport
wait_for_domcontentloaded
wait_for_webgl2
wait_for_cli_stage "test-api" wait --fn 'typeof window.__AW_TEST__ === "object"'
wait_for_cli_stage "test-login selector" wait --fn "Boolean(document.querySelector('[data-auth-action=\"test-login\"]'))"
run_visible_action "test-login action" click '[data-auth-action="test-login"]'
wait_for_js_true '(() => { const s = window.__AW_TEST__.getState(); const issued = s?.authRegistrationStatus === "issued" && s?.authRuntimeStatus === "issued"; const registered = s?.authRegistrationStatus === "registered" && ["registered", "registered_unbound"].includes(s?.authRuntimeStatus) && s?.authSessionEpoch != null; return s?.authReady === true && (issued || registered); })()' "hosted test-login auth readiness"
registration_required="$(ab_read_eval "$SESSION" '(() => { const s = window.__AW_TEST__.getState(); return s?.authRegistrationStatus === "issued" && s?.authRuntimeStatus === "issued"; })()' 2>/dev/null || true)"
case "$registration_required" in
  true|\"true\")
    register_hosted_player_session
    ;;
  false|\"false\")
    printf '[action:hosted player session registration action] skipped; test-login already registered the session\n' >>"$AB_LOG"
    ;;
  *)
    capture_failure_diagnostics "hosted player session registration decision"
    echo "error: hosted player session registration state was unavailable (phase: hosted player session registration decision)" >&2
    exit 1
    ;;
esac
wait_for_js_true '(() => { const s = window.__AW_TEST__.getState(); return s?.authReady === true && s?.authRegistrationStatus === "registered" && ["registered", "registered_unbound"].includes(s?.authRuntimeStatus) && s?.authSessionEpoch != null; })()' "hosted player session registration"

AGENT_ID_JSON="$(json_quote "$AGENT_ID")"
maybe_claim_first_agent "$AGENT_ID_JSON"
if (( FIRST_AGENT_CLAIM_PERFORMED == 0 )); then
  run_visible_action "exact agent selection action" click "$AGENT_SELECTOR"
  wait_for_js_true "(() => window.__AW_TEST__.getState()?.selectedId === ${AGENT_ID_JSON})()" "exact agent selection"
fi
maybe_claim_starter_oc "$AGENT_ID_JSON"
maybe_rebind_post_onboarding_session "$AGENT_ID_JSON"

wait_for_js_true "(() => { const s = window.__AW_TEST__.getState(); const p = s?.viewerProtocol || {}; return s?.authReady === true && s?.authRegistrationStatus === \"registered\" && [\"registered\", \"registered_unbound\"].includes(s?.authRuntimeStatus) && s?.authBoundAgentId === ${AGENT_ID_JSON} && s?.authSessionEpoch != null && s?.authBindingEpoch != null && p?.negotiated === true && Array.isArray(p?.capabilities) && p.capabilities.includes(\"prompt_control_result_v1\") && String(p?.authorityEpoch || \"\").length > 0; })()" "auth binding and prompt-result protocol readiness"

run_visible_action "command panel navigation action" click 'a[href="#viewer-details-panel"]'
wait_for_prompt_surface_continuity "$ACTION_TIMEOUT_MS"
run_visible_action "advanced prompt disclosure action" click 'details.command-surface__advanced-details > summary'
wait_for_js_true '(() => { const details = document.querySelector("details.command-surface__advanced-details[open]"); const toggle = details?.querySelector(`[data-prompt-visibility-toggle="1"]`); return Boolean(toggle && !toggle.disabled && toggle.getClientRects().length > 0); })()' "prompt overrides toggle visible and enabled"
run_visible_action "prompt overrides visibility action" click '[data-prompt-visibility-toggle="1"]'
wait_for_js_true '(() => { const s = window.__AW_TEST__.getState(); const panel = document.querySelector(`section#viewer-details-panel[data-viewer-route-panel="command"]`); return s?.promptOverridesVisible === true && Boolean(panel?.querySelector("#prompt-short")) && Boolean(panel?.querySelector("#strong-auth-approval-code")); })()' "prompt overrides visible state and exact prompt DOM"
run_visible_action "strong-auth approval fill action" fill "#strong-auth-approval-code" "$OASIS7_HOSTED_STRONG_AUTH_APPROVAL_CODE"
run_visible_action "short-term goal fill action" fill "#prompt-short" "$PROMPT_GOAL"

before_state="$(state_raw)"
write_safe_state "$before_state" "$OUT_DIR/state-before.json"
before_version="$(json_get "$before_state" selectedPromptVersion)"
if ! [[ "$before_version" =~ ^[0-9]+$ ]]; then
  echo "error: selected prompt version is unavailable before preview" >&2
  exit 1
fi

# All prompt changes below are visible browser actions. The test API is used
# only for the permitted server binding, state readback, and artifact capture.
run_visible_action "preview action" click 'button[data-prompt-action="preview"]'
wait_for_prompt_feedback preview
preview_state="$(state_raw)"
write_safe_state "$preview_state" "$OUT_DIR/state-after-preview.json"

run_visible_action "apply action" click 'button[data-prompt-action="apply"]'
wait_for_prompt_feedback apply
wait_for_apply_version_convergence "$before_version"
apply_state="$(state_raw)"
write_safe_state "$apply_state" "$OUT_DIR/state-after-apply.json"

run_visible_action "rollback target fill action" fill "#prompt-rollback-version" "$before_version"
run_visible_action "rollback action" click 'button[data-prompt-action="rollback"]'
wait_for_prompt_feedback rollback "$before_version"
wait_for_rollback_version_convergence "$before_version"
rollback_state="$(state_raw)"
write_safe_state "$rollback_state" "$OUT_DIR/state-after-rollback.json"
capture_prompt_layout
ab_screenshot "$SESSION" "$OUT_DIR/prompt-control.png" >/dev/null
ab_cmd "$SESSION" snapshot >/dev/null 2>&1 || true
capture_success_browser_diagnostics
quiesce_for_manifest

python3 - "$OUT_DIR" "$CASE_ID" "$AGENT_ID" <<'PY'
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
summary = {
    "acceptanceEligible": True,
    "agentId": sys.argv[3],
    "blockedCases": {
        "AC-PROMPT-007": "requires a real second authorized actor revocation or transfer",
        "AC-PROMPT-009": "requires ordinary and race-path convergence with exactly one authority result",
        "AC-PROMPT-011": "requires reconnect and other-entry observation before persistence or sync claims",
    },
    "caseId": sys.argv[2],
    "evidenceTier": "real_browser_single_actor_strong_auth",
    "note": "Single-actor headed strong-auth preview/apply/rollback evidence. Race, gameplay consequence, reconnect, persistence, and sync claims remain out of scope.",
}
(root / "run-summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
provider_calls_during_verification=true
if (( HOSTED_LOCAL_MOCK == 1 )); then
  # The local Runtime fixture is an in-process bounded provider lane.  It is
  # recorded separately from external provider calls and never counts as real
  # inference or external provider spend.
  provider_calls_during_verification=false
fi
write_manifest "$OUT_DIR" "real_browser_single_actor_strong_auth" true "$provider_calls_during_verification" "$AGENT_ID"
echo "headed single-actor strong-auth prompt flow complete (manifest: $OUT_DIR/artifact-manifest.json)"
exit 0
