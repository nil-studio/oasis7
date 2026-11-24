#!/usr/bin/env bash
set -euo pipefail

CONFIG_PATH="${OASIS7_LETAI_CONFIG_PATH:-${HOME}/Documents/keys/letai.txt}"
OUTPUT_PATH=""
AUTHORITATIVE_MAPPING_PATH=""
MODEL="${OASIS7_LETAI_CHAT_MODEL:-gpt-5.4}"
CHAT_BASE_URL="${OASIS7_LETAI_BASE_URL:-https://api.letai.run/v1}"
PLATFORM_BASE_URL="${LETAI_PLATFORM_BASE_URL:-https://api.letai.run}"
EXTERNAL_USER_ID="${OASIS7_LETAI_LOCAL_EXTERNAL_USER_ID:-oasis7-local-test-${USER:-operator}}"
EXTERNAL_PROJECT_ID="${OASIS7_LETAI_LOCAL_EXTERNAL_PROJECT_ID:-oasis7-local-test-project}"

usage() {
  cat <<'USAGE'
Usage: ./scripts/ensure-letai-local-token-config.sh --out <path> [options]

Normalize a local LetAI config for real local gameplay tests. If the input file
already contains a token_key/api_key, it writes a sanitized equivalent config.
If the input only contains a platform key, it uses LetAI platform APIs to upsert
a stable local test user/project and writes the returned token_key.

Options:
  --config <path>              Source LetAI config (default: $HOME/Documents/keys/letai.txt)
  --authoritative-mapping <path>
                               Mapping config whose explicit project token must match the source
  --out <path>                 Output config containing token_key; written 0600
  --model <id>                 Model for output config (default: gpt-5.4)
  --chat-base-url <url>        Chat API base URL (default: https://api.letai.run/v1)
  --platform-base-url <url>    Platform API base URL (default: https://api.letai.run)
  --external-user-id <id>      Stable local test external user id
  --external-project-id <id>   Stable local test external project id
  -h, --help                   Show help
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --config)
      CONFIG_PATH="${2:-}"
      shift 2
      ;;
    --authoritative-mapping)
      AUTHORITATIVE_MAPPING_PATH="${2:-}"
      shift 2
      ;;
    --out)
      OUTPUT_PATH="${2:-}"
      shift 2
      ;;
    --model)
      MODEL="${2:-}"
      shift 2
      ;;
    --chat-base-url)
      CHAT_BASE_URL="${2:-}"
      shift 2
      ;;
    --platform-base-url)
      PLATFORM_BASE_URL="${2:-}"
      shift 2
      ;;
    --external-user-id)
      EXTERNAL_USER_ID="${2:-}"
      shift 2
      ;;
    --external-project-id)
      EXTERNAL_PROJECT_ID="${2:-}"
      shift 2
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

if [[ -z "$CONFIG_PATH" || -z "$OUTPUT_PATH" || -z "$MODEL" || -z "$CHAT_BASE_URL" || -z "$PLATFORM_BASE_URL" ]]; then
  echo "error: --config, --out, --model, --chat-base-url, and --platform-base-url must be non-empty" >&2
  exit 2
fi
if [[ ! -f "$CONFIG_PATH" ]]; then
  echo "error: LetAI config not found: $CONFIG_PATH" >&2
  exit 2
fi
if [[ -n "$AUTHORITATIVE_MAPPING_PATH" && ! -f "$AUTHORITATIVE_MAPPING_PATH" ]]; then
  echo "error: authoritative LetAI mapping not found: $AUTHORITATIVE_MAPPING_PATH" >&2
  exit 2
fi

mkdir -p "$(dirname "$OUTPUT_PATH")"

python3 - "$CONFIG_PATH" "$OUTPUT_PATH" "$MODEL" "$CHAT_BASE_URL" "$PLATFORM_BASE_URL" "$EXTERNAL_USER_ID" "$EXTERNAL_PROJECT_ID" "$AUTHORITATIVE_MAPPING_PATH" <<'PY'
from __future__ import annotations

