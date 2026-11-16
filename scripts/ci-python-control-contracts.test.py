#!/usr/bin/env python3
"""Trusted assertion loading, completion evidence and real offline CI fixtures."""
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch


HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('controls', HERE / 'ci-python-control-contracts.py')
controls = importlib.util.module_from_spec(spec)
spec.loader.exec_module(controls)


# Independent minimum assertions carried by the immutable baseline test source.
# New invariants may be added; a candidate cannot remove these and its own checks.
REQUIRED_FIXED_TESTS = json.loads(r'''
{
  "scripts/ci-formal-process.test.py": [
    "Transition.test_selected_requires_real_success_and_not_selected_requires_skip",
    "Transition.test_formal_builder_failure_and_skip_combinations",
    "FastExecutionSelection.test_old_planner_and_uncertain_scope_cannot_narrow_old_inventory",
    "FastExecutionSelection.test_changed_test_policy_includes_expanded_features_before_merge",
    "FastExecutionSelection.test_source_and_tested_merge_are_bound_separately",
    "FastExecutionSelection.test_subset_artifact_rejects_another_consumer_plan_before_files",
    "FastExecutionSelection.test_selected_package_or_behavior_cannot_disappear_from_artifact",
    "FastExecutionSelection.test_selected_fast_bundle_runs_behavior_and_rejects_zero_execution",
    "FastTargetInventory.test_minimum_cases_target_and_actual_features_are_verified",
    "FastTargetInventory.test_full_supplements_cannot_replace_original_required_inventory",
    "InlineExecutionSelection.test_known_inline_capability_rejects_incomplete_or_expanded_plans",
    "InlineExecutionSelection.test_clippy_owner_requires_the_complete_known_inline_plan",
    "DriverCoverage.test_inline_driver_builds_once_runs_manifest_and_propagates_each_failure"
  ],
  "scripts/ci-python-control-contracts.test.py": [
    "TrustedContracts.test_baseline_code_loads_candidate_with_correct_main_and_inherited_inventory",
    "TrustedContracts.test_removing_candidate_tests_cannot_hide_a_broken_implementation",
    "TrustedContracts.test_import_exit_zero_and_os_exit_zero_have_no_completion_witness",
    "TrustedContracts.test_selected_test_failure_skip_and_empty_inventory_fail_closed",
    "TrustedContracts.test_candidate_cannot_retire_one_case_while_other_cases_still_exist",
    "TrustedContracts.test_procedural_main_cannot_drop_one_scenario_or_its_assertions",
    "TrustedContracts.test_procedural_scenario_failure_caught_by_main_is_still_failure",
    "TrustedContracts.test_inline_procedural_scenarios_have_stable_required_identities",
    "TrustedContracts.test_baseline_test_helpers_cannot_be_replaced_by_candidate_noops",
    "TrustedContracts.test_support_shell_uses_candidate_transitive_helpers_and_keeps_baseline_assertions",
    "TrustedContracts.test_support_shell_injection_is_limited_to_its_independent_fixture",
    "TrustedContracts.test_legacy_support_driver_keeps_the_original_fixture_path",
    "TrustedContracts.test_fixed_safety_inventory_exists_in_current_repository",
    "SelectionAndMigration.test_registered_test_path_alone_selects_the_complete_baseline_contract",
    "SelectionAndMigration.test_target_only_policy_advance_does_not_authorize_source_migration",
    "SelectionAndMigration.test_full_dispatch_executes_fixed_and_selected_baseline_contracts"
  ],
  "scripts/ci-required-result.test.py": [
    "Results.test_command_center_failure_cannot_pass_gate",
    "Results.test_expected_success",
    "Results.test_selected_failure_cancelled_missing_skipped_unknown",
    "Results.test_selected_workspace_support_failure_or_missing_result_fails_gate",
    "Results.test_select_failure_or_skip",
    "Results.test_unselected_failure_is_not_hidden",
    "Results.test_added_workflow_dependency_must_explicitly_succeed",
    "Results.test_invalid_groups_fail_closed",
    "Results.test_extra_legacy_plan_fields_do_not_change_results",
    "Results.test_full_main_and_exact_manual_candidate",
    "Results.test_full_rejects_every_missing_or_unsuccessful_required_job",
    "Results.test_full_cannot_hide_platform_by_omitting_inventory",
    "Results.test_full_checks_exact_identity_profile_and_inventory_version",
    "Results.test_full_candidate_requires_read_only_full_lane_not_main_success"
  ],
  "scripts/plan-rust-required-scope.test.py": [
    "Selection.test_unknown_inputs_block_even_with_full_control_or_extra_groups",
    "Selection.test_unknown_deleted_and_renamed_endpoints_block",
    "Selection.test_wrong_merge_source_and_unrelated_base_fail_closed",
    "Selection.test_merge_resolution_lock_change_still_full",
    "Selection.test_source_lock_change_still_full_on_advanced_target",
    "Selection.test_component_unavailable_feature_keeps_expanded_execution",
    "Selection.test_fast_test_tier_downgrade_requires_full_feature_execution",
    "Selection.test_fast_unmapped_ignore_is_rejected_instead_of_silently_skipped",
    "Selection.test_fast_item_macros_keep_full_coverage_for_every_delimiter",
    "Selection.test_fast_cross_module_same_function_name_cannot_hide_test_tier_move",
    "Selection.test_component_native_host_build_dependencies_keep_generated_portable_consumers",
    "Selection.test_component_native_proc_macro_dependencies_keep_generated_portable_consumers",
    "Selection.test_component_portable_and_alternate_library_mounts_keep_wasm_and_launcher",
    "Selection.test_component_unparsed_explicit_cargo_root_cannot_prove_portable_exclusion",
    "Selection.test_fast_cross_package_path_and_include_cannot_authorize_shared_tests",
    "Selection.test_bounded_inventory_old_policy_cannot_self_enrol_from_candidate",
    "Selection.test_fast_capability_must_come_from_trusted_config",
    "Selection.test_full_profile_expands_inventory_without_diff_and_binds_fixed_identity",
    "EnclosingFunctionCfgOwners.test_added_module_does_not_change_existing_initializer_cfg_policy",
    "EnclosingFunctionCfgOwners.test_initializer_expression_or_gate_changes_keep_expanded_coverage",
    "EnclosingFunctionCfgOwners.test_function_impl_and_module_moves_do_not_cancel_cfg_changes",
    "EnclosingFunctionCfgOwners.test_macro_and_unknown_parent_contexts_keep_conservative_handling"
  ]
}
''')

