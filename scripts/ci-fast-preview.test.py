#!/usr/bin/env python3
"""Exercise diagnostic Git identities, fresh-runner restoration, and coverage guards."""
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location('preview', ROOT / 'scripts/ci-fast-preview.py')
PREVIEW = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PREVIEW)


def command(repo, *args):
    return subprocess.check_output(['git', '-C', str(repo), *args], text=True, stderr=subprocess.PIPE).strip()


def fixture(directory):
    repo = Path(directory) / 'repo'
    repo.mkdir()
    command(repo, 'init', '-q', '-b', 'main')
    command(repo, 'config', 'user.name', 'CI fixture')
    command(repo, 'config', 'user.email', 'fixture@example.invalid')
    for path in PREVIEW.SAMPLE_PATHS:
        destination = repo / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text('pub fn state_contract() {}\n')
    (repo / 'scripts').mkdir()
    (repo / 'scripts/ci-required-scope.json').write_text('{}\n')
    (repo / 'README.md').write_text('test fixture\n')
    command(repo, 'add', '.')
    command(repo, 'commit', '-qm', 'baseline')
    return repo, command(repo, 'rev-parse', 'HEAD')


def valid_plan(source):
    return {'scope': 'targeted', 'execution_profile': 'pr-fast',
            'fast_execution_version': 1, 'ordinary_execution': 'inline',
            'ordinary_clippy_group': 'rust_baseline',
            'test_policy_changed': False, 'fast_plan_exceptions': [],
            'rust_baseline_checks': [], 'oasis7_required_wasm_packages': [],
            'groups': sorted(PREVIEW.EXPECTED_GROUPS),
            'source_sha': source, 'tested_sha': source, 'policy_sha': source,
            'architecture': dict(version=1, mode='targeted', source=source, tested=source,
                                 packages=['oasis7_runtime', 'oasis7_world_state'],
                                 build_policy_version=1, build_policy='ordinary-targets'),
            'ordinary_fast_targets': [dict(package=package, target_kind=kind, target_name=name,
                                          default_features=False, features=['test_tier_required'])
                for package, kind, name in sorted(PREVIEW.EXPECTED_TARGETS)]}


def fixture_planner(repo):
    """Run a real planner subprocess without compiling a product workspace."""
    (repo / 'scripts/plan-rust-required-scope.py').write_text("""import argparse,json,pathlib
p=argparse.ArgumentParser()
for key in ('profile','base-ref','head-ref','test-ref','policy-ref','config','formal-config','output'):
 p.add_argument('--'+key,required=True)
a=p.parse_args()
assert a.base_ref != a.head_ref
assert a.head_ref == a.test_ref == a.policy_ref
plan=PLAN
for key in ('source_sha','tested_sha','policy_sha'): plan[key]=a.head_ref
for key in ('source','tested'): plan['architecture'][key]=a.head_ref
pathlib.Path(a.output).write_text(json.dumps(plan))
""".replace('PLAN', repr(valid_plan('unused'))))


def commit(repo, message):
    command(repo, 'add', '.')
    command(repo, 'commit', '-qm', message)
    return command(repo, 'rev-parse', 'HEAD')


def write_files(repo, files):
    for relative, contents in files.items():
        path = repo / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        if contents is None:
            path.unlink()
        else:
            path.write_text(contents)


def merge_fixture(directory, source_changes=None, target_changes=None, merge_changes=None):
    repo, _ = fixture(directory)
    fixture_planner(repo)
    (repo / 'scripts/ci-tests.sh').write_text('# baseline driver\n')
    fork = commit(repo, 'fixture planner and driver')
    command(repo, 'checkout', '-qb', 'candidate')
    write_files(repo, {'README.md': 'candidate docs\n', **(source_changes or {})})
    source = commit(repo, 'source change')
    command(repo, 'checkout', '-q', 'main')
    write_files(repo, {'target-doc.md': 'event target docs\n'})
    base = commit(repo, 'event target')
    target = base
    if target_changes:
        write_files(repo, target_changes)
        target = commit(repo, 'advanced target')
    command(repo, 'merge', '--no-ff', '--no-commit', source)
    write_files(repo, merge_changes or {})
    tested = commit(repo, 'tested merge')
    return repo, base, source, target, tested, fork


