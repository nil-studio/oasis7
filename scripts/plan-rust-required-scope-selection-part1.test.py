#!/usr/bin/env python3
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('selector', HERE / 'plan-rust-required-scope.py')
selector = importlib.util.module_from_spec(spec)
spec.loader.exec_module(selector)

RETIREMENT_INPUT_CONTRACTS = {
    '.gitattributes': None,
    'scripts/unified-world-code-terminology-scan.sh': {'baseline'},
    'scripts/unified-world-code-terminology-scan.test.sh': {'baseline'},
}


class SelectionPart1(unittest.TestCase):
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

    def test_workspace_support_host_schedule_preserves_targeted_game_client_and_full(self):
        for scope, profiles, expected in (
            ('targeted', ['oasis7_game_client'], True),
            ('targeted', ['oasis7_local_signer'], True),
            ('targeted', ['oasis7_context'], False),
            ('full', ['oasis7_context'], True),
        ):
            with self.subTest(scope=scope, profiles=profiles):
                self.assertIs(selector.workspace_support_requires_host(
                    {'scope': scope, 'workspace_support_profiles': profiles}), expected)

    def test_retirement_input_exact_ownership_covers_add_modify_delete_and_rename(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        with patch.object(selector, 'cargo_graph', return_value=({}, {}, {})):
            for index, (path, expected) in enumerate(RETIREMENT_INPUT_CONTRACTS.items()):
                renamed = f'doc/retirement-input-{index}.md'
                for operation in ('add', 'modify', 'rename-out', 'rename-back', 'delete'):
                    with self.subTest(path=path, operation=operation):
                        if operation == 'delete':
                            self.git('rm', path)
                        elif operation == 'rename-out':
                            self.git('mv', path, renamed)
                        elif operation == 'rename-back':
                            self.git('mv', renamed, path)
                        else:
                            self.write(path, '# synthetic ownership input ' + operation + '\n')
                        head = self.commit()
                        plan = selector.select(config, self.base, head)
                        self.assertEqual(plan['scope'], 'full' if expected is None else 'targeted')
                        self.assertEqual(set(plan['groups']),
                                         set(config['groups']) if expected is None else expected)
                        self.base = head

    def test_retirement_input_neighbors_and_unknown_rename_endpoints_stay_rejected(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        neighbors = (
            '.gitattributes.local',
            'scripts/.gitattributes',
            'scripts/unified-world-code-terminology-extra.sh',
            'scripts/unified-world-code-terminology-scan-extra.test.sh',
            'scripts/fixtures/document-corpus-v3/legacy/extra.json',
            'scripts/fixtures/document-corpus-v3/legacy/nested/inventory.json',
            'scripts/fixtures/document-corpus-v3/inventory.json',
        )
        with patch.object(selector, 'cargo_graph', return_value=({}, {}, {})):
            for path in neighbors:
                for operation in ('add', 'delete'):
                    with self.subTest(path=path, operation=operation):
                        if operation == 'delete':
                            self.git('rm', path)
                        else:
                            self.write(path, '# unregistered neighbor\n')
                        head = self.commit()
                        with self.assertRaises(selector.UnknownInputsError) as caught:
                            selector.select(config, self.base, head)
                        self.assertIn(path, str(caught.exception))
                        self.base = head
            registered = 'scripts/unified-world-code-terminology-scan.sh'
            unknown = 'scripts/unified-world-code-terminology-renamed.sh'
            self.write(registered, '#!/usr/bin/env bash\n')
            self.base = self.commit()
            for source, target in ((registered, unknown), (unknown, registered)):
                with self.subTest(source=source, target=target):
                    self.git('mv', source, target)
                    head = self.commit()
                    with self.assertRaises(selector.UnknownInputsError) as caught:
                        selector.select(config, self.base, head)
                    self.assertIn(unknown, str(caught.exception))
                    self.base = head

    def test_retirement_input_registration_requires_the_trusted_baseline(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        registration_reasons = {'git_checkout_attributes', 'unified_world_terminology'}
        old = dict(config, rules=[rule for rule in config['rules']
                                  if rule['reason'] not in registration_reasons])
        self.write('scripts/ci-required-scope.json', json.dumps(config))
        for path in RETIREMENT_INPUT_CONTRACTS:
            self.write(path, '# candidate registration input\n')
        head = self.commit()
        with patch.object(selector, 'cargo_graph', return_value=({}, {}, {})):
            for additions in ((), ('doc_checker_contracts',)):
                with self.subTest(additions=additions):
                    with self.assertRaises(selector.UnknownInputsError) as caught:
                        selector.select(old, self.base, head, additions=additions)
                    for path in RETIREMENT_INPUT_CONTRACTS:
                        self.assertIn(path, str(caught.exception))
            self.assertEqual(selector.select(config, self.base, head)['scope'], 'full')

    def test_product_doc_ci_range_exact_ownership_uses_trusted_config(self):
        paths = ('scripts/resolve-product-doc-ci-range.py', 'scripts/resolve-product-doc-ci-range.test.py')
        trusted = json.loads((HERE / 'ci-required-scope.json').read_text())
        old = json.loads(json.dumps(trusted))
        rule = next(rule for rule in old['rules'] if rule['reason'] == 'document_checker')
        rule['match'] = [pattern for pattern in rule['match'] if pattern not in paths]
        for path in paths:
            self.write(path, '# existing resolver input\n')
        self.write('scripts/ci-required-scope.json', json.dumps(old))
        self.base = self.commit()
        with patch.object(selector, 'cargo_graph', return_value=({}, {}, {})):
            for path in paths:
                with self.subTest(path=path):
                    self.write(path, '# changed resolver input\n')
                    head = self.commit()
                    plan = selector.select(trusted, self.base, head)
                    self.assertEqual(plan['scope'], 'targeted')
                    self.assertEqual(set(plan['groups']), {'baseline', 'doc_checker_contracts'})
                    with self.assertRaises(selector.UnknownInputsError):
                        selector.select(old, self.base, head)
                    self.base = head
            # Candidate configuration, even a shared-control full fallback,
            # cannot enrol this same change against the old trusted policy.
            self.write('scripts/ci-required-scope.json', json.dumps(trusted))
            self.write(paths[1], '# candidate also changes its own test\n')
            head = self.commit()
            with self.assertRaises(selector.UnknownInputsError):
                selector.select(old, self.base, head)
            self.base = head
            self.write('scripts/resolve-product-doc-ci-range-extra.py', '# unregistered neighbor\n')
            with self.assertRaises(selector.UnknownInputsError):
                selector.select(trusted, self.base, self.commit())

    def test_ci_download_exact_registration_and_old_baseline(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        paths = ('scripts/install-ci-trunk.sh', 'scripts/install-ci-trunk.test.sh')
        expected = {'baseline', 'workflow_governance', 'packaging_contracts', 'launcher_web',
                    'macos_package_contract'}
        old = dict(config, rules=[r for r in config['rules'] if r['reason'] != 'ci_download_contracts'])
        with patch.object(selector, 'cargo_graph', return_value=({}, {}, {})):
            for path in paths:
                for operation in ('add', 'modify', 'delete'):
                    with self.subTest(path=path, operation=operation):
                        if operation == 'delete':
                            self.git('rm', path)
                        else:
                            self.write(path, '# synthetic download input ' + operation)
                        head = self.commit()
                        plan = selector.select(config, self.base, head)
                        self.assertEqual(plan['scope'], 'targeted')
                        self.assertEqual(set(plan['groups']), expected)
                        with self.assertRaises(selector.UnknownInputsError):
                            selector.select(old, self.base, head)
                        self.base = head
            # An otherwise full control change cannot grant its own ownership.
            self.write('scripts/ci-required-scope.json', json.dumps(config))
            self.write(paths[0], '# candidate self-registration')
            head = self.commit()
            with self.assertRaises(selector.UnknownInputsError):
                selector.select(old, self.base, head)
            self.base = head
            for unknown in ('scripts/install-ci-trunk-extra.sh',
                            'scripts/install-ci-wasm-opt-extra.py'):
                with self.subTest(unknown=unknown):
                    self.write(unknown, '# unregistered neighbor')
                    with self.assertRaises(selector.UnknownInputsError):
                        selector.select(config, self.base, self.commit())
                    self.git('rm', unknown)
                    self.base = self.commit()
            unknown = 'scripts/install-ci-trunk-renamed.sh'
            self.git('mv', paths[0], unknown)
            head = self.commit()
            with self.assertRaises(selector.UnknownInputsError):
                selector.select(config, self.base, head)
            self.base = head
            self.git('mv', unknown, paths[0])
            with self.assertRaises(selector.UnknownInputsError):
                selector.select(config, self.base, self.commit())

    def test_wasm_bindgen_tool_consumers_require_exact_trusted_registration(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        paths = ('scripts/ensure-wasm-bindgen-cli.sh',
                 'scripts/ensure-wasm-bindgen-cli.test.sh')
        expected = {'workflow_governance', 'viewer_js_required',
                    'viewer_performance_report', 'operational_contracts'}
        old = dict(config, rules=[rule for rule in config['rules']
                                 if rule['reason'] != 'wasm_bindgen_tool_consumers'])
        with patch.object(selector, 'cargo_graph', return_value=({}, {}, {})):
            for path in paths:
                for operation in ('add', 'modify', 'delete'):
                    with self.subTest(path=path, operation=operation):
                        if operation == 'delete':
                            self.git('rm', path)
                        else:
                            self.write(path, '# tool consumer input ' + operation)
                        head = self.commit()
                        plan = selector.select(config, self.base, head)
                        self.assertEqual(plan['scope'], 'targeted')
                        self.assertTrue(expected.issubset(plan['groups']))
                        with self.assertRaises(selector.UnknownInputsError):
                            selector.select(old, self.base, head)
                        self.base = head
            self.write('scripts/ci-required-scope.json', json.dumps(config))
            self.write(paths[0], '# candidate cannot register itself')
            head = self.commit()
            with self.assertRaises(selector.UnknownInputsError):
                selector.select(old, self.base, head)
            self.base = head
            unknown = 'scripts/ensure-wasm-bindgen-cli-extra.sh'
            self.git('mv', paths[0], unknown)
            head = self.commit()
            with self.assertRaises(selector.UnknownInputsError):
                selector.select(config, self.base, head)
            self.base = head
            self.git('mv', unknown, paths[0])
            with self.assertRaises(selector.UnknownInputsError):
                selector.select(config, self.base, self.commit())

    def test_engineering_telemetry_exact_registration_and_old_baseline(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        paths = (
            'scripts/engineering_telemetry.py',
            'scripts/engineering_telemetry_adapters.py',
            'scripts/tests/fixtures/engineering-telemetry/input.json',
            'scripts/tests/test_engineering_telemetry.py',
            'scripts/tests/test_engineering_telemetry_adapters.py',
        )
        old = dict(config, rules=[r for r in config['rules']
                                  if r['reason'] != 'engineering_telemetry_contracts'])
        with patch.object(selector, 'cargo_graph', return_value=({}, {}, {})):
            for path in paths:
                with self.subTest(path=path):
                    self.write(path, '# synthetic telemetry input')
                    head = self.commit()
                    plan = selector.select(config, self.base, head)
                    self.assertEqual(plan['scope'], 'targeted')
                    self.assertEqual(set(plan['groups']), {'baseline', 'workflow_governance'})
                    with self.assertRaises(selector.UnknownInputsError):
                        selector.select(old, self.base, head)
                    self.base = head
                    self.git('rm', path)
                    head = self.commit()
                    self.assertEqual(set(selector.select(config, self.base, head)['groups']),
                                     {'baseline', 'workflow_governance'})
                    self.base = head
            # Candidate registration plus a full control edit cannot establish authority.
            self.write('scripts/ci-required-scope.json', json.dumps(config))
            self.write(paths[0], '# candidate input')
            head = self.commit()
            with self.assertRaises(selector.UnknownInputsError):
                selector.select(old, self.base, head)
            self.base = head
            for unknown in ('scripts/engineering_telemetry_extra.py',
                            'scripts/tests/test_engineering_telemetry_extra.py',
                            'scripts/tests/fixtures/engineering-telemetry/extra.json'):
                with self.subTest(unknown=unknown):
                    self.write(unknown, '# unregistered neighbor')
                    with self.assertRaises(selector.UnknownInputsError):
                        selector.select(config, self.base, self.commit())
                    self.git('rm', unknown)
                    self.base = self.commit()
            unknown = 'scripts/engineering_telemetry_renamed.py'
            self.git('mv', paths[0], unknown)
            with self.assertRaises(selector.UnknownInputsError):
                selector.select(config, self.base, self.commit())

    def test_m1_storage_source_has_explicit_runtime_and_builder_consumers(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        path = 'crates/oasis7_builtin_wasm_modules/m1_power_storage/src/lib.rs'
        self.write(path, '// existing standalone WASM source\n')
        head = self.commit()
        with patch.object(selector, 'cargo_graph', return_value=({}, {}, {})):
            plan = selector.select(config, self.base, head)
            self.assertEqual(set(plan['groups']), {
                'baseline', 'rust_baseline', 'oasis7_required', 'formal_process',
                'launcher_web', 'workspace_support',
            })
            self.assertEqual(plan['workspace_support_profiles'], ['wasm_build_suite'])
            trusted_before_registration = dict(config, rules=[
                rule for rule in config['rules']
                if rule['reason'] != 'm1_storage_wasm_consumers'
            ])
            with self.assertRaises(selector.UnknownInputsError):
                selector.select(trusted_before_registration, self.base, head)
            self.base = head
            self.git('rm', path)
            deleted = self.commit()
            self.assertIn('oasis7_required', selector.select(config, self.base, deleted)['groups'])

    def test_m1_storage_registration_does_not_admit_neighboring_or_renamed_unknown_sources(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        source = 'crates/oasis7_builtin_wasm_modules/m1_power_storage/src/lib.rs'
        unknown = 'crates/oasis7_builtin_wasm_modules/m1_future/src/lib.rs'
        self.write(source, '// registered source\n')
        self.base = self.commit()
        self.write(unknown, '// not registered\n')
        with patch.object(selector, 'cargo_graph', return_value=({}, {}, {})):
            with self.assertRaises(selector.UnknownInputsError):
                selector.select(config, self.base, self.commit())
            self.git('rm', unknown)
            self.base = self.commit()
            Path(unknown).parent.mkdir(parents=True, exist_ok=True)
            self.git('mv', source, unknown)
            with self.assertRaises(selector.UnknownInputsError):
                selector.select(config, self.base, self.commit())

    def test_wasm_consumer_exact_paths_select_real_suite_and_domain_groups(self):
        config = json.loads((HERE / "ci-required-scope.json").read_text())
        graph = ({}, {}, {})
        cases = {
            "scripts/module-release-node-attestation-flow.sh": {"operational_contracts", "formal_process", "workspace_support"},
            "scripts/stage-wasm-summary-imports.sh": {"packaging_contracts", "workspace_support"},
            "scripts/wasm-release-evidence-report.sh": {"operational_contracts", "packaging_contracts", "workspace_support"},
            "scripts/wasm-summary-bundle-smoke.sh": {"packaging_contracts", "workspace_support"},
        }
        with patch.object(selector, "cargo_graph", return_value=graph):
            for path, required in cases.items():
                self.write(path, "# exact existing consumer\n")
                head = self.commit()
                plan = selector.select(config, self.base, head)
                self.assertTrue(required.issubset(plan["groups"]))
                self.assertEqual(plan["workspace_support_profiles"], ["wasm_build_suite"])
                self.base = head
            for path in ("scripts/module-release-node-attestation-flow-future.sh", "scripts/stage-wasm-summary-imports-future.sh", "scripts/wasm-release-evidence-report-future.sh", "scripts/wasm-summary-bundle-smoke-future.sh"):
                self.write(path, "# not registered\n")
                head = self.commit()
                with self.assertRaises(selector.UnknownInputsError):
                    selector.select(config, self.base, head)
                self.base = head

    def test_architecture_inputs_have_trusted_owner_and_select_execution(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        for path in ('scripts/cargo-dev-architecture.py', 'scripts/cargo-dev-architecture.json',
                     'scripts/cargo-dev-architecture.test.py'):
            with self.subTest(path=path):
                # The existing cargo-dev* owner is already trusted before this
                # PR. It must recognize first introduction without self-enrolment.
                trusted = copy_config = json.loads(json.dumps(config))
                copy_config['rules'] = [r for r in config['rules']
                                        if r['reason'] != 'cargo_architecture_boundaries']
                self.write(path, '# input')
                head = self.commit()
                self.assertIn('cargo_tooling_contracts', selector.select(trusted, self.base, head)['groups'])
                plan = selector.select(config, self.base, head)
                self.assertTrue({'rust_baseline', 'cargo_tooling_contracts'}.issubset(plan['groups']))
                self.base = head

    def test_architecture_direct_inputs_do_not_flatten_reverse_profiles(self):
        self.write('crates/core/src/lib.rs', '// ordinary source')
        plan = selector.select(self.config, self.base, self.commit())
        self.assertEqual(plan['architecture']['mode'], 'targeted')
        self.assertEqual(plan['architecture']['packages'], ['core'])
        self.assertIn('consumer', plan['groups'])  # ordinary CI stays conservative

    def test_unchanged_git_blobs_are_read_and_lexed_once_per_selection(self):
        self.write('doc/readme.md', 'documentation alone changed')
        head = self.commit()
        original = selector.subprocess.check_output
        batches = []

        def read(command, **kwargs):
            if command == ['git', 'cat-file', '--batch']:
                batches.extend(kwargs['input'].splitlines())
            return original(command, **kwargs)

        with patch.object(selector.subprocess, 'check_output', side_effect=read), \
                patch.object(selector, 'rust_source_inputs', wraps=selector.rust_source_inputs) as parse:
            plan = selector.select(self.config, self.base, head)
        self.assertEqual(plan['groups'], ['baseline'])
        # All three lib.rs files are the same blob, unchanged across two refs.
        self.assertEqual(parse.call_count, 1)
        self.assertEqual(len(batches), len(set(batches)))
        self.assertTrue(batches)

    def test_blob_parse_reuse_keeps_each_tree_path_and_graph_independent(self):
        source = 'const INPUT: &str = include_str!("../fixture.txt");\n'
        for name in ('core', 'consumer', 'other'):
            self.write(f'crates/{name}/src/lib.rs', source)
            self.write(f'crates/{name}/fixture.txt', name)
        base = self.commit()
        cache = selector.GitSourceCache()
        first = selector.cargo_graph(base, source_cache=cache)
        for name in ('core', 'consumer', 'other'):
            path = f'crates/{name}/fixture.txt'
            self.assertEqual(first[2][path], {name})
            self.assertEqual(first[2]['__rust_data_readers__'][path], {f'crates/{name}/src/lib.rs'})
        # Mutating one returned graph must not alter another version's objects.
        first[0]['core'][1]['package']['name'] = 'tampered'
        first[1]['core'].clear()
        first[2]['crates/core/fixture.txt'].clear()
        second = selector.cargo_graph(base, source_cache=cache)
        self.assertEqual(second[0]['core'][1]['package']['name'], 'core')
        self.assertEqual(second[1]['core'], {'consumer'})
        self.assertEqual(second[2]['crates/core/fixture.txt'], {'core'})
        self.git('mv', 'crates/core/src/lib.rs', 'crates/core/src/relocated.rs')
        self.write('crates/core/src/lib.rs', 'mod relocated;\n')
        third = selector.cargo_graph(self.commit(), source_cache=cache)
        self.assertEqual(third[2]['__rust_data_readers__']['crates/core/fixture.txt'],
                         {'crates/core/src/relocated.rs'})

    def test_cached_literals_still_resolve_new_tracked_inputs_in_each_tree(self):
        self.write('crates/core/src/lib.rs', 'const PATH: &str = "doc/new-fixture.txt";\n')
        base = self.commit()
        self.write('doc/new-fixture.txt', 'newly tracked input')
        plan = selector.select(self.config, base, self.commit())
        self.assertEqual(set(plan['groups']), {'baseline', 'rust_baseline', 'core', 'consumer'})

    def test_parse_cache_does_not_survive_selection_or_mutable_ref_changes(self):
        self.write('doc/readme.md', 'first change')
        self.commit()
        self.assertEqual(selector.select(self.config, self.base, 'HEAD')['groups'], ['baseline'])
        self.write('crates/core/src/lib.rs', 'const DATA: &str = include_str!(env!("UNKNOWN_INPUT"));\n')
        self.commit()
        changed = selector.select(self.config, self.base, 'HEAD')
        self.assertEqual(changed['scope'], 'full')
        self.assertTrue(any('unresolved embedded consumer' in reason for reason in changed['reasons']))

    def test_blob_cache_rejects_incomplete_or_mismatched_batch_objects(self):
        oid = b'a' * 40
        cases = [b'b' * 40 + b' blob 1\nx\n', oid + b' tree 1\nx\n',
                 oid + b' blob 2\nx\n', oid + b' blob 1\nx?',
                 oid + b' blob 1\nx\nextra', oid + b' missing\n']
        for data in cases:
            with self.subTest(data=data):
                cache = selector.GitSourceCache()
                with patch.object(selector.subprocess, 'check_output', return_value=data), \
                        self.assertRaises(ValueError):
                    cache.load([oid])
                self.assertEqual(cache.blobs, {})

    def test_architecture_shared_and_checker_controls_build_full(self):
        for path in ('Cargo.toml', 'Cargo.lock', 'crates/core/build.rs', 'rust-toolchain.toml',
                     'scripts/cargo-dev-architecture.py', 'scripts/cargo-dev-architecture.json'):
            with self.subTest(path=path):
                if path == 'Cargo.toml':
                    self.write(path, Path(path).read_text() + '\n# changed\n')
                else:
                    self.write(path, '# control')
                config = json.loads(json.dumps(self.config))
                config['rules'].append({'match': ['scripts/cargo-dev*'], 'groups': ['rust_baseline'], 'reason': 'architecture'})
                self.assertEqual(selector.select(config, self.base, self.commit())['architecture']['mode'], 'full')


if __name__ == '__main__':
    unittest.main()
