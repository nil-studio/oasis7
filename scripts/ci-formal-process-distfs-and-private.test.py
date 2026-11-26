#!/usr/bin/env python3
"""Private unit test selection, distfs clippy handoff, and distfs named integration driver regressions split from ci-formal-process.test.py."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def module(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), HERE / (name + '.py'))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


artifacts = module('ci-test-artifacts')


def fast_execution_selection(packages=('oasis7_viewer_codec',), suites=()):
    return dict(fast_plan_version=1, execution_profile='pr-fast', scope='targeted',
                groups=['baseline', 'oasis7_required'] + (['formal_process'] if suites else []),
                oasis7_required_packages=list(packages),
                oasis7_required_wasm_packages=[p for p in packages if p in artifacts.PORTABLE_REQUIRED_PACKAGES],
                formal_suites=list(suites), test_policy_changed=False,
                architecture=dict(source='source', tested='tested'))


def fast_target(**changes):
    return dict(package='oasis7_viewer_codec', target_kind='test', target_name='pr_fast',
                default_features=False, features=['test_tier_required'], cases=['required::round_trip']) | changes


def fast_target_selection(targets=None, full=False):
    selected = fast_execution_selection(('oasis7_viewer_codec', 'oasis7_runtime'))
    key = 'ordinary_full_fast_targets' if full else 'ordinary_fast_targets'
    selected['full_fast_test_inventory_version' if full else 'fast_test_inventory_version'] = 1
    selected[key] = [fast_target()] if targets is None else targets
    if full:
        selected.update(execution_profile='periodic-full', scope='explicit-full')
    return selected


def fast_target_binary(target=None, variant='required-fast-0'):
    target = target or fast_target()
    return dict(ordinary=True, test=True, package=target['package'], target=target['target_name'],
                target_kind=[target['target_kind']], default_features=target['default_features'],
                requested_features=target['features'], features=target['features'],
                tests=target['cases'] + ['new_regression'], ignored_tests=[], variant=variant)


def inline_execution_selection(checks=None):
    return fast_target_selection() | {
        'fast_execution_version': 1, 'ordinary_execution': 'inline',
        'rust_baseline_checks': [] if checks is None else checks,
    }


def native_package_partition_selection():
    targets = [fast_target(package=package, target_kind=kind, target_name=name)
               for package in ('oasis7_runtime', 'oasis7_world_state')
               for kind, name in (('lib', package), ('test', 'pr_fast'))]
    return inline_execution_selection() | {
        'ordinary_fast_targets': targets, 'ordinary_partition_version': 1,
        'ordinary_partitions': ['oasis7_runtime', 'oasis7_world_state'],
        'groups': ['baseline', 'rust_baseline', 'oasis7_required'],
        'oasis7_required_packages': ['oasis7_runtime', 'oasis7_world_state'],
        'ordinary_clippy_group': 'rust_baseline', 'oasis7_required_wasm_packages': [],
    }


class PrivateUnitTestSelection(unittest.TestCase):
    def selection(self):
        owner = 'oasis7_world_state'
        target = fast_target(package=owner, target_kind='lib', target_name=owner)
        selected = inline_execution_selection()
        return selected | {
            'source_sha': 'a' * 40, 'tested_sha': 'b' * 40, 'policy_sha': 'c' * 40,
            'architecture': dict(base='c' * 40, source='a' * 40, tested='b' * 40),
            'groups': ['baseline', 'rust_baseline', 'oasis7_required'],
            'ordinary_fast_targets': [target], 'oasis7_required_packages': [owner],
            'oasis7_required_wasm_packages': [],
            'ordinary_clippy_group': 'rust_baseline', 'formal_suites': [], 'fast_plan_exceptions': [],
            'ordinary_unit_test_contract_version': 1,
            'ordinary_unit_test_sources': {f'crates/{owner}/src/faults.rs': owner},
            'ordinary_unit_test_proof_refs': ['a' * 40, 'b' * 40, 'c' * 40],
            'ordinary_unit_test_excluded_groups': ['distfs', 'net', 'node'],
            'ordinary_unit_test_excluded_targets': [dict(package='oasis7_runtime', target_kind='lib',
                                                        target_name='oasis7_runtime')],
        }

    def test_complete_owner_build_and_clippy_keep_their_original_commands(self):
        selected = self.selection()
        args = ['-p', 'oasis7_world_state', '--lib', '--no-default-features', '--features', 'test_tier_required']
        self.assertEqual(artifacts.required_native_commands(selected), [args])
        self.assertEqual(artifacts.build_variants('ordinary', selected),
                         [('required-fast-0', ['test', *args, '--no-run'], ['test_tier_required'])])
        target = selected['ordinary_fast_targets'][0]
        binary = fast_target_binary(target)
        manifest = dict(scope='ordinary', execution_plan=selected, binaries={'world': binary})
        artifacts.validate_ordinary_selection(manifest)
        for names in ([], ['required::round_trip']):
            changed = copy.deepcopy(manifest)
            changed['binaries']['world']['tests'] = names
            if names:
                changed['binaries']['world']['ignored_tests'] = names
            with self.subTest(tests=names), self.assertRaises(ValueError):
                artifacts.validate_ordinary_selection(changed)

    def test_partial_or_inconsistent_source_contract_cannot_erase_execution(self):
        selected = self.selection()
        mutations = [{key: None} for key in artifacts.UNIT_TEST_CONTRACT_FIELDS]
        mutations += [
            {'ordinary_unit_test_contract_version': True}, {'ordinary_unit_test_contract_version': 2},
            {'ordinary_unit_test_sources': {}}, {'ordinary_unit_test_sources': {'../faults.rs': 'oasis7_world_state'}},
            {'ordinary_unit_test_proof_refs': ['a' * 40]},
            {'ordinary_unit_test_proof_refs': ['a' * 40, 'b' * 40]},
            {'ordinary_unit_test_excluded_groups': ['rust_baseline']},
            {'ordinary_unit_test_excluded_targets': [dict(package='oasis7_world_state', target_kind='lib',
                                                         target_name='oasis7_world_state')]},
            {'ordinary_fast_targets': []}, {'test_policy_changed': True},
            {'groups': ['baseline', 'oasis7_required']}, {'oasis7_required_packages': []},
            {'ordinary_clippy_group': 'oasis7_required'}, {'formal_suites': ['world-history']},
            {'scope': 'full'}, {'execution_profile': 'periodic-full'},
        ]
        for mutation in mutations:
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                artifacts.required_native_commands(selected | mutation)
        for key in artifacts.UNIT_TEST_CONTRACT_FIELDS | {'fast_plan_version'}:
            partial = dict(selected)
            del partial[key]
            with self.subTest(missing=key), self.assertRaises(ValueError):
                artifacts.required_native_commands(partial)

    def test_every_remaining_required_owner_still_blocks_on_failure_or_missing(self):
        selected = self.selection()
        checker = module('ci-required-result')
        jobs = {group: group.replace('_', '-') for group in selected['groups']}
        success = {name: {'result': 'success'} for name in ['select', *jobs.values()]}
        self.assertTrue(checker.validate(selected, success, jobs))
        for group, job in jobs.items():
            for result in ('failure', 'cancelled', 'skipped', None):
                with self.subTest(group=group, result=result), self.assertRaises(ValueError):
                    checker.validate(selected, success | {job: {'result': result}}, jobs)
            with self.subTest(group=group, result='missing'), self.assertRaises(ValueError):
                checker.validate(selected, {name: value for name, value in success.items() if name != job}, jobs)


class DistfsClippyHandoff(unittest.TestCase):
    def selection(self):
        selected = native_package_partition_selection()
        for target in selected['ordinary_fast_targets']:
            if target['target_kind'] == 'test':
                target['target_name'] = ('pr_fast_incremental' if target['package'] == 'oasis7_runtime'
                                         else 'pr_fast_reducers')
        return selected | {
            'distfs_clippy_contract_version': 1, 'distfs_clippy_group': 'rust_baseline',
            'groups': ['baseline', 'rust_baseline', 'oasis7_required', 'distfs'],
            'formal_suites': [], 'fast_plan_exceptions': [],
            'architecture': dict(mode='targeted', build_policy_version=1,
                build_policy='ordinary-targets', consumer_groups={'distfs': ['oasis7_distfs']}),
        }

    def test_original_lint_inventory_has_one_owner_and_keeps_both_required_groups(self):
        selected = self.selection()
        expected = [['-p', 'oasis7_cas', '-p', 'oasis7_formal_protocol', '--lib'],
                    ['-p', 'oasis7_distfs', '--lib', '--bin', 'oasis7_durability_endpoint']]
        self.assertEqual(artifacts.required_native_commands(selected)[-2:], expected)
        self.assertEqual(artifacts.required_arguments(selected, 'distfs-clippy-group'), ['rust_baseline'])
        legacy = {key: value for key, value in selected.items()
                  if key not in ('distfs_clippy_contract_version', 'distfs_clippy_group')}
        self.assertEqual(len(artifacts.required_native_commands(legacy)), 4)
        for plan in (None, {}, legacy, fast_target_selection(full=True)):
            self.assertEqual(artifacts.required_arguments(plan, 'distfs-clippy-group'), ['distfs'])
        checker = module('ci-required-result')
        jobs = {group: group.replace('_', '-') for group in selected['groups']}
        successful = {name: {'result': 'success'} for name in ['select', *jobs.values()]}
        self.assertTrue(checker.validate(selected, successful, jobs))
        for owner in ('rust-baseline', 'distfs'):
            for failure in ('failure', 'cancelled', 'skipped', None):
                with self.subTest(owner=owner, status=failure), self.assertRaises(ValueError):
                    checker.validate(selected, successful | {owner: {'result': failure}}, jobs)

    def test_partial_untrusted_or_incomplete_handoff_cannot_skip_lint(self):
        selected = self.selection()
        invalid = [('distfs_clippy_contract_version', value) for value in (True, None, '1', 2)]
        invalid += [('distfs_clippy_group', value) for value in ('distfs', None, 'other')]
        invalid += [('ordinary_clippy_group', 'oasis7_required'), ('ordinary_execution', 'artifact'),
                    ('fast_execution_version', 2), ('scope', 'full'), ('execution_profile', 'candidate-full'),
                    ('test_policy_changed', True), ('fast_plan_exceptions', ['extra']),
                    ('formal_suites', ['world-history']), ('oasis7_required_wasm_packages', ['oasis7_runtime']),
                    ('ordinary_partitions', ['all'])]
        invalid += [('groups', [group for group in selected['groups'] if group != missing])
                    for missing in ('distfs', 'rust_baseline', 'oasis7_required')]
        candidates = [selected | {key: value} for key, value in invalid]
        candidates += [{key: value for key, value in selected.items() if key != missing}
                       for missing in ('distfs_clippy_contract_version', 'distfs_clippy_group', 'fast_plan_version')]
        candidates += [selected | {'ordinary_fast_targets': selected['ordinary_fast_targets'][:-1]},
                       selected | {'ordinary_fast_targets': selected['ordinary_fast_targets'][:1] * 4}]
        for changes in ({'features': []}, {'default_features': True}, {'target_name': 'unknown'},
                        {'target_kind': []}, {'cases': []}):
            candidate = copy.deepcopy(selected)
            candidate['ordinary_fast_targets'][0].update(changes)
            candidates.append(candidate)
        for candidate in candidates:
            with self.subTest(plan=candidate):
                with self.assertRaises(ValueError):
                    artifacts.required_arguments(candidate, 'distfs-clippy-group')
                with self.assertRaises(ValueError):
                    artifacts.required_native_commands(candidate)

    def test_whole_driver_moves_exact_lints_preserves_tests_and_propagates_failures(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo, binary = root / 'repo', root / 'bin'
            scripts = repo / 'scripts'
            scripts.mkdir(parents=True)
            binary.mkdir()
            (scripts / 'cargo-dev-architecture.py').write_text('print("architecture fixture")\n')
            (scripts / 'check-rust-hygiene.py').write_text('print("rust hygiene fixture")\n')
            for name in ('check-rustsec-ignore-baseline.test.sh', 'check-rustsec-ignore-baseline.sh',
                         'ensure-cargo-deny.sh', 'check-rust-file-size.test.sh', 'check-rust-file-size.sh'):
                path = scripts / name
                path.write_text('#!/usr/bin/env bash\nexit 0\n')
                path.chmod(0o755)
            cargo = binary / 'cargo'
            cargo.write_text('''#!/usr/bin/env bash
set -euo pipefail
args_json=""; sep=""
for arg in "$@"; do
  s="${arg//\\\\/\\\\\\\\}"; s="${s//\\"/\\\\\\"}"
  args_json+="${sep}\\"${s}\\""; sep=", "
done
printf '[%s]\\n' "$args_json" >> "$CARGO_LOG"
if [[ "${1:-}" == "clippy" && -n "${FAIL_CLIPPY_PACKAGE:-}" ]]; then
  args=("$@")
  for (( i = 0; i + 1 < ${#args[@]}; i++ )); do
    if [[ "${args[$i]}" == "-p" ]]; then
      [[ "${args[$((i + 1))]}" != "$FAIL_CLIPPY_PACKAGE" ]] || exit 19
      break
    fi
  done
fi
''')
            cargo.chmod(0o755)
            subprocess.run(['git', 'init', '-q', str(repo)], check=True)
            subprocess.run(['git', '-C', str(repo), 'add', '.'], check=True)
            subprocess.run(['git', '-C', str(repo), '-c', 'user.name=CI fixture', '-c',
                            'user.email=fixture@example.invalid', 'commit', '-qm', 'fixture'], check=True)
            sha = subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'HEAD'], text=True).strip()
            selected = self.selection() | {'source_sha': sha, 'tested_sha': sha}
            selected['architecture'].update(source=sha, tested=sha)
            selection, log = root / 'selection.json', root / 'cargo.jsonl'
            original_lints = [
                ['clippy', '-p', 'oasis7_cas', '-p', 'oasis7_formal_protocol', '--lib'],
                ['clippy', '-p', 'oasis7_distfs', '--lib', '--bin', 'oasis7_durability_endpoint']]
            original_lints = [args + ['--', '-D', 'warnings', '-D', 'clippy::correctness',
                                      '-D', 'clippy::suspicious'] for args in original_lints]
            distfs_lib_test_variants = (
                [['test', '-p', 'oasis7_distfs', '--lib', '--bin', 'oasis7_durability_endpoint', '--', '--test-threads=1']],
                [
                    ['test', '-p', 'oasis7_distfs', '--lib', '--bin', 'oasis7_durability_endpoint',
                     '--', '--skip', 'controlled_authority::formal::transport::'],
                    ['test', '-p', 'oasis7_distfs', '--lib', 'controlled_authority::formal::transport::',
                     '--', '--test-threads=1'],
                ],
            )
            allowed_tests = tuple(
                [cas_cmd, *distfs_cmds]
                for cas_cmd in (
                    ['test', '-p', 'oasis7_cas', '-p', 'oasis7_formal_protocol', '--lib', '--', '--test-threads=1'],
                    ['test', '-p', 'oasis7_cas', '-p', 'oasis7_formal_protocol', '--lib'],
                )
                for distfs_cmds in distfs_lib_test_variants
            )
            for mode, failed in (('handoff', ''), ('legacy', ''), ('handoff', 'oasis7_cas'),
                                 ('handoff', 'oasis7_distfs'), ('manual', '')):
                current = copy.deepcopy(selected)
                if mode == 'legacy':
                    del current['distfs_clippy_contract_version'], current['distfs_clippy_group']
                selection.write_text(json.dumps(current))
                observed = {}
                for group in (('distfs',) if mode == 'manual' else ('rust_baseline', 'distfs')):
                    log.unlink(missing_ok=True)
                    plan_args = [] if mode == 'manual' else ['--plan', str(selection)]
                    result = subprocess.run(['bash', str(HERE / 'ci-tests.sh'), 'required', '--group', group,
                        *plan_args, '--repo-root', str(repo)], text=True, capture_output=True,
                        env=dict(os.environ, PATH=str(binary) + os.pathsep + os.environ['PATH'], SOURCE_SHA=sha,
                                 CI_VERBOSE='', CARGO_LOG=str(log), FAIL_CLIPPY_PACKAGE=failed,
                                 OASIS7_CI_RUN_PROVIDER_LIVE_GATE='false', OASIS7_CI_RUN_HOSTED_ACCOUNT_SMOKE='false'))
                    commands = [json.loads(line) for line in log.read_text().splitlines()]
                    observed[group] = [args for args in commands if args[0] == 'clippy' and args[2] in ('oasis7_cas', 'oasis7_distfs')]
                    with self.subTest(mode=mode, failed=failed, group=group):
                        self.assertEqual(result.returncode, 19 if group == 'rust_baseline' and failed else 0,
                                         result.stdout + result.stderr)
                        if group == 'distfs':
                            self.assertIn([args for args in commands if args[0] == 'test'], allowed_tests)
                owner = 'rust_baseline' if mode == 'handoff' else 'distfs'
                self.assertEqual(observed[owner], original_lints[:1] if failed == 'oasis7_cas' else original_lints)
                self.assertEqual(observed.get('distfs' if owner == 'rust_baseline' else 'rust_baseline', []), [])
            for field in ('source_sha', 'tested_sha'):
                selection.write_text(json.dumps(selected | {field: '0' * 40}))
                log.unlink(missing_ok=True)
                result = subprocess.run(['bash', str(HERE / 'ci-tests.sh'), 'required', '--group', 'distfs',
                    '--plan', str(selection), '--repo-root', str(repo)], text=True, capture_output=True,
                    env=dict(os.environ, PATH=str(binary) + os.pathsep + os.environ['PATH'], SOURCE_SHA=sha,
                             CI_VERBOSE='', CARGO_LOG=str(log), FAIL_CLIPPY_PACKAGE=''))
                self.assertNotEqual(result.returncode, 0)
                self.assertIn('execution plan identity mismatch', result.stderr)
                self.assertFalse(log.exists())


class DistfsNamedIntegrationDriver(unittest.TestCase):
    def selection(self, targets=None):
        selected = fast_target_selection(targets)
        selected['groups'].append('distfs')
        selected['oasis7_required_packages'] = sorted({target['package']
            for target in selected['ordinary_fast_targets']})
        selected['oasis7_required_wasm_packages'] = [package for package in selected['oasis7_required_packages']
            if package in artifacts.PORTABLE_REQUIRED_PACKAGES]
        return selected

    def test_industry_phase_requires_the_exact_selected_test_identity(self):
        industry = fast_target(package='oasis7', target_name='incremental_industry_jobs')
        cases = [([industry], 'run'),
                 ([industry, fast_target()], 'run'),
                 ([industry | {'package': 'oasis7_runtime'}], 'skip'),
                 ([industry | {'target_kind': 'lib'}], 'skip'),
                 ([industry | {'target_name': 'another_industry_test'}], 'skip')]
        for profile in ('pr-fast', 'main-fast'):
            for targets, expected in cases:
                selected = self.selection(targets) | {'execution_profile': profile}
                with self.subTest(profile=profile, targets=targets):
                    self.assertEqual(artifacts.required_arguments(selected, 'distfs-industry'), [expected])
        # The phase cannot turn a future, currently unselected package into an
        # authorized ordinary consumer merely because its target name matches.
        with self.assertRaises(ValueError):
            artifacts.required_arguments(self.selection([industry | {'package': 'oasis7_distfs'}]),
                                         'distfs-industry')

    def test_industry_phase_keeps_complete_runs_when_fast_inventory_is_inactive(self):
        selected = self.selection()
        legacy = {key: value for key, value in selected.items() if key != 'fast_test_inventory_version'}
        legacy['ordinary_fast_targets'] = 'stale capability data'
        candidates = [None, {}, fast_execution_selection(), legacy]
        for changes in ({'scope': 'full'}, {'scope': 'explicit-full'}, {'test_policy_changed': True},
                        {'execution_profile': 'periodic-full'}, {'execution_profile': 'candidate-full'}):
            candidates.append(selected | changes | {
                'fast_test_inventory_version': True, 'ordinary_fast_targets': 'stale capability data'})
        for candidate in candidates:
            with self.subTest(plan=candidate):
                self.assertEqual(artifacts.required_arguments(candidate, 'distfs-industry'), ['run'])

    def test_industry_phase_rejects_missing_empty_and_invalid_active_inventory(self):
        selected = self.selection()
        candidates = [selected | {key: value} for key, value in (
            ('fast_plan_version', True), ('fast_test_inventory_version', True),
            ('fast_test_inventory_version', 2), ('ordinary_fast_targets', []),
            ('ordinary_fast_targets', None), ('ordinary_fast_targets', 'invalid'),
            ('ordinary_fast_targets', selected['ordinary_fast_targets'] * 2))]
        candidates.append({key: value for key, value in selected.items() if key != 'ordinary_fast_targets'})
        for changes in ({'package': 'unselected'}, {'target_kind': 'bin'}, {'cases': []},
                        {'target_name': '../incremental_industry_jobs'}):
            candidate = copy.deepcopy(selected)
            candidate['ordinary_fast_targets'][0].update(changes)
            candidates.append(candidate)
        for candidate in candidates:
            with self.subTest(plan=candidate), self.assertRaises(ValueError):
                artifacts.required_arguments(candidate, 'distfs-industry')

    def test_named_target_presence_handoff_and_failures_preserve_original_commands(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo, binary = root / 'repo', root / 'bin'
            scripts = repo / 'scripts'
            scripts.mkdir(parents=True)
            binary.mkdir()
            (scripts / 'cargo-dev-architecture.py').write_text('print("architecture fixture")\n')
            (scripts / 'check-rust-hygiene.py').write_text('print("rust hygiene fixture")\n')
            for name in ('check-rustsec-ignore-baseline.test.sh', 'check-rustsec-ignore-baseline.sh',
                         'ensure-cargo-deny.sh', 'check-rust-file-size.test.sh', 'check-rust-file-size.sh'):
                path = scripts / name
                path.write_text('#!/usr/bin/env bash\nexit 0\n')
                path.chmod(0o755)
            crate = repo / 'crates/oasis7_distfs'
            (crate / 'tests').mkdir(parents=True)
            (crate / 'Cargo.toml').write_text(
                '[package]\nname="oasis7_distfs"\nversion="0.1.0"\nedition="2024"\n')
            # A different integration file must not authorize the named driver.
            (crate / 'tests/other.rs').write_text('#[test] fn unrelated() {}\n')
            named = crate / 'tests/incremental_industry_jobs.rs'
            cargo = binary / 'cargo'
            cargo.write_text('''#!/usr/bin/env bash
set -euo pipefail
args_json=""; sep=""
for arg in "$@"; do
  s="${arg//\\\\/\\\\\\\\}"; s="${s//\\"/\\\\\\"}"
  args_json+="${sep}\\"${s}\\""; sep=", "
done
printf '[%s]\\n' "$args_json" >> "$CARGO_LOG"
if [[ "${1:-}" == "${FAIL_NAMED_COMMAND:-}" && -n "${1:-}" ]]; then
  args=("$@")
  for (( i = 0; i + 1 < ${#args[@]}; i++ )); do
    if [[ "${args[$i]}" == "--test" ]]; then
      if [[ "${args[$((i + 1))]}" == "incremental_industry_jobs" ]]; then
        if [[ "$1" == "test" ]]; then exit 23; else exit 29; fi
      fi
      break
    fi
  done
fi
''')
            cargo.chmod(0o755)
            subprocess.run(['git', 'init', '-q', str(repo)], check=True)
            selection, env_selection, log = root / 'selection.json', root / 'env-selection.json', root / 'cargo.jsonl'
            lint_flags = ['--', '-D', 'warnings', '-D', 'clippy::correctness', '-D', 'clippy::suspicious']
            original_lints = [
                ['clippy', '-p', 'oasis7_cas', '-p', 'oasis7_formal_protocol', '--lib'] + lint_flags,
                ['clippy', '-p', 'oasis7_distfs', '--lib', '--bin', 'oasis7_durability_endpoint'] + lint_flags]
            distfs_lib_test_variants = (
                [['test', '-p', 'oasis7_distfs', '--lib', '--bin', 'oasis7_durability_endpoint', '--', '--test-threads=1']],
                [
                    ['test', '-p', 'oasis7_distfs', '--lib', '--bin', 'oasis7_durability_endpoint',
                     '--', '--skip', 'controlled_authority::formal::transport::'],
                    ['test', '-p', 'oasis7_distfs', '--lib', 'controlled_authority::formal::transport::',
                     '--', '--test-threads=1'],
                ],
            )
            allowed_base_tests = tuple(
                [cas_cmd, *distfs_cmds]
                for cas_cmd in (
                    ['test', '-p', 'oasis7_cas', '-p', 'oasis7_formal_protocol', '--lib', '--', '--test-threads=1'],
                    ['test', '-p', 'oasis7_cas', '-p', 'oasis7_formal_protocol', '--lib'],
                )
                for distfs_cmds in distfs_lib_test_variants
            )
            target_args = ['-p', 'oasis7_distfs', '--test', 'incremental_industry_jobs']
            named_lint = ['clippy', *target_args, *lint_flags]
            merged_distfs_lint = [
                'clippy', '-p', 'oasis7_distfs', '--lib', '--bin', 'oasis7_durability_endpoint',
                '--test', 'incremental_industry_jobs', *lint_flags]
            allowed_named_tests = (
                ['test', *target_args, '--', '--test-threads=1'],
                ['test', *target_args],
            )
            for present in (False, True):
                if present:
                    named.write_text('#[test] fn industry_contract() {}\n')
                subprocess.run(['git', '-C', str(repo), 'add', '.'], check=True)
                subprocess.run(['git', '-C', str(repo), '-c', 'user.name=CI fixture', '-c',
                                'user.email=fixture@example.invalid', 'commit', '-qm', 'fixture'], check=True)
                sha = subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'HEAD'], text=True).strip()
                selected = DistfsClippyHandoff().selection() | {'source_sha': sha, 'tested_sha': sha}
                selected['architecture'].update(source=sha, tested=sha)
                fast = {key: value for key, value in selected.items()
                        if key not in ('distfs_clippy_contract_version', 'distfs_clippy_group')}
                package = fast_execution_selection(('oasis7_runtime', 'oasis7_world_state'))
                package['groups'].append('distfs')
                package.update(source_sha=sha, tested_sha=sha)
                package['architecture'].update(source=sha, tested=sha)
                industry = self.selection([fast_target(
                    package='oasis7', target_name='incremental_industry_jobs')])
                industry.update(source_sha=sha, tested_sha=sha)
                industry['architecture'].update(source=sha, tested=sha)
                # Explicit --plan stays authoritative; the env-only handoff is
                # consumed solely by the new industry check, not by old lint routing.
                cases = [
                    ('handoff', selected, None, False), ('fast', fast, None, False),
                    ('package', package, None, True), ('full', package | {'scope': 'full'}, None, True),
                    ('policy', industry | {'test_policy_changed': True}, None, True),
                    ('industry', industry, None, True), ('legacy', {}, None, True), ('manual', None, None, True),
                    ('env-fast', None, fast, False), ('env-package', None, package, True),
                    ('env-full', None, package | {'scope': 'full'}, True),
                    ('explicit-skip', fast, industry, False), ('explicit-run', industry, fast, True),
                    ('explicit-invalid-env', industry, {'fast_plan_version': True}, True),
                    ('full-tier', None, fast | {'source_sha': '0' * 40}, True),
                ]
                for mode, current, environment_plan, run_named in cases:
                    selection.write_text(json.dumps(current))
                    env_selection.write_text(json.dumps(environment_plan))
                    for failed in (('', 'clippy', 'test') if present and run_named else ('',)):
                        for group in (('rust_baseline', 'distfs') if mode in ('handoff', 'fast') else ('distfs',)):
                            log.unlink(missing_ok=True)
                            plan_args = [] if current is None else ['--plan', str(selection)]
                            command = ['bash', str(HERE / 'ci-tests.sh'), 'required', '--group', group,
                                       *plan_args, '--repo-root', str(repo)]
                            if mode == 'full-tier':
                                # Exercise the sourced full-tier entry without
                                # invoking unrelated full-suite product groups.
                                command = ['bash', '-c', '''set -euo pipefail
source "$1/ci-tests.sh"
driver_dir="$1"; repo_root="$2"; ci_python="$3"; tier=full; required_plan=""
cd "$repo_root"
run_oasis7_distfs_clippy
run_oasis7_distfs_tests
''', 'fixture', str(HERE), str(repo), sys.executable]
                            result = subprocess.run(
                                command, text=True, capture_output=True,
                                env=dict(os.environ, PATH=str(binary) + os.pathsep + os.environ['PATH'],
                                         SOURCE_SHA=sha, CI_VERBOSE='', CARGO_LOG=str(log),
                                         FAIL_NAMED_COMMAND=failed,
                                         OASIS7_CI_DISTFS_PLAN_FILE=str(env_selection) if environment_plan is not None else '',
                                         OASIS7_CI_RUN_PROVIDER_LIVE_GATE='false',
                                         OASIS7_CI_RUN_HOSTED_ACCOUNT_SMOKE='false'))
                            commands = [json.loads(line) for line in log.read_text().splitlines()]
                            observed_lints = [args for args in commands if args[0] == 'clippy'
                                              and args[2] in ('oasis7_cas', 'oasis7_distfs')]
                            owner = 'rust_baseline' if mode == 'handoff' else 'distfs'
                            expected_lints = list(original_lints) if group == owner else []
                            if present and run_named and group == 'distfs':
                                expected_lints.append(named_lint)
                            expected_lint_variants = [expected_lints]
                            if present and run_named and group == 'distfs' and owner == 'distfs':
                                expected_lint_variants.append([original_lints[0], merged_distfs_lint])
                            expected_test_variants = [[]]
                            if group == 'distfs' and failed != 'clippy':
                                if present and run_named:
                                    expected_test_variants = [
                                        base + [named_cmd]
                                        for base in allowed_base_tests
                                        for named_cmd in allowed_named_tests
                                    ]
                                else:
                                    expected_test_variants = list(allowed_base_tests)
                            expected_status = (29 if failed == 'clippy' else 23 if failed == 'test' else 0)
                            with self.subTest(present=present, mode=mode, failed=failed, group=group):
                                self.assertEqual(result.returncode, expected_status if group == 'distfs' else 0,
                                                 result.stdout + result.stderr)
                                self.assertIn(observed_lints, expected_lint_variants)
                                self.assertIn([args for args in commands if args[0] == 'test'], expected_test_variants)
                if not present:
                    continue
                invalid = [industry | {'ordinary_fast_targets': []},
                           {key: value for key, value in industry.items() if key != 'ordinary_fast_targets'},
                           industry | {'source_sha': '0' * 40}, industry | {'tested_sha': '0' * 40}]
                malformed = copy.deepcopy(industry)
                malformed['ordinary_fast_targets'][0]['cases'] = []
                invalid.append(malformed)
                for current in invalid:
                    for via_env in (False, True):
                        selection.write_text(json.dumps(current))
                        log.unlink(missing_ok=True)
                        result = subprocess.run(
                            ['bash', str(HERE / 'ci-tests.sh'), 'required', '--group', 'distfs',
                             *([] if via_env else ['--plan', str(selection)]), '--repo-root', str(repo)],
                            text=True, capture_output=True,
                            env=dict(os.environ, PATH=str(binary) + os.pathsep + os.environ['PATH'],
                                     SOURCE_SHA=sha, CI_VERBOSE='', CARGO_LOG=str(log), FAIL_NAMED_COMMAND='',
                                     OASIS7_CI_DISTFS_PLAN_FILE=str(selection) if via_env else '',
                                     OASIS7_CI_RUN_PROVIDER_LIVE_GATE='false',
                                     OASIS7_CI_RUN_HOSTED_ACCOUNT_SMOKE='false'))
                        commands = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
                        with self.subTest(plan=current, via_env=via_env):
                            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                            self.assertNotIn('industry integration is not selected', result.stdout)
                            self.assertNotIn(named_lint, commands)
                            self.assertEqual([args for args in commands if args[0] == 'test'], [])
                            if current.get('source_sha') != sha or current.get('tested_sha') != sha:
                                self.assertIn('execution plan identity mismatch', result.stderr)


if __name__ == '__main__':
    unittest.main()
