#!/usr/bin/env bash
# ci-driver-library: functions-v1
# Sourcing loads definitions only; execute this file to initialize and dispatch CI.

usage() {
  cat <<'USAGE'
Usage: ./scripts/ci-tests.sh [commit|required|full|full-standard|full-extended|full-core|full-support|cache-native] [--repo-root PATH] [--group NAME] [--plan PATH] [--ordinary-partition NAME] [--support-plan-file PATH|--support-plan JSON|--support-profiles JSON] [--support-partition NAME]

  commit        Run explicit local diagnostics; the ordinary pre-commit hook remains a no-op.
  required      Run the explicit heavier required gate for local validation and PR gate.
  full          Run required checks plus all extended feature/integration tests.
  full-standard Run full checks excluding the independent formal-process suites.
  full-extended Run only extensions; hosted full must also run every required group and platform.
  full-core     Run doc/fmt plus the heaviest `oasis7 --tests` full-tier shard.
  full-support  Run the remaining support crates/viewer shard plus `oasis7 --lib --bins`.
  cache-native  Compile missing shared-cache variants without running tests.

Requires Python 3.11 or newer with the standard-library tomllib module.
The runner discovers a supported interpreter on PATH.

Default: none (explicit tier required)
USAGE
}

run() {
  if [[ "$1" == python3 ]]; then
    shift
    set -- "$ci_python" "$@"
  fi
  echo "+ $*"
  "$@"
}

# Observation only, enabled solely by the workspace-support job. Keep stdout
# evidence usable even when ENOSPC prevents writing an artifact. Never wrap the
# test dispatcher in an if/|| context: doing so disables its errexit behavior.
support_capacity_snapshot() {
  [[ "${OASIS7_SUPPORT_CAPACITY:-0}" == 1 ]] || return 0
  if timeout 25s "${ci_python:-python3}" - "$1" <<'PY_CAPACITY'
import datetime
import json
import os
from pathlib import Path
import subprocess
import sys
import time

checkpoint = sys.argv[1][:160]
started = time.monotonic()
deadline = started + 20
root = Path.cwd()
custody = root / 'crates/oasis7_custody_worker'
cargo = Path(os.environ.get('CARGO_HOME', str(Path.home() / '.cargo')))
rustup = Path(os.environ.get('RUSTUP_HOME', str(Path.home() / '.rustup')))
temporary = Path(os.environ.get('RUNNER_TEMP', '/tmp'))


def emit(kind, **fields):
    print('SUPPORT_CAPACITY ' + json.dumps(dict(
        version=1, checkpoint=checkpoint, kind=kind, **fields), sort_keys=True), flush=True)


def path_status(path):
    # Do not traverse a symlink in an allowlisted root or one of its ancestors.
    if any(part.is_symlink() for part in (path, *path.parents)):
        return 'symlink'
    return 'present' if path.is_dir() else 'missing'


emit('identity', utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
     metadata={name: os.environ.get(name, '')[:160] for name in (
         'GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT', 'GITHUB_JOB', 'ImageOS', 'ImageVersion',
         'RUNNER_OS', 'RUNNER_ARCH', 'OASIS7_SUPPORT_SOURCE_SHA',
         'OASIS7_SUPPORT_TESTED_SHA', 'OASIS7_SUPPORT_POLICY_SHA',
         'OASIS7_SUPPORT_RANGE_BASE', 'OASIS7_SUPPORT_JOB_STATUS')},
     cache_family='ci-shared-native-trusted-v4',
     directory_semantics='independent overlapping observations; do not sum; hardlinks counted once per directory')
for label, path in [('workspace', root), ('runner-temp', temporary), ('tmp', Path('/tmp'))]:
    try:
        status = path_status(path)
        if status != 'present':
            emit('filesystem', label=label, status=status)
            continue
        usage = os.statvfs(path)
        emit('filesystem', label=label, status='ok', device=os.stat(path).st_dev,
             total_bytes=usage.f_blocks * usage.f_frsize,
             available_bytes=usage.f_bavail * usage.f_frsize,
             free_bytes=usage.f_bfree * usage.f_frsize,
             total_inodes=usage.f_files, free_inodes=usage.f_ffree,
             available_inodes=usage.f_favail)
    except OSError as error:
        emit('filesystem', label=label, status='error', error=type(error).__name__)
paths = [(name, root / name) for name in (
    'target', 'target/debug', 'target/debug/deps', 'target/debug/build',
    'target/debug/incremental', 'target/release',
    'target/wasm32-unknown-unknown/debug', 'target/wasm32-unknown-unknown/release')]
paths += [('custody/' + name, custody / name) for name in ('build', 'artifacts', 'node_modules')]
paths += [('cargo/' + name, cargo / name) for name in ('registry', 'git', 'bin')]
paths += [('rustup/toolchains', rustup / 'toolchains'), ('runner-temp', temporary), ('tmp', Path('/tmp'))]
for label, path in paths:
    values = {}
    try:
        status = path_status(path)
        if status == 'present':
            for field, options in [('allocated_bytes', ['--block-size=1']),
                                   ('apparent_bytes', ['--bytes']), ('inodes', ['--inodes'])]:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    status = 'timeout'
                    break
                # GNU du does not follow directory symlinks; -x stays on this
                # filesystem. Only aggregate numbers leave this subprocess.
                result = subprocess.run(['du', '-sx', *options, '--', str(path)],
                                        text=True, stdout=subprocess.PIPE,
                                        stderr=subprocess.DEVNULL, timeout=remaining)
                if result.returncode:
                    status = 'error'
                    break
                values[field] = int(result.stdout.split('\t', 1)[0])
            else:
                status = 'ok'
        emit('directory', label=label, status=status, **values)
    except subprocess.TimeoutExpired:
        emit('directory', label=label, status='timeout', **values)
    except (OSError, ValueError) as error:
        emit('directory', label=label, status='error', error=type(error).__name__, **values)
emit('complete', elapsed_seconds=round(time.monotonic() - started, 3))
PY_CAPACITY
  then
    :
  else
    printf 'SUPPORT_CAPACITY_WARNING checkpoint=%s observer_status=%s\n' "$1" "$?"
  fi
  return 0
}

run_cargo() {
  if [[ "${CI_VERBOSE:-}" == "1" ]]; then
    run env -u RUSTC_WRAPPER cargo --verbose "$@"
  else
    run env -u RUSTC_WRAPPER cargo "$@"
  fi
}

run_cargo_clippy() {
  local lint_flags=(
    -D warnings
    -D clippy::correctness
    -D clippy::suspicious
  )
  if [[ "${CI_VERBOSE:-}" == "1" ]]; then
    run env -u RUSTC_WRAPPER cargo clippy --verbose "$@" -- "${lint_flags[@]}"
  else
    run env -u RUSTC_WRAPPER cargo clippy "$@" -- "${lint_flags[@]}"
  fi
}




build_oasis7_viewer_binary() {
  local features="$1"
  local viewer_manifest viewer_binary
  viewer_manifest="$(mktemp)"
  if run_cargo build -p oasis7_viewer_runtime --bin oasis7_viewer_live --features "$features" --message-format=json > "$viewer_manifest"; then
    :
  else
    local status=$?
    rm -f "$viewer_manifest"
    return "$status"
  fi
  if viewer_binary="$("$ci_python" - "$viewer_manifest" <<'PY_VIEWER'
import json, pathlib, sys
artifacts = []
for line in pathlib.Path(sys.argv[1]).read_text().splitlines():
    if not line.startswith('{'):
        continue
    value = json.loads(line)
    if value.get('reason') == 'compiler-artifact' and value.get('target', {}).get('name') == 'oasis7_viewer_live' and value.get('executable'):
        artifacts.append(value['executable'])
assert len(artifacts) == 1, artifacts
assert pathlib.Path(artifacts[0]).is_file(), artifacts
print(artifacts[0])
PY_VIEWER
  )"; then
    :
  else
    local status=$?
    rm -f "$viewer_manifest"
    return "$status"
  fi
  rm -f "$viewer_manifest"
  printf '%s\n' "$viewer_binary"
}

