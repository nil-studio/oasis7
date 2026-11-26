#!/usr/bin/env python3
import contextlib
import copy
import fnmatch
import inspect
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

    def test_portable_cache_sdk_inputs_require_exact_trusted_ownership(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        old = copy.deepcopy(config)
        old['rules'] = [rule for rule in old['rules'] if rule.get('reason') != 'portable_cache_sdk_inputs']
        with patch.object(selector, 'cargo_graph', return_value=({}, {}, {})):
            for path in ('vendor/actions-cache-explicit-context/index.mjs',
                         'vendor/actions-cache-explicit-context/licenses.txt'):
                for value in ('first SDK input', 'modified SDK input', None):
                    base = self.git('rev-parse', 'HEAD')
                    if value is None:
                        Path(path).unlink()
                    else:
                        self.write(path, value)
                    head = self.commit()
                    plan = selector.select(config, base, head)
                    self.assertEqual(plan['scope'], 'targeted')
                    self.assertEqual(set(plan['groups']), {'baseline', 'workflow_governance'})
                    self.assertIn('portable_cache_sdk_inputs: ' + path, plan['reasons'])
                    # The tested tree already contains the new declaration;
                    # an old immutable authority must still refuse its inputs.
                    with self.assertRaises(selector.UnknownInputsError):
                        selector.select(old, base, head)

    def test_portable_cache_sdk_unknown_neighbors_and_rename_endpoints_stay_closed(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        with patch.object(selector, 'cargo_graph', return_value=({}, {}, {})):
            for path in ('vendor/actions-cache-explicit-context/extra.mjs',
                         'vendor/actions-cache-explicit-context/nested/index.mjs',
                         'vendor/actions-cache-explicit-context/extra.txt',
                         'vendor/actions-cache-other/index.mjs'):
                base = self.git('rev-parse', 'HEAD')
                self.write(path, 'unknown input')
                with self.assertRaises(selector.UnknownInputsError):
                    selector.select(config, base, self.commit())
            known = 'vendor/actions-cache-explicit-context/licenses.txt'
            original = 'vendor/actions-cache-explicit-context/index.mjs'
            unknown = 'vendor/actions-cache-explicit-context/unknown.mjs'
            self.write(original, 'unique rename fixture'); base = self.commit()
            self.git('mv', original, known)
            plan = selector.select(config, base, self.commit())
            self.assertEqual(set(plan['groups']), {'baseline', 'workflow_governance'})
            for source, destination in ((known, unknown), (unknown, known)):
                base = self.git('rev-parse', 'HEAD')
                self.git('mv', source, destination)
                with self.assertRaises(selector.UnknownInputsError):
                    selector.select(config, base, self.commit())

    def test_quic_patch_inputs_select_network_consumers_without_self_authority(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        old = copy.deepcopy(config)
        old['rules'] = [rule for rule in old['rules']
                             if rule.get('reason') != 'quic_patch_consumers']
        for path in ('vendor/libp2p-quic-0.15.0/src/connection/connecting.rs',
                     'vendor/libp2p-quic-0.15.0/src/transport.rs',
                     'vendor/libp2p-quic-0.15.0/LICENSE',
                     'vendor/libp2p-quic-0.15.0/.gitignore',
                     'vendor/libp2p-quic-0.15.0/upstream-source-sha256.json',
                     'vendor/libp2p-quic-0.15.0/local-patch-sha256.json'):
            with self.subTest(path=path):
                base = self.git('rev-parse', 'HEAD')
                self.write(path, 'QUIC patch input')
                head = self.commit()
                plan = selector.select(config, base, head)
                self.assertEqual(plan['scope'], 'targeted')
                self.assertTrue({'rust_baseline', 'net', 'node'}.issubset(plan['groups']))
                with self.assertRaises(selector.UnknownInputsError):
                    selector.select(old, base, head)
        for path in ('vendor-libp2p-quic-0.14.0/src/transport.rs',
                     'vendor-libp2p-quic-0.15.0/src/transport.rs',
                     'vendor/libp2p-quic-0.16.0/src/transport.rs',
                     'vendor/unregistered.json'):
            with self.subTest(path=path):
                base = self.git('rev-parse', 'HEAD')
                self.write(path, 'unregistered version')
                with self.assertRaises(selector.UnknownInputsError):
                    selector.select(config, base, self.commit())

    def test_fixed_swarm_patch_sources_select_network_consumers(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        for path in ('vendor/libp2p-swarm-0.49.0/src/lib.rs',
                     'vendor/libp2p-swarm-0.49.0/LICENSE',
                     'vendor/libp2p-swarm-0.49.0/upstream-source-sha256.json'):
            with self.subTest(path=path):
                base = self.git('rev-parse', 'HEAD')
                self.write(path, 'fixed Swarm patch source')
                plan = selector.select(config, base, self.commit())
                self.assertEqual(plan['scope'], 'targeted')
                self.assertTrue({'rust_baseline', 'net', 'node'}.issubset(plan['groups']))
        for path in ('vendor-libp2p-swarm-0.48.0/src/lib.rs',
                     'vendor-libp2p-swarm-0.48.0/unregistered.json',
                     'vendor-libp2p-swarm-0.49.0/src/lib.rs',
                     'vendor-libp2p-swarm-0.49.0/LICENSE',
                     'vendor/libp2p-swarm-0.49.0/unregistered.json',
                     'vendor/libp2p-swarm-0.50.0/src/lib.rs',
                     'vendor/libp2p-swarm-0.50.0/LICENSE'):
            with self.subTest(path=path):
                base = self.git('rev-parse', 'HEAD')
                self.write(path, 'unregistered input')
                with self.assertRaises(selector.UnknownInputsError):
                    selector.select(config, base, self.commit())

    def test_rendezvous_patch_inputs_select_network_consumers(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        for path in ('scripts/libp2p-security-source.test.py',
                     '.dockerignore', 'docker/wasm-builder/Dockerfile'):
            with self.subTest(path=path):
                case_base = self.git('rev-parse', 'HEAD')
                self.write(path, 'network patch input')
                plan = selector.select(config, case_base, self.commit())
                expected = ({'net'} if path.startswith('scripts/')
                            else {'net', 'packaging_contracts'})
                self.assertTrue(expected.issubset(plan['groups']))
                self.assertEqual(plan['scope'], 'targeted')

    def test_if_watch_patch_inputs_select_network_consumers(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        for path in ('vendor/if-watch-3.2.2/src/linux.rs',
                     'vendor/if-watch-3.2.2/upstream-source-sha256.json',
                     'vendor/if-watch-3.2.2/Cargo.toml.orig',
                     'vendor/if-watch-3.2.2/Cargo.toml'):
            with self.subTest(path=path):
                case_base = self.git('rev-parse', 'HEAD')
                content = ('[package]\nname="if-watch"\nversion="3.2.2"\n'
                           if path.endswith('Cargo.toml') else 'network patch input')
                self.write(path, content)
                plan = selector.select(config, case_base, self.commit())
                self.assertTrue({'rust_baseline', 'net', 'node'}.issubset(plan['groups']))
                # Manifest controls intentionally retain the existing full scope.
                self.assertEqual(plan['scope'], 'full' if path.endswith('Cargo.toml') else 'targeted')
        case_base = self.git('rev-parse', 'HEAD')
        self.write('vendor/if-watch-3.2.3/src/linux.rs', 'unregistered version')
        with self.assertRaises(selector.UnknownInputsError):
            selector.select(config, case_base, self.commit())

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

    def test_invalid_merge_cannot_bypass_unknown_path(self):
        self.write('unknown.file', 'unknown despite invalid merge topology')
        head = self.commit()
        with self.assertRaisesRegex(selector.UnknownInputsError, 'unknown.file'):
            selector.select(self.config, self.base, head, test_head=self.base)

    def test_parse_uncertainty_cannot_register_unknown_path(self):
        self.write('crates/core/Cargo.toml', 'broken manifest')
        self.write('unknown.file', 'must remain unknown')
        head = self.commit()
        with self.assertRaisesRegex(selector.UnknownInputsError, 'unknown.file'):
            selector.select(self.config, self.base, head)
        Path('unknown.file').unlink()
        plan = selector.select(self.config, self.base, self.commit())
        self.assertEqual(plan['architecture']['mode'], 'full')
        self.assertEqual(plan['scope'], 'full')

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
        for name in ('plan-rust-required-scope.py', 'ci-required-result.py', 'ci-formal-plan.py',
                     'ci-formal-process-suites.json'):
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
        step = step.split('\n      - ', 1)[0]
        script = textwrap.dedent(step.split('        run: |\n', 1)[1])
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

    def test_real_wasm_fixture_sources_do_not_expand_unrelated_documents(self):
        # These production test strings caused the docs-only full fallback in #5177.
        for name in ('source_inputs_tests.rs', 'producer_recipe.rs', 'producer_recipe_guard.rs'):
            source = HERE.parent / 'crates/oasis7_wasm_build/src' / name
            self.write('crates/core/src/' + name, source.read_text())
        self.base = self.commit()
        self.write('doc/readme.md', 'unrelated document with real fixture sources present')
        plan = self.plan()
        self.assertEqual(plan['scope'], 'targeted')
        self.assertEqual(plan['groups'], ['baseline'])

    def test_rust_quoted_examples_and_comments_are_not_embedded_inputs(self):
        cases = {
            'ordinary': r'const S: &str = "include_str!(\"missing.txt\"); include_bytes!(env!(\"NO\"));";',
            'byte_and_c': r'const B: &[u8] = b"include_bytes!(\"missing.txt\")"; const C: &core::ffi::CStr = c"include_str!(\"missing.txt\")";',
            'raw_delimiters': r'''const A: &str = r###"include_str!(env!("NO")); "# include_bytes!(env!("NO")); "##"###;
                const B: &[u8] = br##"include_bytes!(env!("NO"))"#"##;
                const C: &core::ffi::CStr = cr#"include_str!(env!("NO"))"#;''',
            'nested_comments': '''/* outer include_str!(env!("NO"))
                /* nested include_bytes!(env!("NO")) */ still outer */
                // include_bytes!(env!("NO"))
                /// include_str!(env!("NO"))
                pub fn documented() {}''',
            'characters_and_lifetimes': r'''fn example<'a>(value: &'a str) -> &'a str {
                let _ = '"'; let _ = b'\''; let _ = '\u{22}';
                let _ = "include_str!(\"missing.txt\")"; value
            }''',
            'identifier_boundary': 'not_include_str!(env!("NOT_AN_INCLUDE"));',
            'unicode_identifier_boundary': 'e\u0301include_str!(env!("NOT_AN_INCLUDE"));',
            'quoted_real_path': r'''const EXAMPLE: &str = r##"include_str!("../../../doc/readme.md")"##;''',
            'literal_suffixes': r'''discard!("example"include_str!(env!("NO")));
                discard!(r#"example"#include_bytes!(env!("NO")));
                discard!(b'x'include_str!(env!("NO")));
                discard!(1include_str!(env!("NO")));
                discard!(1.0include_bytes!(env!("NO")));''',
        }
        for label, source in cases.items():
            with self.subTest(case=label):
                self.write('crates/core/src/lib.rs', source)
                self.base = self.commit()
                self.write('doc/readme.md', 'unrelated document: ' + label)
                plan = self.plan()
                self.assertEqual(plan['scope'], 'targeted')
                self.assertEqual(plan['groups'], ['baseline'])

    def test_real_include_after_quoted_examples_keeps_document_consumers(self):
        source = r'''fn sample<'a>(value: &'a str) -> &'a str {
            let _ = '"'; let _ = b'\''; let _ = r##"include_str!(env!("NO"))"##;
            value
        }
        const DOCUMENT: &str = include_str!("../../../doc/readme.md");'''
        self.write('crates/core/src/lib.rs', source)
        self.base = self.commit()
        self.write('doc/readme.md', 'real embedded document still changes consumers')
        plan = self.plan()
        self.assertEqual(plan['scope'], 'targeted')
        self.assertEqual(set(plan['groups']), {'baseline', 'rust_baseline', 'core', 'consumer'})

    def test_real_byte_and_raw_path_literals_keep_fixture_consumers(self):
        for literal in ('b"doc/readme.md"', 'br#"doc/readme.md"#',
                        'c"doc/readme.md"', 'cr#"doc/readme.md"#'):
            with self.subTest(literal=literal):
                self.write('crates/core/src/lib.rs', f'const PATH = {literal};')
                self.base = self.commit()
                self.write('doc/readme.md', 'fixture path: ' + literal)
                self.assertEqual(set(self.plan()['groups']),
                                 {'baseline', 'rust_baseline', 'core', 'consumer'})

    def test_include_after_field_colon_is_not_a_foreign_macro(self):
        for source in ('const S = Holder { text: include_str!("data.txt") };',
                       'json!({"value": include_str!("data.txt")});'):
            with self.subTest(source=source):
                self.write('crates/core/src/lib.rs', source)
                self.write('crates/core/src/data.txt', 'actual embedded input')
                self.base = self.commit()
                self.write('doc/readme.md', 'unrelated document: ' + source)
                self.assertEqual(self.plan()['groups'], ['baseline'])
                self.base = self.git('rev-parse', 'HEAD')
                self.write('crates/core/src/data.txt', source)
                self.assertEqual(set(self.plan()['groups']),
                                 {'baseline', 'rust_baseline', 'core', 'consumer'})

    def test_unknown_embed_diagnostics_identify_source_and_owner(self):
        self.write('crates/core/src/lib.rs', '\nconst D: &str = include_str!(env!("UNKNOWN"));')
        self.base = self.commit()
        self.write('doc/readme.md', 'unrelated document')
        plan = self.plan()
        self.assertEqual(plan['scope'], 'full')
        self.assertIn('unresolved Rust embed: crates/core/src/lib.rs:2 (core): '
                      'include_str!(env!("UNKNOWN"))', plan['reasons'])

    def test_standard_library_include_paths_keep_precise_consumers(self):
        for namespace in ('std::', 'core::', '::std::', '::core::'):
            with self.subTest(namespace=namespace):
                self.write('doc/embedded.md', 'actual embedded document')
                self.write('crates/core/src/lib.rs',
                           f'const D: &str = {namespace}include_str!("../../../doc/embedded.md");')
                self.base = self.commit()
                self.write('doc/readme.md', 'unrelated document: ' + namespace)
                self.assertEqual(self.plan()['groups'], ['baseline'])
                self.base = self.git('rev-parse', 'HEAD')
                self.write('doc/embedded.md', 'changed: ' + namespace)
                self.assertEqual(set(self.plan()['groups']),
                                 {'baseline', 'rust_baseline', 'core', 'consumer'})
        self.write('crates/core/src/lib.rs',
                   'const D: &str = foreign::std::include_str!("../../../doc/embedded.md");')
        self.base = self.commit()
        self.assert_unresolved_document('foreign macro is not a registered builtin')

    def test_real_include_token_variants_never_skip_document_consumers(self):
        sources = (
            'const D: &str = include_str /* separator */ ! ("../../../doc/readme.md");',
            'const D: &str = include_str!["../../../doc/readme.md"];',
            'const D: &str = include_str!{"../../../doc/readme.md"};',
            'const D: &str = std::include_str!(r"../../../doc/readme.md");',
            'macro_rules! document { () => { include_str!("../../../doc/readme.md") }; }',
        )
        for index, source in enumerate(sources):
            with self.subTest(source=source):
                self.write('crates/core/src/lib.rs', source)
                self.base = self.commit()
                self.write('doc/readme.md', f'actual include variant {index}')
                # Unsupported expression forms may stay conservative, but cannot skip readers.
                self.assertTrue({'core', 'consumer'}.issubset(self.plan()['groups']))

    def test_real_dynamic_include_survives_literal_and_comment_scanning(self):
        sources = (
            r'const EXAMPLE: &str = "include_str!(\"fixture\")"; const D: &str = include_str!(env!("UNKNOWN"));',
            r'''fn f<'a>(value: &'a str) -> &'static str {
                let _ = '"'; let _ = b'\''; let _ = value; include_str!(env!("UNKNOWN"))
            }''',
            'opaque!(include_bytes!(env!("UNKNOWN")));',
            'macro_rules! dynamic { () => { include_str!(env!("UNKNOWN")) }; }',
            'const D: &str = include_str /* separator */ ! [env!("UNKNOWN")];',
        )
        for index, source in enumerate(sources):
            with self.subTest(source=source):
                self.write('crates/core/src/lib.rs', source)
                self.base = self.commit()
                self.write('doc/readme.md', f'unrelated document with dynamic include {index}')
                self.assertEqual(self.plan()['scope'], 'full')

    def test_registered_generated_inputs_ignore_fixture_strings_without_hiding_dynamic_calls(self):
        _, consumer, _ = self.native_generated_fixture()
        original = Path(consumer).read_text()
        example = r'const EXAMPLE: &str = "include_str!(concat!(env!(\"OUT_DIR\"), \"/not-registered\"))";'
        self.write(consumer, original + '\n' + example + '\n')
        self.base = self.commit()
        self.write('doc/readme.md', 'registered real outputs plus a fixture string')
        self.assertEqual(self.plan()['groups'], ['baseline'])
        self.write(consumer, original + '\n' + example + '\nconst UNKNOWN: &str = include_str!(env!("OTHER_INPUT"));\n')
        self.base = self.commit()
        self.assert_unresolved_document('real dynamic call cannot be cancelled by registered outputs')

    def test_macro_namespace_parameter_cannot_claim_registered_generated_output(self):
        _, consumer, _ = self.native_generated_fixture()
        original = Path(consumer).read_text()
        for namespace in ('std', 'core'):
            with self.subTest(namespace=namespace):
                dynamic = ('macro_rules! input { ($' + namespace + ':ident) => { $' + namespace
                           + '::include_str!(concat!(env!("OUT_DIR"), "/native-toolchain.txt")) }; }')
                self.write(consumer, original + '\n' + dynamic + '\n')
                self.base = self.commit()
                self.assert_unresolved_document('namespace parameter: ' + namespace)

    def test_scoped_github_controls_select_only_governance(self):
        self.config = json.loads((HERE / 'ci-required-scope.json').read_text())
        paths = (
            '.github/ISSUE_TEMPLATE/code-development.yml',
            '.github/ISSUE_TEMPLATE/config.yml',
            '.github/ISSUE_TEMPLATE/decision.yml',
            '.github/ISSUE_TEMPLATE/defect.yml',
            '.github/ISSUE_TEMPLATE/document.yml',
            '.github/ISSUE_TEMPLATE/test.yml',
            '.github/instructions/pr-review.instructions.md',
            '.github/pull_request_template.md',
            '.github/workflows/codeql.yml',
            '.github/workflows/codeql-rust-capacity.yml',
            '.github/workflows/codeql-rust-cargo-check.yml',
            '.github/workflows/codeql-rust-native-diagnostic.yml',
            '.github/workflows/codeql-rust-cfg-diagnostic.yml',
            '.github/workflows/codeql-rust-wasm-context.yml',
            '.github/workflows/codeql-rust-wasm-metadata.yml',
            '.github/workflows/codeql-platform-observation.yml',
        )
        for path in paths:
            self.write(path, '# original control\n')
        self.base = self.commit()
        for path in paths:
            with self.subTest(path=path):
                self.write(path, '# changed control\n')
                plan = self.plan()
                self.assertEqual(plan['scope'], 'targeted')
                self.assertEqual(set(plan['groups']), {'baseline', 'workflow_governance'})
                self.base = self.git('rev-parse', 'HEAD')

    def test_scoped_codeql_diagnostics_keep_contracts_and_require_trusted_registration(self):
        self.config = json.loads((HERE / 'ci-required-scope.json').read_text())
        driver = (HERE / 'ci-tests.sh').read_text().split(
            'run_workflow_governance_operational_contract_tests() {\n', 1)[1].split('\n}', 1)[0]
        for name in ('codeql-rust-wasm-context', 'codeql-rust-wasm-metadata',
                     'codeql-platform-observation'):
            with self.subTest(diagnostic=name):
                workflow = f'.github/workflows/{name}.yml'
                contract = f'scripts/security/{name}.test.py'
                paths = (workflow, f'scripts/security/{name}.py', contract,
                         'doc/testing/ci/codeql-integration.design.md',
                         'doc/testing/ci/codeql-operations.md')
                for path in paths:
                    self.write(path, '# original diagnostic input\n')
                self.base = self.commit()
                for path in paths:
                    self.write(path, '# changed diagnostic input\n')
                head = self.commit()
                plan = selector.select(self.config, self.base, head, profile='pr-fast')
                self.assertEqual(plan['scope'], 'targeted')
                self.assertEqual(plan['groups'], ['baseline', 'workflow_governance'])
                self.assertEqual(plan['affected_packages'], [])
                self.assertEqual(plan['oasis7_required_packages'], [])
                self.assertIn(f'  run python3 ./{contract}\n', driver + '\n')

                old_policy = copy.deepcopy(self.config)
                old_policy['scoped_control_paths'].pop(workflow, None)
                self.assertEqual(selector.select(old_policy, self.base, head,
                                                 profile='pr-fast')['scope'], 'full')
                self.base = head

    def test_wasm_context_scope_keeps_rust_consumers_shared_controls_and_rename_endpoints(self):
        self.config = json.loads((HERE / 'ci-required-scope.json').read_text())
        self.config['package_groups'].update(core=['oasis7_required'], consumer=['node'])
        workflow = '.github/workflows/codeql-rust-wasm-context.yml'
        self.write(workflow, '# diagnostic\n')
        self.base = self.commit()
        self.write(workflow, '# diagnostic and product change\n')
        self.write('crates/core/src/lib.rs', '// actual product change\n')
        expected = {'baseline', 'workflow_governance', 'rust_baseline', 'oasis7_required', 'node'}
        plan = self.plan()
        self.assertEqual(plan['scope'], 'targeted')
        self.assertEqual(set(plan['groups']), expected)

        self.write('crates/core/src/lib.rs',
                   'const INPUT: &str = include_str!("../../../' + workflow + '");\n')
        self.base = self.commit()
        self.write(workflow, '# changed real embedded input\n')
        self.assertEqual(set(self.plan()['groups']), expected)

        self.base = self.git('rev-parse', 'HEAD')
        self.write('Cargo.lock', '# shared build input\n')
        self.write(workflow, '# diagnostic plus shared control\n')
        plan = self.plan()
        self.assertEqual(plan['scope'], 'full')
        self.assertIn('shared build or CI control changed: Cargo.lock', plan['reasons'])

        self.base = self.git('rev-parse', 'HEAD')
        renamed = '.github/workflows/codeql-rust-wasm-context-copy.yml'
        self.git('mv', workflow, renamed)
        plan = self.plan()
        self.assertEqual(plan['scope'], 'full')
        self.assertIn('shared build or CI control changed: ' + renamed, plan['reasons'])

    def test_scoped_github_controls_do_not_exempt_unknown_or_shared_controls(self):
        self.config = json.loads((HERE / 'ci-required-scope.json').read_text())
        for path in (
                '.github/ISSUE_TEMPLATE/future.yml', '.github/workflows/codeql-new.yml',
                '.github/workflows/rust.yml', '.github/actions/shared/action.yml',
                'scripts/ci-tests.sh', 'Cargo.lock'):
            with self.subTest(path=path):
                self.write(path, '# independent or shared control\n')
                self.assertEqual(self.plan()['scope'], 'full')
                self.base = self.git('rev-parse', 'HEAD')
        self.config.pop('scoped_control_paths', None)
        self.config.pop('python_control_contract_version', None)
        self.config.pop('python_control_contracts', None)
        self.write('.github/pull_request_template.md', 'old trusted baseline remains conservative')
        self.assertEqual(self.plan()['scope'], 'full')

    def test_scoped_github_controls_keep_mixed_changes_and_embedded_consumers(self):
        self.config = json.loads((HERE / 'ci-required-scope.json').read_text())
        self.config['package_groups'].update(core=['oasis7_required'], consumer=['node'])
        path = '.github/pull_request_template.md'
        self.write(path, 'ordinary template')
        self.write('crates/core/src/lib.rs', '// independent library change')
        plan = self.plan()
        expected = {'baseline', 'workflow_governance', 'rust_baseline', 'oasis7_required', 'node'}
        self.assertEqual(plan['scope'], 'targeted')
        self.assertEqual(set(plan['groups']), expected)

        self.write('crates/core/src/lib.rs', 'const TEMPLATE: &str = include_str!("../../../' + path + '");')
        self.base = self.commit()
        self.write(path, 'real embedded template changed')
        plan = self.plan()
        self.assertEqual(plan['scope'], 'targeted')
        self.assertEqual(set(plan['groups']), expected)

        self.base = self.git('rev-parse', 'HEAD')
        self.write(path, 'template plus a shared control')
        self.write('Cargo.lock', '# changed lock')
        self.assertEqual(self.plan()['scope'], 'full')

    def test_scoped_github_controls_keep_deletions_and_both_rename_endpoints(self):
        self.config = json.loads((HERE / 'ci-required-scope.json').read_text())
        first = '.github/ISSUE_TEMPLATE/document.yml'
        second = '.github/ISSUE_TEMPLATE/test.yml'
        self.write(first, 'unique existing metadata')
        self.base = self.commit()
        self.git('mv', first, second)
        plan = self.plan()
        self.assertEqual(plan['scope'], 'targeted')
        self.assertEqual(set(plan['groups']), {'baseline', 'workflow_governance'})
        self.base = self.git('rev-parse', 'HEAD')
        Path(second).unlink()
        plan = self.plan()
        self.assertEqual(plan['scope'], 'targeted')
        self.assertEqual(set(plan['groups']), {'baseline', 'workflow_governance'})

        self.write(first, 'unique metadata renamed outside the approved paths')
        self.base = self.commit()
        self.git('mv', first, '.github/ISSUE_TEMPLATE/future.yml')
        self.assertEqual(self.plan()['scope'], 'full')

    def test_scoped_github_metadata_rename_does_not_lose_previous_source_owner(self):
        self.config = json.loads((HERE / 'ci-required-scope.json').read_text())
        self.config['package_groups'].update(core=['oasis7_required'], consumer=['node'])
        self.write('crates/core/src/lib.rs', '// source renamed into a metadata path')
        self.base = self.commit()
        Path('.github').mkdir()
        self.git('mv', 'crates/core/src/lib.rs', '.github/pull_request_template.md')
        plan = self.plan()
        self.assertEqual(plan['scope'], 'targeted')
        self.assertEqual(set(plan['groups']),
                         {'baseline', 'workflow_governance', 'rust_baseline', 'oasis7_required', 'node'})

    def test_scoped_control_configuration_rejects_ambiguous_or_unowned_paths(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        malformed = (
            [],
            {'.github/workflows/*.yml': ['workflow_governance']},
            {'.github/../Cargo.toml': ['workflow_governance']},
            {'.github//pull_request_template.md': ['workflow_governance']},
            {'.github\\pull_request_template.md': ['workflow_governance']},
            {'/.github/pull_request_template.md': ['workflow_governance']},
            {'scripts/ci-tests.sh': ['workflow_governance']},
            {'.github/pull_request_template.md': ['nonexistent_group']},
            {'.github/pull_request_template.md': ['workflow_governance', 'workflow_governance']},
            {'.github/pull_request_template.md': 'workflow_governance'},
        )
        for mapping in malformed:
            with self.subTest(mapping=mapping):
                candidate = dict(config, scoped_control_paths=mapping)
                with self.assertRaises(ValueError):
                    selector.select(candidate, self.base, self.base)

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
        self.assertEqual(set(plan), {'groups', 'resources', 'reasons', 'scope', 'architecture',
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
            'cargo-dev.sh': {'baseline', 'cargo_tooling_contracts'},
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
            'doc/p2p/blockchain/local-file-signing-backend.runbook.md': {'baseline', 'packaging_contracts', 'macos_package_contract'},
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

    def test_custody_inputs_select_worker_platform_proof_and_core_consumers(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        core, worker = 'oasis7_custody_core', 'oasis7_custody_worker'
        for package in (core, worker):
            self.write(f'crates/{package}/Cargo.toml',
                       f'[package]\nname="{package}"\nversion="0.1.0"\n')
        base = self.commit()
        graph = ({p: (f'crates/{p}', {}) for p in (core, worker)},
                 {core: {worker}, worker: set()}, {})
        cases = [(core, 'src/lib.rs'), (core, 'schema/custody.schema.json'),
                 (worker, 'src/lib.rs'), (worker, 'wrangler.identity.jsonc'),
                 (worker, 'platform-tests/isolation.test.ts'),
                 (worker, 'scripts/verify.sh'), (worker, 'package.json')]
        with patch.object(selector, 'cargo_graph', return_value=graph):
            for package, path in cases:
                with self.subTest(package=package, path=path):
                    self.write(f'crates/{package}/{path}', '// changed')
                    head = self.commit()
                    plan = selector.select(config, base, head)
                    self.assertEqual(plan['scope'], 'targeted')
                    self.assertEqual(set(plan['groups']),
                                     {'baseline', 'rust_baseline', 'workspace_support'})
                    self.assertEqual(set(plan['workspace_support_profiles']),
                                     {core, worker} if package == core else {worker})
                    self.assertTrue({'node', 'wasm_target'}.issubset(plan['resources']))
                    base = head

    def test_native_custody_inputs_add_only_actual_worker_platform_profile(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        backend, worker = 'oasis7_launcher_backend', 'oasis7_custody_worker'
        for package in (backend, worker):
            self.write(f'crates/{package}/Cargo.toml',
                       f'[package]\nname="{package}"\nversion="0.1.0"\n')
        base = self.commit()
        graph = ({p: (f'crates/{p}', {}) for p in (backend, worker)},
                 {backend: set(), worker: set()}, {})
        paths = [path for path in config['workspace_support_profile_paths'][worker]
                 if path.startswith(f'crates/{backend}/')]
        self.assertIn('crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_identity/service.rs', paths)
        with patch.object(selector, 'cargo_graph', return_value=graph):
            for path in paths:
                with self.subTest(path=path):
                    self.write(path, '// authority input only')
                    head = self.commit()
                    plan = selector.select(config, base, head)
                    self.assertEqual(plan['scope'], 'targeted')
                    self.assertIn('workspace_support', plan['groups'])
                    self.assertEqual(plan['workspace_support_profiles'], [worker])
                    self.assertTrue({'node', 'wasm_target', 'system_deps'}.issubset(plan['resources']))
                    self.assertIn(backend, plan['architecture']['packages'])
                    base = head
            self.write('crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_identity/world.rs', '// unrelated identity world feature')
            head = self.commit()
            plan = selector.select(config, base, head)
            self.assertNotIn(worker, plan['workspace_support_profiles'])
            self.assertNotIn('workspace_support', plan['groups'])
            base = head
            self.git('mv', paths[0], paths[0] + '.retired')
            head = self.commit()
            self.assertEqual(selector.select(config, base, head)['workspace_support_profiles'], [worker])
            base = head
            Path(paths[1]).unlink()
            self.assertEqual(selector.select(config, base, self.commit())['workspace_support_profiles'], [worker])

    @staticmethod
    def owner_setup_inputs():
        return [
            'crates/oasis7_agent_api/src/game/goal.rs',
            'crates/oasis7_agent_api/src/game/task.rs',
            'crates/oasis7_agent_api/src/game/goal_prepare.rs',
            'crates/oasis7_agent_api/src/game/goal_publish.rs',
            'crates/oasis7_agent_api/src/game/executor.rs',
            'crates/oasis7_client_api/src/world_service/entry_control.rs',
            'crates/oasis7_client_api/src/world_service/private_goal_read.rs',
            'crates/oasis7_client_api/src/world_service/owner_setup_observation.rs',
            'crates/oasis7_client_api/src/world_service_wire.rs',
            'crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_identity/entry_owner_control.rs',
            'crates/oasis7_chain_runtime/src/host/world_service_private_goal.rs',
        ]

    def test_owner_setup_inputs_select_worker_profile_with_real_package_ownership(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        worker = 'oasis7_custody_worker'
        paths = self.owner_setup_inputs()
        packages = {path.split('/')[1] for path in paths} | {worker}
        for package in packages:
            self.write(f'crates/{package}/Cargo.toml', f'[package]\nname="{package}"\nversion="0.1.0"\n')
        base = self.commit()
        graph = ({p: (f'crates/{p}', {}) for p in packages}, {p: set() for p in packages}, {})
        with patch.object(selector, 'cargo_graph', return_value=graph):
            for path in paths:
                with self.subTest(path=path):
                    self.write(path, '// fixed owner setup protocol input')
                    head = self.commit()
                    plan = selector.select(config, base, head)
                    self.assertEqual(plan['scope'], 'targeted')
                    self.assertIn(worker, plan['workspace_support_profiles'])
                    self.assertIn(path.split('/')[1], plan['architecture']['packages'])
                    self.assertTrue({'node', 'wasm_target'}.issubset(plan['resources']))
                    base = head
            self.git('mv', paths[0], paths[0] + '.retired')
            head = self.commit()
            self.assertIn(worker, selector.select(config, base, head)['workspace_support_profiles'])
            base = head
            Path(paths[1]).unlink()
            self.assertIn(worker, selector.select(config, base, self.commit())['workspace_support_profiles'])

    def test_participant_sdk_and_world_inputs_add_actual_worker_platform_profile(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        worker = 'oasis7_custody_worker'
        packages = ('oasis7_client_api', 'oasis7_chain_runtime', worker)
        for package in packages:
            self.write(f'crates/{package}/Cargo.toml',
                       f'[package]\nname="{package}"\nversion="0.1.0"\n')
        base = self.commit()
        graph = ({p: (f'crates/{p}', {}) for p in packages},
                 {p: set() for p in packages}, {})
        paths = [path for path in config['workspace_support_profile_paths'][worker]
                 if path.startswith(('crates/oasis7_client_api/', 'crates/oasis7_chain_runtime/'))
                 and path not in self.owner_setup_inputs()]
        self.assertEqual(len(paths), 12)
        with patch.object(selector, 'cargo_graph', return_value=graph):
            for path in paths:
                with self.subTest(path=path):
                    self.write(path, '// participant canonical protocol input')
                    head = self.commit()
                    plan = selector.select(config, base, head)
                    self.assertEqual(plan['scope'], 'targeted')
                    self.assertIn(worker, plan['workspace_support_profiles'])
                    self.assertIn(path.split('/')[1], plan['architecture']['packages'])
                    self.assertTrue({'node', 'wasm_target'}.issubset(plan['resources']))
                    base = head
            self.git('mv', paths[0], paths[0] + '.retired')
            head = self.commit()
            self.assertIn(worker, selector.select(config, base, head)['workspace_support_profiles'])
            base = head
            Path(paths[1]).unlink()
            self.assertIn(worker, selector.select(config, base, self.commit())['workspace_support_profiles'])

    def test_domain_types_selects_its_own_support_profile(self):
        package = 'oasis7_domain_types'
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        manifest = f'[package]\nname="{package}"\nversion="0.1.0"\n'
        self.write(f'crates/{package}/Cargo.toml', manifest)
        self.write(f'crates/{package}/src/lib.rs', 'pub fn marker() {}\n')
        base = self.commit()
        self.write(f'crates/{package}/src/lib.rs', 'pub fn changed() {}\n')
        head = self.commit()

        plan = selector.select(config, base, head)
        self.assertEqual(plan['workspace_support_profiles'], [package])
        self.assertEqual(selector.support_partitions(plan['workspace_support_profiles']), ['other'])
        full = selector.full_plan(config, 'candidate-full', head)
        self.assertIn(package, full['workspace_support_profiles'])
        self.write('doc/readme.md', 'unrelated documentation\n')
        docs = selector.select(config, head, self.commit())
        self.assertNotIn(package, docs['workspace_support_profiles'])

    def test_profile_path_mapping_is_additive_and_does_not_bypass_full_or_ownership(self):
        config = json.loads(json.dumps(self.config))
        config['groups'].append('workspace_support')
        config['resources']['workspace_support'] = []
        profile = selector.SUPPORT_PROFILES[0]
        config['workspace_support_profile_paths'] = {profile: ['crates/core/src/lib.rs', 'unknown/input.rs']}
        self.write('crates/core/src/lib.rs', '// consumer input')
        head = self.commit()
        selected = selector.select(config, self.base, head)
        self.assertEqual(selected['workspace_support_profiles'], [profile])
        self.assertEqual(set(selector.select(config, self.base, head, additions=['workspace_support'])['workspace_support_profiles']), set(selector.SUPPORT_PROFILES))
        config['rules'].append({'match': ['crates/core/src/lib.rs'], 'groups': ['workspace_support'], 'reason': 'legacy group'})
        self.assertEqual(set(selector.select(config, self.base, head)['workspace_support_profiles']), set(selector.SUPPORT_PROFILES))
        self.write('Cargo.lock', '# shared control')
        self.assertEqual(set(selector.select(config, self.base, self.commit())['workspace_support_profiles']), set(selector.SUPPORT_PROFILES))
        self.write('unknown/input.rs', '// no owner')
        with self.assertRaises(selector.UnknownInputsError):
            selector.select(config, self.base, self.commit())

    def test_profile_path_mapping_rejects_unknown_profiles_and_malformed_paths(self):
        for mapping in [[], {'unknown-profile': ['crates/core/src/lib.rs']},
                        {selector.SUPPORT_PROFILES[0]: []},
                        *[{selector.SUPPORT_PROFILES[0]: [path]} for path in
                          ['', '/tmp/source.rs', '../source.rs', 'crates/../source.rs', 'crates\\source.rs', './source.rs', {}, None]],
                        {selector.SUPPORT_PROFILES[0]: ['source.rs', 'source.rs']}]:
            with self.subTest(mapping=mapping):
                config = json.loads(json.dumps(self.config))
                config['groups'].append('workspace_support')
                config['resources']['workspace_support'] = []
                config['workspace_support_profile_paths'] = mapping
                with self.assertRaises(ValueError):
                    selector.validate_config(config)

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
                    'oasis7_required', 'formal_process'}
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

    def test_bootstrap_codegen_inputs_use_existing_cargo_owner(self):
        # These paths are already owned in the trusted baseline; no candidate
        # path rule or full-scope override is needed for first introduction.
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        self.write('crates/oasis7_agent_api/Cargo.toml',
                   '[package]\nname="oasis7_agent_api"\nversion="0.1.0"\n')
        base = self.commit()
        graph = ({'oasis7_agent_api': ('crates/oasis7_agent_api', {})},
                 {'oasis7_agent_api': set()}, {})
        inputs = (
            'src/game/read_bootstrap.openapi.json',
            'protocol_codegen/generate.py', 'protocol_codegen/profile.json', 'protocol_codegen/toolchain.lock.json',
            'protocol_codegen/test_generate.py',
            'protocol_codegen/generated/types.ts',
            'protocol_codegen/generated/schema.json',
            'protocol_codegen/generated/manifest.json',
            'protocol_conformance/package.json',
            'protocol_conformance/package-lock.json',
            'protocol_conformance/tsconfig.json',
            'protocol_conformance/consumer.ts', 'protocol_conformance/check-generated.mjs',
        )
        with patch.object(selector, 'cargo_graph', return_value=graph):
            for path in inputs:
                with self.subTest(path=path):
                    self.write('crates/oasis7_agent_api/' + path, 'fixture')
                    head = self.commit()
                    plan = selector.select(config, base, head)
                    self.assertEqual(plan['scope'], 'targeted')
                    self.assertIn('workspace_support', plan['groups'])
                    self.assertIn('oasis7_agent_api', plan['workspace_support_profiles'])
                    base = head

    def test_bootstrap_handler_and_fixtures_select_ts_conformance(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        self.write('crates/oasis7_viewer_runtime/Cargo.toml',
                   '[package]\nname="oasis7_viewer_runtime"\nversion="0.1.0"\n')
        base = self.commit()
        graph = ({'oasis7_viewer_runtime': ('crates/oasis7_viewer_runtime', {})},
                 {'oasis7_viewer_runtime': set()}, {})
        with patch.object(selector, 'cargo_graph', return_value=graph):
            for path in ('src/game_read_api.rs', 'src/game_read_api_tests.rs',
                         'src/game_read_api_protocol_fixtures_tests.rs',
                         'tests/fixtures/signed_read_bootstrap.json'):
                self.write('crates/oasis7_viewer_runtime/' + path, 'fixture')
                head = self.commit()
                plan = selector.select(config, base, head)
                self.assertIn('workspace_support', plan['groups'])
                self.assertIn('oasis7_agent_api', plan['workspace_support_profiles'])
                base = head

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
                                     'workspace_support', 'oasis7_required', 'formal_process'})
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
                        expected.update({'oasis7_required', 'formal_process'})
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

        def graph_for(ref, generated_inputs=(), *, source_cache=None):
            return ((old_manifests, old_reverse, {}) if ref == base
                    else (new_manifests, new_reverse, {}))

        with patch.object(selector, 'cargo_graph', side_effect=graph_for):
            plan = selector.select(config, base, head)
        self.assertEqual(plan['scope'], 'targeted')
        self.assertEqual(set(plan['groups']), {'baseline', 'rust_baseline', 'workspace_support',
                                               'oasis7_required', 'formal_process'})


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

    def test_formal_full_only_policy_defers_every_fast_path_without_narrowing_ordinary(self):
        old = self.fast_config()
        config = dict(old, formal_full_profiles_only_version=1)
        self.write('crates/core/src/lib.rs', '#[cfg(unix)]\npub fn platform() {}')
        self.base = self.commit()
        cases = [
            ('crates/core/src/history.rs', 'pub fn history() {}', ()),
            ('crates/consumer/src/lib.rs', 'pub fn unmapped() {}', ()),
            ('crates/core/src/lib.rs', '#[cfg(windows)]\npub fn platform() {}', ()),
            ('Cargo.lock', '# lock change', ()),
            ('.github/workflows/rust.yml', '# shared control change', ()),
            ('doc/readme.md', 'explicit group request', ('formal_process',)),
        ]
        for path, content, additions in cases:
            base = self.git('rev-parse', 'HEAD')
            self.write(path, content)
            head = self.commit()
            for profile in ('pr-fast', 'main-fast'):
                with self.subTest(path=path, profile=profile):
                    legacy = selector.select(old, base, head, profile=profile, additions=additions)
                    current = selector.select(config, base, head, profile=profile, additions=additions)
                    self.assertIn('formal_process', legacy['groups'])
                    if path in ('crates/core/src/lib.rs', 'Cargo.lock', '.github/workflows/rust.yml'):
                        self.assertEqual(legacy['scope'], 'full')
                    self.assertEqual(current['formal_full_profiles_only_version'], 1)
                    self.assertEqual(set(current['groups']), set(legacy['groups']) - {'formal_process'})
                    self.assertEqual(current['formal_suites'], [])
                    self.assertEqual(current['formal_shards'], [])
                    changed_fields = {'formal_full_profiles_only_version', 'formal_deferred_ordinary_cases',
                                      'groups', 'resources', 'formal_suites', 'formal_shards',
                                      'fast_plan_exceptions', 'reasons'}
                    self.assertEqual({key: value for key, value in current.items() if key not in changed_fields},
                                     {key: value for key, value in legacy.items() if key not in changed_fields})
                    self.assertTrue(any('complete profiles' in reason for reason in current['reasons']))

    def test_formal_full_only_policy_retains_all_full_suites_and_legacy_base(self):
        old = self.fast_config()
        config = dict(old, formal_full_profiles_only_version=1)
        # Candidate metadata cannot activate a capability missing from BASE.
        self.write('scripts/ci-required-scope.json', json.dumps(config))
        head = self.commit()
        legacy = selector.select(old, self.base, head, profile='pr-fast')
        self.assertNotIn('formal_full_profiles_only_version', legacy)
        self.assertEqual(legacy['formal_suites'], sorted(selector.FORMAL_SUITES))
        self.assertEqual(set(legacy['groups']), set(old['groups']))
        for profile in ('periodic-full', 'candidate-full'):
            with self.subTest(profile=profile):
                full = selector.full_plan(config, profile, head, policy_ref=self.base)
                self.assertEqual(full['formal_full_profiles_only_version'], 1)
                self.assertEqual(set(full['groups']), set(config['groups']))
                self.assertEqual(full['formal_suites'], sorted(selector.FORMAL_SUITES))
                self.assertEqual(full['formal_shards'], sorted(selector.FORMAL_SUITES))
        empty = selector.select(config, head, head, profile='pr-fast', additions=('formal_process',))
        self.assertNotIn('formal_process', empty['groups'])
        self.assertEqual(empty['formal_suites'], [])
        self.write('config.json', json.dumps(config))
        arguments = ['selector', '--profile', 'pr-fast', '--base-ref', self.base, '--head-ref', head,
                     '--config', 'config.json', '--github-output', 'outputs.txt']
        with patch.object(sys, 'argv', arguments), contextlib.redirect_stdout(io.StringIO()):
            selector.main()
        outputs = dict(line.split('=', 1) for line in Path('outputs.txt').read_text().splitlines())
        self.assertEqual(outputs['run_formal_process'], 'false')
        self.assertEqual(json.loads(outputs['formal_shards']), [])

    def test_formal_full_only_capability_rejects_invalid_or_incomplete_trusted_inventory(self):
        for value in (True, False, 0, 2, '1', None):
            with self.subTest(value=value), self.assertRaisesRegex(ValueError, 'Formal full-profile'):
                selector.validate_config(dict(self.fast_config(), formal_full_profiles_only_version=value))
        for field in ('fast_scope_version', 'formal_process'):
            config = dict(self.fast_config(), formal_full_profiles_only_version=1)
            if field == 'formal_process':
                config['groups'].remove(field)
                del config['resources'][field]
                for groups in config['package_groups'].values():
                    groups.remove(field)
            else:
                del config[field]
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, 'Formal full-profile'):
                selector.validate_config(config)
        for deferred in ([], {'other': ['formal::heavy']}, {'runtime-tests': []},
                         {'runtime-tests': ['formal::heavy', 'formal::heavy']},
                         {'runtime-tests': ['formal::*']}, {'runtime-tests': [None]}):
            with self.subTest(deferred=deferred), self.assertRaisesRegex(ValueError, 'deferred ordinary Formal'):
                selector.validate_config(dict(self.fast_config(), formal_full_profiles_only_version=1,
                                              formal_deferred_ordinary_cases=deferred))

    def test_fast_profile_preserves_actual_affected_package_and_wasm_closure(self):
        config = self.fast_config()
        self.write('crates/core/src/lib.rs', 'pub fn actual_changed_behavior() {}')
        head = self.commit()
        plan = selector.select(config, self.base, head, profile='pr-fast')
        self.assertEqual(plan['oasis7_required_packages'], ['consumer', 'core'])
        self.assertEqual(plan['oasis7_required_wasm_packages'], ['core'])
        self.assertNotIn('formal_process', plan['groups'])
        self.assertEqual(plan['formal_suites'], [])
        self.assertEqual(plan['fast_plan_exceptions'], [])
        self.assertEqual(plan['source_sha'], head)
        self.assertEqual(plan['policy_sha'], self.base)
        self.assertIn('formal_process', selector.select(config, self.base, head)['groups'])

    def bounded_config(self, paths=('crates/core/src/lib.rs',)):
        config = self.fast_config()
        config.update(fast_test_inventory_version=1, ordinary_fast_targets={
            'core-lib': {'package': 'core', 'target_kind': 'lib', 'target_name': 'core',
                         'default_features': False, 'features': ['test_tier_required'],
                         'cases': ['existing']}}, ordinary_fast_rules=[
                {'paths': list(paths), 'targets': ['core-lib'], 'reason': 'bounded contract'}])
        return config

    def test_bounded_inventory_keeps_reverse_package_and_wasm_closure(self):
        config = self.bounded_config()
        self.write('crates/core/src/lib.rs', 'pub fn changed() {}\n')
        head = self.commit()
        for profile in ('pr-fast', 'main-fast'):
            with self.subTest(profile=profile):
                plan = selector.select(config, self.base, head, profile=profile)
                self.assertEqual(plan['ordinary_fast_targets'], [config['ordinary_fast_targets']['core-lib']])
                self.assertEqual(plan['fast_test_inventory_version'], 1)
                self.assertEqual(plan['oasis7_required_packages'], ['consumer', 'core'])
                self.assertEqual(plan['oasis7_required_wasm_packages'], ['core'])
                self.assertEqual(plan['formal_suites'], [])

    def test_bounded_inventory_defers_only_completely_mapped_formal_inputs(self):
        config = self.bounded_config(('crates/core/src/history.rs',))
        self.write('crates/core/src/history.rs', 'pub fn changed() {}\n')
        head = self.commit()
        self.assertEqual(selector.select(config, self.base, head, profile='pr-fast')['formal_suites'], ['world-history'])
        self.write('crates/core/src/other.rs', 'pub fn additional() {}\n')
        plan = selector.select(config, self.base, self.commit(), profile='pr-fast')
        self.assertNotIn('ordinary_fast_targets', plan)
        self.assertEqual(plan['formal_suites'], ['world-history'])

    def component_config(self):
        config = self.bounded_config()
        config['groups'].append('launcher_web')
        config['resources']['launcher_web'] = []
        config['package_groups']['core'].append('launcher_web')
        config.update(fast_execution_version=1, architecture_build_policy_version=1,
                      ordinary_fast_component_rules=[{
            'match': ['crates/core/src/**'], 'exclude': ['crates/core/src/bin/**'],
            'targets': ['core-lib'], 'reason': 'whole component behavior'}])
        return config

    def component_fixture(self, *, native=True, shared=False):
        gate = '#[cfg(not(target_arch="wasm32"))]\n' if native else ''
        self.write('crates/core/src/lib.rs', gate + 'mod behavior;\n')
        self.write('crates/core/src/behavior.rs', 'pub fn value() -> u8 { 1 }\n')
        if shared:
            self.write('crates/consumer/src/lib.rs',
                       '#[path="../../core/src/behavior.rs"] mod shared;\n')
        base = self.commit()
        self.write('crates/core/src/behavior.rs', 'pub fn value() -> u8 { 2 }\n')
        return base, self.commit()

    def test_component_inventory_executes_owner_and_preserves_affected_evidence(self):
        config = self.component_config()
        base, head = self.component_fixture()
        for profile in ('pr-fast', 'main-fast'):
            plan = selector.select(config, base, head, profile=profile)
            self.assertEqual(plan['ordinary_execution'], 'inline')
            self.assertEqual(plan['fast_execution_version'], 1)
            self.assertEqual(plan['ordinary_clippy_group'], 'rust_baseline')
            self.assertTrue({'rust_baseline', 'oasis7_required'}.issubset(plan['groups']))
            self.assertEqual(plan['rust_baseline_checks'], [])
            self.assertEqual(plan['ordinary_fast_targets'], [config['ordinary_fast_targets']['core-lib']])
            self.assertEqual(plan['affected_packages'], ['consumer', 'core'])
            self.assertEqual(plan['architecture']['packages'], ['core'])
            self.assertEqual(plan['architecture']['build_policy_version'], 1)
            self.assertEqual(plan['architecture']['build_policy'], 'ordinary-targets')
            self.assertEqual(plan['oasis7_required_packages'], ['consumer', 'core'])
            self.assertEqual(plan['oasis7_required_wasm_packages'], [])
            self.assertNotIn('launcher_web', plan['groups'])

    def test_formal_deferral_preserves_native_component_architecture_and_clippy_decisions(self):
        old = self.component_config()
        old['formal_risk_rules'].append(dict(match=['crates/core/src/behavior.rs'],
                                            suites=['world-history'], reason='native history risk'))
        current = dict(old, formal_full_profiles_only_version=1)
        base, head = self.component_fixture()
        for profile in ('pr-fast', 'main-fast'):
            legacy = selector.select(old, base, head, profile=profile)
            deferred = selector.select(current, base, head, profile=profile)
            self.assertEqual(legacy['ordinary_execution'], 'inline')
            self.assertEqual(legacy['formal_suites'], ['world-history'])
            self.assertNotIn('build_policy', legacy['architecture'])
            changed_fields = {'formal_full_profiles_only_version', 'formal_deferred_ordinary_cases',
                              'groups', 'resources', 'formal_suites', 'formal_shards',
                              'fast_plan_exceptions', 'reasons'}
            with self.subTest(profile=profile):
                self.assertEqual({key: value for key, value in deferred.items() if key not in changed_fields},
                                 {key: value for key, value in legacy.items() if key not in changed_fields})
                self.assertNotIn('formal_process', deferred['groups'])


    def test_native_package_partition_capability_keeps_the_full_parent_and_old_fallback(self):
        self.assertEqual(selector.ORDINARY_PARTITION_PACKAGES,
                         frozenset(('oasis7_runtime', 'oasis7_world_state')))
        config = self.component_config()
        config['ordinary_partition_version'] = 1
        config['ordinary_fast_targets']['consumer-lib'] = dict(
            config['ordinary_fast_targets']['core-lib'], package='consumer', target_name='consumer')
        config['ordinary_fast_component_rules'][0]['targets'].append('consumer-lib')
        base, head = self.component_fixture()
        # Use the existing miniature Cargo graph as the two allowlisted owners;
        # the production allowlist is checked independently above.
        with patch.object(selector, 'ORDINARY_PARTITION_PACKAGES', frozenset(('core', 'consumer'))):
            for profile in ('pr-fast', 'main-fast'):
                plan = selector.select(config, base, head, profile=profile)
                self.assertEqual(plan['ordinary_partition_version'], 1)
                self.assertEqual(plan['ordinary_partitions'], ['consumer', 'core'])
                self.assertEqual({target['package'] for target in plan['ordinary_fast_targets']}, {'core', 'consumer'})
                self.assertEqual(plan['ordinary_clippy_group'], 'rust_baseline')
                old = dict(config)
                del old['ordinary_partition_version']
                self.assertNotIn('ordinary_partitions', selector.select(old, base, head, profile=profile))
            full = selector.full_plan(config, 'candidate-full', head)
            self.assertNotIn('ordinary_partition_version', full)
            self.assertNotIn('ordinary_partitions', full)
        # Other package combinations keep one complete inline execution.
        self.assertEqual(selector.select(config, base, head, profile='pr-fast')['ordinary_partitions'], ['all'])
        for version in (True, False, None, '1', 2):
            with self.subTest(version=version), self.assertRaises(ValueError):
                selector.validate_config(dict(config, ordinary_partition_version=version))
        unsupported = dict(config)
        del unsupported['fast_execution_version']
        with self.assertRaises(ValueError):
            selector.validate_config(unsupported)

    def test_component_architecture_policy_requires_independent_trusted_capability(self):
        config = self.component_config()
        base, head = self.component_fixture()
        for version in (None, 2):
            old = dict(config)
            if version is None:
                del old['architecture_build_policy_version']
            else:
                old['architecture_build_policy_version'] = version
            with self.subTest(version=version):
                plan = selector.select(old, base, head, profile='pr-fast')
                self.assertEqual(plan['ordinary_execution'], 'inline')
                self.assertEqual(plan['architecture']['mode'], 'targeted')
                self.assertNotIn('build_policy_version', plan['architecture'])
                self.assertNotIn('build_policy', plan['architecture'])
        for version in (True, False, None, '1', []):
            with self.subTest(invalid=version), self.assertRaises(ValueError):
                selector.validate_config(dict(config, architecture_build_policy_version=version))
        with self.assertRaises(ValueError):
            selector.validate_config(dict(self.bounded_config(), architecture_build_policy_version=1))

    def test_component_architecture_policy_preserves_mixed_support_owner_compilation(self):
        config = self.component_config()
        support = 'oasis7_custody_core'
        self.write('crates/other/Cargo.toml',
                   f'[package]\nname="{support}"\nversion="0.1.0"\n')
        self.write('crates/other/src/lib.rs', 'pub fn capability() -> u8 { 1 }\n')
        base, _ = self.component_fixture()
        self.write('crates/other/src/lib.rs', 'pub fn capability() -> u8 { 2 }\n')
        head = self.commit()
        config['groups'].append('workspace_support')
        config['resources']['workspace_support'] = []
        for group in ('workspace_support', 'rust_baseline'):
            config['package_groups'][support] = [group]
            with self.subTest(group=group):
                plan = selector.select(config, base, head, profile='pr-fast')
                self.assertEqual(plan['ordinary_execution'], 'inline')
                self.assertEqual(plan['fast_plan_exceptions'], [])
                self.assertEqual(plan['architecture']['packages'], ['core', support])
                self.assertEqual({t['package'] for t in plan['ordinary_fast_targets']}, {'core'})
                if group == 'rust_baseline':
                    self.assertEqual(set(plan['groups']), {'baseline', 'rust_baseline', 'oasis7_required'})
                else:
                    self.assertEqual(plan['workspace_support_profiles'], [support])
                self.assertNotIn('build_policy_version', plan['architecture'])

    def test_component_architecture_policy_preserves_additional_required_groups(self):
        config = self.component_config()
        config['rules'].append({'match': ['web/**'], 'groups': ['launcher_web'], 'reason': 'web source'})
        base, head = self.component_fixture()
        explicit = selector.select(config, base, head, additions=('rust_baseline',), profile='pr-fast')
        self.assertEqual(explicit['ordinary_execution'], 'inline')
        self.assertEqual(explicit['architecture']['mode'], 'full')
        self.assertNotIn('build_policy_version', explicit['architecture'])
        self.write('web/style.css', 'body { color: black; }\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertEqual(plan['ordinary_execution'], 'inline')
        self.assertEqual(plan['architecture']['packages'], ['core'])
        self.assertIn('launcher_web', plan['groups'])
        self.assertEqual(plan['fast_plan_exceptions'], [])
        self.assertNotIn('build_policy_version', plan['architecture'])

    def native_consumer_fixture(self):
        config = self.component_config()
        for group in ('distfs', 'net', 'node'):
            package = 'oasis7_' + group
            self.write(f'crates/{package}/Cargo.toml',
                       f'[package]\nname="{package}"\nversion="0.1.0"\n'
                       '[dependencies]\ncore={path="../core"}\n')
            self.write(f'crates/{package}/src/lib.rs', 'pub fn consumer() {}\n')
            config['groups'].append(group)
            config['resources'][group] = []
            config['package_groups'][package] = [group, 'formal_process']
        base, head = self.component_fixture()
        return config, base, head

    def test_native_reverse_consumer_groups_keep_fast_architecture_and_all_checks(self):
        config, base, head = self.native_consumer_fixture()
        for profile in ('pr-fast', 'main-fast'):
            with self.subTest(profile=profile):
                plan = selector.select(config, base, head, profile=profile)
                self.assertEqual(set(plan['groups']),
                                 {'baseline', 'rust_baseline', 'oasis7_required', 'distfs', 'net', 'node'})
                self.assertEqual(plan['ordinary_fast_targets'], [config['ordinary_fast_targets']['core-lib']])
                self.assertEqual(plan['ordinary_clippy_group'], 'rust_baseline')
                self.assertEqual(plan['architecture']['packages'], ['core'])
                self.assertEqual(plan['architecture'].get('build_policy'), 'ordinary-targets')
                self.assertEqual(plan['architecture'].get('consumer_groups'),
                                 {group: ['oasis7_' + group] for group in ('distfs', 'net', 'node')})
                self.assertEqual(plan['affected_packages'],
                                 ['consumer', 'core', 'oasis7_distfs', 'oasis7_net', 'oasis7_node'])
                self.assertEqual(plan['formal_suites'], [])
                self.assertEqual(plan['fast_plan_exceptions'], [])

    def test_distfs_clippy_handoff_requires_its_independent_trusted_capability_and_complete_native_parent(self):
        self.assertEqual(selector.DISTFS_CLIPPY_TARGETS, frozenset((
            ('oasis7_runtime', 'lib', 'oasis7_runtime'),
            ('oasis7_runtime', 'test', 'pr_fast_incremental'),
            ('oasis7_world_state', 'lib', 'oasis7_world_state'),
            ('oasis7_world_state', 'test', 'pr_fast_reducers'))))
        config, base, head = self.native_consumer_fixture()
        config.update(ordinary_partition_version=1, distfs_clippy_contract_version=1)
        config['ordinary_fast_targets']['consumer-lib'] = dict(
            config['ordinary_fast_targets']['core-lib'], package='consumer', target_name='consumer')
        config['ordinary_fast_component_rules'][0]['targets'].append('consumer-lib')
        # Preserve both complete miniature lib owners; pin the real four-target
        # inventory independently above rather than inventing a new fast scope.
        identities = frozenset((package, 'lib', package) for package in ('core', 'consumer'))
        with patch.object(selector, 'ORDINARY_PARTITION_PACKAGES', frozenset(('core', 'consumer'))), \
             patch.object(selector, 'DISTFS_CLIPPY_TARGETS', identities):
            for profile in ('pr-fast', 'main-fast'):
                plan = selector.select(config, base, head, profile=profile)
                self.assertEqual(plan['distfs_clippy_contract_version'], 1)
                self.assertEqual(plan['distfs_clippy_group'], 'rust_baseline')
                self.assertTrue({'distfs', 'rust_baseline', 'oasis7_required'}.issubset(plan['groups']))
                self.assertEqual(len(plan['ordinary_fast_targets']), 2)
                old = dict(config)
                del old['distfs_clippy_contract_version']
                previous = selector.select(old, base, head, profile=profile)
                self.assertNotIn('distfs_clippy_group', previous)
                self.assertEqual(plan['groups'], previous['groups'])
                self.assertEqual(plan['ordinary_fast_targets'], previous['ordinary_fast_targets'])
            explicit = selector.select(config, base, head, additions=('distfs',), profile='pr-fast')
            self.assertNotIn('distfs_clippy_group', explicit)
            for profile in ('candidate-full', 'periodic-full'):
                self.assertNotIn('distfs_clippy_group', selector.full_plan(config, profile, head))
        # A different owner/target inventory retains the original lint owner.
        self.assertNotIn('distfs_clippy_group', selector.select(config, base, head, profile='pr-fast'))
        for version in (True, False, None, '1', 2):
            with self.subTest(version=version), self.assertRaises(ValueError):
                selector.validate_config(dict(config, distfs_clippy_contract_version=version))
        for capability in ('fast_execution_version', 'ordinary_partition_version', 'architecture_build_policy_version'):
            old = dict(config)
            del old[capability]
            with self.subTest(missing=capability), self.assertRaises(ValueError):
                selector.validate_config(old)

    def test_native_consumer_group_does_not_cover_direct_unmapped_inputs_or_requests(self):
        config, base, head = self.native_consumer_fixture()
        for group in ('net', 'node', 'distfs'):
            with self.subTest(group=group, reason='explicit request'):
                plan = selector.select(config, base, head, additions=(group,), profile='pr-fast')
                self.assertNotIn('build_policy', plan['architecture'])
            self.write(f'crates/oasis7_{group}/src/lib.rs', 'pub fn directly_changed() {}\n')
            with self.subTest(group=group, reason='direct owner'):
                plan = selector.select(config, base, self.commit(), profile='pr-fast')
                self.assertIn('oasis7_' + group, plan['architecture']['packages'])
                self.assertNotIn('build_policy', plan['architecture'])
            self.git('reset', '--hard', head)
        path = 'tools/native-consumer-input.txt'
        self.write(path, 'additional non-Cargo input\n')
        head = self.commit()
        for group in ('net', 'node', 'distfs'):
            with self.subTest(group=group, reason='direct path contract'):
                mixed = json.loads(json.dumps(config))
                mixed['rules'].append({'match': [path], 'groups': [group], 'reason': 'actual separate input'})
                plan = selector.select(mixed, base, head, profile='pr-fast')
                self.assertEqual(plan['architecture']['packages'], ['core'])
                self.assertIn(group, plan['groups'])
                self.assertNotIn('build_policy', plan['architecture'])

    def test_component_architecture_policy_cannot_borrow_scoped_control_exceptions(self):
        config = self.component_config()
        path = '.github/scoped-metadata.yml'
        config['scoped_control_paths'] = {path: []}
        base, _ = self.component_fixture()
        self.write(path, '# independently scoped CI metadata\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertEqual(plan['ordinary_execution'], 'inline')
        self.assertEqual(plan['architecture']['mode'], 'targeted')
        self.assertEqual(plan['fast_plan_exceptions'], [])
        self.assertEqual(set(plan['groups']), {'baseline', 'rust_baseline', 'oasis7_required'})
        self.assertNotIn('build_policy_version', plan['architecture'])

    def test_component_portable_and_alternate_library_mounts_keep_wasm_and_launcher(self):
        config = self.component_config()
        for native, shared in ((False, False), (True, True)):
            base, head = self.component_fixture(native=native, shared=shared)
            with self.subTest(native=native, shared=shared):
                plan = selector.select(config, base, head, profile='pr-fast')
                self.assertEqual(plan['ordinary_execution'], 'inline')
                self.assertEqual(plan['oasis7_required_wasm_packages'], ['core'])
                self.assertNotIn('build_policy_version', plan['architecture'])
                self.assertIn('launcher_web', plan['groups'])

    def test_component_portable_binary_mounts_keep_launcher_build(self):
        config = self.component_config()
        consumer_manifest = Path('crates/consumer/Cargo.toml').read_text()
        for path, declaration in (
                ('crates/consumer/src/main.rs', ''),
                ('crates/consumer/entry.rs', '\n[[bin]]\nname="portable"\npath="entry.rs"\ntest=false\n'),
                ('crates/consumer/src/bin/portable.rs', '\n[[bin]]\nname="portable"\ntest=false\n')):
            self.write('crates/core/src/lib.rs', '#[cfg(not(target_arch="wasm32"))] mod behavior;\n')
            self.write('crates/core/src/behavior.rs', 'pub fn value() -> u8 { 1 }\n')
            relative = os.path.relpath('crates/core/src/behavior.rs', Path(path).parent)
            self.write(path, f'#[path="{relative}"] mod shared;\nfn main() {{ shared::value(); }}\n')
            self.write('crates/consumer/Cargo.toml', consumer_manifest + declaration)
            base = self.commit()
            self.write('crates/core/src/behavior.rs', 'pub fn value() -> u8 { 2 }\n')
            with self.subTest(path=path):
                plan = selector.select(config, base, self.commit(), profile='pr-fast')
                self.assertEqual(plan['ordinary_execution'], 'inline')
                self.assertEqual(plan['oasis7_required_wasm_packages'], ['core'])
                self.assertIn('launcher_web', plan['groups'])
            Path(path).unlink()

    def test_component_portable_data_readers_keep_wasm_and_launcher(self):
        config = self.component_config()
        for macro in ('include_str', 'include_bytes'):
            self.write('crates/core/src/lib.rs', '#[cfg(not(target_arch="wasm32"))] mod behavior;\n')
            self.write('crates/core/src/behavior.rs', 'pub fn value() -> u8 { 1 }\n')
            value_type = '&str' if macro == 'include_str' else '&[u8]'
            self.write('crates/consumer/src/lib.rs',
                       f'const SOURCE: {value_type} = {macro}!("../../core/src/behavior.rs");\n')
            base = self.commit()
            self.write('crates/core/src/behavior.rs', 'pub fn value() -> u8 { 2 }\n')
            with self.subTest(macro=macro):
                plan = selector.select(config, base, self.commit(), profile='pr-fast')
                self.assertEqual(plan['ordinary_execution'], 'inline')
                self.assertEqual(plan['oasis7_required_wasm_packages'], ['core'])
                self.assertIn('launcher_web', plan['groups'])

    def test_component_custom_production_root_resolves_implicit_child_modules(self):
        config = self.component_config()
        original_manifest = Path('crates/consumer/Cargo.toml').read_text()
        for kind in ('bin', 'lib'):
            declaration = ('\n[[bin]]\nname="portable"\n' if kind == 'bin' else '\n[lib]\n')
            self.write('crates/consumer/Cargo.toml', original_manifest + declaration + 'path="entry.rs"\ntest=false\n')
            self.write('crates/core/src/lib.rs', '#[cfg(not(target_arch="wasm32"))] mod behavior;\n')
            self.write('crates/core/src/behavior.rs', 'pub fn value() -> u8 { 1 }\n')
            self.write('crates/consumer/entry.rs', 'mod shared;\npub fn main() { shared::value(); }\n')
            self.write('crates/consumer/shared.rs',
                       '#[path="../core/src/behavior.rs"] mod original;\npub use original::value;\n')
            base = self.commit()
            self.write('crates/core/src/behavior.rs', 'pub fn value() -> u8 { 2 }\n')
            with self.subTest(kind=kind):
                plan = selector.select(config, base, self.commit(), profile='pr-fast')
                self.assertEqual(plan['oasis7_required_wasm_packages'], ['core'])
                self.assertIn('launcher_web', plan['groups'])

    def test_component_data_reader_never_proves_executable_source_ownership(self):
        config = self.component_config()
        self.write('crates/core/src/lib.rs', 'const SOURCE: &str = include_str!("behavior.rs");\n')
        self.write('crates/core/src/behavior.rs', '#[test] fn not_executed() {}\n')
        plan = selector.select(config, self.base, self.commit(), profile='pr-fast')
        self.assertNotIn('ordinary_execution', plan)
        self.assertNotIn('ordinary_fast_targets', plan)

    def test_component_native_test_data_reader_does_not_require_web_packaging(self):
        config = self.component_config()
        self.write('crates/core/src/lib.rs', '#[cfg(not(target_arch="wasm32"))] mod behavior;\n')
        self.write('crates/core/src/behavior.rs', 'pub fn value() -> u8 { 1 }\n')
        self.write('crates/consumer/src/lib.rs', '#[cfg(test)] mod tests;\n')
        self.write('crates/consumer/src/tests.rs', 'const SOURCE: &str = include_str!("../../core/src/behavior.rs");\n')
        base = self.commit()
        self.write('crates/core/src/behavior.rs', 'pub fn value() -> u8 { 2 }\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertEqual(plan['oasis7_required_wasm_packages'], [])
        self.assertNotIn('launcher_web', plan['groups'])

    def test_component_unknown_portable_data_input_cannot_authorize_deferral(self):
        config = self.component_config()
        self.write('crates/core/src/lib.rs', '#[cfg(not(target_arch="wasm32"))] mod behavior;\n')
        self.write('crates/core/src/behavior.rs', 'pub fn value() -> u8 { 1 }\n')
        self.write('crates/consumer/src/lib.rs', 'const SOURCE: &str = include_str!(env!("SOURCE_PATH"));\n')
        base = self.commit()
        self.write('crates/core/src/behavior.rs', 'pub fn value() -> u8 { 2 }\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertEqual(plan['oasis7_required_wasm_packages'], ['core'])
        self.assertIn('launcher_web', plan['groups'])

    def test_component_build_script_literal_reader_keeps_portable_consumers(self):
        config = self.component_config()
        self.write('crates/core/src/lib.rs', '#[cfg(not(target_arch="wasm32"))] mod behavior;\n')
        self.write('crates/core/src/behavior.rs', 'pub fn value() -> u8 { 1 }\n')
        self.write('crates/consumer/build.rs',
                   'fn main() { let value = std::fs::read_to_string("../core/src/behavior.rs").unwrap();\n'
                   'println!("cargo:rustc-env=SOURCE_BODY={value}"); }\n')
        self.write('crates/consumer/src/lib.rs', 'const SOURCE: &str = env!("SOURCE_BODY");\n')
        base = self.commit()
        self.write('crates/core/src/behavior.rs', 'pub fn value() -> u8 { 2 }\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertEqual(plan['oasis7_required_wasm_packages'], ['core'])
        self.assertIn('launcher_web', plan['groups'])

    def test_component_native_host_build_dependencies_keep_generated_portable_consumers(self):
        config = self.component_config()
        config['oasis7_required_wasm_packages'] = ['consumer']
        consumer_manifest = Path('crates/consumer/Cargo.toml').read_text()
        other_manifest = Path('crates/other/Cargo.toml').read_text()
        workspace_manifest = Path('Cargo.toml').read_text()
        for kind in ('direct', 'transitive', 'target-renamed-workspace'):
            self.write('Cargo.toml', workspace_manifest + 'generator={path="crates/other",package="other"}\n')
            self.write('crates/other/Cargo.toml', other_manifest + '[dependencies]\ncore={workspace=true}\n')
            self.write('crates/other/src/lib.rs', '#[cfg(not(target_arch="wasm32"))] pub use core::value;\n')
            if kind == 'direct':
                dependency = '\n[build-dependencies]\ncore={workspace=true}\n'
                generator = 'core'
            elif kind == 'transitive':
                dependency = '\n[build-dependencies]\ngenerator={path="../other",package="other"}\n'
                generator = 'generator'
            else:
                dependency = '\n[target.\'cfg(unix)\'.build-dependencies]\ngenerator={workspace=true}\n'
                generator = 'generator'
            self.write('crates/consumer/Cargo.toml', consumer_manifest + dependency)
            self.write('crates/consumer/build.rs',
                       f'fn main() {{ println!("cargo:rustc-env=VALUE={{}}", {generator}::value()); }}\n')
            self.write('crates/consumer/src/lib.rs', 'pub const VALUE: &str = env!("VALUE");\n')
            self.write('crates/core/src/lib.rs', '#[cfg(not(target_arch="wasm32"))] mod behavior;\n'
                       '#[cfg(not(target_arch="wasm32"))] pub use behavior::value;\n')
            self.write('crates/core/src/behavior.rs', 'pub fn value() -> u8 { 1 }\n')
            base = self.commit()
            self.write('crates/core/src/behavior.rs', 'pub fn value() -> u8 { 2 }\n')
            with self.subTest(kind=kind):
                plan = selector.select(config, base, self.commit(), profile='pr-fast')
                self.assertEqual(plan['ordinary_execution'], 'inline')
                self.assertEqual(plan['oasis7_required_wasm_packages'], ['consumer'])
                self.assertIn('launcher_web', plan['groups'])

    def test_component_native_proc_macro_dependencies_keep_generated_portable_consumers(self):
        config = self.component_config()
        config['oasis7_required_wasm_packages'] = ['consumer']
        macro_manifest = Path('crates/other/Cargo.toml').read_text()
        consumer_manifest = Path('crates/consumer/Cargo.toml').read_text()
        for declaration in ('proc-macro=true', 'proc_macro=true',
                            'crate-type=["proc-macro"]', 'crate_type=["proc-macro"]'):
            self.write('crates/other/Cargo.toml', macro_manifest
                       + '\n[lib]\n' + declaration + '\n[dependencies]\ncore={workspace=true}\n')
            self.write('crates/other/src/lib.rs', 'extern crate proc_macro;\n#[proc_macro]\n'
                       'pub fn generated(_: proc_macro::TokenStream) -> proc_macro::TokenStream {\n'
                       'format!("pub const VALUE: u8 = {};", core::value()).parse().unwrap() }\n')
            self.write('crates/consumer/Cargo.toml', consumer_manifest
                       + '\n[dependencies]\nother={path="../other"}\n')
            self.write('crates/consumer/src/lib.rs', 'other::generated!();\n')
            self.write('crates/core/src/lib.rs', '#[cfg(not(target_arch="wasm32"))] mod behavior;\n'
                       '#[cfg(not(target_arch="wasm32"))] pub use behavior::value;\n')
            self.write('crates/core/src/behavior.rs', 'pub fn value() -> u8 { 1 }\n')
            base = self.commit()
            self.write('crates/core/src/behavior.rs', 'pub fn value() -> u8 { 2 }\n')
            with self.subTest(declaration=declaration):
                plan = selector.select(config, base, self.commit(), profile='pr-fast')
                self.assertEqual(plan['ordinary_execution'], 'inline')
                self.assertEqual(plan['oasis7_required_wasm_packages'], ['consumer'])
                self.assertIn('launcher_web', plan['groups'])

    def test_component_native_source_mounted_by_build_script_keeps_portable_consumers(self):
        config = self.component_config()
        config['oasis7_required_wasm_packages'] = ['consumer']
        manifest = Path('crates/core/Cargo.toml').read_text()
        for build_path in ('build.rs', 'src/build.rs'):
            self.write('crates/core/Cargo.toml', manifest.replace('[package]', '[package]\nbuild="' + build_path + '"'))
            self.write('crates/core/src/lib.rs', '#[cfg(not(target_arch="wasm32"))] mod behavior;\n'
                       'pub const VALUE: &str = env!("VALUE");\n')
            root = ('#[path="src/behavior.rs"] mod behavior;\n' if build_path == 'build.rs' else 'mod behavior;\n')
            self.write('crates/core/' + build_path, root
                       + 'fn main() { println!("cargo:rustc-env=VALUE={}", behavior::value()); }\n')
            self.write('crates/core/src/behavior.rs', 'pub fn value() -> u8 { 1 }\n')
            self.write('crates/consumer/src/lib.rs', 'pub use core::VALUE;\n')
            base = self.commit()
            self.write('crates/core/src/behavior.rs', 'pub fn value() -> u8 { 2 }\n')
            with self.subTest(build_path=build_path):
                plan = selector.select(config, base, self.commit(), profile='pr-fast')
                self.assertEqual(plan['ordinary_execution'], 'inline')
                self.assertEqual(plan['oasis7_required_wasm_packages'], ['consumer'])
                self.assertIn('launcher_web', plan['groups'])
            Path('crates/core/' + build_path).unlink()

    def test_component_unparsed_explicit_cargo_root_cannot_prove_portable_exclusion(self):
        config = self.component_config()
        config['oasis7_required_wasm_packages'] = ['consumer']
        manifest = Path('crates/consumer/Cargo.toml').read_text()
        for kind in ('build', 'lib', 'bin'):
            if kind == 'build':
                declaration = manifest.replace('[package]', '[package]\nbuild="entry.source"')
            else:
                table = '\n[lib]\n' if kind == 'lib' else '\n[[bin]]\nname="portable"\n'
                declaration = manifest + table + 'path="entry.source"\n'
            self.write('crates/consumer/Cargo.toml', declaration)
            self.write('crates/consumer/entry.source',
                       '#[path="../core/src/behavior.rs"] mod behavior;\nfn main() { behavior::value(); }\n')
            self.write('crates/core/src/lib.rs', '#[cfg(not(target_arch="wasm32"))] mod behavior;\n')
            self.write('crates/core/src/behavior.rs', 'pub fn value() -> u8 { 1 }\n')
            base = self.commit()
            self.write('crates/core/src/behavior.rs', 'pub fn value() -> u8 { 2 }\n')
            with self.subTest(kind=kind):
                plan = selector.select(config, base, self.commit(), profile='pr-fast')
                self.assertEqual(plan['ordinary_execution'], 'inline')
                self.assertEqual(plan['oasis7_required_wasm_packages'], ['consumer'])
                self.assertIn('launcher_web', plan['groups'])

    def test_component_new_module_case_has_actual_selected_library_route(self):
        config = self.component_config()
        self.write('crates/core/src/lib.rs', 'mod added;\n')
        self.write('crates/core/src/added.rs', '#[test] fn positive() {}\n#[test] fn rejection() {}\n')
        plan = selector.select(config, self.base, self.commit(), profile='pr-fast')
        self.assertFalse(plan['test_policy_changed'])
        self.assertEqual(plan['ordinary_execution'], 'inline')

    def test_component_orphan_binary_and_unselected_integration_cannot_use_lib_inventory(self):
        config = self.component_config()
        for path in ('crates/core/src/orphan.rs', 'crates/core/src/bin/tool.rs',
                     'crates/core/tests/new_integration.rs'):
            base = self.git('rev-parse', 'HEAD')
            self.write(path, '#[test] fn behavior() {}\n')
            with self.subTest(path=path):
                plan = selector.select(config, base, self.commit(), profile='pr-fast')
                self.assertNotIn('ordinary_execution', plan)
                self.assertNotIn('ordinary_fast_targets', plan)
                self.assertNotIn('ordinary_clippy_group', plan)
                self.assertNotIn('build_policy_version', plan['architecture'])

    def test_component_unavailable_feature_keeps_expanded_execution(self):
        config = self.component_config()
        self.write('crates/core/src/lib.rs', '#[cfg(feature="test_tier_full")] mod behavior;\n')
        self.write('crates/core/src/behavior.rs', '#[test] fn contract() {}\n')
        base = self.commit()
        self.write('crates/core/src/behavior.rs', '#[test] fn contract() { assert!(true); }\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertTrue(plan['test_policy_changed'])
        self.assertNotIn('ordinary_execution', plan)
        self.assertNotIn('ordinary_clippy_group', plan)
        self.assertNotIn('build_policy_version', plan['architecture'])

    def test_component_cannot_override_exact_wiring_or_explicit_risk(self):
        config = self.component_config()
        config['ordinary_fast_rules'][0]['added_declarations'] = ['mod allowed;']
        self.write('crates/core/src/lib.rs', 'pub fn unrelated_change() {}\n')
        plan = selector.select(config, self.base, self.commit(), profile='pr-fast')
        self.assertNotIn('ordinary_execution', plan)
        config = self.component_config()
        self.write('crates/core/src/lib.rs', 'mod history;\n')
        self.write('crates/core/src/history.rs', 'pub fn revision() -> u8 { 1 }\n')
        base = self.commit()
        self.write('crates/core/src/history.rs', 'pub fn revision() -> u8 { 2 }\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertEqual(plan['ordinary_execution'], 'inline')
        self.assertEqual(plan['formal_suites'], ['world-history'])
        self.assertIn('formal_process', plan['groups'])
        self.assertNotIn('build_policy_version', plan['architecture'])

    def test_component_direct_web_rules_and_explicit_additions_keep_launcher(self):
        config = self.component_config()
        base, head = self.component_fixture()
        plan = selector.select(config, base, head, additions=('launcher_web',), profile='pr-fast')
        self.assertIn('launcher_web', plan['groups'])
        config['rules'].append({'match': ['web/**'], 'groups': ['launcher_web'], 'reason': 'web source'})
        self.write('web/style.css', 'body { color: black; }')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertEqual(plan['ordinary_execution'], 'inline')
        self.assertIn('launcher_web', plan['groups'])

    def test_component_capability_requires_trusted_inventory_and_full_stays_complete(self):
        config = self.component_config()
        base, head = self.component_fixture()
        old = self.bounded_config()
        old_plan = selector.select(old, base, head, profile='pr-fast')
        self.assertNotIn('ordinary_execution', old_plan)
        self.assertNotIn('ordinary_clippy_group', old_plan)
        for profile in ('periodic-full', 'candidate-full'):
            plan = selector.full_plan(config, profile, head, policy_ref=base)
            self.assertNotIn('ordinary_execution', plan)
            self.assertNotIn('rust_baseline_checks', plan)
            self.assertNotIn('ordinary_clippy_group', plan)
            self.assertNotIn('build_policy_version', plan['architecture'])
            self.assertEqual(plan['formal_suites'], sorted(selector.FORMAL_SUITES))
            self.assertEqual(plan['oasis7_required_wasm_packages'], ['core', 'other'])
        for key, value in (('fast_execution_version', True), ('fast_execution_version', 2),
                           ('ordinary_fast_component_rules', [])):
            invalid = dict(config, **{key: value})
            with self.subTest(key=key), self.assertRaises(ValueError):
                selector.validate_config(invalid)
        del config['fast_execution_version']
        with self.assertRaises(ValueError):
            selector.validate_config(config)

    def test_component_clippy_owner_transfer_is_absent_for_shared_controls(self):
        config = self.component_config()
        base, _ = self.component_fixture()
        self.write('Cargo.lock', '# shared dependency input\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertEqual(plan['scope'], 'full')
        self.assertNotIn('ordinary_execution', plan)
        self.assertNotIn('ordinary_clippy_group', plan)
        self.assertNotIn('build_policy_version', plan['architecture'])

    def test_component_keeps_baseline_checks_for_affected_inputs(self):
        config = self.component_config()
        config['rules'].append({'match': ['tools/standalone/**'], 'groups': [], 'reason': 'standalone tool'})
        base, _ = self.component_fixture()
        self.write('tools/standalone/source.py', '# changed standalone input\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertEqual(plan['rust_baseline_checks'], ['standalone_tool_lockfiles'])

    def test_registered_components_keep_cross_crate_behavior_owners(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        state = next(rule for rule in config['ordinary_fast_component_rules']
                     if 'crates/oasis7_world_state/src/**' in rule['match'])
        self.assertEqual(set(state['targets']), {'world-state-lib', 'runtime-lib', 'world-reducers'})
        for target in config['ordinary_fast_targets'].values():
            self.assertTrue(target['cases'])

    def test_bounded_inventory_unmapped_reverse_consumer_retains_full_formal(self):
        config = self.bounded_config()
        self.write('crates/core/src/lib.rs', 'pub fn changed() {}\n')
        self.write('crates/consumer/src/lib.rs', 'pub fn changed() {}\n')
        plan = selector.select(config, self.base, self.commit(), profile='pr-fast')
        self.assertNotIn('ordinary_fast_targets', plan)
        self.assertEqual(plan['formal_suites'], sorted(selector.FORMAL_SUITES))

    def test_bounded_inventory_unrelated_docs_do_not_expand_targets(self):
        config = self.bounded_config()
        self.write('crates/core/src/lib.rs', 'pub fn changed() {}\n')
        self.write('doc/readme.md', 'updated documentation')
        plan = selector.select(config, self.base, self.commit(), profile='pr-fast')
        self.assertEqual(plan['fast_test_inventory_version'], 1)

    def test_bounded_inventory_never_adds_unaffected_target_packages(self):
        config = self.bounded_config()
        config['ordinary_fast_targets']['core-lib']['package'] = 'other'
        config['ordinary_fast_targets']['core-lib']['target_name'] = 'other'
        self.write('crates/core/src/lib.rs', 'pub fn changed() {}\n')
        plan = selector.select(config, self.base, self.commit(), profile='pr-fast')
        self.assertNotIn('ordinary_fast_targets', plan)
        self.assertEqual(plan['oasis7_required_packages'], ['consumer', 'core'])

    def test_bounded_inventory_controls_keep_original_full_scope(self):
        config = self.bounded_config()
        self.write('crates/core/src/lib.rs', 'pub fn changed() {}\n')
        self.write('Cargo.lock', '# shared dependency update')
        plan = selector.select(config, self.base, self.commit(), profile='pr-fast')
        self.assertEqual(plan['scope'], 'full')
        self.assertNotIn('ordinary_fast_targets', plan)
        self.assertEqual(plan['oasis7_required_packages'], ['consumer', 'core', 'other'])

    def test_bounded_inventory_old_policy_cannot_self_enrol_from_candidate(self):
        config = self.bounded_config()
        self.write('crates/core/src/lib.rs', 'pub fn changed() {}\n')
        self.write('scripts/ci-required-scope.json', json.dumps(config))
        plan = selector.select(self.fast_config(), self.base, self.commit(), profile='pr-fast')
        self.assertNotIn('fast_test_inventory_version', plan)
        self.assertNotIn('ordinary_fast_targets', plan)

    def test_fast_cross_package_path_and_include_cannot_authorize_shared_tests(self):
        config = self.fast_config()
        child = 'crates/core/src/lib.rs'
        for reference in ('#[path="../../core/src/lib.rs"] mod shared;',
                          'include!("../../core/src/lib.rs");'):
            self.write(child, '#[test] fn existing() {}\n')
            self.write('crates/other/src/lib.rs', '#[cfg(feature="test_tier_full")]\n' + reference + '\n')
            base = self.commit()
            self.write(child, '#[test] fn existing() {}\n#[test] fn added() {}\n')
            with self.subTest(reference=reference):
                plan = selector.select(config, base, self.commit(), profile='pr-fast')
                self.assertTrue(plan['test_policy_changed'])
                self.assertIn('other', plan['oasis7_required_packages'])

    def test_fast_new_production_helper_excluded_from_harness_is_conservative(self):
        config = self.fast_config()
        self.write('crates/core/src/lib.rs', '#[cfg(not(test))] pub fn production_helper() {}\n')
        plan = selector.select(config, self.base, self.commit(), profile='pr-fast')
        self.assertFalse(plan['test_policy_changed'])
        self.assertEqual(plan['scope'], 'full')

    def test_fast_escaped_module_paths_preserve_all_consumers_and_feature_checks(self):
        config = self.fast_config()
        cases = [
            ('crates/core/tests/support/cases.rs', 'crates/core/tests/full.rs',
             r'#[path="support/ca\x73es.rs"] mod expanded;'),
            ('crates/core/src/lib.rs', 'crates/other/src/lib.rs',
             r'#[path="../../core/src/li\x62.rs"] mod expanded;'),
        ]
        for child, parent, link in cases:
            self.write('crates/core/tests/ordinary.rs', '#[path="support/cases.rs"] mod ordinary;\n')
            self.write(child, '#[test] fn existing() {}\n')
            self.write(parent, '#[cfg(feature="test_tier_full")]\n' + link + '\n')
            base = self.commit()
            self.write(child, '#[test] fn existing() {}\n#[test] fn added() {}\n')
            with self.subTest(link=link):
                plan = selector.select(config, base, self.commit(), profile='pr-fast')
                self.assertEqual(plan['scope'], 'full')
                self.assertEqual(set(plan['groups']), set(config['groups']))
                self.assertEqual(plan['oasis7_required_packages'], ['consumer', 'core', 'other'])
                self.assertTrue(plan['test_policy_changed'])

    def test_fast_inline_raw_identifier_preserves_full_parent_context(self):
        config = self.fast_config()
        self.write('crates/core/src/lib.rs', '#[path="type/cases.rs"] mod ordinary;\n'
                   'mod r#type { #[cfg(feature="test_tier_full")] mod cases; }\n')
        child = 'crates/core/src/type/cases.rs'
        self.write(child, '#[test] fn existing() {}\n')
        base = self.commit()
        self.write(child, '#[test] fn existing() {}\n#[test] fn added() {}\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertTrue(plan['test_policy_changed'])

    def test_fast_known_include_chain_keeps_native_module_additions_ordinary(self):
        config = self.fast_config()
        self.write('crates/core/src/lib.rs', '#[cfg(not(target_arch="wasm32"))] include!("native.rs");\n')
        self.write('crates/core/src/native.rs', 'mod world;\n')
        self.write('crates/core/src/world.rs', 'pub fn existing() {}\n')
        base = self.commit()
        self.write('crates/core/src/world.rs', 'pub fn existing() {}\n'
                   '#[cfg(not(target_arch="wasm32"))] pub fn added() {}\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertEqual(plan['scope'], 'targeted')
        self.assertFalse(plan['test_policy_changed'])
        self.assertEqual(plan['oasis7_required_packages'], ['consumer', 'core'])

    def test_fast_new_external_case_cannot_inherit_a_disabled_parent(self):
        config = self.fast_config()
        child = 'crates/core/src/checks.rs'
        for gate in ('windows', 'all(test, feature="unexecuted")'):
            self.write('crates/core/src/lib.rs', f'#[cfg({gate})] mod checks;\n')
            self.write(child, '#[test] fn existing() {}\n')
            base = self.commit()
            self.write(child, '#[test] fn existing() {}\n#[test] fn added() {}\n')
            with self.subTest(gate=gate), self.assertRaisesRegex(
                    selector.TestPolicyError, 'validated test feature combinations'):
                selector.select(config, base, self.commit(), profile='pr-fast')

    def test_fast_escaped_unknown_parent_gate_cannot_hide_new_cases_in_full_scope(self):
        config = self.fast_config()
        manifest = Path('crates/core/Cargo.toml').read_text()
        self.write('crates/core/Cargo.toml', manifest + 'custom_off=[]\n')
        child = 'crates/core/src/cases.rs'
        for reference in (r'#[path="ca\x73es.rs"] mod child;',
                          r'#[cfg_attr(test, path="ca\x73es.rs")] mod child;',
                          r'include!("ca\x73es.rs");'):
            self.write('crates/core/src/lib.rs', '#[cfg(feature="custom_off")]\n' + reference + '\n')
            self.write(child, '#[test] fn existing() {}\n')
            base = self.commit()
            self.write(child, '#[test] fn existing() {}\n#[test] fn added() {}\n')
            with self.subTest(reference=reference), self.assertRaisesRegex(
                    selector.TestPolicyError, 'validated test feature combinations'):
                selector.select(config, base, self.commit(), profile='pr-fast')

    def test_fast_inline_include_keeps_every_parent_gate(self):
        config = self.fast_config()
        manifest = Path('crates/core/Cargo.toml').read_text()
        self.write('crates/core/Cargo.toml', manifest + 'custom_off=[]\n')
        child = 'crates/core/src/cases.rs'
        for feature in ('test_tier_full', 'custom_off'):
            self.write('crates/core/src/lib.rs', '#[path="cases.rs"] mod ordinary;\n'
                       f'#[cfg(feature="{feature}")] mod expanded {{ include!("cases.rs"); }}\n')
            self.write(child, '#[test] fn existing() {}\n')
            base = self.commit()
            self.write(child, '#[test] fn existing() {}\n#[test] fn added() {}\n')
            with self.subTest(feature=feature):
                if feature == 'custom_off':
                    with self.assertRaisesRegex(selector.TestPolicyError, 'validated test feature combinations'):
                        selector.select(config, base, self.commit(), profile='pr-fast')
                else:
                    plan = selector.select(config, base, self.commit(), profile='pr-fast')
                    self.assertTrue(plan['test_policy_changed'])

    def test_fast_inline_path_context_cannot_hide_an_additional_parent_gate(self):
        config = self.fast_config()
        manifest = Path('crates/core/Cargo.toml').read_text()
        self.write('crates/core/Cargo.toml', manifest + 'custom_off=[]\n')
        child = 'crates/core/src/full/cases.rs'
        for feature in ('test_tier_full', 'custom_off'):
            self.write('crates/core/src/lib.rs', '#[path="full/cases.rs"] mod ordinary;\n'
                       '#[path="full"] mod expanded {\n'
                       f'    #[cfg(feature="{feature}")] mod cases;\n}}\n')
            self.write(child, '#[test] fn existing() {}\n')
            base = self.commit()
            self.write(child, '#[test] fn existing() {}\n#[test] fn added() {}\n')
            with self.subTest(feature=feature):
                if feature == 'custom_off':
                    with self.assertRaisesRegex(selector.TestPolicyError, 'validated test feature combinations'):
                        selector.select(config, base, self.commit(), profile='pr-fast')
                else:
                    plan = selector.select(config, base, self.commit(), profile='pr-fast')
                    self.assertTrue(plan['test_policy_changed'])
                    self.assertEqual(plan['scope'], 'full')

    def test_fast_support_integration_addition_requires_expanded_target_execution(self):
        config = self.fast_config()
        package = 'oasis7_agent_harness'
        config['groups'].append('workspace_support')
        config['resources']['workspace_support'] = []
        config['package_groups'][package] = ['workspace_support']
        self.write(f'crates/{package}/Cargo.toml', f'[package]\nname="{package}"\nversion="0.1.0"\n')
        self.write(f'crates/{package}/src/lib.rs', '')
        base = self.commit()
        path = f'crates/{package}/tests/new.rs'
        self.write(path, '#[test] fn integration_regression() {}\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertTrue(plan['test_policy_changed'])
        self.assertEqual(plan['test_policy_paths'], [path])
        self.assertEqual(plan['workspace_support_profiles'], [package])

    def test_fast_added_plain_case_does_not_mask_an_existing_tier_change(self):
        config = self.fast_config()
        path = 'crates/core/src/lib.rs'
        self.write(path, '#[cfg(feature = "test_tier_required")] #[test] fn regression() {}\n')
        base = self.commit()
        self.write(path, '#[cfg(feature = "test_tier_full")] #[test] fn regression() {}\n'
                         '#[test] fn additional_ordinary_case() {}\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertTrue(plan['test_policy_changed'])
        self.assertEqual(plan['test_policy_paths'], [path])
        self.assertTrue(any('full features' in reason for reason in plan['fast_plan_exceptions']))

    def test_fast_new_full_cases_keep_direct_module_and_file_gate_coverage(self):
        config = self.fast_config()
        path = 'crates/core/src/lib.rs'
        cases = [
            ('#[test] fn existing() {}\n',
             '#[test] fn existing() {}\n#[cfg(feature = "test_tier_full")] #[test] fn added() {}\n'),
            ('#[cfg(feature = "test_tier_full")] mod tests {}\n',
             '#[cfg(feature = "test_tier_full")] mod tests { #[test] fn added() {} }\n'),
            ('#![cfg(feature = "test_tier_full")]\n',
             '#![cfg(feature = "test_tier_full")]\n#[test] fn added() {}\n'),
        ]
        for before, after in cases:
            with self.subTest(after=after):
                self.write(path, before)
                base = self.commit()
                self.write(path, after)
                plan = selector.select(config, base, self.commit(), profile='pr-fast')
                self.assertTrue(plan['test_policy_changed'])
                self.assertEqual(plan['test_policy_paths'], [path])
                self.assertTrue(any('full features' in reason for reason in plan['fast_plan_exceptions']))

    def test_fast_new_file_level_test_gate_is_ordinary_but_full_still_expands(self):
        config = self.fast_config()
        for index, gate in enumerate(('test', 'feature = "test_tier_full"')):
            path = f'crates/core/tests/added_{index}.rs'
            base = self.git('rev-parse', 'HEAD')
            self.write(path, f'#![cfg({gate})]\n#[test] fn added() {{}}\n')
            plan = selector.select(config, base, self.commit(), profile='pr-fast')
            expanded = gate != 'test'
            self.assertEqual(plan['test_policy_changed'], expanded)
            self.assertEqual(plan['test_policy_paths'], [path] if expanded else [])
            self.assertEqual(plan['oasis7_required_packages'], ['core'])

    def test_fast_plain_case_in_external_module_keeps_unresolved_parent_gate_coverage(self):
        config = self.fast_config()
        self.write('crates/core/src/lib.rs', '#[cfg(feature = "test_tier_full")] mod checks;\n')
        path = 'crates/core/src/checks.rs'
        self.write(path, '#[test] fn existing() {}\n')
        base = self.commit()
        self.write(path, '#[test] fn existing() {}\n#[test] fn added() {}\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertTrue(plan['test_policy_changed'])
        self.assertEqual(plan['test_policy_paths'], [path])

    def test_fast_plain_integration_addition_keeps_cargo_target_gates_conservative(self):
        config = self.fast_config()
        path = 'crates/core/tests/behavior.rs'
        self.write(path, '#[test] fn existing() {}\n')
        manifest = Path('crates/core/Cargo.toml').read_text()
        base = self.commit()
        self.write(path, '#[test] fn existing() {}\n#[test] fn ordinary_added() {}\n')
        ordinary = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertFalse(ordinary['test_policy_changed'])
        self.assertEqual(ordinary['oasis7_required_packages'], ['core'])
        self.write('crates/core/Cargo.toml', manifest + '\n[[test]]\nname="behavior"\npath="tests/behavior.rs"\n'
                   'required-features=["test_tier_full"]\n')
        base = self.commit()
        self.write(path, '#[test] fn existing() {}\n#[test] fn ordinary_added() {}\n#[test] fn full_added() {}\n')
        gated = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertTrue(gated['test_policy_changed'])
        self.assertEqual(gated['test_policy_paths'], [path])

    def integration_target_fixture(self, declarations='', *, edition='edition="2024"', features=None):
        # A dependency-free real Cargo workspace also acts as the execution
        # oracle; the selector still reads immutable Git revisions.
        self.write('Cargo.toml', '[workspace]\nmembers=["crates/core"]\nresolver="2"\n'
                   '[workspace.package]\nedition="2024"\n')
        self.write('crates/core/Cargo.toml', '[package]\nname="core"\nversion="0.1.0"\n' + edition +
                   '\n[features]\n' + (features if features is not None else
                   'test_tier_required=[]\ntest_tier_full=["test_tier_required"]\n') + declarations)
        self.write('crates/core/src/lib.rs', '')

    def test_explicit_full_target_keeps_automatic_file_and_directory_siblings(self):
        self.integration_target_fixture(
            '\n[[test]]\nname="full"\nrequired-features=["test_tier_full"]\n',
            edition='edition.workspace=true')
        self.write('crates/core/tests/full.rs',
                   '#[cfg(not(feature="test_tier_full"))] compile_error!("full target compiled");\n'
                   '#[test] fn full_case() {}\n')
        for name, relative in (('smoke', 'tests/smoke.rs'), ('nested', 'tests/nested/main.rs')):
            path = 'crates/core/' + relative
            self.write(path, '#[test] fn existing() {}\n')
            base = self.commit()
            self.write(path, '#[test] fn existing() {}\n#[test] fn added() {}\n')
            head = self.commit()
            with self.subTest(target=name):
                legacy = selector.select(self.fast_config(), base, head, profile='pr-fast')
                self.assertFalse(legacy['test_policy_changed'])
                config = self.bounded_config((path,))
                config['ordinary_fast_targets']['core-lib'].update(target_kind='test', target_name=name)
                selected = selector.select(config, base, head, profile='pr-fast')
                self.assertFalse(selected['test_policy_changed'])
                self.assertIn('ordinary_fast_targets', selected)
                output = self.ordinary_route_cargo(
                    ['-p', 'core', '--test', name, '--no-default-features', '--features', 'test_tier_required'])
                self.assertIn('added ... ok', output)
                self.assertIn('2 passed; 0 failed', output)

    def test_explicit_test_roots_match_cargo_paths_and_overrides(self):
        self.integration_target_fixture('\n[[test]]\nname="declared"\npath="custom/entry.rs"\n'
                                        '\n[[test]]\nname="nested"\n',
                                        edition='edition="2024"\nautotests=false')
        for relative in ('custom/entry.rs', 'tests/nested/main.rs', 'tests/automatic.rs'):
            self.write('crates/core/' + relative, '#[test] fn existing() {}\n')
        sources = {str(path): path.read_bytes() for path in Path('crates/core').rglob('*.rs')}
        manifest = selector.tomllib.loads(Path('crates/core/Cargo.toml').read_text())
        expected = {'declared': 'crates/core/custom/entry.rs', 'nested': 'crates/core/tests/nested/main.rs'}
        self.assertEqual({name: value[0] for name, value in
                          selector.integration_test_targets(sources, 'crates/core', manifest).items()}, expected)
        metadata = json.loads(subprocess.check_output(
            ['cargo', 'metadata', '--no-deps', '--offline', '--format-version', '1'], text=True))
        actual = {target['name']: str(Path(target['src_path']).relative_to(Path.cwd()))
                  for target in metadata['packages'][0]['targets'] if target['kind'] == ['test']}
        self.assertEqual(actual, expected)
        # Both the declared name and its path override automatic discovery;
        # neither dormant file can independently witness a selected target.
        manifest['package']['autotests'] = True
        manifest['test'][0].update(name='automatic', path='tests/nested/main.rs')
        manifest['test'].pop()
        self.assertEqual({name: value[0] for name, value in
                          selector.integration_test_targets(sources, 'crates/core', manifest).items()},
                         {'automatic': 'crates/core/tests/nested/main.rs'})

    def test_overridden_automatic_path_keeps_its_actual_module_directory(self):
        # With name=foo the automatic foo target is overridden; name=driver
        # leaves foo as another real target. Neither gives foo crate semantics
        # when it is imported as a module by the selected driver.
        for name in ('foo', 'driver'):
            self.integration_target_fixture('\n[[test]]\nname="' + name + '"\npath="tests/driver.rs"\n')
            self.write('crates/core/tests/driver.rs', 'mod foo;\n#[test] fn existing() {}\n')
            self.write('crates/core/tests/foo.rs', 'mod checks;\n')
            self.write('crates/core/tests/foo/checks.rs', '#[test] fn actual_child() {}\n')
            path = 'crates/core/tests/checks.rs'
            self.write(path, '#[test] fn unselected() { assert_eq!(1, 1); }\n')
            base = self.commit()
            self.write(path, '#[test] fn unselected() { assert_eq!(2, 2); }\n')
            config = self.bounded_config((path,))
            config['ordinary_fast_targets']['core-lib'].update(target_kind='test', target_name=name)
            with self.subTest(target=name):
                plan = selector.select(config, base, self.commit(), profile='pr-fast')
                self.assertTrue(plan['test_policy_changed'])
                self.assertNotIn('ordinary_fast_targets', plan)
                output = self.ordinary_route_cargo(
                    ['-p', 'core', '--test', name, '--no-default-features', '--features', 'test_tier_required'])
                self.assertIn('foo::checks::actual_child ... ok', output)
                self.assertNotIn('unselected', output)

    def test_full_target_imported_as_module_uses_the_selected_root_context(self):
        self.integration_target_fixture(
            '\n[[test]]\nname="full"\npath="custom/foo.rs"\nrequired-features=["test_tier_full"]\n'
            '\n[[test]]\nname="driver"\npath="custom/driver.rs"\n')
        self.write('crates/core/custom/driver.rs', 'mod foo;\n#[test] fn existing() {}\n')
        self.write('crates/core/custom/foo.rs', 'mod checks;\n')
        actual, decoy = 'crates/core/custom/foo/checks.rs', 'crates/core/custom/checks.rs'
        self.write(actual, '#[test] fn actual_child() { assert_eq!(1, 1); }\n')
        self.write(decoy, '#[test] fn other_target_child() { assert_eq!(1, 1); }\n')
        base = self.commit()
        for path, expected in ((actual, True), (decoy, False)):
            self.write(path, Path(path).read_text().replace('(1, 1)', '(2, 2)'))
            config = self.bounded_config((path,))
            config['ordinary_fast_targets']['core-lib'].update(target_kind='test', target_name='driver')
            head = self.commit()
            with self.subTest(path=path):
                plan = selector.select(config, base, head, profile='pr-fast')
                self.assertEqual('ordinary_fast_targets' in plan, expected)
                self.assertEqual(plan['test_policy_changed'], not expected)
            base = head
        output = self.ordinary_route_cargo(
            ['-p', 'core', '--test', 'driver', '--no-default-features', '--features', 'test_tier_required'])
        self.assertIn('foo::checks::actual_child ... ok', output)
        self.assertNotIn('other_target_child', output)

    def test_selected_module_directory_must_match_its_actual_mount(self):
        for name, literal in (('helper', True), ('lib', False), ('main', False), ('mod', False)):
            self.integration_target_fixture('\n[[test]]\nname="full"\nrequired-features=["test_tier_full"]\n')
            self.write('crates/core/tests/full.rs', '#[test] fn full_case() {}\n')
            module_name = 'r#mod' if name == 'mod' else name
            mount = '#[path="helper.rs"] mod helper;' if literal else 'mod ' + module_name + '; // tests'
            self.write('crates/core/tests/probe.rs', mount + '\n#[test] fn existing() {}\n')
            self.write('crates/core/tests/' + name + '.rs', 'mod checks;\n')
            sibling, nested = 'crates/core/tests/checks.rs', 'crates/core/tests/' + name + '/checks.rs'
            actual, decoy = (sibling, nested) if literal else (nested, sibling)
            self.write(actual, '#[test] fn actual_child() {}\n')
            self.write(decoy, '#[test] fn decoy() { assert_eq!(1, 1); }\n')
            base = self.commit()
            self.write(decoy, '#[test] fn decoy() { assert_eq!(2, 2); }\n')
            config = self.bounded_config((decoy,))
            config['ordinary_fast_targets']['core-lib'].update(target_kind='test', target_name='probe')
            with self.subTest(name=name, literal=literal):
                plan = selector.select(config, base, self.commit(), profile='pr-fast')
                self.assertTrue(plan['test_policy_changed'])
                self.assertNotIn('ordinary_fast_targets', plan)
                output = self.ordinary_route_cargo(
                    ['-p', 'core', '--test', 'probe', '--no-default-features', '--features', 'test_tier_required'])
                self.assertIn(module_name + '::checks::actual_child ... ok', output)
                self.assertNotIn('decoy', output)

    def test_integration_target_ambiguity_and_legacy_auto_settings_are_conservative(self):
        sources = {'crates/core/tests/smoke.rs': b'', 'crates/core/tests/full.rs': b''}
        manifest = {'package': {'name': 'core', 'edition': '2024'},
                    'test': [{'name': 'full', 'required-features': ['test_tier_full']}]}
        for change in ('duplicate', 'ambiguous', 'missing', 'unknown-edition', 'invalid-auto'):
            candidate, files = copy.deepcopy(manifest), dict(sources)
            if change == 'duplicate':
                candidate['test'].append(dict(candidate['test'][0]))
            elif change == 'ambiguous':
                files['crates/core/tests/full/main.rs'] = b''
            elif change == 'missing':
                del files['crates/core/tests/full.rs']
            elif change == 'unknown-edition':
                candidate['package']['edition'] = '2099'
            else:
                candidate['package']['autotests'] = 'true'
            with self.subTest(change=change):
                self.assertEqual(selector.integration_test_targets(files, 'crates/core', candidate), {})
        for edition in (None, '2015'):
            manifest['package'].pop('edition', None)
            if edition is not None:
                manifest['package']['edition'] = edition
            self.assertNotIn('smoke', selector.integration_test_targets(sources, 'crates/core', manifest))
            manifest['package']['autotests'] = True
            self.assertIn('smoke', selector.integration_test_targets(sources, 'crates/core', manifest))
            manifest['package'].pop('autotests')

    def test_selected_manifest_gate_uses_local_feature_closure_and_root_directory(self):
        self.integration_target_fixture(
            '\n[[test]]\nname="selected"\npath="custom/entry.rs"\nrequired-features=["hidden"]\n',
            features='test_tier_required=["alias"]\nalias=["hidden"]\nhidden=[]\n'
                     'test_tier_full=["test_tier_required"]\n')
        self.write('crates/core/custom/entry.rs', 'mod checks;\n')
        path = 'crates/core/custom/checks.rs'
        self.write(path, '#[test] fn existing() {}\n')
        base = self.commit()
        self.write(path, '#[test] fn existing() {}\n#[test] fn added() {}\n')
        config = self.bounded_config((path,))
        config['ordinary_fast_targets']['core-lib'].update(target_kind='test', target_name='selected')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertFalse(plan['test_policy_changed'])
        self.assertIn('ordinary_fast_targets', plan)
        output = self.ordinary_route_cargo(
            ['-p', 'core', '--test', 'selected', '--no-default-features', '--features', 'test_tier_required'])
        self.assertIn('checks::added ... ok', output)

    def test_selected_cfg_and_manifest_gate_share_actual_defaults_and_aliases(self):
        for default_features, enabled in ((False, False), (True, True)):
            self.integration_target_fixture(
                '\n[[test]]\nname="selected"\nrequired-features=["hidden"]\n',
                features='default=["alias"]\nalias=["hidden"]\nhidden=[]\n'
                         'test_tier_required=[]\ntest_tier_full=["test_tier_required"]\n')
            path = 'crates/core/tests/selected.rs'
            self.write(path, '#![cfg(feature="hidden")]\n#[test] fn existing() { assert_eq!(1, 1); }\n')
            base = self.commit()
            self.write(path, '#![cfg(feature="hidden")]\n#[test] fn existing() { assert_eq!(2, 2); }\n')
            config = self.bounded_config((path,))
            config['ordinary_fast_targets']['core-lib'].update(
                target_kind='test', target_name='selected', default_features=default_features)
            with self.subTest(default_features=default_features):
                plan = selector.select(config, base, self.commit(), profile='pr-fast')
                self.assertEqual('ordinary_fast_targets' in plan, enabled)
                self.assertEqual(plan['test_policy_changed'], not enabled)

    def test_transitive_feature_cannot_prove_a_negated_selected_route(self):
        self.integration_target_fixture(features='test_tier_required=["hidden"]\nhidden=[]\n'
                                        'test_tier_full=["test_tier_required"]\n')
        self.write('crates/core/src/lib.rs', '#[cfg(all(test, not(feature="hidden")))] mod checks;\n')
        path = 'crates/core/src/checks.rs'
        self.write(path, '#[test] fn existing() { assert_eq!(1, 1); }\n')
        base = self.commit()
        self.write(path, '#[test] fn existing() { assert_eq!(2, 2); }\n')
        plan = selector.select(self.bounded_config((path,)), base, self.commit(), profile='pr-fast')
        self.assertTrue(plan['test_policy_changed'])
        self.assertNotIn('ordinary_fast_targets', plan)
        output = self.ordinary_route_cargo(
            ['-p', 'core', '--lib', '--no-default-features', '--features', 'test_tier_required'], list_only=True)
        self.assertNotIn('checks::existing', output)
        self.assertIn('0 tests', output)

    def test_fast_unproven_new_case_gates_or_macro_context_are_not_ordinary_additions(self):
        config = self.fast_config()
        path = 'crates/core/src/lib.rs'
        self.write(path, '#[cfg(feature = "unknown_gate")] mod tests {}\n')
        base = self.commit()
        self.write(path, '#[cfg(feature = "unknown_gate")] mod tests { #[test] fn added() {} }\n')
        with self.assertRaisesRegex(selector.TestPolicyError, 'validated test feature combinations'):
            selector.select(config, base, self.commit(), profile='pr-fast')
        self.write(path, '#[test] fn existing() {}\n')
        base = self.commit()
        for addition in ('#[unknown_macro] #[test] fn added() {}\n',
                         'unknown_macro! { #[test] fn added() {} }\n'):
            with self.subTest(addition=addition):
                self.write(path, '#[test] fn existing() {}\n' + addition)
                plan = selector.select(config, base, self.commit(), profile='pr-fast')
                self.assertTrue(plan['test_policy_changed'])

    def test_fast_test_owner_deletion_or_rename_still_requires_review(self):
        config = self.fast_config()
        path = 'crates/core/src/lib.rs'
        self.write(path, '#[test] fn original() {}\n')
        base = self.commit()
        for after in ('', '#[test] fn renamed() {}\n'):
            with self.subTest(after=after):
                self.write(path, after)
                plan = selector.select(config, base, self.commit(), profile='pr-fast')
                self.assertTrue(plan['test_policy_changed'])
                self.assertEqual(plan['test_policy_paths'], [path])
                self.assertTrue(any('independent retirement review' in reason for reason in plan['fast_plan_exceptions']))

    def test_fast_item_macros_keep_full_coverage_for_every_delimiter(self):
        config = self.fast_config()
        path = 'crates/core/src/lib.rs'
        before = ('macro_rules! gated { ($($item:item)*) => { '
                  '#[cfg(feature = "test_tier_full")] mod generated { $($item)* } }; }\n')
        self.write(path, before)
        base = self.commit()
        for opening, closing in (('(', ')'), ('[', ']'), ('{', '}')):
            with self.subTest(opening=opening):
                self.write(path, before + 'gated!' + opening + '#[test] fn added() {}' + closing + ';\n')
                plan = selector.select(config, base, self.commit(), profile='pr-fast')
                self.assertTrue(plan['test_policy_changed'])
                self.assertEqual(plan['test_policy_paths'], [path])

    def test_fast_test_tier_downgrade_requires_full_feature_execution(self):
        config = self.fast_config()
        path = 'crates/core/src/lib.rs'
        self.write(path, '#[cfg(feature = "test_tier_required")]\n#[test]\nfn regression() {}\n')
        base = self.commit()
        self.write(path, '#[cfg(feature = "test_tier_full")]\n#[test]\nfn regression() {}\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertTrue(plan['test_policy_changed'])
        self.assertEqual(plan['test_policy_paths'], [path])
        self.assertIn('core', plan['oasis7_required_packages'])
        self.assertTrue(any('full features' in reason for reason in plan['fast_plan_exceptions']))

    def test_fast_unmapped_ignore_is_rejected_instead_of_silently_skipped(self):
        config = self.fast_config()
        path = 'crates/core/src/lib.rs'
        self.write(path, '#[test]\nfn regression() {}\n')
        base = self.commit()
        for annotation in ('#[ignore]', '#[cfg_attr(feature = "test_tier_full", ignore)]'):
            with self.subTest(annotation=annotation):
                self.write(path, f'#[test]\n{annotation}\nfn regression() {{}}\n')
                with self.assertRaisesRegex(selector.TestPolicyError, 'trusted execution mapping'):
                    selector.select(config, base, self.commit(), profile='pr-fast')

    def ordinary_route_cargo(self, arguments, *, list_only=False):
        environment = dict(os.environ)
        environment.pop('RUSTC_WRAPPER', None)
        command = ['cargo', 'test', *arguments, '--offline']
        if list_only:
            command += ['--', '--list']
        result = subprocess.run(command, env=environment, text=True, capture_output=True, timeout=60)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result.stdout

    def test_fast_cross_module_same_function_name_cannot_hide_test_tier_move(self):
        config = self.fast_config()
        path = 'crates/core/src/lib.rs'
        self.write(path, 'mod first { #[cfg(feature = "test_tier_full")] #[test] fn regression() {} }\n'
                         'mod second { #[test] fn regression() {} }\n')
        base = self.commit()
        self.write(path, 'mod first { #[test] fn regression() {} }\n'
                         'mod second { #[cfg(feature = "test_tier_full")] #[test] fn regression() {} }\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertTrue(plan['test_policy_changed'])
        self.assertEqual(plan['test_policy_paths'], [path])

    def test_fast_tier_feature_requires_actual_expanded_driver_for_owning_package(self):
        config = self.fast_config()
        # This package has its own necessary group but no ordinary/support
        # runner that consumes test_policy_changed. Its dependent packages do
        # not authorize a tier migration in this package's test harness.
        config['oasis7_required_packages'].remove('other')
        config['package_groups']['other'] = ['other']
        path = 'crates/other/src/lib.rs'
        self.write(path, '#[test]\nfn regression() {}\n')
        manifest = Path('crates/other/Cargo.toml').read_text()
        self.write('crates/other/Cargo.toml', manifest.replace('test_tier_full=["test_tier_required"]\n', ''))
        base = self.commit()
        self.write('crates/other/Cargo.toml', manifest)
        self.write(path, '#[cfg(feature = "test_tier_full")]\n#[test]\nfn regression() {}\n')
        with self.assertRaisesRegex(selector.TestPolicyError, 'no expanded-feature driver'):
            selector.select(config, base, self.commit(), profile='pr-fast')

    def test_fast_tier_feature_must_be_declared_by_the_actual_executing_package(self):
        config = self.fast_config()
        path = 'crates/core/src/lib.rs'
        self.write(path, '#[test]\nfn regression() {}\n')
        manifest = Path('crates/core/Cargo.toml').read_text()
        self.write('crates/core/Cargo.toml', manifest.replace('test_tier_full=["test_tier_required"]\n', ''))
        base = self.commit()
        self.write(path, '#[cfg(feature = "test_tier_full")]\n#[test]\nfn regression() {}\n')
        with self.assertRaisesRegex(selector.TestPolicyError, 'not declared by its executing package'):
            selector.select(config, base, self.commit(), profile='pr-fast')

    def test_fast_production_import_alias_cfg_preserves_actual_test_mount(self):
        config = self.fast_config()
        self.write("crates/core/src/lib.rs", "mod production; mod history;\n#[cfg(test)] mod test_bridge;\n")
        self.write("crates/core/src/production.rs", "pub fn value() {}\n")
        self.write("crates/core/src/test_bridge.rs", "pub fn value() {}\n")
        self.write("crates/core/src/history.rs", "#[test] fn history() {}\n")
        base = self.commit()
        for test_gate in ("test", 'all(test, any(target_os = "linux", target_os = "macos"))'):
            self.write("crates/core/src/history.rs", f"#[cfg(not(test))] use super::production as bridge;\n#[cfg({test_gate})] use super::test_bridge as bridge;\n#[test] fn history() {{ bridge::value(); }}\n")
            with self.subTest(test_gate=test_gate):
                plan = selector.select(config, base, self.commit(), profile="pr-fast")
                self.assertEqual(plan["scope"], "full")
                self.assertIn("world-history", plan["formal_suites"])
                self.assertTrue(any("production cfg changed" in reason for reason in plan["reasons"]))

    def test_fast_production_import_cfg_allows_proven_alternative_mounts(self):
        config = self.fast_config()
        self.write("crates/core/src/lib.rs", 'mod production; #[cfg(not(test))] mod history;\n#[cfg(test)] #[path="history.rs"] mod history_tests;\n#[cfg(test)] mod test_bridge;\n')
        self.write("crates/core/src/production.rs", "pub fn value() {}\n")
        self.write("crates/core/src/test_bridge.rs", "pub fn value() {}\n")
        self.write("crates/core/src/history.rs", "pub fn existing() {}\n")
        base = self.commit()
        self.write("crates/core/src/history.rs", '#[cfg(not(test))] use super::production as bridge;\n#[cfg(all(test,any(target_os="linux",target_os="macos")))] use super::test_bridge as bridge;\n#[test] fn history() { bridge::value(); }\n')
        plan = selector.select(config, base, self.commit(), profile="pr-fast")
        self.assertEqual(plan["scope"],"full")
        self.assertIn("world-history",plan["formal_suites"])

    def test_fast_production_import_cfg_uses_real_default_features(self):
        config = self.fast_config()
        manifest = Path("crates/core/Cargo.toml")
        self.write(str(manifest), manifest.read_text().replace('[features]\n', '[features]\ndefault=["native"]\nnative=[]\n'))
        self.write("crates/core/src/lib.rs", '#[cfg(all(not(target_arch="wasm32"), feature="native"))] mod host;\n')
        self.write("crates/core/src/host.rs", 'mod production; #[cfg(not(test))] mod history;\n#[cfg(all(test, unix))] #[path="host/history.rs"] mod history_tests;\n')
        self.write("crates/core/src/host/production.rs", "pub fn value() {}\n")
        self.write("crates/core/src/host/history.rs", "pub fn existing() {}\n")
        base = self.commit()
        self.write("crates/core/src/host/history.rs", "#[cfg(not(test))] use super::production as bridge;\n#[test] fn existing() {}\n")
        plan = selector.select(config, base, self.commit(), profile="pr-fast")
        self.assertEqual(plan["scope"], "full")
        self.assertTrue(any("production cfg changed" in reason for reason in plan["reasons"]))

    def test_fast_production_import_cfg_requires_real_feature_driver(self):
        config = self.fast_config()
        config['oasis7_required_packages'].remove('core')
        self.write("crates/core/src/lib.rs", 'mod production; #[cfg(not(test))] mod history;\n#[cfg(all(test, feature="test_tier_full"))] #[path="history.rs"] mod history_tests;\n')
        self.write("crates/core/src/production.rs", "pub fn value() {}\n")
        self.write("crates/core/src/history.rs", "pub fn existing() {}\n")
        base = self.commit()
        self.write("crates/core/src/history.rs", "#[cfg(not(test))] use super::production as bridge;\n#[test] fn hidden() {}\n")
        with self.assertRaises(selector.TestPolicyError):
            selector.select(config, base, self.commit(), profile="pr-fast")

    def test_fast_production_import_cfg_keeps_unknown_and_hidden_tests_rejected(self):
        config = self.fast_config()
        self.write("crates/core/src/lib.rs", "mod production; mod history;\n#[cfg(test)] mod test_bridge;\n")
        self.write("crates/core/src/production.rs", "pub fn value() {}\n")
        self.write("crates/core/src/test_bridge.rs", "pub fn value() {}\n")
        self.write("crates/core/src/history.rs", "#[test] fn history() {}\n")
        base = self.commit()
        alternatives = [
            "#[cfg(not(test))] use super::missing as bridge;\n",
            "#[cfg_attr(not(test), cfg(any()))] use super::production as bridge;\n",
            "#[cfg(all(not(test), mystery))] use super::production as bridge;\n",
            "#[cfg(all(not(test), any()))] use super::production as bridge;\n",
            "#[cfg(all(not(test), feature=\"undeclared\"))] use super::production as bridge;\n",
            "fn helper() { #[cfg(not(test))] use super::production as bridge; }\n",
            "macro_rules! inject { () => { #[cfg(not(test))] use super::production as bridge; } }\n",
            "#[cfg(not(test))] use super::production::{value};\n",
            "#[cfg(not(test))] #[test] fn hidden() {}\n",
            "#[cfg(not(test))] mod hidden { #[test] fn hidden() {} }\n",
        ]
        for addition in alternatives:
            self.write("crates/core/src/history.rs", addition + "#[test] fn history() {}\n")
            with self.subTest(addition=addition), self.assertRaises(selector.TestPolicyError):
                selector.select(config, base, self.commit(), profile="pr-fast")

    def test_fast_production_import_cfg_requires_actual_test_mount(self):
        config = self.fast_config()
        self.write("crates/core/src/lib.rs", "mod production; #[cfg(not(test))] mod history;\n")
        self.write("crates/core/src/production.rs", "pub fn value() {}\n")
        self.write("crates/core/src/history.rs", "pub fn existing() {}\n")
        base = self.commit()
        self.write("crates/core/src/history.rs", "#[cfg(not(test))] use super::production as bridge;\n#[test] fn hidden() {}\n")
        with self.assertRaises(selector.TestPolicyError):
            selector.select(config, base, self.commit(), profile="pr-fast")

    def test_fast_production_platform_cfg_expands_without_test_policy_rejection(self):
        config = self.fast_config()
        self.write('crates/core/src/lib.rs', 'pub fn product_path() {}\n')
        base = self.commit()
        self.write('crates/core/src/lib.rs', '#[cfg(unix)]\npub fn product_path() {}\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertEqual(plan['scope'], 'full')
        self.assertEqual(set(plan['groups']), set(config['groups']))
        self.assertFalse(plan['test_policy_changed'])
        self.assertTrue(any('production cfg changed' in reason for reason in plan['reasons']))

    def test_fast_same_ignored_short_name_in_other_binary_does_not_authorize_skip(self):
        config = self.fast_config()
        inventory = {'suites': {name: [] for name in selector.FORMAL_SUITES}}
        inventory['suites']['runtime-protocol'] = [
            {'binary': 'unrelated-tests', 'name': 'unrelated::regression', 'ignored': True}]
        self.write('crates/core/src/lib.rs', '#[test]\n#[ignore]\nfn regression() {}\n')
        with self.assertRaisesRegex(selector.TestPolicyError, 'trusted execution mapping'):
            selector.select(config, self.base, self.commit(), profile='pr-fast', formal_inventory=inventory)

    def test_fast_deleted_or_renamed_test_keeps_owner_and_review_reason(self):
        config = self.fast_config()
        path = 'crates/core/tests/old.rs'
        self.write(path, '#[test]\nfn regression() {}\n')
        base = self.commit()
        self.git('mv', path, 'crates/core/tests/new.rs')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertTrue(plan['test_policy_changed'])
        self.assertEqual(plan['oasis7_required_packages'], ['core'])
        self.assertEqual(plan['test_policy_paths'], ['crates/core/tests/new.rs', path])

    def test_fast_docs_do_not_claim_rust_or_test_results(self):
        config = self.fast_config()
        self.write('doc/readme.md', 'documentation only')
        plan = selector.select(config, self.base, self.commit(), profile='pr-fast')
        self.assertEqual(plan['groups'], ['baseline'])
        self.assertEqual(plan['oasis7_required_packages'], [])
        self.assertEqual(plan['oasis7_required_wasm_packages'], [])
        self.assertEqual(plan['formal_suites'], [])
        self.assertFalse(plan['test_policy_changed'])

    def test_fast_command_center_ui_does_not_package_but_native_or_mixed_inputs_do(self):
        config = self.fast_config()
        self.write('tools/oasis7-command-center/ui/src/App.tsx', 'ui fixture')
        plan = selector.select(config, self.base, self.commit(), profile='pr-fast')
        self.assertFalse(plan['command_center_package'])
        self.write('tools/oasis7-command-center/native/src/main.rs', 'fn main() {}')
        plan = selector.select(config, self.base, self.commit(), profile='pr-fast')
        self.assertTrue(plan['command_center_package'])

    def test_fast_capability_must_come_from_trusted_config(self):
        self.write('crates/core/src/lib.rs', 'pub fn change() {}')
        head = self.commit()
        with self.assertRaisesRegex(ValueError, 'trusted baseline fast_scope_version'):
            selector.select(self.config, self.base, head, profile='pr-fast')
        self.write('scripts/ci-required-scope.json', json.dumps(self.fast_config()))
        with self.assertRaisesRegex(ValueError, 'trusted baseline fast_scope_version'):
            selector.select(self.config, self.base, self.commit(), profile='pr-fast')

    def test_full_profile_expands_inventory_without_diff_and_binds_fixed_identity(self):
        config = self.fast_config()
        for profile in ('periodic-full', 'candidate-full'):
            with self.subTest(profile=profile), patch.object(selector, 'changed_paths',
                                                           side_effect=AssertionError('must not diff')):
                plan = selector.full_plan(config, profile, self.base, policy_ref=self.base)
            self.assertEqual(plan['scope'], 'explicit-full')
            self.assertEqual(set(plan['groups']), set(config['groups']))
            self.assertEqual(plan['source_sha'], self.base)
            self.assertEqual(plan['tested_sha'], self.base)
            self.assertEqual(plan['policy_sha'], self.base)
            self.assertEqual(plan['formal_suites'], sorted(selector.FORMAL_SUITES))
            self.assertTrue(plan['command_center_package'])
            self.assertEqual(plan['check_inventory_version'], selector.CHECK_INVENTORY_VERSION)

    def test_full_cli_outputs_all_groups_and_actual_matrix_not_old_empty_plan(self):
        config = self.fast_config()
        self.write('config.json', json.dumps(config))
        arguments = ['selector', '--profile', 'periodic-full', '--head-ref', self.base,
                     '--policy-ref', self.base, '--config', 'config.json', '--github-output', 'outputs.txt']
        with patch.object(sys, 'argv', arguments), contextlib.redirect_stdout(io.StringIO()):
            selector.main()
        outputs = dict(line.split('=', 1) for line in Path('outputs.txt').read_text().splitlines())
        plan = json.loads(outputs['plan'])
        self.assertEqual(set(plan['groups']), set(config['groups']))
        self.assertTrue(all(outputs['run_' + group] == 'true' for group in config['groups']))
        self.assertEqual(json.loads(outputs['formal_shards']), sorted(selector.FORMAL_SUITES))
        self.assertEqual(outputs['command_center_package'], 'true')


class PythonControlContracts(unittest.TestCase):
    setUp = Selection.setUp
    tearDown = Selection.tearDown
    git = Selection.git
    write = Selection.write
    commit = Selection.commit

    def trusted(self):
        return json.loads((HERE / 'ci-required-scope.json').read_text())

    def change(self, path, content='# changed Python input\n'):
        self.write(path, content)
        return self.commit()

    def test_local_check_adapter_selects_its_trusted_contract(self):
        config = self.trusted()
        test = 'scripts/ci-local-check-plan.test.py'
        for path in ('scripts/ci-local-check-plan.py', test):
            with self.subTest(path=path):
                plan = selector.select(config, self.base, self.change(path), profile='pr-fast')
                self.assertEqual(plan['scope'], 'targeted')
                self.assertEqual(set(plan['groups']), {'baseline', 'workflow_governance'})
                self.assertEqual(plan['python_control_tests'], {'workflow_governance': [test]})
                self.base = self.git('rev-parse', 'HEAD')

    def test_local_check_adapter_cannot_enrol_itself_from_candidate_policy(self):
        config = self.trusted()
        old = json.loads(json.dumps(config))
        old.pop('formal_full_profiles_only_version', None)
        old.pop('formal_deferred_ordinary_cases', None)
        paths = ('scripts/ci-local-check-plan.py', 'scripts/ci-local-check-plan.test.py')
        for path in paths:
            old['scoped_control_paths'].pop(path, None)
            old['python_control_contracts'].pop(path, None)
        self.write('scripts/ci-required-scope.json', json.dumps(config))
        for path in (*paths, 'scripts/ci-local-check-plan-future.py'):
            with self.subTest(path=path):
                head = self.change(path)
                plan = selector.select(old, self.base, head, profile='pr-fast')
                self.assertEqual(plan['scope'], 'full')
                self.assertEqual(set(plan['groups']), set(old['groups']))
                self.assertEqual(plan['python_control_tests'], {})
                self.base = head

    def test_registered_control_uses_owned_contract_without_native_groups(self):
        config = self.trusted()
        path = 'scripts/plan-rust-required-scope.py'
        plan = selector.select(config, self.base, self.change(path), profile='pr-fast')
        self.assertEqual(plan['scope'], 'targeted')
        self.assertEqual(set(plan['groups']), {'baseline', 'workflow_governance'})
        self.assertEqual(plan['python_control_contract_version'], 1)
        self.assertEqual(plan['python_control_paths'], [path])
        self.assertIn('scripts/plan-rust-required-scope.test.py',
                      plan['python_control_tests']['workflow_governance'])
        self.assertIn('rust_toolchain', plan['resources'])
        self.assertEqual(plan['oasis7_required_packages'], [])
        self.assertEqual(plan['formal_suites'], [])

    def with_doc_control_contract(self):
        config = self.trusted()
        self.assertNotIn('scripts/document_corpus.py', config['scoped_control_paths'])
        self.assertNotIn('scripts/document_corpus.py', config['python_control_contracts'])
        path = 'scripts/ci-doc-control.py'
        config['scoped_control_paths'][path] = ['doc_checker_contracts']
        config['python_control_contracts'][path] = {
            'group': 'doc_checker_contracts',
            'tests': [
                'scripts/document-corpus-inventory-check.test.py',
                'scripts/product-doc-governance-check.test.py',
            ],
        }
        return config

    def test_document_control_keeps_its_own_contract_and_trusted_runner(self):
        path = 'scripts/ci-doc-control.py'
        plan = selector.select(self.with_doc_control_contract(), self.base, self.change(path))
        self.assertEqual(plan['scope'], 'targeted')
        self.assertEqual(set(plan['groups']),
                         {'baseline', 'doc_checker_contracts', 'workflow_governance'})
        self.assertIn('scripts/document-corpus-inventory-check.test.py',
                      plan['python_control_tests']['doc_checker_contracts'])

    def test_minimum_document_test_only_removal_keeps_base_contracts(self):
        config = self.with_doc_control_contract()
        tests = config['python_control_contracts']['scripts/ci-doc-control.py']['tests']
        for path in tests:
            with self.subTest(path=path):
                self.assertNotIn(path, config['python_control_contracts'])
                self.write(path, 'def test_minimum_contract():\n    assert True\n')
                self.base = self.commit()
                head = self.change(path, '')
                plan = selector.select(config, self.base, head, profile='pr-fast')
                self.assertEqual(plan['scope'], 'targeted')
                self.assertEqual(set(plan['groups']),
                                 {'baseline', 'doc_checker_contracts', 'workflow_governance'})
                self.assertEqual(plan['python_control_paths'], [path])
                self.assertEqual(plan['python_control_tests'], {'doc_checker_contracts': tests})
                self.assertEqual(plan['oasis7_required_packages'], [])
                self.base = head

    def test_minimum_test_changes_union_with_other_contracts_and_shared_inputs(self):
        config = self.with_doc_control_contract()
        config['package_groups'].update(core=['oasis7_required'], consumer=['node'])
        document_test = 'scripts/document-corpus-inventory-check.test.py'
        cache_source = 'scripts/ci-ecs-cache.py'
        self.write(document_test, '# removed candidate assertions\n')
        self.write('crates/core/src/lib.rs', '// mixed native change\n')
        head = self.change(cache_source)
        plan = selector.select(config, self.base, head)
        self.assertTrue({'doc_checker_contracts', 'compile_metrics', 'workflow_governance',
                         'rust_baseline', 'oasis7_required', 'node'}.issubset(plan['groups']))
        self.assertEqual(plan['python_control_paths'], sorted([document_test, cache_source]))
        self.assertIn(document_test, plan['python_control_tests']['doc_checker_contracts'])
        self.assertIn('scripts/ci-ecs-cache.test.py', plan['python_control_tests']['compile_metrics'])
        self.write('Cargo.lock', '# shared dependency change\n')
        full = selector.select(config, self.base, self.commit())
        self.assertEqual(full['scope'], 'full')
        self.assertEqual(full['python_control_paths'], plan['python_control_paths'])
        self.assertEqual(full['python_control_tests'], plan['python_control_tests'])

    def test_only_exact_trusted_minimum_tests_register_reverse_inputs(self):
        config = self.with_doc_control_contract()
        path = 'scripts/registered-minimum.test.py'
        config['python_control_contracts']['scripts/ci-doc-control.py']['tests'].append(path)
        head = self.change(path)
        plan = selector.select(config, self.base, head)
        self.assertEqual(set(plan['groups']),
                         {'baseline', 'doc_checker_contracts', 'workflow_governance'})
        self.assertEqual(plan['python_control_paths'], [path])
        self.assertIn(path, plan['python_control_tests']['doc_checker_contracts'])
        self.base = head
        neighbor = 'scripts/document-corpus-inventory-check-future.test.py'
        head = self.change(neighbor)
        plan = selector.select(config, self.base, head)
        self.assertEqual(set(plan['groups']), {'baseline', 'doc_checker_contracts'})
        self.assertEqual(plan['python_control_paths'], [])
        self.assertEqual(plan['python_control_tests'], {})
        self.base = head
        with self.assertRaises(selector.UnknownInputsError):
            selector.select(config, self.base, self.change('scripts/registered-minimum-future.test.py'))

    def test_contract_sources_and_tests_are_selected_as_one_trusted_union(self):
        config = self.trusted()
        source = 'scripts/ci-required-result.py'
        test = 'scripts/ci-required-result.test.py'
        self.write(source, '# changed gate implementation\n')
        plan = selector.select(config, self.base, self.change(test, '# removed all candidate assertions\n'))
        self.assertEqual(plan['python_control_paths'], sorted([source, test]))
        tests = plan['python_control_tests']['workflow_governance']
        self.assertIn(test, tests)
        self.assertEqual(tests, sorted(set(tests)))

    def test_old_baseline_cannot_borrow_candidate_control_registration(self):
        old = self.trusted()
        old.pop('python_control_contract_version', None)
        old.pop('python_control_contracts', None)
        old['scoped_control_paths'] = {path: groups for path, groups in old['scoped_control_paths'].items()
                                       if not path.endswith('.py')}
        path = 'scripts/plan-rust-required-scope.py'
        self.write('scripts/ci-required-scope.json', json.dumps(self.trusted()))
        plan = selector.select(old, self.base, self.change(path), profile='pr-fast')
        self.assertEqual(plan['scope'], 'full')
        self.assertNotIn('python_control_contract_version', plan)
        self.assertIn('shared build or CI control changed: ' + path, plan['reasons'])

    def test_deleted_and_renamed_registered_controls_keep_both_contracts(self):
        config = self.trusted()
        first = 'scripts/ci-cache-archive.py'
        second = 'scripts/ci-cache-policy.py'
        self.write(first, '# distinctive existing implementation\n')
        self.base = self.commit()
        self.git('mv', first, second)
        plan = selector.select(config, self.base, self.commit())
        self.assertEqual(plan['scope'], 'targeted')
        self.assertEqual(plan['python_control_paths'], sorted([first, second]))
        self.base = self.git('rev-parse', 'HEAD')
        Path(second).unlink()
        plan = selector.select(config, self.base, self.commit())
        self.assertEqual(plan['python_control_paths'], [second])
        self.assertIn('scripts/ci-cache-policy.test.py',
                      plan['python_control_tests']['workflow_governance'])

    def test_unknown_control_and_shared_build_inputs_keep_full_coverage(self):
        config = self.trusted()
        for path in ('scripts/ci-new-unregistered.py', 'scripts/ci-tests.sh',
                     'scripts/ci-python-control-contracts.py', 'scripts/ci-verify-m1-wasm-summaries.py',
                     'scripts/ci-compile-metrics-gate.py', '.github/workflows/rust.yml',
                     'Cargo.lock', 'rust-toolchain.toml'):
            with self.subTest(path=path):
                head = self.change(path)
                self.assertEqual(selector.select(config, self.base, head)['scope'], 'full')
                self.base = head

    def test_unknown_ordinary_python_is_not_registered_by_extension(self):
        config = self.trusted()
        for path in ('scripts/new-helper.py', 'scripts/doc_content_warnings_future.py',
                     'tools/future_utility/main.py'):
            with self.subTest(path=path):
                with self.assertRaises(selector.UnknownInputsError):
                    selector.select(config, self.base, self.change(path))
                Path(path).unlink()
                self.base = self.commit()

    def test_embedded_rust_and_reverse_consumers_are_not_subtracted(self):
        config = self.trusted()
        config['package_groups'].update(core=['oasis7_required'], consumer=['node'])
        path = 'scripts/ci-cache-policy.py'
        self.write(path, '# embedded Python contract\n')
        self.write('crates/core/src/lib.rs',
                   'const SCRIPT: &str = include_str!("../../../' + path + '");\n')
        self.base = self.commit()
        plan = selector.select(config, self.base, self.change(path))
        self.assertEqual(plan['scope'], 'targeted')
        self.assertTrue({'workflow_governance', 'rust_baseline', 'oasis7_required', 'node'}
                        .issubset(plan['groups']))
        self.assertEqual(plan['architecture']['packages'], ['core'])

    def test_control_contracts_union_with_rust_other_rules_and_shared_inputs(self):
        config = self.with_doc_control_contract()
        config['package_groups'].update(core=['oasis7_required'], consumer=['node'])
        path = 'scripts/ci-doc-control.py'
        config['rules'].append({'match': [path], 'groups': ['net'], 'reason': 'additional actual consumer'})
        self.write('crates/core/src/lib.rs', '// mixed native change\n')
        head = self.change(path)
        plan = selector.select(config, self.base, head)
        self.assertTrue({'doc_checker_contracts', 'workflow_governance', 'rust_baseline',
                         'oasis7_required', 'node', 'net'}.issubset(plan['groups']))
        self.write('Cargo.lock', '# same change also changes shared dependencies\n')
        plan = selector.select(config, self.base, self.commit())
        self.assertEqual(plan['scope'], 'full')
        self.assertEqual(plan['python_control_paths'], [path])

    def test_native_signer_runner_keeps_real_platform_consumers(self):
        path = 'scripts/ci-local-signer-tests.py'
        plan = selector.select(self.trusted(), self.base, self.change(path))
        self.assertEqual(plan['scope'], 'targeted')
        self.assertTrue({'workflow_governance', 'packaging_contracts', 'fleet_health'}
                        .issubset(plan['groups']))

    def test_registered_existing_helpers_use_actual_contract_groups(self):
        config = self.trusted()
        cases = {
            'scripts/ci_process_runner.py': {'workflow_governance'},
            'scripts/ci-custody-platform.py': {'workflow_governance'},
            'scripts/ci-custody-platform.test.py': {'workflow_governance'},
            'scripts/ci-worker-build.py': {'workflow_governance'},
            'scripts/ci-worker-build.test.py': {'workflow_governance'},
            'scripts/doc_content_warnings.py': {'doc_checker_contracts'},
            'scripts/pr-review-threads.py': {'workflow_governance'},
            'scripts/pr-review-threads.test.py': {'workflow_governance'},
            'scripts/runtime-module-routing-perf-harness.test.py': {'cargo_tooling_contracts'},
            'scripts/safe_git_archive.py': {'workflow_governance', 'packaging_contracts'},
            'scripts/safe-git-archive.test.py': {'workflow_governance', 'packaging_contracts'},
            'scripts/release-web-fixture-process.py': {'launcher_web', 'operational_contracts'},
            'scripts/release-web-startup.test.py': {'launcher_web', 'operational_contracts'},
        }
        for path, groups in cases.items():
            with self.subTest(path=path):
                head = self.change(path)
                plan = selector.select(config, self.base, head)
                self.assertEqual(plan['scope'], 'targeted')
                self.assertTrue(groups.issubset(plan['groups']))
                self.assertNotIn('oasis7_required', plan['groups'])
                if path == 'scripts/ci_process_runner.py':
                    self.assertEqual(set(plan['groups']), {'baseline', 'workflow_governance'})
                    self.assertEqual(plan['python_control_paths'], [path])
                    self.assertEqual(plan['python_control_tests'], {'workflow_governance': [
                        'scripts/ci-custody-platform.test.py',
                        'scripts/ci-formal-process.test.py',
                        'scripts/ci-local-check.test.py',
                        'scripts/ci-storage-acceptance.test.py',
                        'scripts/ci-support-profiles.test.py',
                        'scripts/ci-worker-build.test.py',
                    ]})
                    older = copy.deepcopy(config)
                    del older['scoped_control_paths'][path]
                    del older['python_control_contracts'][path]
                    self.assertEqual(selector.select(older, self.base, head)['scope'], 'full')
                self.base = head

    def test_full_profile_keeps_the_complete_control_contract_inventory(self):
        config = self.trusted()
        plan = selector.full_plan(config, 'periodic-full', self.base)
        self.assertEqual(plan['python_control_paths'], sorted(config['python_control_contracts']))
        for contract in config['python_control_contracts'].values():
            self.assertTrue(set(contract['tests']).issubset(plan['python_control_tests'][contract['group']]))

    def test_invalid_or_unproved_python_scopes_are_rejected(self):
        trusted = self.trusted()
        path = 'scripts/plan-rust-required-scope.py'
        for label, change in (
            ('boolean version', lambda c: c.update(python_control_contract_version=True)),
            ('unsupported version', lambda c: c.update(python_control_contract_version=2)),
            ('no version', lambda c: c.pop('python_control_contract_version', None)),
            ('no contracts', lambda c: c.update(python_control_contracts={})),
            ('no tests', lambda c: c['python_control_contracts'][path].update(tests=[])),
            ('wrong owner group', lambda c: c['python_control_contracts'][path].update(group='node')),
            ('unknown test group', lambda c: c['python_control_contracts'][path].update(group='future')),
            ('duplicate tests', lambda c: c['python_control_contracts'][path].update(tests=['scripts/one.test.py'] * 2)),
            ('test path traversal', lambda c: c['python_control_contracts'][path].update(tests=['scripts/../outside.py'])),
            ('test glob', lambda c: c['python_control_contracts'][path].update(tests=['scripts/*.test.py'])),
            ('implementation is not a test', lambda c: c['python_control_contracts'][path].update(tests=[path])),
            ('untrusted shell', lambda c: c['python_control_contracts'][path].update(tests=['scripts/ci-tests.sh'])),
        ):
            with self.subTest(label=label):
                config = json.loads(json.dumps(trusted))
                change(config)
                with self.assertRaises(ValueError):
                    selector.validate_config(config)


class EnclosingFunctionCfgOwners(unittest.TestCase):
    PATH = 'crates/oasis7_viewer_runtime/src/viewer/runtime_live.rs'
    BEFORE = textwrap.dedent('''\
        mod agent_service_pump;
        struct Server {
            #[cfg(any(test, feature = "test_tier_required"))]
            diagnostics: usize,
            pending: Option<usize>,
            #[cfg(test)]
            recovery: Option<usize>,
        }
        impl Server {
            fn new() -> Self {
                let server = Self {
                    #[cfg(any(test, feature = "test_tier_required"))]
                    diagnostics: 0,
                    pending: Some(1).map(|generation| { generation + 1 }),
                    #[cfg(test)]
                    recovery: None,
                };
                server
            }
        }
    ''')

    def policy(self, before, after):
        def source(*args):
            self.assertEqual(args[0], 'show')
            ref, path = args[1].split(':', 1)
            self.assertEqual(path, self.PATH)
            return {'base': before, 'source': after}[ref].encode()
        with patch.object(selector, 'git', side_effect=source):
            return selector.changed_test_policy(
                'base', 'source', [self.PATH], {self.PATH: {'oasis7_viewer_runtime'}},
                {'oasis7_viewer_runtime'},
                {'oasis7_viewer_runtime': {'test_tier_required', 'test_tier_full'}})

    def assert_expanded(self, before, after):
        paths, ignored, production = self.policy(before, after)
        self.assertFalse(ignored)
        self.assertIn(self.PATH, paths + production)

    def test_added_module_does_not_change_existing_initializer_cfg_policy(self):
        after = self.BEFORE.replace('mod agent_service_pump;',
                                   'mod agent_service_pump;\nmod agent_service_result;')
        self.assertEqual(self.policy(self.BEFORE, after), ([], False, []))

    def test_unrelated_items_do_not_change_existing_method_cfg_policy(self):
        variants = [
            'struct Unrelated;\n' + self.BEFORE,
            self.BEFORE.replace('impl Server {', 'impl Server { fn unrelated() {}'),
            'impl Other { fn new() -> Self { Self } }\n' + self.BEFORE,
            'mod unrelated { pub fn new() {} }\n' + self.BEFORE,
        ]
        for after in variants:
            with self.subTest(after=after):
                self.assertEqual(self.policy(self.BEFORE, after), ([], False, []))

    def test_method_local_cfg_is_stable_only_outside_its_complete_body(self):
        before = ('impl Server { fn read() { '
                  '#[cfg(any(test, feature="test_tier_required"))] let value = input(); } }')
        self.assertEqual(self.policy(before, 'mod unrelated;\n' + before), ([], False, []))
        self.assert_expanded(before, before.replace('input()', 'other_input()'))
        self.assert_expanded(before, before.replace('fn read() {', 'fn read() { extra!();'))
        self.assert_expanded(before, before.replace('fn read() {', 'fn read() { fn nested() {}'))

    def test_comments_and_whitespace_do_not_change_initializer_cfg_policy(self):
        after = self.BEFORE.replace('diagnostics: 0,', 'diagnostics /* unchanged */ :  0,')
        self.assertEqual(self.policy(self.BEFORE, after), ([], False, []))

    def test_initializer_expression_or_gate_changes_keep_expanded_coverage(self):
        variants = [
            self.BEFORE.replace('diagnostics: 0,', 'diagnostics: 1,'),
            self.BEFORE.replace('generation + 1', 'generation + 2'),
            self.BEFORE.replace('feature = "test_tier_required"', 'feature = "test_tier_full"'),
            self.BEFORE.replace('fn new() -> Self {', 'fn new() -> Self { extra!();'),
        ]
        for after in variants:
            with self.subTest(after=after):
                self.assert_expanded(self.BEFORE, after)

    def test_function_impl_and_module_moves_do_not_cancel_cfg_changes(self):
        variants = [
            self.BEFORE.replace('fn new()', 'fn create()'),
            self.BEFORE.replace('impl Server', 'impl Other'),
            'mod nested {\n' + self.BEFORE + '\n}',
        ]
        for after in variants:
            with self.subTest(after=after):
                self.assert_expanded(self.BEFORE, after)

    def test_nested_scope_moves_keep_expanded_coverage(self):
        before = ('impl Server { fn read() { if first { '
                  '#[cfg(any(test, feature="test_tier_required"))] let value = input(); '
                  '} else { other(); } } }')
        after = ('impl Server { fn read() { if first { other(); } else { '
                 '#[cfg(any(test, feature="test_tier_required"))] let value = input(); } } }')
        self.assert_expanded(before, after)

    def test_macro_and_unknown_parent_contexts_keep_conservative_handling(self):
        variants = [
            'fn read() { opaque! { #[cfg(test)] let value = input(); } }',
            'opaque! { impl Server { fn read() { #[cfg(test)] let value = input(); } } }',
            'fn outer() { fn nested() { #[cfg(test)] let value = input(); } }',
            'trait Reader { fn read() { #[cfg(test)] let value = input(); } }',
            'impl Reader for Server { fn read() { #[cfg(test)] let value = input(); } }',
            'impl<T> Server<T> { fn read() { #[cfg(test)] let value = input(); } }',
        ]
        for before in variants:
            with self.subTest(before=before):
                self.assert_expanded(before, 'mod unrelated;\n' + before)


if __name__ == '__main__':
    unittest.main()
