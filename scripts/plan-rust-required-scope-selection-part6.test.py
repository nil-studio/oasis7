#!/usr/bin/env python3
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import textwrap
import unittest

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('selector', HERE / 'plan-rust-required-scope.py')
selector = importlib.util.module_from_spec(spec)
spec.loader.exec_module(selector)


class SelectionPart6(unittest.TestCase):
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

if __name__ == '__main__':
    unittest.main()
