#!/usr/bin/env python3
"""Run immutable baseline CI assertions against the tested Python implementation.

This runner is copied from policy_sha by the selector job. It never uses a
candidate test file as the authority for a narrowed CI-control change.
"""
import argparse
import ast
import copy
import hashlib
import importlib.machinery
import json
import linecache
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import types
import unittest


VERSION = 1
PLANNER_TEST = 'scripts/plan-rust-required-scope.test.py'
# These are safety properties, not an exhaustive snapshot of path-selection
# policy. A full change can evolve that policy without satisfying obsolete
# "every Python edit is full" assertions from an earlier baseline.
FIXED_TESTS = {
    PLANNER_TEST: [
        'Selection.test_unknown_inputs_block_even_with_full_control_or_extra_groups',
        'Selection.test_unknown_deleted_and_renamed_endpoints_block',
        'Selection.test_wrong_merge_source_and_unrelated_base_fail_closed',
        'Selection.test_merge_resolution_lock_change_still_full',
        'Selection.test_source_lock_change_still_full_on_advanced_target',
        'Selection.test_component_unavailable_feature_keeps_expanded_execution',
        'Selection.test_fast_test_tier_downgrade_requires_full_feature_execution',
        'Selection.test_fast_unmapped_ignore_is_rejected_instead_of_silently_skipped',
        'Selection.test_fast_item_macros_keep_full_coverage_for_every_delimiter',
        'Selection.test_fast_cross_module_same_function_name_cannot_hide_test_tier_move',
        'Selection.test_component_native_host_build_dependencies_keep_generated_portable_consumers',
        'Selection.test_component_native_proc_macro_dependencies_keep_generated_portable_consumers',
        'Selection.test_component_portable_and_alternate_library_mounts_keep_wasm_and_launcher',
        'Selection.test_component_unparsed_explicit_cargo_root_cannot_prove_portable_exclusion',
        'Selection.test_fast_cross_package_path_and_include_cannot_authorize_shared_tests',
        'Selection.test_bounded_inventory_old_policy_cannot_self_enrol_from_candidate',
        'Selection.test_fast_capability_must_come_from_trusted_config',
        'Selection.test_full_profile_expands_inventory_without_diff_and_binds_fixed_identity',
        'EnclosingFunctionCfgOwners.test_added_module_does_not_change_existing_initializer_cfg_policy',
        'EnclosingFunctionCfgOwners.test_initializer_expression_or_gate_changes_keep_expanded_coverage',
        'EnclosingFunctionCfgOwners.test_function_impl_and_module_moves_do_not_cancel_cfg_changes',
        'EnclosingFunctionCfgOwners.test_macro_and_unknown_parent_contexts_keep_conservative_handling',
    ],
    'scripts/ci-formal-process.test.py': [
        'Transition.test_selected_requires_real_success_and_not_selected_requires_skip',
        'Transition.test_formal_builder_failure_and_skip_combinations',
        'FastExecutionSelection.test_old_planner_and_uncertain_scope_cannot_narrow_old_inventory',
        'FastExecutionSelection.test_changed_test_policy_includes_expanded_features_before_merge',
        'FastExecutionSelection.test_source_and_tested_merge_are_bound_separately',
        'FastExecutionSelection.test_subset_artifact_rejects_another_consumer_plan_before_files',
        'FastExecutionSelection.test_selected_package_or_behavior_cannot_disappear_from_artifact',
        'FastExecutionSelection.test_selected_fast_bundle_runs_behavior_and_rejects_zero_execution',
        'FastTargetInventory.test_minimum_cases_target_and_actual_features_are_verified',
        'FastTargetInventory.test_full_supplements_cannot_replace_original_required_inventory',
        'InlineExecutionSelection.test_known_inline_capability_rejects_incomplete_or_expanded_plans',
        'InlineExecutionSelection.test_clippy_owner_requires_the_complete_known_inline_plan',
        'DriverCoverage.test_inline_driver_builds_once_runs_manifest_and_propagates_each_failure',
    ],
    'scripts/ci-required-result.test.py': None,
    'scripts/ci-python-control-contracts.test.py': [
        'TrustedContracts.test_baseline_code_loads_candidate_with_correct_main_and_inherited_inventory',
        'TrustedContracts.test_removing_candidate_tests_cannot_hide_a_broken_implementation',
        'TrustedContracts.test_import_exit_zero_and_os_exit_zero_have_no_completion_witness',
        'TrustedContracts.test_selected_test_failure_skip_and_empty_inventory_fail_closed',
        'TrustedContracts.test_candidate_cannot_retire_one_case_while_other_cases_still_exist',
        'TrustedContracts.test_procedural_main_cannot_drop_one_scenario_or_its_assertions',
        'TrustedContracts.test_procedural_scenario_failure_caught_by_main_is_still_failure',
        'TrustedContracts.test_inline_procedural_scenarios_have_stable_required_identities',
        'TrustedContracts.test_baseline_test_helpers_cannot_be_replaced_by_candidate_noops',
        'TrustedContracts.test_fixed_safety_inventory_exists_in_current_repository',
        'SelectionAndMigration.test_registered_test_path_alone_selects_the_complete_baseline_contract',
        'SelectionAndMigration.test_target_only_policy_advance_does_not_authorize_source_migration',
        'SelectionAndMigration.test_full_dispatch_executes_fixed_and_selected_baseline_contracts',
    ],
}