class Preview(unittest.TestCase):
    def test_current_source_unchanged_and_diagnostic_baseline_exact(self):
        with tempfile.TemporaryDirectory() as directory:
            repo, source = fixture(directory)
            before_index = command(repo, 'write-tree')
            synthetic, tree = PREVIEW.synthetic_baseline(repo, source)
            self.assertEqual(command(repo, 'rev-parse', 'HEAD'), source)
            self.assertEqual(command(repo, 'status', '--porcelain'), '')
            self.assertEqual(command(repo, 'write-tree'), before_index)
            self.assertEqual(command(repo, 'rev-parse', synthetic + '^'), source)
            self.assertEqual(command(repo, 'rev-parse', synthetic + '^{tree}'), tree)
            self.assertEqual(set(command(repo, 'diff', '--name-only', synthetic, source).splitlines()),
                             set(PREVIEW.SAMPLE_PATHS))
            for path in PREVIEW.SAMPLE_PATHS:
                self.assertEqual(PREVIEW.git(repo, 'show', synthetic + ':' + path),
                                 PREVIEW.git(repo, 'show', source + ':' + path) + PREVIEW.COMMENT)
            self.assertEqual(PREVIEW.synthetic_baseline(repo, source), (synthetic, tree))

    def test_fresh_checkout_reconstructs_same_objects_and_rejects_identity_drift(self):
        with tempfile.TemporaryDirectory() as directory:
            repo, source = fixture(directory)
            synthetic, tree = PREVIEW.synthetic_baseline(repo, source)
            clone = Path(directory) / 'consumer'
            subprocess.run(['git', 'clone', '-q', '--no-local', str(repo), str(clone)], check=True)
            missing = subprocess.run(['git', '-C', str(clone), 'cat-file', '-e', synthetic],
                                     capture_output=True)
            self.assertNotEqual(missing.returncode, 0)
            metadata = dict(schema_version=1, authority='diagnostic-only', workload='synthetic-comment-diff',
                            paths=list(PREVIEW.SAMPLE_PATHS), source_sha=source, tested_sha=source,
                            policy_sha=source, source_tree=command(repo, 'rev-parse', source + '^{tree}'),
                            synthetic_base_sha=synthetic, synthetic_base_tree=tree)
            PREVIEW.restore_baseline(clone, metadata)
            self.assertEqual(command(clone, 'rev-parse', synthetic), synthetic)
            for key in ('source_sha', 'tested_sha', 'policy_sha', 'source_tree', 'synthetic_base_sha', 'synthetic_base_tree'):
                altered = dict(metadata, **{key: '0' * 40})
                with self.subTest(key=key), self.assertRaises(ValueError):
                    PREVIEW.restore_baseline(clone, altered)
            with self.assertRaises(ValueError):
                PREVIEW.restore_baseline(clone, dict(metadata, authority='required'))

    def test_no_policy_change_explicitly_disables_preview(self):
        with tempfile.TemporaryDirectory() as directory:
            repo, base = fixture(directory)
            (repo / 'README.md').write_text('ordinary documentation change\n')
            command(repo, 'commit', '-qam', 'docs')
            output = Path(directory) / 'github-output'
            self.assertIsNone(PREVIEW.plan_preview(repo, base, 'HEAD', Path(directory) / 'plan.json', output))
            self.assertEqual(output.read_text(), 'enabled=false\n')
            self.assertFalse((Path(directory) / 'plan.json').exists())

    def test_python_control_changes_do_not_start_product_rust_preview(self):
        for script in ('plan-rust-required-scope', 'ci-fast-preview',
                       'ci-required-result', 'document_corpus'):
            for suffix in ('.py', '.test.py'):
                with self.subTest(path=script + suffix), tempfile.TemporaryDirectory() as directory:
                    repo, base = fixture(directory)
                    path = repo / 'scripts' / (script + suffix)
                    path.write_text('raise SystemExit("Python contracts must not start a product preview")\n')
                    command(repo, 'add', '.')
                    command(repo, 'commit', '-qm', 'Python contract change')
                    output = Path(directory) / 'github-output'
                    plan = Path(directory) / 'plan.json'
                    self.assertIsNone(PREVIEW.plan_preview(repo, base, 'HEAD', plan, output))
                    self.assertEqual(output.read_text(), 'enabled=false\n')
                    self.assertFalse(plan.exists())
                    self.assertFalse(Path(str(plan) + '.metadata.json').exists())

    def test_advanced_target_policy_change_does_not_start_documentation_preview(self):
        # The PR forks before the event base, then main alone changes a driver
        # before GitHub creates its tested target/source merge (#5393).
        with tempfile.TemporaryDirectory() as directory:
            repo, _ = fixture(directory)
            fixture_planner(repo)
            commit(repo, 'fixture planner')
            command(repo, 'checkout', '-qb', 'candidate')
            (repo / 'README.md').write_text('candidate documentation\n')
            source = commit(repo, 'candidate docs')
            command(repo, 'checkout', '-q', 'main')
            (repo / 'target-doc.md').write_text('target documentation\n')
            base = commit(repo, 'event target')
            (repo / 'scripts/ci-tests.sh').write_text('# target-only driver change\n')
            target = commit(repo, 'main driver advances')
            command(repo, 'merge', '--no-ff', '-qm', 'tested merge', source)
            tested = command(repo, 'rev-parse', 'HEAD')
            self.assertEqual(command(repo, 'show', '-s', '--format=%P', tested).split(),
                             [target, source])
            self.assertIn('scripts/ci-tests.sh',
                          command(repo, 'diff', '--name-only', base, tested).splitlines())
            self.assertEqual(command(repo, 'diff', '--name-only', target, tested), 'README.md')
            output = Path(directory) / 'github-output'
            plan = Path(directory) / 'plan.json'
            self.assertIsNone(PREVIEW.plan_preview(repo, base, tested, plan, output, source_ref=source))
            self.assertEqual(output.read_text(), 'enabled=false\n')
            self.assertFalse(plan.exists())
            self.assertFalse(Path(str(plan) + '.metadata.json').exists())

    def test_real_pr_cli_preserves_source_input_and_records_actual_merge_ranges(self):
        with tempfile.TemporaryDirectory() as directory:
            repo, base, source, target, tested, fork = merge_fixture(
                directory, {'scripts/ci-tests.sh': '# candidate driver\n'},
                {'scripts/ci-required-scope.json': '{"main_only":true}\n'})
            output = Path(directory) / 'plan.json'
            github_output = Path(directory) / 'github-output'
            subprocess.run([sys.executable, str(ROOT / 'scripts/ci-fast-preview.py'),
                            '--repo-root', str(repo), 'plan', '--base-ref', base,
                            '--head-ref', tested, '--source-ref', source,
                            '--output', str(output), '--github-output', str(github_output)],
                           text=True, capture_output=True, check=True)
            metadata = json.loads(Path(str(output) + '.metadata.json').read_text())
            self.assertEqual(metadata['actual_pr_base_sha'], base)
            self.assertEqual(metadata['actual_pr_source_sha'], source)
            self.assertEqual(metadata['actual_target_sha'], target)
            self.assertEqual(metadata['source_base_sha'], fork)
            self.assertEqual(metadata['comparisons'],
                             [{'base': fork, 'head': source}, {'base': target, 'head': tested}])
            self.assertEqual(metadata['trigger_paths'], ['scripts/ci-tests.sh'])
            for key in ('source_sha', 'tested_sha', 'policy_sha'):
                self.assertEqual(metadata[key], tested)
            self.assertEqual(metadata['authority'], 'diagnostic-only')
            self.assertIn('enabled=true\n', github_output.read_text())
            PREVIEW.validate_plan(json.loads(output.read_text()), tested)
            PREVIEW.restore_baseline(repo, metadata)

    def test_merge_only_driver_change_starts_preview(self):
        with tempfile.TemporaryDirectory() as directory:
            repo, base, source, target, tested, fork = merge_fixture(
                directory, target_changes={'target-doc.md': 'advanced target docs\n'},
                merge_changes={'scripts/ci-tests.sh': '# merge resolution driver\n'})
            self.assertNotIn('scripts/ci-tests.sh', PREVIEW.changed_paths(repo, fork, source))
            metadata = PREVIEW.plan_preview(repo, base, tested, Path(directory) / 'plan.json',
                                            source_ref=source)
            self.assertEqual(metadata['trigger_paths'], ['scripts/ci-tests.sh'])
            self.assertEqual(metadata['actual_target_sha'], target)

    def test_source_driver_change_survives_merge_resolution_restoring_target(self):
        with tempfile.TemporaryDirectory() as directory:
            repo, base, source, target, tested, _ = merge_fixture(
                directory, {'scripts/ci-tests.sh': '# candidate driver\n'},
                {'target-doc.md': 'advanced target docs\n'},
                {'scripts/ci-tests.sh': '# baseline driver\n'})
            self.assertNotIn('scripts/ci-tests.sh', PREVIEW.changed_paths(repo, target, tested))
            metadata = PREVIEW.plan_preview(repo, base, tested, Path(directory) / 'plan.json',
                                            source_ref=source)
            self.assertEqual(metadata['trigger_paths'], ['scripts/ci-tests.sh'])

    def test_source_only_merge_retains_the_complete_event_range(self):
        with tempfile.TemporaryDirectory() as directory:
            repo, base, _, _, tested, _ = merge_fixture(
                directory, target_changes={'scripts/ci-tests.sh': '# target driver\n'})
            metadata = PREVIEW.plan_preview(repo, base, tested, Path(directory) / 'plan.json')
            self.assertIsNone(metadata['actual_target_sha'])
            self.assertEqual(metadata['actual_pr_source_sha'], tested)
            self.assertEqual(metadata['comparisons'], [{'base': base, 'head': tested}])
            self.assertEqual(metadata['trigger_paths'], ['scripts/ci-tests.sh'])

    def test_deleted_and_both_renamed_policy_endpoints_still_start_preview(self):
        for operation in ('delete', 'rename-away', 'rename-into'):
            with self.subTest(operation=operation), tempfile.TemporaryDirectory() as directory:
                repo, _ = fixture(directory)
                fixture_planner(repo)
                ordinary_path = 'scripts/ordinary\tname\nwith spaces.sh'
                initial = ordinary_path if operation == 'rename-into' else 'scripts/ci-tests.sh'
                (repo / initial).write_text('# execution driver\n')
                base = commit(repo, 'original driver')
                if operation == 'delete':
                    (repo / initial).unlink()
                else:
                    destination = 'scripts/ci-tests.sh' if operation == 'rename-into' else ordinary_path
                    command(repo, 'mv', initial, destination)
                source = commit(repo, operation)
                paths = PREVIEW.changed_paths(repo, base, source)
                self.assertIn('scripts/ci-tests.sh', paths)
                if operation != 'delete':
                    self.assertIn(ordinary_path, paths)
                metadata = PREVIEW.plan_preview(repo, base, source, Path(directory) / 'plan.json')
                self.assertEqual(metadata['trigger_paths'], ['scripts/ci-tests.sh'])

    def test_invalid_pr_identities_fail_before_a_skipped_or_enabled_result(self):
        with tempfile.TemporaryDirectory() as directory:
            repo, base, source, target, tested, fork = merge_fixture(directory)
            tree = command(repo, 'rev-parse', tested + '^{tree}')
            unrelated_base = command(repo, 'commit-tree', tree, '-p', fork, '-m', 'unrelated base')
            octopus = command(repo, 'commit-tree', tree, '-p', target, '-p', source,
                              '-p', unrelated_base, '-m', 'three parents')
            cases = ((base, fork, tested), (base, source, target),
                     (unrelated_base, source, tested), (base, source, octopus),
                     (base, '0' * 40, tested), ('0' * 40, source, tested))
            for index, (event_base, candidate, checkout) in enumerate(cases):
                with self.subTest(case=index):
                    command(repo, 'checkout', '-q', '--detach', checkout)
                    output = Path(directory) / f'plan-{index}.json'
                    github_output = Path(directory) / f'github-output-{index}'
                    with self.assertRaises((ValueError, subprocess.CalledProcessError)):
                        PREVIEW.plan_preview(repo, event_base, checkout, output, github_output,
                                             source_ref=candidate)
                    self.assertFalse(output.exists())
                    self.assertFalse(github_output.exists())

    def test_ambiguous_source_target_merge_base_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            repo, base = fixture(directory)
            tree = command(repo, 'rev-parse', base + '^{tree}')
            left = command(repo, 'commit-tree', tree, '-p', base, '-m', 'left fork')
            right = command(repo, 'commit-tree', tree, '-p', base, '-m', 'right fork')
            target = command(repo, 'commit-tree', tree, '-p', left, '-p', right, '-m', 'target')
            source = command(repo, 'commit-tree', tree, '-p', right, '-p', left, '-m', 'source')
            tested = command(repo, 'commit-tree', tree, '-p', target, '-p', source, '-m', 'tested')
            self.assertEqual(set(command(repo, 'merge-base', '--all', target, source).split()),
                             {left, right})
            command(repo, 'checkout', '-q', '--detach', tested)
            with self.assertRaisesRegex(ValueError, 'ambiguous source/target merge base'):
                PREVIEW.plan_preview(repo, base, tested, Path(directory) / 'plan.json',
                                     source_ref=source)

    def test_real_cli_runs_candidate_planner_but_keeps_distinct_authority(self):
        # The actual subprocess parses and reports the identities received
        # by the candidate planner; it never receives the real PR base.
        for policy_path in ('.github/workflows/rust.yml', 'scripts/ci-required-scope.json',
                            'scripts/ci-tests.sh', 'scripts/ci-test-artifacts.py',
                            'scripts/cargo-dev-architecture.py', 'scripts/cargo-dev-architecture.json'):
            with self.subTest(path=policy_path), tempfile.TemporaryDirectory() as directory:
                repo, base = fixture(directory)
                fixture_planner(repo)
                execution_input = repo / policy_path
                execution_input.parent.mkdir(parents=True, exist_ok=True)
                execution_input.write_text('{"candidate": true}\n')
                command(repo, 'add', '.')
                command(repo, 'commit', '-qm', 'candidate policy and execution input')
                source = command(repo, 'rev-parse', 'HEAD')
                output = Path(directory) / 'plan.json'
                github_output = Path(directory) / 'github-output'
                metadata = PREVIEW.plan_preview(repo, base, source, output, github_output)
                self.assertEqual(metadata['authority'], 'diagnostic-only')
                self.assertEqual(metadata['source_sha'], source)
                self.assertEqual(metadata['actual_pr_base_sha'], base)
                self.assertNotEqual(metadata['synthetic_base_sha'], base)
                self.assertEqual(json.loads(Path(str(output) + '.metadata.json').read_text()), metadata)
                self.assertIn('enabled=true\n', github_output.read_text())
                self.assertEqual(command(repo, 'status', '--porcelain'), '')
                with self.assertRaises(ValueError):
                    PREVIEW.plan_preview(repo, base, base, output, github_output)

    def test_missing_extra_or_changed_work_never_becomes_a_smaller_benchmark(self):
        source = 'a' * 40
        original = valid_plan(source)
        PREVIEW.validate_plan(original, source)
        mutations = (
            ('scope', 'full'), ('execution_profile', 'candidate-full'),
            ('ordinary_execution', 'artifacts'), ('fast_execution_version', True),
            ('ordinary_clippy_group', None), ('ordinary_clippy_group', 'oasis7_required'),
            ('test_policy_changed', True), ('fast_plan_exceptions', ['risk']),
            ('rust_baseline_checks', ['newapi_accounting']),
            ('oasis7_required_wasm_packages', ['oasis7_viewer_codec']),
            ('groups', ['baseline', 'oasis7_required']),
            ('groups', sorted(PREVIEW.EXPECTED_GROUPS) + ['launcher_web']),
            ('ordinary_fast_targets', original['ordinary_fast_targets'][:-1]),
            ('source_sha', 'b' * 40), ('tested_sha', 'b' * 40), ('policy_sha', 'b' * 40),
        )
        for key, value in mutations:
            plan = copy.deepcopy(original)
            plan[key] = value
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                PREVIEW.validate_plan(plan, source)
        for field, value in (('default_features', True), ('default_features', 0),
                             ('features', []), ('features', ['test_tier_full'])):
            plan = copy.deepcopy(original)
            plan['ordinary_fast_targets'][0][field] = value
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                PREVIEW.validate_plan(plan, source)
        for field, value in (('build_policy_version', None), ('build_policy_version', True),
                             ('build_policy_version', 2), ('build_policy', 'unknown'),
                             ('mode', 'full'), ('packages', ['oasis7_custody_core']),
                             ('source', 'b' * 40), ('tested', 'b' * 40)):
            plan = copy.deepcopy(original)
            plan['architecture'][field] = value
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                PREVIEW.validate_plan(plan, source)


if __name__ == '__main__':
    unittest.main()
