#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

tmp_dir="${TMPDIR:-/tmp}/oasis7-local-letai-game-test-cli-$$"
mkdir -p "$tmp_dir"
trap 'rm -rf "$tmp_dir"' EXIT

assert_contains() {
  local path="$1"
  local expected="$2"
  if ! grep -Fq -- "$expected" "$path"; then
    echo "expected $path to contain: $expected" >&2
    echo "--- $path ---" >&2
    sed -n '1,160p' "$path" >&2 || true
    exit 1
  fi
}

assert_not_contains() {
  local path="$1"
  local unexpected="$2"
  if grep -Fq -- "$unexpected" "$path"; then
    echo "expected $path not to contain: $unexpected" >&2
    echo "--- $path ---" >&2
    sed -n '1,200p' "$path" >&2 || true
    exit 1
  fi
}

free_bind_addr() {
  python3 - <<'PY'
from __future__ import annotations

import socket

with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
    sock.bind(("127.0.0.1", 0))
    print(f"127.0.0.1:{sock.getsockname()[1]}")
PY
}

free_port() {
  free_bind_addr | sed 's/^.*://'
}

config_path="$tmp_dir/letai.env"
cat >"$config_path" <<'EOF'
Key: test-secret-key
EOF

fake_bin_dir="$tmp_dir/bin"
mkdir -p "$fake_bin_dir"
for tool in node npm; do
  printf '#!/usr/bin/env bash\nexit 0\n' >"$fake_bin_dir/$tool"
  chmod +x "$fake_bin_dir/$tool"
done
for bin in \
  oasis7_llm_provider_probe \
  oasis7_game_launcher \
  oasis7_viewer_live \
  oasis7_chain_runtime \
  oasis7_provider_local_bridge
do
  printf '#!/usr/bin/env bash\nexit 0\n' >"$fake_bin_dir/$bin"
  chmod +x "$fake_bin_dir/$bin"
done

dist_dir="$tmp_dir/dist"
mkdir -p "$dist_dir"
touch "$dist_dir/index.html"

help_out="$tmp_dir/help.out"
./scripts/run-local-letai-game-test.sh --help >"$help_out"
assert_contains "$help_out" "Daily manual local-only world playtest"
assert_contains "$help_out" "--local-world-playtest"
assert_contains "$help_out" "--help-all"
assert_not_contains "$help_out" "--chat-probe-timeout-ms"
assert_not_contains "$help_out" "--skip-bridge-smoke"

help_all_out="$tmp_dir/help-all.out"
./scripts/run-local-letai-game-test.sh --help-all >"$help_all_out"
assert_contains "$help_all_out" "Advanced/debug/compatibility options"
assert_contains "$help_all_out" "--chat-probe-timeout-ms"
assert_contains "$help_all_out" "--skip-bridge-smoke"

preflight_out="$tmp_dir/preflight.out"
OASIS7_LOCAL_LETAI_SOURCE_BIN_DIR="$fake_bin_dir" \
OASIS7_LOCAL_LETAI_VIEWER_DIST_DIR="$dist_dir" \
PATH="$fake_bin_dir:$PATH" \
	  ./scripts/run-local-letai-game-test.sh \
	    --config "$config_path" \
	    --bind "$(free_bind_addr)" \
	    --no-ensure-token-config \
	    --no-default-proxy \
	    --preflight-only \
	    --startup-profile playtest \
	    --reuse-existing-build \
	    --output-dir "$tmp_dir/out" \
	    -- \
	    --viewer-port "$(free_port)" \
	    --live-bind "$(free_bind_addr)" \
	    --web-bind "$(free_bind_addr)" \
	    --chain-disable \
	    >"$preflight_out"

assert_contains "$preflight_out" "local LetAI game test preflight passed"
assert_contains "$preflight_out" "startup_profile=playtest"
assert_contains "$preflight_out" "provider_smoke_mode=degraded"
assert_contains "$preflight_out" "source_build=reuse-existing"