def checked(command, *, root, env=None):
    result = subprocess.run(command, cwd=root, env=env, text=True, capture_output=True)
    if result.returncode:
        raise ValueError(f'command failed ({result.returncode}): {command!r}\n'
                         + result.stdout + result.stderr)
    return result.stdout


def git_source(root, revision, path):
    if not re.fullmatch(r'[0-9a-f]{40}', revision):
        raise ValueError('policy must be a fixed commit identity')
    if (not isinstance(path, str) or not path.startswith('scripts/')
            or '..' in path.split('/') or '\\' in path or Path(path).is_absolute()):
        raise ValueError('invalid trusted Python contract path')
    return checked(['git', 'show', f'{revision}:{path}'], root=root)


def contract_selection(config, plan):
    if type(plan.get('python_control_contract_version')) is not int or plan['python_control_contract_version'] != VERSION:
        raise ValueError('unsupported Python control contract plan')
    paths, contracts = plan.get('python_control_paths'), config.get('python_control_contracts')
    if (not isinstance(paths, list) or any(not isinstance(path, str) for path in paths)
            or len(paths) != len(set(paths)) or not isinstance(contracts, dict)):
        raise ValueError('untrusted Python control path inventory')
    eligible = set(contracts)
    for spec in contracts.values():
        eligible.update(spec.get('tests', []))
    if set(paths) - eligible:
        raise ValueError('untrusted Python control path inventory')
    selected = (sorted(contracts) if plan.get('scope') == 'explicit-full' else
                [path for path, spec in contracts.items()
                 if path in paths or set(paths).intersection(spec['tests'])])
    expected = {}
    for path in selected:
        spec = contracts[path]
        if (not isinstance(spec, dict) or spec.get('group') not in plan.get('groups', [])
                or not isinstance(spec.get('tests'), list) or not spec['tests']
                or any(not isinstance(test, str) or not test.startswith('scripts/')
                       or not test.endswith('.test.py') or '..' in test.split('/')
                       or '\\' in test for test in spec['tests'])):
            raise ValueError('invalid baseline Python contract registration')
        expected.setdefault(spec['group'], set()).update(spec['tests'])
    expected = {group: sorted(tests) for group, tests in sorted(expected.items())}
    if plan.get('python_control_tests') != expected:
        raise ValueError('Python contract inventory differs from trusted baseline')
    if expected and 'workflow_governance' not in plan.get('groups', []):
        raise ValueError('Python contracts have no selected trusted executor')
    return sorted({test for tests in expected.values() for test in tests})


