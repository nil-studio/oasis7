#!/usr/bin/env bash
# Only the release Web QA consumer uses this explicit test-tier signed backend.
release_web_fixture_pid=""
release_web_fixture_dir=""

release_web_check_process() {
  local pid=$1 label=$2 log=$3 status
  [[ -z "$pid" ]] && return 0
  if kill -0 "$pid" 2>/dev/null; then return 0; fi
  if wait "$pid"; then status=0; else status=$?; fi
  echo "error: $label process exited: status=$status" >&2
  [[ ! -f "$log" ]] || tail -n 120 "$log" >&2
  return 1
}

release_web_check_startup() {
  release_web_check_process "${launcher_pid:-${stack_pid:-}}" launcher "${launcher_log:-${run_game_test_log:-/dev/null}}" || return 1
  release_web_check_process "$release_web_fixture_pid" fixture "${release_web_fixture_log:-/dev/null}"
}

release_web_fixture_build_viewer() {
  local log=$1 wasm_bindgen_bin
  source "$repo_root/scripts/viewer-dependency-preflight.sh"
  if ! (
    viewer_dependency_preflight "$repo_root" build || exit 1
    wasm_bindgen_bin=$("$repo_root/scripts/ensure-wasm-bindgen-cli.sh" --print-bin) || exit 1
    WASM_BINDGEN_BIN="$wasm_bindgen_bin" npm --prefix "$repo_root/crates/oasis7_viewer" run build:viewer:visual-test || exit 1
    source "$repo_root/scripts/viewer-web-dist-contract.sh"
    local test_dist="$repo_root/crates/oasis7_viewer/.viewer-test-dist" source_path destination_path
    while read -r source_path destination_path; do
      if [[ "$destination_path" == "index.html" ]]; then
        cp "$test_dist/$source_path" "$test_dist/$destination_path" || exit 1
      fi
    done < <(viewer_web_dist_contract_pairs)
    [[ -f "$test_dist/index.html" ]] || exit 1
  ) >"$log" 2>&1; then
    echo 'error: release Web visual-test artifact build failed' >&2
    tail -n 120 "$log" >&2
    return 1
  fi
}

release_web_fixture_build() {
  local manifest=$1
  oasis7_cargo_dev test -p oasis7_chain_runtime --lib --features test_tier_required --no-run --message-format=json >"$manifest" 2>"$manifest.log" || {
    tail -n 120 "$manifest.log" >&2
    return 1
  }
  release_web_fixture_bin=$(python3 - "$manifest" <<'CODE'
import json, sys
executables = []
for line in open(sys.argv[1]):
    try: item = json.loads(line)
    except ValueError: continue
    if item.get('reason') == 'compiler-artifact' and item.get('target', {}).get('name') == 'oasis7_chain_runtime' and item.get('profile', {}).get('test') and item.get('executable'):
        executables.append(item['executable'])
if len(executables) != 1: raise SystemExit('error: expected one actual chain runtime test executable')
print(executables[0])
CODE
  ) || return 1
}