missing_npm_err="$tmp_dir/missing-npm.err"
set +e
OASIS7_LOCAL_LETAI_TEST_MISSING_NPM=1 \
OASIS7_LOCAL_LETAI_VIEWER_DIST_DIR="$tmp_dir/missing-dist" \
PATH="$fake_bin_dir:$PATH" \
	  ./scripts/run-local-letai-game-test.sh \
	    --config "$config_path" \
	    --bind "$(free_bind_addr)" \
	    --no-ensure-token-config \
	    --no-default-proxy \
	    --preflight-only \
	    --output-dir "$tmp_dir/out-missing-npm" \
	    -- \
	    --viewer-port "$(free_port)" \
	    --live-bind "$(free_bind_addr)" \
	    --web-bind "$(free_bind_addr)" \
	    --chain-disable \
	    >"$tmp_dir/missing-npm.out" 2>"$missing_npm_err"
missing_npm_status=$?
set -e

if [[ "$missing_npm_status" -eq 0 ]]; then
  echo "expected missing npm preflight to fail" >&2
  exit 1
fi
assert_contains "$missing_npm_err" "error: local playtest preflight failed: missing npm and viewer dist"
assert_contains "$missing_npm_err" "Install npm, or build/copy viewer dist"
assert_contains "$missing_npm_err" "rerun with --reuse-existing-build only after a successful source build"

missing_config_err="$tmp_dir/missing-config.err"
set +e
OASIS7_LOCAL_LETAI_SOURCE_BIN_DIR="$fake_bin_dir" \
OASIS7_LOCAL_LETAI_VIEWER_DIST_DIR="$dist_dir" \
PATH="$fake_bin_dir:$PATH" \
  ./scripts/run-local-letai-game-test.sh \
    --config "$tmp_dir/missing-letai.env" \
    --bind "$(free_bind_addr)" \
    --no-ensure-token-config \
    --no-default-proxy \
    --preflight-only \
    --reuse-existing-build \
    --output-dir "$tmp_dir/out-missing-config" \
    -- \
    --viewer-port "$(free_port)" \
    --live-bind "$(free_bind_addr)" \
    --web-bind "$(free_bind_addr)" \
    --chain-disable \
    >"$tmp_dir/missing-config.out" 2>"$missing_config_err"
missing_config_status=$?
set -e

if [[ "$missing_config_status" -eq 0 ]]; then
  echo "expected missing config preflight to fail" >&2
  exit 1
fi
assert_contains "$missing_config_err" "error: local playtest preflight failed: LetAI config not found"
assert_contains "$missing_config_err" "pass --config <path> or set OASIS7_LETAI_CONFIG_PATH"

if command -v lsof >/dev/null 2>&1; then
occupied_port_file="$tmp_dir/occupied-port.txt"
python3 - "$occupied_port_file" <<'PY' &
from __future__ import annotations

import socket
import sys
import time

port_file = sys.argv[1]
sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
sock.bind(("127.0.0.1", 0))
sock.listen(1)
with open(port_file, "w", encoding="utf-8") as handle:
    handle.write(str(sock.getsockname()[1]))
    handle.flush()
time.sleep(30)
PY
occupied_listener_pid=$!
for _ in $(seq 1 50); do
  [[ -s "$occupied_port_file" ]] && break
  sleep 0.1
done
occupied_port="$(cat "$occupied_port_file")"
occupied_err="$tmp_dir/occupied-bind.err"
set +e
OASIS7_LOCAL_LETAI_SOURCE_BIN_DIR="$fake_bin_dir" \
OASIS7_LOCAL_LETAI_VIEWER_DIST_DIR="$dist_dir" \
PATH="$fake_bin_dir:$PATH" \
  ./scripts/run-local-letai-game-test.sh \
    --config "$config_path" \
    --bind "127.0.0.1:$occupied_port" \
    --no-ensure-token-config \
    --no-default-proxy \
    --preflight-only \
    --reuse-existing-build \
    --output-dir "$tmp_dir/out-occupied-bind" \
    -- \
    --viewer-port "$(free_port)" \
    --live-bind "$(free_bind_addr)" \
    --web-bind "$(free_bind_addr)" \
    --chain-disable \
    >"$tmp_dir/occupied-bind.out" 2>"$occupied_err"
occupied_status=$?
set -e
kill "$occupied_listener_pid" >/dev/null 2>&1 || true
wait "$occupied_listener_pid" >/dev/null 2>&1 || true

if [[ "$occupied_status" -eq 0 ]]; then
  echo "expected occupied bind preflight to fail" >&2
  exit 1