def procedural_cases(tree):
    functions = {node.name: node for node in tree.body if isinstance(node, ast.FunctionDef)}
    main = functions.get('main')
    if main is None:
        return [], {}, {}
    named = sorted({node.id for node in ast.walk(main) if isinstance(node, ast.Name)
                    and node.id in functions and node.id.startswith(('test_', 'scenario_'))})
    inline, occurrences = {}, {}

    def is_scenario(node):
        return isinstance(node, ast.Call) and isinstance(node.func, ast.Name) and node.func.id == 'scenario'

    def visit(node, context=()):
        if isinstance(node, ast.For) and any(is_scenario(child) for child in ast.walk(node)):
            if not isinstance(node.iter, (ast.Tuple, ast.List)) or not isinstance(node.target, ast.Name):
                raise ValueError('procedural scenario loop requires an explicit finite inventory')
            for value in node.iter.elts:
                identity = node.target.id + '=' + ast.dump(value, include_attributes=False)
                for child in node.body:
                    visit(child, (*context, identity))
            for child in node.orelse:
                visit(child, context)
            return
        if is_scenario(node):
            if 'scenario' not in functions:
                raise ValueError('procedural scenario has no local asserted implementation')
            identity = ast.dump(node, include_attributes=False) + json.dumps(context)
            digest = hashlib.sha256(identity.encode()).hexdigest()
            index = occurrences.get(digest, 0)
            occurrences[digest] = index + 1
            inline[f'scenario:{digest}:{index}'] = (node.lineno, node.end_lineno)
        for child in ast.iter_child_nodes(node):
            visit(child, context)

    visit(main)
    return ['main', *named, *sorted(inline)], {name: name for name in named}, inline


def test_inventory(source):
    """Enumerate immutable unittest methods, including local inheritance."""
    tree = ast.parse(source)
    classes = {node.name: node for node in tree.body if isinstance(node, ast.ClassDef)}
    resolved, visiting = {}, set()

    def methods(name):
        if name in resolved:
            return resolved[name]
        if name in visiting:
            raise ValueError('cyclic test class inventory')
        visiting.add(name)
        node = classes[name]
        inherited, case = set(), False
        for base in node.bases:
            base_name = ast.unparse(base)
            if base_name in ('unittest.TestCase', 'TestCase'):
                case = True
            elif base_name in classes:
                parent_case, parent_methods = methods(base_name)
                case |= parent_case
                inherited.update(parent_methods)
        inherited.update(child.name for child in node.body
                         if isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef))
                         and child.name.startswith('test'))
        visiting.remove(name)
        resolved[name] = case, inherited
        return resolved[name]

    identifiers = sorted(f'{name}.{method}' for name in classes
                         for is_case, values in [methods(name)] if is_case for method in values)
    if identifiers:
        return 'unittest', identifiers
    main = next((node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'main'), None)
    assertions = [node for node in ast.walk(tree) if isinstance(node, ast.Assert)]
    if main is None or not assertions:
        raise ValueError('contract has no statically enumerable tests or asserted main')
    return 'main', procedural_cases(tree)[0]


def without_main_guard(source, filename):
    tree = ast.parse(source, filename)
    tree.body = [node for node in tree.body if not (
        isinstance(node, ast.If) and isinstance(node.test, ast.Compare)
        and isinstance(node.test.left, ast.Name) and node.test.left.id == '__name__'
        and len(node.test.ops) == 1 and isinstance(node.test.ops[0], ast.Eq)
        and len(node.test.comparators) == 1 and isinstance(node.test.comparators[0], ast.Constant)
        and node.test.comparators[0].value == '__main__')]
    return compile(tree, filename, 'exec')