run_oasis7_package_tests() {
  local features="$1"
  shift
  run_cargo test -p oasis7 -p oasis7_simulator -p oasis7_runtime -p oasis7_world_state -p oasis7_world_projection -p oasis7_world_service -p scenario_test_runner -p oasis7_chain_runtime -p oasis7_executable_image -p oasis7_client_cli -p oasis7_protocol_tools -p oasis7_launcher_backend -p oasis7_newapi_bridge_service -p oasis7_ops_tools -p oasis7_provider_local_bridge -p oasis7_provider_tools -p oasis7_viewer_runtime -p oasis7_viewer_codec -p oasis7_observability -p oasis7_hosted_access -p oasis7_letai_credit_adapter -p oasis7_launcher_config -p oasis7_wasm_materializer -p oasis7_game_client -p oasis7_browser_game_client -p oasis7_cas -p oasis7_formal_protocol "$@" --features "$features"
}

run_oasis7_test_tier_with_viewer() {
  local features="$1"
  shift
  if [[ "$#" -eq 0 ]]; then
    set -- --tests
  fi
  local viewer_binary
  viewer_binary="$(build_oasis7_viewer_binary "$features")"
  PRE2_VIEWER_BINARY="$viewer_binary" run_oasis7_package_tests "$features" "$@"
}