fi
assert_contains "$occupied_err" "error: local playtest preflight failed: provider bind address is already in use"
assert_contains "$occupied_err" "Stop the previous local playtest stack, or pass --bind <free host:port>"
else
  echo "skip: occupied bind preflight assertion requires lsof" >&2
fi

missing_node_err="$tmp_dir/missing-node.err"
set +e
OASIS7_LOCAL_LETAI_TEST_MISSING_NODE=1 \
OASIS7_LOCAL_LETAI_SOURCE_BIN_DIR="$fake_bin_dir" \
OASIS7_LOCAL_LETAI_VIEWER_DIST_DIR="$dist_dir" \
PATH="$fake_bin_dir:$PATH" \
	  ./scripts/run-local-letai-game-test.sh \
	    --config "$config_path" \
	    --bind "$(free_bind_addr)" \
	    --no-ensure-token-config \
	    --no-default-proxy \
	    --preflight-only \
	    --reuse-existing-build \
	    --output-dir "$tmp_dir/out-missing-node" \
	    -- \
	    --viewer-port "$(free_port)" \
	    --live-bind "$(free_bind_addr)" \
	    --web-bind "$(free_bind_addr)" \
	    --chain-disable \
	    >"$tmp_dir/missing-node.out" 2>"$missing_node_err"
missing_node_status=$?
set -e

if [[ "$missing_node_status" -eq 0 ]]; then
  echo "expected missing Node.js preflight to fail" >&2
  exit 1
fi
assert_contains "$missing_node_err" "error: local playtest preflight failed: missing Node.js runtime"
assert_contains "$missing_node_err" "Install Node.js and npm, or use a prepared bundle/source build that does not require frontend rebuilds"

missing_build_err="$tmp_dir/missing-build.err"
set +e
OASIS7_LOCAL_LETAI_SOURCE_BIN_DIR="$tmp_dir/missing-bin" \
OASIS7_LOCAL_LETAI_VIEWER_DIST_DIR="$dist_dir" \
PATH="$fake_bin_dir:$PATH" \
	  ./scripts/run-local-letai-game-test.sh \
	    --config "$config_path" \
	    --bind "$(free_bind_addr)" \
	    --no-ensure-token-config \
	    --no-default-proxy \
	    --preflight-only \
	    --reuse-existing-build \
	    --output-dir "$tmp_dir/out-missing-build" \
	    -- \
	    --viewer-port "$(free_port)" \
	    --live-bind "$(free_bind_addr)" \
	    --web-bind "$(free_bind_addr)" \
	    --chain-disable \
	    >"$tmp_dir/missing-build.out" 2>"$missing_build_err"
missing_build_status=$?
set -e

if [[ "$missing_build_status" -eq 0 ]]; then
  echo "expected missing build preflight to fail" >&2
  exit 1
fi
assert_contains "$missing_build_err" "error: local playtest preflight failed: --reuse-existing-build requested but required binaries are missing"
assert_contains "$missing_build_err" "Run without --reuse-existing-build once"

strict_plan="$tmp_dir/strict-plan.out"
OASIS7_LOCAL_LETAI_SOURCE_BIN_DIR="$fake_bin_dir" \
OASIS7_LOCAL_LETAI_VIEWER_DIST_DIR="$dist_dir" \
PATH="$fake_bin_dir:$PATH" \
	  ./scripts/run-local-letai-game-test.sh \
	    --config "$config_path" \
	    --bind "$(free_bind_addr)" \
	    --no-ensure-token-config \
	    --no-default-proxy \
	    --dry-run-launch \
	    --provider-smoke-mode strict \
	    --reuse-existing-build \
	    --output-dir "$tmp_dir/out-strict" \
	    -- \
	    --viewer-port "$(free_port)" \
	    --live-bind "$(free_bind_addr)" \
	    --web-bind "$(free_bind_addr)" \
	    --chain-disable \
	    >"$strict_plan"

assert_contains "$strict_plan" "provider_smoke_mode=strict"
assert_contains "$strict_plan" "bridge_smoke=required"
assert_contains "$strict_plan" "chain_profile=disabled"
if grep -Fq -- "--skip-bridge-smoke" "$strict_plan"; then
  echo "strict provider smoke mode must not skip bridge smoke" >&2
  exit 1
