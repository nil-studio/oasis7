#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

tmp_dir="${TMPDIR:-/tmp}/oasis7-run-launcher-stack-hosted-local-mock-funding-$$"
mkdir -p "$tmp_dir"
trap 'rm -rf "$tmp_dir"' EXIT

authority="$tmp_dir/local-test-provider-authority.json"
wasm="$tmp_dir/local-test-provider.wasm"
metadata="$tmp_dir/local-test-provider.metadata.json"
config_json="$tmp_dir/provider-config.json"
printf '{}\n' >"$authority"
printf 'wasm-fixture\n' >"$wasm"
printf '{"source_hash":"%064d","build_manifest_hash":"%064d"}\n' 0 0 >"$metadata"

base_args=(
  --deployment-mode hosted_public_join
  --chain-enable
  --chain-local-standalone-test
  --local-test-provider-authority "$authority"
  --local-test-provider-wasm "$wasm"
  --local-test-provider-metadata "$metadata"
  --local-test-provider-agent-id starter-agent-0
  --local-test-provider-owner-binding local-test-owner-0
  --local-test-provider-finality-block-hash "blake3:$(printf '0%.0s' {1..64})"
  --local-test-provider-session-mode hosted_public_join
  --agent-decision-source provider_backed
  --agent-provider-lane local-mock
  --agent-provider-transport loopback_http
  --agent-provider-url http://127.0.0.1:5841
  --agent-execution-lane player_parity
  --print-agent-provider-config
)

OASIS7_AGENT_PROVIDER_BACKEND=provider_local_mock \
OASIS7_AGENT_PROVIDER_CONTRACT=worldsim_provider_v1 \
  ./scripts/run-launcher-stack.sh "${base_args[@]}" >"$config_json"
python3 - "$config_json" <<'PY'
import json
import sys
from pathlib import Path

payload = json.loads(Path(sys.argv[1]).read_text())
assert payload["deployment_mode"] == "hosted_public_join"
assert payload["agent_decision_source"] == "provider_backed"
assert payload["agent_provider_lane"] == "local-mock"
assert payload["agent_provider_backend"] == "provider_local_mock"
assert payload["agent_provider_contract"] == "worldsim_provider_v1"
assert payload["agent_provider_transport"] == "loopback_http"
assert payload["agent_provider_url"] == "http://127.0.0.1:5841"
assert payload["agent_execution_lane"] == "player_parity"
assert payload["local_test_provider_setup_enabled"] == "1"
assert payload["local_test_provider_admission"] == "hosted_local_mock_test_tier"
assert payload["local_test_provider_session_mode"] == "hosted_public_join"
PY

expect_failure() {
  local label="$1"
  local expected="$2"
  shift 2
  local stderr_path="$tmp_dir/$label.stderr"
  if ./scripts/run-launcher-stack.sh "$@" >"$config_json" 2>"$stderr_path"; then
    echo "$label unexpectedly succeeded" >&2
    exit 1
  fi
  grep -Fq -- "$expected" "$stderr_path"
}

chain_disabled_args=("${base_args[@]}")
chain_disabled_args[2]=--chain-disable
expect_failure chain-disabled "--chain-enable" "${chain_disabled_args[@]}"

standalone_missing_args=("${base_args[@]}")
filtered_standalone_args=()
for arg in "${standalone_missing_args[@]}"; do
  [[ "$arg" == "--chain-local-standalone-test" ]] && continue
  filtered_standalone_args+=("$arg")
done
standalone_missing_args=("${filtered_standalone_args[@]}")
expect_failure standalone-missing "--chain-local-standalone-test" "${standalone_missing_args[@]}"

wrong_contract_args=("${base_args[@]}")
if OASIS7_AGENT_PROVIDER_BACKEND=provider_local_mock \
  OASIS7_AGENT_PROVIDER_CONTRACT=wrong_contract_v1 \
  ./scripts/run-launcher-stack.sh "${wrong_contract_args[@]}" \
  >"$config_json" 2>"$tmp_dir/wrong-contract.stderr"; then
  echo "wrong-contract unexpectedly succeeded" >&2
  exit 1
fi
grep -Fq -- "exact test-tier HostedPublicJoin" "$tmp_dir/wrong-contract.stderr"

non_loopback_args=("${base_args[@]}")
for index in "${!non_loopback_args[@]}"; do
  if [[ "${non_loopback_args[$index]}" == "http://127.0.0.1:5841" ]]; then
    non_loopback_args[$index]=https://provider.example:5841
  fi
done
if OASIS7_AGENT_PROVIDER_BACKEND=provider_local_mock \
  OASIS7_AGENT_PROVIDER_CONTRACT=worldsim_provider_v1 \
  ./scripts/run-launcher-stack.sh "${non_loopback_args[@]}" \
  >"$config_json" 2>"$tmp_dir/non-loopback.stderr"; then
  echo "non-loopback unexpectedly succeeded" >&2
  exit 1
