#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT_DIR/scripts/cargo-dev-lib.sh"
CONFIG_ARGS=()
BIND_ADDR="127.0.0.1:5841"
PROVIDER_AGENT="letai-local"
PROVIDER_THINKING="off"
PROVIDER_BACKEND="rust-direct-letai"
AUTH_TOKEN=""
PRINT_CONFIG="0"
MODEL_OVERRIDDEN="0"

usage() {
  cat <<'USAGE'
Usage: ./scripts/run-local-letai-provider-bridge.sh [options]

Start the local provider bridge against LetAI's chat-completions API path.
By default the bridge uses the Rust direct LetAI adapter; the legacy Python
provider CLI remains available for targeted compatibility diagnostics.
For normal local real-play, start `run-local-letai-game-test.sh` instead. This
bridge script is the lower-level entrypoint used by that wrapper; starting the
binary directly is for diagnostics only and must explicitly preserve the same
LetAI token, platform user/key, and auto-topup environment.

Options:
  --config <path>             LetAI config file for with-letai-llm-config.sh
  --model <id>                Model override
  --base-url <url>            LetAI chat API base URL override
  --bind <host:port>          Local provider bind address (default: 127.0.0.1:5841)
  --provider-agent <id>       Provider agent id (default: letai-local)
  --provider-thinking <level> Provider thinking level (default: off)
  --provider-backend <name>   rust-direct-letai|legacy-cli (default: rust-direct-letai)
  --auth-token <token>        Optional local bridge bearer token
  --auto-topup-usd <amount>   Auto top up on insufficient quota (default: 0.1)
  --print-config              Print sanitized resolved config and exit
  -h, --help                  Show help

Pair with:
  ./scripts/run-launcher-stack.sh --agent-provider-lane local
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --config|--model|--base-url)
      if [[ $# -lt 2 || -z "${2:-}" ]]; then
        echo "error: $1 requires a value" >&2
        exit 2
      fi
      CONFIG_ARGS+=("$1" "$2")
      if [[ "$1" == "--model" ]]; then
        MODEL_OVERRIDDEN="1"
      fi
      shift 2
      ;;
    --bind)
      BIND_ADDR="${2:-}"
      shift 2
      ;;
    --provider-agent)
      PROVIDER_AGENT="${2:-}"
      shift 2
      ;;
    --provider-thinking)
      PROVIDER_THINKING="${2:-}"
      shift 2
      ;;
    --auth-token)
      AUTH_TOKEN="${2:-}"
      shift 2
      ;;
    --auto-topup-usd)
      export OASIS7_LETAI_AUTO_TOPUP_USD="${2:-}"
      shift 2
      ;;
    --provider-backend)
      PROVIDER_BACKEND="${2:-}"
      shift 2
      ;;
    --print-config)
      PRINT_CONFIG="1"
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

if [[ "$MODEL_OVERRIDDEN" == "0" ]]; then
  DEFAULT_MODEL_ARGS=(--model "${OASIS7_LETAI_CHAT_MODEL:-gpt-5.4}")
  if [[ "${#CONFIG_ARGS[@]}" -gt 0 ]]; then
    CONFIG_ARGS=("${DEFAULT_MODEL_ARGS[@]}" "${CONFIG_ARGS[@]}")
  else
    CONFIG_ARGS=("${DEFAULT_MODEL_ARGS[@]}")
  fi
fi

if [[ -z "$BIND_ADDR" ]]; then
  echo "error: --bind cannot be empty" >&2
  exit 2
fi
if [[ -z "$PROVIDER_AGENT" ]]; then
  echo "error: --provider-agent cannot be empty" >&2
  exit 2
fi
if [[ -z "$PROVIDER_THINKING" ]]; then
  echo "error: --provider-thinking cannot be empty" >&2
  exit 2
fi
case "$PROVIDER_BACKEND" in
  rust-direct-letai|legacy-cli) ;;
  *)
    echo "error: --provider-backend must be rust-direct-letai or legacy-cli" >&2
    exit 2
    ;;
esac

if [[ "$PRINT_CONFIG" == "1" ]]; then
  if [[ "${#CONFIG_ARGS[@]}" -gt 0 ]]; then
    "$ROOT_DIR/scripts/with-letai-llm-config.sh" "${CONFIG_ARGS[@]}" --print-config
  else
    "$ROOT_DIR/scripts/with-letai-llm-config.sh" --print-config
  fi
  printf 'bridge_bind=%s\n' "$BIND_ADDR"
  printf 'provider_backend=%s\n' "$PROVIDER_BACKEND"
  if [[ "$PROVIDER_BACKEND" == "legacy-cli" ]]; then
    printf 'provider_cli=%s\n' "$ROOT_DIR/scripts/provider-remote-https/letai_provider_cli.py"
  fi
  printf 'auth_token_present=%s\n' "$([[ -n "$AUTH_TOKEN" ]] && echo true || echo false)"
  exit 0
fi