fi
if grep -Fq -- "--skip-llm-provider-preflight" "$strict_plan"; then
  echo "strict provider smoke mode must not skip launcher provider preflight" >&2
  exit 1
fi

degraded_plan="$tmp_dir/degraded-plan.out"
OASIS7_LOCAL_LETAI_SOURCE_BIN_DIR="$fake_bin_dir" \
OASIS7_LOCAL_LETAI_VIEWER_DIST_DIR="$dist_dir" \
PATH="$fake_bin_dir:$PATH" \
	  ./scripts/run-local-letai-game-test.sh \
	    --config "$config_path" \
	    --bind "$(free_bind_addr)" \
	    --no-ensure-token-config \
	    --no-default-proxy \
	    --dry-run-launch \
	    --provider-smoke-mode degraded \
	    --reuse-existing-build \
	    --output-dir "$tmp_dir/out-degraded" \
	    -- \
	    --viewer-port "$(free_port)" \
	    --live-bind "$(free_bind_addr)" \
	    --web-bind "$(free_bind_addr)" \
	    --chain-disable \
	    >"$degraded_plan"

assert_contains "$degraded_plan" "provider_smoke_mode=degraded"
assert_contains "$degraded_plan" "chain_profile=disabled"
assert_contains "$degraded_plan" "bridge_smoke=degrade-on-failure"
assert_contains "$degraded_plan" "degraded startup will continue after provider smoke failure"
assert_contains "$degraded_plan" "--skip-llm-provider-preflight"
assert_contains "$degraded_plan" "OASIS7_RUN_LAUNCHER_STACK_SKIP_SOURCE_BUILD=1"

local_world_dry_run="$tmp_dir/local-world-dry-run.out"
OASIS7_LOCAL_LETAI_SOURCE_BIN_DIR="$fake_bin_dir" \
OASIS7_LOCAL_LETAI_VIEWER_DIST_DIR="$dist_dir" \
PATH="$fake_bin_dir:$PATH" \
  ./scripts/run-local-letai-game-test.sh \
    --config "$config_path" \
    --bind "$(free_bind_addr)" \
    --no-ensure-token-config \
    --no-default-proxy \
    --local-world-playtest \
    --dry-run-launch \
    --output-dir "$tmp_dir/out-local-world-dry-run" \
    -- \
    --viewer-port "$(free_port)" \
    --live-bind "$(free_bind_addr)" \
    --web-bind "$(free_bind_addr)" \
    >"$local_world_dry_run"

assert_contains "$local_world_dry_run" "startup_profile=playtest"
assert_contains "$local_world_dry_run" "provider_smoke_mode=skip"
assert_contains "$local_world_dry_run" "chain_profile=local-standalone"
assert_contains "$local_world_dry_run" "--json-ready"
assert_not_contains "$local_world_dry_run" "local LetAI game test detached"
assert_not_contains "$local_world_dry_run" "supervisor_script="

detached_out="$tmp_dir/detached.out"
detached_dir="$tmp_dir/out-detached"
detached_abs_dir="$(mkdir -p "$detached_dir" && cd "$detached_dir" && pwd)"
OASIS7_LOCAL_LETAI_SOURCE_BIN_DIR="$fake_bin_dir" \
OASIS7_LOCAL_LETAI_VIEWER_DIST_DIR="$dist_dir" \
OASIS7_LOCAL_LETAI_TEST_DETACH_NO_SUBMIT=1 \
PATH="$fake_bin_dir:$PATH" \
	  ./scripts/run-local-letai-game-test.sh \
	    --config "$config_path" \
	    --bind "$(free_bind_addr)" \
	    --no-ensure-token-config \
	    --no-default-proxy \
	    --detach \
	    --provider-smoke-mode degraded \
	    --reuse-existing-build \
	    --output-dir "$detached_dir" \
	    -- \
	    --viewer-port "$(free_port)" \
	    --live-bind "$(free_bind_addr)" \
	    --web-bind "$(free_bind_addr)" \
	    --chain-disable \
	    >"$detached_out"

