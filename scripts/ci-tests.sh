#!/usr/bin/env bash
set -euo pipefail

driver_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo_root=$(cd "$driver_dir/.." && pwd)

tier="${1:-}"

usage() {
  cat <<'USAGE'
Usage: ./scripts/ci-tests.sh [commit|required|full|full-standard|full-core|full-support] [--repo-root PATH] [--group NAME] [--support-plan JSON|--support-profiles JSON]

  commit        Run the lightweight local commit gate used by pre-commit.
  required      Run the explicit heavier required gate for local validation and PR gate.
  full          Run required checks plus all extended feature/integration tests.
  full-standard Run full checks excluding the independent formal-process suites.
  full-core     Run doc/fmt plus the heaviest `oasis7 --tests` full-tier shard.
  full-support  Run the remaining support crates/viewer shard plus `oasis7 --lib --bins`.

Requires Python 3.11 or newer with the standard-library tomllib module.
The runner discovers a supported interpreter on PATH.

Default: none (explicit tier required)
USAGE
}

if [[ $# -eq 0 ]]; then
  usage
  exit 2
fi

case "$tier" in
  commit|required|full|full-standard|full-core|full-support) ;;
  *)
    usage
    exit 1
    ;;
esac

shift
group=""
support_profiles=""
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

run() {
  if [[ "$1" == python3 ]]; then
    shift
    set -- "$ci_python" "$@"
  fi
  echo "+ $*"
  "$@"
}

prepare_local_cargo_target() {
  if [[ "${CI:-}" != "1" && "${CI:-}" != "true" && -z "${CARGO_TARGET_DIR:-}" ]]; then
    (cd "$repo_root" && "$driver_dir/cargo-dev.sh" --prepare-target)
  fi
}

run_cargo() {
  prepare_local_cargo_target
  if [[ "${CI_VERBOSE:-}" == "1" ]]; then
    run env -u RUSTC_WRAPPER cargo --verbose "$@"
  else
    run env -u RUSTC_WRAPPER cargo "$@"
  fi
}

run_cargo_clippy() {
  prepare_local_cargo_target
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

run_oasis7_test_tier_with_viewer() {
  local features="$1"
  shift
  if [[ "$#" -eq 0 ]]; then
    set -- --tests
  fi
  local viewer_binary
  viewer_binary="$(build_oasis7_viewer_binary "$features")"
  PRE2_VIEWER_BINARY="$viewer_binary" run_cargo test -p oasis7 -p oasis7_simulator -p oasis7_runtime -p oasis7_world_service -p scenario_test_runner -p oasis7_chain_runtime -p oasis7_client_cli -p oasis7_launcher_backend -p oasis7_newapi_bridge_service -p oasis7_ops_tools -p oasis7_provider_local_bridge -p oasis7_provider_tools -p oasis7_viewer_runtime -p oasis7_observability -p oasis7_hosted_access -p oasis7_letai_credit_adapter -p oasis7_launcher_config -p oasis7_wasm_materializer -p oasis7_game_client -p oasis7_cas -p oasis7_formal_protocol "$@" --features "$features"
}

run_oasis7_required_tier_tests() {
  run_cargo check -p oasis7_agent_api -p oasis7_simulator --target wasm32-unknown-unknown --locked
  if [[ -n "${OASIS7_CI_TEST_BUNDLE:-}" ]]; then
    run python3 "$driver_dir/ci-test-artifacts.py" run --repo-root "$repo_root" --bundle "$OASIS7_CI_TEST_BUNDLE" --suite ordinary
    return
  fi
  run_oasis7_test_tier_with_viewer test_tier_required
}

run_scenario_regression_tests() {
  run_cargo test -p oasis7_ops_tools --test oasis7_init_demo --features test_tier_full oasis7_init_demo_runs_
}

run_oasis7_required_tier_clippy() {
  run_cargo_clippy -p oasis7 -p oasis7_simulator -p oasis7_runtime -p oasis7_world_service -p scenario_test_runner -p oasis7_chain_runtime -p oasis7_client_cli -p oasis7_launcher_backend -p oasis7_newapi_bridge_service -p oasis7_ops_tools -p oasis7_provider_local_bridge -p oasis7_provider_tools -p oasis7_viewer_runtime -p oasis7_observability -p oasis7_hosted_access -p oasis7_letai_credit_adapter -p oasis7_launcher_config -p oasis7_wasm_materializer -p oasis7_game_client -p oasis7_cas -p oasis7_formal_protocol --tests --features test_tier_required
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
  run python3 ./scripts/libp2p-compat.test.py
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

run_oasis7_workspace_support_crate_tests() {
  run_cargo_clippy -p oasis7_agent_api --all-targets
  run_cargo_clippy -p oasis7_agent_harness --all-targets
  run_cargo_clippy -p oasis7_client_storage -p oasis7_signer_protocol --all-targets
  run_cargo_clippy -p oasis7_game_client --all-targets
  run_cargo_clippy -p oasis7_model_provider --all-targets
  run_cargo test -p oasis7_agent_api
  run_cargo test -p oasis7_agent_harness --lib
  run_cargo test -p oasis7_client_storage -p oasis7_signer_protocol --lib
  run_cargo test -p oasis7_game_client --lib
  run_cargo test -p oasis7_model_provider --all-targets
  run_cargo test \
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
  local selected profile
  selected=$("$ci_python" - "$support_profiles" "$driver_dir" <<'PY'
import importlib.util, json, sys
from pathlib import Path
spec = importlib.util.spec_from_file_location('scope', Path(sys.argv[2]) / 'plan-rust-required-scope.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
raw = sys.argv[1]
if raw.startswith('plan:'):
    plan = json.loads(raw[5:])
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
if raw.startswith('plan:') and plan.get('scope') == 'full':
    profiles = list(module.SUPPORT_PROFILES)
print('\n'.join(sorted(profiles)))
PY
  ) || return 2
  while IFS= read -r profile; do
    echo "+ support profile: $profile"
    case "$profile" in
      oasis7_agent_api) run_cargo_clippy -p "$profile" --all-targets; run_cargo test -p "$profile" ;;
      oasis7_model_provider) run_cargo_clippy -p "$profile" --all-targets; run_cargo test -p "$profile" --all-targets ;;
      oasis7_agent_harness|oasis7_game_client) run_cargo_clippy -p "$profile" --all-targets; run_cargo test -p "$profile" --lib ;;
      oasis7_wasm_executor) run_cargo test -p "$profile" --features wasmtime --lib ;;
      oasis7_client_storage|oasis7_signer_protocol) run_cargo_clippy -p "$profile" --all-targets; run_cargo test -p "$profile" --lib ;;
      oasis7_secure_envelope|oasis7_context) run_cargo test -p "$profile" --all-targets --locked ;;
      oasis7_local_signer) run_cargo test -p "$profile" --lib; run_cargo test -p "$profile" --test context_cli --locked ;;
      oasis7_client_launcher) run_cargo test -p "$profile" --bin oasis7_client_launcher ;;
      wasm_build_suite|wasm_module_observe) OASIS7_WASM_BUILD_STD=0 run_cargo test -p "$profile" ;;
      *) run_cargo test -p "$profile" --lib ;;
    esac
    echo "+ support profile completed: $profile"
  done <<< "$selected"
}

