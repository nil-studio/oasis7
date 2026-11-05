#!/usr/bin/env python3
"""Exercise the actual cache driver against the frozen full compiler inventory."""
from collections import Counter
import copy
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib
import unittest

ROOT = Path(__file__).resolve().parent.parent
DRIVER = ROOT / 'scripts/ci-tests.sh'

# Frozen from 4f6f20ece923c5705e26ef879c2b83e0e637b3f9 before separating
# the cache producer. Keeping the actual former helpers here makes the oracle
# independent of the new implementation and usable in shallow checkouts.
# The provider preparation's separate builtin WASM workspace is outside this
# native archive; its root sync checker is already the last legacy cache call.
FROZEN_FULL_DRIVER = r'''
run_cargo() {
  if [[ "${CI_VERBOSE:-}" == "1" ]]; then
    run env -u RUSTC_WRAPPER cargo --verbose "$@"
  else
    run env -u RUSTC_WRAPPER cargo "$@"
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
  run_cargo test -p oasis7 -p oasis7_simulator -p oasis7_runtime -p oasis7_world_state -p oasis7_world_projection -p oasis7_world_service -p scenario_test_runner -p oasis7_chain_runtime -p oasis7_client_cli -p oasis7_protocol_tools -p oasis7_launcher_backend -p oasis7_newapi_bridge_service -p oasis7_ops_tools -p oasis7_provider_local_bridge -p oasis7_provider_tools -p oasis7_viewer_runtime -p oasis7_viewer_codec -p oasis7_observability -p oasis7_hosted_access -p oasis7_letai_credit_adapter -p oasis7_launcher_config -p oasis7_wasm_materializer -p oasis7_game_client -p oasis7_browser_game_client -p oasis7_cas -p oasis7_formal_protocol "$@" --features "$features"
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

run_oasis7_full_tier_tests() {
  run_oasis7_test_tier_with_viewer "test_tier_full,wasmtime,viewer_live_integration"
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

run_oasis7_llm_baseline_fixture_smoke() {
  run ./scripts/llm-baseline-fixture-smoke.sh
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

run_full_extended_tier_tests() {
  run_oasis7_full_tier_tests
  run_workspace_support_full_feature_tests
  run_oasis7_llm_baseline_fixture_smoke
  run_oasis7_production_mock_guard
  run_oasis7_test_tier_with_viewer "test_tier_required,wasmtime" --lib --bins
}

run_shared_native_cache_builds() {
  # Bundle consumers do not populate this runner's target. Reuse the ordinary
  # producer's exact variants, then fill the Release-only root-workspace gaps.
  run python3 "$driver_dir/ci-test-artifacts.py" cache-build --scope ordinary --repo-root "$repo_root"
  run_oasis7_package_tests wasmtime --lib --bins --no-run --locked
  run_cargo build --locked -p oasis7_chain_runtime --bin oasis7_chain_runtime
  run_cargo build --locked -p oasis7_distfs --bin hydrate_builtin_wasm
  run_cargo build --locked -p oasis7_distfs --features sync-builtin-wasm-identity --bin sync_builtin_wasm_identity
}
'''

# Actual four ordinary build variants emitted by that BASE artifact helper.
LEGACY_ORDINARY = [['build', '-p', 'oasis7_viewer_runtime', '--bin', 'oasis7_viewer_live', '--features', 'test_tier_required'], ['test', '-p', 'oasis7', '-p', 'oasis7_simulator', '-p', 'oasis7_runtime', '-p', 'oasis7_world_state', '-p', 'oasis7_world_projection', '-p', 'oasis7_world_service', '-p', 'scenario_test_runner', '-p', 'oasis7_chain_runtime', '-p', 'oasis7_client_cli', '-p', 'oasis7_protocol_tools', '-p', 'oasis7_launcher_backend', '-p', 'oasis7_newapi_bridge_service', '-p', 'oasis7_ops_tools', '-p', 'oasis7_provider_local_bridge', '-p', 'oasis7_provider_tools', '-p', 'oasis7_viewer_runtime', '-p', 'oasis7_viewer_codec', '-p', 'oasis7_observability', '-p', 'oasis7_hosted_access', '-p', 'oasis7_letai_credit_adapter', '-p', 'oasis7_launcher_config', '-p', 'oasis7_wasm_materializer', '-p', 'oasis7_game_client', '-p', 'oasis7_browser_game_client', '-p', 'oasis7_cas', '-p', 'oasis7_formal_protocol', '--tests', '--features', 'test_tier_required', '--no-run'], ['test', '-p', 'oasis7', '--lib', '--no-run'], ['test', '-p', 'oasis7_viewer_runtime', '--lib', '--no-run']]