def run_worker(request_path, witness_path):
    """Only the parent can accept completion; exit(0) is not completion."""
    request = json.loads(request_path.read_text())
    root, snapshot = Path(request['root']), Path(request['snapshot'])
    path = request['path']
    filename = str(root / path)
    source = (snapshot / path).read_text()
    original_get_code = importlib.machinery.SourceFileLoader.get_code

    def trusted_test_code(loader, fullname):
        try:
            relative = Path(loader.path).resolve().relative_to(root)
        except ValueError:
            return original_get_code(loader, fullname)
        saved = snapshot / relative
        if str(relative).endswith('.test.py') and saved.is_file():
            text = saved.read_text()
            logical = str(root / relative)
            linecache.cache[logical] = (len(text), None, text.splitlines(keepends=True), logical)
            return compile(text, logical, 'exec')
        return original_get_code(loader, fullname)

    # Test helpers imported with spec_from_file_location must also use baseline
    # assertions. Implementations and data continue to come from the candidate.
    importlib.machinery.SourceFileLoader.get_code = trusted_test_code
    sys.path.insert(0, str(root / 'scripts'))
    sys.dont_write_bytecode = True
    module = types.ModuleType('__main__')
    module.__file__ = filename
    sys.modules['__main__'] = module
    linecache.cache[filename] = (len(source), None, source.splitlines(keepends=True), filename)
    exec(without_main_guard(source, filename), module.__dict__)
    completed, successes = [], []
    if request['kind'] == 'unittest':
        class Result(unittest.TextTestResult):
            def stopTest(self, test):
                completed.append(test.id().removeprefix('__main__.'))
                super().stopTest(test)

            def addSuccess(self, test):
                successes.append(test.id().removeprefix('__main__.'))
                super().addSuccess(test)

        suite = unittest.defaultTestLoader.loadTestsFromNames(request['ids'], module)
        result = unittest.TextTestRunner(verbosity=1, resultclass=Result).run(suite)
        report = dict(completed=completed, successes=successes, count=result.testsRun,
                      skipped=len(result.skipped), passed=result.wasSuccessful())
    else:
        _, named, inline = procedural_cases(ast.parse(source))
        assertion_lines = {}
        for saved in (snapshot / 'scripts').rglob('*.test.py'):
            logical = str(root / saved.relative_to(snapshot))
            assertion_lines[logical] = {node.lineno for node in ast.walk(ast.parse(saved.read_text()))
                                       if isinstance(node, ast.Assert)}
        reached, active, counts, failures, occurrences = set(), [], {}, [], {}

        def wrap(function, fixed_case=None):
            def scenario(*args, **kwargs):
                case = fixed_case
                if case is None:
                    caller = sys._getframe(1)
                    matches = [key for key, (start, end) in inline.items()
                               if caller.f_code.co_filename == filename and start <= caller.f_lineno <= end]
                    position = (caller.f_code.co_filename, caller.f_lineno)
                    index = occurrences.get(position, 0)
                    if not matches or index >= len(matches):
                        failures.append('scenario outside fixed call inventory')
                        raise ValueError('procedural scenario escaped its fixed call inventory')
                    case = matches[index]
                    occurrences[position] = index + 1
                active.append(case)
                counts.setdefault(case, 0)
                try:
                    value = function(*args, **kwargs)
                except BaseException:
                    failures.append(case)
                    raise
                else:
                    if counts[case]:
                        successes.append(case)
                    return value
                finally:
                    completed.append(case)
                    active.pop()
            scenario.__name__ = function.__name__
            return scenario

        for name in named:
            module.__dict__[name] = wrap(module.__dict__[name], name)
        if inline:
            module.scenario = wrap(module.scenario)

        def trace(frame, event, arg):
            if event == 'line' and frame.f_lineno in assertion_lines.get(frame.f_code.co_filename, ()):
                reached.add((frame.f_code.co_filename, frame.f_lineno))
                for case in active:
                    counts[case] += 1
            return trace

        sys.settrace(trace)
        try:
            module.main()
        finally:
            sys.settrace(None)
        report = dict(completed=['main', *completed], successes=['main', *successes], count=1 + len(completed), skipped=0,
                      passed=bool(reached) and not failures, assertions=len(reached))
    report['complete'] = True
    witness_path.write_text(json.dumps(report))