run_oasis7_required_tier_tests() {
  local plan_args=() wasm_args=() partition_args=() argument selected
  if [[ -n "${ordinary_partition:-}" && "$ordinary_partition" != all ]]; then
    partition_args=(--ordinary-partition "$ordinary_partition")
  fi
  if [[ -n "${required_plan:-}" ]]; then
    plan_args=(--plan "$required_plan")
    if [[ -n "${ordinary_partition:-}" ]]; then
      run python3 "$driver_dir/ci-test-artifacts.py" required-args --repo-root "$repo_root" "${plan_args[@]}" \
        --phase partition --ordinary-partition "$ordinary_partition" || return
    fi
    selected=$("$ci_python" "$driver_dir/ci-test-artifacts.py" required-args --repo-root "$repo_root" "${plan_args[@]}" --phase wasm) || return
    while IFS= read -r argument; do
      [[ -z "$argument" ]] || wasm_args+=("$argument")
    done <<< "$selected"
    if [[ ${#wasm_args[@]} -gt 0 ]]; then run_cargo check "${wasm_args[@]}"; fi
  else
    run_cargo check -p oasis7_agent_api -p oasis7_simulator -p oasis7_viewer_codec -p oasis7_viewer_runtime --lib --target wasm32-unknown-unknown --locked
  fi
  if [[ "${ordinary_partition:-}" == controls ]]; then return; fi
  if [[ -n "${OASIS7_CI_TEST_BUNDLE:-}" ]]; then
    run python3 "$driver_dir/ci-test-artifacts.py" run --repo-root "$repo_root" --bundle "$OASIS7_CI_TEST_BUNDLE" --suite ordinary "${plan_args[@]}" "${partition_args[@]}"
    return
  fi
  if [[ -n "${required_plan:-}" ]]; then
    local temporary status=0
    temporary=$(mktemp -d)
    if run python3 "$driver_dir/ci-test-artifacts.py" build --repo-root "$repo_root" --scope ordinary --bundle "$temporary/bundle" "${plan_args[@]}" "${partition_args[@]}"; then
      run python3 "$driver_dir/ci-test-artifacts.py" run --repo-root "$repo_root" --bundle "$temporary/bundle" --suite ordinary "${plan_args[@]}" "${partition_args[@]}" || status=$?
    else
      status=$?
    fi
    rm -rf "$temporary"
    return "$status"
  fi
  run_oasis7_test_tier_with_viewer test_tier_required
}

run_scenario_regression_tests() {
  run_cargo test -p oasis7_simulator --test oasis7_init_demo --features test_tier_full oasis7_init_demo_runs_
}

run_oasis7_required_tier_clippy() {
  if [[ -n "${required_plan:-}" ]]; then
    local selected argument args=()
    selected=$("$ci_python" "$driver_dir/ci-test-artifacts.py" required-args --repo-root "$repo_root" --plan "$required_plan" --phase native-commands) || return
    selected=$("$ci_python" -c '
import json, sys
commands = json.loads(sys.argv[1])
if not isinstance(commands, list) or not commands:
    raise ValueError("missing native commands")
for command in commands:
    if (not isinstance(command, list) or not command
            or any(not isinstance(arg, str) or not arg or "\n" in arg or "\r" in arg for arg in command)):
        raise ValueError("invalid native command")
    print("\n".join(command))
    print()
' "$selected") || return
    while IFS= read -r argument; do
      if [[ -n "$argument" ]]; then
        args+=("$argument")
      elif [[ ${#args[@]} -gt 0 ]]; then
        run_cargo_clippy "${args[@]}" || return
        args=()
      fi
    done <<< "$selected"
    if [[ ${#args[@]} -gt 0 ]]; then
      run_cargo_clippy "${args[@]}"
    fi
    return
  fi
  run_cargo_clippy -p oasis7 -p oasis7_simulator -p oasis7_runtime -p oasis7_world_state -p oasis7_world_projection -p oasis7_world_service -p scenario_test_runner -p oasis7_chain_runtime -p oasis7_executable_image -p oasis7_client_cli -p oasis7_protocol_tools -p oasis7_launcher_backend -p oasis7_newapi_bridge_service -p oasis7_ops_tools -p oasis7_provider_local_bridge -p oasis7_provider_tools -p oasis7_viewer_runtime -p oasis7_viewer_codec -p oasis7_observability -p oasis7_hosted_access -p oasis7_letai_credit_adapter -p oasis7_launcher_config -p oasis7_wasm_materializer -p oasis7_game_client -p oasis7_browser_game_client -p oasis7_cas -p oasis7_formal_protocol --tests --features test_tier_required
}

run_oasis7_required_clippy_for_group() {
  local selected_group=oasis7_required
  if [[ "$tier" == required && -n "${required_plan:-}" ]]; then
    selected_group=$("$ci_python" "$driver_dir/ci-test-artifacts.py" required-args --repo-root "$repo_root" \
      --plan "$required_plan" --phase clippy-group) || return
  fi
  case "$selected_group" in
    oasis7_required|rust_baseline) ;;
    *) echo "Unknown ordinary Clippy group: $selected_group" >&2; return 2 ;;
  esac
  if [[ "$selected_group" == "$1" ]]; then
    run_oasis7_required_tier_clippy
  fi
}

run_oasis7_full_tier_tests() {
  run_oasis7_test_tier_with_viewer "test_tier_full,wasmtime,viewer_live_integration"
}

run_oasis7_production_mock_guard() {
  local manifest
  manifest="$(mktemp)"
  if run_cargo build -p oasis7_chain_runtime -p oasis7_launcher_backend --bin oasis7_chain_runtime --bin oasis7_game_launcher --features wasmtime --message-format=json > "$manifest"; then
    :
  else
    local status=$?
    rm -f "$manifest"
    return "$status"
  fi
  if "$ci_python" "$repo_root/scripts/hosted-local-mock-production-guard.py" "$manifest"; then
    rm -f "$manifest"
  else
    local status=$?
    rm -f "$manifest"
    return "$status"
  fi
}

run_oasis7_consensus_tests() {
  run_cargo test -p oasis7_consensus --lib
}

run_oasis7_consensus_clippy() {
  run_cargo_clippy -p oasis7_consensus --lib
}

run_oasis7_distfs_tests() {
  run_cargo test -p oasis7_cas -p oasis7_formal_protocol --lib -- --test-threads=1
  # Keep independent RPC deadline fixtures from contending with other storage fixtures.
  run_cargo test -p oasis7_distfs --lib --bin oasis7_durability_endpoint -- --test-threads=1
}

run_oasis7_distfs_clippy() {
  if [[ "${tier:-}" == required && -n "${required_plan:-}" ]]; then
    local owner
    owner=$("$ci_python" "$driver_dir/ci-test-artifacts.py" required-args --repo-root "$repo_root" \
      --plan "$required_plan" --phase distfs-clippy-group) || return
    case "$owner" in
      rust_baseline) return 0 ;;
      distfs) ;;
      *) echo "Unknown DistFS Clippy group: $owner" >&2; return 2 ;;
    esac
  fi
  run_cargo_clippy -p oasis7_cas -p oasis7_formal_protocol --lib
  run_cargo_clippy -p oasis7_distfs --lib --bin oasis7_durability_endpoint
}

run_oasis7_node_tests() {
  run_cargo test -p oasis7_node --lib
}

run_oasis7_node_clippy() {
  run_cargo_clippy -p oasis7_node --lib
}

run_oasis7_net_tests() {
  run_cargo test -p oasis7_net --lib
}

run_oasis7_net_clippy() {
  run_cargo_clippy -p oasis7_net --lib
}

run_oasis7_net_libp2p_tests() {
  run python3 ./scripts/libp2p-security-source.test.py
  if [[ "$(uname -s)" == Linux ]]; then
    # Unit tests avoid the upstream cdylib's un-hashed libif_watch.rlib
    # overwriting the workspace dependency build in the shared target directory.
    run_cargo test --locked --manifest-path ./vendor-if-watch-3.2.2/Cargo.toml \
      --target-dir ./target --all-features --lib
  fi
  run python3 ./scripts/libp2p-compat.test.py
  run_cargo test --locked --manifest-path ./vendor-libp2p-rendezvous-0.18.1/Cargo.toml \
    --target-dir ./target --lib -- --test-threads=1
  run_cargo test -p oasis7_net --features libp2p --lib
  if [[ "${GITHUB_ACTIONS:-}" == true && "${RUNNER_OS:-}" == Linux ]]; then
    CC_wasm32_unknown_unknown=clang AR_wasm32_unknown_unknown=llvm-ar run_cargo check -p oasis7_net --no-default-features --target wasm32-unknown-unknown --locked
    run env -u RUSTC_WRAPPER CC_wasm32_unknown_unknown=clang AR_wasm32_unknown_unknown=llvm-ar \
      cargo check -p oasis7_node --features libp2p --target wasm32-unknown-unknown --locked
  fi
}

run_oasis7_net_libp2p_clippy() {
  run_cargo_clippy -p oasis7_net --features libp2p --lib
}

run_signed_read_protocol_contract_tests() {
  # Development-only tools; check never regenerates the committed bundle.
  run "$ci_python" crates/oasis7_agent_api/protocol_codegen/generate.py check
  run "$ci_python" -m unittest discover -s crates/oasis7_agent_api/protocol_codegen -p 'test_*.py'
  run npm ci --prefix crates/oasis7_agent_api/protocol_conformance --ignore-scripts
  run env PYTHON="$ci_python" npm test --prefix crates/oasis7_agent_api/protocol_conformance
}

run_worker_wasm_compile_profiles() {
  # Run the trusted planner's exact compile tuples; candidate role flags do not
  # become test-execution evidence and mutually exclusive roles stay separate.
  run "$ci_python" - "$driver_dir/plan-rust-required-scope.py" <<'PY_WORKER_COMPILE'
import importlib.util
import subprocess
import sys
import tomllib
from pathlib import Path
spec = importlib.util.spec_from_file_location('scope', sys.argv[1])
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
manifest = tomllib.loads(Path('crates/oasis7_custody_worker/Cargo.toml').read_text())
module.run_worker_wasm_compile_checks(manifest)
PY_WORKER_COMPILE
}

run_game_client_cli_contracts() {
  run_cargo test -p oasis7_game_client --test dev_test_service_cli --test dev_test_local_transport --test dev_test_local_cli --locked
  if [[ "${GITHUB_ACTIONS:-}" == true ]]; then
    if [[ "${RUNNER_OS:-}" != Linux || "${RUNNER_ENVIRONMENT:-}" != github-hosted ]]; then
      echo "game-client cross-UID fixture requires an isolated GitHub-hosted Linux runner" >&2
      return 2
    fi
    run_cargo build -p oasis7_game_client --bin oasis7_game_client --locked
    # Root timeout owns the entire inherited process group, including the
    # fixture's distinct-UID clients. Failure/cancellation never completes support.
    run sudo -n timeout --signal=TERM --kill-after=5s 60s "$ci_python" \
      crates/oasis7_game_client/tests/fixtures/dev_test_local_linux.py \
      target/debug/oasis7_game_client
  else
    echo "+ cross-UID fixture NOT EXECUTED locally; hosted Linux evidence still required" >&2
  fi
}

run_dev_test_worker_checks() {
  if ! command -v worker-build >/dev/null || [[ "$(worker-build --version)" != "0.7.5" ]]; then
    run_cargo install worker-build --version 0.7.5 --locked
  fi
  run bash crates/oasis7_dev_test_worker/scripts/verify.sh
}

run_oasis7_workspace_support_crate_tests() {
  run_dev_test_worker_checks
  run_signed_read_protocol_contract_tests
  run_cargo test -p oasis7_custody_core --all-targets --locked
  run bash crates/oasis7_custody_worker/scripts/verify.sh --platform; run_worker_wasm_compile_profiles
  run_cargo_clippy -p oasis7_agent_api --all-targets
  run_cargo_clippy -p oasis7_agent_harness --all-targets
  run_cargo_clippy -p oasis7_client_storage -p oasis7_signer_protocol --all-targets
  run_cargo_clippy -p oasis7_browser_game_client --all-targets
  run_cargo_clippy -p oasis7_game_client --all-targets
  run_cargo_clippy -p oasis7_model_provider --all-targets
  run_cargo test -p oasis7_agent_api
  run_cargo test -p oasis7_agent_harness --lib
  run_cargo test -p oasis7_client_storage -p oasis7_signer_protocol --lib
  run_cargo test -p oasis7_game_client --lib
  run_game_client_cli_contracts
  run_cargo test -p oasis7_model_provider --all-targets
  run_cargo test \
    -p oasis7_browser_game_client \
    -p oasis7_client_api \
    -p oasis7_local_signer \
    -p oasis7_launcher_ui \
    -p oasis7_proto \
    -p oasis7_wasm_abi \
    -p oasis7_wasm_build \
    -p oasis7_wasm_router \
    -p oasis7_wasm_sdk \
    -p oasis7_wasm_store \
    -p pixel_world_bridge \
    --lib
  run_cargo test -p oasis7_wasm_executor --features wasmtime --lib
  run_cargo test -p oasis7_secure_envelope -p oasis7_context --all-targets --locked
  run_cargo test -p oasis7_local_signer --test context_cli --locked
  run_cargo test -p oasis7_client_launcher --bin oasis7_client_launcher
  # Native tool tests build the real template with the installed stable WASM target.
  OASIS7_WASM_BUILD_STD=0 run_cargo test -p wasm_build_suite -p wasm_module_observe
}

run_selected_workspace_support() {
  if [[ -z "$support_profiles" ]]; then
    run_oasis7_workspace_support_crate_tests
    return
  fi
  local selected profile expand_targets expanded_features complete_targets
  local -a expanded_args
  selected=$("$ci_python" - "$support_profiles" "$driver_dir" "${support_partition:-all}" <<'PY'
import importlib.util, json, sys
from pathlib import Path
spec = importlib.util.spec_from_file_location('scope', Path(sys.argv[2]) / 'plan-rust-required-scope.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
raw = sys.argv[1]
plan_input = raw.startswith(('plan:', 'plan-file:'))
if plan_input:
    plan = (json.loads(Path(raw[len('plan-file:'):]).read_text(encoding='utf-8'))
            if raw.startswith('plan-file:') else json.loads(raw[5:]))
    if not isinstance(plan, dict) or 'workspace_support' not in plan.get('groups', []):
        raise SystemExit('support job missing from plan')
    if 'workspace_support_profile_version' not in plan:
        # A trusted base planner predating profiles cannot narrow this job.
        profiles = list(module.SUPPORT_PROFILES)
    elif plan['workspace_support_profile_version'] != 1:
        raise SystemExit('unknown support profile plan version')
    else:
        profiles = plan.get('workspace_support_profiles')
else:
    profiles = json.loads(raw)
if (not isinstance(profiles, list) or not profiles or
        any(not isinstance(p, str) for p in profiles) or
        len(set(profiles)) != len(profiles) or
        not set(profiles).issubset(module.SUPPORT_PROFILES)):
    raise SystemExit('invalid or missing workspace support profiles')
# A trusted base planner cannot name newly extracted packages. Full scope
# always expands to the candidate inventory after validating its known input,
# so an additive profile never narrows or breaks the full support job.
if plan_input and plan.get('scope') == 'full':
    profiles = list(module.SUPPORT_PROFILES)
partition = sys.argv[3]
if plan_input and ('workspace_support_partition_version' in plan or 'workspace_support_partitions' in plan):
    if (type(plan.get('workspace_support_partition_version')) is not int
            or plan['workspace_support_partition_version'] != 1
            or plan.get('workspace_support_partitions') != module.support_partitions(profiles)
            or (partition != 'all' and partition not in plan['workspace_support_partitions'])):
        raise SystemExit('invalid complete workspace support partitions')
    if partition != 'all':
        profiles = [profile for profile in profiles
                    if ('custody' if profile == 'oasis7_custody_worker' else 'other') == partition]
elif partition != 'all':
    raise SystemExit('support partition requires a trusted capability')
expanded = plan_input and plan.get('fast_plan_version') == 1 and plan.get('test_policy_changed') is True
if expanded:
    spec = importlib.util.spec_from_file_location('artifacts', Path(sys.argv[2]) / 'ci-test-artifacts.py')
    artifacts = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(artifacts)
    artifacts.ROOT = Path.cwd()
for profile in sorted(profiles):
    features = artifacts.package_features([profile], plan) if expanded else []
    print(profile + '|' + ('true' if expanded else 'false') + '|' + ','.join(features))
PY
  ) || return 2
  while IFS='|' read -r profile expand_targets expanded_features; do
    [[ "${OASIS7_SUPPORT_CAPACITY:-0}" != 1 ]] || support_capacity_snapshot "profile-before:$profile"
    echo "+ support profile: $profile"
    complete_targets=false
    case "$profile" in
      oasis7_agent_api) run_signed_read_protocol_contract_tests; run_cargo_clippy -p "$profile" --all-targets; run_cargo test -p "$profile" ;;
      oasis7_model_provider) run_cargo_clippy -p "$profile" --all-targets; run_cargo test -p "$profile" --all-targets; complete_targets=true ;;
      oasis7_agent_harness|oasis7_browser_game_client) run_cargo_clippy -p "$profile" --all-targets; run_cargo test -p "$profile" --lib ;;
      oasis7_game_client) run_cargo_clippy -p "$profile" --all-targets; run_cargo test -p "$profile" --lib; run_game_client_cli_contracts ;;
      oasis7_dev_test_worker) run_dev_test_worker_checks; complete_targets=true ;;
      oasis7_custody_core) run_cargo test -p "$profile" --all-targets --locked; complete_targets=true ;;
      oasis7_custody_worker) run bash crates/oasis7_custody_worker/scripts/verify.sh --platform; run_worker_wasm_compile_profiles ;;
      oasis7_wasm_executor) run_cargo test -p "$profile" --features wasmtime --lib ;;
      oasis7_client_storage|oasis7_signer_protocol) run_cargo_clippy -p "$profile" --all-targets; run_cargo test -p "$profile" --lib ;;
      oasis7_secure_envelope|oasis7_context) run_cargo test -p "$profile" --all-targets --locked; complete_targets=true ;;
      oasis7_local_signer) run_cargo test -p "$profile" --lib; run_cargo test -p "$profile" --test context_cli --locked ;;
      oasis7_client_launcher) run_cargo test -p "$profile" --bin oasis7_client_launcher ;;
      wasm_build_suite|wasm_module_observe) OASIS7_WASM_BUILD_STD=0 run_cargo test -p "$profile" ;;
      *) run_cargo test -p "$profile" --lib ;;
    esac
    if [[ "$expand_targets" == true && ( -n "$expanded_features" || "$complete_targets" != true ) ]]; then
      # A new target must run even when this package has no tier features.
      # Reuse only actual same-combination all-targets tests, never Clippy.
      expanded_args=()
      if [[ -n "$expanded_features" ]]; then expanded_args=(--features "$expanded_features"); fi
      case "$profile" in
        wasm_build_suite|wasm_module_observe) OASIS7_WASM_BUILD_STD=0 run_cargo test -p "$profile" --all-targets "${expanded_args[@]}" ;;
        *) run_cargo test -p "$profile" --all-targets "${expanded_args[@]}" ;;
      esac
    fi
    [[ "${OASIS7_SUPPORT_CAPACITY:-0}" != 1 ]] || support_capacity_snapshot "profile-after:$profile"
    echo "+ support profile completed: $profile"
  done <<< "$selected"
}