FROZEN_FIXTURE_SMOKE = r'''
#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"
source "$repo_root/scripts/cargo-dev-lib.sh"

fixture_dir="fixtures/llm_baseline/state_01"
snapshot_path="$fixture_dir/snapshot.json"
journal_path="$fixture_dir/journal.json"

if [[ ! -f "$snapshot_path" ]]; then
  echo "missing baseline snapshot fixture: $snapshot_path" >&2
  exit 2
fi

if [[ ! -f "$journal_path" ]]; then
  echo "missing baseline journal fixture: $journal_path" >&2
  exit 2
fi

echo "+ baseline fixture: $fixture_dir"
echo "+ oasis7_cargo_dev test -p oasis7 --features test_tier_full simulator::tests::persist::kernel_loads_tracked_llm_baseline_fixture_state -- --nocapture"
oasis7_cargo_dev test -p oasis7 --features test_tier_full simulator::tests::persist::kernel_loads_tracked_llm_baseline_fixture_state -- --nocapture
echo "+ oasis7_cargo_dev test -p oasis7_provider_tools --bin oasis7_llm_agent_demo --features test_tier_full runtime_bridge_continues_governance_from_tracked_baseline_fixture -- --nocapture"
oasis7_cargo_dev test -p oasis7_provider_tools --bin oasis7_llm_agent_demo --features test_tier_full runtime_bridge_continues_governance_from_tracked_baseline_fixture -- --nocapture
echo "+ oasis7_cargo_dev test -p oasis7_provider_tools --bin oasis7_llm_agent_demo --features test_tier_full runtime_bridge_civic_hotspot_preset_seeds_followup_handles -- --nocapture"
oasis7_cargo_dev test -p oasis7_provider_tools --bin oasis7_llm_agent_demo --features test_tier_full runtime_bridge_civic_hotspot_preset_seeds_followup_handles -- --nocapture
'''

FROZEN_CARGO_DEV_LIB = r'''
#!/usr/bin/env bash

oasis7_cargo_dev_repo_root() {
  local repo_root
  repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
  printf '%s\n' "$repo_root"
}

oasis7_cargo_dev_use_wrapper() {
  if [[ "${OASIS7_FORCE_RAW_CARGO:-0}" == "1" ]]; then
    return 1
  fi
  if [[ "${CI:-}" == "1" || "${CI:-}" == "true" ]]; then
    return 1
  fi
  return 0
}

oasis7_cargo_dev_target_dir() {
  local repo_root="${1:-$(oasis7_cargo_dev_repo_root)}"
  # Ask Cargo so .cargo/config.toml, CARGO_HOME config and environment use
  # the same precedence as the build. Metadata does not compile or fetch.
  local python_bin metadata
  python_bin="$("$repo_root/scripts/find-python-with-module.sh" json)" || return
  metadata="$(cd "$repo_root" && OASIS7_CARGO_DEV_REPO_ROOT="$repo_root" oasis7_cargo_dev metadata --offline --no-deps --format-version 1)" || return
  "$python_bin" -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])' <<<"$metadata"
}

oasis7_cargo_dev_debug_bin_dir() {
  local repo_root="${1:-$(oasis7_cargo_dev_repo_root)}"
  local target_dir
  target_dir="$(oasis7_cargo_dev_target_dir "$repo_root")" || return
  printf '%s/debug\n' "$target_dir"
}

oasis7_cargo_dev() {
  local repo_root="${OASIS7_CARGO_DEV_REPO_ROOT:-$(oasis7_cargo_dev_repo_root)}"
  if oasis7_cargo_dev_use_wrapper; then
    (cd "$repo_root" && ./scripts/cargo-dev.sh "$@")
  else
    env -u RUSTC_WRAPPER cargo "$@"
  fi
}
'''

FILTERS = {
    'simulator::tests::persist::kernel_loads_tracked_llm_baseline_fixture_state',
    'runtime_bridge_continues_governance_from_tracked_baseline_fixture',
    'runtime_bridge_civic_hotspot_preset_seeds_followup_handles',
}


