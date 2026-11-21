#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

tmp_dir="${TMPDIR:-/tmp}/oasis7-run-launcher-stack-local-mock-$$"
mkdir -p "$tmp_dir"
trap 'rm -rf "$tmp_dir"' EXIT

config_json="$tmp_dir/provider-config.json"

./scripts/run-launcher-stack.sh \
  --deployment-mode trusted_local_only \
  --allow-trusted-local-playtest \
  --agent-provider-lane local-mock \
  --provider-bootstrap-authority "$tmp_dir/provider-authority-one.json" \
  --provider-bootstrap-authority "$tmp_dir/provider authority-two.json" \
  --print-agent-provider-config \
  >"$config_json"

python3 - "$config_json" <<'PY'
import json
import sys
from pathlib import Path

payload = json.loads(Path(sys.argv[1]).read_text())
assert payload["deployment_mode"] == "trusted_local_only"
assert payload["allow_trusted_local_playtest"] == "1"
assert payload["agent_decision_source"] == "provider_backed"
assert payload["agent_provider_lane"] == "local-mock"
assert payload["agent_provider_backend"] == "provider_local_mock"
assert payload["agent_provider_contract"] == "worldsim_provider_v1"
assert payload["agent_provider_transport"] == "loopback_http"
assert payload["agent_provider_url"] == "http://127.0.0.1:5841"
assert payload["agent_provider_profile"] == "oasis7_p0_low_freq_npc"
assert payload["provider_bootstrap_authority_count"] == "2"
assert payload["major_world_event_visibility"] == "unknown"
assert payload["chain_link_policy"] == "shadow"
assert payload["agent_chat_echo"] == "0"
PY

./scripts/run-launcher-stack.sh \
  --deployment-mode trusted_local_only \
  --allow-trusted-local-playtest \
  --agent-provider-lane local-mock \
  --major-world-event-visibility restricted \
  --print-agent-provider-config \
  >"$config_json"

python3 - "$config_json" <<'PY'
import json
import sys
from pathlib import Path

payload = json.loads(Path(sys.argv[1]).read_text())
assert payload["major_world_event_visibility"] == "restricted"
assert payload["provider_bootstrap_authority_count"] == "0"
PY

invalid_error="$tmp_dir/invalid-visibility.stderr"
if ./scripts/run-launcher-stack.sh \
  --agent-provider-lane local-mock \
  --major-world-event-visibility invalid \
  --print-agent-provider-config \
  >"$config_json" 2>"$invalid_error"; then
  echo "invalid visibility policy unexpectedly succeeded" >&2
  exit 1
fi
grep -Fq -- "--major-world-event-visibility must be one of" "$invalid_error"

OASIS7_RUNTIME_AGENT_CHAT_ECHO=1 ./scripts/run-launcher-stack.sh \
  --deployment-mode trusted_local_only \
  --allow-trusted-local-playtest \
  --agent-provider-lane local-mock \
  --print-agent-provider-config \
  >"$config_json"

python3 - "$config_json" <<'PY'
import json
import sys
from pathlib import Path

payload = json.loads(Path(sys.argv[1]).read_text())
assert payload["agent_chat_echo"] == "1"
PY

# The ready-message heredoc runs under set -u.  Keep the copied agent-browser
# example's command substitution and runtime variables literal until the user
# pastes the example into their own shell.
launcher_example="$tmp_dir/agent-browser-example.txt"
sed -n '/^agent-browser example:/,/^Press Ctrl+C to stop launcher process\./p' \
  ./scripts/run-launcher-stack.sh >"$launcher_example"
grep -Fq 'AB_SESSION="\$(agent-browser session id --scope worktree --prefix launcher)"' \
  "$launcher_example"
grep -Fq 'agent-browser --session "\$AB_SESSION" --headed open "\$GAME_URL"' \
  "$launcher_example"
grep -Fq 'agent-browser --session "\$AB_SESSION" wait --load domcontentloaded' \
  "$launcher_example"