def run_contract(root, snapshot, path, identifiers=None, *, retain_candidate=False, origin='trusted-baseline'):
    source = (snapshot / path).read_text()
    kind, inventory = test_inventory(source)
    selected = inventory if identifiers is None else identifiers
    if not selected or len(selected) != len(set(selected)) or not set(selected) <= set(inventory):
        raise ValueError('missing fixed baseline test inventory: ' + path)
    if retain_candidate:
        candidate = root / path
        if not candidate.is_file() or candidate.resolve() != candidate:
            raise ValueError('candidate removed registered Python tests: ' + path)
        candidate_kind, candidate_inventory = test_inventory(candidate.read_text())
        if kind != candidate_kind or not set(inventory) <= set(candidate_inventory):
            raise ValueError('candidate retired baseline Python test identities: ' + path)
    with tempfile.TemporaryDirectory(prefix='ci-python-witness-') as directory:
        request, witness = Path(directory) / 'request.json', Path(directory) / 'complete.json'
        request.write_text(json.dumps(dict(root=str(root), snapshot=str(snapshot), path=path,
                                          kind=kind, ids=selected)))
        result = subprocess.run([sys.executable, str(Path(__file__).resolve()), '_worker',
                                 '--request', str(request), '--witness', str(witness)], cwd=root)
        report = json.loads(witness.read_text()) if witness.is_file() else {}
        if (result.returncode or report.get('complete') is not True or report.get('passed') is not True
                or report.get('count') != len(selected) or report.get('skipped') != 0
                or sorted(report.get('completed', [])) != sorted(selected)
                or sorted(report.get('successes', [])) != sorted(selected)):
            evidence = dict(exit_code=result.returncode, complete=report.get('complete'),
                            count=report.get('count'), expected_count=len(selected),
                            skipped=report.get('skipped'), passed=report.get('passed'),
                            missing=sorted(set(selected) - set(report.get('successes', []))))
            raise ValueError(f'{origin} Python contract did not completely execute: {path}: '
                             + json.dumps(evidence, sort_keys=True))
    counts = (f'cases={len(selected)}' if kind == 'unittest' else
              f'entrypoints=1 scenarios={len(selected) - 1} assertions={report["assertions"]}')
    print(f'python_control_contract={path} {counts} source={origin}', flush=True)
    return len(selected)


def migration_paths(root, policy_sha, plan, config):
    """An explicit full policy migration is proved from Git, never a plan flag."""
    if plan.get('scope') not in ('full', 'explicit-full'):
        return []
    if set(plan.get('groups', [])) != set(config['groups']):
        raise ValueError('full Python control migration omitted a required group')
    boundaries = ['scripts/ci-required-scope.json', 'scripts/ci-python-control-contracts.py']
    changed = set()
    for field in ('source_sha', 'tested_sha'):
        revision = plan.get(field, '')
        if not isinstance(revision, str) or not re.fullmatch(r'[0-9a-f]{40}', revision):
            raise ValueError('invalid Python control source identity')
        # Target-only advances must not become reverse deletions attributed to
        # an older PR head. Also retain actual tested-merge resolutions.
        previous = (checked(['git', 'merge-base', policy_sha, revision], root=root).strip()
                    if field == 'source_sha' else policy_sha)
        changed.update(checked(['git', 'diff', '--name-only', '--no-renames', previous,
                                revision, '--', *boundaries], root=root).splitlines())
    if changed - set(boundaries):
        raise ValueError('unexpected policy migration path')
    return sorted(changed)


