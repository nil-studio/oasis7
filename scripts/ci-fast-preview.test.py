#!/usr/bin/env python3
"""Exercise diagnostic Git identities, fresh-runner restoration, and coverage guards."""
import copy
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import unittest
from unittest.mock import patch


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


def valid_plan(source, sample='runtime-world'):
    plan = {'scope': 'targeted', 'execution_profile': 'pr-fast',
            'fast_execution_version': 1, 'ordinary_execution': 'inline',
            'ordinary_partition_version': 1,
            'ordinary_partitions': ['oasis7_runtime', 'oasis7_world_state'],
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
    if sample == 'game-client-private-unix':
        plan['ordinary_partitions'] = ['all']
        plan['architecture']['packages'] = ['oasis7_game_client']
        plan['ordinary_fast_targets'] = [dict(package='oasis7_game_client', target_kind=kind,
            target_name=name, default_features=True, features=[]) for kind, name in (
                ('lib', 'oasis7_game_client'), ('test', 'identity_cli'), ('test', 'dev_test_catalog_cli'),
                ('test', 'legacy_profile'), ('test', 'dev_test_service_cli'))]
        plan.update(ordinary_support_contract_version=1, ordinary_support_handoffs=['oasis7_game_client'],
                    ordinary_support_deferred=['oasis7_agent_harness', 'oasis7_model_provider'],
                    workspace_support_profiles=[], formal_suites=[], formal_shards=[])
    return plan


def fixture_planner(repo, consumer_groups=None, sample='runtime-world'):
    """Run a real planner subprocess without compiling a product workspace."""
    plan = valid_plan('unused', sample)
    if consumer_groups:
        plan['groups'] = sorted(PREVIEW.EXPECTED_GROUPS | consumer_groups.keys())
        plan['affected_packages'] = ['oasis7_runtime', 'oasis7_world_state',
                                     *(name for names in consumer_groups.values() for name in names)]
        plan['architecture']['consumer_groups'] = consumer_groups
        if 'distfs' in consumer_groups:
            plan.update(distfs_clippy_contract_version=1, distfs_clippy_group='rust_baseline')
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
plan['architecture']['base']=a.base_ref
pathlib.Path(a.output).write_text(json.dumps(plan))
""".replace('PLAN', repr(plan)))


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


class DiagnosticRegistrationPreview(unittest.TestCase):
    driver = 'scripts/ci-tests.sh'
    first = '  run python3 ./scripts/security/first.test.py\n'
    second = '  run python3 ./scripts/security/test_second.py\n'
    added = '  run python3 ./scripts/security/new.test.py\n'
    body = ('#!/bin/sh\n\nrun_workflow_governance_operational_contract_tests() {\n'
            + first + second + '}\n\nrun_workflow_governance_operational_contract_tests\n')

    def fixture(self, directory, *, capability=1):
        repo, _ = fixture(directory)
        fixture_planner(repo)
        config = json.loads((ROOT / 'scripts/ci-required-scope.json').read_text())
        if capability is None:
            config.pop('additive_governance_registration_version')
        else:
            config['additive_governance_registration_version'] = capability
        write_files(repo, {
            'Cargo.toml': '[workspace]\nmembers=["crates/*"]\n',
            'crates/oasis7_runtime/Cargo.toml': '[package]\nname="oasis7_runtime"\nversion="0.1.0"\n',
            'crates/oasis7_world_state/Cargo.toml': '[package]\nname="oasis7_world_state"\nversion="0.1.0"\n',
            self.driver: self.body,
            'scripts/security/first.test.py': '# existing diagnostic\n',
            'scripts/security/test_second.py': '# existing diagnostic\n',
            'scripts/ci-required-scope.json': json.dumps(config),
        })
        return repo, commit(repo, 'trusted diagnostic registration policy')

    def add(self, repo, changes=None):
        write_files(repo, {self.driver: self.body.replace(self.second, self.added + self.second),
                          'scripts/security/new.test.py': '# new diagnostic\n', **(changes or {})})
        return commit(repo, 'candidate diagnostic registration')

    def triggers(self, repo, base, source, tested=None):
        comparisons, _, _ = PREVIEW.preview_comparisons(repo, base, source, tested or source)
        changed = {path for before, after in comparisons for path in PREVIEW.changed_paths(repo, before, after)}
        return PREVIEW.product_preview_triggers(repo, base, comparisons, changed)

    def test_pure_addition_runs_diagnostics_without_product_benchmark(self):
        with tempfile.TemporaryDirectory() as directory:
            repo, base = self.fixture(directory)
            source = self.add(repo, {'.github/workflows/codeql-rust-capacity.yml': '# scoped diagnostic\n',
                                     'doc/diagnostic.md': 'diagnostic documentation\n'})
            output, enabled = Path(directory) / 'selection.json', Path(directory) / 'outputs.txt'
            with contextlib.redirect_stdout(io.StringIO()), patch.object(
                    PREVIEW, 'synthetic_baseline', side_effect=AssertionError('must not compile product sample')):
                self.assertIsNone(PREVIEW.plan_preview(repo, base, source, output, enabled))
            self.assertEqual(enabled.read_text(), 'enabled=false\n')
            self.assertFalse(output.exists())

    def test_old_and_unsupported_baseline_capabilities_retain_preview(self):
        for capability in (None, False, True, 0, 2, '1'):
            with self.subTest(capability=capability), tempfile.TemporaryDirectory() as directory:
                repo, base = self.fixture(directory, capability=capability)
                source = self.add(repo)
                self.assertEqual(self.triggers(repo, base, source), {self.driver})

    def test_candidate_policy_cannot_remove_its_own_preview(self):
        with tempfile.TemporaryDirectory() as directory:
            repo, base = self.fixture(directory, capability=None)
            current = (ROOT / 'scripts/ci-required-scope.json').read_text()
            source = self.add(repo, {'scripts/ci-required-scope.json': current})
            self.assertEqual(self.triggers(repo, base, source),
                             {self.driver, 'scripts/ci-required-scope.json'})

    def test_other_policy_and_product_mixtures_keep_all_preview_triggers(self):
        cases = (
            {'.github/workflows/rust.yml': '# required execution changed\n'},
            {'scripts/ci-required-scope.json': '{"candidate_policy":true}\n'},
            {'scripts/ci-test-artifacts.py': '# artifact policy changed\n'},
            {PREVIEW.SAMPLE_PATHS[0]: 'pub fn changed_product() {}\n'},
            {'tools/oasis7-command-center/ui/src/app.tsx': '// changed UI\n'},
            {self.driver: self.body.replace(self.second, self.added + self.second) + 'exit 0\n'},
            {self.driver: self.body.replace(self.second, self.added + self.second).replace(self.first, '')},
        )
        for changes in cases:
            with self.subTest(paths=sorted(changes)), tempfile.TemporaryDirectory() as directory:
                repo, base = self.fixture(directory)
                source = self.add(repo, changes)
                self.assertEqual(self.triggers(repo, base, source),
                                 {self.driver} | (changes.keys() & PREVIEW.POLICY_INPUTS))

    def test_regular_blob_and_baseline_policy_read_failures_retain_preview(self):
        for case in ('missing', 'symlink', 'driver mode', 'unreadable policy'):
            with self.subTest(case=case), tempfile.TemporaryDirectory() as directory:
                repo, base = self.fixture(directory)
                if case == 'unreadable policy':
                    (repo / 'scripts/ci-required-scope.json').write_text('not json\n')
                    base = commit(repo, 'unreadable baseline policy')
                self.add(repo)
                target = repo / 'scripts/security/new.test.py'
                if case in ('missing', 'symlink'):
                    target.unlink()
                    if case == 'symlink':
                        target.symlink_to('first.test.py')
                elif case == 'driver mode':
                    (repo / self.driver).chmod(0o755)
                source = command(repo, 'rev-parse', 'HEAD') if case == 'unreadable policy' else commit(repo, case)
                self.assertEqual(self.triggers(repo, base, source), {self.driver})

    def test_tested_merge_cannot_remove_source_registrations(self):
        for mutate in (False, True):
            with self.subTest(merge_mutation=mutate), tempfile.TemporaryDirectory() as directory:
                repo, base = self.fixture(directory)
                command(repo, 'checkout', '-qb', 'candidate')
                source = self.add(repo)
                command(repo, 'checkout', '-q', 'main')
                write_files(repo, {'README.md': 'target advanced\n'})
                commit(repo, 'target advance')
                command(repo, 'merge', '--no-ff', '--no-commit', source)
                if mutate:
                    write_files(repo, {self.driver: self.body.replace(self.second, self.added + self.second)
                                      .replace(self.first, '')})
                tested = commit(repo, 'tested merge')
                self.assertEqual(self.triggers(repo, base, source, tested), {self.driver} if mutate else set())

    def test_existing_rust_fixture_consumer_keeps_product_preview(self):
        with tempfile.TemporaryDirectory() as directory:
            repo, _ = self.fixture(directory)
            write_files(repo, {
                'crates/core/Cargo.toml': '[package]\nname="core"\nversion="0.1.0"\n',
                'crates/core/src/lib.rs':
                    'pub const DIAGNOSTIC: &str = include_str!("../../../scripts/security/new.test.py");\n',
                'scripts/security/new.test.py': 'VALUE = 1\n',
            })
            base = commit(repo, 'existing Rust fixture reader')
            source = self.add(repo, {'scripts/security/new.test.py': 'VALUE = 2\n'})
            self.assertEqual(self.triggers(repo, base, source), {self.driver})

    def test_unreadable_or_uncertain_cargo_graph_keeps_product_preview(self):
        for contents in (None, 'not TOML'):
            with self.subTest(manifest=contents), tempfile.TemporaryDirectory() as directory:
                repo, _ = self.fixture(directory)
                write_files(repo, {'Cargo.toml': contents})
                base = commit(repo, 'unreadable baseline manifest')
                source = self.add(repo)
                self.assertEqual(self.triggers(repo, base, source), {self.driver})
        with tempfile.TemporaryDirectory() as directory:
            repo, _ = self.fixture(directory)
            write_files(repo, {'crates/oasis7_runtime/src/lib.rs':
                              'pub const UNKNOWN: &str = include_str!(env!("UNREGISTERED_FIXTURE"));\n'})
            base = commit(repo, 'uncertain embedded reader')
            source = self.add(repo)
            self.assertEqual(self.triggers(repo, base, source), {self.driver})

    def test_merge_only_addition_is_proved_and_target_only_addition_is_excluded(self):
        for target_only in (False, True):
            with self.subTest(target_only=target_only), tempfile.TemporaryDirectory() as directory:
                repo, base = self.fixture(directory)
                command(repo, 'checkout', '-qb', 'candidate')
                write_files(repo, {'doc/candidate.md': 'candidate documentation\n'})
                source = commit(repo, 'source docs')
                command(repo, 'checkout', '-q', 'main')
                if target_only:
                    self.add(repo)
                else:
                    write_files(repo, {'target.md': 'target documentation\n'})
                    commit(repo, 'target docs')
                command(repo, 'merge', '--no-ff', '--no-commit', source)
                if not target_only:
                    write_files(repo, {self.driver: self.body.replace(self.second, self.added + self.second),
                                      'scripts/security/new.test.py': '# merge registration\n'})
                tested = commit(repo, 'tested merge')
                self.assertEqual(self.triggers(repo, base, source, tested), set())


class Preview(unittest.TestCase):
    def test_private_unix_sample_requires_complete_cli_targets_and_lint_handoffs(self):
        sample, source = 'game-client-private-unix', 'a' * 40
        selected = valid_plan(source, sample)
        PREVIEW.validate_plan(selected, source, sample=sample)
        changes = [('ordinary_support_deferred', []), ('ordinary_support_contract_version', True),
                   ('workspace_support_profiles', ['oasis7_game_client']),
                   ('formal_suites', ['lifecycle-cli']), ('formal_shards', ['lifecycle-cli']),
                   ('groups', selected['groups'] + ['workspace_support'])]
        for key, value in changes:
            with self.subTest(key=key), self.assertRaises(ValueError):
                PREVIEW.validate_plan(selected | {key: value}, source, sample=sample)
        for changes in ({'default_features': False}, {'features': ['test_tier_required']},
                        {'target_name': 'unregistered_cli'}):
            changed = copy.deepcopy(selected)
            changed['ordinary_fast_targets'][-1].update(changes)
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                PREVIEW.validate_plan(changed, source, sample=sample)
        with self.assertRaises(ValueError):
            PREVIEW.validate_plan(selected | {'ordinary_fast_targets': selected['ordinary_fast_targets'][:-1]},
                                  source, sample=sample)

    def test_private_unix_baseline_restores_in_new_clone_and_rejects_sample_mixups(self):
        with tempfile.TemporaryDirectory() as directory:
            repo, base = fixture(directory)
            path = 'crates/oasis7_game_client/src/dev_test_service_cli.rs'
            write_files(repo, {path: 'pub(crate) fn run() {}\n', 'scripts/ci-required-scope.json': '{"changed":true}\n'})
            fixture_planner(repo, sample='game-client-private-unix')
            source = commit(repo, 'register private CLI diagnostic fixture')
            metadata = PREVIEW.plan_preview(repo, base, source, Path(directory) / 'selection.json',
                                            sample='game-client-private-unix')
            self.assertEqual(metadata['paths'], [path])
            self.assertEqual(metadata['sample'], 'game-client-private-unix')
            self.assertEqual(set(PREVIEW.changed_paths(repo, metadata['synthetic_base_sha'], source)), {path})
            clone = Path(directory) / 'clone'
            subprocess.run(['git', 'clone', '-q', '--no-local', str(repo), str(clone)], check=True)
            PREVIEW.restore_baseline(clone, metadata)
            plan = json.loads((Path(directory) / 'selection.json').read_text())
            PREVIEW.restore_baseline(clone, metadata, sample='game-client-private-unix', plan=plan)
            other_base, other_tree = PREVIEW.synthetic_baseline(clone, source)
            other = metadata | {'sample': 'runtime-world', 'paths': list(PREVIEW.SAMPLE_PATHS),
                                'ordinary_partitions': ['oasis7_runtime', 'oasis7_world_state'],
                                'synthetic_base_sha': other_base, 'synthetic_base_tree': other_tree}
            other_plan = valid_plan(source)
            other_plan['architecture']['base'] = other_base
            # An intact valid pair from the other sample must not be relabelled
            # by its artifact directory or matrix row; swapping only the plan
            # must also fail before any product command starts.
            PREVIEW.restore_baseline(clone, other, sample='runtime-world', plan=other_plan)
            for partition in ('oasis7_runtime', 'oasis7_world_state'):
                PREVIEW.restore_baseline(clone, other, sample='runtime-world', plan=other_plan,
                                         group='oasis7_required', partition=partition)
            for group, partition in (('oasis7_required', 'all'), ('oasis7_required', 'unknown'),
                                     ('rust_baseline', 'oasis7_runtime'), ('unknown', 'all')):
                with self.subTest(group=group, partition=partition), self.assertRaises(ValueError):
                    PREVIEW.restore_baseline(clone, other, sample='runtime-world', plan=other_plan,
                                             group=group, partition=partition)
            for changed_metadata, changed_plan in ((other, other_plan), (metadata, other_plan)):
                with self.assertRaises(ValueError):
                    PREVIEW.restore_baseline(clone, changed_metadata,
                                             sample='game-client-private-unix', plan=changed_plan)
            for changed in (metadata | {'sample': 'runtime-world'}, metadata | {'sample': 'unknown'},
                            metadata | {'paths': list(PREVIEW.SAMPLE_PATHS)}):
                with self.subTest(changed=changed), self.assertRaises(ValueError):
                    PREVIEW.restore_baseline(clone, changed)

    def test_matrix_keeps_all_sample_groups_and_never_publishes_partial_success(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'github-output'
            root = Path(directory) / 'plans'
            source = 'a' * 40
            metadata = [dict(sample='runtime-world', source_sha=source, tested_sha=source,
                             ordinary_partitions=['oasis7_runtime', 'oasis7_world_state'],
                             synthetic_base_sha='b' * 40, groups=['baseline', 'rust_baseline', 'oasis7_required', 'net']),
                        dict(sample='game-client-private-unix', source_sha=source, tested_sha=source,
                             ordinary_partitions=['all'],
                             synthetic_base_sha='c' * 40, groups=['baseline', 'rust_baseline', 'oasis7_required'])]
            by_sample = {item['sample']: item for item in metadata}
            with patch.object(PREVIEW, 'plan_preview',
                              side_effect=lambda *args, **kwargs: by_sample[kwargs['sample']]) as plan:
                PREVIEW.plan_matrix(Path(directory), 'base', 'head', root, output, source_ref='source')
                calls = plan.call_args_list
            values = dict(line.split('=', 1) for line in output.read_text().splitlines())
            rows = json.loads(values['matrix'])['include']
            self.assertEqual(len(rows), 8)
            self.assertEqual({row['sample'] for row in rows}, {'runtime-world', 'game-client-private-unix'})
            self.assertIn({'sample': 'runtime-world', 'group': 'net', 'partition': 'all',
                           'synthetic_base_sha': 'b' * 40}, rows)
            ordinary = [row for row in rows if row['sample'] == 'runtime-world' and row['group'] == 'oasis7_required']
            self.assertEqual([row['partition'] for row in ordinary], ['oasis7_runtime', 'oasis7_world_state'])
            self.assertEqual(len({(row['sample'], row['group'], row['partition']) for row in rows}), len(rows))
            self.assertEqual(len({str(call.args[3]) for call in calls}), 2)
            self.assertTrue(all(len(call.args) == 4 and 'github_output' not in call.kwargs for call in calls))
            output.unlink()
            def fail_cli(*args, **kwargs):
                if kwargs['sample'] == 'game-client-private-unix':
                    raise ValueError('bad CLI plan')
                return by_sample[kwargs['sample']]

            with patch.object(PREVIEW, 'plan_preview', side_effect=fail_cli):
                with self.assertRaises(ValueError):
                    PREVIEW.plan_matrix(Path(directory), 'base', 'head', root, output)
            self.assertFalse(output.exists())

    def test_sample_planners_overlap_without_publishing_before_both_finish(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'github-output'
            entered = threading.Barrier(2, timeout=5)
            runtime_finished = threading.Event()
            source = 'a' * 40

            def plan_sample(*args, **kwargs):
                sample = kwargs['sample']
                entered.wait()  # A serial implementation cannot enter both planners.
                if sample == 'runtime-world':
                    runtime_finished.set()
                else:
                    self.assertTrue(runtime_finished.wait(5))
                    self.assertFalse(output.exists())
                return dict(sample=sample, source_sha=source, tested_sha=source,
                            ordinary_partitions=['oasis7_runtime', 'oasis7_world_state'] if sample == 'runtime-world' else ['all'],
                            synthetic_base_sha=('b' if sample == 'runtime-world' else 'c') * 40,
                            groups=['baseline', 'rust_baseline', 'oasis7_required'])

            with patch.object(PREVIEW, 'plan_preview', side_effect=plan_sample):
                result = PREVIEW.plan_matrix(Path(directory), 'base', 'head', directory, output)
            self.assertEqual([item['sample'] for item in result], list(PREVIEW.SAMPLES))
            self.assertEqual(output.read_text().count('enabled=true\n'), 1)

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

    def test_native_reverse_consumer_groups_reach_the_complete_preview_matrix(self):
        consumers = {'distfs': ['oasis7_distfs'], 'net': ['oasis7_net'], 'node': ['oasis7_node']}
        with tempfile.TemporaryDirectory() as directory:
            repo, base = fixture(directory)
            fixture_planner(repo, consumers)
            (repo / 'scripts/ci-tests.sh').write_text('# candidate driver\n')
            source = commit(repo, 'candidate with actual native reverse consumers')
            output, github_output = Path(directory) / 'plan.json', Path(directory) / 'github-output'
            metadata = PREVIEW.plan_preview(repo, base, source, output, github_output)
            selected = json.loads(output.read_text())
            values = dict(line.split('=', 1) for line in github_output.read_text().splitlines())
            expected = sorted(['baseline', 'rust_baseline', 'oasis7_required', 'distfs', 'net', 'node'])
            self.assertEqual(selected['groups'], expected)
            self.assertEqual(json.loads(values['groups']), expected)
            self.assertEqual(metadata['groups'], expected)
            self.assertEqual(selected['architecture']['consumer_groups'], consumers)
            self.assertEqual(len(selected['ordinary_fast_targets']), 4)
            self.assertEqual(selected['distfs_clippy_contract_version'], 1)
            self.assertEqual(selected['distfs_clippy_group'], 'rust_baseline')

    def test_distfs_preview_cannot_omit_or_misassign_the_original_lint_handoff(self):
        source = 'a' * 40
        selected = valid_plan(source)
        selected.update(distfs_clippy_contract_version=1, distfs_clippy_group='rust_baseline',
                        affected_packages=['oasis7_runtime', 'oasis7_world_state', 'oasis7_distfs'])
        selected['groups'].append('distfs')
        selected['architecture']['consumer_groups'] = {'distfs': ['oasis7_distfs']}
        PREVIEW.validate_plan(selected, source)
        for field, value in (('distfs_clippy_contract_version', True), ('distfs_clippy_contract_version', 2),
                             ('distfs_clippy_group', 'distfs'), ('distfs_clippy_group', None)):
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                PREVIEW.validate_plan(selected | {field: value}, source)
        for missing in ('distfs_clippy_contract_version', 'distfs_clippy_group'):
            candidate = copy.deepcopy(selected)
            del candidate[missing]
            with self.subTest(missing=missing), self.assertRaises(ValueError):
                PREVIEW.validate_plan(candidate, source)
        with self.assertRaises(ValueError):
            PREVIEW.validate_plan(valid_plan(source) | {'distfs_clippy_contract_version': 1,
                                                       'distfs_clippy_group': 'rust_baseline'}, source)

    def test_unproved_duplicate_or_misassigned_consumer_groups_are_rejected(self):
        source = 'a' * 40
        original = valid_plan(source)
        original['groups'] += ['net']
        original['affected_packages'] = ['oasis7_runtime', 'oasis7_world_state', 'oasis7_net']
        original['architecture']['consumer_groups'] = {'net': ['oasis7_net']}
        PREVIEW.validate_plan(original, source)
        for mapping in (None, {}, {'node': ['oasis7_net']}, {'net': []},
                        {'net': ['oasis7_net', 'oasis7_net']}, {'net': ['unknown']},
                        {'net': ['oasis7_runtime']}, {'net': 'oasis7_net'}, {'net': [1]}):
            plan = copy.deepcopy(original)
            plan['architecture']['consumer_groups'] = mapping
            with self.subTest(mapping=mapping), self.assertRaises(ValueError):
                PREVIEW.validate_plan(plan, source)
        for groups in (original['groups'] + ['net'], original['groups'] + ['launcher_web'],
                       ['baseline', 'oasis7_required', 'net'], original['groups'] + [1]):
            plan = copy.deepcopy(original)
            plan['groups'] = groups
            with self.subTest(groups=groups), self.assertRaises(ValueError):
                PREVIEW.validate_plan(plan, source)


if __name__ == '__main__':
    unittest.main()
