#!/usr/bin/env python3
import copy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import textwrap
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


class MethodCallCfgOwners(unittest.TestCase):
    PATH = 'crates/oasis7_chain_runtime/src/host/execution_bridge/formal_writer.rs'
    HOOK = '#[cfg(all(test, unix))] crash::stop_after("publication_saved")?;'
    BEFORE = textwrap.dedent('''\
        impl<P: Endpoint, R: Endpoint> Writer<P, R> {
            fn resume(&mut self) -> Result<(), String> {
                #[cfg(all(test, unix))]
                crash::stop_after("dual_qualified")?;
                let head = verify_qualified(
                    &proof,
                    if normal { self.authority() } else { self.recovery() },
                    Some(self.head()),
                    if normal { None } else { self.recovery() },
                ).map_err(|error| error.to_string())?;
                #[cfg(all(test, unix))]
                crash::stop_after("publication_saved")?;
                Ok(())
            }
        }
    ''')
    AFTER = BEFORE.replace(
        'let head = verify_qualified(\n'
        '            &proof,\n'
        '            if normal { self.authority() } else { self.recovery() },\n'
        '            Some(self.head()),\n'
        '            if normal { None } else { self.recovery() },\n'
        '        ).map_err(|error| error.to_string())?;',
        'let head = saved_candidate::verify_qualified_commit(&self.anchor, &proof)?;')

    def policy(self, before, after):
        def source(*args):
            self.assertEqual(args[0], 'show')
            ref, path = args[1].split(':', 1)
            self.assertEqual(path, self.PATH)
            return {'base': before, 'source': after}[ref].encode()
        with patch.object(selector, 'git', side_effect=source):
            return selector.changed_test_policy(
                'base', 'source', [self.PATH], {self.PATH: {'oasis7_chain_runtime'}},
                {'oasis7_chain_runtime'},
                {'oasis7_chain_runtime': {'test_tier_required', 'test_tier_full'}})

    def assert_expanded(self, before, after):
        paths, ignored, production = self.policy(before, after)
        self.assertFalse(ignored)
        self.assertIn(self.PATH, paths + production)

    def test_generic_method_call_gate_survives_sibling_business_edit(self):
        self.assertNotEqual(self.BEFORE, self.AFTER)
        self.assertEqual(self.policy(self.BEFORE, self.AFTER), ([], False, []))

    def test_real_formal_writer_call_extraction_does_not_change_test_policy(self):
        # PR #6180 left every crash hook intact. The shorter verifier call moved
        # publication_saved by 67 tokens within the same generic impl method.
        after = (HERE.parent / self.PATH).read_text()
        extracted = textwrap.dedent('''\
            let head = saved_candidate::verify_qualified_commit(
                &self.anchor,
                self.recovery.as_ref(),
                &proof,
                candidate.candidate.record.position,
            )?;''')
        original = textwrap.dedent('''\
            let head = verify_formal_qualified(
                &proof,
                if proof.decision.branch == FormalCompletionBranchV1::Normal {
                    self.anchor.authority()
                } else {
                    self.recovery
                        .as_ref()
                        .ok_or("recovery proof lacks checked context")?
                        .original_authority()
                },
                Some(self.anchor.head()),
                if proof.decision.branch == FormalCompletionBranchV1::Normal {
                    None
                } else {
                    self.recovery.as_ref()
                },
            )
            .map_err(|e| e.to_string())?;''')
        extracted = textwrap.indent(extracted, '        ')
        original = textwrap.indent(original, '        ')
        self.assertEqual(after.count(extracted), 1)
        self.assertEqual(self.policy(after.replace(extracted, original), after), ([], False, []))

    def test_plain_generic_and_trait_impl_call_owners_are_stable(self):
        headers = (
            'impl Server',
            'impl<P: Endpoint, R: Endpoint> Writer<P, R>',
            'impl<T: Endpoint> Resume for Writer<T>',
            "impl<'a, T> Resume for Writer<'a, T> where T: Fn() -> usize",
        )
        for header in headers:
            before = header + ' { fn resume(&mut self) -> Result<(), String> { ' + self.HOOK + ' Ok(()) } }'
            variants = (
                'mod unrelated; ' + before,
                before.replace('fn resume', 'fn unrelated() {} fn resume'),
                before.replace(self.HOOK, 'let unrelated = business(); ' + self.HOOK),
                before.replace(self.HOOK, self.HOOK + ' business();'),
            )
            for after in variants:
                with self.subTest(header=header, after=after):
                    self.assertEqual(self.policy(before, after), ([], False, []))

    def test_call_gate_expression_or_lexical_owner_changes_expand(self):
        before = 'impl<T: Endpoint> Writer<T> { fn resume(&mut self) -> Result<(), String> { ' + self.HOOK + ' Ok(()) } }'
        variants = (
            before.replace('all(test, unix)', 'all(test, feature="test_tier_full")'),
            before.replace('"publication_saved"', '"candidate_saved"'),
            before.replace('stop_after', 'stop_before'),
            before.replace('Writer<T>', 'Other<T>'),
            before.replace('T: Endpoint', 'T: OtherEndpoint'),
            before.replace('fn resume', 'fn reopen'),
            before.replace('&mut self', '&self'),
            'mod other { ' + before + ' }',
        )
        for after in variants:
            with self.subTest(after=after):
                self.assert_expanded(before, after)

    def test_call_moved_into_other_method_or_conditional_scope_expands(self):
        before = 'impl<T> Writer<T> { fn resume() { ' + self.HOOK + ' } fn other() {} }'
        variants = (
            before.replace(self.HOOK, '').replace('fn other() {}', 'fn other() { ' + self.HOOK + ' }'),
            before.replace(self.HOOK, 'if ready { ' + self.HOOK + ' }'),
            before.replace(self.HOOK, 'let action = || { ' + self.HOOK + ' };'),
            before.replace(self.HOOK, 'opaque! { ' + self.HOOK + ' }'),
        )
        for after in variants:
            with self.subTest(after=after):
                self.assert_expanded(before, after)

    def test_identical_calls_keep_separate_gated_and_ungated_occurrences(self):
        bare = self.HOOK.split('] ', 1)[1]
        wrapper = 'impl<T> Writer<T> { fn resume() { %s } }'
        before = wrapper % (self.HOOK + bare)
        self.assert_expanded(before, wrapper % (bare + self.HOOK))
        self.assert_expanded(wrapper % (self.HOOK * 2), wrapper % self.HOOK)
        self.assert_expanded(wrapper % self.HOOK, wrapper % (self.HOOK * 2))

    def test_nested_macro_trait_and_ambiguous_headers_stay_conservative(self):
        wrappers = (
            'opaque! { impl<T> Writer<T> { fn resume() { %s } } }',
            'fn outer() { impl<T> Writer<T> { fn resume() { %s } } }',
            'impl<T> Writer<T> { fn outer() { fn resume() { %s } } }',
            'impl<T> Writer<T> { fn resume() { opaque! { %s } } }',
            'impl<T> Writer<T> { fn resume() { if ready { %s } } }',
            'trait Resume { fn resume() { %s } }',
            'impl Writer<{ 1 + 1 }> { fn resume() { %s } }',
            'fn outer() -> impl Resume { fn resume() { %s } Writer }',
        )
        for wrapper in wrappers:
            before = wrapper % self.HOOK
            with self.subTest(wrapper=wrapper):
                self.assert_expanded(before, 'mod unrelated; ' + before)

    def test_macro_call_and_nested_argument_scope_stay_conservative(self):
        for hook in (
            '#[cfg(test)] hook!("publication_saved");',
            '#[cfg(test)] crash::stop_after({ value() })?;',
            '#[cfg(test)] crash::stop_after(value!())?;',
        ):
            before = 'impl<T> Writer<T> { fn resume() { ' + hook + ' } }'
            with self.subTest(hook=hook):
                self.assert_expanded(before, 'mod unrelated; ' + before)

    def test_attribute_macro_input_keeps_conservative_ownership(self):
        wrappers = (
            '#[opaque] impl<T> Writer<T> { fn resume() { %s } }',
            '#[opaque] mod inner { impl<T> Writer<T> { fn resume() { %s } } }',
            'impl<T> Writer<T> { #[opaque] fn resume() { %s } }',
            'impl<T> Writer<T> { fn resume() { #[opaque] %s } }',
            'impl<T> Writer<T> { fn resume() { #[cfg_attr(test, opaque)] %s } }',
            'mod inner { #![opaque] impl<T> Writer<T> { fn resume() { %s } } }',
        )
        for wrapper in wrappers:
            before = wrapper % self.HOOK
            with self.subTest(wrapper=wrapper):
                self.assert_expanded(before, 'mod unrelated; ' + before)

    def test_new_unknown_gate_and_ignore_remain_rejected(self):
        for hook in (
            self.HOOK.replace('all(test, unix)', 'all(test, unknown_gate)'),
            '#[cfg_attr(test, ignore)] ' + self.HOOK,
        ):
            with self.subTest(hook=hook), self.assertRaises(selector.TestPolicyError):
                self.policy(self.BEFORE, self.BEFORE.replace(
                    '#[cfg(all(test, unix))]\n        crash::stop_after("publication_saved")?;', hook))