class TrustedContracts(unittest.TestCase):
    TEST = 'scripts/probe.test.py'
    SOURCE = '''import importlib.util
from pathlib import Path
import sys
import unittest
spec = importlib.util.spec_from_file_location('probe', Path(__file__).with_name('probe.py'))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)
class Parent(unittest.TestCase):
    def test_value(self):
        self.assertEqual(probe.VALUE, 7)
        self.assertIs(sys.modules['__main__'].__dict__['Parent'], Parent)
        self.assertEqual(Path(__file__).name, 'probe.test.py')
class Child(Parent):
    pass
if __name__ == '__main__':
    unittest.main()
'''

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        directory = Path(self.temp.name).resolve()
        self.root, self.snapshot = directory / 'candidate', directory / 'baseline'
        self.write(self.root, 'scripts/probe.py', 'VALUE = 7\n')
        self.write(self.root, self.TEST, self.SOURCE)
        self.write(self.snapshot, self.TEST, self.SOURCE)

    @staticmethod
    def write(root, path, text):
        destination = root / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(text)

    def run_contract(self, **options):
        return controls.run_contract(self.root, self.snapshot, self.TEST, **options)

    def test_repeated_named_scenarios_keep_each_call_and_nested_assertions(self):
        source = '''def scenario(value):
    assert value > 0
def scenario_checked(value):
    scenario(value)
def main():
    for value in (1, 2):
        for other in (3, 4):
            scenario_checked(value + other)
    scenario_checked(5)
'''
        self.write(self.root, self.TEST, source)
        self.write(self.snapshot, self.TEST, source)
        self.assertEqual(len(controls.test_inventory(source)[1]), 6)
        self.assertEqual(self.run_contract(), 6)
        broken = source.replace('scenario(value)\n', 'try:\n        scenario(0)\n    except AssertionError:\n        pass\n')
        self.write(self.snapshot, self.TEST, broken)
        with self.assertRaisesRegex(ValueError, 'did not completely execute'):
            self.run_contract()

    def test_named_scenario_inventory_loop_and_local_action_reject_missing_completion(self):
        for source in ('def main():\n    assert True\n    scenario()\n',
                       'from external import scenario\ndef main():\n    assert True\n    scenario()\n'):
            with self.assertRaisesRegex(ValueError, 'no local asserted implementation'):
                controls.test_inventory(source)
        source = '''def scenario_first():
    assert 1 + 1 == 2
def scenario_second():
    assert 2 + 2 == 4
def main():
    scenarios = (scenario_first, scenario_second)
    for scenario in scenarios:
        scenario()
'''
        self.write(self.root, self.TEST, source)
        self.write(self.snapshot, self.TEST, source)
        self.assertEqual(self.run_contract(), 3)
        path = 'scripts/product-doc-content-check.test.py'
        self.write(self.root, path, 'def main():\n    raise SystemExit(0)\n    assert True\n')
        with self.assertRaisesRegex(ValueError, 'did not completely execute'):
            controls.local_contract(self.root, path)
        with self.assertRaisesRegex(ValueError, 'audited procedural'):
            controls.local_contract(self.root, self.TEST)

    def test_baseline_code_loads_candidate_with_correct_main_and_inherited_inventory(self):
        self.assertEqual(controls.test_inventory(self.SOURCE),
                         ('unittest', ['Child.test_value', 'Parent.test_value']))
        self.write(self.root, self.TEST, 'raise AssertionError("candidate assertions are not baseline")\n')
        self.assertEqual(self.run_contract(), 2)

    def test_removing_candidate_tests_cannot_hide_a_broken_implementation(self):
        self.write(self.root, self.TEST, '# candidate removed every assertion\n')
        self.write(self.root, 'scripts/probe.py', 'VALUE = 0\n')
        with self.assertRaisesRegex(ValueError, 'did not completely execute'):
            self.run_contract()

    def test_import_exit_zero_and_os_exit_zero_have_no_completion_witness(self):
        for source in ('raise SystemExit(0)\n', 'import os\nos._exit(0)\n'):
            with self.subTest(source=source):
                self.write(self.root, 'scripts/probe.py', source)
                with self.assertRaisesRegex(ValueError, 'did not completely execute'):
                    self.run_contract()

    def test_selected_test_failure_skip_and_empty_inventory_fail_closed(self):
        variants = [self.SOURCE.replace('def test_value(self):', '@unittest.skip("removed")\n    def test_value(self):'),
                    self.SOURCE.replace('self.assertEqual(probe.VALUE, 7)', 'self.fail("failure")')]
        for source in variants:
            with self.subTest(source=source):
                self.write(self.snapshot, self.TEST, source)
                with self.assertRaisesRegex(ValueError, 'did not completely execute'):
                    self.run_contract()
        self.write(self.snapshot, self.TEST, 'import unittest\n')
        with self.assertRaisesRegex(ValueError, 'no statically enumerable'):
            self.run_contract()

    def test_candidate_cannot_retire_one_case_while_other_cases_still_exist(self):
        self.write(self.root, self.TEST, self.SOURCE.replace('class Child(Parent):\n    pass\n', ''))
        with self.assertRaisesRegex(ValueError, 'retired baseline'):
            self.run_contract(retain_candidate=True)
        self.write(self.root, self.TEST, self.SOURCE + '\nclass Added(Parent):\n    pass\n')
        self.assertEqual(self.run_contract(retain_candidate=True), 2)

    SUPPORT_TEST = 'scripts/ci-support-profiles.test.py'
    SUPPORT_SOURCE = '''from pathlib import Path
import subprocess
import sys
import unittest
ROOT = Path(__file__).resolve().parent.parent
DRIVER = (ROOT / 'scripts/ci-tests.sh').read_text()
FUNCTION = DRIVER.split('run_selected_workspace_support() {', 1)[1].split('\\n}', 1)[0]
class Profiles(unittest.TestCase):
    def test_recorded_candidate_command(self):
        script = 'set -euo pipefail\\nci_python="$1"; driver_dir="$2"\\n'
        script += 'run_cargo() { echo "cargo $*"; }\\n'
        script += 'run_selected_workspace_support() {' + FUNCTION + '\\n}\\n'
        script += 'run_selected_workspace_support\\n'
        result = subprocess.run(['bash', '-c', script, 'test', sys.executable,
                                 str(ROOT / 'scripts'), '["fixture"]'],
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, 'cargo test -p fixture --lib\\n')
'''
    SUPPORT_DRIVER = '''#!/usr/bin/env bash
# ci-driver-library: functions-v1
run_new_support_helper() {
  run_transitive_support_helper
}
run_transitive_support_helper() {
  bash -c '[[ -z "${BASH_ENV+x}" ]] && ! declare -F run_new_support_helper >/dev/null'
  run_cargo test -p fixture --lib
}
run_selected_workspace_support() {
  run_new_support_helper
}
if [[ "${BASH_SOURCE[0]}" != "$0" ]]; then return 0; fi
echo 'CLI must not execute while loading functions' >&2
exit 91
'''

    def support_fixture(self, driver, source=None):
        self.write(self.root, 'scripts/ci-tests.sh', driver)
        self.write(self.snapshot, self.SUPPORT_TEST, source or self.SUPPORT_SOURCE)
        # Candidate assertions are deliberately unusable: the old BASE source
        # must still own the expected command and its failure assertions.
        self.write(self.root, self.SUPPORT_TEST, 'raise SystemExit(0)\n')
        return controls.run_contract(self.root, self.snapshot, self.SUPPORT_TEST)

    def test_support_shell_uses_candidate_transitive_helpers_and_keeps_baseline_assertions(self):
        self.assertEqual(self.support_fixture(self.SUPPORT_DRIVER), 1)
        variants = {
            'missing helper': self.SUPPORT_DRIVER.replace('run_transitive_support_helper() {',
                                                         'removed_helper() {'),
            'wrong candidate command': self.SUPPORT_DRIVER.replace('-p fixture', '-p wrong'),
            'real command failure': self.SUPPORT_DRIVER.replace('run_cargo test', 'bash -c "exit 23"\n  run_cargo test'),
            'early success exit': self.SUPPORT_DRIVER.replace('run_new_support_helper\n}', 'exit 0\n}'),
            'source failure': self.SUPPORT_DRIVER.replace('then return 0;', 'then return 24;'),
        }
        for label, driver in variants.items():
            with self.subTest(label=label), self.assertRaisesRegex(ValueError, 'did not completely execute'):
                self.support_fixture(driver)

    def test_support_shell_injection_is_limited_to_its_independent_fixture(self):
        source = self.SUPPORT_SOURCE.replace('import subprocess', 'import os\nimport subprocess') + '''
    def test_unrelated_bash_does_not_import_candidate_functions(self):
        result = subprocess.run(['bash', '-c',
                                 '[[ -z "${BASH_ENV+x}" ]] && ! declare -F run_new_support_helper >/dev/null'],
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
    def test_explicit_environment_keeps_values_and_unrelated_startup(self):
        startup = ROOT / 'custom-bash-env.sh'
        startup.write_text('export EXISTING_STARTUP=preserved\\n')
        environment = dict(os.environ, BASH_ENV=str(startup), EXPLICIT_VALUE='kept')
        for caller, scripts, extra in [('unrelated', str(ROOT / 'scripts'), ['[]']),
                                       ('test', str(ROOT / 'other-scripts'), ['[]']),
                                       ('test', str(ROOT / 'scripts'), []),
                                       ('full-support', str(ROOT / 'scripts'), ['[]'])]:
            result = subprocess.run(['bash', '-c',
                '[[ "$EXPLICIT_VALUE" == kept && "$EXISTING_STARTUP" == preserved ]]',
                caller, sys.executable, scripts, *extra], env=environment)
            self.assertEqual(result.returncode, 0)
        for caller, extra in [('test', ['[]']), ('full-support', [])]:
            result = subprocess.run(['bash', '-c',
                '[[ "$EXPLICIT_VALUE" == kept && -z "${BASH_ENV+x}" ]] && declare -F run_new_support_helper >/dev/null',
                caller, sys.executable, str(ROOT / 'scripts'), *extra], env=environment)
            self.assertEqual(result.returncode, 0)
        self.assertEqual(environment['BASH_ENV'], str(startup))
'''
        self.assertEqual(self.support_fixture(self.SUPPORT_DRIVER, source), 3)
        self.write(self.snapshot, self.TEST, self.SUPPORT_SOURCE)
        with self.assertRaisesRegex(ValueError, 'did not completely execute'):
            self.run_contract()

    def test_legacy_support_driver_keeps_the_original_fixture_path(self):
        legacy = self.SUPPORT_DRIVER.replace('# ci-driver-library: functions-v1\n', '')
        legacy = legacy.replace('  run_new_support_helper\n}', '  run_cargo test -p fixture --lib\n}')
        self.assertEqual(self.support_fixture(legacy), 1)

    def test_procedural_main_executes_real_assertions_and_rejects_early_exit(self):
        source = '''from pathlib import Path
import importlib.util
spec = importlib.util.spec_from_file_location('probe', Path(__file__).with_name('probe.py'))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)
def main():
    assert probe.VALUE == 7
if __name__ == '__main__':
    main()
'''
        self.write(self.snapshot, self.TEST, source)
        self.assertEqual(self.run_contract(), 1)
        self.write(self.root, 'scripts/probe.py', 'raise SystemExit(0)\n')
        with self.assertRaisesRegex(ValueError, 'did not completely execute'):
            self.run_contract()
        self.write(self.root, 'scripts/probe.py', 'VALUE=7\n')
        self.write(self.snapshot, self.TEST, source.replace('    assert probe.VALUE == 7', '    if False:\n        assert probe.VALUE == 7'))
        with self.assertRaisesRegex(ValueError, 'did not completely execute'):
            self.run_contract()

    def test_procedural_main_cannot_drop_one_scenario_or_its_assertions(self):
        source = '''def test_first():
    assert 1 + 1 == 2
def test_second():
    assert 2 + 2 == 4
def main():
    for test in [test_first, test_second]:
        test()
'''
        self.write(self.snapshot, self.TEST, source)
        self.write(self.root, self.TEST, source)
        self.assertEqual(self.run_contract(retain_candidate=True), 3)
        self.write(self.root, self.TEST, source.replace('[test_first, test_second]', '[test_first]'))
        with self.assertRaisesRegex(ValueError, 'retired baseline'):
            self.run_contract(retain_candidate=True)
        self.write(self.snapshot, self.TEST, source.replace('assert 2 + 2 == 4', 'pass'))
        with self.assertRaisesRegex(ValueError, 'did not completely execute'):
            self.run_contract()

    def test_inline_procedural_scenarios_have_stable_required_identities(self):
        source = '''def scenario(value):
    assert value > 0
def main():
    scenario(1)
    scenario(2)
'''
        self.write(self.snapshot, self.TEST, source)
        self.write(self.root, self.TEST, source)
        self.assertEqual(self.run_contract(retain_candidate=True), 3)
        before = controls.test_inventory(source)
        self.assertEqual(before, controls.test_inventory('# unrelated new lines\n\n' + source))
        self.write(self.root, self.TEST, source.replace('    scenario(2)\n', ''))
        with self.assertRaisesRegex(ValueError, 'retired baseline'):
            self.run_contract(retain_candidate=True)
        loop = source.replace('    scenario(1)\n    scenario(2)',
                              '    for value in (1, 2, 3):\n        scenario(value)')
        self.write(self.snapshot, self.TEST, loop)
        self.write(self.root, self.TEST, loop)
        self.assertEqual(self.run_contract(retain_candidate=True), 4)
        self.write(self.root, self.TEST, loop.replace('(1, 2, 3)', '(1, 3)'))
        with self.assertRaisesRegex(ValueError, 'retired baseline'):
            self.run_contract(retain_candidate=True)
        repeated = '''def scenario(value):
    assert value > 0
def main():
    count = 0
    while count < 2:
        count += 1
        for value in (1, 2):
            try:
                scenario(value)
            except ValueError:
                pass
'''
        self.write(self.snapshot, self.TEST, repeated)
        with self.assertRaisesRegex(ValueError, 'did not completely execute'):
            self.run_contract()

    def test_procedural_scenario_failure_caught_by_main_is_still_failure(self):
        source = '''def test_case():
    assert False
def main():
    try:
        test_case()
    except AssertionError:
        pass
'''
        escaped = source.replace('    assert False', '    try:\n        assert False\n    finally:\n        marker = 1')
        for failed in (source, escaped):
            with self.subTest(source=failed):
                self.write(self.snapshot, self.TEST, failed)
                with self.assertRaisesRegex(ValueError, 'did not completely execute'):
                    self.run_contract()
        handled = '''def test_case():
    try:
        raise ValueError('expected')
    except ValueError as error:
        assert str(error) == 'expected'
def main():
    test_case()
'''
        self.write(self.snapshot, self.TEST, handled)
        self.assertEqual(self.run_contract(), 2)

    def test_baseline_test_helpers_cannot_be_replaced_by_candidate_noops(self):
        source = '''from pathlib import Path
import importlib.util
def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module
helper = load('helper.test')
probe = load('probe')
def main():
    helper.verify(probe.VALUE)
    assert True
'''
        self.write(self.snapshot, self.TEST, source)
        self.write(self.snapshot, 'scripts/helper.test.py', 'def verify(value):\n    assert value == 7\n')
        self.write(self.root, 'scripts/helper.test.py', 'def verify(value):\n    pass\n')
        self.write(self.root, 'scripts/probe.py', 'VALUE=0\n')
        with self.assertRaisesRegex(ValueError, 'did not completely execute'):
            self.run_contract()

    def test_fixed_safety_inventory_exists_in_current_repository(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        registered = {test for entry in config['python_control_contracts'].values() for test in entry['tests']}
        self.assertLessEqual(set(controls.FIXED_TESTS), registered)
        for path, required in REQUIRED_FIXED_TESTS.items():
            self.assertIn(path, controls.FIXED_TESTS)
            actual = controls.FIXED_TESTS[path]
            if actual is None:
                actual = controls.test_inventory((HERE.parent / path).read_text())[1]
            self.assertLessEqual(set(required), set(actual), path)
        for path, identifiers in controls.FIXED_TESTS.items():
            with self.subTest(path=path):
                kind, inventory = controls.test_inventory((HERE.parent / path).read_text())
                self.assertEqual(kind, 'unittest')
                self.assertTrue(inventory)
                self.assertLessEqual(set(identifiers or inventory), set(inventory))


class SelectionAndMigration(unittest.TestCase):
    def setUp(self):
        self.groups = ['baseline', 'workflow_governance', 'cargo_tooling_contracts']
        self.config = dict(groups=self.groups, python_control_contracts={
            'scripts/control.py': dict(group='workflow_governance', tests=['scripts/control.test.py']),
            'scripts/other.py': dict(group='cargo_tooling_contracts', tests=['scripts/other.test.py']),
        })
        self.plan = dict(scope='targeted', groups=['baseline', 'workflow_governance'],
                         python_control_contract_version=1, python_control_paths=['scripts/control.py'],
                         python_control_tests={'workflow_governance': ['scripts/control.test.py']})

    def test_targeted_and_ordinary_full_use_actual_paths_explicit_full_uses_all(self):
        for scope in ('targeted', 'full'):
            self.plan['scope'] = scope
            self.assertEqual(controls.contract_selection(self.config, self.plan), ['scripts/control.test.py'])
        self.plan.update(scope='full', python_control_paths=[], python_control_tests={})
        self.assertEqual(controls.contract_selection(self.config, self.plan), [])
        self.plan.update(scope='explicit-full', groups=self.groups,
                         python_control_paths=list(self.config['python_control_contracts']),
                         python_control_tests={'workflow_governance': ['scripts/control.test.py'],
                                               'cargo_tooling_contracts': ['scripts/other.test.py']})
        self.assertEqual(controls.contract_selection(self.config, self.plan),
                         ['scripts/control.test.py', 'scripts/other.test.py'])

    def test_registered_test_path_alone_selects_the_complete_baseline_contract(self):
        self.config['python_control_contracts']['scripts/control.py']['tests'].append('scripts/neighbor.test.py')
        self.plan.update(python_control_paths=['scripts/control.test.py'],
                         python_control_tests={'workflow_governance': ['scripts/control.test.py', 'scripts/neighbor.test.py']})
        self.assertEqual(controls.contract_selection(self.config, self.plan),
                         ['scripts/control.test.py', 'scripts/neighbor.test.py'])

    def test_candidate_inventory_missing_group_unknown_path_and_bad_version_are_rejected(self):
        mutations = [('python_control_contract_version', True), ('python_control_contract_version', 2),
                     ('python_control_tests', {}), ('python_control_paths', ['scripts/not_registered.py']),
                     ('python_control_paths', ['scripts/control.py', 'scripts/control.py']),
                     ('groups', ['baseline'])]
        for field, value in mutations:
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                controls.contract_selection(self.config, self.plan | {field: value})

    def test_full_migration_is_bound_to_actual_policy_git_changes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            def command(*args):
                return subprocess.check_output(args, cwd=root, text=True).strip()
            def commit(path, value):
                TrustedContracts.write(root, path, value)
                command('git', 'add', '.')
                command('git', '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', 'fixture')
                return command('git', 'rev-parse', 'HEAD')
            command('git', 'init', '-q')
            base = commit('scripts/ci-required-scope.json', '{}')
            head = commit('Cargo.lock', 'mixed lock change')
            plan = self.plan | dict(scope='full', groups=self.groups, source_sha=head, tested_sha=head,
                                    python_control_policy_migration=True)
            self.assertEqual(controls.migration_paths(root, base, plan, self.config), [])
            changed = commit('scripts/ci-required-scope.json', '{"revision":2}')
            plan.update(source_sha=changed, tested_sha=changed)
            self.assertEqual(controls.migration_paths(root, base, plan, self.config), ['scripts/ci-required-scope.json'])
            self.assertEqual(controls.migration_paths(root, base, plan | {'scope': 'targeted'}, self.config), [])
            with self.assertRaisesRegex(ValueError, 'omitted a required group'):
                controls.migration_paths(root, base, plan | {'groups': ['baseline']}, self.config)
            old = dict(self.config, groups=[*self.groups, 'formal_process'])
            current = dict(old, formal_full_profiles_only_version=1)
            for profile in ('pr-fast', 'main-fast'):
                fast = plan | dict(execution_profile=profile, formal_full_profiles_only_version=1)
                self.assertEqual(controls.migration_paths(root, base, fast, current),
                                 ['scripts/ci-required-scope.json'])
                # A candidate marker cannot narrow the old BASE inventory.
                with self.assertRaisesRegex(ValueError, 'omitted a required group'):
                    controls.migration_paths(root, base, fast, old)
                for field, value in [('groups', ['baseline']), ('groups', old['groups']),
                                     ('scope', 'explicit-full'), ('execution_profile', 'periodic-full'),
                                     ('formal_full_profiles_only_version', True),
                                     ('formal_full_profiles_only_version', None)]:
                    with self.subTest(profile=profile, field=field, value=value), self.assertRaises(ValueError):
                        controls.migration_paths(root, base, fast | {field: value}, current)
            for profile in ('periodic-full', 'candidate-full'):
                full = plan | dict(scope='explicit-full', groups=old['groups'], execution_profile=profile,
                                   formal_full_profiles_only_version=1)
                self.assertEqual(controls.migration_paths(root, base, full, current),
                                 ['scripts/ci-required-scope.json'])
                with self.assertRaisesRegex(ValueError, 'omitted a required group'):
                    controls.migration_paths(root, base, full | {'groups': self.groups}, current)

    def test_target_only_policy_advance_does_not_authorize_source_migration(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            def command(*args):
                return subprocess.check_output(args, cwd=root, text=True, stderr=subprocess.DEVNULL).strip()
            def commit(path, value):
                TrustedContracts.write(root, path, value)
                command('git', 'add', '.')
                command('git', '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', 'fixture')
                return command('git', 'rev-parse', 'HEAD')
            command('git', 'init', '-q')
            ancestor = commit('scripts/ci-required-scope.json', '{}')
            command('git', 'switch', '-c', 'source')
            source = commit('Cargo.lock', 'candidate keeps ordinary full')
            command('git', 'switch', '-c', 'policy', ancestor)
            policy = commit('scripts/ci-required-scope.json', '{"target_only":true}')
            command('git', '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
                    'merge', '--no-ff', '-m', 'tested merge', 'source')
            tested = command('git', 'rev-parse', 'HEAD')
            plan = self.plan | dict(scope='full', groups=self.groups, source_sha=source, tested_sha=tested)
            self.assertEqual(controls.migration_paths(root, policy, plan, self.config), [])

    def test_full_dispatch_executes_fixed_and_selected_baseline_contracts(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            def command(*args):
                return subprocess.check_output(args, cwd=root, text=True, stderr=subprocess.DEVNULL).strip()
            test = 'scripts/control.test.py'
            other = 'scripts/other.test.py'
            source = 'import unittest\nclass Check(unittest.TestCase):\n    def test_required(self): self.assertTrue(True)\n'
            config = copy.deepcopy(self.config)
            config['python_control_contracts']['scripts/control.py']['tests'].append(other)
            for path in (test, other):
                TrustedContracts.write(root, path, source)
            TrustedContracts.write(root, 'scripts/ci-required-scope.json', json.dumps(config))
            command('git', 'init', '-q')
            command('git', 'add', '.')
            command('git', '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', 'fixture')
            revision = command('git', 'rev-parse', 'HEAD')
            plan = self.plan | dict(scope='full', groups=self.groups, policy_sha=revision,
                                    source_sha=revision, tested_sha=revision,
                                    python_control_tests={'workflow_governance': [test, other]})
            for scope in ('full', 'explicit-full'):
                selected = copy.deepcopy(plan)
                selected['scope'] = scope
                if scope == 'explicit-full':
                    selected['python_control_paths'] = list(config['python_control_contracts'])
                    selected['python_control_tests']['cargo_tooling_contracts'] = [other]
                with self.subTest(scope=scope), patch.object(controls, 'FIXED_TESTS', {test: ['Check.test_required']}), \
                     patch.object(controls, 'run_contract', return_value=1) as execute, \
                     patch.object(controls, 'run_rust_fixture') as rust, \
                     patch.dict(controls.os.environ, SOURCE_SHA=revision):
                    controls.run_contracts(root, revision, selected)
                    actual = [(call.args[2], call.kwargs.get('origin', 'trusted-baseline'))
                              for call in execute.call_args_list]
                    self.assertIn((test, 'trusted-baseline'), actual)
                    self.assertIn((test, 'candidate'), actual)
                    self.assertIn((other, 'trusted-baseline'), actual)
                    self.assertIn((other, 'candidate'), actual)
                    rust.assert_called_once()


class RealRustControlFixture(unittest.TestCase):
    def test_actual_planner_executor_gate_and_cfg_feature_owners(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        controls.run_rust_fixture(HERE.parent, config)


class GovernanceOwnership(unittest.TestCase):
    """Execute real worker processes: a complete suite must subsume its subset."""
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.control = 'scripts/control.test.py'
        self.fixed = 'scripts/fixed.test.py'
        self.other = 'scripts/other.test.py'
        self.governed = 'scripts/governed.test.py'
        self.config = dict(python_control_contract_version=1,
                           groups={'workflow_governance': {}},
                           workflow_governance_python_tests=[self.control, self.governed],
                           python_control_contracts={
                               'scripts/control.py': dict(group='workflow_governance',
                                                         tests=[self.control, self.other]),
                               'scripts/fixed.py': dict(group='workflow_governance', tests=[self.fixed])})
        for path, names in ((self.control, ['required', 'preserved']),
                            (self.fixed, ['required']), (self.other, ['required']),
                            (self.governed, ['required'])):
            self.write_test(path, names, 'baseline')
        self.write('scripts/ci-required-scope.json', json.dumps(self.config))
        self.write('scripts/ci-tests.sh',
                   'run_workflow_governance_remaining_contract_tests() {\n'
                   '  printf "remaining\\n" >> "$repo_root/trace.log"\n}\n')
        self.git('init', '-q')
        self.policy = self.commit()
        for path, names in ((self.control, ['required', 'preserved', 'new']),
                            (self.fixed, ['required', 'new']), (self.other, ['required']),
                            (self.governed, ['required'])):
            self.write_test(path, names, 'candidate')
        candidate = self.commit()
        self.plan = dict(scope='targeted', groups=['workflow_governance'],
                         python_control_contract_version=1,
                         python_control_paths=['scripts/control.py'],
                         python_control_tests={'workflow_governance': [self.control, self.other]},
                         source_sha=candidate, tested_sha=candidate, policy_sha=self.policy)

    def write(self, path, text):
        TrustedContracts.write(self.root, path, text)

    def git(self, *args):
        return subprocess.check_output(['git', *args], cwd=self.root, text=True,
                                       stderr=subprocess.DEVNULL).strip()

    def commit(self):
        self.git('add', '.')
        self.git('-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
                 'commit', '-qm', 'governance fixture')
        return self.git('rev-parse', 'HEAD')

    def write_test(self, path, names, origin):
        source = 'import unittest\nfrom pathlib import Path\nclass Check(unittest.TestCase):\n'
        for name in names:
            source += (f'    def test_{name}(self):\n'
                       '        with (Path(__file__).resolve().parent.parent / "trace.log").open("a") as log:\n'
                       f'            log.write("{origin}:{path}:{name}\\n")\n'
                       '        self.assertTrue(True)\n')
        self.write(path, source)

    def execute(self):
        with patch.object(controls, 'FIXED_TESTS', {
                self.control: ['Check.test_required'], self.fixed: ['Check.test_required']}), \
             patch.object(controls, 'run_rust_fixture') as rust, \
             patch.dict(controls.os.environ, SOURCE_SHA=self.plan['source_sha']):
            result = controls.run_contracts(self.root, self.policy, self.plan, governance=True)
            rust.assert_called_once()
            return result

    def test_complete_suites_execute_every_origin_and_case_once(self):
        self.assertEqual(self.execute(), 11)
        records = (self.root / 'trace.log').read_text().splitlines()
        expected = []
        for origin, path, names in (
                ('baseline', self.control, ['required', 'preserved']),
                ('baseline', self.fixed, ['required']), ('baseline', self.other, ['required']),
                ('candidate', self.control, ['required', 'preserved', 'new']),
                ('candidate', self.fixed, ['required', 'new']),
                ('candidate', self.other, ['required']), ('candidate', self.governed, ['required'])):
            expected.extend(f'{origin}:{path}:{name}' for name in names)
        self.assertCountEqual(records, [*expected, 'remaining'])
        self.assertEqual(records[-1], 'remaining')

    def test_candidate_cannot_retire_governance_files_or_selected_baseline_cases(self):
        with self.subTest(kind='file'):
            self.write('scripts/ci-required-scope.json', json.dumps(dict(
                self.config, workflow_governance_python_tests=[self.control])))
            with self.assertRaisesRegex(ValueError, 'retired complete governance'):
                self.execute()
        self.write('scripts/ci-required-scope.json', json.dumps(self.config))
        self.write_test(self.control, ['required', 'new'], 'candidate')
        with self.subTest(kind='case'), self.assertRaisesRegex(ValueError, 'retired baseline'):
            self.execute()

    def test_new_candidate_skip_fails_before_remaining(self):
        path = self.root / self.control
        path.write_text(path.read_text().replace('    def test_new(self):',
                        '    @unittest.skip("candidate omitted new regression")\n    def test_new(self):'))
        with self.assertRaisesRegex(ValueError, 'did not completely execute'):
            self.execute()
        self.assertNotIn('remaining', (self.root / 'trace.log').read_text().splitlines())

    def test_remaining_stops_on_first_failure(self):
        self.write('scripts/ci-tests.sh',
                   'run_workflow_governance_remaining_contract_tests() {\n'
                   '  false\n  printf "incorrect\\n" >> "$repo_root/trace.log"\n}\n')
        with self.assertRaises(subprocess.CalledProcessError):
            controls.run_governance_remaining(self.root)
        self.assertFalse((self.root / 'trace.log').exists())

    def test_inventory_rejects_ambiguous_or_escaping_paths(self):
        for paths in ([], [self.control, self.control], ['../escape.test.py'],
                      ['scripts/../escape.test.py'], ['/scripts/root.test.py'],
                      ['scripts/*.test.py'], ['scripts/a\\b.test.py'], ['scripts//a.test.py']):
            with self.subTest(paths=paths), self.assertRaises(ValueError):
                controls.governance_tests(dict(workflow_governance_python_tests=paths))


if __name__ == '__main__':
    unittest.main()
