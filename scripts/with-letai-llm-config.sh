#!/usr/bin/env bash
set -euo pipefail

CONFIG_PATH="${OASIS7_LETAI_CONFIG_PATH:-${HOME}/Documents/keys/letai.txt}"
MODEL="${OASIS7_LETAI_MODEL:-${OASIS7_LLM_MODEL:-custom-right-codes/gpt-5.4}}"
BASE_URL="${OASIS7_LETAI_BASE_URL:-${OASIS7_LLM_BASE_URL:-https://api.letai.run/v1}}"
PRINT_CONFIG="0"

usage() {
  cat <<'USAGE'
Usage: ./scripts/with-letai-llm-config.sh [options] -- <command> [args...]

Load a local LetAI credential file into oasis7 LLM env vars, then exec a command.
The credential value is never printed by this wrapper.

Options:
  --config <path>     LetAI config file (default: $OASIS7_LETAI_CONFIG_PATH or $HOME/Documents/keys/letai.txt)
  --model <id>        OASIS7_LLM_MODEL override (default: $OASIS7_LETAI_MODEL, $OASIS7_LLM_MODEL, or custom-right-codes/gpt-5.4)
  --base-url <url>    OASIS7_LLM_BASE_URL override (default: $OASIS7_LETAI_BASE_URL, $OASIS7_LLM_BASE_URL, or https://api.letai.run/v1)
  --print-config      Print sanitized resolved config as JSON and exit
  -h, --help          Show help

Accepted config fields:
  Doc                    optional documentation URL, not used as the model API endpoint
  base_url / url / OASIS7_LLM_BASE_URL
  Key / api_key / token_key / OASIS7_LLM_API_KEY
  platform_key / OASIS7_REMOTE_LLM_PLATFORM_KEY
  platform_user_id / OASIS7_REMOTE_LLM_PLATFORM_USER_ID
  platform_project_id / OASIS7_REMOTE_LLM_PLATFORM_PROJECT_ID

Examples:
  ./scripts/with-letai-llm-config.sh -- ./scripts/check-active-llm-provider.sh --pretty
  ./scripts/with-letai-llm-config.sh -- ./scripts/run-launcher-stack.sh --agent-decision-source builtin_llm
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --config)
      CONFIG_PATH="${2:-}"
      shift 2
      ;;
    --model)
      MODEL="${2:-}"
      shift 2
      ;;
    --base-url)
      BASE_URL="${2:-}"
      shift 2
      ;;
    --print-config)
      PRINT_CONFIG="1"
      shift
      ;;
    --)
      shift
      break
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown option before --: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "$CONFIG_PATH" ]]; then
  echo "error: --config cannot be empty" >&2
  exit 2
fi
if [[ -z "$MODEL" ]]; then
  echo "error: --model cannot be empty" >&2
  exit 2
fi
if [[ -z "$BASE_URL" ]]; then
  echo "error: --base-url cannot be empty" >&2
  exit 2
fi
if [[ ! -f "$CONFIG_PATH" ]]; then
  echo "error: LetAI config file not found: $CONFIG_PATH" >&2
  exit 1
fi

resolved_env="$(
  python3 - "$CONFIG_PATH" "$MODEL" "$BASE_URL" <<'PY'
from __future__ import annotations

import json
import shlex
import sys
from pathlib import Path

path = Path(sys.argv[1])
model = sys.argv[2].strip()
default_base_url = sys.argv[3].strip()

aliases = {
    "doc": "doc_url",
    "base_url": "base_url",
    "url": "base_url",
    "oasis7_llm_base_url": "base_url",
    "key": "api_key",
    "api_key": "api_key",
    "token_key": "api_key",
    "auth_token": "api_key",
    "oasis7_llm_api_key": "api_key",
    "platform_key": "platform_key",
    "letai_platform_key": "platform_key",
    "oasis7_remote_llm_platform_key": "platform_key",
    "platform_user_id": "platform_user_id",
    "letai_platform_user_id": "platform_user_id",
    "oasis7_remote_llm_platform_user_id": "platform_user_id",
    "platform_project_id": "platform_project_id",
    "letai_platform_project_id": "platform_project_id",
    "oasis7_remote_llm_platform_project_id": "platform_project_id",
}

parsed: dict[str, str] = {}
for line in path.read_text(errors="replace").splitlines():
    raw = line.strip()
    if not raw or raw.startswith("#"):
        continue
    sep = "=" if "=" in raw else ":" if ":" in raw else None
    if not sep:
        continue
    key, value = raw.split(sep, 1)
    normalized = aliases.get(key.strip().lower())
    if not normalized:
        continue
    value = value.strip().strip("\"").strip("'")
    if value:
        parsed[normalized] = value

base_url = parsed.get("base_url", default_base_url).strip()
doc_url = parsed.get("doc_url", "").strip()
api_key = parsed.get("api_key", "").strip()
platform_key = parsed.get("platform_key", "").strip()
platform_user_id = parsed.get("platform_user_id", "").strip()
platform_project_id = parsed.get("platform_project_id", "").strip()

if not base_url:
    raise SystemExit("error: LetAI base URL is empty")
if not api_key:
    raise SystemExit("error: LetAI config missing Key/api_key field")
if not base_url.startswith(("http://", "https://")):
    raise SystemExit("error: LetAI base URL must start with http:// or https://")

payload = {
    "base_url": base_url,
    "model": model,
    "doc_url_present": bool(doc_url),
    "api_key_len": len(api_key),
    "api_key_present": True,
    "platform_key_len": len(platform_key),
    "platform_key_present": bool(platform_key),
    "platform_user_id_len": len(platform_user_id),
    "platform_user_id_present": bool(platform_user_id),
    "platform_project_id_len": len(platform_project_id),
    "platform_project_id_present": bool(platform_project_id),
}

print("export OASIS7_LLM_BASE_URL=" + shlex.quote(base_url))
print("export OASIS7_LLM_API_KEY=" + shlex.quote(api_key))
print("export OASIS7_LLM_MODEL=" + shlex.quote(model))
if platform_key:
    print("export OASIS7_REMOTE_LLM_PLATFORM_KEY=" + shlex.quote(platform_key))
if platform_user_id:
    print("export OASIS7_REMOTE_LLM_PLATFORM_USER_ID=" + shlex.quote(platform_user_id))
if platform_project_id:
    print("export OASIS7_REMOTE_LLM_PLATFORM_PROJECT_ID=" + shlex.quote(platform_project_id))
print("export OASIS7_LETAI_SANITIZED_CONFIG_JSON=" + shlex.quote(json.dumps(payload, ensure_ascii=False, sort_keys=True)))
PY
)"

eval "$resolved_env"

if [[ "$PRINT_CONFIG" == "1" ]]; then
  printf '%s\n' "$OASIS7_LETAI_SANITIZED_CONFIG_JSON"
  exit 0
fi

if [[ $# -eq 0 ]]; then
  echo "error: missing command after --" >&2
  usage >&2
  exit 2
fi

exec "$@"