run_rustsec_advisory_check() {
  run ./scripts/check-rustsec-ignore-baseline.test.sh
  run ./scripts/check-rustsec-ignore-baseline.sh
  run ./scripts/ensure-cargo-deny.sh
  run cargo deny --locked --all-features check advisories bans
}

run_oasis7_llm_baseline_fixture_smoke() {
  run ./scripts/llm-baseline-fixture-smoke.sh
}

run_provider_remote_https_smoke() {
  run ./scripts/run-local-letai-game-test.test.sh
  run ./scripts/provider-remote-https/letai-provider-cli.test.sh
  run ./scripts/provider-remote-https/provider-bridge-contract-smoke.test.sh
}

run_doc_checker_contract_tests() {
  run python3 ./scripts/resolve-product-doc-ci-range.test.py
  run python3 ./scripts/doc-governance-scope.test.py
  run python3 ./scripts/document-corpus-inventory-check.test.py
  run python3 ./scripts/product-doc-governance-check.test.py
  run python3 ./scripts/product-doc-content-check.test.py
  run python3 ./scripts/doc-core-content-check.test.py
  run python3 ./scripts/doc-content-warnings.test.py
  run_system_design_traceability_tests
  run bash ./scripts/product-doc-content-callers.test.sh
  run bash ./scripts/doc-governance-check.test.sh
  run bash ./scripts/find-python-with-module.test.sh
}

run_cargo_tooling_baseline_contract_tests() {
  run python3 ./scripts/ci-bin-ownership.test.py
  run bash ./scripts/cargo-dev-windows-toolchain.test.sh
  run bash ./scripts/cargo-dev-worktree-isolation.test.sh
  run bash ./scripts/new-task-worktree.test.sh
  run bash ./scripts/check-launcher-p2p-dependency-surface.test.sh
  run python3 ./scripts/cargo-dev-architecture.test.py
}

run_cargo_tooling_contract_tests() {
  run_cargo_tooling_baseline_contract_tests
  run python3 ./scripts/runtime-module-routing-perf-harness.test.py
  run bash ./scripts/cargo-dev-lib.test.sh
  run bash ./scripts/check-standalone-tool-lockfiles.test.sh
  run ./scripts/check-standalone-tool-lockfiles.sh
}

run_dot_todo_contract_tests() {
  # Registration lands before the feature. Once either file exists, require both.
  if [[ -e scripts/dot-todo.py || -L scripts/dot-todo.py || -e scripts/dot-todo.test.py || -L scripts/dot-todo.test.py ]]; then
    if [[ ! -f scripts/dot-todo.py || ! -f scripts/dot-todo.test.py ]]; then
      echo "dot TODO source and contract test must both be present" >&2
      return 1
    fi
    run python3 ./scripts/dot-todo.test.py
  fi
}