def compile_context(call):
    """Compare Cargo inputs, preserving the roots that determine feature unions."""
    words = list(call['args'])
    if words[0] == '--verbose':
        words.pop(0)
    operation = words.pop(0)
    packages, selectors, features = [], [], []
    defaults, profile = True, 'default'
    while words:
        option = words.pop(0)
        if option == '-p':
            packages.append(words.pop(0))
        elif option in ('--bin', '--test'):
            selectors.append((option, words.pop(0)))
        elif option in ('--lib', '--bins', '--tests', '--all-targets'):
            selectors.append((option, ''))
        elif option == '--features':
            features.extend(words.pop(0).split(','))
        elif option == '--no-default-features':
            defaults = False
        elif option == '--profile':
            profile = words.pop(0)
        elif option == '--release':
            profile = 'release'
        elif option in ('--locked', '--no-run', '--message-format=json') or option in FILTERS:
            pass
        elif option == '--':
            if words != ['--nocapture']:
                raise AssertionError('unknown execution-only Cargo arguments: ' + repr(words))
            break
        else:
            raise AssertionError('unreviewed Cargo input: ' + option)
    return (operation, tuple(sorted(packages)), tuple(sorted(selectors)), defaults,
            tuple(sorted(features)), profile, call['environment']['OASIS7_WASM_BUILD_STD'])


