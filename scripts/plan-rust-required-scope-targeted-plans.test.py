#!/usr/bin/env python3
import contextlib
import copy
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('selector', HERE / 'plan-rust-required-scope.py')
selector = importlib.util.module_from_spec(spec)
spec.loader.exec_module(selector)


class _ScopeFixtureBase(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.previous = Path.cwd()
        os.chdir(self.temp.name)
        self.git('init', '-q')
        self.git('config', 'user.email', 'ci@example.invalid')
        self.git('config', 'user.name', 'CI fixture')
        self.write('Cargo.toml', '[workspace]\nmembers=["crates/*"]\n[workspace.dependencies]\ncore={path="crates/core"}\n')
        for package in ['core', 'consumer', 'other']:
            manifest = f'[package]\nname="{package}"\nversion="0.1.0"\n'
            if package == 'consumer':
                manifest += '[target.\'cfg(unix)\'.dev-dependencies]\ncore={workspace=true,optional=true}\n'
            manifest += '[features]\ntest_tier_required=[]\ntest_tier_full=["test_tier_required"]\n'
            self.write(f'crates/{package}/Cargo.toml', manifest)
            self.write(f'crates/{package}/src/lib.rs', '')
        self.write('doc/readme.md', 'ordinary doc')
        # Plans that grant an optional execution protocol also inspect its
        # declaration in this immutable tested source, independently of policy.
        self.write('scripts/ci-required-scope.json', (HERE / 'ci-required-scope.json').read_text())
        self.base = self.commit()
        groups = ['baseline', 'rust_baseline', 'core', 'consumer', 'other', *selector.PLATFORM_GROUPS]
        self.config = {'groups': groups, 'resources': {g: [] for g in groups},
                       'package_groups': {p: [p] for p in ['core', 'consumer', 'other']},
                       'rules': [{'match': ['doc/**'], 'groups': [], 'reason': 'doc'}]}

    def tearDown(self):
        os.chdir(self.previous)
        self.temp.cleanup()

    def git(self, *args):
        return subprocess.check_output(['git', *args], stderr=subprocess.DEVNULL).decode().strip()

    def write(self, path, content):
        Path(path).parent.mkdir(parents=True, exist_ok=True)
        Path(path).write_text(content)

    def commit(self):
        self.git('add', '-A')
        self.git('commit', '-qm', 'fixture')
        return self.git('rev-parse', 'HEAD')

    def fast_config(self):
        config = json.loads(json.dumps(self.config))
        for group in ('oasis7_required', 'formal_process', 'command_center'):
            config['groups'].append(group)
            config['resources'][group] = []
        config.update(
            fast_scope_version=1,
            oasis7_required_packages=['core', 'consumer', 'other'],
            oasis7_required_wasm_packages=['core', 'other'],
            formal_risk_rules=[
                {'match': ['crates/core/src/**', 'crates/core/tests/**'], 'suites': [],
                 'reason': 'core behavior has ordinary contracts'},
                {'match': ['crates/core/src/history.rs'], 'suites': ['world-history'],
                 'reason': 'retained history'}],
            command_center_fast_paths=['tools/oasis7-command-center/ui/src/**'])
        config['package_groups'] = {name: ['oasis7_required', 'formal_process']
                                    for name in ('core', 'consumer', 'other')}
        config['rules'].append({'match': ['tools/oasis7-command-center/**'],
                                'groups': ['command_center'], 'reason': 'desktop'})
        return config


class ScaleRiskGitPlans(_ScopeFixtureBase):
    source = 'crates/core/src/feedback.rs'
    anchor = 'crates/core/src/lib.rs'

    def setUp(self):
        super().setUp()
        self.config = self.fast_config()
        self.body = 'pub fn feedback() -> u8 { 1 }\n'
        self.root = 'mod feedback; pub fn core() -> u8 { 1 }\n'
        self.target = dict(package='core', target_kind='test', target_name='scale', default_features=True,
                           features=['test_tier_full'], cases=['full_scale'])
        self.config['ordinary_scale_contract'] = dict(version=1, targets=[self.target],
            isolated_sources={self.source: selector.scale_source_boundary(self.body)},
            anchors={self.anchor: hashlib.sha256(self.root.encode()).hexdigest()})
        self.write(self.source, self.body)
        self.write(self.anchor, self.root)
        self.write('crates/core/tests/scale.rs',
                   '#[cfg(feature="test_tier_full")] #[test] fn full_scale() { assert!(true); }\n')
        self.write('scripts/ci-required-scope.json', json.dumps(self.config))
        self.base = self.commit()

    def select(self, source, tested=None, config=None):
        return selector.select(self.config if config is None else config, self.base, source,
                               tested, profile='pr-fast', policy_ref=self.base)

    def test_actual_body_change_keeps_required_packages_and_defers_only_scale_matrix(self):
        self.write(self.source, self.body.replace('1', '2'))
        head = self.commit()
        selected = self.select(head)
        old = copy.deepcopy(self.config)
        old.pop('ordinary_scale_contract')
        legacy = self.select(head, config=old)
        self.assertEqual(selected['scope'], 'targeted')
        self.assertIs(selected['test_policy_changed'], False)
        self.assertEqual(selected['oasis7_required_packages'], ['consumer', 'core'])
        self.assertEqual(selected['oasis7_required_packages'], legacy['oasis7_required_packages'])
        self.assertNotIn('ordinary_risk_targets', selected)
        self.assertTrue(any('isolated function-body' in reason for reason in selected['reasons']))

    def test_topology_tests_manifests_and_mixed_core_inputs_retain_full_cases(self):
        for path, contents in (
                (self.source, self.body + 'pub fn helper() {}\n'),
                (self.anchor, self.root.replace('1', '2')),
                ('crates/core/Cargo.toml', Path('crates/core/Cargo.toml').read_text() + '\n# changed\n'),
                ('crates/core/tests/scale.rs',
                 '#[cfg(feature="test_tier_full")] #[test] fn full_scale() { assert!(false); }\n')):
            with self.subTest(path=path):
                self.git('checkout', '-q', self.base)
                self.write(self.source, self.body.replace('1', '2'))
                self.write(path, contents)
                selected = self.select(self.commit())
                self.assertEqual(selected['ordinary_risk_targets'], [self.target])
                self.assertEqual(selected['risk_test_inventory_version'], 1)

    def test_merge_difference_and_legacy_policy_cannot_borrow_candidate_exemption(self):
        self.write(self.source, self.body.replace('1', '2'))
        source = self.commit()
        self.git('checkout', '-q', self.base)
        self.write(self.anchor, self.root + 'pub fn new_core_input() {}\n')
        self.commit()
        self.git('merge', '--no-ff', '-qm', 'tested merge', source)
        tested = self.git('rev-parse', 'HEAD')
        self.assertEqual(self.select(source, tested)['ordinary_risk_targets'], [self.target])
        old = copy.deepcopy(self.config)
        old.pop('ordinary_scale_contract')
        self.assertNotIn('ordinary_risk_targets', self.select(source, tested, old))
        for profile in ('periodic-full', 'candidate-full'):
            with self.subTest(profile=profile):
                complete = selector.full_plan(self.config, profile, source, policy_ref=self.base)
                self.assertNotIn('ordinary_risk_targets', complete)

    def test_body_also_read_as_consumer_data_retains_full_scale_contract(self):
        self.write('crates/consumer/src/lib.rs',
                   'pub const INPUT: &str = include_str!("../../core/src/feedback.rs");\n')
        self.base = self.commit()
        self.write(self.source, self.body.replace('1', '2'))
        selected = self.select(self.commit())
        self.assertEqual(selected['ordinary_risk_targets'], [self.target])


class TargetedRuntimePartitionPlans(_ScopeFixtureBase):
    source = 'crates/oasis7_chain_runtime/src/host/node_lifecycle/formal_full_sync.rs'

    def setUp(self):
        super().setUp()
        self.config = json.loads((HERE / 'ci-required-scope.json').read_text())
        self.write('crates/oasis7_chain_runtime/Cargo.toml',
                   '[package]\nname="oasis7_chain_runtime"\nversion="0.1.0"\n'
                   '[features]\ndefault=["wasmtime","node-libp2p"]\nwasmtime=[]\nnode-libp2p=[]\n'
                   'test_tier_required=[]\ntest_tier_full=["test_tier_required"]\n')
        self.write('crates/oasis7_chain_runtime/src/lib.rs',
                   '#[path="host/node_lifecycle/formal_full_sync.rs"] mod formal_full_sync;\n')
        self.write(self.source, 'pub fn target_cut() -> u64 { 1 }\n')
        self.write('scripts/ci-required-scope.json', json.dumps(self.config))
        self.base = self.commit()

    def test_targeted_chain_runtime_retains_original_plan_and_all_build_inputs(self):
        self.write(self.source, 'pub fn target_cut() -> u64 { 2 }\n')
        head = self.commit()
        for profile in ('pr-fast', 'main-fast'):
            with self.subTest(profile=profile):
                current = selector.select(self.config, self.base, head, profile=profile)
                legacy = selector.select(self.config | {'ordinary_artifact_partition_version': 1},
                                         self.base, head, profile=profile)
                self.assertEqual(current['scope'], 'targeted')
                self.assertEqual(current['ordinary_artifact_partition_version'], 2)
                self.assertEqual(current['ordinary_dynamic_partition_version'], 1)
                self.assertEqual(current['ordinary_universal_sharding_version'], 1)
                self.assertEqual(current['ordinary_controls_in_rust_baseline_version'], 1)
                self.assertEqual(current['ordinary_clippy_group'], 'rust_baseline')
                self.assertEqual(current['ordinary_wasm_group'], 'rust_baseline')
                self.assertEqual(current['ordinary_partitions'],
                                 ['native-0', 'native-1'])
                self.assertEqual(current['oasis7_required_packages'], ['oasis7_chain_runtime'])
                self.assertIs(current['test_policy_changed'], False)
                self.assertNotIn('ordinary_fast_targets', current)
                self.assertNotIn('fast_execution_version', current)
                self.assertEqual(current['architecture']['build_policy_version'], 2)
                self.assertEqual(current['architecture']['build_policy'], 'ordinary-targets')
                self.assertNotIn('build_policy', legacy['architecture'])
                arch_reason = ('all architecture inputs have mandatory ordinary artifact and strict Clippy coverage; '
                               'extra profile compilation stays in full, all dependency graphs remain required')
                self.assertIn(arch_reason, current['reasons'])
                comparable_current = {
                    key: value for key, value in current.items()
                    if key not in ('ordinary_artifact_partition_version', 'ordinary_partitions',
                                   'ordinary_dynamic_partition_version', 'ordinary_universal_sharding_version',
                                   'ordinary_controls_in_rust_baseline_version',
                                   'ordinary_clippy_group', 'ordinary_wasm_group',
                                   'ordinary_harness_costs', 'ordinary_artifact_delivery_version')
                }
                comparable_current['architecture'] = {
                    key: value for key, value in current['architecture'].items()
                    if key not in ('build_policy_version', 'build_policy')
                }
                comparable_current['reasons'] = [r for r in current['reasons'] if r != arch_reason]
                self.assertEqual(comparable_current, legacy)
                heavy_plan = dict(current)
                heavy_plan.pop('ordinary_dynamic_partition_version', None)
                heavy_plan['oasis7_required_packages'] = ['oasis7', 'oasis7_chain_runtime']
                selector.complete_ordinary_partitions(self.config, heavy_plan)
                self.assertNotIn('ordinary_dynamic_partition_version', heavy_plan)
                self.assertEqual(heavy_plan['ordinary_partitions'],
                                 ['native-0', 'native-1', 'native-2', 'native-3'])
                legacy_controls = dict(heavy_plan)
                for key in ('ordinary_controls_in_rust_baseline_version', 'ordinary_clippy_group', 'ordinary_wasm_group'):
                    legacy_controls.pop(key, None)
                selector.complete_ordinary_partitions(
                    {k: v for k, v in self.config.items() if k != 'ordinary_controls_in_rust_baseline_version'},
                    legacy_controls,
                )
                self.assertEqual(legacy_controls['ordinary_partitions'],
                                 ['controls', 'native-0', 'native-1', 'native-2', 'native-3'])

    def test_full_fast_uses_case_partitions_without_changing_the_original_selected_checks(self):
        self.write('Cargo.toml', Path('Cargo.toml').read_text() + '\n# shared control input\n')
        head = self.commit()
        current_config = self.config | {'ordinary_artifact_delivery_version': 2}
        old_config = self.config | {'ordinary_artifact_delivery_version': 1}
        physical = {'ordinary_artifact_partition_version', 'ordinary_partitions',
                    'ordinary_artifact_delivery_version', 'ordinary_harness_costs',
                    'ordinary_universal_sharding_version'}
        for profile in ('pr-fast', 'main-fast'):
            with self.subTest(profile=profile):
                current = selector.select(current_config, self.base, head, profile=profile)
                legacy = selector.select(old_config, self.base, head, profile=profile)
                self.assertEqual(current['scope'], 'full')
                self.assertIs(current['test_policy_changed'], False)
                self.assertEqual(current['ordinary_artifact_partition_version'], 2)
                self.assertEqual(current['ordinary_artifact_delivery_version'], 2)
                self.assertNotIn('ordinary_universal_sharding_version', current)
                self.assertEqual(current['ordinary_partitions'],
                                 ['native-0', 'native-1', 'native-2', 'native-3'])
                self.assertEqual((legacy['ordinary_artifact_partition_version'],
                                  legacy['ordinary_artifact_delivery_version']), (1, 1))
                self.assertEqual({key: value for key, value in current.items() if key not in physical},
                                 {key: value for key, value in legacy.items() if key not in physical})
                self.assertTrue(current['ordinary_risk_targets'])
                self.assertEqual(sum(len(target['cases']) for target in current['ordinary_risk_targets']), 5)

    def test_main_fast_cache_capability_preserves_complete_v1_v2_and_expanded_builds(self):
        field = 'ordinary_main_fast_cache_writer_version'
        self.write('Cargo.toml', Path('Cargo.toml').read_text() + '\n# shared control input\n')
        for expanded in (False, True):
            if expanded:
                self.write(self.source, 'pub fn target_cut() -> u64 { 2 }\n'
                           '#[cfg(feature="test_tier_full")] #[test] fn added_full_case() {}\n')
            head = self.commit()
            for delivery in (1, 2):
                enabled = self.config | {field: 1, 'ordinary_artifact_delivery_version': delivery}
                disabled = {key: value for key, value in enabled.items() if key != field}
                with self.subTest(expanded=expanded, delivery=delivery):
                    plan = selector.select(enabled, self.base, head, profile='main-fast')
                    self.assertEqual(plan['scope'], 'full')
                    self.assertIs(plan['test_policy_changed'], expanded)
                    self.assertEqual(plan['ordinary_artifact_partition_version'],
                                     1 if expanded or delivery == 1 else 2)
                    self.assertEqual(plan['oasis7_required_packages'], sorted(enabled['oasis7_required_packages']))
                    self.assertNotIn('fast_execution_version', plan)
                    self.assertEqual(plan.pop(field), 1)
                    self.assertEqual(plan, selector.select(disabled, self.base, head, profile='main-fast'))
                    self.assertNotIn(field, selector.select(enabled, self.base, head, profile='pr-fast'))
            for profile in ('periodic-full', 'candidate-full'):
                self.assertNotIn(field, selector.full_plan(self.config | {field: 1}, profile, head))

    def test_main_fast_cache_capability_cannot_self_enrol_or_cover_partial_plans(self):
        field = 'ordinary_main_fast_cache_writer_version'
        old = {key: value for key, value in self.config.items() if key != field}
        self.write('scripts/ci-required-scope.json', json.dumps(old))
        self.base = self.commit()
        enabled = old | {field: 1}
        self.write(self.source, 'pub fn target_cut() -> u64 { 2 }\n')
        targeted = self.commit()
        self.assertNotIn(field, selector.select(enabled, self.base, targeted, profile='main-fast'))
        self.write('scripts/ci-required-scope.json', json.dumps(enabled))
        candidate = self.commit()
        legacy = selector.select(old, self.base, candidate, profile='main-fast')
        self.assertEqual(legacy['scope'], 'full')
        self.assertNotIn(field, legacy)
        self.assertEqual(selector.select(enabled, self.base, candidate, profile='main-fast')[field], 1)
        without_partitions = {key: value for key, value in enabled.items() if key not in (
            'ordinary_artifact_partition_version', 'ordinary_artifact_delivery_version', 'ordinary_harness_costs')}
        self.assertNotIn(field, selector.select(without_partitions, self.base, candidate, profile='main-fast'))
        inline = dict(scope='full', groups=['oasis7_required'], execution_profile='main-fast',
                      ordinary_execution='inline', fast_execution_version=1)
        selector.complete_ordinary_partitions(enabled, inline)
        self.assertNotIn(field, inline)

    def test_main_fast_cache_capability_accepts_only_the_known_integer_version(self):
        field = 'ordinary_main_fast_cache_writer_version'
        selector.validate_config({key: value for key, value in self.config.items() if key != field})
        selector.validate_config(self.config | {field: 1})
        for invalid in (0, 2, True, False, 1.0, '1', None):
            with self.subTest(version=invalid), self.assertRaisesRegex(ValueError, 'main-fast cache writer'):
                selector.validate_config(self.config | {field: invalid})

    def test_cost_hints_require_trusted_configuration_and_only_fast_v2_consumes_them(self):
        self.write(self.source, 'pub fn target_cut() -> u64 { 2 }\n')
        head = self.commit()
        old = dict(self.config)
        profile = old.pop('ordinary_harness_costs')
        for execution in ('pr-fast', 'main-fast'):
            current = selector.select(self.config, self.base, head, profile=execution)
            legacy = selector.select(old, self.base, head, profile=execution)
            self.assertEqual(current.pop('ordinary_harness_costs'), profile)
            self.assertEqual(current.pop('ordinary_dynamic_partition_version'), 1)
            self.assertEqual(current.pop('ordinary_universal_sharding_version'), 1)
            self.assertEqual(current['ordinary_partitions'], ['native-0', 'native-1'])
            self.assertEqual(legacy['ordinary_partitions'],
                             ['native-0', 'native-1', 'native-2', 'native-3'])
            current['ordinary_partitions'] = legacy['ordinary_partitions']
            self.assertEqual(current, legacy)
        for execution in ('periodic-full', 'candidate-full'):
            self.assertNotIn('ordinary_harness_costs', selector.full_plan(self.config, execution, head))
        self.write('Cargo.toml', Path('Cargo.toml').read_text() + '\n# shared control\n')
        head = self.commit()
        self.assertEqual(selector.select(self.config, self.base, head, profile='pr-fast')['ordinary_harness_costs'], profile)
        self.assertNotIn('ordinary_harness_costs', selector.select(
            self.config | {'ordinary_artifact_delivery_version': 1}, self.base, head, profile='pr-fast'))
        row = profile['harnesses'][0]
        for change in ({'version': True}, {'fallback_ms': 0}, {'harnesses': [row, row]},
                       {'evidence': {}}, {'harnesses': [row | {'elapsed_ms': float('nan')}]},
                       {'harnesses': [row | {'mode': 'skip'}]}):
            with self.subTest(change=change), self.assertRaises(ValueError):
                selector.validate_config(self.config | {'ordinary_harness_costs': profile | change})

    def test_old_baseline_cannot_be_enrolled_by_candidate_configuration(self):
        self.write(self.source, 'pub fn target_cut() -> u64 { 2 }\n')
        head = self.commit()
        self.write('scripts/ci-required-scope.json', json.dumps(self.config) + '\n')
        candidate_head = self.commit()
        for version in (None, 1):
            old = copy.deepcopy(self.config)
            if version is None:
                old.pop('ordinary_artifact_partition_version')
                old.pop('ordinary_artifact_delivery_version', None)
                old.pop('ordinary_harness_costs', None)
            else:
                old['ordinary_artifact_partition_version'] = version
            with self.subTest(version=version):
                selected = selector.select(old, self.base, head, profile='pr-fast')
                self.assertNotIn('ordinary_partitions', selected)
                candidate = selector.select(old, self.base, candidate_head, profile='pr-fast')
                self.assertNotEqual(candidate.get('ordinary_artifact_partition_version'), 2)

    def test_full_profiles_and_policy_changes_keep_the_existing_complete_path(self):
        self.write(self.source, 'pub fn target_cut() -> u64 { 2 }\n'
                   '#[cfg(feature="test_tier_full")] #[test] fn added_full_case() {}\n')
        head = self.commit()
        legacy_cfg = {k: v for k, v in self.config.items() if k != 'ordinary_universal_sharding_version'}
        legacy_changed = selector.select(legacy_cfg, self.base, head, profile='pr-fast')
        self.assertIs(legacy_changed['test_policy_changed'], True)
        self.assertNotEqual(legacy_changed.get('ordinary_artifact_partition_version'), 2)
        self.assertNotEqual(legacy_changed.get('ordinary_artifact_delivery_version'), 2)
        changed = selector.select(self.config, self.base, head, profile='pr-fast')
        self.assertIs(changed['test_policy_changed'], True)
        self.assertEqual(changed['ordinary_universal_sharding_version'], 1)
        self.assertEqual(changed['ordinary_artifact_partition_version'], 2)
        self.assertEqual(changed['ordinary_artifact_delivery_version'], 2)
        self.assertEqual(changed['ordinary_partitions'], ['native-0', 'native-1', 'native-2', 'native-3'])
        self.assertNotIn('ordinary_dynamic_partition_version', changed)
        self.assertNotIn('build_policy', changed['architecture'])
        for profile in ('periodic-full', 'candidate-full'):
            with self.subTest(profile=profile):
                complete = selector.full_plan(self.config, profile, head)
                self.assertEqual(complete['ordinary_artifact_partition_version'], 1)
                self.assertEqual(complete['ordinary_artifact_delivery_version'], 1)
                self.assertNotIn('ordinary_universal_sharding_version', complete)
                self.assertEqual(complete['ordinary_partitions'],
                                 ['runtime', 'component-libs', 'integrations-0', 'integrations-1'])

    def test_targeted_policy_change_partitions_preserve_the_complete_original_plan(self):
        for package in ('oasis7', 'oasis7_viewer_runtime'):
            self.write(f'crates/{package}/Cargo.toml',
                       f'[package]\nname="{package}"\nversion="0.1.0"\n'
                       '[dependencies]\noasis7_chain_runtime={path="../oasis7_chain_runtime"}\n'
                       '[features]\ntest_tier_required=[]\ntest_tier_full=["test_tier_required"]\n')
            self.write(f'crates/{package}/src/lib.rs', '')
        self.base = self.commit()
        self.write(self.source, 'pub fn target_cut() -> u64 { 2 }\n'
                   '#[cfg(feature="test_tier_full")] #[test] fn added_full_case() {}\n')
        head = self.commit()
        physical = {'ordinary_artifact_partition_version', 'ordinary_partitions',
                    'ordinary_artifact_delivery_version', 'ordinary_harness_costs',
                    'ordinary_universal_sharding_version',
                    'ordinary_controls_in_rust_baseline_version',
                    'ordinary_clippy_group', 'ordinary_wasm_group'}
        legacy_config = {key: value for key, value in self.config.items() if key not in physical}
        v1_policy_config = {key: value for key, value in self.config.items()
                            if key != 'ordinary_universal_sharding_version'}
        for profile in ('pr-fast', 'main-fast'):
            with self.subTest(profile=profile):
                selected = selector.select(self.config, self.base, head, profile=profile)
                v1_selected = selector.select(v1_policy_config, self.base, head, profile=profile)
                legacy = selector.select(legacy_config, self.base, head, profile=profile)
                self.assertEqual(selected['scope'], 'targeted')
                self.assertIs(selected['test_policy_changed'], True)
                self.assertEqual(selected['oasis7_required_packages'],
                                 ['oasis7', 'oasis7_chain_runtime', 'oasis7_viewer_runtime'])
                self.assertEqual(selected['ordinary_universal_sharding_version'], 1)
                self.assertEqual(selected['ordinary_artifact_partition_version'], 2)
                self.assertEqual(selected['ordinary_artifact_delivery_version'], 2)
                self.assertEqual(selected['ordinary_partitions'],
                                 ['native-0', 'native-1', 'native-2', 'native-3'])
                self.assertEqual(v1_selected['ordinary_artifact_partition_version'], 1)
                self.assertEqual(v1_selected['ordinary_artifact_delivery_version'], 1)
                self.assertEqual(v1_selected['ordinary_partitions'],
                                 ['runtime', 'component-libs', 'integrations-0', 'integrations-1'])
                self.assertNotIn('ordinary_partitions', legacy)
                self.assertEqual({key: value for key, value in selected.items() if key not in physical}, legacy)
                output = Path('github-output-' + profile)
                with patch.object(sys, 'argv', ['planner', '--base-ref', self.base, '--head-ref', head,
                        '--profile', profile, '--config', str(HERE / 'ci-required-scope.json'),
                        '--formal-config', str(Path.cwd() / 'absent-formal.json'), '--github-output', str(output)]), \
                     contextlib.redirect_stdout(io.StringIO()):
                    selector.main()
                projected = dict(line.split('=', 1) for line in output.read_text().splitlines())
                self.assertEqual(json.loads(projected['plan']), selected)
                self.assertEqual(json.loads(projected['ordinary_partitions']), selected['ordinary_partitions'])
                self.assertEqual(projected['run_oasis7_required'], 'true')

    def test_policy_partition_admission_requires_the_complete_layout_and_execution_contract(self):
        original = dict(scope='targeted', groups=['oasis7_required'], execution_profile='pr-fast',
                        test_policy_changed=True,
                        oasis7_required_packages=['oasis7', 'oasis7_chain_runtime', 'oasis7_viewer_runtime'])
        legacy_cfg = {k: v for k, v in self.config.items() if k != 'ordinary_universal_sharding_version'}
        changes = [
            ('oasis7_required_packages', [package for package in original['oasis7_required_packages']
                                         if package != missing])
            for missing in original['oasis7_required_packages']
        ] + [
            ('oasis7_required_packages', None), ('oasis7_required_packages', 'oasis7'),
            ('ordinary_execution', 'inline'), ('fast_execution_version', 1),
            ('ordinary_partition_version', 1), ('ordinary_fast_targets', []),
            ('ordinary_full_fast_targets', []), ('groups', ['baseline']),
            ('execution_profile', 'candidate-full'), ('execution_profile', 'periodic-full'),
            *[('test_policy_changed', value) for value in (None, 1, 0, '', [], {}, 'true')],
        ]
        for key, value in changes:
            with self.subTest(key=key, value=value):
                selected = original | {key: value}
                selector.complete_ordinary_partitions(legacy_cfg, selected)
                self.assertNotIn('ordinary_artifact_partition_version', selected)
                self.assertNotIn('ordinary_artifact_delivery_version', selected)
        for version in (1, 2):
            selected = copy.deepcopy(original)
            selector.complete_ordinary_partitions(legacy_cfg | {'ordinary_artifact_partition_version': version}, selected)
            self.assertEqual(selected['ordinary_artifact_partition_version'], 1)
            self.assertEqual(selected['ordinary_artifact_delivery_version'], 1)
        selected_v2 = copy.deepcopy(original)
        selector.complete_ordinary_partitions(self.config, selected_v2)
        self.assertEqual(selected_v2['ordinary_universal_sharding_version'], 1)
        self.assertEqual(selected_v2['ordinary_artifact_partition_version'], 2)
        self.assertEqual(selected_v2['ordinary_artifact_delivery_version'], 2)
        old = {key: value for key, value in self.config.items() if key not in (
            'ordinary_artifact_partition_version', 'ordinary_artifact_delivery_version',
            'ordinary_harness_costs', 'ordinary_universal_sharding_version')}
        selected = copy.deepcopy(original)
        selector.complete_ordinary_partitions(old, selected)
        self.assertEqual(selected, original)

    def test_only_eligible_noninline_chain_runtime_with_explicit_unchanged_policy_is_partitioned(self):
        original = dict(scope='targeted', groups=['oasis7_required'], execution_profile='pr-fast',
                        test_policy_changed=False, oasis7_required_packages=['oasis7_chain_runtime'])
        legacy_cfg = {k: v for k, v in self.config.items() if k != 'ordinary_universal_sharding_version'}
        for key, value in [
            ('ordinary_execution', 'inline'), ('oasis7_required_packages', ['oasis7_runtime']),
            ('groups', ['baseline']), ('execution_profile', 'candidate-full'),
            *[('test_policy_changed', item) for item in (None, True, 0, '', [], {}, 'false')],
        ]:
            with self.subTest(key=key, value=value):
                selected = original | {key: value}
                selector.complete_ordinary_partitions(legacy_cfg, selected)
                self.assertNotIn('ordinary_artifact_partition_version', selected)
        # Under universal sharding, multi-harness packages (>=4 costed harnesses) and boolean test_policy_changed
        # enroll into v2, while single-harness non-chain_runtime packages and non-boolean policy values remain excluded.
        for packages, expected_partitions in (
                (['oasis7_runtime'], ['controls', 'native-0', 'native-1', 'native-2', 'native-3']),
                (['oasis7_world_state'], ['controls', 'native-0', 'native-1', 'native-2', 'native-3']),
                (['oasis7_game_client'], ['controls', 'native-0', 'native-1']),
        ):
            selected = original | {'oasis7_required_packages': packages}
            selector.complete_ordinary_partitions(self.config, selected)
            self.assertEqual(selected['ordinary_universal_sharding_version'], 1)
            self.assertEqual(selected['ordinary_artifact_partition_version'], 2)
            self.assertEqual(selected['ordinary_partitions'], expected_partitions)
        single_leaf = original | {'oasis7_required_packages': ['oasis7_cas']}
        selector.complete_ordinary_partitions(self.config, single_leaf)
        self.assertNotIn('ordinary_artifact_partition_version', single_leaf)
        for bad_policy in (None, 0, 1, '', [], {}, 'false'):
            invalid_policy = original | {'test_policy_changed': bad_policy}
            selector.complete_ordinary_partitions(self.config, invalid_policy)
            self.assertNotIn('ordinary_artifact_partition_version', invalid_policy)
        for version in (None, False, True, 0, 3, '2'):
            with self.subTest(version=version), self.assertRaisesRegex(ValueError, 'ordinary artifact'):
                selector.validate_config(self.config | {'ordinary_artifact_partition_version': version})
        for invalid in (None, False, True, 0, 2, '1', 1.0):
            with self.subTest(universal=invalid), self.assertRaisesRegex(ValueError, 'ordinary universal sharding'):
                selector.validate_config(self.config | {'ordinary_universal_sharding_version': invalid})
        selector.validate_config(self.config | {'ordinary_artifact_partition_version': 1})
        selector.validate_config(self.config)

    def test_delivery_versions_preserve_old_authority_and_complete_profiles(self):
        self.write(self.source, 'pub fn target_cut() -> u64 { 2 }\n')
        head = self.commit()
        old = dict(self.config)
        old.pop('ordinary_artifact_delivery_version')
        for profile in ('periodic-full', 'candidate-full'):
            current = selector.full_plan(self.config, profile, head)
            legacy = selector.full_plan(old, profile, head)
            self.assertEqual(current.pop('ordinary_artifact_delivery_version'), 1)
            self.assertEqual(current, legacy)
        for profile in ('pr-fast', 'main-fast'):
            current = selector.select(self.config, self.base, head, profile=profile)
            self.assertEqual(current.pop('ordinary_artifact_delivery_version'), 2)
            self.assertEqual(current.pop('ordinary_universal_sharding_version'), 1)
            for legacy_config in (old, self.config | {'ordinary_artifact_delivery_version': 1}):
                legacy = selector.select(legacy_config, self.base, head, profile=profile)
                self.assertEqual(legacy['ordinary_artifact_partition_version'], 2)
                self.assertNotIn('ordinary_artifact_delivery_version', legacy)
                self.assertNotIn('ordinary_universal_sharding_version', legacy)
                self.assertEqual(current, legacy)
        # A candidate with delivery=2 cannot enroll the old BASE in either the
        # new full/fast partition or the new transport, even on a policy fallback.
        self.write('scripts/ci-required-scope.json', json.dumps(self.config) + '\n')
        self.write(self.source, 'pub fn target_cut() -> u64 { 2 }\n'
                   '#[cfg(feature="test_tier_full")] #[test] fn added_full_case() {}\n')
        migration = self.commit()
        for config in (old, self.config | {'ordinary_artifact_delivery_version': 1}, self.config):
            current = selector.select(config, self.base, migration, profile='pr-fast')
            self.assertIs(current['test_policy_changed'], True)
            self.assertEqual(current['ordinary_artifact_partition_version'], 1)
            self.assertEqual(current.get('ordinary_artifact_delivery_version'),
                             None if config is old else 1)
        for version in (True, False, None, 0, 3, '1'):
            with self.subTest(version=version), self.assertRaises(ValueError):
                selector.validate_config(self.config | {'ordinary_artifact_delivery_version': version})
        for partition_version in (1, 2):
            for delivery_version in (1, 2):
                selector.validate_config(self.config | {'ordinary_artifact_partition_version': partition_version,
                                                       'ordinary_artifact_delivery_version': delivery_version})

    def test_full_fast_keeps_v1_when_any_existing_case_partition_condition_is_missing(self):
        original = dict(scope='full', groups=['oasis7_required'], execution_profile='pr-fast',
                        test_policy_changed=False, oasis7_required_packages=['oasis7_chain_runtime'])
        for key, value in [('scope', 'explicit-full'), ('execution_profile', 'periodic-full'),
                           ('execution_profile', 'candidate-full'), ('test_policy_changed', True),
                           ('test_policy_changed', None), ('ordinary_fast_targets', []),
                           ('ordinary_full_fast_targets', []), ('oasis7_required_packages', ['oasis7_runtime'])]:
            with self.subTest(key=key, value=value):
                selected = original | {key: value}
                selector.complete_ordinary_partitions(self.config, selected)
                self.assertEqual(selected['ordinary_artifact_partition_version'], 1)
                self.assertEqual(selected['ordinary_artifact_delivery_version'], 1)
                self.assertNotIn('ordinary_harness_costs', selected)

    def test_architecture_artifact_build_policy_v2_excludes_manifests_cfg_and_historical_sources(self):
        # Historical tested_sha lacking architecture_artifact_build_policy_version withholds v2.
        historical_cfg = {k: v for k, v in self.config.items() if k != 'architecture_artifact_build_policy_version'}
        self.write('scripts/ci-required-scope.json', json.dumps(historical_cfg))
        historical_base = self.commit()
        self.write(self.source, 'pub fn target_cut() -> u64 { 2 }\n')
        historical_tested = self.commit()
        self.write('scripts/ci-required-scope.json', json.dumps(self.config))
        new_policy = self.commit()
        mixed = selector.select(
            self.config,
            historical_base,
            historical_tested,
            historical_tested,
            profile='pr-fast',
            policy_ref=new_policy,
        )
        self.assertEqual(mixed['scope'], 'targeted')
        self.assertNotIn('build_policy', mixed['architecture'])

        # Candidate cannot self-enrol when BASE policy lacks architecture_artifact_build_policy_version.
        self.assertNotIn(
            'build_policy',
            selector.select(historical_cfg, historical_base, new_policy, profile='pr-fast')['architecture'],
        )

        # Manifest edits and scoped production #[cfg(...)] changes retain extra architecture profile checks.
        self.git('checkout', '-q', self.base)
        self.write('crates/oasis7_chain_runtime/Cargo.toml',
                   Path('crates/oasis7_chain_runtime/Cargo.toml').read_text() + '\n# manifest comment\n')
        manifest_head = self.commit()
        self.assertNotIn(
            'build_policy',
            selector.select(self.config, self.base, manifest_head, profile='pr-fast')['architecture'],
        )

        self.git('checkout', '-q', self.base)
        self.write(self.source, '#[cfg(unix)]\npub fn target_cut() -> u64 { 3 }\n')
        cfg_head = self.commit()
        cfg_plan = selector.select(self.config, self.base, cfg_head, profile='pr-fast')
        self.assertEqual(cfg_plan['scope'], 'targeted')
        self.assertNotIn('build_policy', cfg_plan['architecture'])

    def test_universal_sharding_withheld_for_historical_tested_source_and_candidate_self_enrol(self):
        historical_cfg = {k: v for k, v in self.config.items() if k != 'ordinary_universal_sharding_version'}
        self.write('scripts/ci-required-scope.json', json.dumps(historical_cfg))
        historical_base = self.commit()
        self.write(self.source, 'pub fn target_cut() -> u64 { 2 }\n'
                   '#[cfg(feature="test_tier_full")] #[test] fn added_full_case() {}\n')
        historical_tested = self.commit()
        self.write('scripts/ci-required-scope.json', json.dumps(self.config))
        new_policy = self.commit()
        mixed = selector.select(
            self.config,
            historical_base,
            historical_tested,
            historical_tested,
            profile='pr-fast',
            policy_ref=new_policy,
        )
        self.assertIs(mixed['test_policy_changed'], True)
        self.assertNotIn('ordinary_universal_sharding_version', mixed)
        self.assertNotIn('ordinary_artifact_partition_version', mixed)

        # Candidate cannot self-enrol when BASE policy lacks ordinary_universal_sharding_version.
        candidate_attempt = selector.select(historical_cfg, historical_base, new_policy, profile='pr-fast')
        self.assertNotIn('ordinary_universal_sharding_version', candidate_attempt)

    def test_runtime_full_wasm_plans_withhold_dockerless_v2_and_keep_v1_host_shards(self):
        runtime_policy_plan = dict(
            scope='targeted',
            groups=['oasis7_required'],
            execution_profile='pr-fast',
            test_policy_changed=True,
            oasis7_required_packages=['oasis7', 'oasis7_chain_runtime', 'oasis7_runtime', 'oasis7_viewer_runtime'],
        )
        selector.complete_ordinary_partitions(self.config, runtime_policy_plan)
        self.assertNotIn('ordinary_universal_sharding_version', runtime_policy_plan)
        self.assertEqual(runtime_policy_plan['ordinary_artifact_partition_version'], 1)
        self.assertEqual(runtime_policy_plan['ordinary_artifact_delivery_version'], 1)
        self.assertEqual(
            runtime_policy_plan['ordinary_partitions'],
            ['controls', 'runtime', 'component-libs', 'integrations-0', 'integrations-1'],
        )

        non_runtime_policy_plan = dict(
            scope='targeted',
            groups=['oasis7_required'],
            execution_profile='pr-fast',
            test_policy_changed=True,
            oasis7_required_packages=['oasis7', 'oasis7_chain_runtime', 'oasis7_viewer_runtime'],
        )
        selector.complete_ordinary_partitions(self.config, non_runtime_policy_plan)
        self.assertEqual(non_runtime_policy_plan['ordinary_universal_sharding_version'], 1)
        self.assertEqual(non_runtime_policy_plan['ordinary_artifact_partition_version'], 2)
        self.assertEqual(non_runtime_policy_plan['ordinary_artifact_delivery_version'], 2)

        runtime_lib_risk_plan = dict(
            scope='targeted',
            groups=['oasis7_required'],
            execution_profile='pr-fast',
            test_policy_changed=False,
            oasis7_required_packages=['oasis7_chain_runtime', 'oasis7_runtime'],
            ordinary_risk_targets=[dict(
                package='oasis7_runtime', target_kind='lib', target_name='oasis7_runtime',
                default_features=True, features=['test_tier_full'], cases=['wasm_case'],
            )],
        )
        selector.complete_ordinary_partitions(self.config, runtime_lib_risk_plan)
        self.assertNotIn('ordinary_universal_sharding_version', runtime_lib_risk_plan)
        self.assertNotIn('ordinary_artifact_partition_version', runtime_lib_risk_plan)

        runtime_test_risk_plan = dict(
            runtime_lib_risk_plan,
            ordinary_risk_targets=[dict(
                package='oasis7_runtime', target_kind='test', target_name='incremental_full_scales',
                default_features=True, features=['test_tier_full'], cases=['scale_case'],
            )],
        )
        selector.complete_ordinary_partitions(self.config, runtime_test_risk_plan)
        self.assertEqual(runtime_test_risk_plan['ordinary_artifact_partition_version'], 2)
        self.assertEqual(runtime_test_risk_plan['ordinary_artifact_delivery_version'], 2)

    def test_architecture_artifact_build_policy_v3_covers_portable_wasm_and_extended_consumers(self):
        self.write('crates/oasis7_viewer_runtime/Cargo.toml',
                   '[package]\nname="oasis7_viewer_runtime"\nversion="0.1.0"\n'
                   '[dependencies]\noasis7_chain_runtime={path="../oasis7_chain_runtime"}\n'
                   '[features]\ntest_tier_required=[]\ntest_tier_full=["test_tier_required"]\n')
        self.write('crates/oasis7_viewer_runtime/src/lib.rs', '')
        self.write('crates/oasis7_browser_game_client/Cargo.toml',
                   '[package]\nname="oasis7_browser_game_client"\nversion="0.1.0"\n'
                   '[dependencies]\noasis7_viewer_runtime={path="../oasis7_viewer_runtime"}\n')
        self.write('crates/oasis7_browser_game_client/src/lib.rs', '')
        self.base = self.commit()
        self.write(self.source, 'pub fn target_cut() -> u64 { 2 }\n')
        head = self.commit()
        selected = selector.select(self.config, self.base, head, profile='pr-fast')
        self.assertEqual(selected['scope'], 'targeted')
        self.assertEqual(selected['oasis7_required_wasm_packages'], ['oasis7_viewer_runtime'])
        self.assertIn('viewer_js_required', selected['groups'])
        self.assertEqual(selected['architecture']['build_policy_version'], 4)
        self.assertEqual(selected['architecture']['build_policy'], 'ordinary-targets')
        self.assertEqual(
            selected['architecture']['consumer_groups']['viewer_js_required'],
            ['oasis7_browser_game_client'],
        )


if __name__ == '__main__':
    unittest.main()