run_ci_wasm_opt_contract_tests() {
  # Register before implementation; a partially introduced pair must fail.
  local path present=0
  local paths=(scripts/install-ci-wasm-opt.py scripts/install-ci-wasm-opt.test.py)
  for path in "${paths[@]}"; do
    if [[ -e "$path" || -L "$path" ]]; then present=1; fi
  done
  if [[ "$present" == 0 ]]; then
    echo "wasm-opt contracts: not introduced; no tests run"
    return 0
  fi
  for path in "${paths[@]}"; do
    if [[ ! -f "$path" || -L "$path" ]]; then
      echo "wasm-opt requires regular source and contract test files: $path" >&2
      return 1
    fi
  done
  run python3 - <<'PY_WASM_OPT'
import importlib.util
import unittest

spec = importlib.util.spec_from_file_location('ci_wasm_opt_contracts', 'scripts/install-ci-wasm-opt.test.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
suite = unittest.defaultTestLoader.loadTestsFromModule(module)
if suite.countTestCases() == 0:
    raise SystemExit('no wasm-opt contract tests')
result = unittest.TextTestRunner(verbosity=2).run(suite)
raise SystemExit(not result.wasSuccessful() or result.testsRun == len(result.skipped))
PY_WASM_OPT
}

run_engineering_telemetry_contract_tests() {
  # Register before the feature lands; a partial installation must fail closed.
  local path present=0
  local paths=(
    scripts/engineering_telemetry.py
    scripts/engineering_telemetry_adapters.py
    scripts/tests/fixtures/engineering-telemetry/input.json
    scripts/tests/test_engineering_telemetry.py
    scripts/tests/test_engineering_telemetry_adapters.py
  )
  for path in "${paths[@]}"; do
    if [[ -e "$path" || -L "$path" ]]; then present=1; fi
  done
  if [[ "$present" == 0 ]]; then
    echo "engineering telemetry contracts: not introduced; no tests run"
    return 0
  fi
  for path in "${paths[@]}"; do
    if [[ ! -f "$path" || -L "$path" ]]; then
      echo "engineering telemetry requires a regular source, fixture and test file: $path" >&2
      return 1
    fi
  done
  run python3 - <<'PY_TELEMETRY'
import unittest

suite = unittest.TestSuite()
for name in ('test_engineering_telemetry.py', 'test_engineering_telemetry_adapters.py'):
    tests = unittest.defaultTestLoader.discover('scripts/tests', pattern=name)
    if tests.countTestCases() == 0:
        raise SystemExit('engineering telemetry: no tests in ' + name)
    suite.addTests(tests)
raise SystemExit(not unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful())
PY_TELEMETRY
}

run_workflow_governance_baseline_contract_tests() {
  run_dot_todo_contract_tests
  run node ./scripts/cache-permission-probe.test.cjs
  run bash ./scripts/plan-wasm-determinism-scope.test.sh
  run bash ./scripts/ensure-wasm-bindgen-cli.test.sh
  run bash ./scripts/rust-full-tier-trunk-prerequisite-contract.test.sh
  run bash ./scripts/worktree-gc-report.test.sh
  run python3 ./scripts/resource-cleanup-executor.test.py
}

run_workflow_governance_operational_contract_tests() {
  run python3 ./scripts/security/codeql-plan.test.py
  run python3 ./scripts/security/codeql-workflow.test.py
  run python3 ./scripts/security/codeql-rust-coverage.test.py
  run python3 ./scripts/security/codeql-rust-model-diagnostic.test.py
  run python3 ./scripts/security/test_codeql_rust_trait_scope_plan.py
  run python3 ./scripts/security/codeql-rust-refactored-source.test.py
  run python3 ./scripts/security/codeql-rust-capacity.test.py
  run python3 ./scripts/security/codeql-rust-native-diagnostic.test.py
  run python3 ./scripts/security/viewer-browser-install-diagnostic.test.py
  run python3 ./scripts/security/codeql-rust-wasm-metadata.test.py
  run python3 ./scripts/security/codeql-rust-wasm-context.test.py
  run python3 ./scripts/security/codeql-rust-qualifier-comparison.test.py
  run python3 ./scripts/security/codeql-rust-candidate-workload.test.py
  run python3 ./scripts/security/codeql-rust-production.test.py
  run python3 ./scripts/security/test_codeql_rust_candidate_profile.py
  run python3 ./scripts/security/codeql-rust-action-deadline.test.py
  run python3 ./scripts/security/codeql-rust-capacity-evidence.test.py
  run python3 ./scripts/security/codeql-rust-capacity-layout.test.py
  run python3 ./scripts/security/test_codeql_rust_evaluator_profile.py
  run python3 ./scripts/security/test_codeql_rust_cfg_graph.py
  run python3 ./scripts/security/test_codeql_rust_callee_diagnostic.py
  run python3 ./scripts/security/codeql-rust-cfg-workflow.test.py
  run python3 ./scripts/security/codeql-health.test.py
  run python3 ./scripts/security/codeql-upload-association.test.py
  run python3 ./scripts/security/codeql-platform-observation.test.py
  run python3 ./scripts/security/codeql-acceptance.test.py
}

run_workflow_governance_remaining_contract_tests() {
  run_ci_wasm_opt_contract_tests
  run_engineering_telemetry_contract_tests
  run_workflow_governance_baseline_contract_tests
  run_workflow_governance_operational_contract_tests
  run python3 ./scripts/package-source-plan.test.py
  run python3 ./scripts/safe-git-archive.test.py
}

run_workflow_governance_contract_tests() {
  run python3 ./scripts/ci-python-control-contracts.py local-governance --repo-root "$repo_root"
}

run_packaging_artifact_contract_tests() {
  run bash ./scripts/build-wasm-module.test.sh
  run python3 ./scripts/package-source-plan.test.py
  run python3 ./scripts/package-release-version.test.py
  run python3 ./scripts/safe-git-archive.test.py
  run bash ./scripts/native-packaging-contract.test.sh
  run bash ./scripts/packaging-artifact-size-contract.test.sh
  run bash ./scripts/package-workflow-cache-reuse-contract.test.sh
  run python3 ./scripts/package-ecs.test.py
  run python3 ./scripts/ecs-preview-release.test.py
  run python3 ./scripts/ecs-preview-update.test.py
  run python3 ./scripts/ecs-preview-install.test.py
  run python3 ./scripts/ecs-preview-compatibility.test.py
  run python3 ./scripts/ci-ecs-cache.test.py
  run python3 ./scripts/package-ecs-resource-monitor.test.py
  run bash ./scripts/copy-viewer-web-dist.test.sh
}

run_packaging_contract_tests() {
  run_packaging_artifact_contract_tests
  run bash ./scripts/release-packages-trunk-cache-contract.test.sh
  run python3 ./scripts/ci-local-signer-tests.py

}

run_operational_identity_contract_tests() {
  run python3 ./scripts/p2p-public-testnet-full-network-clean-room.test.py
  run python3 ./scripts/p2p-public-testnet-full-network-clean-room-adapter.test.py
  run python3 ./scripts/p2p-public-testnet-identity-v2-signing-tool.test.py
  run python3 ./scripts/p2p-public-testnet-identity-v2-cli-bridge.test.py
  run python3 ./scripts/p2p-public-testnet-identity-v2-evidence-aggregate.test.py
  run python3 ./scripts/p2p-public-testnet-peer-registry.test.py
}

run_operational_node_contract_tests() {
  run bash ./scripts/local-letai-provider-bridge.test.sh
  run bash ./scripts/traffic-monitor-summary.test.sh
  run bash ./scripts/p2p-real-env-observability-monitor.test.sh
  run ./scripts/game-world-state-sync-commit-module-required.test.sh
  run ./scripts/state-sync-closure-evidence-template.test.sh
  run ./scripts/s10-five-node-game-soak-summary.test.sh
  run ./scripts/release-gate-bash-preflight.test.sh
  run python3 ./scripts/release-web-startup.test.py
  run bash ./scripts/p2p-public-testnet-local-observer-sync.test.sh
  run bash ./scripts/build-game-launcher-bundle-dev-cache.test.sh
  run bash ./scripts/build-game-launcher-bundle-ops-default.test.sh
  run bash ./scripts/build-game-launcher-bundle-macos-bash3.test.sh
  run bash ./scripts/testnet-packages-linux-bundle-bootstrap-contract.test.sh
  run bash ./scripts/testnet-packages-windows-governed-closure.test.sh
  run_provider_remote_https_smoke
  run bash ./scripts/p2p-public-testnet-bootstrap-fresh-validator-host.test.sh
  run bash ./scripts/p2p-public-testnet-service-readback.test.sh
  run bash ./scripts/p2p-public-testnet-package-node-upgrade.test.sh
  run bash ./scripts/p2p-public-testnet-package-node-operation-transaction.test.sh
  run bash ./scripts/p2p-public-testnet-package-node-upgrade-health.test.sh
  run bash ./scripts/p2p-public-testnet-package-node-upgrade-order.test.sh
  run bash ./scripts/p2p-public-testnet-package-node-upgrade-rollback-contract.test.sh
  run bash ./scripts/p2p-observer-checkpoint-closure-probe.test.sh
  run python3 ./scripts/p2p-observer-checkpoint-closure-probe-safety.test.py
}

run_operational_contract_tests() {
  run python3 ./scripts/oasis7-runtime-finality-baseline.test.py
  run_operational_identity_contract_tests
  run_operational_node_contract_tests
}


run_all_required_gate_capability_contract_tests() {
  run_doc_checker_contract_tests
  run_cargo_tooling_contract_tests
  run_workflow_governance_contract_tests
  run_packaging_contract_tests
  run_operational_contract_tests

  run_codex_agent_config_validation
  run_compile_metrics_contract_tests
  run bash ./scripts/viewer-performance-report-only-contract.test.sh
}

run_provider_bridge_live_gate() {
  run ./scripts/provider-remote-https/provider-bridge-live-gate.sh
}

run_newapi_bridge_service_accounting_tests() {
  run env -u RUSTC_WRAPPER cargo test -p oasis7_newapi_bridge_service --bin oasis7_newapi_bridge_service -- --nocapture
}

run_oasis7_viewer_software_safe_feedback_contract_tests() {
  run ./scripts/viewer-dependency-preflight.test.sh
  viewer_dependency_preflight "$repo_root" test
  run npm --prefix crates/oasis7_viewer run test:frontend-structure
  run npm --prefix crates/oasis7_viewer run test:feedback-contract
  run node crates/oasis7_viewer/scripts/gameplay-attraction-scenario.test.mjs
  run python3 scripts/tests/test_industrial_starter_evidence.py
  run ./scripts/copy-viewer-web-dist.test.sh
  run ./scripts/agent-browser-viewer-dist-freshness-test.sh
  run bash ./scripts/agent-browser-lifecycle-contract.test.sh
  run bash ./scripts/viewer-prompt-control-regression.test.sh
  run bash ./scripts/viewer-prompt-control-race.test.sh
  run bash ./scripts/viewer-prompt-control-race-identity.test.sh
  run node --test crates/oasis7_viewer/scripts/agent-browser-visual-runner-lifecycle.test.mjs
  run ./scripts/bundle-freshness-lib.test.sh
  if [[ "${1:-}" != --source-only ]]; then
    run npm --prefix crates/oasis7_viewer run test:ui
  fi
}

run_oasis7_viewer_software_safe_build() {
  if [[ "${1:-}" == --with-ui ]]; then
    run_oasis7_viewer_software_safe_build_with_ui
  else
    run ./scripts/build-viewer-software-safe.sh
  fi
}

run_oasis7_viewer_software_safe_build_with_ui() {
  # UI reads source/fixtures; the build alone writes the generated bundle/dist.
  # Supervise their shared process tree so failure, cancellation, or leftover
  # descendants cannot cross the browser-security build barrier.
  run "$ci_python" - "$driver_dir" "$repo_root" <<'PY_VIEWER_PARALLEL'
import os
from pathlib import Path
import sys
import tempfile

driver, root = map(Path, sys.argv[1:])
sys.path.insert(0, str(driver))
from ci_process_runner import run

command = ['bash', '-c', '''set -euo pipefail
driver_dir="$1"; repo_root="$2"; ci_python="$3"
source "$driver_dir/ci-tests.sh"
run npm --prefix crates/oasis7_viewer run test:ui &
ui_pid=$!
run_oasis7_viewer_software_safe_build &
build_pid=$!
wait "$ui_pid"
wait "$build_pid"
''', 'viewer-ui-and-build', str(driver), str(root), sys.executable]
with tempfile.TemporaryDirectory(prefix='oasis7-viewer-checks-') as temporary:
    result = run(command, root, os.environ, 'viewer_js_required', 'ui-and-production-build',
                 Path(temporary) / 'results.jsonl', timeout_seconds=2400)
if not result['passed']:
    raise SystemExit(1)
PY_VIEWER_PARALLEL
}

run_pixel_world_bridge_lib_tests() {
  run_cargo test -p pixel_world_bridge --lib
}

run_pixel_world_bridge_wasm_check() {
  run_cargo check -p pixel_world_bridge --target wasm32-unknown-unknown
}

run_oasis7_viewer_browser_functional_regression() {
  AGENT_BROWSER_BIN="${AGENT_BROWSER_BIN:-agent-browser}" run node crates/oasis7_viewer/scripts/browser-functional-regression.mjs
}

run_oasis7_viewer_visual_test_build() {
  local bindgen_bin
  bindgen_bin="$(./scripts/ensure-wasm-bindgen-cli.sh --print-bin)"
  run env WASM_BINDGEN_BIN="$bindgen_bin" npm --prefix crates/oasis7_viewer run build:viewer:visual-test
}

run_oasis7_viewer_performance_smoke_report_only() {
  run ./scripts/viewer-performance-report-only.sh
}

run_hosted_account_local_smoke() {
  run bash ./scripts/hosted-account-staging-smoke.sh --mode local
}

run_oasis7_client_launcher_web_build() {
  run ./scripts/worktree-harness-lifecycle.test.sh
  run ./scripts/worktree-harness-lifecycle-races.test.sh
  run ./scripts/worktree-harness-contract.test.sh
  run ./scripts/launcher-help-contract.test.sh
  run mkdir -p output/release/web-launcher-dist
  (
    cd crates/oasis7_client_launcher
    run env -u NO_COLOR trunk build --release --dist ../../output/release/web-launcher-dist
  )
}

run_codex_agent_config_validation() {
  run python3 ./scripts/validate-codex-agent-config.py
}

run_compile_metrics_contract_tests() {
  run bash ./scripts/ci-compile-metrics-contract.test.sh
  run python3 ./scripts/ci-ecs-build-metrics.test.py
  run python3 ./scripts/ci-ecs-cache.test.py
}

run_site_contract_tests() {
  run ./scripts/site-link-check.sh
  run ./scripts/site-homepage-claim-check.sh
  run ./scripts/site-manual-sync-check.sh
  run ./scripts/site-download-check.sh
  run bash ./scripts/site-checks-contract.test.sh
  run env OASIS7_SITE_QA_HEADLESS=1 node ./scripts/site-deck-security-browser.test.cjs
}

run_system_design_traceability_tests() {
  run python3 ./scripts/system-design-traceability-check.test.py
}



product_doc_range() {
  local base_oid="${OASIS7_PRODUCT_DOC_BASE:-}"
  local head_oid="${OASIS7_PRODUCT_DOC_HEAD:-}"
  if [[ -n "$base_oid" || -n "$head_oid" ]]; then
    [[ -n "$base_oid" && -n "$head_oid" ]] || {
      echo "product-doc-content: explicit base/head must be supplied together" >&2
      return 1
    }
    printf '%s\n%s\n' "$base_oid" "$head_oid"
    return 0
  fi
  if [[ -n "${GITHUB_EVENT_PATH:-}" ]]; then
    if [[ ! -f "$GITHUB_EVENT_PATH" ]]; then
      echo "product-doc-content: CI event path is missing or unreadable" >&2
      return 1
    fi
    python3 - "$GITHUB_EVENT_PATH" "${GITHUB_EVENT_NAME:-}" "${GITHUB_SHA:-}" <<'PY'
import json
import re
import sys

event_path, event_name, default_head = sys.argv[1:]
try:
    with open(event_path, encoding="utf-8") as handle:
        payload = json.load(handle)
except (OSError, ValueError) as exc:
    raise SystemExit(f"product-doc-content: malformed CI event: {exc}")
if event_name == "pull_request":
    base = ((payload.get("pull_request") or {}).get("base") or {}).get("sha")
    head = ((payload.get("pull_request") or {}).get("head") or {}).get("sha")
elif event_name == "push":
    base = payload.get("before")
    head = payload.get("after")
else:
    raise SystemExit(f"product-doc-content: unsupported CI event range: {event_name or '<empty>'}")
if not isinstance(base, str) or not isinstance(head, str) or not base or not head:
    raise SystemExit("product-doc-content: CI event did not provide both base/head OIDs")
if not re.fullmatch(r"[0-9a-fA-F]{40}", base) or not re.fullmatch(r"[0-9a-fA-F]{40}", head):
    raise SystemExit("product-doc-content: CI event base/head must be full 40-character OIDs")
print(base)
print(head)
PY
    return $?
  fi
  if [[ "${CI:-}" == "true" || "${GITHUB_ACTIONS:-}" == "true" ]]; then
    echo "product-doc-content: CI requires explicit base/head OIDs or a valid event payload" >&2
    return 1
  fi
}

run_product_doc_governance_check() {
  local range
  range="$(product_doc_range)" || return 1
  if [[ -n "$range" ]]; then
    local base_oid head_oid
    base_oid="$(printf '%s\n' "$range" | sed -n '1p')"
    head_oid="$(printf '%s\n' "$range" | sed -n '2p')"
    [[ -n "$base_oid" && -n "$head_oid" ]] || {
      echo "product-doc-content: event did not provide explicit base/head" >&2
      return 1
    }
    local corpus_args=()
    if [[ "$tier" != required || "${OASIS7_CI_PROFILE:-}" == periodic-full || "${OASIS7_CI_PROFILE:-}" == candidate-full ]]; then
      corpus_args=(--full-corpus)
    fi
    OASIS7_PRODUCT_DOC_BASE="$base_oid" OASIS7_PRODUCT_DOC_HEAD="$head_oid" \
      run ./scripts/doc-governance-check.sh "${corpus_args[@]}"
  else
    run ./scripts/doc-governance-check.sh --full-corpus
  fi
}

run_standalone_tool_lockfiles_checks() {
  run bash ./scripts/check-standalone-tool-lockfiles.test.sh
  run ./scripts/check-standalone-tool-lockfiles.sh
}



run_required_gate_checks() {
  run ./scripts/unified-world-code-terminology-scan.test.sh
  run_product_doc_governance_check
  run ./scripts/lint-skills.sh
  run ./scripts/check-windows-paths.sh
  run bash ./scripts/check-script-executable-bits.sh
  if [[ -n "${OASIS7_PRODUCT_DOC_BASE:-}" && -n "${OASIS7_PRODUCT_DOC_HEAD:-}" ]]; then
    run git diff --check "$OASIS7_PRODUCT_DOC_BASE" "$OASIS7_PRODUCT_DOC_HEAD"
  else
    run git diff --check
  fi
  run python3 ./scripts/validate-codex-agent-config.py
}



run_commit_gate_checks() {
  run_required_gate_checks
  run_oasis7_consensus_tests
  run_oasis7_distfs_tests
  run_oasis7_viewer_software_safe_feedback_contract_tests
}

run_full_core_tier_tests() {
  run_required_gate_checks
  run_all_required_gate_capability_contract_tests
  run_oasis7_full_tier_tests
}

run_full_support_tier_tests() {
  run_all_required_gate_capability_contract_tests
  run_oasis7_consensus_tests
  run_oasis7_distfs_tests
  run_oasis7_node_tests
  run_oasis7_net_tests
  run_oasis7_net_libp2p_tests
  run_oasis7_workspace_support_crate_tests
  run_oasis7_llm_baseline_fixture_smoke
  run_oasis7_viewer_software_safe_feedback_contract_tests
  run_oasis7_viewer_software_safe_build
  run_oasis7_test_tier_with_viewer wasmtime --lib --bins
}

run_shared_native_cache_builds() {
  # Independent trusted cache runners start with no full-test target outputs.
  run python3 "$driver_dir/ci-shared-native-cache.py" --repo-root "$repo_root"
}

run_full_required_superset() {
  run bash ./scripts/testnet-packages-macos-arm64-contract.test.sh
  run_required_gate_checks
  run_all_required_gate_capability_contract_tests
  run_site_contract_tests
  run_oasis7_required_tier_clippy
  run_oasis7_required_tier_tests
  run_cargo test -p oasis7_viewer_runtime --lib snapshot_progress::
  run_cargo test -p oasis7 --lib snapshot_player_gameplay_execution_state_backfills_from_legacy_fields
  run_scenario_regression_tests
  run_oasis7_consensus_clippy
  run_oasis7_consensus_tests
  run_oasis7_distfs_clippy
  run_oasis7_distfs_tests
  run_oasis7_node_clippy
  run_oasis7_node_tests
  run_oasis7_net_clippy
  run_oasis7_net_libp2p_clippy
  run_oasis7_net_tests
  run_oasis7_net_libp2p_tests
  run_oasis7_viewer_software_safe_feedback_contract_tests --source-only
  run_oasis7_viewer_software_safe_build --with-ui
  run node crates/oasis7_viewer/scripts/viewer-auth-browser-security-smoke.mjs
  run_oasis7_viewer_visual_test_build
  run_oasis7_viewer_browser_functional_regression
  run_oasis7_viewer_performance_smoke_report_only
  run_pixel_world_bridge_lib_tests
  run_pixel_world_bridge_wasm_check
  run_oasis7_client_launcher_web_build
  run_oasis7_workspace_support_crate_tests
}

run_workspace_support_full_feature_tests() {
  local selected profile features
  local -a feature_args
  selected=$("$ci_python" - "$driver_dir" <<'PY'
import importlib.util, sys
from pathlib import Path
modules = {}
for name in ('plan-rust-required-scope', 'ci-test-artifacts'):
    spec = importlib.util.spec_from_file_location(name, Path(sys.argv[1]) / (name + '.py'))
    modules[name] = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(modules[name])
artifacts = modules['ci-test-artifacts']
artifacts.ROOT = Path.cwd()
for package in sorted(modules['plan-rust-required-scope'].SUPPORT_PROFILES):
    features = artifacts.package_features([package], include_full=True)
    print(package + '|' + ','.join(features))
PY
  ) || return 2
  while IFS='|' read -r profile features; do
    [[ -n "$profile" ]] || continue
    echo "+ complete support features: $profile"
    feature_args=()
    if [[ -n "$features" ]]; then feature_args=(--features "$features"); fi
    case "$profile" in
      wasm_build_suite|wasm_module_observe) OASIS7_WASM_BUILD_STD=0 run_cargo test -p "$profile" --all-targets "${feature_args[@]}" ;;
      *) run_cargo test -p "$profile" --all-targets "${feature_args[@]}" ;;
    esac
  done <<<"$selected"
}