export OASIS7_LOCAL_LETAI_PROVIDER_BIND="$BIND_ADDR"
export OASIS7_LOCAL_LETAI_PROVIDER_AGENT="$PROVIDER_AGENT"
export OASIS7_LOCAL_LETAI_PROVIDER_THINKING="$PROVIDER_THINKING"
export OASIS7_LOCAL_LETAI_PROVIDER_BACKEND="$PROVIDER_BACKEND"
export OASIS7_LOCAL_LETAI_PROVIDER_AUTH_TOKEN="$AUTH_TOKEN"
export OASIS7_LOCAL_LETAI_PROVIDER_CLI="$ROOT_DIR/scripts/provider-remote-https/letai_provider_cli.py"
export OASIS7_LOCAL_LETAI_PROVIDER_REPO_ROOT="$ROOT_DIR"
export OASIS7_LOCAL_LETAI_PROVIDER_BRIDGE_BIN="${OASIS7_LOCAL_LETAI_PROVIDER_BRIDGE_BIN:-$(oasis7_cargo_dev_debug_bin_dir "$ROOT_DIR")/oasis7_provider_local_bridge}"

WRAPPER_CMD=("$ROOT_DIR/scripts/with-letai-llm-config.sh")
if [[ "${#CONFIG_ARGS[@]}" -gt 0 ]]; then
  WRAPPER_CMD+=("${CONFIG_ARGS[@]}")
fi

exec "${WRAPPER_CMD[@]}" -- bash -s <<'BASH'
set -euo pipefail
cd "$OASIS7_LOCAL_LETAI_PROVIDER_REPO_ROOT"
source "$OASIS7_LOCAL_LETAI_PROVIDER_REPO_ROOT/scripts/cargo-dev-lib.sh"
export OASIS7_REMOTE_LLM_BASE_URL="$OASIS7_LLM_BASE_URL"
export OASIS7_REMOTE_LLM_API_KEY="$OASIS7_LLM_API_KEY"
export OASIS7_REMOTE_LLM_MODEL="$OASIS7_LLM_MODEL"
export OASIS7_REMOTE_LLM_MAX_OUTPUT_TOKENS="${OASIS7_REMOTE_LLM_MAX_OUTPUT_TOKENS:-256}"
export OASIS7_REMOTE_LLM_TEMPERATURE="${OASIS7_REMOTE_LLM_TEMPERATURE:-0}"
export OASIS7_REMOTE_LLM_STREAM="${OASIS7_REMOTE_LLM_STREAM:-true}"
export OASIS7_REMOTE_LLM_AUTO_TOPUP_USD="${OASIS7_REMOTE_LLM_AUTO_TOPUP_USD:-${OASIS7_LETAI_AUTO_TOPUP_USD:-0.1}}"
export OASIS7_REMOTE_LLM_PLATFORM_KEY="${OASIS7_REMOTE_LLM_PLATFORM_KEY:-}"
export OASIS7_REMOTE_LLM_PLATFORM_USER_ID="${OASIS7_REMOTE_LLM_PLATFORM_USER_ID:-}"
export OASIS7_REMOTE_LLM_PLATFORM_BASE_URL="${OASIS7_REMOTE_LLM_PLATFORM_BASE_URL:-${LETAI_PLATFORM_BASE_URL:-https://api.letai.run}}"

if [[ -x "$OASIS7_LOCAL_LETAI_PROVIDER_BRIDGE_BIN" && "${OASIS7_LOCAL_LETAI_PROVIDER_FORCE_CARGO_RUN:-0}" != "1" ]]; then
  cmd=("$OASIS7_LOCAL_LETAI_PROVIDER_BRIDGE_BIN")
elif oasis7_cargo_dev_use_wrapper; then
  cmd=("$OASIS7_LOCAL_LETAI_PROVIDER_REPO_ROOT/scripts/cargo-dev.sh" run -p oasis7_provider_local_bridge --bin oasis7_provider_local_bridge --)
else
  cmd=(env -u RUSTC_WRAPPER cargo run -p oasis7_provider_local_bridge --bin oasis7_provider_local_bridge --)
fi
bin_mtime=""
bin_sha256=""
if [[ "${cmd[0]}" == "$OASIS7_LOCAL_LETAI_PROVIDER_BRIDGE_BIN" ]]; then
  bin_mtime="$(stat -f '%Sm' -t '%Y-%m-%dT%H:%M:%S%z' "${cmd[0]}" 2>/dev/null || stat -c '%y' "${cmd[0]}" 2>/dev/null || true)"
  bin_sha256="$(shasum -a 256 "${cmd[0]}" 2>/dev/null | awk '{print $1}')"
fi
printf 'local_letai_provider_bridge_exec cmd0=%s bin_mtime=%s bin_sha256=%s force_cargo=%s\n' \
  "${cmd[0]}" "$bin_mtime" "$bin_sha256" "${OASIS7_LOCAL_LETAI_PROVIDER_FORCE_CARGO_RUN:-0}" >&2
cmd+=(
  --bind "$OASIS7_LOCAL_LETAI_PROVIDER_BIND"
  --provider-backend "$OASIS7_LOCAL_LETAI_PROVIDER_BACKEND"
  --provider-agent "$OASIS7_LOCAL_LETAI_PROVIDER_AGENT"
  --provider-thinking "$OASIS7_LOCAL_LETAI_PROVIDER_THINKING"
  --gateway-health-url "${OASIS7_LLM_BASE_URL%/}/models"
)

if [[ "$OASIS7_LOCAL_LETAI_PROVIDER_BACKEND" == "legacy-cli" ]]; then
  cmd+=(--provider-cli-bin "$OASIS7_LOCAL_LETAI_PROVIDER_CLI")
fi

if [[ -n "$OASIS7_LOCAL_LETAI_PROVIDER_AUTH_TOKEN" ]]; then
  cmd+=(--auth-token "$OASIS7_LOCAL_LETAI_PROVIDER_AUTH_TOKEN")
fi

exec "${cmd[@]}"
BASH