run_rustsec_advisory_check() {
  run ./scripts/check-rustsec-ignore-baseline.sh
  run ./scripts/ensure-cargo-deny.sh
  run cargo deny check advisories
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
  run python3 ./scripts/document-corpus-inventory-check.test.py
  run python3 ./scripts/doc-evidence-inventory-check.test.py
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
}

run_cargo_tooling_contract_tests() {
  run_cargo_tooling_baseline_contract_tests
  run python3 ./scripts/cargo-cache.test.py
  run python3 ./scripts/cargo-incremental-clean.test.py
  run python3 ./scripts/runtime-module-routing-perf-harness.test.py
  run bash ./scripts/cargo-dev-lib.test.sh
  run bash ./scripts/check-standalone-tool-lockfiles.test.sh
  run ./scripts/check-standalone-tool-lockfiles.sh
}

run_workflow_governance_baseline_contract_tests() {
  run node ./scripts/cache-permission-probe.test.cjs
  run python3 ./scripts/plan-rust-required-scope.test.py
  run python3 ./scripts/ci-formal-process.test.py
  run python3 ./scripts/ci-required-result.test.py
  run python3 ./scripts/ci-workflow.test.py
  run python3 ./scripts/ci-support-profiles.test.py
  run bash ./scripts/plan-wasm-determinism-scope.test.sh
  run bash ./scripts/ensure-wasm-bindgen-cli.test.sh
  run python3 ./scripts/ci-local-signer-tests.test.py
  run bash ./scripts/rust-full-tier-trunk-prerequisite-contract.test.sh
  run bash ./scripts/worktree-gc-report.test.sh
  run python3 ./scripts/resource-cleanup-executor.test.py
  run python3 ./scripts/pr-review-threads.test.py
}

run_workflow_governance_operational_contract_tests() {
  run python3 ./scripts/security/codeql-plan.test.py
  run python3 ./scripts/security/codeql-workflow.test.py
  run python3 ./scripts/security/codeql-health.test.py
  run python3 ./scripts/security/codeql-upload-association.test.py
  run python3 ./scripts/security/codeql-acceptance.test.py
}

run_workflow_governance_contract_tests() {
  run_workflow_governance_baseline_contract_tests
  run_workflow_governance_operational_contract_tests
}

run_packaging_artifact_contract_tests() {
  run python3 ./scripts/package-source-plan.test.py
  run python3 ./scripts/package-release-version.test.py
  run python3 ./scripts/safe-git-archive.test.py
  run bash ./scripts/native-packaging-contract.test.sh
  run bash ./scripts/packaging-artifact-size-contract.test.sh
  run bash ./scripts/package-workflow-cache-reuse-contract.test.sh
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
  prepare_local_cargo_target
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
  run npm --prefix crates/oasis7_viewer run test:ui
}