prepare_full_provider_artifact() {
  local provider_bundle provider_env
  mkdir -p "$repo_root/target"
  provider_bundle=$(mktemp -d "$repo_root/target/ci-preview-provider.XXXXXX")
  provider_env="$provider_bundle/environment.sh"
  run "$ci_python" "$driver_dir/ci-test-artifacts.py" prepare-provider --repo-root "$repo_root" \
    --bundle "$provider_bundle" --output "$provider_env"
  source "$provider_env"
}

run_full_extended_tier_tests() {
  run_oasis7_full_tier_tests
  run_workspace_support_full_feature_tests
  run_oasis7_llm_baseline_fixture_smoke
  run_oasis7_production_mock_guard
  run_oasis7_test_tier_with_viewer "test_tier_required,wasmtime" --lib --bins
}

run_rust_baseline() {
  local extra_checks=$'newapi_accounting\nstandalone_tool_lockfiles' check
  if [[ "$tier" == required && -n "${required_plan:-}" ]]; then
    extra_checks=$("$ci_python" "$driver_dir/ci-test-artifacts.py" required-args --repo-root "$repo_root" \
      --plan "$required_plan" --phase baseline-checks) || return
  fi
  run env -u RUSTC_WRAPPER cargo fmt --all -- --check
  # Explicit preparation for the locked offline architecture inspector. A
  # restored cache is optional and cannot stand in for dependency availability.
  run_cargo fetch --locked
  local architecture_args=()
  if [[ "$tier" == required && -n "${required_plan:-}" ]]; then
    architecture_args=(--plan-file "$required_plan")
  elif [[ "$tier" == required && -n "${OASIS7_CI_ARCHITECTURE_PLAN_FILE:-}" ]]; then
    architecture_args=(--plan-file "$OASIS7_CI_ARCHITECTURE_PLAN_FILE")
  elif [[ "$tier" == required && -n "${OASIS7_CI_ARCHITECTURE_PLAN:-}" ]]; then
    architecture_args=(--plan "$OASIS7_CI_ARCHITECTURE_PLAN")
  fi
  run python3 ./scripts/cargo-dev-architecture.py "${architecture_args[@]}"
  run_rustsec_advisory_check
  run ./scripts/check-rust-file-size.test.sh
  run ./scripts/check-rust-file-size.sh
  run_oasis7_required_clippy_for_group rust_baseline
  while IFS= read -r check; do
    case "$check" in
      newapi_accounting) run_newapi_bridge_service_accounting_tests ;;
      standalone_tool_lockfiles) run_standalone_tool_lockfiles_checks ;;
      '') ;;
      *) echo "Unknown Rust baseline check: $check" >&2; return 2 ;;
    esac
  done <<< "$extra_checks"
}