def run_contracts(root, policy_sha, plan):
    config = json.loads(git_source(root, policy_sha, 'scripts/ci-required-scope.json'))
    selected = contract_selection(config, plan)
    if (any(not isinstance(plan.get(field), str) or not re.fullmatch(r'[0-9a-f]{40}', plan[field])
            for field in ('source_sha', 'tested_sha', 'policy_sha'))
            or plan.get('policy_sha') != policy_sha
            or plan.get('tested_sha') != checked(['git', 'rev-parse', 'HEAD'], root=root).strip()
            or os.environ.get('SOURCE_SHA', plan.get('source_sha')) != plan.get('source_sha')):
        raise ValueError('Python contract plan identity mismatch')
    migration = migration_paths(root, policy_sha, plan, config)
    all_tests = {test for spec in config['python_control_contracts'].values() for test in spec['tests']}
    if not set(FIXED_TESTS) <= all_tests:
        raise ValueError('fixed safety assertions are missing from the hashed contract inventory')
    with tempfile.TemporaryDirectory(prefix='ci-python-baseline-') as directory:
        snapshot = Path(directory)
        for path in sorted(all_tests):
            destination = snapshot / path
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_text(git_source(root, policy_sha, path))
        candidate_snapshot = snapshot / 'candidate'
        for path in all_tests:
            destination = candidate_snapshot / path
            destination.parent.mkdir(parents=True, exist_ok=True)
            if (root / path).is_file():
                if (root / path).resolve() != root / path:
                    raise ValueError('candidate Python test is not a regular contained file: ' + path)
                destination.write_text((root / path).read_text())
        cases = 0
        for path, identifiers in FIXED_TESTS.items():
            cases += run_contract(root, snapshot, path, identifiers)
            required = identifiers or test_inventory((snapshot / path).read_text())[1]
            if not set(required) <= set(test_inventory((root / path).read_text())[1]):
                raise ValueError('candidate retired fixed safety tests: ' + path)
            cases += run_contract(root, candidate_snapshot, path, required, origin='candidate')
        if migration:
            print('python_control_policy_migration=' + json.dumps(dict(
                changed_paths=migration, retained='fixed baseline safety tests and real Rust fixture',
                historical_contracts_delegated_to_candidate=selected), sort_keys=True), flush=True)
        for path in selected:
            # Planner policy changes are covered by the fixed baseline safety
            # subset. The complete candidate suite still runs in governance.
            if path == PLANNER_TEST:
                continue
            if not migration:
                if path not in FIXED_TESTS or FIXED_TESTS[path] is not None:
                    cases += run_contract(root, snapshot, path, retain_candidate=True)
                else:
                    old = set(test_inventory((snapshot / path).read_text())[1])
                    if not old <= set(test_inventory((root / path).read_text())[1]):
                        raise ValueError('candidate retired baseline Python test identities: ' + path)
            # Re-running candidate tests under the same completion protocol
            # rejects a newly added skip and proves new regression cases.
            cases += run_contract(root, candidate_snapshot, path, origin='candidate')
        run_rust_fixture(root, config)
    return cases