fi
grep -Fq -- "exact test-tier HostedPublicJoin" "$tmp_dir/non-loopback.stderr"

builtin_args=(
  --deployment-mode trusted_local_only
  --allow-trusted-local-playtest
  --chain-enable
  --chain-local-standalone-test
  --local-test-provider-authority "$authority"
  --local-test-provider-wasm "$wasm"
  --local-test-provider-metadata "$metadata"
  --local-test-provider-finality-block-hash "blake3:$(printf '0%.0s' {1..64})"
  --local-test-provider-session-mode hosted_public_join
  --agent-decision-source builtin_llm
  --print-agent-provider-config
)
./scripts/run-launcher-stack.sh "${builtin_args[@]}" >"$config_json"
python3 - "$config_json" <<'PY'
import json
import sys
from pathlib import Path

payload = json.loads(Path(sys.argv[1]).read_text())
assert payload["agent_decision_source"] == "builtin_llm"
assert payload["local_test_provider_admission"] == "builtin_llm_dev_local"
PY

# Exercise the real source-mode launcher up to its Cargo call. The fake build
# records argv and stops before any binaries or services are started.
mkdir -p "$tmp_dir/fake-bin" "$tmp_dir/static"
cat >"$tmp_dir/fake-bin/cargo" <<'SH'
#!/usr/bin/env bash
case "$1" in
  metadata) python3 -c 'import json, os; print(json.dumps({"target_directory": os.environ["FIXTURE_TARGET_DIR"]}))' ;;
  build) printf '%s\0' "$@" >"$FIXTURE_CARGO_ARGS"; exit 47 ;;
  *) exit 48 ;;
esac
SH
# No listener will be started; avoid depending on host port availability.
printf '#!/bin/sh\nexit 1\n' >"$tmp_dir/fake-bin/lsof"
chmod +x "$tmp_dir/fake-bin/"*

capture_build() {
  local label="$1" launcher_bash="$2" expectation="$3" status=0
  shift 3
  local args_file="$tmp_dir/$label.cargo-args"
  PATH="$tmp_dir/fake-bin:$PATH" \
    OASIS7_FORCE_RAW_CARGO=1 \
    OASIS7_RUN_LAUNCHER_STACK_SKIP_SOURCE_BUILD=0 \
    OASIS7_AGENT_PROVIDER_BACKEND=provider_local_mock \
    OASIS7_AGENT_PROVIDER_CONTRACT=worldsim_provider_v1 \
    FIXTURE_TARGET_DIR="$tmp_dir/target" FIXTURE_CARGO_ARGS="$args_file" \
    "$launcher_bash" ./scripts/run-launcher-stack.sh \
      --viewer-static-dir "$tmp_dir/static" --output-dir "$tmp_dir/$label" \
      "$@" >"$tmp_dir/$label.stdout" 2>"$tmp_dir/$label.stderr" || status=$?
  if [[ "$status" != 47 || ! -s "$args_file" ]]; then
    cat "$tmp_dir/$label.stderr" >&2
    echo "expected $label to reach the fake source-mode Cargo build, got $status" >&2
    return 1
  fi
  python3 - "$args_file" "$expectation" <<'PYBUILD'
import sys
from pathlib import Path

raw = Path(sys.argv[1]).read_bytes()
assert raw.endswith(b"\0"), raw
args = raw[:-1].split(b"\0")
assert args[0] == b"build", args
positions = [i for i, arg in enumerate(args) if arg == b"--features"]
if sys.argv[2] == "test-tier":
    assert len(positions) == 1, args
    assert args[positions[0] + 1] == b"test_tier_required", args
else:
    assert not positions and b"test_tier_required" not in args, args
PYBUILD
}

hosted_build_args=()
for arg in "${base_args[@]}"; do
  [[ "$arg" == "--print-agent-provider-config" ]] || hosted_build_args+=("$arg")
done
capture_build hosted-test-tier "$BASH" test-tier "${hosted_build_args[@]}"

# No provider authority arguments in this ordinary source-mode case. Running
# the real path (rather than demanding that the shell itself fails an empty
# array reproducer) also protects Bash 3.2 on macOS.
ordinary_build_args=(
  --deployment-mode trusted_local_only
  --allow-trusted-local-playtest
  --chain-disable
  --agent-decision-source builtin_llm
)
capture_build ordinary "$BASH" ordinary "${ordinary_build_args[@]}"
if [[ "$OSTYPE" == darwin* && "$BASH" != /bin/bash ]]; then
  capture_build ordinary-macos-system-bash /bin/bash ordinary "${ordinary_build_args[@]}"
fi

echo "run-launcher-stack Hosted local-mock funding contract: passed"