assert_contains "$detached_out" "local LetAI game test detached"
assert_contains "$detached_out" "supervisor_script=$detached_abs_dir/local-letai-game-test.detached.sh"
assert_contains "$detached_abs_dir/local-letai-game-test.detached.sh" "OASIS7_RUN_LAUNCHER_STACK_SKIP_SOURCE_BUILD=1"
assert_contains "$detached_abs_dir/local-letai-game-test.detached.sh" "--provider-smoke-mode degraded"
assert_contains "$detached_abs_dir/local-letai-game-test.detached.sh" "--reuse-existing-build"
assert_contains "$detached_abs_dir/local-letai-game-test.detached.sh" "--skip-llm-provider-preflight"

echo "local letai game test CLI smoke passed"

# Exercise the actual public-testnet harness entrypoint using a fake checkout
# and local service/network adapters. Never contact the world or a provider.
harness="$tmp_dir/public-testnet-harness"
mkdir -p "$harness/scripts" "$harness/target/debug" "$harness/bin" "$harness/tmp"
cp scripts/run-local-public-testnet-letai-test-environment.sh "$harness/scripts/"
cat >"$harness/scripts/cargo-dev-lib.sh" <<'BASH'
oasis7_cargo_dev_target_dir() { printf '%s/target\n' "$1"; }
BASH
cat >"$harness/bin/lsof" <<'BASH'
#!/usr/bin/env bash
exit 1
BASH
cat >"$harness/bin/curl" <<'BASH'
#!/usr/bin/env bash
printf '%s\n' '{"world_id":"fixture","network_tier":{"tier":"public_testnet","chain_id":"fixture","network_id":"fixture"},"readiness":{"status":"ready"}}'
BASH
cat >"$harness/bin/launchctl" <<'BASH'
#!/usr/bin/env bash
set -euo pipefail
case "$1" in
  remove) exit 0 ;;
  submit)
    while [[ "$1" != "--" ]]; do shift; done
    shift
    unset http_proxy HTTP_PROXY https_proxy HTTPS_PROXY all_proxy ALL_PROXY no_proxy NO_PROXY
    "$@"
    ;;
  *) exit 2 ;;
esac
BASH
cat >"$harness/scripts/run-local-letai-provider-bridge.sh" <<'PY'
#!/usr/bin/env python3
import json, os, stat, sys
from pathlib import Path
args = sys.argv[1:]
path = Path(args[args.index("--config") + 1])
if "token_key=fake-harness-test-token" not in path.read_text():
    raise SystemExit("test fixture token field absent")
mode = stat.S_IMODE(path.stat().st_mode)
if mode != 0o600:
    raise SystemExit("merged config is not private")
with Path(os.environ["HARNESS_RECEIPT"] + ".provider").open("w") as receipt:
    json.dump({"config": str(path), "mode": mode,
               "http_proxy": os.environ.get("http_proxy"),
               "HTTP_PROXY": os.environ.get("HTTP_PROXY"),
               "https_proxy": os.environ.get("https_proxy"),
               "all_proxy": os.environ.get("all_proxy")}, receipt)
PY
cat >"$harness/target/debug/oasis7_newapi_bridge_service" <<'PY'
#!/usr/bin/env python3
import json, os, stat, sys
from pathlib import Path
args = sys.argv[1:]
path = Path(args[args.index("--state-path") + 1])
path.write_text("{}\n")
with Path(os.environ["HARNESS_RECEIPT"] + ".state").open("w") as receipt:
    json.dump({"state": str(path), "mode": stat.S_IMODE(path.stat().st_mode)}, receipt)