def run_rust_fixture(candidate, baseline):
    """Exercise real Cargo/libtest without resolving the product workspace."""
    with tempfile.TemporaryDirectory(prefix='ci-control-rust-') as directory:
        root = Path(directory)

        def write(path, text):
            destination = root / path
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_text(text)

        def command(*arguments, env=None):
            return checked(list(arguments), root=root, env=env)

        def commit():
            command('git', 'add', '.')
            command('git', '-c', 'user.name=CI fixture', '-c', 'user.email=fixture@example.invalid',
                    'commit', '-qm', 'fixture')
            return command('git', 'rev-parse', 'HEAD').strip()

        package = 'oasis7_runtime'
        source_path = f'crates/{package}/src/behavior.rs'
        write('.gitignore', '/target/\n/bundle*/\n/results*/\n/selected*.json\n')
        write('Cargo.toml', '[workspace]\nmembers=["crates/oasis7_runtime"]\nresolver="2"\n')
        write(f'crates/{package}/Cargo.toml',
              '[package]\nname="oasis7_runtime"\nversion="0.0.0"\nedition="2021"\n'
              '[features]\ndefault=[]\ntest_tier_required=[]\ntest_tier_full=["test_tier_required"]\n')
        write(f'crates/{package}/src/lib.rs', '#[cfg(not(target_arch="wasm32"))]\nmod behavior;\n')
        before = '''struct State {
    #[cfg(any(test, feature="test_tier_required"))]
    value: usize,
}
#[cfg(any(test, feature="test_tier_required"))]
impl State {
    fn new() -> Self {
        Self {
            #[cfg(any(test, feature="test_tier_required"))]
            value: 7,
        }
    }
    fn read(&self) -> usize {
        #[cfg(any(test, feature="test_tier_required"))]
        let value = self.value;
        value
    }
}
#[cfg(any(test, feature="test_tier_required"))]
fn free_function() -> usize {
    #[cfg(any(test, feature="test_tier_required"))]
    let result = 7;
    result
}
#[cfg(test)] mod tests {
    #[test] fn contract() { assert_eq!(super::State::new().read(), 7); }
    #[cfg(feature="test_tier_required")]
    #[test] fn tier_case() { assert!(super::free_function() >= 7); }
}
'''
        write(source_path, before)
        write('scripts/ci-formal-process-suites.json', '{}')
        target = dict(package=package, target_kind='lib', target_name=package,
                      default_features=False, features=['test_tier_required'],
                      cases=['behavior::tests::contract', 'behavior::tests::tier_case'])
        groups = baseline['groups']
        config = dict(groups=groups, resources={group: [] for group in groups}, rules=[],
                      package_groups={package: ['oasis7_required', 'formal_process']},
                      fast_scope_version=1, fast_test_inventory_version=1, fast_execution_version=1,
                      architecture_build_policy_version=1, oasis7_required_packages=[package],
                      oasis7_required_wasm_packages=[package],
                      formal_risk_rules=[dict(match=[f'crates/{package}/src/**'], suites=[], reason='fixture')],
                      ordinary_fast_targets={'fixture-lib': target},
                      ordinary_fast_rules=[dict(paths=[source_path], targets=['fixture-lib'], reason='fixture')],
                      ordinary_fast_component_rules=[dict(match=[source_path], exclude=[],
                                                          targets=['fixture-lib'], reason='fixture')])
        config_path = root / 'fixture-policy.json'
        config_path.write_text(json.dumps(config))
        command('git', 'init', '-q')
        command('cargo', 'generate-lockfile', '--offline')
        base = commit()
        # A new sibling item must not change the identity of existing cfgs in
        # the initializer, method body or free-function local statements.
        unchanged = 'fn newly_added_sibling() -> usize { 1 }\n' + before
        write(source_path, unchanged)
        head = commit()
        environment = dict(os.environ, SOURCE_SHA=head, CARGO_NET_OFFLINE='true',
                           CARGO_TARGET_DIR=str(root / 'target'))
        environment.pop('RUSTC_WRAPPER', None)
        planner = candidate / 'scripts/plan-rust-required-scope.py'
        artifacts = candidate / 'scripts/ci-test-artifacts.py'
        gate = candidate / 'scripts/ci-required-result.py'

        def select(previous, current, name):
            destination = root / name
            command(sys.executable, str(planner), '--config', str(config_path),
                    '--formal-config', str(root / 'scripts/ci-formal-process-suites.json'),
                    '--base-ref', previous, '--head-ref', current, '--test-ref', current,
                    '--policy-ref', base, '--profile', 'pr-fast', '--output', str(destination),
                    env=environment)
            return destination, json.loads(destination.read_text())

        plan_path, selected = select(base, head, 'selected.json')
        if (selected.get('test_policy_changed') is not False
                or selected.get('ordinary_fast_targets') != [target]
                or selected.get('ordinary_execution') != 'inline'):
            raise ValueError('unchanged Rust cfg owners did not retain the actual native target: '
                             + json.dumps(selected, sort_keys=True))
        bundle, results = root / 'bundle', root / 'results/ordinary.jsonl'
        command(sys.executable, str(artifacts), 'build', '--repo-root', str(root),
                '--scope', 'ordinary', '--bundle', str(bundle), '--plan', str(plan_path), env=environment)
        command(sys.executable, str(artifacts), 'run', '--repo-root', str(root),
                '--bundle', str(bundle), '--plan', str(plan_path), '--results', str(results), env=environment)
        records = [json.loads(line) for line in results.read_text().splitlines()]
        if len(records) != 1 or records[0].get('passed') is not True or records[0].get('expected_count') != 2:
            raise ValueError('real Rust control fixture did not execute its two required cases')
        jobs = {group: group.replace('_', '-') for group in groups}
        needs = {'select': {'result': 'success'}, **{
            job: {'result': 'success' if group in selected['groups'] else 'skipped'}
            for group, job in jobs.items()}}

        def gate_command(actual):
            return [sys.executable, str(gate), '--config', str(config_path),
                    '--plan', json.dumps(selected), '--needs', json.dumps(actual)]

        command(*gate_command(needs))
        for state in ('failure', 'cancelled', 'skipped', None):
            invalid = copy.deepcopy(needs)
            invalid['oasis7-required'] = {'result': state}
            if subprocess.run(gate_command(invalid), cwd=root, capture_output=True).returncode == 0:
                raise ValueError('real planned executor result did not block the required gate')
        # A real tier move still expands the selection, and the expanded feature
        # combination actually discovers/runs the moved case in this same crate.
        changed = unchanged.replace('#[cfg(feature="test_tier_required")]\n    #[test]',
                                    '#[cfg(feature="test_tier_full")]\n    #[test]')
        write(source_path, changed)
        expanded_head = commit()
        environment['SOURCE_SHA'] = expanded_head
        expanded_path, expanded = select(head, expanded_head, 'selected-expanded.json')
        if expanded.get('test_policy_changed') is not True or 'ordinary_fast_targets' in expanded:
            raise ValueError('real test feature move escaped expanded execution')
        arguments = command(sys.executable, str(artifacts), 'required-args', '--repo-root', str(root),
                            '--plan', str(expanded_path), '--phase', 'native', env=environment)
        if 'test_tier_full' not in arguments:
            raise ValueError('expanded plan dropped the actual full feature execution')
        output = command('cargo', 'test', '--offline', '--locked', '-p', package, '--lib',
                         '--no-default-features', '--features', 'test_tier_required,test_tier_full', env=environment)
        if '2 passed' not in output:
            raise ValueError('expanded Rust feature fixture did not run both behavior cases')
        print('python_control_rust_fixture=passed plan=real executor=real gate=real required_cases=2 expanded_cases=2', flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('run', '_worker', 'fixture'))
    parser.add_argument('--repo-root', type=Path, default=Path.cwd())
    parser.add_argument('--policy-sha')
    parser.add_argument('--plan', type=Path)
    parser.add_argument('--request', type=Path)
    parser.add_argument('--witness', type=Path)
    args = parser.parse_args()
    if args.action == '_worker':
        run_worker(args.request, args.witness)
    elif args.action == 'fixture':
        config = json.loads((args.repo_root / 'scripts/ci-required-scope.json').read_text())
        run_rust_fixture(args.repo_root.resolve(), config)
    else:
        if not args.policy_sha or not args.plan:
            parser.error('run requires --policy-sha and --plan')
        cases = run_contracts(args.repo_root.resolve(), args.policy_sha, json.loads(args.plan.read_text()))
        print(f'Python control contracts passed ({cases} witnessed checks plus real offline Rust fixture).')


if __name__ == '__main__':
    main()