release_web_fixture_start() {
  local out_dir=$1 step
  unset OASIS7_WORLD_SERVICE_AGENT_PRIVATE_KEY OASIS7_WORLD_SERVICE_AGENT_DELEGATION_GENERATION
  release_web_fixture_log="$out_dir/signed-world-service-fixture.log"
  release_web_fixture_dir=$(mktemp -d "${TMPDIR:-/tmp}/oasis7-release-web.XXXXXX")
  chmod 700 "$release_web_fixture_dir"
  export OASIS7_RELEASE_WEB_FIXTURE_CONFIG="$release_web_fixture_dir/config.json"
  export OASIS7_RELEASE_WEB_FIXTURE_STOP="$release_web_fixture_dir/stop"
  export OASIS7_RELEASE_WEB_FIXTURE_TIMEOUT_SECS=900
  local exact_test
  exact_test=$("$release_web_fixture_bin" --list | sed -n 's/^\(.*qa_conformance::release_web_fixture::serve_release_web_fixture\): test$/\1/p')
  [[ -n "$exact_test" ]] || { echo 'error: release Web fixture test missing' >&2; return 1; }
  python3 "$repo_root/scripts/release-web-fixture-process.py" "$OASIS7_RELEASE_WEB_FIXTURE_STOP" 910 \
    "$release_web_fixture_bin" "$exact_test" --exact --ignored --nocapture >"$release_web_fixture_log" 2>&1 &
  release_web_fixture_pid=$!
  for ((step = 0; step < 60; step++)); do
    release_web_check_process "$release_web_fixture_pid" fixture "$release_web_fixture_log" || return 1
    [[ ! -f "$OASIS7_RELEASE_WEB_FIXTURE_CONFIG" ]] || break
    sleep 1
  done
  [[ -f "$OASIS7_RELEASE_WEB_FIXTURE_CONFIG" ]] || { echo 'error: signed fixture configuration deadline exceeded' >&2; return 1; }
  release_web_fixture_import "$OASIS7_RELEASE_WEB_FIXTURE_CONFIG" || return 1
  echo 'release Web backend: isolated ordinary-scope signed HTTP QA fixture'
}

release_web_fixture_import() {
  unset OASIS7_WORLD_SERVICE_AGENT_PRIVATE_KEY OASIS7_WORLD_SERVICE_AGENT_DELEGATION_GENERATION
  local config_lines
  config_lines=$(python3 - "$1" <<'CODE'
import json, os, pathlib, stat, sys
p = pathlib.Path(sys.argv[1])
if p.is_symlink() or stat.S_IMODE(p.stat().st_mode) != 0o600: raise SystemExit('error: fixture configuration must be a private regular file')
if not p.is_file(): raise SystemExit('error: fixture configuration must be a regular file')
names = { 'OASIS7_WORLD_SERVICE_' + name for name in ('ENDPOINT', 'PUBLIC_KEY', 'WORLD_ID', 'GENESIS_DIGEST', 'SCOPE', 'READ_PRIVATE_KEY') }
data = json.loads(p.read_text())
if set(data) != names or any(not isinstance(v, str) or not v or '\n' in v or '\r' in v for v in data.values()): raise SystemExit('error: invalid signed fixture configuration')
if data['OASIS7_WORLD_SERVICE_SCOPE'] != 'public': raise SystemExit('error: fixture requires ordinary public scope')
from urllib.parse import urlparse
endpoint = urlparse(data['OASIS7_WORLD_SERVICE_ENDPOINT'])
if endpoint.scheme != 'http' or endpoint.hostname not in ('127.0.0.1', '::1') or not endpoint.port: raise SystemExit('error: fixture must bind loopback HTTP')
for key in sorted(names): print(key + '=' + data[key])
CODE
  ) || return 1
  while IFS= read -r entry; do export "$entry"; done <<< "$config_lines"
}

release_web_fixture_stop() {
  if [[ -n "$release_web_fixture_pid" ]]; then
    [[ -z "$release_web_fixture_dir" ]] || touch "$release_web_fixture_dir/stop"
    # Supervisor bounds graceful shutdown, TERM, and KILL; never unbounded wait.
    local step
    for ((step = 0; step < 100; step++)); do
      kill -0 "$release_web_fixture_pid" 2>/dev/null || break
      sleep .1
    done
    if kill -0 "$release_web_fixture_pid" 2>/dev/null && [[ -f "$release_web_fixture_dir/stop.pid" ]]; then
      local owned_group
      owned_group=$(cat "$release_web_fixture_dir/stop.pid")
      [[ ! "$owned_group" =~ ^[0-9]+$ ]] || kill -KILL -- "-$owned_group" 2>/dev/null || true
    fi
    kill -KILL "$release_web_fixture_pid" 2>/dev/null || true
    wait "$release_web_fixture_pid" 2>/dev/null || true
    release_web_fixture_pid=""
  fi
  [[ -z "$release_web_fixture_dir" ]] || rm -rf "$release_web_fixture_dir"
  release_web_fixture_dir=""
}