run_group() {
  case "$1" in
    baseline) run_required_gate_checks ;;
    rust_baseline) run_rust_baseline ;;
    oasis7_required)
      if [[ -n "${ordinary_partition:-}" ]]; then
        run "$ci_python" "$driver_dir/ci-test-artifacts.py" required-args --repo-root "$repo_root" \
          --plan "$required_plan" --phase partition --ordinary-partition "$ordinary_partition" || return
      fi
      case "${ordinary_partition:-all}" in
        runtime|component-libs|integrations-0|integrations-1|native-[0-3])
          [[ -n "${OASIS7_CI_TEST_BUNDLE:-}" ]] || { echo 'Complete ordinary partition requires the shared artifact' >&2; return 2; }
          run "$ci_python" "$driver_dir/ci-test-artifacts.py" run --repo-root "$repo_root" \
            --bundle "$OASIS7_CI_TEST_BUNDLE" --suite ordinary --plan "$required_plan" --ordinary-partition "$ordinary_partition"
          ;;
        *)
          run_oasis7_required_clippy_for_group oasis7_required
          run_oasis7_required_tier_tests
          if [[ -z "${OASIS7_CI_TEST_BUNDLE:-}" && -z "${required_plan:-}" ]]; then
            run_cargo test -p oasis7_viewer_runtime --lib snapshot_progress::
            run_cargo test -p oasis7 --lib snapshot_player_gameplay_execution_state_backfills_from_legacy_fields
          fi
          ;;
      esac
      ;;
    formal_process) run bash "$driver_dir/formal-live-local-tests.sh" "$repo_root" ;;
    consensus) run_oasis7_consensus_clippy; run_oasis7_consensus_tests ;;
    distfs) run_oasis7_distfs_clippy; run_oasis7_distfs_tests ;;
    node) run_oasis7_node_clippy; run_oasis7_node_tests ;;
    net) run_oasis7_net_clippy; run_oasis7_net_libp2p_clippy; run_oasis7_net_tests; run_oasis7_net_libp2p_tests ;;
    viewer_js_required) run_oasis7_viewer_software_safe_feedback_contract_tests --source-only; run_oasis7_viewer_software_safe_build --with-ui; run_oasis7_viewer_browser_functional_regression; run node crates/oasis7_viewer/scripts/viewer-auth-browser-security-smoke.mjs ;;
    viewer_performance_report) run bash ./scripts/viewer-performance-report-only-contract.test.sh; run_oasis7_viewer_software_safe_build; run_oasis7_viewer_visual_test_build; run_oasis7_viewer_performance_smoke_report_only ;;
    pixel_world_bridge) run_pixel_world_bridge_lib_tests; run_pixel_world_bridge_wasm_check ;;
    launcher_web) run_oasis7_client_launcher_web_build ;;
    workspace_support) run_selected_workspace_support ;;
    scenario_regression) run_scenario_regression_tests ;;
    operational_contracts) run_operational_contract_tests ;;
    packaging_contracts) run_packaging_contract_tests ;;
    workflow_governance) run_workflow_governance_contract_tests ;;
    codex_agent_config_validation) run_codex_agent_config_validation ;;
    compile_metrics) run_compile_metrics_contract_tests ;;
    site_quality) run_site_contract_tests ;;
    doc_checker_contracts) run_doc_checker_contract_tests ;;
    cargo_tooling_contracts) run_cargo_tooling_contract_tests ;;
    *) echo "Unknown CI group: $1" >&2; return 2 ;;
  esac
}