import hashlib
import json
import os
import re
import ssl
import sys
import tempfile
import urllib.error
import urllib.request
from pathlib import Path

source = Path(sys.argv[1])
out = Path(sys.argv[2])
model = sys.argv[3].strip()
chat_base_url = sys.argv[4].strip().rstrip("/")
platform_base_url = sys.argv[5].strip().rstrip("/")
external_user_id = sys.argv[6].strip()
external_project_id = sys.argv[7].strip()
authoritative_mapping_path = sys.argv[8].strip()

aliases = {
    "base_url": "chat_base_url",
    "url": "chat_base_url",
    "oasis7_llm_base_url": "chat_base_url",
    "token_key": "api_key",
    "api_key": "api_key",
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

def parse_config(path: Path) -> tuple[dict[str, str], str]:
    parsed: dict[str, str] = {}
    raw_key = ""
    for line in path.read_text(errors="replace").splitlines():
        raw = line.strip()
        if not raw or raw.startswith("#"):
            continue
        sep = "=" if "=" in raw else ":" if ":" in raw else None
        if not sep:
            continue
        key, value = raw.split(sep, 1)
        clean_key = key.strip().lower()
        value = value.strip().strip('"').strip("'")
        if not value:
            continue
        if clean_key == "key":
            raw_key = value
            continue
        normalized = aliases.get(clean_key)
        if normalized:
            parsed[normalized] = value
    return parsed, raw_key


parsed, raw_key = parse_config(source)

if parsed.get("chat_base_url"):
    chat_base_url = parsed["chat_base_url"].rstrip("/")
api_key = parsed.get("api_key", "").strip()
platform_key = parsed.get("platform_key", "").strip()
platform_user_id = parsed.get("platform_user_id", "").strip()
platform_project_id = parsed.get("platform_project_id", "").strip()

if raw_key and not api_key and not platform_key:
    # LetAI platform keys observed in the operator file are longer than project
    # token_key values; keep this heuristic local and never print the value.
    if len(raw_key) >= 50:
        platform_key = raw_key
    else:
        api_key = raw_key

authoritative_mapping_present = bool(authoritative_mapping_path)
authoritative_mapping_match = False
authoritative_project_id = ""
authoritative_user_id = ""
mapping_fingerprint = ""
if authoritative_mapping_present:
    mapping_config, _ = parse_config(Path(authoritative_mapping_path))
    authoritative_token = mapping_config.get("api_key", "").strip()
    authoritative_project_id = mapping_config.get("platform_project_id", "").strip()
    authoritative_user_id = mapping_config.get("platform_user_id", "").strip()
    if not authoritative_token or not authoritative_project_id:
        raise SystemExit(
            "error: authoritative mapping requires explicit token_key/api_key and platform_project_id"
        )
    if not api_key:
        raise SystemExit(
            "error: authoritative mapping requires the selected config to contain token_key/api_key"
        )
    selected_project_id = parsed.get("platform_project_id", "").strip()
    selected_user_id = parsed.get("platform_user_id", "").strip()
    if (
        api_key != authoritative_token
        or (selected_project_id and selected_project_id != authoritative_project_id)
        or (authoritative_user_id and selected_user_id and selected_user_id != authoritative_user_id)
    ):
        raise SystemExit("error: authoritative project token mapping mismatch")
    authoritative_mapping_match = True
    mapping_fingerprint = hashlib.sha256(
        f"{authoritative_project_id}\0{api_key}".encode("utf-8")
    ).hexdigest()[:16]
    if authoritative_user_id:
        platform_user_id = authoritative_user_id
    platform_project_id = authoritative_project_id

if not chat_base_url.startswith(("http://", "https://")):
    raise SystemExit("error: chat base URL must start with http:// or https://")
if not platform_base_url.startswith(("http://", "https://")):
    raise SystemExit("error: platform base URL must start with http:// or https://")

def extract_string(value, keys):
    if isinstance(value, dict):
        for key in keys:
            child = value.get(key)
            if isinstance(child, str) and child.strip():
                return child.strip()
            if isinstance(child, (int, float)):
                return str(child)
        for child in value.values():
            found = extract_string(child, keys)
            if found:
                return found
    elif isinstance(value, list):
        for child in value:
            found = extract_string(child, keys)
            if found:
                return found
    return None

def post_json(path: str, payload: dict) -> dict:
    body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
    request = urllib.request.Request(
        platform_base_url + path,
        data=body,
        headers={
            "authorization": f"Bearer {platform_key}",
            "content-type": "application/json",
            "accept": "application/json",
            "user-agent": "oasis7-local-letai-game-test/1.0",
        },
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=60, context=ssl.create_default_context()) as response:
            text = response.read().decode("utf-8", errors="replace")
    except urllib.error.HTTPError as err:
        _ = err.read()
        raise SystemExit(f"error: LetAI platform request failed: HTTP {err.code}") from None
    except urllib.error.URLError as err:
        raise SystemExit(f"error: LetAI platform request failed: {err.reason}") from None
    if not text.strip():
        return {}
    try:
        return json.loads(text)
    except json.JSONDecodeError as err:
        raise SystemExit(f"error: LetAI platform response was not JSON: {err}") from None

generated = False
if not api_key:
    if not platform_key:
        raise SystemExit("error: config needs token_key/api_key or platform_key")
    user_payload = {
        "external_user_id": external_user_id,
        "external_user_name": "oasis7 local test",
        "email": f"{external_user_id}@local.oasis7.test",
        "metadata": {"source": "oasis7-local-letai-game-test"},
    }
    user_response = post_json("/api/platform/open/users/upsert", user_payload)
    platform_user_id = extract_string(user_response, ["platform_user_id", "user_id", "id"]) or ""
    if not platform_user_id:
        raise SystemExit("error: LetAI users/upsert response missing platform_user_id")
    project_payload = {
        "external_project_id": external_project_id,
        "external_project_name": "oasis7 local test",
        "metadata": {"source": "oasis7-local-letai-game-test"},
    }
    project_response = post_json(
        f"/api/platform/open/users/{platform_user_id}/projects/upsert",
        project_payload,
    )
    platform_project_id = extract_string(project_response, ["platform_project_id", "project_id", "id"]) or ""
    api_key = extract_string(project_response, ["token_key"]) or ""
    if not api_key:
        raise SystemExit("error: LetAI projects/upsert response missing token_key")
    generated = True

lines = [
    "# Generated by scripts/ensure-letai-local-token-config.sh; do not commit.",
    f"base_url = {chat_base_url}",
    f"token_key = {api_key}",
    f"model = {model}",
]
if platform_key:
    lines.append(f"platform_key = {platform_key}")
if platform_user_id:
    lines.append(f"platform_user_id = {platform_user_id}")
if platform_project_id:
    lines.append(f"platform_project_id = {platform_project_id}")

fd, tmp_name = tempfile.mkstemp(prefix=out.name + ".", dir=str(out.parent))
try:
    with os.fdopen(fd, "w") as handle:
        handle.write("\n".join(lines) + "\n")
    os.chmod(tmp_name, 0o600)
    os.replace(tmp_name, out)
finally:
    if os.path.exists(tmp_name):
        os.unlink(tmp_name)

print(json.dumps({
    "ok": True,
    "generated_token": generated,
    "output": str(out),
    "token_key_len": len(api_key),
    "platform_key_present": bool(platform_key),
    "platform_user_id_present": bool(platform_user_id),
    "authoritative_mapping_present": authoritative_mapping_present,
    "authoritative_mapping_match": authoritative_mapping_match,
    "authoritative_project_id_present": bool(authoritative_project_id),
    "mapping_fingerprint": mapping_fingerprint,
}, sort_keys=True))
PY