run_oasis7_viewer_software_safe_build() {
  run ./scripts/build-viewer-software-safe.sh
}

run_pixel_world_bridge_lib_tests() {
  run_cargo test -p pixel_world_bridge --lib
}

run_pixel_world_bridge_wasm_check() {
  run_cargo check -p pixel_world_bridge --target wasm32-unknown-unknown
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
}

run_site_contract_tests() {
  run ./scripts/site-link-check.sh
  run ./scripts/site-homepage-claim-check.sh
  run ./scripts/site-manual-sync-check.sh
  run ./scripts/site-download-check.sh
  run bash ./scripts/site-checks-contract.test.sh
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
    OASIS7_PRODUCT_DOC_BASE="$base_oid" OASIS7_PRODUCT_DOC_HEAD="$head_oid" \
      run ./scripts/doc-governance-check.sh --full-corpus
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
  run_cargo test -p oasis7 -p oasis7_simulator -p oasis7_runtime -p oasis7_world_service -p scenario_test_runner -p oasis7_chain_runtime -p oasis7_client_cli -p oasis7_launcher_backend -p oasis7_newapi_bridge_service -p oasis7_ops_tools -p oasis7_provider_local_bridge -p oasis7_provider_tools -p oasis7_viewer_runtime -p oasis7_observability -p oasis7_hosted_access -p oasis7_letai_credit_adapter -p oasis7_launcher_config -p oasis7_wasm_materializer -p oasis7_game_client -p oasis7_cas -p oasis7_formal_protocol --features wasmtime --lib --bins
}

run_full_required_superset() {
  run bash ./scripts/testnet-packages-macos-arm64-contract.test.sh
  run_required_gate_checks
  run_all_required_gate_capability_contract_tests
  run_site_contract_tests
  run_oasis7_required_tier_tests
  run_scenario_regression_tests
  run_oasis7_consensus_tests
  run_oasis7_distfs_tests
  run_oasis7_node_tests
  run_oasis7_net_tests
  run_oasis7_net_libp2p_tests
  run_oasis7_viewer_software_safe_feedback_contract_tests
  run_oasis7_viewer_software_safe_build
  run_pixel_world_bridge_lib_tests
  run_pixel_world_bridge_wasm_check
  run_oasis7_client_launcher_web_build
  run_oasis7_workspace_support_crate_tests
}

run_rust_baseline() {
  run env -u RUSTC_WRAPPER cargo fmt --all -- --check
  run_rustsec_advisory_check
  run ./scripts/check-rust-file-size.test.sh
  run ./scripts/check-rust-file-size.sh
  run_newapi_bridge_service_accounting_tests
  run_standalone_tool_lockfiles_checks
}

run_group() {
  case "$1" in
    baseline) run_required_gate_checks ;;
    rust_baseline) run_rust_baseline ;;
    oasis7_required) run_oasis7_required_tier_clippy; run_oasis7_required_tier_tests; if [[ -z "${OASIS7_CI_TEST_BUNDLE:-}" ]]; then run_cargo test -p oasis7 --lib snapshot_progress::; run_cargo test -p oasis7 --lib snapshot_player_gameplay_execution_state_backfills_from_legacy_fields; fi ;;
    formal_process) run bash "$driver_dir/formal-live-local-tests.sh" "$repo_root" ;;
    consensus) run_oasis7_consensus_clippy; run_oasis7_consensus_tests ;;
    distfs) run_oasis7_distfs_clippy; run_oasis7_distfs_tests ;;
    node) run_oasis7_node_clippy; run_oasis7_node_tests ;;
    net) run_oasis7_net_clippy; run_oasis7_net_libp2p_clippy; run_oasis7_net_tests; run_oasis7_net_libp2p_tests ;;
    viewer_js_required) run_oasis7_viewer_software_safe_feedback_contract_tests; run_oasis7_viewer_software_safe_build; run node crates/oasis7_viewer/scripts/viewer-auth-browser-security-smoke.mjs ;;
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

echo "+ ci test tier: $tier"
if [[ -n "$support_profiles" && ( "$tier" != required || "$group" != workspace_support ) ]]; then
  echo '--support-profiles requires required --group workspace_support' >&2
  exit 2
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
    full|full-standard) run_full_required_superset; run_rust_baseline; run_oasis7_full_tier_tests; run_oasis7_llm_baseline_fixture_smoke; run_oasis7_production_mock_guard; run_oasis7_test_tier_with_viewer "test_tier_required,wasmtime" --lib --bins; if [[ "$tier" == full ]]; then run_group formal_process; fi ;;
    full-core) run_full_core_tier_tests ;;
    full-support) run_full_support_tier_tests ;;
  esac
fi

if [[ "${OASIS7_CI_RUN_PROVIDER_LIVE_GATE:-false}" == true ]]; then run_provider_bridge_live_gate; fi
if [[ "${OASIS7_CI_RUN_HOSTED_ACCOUNT_SMOKE:-false}" == true ]]; then run_hosted_account_local_smoke; fi
