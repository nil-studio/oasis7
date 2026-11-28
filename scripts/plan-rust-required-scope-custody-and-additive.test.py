#!/usr/bin/env python3
import copy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

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


class AdditiveGovernanceRegistrations(_ScopeFixtureBase):
    driver = 'scripts/ci-tests.sh'
    first = '  run python3 ./scripts/security/first.test.py\n'
    second = '  run python3 ./scripts/security/test_second.py\n'
    added = '  run python3 ./scripts/security/new.test.py\n'
    body = ('#!/bin/sh\nset -eu\n\n'
            'run_workflow_governance_operational_contract_tests() {\n'
            + first + second + '}\n\nrun_workflow_governance_operational_contract_tests\n')

    def setUp(self):
        super().setUp()
        self.config = json.loads((HERE / 'ci-required-scope.json').read_text())
        self.write(self.driver, self.body)
        self.write('scripts/security/first.test.py', '# existing first diagnostic\n')
        self.write('scripts/security/test_second.py', '# existing second diagnostic\n')
        self.write('scripts/ci-required-scope.json', json.dumps(self.config))
        self.base = self.commit()

    def candidate(self, body=None, target='# new diagnostic\n'):
        self.git('checkout', '-q', self.base)
        self.write(self.driver, body if body is not None else self.body.replace(self.second, self.added + self.second))
        if target is not None:
            self.write('scripts/security/new.test.py', target)

    def select(self, source, tested=None, config=None):
        return selector.select(config if config is not None else self.config,
                               self.base, source, tested, profile='pr-fast', policy_ref=self.base)

    def assert_full(self, plan):
        self.assertEqual(plan['scope'], 'full')
        self.assertEqual(set(plan['groups']), set(self.config['groups']) - {'formal_process'})
        self.assertEqual(plan['formal_suites'], [])
        self.assertEqual(plan['formal_shards'], [])

    def test_old_formal_policy_preserves_full_fallback_for_invalid_registration(self):
        old = copy.deepcopy(self.config)
        old.pop('formal_full_profiles_only_version')
        old.pop('formal_deferred_ordinary_cases')
        self.candidate(self.body + 'exit 0\n')
        plan = self.select(self.commit(), config=old)
        self.assertEqual(plan['scope'], 'full')
        self.assertEqual(set(plan['groups']), set(self.config['groups']))
        self.assertEqual(plan['formal_suites'], sorted(selector.FORMAL_SUITES))

    def test_addition_keeps_existing_tests_and_trusted_diagnostic_companions(self):
        self.candidate()
        self.write('.github/workflows/codeql.yml', '# already scoped diagnostic workflow\n')
        self.write('doc/readme.md', 'diagnostic documentation\n')
        plan = self.select(self.commit())
        self.assertEqual(plan['scope'], 'targeted')
        self.assertEqual(plan['groups'], ['baseline', 'workflow_governance'])
        self.assertEqual(plan['affected_packages'], [])
        self.assertEqual(plan['formal_suites'], [])
        self.assertEqual(plan['fast_plan_exceptions'], [])
        self.assertTrue(any('proved additive diagnostic test registration' in reason for reason in plan['reasons']))

    def test_existing_test_prefix_naming_can_be_added(self):
        self.candidate(self.body.replace(self.second, self.second +
                       '  run python3 ./scripts/security/test_new_diagnostic.py\n'), target=None)
        self.write('scripts/security/test_new_diagnostic.py', '# diagnostic\n')
        self.assertEqual(self.select(self.commit())['groups'], ['baseline', 'workflow_governance'])

    def test_removal_reorder_duplicate_shell_syntax_and_other_driver_changes_stay_full(self):
        valid = self.body.replace(self.second, self.added + self.second)
        variants = {
            'remove old registration': valid.replace(self.first, ''),
            'change old arguments': valid.replace(self.first, self.first.rstrip() + ' --skip\n'),
            'reorder old registrations': valid.replace(self.first + self.added + self.second,
                                                       self.second + self.added + self.first),
            'duplicate old registration': valid.replace(self.first, self.first * 2),
            'duplicate new registration': valid.replace(self.added, self.added * 2),
            'shell substitution': valid.replace('new.test.py', '$(true).test.py'),
            'new arguments': valid.replace(self.added, self.added.rstrip() + ' --skip\n'),
            'control flow': valid.replace(self.added, '  if true; then\n' + self.added + '  fi\n'),
            'comment': valid.replace(self.added, '  # allow additional shell syntax\n' + self.added),
            'blank line': valid.replace(self.added, '\n' + self.added),
            'outside function': valid + 'exit 0\n',
            'duplicate function': valid + self.body,
            'renamed function': valid.replace('run_workflow_governance_operational_contract_tests', 'other'),
            'non-test executable': valid.replace('new.test.py', 'new.py'),
            'nested path': valid.replace('new.test.py', 'nested/new.test.py'),
            'invalid encoding': valid + '\udcff',
            'unbounded input': valid + '#' * (256 * 1024),
        }
        for reason, body in variants.items():
            with self.subTest(reason=reason):
                self.candidate(target=None)
                Path(self.driver).write_bytes(body.encode('utf-8', errors='surrogateescape'))
                self.write('scripts/security/new.test.py', '# diagnostic\n')
                self.assert_full(self.select(self.commit()))

    def test_new_registration_requires_regular_existing_trusted_diagnostic_test(self):
        for kind in ('missing', 'symlink', 'directory', 'excluded', 'other group', 'full rule'):
            with self.subTest(kind=kind):
                self.candidate(target=None)
                target = Path('scripts/security/new.test.py')
                if kind == 'symlink':
                    target.symlink_to('first.test.py')
                elif kind == 'directory':
                    self.write(str(target / 'child'), '# not a regular executable\n')
                elif kind not in ('missing',):
                    target.write_text('# diagnostic\n')
                config = copy.deepcopy(self.config)
                if kind == 'excluded':
                    for rule in config['rules']:
                        if rule['reason'] == 'security':
                            rule['exclude'] = [str(target)]
                    # This path stays registered as a document-like input, but
                    # that does not authorize running it as a diagnostic test.
                    config['rules'].append({'match': [str(target)], 'groups': [], 'reason': 'fixture'})
                elif kind in ('other group', 'full rule'):
                    config['rules'].append({'match': [str(target)], 'groups': ['net'],
                                            'reason': 'fixture', 'full': kind == 'full rule'})
                self.assert_full(self.select(self.commit(), config=config))

    def test_mode_change_deletion_and_rename_do_not_gain_the_exception(self):
        self.candidate()
        Path(self.driver).chmod(0o755)
        self.assert_full(self.select(self.commit()))
        self.candidate()
        Path(self.driver).unlink()
        self.assert_full(self.select(self.commit()))
        self.candidate()
        self.git('mv', self.driver, 'scripts/ci-tests-other.sh')
        self.assert_full(self.select(self.commit()))

    def test_missing_unsupported_or_boolean_baseline_capability_stays_full(self):
        self.candidate()
        source = self.commit()
        for version in (None, False, True, 0, 2, '1'):
            with self.subTest(version=version):
                config = copy.deepcopy(self.config)
                config.pop('additive_governance_registration_version')
                if version is not None:
                    config['additive_governance_registration_version'] = version
                self.assert_full(self.select(source, config=config))

    def test_candidate_policy_cannot_enroll_its_own_registration(self):
        old = copy.deepcopy(self.config)
        old.pop('additive_governance_registration_version')
        self.write('scripts/ci-required-scope.json', json.dumps(old))
        self.base = self.commit()
        self.candidate()
        self.write('scripts/ci-required-scope.json', json.dumps(self.config))
        self.assert_full(self.select(self.commit(), config=old))

    def test_product_and_shared_control_mixtures_stay_full(self):
        for path, contents in (
                ('crates/core/src/lib.rs', 'pub fn changed() {}\n'),
                ('tools/oasis7-command-center/ui/src/app.tsx', '// UI change\n'),
                ('.github/workflows/rust.yml', '# required workflow change\n'),
                ('Cargo.toml', '[workspace]\nmembers=["crates/*"]\nresolver="2"\n'
                               '[workspace.dependencies]\ncore={path="crates/core"}\n'),
                ('scripts/ci-required-scope.json', json.dumps(self.config) + '\n'),
                ('scripts/build-diagnostic.sh', '# shared build change\n')):
            with self.subTest(path=path):
                self.candidate()
                self.write(path, contents)
                self.assert_full(self.select(self.commit()))

    def test_registered_security_fixture_with_rust_consumer_stays_full(self):
        self.write('scripts/security/new.test.py', 'VALUE = 1\n')
        self.write('crates/core/src/lib.rs',
                   'pub const DIAGNOSTIC: &str = include_str!("../../../scripts/security/new.test.py");\n')
        self.base = self.commit()
        self.candidate(target='VALUE = 2\n')
        plan = self.select(self.commit())
        self.assert_full(plan)
        self.assertIn('core', plan['affected_packages'])

    def test_unknown_companion_remains_an_ownership_error(self):
        self.candidate()
        self.write('unregistered/new-input.bin', 'unknown input\n')
        with self.assertRaises(selector.UnknownInputsError):
            self.select(self.commit())

    def test_source_and_tested_merge_both_must_preserve_registrations(self):
        for mutate in (False, True):
            with self.subTest(merge_mutation=mutate):
                self.candidate()
                source = self.commit()
                self.git('checkout', '-q', self.base)
                self.write('doc/readme.md', 'target advanced\n')
                self.commit()
                self.git('merge', '--no-ff', '--no-commit', source)
                if mutate:
                    self.write(self.driver, self.body.replace(self.second, self.added + self.second)
                               .replace(self.first, ''))
                tested = self.commit()
                plan = self.select(source, tested)
                if mutate:
                    self.assert_full(plan)
                else:
                    self.assertEqual(plan['groups'], ['baseline', 'workflow_governance'])

    def test_merge_only_addition_is_proved_without_attributing_target_history(self):
        self.write('doc/readme.md', 'source docs\n')
        source = self.commit()
        self.git('checkout', '-q', self.base)
        self.write('doc/target.md', 'target advanced\n')
        self.commit()
        self.git('merge', '--no-ff', '--no-commit', source)
        self.write(self.driver, self.body.replace(self.second, self.added + self.second))
        self.write('scripts/security/new.test.py', '# diagnostic\n')
        self.assertEqual(self.select(source, self.commit())['groups'], ['baseline', 'workflow_governance'])


if __name__ == '__main__':
    unittest.main()
