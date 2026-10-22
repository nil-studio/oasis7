#!/usr/bin/env python3
import contextlib
import hashlib
import io
import sys
from unittest.mock import patch
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import textwrap
import unittest

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('selector', HERE / 'plan-rust-required-scope.py')
selector = importlib.util.module_from_spec(spec)
spec.loader.exec_module(selector)

PR4889_DOC_PATHS = (
    'doc/engineering/command-center/README.md',
    'doc/engineering/command-center/architecture/README.md',
    'doc/engineering/command-center/command-center.prd.md',
    'doc/engineering/command-center/command-center.design.md',
    'tools/oasis7-command-center/README.md',
    'doc/.governance/document-corpus/objects/6f/6fd2250a1448d193b624ebdd961f4460b39a0d006bb30ef14d5cb6345a203049.json',
    'doc/.governance/document-corpus/objects/c1/c148881578bf66447a9b6256b32cee7cea9b032d6c58f7ecece08285796b074f.json',
    'doc/.governance/document-corpus/objects/d2/d2970f104b5996a9da0e9ac9c3b6494bfe0ffb20419f0d60f9c77bb97ee5d772.json',
    'doc/.governance/document-corpus/objects/fd/fd8aef2ec18a5cd959fe1a23c9ae1a1d6dd95c63b6de435b6eb1472a5717b1ec.json',
)