# Keep the source interface free of CLI setup and process-wide side effects.
if [[ "${BASH_SOURCE[0]}" != "$0" ]]; then return 0; fi

set -euo pipefail

driver_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo_root=$(cd "$driver_dir/.." && pwd)

tier="${1:-}"

if [[ $# -eq 0 ]]; then
  usage
  exit 2
fi

case "$tier" in
  commit|required|full|full-standard|full-extended|full-core|full-support|cache-native) ;;
  *)
    usage
    exit 1
    ;;
esac

shift
group=""
support_profiles=""
required_plan=""
ordinary_partition=""
support_partition="all"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo-root)
      [[ $# -ge 2 && -n "$2" ]] || { usage; exit 1; }
      repo_root=$(cd "$2" && pwd)
      shift 2
      ;;
    --group)
      [[ $# -ge 2 && -n "$2" ]] || { usage; exit 1; }
      group="$2"
      shift 2
      ;;
    --support-profiles)
      [[ $# -ge 2 && -n "$2" ]] || { usage; exit 2; }
      support_profiles="$2"
      shift 2
      ;;
    --support-plan)
      [[ $# -ge 2 && -n "$2" ]] || { usage; exit 2; }
      support_profiles="plan:$2"
      shift 2
      ;;
    --support-plan-file)
      [[ $# -ge 2 && -n "$2" ]] || { usage; exit 2; }
      support_profiles="plan-file:$2"
      shift 2
      ;;
    --plan)
      [[ $# -ge 2 && -n "$2" ]] || { usage; exit 2; }
      required_plan="$2"
      shift 2
      ;;
    --support-partition)
      [[ $# -ge 2 && -n "$2" ]] || { usage; exit 2; }
      support_partition="$2"
      shift 2
      ;;
    --ordinary-partition)
      [[ $# -ge 2 && -n "$2" && -z "$ordinary_partition" ]] || { usage; exit 2; }
      ordinary_partition="$2"
      shift 2
      ;;
    *)
      usage
      exit 1
      ;;
  esac
done

ci_python="$(bash "$driver_dir/find-python-with-module.sh" tomllib)"
cd "$repo_root"
# Keep sourced driver definitions with the driver, even when the tested tree differs.
source "$driver_dir/viewer-dependency-preflight.sh"

echo "+ ci test tier: $tier"
if [[ -n "$support_profiles" && ( "$tier" != required || "$group" != workspace_support ) ]]; then
  echo '--support-profiles requires required --group workspace_support' >&2
  exit 2
fi
if [[ "$support_partition" != all && ( "$tier" != required || "$group" != workspace_support || -z "$support_profiles" ) ]]; then
  echo '--support-partition requires required --group workspace_support --support-plan' >&2
  exit 2
fi
if [[ -n "$required_plan" && ( "$tier" != required || ( "$group" != oasis7_required && "$group" != rust_baseline && "$group" != distfs ) ) ]]; then
  echo '--plan requires required --group oasis7_required, rust_baseline or distfs' >&2
  exit 2
fi
if [[ -n "$ordinary_partition" && ( "$tier" != required || "$group" != oasis7_required || -z "$required_plan" ) ]]; then
  echo '--ordinary-partition requires required --group oasis7_required --plan' >&2
  exit 2
fi
if [[ -z "$group" ]]; then
  case "$tier" in
    full|full-standard|full-extended|full-core) prepare_full_provider_artifact ;;
  esac
fi
if [[ -n "$group" ]]; then
  [[ "$tier" == required ]] || { echo '--group requires required tier' >&2; exit 2; }
  run_group "$group"
else
  case "$tier" in
    commit) run_commit_gate_checks ;;
    required)
      for group in baseline rust_baseline oasis7_required consensus distfs node net viewer_js_required pixel_world_bridge launcher_web workspace_support scenario_regression operational_contracts packaging_contracts workflow_governance codex_agent_config_validation compile_metrics site_quality doc_checker_contracts cargo_tooling_contracts; do run_group "$group"; done
      ;;
    full|full-standard) run_full_required_superset; run_rust_baseline; run_full_extended_tier_tests; if [[ "$tier" == full ]]; then run_group formal_process; fi ;;
    full-extended) run_full_extended_tier_tests ;;
    full-core) run_full_core_tier_tests ;;
    full-support) run_full_support_tier_tests ;;
    cache-native) run_shared_native_cache_builds ;;
  esac
fi

if [[ "${OASIS7_CI_RUN_PROVIDER_LIVE_GATE:-false}" == true ]]; then run_provider_bridge_live_gate; fi
if [[ "${OASIS7_CI_RUN_HOSTED_ACCOUNT_SMOKE:-false}" == true ]]; then run_hosted_account_local_smoke; fi
