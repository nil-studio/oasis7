#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

tmp_dir="${TMPDIR:-/tmp}/oasis7-local-letai-provider-test-$$"
mkdir -p "$tmp_dir"
trap 'rm -rf "$tmp_dir"' EXIT

config_path="$tmp_dir/letai.txt"
cat >"$config_path" <<'EOF'
Doc: https://docs.example.test/not-an-api-endpoint
Key: test-secret-key
platform_key = platform-secret-key
platform_user_id = platform-user-1
EOF

check_config="$tmp_dir/check-config.txt"
./scripts/check-letai-chat-completions.sh \
  --config "$config_path" \
  --model test-model \
  --base-url https://api.example.test/v1 \
  --print-config \
  >"$check_config"

bridge_config="$tmp_dir/bridge-config.txt"
./scripts/run-local-letai-provider-bridge.sh \
  --config "$config_path" \
  --model test-model \
  --base-url https://api.example.test/v1 \
  --bind 127.0.0.1:5999 \
  --print-config \
  >"$bridge_config"

legacy_bridge_config="$tmp_dir/legacy-bridge-config.txt"
./scripts/run-local-letai-provider-bridge.sh \
  --config "$config_path" \
  --model test-model \
  --base-url https://api.example.test/v1 \
  --bind 127.0.0.1:5999 \
  --provider-backend legacy-cli \
  --print-config \
  >"$legacy_bridge_config"

fake_bridge="$tmp_dir/fake-provider-bridge"
cat >"$fake_bridge" <<'EOF'
#!/usr/bin/env bash
printf 'explicit-provider-binary-executed\n'
EOF
chmod +x "$fake_bridge"
override_output="$tmp_dir/override-output.txt"
OASIS7_LOCAL_LETAI_PROVIDER_BRIDGE_BIN="$fake_bridge" \
  ./scripts/run-local-letai-provider-bridge.sh \
  --config "$config_path" --model test-model --base-url https://api.example.test/v1 \
  >"$override_output"
[[ "$(tail -n 1 "$override_output")" == "explicit-provider-binary-executed" ]] || {
  echo "error: provider binary override was not executed" >&2
  exit 1
}

mkdir -p "$tmp_dir/bin"
cat >"$tmp_dir/bin/cargo" <<'EOF'
#!/usr/bin/env bash
if [[ "${1:-}" == metadata ]]; then
  printf '{"target_directory":"%s/target"}\n' "$PWD"
  exit 0
fi
printf 'cargo-target=%s\n' "${CARGO_TARGET_DIR:-unset}"
printf 'cargo-arg=%s\n' "$@"
EOF
chmod +x "$tmp_dir/bin/cargo"
forced_output="$tmp_dir/forced-output.txt"
CI= OASIS7_FORCE_RAW_CARGO=0 OASIS7_CARGO_SCCACHE=0 \
  PATH="$tmp_dir/bin:$PATH" OASIS7_LOCAL_LETAI_PROVIDER_FORCE_CARGO_RUN=1 \
  OASIS7_LOCAL_LETAI_PROVIDER_BRIDGE_BIN="$fake_bridge" \
  ./scripts/run-local-letai-provider-bridge.sh \
  --config "$config_path" --model test-model --base-url https://api.example.test/v1 \
  >"$forced_output"
expected_target="${CARGO_TARGET_DIR:-unset}"
rg -Fqx "cargo-target=$expected_target" "$forced_output"
rg -Fqx 'cargo-arg=oasis7_provider_local_bridge' "$forced_output"
rg -Fqx 'cargo-arg=--bind' "$forced_output"

python3 - "$check_config" "$bridge_config" "$legacy_bridge_config" <<'PY'
from __future__ import annotations

import json
import sys
from pathlib import Path

check_text = Path(sys.argv[1]).read_text()
bridge_text = Path(sys.argv[2]).read_text()
legacy_bridge_text = Path(sys.argv[3]).read_text()

payload = json.loads(check_text)
assert payload["base_url"] == "https://api.example.test/v1"
assert payload["model"] == "test-model"
assert payload["api_key_present"] is True
assert payload["api_key_len"] == len("test-secret-key")
assert payload["platform_key_present"] is True
assert payload["platform_user_id_present"] is True
assert "test-secret-key" not in check_text
assert "test-secret-key" not in bridge_text
assert "platform-secret-key" not in check_text
assert "platform-secret-key" not in bridge_text
assert "platform-secret-key" not in legacy_bridge_text
assert "bridge_bind=127.0.0.1:5999" in bridge_text
assert "provider_backend=rust-direct-letai" in bridge_text
assert "provider_cli=" not in bridge_text
assert "auth_token_present=false" in bridge_text
assert "provider_backend=legacy-cli" in legacy_bridge_text
assert "provider_cli=" in legacy_bridge_text
PY

echo "local letai provider scripts config smoke passed"