class ScopeSliceAndDirectSupportGateTests(_ScopeFixtureBase):
    def setUp(self):
        super().setUp()
        self.config = json.loads((HERE / 'ci-required-scope.json').read_text())
        self.write(
            'Cargo.toml',
            '[workspace]\nmembers=["crates/oasis7_runtime", "crates/oasis7_context", '
            '"crates/oasis7_local_signer", "crates/oasis7_custody_worker"]\nresolver="2"\n',
        )
        for crate in ('oasis7_runtime', 'oasis7_context', 'oasis7_local_signer', 'oasis7_custody_worker'):
            self.write(f'crates/{crate}/Cargo.toml', f'[package]\nname="{crate}"\nversion="0.1.0"\nedition="2021"\n')
            self.write(f'crates/{crate}/src/lib.rs', 'pub fn item() {}\n')
        self.write('scripts/ci-required-scope.json', json.dumps(self.config))
        self.base = self.commit()

    def test_ordinary_scope_config_slice_narrows_harness_costs_and_fast_rules_while_keeping_control_full(self):
        # 1) ordinary_harness_costs-only edit stays targeted without test_policy_changed
        costs_cfg = copy.deepcopy(self.config)
        costs_cfg['ordinary_harness_costs']['fallback_ms'] += 10
        self.write('scripts/ci-required-scope.json', json.dumps(costs_cfg))
        costs_head = self.commit()
        plan = selector.select(self.config, self.base, costs_head, profile='pr-fast')
        self.assertEqual(plan['scope'], 'targeted')
        self.assertEqual(
            set(plan['groups']),
            {'baseline', 'workflow_governance', 'rust_baseline', 'oasis7_required'},
        )
        self.assertFalse(plan['test_policy_changed'])
        self.assertEqual(plan['oasis7_required_packages'], sorted(self.config['oasis7_required_packages']))

        # 2) ordinary_scale_contract / ordinary_fast_rules edit widens policy + consensus/node/distfs, still targeted
        fast_cfg = copy.deepcopy(costs_cfg)
        fast_cfg['ordinary_fast_rules'][0]['reason'] = 'updated-slice-reason'
        self.write('scripts/ci-required-scope.json', json.dumps(fast_cfg))
        fast_head = self.commit()
        plan = selector.select(self.config, costs_head, fast_head, profile='pr-fast')
        self.assertEqual(plan['scope'], 'targeted')
        self.assertEqual(
            set(plan['groups']),
            {'baseline', 'workflow_governance', 'rust_baseline', 'oasis7_required', 'consensus', 'node', 'distfs'},
        )
        self.assertTrue(plan['test_policy_changed'])
        self.assertIn('scripts/ci-required-scope.json', plan['test_policy_paths'])

        # 3) Untrusted/old baseline without ordinary_scope_config_slice_version keeps full fallback
        old_policy = copy.deepcopy(self.config)
        del old_policy['ordinary_scope_config_slice_version']
        self.assertEqual(selector.select(old_policy, costs_head, fast_head, profile='pr-fast')['scope'], 'full')

        # 4) Editing scoped_control_paths, groups, or duplicate JSON keys falls back to full
        control_cfg = copy.deepcopy(fast_cfg)
        control_cfg['scoped_control_paths']['.github/dependabot.yml'] = ['workflow_governance', 'site_quality']
        self.write('scripts/ci-required-scope.json', json.dumps(control_cfg))
        control_head = self.commit()
        self.assertEqual(selector.select(self.config, fast_head, control_head, profile='pr-fast')['scope'], 'full')

        dup_raw = '{"fast_scope_version": 1, "fast_scope_version": 1}'
        self.write('scripts/ci-required-scope.json', dup_raw)
        dup_head = self.commit()
        self.assertEqual(selector.select(self.config, fast_head, dup_head, profile='pr-fast')['scope'], 'full')

    def test_ordinary_scope_config_slice_v2_narrows_additive_and_retired_rules_safely(self):
        # 1) Adding a non-empty, non-full rule (e.g. PR #6768 operational script registration) stays targeted
        added_cfg = copy.deepcopy(self.config)
        added_cfg['rules'].append({
            'match': ['scripts/release-candidate-bundle.sh'],
            'groups': ['operational_contracts'],
            'reason': 'release_candidate_bundle_operational_contract',
        })
        self.write('scripts/ci-required-scope.json', json.dumps(added_cfg))
        added_head = self.commit()
        plan = selector.select(self.config, self.base, added_head, profile='pr-fast')
        self.assertEqual(plan['scope'], 'targeted')
        self.assertEqual(
            set(plan['groups']),
            {'baseline', 'workflow_governance', 'operational_contracts'},
        )
        self.assertFalse(plan['test_policy_changed'])

        # 2) Under v1 config, any rules edit still conservatively falls back to full
        v1_policy = copy.deepcopy(self.config)
        v1_policy['ordinary_scope_config_slice_version'] = 1
        self.assertEqual(selector.select(v1_policy, self.base, added_head, profile='pr-fast')['scope'], 'full')

        # 3) Removing a rule that matches zero tracked files in both trees stays targeted
        self.write('scripts/ci-required-scope.json', json.dumps(self.config))
        removed_head = self.commit()
        plan = selector.select(added_cfg, added_head, removed_head, profile='pr-fast')
        self.assertEqual(plan['scope'], 'targeted')
        self.assertEqual(set(plan['groups']), {'baseline', 'workflow_governance'})

        # 4) Removing a rule that matches an existing tracked file falls back to full
        self.write('scripts/release-candidate-bundle.sh', '#!/usr/bin/env bash\n')
        self.write('scripts/ci-required-scope.json', json.dumps(added_cfg))
        active_base = self.commit()
        self.write('scripts/ci-required-scope.json', json.dumps(self.config))
        active_removed = self.commit()
        self.assertEqual(selector.select(added_cfg, active_base, active_removed, profile='pr-fast')['scope'], 'full')

        # 5) Adding a rule with empty groups (silent ignore) or reordering rules falls back to full
        empty_groups_cfg = copy.deepcopy(self.config)
        empty_groups_cfg['rules'].append({'match': ['untracked-candidate/**'], 'groups': [], 'reason': 'ignore'})
        self.write('scripts/ci-required-scope.json', json.dumps(empty_groups_cfg))
        empty_head = self.commit()
        self.assertEqual(selector.select(self.config, active_removed, empty_head, profile='pr-fast')['scope'], 'full')

        reordered_cfg = copy.deepcopy(self.config)
        reordered_cfg['rules'] = list(reversed(reordered_cfg['rules']))
        self.write('scripts/ci-required-scope.json', json.dumps(reordered_cfg))
        reordered_head = self.commit()
        self.assertEqual(selector.select(self.config, active_removed, reordered_head, profile='pr-fast')['scope'], 'full')

        # 6) Extending an existing rule's `match` list (order-preserving superset) + introducing
        #    a new file matching that extension in the SAME PR succeeds without UnknownInputsError
        #    and stays targeted to workflow_governance + target group.
        superset_cfg = copy.deepcopy(self.config)
        superset_cfg['rules'][0]['match'] = [*superset_cfg['rules'][0]['match'], 'scripts/brand-new-hygiene-check.py']
        target_groups = set(superset_cfg['rules'][0]['groups'])
        self.write('scripts/ci-required-scope.json', json.dumps(superset_cfg))
        self.write('scripts/brand-new-hygiene-check.py', '#!/usr/bin/env python3\nprint("ok")\n')
        same_pr_head = self.commit()
        same_pr_plan = selector.select(self.config, active_removed, same_pr_head, profile='pr-fast')
        self.assertEqual(same_pr_plan['scope'], 'targeted')
        self.assertEqual(set(same_pr_plan['groups']), {'baseline', 'workflow_governance', *target_groups})

        # 7) Co-changing scripts/ci-tests.sh alongside a new script + same-PR rule registration
        #    still conservatively selects scope='full' for ci-tests.sh, while avoiding UnknownInputsError.
        self.write('scripts/ci-tests.sh', '#!/usr/bin/env bash\necho updated\n')
        cochange_head = self.commit()
        cochange_plan = selector.select(self.config, active_removed, cochange_head, profile='pr-fast')
        self.assertEqual(cochange_plan['scope'], 'full')

        # 8) Pre-existing unregistered file modified (not newly added) cannot self-register via candidate rules.
        self.write('scripts/ci-required-scope.json', json.dumps(self.config))
        self.write('scripts/preexisting-unmapped.py', 'v = 1\n')
        preexisting_base = self.commit()
        attempt_cfg = copy.deepcopy(self.config)
        attempt_cfg['rules'].append({
            'match': ['scripts/preexisting-unmapped.py'],
            'groups': ['baseline'],
            'reason': 'late_self_registration',
        })
        self.write('scripts/ci-required-scope.json', json.dumps(attempt_cfg))
        self.write('scripts/preexisting-unmapped.py', 'v = 2\n')
        attempt_head = self.commit()
        with self.assertRaises(selector.UnknownInputsError):
            selector.select(self.config, preexisting_base, attempt_head, profile='pr-fast')

    def test_ordinary_scope_config_slice_v2_narrows_additive_governance_test_splits_safely(self):
        self.write('scripts/ci-workflow.test.py', '# existing suite\n')
        self.write('scripts/ci-required-scope.json', json.dumps(self.config))
        split_base = self.commit()

        new_test = 'scripts/ci-workflow-split-part99.test.py'
        split_cfg = copy.deepcopy(self.config)
        split_cfg['scoped_control_paths'][new_test] = ['workflow_governance']
        split_cfg['workflow_governance_python_tests'] = [
            *split_cfg['workflow_governance_python_tests'],
            new_test,
        ]
        split_cfg['python_control_contracts'][new_test] = {
            'group': 'workflow_governance',
            'tests': [new_test],
        }
        split_cfg['python_control_contracts']['scripts/ci-workflow.test.py']['tests'] = [
            *split_cfg['python_control_contracts']['scripts/ci-workflow.test.py']['tests'],
            new_test,
        ]
        self.write('scripts/ci-workflow.test.py', '# split out part99\n')
        self.write(new_test, '# split suite\n')
        self.write('scripts/ci-required-scope.json', json.dumps(split_cfg))
        split_head = self.commit()

        # 1) Purely additive governance test split slices cleanly to baseline + workflow_governance
        plan = selector.select(self.config, split_base, split_head, profile='pr-fast')
        self.assertEqual(plan['scope'], 'targeted', plan['reasons'])
        self.assertEqual(set(plan['groups']), {'baseline', 'workflow_governance'})
        self.assertIn('scripts/ci-workflow.test.py', plan['python_control_paths'])
        self.assertFalse(plan['test_policy_changed'])

        # 2) Under v1 policy, governance test split conservatively falls back to full
        v1_policy = copy.deepcopy(self.config)
        v1_policy['ordinary_scope_config_slice_version'] = 1
        self.assertEqual(selector.select(v1_policy, split_base, split_head, profile='pr-fast')['scope'], 'full')

        # 3) Co-changing scripts/ci-tests.sh still conservatively falls back to full
        self.write('scripts/ci-tests.sh', '#!/usr/bin/env bash\necho changed\n')
        cochange_head = self.commit()
        self.assertEqual(selector.select(self.config, split_base, cochange_head, profile='pr-fast')['scope'], 'full')

        # 4) Pre-existing unregistered scripts/*.test.py modified (not newly added) cannot self-enroll
        pre_test = 'scripts/ci-preexisting-unregistered.test.py'
        self.write('scripts/ci-required-scope.json', json.dumps(self.config))
        self.write(pre_test, 'x = 1\n')
        pre_base = self.commit()
        pre_cfg = copy.deepcopy(self.config)
        pre_cfg['scoped_control_paths'][pre_test] = ['workflow_governance']
        pre_cfg['workflow_governance_python_tests'] = [*pre_cfg['workflow_governance_python_tests'], pre_test]
        pre_cfg['python_control_contracts'][pre_test] = {'group': 'workflow_governance', 'tests': [pre_test]}
        self.write('scripts/ci-required-scope.json', json.dumps(pre_cfg))
        self.write(pre_test, 'x = 2\n')
        pre_head = self.commit()
        self.assertEqual(selector.select(self.config, pre_base, pre_head, profile='pr-fast')['scope'], 'full')

        # 5) Reordering workflow_governance_python_tests or mapping to another group falls back to full
        reordered_cfg = copy.deepcopy(split_cfg)
        reordered_cfg['workflow_governance_python_tests'] = list(
            reversed(reordered_cfg['workflow_governance_python_tests'])
        )
        self.write('scripts/ci-required-scope.json', json.dumps(reordered_cfg))
        reordered_head = self.commit()
        self.assertEqual(selector.select(self.config, split_base, reordered_head, profile='pr-fast')['scope'], 'full')

        # 6) Splitting a non-EVOLVING_POLICY_TESTS suite (e.g. ci-local-check.test.py) keeps scope='full'
        #    so policy_sha ci-python-control-contracts.py authorizes baseline identity migration.
        self.write('scripts/ci-local-check.test.py', '# non-evolving suite\n')
        self.write('scripts/ci-required-scope.json', json.dumps(self.config))
        nonevolving_base = self.commit()
        nonevolving_new = 'scripts/ci-local-check-part2.test.py'
        nonevolving_cfg = copy.deepcopy(self.config)
        nonevolving_cfg['scoped_control_paths'][nonevolving_new] = ['workflow_governance']
        nonevolving_cfg['workflow_governance_python_tests'] = [
            *nonevolving_cfg['workflow_governance_python_tests'],
            nonevolving_new,
        ]
        nonevolving_cfg['python_control_contracts'][nonevolving_new] = {
            'group': 'workflow_governance',
            'tests': [nonevolving_new],
        }
        self.write('scripts/ci-local-check.test.py', '# retired case into part2\n')
        self.write(nonevolving_new, '# split part2\n')
        self.write('scripts/ci-required-scope.json', json.dumps(nonevolving_cfg))
        nonevolving_head = self.commit()
        self.assertEqual(
            selector.select(self.config, nonevolving_base, nonevolving_head, profile='pr-fast')['scope'],
            'full',
        )

    def test_standalone_non_workspace_cargo_manifests_scope_to_owners_and_keep_root_or_patch_full(self):
        cases = {
            'tools/oasis7-command-center/Cargo.toml': (
                '[workspace]\nmembers=["core"]\n',
                {'baseline', 'rust_baseline', 'cargo_tooling_contracts', 'command_center'},
                [],
            ),
            'tools/wasm_build_suite/templates/minimal_module/Cargo.toml': (
                '[package]\nname="minimal_module"\nversion="0.1.0"\n',
                {'baseline', 'rust_baseline', 'cargo_tooling_contracts', 'workspace_support'},
                ['wasm_build_suite'],
            ),
            'crates/oasis7_builtin_wasm_modules/m1_rule_move/Cargo.toml': (
                '[package]\nname="m1_rule_move"\nversion="0.1.0"\n',
                {'baseline', 'rust_baseline', 'cargo_tooling_contracts', 'distfs', 'oasis7_required', 'workspace_support'},
                ['oasis7_wasm_build', 'wasm_build_suite'],
            ),
        }
        for path, (body, _, _) in cases.items():
            self.write(path, body)
        base = self.commit()
        for path, (body, expected_groups, expected_profiles) in cases.items():
            with self.subTest(path=path):
                self.write(path, body + '# comment\n')
                head = self.commit()
                plan = selector.select(self.config, base, head, profile='pr-fast')
                self.assertEqual(plan['scope'], 'targeted', plan['reasons'])
                self.assertEqual(set(plan['groups']), expected_groups)
                self.assertEqual(plan['workspace_support_profiles'], expected_profiles)
                base = head

        # Malformed TOML or [patch] section in standalone manifest fails closed to full
        self.write('tools/oasis7-command-center/Cargo.toml', '[patch.crates-io]\nfoo={path="../foo"}\n')
        patch_head = self.commit()
        self.assertEqual(selector.select(self.config, base, patch_head, profile='pr-fast')['scope'], 'full')

    def test_scoped_independent_workflows_and_non_python_control_scripts_preserve_forbidden_shared_controls(self):
        for path, expected in (
            ('.github/dependabot.yml', {'baseline', 'workflow_governance'}),
            ('.github/workflows/pages.yml', {'baseline', 'workflow_governance', 'site_quality'}),
            ('.github/workflows/rust-cache.yml', {'baseline', 'workflow_governance', 'packaging_contracts', 'macos_package_contract'}),
            ('scripts/ci-rust-governance-report.sh', {'baseline', 'workflow_governance'}),
            ('scripts/ci-compile-metrics.sh', {'baseline', 'workflow_governance', 'compile_metrics'}),
        ):
            with self.subTest(path=path):
                self.write(path, '#existing\n')
                base = self.commit()
                self.write(path, '#updated\n')
                head = self.commit()
                plan = selector.select(self.config, base, head, profile='pr-fast')
                self.assertEqual(plan['scope'], 'targeted')
                self.assertEqual(set(plan['groups']), expected)

        for forbidden in (
            '.github/workflows/rust.yml',
            '.github/workflows/command-center.yml',
            '.github/actions/setup-linux-mold/action.yml',
            '.github/actions/cache-permission-probe/action.yml',
            'scripts/ci-tests.sh',
            'scripts/ci-required-scope.json',
            'scripts/ci-formal-process-suites.json',
            'scripts/plan-rust-required-scope.sh',
        ):
            with self.subTest(forbidden=forbidden), self.assertRaises(ValueError):
                bad = copy.deepcopy(self.config)
                bad['scoped_control_paths'][forbidden] = ['workflow_governance']
                selector.validate_config(bad)

    def test_workspace_support_direct_gate_capability_emits_only_for_non_custody_image_profiles(self):
        # Live config (v2): image-only non-custody support crate (oasis7_context) -> direct gate v2 emitted
        self.write('crates/oasis7_context/src/lib.rs', 'pub fn changed() {}\n')
        ctx_head = self.commit()
        plan = selector.select(self.config, self.base, ctx_head, profile='pr-fast')
        self.assertEqual(plan['workspace_support_partitions'], ['other'])
        self.assertEqual(plan.get('workspace_support_direct_gate_version'), 2)
        self.assertNotIn('custody_execution_version', plan)

        # Exact targeted signer support can execute directly in the non-root image.
        self.git('checkout', '-q', self.base)
        self.write('crates/oasis7_local_signer/src/lib.rs', 'pub fn changed() {}\n')
        signer_head = self.commit()
        signer_plan = selector.select(self.config, self.base, signer_head, profile='pr-fast')
        self.assertEqual(signer_plan['workspace_support_profiles'], ['oasis7_local_signer'])
        self.assertEqual(signer_plan.get('workspace_support_direct_gate_version'), 2)
        self.assertTrue(selector.workspace_support_selects_signer_image(signer_plan))

        self.write('crates/oasis7_context/src/lib.rs', 'pub fn changed_context() {}\n')
        mixed_head = self.commit()
        mixed_plan = selector.select(self.config, self.base, mixed_head, profile='pr-fast')
        self.assertEqual(mixed_plan['workspace_support_profiles'], ['oasis7_context', 'oasis7_local_signer'])
        self.assertTrue(selector.workspace_support_selects_signer_image(mixed_plan))
        self.assertEqual(mixed_plan.get('workspace_support_direct_gate_version'), 2)
        for profiles in (['oasis7_local_signer', 'oasis7_signer_protocol'],
                         ['oasis7_context', 'oasis7_local_signer', 'oasis7_signer_protocol']):
            with self.subTest(profiles=profiles):
                self.assertTrue(selector.workspace_support_selects_signer_image(
                    dict(mixed_plan, workspace_support_profiles=profiles)))
        for extra in ('oasis7_game_client', 'oasis7_dev_test_worker', 'oasis7_secure_envelope'):
            with self.subTest(extra=extra):
                self.assertFalse(selector.workspace_support_selects_signer_image(
                    dict(mixed_plan, workspace_support_profiles=sorted([*mixed_plan['workspace_support_profiles'], extra]))))

        # Live config (v2, custody offboarded): custody crate -> no custody_execution_version, direct gate v2 emitted, other partition
        self.git('checkout', '-q', self.base)
        self.write('crates/oasis7_custody_worker/src/lib.rs', 'pub fn changed() {}\n')
        custody_head = self.commit()
        custody_plan = selector.select(self.config, self.base, custody_head, profile='pr-fast')
        self.assertNotIn('custody_execution_version', custody_plan)
        self.assertEqual(custody_plan.get('workspace_support_direct_gate_version'), 2)
        self.assertEqual(custody_plan.get('workspace_support_partitions'), ['other'])

        # Policy v1 compatibility: emits 1 for non-custody other-only, omits for custody-split
        v1_config = copy.deepcopy(self.config)
        v1_config['workspace_support_direct_gate_version'] = 1
        v1_config['custody_execution_version'] = 1
        v1_config['custody_node_shards_version'] = 2
        ctx_v1 = selector.select(v1_config, self.base, ctx_head, profile='pr-fast')
        self.assertEqual(ctx_v1.get('workspace_support_direct_gate_version'), 1)
        custody_v1 = selector.select(v1_config, self.base, custody_head, profile='pr-fast')
        self.assertEqual(custody_v1.get('custody_execution_version'), 1)
        self.assertNotIn('workspace_support_direct_gate_version', custody_v1)


if __name__ == '__main__':
    unittest.main()