class Selection(unittest.TestCase):
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
            self.write(f'crates/{package}/Cargo.toml', manifest)
            self.write(f'crates/{package}/src/lib.rs', '')
        self.write('doc/readme.md', 'ordinary doc')
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

    def plan(self, head=None):
        return selector.select(self.config, self.base, head or self.commit())

    def test_libp2p_compatibility_packages_and_driver_select_network_tests(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        for path in ('tests/libp2p-compat/common/src/protocol.rs',
                     'tests/libp2p-compat/candidate/src/main.rs',
                     'tests/libp2p-compat/legacy/src/main.rs',
                     'scripts/libp2p-compat.test.py'):
            with self.subTest(path=path):
                case_base = self.git('rev-parse', 'HEAD')
                self.write(path, 'compatibility input')
                plan = selector.select(config, case_base, self.commit())
                self.assertIn('net', plan['groups'])
                self.assertEqual(plan['scope'], 'targeted')

    def native_generated_fixture(self, register=True):
        # Retain the real producer and its three OUT_DIR consumers. A workspace
        # without these sources failed to reproduce the docs-only over-selection.
        self.config = json.loads((HERE / 'ci-required-scope.json').read_text())
        package = 'oasis7_chain_runtime'
        directory = 'crates/' + package
        producer = directory + '/build.rs'
        consumer = directory + '/src/host/native_environment_capture.rs'
        self.write(directory + '/Cargo.toml', f'[package]\nname="{package}"\nversion="0.1.0"\n')
        self.write(directory + '/src/lib.rs', '')
        for path in (producer, consumer):
            self.write(path, (HERE.parent / path).read_text())
        self.write('Cargo.lock', '# existing workspace lock\n')
        self.base = self.commit()
        declaration = {
            'package': package,
            'producer': producer,
            'producer_sha256': hashlib.sha256(selector.git('show', f'{self.base}:{producer}')).hexdigest(),
            'outputs': ['native-build-metadata.txt', 'native-toolchain.txt', 'native-Cargo.lock'],
        }
        if register:
            self.config['generated_inputs'] = [declaration]
        return producer, consumer, declaration

    def assert_unresolved_document(self, label):
        self.write('doc/readme.md', label)
        plan = self.plan()
        self.assertEqual(plan['scope'], 'full')
        self.assertEqual(set(plan['groups']), set(self.config['groups']))
        self.assertIn('unresolved embedded consumer input: doc/readme.md', plan['reasons'])
        self.base = self.git('rev-parse', 'HEAD')

    def test_chain_extraction_existing_script_inputs_are_registered(self):
        self.config = json.loads((HERE / 'ci-required-scope.json').read_text())
        expected = {
            'scripts/module-release-node-acceptance.sh': {'oasis7_required', 'formal_process', 'operational_contracts'},
            'scripts/release-web-fixture-lib.sh': {'launcher_web', 'operational_contracts'},
            'scripts/run-launcher-stack.sh': {'launcher_web', 'operational_contracts'},
        }
        for path, groups in expected.items():
            with self.subTest(path=path):
                self.write(path, '# changed existing chain caller')
                plan = self.plan()
                self.assertEqual(plan['scope'], 'targeted')
                self.assertTrue(groups.issubset(plan['groups']))
                self.base = self.git('rev-parse', 'HEAD')

    def test_hosted_passkey_browser_smoke_has_launcher_and_viewer_owners(self):
        self.config = json.loads((HERE / 'ci-required-scope.json').read_text())
        self.write('scripts/hosted-passkey-virtual-authenticator-smoke.mjs', '// changed browser smoke')
        plan = self.plan()
        self.assertEqual(plan['scope'], 'targeted')
        self.assertTrue({'launcher_web', 'viewer_js_required'}.issubset(plan['groups']))

    def test_rustsec_baseline_inputs_are_registered(self):
        self.config = json.loads((HERE / 'ci-required-scope.json').read_text())
        for path in ('deny.toml', 'scripts/rustsec-ignore-direct-dependency-baseline.json',
                     'scripts/check-rustsec-ignore-baseline.sh',
                     'scripts/check-rustsec-ignore-baseline.test.sh'):
            with self.subTest(path=path):
                self.write(path, 'changed security baseline input')
                plan = self.plan()
                self.assertEqual(plan['scope'], 'targeted')
                self.assertTrue({'rust_baseline', 'operational_contracts'}.issubset(plan['groups']))
                self.base = self.git('rev-parse', 'HEAD')

    def test_independent_targets_do_not_change_library_consumers(self):
        for path in ['src/bin/tool.rs', 'src/bin/tool/helpers.rs',
                     'tests/integration.rs', 'examples/example.rs', 'benches/bench.rs']:
            with self.subTest(path=path):
                self.write('crates/core/' + path, '// independent target')
                self.assertEqual(set(self.plan()['groups']), {'baseline', 'rust_baseline', 'core'})

    def test_library_and_leaf_source_rename_keep_consumers(self):
        self.write('crates/core/src/lib.rs', '// library changed')
        self.assertIn('consumer', self.plan()['groups'])
        self.base = self.git('rev-parse', 'HEAD')
        Path('crates/core/tests').mkdir()
        self.git('mv', 'crates/core/src/lib.rs', 'crates/core/tests/moved.rs')
        self.assertIn('consumer', self.plan()['groups'])

    def test_library_import_of_target_source_keeps_consumers(self):
        self.write('crates/core/src/lib.rs', '#[path = "bin/shared.rs"] mod shared;')
        self.write('crates/core/src/bin/shared.rs', '#[path = "nested/child.rs"] mod child;')
        self.write('crates/core/src/bin/nested/child.rs', '// child')
        self.base = self.commit()
        self.write('crates/core/src/bin/nested/child.rs', '// changed')
        self.assertIn('consumer', self.plan()['groups'])

    def test_unresolved_library_include_keeps_target_consumers(self):
        self.write('crates/core/src/lib.rs', 'include!(concat!(env!("UNRESOLVED"), "/shared.rs"));')
        self.write('crates/core/tests/shared.rs', '// shared')
        self.base = self.commit()
        self.write('crates/core/tests/shared.rs', '// changed')
        self.assertIn('consumer', self.plan()['groups'])

    def test_manifest_relative_library_fixture_keeps_target_consumers(self):
        self.write('crates/core/src/lib.rs', 'const X: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/shared.json"));')
        self.write('crates/core/tests/shared.json', '{}')
        self.base = self.commit()
        self.write('crates/core/tests/shared.json', '{"changed":true}')
        self.assertIn('consumer', self.plan()['groups'])

    def test_cross_package_source_import_keeps_nested_library_readers(self):
        self.write('crates/core/src/lib.rs', '#[path = "../../other/src/bin/shared.rs"] mod shared;')
        self.write('crates/other/src/bin/shared.rs', 'mod nested;')
        self.write('crates/other/src/bin/shared/nested.rs', '// shared child')
        self.base = self.commit()
        self.write('crates/other/src/bin/shared/nested.rs', '// changed shared child')
        self.assertIn('consumer', self.plan()['groups'])

    def test_cross_package_import_fixture_keeps_library_readers(self):
        self.write('crates/core/src/lib.rs', '#[path = "../../other/src/bin/shared.rs"] mod shared;')
        self.write('crates/other/src/bin/shared.rs', 'const FIXTURE: &str = include_str!("../../../../doc/readme.md");')
        self.base = self.commit()
        self.write('doc/readme.md', 'imported fixture changed')
        self.assertIn('consumer', self.plan()['groups'])

    def test_root_source_import_keeps_registered_nested_readers(self):
        self.config['rules'].append({'match': ['shared/**'], 'groups': ['other'], 'reason': 'shared fixture'})
        self.write('crates/core/src/lib.rs', '#[path = "../../../shared.rs"] mod shared;')
        self.write('shared.rs', 'mod nested;')
        self.write('shared/nested.rs', '// shared child')
        self.base = self.commit()
        self.write('shared/nested.rs', '// changed shared child')
        self.assertIn('consumer', self.plan()['groups'])

    def test_root_import_does_not_register_unrelated_paths(self):
        self.write('crates/core/src/lib.rs', '#[path = "../../../shared.rs"] mod shared;')
        self.write('shared.rs', '// shared source')
        self.base = self.commit()
        self.write('scripts/unregistered.sh', '# unrelated new input')
        with self.assertRaises(selector.UnknownInputsError):
            self.plan()

    def test_leaf_fixture_does_not_change_library_consumers(self):
        self.write('crates/core/tests/integration.rs', 'const FIXTURE: &str = include_str!("../../../doc/readme.md");')
        self.base = self.commit()
        self.write('doc/readme.md', 'fixture changed')
        self.assertEqual(set(self.plan()['groups']), {'baseline', 'rust_baseline', 'core'})

    def test_shared_library_and_leaf_fixture_keeps_consumers(self):
        self.write('crates/core/tests/integration.rs', 'const FIXTURE: &str = include_str!("../../../doc/readme.md");')
        self.write('crates/core/src/lib.rs', 'const FIXTURE: &str = include_str!("../../../doc/readme.md");')
        self.base = self.commit()
        self.write('doc/readme.md', 'fixture changed')
        self.assertIn('consumer', self.plan()['groups'])

    def test_oasis7_library_reaches_formal_through_chain_runtime(self):
        self.config = json.loads((HERE / 'ci-required-scope.json').read_text())
        self.write('crates/oasis7/Cargo.toml', '[package]\nname="oasis7"\nversion="0.1.0"\n')
        self.write('crates/oasis7/src/lib.rs', '')
        self.write('crates/oasis7_chain_runtime/Cargo.toml', '[package]\nname="oasis7_chain_runtime"\nversion="0.1.0"\n[dependencies]\noasis7={path="../oasis7",default-features=false}\n')
        self.write('crates/oasis7_chain_runtime/src/main.rs', '')
        self.base = self.commit()
        self.write('crates/oasis7/src/bin/viewer.rs', '// viewer binary')
        self.assertNotIn('formal_process', self.plan()['groups'])
        self.base = self.git('rev-parse', 'HEAD')
        self.write('crates/oasis7/src/lib.rs', '// world state changed')
        self.assertIn('formal_process', self.plan()['groups'])
        self.base = self.git('rev-parse', 'HEAD')
        self.write('crates/oasis7_chain_runtime/src/main.rs', '// runtime changed')
        self.assertIn('formal_process', self.plan()['groups'])

    def test_independent_bin_callers_are_registered(self):
        self.config = json.loads((HERE / "ci-required-scope.json").read_text())
        for path in ['scripts/oasis7-pure-api-parity-smoke.sh', 'scripts/governance-registry-drill.sh', 'scripts/governed-rollback-v2-drill.sh', 'scripts/hosted-account-staging-smoke.sh', 'scripts/llm-baseline-fixture-smoke.sh', 'scripts/llm-longrun-stress.sh', 'scripts/public-testnet-faucet/package-public-testnet-faucet.sh', 'scripts/check-active-llm-provider.sh', 'scripts/governance-registry-live-drill.sh', 'scripts/run-local-letai-provider-bridge.sh', 'scripts/provider-parity-p0.sh', 'scripts/oasis7-liveops-grant.sh', 'scripts/newapi-bridge-service/package-newapi-bridge-service.sh', 'scripts/run-local-public-testnet-letai-test-environment.sh', 'scripts/runtime-module-routing-perf-harness.sh']:
            with self.subTest(path=path):
                self.write(path, "changed existing bin caller")
                plan = self.plan()
                self.assertEqual(plan["scope"], "targeted")
                self.assertTrue({"oasis7_required", "operational_contracts"}.issubset(plan["groups"]))
                self.base = self.git("rev-parse", "HEAD")

    def test_command_center_pr4790_paths_are_targeted(self):
        # Exact paths from 4f12acc; fixture stays usable without its Git object.
        self.config = json.loads((HERE / 'ci-required-scope.json').read_text())
        paths = ['doc/.governance/document-corpus/objects/30/307677361b6dc1927f90fb3a6ff9960fbadaa3816a721dea819ab00c86f658c1.json', 'doc/.governance/document-corpus/objects/d2/d2970f104b5996a9da0e9ac9c3b6494bfe0ffb20419f0d60f9c77bb97ee5d772.json', 'doc/engineering/command-center/README.md', 'doc/engineering/command-center/document-tree.design.md', 'tools/oasis7-command-center/README.md', 'tools/oasis7-command-center/core/examples/export_types.rs', 'tools/oasis7-command-center/core/src/document_allocations.rs', 'tools/oasis7-command-center/core/src/document_inventory.rs', 'tools/oasis7-command-center/core/src/document_pairings.rs', 'tools/oasis7-command-center/core/src/index.rs', 'tools/oasis7-command-center/core/tests/document_allocations.rs', 'tools/oasis7-command-center/ui/src/DocumentAllocations.tsx', 'tools/oasis7-command-center/ui/src/DocumentInventory.test.tsx', 'tools/oasis7-command-center/ui/src/DocumentInventory.tsx', 'tools/oasis7-command-center/ui/src/DocumentRelations.tsx', 'tools/oasis7-command-center/ui/src/generated/core.ts']
        self.assertEqual(len(paths), 16)
        for path in paths:
            self.write(path, 'changed allocation input')
        plan = self.plan()
        self.assertEqual(plan['scope'], 'targeted')
        self.assertEqual(set(plan['groups']), {'baseline', 'command_center'})
        self.assertFalse(any('unknown input:' in reason for reason in plan['reasons']))

    def test_command_center_shared_tools_keep_dedicated_checks(self):
        self.config = json.loads((HERE / 'ci-required-scope.json').read_text())
        for path in ('scripts/cargo-dev.sh', 'scripts/cargo-cache.py',
                     'scripts/find-python-with-module.sh'):
            with self.subTest(path=path):
                self.write(path, 'shared tool changed')
                self.assertIn('command_center', self.plan()['groups'])
                Path(path).unlink()
                self.commit()

    def test_command_center_readme_only_keeps_baseline(self):
        self.config = json.loads((HERE / 'ci-required-scope.json').read_text())
        path = 'tools/oasis7-command-center/README.md'
        self.write(path, 'existing tool documentation')
        self.base = self.commit()
        self.write(path, 'updated tool documentation')

        plan = self.plan()

        self.assertEqual(plan['scope'], 'targeted')
        self.assertEqual(plan['groups'], ['baseline'])

    def test_command_center_pr4889_docs_with_native_generated_inputs_keep_baseline(self):
        # Use the production registry, not a fixture-created exemption. Both the
        # declaration and the narrow README rule must be present for this to pass.
        self.native_generated_fixture(register=False)

        def document(path, revision):
            if path.endswith('.json'):
                return json.dumps({'record': {'content_sha256': hashlib.sha256(revision.encode()).hexdigest()}})
            return 'command center documentation: ' + revision

        for path in PR4889_DOC_PATHS:
            self.write(path, document(path, 'before'))
        self.base = self.commit()
        for index, path in enumerate(PR4889_DOC_PATHS):
            self.write(path, document(path, f'one path {index}'))
            head = self.commit()
            with self.subTest(path=path):
                plan = self.plan(head)
                self.assertEqual(plan['scope'], 'targeted')
                self.assertEqual(plan['groups'], ['baseline'])
            self.base = head
        for path in PR4889_DOC_PATHS:
            self.write(path, document(path, 'all nine paths'))
        plan = self.plan()
        self.assertEqual(plan['scope'], 'targeted')
        self.assertEqual(plan['groups'], ['baseline'])

    def test_unknown_generated_output_expressions_keep_full_fallback(self):
        _, consumer, _ = self.native_generated_fixture()
        expressions = {
            'unknown environment': 'concat!(env!("UNKNOWN_DIR"), "/native-toolchain.txt")',
            'unregistered output': 'concat!(env!("OUT_DIR"), "/new-output.txt")',
            'computed output': 'concat!(env!("OUT_DIR"), env!("OUTPUT_NAME"))',
            'parent escape': 'concat!(env!("OUT_DIR"), "/../native-toolchain.txt")',
            'nested output': 'concat!(env!("OUT_DIR"), "/nested/native-toolchain.txt")',
        }
        for label, expression in expressions.items():
            with self.subTest(expression=label):
                self.write(consumer, f'const X: &[u8] = include_bytes!({expression});\n')
                self.base = self.commit()
                self.assert_unresolved_document(label)

    def test_generated_input_binding_requires_actual_enabled_producer(self):
        producer, _, declaration = self.native_generated_fixture()
        manifest = 'crates/oasis7_chain_runtime/Cargo.toml'
        original_manifest = Path(manifest).read_text()
        alternate = 'crates/oasis7_chain_runtime/alternate-build.rs'
        self.write(alternate, Path(producer).read_text())
        cases = (
            ('wrong digest', dict(declaration, producer_sha256='0' * 64), ''),
            ('wrong owner', dict(declaration, package='core'), ''),
            ('inactive producer', dict(declaration, producer=alternate), ''),
            ('disabled build', declaration, 'build = false\n'),
            ('redirected build', declaration, 'build = "alternate-build.rs"\n'),
        )
        for label, entry, build in cases:
            with self.subTest(binding=label):
                self.config['generated_inputs'] = [entry]
                self.write(manifest, original_manifest + build + '# ' + label + '\n')
                self.base = self.commit()
                self.assert_unresolved_document(label)

    def test_generated_producer_drift_is_checked_at_each_git_ref(self):
        producer, _, declaration = self.native_generated_fixture()
        original = self.base
        self.write(producer, Path(producer).read_text() + '\n// producer changed\n')
        changed = self.commit()
        self.assertEqual(self.plan(changed)['scope'], 'full')
        self.base = changed
        self.assert_unresolved_document('docs after producer changed')

        current = dict(declaration, producer_sha256=hashlib.sha256(
            selector.git('show', f'{changed}:{producer}')).hexdigest())
        for ref, entry, unresolved in (
                (original, declaration, set()),
                (changed, declaration, {'oasis7_chain_runtime'}),
                (original, current, {'oasis7_chain_runtime'}),
                (changed, current, set())):
            with self.subTest(ref=ref, digest=entry['producer_sha256']):
                graph = selector.cargo_graph(ref, [entry])
                self.assertEqual(graph[2].get('__unresolved_embed__', set()), unresolved)

    def test_generated_cross_package_reader_requires_every_owner_binding(self):
        producer, _, declaration = self.native_generated_fixture()
        self.write('crates/core/src/lib.rs',
                   '#[path = "../../oasis7_chain_runtime/src/host/'
                   'native_environment_capture.rs"] mod captured;\n')
        self.base = self.commit()
        self.assert_unresolved_document('only the physical source owner has a producer')
        graph = selector.cargo_graph(self.base, self.config['generated_inputs'])
        self.assertEqual(graph[2]['__unresolved_embed__'], {'core'})

        # The importing crate has its own OUT_DIR. Register its actual producer
        # separately before the same source becomes resolved for both readers.
        importing_producer = 'crates/core/build.rs'
        self.write(importing_producer, Path(producer).read_text())
        self.config['generated_inputs'].append(dict(
            declaration, package='core', producer=importing_producer))
        self.base = self.commit()
        self.write('doc/readme.md', 'each reader now has its own registered producer')
        plan = self.plan()
        self.assertEqual(plan['scope'], 'targeted')
        self.assertEqual(plan['groups'], ['baseline'])

    def test_generated_output_registration_rejects_non_leaf_names(self):
        _, _, declaration = self.native_generated_fixture()
        self.write('doc/readme.md', 'document change with invalid registration')
        head = self.commit()
        for outputs in ([], [''], ['.'], ['..'], ['nested/output.txt'],
                        ['nested\\output.txt'], ['output.txt', 'output.txt']):
            with self.subTest(outputs=outputs):
                self.config['generated_inputs'] = [dict(declaration, outputs=outputs)]
                with self.assertRaises(ValueError):
                    self.plan(head)

    def test_generated_producer_must_be_a_regular_git_blob(self):
        producer, _, declaration = self.native_generated_fixture()
        target = 'saved-producer.rs'
        self.write('crates/oasis7_chain_runtime/' + target, Path(producer).read_text())
        self.base = self.commit()
        link_blob = subprocess.check_output(
            ['git', 'hash-object', '-w', '--stdin'], input=target.encode()).decode().strip()
        # Even a digest that matches the symlink's Git bytes must not qualify it.
        self.config['generated_inputs'] = [dict(
            declaration, producer_sha256=hashlib.sha256(target.encode()).hexdigest())]
        for kind in ('symlink', 'missing'):
            with self.subTest(producer=kind):
                if kind == 'symlink':
                    self.git('update-index', '--cacheinfo', '120000', link_blob, producer)
                else:
                    self.git('update-index', '--force-remove', '--', producer)
                self.git('commit', '-qm', 'fixture producer ' + kind)
                self.base = self.git('rev-parse', 'HEAD')
                self.write('doc/readme.md', kind + ' producer must stay unresolved')
                # Preserve the synthetic index mode without requiring filesystem
                # symlink privileges on Windows or restaging the regular file.
                self.git('add', '--', 'doc/readme.md')
                self.git('commit', '-qm', 'fixture document change')
                plan = self.plan(self.git('rev-parse', 'HEAD'))
                self.assertEqual(plan['scope'], 'full')
                self.assertIn('unresolved embedded consumer input: doc/readme.md', plan['reasons'])

    def test_rule_exclusion_keeps_real_embedded_readers_and_other_rules(self):
        self.config = json.loads((HERE / 'ci-required-scope.json').read_text())
        self.config['package_groups'].update(core=['oasis7_required'], consumer=['node'])
        path = 'tools/oasis7-command-center/README.md'
        self.write(path, 'embedded tool documentation')
        self.write('crates/core/src/lib.rs',
                   'const README: &str = include_str!("../../../' + path + '");\n')
        self.base = self.commit()
        self.write(path, 'updated embedded tool documentation')
        plan = self.plan()
        self.assertEqual(plan['scope'], 'targeted')
        self.assertEqual(set(plan['groups']), {'baseline', 'rust_baseline', 'oasis7_required', 'node'})

        self.base = self.git('rev-parse', 'HEAD')
        self.config['rules'].append({'match': [path], 'groups': ['workflow_governance'],
                                     'reason': 'independent documentation contract'})
        self.write(path, 'another updated embedded tool document')
        self.assertEqual(set(self.plan()['groups']),
                         {'baseline', 'rust_baseline', 'oasis7_required', 'node', 'workflow_governance'})

    def test_known_generated_inputs_keep_control_and_unknown_input_gates(self):
        producer, _, _ = self.native_generated_fixture()
        controls = ('Cargo.lock', producer, 'rust-toolchain.toml', '.cargo/config.toml',
                    'scripts/ci-required-scope.json')
        for path in controls:
            with self.subTest(control=path):
                previous = Path(path).read_text() if Path(path).exists() else ''
                self.write(path, previous + '\n# control changed\n')
                plan = self.plan()
                self.assertEqual(plan['scope'], 'full')
                self.assertIn('shared build or CI control changed: ' + path, plan['reasons'])
                self.base = self.git('rev-parse', 'HEAD')
        self.write('unregistered.file', 'unregistered input despite full fallback')
        self.config['rules'].append({'match': ['unregistered.file'], 'exclude': ['unregistered.file'],
                                     'groups': ['command_center'], 'reason': 'excluded rule is not ownership'})
        with self.assertRaisesRegex(selector.UnknownInputsError, 'unregistered.file'):
            self.plan()

    def test_hosted_local_mock_production_guard_is_owned_before_introduction(self):
        self.config = json.loads((HERE / 'ci-required-scope.json').read_text())
        path = 'scripts/hosted-local-mock-production-guard.py'
        self.assertNotIn(path, self.git('ls-tree', '-r', '--name-only', self.base).splitlines())
        self.write(path, '# trusted-scope selector fixture')

        plan = self.plan()

        self.assertEqual(plan['scope'], 'targeted')
        self.assertEqual(set(plan['groups']), {'baseline', 'workflow_governance'})
        self.assertTrue(any(path in reason for reason in plan['reasons']))

    def test_command_center_control_stays_full(self):
        self.config = json.loads((HERE / 'ci-required-scope.json').read_text())
        for path in ('tools/oasis7-command-center/Cargo.lock',
                     'tools/oasis7-command-center/core/build.rs'):
            with self.subTest(path=path):
                self.write(path, 'unknown or shared control')
                self.assertEqual(self.plan()['scope'], 'full')
                Path(path).unlink()
                self.commit()

    def test_unknown_inputs_block_even_with_full_control_or_extra_groups(self):
        self.write('tools/new-tool/src/lib.rs', 'unregistered tool')
        self.write('unknown.file', 'unregistered input')
        self.write('Cargo.lock', 'known full control')
        head = self.commit()
        for additions in ((), tuple(self.config['groups'])):
            with self.subTest(additions=additions), self.assertRaises(ValueError) as error:
                selector.select(self.config, self.base, head, additions=additions)
            self.assertIn('tools/new-tool/src/lib.rs', str(error.exception))
            self.assertIn('unknown.file', str(error.exception))
            self.assertNotIn('Cargo.lock', str(error.exception))

    def test_unknown_deleted_and_renamed_endpoints_block(self):
        self.write('unknown-old.file', 'unique unknown file content')
        self.base = self.commit()
        self.git('mv', 'unknown-old.file', 'doc/moved.md')
        with self.assertRaisesRegex(ValueError, 'unknown-old.file'):
            self.plan()
        self.base = self.git('rev-parse', 'HEAD')
        self.git('mv', 'doc/moved.md', 'unknown-new.file')
        with self.assertRaisesRegex(ValueError, 'unknown-new.file'):
            self.plan()
        self.base = self.git('rev-parse', 'HEAD')
        Path('unknown-new.file').unlink()
        with self.assertRaisesRegex(ValueError, 'unknown-new.file'):
            self.plan()

    def test_unknown_input_cli_fails_without_success_outputs(self):
        self.write('unknown.file', 'unregistered input')
        head = self.commit()
        config = Path('config.json')
        config.write_text(json.dumps(self.config))
        process = subprocess.run([sys.executable, str(HERE / 'plan-rust-required-scope.py'),
                                  '--config', str(config), '--base-ref', self.base,
                                  '--head-ref', head, '--github-output', 'github-output',
                                  '--output', 'plan.json'], text=True, capture_output=True)
        self.assertEqual(process.returncode, 2, process.stderr)
        self.assertIn('unknown.file', process.stderr)
        self.assertIn('trusted baseline', process.stderr)
        self.assertEqual(process.stdout, '')
        self.assertFalse(Path('github-output').exists())
        self.assertFalse(Path('plan.json').exists())

    def test_workflow_uses_trusted_rules_and_emits_no_success_on_unknown(self):
        for name in ('plan-rust-required-scope.py', 'ci-required-result.py'):
            self.write('scripts/' + name, (HERE / name).read_text())
        self.write('scripts/ci-required-scope.json', json.dumps(self.config))
        self.base = self.commit()
        self.write('unknown.file', 'unregistered input')
        candidate = dict(self.config, rules=[*self.config['rules'],
                         {'match': ['unknown.file'], 'groups': [], 'reason': 'self exemption'}])
        self.write('scripts/ci-required-scope.json', json.dumps(candidate))
        head = self.commit()
        workflow = (HERE.parent / '.github/workflows/rust.yml').read_text()
        step = workflow.split('        name: Select using trusted baseline Git data', 1)[1]
        script = textwrap.dedent(step.split('        run: |\n', 1)[1]
                                .split('      - id: formal-select', 1)[0])
        with tempfile.TemporaryDirectory() as temporary:
            runner = Path(temporary)
            (runner / 'bin').mkdir()
            (runner / 'bin/python3').symlink_to(sys.executable)
            env = dict(os.environ, BASE_SHA=self.base, SOURCE_SHA=head, EVENT_NAME='pull_request',
                       RUNNER_TEMP=str(runner), GITHUB_OUTPUT=str(runner / 'outputs'),
                       GITHUB_STEP_SUMMARY=str(runner / 'summary'),
                       PATH=str(runner / 'bin') + os.pathsep + os.environ['PATH'])
            process = subprocess.run(['bash', '-c', script], env=env, text=True, capture_output=True)
            self.assertNotEqual(process.returncode, 0)
            self.assertIn('unknown.file', process.stderr)
            self.assertFalse((runner / 'outputs').exists())
            self.assertFalse((runner / 'selection.json').exists())

    def test_docs_baseline_only(self):
        self.write('doc/readme.md', 'changed')
        self.assertEqual(self.plan()['groups'], ['baseline'])

    def test_optional_target_dev_reverse_consumer(self):
        self.write('crates/core/src/lib.rs', '// changed')
        self.assertEqual(set(self.plan()['groups']), {'baseline', 'rust_baseline', 'core', 'consumer'})

    def test_deleted_dependency_uses_old_graph(self):
        self.write('crates/consumer/Cargo.toml', '[package]\nname="consumer"\nversion="0.1.0"\n')
        self.write('crates/core/src/lib.rs', '// changed')
        head = self.commit()
        affected = selector.reverse_consumers({'core'}, selector.cargo_graph(self.base)[1])
        self.assertIn('consumer', affected)
        self.assertEqual(self.plan(head)['scope'], 'full')  # manifest changes always conservative

    def test_new_reverse_consumer_and_renamed_dependency(self):
        self.write('crates/other/Cargo.toml', '[package]\nname="other"\nversion="0.1.0"\n[dependencies]\nalias={package="core",path="../core"}\n')
        head = self.commit()
        self.assertIn('other', selector.reverse_consumers({'core'}, selector.cargo_graph(head)[1]))

    def test_move_covers_both_packages(self):
        self.write('crates/core/src/moved.rs', 'unique move contents')
        self.base = self.commit()
        self.git('mv', 'crates/core/src/moved.rs', 'crates/other/src/moved.rs')
        self.assertTrue({'core', 'consumer', 'other'}.issubset(self.plan()['groups']))

    def test_deleted_source_old_owner(self):
        Path('crates/core/src/lib.rs').unlink()
        self.assertIn('consumer', self.plan()['groups'])

    def test_unusual_path_nul_diff_json_safe(self):
        path = 'crates/core/src/space;line\nname.rs'
        self.write(path, 'contents')
        head = self.commit()
        self.assertIn(path, selector.changed_paths(self.base, head))
        self.assertIn('consumer', self.plan(head)['groups'])
        self.assertNotIn('\n', json.dumps(self.plan(head)))

    def test_embedded_document_consumed(self):
        self.write('crates/core/src/lib.rs', 'const D:&str=include_str!("../../../doc/readme.md");')
        self.base = self.commit()
        self.write('doc/readme.md', 'changed consumer input')
        self.assertIn('consumer', self.plan()['groups'])

    def test_build_lock_control_and_graph_failure_full(self):
        for path in ['Cargo.lock', 'crates/core/build.rs', '.github/workflows/test.yml']:
            with self.subTest(path=path):
                self.write(path, 'change')
                self.assertEqual(self.plan()['scope'], 'full')
        self.assertEqual(self.plan('absent-object')['scope'], 'full')

    def test_tested_merge_extra_paths(self):
        self.write('doc/readme.md', 'source doc')
        source = self.commit()
        self.write('crates/other/src/lib.rs', '// actual merge')
        tested = self.commit()
        plan = selector.select(self.config, self.base, source, tested)
        self.assertIn('other', plan['groups'])
        self.assertEqual(plan['scope'], 'full')  # unverifiable merge topology

    def merge_fixture(self, target_path='Cargo.lock', resolution=None, target_content='// target advance'):
        self.git('checkout', '-qb', 'source', self.base)
        self.write('doc/readme.md', 'source doc')
        source = self.commit()
        self.git('checkout', '-qb', 'target', self.base)
        self.write(target_path, target_content)
        target = self.commit()
        self.git('merge', '--no-ff', '-qm', 'merge fixture', source)
        if resolution:
            self.write(*resolution)
            self.git('add', '-A')
            self.git('commit', '--amend', '-qm', 'merge resolution')
        return source, target, self.git('rev-parse', 'HEAD')

    def test_target_lock_advance_does_not_expand_docs_merge(self):
        source, target, tested = self.merge_fixture()
        for base in (self.base, target):
            with self.subTest(base=base):
                plan = selector.select(self.config, base, source, tested)
                self.assertEqual(plan['scope'], 'targeted')
                self.assertEqual(plan['groups'], ['baseline'])

    def test_merge_resolution_keeps_extra_consumer(self):
        source, target, tested = self.merge_fixture(
            resolution=('crates/core/src/lib.rs', '// resolution'))
        plan = selector.select(self.config, self.base, source, tested)
        self.assertEqual(plan['scope'], 'targeted')
        self.assertEqual(set(plan['groups']), {'baseline', 'rust_baseline', 'core', 'consumer'})

    def test_merge_resolution_lock_change_still_full(self):
        source, target, tested = self.merge_fixture(resolution=('Cargo.lock', '// resolution'))
        self.assertEqual(selector.select(self.config, self.base, source, tested)['scope'], 'full')

    def test_wrong_merge_source_and_unrelated_base_fail_closed(self):
        source, target, tested = self.merge_fixture()
        self.assertEqual(selector.select(self.config, self.base, target, tested)['scope'], 'full')
        self.git('checkout', '-qb', 'unrelated', self.base)
        self.write('doc/unrelated.md', 'unrelated event base')
        unrelated = self.commit()
        self.assertEqual(selector.select(self.config, unrelated, source, tested)['scope'], 'full')

    def test_target_graph_keeps_new_reverse_consumer(self):
        source, target, tested = self.merge_fixture(
            'crates/other/Cargo.toml', ('crates/core/src/lib.rs', '// resolution'),
            '[package]\nname="other"\nversion="0.1.0"\n[dependencies]\ncore={path="../core"}\n')
        plan = selector.select(self.config, self.base, source, tested)
        self.assertEqual(plan['scope'], 'targeted')
        self.assertIn('other', plan['groups'])

    def test_source_lock_change_still_full_on_advanced_target(self):
        source, target, tested = self.merge_fixture('crates/other/src/lib.rs')
        self.git('checkout', 'source')
        self.write('Cargo.lock', '// source lock')
        source = self.commit()
        self.git('checkout', 'target')
        self.git('merge', '--no-ff', '-qm', 'merge fixture', source)
        tested = self.git('rev-parse', 'HEAD')
        self.assertEqual(selector.select(self.config, target, source, tested)['scope'], 'full')

    def test_additions_only_expand_and_groups_only(self):
        self.write('doc/readme.md', 'changed')
        head = self.commit()
        plan = selector.select(self.config, self.base, head, additions=['other'])
        self.assertEqual(set(plan['groups']), {'baseline', 'other'})
        self.assertNotIn('matrices', plan)
        with self.assertRaises(ValueError):
            selector.select(self.config, self.base, head, additions=['unknown'])

    def test_github_output_contains_plan_and_flags_without_matrix_outputs(self):
        self.write('doc/readme.md', 'changed')
        head = self.commit()
        Path('config.json').write_text(json.dumps(self.config))
        arguments = ['selector', '--base-ref', self.base, '--head-ref', head,
                     '--config', 'config.json', '--github-output', 'outputs.txt']
        with patch.object(sys, 'argv', arguments), contextlib.redirect_stdout(io.StringIO()):
            selector.main()
        outputs = dict(line.split('=', 1) for line in Path('outputs.txt').read_text().splitlines())
        plan = json.loads(outputs['plan'])
        self.assertEqual(set(plan), {'groups', 'resources', 'reasons', 'scope',
                                    'workspace_support_profiles', 'workspace_support_profile_version'})
        self.assertEqual(outputs['run_baseline'], 'true')
        self.assertEqual(outputs['run_other'], 'false')
        self.assertFalse(any(name.startswith('matrix') for name in outputs))

    def test_real_tool_rules_select_only_related_groups(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        cases = {
            'resource-cleanup-executor.py': {'baseline', 'workflow_governance', 'fleet_health'},
            'new-task-worktree.sh': {'baseline', 'cargo_tooling_contracts', 'fleet_health'},
            'worktree-gc-report.sh': {'baseline', 'workflow_governance'},
            'pr-review-thread-closeout.py': {'baseline', 'workflow_governance'},
            'cargo-dev.sh': {'baseline', 'cargo_tooling_contracts', 'command_center'},
        }
        for filename, expected in cases.items():
            with self.subTest(filename=filename):
                self.write('scripts/' + filename, 'fixture')
                head = self.commit()
                plan = selector.select(config, self.base, head)
                self.assertEqual(set(plan['groups']), expected)
                self.assertEqual(plan['scope'], 'targeted')
                self.base = head


    def test_local_signer_precise_consumers(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        common = {'baseline', 'packaging_contracts', 'macos_package_contract', 'fleet_health'}
        cases = {
            'scripts/local-signer/tests/test_future.py': common,
            'doc/p2p/blockchain/local-file-signing-backend.runbook.md': common,
            **{'scripts/local-signer/' + name: common | {'workspace_support'}
               for name in ['installer.py', 'macos_host.py', 'install-release.py', 'package-release.py']},
        }
        for path, expected in cases.items():
            with self.subTest(path=path):
                self.write(path, 'fixture')
                head = self.commit()
                plan = selector.select(config, self.base, head)
                self.assertEqual(set(plan['groups']), expected)
                self.assertEqual(plan['scope'], 'targeted')
                self.base = head
        self.write('scripts/local-signer/new-production.py', 'fixture')
        with self.assertRaisesRegex(ValueError, 'scripts/local-signer/new-production.py'):
            selector.select(config, self.base, self.commit())

    def test_local_signer_deletion_and_rename(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        self.write('scripts/local-signer/installer.py', 'unique production fixture')
        self.base = self.commit()
        Path('scripts/local-signer/tests').mkdir()
        self.git('mv', 'scripts/local-signer/installer.py', 'scripts/local-signer/tests/test_moved.py')
        head = self.commit()
        self.assertEqual(set(selector.select(config, self.base, head)['groups']),
                         {'baseline', 'packaging_contracts', 'macos_package_contract', 'fleet_health', 'workspace_support'})
        self.base = head
        Path('scripts/local-signer/tests/test_moved.py').unlink()
        self.assertEqual(set(selector.select(config, self.base, self.commit())['groups']),
                         {'baseline', 'packaging_contracts', 'macos_package_contract', 'fleet_health'})

    def test_local_signer_crate_keeps_reverse_consumers(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        self.write('crates/oasis7_local_signer/Cargo.toml', '[package]\nname="oasis7_local_signer"\nversion="0.1.0"\n')
        self.write('crates/oasis7_local_signer/src/lib.rs', '')
        self.write('crates/consumer/Cargo.toml', '[package]\nname="consumer"\nversion="0.1.0"\n[dependencies]\noasis7_local_signer={path="../oasis7_local_signer"}\n')
        config['package_groups']['consumer'] = ['node']
        self.base = self.commit()
        self.write('crates/oasis7_local_signer/src/lib.rs', '// changed')
        plan = selector.select(config, self.base, self.commit())
        self.assertEqual(plan['scope'], 'targeted')
        self.assertTrue({'workspace_support', 'node', 'rust_baseline', 'packaging_contracts',
                         'macos_package_contract', 'fleet_health'}.issubset(plan['groups']))

    def test_local_signer_combination_and_security_overlay(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        for path in ['scripts/local-signer/installer.py', 'scripts/local-signer/tests/test_fixture.py',
                     'doc/p2p/blockchain/local-file-signing-backend.runbook.md', 'scripts/security/fixture.py']:
            self.write(path, 'fixture')
        self.assertEqual(set(selector.select(config, self.base, self.commit())['groups']),
                         {'baseline', 'packaging_contracts', 'macos_package_contract', 'fleet_health',
                          'workspace_support', 'workflow_governance'})

    def test_agent_api_and_harness_route_through_support_and_oasis7_consumers(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        packages = ('oasis7_agent_api', 'oasis7_agent_harness', 'oasis7', 'oasis7_chain_runtime')
        for package in packages:
            self.write(f'crates/{package}/Cargo.toml',
                       f'[package]\nname="{package}"\nversion="0.1.0"\n')
            self.write(f'crates/{package}/src/lib.rs', '')
        base = self.commit()
        manifests = {name: (f'crates/{name}', {}) for name in packages}
        reverse = {
            'oasis7_agent_api': {'oasis7_agent_harness', 'oasis7'},
            'oasis7_agent_harness': {'oasis7'},
            'oasis7': {'oasis7_chain_runtime'}, 'oasis7_chain_runtime': set(),
        }
        graph = (manifests, reverse, {})
        expected = {'baseline', 'rust_baseline', 'workspace_support',
                    'oasis7_required', 'launcher_web', 'formal_process'}
        with patch.object(selector, 'cargo_graph', return_value=graph):
            for package in ('oasis7_agent_api', 'oasis7_agent_harness'):
                with self.subTest(package=package):
                    self.write(f'crates/{package}/src/lib.rs', '// changed')
                    plan = selector.select(config, base, self.commit())
                    self.assertEqual(plan['scope'], 'targeted')
                    expected_profiles = {'oasis7_agent_harness'}
                    if package == 'oasis7_agent_api':
                        expected_profiles.add('oasis7_agent_api')
                    self.assertEqual(set(plan['workspace_support_profiles']), expected_profiles)
                    self.assertEqual(set(plan['groups']), expected)
                    base = self.git('rev-parse', 'HEAD')

    def test_game_client_routes_through_support_and_server_consumer(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        packages = ('oasis7_client_api', 'oasis7_game_client', 'oasis7', 'oasis7_chain_runtime')
        for package in packages:
            self.write(f'crates/{package}/Cargo.toml',
                       f'[package]\nname="{package}"\nversion="0.1.0"\n')
            self.write(f'crates/{package}/src/lib.rs', '')
        base = self.commit()
        manifests = {name: (f'crates/{name}', {}) for name in packages}
        graph = (manifests, {
            'oasis7_client_api': {'oasis7_game_client', 'oasis7'},
            'oasis7_game_client': {'oasis7'}, 'oasis7': {'oasis7_chain_runtime'}, 'oasis7_chain_runtime': set(),
        }, {})
        with patch.object(selector, 'cargo_graph', return_value=graph):
            for package in ('oasis7_client_api', 'oasis7_game_client'):
                with self.subTest(package=package):
                    self.write(f'crates/{package}/src/lib.rs', '// changed')
                    plan = selector.select(config, base, self.commit())
                    self.assertEqual(plan['scope'], 'targeted')
                    expected_profiles = {'oasis7_game_client'}
                    if package == 'oasis7_client_api':
                        expected_profiles.add('oasis7_client_api')
                    self.assertEqual(set(plan['workspace_support_profiles']), expected_profiles)
                    self.assertEqual(set(plan['groups']), {'baseline', 'rust_baseline',
                                     'workspace_support', 'oasis7_required', 'launcher_web', 'formal_process'})
                    base = self.git('rev-parse', 'HEAD')

    def test_provider_tests_are_local_and_library_changes_reach_server_consumers(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        packages = ('oasis7_agent_harness', 'oasis7_model_provider', 'oasis7', 'oasis7_chain_runtime')
        for package in packages:
            self.write(f'crates/{package}/Cargo.toml',
                       f'[package]\nname="{package}"\nversion="0.1.0"\n')
            self.write(f'crates/{package}/src/lib.rs', '')
            self.write(f'crates/{package}/tests/transport.rs', '')
        base = self.commit()
        manifests = {name: (f'crates/{name}', {}) for name in packages}
        graph = (manifests, {
            'oasis7_agent_harness': {'oasis7_model_provider', 'oasis7'},
            'oasis7_model_provider': {'oasis7'}, 'oasis7': {'oasis7_chain_runtime'}, 'oasis7_chain_runtime': set(),
        }, {})
        with patch.object(selector, 'cargo_graph', return_value=graph):
            for package, path in [('oasis7_model_provider', 'tests/transport.rs'),
                                  ('oasis7_agent_harness', 'src/lib.rs')]:
                with self.subTest(package=package):
                    self.write(f'crates/{package}/{path}', '// changed')
                    plan = selector.select(config, base, self.commit())
                    self.assertEqual(plan['scope'], 'targeted')
                    expected_profiles = {'oasis7_model_provider'}
                    if package == 'oasis7_agent_harness':
                        expected_profiles.add('oasis7_agent_harness')
                    self.assertEqual(set(plan['workspace_support_profiles']), expected_profiles)
                    expected = {'baseline', 'rust_baseline', 'workspace_support'}
                    if package == 'oasis7_agent_harness':
                        expected.update({'oasis7_required', 'launcher_web', 'formal_process'})
                    self.assertEqual(set(plan['groups']), expected)
                    base = self.git('rev-parse', 'HEAD')

    def test_deleted_agent_harness_source_uses_base_consumer_graph(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        packages = ('oasis7_agent_api', 'oasis7_agent_harness', 'oasis7', 'oasis7_chain_runtime')
        for package in packages:
            self.write(f'crates/{package}/Cargo.toml',
                       f'[package]\nname="{package}"\nversion="0.1.0"\n')
            self.write(f'crates/{package}/src/lib.rs', '')
        base = self.commit()
        old_manifests = {name: (f'crates/{name}', {}) for name in packages}
        old_reverse = {
            'oasis7_agent_api': {'oasis7_agent_harness', 'oasis7'},
            'oasis7_agent_harness': {'oasis7'},
            'oasis7': {'oasis7_chain_runtime'}, 'oasis7_chain_runtime': set(),
        }
        new_manifests = {'oasis7': ('crates/oasis7', {}), 'oasis7_chain_runtime': ('crates/oasis7_chain_runtime', {})}
        new_reverse = {'oasis7': {'oasis7_chain_runtime'}, 'oasis7_chain_runtime': set()}
        Path('crates/oasis7_agent_harness/src/lib.rs').unlink()
        head = self.commit()

        def graph_for(ref, generated_inputs=()):
            return ((old_manifests, old_reverse, {}) if ref == base
                    else (new_manifests, new_reverse, {}))

        with patch.object(selector, 'cargo_graph', side_effect=graph_for):
            plan = selector.select(config, base, head)
        self.assertEqual(plan['scope'], 'targeted')
        self.assertEqual(set(plan['groups']), {'baseline', 'rust_baseline', 'workspace_support',
                                               'oasis7_required', 'launcher_web', 'formal_process'})


if __name__ == '__main__':
    unittest.main()