PY
chmod +x "$harness"/bin/* "$harness/scripts/run-local-letai-provider-bridge.sh" "$harness/target/debug/oasis7_newapi_bridge_service"
printf 'token_key=fake-harness-test-token\nbase_url=https://test.invalid\n' >"$harness/token.env"
printf 'platform_key=fake-harness-test-platform\n' >"$harness/platform.env"

run_public_fixture() (
  local case_name="$1"
  shift
  # Begin with an ordinary permissive umask so the managed outputs must
  # demonstrate their own privacy rather than relying on this environment.
  umask 022
  unset http_proxy HTTP_PROXY https_proxy HTTPS_PROXY all_proxy ALL_PROXY \
    OASIS7_LOCAL_TEST_PROXY_URL OASIS7_LOCAL_TEST_SOCKS_PROXY_URL \
    OASIS7_LETAI_MERGED_CONFIG OASIS7_NEWAPI_BRIDGE_STATE_PATH
  if [[ "$case_name" == override ]]; then
    export HTTP_PROXY=http://existing-proxy.test.invalid:18080
  fi
  cd "$harness"
  PATH="$harness/bin:$PATH" TMPDIR="$harness/tmp" HARNESS_RECEIPT="$harness/$case_name" \
    OASIS7_NEWAPI_BRIDGE_LETAI_PLATFORM_KEY=fake-harness-test-platform \
    ./scripts/run-local-public-testnet-letai-test-environment.sh \
      --no-build --skip-viewer-live --skip-static-viewer --skip-viewer-first-user-smoke \
      --no-operator-commands --pricing-rules pv-1:100:100000:0 \
      --letai-config "$harness/token.env" --letai-platform-env "$harness/platform.env" \
      --output-dir "$case_name output" "$@" >"$harness/$case_name.out" 2>&1
)

run_public_fixture alpha &
alpha_pid=$!
run_public_fixture beta &
beta_pid=$!
wait "$alpha_pid"
wait "$beta_pid"
# In addition to CLI/env precedence, verify an explicit output can replace a
# previously permissive regular file without retaining its permissive mode.
printf 'old fixture\n' >"$harness/explicit.env"
chmod 644 "$harness/explicit.env"
run_public_fixture override --merged-letai-config explicit.env --newapi-state-path explicit-state.json \
  --proxy http://explicit-proxy.test.invalid:18888 --socks-proxy socks5://explicit-proxy.test.invalid:18889
ln -s token.env "$harness/unsafe-link.env"
if run_public_fixture invalid --merged-letai-config unsafe-link.env; then
  echo "public testnet harness accepted a symlink output" >&2
  exit 1
fi
test -L "$harness/unsafe-link.env"
if run_public_fixture alias --merged-letai-config token.env; then
  echo "public testnet harness attempted to replace an input" >&2
  exit 1
fi
mkdir -p "$harness/loose output/newapi-private"
chmod 755 "$harness/loose output/newapi-private"
if run_public_fixture loose; then
  echo "public testnet harness accepted a public default state directory" >&2
  exit 1
fi
python3 - "$harness" <<'PY'
import json, stat, sys
from pathlib import Path
harness = Path(sys.argv[1]).resolve()
temporary_paths = set()
for name in ("alpha", "beta"):
    provider = json.loads((harness / (name + ".provider")).read_text())
    state = json.loads((harness / (name + ".state")).read_text())
    temporary = Path(provider["config"])
    temporary_paths.add(temporary)
    assert temporary.parent.parent == harness / "tmp"
    assert not temporary.parent.exists(), "default credential directory survived startup"
    assert all(provider[k] is None for k in ("http_proxy", "HTTP_PROXY", "https_proxy", "all_proxy"))
    assert state["state"] == str(harness / (name + " output") / "newapi-private" / "bridge-state.json")
    assert stat.S_IMODE(Path(state["state"]).parent.stat().st_mode) == 0o700
    assert state["mode"] == 0o600
assert len(temporary_paths) == 2
provider = json.loads((harness / "override.provider").read_text())
state = json.loads((harness / "override.state").read_text())
assert provider["config"] == str(harness / "explicit.env")
assert stat.S_IMODE((harness / "explicit.env").stat().st_mode) == 0o600
assert provider["http_proxy"] is None and provider["HTTP_PROXY"] == "http://existing-proxy.test.invalid:18080"
assert provider["https_proxy"] == "http://explicit-proxy.test.invalid:18888"
assert provider["all_proxy"] == "socks5://explicit-proxy.test.invalid:18889"
assert state["state"] == str(harness / "explicit-state.json")
assert state["mode"] == 0o600
assert not (harness / "invalid.provider").exists()
assert not (harness / "alias.provider").exists()
assert (harness / "token.env").read_text() == "token_key=fake-harness-test-token\nbase_url=https://test.invalid\n"
assert not (harness / "loose.state").exists()
PY
for output in "$harness"/*.out; do
  assert_not_contains "$output" fake-harness-test-token
  assert_not_contains "$output" fake-harness-test-platform
done
echo "local public testnet isolation and proxy CLI smoke passed"