class CacheBuildTests(unittest.TestCase):
    maxDiff = None

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix='oasis7-periodic-cache-build-')
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.log = self.root / 'cargo.jsonl'
        self.behaviors = self.root / 'behavior.jsonl'
        self.config_path = self.root / 'scripts/ci-required-scope.json'
        self.config_path.parent.mkdir()
        shutil.copyfile(ROOT / 'scripts/ci-required-scope.json', self.config_path)
        shutil.copyfile(ROOT / 'Cargo.toml', self.root / 'Cargo.toml')
        workspace = tomllib.loads((ROOT / 'Cargo.toml').read_text())['workspace']
        for member in workspace['members']:
            for directory in ROOT.glob(member):
                destination = self.root / directory.relative_to(ROOT) / 'Cargo.toml'
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(directory / 'Cargo.toml', destination)
        self.config = json.loads(self.config_path.read_text())
        cargo = self.root / 'cargo'
        cargo.write_text(f'#!{sys.executable}\n' + r'''
import json, os, pathlib, sys
log = pathlib.Path(os.environ['TEST_CARGO_LOG'])
number = len(log.read_text().splitlines()) + 1 if log.exists() else 1
record = {'args': sys.argv[1:], 'wrapper': os.environ.get('RUSTC_WRAPPER'), 'cwd': os.getcwd(),
          'environment': {name: os.environ.get(name) for name in
          ('OASIS7_WASM_BUILD_STD', 'CARGO_INCREMENTAL', 'CARGO_PROFILE_DEV_DEBUG', 'CARGO_PROFILE_TEST_DEBUG')}}
with log.open('a') as stream:
    stream.write(json.dumps(record) + '\n')
if str(number) == os.environ.get('TEST_FAIL_CALL'):
    raise SystemExit(23)
if '--message-format=json' in sys.argv:
    for index, argument in enumerate(sys.argv[:-1]):
        if argument == '--bin':
            name = sys.argv[index + 1]
            binary = pathlib.Path.cwd() / 'recorded-binaries' / name
            binary.parent.mkdir(exist_ok=True)
            binary.write_text('#!/bin/sh\nexit 97\n')
            binary.chmod(0o755)
            print(json.dumps({'reason': 'compiler-artifact', 'target': {'name': name},
                              'executable': str(binary)}))
''')
        cargo.chmod(0o755)
        self.env = dict(os.environ, PATH=str(self.root) + os.pathsep + os.environ['PATH'],
                        CI='true', CI_VERBOSE='0', TEST_CARGO_LOG=str(self.log),
                        TEST_BEHAVIOR_LOG=str(self.behaviors), RUSTC_WRAPPER='must-be-unset',
                        OASIS7_WASM_BUILD_STD='1', CARGO_INCREMENTAL='0',
                        CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0')
        for variable in ('TEST_FAIL_CALL', 'OASIS7_CI_RUN_PROVIDER_LIVE_GATE',
                         'OASIS7_CI_RUN_HOSTED_ACCOUNT_SMOKE'):
            self.env.pop(variable, None)

    def reset_logs(self):
        self.log.unlink(missing_ok=True)
        self.behaviors.unlink(missing_ok=True)

    def build(self):
        self.reset_logs()
        return subprocess.run(['bash', str(DRIVER), 'cache-native', '--repo-root', str(self.root)],
                              env=self.env, text=True, capture_output=True, timeout=45)

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()] if self.log.exists() else []

    def frozen_full_and_legacy_calls(self):
        """Run the old real shell control flow with only executable boundaries recorded."""
        fixtures = self.root / 'fixtures/llm_baseline/state_01'
        fixtures.mkdir(parents=True)
        for name in ('snapshot.json', 'journal.json'):
            (fixtures / name).write_text('{}')
        smoke = self.root / 'scripts/llm-baseline-fixture-smoke.sh'
        smoke.write_text(FROZEN_FIXTURE_SMOKE)
        smoke.chmod(0o755)
        (self.root / 'scripts/cargo-dev-lib.sh').write_text(FROZEN_CARGO_DEV_LIB)
        # The guard is an execution boundary, not a cache compiler input. Record
        # reaching it while the original driver still produces its real manifest.
        (self.root / 'scripts/hosted-local-mock-production-guard.py').write_text(
            "import os, pathlib\npathlib.Path(os.environ['TEST_BEHAVIOR_LOG']).write_text('mock-guard\\n')\n")
        legacy = self.root / 'legacy-ordinary.py'
        legacy.write_text('import os, subprocess\ncommands = ' + repr(LEGACY_ORDINARY) + '\n'
                          + "environment = dict(os.environ); environment.pop('RUSTC_WRAPPER', None)\n"
                          + "for arguments in commands:\n subprocess.run(['cargo', *arguments, '--locked'], env=environment, check=True)\n")
        self.env['TEST_LEGACY_CACHE_SCRIPT'] = str(legacy)
        oracle = r'''set -euo pipefail
repo_root="$1"; driver_dir="$2"; ci_python="$3"
cd "$repo_root"
run() {
  if [[ "$1" == python3 && "$2" == "$driver_dir/ci-test-artifacts.py" && "$3" == cache-build ]]; then
    "$ci_python" "$TEST_LEGACY_CACHE_SCRIPT"
  elif [[ "$1" == python3 ]]; then
    shift; "$ci_python" "$@"
  else
    "$@"
  fi
}
'''
        self.reset_logs()
        result = subprocess.run(['bash', '-c', oracle + FROZEN_FULL_DRIVER
                                 + '\nrun_full_extended_tier_tests\nrun_shared_native_cache_builds\n',
                                 'frozen-full-cache-oracle', str(self.root), str(ROOT / 'scripts'), sys.executable],
                                env=self.env, text=True, capture_output=True, timeout=45)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.behaviors.read_text(), 'mock-guard\n')
        calls = self.calls()
        # Three fixture behaviors include two filters with the same provider
        # compile context: 33 former full calls plus the eight legacy cache calls.
        self.assertEqual(len(calls), 41)
        self.assertEqual(len({compile_context(call) for call in calls}), 40)
        return calls

    def named_calls(self, result):
        names = re.findall(r'^cache_build_variant=([^ ]+) command=', result.stdout, re.M)
        self.assertEqual(len(names), len(self.calls()), result.stdout)
        self.assertEqual(len(set(names)), len(names))
        return dict(zip(names, self.calls()))

    def assert_fast_inventory(self, named, config):
        actual = []
        for name, call in named.items():
            if not name.startswith('fast-'):
                continue
            context = compile_context(call)
            self.assertEqual(context[0], 'test')
            self.assertEqual(len(context[1]), 1, 'package roots must never be unified')
            self.assertTrue(context[2])
            for selector in context[2]:
                self.assertIn(selector[0], ('--lib', '--test'))
                actual.append((*context[:2], (selector,), *context[3:]))
        expected = []
        for target in config['ordinary_fast_targets'].values():
            selector = ('--lib', '') if target['target_kind'] == 'lib' else ('--test', target['target_name'])
            expected.append(('test', (target['package'],), (selector,), target['default_features'],
                             tuple(sorted(target['features'])), 'default', '1'))
        self.assertEqual(Counter(actual), Counter(expected))
        # Combining targets can reduce invocations only for identical root,
        # default-feature policy and explicit features; no cross-root union.
        groups = {(target['package'], target['default_features'], tuple(sorted(target['features'])))
                  for target in config['ordinary_fast_targets'].values()}
        self.assertEqual(sum(name.startswith('fast-') for name in named), len(groups))

    def test_exact_consumer_variants_are_compiled_without_executing_tests(self):
        legacy = {compile_context(call) for call in self.frozen_full_and_legacy_calls()}
        for verbose in ('0', '1'):
            with self.subTest(verbose=verbose):
                self.env['CI_VERBOSE'] = verbose
                result = self.build()
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                named = self.named_calls(result)
                self.assertEqual(len(named), 50)
                self.assertIn('cache_build_complete variants=50 verification=false', result.stdout)
                self.assertEqual({compile_context(call) for name, call in named.items()
                                  if not name.startswith('fast-')}, legacy)
                self.assert_fast_inventory(named, self.config)
                self.assertFalse(self.behaviors.exists(), 'cache warming must not execute a verification guard')
                for call in named.values():
                    self.assertIsNone(call['wrapper'])
                    self.assertEqual(call['cwd'], str(self.root))
                    self.assertIn('--locked', call['args'])
                    self.assertNotIn('--all-features', call['args'])
                    self.assertNotIn('--target-dir', call['args'])
                    if call['args'][0] == 'test':
                        self.assertIn('--no-run', call['args'])
                    self.assertEqual({key: value for key, value in call['environment'].items()
                                      if key != 'OASIS7_WASM_BUILD_STD'},
                                     {'CARGO_INCREMENTAL': '0', 'CARGO_PROFILE_DEV_DEBUG': '0',
                                      'CARGO_PROFILE_TEST_DEBUG': '0'})

    def test_compile_failure_stops_remaining_cache_population(self):
        for number in (1, 27, 29, 32, 40, 41, 50):
            with self.subTest(number=number):
                self.env['TEST_FAIL_CALL'] = str(number)
                result = self.build()
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(len(self.calls()), number)
                self.assertNotIn('cache_build_complete', result.stdout)
                self.assertFalse(self.behaviors.exists())

    def test_invalid_trusted_inventory_is_rejected_before_any_cargo(self):
        invalid = []
        config = copy.deepcopy(self.config)
        config['fast_test_inventory_version'] = 2
        invalid.append(('unknown-version', json.dumps(config)))
        config = copy.deepcopy(self.config)
        config['ordinary_fast_targets']['runtime-basic']['features'] = ['--all-features']
        invalid.append(('unsafe-feature', json.dumps(config)))
        config = copy.deepcopy(self.config)
        config['ordinary_fast_targets']['runtime-basic']['target_name'] = '../../escape'
        invalid.append(('escaping-target', json.dumps(config)))
        config = copy.deepcopy(self.config)
        config['ordinary_fast_targets']['runtime-basic']['cases'] = []
        invalid.append(('empty-required-cases', json.dumps(config)))
        config = copy.deepcopy(self.config)
        config['ordinary_fast_targets']['duplicate'] = config['ordinary_fast_targets']['runtime-basic']
        invalid.append(('ambiguous-target', json.dumps(config)))
        invalid.append(('invalid-json', '{'))
        for name, contents in invalid:
            with self.subTest(name=name):
                self.config_path.write_text(contents)
                result = self.build()
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(self.calls(), [])
                self.assertNotIn('cache_build_complete', result.stdout)

    def test_missing_support_manifest_is_rejected_before_any_cargo(self):
        (self.root / 'crates/oasis7_agent_api/Cargo.toml').unlink()
        result = self.build()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.calls(), [])
        self.assertNotIn('cache_build_complete', result.stdout)

    def test_support_features_come_from_actual_manifest_and_env_is_per_command(self):
        manifest = self.root / 'crates/oasis7_agent_api/Cargo.toml'
        manifest.write_text(manifest.read_text().replace('[features]\n',
            '[features]\ntest_tier_required = []\ntest_tier_full = ["test_tier_required"]\n'
            'wasmtime = []\nviewer_live_integration = []\n'))
        result = self.build()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        named = self.named_calls(result)
        support = compile_context(named['support-oasis7_agent_api'])
        self.assertEqual(support[4], tuple(sorted('oasis7_agent_api/' + feature for feature in
                         ('test_tier_required', 'test_tier_full', 'wasmtime', 'viewer_live_integration'))))
        for name, call in named.items():
            self.assertEqual(call['environment']['OASIS7_WASM_BUILD_STD'],
                             '0' if name in ('support-wasm_build_suite', 'support-wasm_module_observe') else '1')

    def test_fast_targets_with_different_feature_policies_are_not_combined(self):
        config = copy.deepcopy(self.config)
        config['ordinary_fast_targets']['runtime-basic']['default_features'] = True
        config['ordinary_fast_targets']['runtime-incremental']['features'] = []
        config['ordinary_fast_targets']['world-reducers']['features'] = []
        self.config_path.write_text(json.dumps(config))
        result = self.build()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        named = self.named_calls(result)
        self.assert_fast_inventory(named, config)
        self.assertEqual(sum(name.startswith('fast-') for name in named), 13)


if __name__ == '__main__':
    unittest.main()
