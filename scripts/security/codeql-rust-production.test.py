#!/usr/bin/env python3
"""Production query selection, provenance, failure and shared-deadline regressions."""
from contextlib import ExitStack, redirect_stdout
import copy
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
from types import SimpleNamespace
import unittest
from unittest.mock import patch
from test_codeql_rust_resolver import fake_runtime_session


HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('rust_production', HERE / 'codeql-rust-production.py')
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


def sha(value):
    return hashlib.sha256(value).hexdigest()


class ClockAndPaths(unittest.TestCase):
    def test_same_deadline_shrinks_across_preparation_extraction_and_queries(self):
        for now, expected in ((1000, 4600), (3000, 2600), (5499, 101), (5501, 99)):
            self.assertEqual(m.budget(5500, now=now, monotonic=100), expected)
        for invalid in (float('inf'), float('nan'), True, 5501):
            with self.assertRaisesRegex(RuntimeError, 'fixed production analysis deadline'):
                m.budget(invalid, now=1000, monotonic=100)

    def test_manifest_selection_is_dynamic_and_uses_existing_exclusions(self):
        frozen = {name: {} for name in ('Cargo.toml', 'new-member/Cargo.toml', 'independent/Cargo.toml',
                  'target/Cargo.toml', 'third_party/dependency/Cargo.toml', 'README.md')}
        self.assertEqual(m.maintained_manifests(frozen),
                         ['Cargo.toml', 'independent/Cargo.toml', 'new-member/Cargo.toml'])


class Production(unittest.TestCase):
    """Real files and checkout/lock checks; only CodeQL and Cargo proxy calls are substituted."""
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.source, self.runner = self.root / 'source', self.root / 'runner'
        self.source.mkdir(); self.runner.mkdir()
        self.official = self.runner / 'bundle'
        self.official.mkdir()
        self.codeql = self.official / 'codeql'
        self.codeql.write_text('#!/bin/true\n'); self.codeql.chmod(0o700)
        self.database = self.runner / 'databases/rust'
        self.database.mkdir(parents=True)
        (self.database / 'codeql-database.yml').write_text('language: rust\nfinalised: true\n')
        (self.database / 'log').mkdir()
        self.work, self.output = self.runner / 'work', self.runner / 'evidence'
        self.sarif = self.runner / 'results/rust.sarif'
        self.github_output, self.github_env = self.runner / 'output', self.runner / 'env'
        self.github_output.write_text(''); self.github_env.write_text('')
        self.action = self.runner / 'action'
        self.action.mkdir()
        self.config = dict(languages=['rust'], buildMode='none', dbLocation=str(self.database.parent),
                           repositoryRoot=str(self.source), overlayDatabaseMode='none', trapCaches={},
                           useOverlayDatabaseCaching=False, dependencyCachingEnabled=False)
        (self.action / 'config').write_text(json.dumps(self.config))
        for folder in (self.source, self.source / 'independent'):
            folder.mkdir(exist_ok=True)
            (folder / 'Cargo.toml').write_text('[workspace]\nmembers = []\n')
            (folder / 'Cargo.lock').write_text('version = 4\n')
        (self.source / 'lib.rs').write_text('pub fn unchanged() {}\n')
        subprocess.run(['git', 'init', '-q', '-b', 'main'], cwd=self.source, check=True)
        self.commit()
        self.deadline = time.time() + 4400
        self.budget_receipt = dict(mode=m.MODE, source_sha=self.source_sha, job_budget_seconds=5400,
                                   fixed_job_deadline=self.deadline + 600,
                                   analysis_allocated_at=self.deadline - 4500, analysis_allocated_minutes=75,
                                   analysis_deadline=self.deadline, post_analysis_reserve_seconds=600,
                                   helper_terminal_and_upload_reserve_seconds=90)
        (self.runner / 'codeql-rust-production-budget.json').write_text(json.dumps(self.budget_receipt))
        self.env = dict(RUNNER_TEMP=str(self.runner), CODEQL_ACTION_TEMP=str(self.action),
                        GITHUB_ENV=str(self.github_env), GITHUB_OUTPUT=str(self.github_output),
                        GITHUB_EVENT_NAME='workflow_dispatch', GITHUB_REF='refs/heads/main',
                        GITHUB_SHA=self.source_sha, CODEQL_RAM='4096', CODEQL_THREADS='2',
                        CODEQL_EXTRACTOR_RUST_OPTION_CARGO_ALL_TARGETS='true',
                        CODEQL_EXTRACTOR_RUST_OPTION_EXTRA_INCLUDES=json.dumps(
                            [str(self.source / 'crates/oasis7_builtin_wasm_modules/_templates')]))
        self.published = self.official / 'qlpacks/codeql/rust-queries/0.1.46'
        (self.published / 'queries/support').mkdir(parents=True)
        (self.published / 'queries/support/Helper.qll').write_text('predicate helper() { any() }\n')
        self.entries = []
        for index in range(37):
            path = self.published / ('queries/Q' + str(index).zfill(2) + '.ql')
            path.write_text('/** @id rust/production-test-' + str(index) + ' */\nselect 1\n')
            self.entries.append(dict(relative=str(path.relative_to(self.published)),
                                     id='rust/production-test-' + str(index), sha256=m.runtime.digest(path)))
            path.with_suffix('.qlx').write_text('precompiled stock query')
        (self.published / '.codeql/libraries/stock').mkdir(parents=True)
        (self.published / '.codeql/libraries/stock/TypeInference.qll').write_text('embedded stock')
        for name in ('qlpack.yml', 'codeql-pack.lock.yml', '.packinfo'):
            (self.published / name).write_text('published binding')
        (self.published / 'codeql-suites').mkdir()
        suite = self.published / 'codeql-suites/rust-code-scanning.qls'
        suite.write_text('- queries: queries\n')
        self.suite_sha = m.runtime.digest(suite)
        self.entries_sha = sha(json.dumps(self.entries, sort_keys=True, separators=(',', ':')).encode())
        self.closure = {}
        for name in ['rust-all', 'util', 'typeinference', *['fixture-' + str(i) for i in range(10)]]:
            version = {'util': '2.0.48', 'typeinference': '0.0.42'}.get(name, '0.2.25')
            root = self.official / 'qlpacks/codeql' / name / version
            root.mkdir(parents=True)
            manifest = root / 'qlpack.yml'
            manifest.write_text('library: true\nname: codeql/' + name + '\nversion: ' + version + '\ndependencies: {}\n')
            self.closure['codeql/' + name] = dict(name='codeql/' + name, version=version, dependencies={},
                                                manifest_sha256=m.runtime.digest(manifest), root=str(root))
        self.original = Path(self.closure['codeql/rust-all']['root'])
        (self.original / m.comparison.TYPE_FILE).parent.mkdir(parents=True)
        (self.original / m.comparison.TYPE_FILE).write_text('stock')
        (self.original / m.candidate.TRAIT_PATH_FILE).write_text('stock-path')
        (self.original / m.candidate.MODEL_FILE).parent.mkdir(parents=True)
        self.core = b'extensions:\n' + m.candidate.MODEL_MARKER + b'      # Existing models\n'
        (self.original / m.candidate.MODEL_FILE).write_bytes(self.core)
        (self.original / 'rust.dbscheme').write_text('schema')
        self.shared = Path(self.closure['codeql/typeinference']['root'])
        (self.shared / m.candidate.SHARED_FILE).parent.mkdir(parents=True)
        (self.shared / m.candidate.SHARED_FILE).write_text('stock-shared')
        (self.shared / 'Unchanged.qll').write_text('unchanged shared support')
        helper = self.official / 'qlpacks/codeql/suite-helpers/1.0.61'
        helper.mkdir(parents=True)
        (helper / 'qlpack.yml').write_text('name: codeql/suite-helpers\nversion: 1.0.61\nlibrary: false\ndependencies: {}\n')
        self.calls, self.guard_manifests, self.call_options = [], [], []
        self.bad_closure = self.query_failure = self.mutate_source = self.missing_result = False
        self.bad_shared_closure = False
        self.decoded_results = {}
        self.mutate_private = self.mutate_receipt = self.mutate_mode = None
        self.guard_status = 'PASS'
        self.stack = ExitStack()
        self.addCleanup(self.stack.close)
        for target, name, value in (
                (m.candidate, 'DEFAULT_ENTRIES_SHA', self.entries_sha),
                (m.candidate, 'SUITE_SHA', self.suite_sha),
                (m.comparison, 'OFFICIAL_SHA', sha(b'stock')),
                (m.comparison, 'CANDIDATE_SHA', sha(b'patched')),
                (m.candidate, 'TYPE_CANDIDATE_SHA', sha(b'arity-patched')),
                (m.candidate, 'TRAIT_TYPE_SHA', sha(b'trait-patched')),
                (m.candidate, 'TRAIT_PATH_OFFICIAL_SHA', sha(b'stock-path')),
                (m.candidate, 'TRAIT_PATH_SHA', sha(b'trait-path')),
                (m.candidate, 'CONSTRAINT_CANDIDATE_SHA', sha(b'constraint-shared')),
                (m.candidate, 'TARGET_QUALIFIED_SHA', sha(b'target-shared')),
                (m, 'NEGATIVE_PROJECTION_BASE_SHA', sha(b'target-shared')),
                (m, 'NEGATIVE_PROJECTION_SHA', sha(b'negative-shared')),
                (m.capacity.bundle, 'SHARED_OFFICIAL_SHA', sha(b'stock-shared')),
                (m.capacity.bundle, 'SHARED_CANDIDATE_SHA', sha(b'helper-shared')),
                (m.candidate, 'MODEL_OFFICIAL_SHA', sha(self.core)),
                (m.candidate, 'MODEL_CANDIDATE_SHA', sha(self.core.replace(
                    m.candidate.MODEL_MARKER, m.candidate.MODEL_MARKER + m.candidate.MODEL_LINE, 1)))):
            self.stack.enter_context(patch.object(target, name, value))
        self.stack.enter_context(patch.object(m.capacity.bundle, 'bundle_closure', side_effect=lambda *a: copy.deepcopy(self.closure)))
        self.stack.enter_context(patch.object(m.cargo_guard, 'install', side_effect=self.install_guard))
        self.stack.enter_context(patch.object(m.cargo_guard, 'inspect', side_effect=lambda *a, **kw: dict(status=self.guard_status)))
        self.supervision = self.stack.enter_context(patch.object(
            m.action_deadline, 'admit_production', return_value=dict(status='PASS'), create=True))
        self.stack.enter_context(patch.object(m, 'Runner', side_effect=self.runner_factory))

    def commit(self):
        subprocess.run(['git', 'add', '.'], cwd=self.source, check=True)
        subprocess.run(['git', '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
                        'commit', '-qm', 'Freeze production fixture'], cwd=self.source, check=True)
        self.source_sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=self.source, text=True).strip()

    def install_guard(self, work, out, deadline, source_sha, manifests=(), **kwargs):
        self.guard_manifests = list(manifests)
        self.assertEqual(source_sha, self.source_sha)
        self.assertEqual(deadline, self.deadline)
        home = work / 'locked-cargo-home'
        home.mkdir()
        return home

    def runner_factory(self, args, terminal, end):
        fixture = self
        class FakeRunner:
            def __init__(self):
                self.end = end
                self.terminal = terminal
            def __call__(self, argv, name, limit=None, **options):
                terminal['stage'] = name
                args_list = list(map(str, argv))
                fixture.calls.append((name, args_list))
                fixture.call_options.append((name, dict(options)))
                value = {}
                if name == 'version':
                    value = {'version': m.VERSION}
                elif name == 'official-default-suite':
                    value = [str(fixture.published / record['relative']) for record in fixture.entries]
                elif name == 'apply-behavior-patch':
                    (args.work / 'candidate-rust-all' / m.comparison.TYPE_FILE).write_text('patched')
                elif name == 'apply-arity-patch':
                    (args.work / 'candidate-rust-all' / m.comparison.TYPE_FILE).write_text('arity-patched')
                elif name == 'apply-shared-helper-patch':
                    (args.work / 'candidate-typeinference' / m.candidate.SHARED_FILE).write_text('helper-shared')
                elif name == 'apply-trait-visibility-patch':
                    (args.work / 'candidate-rust-all' / m.comparison.TYPE_FILE).write_text('trait-patched')
                    (args.work / 'candidate-rust-all' / m.candidate.TRAIT_PATH_FILE).write_text('trait-path')
                elif name == 'apply-constraint-matching-patch':
                    (args.work / 'candidate-typeinference' / m.candidate.SHARED_FILE).write_text('constraint-shared')
                elif name == 'apply-target-qualified-patch':
                    (args.work / 'candidate-target-typeinference' / m.candidate.SHARED_FILE).write_text('target-shared')
                elif name == 'apply-negative-projection-patch':
                    (args.work / 'candidate-negative-typeinference' / m.candidate.SHARED_FILE).write_text('negative-shared')
                elif name == 'production-default-suite':
                    value = [str(args.work / 'default-queries' / record['relative']) for record in fixture.entries]
                elif name == 'resolver-session':
                    fixture.assertTrue(options['restricted_output'])
                    fixture.assertTrue(options['reap_orphans'])
                    def response(record):
                        closure = m.read_json(args.output / 'selected-closure.json')
                        paths = [p['root'] for p in closure.values()]
                        if fixture.bad_closure:
                            paths.remove(str(args.work / 'candidate-rust-all'))
                            paths.append(str(fixture.original))
                        if fixture.bad_shared_closure:
                            paths.remove(closure['codeql/typeinference']['root'])
                            paths.append(str({'v6': args.work / 'candidate-typeinference',
                                              'v7': args.work / 'candidate-target-typeinference'}.get(
                                                  fixture.bad_shared_closure, fixture.shared)))
                        return dict(libraryPath=paths + [record['root']],
                                    dbscheme=str(args.work / 'candidate-rust-all/rust.dbscheme'),
                                    compilationCache=[str(args.work / 'resolve-compilation-cache')],
                                    relativeName=record['pack'] + '/' + record['relative'], qlPackName=record['pack'])
                    return fake_runtime_session(options['observer'], args.output / (name + '.log'), response)
                elif name.startswith('resolve-'):
                    closure = m.read_json(args.output / 'selected-closure.json')
                    query = Path(next(item.split('=', 1)[1] for item in args_list if item.startswith('--query=')))
                    root = args.work / 'default-queries'
                    paths = [pack['root'] for pack in closure.values()]
                    if fixture.bad_closure:
                        paths.remove(str(args.work / 'candidate-rust-all'))
                        paths.append(str(fixture.original))
                    if fixture.bad_shared_closure:
                        paths.remove(closure['codeql/typeinference']['root'])
                        paths.append(str({'v6': args.work / 'candidate-typeinference',
                                          'v7': args.work / 'candidate-target-typeinference'}.get(
                                              fixture.bad_shared_closure, fixture.shared)))
                    value = dict(libraryPath=paths + [str(root)],
                                 dbscheme=str(args.work / 'candidate-rust-all/rust.dbscheme'),
                                 compilationCache=[str(args.work / 'resolve-compilation-cache')],
                                 relativeName=m.PACK + '/' + str(query.relative_to(root)), qlPackName=m.PACK)
                elif name == 'database-identity':
                    value = dict(sourceLocationPrefix=str(args.source), languages=['rust'],
                                 datasetFolder=str(args.database / 'db-rust'))
                elif name == 'default-query-batch':
                    if fixture.query_failure:
                        raise TimeoutError('fixture fixed query deadline')
                    for index, record in enumerate(fixture.entries):
                        if fixture.missing_result and index == 36:
                            continue
                        result = args.database / 'results' / m.PACK / Path(record['relative']).with_suffix('.bqrs')
                        result.parent.mkdir(parents=True, exist_ok=True)
                        result.write_text('computed current query result')
                    if fixture.mutate_source:
                        (args.source / 'lib.rs').write_text('changed during query\n')
                    if fixture.mutate_private:
                        (args.work / fixture.mutate_private).write_text('changed during query\n')
                    if fixture.mutate_receipt:
                        (args.output / fixture.mutate_receipt).write_text('{}\n')
                    if fixture.mutate_mode:
                        path = args.work / fixture.mutate_mode
                        path.chmod(path.stat().st_mode ^ 0o020)
                elif name.startswith('info-'):
                    decoded = fixture.decoded_results.get(name[5:], {'#select': {'tuples': []}})
                    value = {'result-sets': [dict(name=key, rows=len(result['tuples']))
                                             for key, result in decoded.items()]}
                elif name.startswith('decoded-'):
                    value = copy.deepcopy(fixture.decoded_results.get(name[8:], {'#select': {'tuples': []}}))
                    if '--entities=id,string,url' not in args_list:
                        for result in value.values():
                            for row in result['tuples']:
                                for index, cell in enumerate(row):
                                    if isinstance(cell, dict):
                                        row[index] = {'label': cell['label']}
                elif name == 'interpret-default-results':
                    value = {'version': '2.1.0', 'runs': [{'invocations': [{'executionSuccessful': True}], 'results': []}]}
                    target = Path(next(item.split('=', 1)[1] for item in args_list if item.startswith('--output=')))
                    target.write_text(json.dumps(value))
                (args.output / (name + '.log')).write_text(json.dumps(value))
        return FakeRunner()

    def invoke(self, phase, *, outcome='success', deadline=None):
        argv = [phase, '--codeql', str(self.codeql), '--source', str(self.source), '--source-sha', self.source_sha,
                '--database', str(self.database), '--work', str(self.work), '--output', str(self.output),
                '--sarif', str(self.sarif), '--analysis-deadline', str(self.deadline if deadline is None else deadline)]
        if hasattr(self, 'mode'):
            argv += ['--mode', self.mode]
        if phase == 'run':
            argv += ['--extraction-outcome', outcome]
        with patch.dict(os.environ, self.env), redirect_stdout(io.StringIO()):
            return m.main(argv)

    def terminal(self, phase):
        return json.loads((self.output / ('production-' + phase + '.json')).read_text())

    def prepare_ok(self):
        self.assertEqual(self.invoke('prepare'), 0, self.terminal('prepare'))

    def observation(self, mode=m.action_deadline.FIXED_SOURCE_MODE):
        self.mode = mode
        self.env.update(OASIS7_CODEQL_PRODUCTION_MODE=self.mode, GITHUB_SHA='d' * 40,
                        COMPARE_QUALIFIER_PATCH='false',
                        GITHUB_WORKFLOW_SHA='d' * 40, GITHUB_REPOSITORY='example/repo',
                        GITHUB_WORKFLOW_REF='example/repo/.github/workflows/codeql-rust-capacity.yml@refs/heads/main',
                        CODEQL_THREADS='4', CODEQL_RAM='14570')
        self.budget_receipt.update(mode=self.mode, tool_sha='d' * 40)
        (self.runner / 'codeql-rust-production-budget.json').write_text(json.dumps(self.budget_receipt))
        self.stack.enter_context(patch.object(m.action_deadline, 'FIXED_SOURCE', self.source_sha))
        # The separate real-process tests exercise the staged helper/layout closure.
        # This fixture keeps the real mode/source guard and complete query consumer.
        self.stack.enter_context(patch.object(m.action_deadline, 'fixed_source_provenance',
            side_effect=lambda source, source_sha, **kwargs: m.action_deadline.fixed_source_identity(source_sha, **kwargs)))

    def test_fixed_source_uses_all_37_and_original_diagnostics_core_with_separate_identity(self):
        self.observation()
        self.prepare_ok()
        self.assertEqual(self.invoke('run'), 0, self.terminal('run'))
        terminal = self.terminal('run')
        self.assertEqual((terminal['source_sha'], terminal['tool_sha']), (self.source_sha, 'd' * 40))
        self.assertEqual(terminal['mode'], self.mode)
        self.assertEqual(terminal['acceptance_scope'], 'FIXED_SOURCE_OBSERVATION_ONLY')
        self.assertEqual(terminal['upload_status'], 'DISABLED_FOR_OBSERVATION')
        self.assertEqual(terminal['analysis_deadline'], self.deadline)
        self.assertEqual(len(m.read_json(self.output / 'default-results.json')), 37)
        self.assertEqual(len(list(self.output.glob('decoded-*.log'))), 37)
        batch = next(argv for name, argv in self.calls if name == 'default-query-batch')
        self.assertIn('--threads=4', batch)
        self.assertIn('--ram=14570', batch)
        self.assertEqual([name for name, options in self.call_options if options.get('resource_details')],
                         ['default-query-batch'])
        self.supervision.assert_called_once_with(self.output, self.source, self.source_sha,
                                                 self.deadline, mode=self.mode)
        self.assertEqual((terminal['variant'], terminal['base_variant']),
                         ('qualifier-lines-constraint-target-qualified-v7', 'qualifier-lines-constraint-matching-v6'))
        self.assertFalse((self.work / 'candidate-negative-typeinference').exists())
        self.assertFalse((self.output / 'negative-projection-identity.json').exists())

    def test_negative_observation_retains_real_v7_chain_and_resolves_only_new_shared_root(self):
        self.observation(m.action_deadline.NEGATIVE_PROJECTION_MODE)
        self.prepare_ok()
        base = m.read_json(self.output / 'target-qualified-identity.json')
        projected = m.read_json(self.output / 'negative-projection-identity.json')
        self.assertEqual(projected['base_variant'], m.V7_VARIANT)
        self.assertEqual(projected['variant'], m.NEGATIVE_PROJECTION_VARIANT)
        self.assertEqual(projected['before_files'], base['candidate_files'])
        self.assertEqual(projected['changed_files'], [m.candidate.SHARED_FILE])
        self.assertEqual(projected['base_receipts_sha256'],
                         {'target-qualified-identity.json': m.runtime.digest(self.output / 'target-qualified-identity.json')})
        self.assertEqual(projected['tree_permissions']['v7'], projected['tree_permissions']['candidate'])
        self.assertEqual((self.work / 'candidate-target-typeinference' / m.candidate.SHARED_FILE).read_bytes(), b'target-shared')
        closure = m.read_json(self.output / 'selected-closure.json')
        self.assertEqual(closure['codeql/typeinference']['root'], str(self.work / 'candidate-negative-typeinference'))
        resolved = m.read_json(self.output / 'library-resolution.json')
        self.assertEqual(len(resolved), 37)
        self.assertEqual(len([name for name, _ in self.calls if name == 'resolver-session']), 1)
        self.assertEqual(self.invoke('run'), 0, self.terminal('run'))
        terminal = self.terminal('run')
        self.assertEqual((terminal['variant'], terminal['base_variant']),
                         (m.NEGATIVE_PROJECTION_VARIANT, m.V7_VARIANT))
        self.assertEqual(terminal['mode'], m.action_deadline.NEGATIVE_PROJECTION_MODE)
        self.assertEqual(terminal['upload_status'], 'DISABLED_FOR_OBSERVATION')
        self.assertEqual(terminal['analysis_deadline'], self.deadline)
        self.assertEqual(len(m.read_json(self.output / 'default-results.json')), 37)
        self.assertEqual(len(list(self.output.glob('decoded-*.log'))), 37)
        batch = next(argv for name, argv in self.calls if name == 'default-query-batch')
        self.assertIn('--threads=4', batch)
        self.assertIn('--ram=14570', batch)
        self.assertEqual([name for name, options in self.call_options if options.get('resource_details')], ['default-query-batch'])
        self.supervision.assert_called_once_with(self.output, self.source, self.source_sha,
                                                 self.deadline, mode=self.mode)

    def reject_negative_resolver_root(self, wrong, mode=m.action_deadline.NEGATIVE_PROJECTION_MODE):
        if mode != m.MODE:
            self.observation(mode)
        self.bad_shared_closure = wrong
        self.assertEqual(self.invoke('prepare'), 1)
        self.assertNotIn('default-query-batch', [name for name, _ in self.calls])
        self.assertIn('DEPENDENCY_CLOSURE', self.terminal('prepare')['primary_failure']['error'])

    def test_negative_resolver_rejects_v7_instead_of_candidate(self):
        self.reject_negative_resolver_root('v7')

    def test_negative_resolver_rejects_v6_instead_of_candidate(self):
        self.reject_negative_resolver_root('v6')

    def test_negative_resolver_rejects_official_instead_of_candidate(self):
        self.reject_negative_resolver_root('official')

    def test_production_resolver_rejects_v7_instead_of_negative(self):
        self.reject_negative_resolver_root('v7', m.MODE)

    def test_production_resolver_rejects_v6_instead_of_negative(self):
        self.reject_negative_resolver_root('v6', m.MODE)

    def test_production_resolver_rejects_official_instead_of_negative(self):
        self.reject_negative_resolver_root('official', m.MODE)

    def switch_prepared_observation_mode(self, mode):
        self.mode = mode
        self.env['OASIS7_CODEQL_PRODUCTION_MODE'] = mode
        value = m.read_json(self.output / 'production-budget.json')
        value['mode'] = mode
        (self.output / 'production-budget.json').write_text(json.dumps(value))
        with patch.dict(os.environ, self.env):
            provenance = m.action_deadline.fixed_source_identity(self.source_sha, mode=mode)
        (self.output / 'fixed-source-provenance.json').write_text(json.dumps(provenance))
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('preparation identity', self.terminal('run')['primary_failure']['error'])

    def test_v7_preparation_cannot_be_relabelled_as_negative_candidate(self):
        self.observation()
        self.prepare_ok()
        self.switch_prepared_observation_mode(m.action_deadline.NEGATIVE_PROJECTION_MODE)

    def test_negative_preparation_cannot_be_relabelled_as_v7(self):
        self.observation(m.action_deadline.NEGATIVE_PROJECTION_MODE)
        self.prepare_ok()
        self.switch_prepared_observation_mode(m.action_deadline.FIXED_SOURCE_MODE)

    def test_negative_mode_rejects_an_altered_patch_before_copy(self):
        self.observation(m.action_deadline.NEGATIVE_PROJECTION_MODE)
        self.reject_altered_negative_patch()

    def test_production_rejects_an_altered_negative_patch_before_copy(self):
        self.reject_altered_negative_patch()

    def reject_altered_negative_patch(self):
        with patch.object(m, 'NEGATIVE_PROJECTION_PATCH_SHA', '0' * 64):
            self.assertEqual(self.invoke('prepare'), 1)
        self.assertIn('negative-projection patch identity', self.terminal('prepare')['primary_failure']['error'])
        self.assertFalse((self.work / 'candidate-negative-typeinference').exists())

    def test_negative_receipt_is_rebuilt_even_when_saved_digest_is_rebound(self):
        self.observation(m.action_deadline.NEGATIVE_PROJECTION_MODE)
        self.prepare_ok()
        receipt_path = self.output / 'negative-projection-identity.json'
        value = m.read_json(receipt_path)
        value['base_receipts_sha256']['target-qualified-identity.json'] = '0' * 64
        receipt_path.write_text(json.dumps(value))
        inputs = m.read_json(self.output / 'input-identities.json')
        inputs['library_receipts']['negative-projection-identity.json'] = m.runtime.digest(receipt_path)
        (self.output / 'input-identities.json').write_text(json.dumps(inputs))
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('negative-projection identity differs', self.terminal('run')['primary_failure']['error'])

    def test_negative_actual_bytes_cannot_be_replaced_by_consistent_forged_snapshots(self):
        self.observation(m.action_deadline.NEGATIVE_PROJECTION_MODE)
        self.prepare_ok()
        (self.work / 'candidate-negative-typeinference' / m.candidate.SHARED_FILE).write_text('forged candidate')
        files = m.candidate.regular_snapshot(self.work / 'candidate-negative-typeinference', time.monotonic() + 30)
        receipt_path = self.output / 'negative-projection-identity.json'
        receipt = m.read_json(receipt_path); receipt['candidate_files'] = files
        receipt_path.write_text(json.dumps(receipt))
        inputs = m.read_json(self.output / 'input-identities.json')
        inputs['negative_shared_library'] = files
        inputs['library_receipts']['negative-projection-identity.json'] = m.runtime.digest(receipt_path)
        (self.output / 'input-identities.json').write_text(json.dumps(inputs))
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('base or candidate identity', self.terminal('run')['primary_failure']['error'])

    def test_negative_extra_empty_directory_is_rejected(self):
        self.observation(m.action_deadline.NEGATIVE_PROJECTION_MODE)
        self.prepare_ok()
        (self.work / 'candidate-negative-typeinference/extra').mkdir()
        self.assertEqual(self.invoke('run'), 1)
        self.assertIn('tree entries or permissions', self.terminal('run')['primary_failure']['error'])
        self.assertNotIn('default-query-batch', [name for name, _ in self.calls])

    def test_negative_and_v7_directory_tampering_cannot_rebind_the_saved_receipt(self):
        self.observation(m.action_deadline.NEGATIVE_PROJECTION_MODE)
        self.prepare_ok()
        for root in ('candidate-target-typeinference', 'candidate-negative-typeinference'):
            (self.work / root / 'extra').mkdir(mode=0o755)
        receipt_path = self.output / 'negative-projection-identity.json'
        receipt = m.read_json(receipt_path)
        for label in ('v7', 'candidate'):
            receipt['tree_permissions'][label]['extra'] = dict(kind='directory', mode=0o755)
        receipt_path.write_text(json.dumps(receipt))
        inputs = m.read_json(self.output / 'input-identities.json')
        inputs['negative_tree_permissions'] = receipt['tree_permissions']
        inputs['library_receipts']['negative-projection-identity.json'] = m.runtime.digest(receipt_path)
        (self.output / 'input-identities.json').write_text(json.dumps(inputs))
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('tree entries or permissions', self.terminal('run')['primary_failure']['error'])

    def test_negative_candidate_bytes_are_rechecked_after_query(self):
        self.observation(m.action_deadline.NEGATIVE_PROJECTION_MODE)
        self.prepare_ok()
        self.mutate_private = 'candidate-negative-typeinference/' + m.candidate.SHARED_FILE
        self.assertEqual(self.invoke('run'), 1)
        self.assertIn('base or candidate identity', self.terminal('run')['primary_failure']['error'])
        self.assertFalse(self.sarif.exists())

    def test_negative_full_permissions_are_rechecked_after_query(self):
        self.observation(m.action_deadline.NEGATIVE_PROJECTION_MODE)
        self.prepare_ok()
        self.mutate_mode = 'candidate-negative-typeinference/Unchanged.qll'
        self.assertEqual(self.invoke('run'), 1)
        self.assertIn('tree entries or permissions', self.terminal('run')['primary_failure']['error'])
        self.assertFalse(self.sarif.exists())

    def test_negative_receipt_is_rechecked_after_query(self):
        self.observation(m.action_deadline.NEGATIVE_PROJECTION_MODE)
        self.prepare_ok()
        self.mutate_receipt = 'negative-projection-identity.json'
        self.assertEqual(self.invoke('run'), 1)
        self.assertIn('negative-projection identity differs', self.terminal('run')['primary_failure']['error'])
        self.assertFalse(self.sarif.exists())

    def test_fixed_source_resource_drift_is_recorded_before_queries(self):
        self.observation()
        self.env['CODEQL_RAM'] = '4096'
        self.assertEqual(self.invoke('prepare'), 1)
        self.assertEqual(m.read_json(self.output / 'resources.json')['CODEQL_RAM'], 4096)
        self.assertIn('resources differ', self.terminal('prepare')['primary_failure']['error'])
        self.assertEqual(self.calls, [])

    def test_fixed_source_requires_its_own_trusted_workflow(self):
        self.observation()
        self.env['GITHUB_WORKFLOW_REF'] = 'example/repo/.github/workflows/codeql.yml@refs/heads/main'
        self.assertEqual(self.invoke('prepare'), 1)
        self.assertIn('fixed-source', self.terminal('prepare')['primary_failure']['error'])
        self.assertEqual(self.calls, [])

    def test_fixed_source_run_cannot_consume_changed_tool_or_layout_provenance(self):
        self.observation()
        self.prepare_ok()
        value = m.read_json(self.output / 'fixed-source-provenance.json')
        value['tool_sha'] = 'e' * 40
        (self.output / 'fixed-source-provenance.json').write_text(json.dumps(value))
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('tool/layout identity', self.terminal('run')['primary_failure']['error'])

    def reject_observation_preparation_as_production(self, mode):
        self.observation(mode)
        self.prepare_ok()
        self.mode = m.MODE
        self.env.update(OASIS7_CODEQL_PRODUCTION_MODE=m.MODE, GITHUB_SHA=self.source_sha)
        value = m.read_json(self.output / 'production-budget.json')
        value['mode'] = m.MODE
        (self.output / 'production-budget.json').write_text(json.dumps(value))
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('preparation identity', self.terminal('run')['primary_failure']['error'])

    def test_observation_preparation_cannot_be_restored_as_production(self):
        self.reject_observation_preparation_as_production(m.action_deadline.FIXED_SOURCE_MODE)

    def test_negative_v8_preparation_cannot_be_restored_as_production_v8(self):
        self.reject_observation_preparation_as_production(m.action_deadline.NEGATIVE_PROJECTION_MODE)

    def test_default_production_keeps_current_event_sha_binding(self):
        self.env['GITHUB_SHA'] = 'd' * 40
        self.assertEqual(self.invoke('prepare'), 1)
        self.assertIn('trusted main', self.terminal('prepare')['primary_failure']['error'])
        self.assertEqual(self.calls, [])

    def reject_production_preparation_as_observation(self, mode):
        self.prepare_ok()
        self.observation(mode)
        (self.output / 'production-budget.json').write_text(json.dumps(self.budget_receipt))
        with patch.dict(os.environ, self.env):
            provenance = m.action_deadline.fixed_source_identity(self.source_sha, mode=mode)
        (self.output / 'fixed-source-provenance.json').write_text(json.dumps(provenance))
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('preparation identity', self.terminal('run')['primary_failure']['error'])

    def test_production_preparation_cannot_be_restored_as_observation(self):
        self.reject_production_preparation_as_observation(m.action_deadline.FIXED_SOURCE_MODE)

    def test_production_v8_preparation_cannot_be_restored_as_negative_v8(self):
        self.reject_production_preparation_as_observation(m.action_deadline.NEGATIVE_PROJECTION_MODE)

    def test_unset_action_temp_uses_official_runner_temp_default(self):
        (self.action / 'config').rename(self.runner / 'config')
        self.env.pop('CODEQL_ACTION_TEMP')
        with patch.dict(os.environ):
            os.environ.pop('CODEQL_ACTION_TEMP', None)
            self.prepare_ok()
        self.assertEqual(m.read_json(self.output / 'action-config.json')['sha256'],
                         m.runtime.digest(self.runner / 'config'))

    def test_empty_action_temp_uses_official_runner_temp_default(self):
        (self.action / 'config').rename(self.runner / 'config')
        self.env['CODEQL_ACTION_TEMP'] = ''
        self.prepare_ok()
        self.assertEqual(m.read_json(self.output / 'action-config.json')['sha256'],
                         m.runtime.digest(self.runner / 'config'))

    def test_missing_default_action_config_still_blocks_preparation(self):
        self.env.pop('CODEQL_ACTION_TEMP')
        with patch.dict(os.environ):
            os.environ.pop('CODEQL_ACTION_TEMP', None)
            self.assertEqual(self.invoke('prepare'), 1)
        self.assertIn(str(self.runner / 'config'),
                      self.terminal('prepare')['primary_failure']['error'])
        self.assertEqual(self.github_env.read_text(), '')

    def test_37_actual_bindings_use_one_owned_resolver_session(self):
        self.prepare_ok()
        self.assertEqual(len(m.read_json(self.output / 'library-resolution.json')), 37)
        sessions = [call for call in self.calls if call[0] == 'resolver-session']
        independent = [call for call in self.calls if call[1][1:3] == ['resolve', 'library-path']]
        self.assertEqual(len(sessions), 1)
        self.assertEqual(independent, [])

    def test_source_pack_preserves_queries_and_support_without_stock_bindings(self):
        self.prepare_ok()
        root = self.work / 'default-queries'
        self.assertFalse((root / '.codeql').exists())
        self.assertFalse((root / 'codeql-pack.lock.yml').exists())
        self.assertFalse(list(root.rglob('*.qlx')))
        self.assertEqual((root / 'queries/support/Helper.qll').read_bytes(),
                         (self.published / 'queries/support/Helper.qll').read_bytes())
        self.assertEqual(len(m.read_json(self.output / 'queries.json')), 37)
        self.assertEqual(len(m.read_json(self.output / 'library-resolution.json')), 37)
        self.assertEqual((self.original / m.comparison.TYPE_FILE).read_text(), 'stock')
        self.assertEqual((self.original / m.candidate.MODEL_FILE).read_bytes(), self.core)
        self.assertEqual(self.guard_manifests, [self.source / 'Cargo.toml', self.source / 'independent/Cargo.toml'])
        self.assertEqual(self.terminal('prepare')['source_sha'], self.source_sha)
        self.assertEqual(self.terminal('prepare')['variant'], 'qualifier-lines-negative-projection-v8')
        self.assertEqual(m.read_json(self.output / 'arity-patch-identity.json'), m.candidate.arity_identity())
        self.assertEqual((self.work / 'candidate-rust-all' / m.comparison.TYPE_FILE).read_text(), 'trait-patched')
        self.assertEqual((self.work / 'candidate-rust-all' / m.candidate.TRAIT_PATH_FILE).read_text(), 'trait-path')
        self.assertEqual((self.work / 'candidate-typeinference' / m.candidate.SHARED_FILE).read_text(), 'constraint-shared')
        self.assertEqual((self.shared / m.candidate.SHARED_FILE).read_text(), 'stock-shared')
        self.assertEqual(m.read_json(self.output / 'lines-model-identity.json')['variant'], 'qualifier-lines-v3')
        for name in ('shared-helper-identity.json', 'trait-visibility-identity.json'):
            self.assertEqual(m.read_json(self.output / name)['variant'], 'qualifier-lines-shared-trait-v5')
        final = m.read_json(self.output / 'constraint-matching-identity.json')
        self.assertEqual(final['variant'], 'qualifier-lines-constraint-matching-v6')
        self.assertEqual(final['before_files'][m.candidate.SHARED_FILE]['sha256'], sha(b'helper-shared'))
        self.assertEqual(final['candidate_files'][m.candidate.SHARED_FILE]['sha256'], sha(b'constraint-shared'))
        self.assertEqual(len(m.read_json(self.output / 'selected-closure.json')), 14)
        for receipt in m.read_json(self.output / 'library-resolution.json'):
            paths = receipt['actual']['libraryPath']
            self.assertIn(str(self.work / 'candidate-rust-all'), paths)
            self.assertIn(str(self.work / 'candidate-negative-typeinference'), paths)
            self.assertNotIn(str(self.work / 'candidate-target-typeinference'), paths)
            self.assertNotIn(str(self.work / 'candidate-typeinference'), paths)
            self.assertNotIn(str(self.original), paths)
            self.assertNotIn(str(self.shared), paths)
        self.assertEqual([name for name, _ in self.calls if name.startswith('apply-')],
                         ['apply-behavior-patch', 'apply-arity-patch', 'apply-shared-helper-patch',
                          'apply-trait-visibility-patch', 'apply-constraint-matching-patch',
                          'apply-target-qualified-patch', 'apply-negative-projection-patch'])
        self.assertNotEqual(self.source_sha, m.capacity.CAPACITY_SOURCE)
        self.assertIn('analysis_minutes=', self.github_output.read_text())
        self.assertIn('node_options=--require=', self.github_output.read_text())
        self.assertIn('CARGO_HOME=', self.github_env.read_text())

    def test_actual_resolver_stock_substitution_blocks_preparation(self):
        self.bad_closure = True
        self.assertEqual(self.invoke('prepare'), 1)
        self.assertIn('DEPENDENCY_CLOSURE', self.terminal('prepare')['primary_failure']['error'])
        self.assertEqual(self.github_env.read_text(), '')

    def test_actual_resolver_stock_shared_substitution_blocks_preparation(self):
        self.bad_shared_closure = True
        self.assertEqual(self.invoke('prepare'), 1)
        self.assertIn('DEPENDENCY_CLOSURE', self.terminal('prepare')['primary_failure']['error'])
        self.assertEqual(self.github_env.read_text(), '')

    def test_changed_v6_component_variant_cannot_enter_production_preparation(self):
        with patch.object(m.candidate, 'WORKLOAD_VARIANT', 'future-research-v7'):
            with self.assertRaisesRegex(RuntimeError, 'variant differs from the fixed v6'):
                m.prepare_libraries(SimpleNamespace(), {}, SimpleNamespace())
            self.assertEqual(self.invoke('prepare'), 1)
        self.assertEqual(self.calls, [])
        self.assertEqual(self.github_env.read_text(), '')
        self.assertEqual(self.terminal('prepare')['variant'], 'qualifier-lines-negative-projection-v8')
        self.assertIn('variant differs from the fixed v6', self.terminal('prepare')['primary_failure']['error'])

    def test_changed_v6_component_variant_cannot_restore_prepared_production(self):
        self.prepare_ok()
        before = len(self.calls)
        with patch.object(m.candidate, 'WORKLOAD_VARIANT', 'future-research-v7'):
            self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertEqual(self.terminal('run')['variant'], 'qualifier-lines-negative-projection-v8')
        self.assertIn('variant differs from the fixed v6', self.terminal('run')['primary_failure']['error'])
        self.assertFalse(self.sarif.exists())

    def test_negative_production_prepares_and_restores_with_retained_v6_v7_chain(self):
        self.prepare_ok()
        self.assertEqual(self.invoke('run'), 0, self.terminal('run'))
        for phase in ('prepare', 'run'):
            self.assertEqual(self.terminal(phase)['variant'], 'qualifier-lines-negative-projection-v8')
            self.assertEqual(self.terminal(phase)['base_variant'], 'qualifier-lines-constraint-target-qualified-v7')
        self.assertEqual(m.candidate.WORKLOAD_VARIANT, 'qualifier-lines-constraint-matching-v6')
        self.assertEqual((self.work / 'candidate-typeinference' / m.candidate.SHARED_FILE).read_text(), 'constraint-shared')
        self.assertEqual((self.work / 'candidate-target-typeinference' / m.candidate.SHARED_FILE).read_text(), 'target-shared')
        receipt = m.read_json(self.output / 'target-qualified-identity.json')
        self.assertEqual(receipt['variant'], 'qualifier-lines-constraint-target-qualified-v7')
        self.assertEqual(receipt['base_variant'], 'qualifier-lines-constraint-matching-v6')
        self.assertEqual(receipt['before_files'][m.candidate.SHARED_FILE]['sha256'], sha(b'constraint-shared'))
        self.assertEqual(receipt['candidate_files'][m.candidate.SHARED_FILE]['sha256'], sha(b'target-shared'))
        self.assertEqual(receipt['base_receipts_sha256'], {'constraint-matching-identity.json':
                         m.runtime.digest(self.output / 'constraint-matching-identity.json')})
        projected = m.read_json(self.output / 'negative-projection-identity.json')
        self.assertEqual(projected['before_files'], receipt['candidate_files'])
        self.assertEqual(projected['base_variant'], 'qualifier-lines-constraint-target-qualified-v7')
        self.assertEqual(projected['base_receipts_sha256'], {'target-qualified-identity.json':
                         m.runtime.digest(self.output / 'target-qualified-identity.json')})
        self.assertEqual(projected['tree_permissions']['v7'], projected['tree_permissions']['candidate'])
        self.assertEqual((self.work / 'candidate-negative-typeinference' / m.candidate.SHARED_FILE).read_bytes(),
                         b'negative-shared')
        self.assertEqual(m.read_json(self.output / 'input-identities.json')['negative_shared_library'],
                         projected['candidate_files'])
        self.assertEqual(m.read_json(self.output / 'selected-closure.json')['codeql/typeinference']['root'],
                         str(self.work / 'candidate-negative-typeinference'))
        self.assertEqual(self.terminal('run')['upload_status'], 'NOT_PERFORMED_BY_HELPER')
        self.assertNotIn('tool_sha', self.terminal('run'))
        self.assertNotIn('acceptance_scope', self.terminal('run'))

    def test_changed_target_variant_cannot_enter_production_preparation(self):
        with patch.object(m.candidate, 'RESEARCH_VARIANT', 'future-target-v8'):
            self.assertEqual(self.invoke('prepare'), 1)
        self.assertEqual(self.calls, [])
        self.assertEqual(self.github_env.read_text(), '')
        self.assertIn('variant differs from the fixed v7', self.terminal('prepare')['primary_failure']['error'])

    def test_changed_target_variant_cannot_restore_prepared_production(self):
        self.prepare_ok()
        before = len(self.calls)
        with patch.object(m.candidate, 'RESEARCH_VARIANT', 'future-target-v8'):
            self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('variant differs from the fixed v7', self.terminal('run')['primary_failure']['error'])
        self.assertFalse(self.sarif.exists())

    def test_actual_resolver_v6_shared_substitution_blocks_preparation(self):
        self.bad_shared_closure = 'v6'
        self.assertEqual(self.invoke('prepare'), 1)
        self.assertIn('DEPENDENCY_CLOSURE', self.terminal('prepare')['primary_failure']['error'])
        self.assertEqual(self.github_env.read_text(), '')

    def test_new_independent_workspace_is_included_from_current_checkout(self):
        extra = self.source / 'new-independent'; extra.mkdir()
        (extra / 'Cargo.toml').write_text('[workspace]\nmembers = []\n')
        (extra / 'Cargo.lock').write_text('version = 4\n')
        self.commit(); self.env['GITHUB_SHA'] = self.source_sha
        self.budget_receipt['source_sha'] = self.source_sha
        (self.runner / 'codeql-rust-production-budget.json').write_text(json.dumps(self.budget_receipt))
        self.prepare_ok()
        self.assertEqual(self.terminal('prepare')['maintained_manifests'], 3)
        self.assertIn(extra / 'Cargo.toml', self.guard_manifests)

    def test_missing_independent_lock_blocks_extraction_preparation(self):
        (self.source / 'independent/Cargo.lock').unlink()
        self.commit(); self.env['GITHUB_SHA'] = self.source_sha
        self.budget_receipt['source_sha'] = self.source_sha
        (self.runner / 'codeql-rust-production-budget.json').write_text(json.dumps(self.budget_receipt))
        self.assertEqual(self.invoke('prepare'), 1)
        self.assertIn('locks are not frozen', self.terminal('prepare')['primary_failure']['error'])

    def test_full_query_execution_preserves_coverage_unknown_as_separate_acceptance(self):
        self.prepare_ok()
        self.assertEqual(self.invoke('run'), 0, self.terminal('run'))
        terminal = self.terminal('run')
        self.assertEqual(terminal['status'], 'COMPLETE')
        self.assertEqual(terminal['execution_status'], 'success')
        self.assertEqual(terminal['default_queries'], 37)
        self.assertEqual(terminal['coverage_acceptance'], 'PENDING_HOSTED_REVIEW')
        self.assertEqual(terminal['coverage_status'], 'COVERAGE_REQUIRES_REVIEW')
        self.assertTrue(self.sarif.is_file())
        self.assertFalse(list(self.output.rglob('*.sarif')))
        batch = [argv for name, argv in self.calls if name == 'default-query-batch']
        self.assertEqual(len(batch), 1)
        self.assertIn('--no-use-precompiled', batch[0])
        self.assertEqual(sum(item.endswith('.ql') for item in batch[0]), 37)
        self.assertEqual([name for name, options in self.call_options if options.get('resource_details')],
                         ['default-query-batch'])
        self.assertEqual(len(m.read_json(self.output / 'default-results.json')), 37)

    def test_full_decode_preserves_entity_identity_locations_and_all_result_sets(self):
        first = dict(id=101, label='include!(...)',
                     url=dict(uri='file:///source/host.rs', startLine=4, startColumn=1, endLine=4, endColumn=18))
        second = dict(id=102, label='include!(...)',
                      url=dict(uri='file:///source/other.rs', startLine=8, startColumn=1, endLine=8, endColumn=18))
        expected = {
            '#select': {'columns': [{'kind': 'Entity'}, {'kind': 'String'}],
                        'tuples': [[first, 'unexpanded'], [second, 'unexpanded'],
                                   [dict(id=103, label='location unavailable'), 'unexpanded'],
                                   [first, 'unexpanded']]},
            'diagnosticAttributes': {'columns': [{'kind': 'String'}, {'kind': 'String'}],
                                     'tuples': [['severity', 'warning']]},
            'emptySet': {'columns': [{'kind': 'Int'}], 'tuples': []},
        }
        self.decoded_results['09'] = copy.deepcopy(expected)
        self.prepare_ok()
        self.assertEqual(self.invoke('run'), 0, self.terminal('run'))
        self.assertEqual(m.read_json(self.output / 'decoded-09.log'), expected)
        decodes = [argv for _, argv in self.calls if argv[1:3] == ['bqrs', 'decode']]
        self.assertEqual(len(decodes), 37)
        self.assertTrue(all('--entities=id,string,url' in argv for argv in decodes))
        self.assertFalse(any(arg.startswith(('--result-set', '--rows', '--start-at', '--limit'))
                             for argv in decodes for arg in argv))
        self.assertEqual(sum(name == 'default-query-batch' for name, _ in self.calls), 1)
        self.assertEqual(self.terminal('run')['coverage_acceptance'], 'PENDING_HOSTED_REVIEW')

    def test_failed_extraction_never_starts_queries_but_keeps_failure_and_coverage(self):
        self.prepare_ok()
        before = len(self.calls)
        self.assertEqual(self.invoke('run', outcome='failure'), 1)
        self.assertEqual(self.calls[before:], [])
        self.assertFalse(self.sarif.exists())
        self.assertIn('extraction did not succeed', self.terminal('run')['primary_failure']['error'])
        self.assertTrue((self.output / 'coverage.json').is_file())

    def test_renewed_deadline_cannot_reuse_preparation(self):
        self.prepare_ok()
        self.assertEqual(self.invoke('run', deadline=self.deadline + 1), 1)
        self.assertRegex(self.terminal('run')['primary_failure']['error'], 'deadline|budget')
        self.assertNotIn('default-query-batch', [name for name, _ in self.calls])

    def test_old_preparation_variants_cannot_be_promoted_to_v8(self):
        self.prepare_ok()
        prepared = self.terminal('prepare')
        before = len(self.calls)
        for variant in ('qualifier-lines-v2', 'qualifier-lines-v3', 'qualifier-lines-shared-trait-v5',
                        'qualifier-lines-constraint-matching-v6', 'qualifier-lines-constraint-target-qualified-v7'):
            with self.subTest(variant=variant):
                prepared['variant'] = variant
                (self.output / 'production-prepare.json').write_text(json.dumps(prepared))
                self.assertEqual(self.invoke('run'), 1)
                self.assertEqual(len(self.calls), before)
                self.assertIn('preparation identity', self.terminal('run')['primary_failure']['error'])
                self.assertEqual(self.terminal('prepare')['variant'], variant)
                self.assertFalse(self.sarif.exists())

    def test_relabelled_preparation_without_fixed_base_cannot_restore(self):
        self.prepare_ok()
        original = self.terminal('prepare')
        before = len(self.calls)
        for base in (None, 'forged-base'):
            with self.subTest(base=base):
                prepared = dict(original)
                if base is None:
                    del prepared['base_variant']
                else:
                    prepared['base_variant'] = base
                (self.output / 'production-prepare.json').write_text(json.dumps(prepared))
                self.assertEqual(self.invoke('run'), 1)
                self.assertEqual(len(self.calls), before)
                self.assertIn('preparation identity', self.terminal('run')['primary_failure']['error'])
                self.assertFalse(self.sarif.exists())

    def test_old_type_bytes_are_rejected_even_with_an_updated_input_snapshot(self):
        self.prepare_ok()
        private = self.work / 'candidate-rust-all'
        (private / m.comparison.TYPE_FILE).write_text('patched')
        inputs = m.read_json(self.output / 'input-identities.json')
        inputs['repaired_library'] = m.candidate.regular_snapshot(private, time.monotonic() + 30)
        (self.output / 'input-identities.json').write_text(json.dumps(inputs))
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('trait visibility annotation patch', self.terminal('run')['primary_failure']['error'])
        self.assertFalse(self.sarif.exists())

    def test_old_shared_bytes_are_rejected_even_with_an_updated_input_snapshot(self):
        self.prepare_ok()
        private = self.work / 'candidate-typeinference'
        (private / m.candidate.SHARED_FILE).write_text('helper-shared')
        inputs = m.read_json(self.output / 'input-identities.json')
        inputs['repaired_shared_library'] = m.candidate.regular_snapshot(private, time.monotonic() + 30)
        (self.output / 'input-identities.json').write_text(json.dumps(inputs))
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('ConstraintMatching overlay', self.terminal('run')['primary_failure']['error'])
        self.assertFalse(self.sarif.exists())

    def test_v6_bytes_cannot_replace_target_even_with_an_updated_input_snapshot(self):
        self.prepare_ok()
        private = self.work / 'candidate-target-typeinference'
        (private / m.candidate.SHARED_FILE).write_text('constraint-shared')
        inputs = m.read_json(self.output / 'input-identities.json')
        inputs['target_shared_library'] = m.candidate.regular_snapshot(private, time.monotonic() + 30)
        (self.output / 'input-identities.json').write_text(json.dumps(inputs))
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('target-qualified overlay', self.terminal('run')['primary_failure']['error'])
        self.assertFalse(self.sarif.exists())

    def test_shared_non_executable_permission_changes_block_restore(self):
        self.prepare_ok()
        before = len(self.calls)
        for name in ('candidate-typeinference', 'candidate-target-typeinference'):
            with self.subTest(tree=name):
                path = self.work / name / 'Unchanged.qll'
                mode = path.stat().st_mode
                path.chmod(mode ^ 0o020)
                try:
                    self.assertEqual(self.invoke('run'), 1)
                    self.assertEqual(len(self.calls), before)
                    self.assertIn('overlay permissions', self.terminal('run')['primary_failure']['error'])
                    self.assertFalse(self.sarif.exists())
                finally:
                    path.chmod(mode)

    def test_saved_official_or_v6_shared_closure_cannot_replace_target(self):
        self.prepare_ok()
        path = self.output / 'selected-closure.json'
        closure = m.read_json(path)
        before = len(self.calls)
        for root in (self.shared, self.work / 'candidate-typeinference'):
            with self.subTest(root=str(root)):
                closure['codeql/typeinference']['root'] = str(root)
                path.write_text(json.dumps(closure))
                self.assertEqual(self.invoke('run'), 1)
                self.assertEqual(len(self.calls), before)
                self.assertIn('repaired dependency selection', self.terminal('run')['primary_failure']['error'])
                self.assertFalse(self.sarif.exists())

    def test_missing_arity_receipt_blocks_production_queries(self):
        self.prepare_ok()
        (self.output / 'arity-patch-identity.json').unlink()
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('arity-patch-identity.json', self.terminal('run')['primary_failure']['error'])
        self.assertFalse(self.sarif.exists())

    def test_missing_intermediate_or_final_receipt_blocks_queries(self):
        self.prepare_ok()
        before = len(self.calls)
        for name in ('lines-model-identity.json', 'shared-helper-identity.json',
                     'trait-visibility-identity.json', 'constraint-matching-identity.json',
                     'target-qualified-identity.json'):
            with self.subTest(receipt=name):
                path = self.output / name
                saved = path.read_bytes()
                path.unlink()
                try:
                    self.assertEqual(self.invoke('run'), 1)
                    self.assertEqual(len(self.calls), before)
                    self.assertIn(name, self.terminal('run')['primary_failure']['error'])
                    self.assertFalse(self.sarif.exists())
                finally:
                    path.write_bytes(saved)

    def test_forged_layer_receipts_fail_even_with_refreshed_hash_links(self):
        self.prepare_ok()
        names = ('lines-model-identity.json', 'shared-helper-identity.json',
                 'trait-visibility-identity.json', 'constraint-matching-identity.json',
                 'target-qualified-identity.json')
        saved = {name: (self.output / name).read_bytes() for name in names}
        before = len(self.calls)
        for name in names:
            with self.subTest(receipt=name):
                value = json.loads(saved[name])
                value['variant'] = 'forged-layer'
                (self.output / name).write_text(json.dumps(value))
                # Make the digest chain self-consistent; the fixed layer identities
                # must still reject this instead of trusting the stored receipts.
                for linked in ('trait-visibility-identity.json', 'constraint-matching-identity.json',
                               'target-qualified-identity.json'):
                    path = self.output / linked
                    value = m.read_json(path)
                    value['base_receipts_sha256'] = {key: m.runtime.digest(self.output / key)
                                                     for key in value['base_receipts_sha256']}
                    path.write_text(json.dumps(value))
                inputs = m.read_json(self.output / 'input-identities.json')
                inputs['library_receipts'] = {key: m.runtime.digest(self.output / key)
                                              for key in inputs['library_receipts']}
                (self.output / 'input-identities.json').write_text(json.dumps(inputs))
                self.assertEqual(self.invoke('run'), 1)
                self.assertEqual(len(self.calls), before)
                self.assertFalse(self.sarif.exists())
                self.assertRegex(self.terminal('run')['primary_failure']['error'],
                                 'intermediate identity|visibility identity|ConstraintMatching identity|target-qualified identity')
                for restored, data in saved.items():
                    (self.output / restored).write_bytes(data)

    def test_changed_arity_receipt_cannot_relabel_the_same_library(self):
        self.prepare_ok()
        path = self.output / 'arity-patch-identity.json'
        receipt = m.read_json(path)
        receipt['base_type_sha256'] = '0' * 64
        path.write_text(json.dumps(receipt))
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('arity patch identity differs', self.terminal('run')['primary_failure']['error'])

    def test_query_input_mutation_is_rejected_before_evaluation(self):
        self.prepare_ok()
        (self.work / 'default-queries/queries/support/Helper.qll').write_text('changed support')
        self.assertEqual(self.invoke('run'), 1)
        self.assertIn('query or library bytes changed', self.terminal('run')['primary_failure']['error'])
        self.assertNotIn('default-query-batch', [name for name, _ in self.calls])

    def test_cargo_guard_failure_blocks_queries(self):
        self.prepare_ok(); self.guard_status = 'FAIL'
        self.assertEqual(self.invoke('run'), 1)
        self.assertIn('Cargo audit failed', self.terminal('run')['primary_failure']['error'])
        self.assertNotIn('default-query-batch', [name for name, _ in self.calls])

    def test_missing_absolute_extraction_supervision_blocks_queries(self):
        self.prepare_ok()
        self.supervision.side_effect = RuntimeError('absolute Action deadline supervision was not admitted')
        self.assertEqual(self.invoke('run'), 1)
        self.assertIn('deadline supervision', self.terminal('run')['primary_failure']['error'])
        self.assertNotIn('default-query-batch', [name for name, _ in self.calls])

    def test_query_failure_is_not_replaced_by_successful_failure_capture(self):
        self.prepare_ok(); self.query_failure = True
        self.assertEqual(self.invoke('run'), 1)
        terminal = self.terminal('run')
        self.assertEqual(terminal['execution_status'], 'failure')
        self.assertEqual(terminal['queries_status'], 'INCOMPLETE')
        self.assertIn('fixture fixed query deadline', terminal['primary_failure']['error'])
        self.assertNotIn('interpret-default-results', [name for name, _ in self.calls])
        self.assertFalse(self.sarif.exists())

    def test_missing_bqrs_blocks_sarif(self):
        self.prepare_ok(); self.missing_result = True
        self.assertEqual(self.invoke('run'), 1)
        self.assertIn('missing or unsafe production default BQRS', self.terminal('run')['primary_failure']['error'])
        self.assertFalse(self.sarif.exists())

    def test_source_mutation_during_queries_blocks_verified_upload_output(self):
        self.prepare_ok(); self.mutate_source = True
        self.assertEqual(self.invoke('run'), 1)
        self.assertIn('checkout or lock inputs changed', self.terminal('run')['primary_failure']['error'])
        self.assertFalse(self.sarif.exists())

    def test_rust_library_mutation_during_queries_blocks_sarif(self):
        self.prepare_ok()
        self.mutate_private = 'candidate-rust-all/' + m.comparison.TYPE_FILE
        self.assertEqual(self.invoke('run'), 1)
        self.assertIn('candidate differs from the fixed trait visibility annotation patch',
                      self.terminal('run')['primary_failure']['error'])
        self.assertFalse(self.sarif.exists())

    def test_shared_library_mutation_during_queries_blocks_sarif(self):
        self.prepare_ok()
        self.mutate_private = 'candidate-typeinference/Unchanged.qll'
        self.assertEqual(self.invoke('run'), 1)
        self.assertIn('candidate differs from the fixed ConstraintMatching overlay',
                      self.terminal('run')['primary_failure']['error'])
        self.assertFalse(self.sarif.exists())

    def test_target_library_mutation_during_queries_blocks_sarif(self):
        self.prepare_ok()
        self.mutate_private = 'candidate-target-typeinference/Unchanged.qll'
        self.assertEqual(self.invoke('run'), 1)
        self.assertIn('candidate differs from the fixed target-qualified overlay',
                      self.terminal('run')['primary_failure']['error'])
        self.assertFalse(self.sarif.exists())

    def test_target_non_executable_permission_mutation_during_queries_blocks_sarif(self):
        self.prepare_ok()
        self.mutate_mode = 'candidate-target-typeinference/Unchanged.qll'
        self.assertEqual(self.invoke('run'), 1)
        self.assertIn('overlay permissions', self.terminal('run')['primary_failure']['error'])
        self.assertFalse(self.sarif.exists())

    def test_library_receipt_mutation_during_queries_blocks_sarif(self):
        self.prepare_ok()
        self.mutate_receipt = 'constraint-matching-identity.json'
        self.assertEqual(self.invoke('run'), 1)
        self.assertIn('ConstraintMatching identity differs after preparation',
                      self.terminal('run')['primary_failure']['error'])
        self.assertFalse(self.sarif.exists())

    def test_target_receipt_mutation_during_queries_blocks_sarif(self):
        self.prepare_ok()
        self.mutate_receipt = 'target-qualified-identity.json'
        self.assertEqual(self.invoke('run'), 1)
        self.assertIn('target-qualified identity differs after preparation',
                      self.terminal('run')['primary_failure']['error'])
        self.assertFalse(self.sarif.exists())

    def test_production_negative_bytes_are_rechecked_after_query(self):
        self.prepare_ok()
        self.mutate_private = 'candidate-negative-typeinference/' + m.candidate.SHARED_FILE
        self.assertEqual(self.invoke('run'), 1)
        self.assertIn('negative-projection patch, base or candidate identity differs',
                      self.terminal('run')['primary_failure']['error'])
        self.assertFalse(self.sarif.exists())

    def test_production_negative_permissions_are_rechecked_after_query(self):
        self.prepare_ok()
        self.mutate_mode = 'candidate-negative-typeinference/' + m.candidate.SHARED_FILE
        self.assertEqual(self.invoke('run'), 1)
        self.assertIn('negative-projection tree entries or permissions differ',
                      self.terminal('run')['primary_failure']['error'])
        self.assertFalse(self.sarif.exists())

    def test_production_negative_receipt_is_rechecked_after_query(self):
        self.prepare_ok()
        self.mutate_receipt = 'negative-projection-identity.json'
        self.assertEqual(self.invoke('run'), 1)
        self.assertIn('negative-projection identity differs after preparation',
                      self.terminal('run')['primary_failure']['error'])
        self.assertFalse(self.sarif.exists())

    def test_pr_cannot_enter_main_only_repaired_queries(self):
        self.env.update(GITHUB_EVENT_NAME='pull_request', GITHUB_REF='refs/pull/42/merge')
        self.assertEqual(self.invoke('prepare'), 1)
        self.assertEqual(self.calls, [])
        self.assertIn('trusted main maintenance', self.terminal('prepare')['primary_failure']['error'])

    def test_path_overlap_and_symlink_are_rejected_before_writes(self):
        self.work = self.source / 'work'
        with self.assertRaisesRegex(RuntimeError, 'inside RUNNER_TEMP'):
            self.invoke('prepare')
        self.assertFalse(self.work.exists())
        self.work = self.runner / 'linked'
        self.work.symlink_to(self.source, target_is_directory=True)
        with self.assertRaisesRegex(RuntimeError, 'unsafe production path'):
            self.invoke('prepare')


def validate_real_production_bindings(test, codeql, work, output, *, end, run):
    """The original 37 consumer; the caller owns the only deadline and evidence."""
    if run.end != end:
        raise RuntimeError('production binding deadline differs from its caller')
    args = SimpleNamespace(codeql=codeql, source=run.args.source, work=work, output=output)
    run([codeql, 'version', '--format=json'], 'version', 20)
    test.assertEqual(m.read_json(output / 'version.log')['version'], m.VERSION)
    test.assertIs(m.read_json(output / 'version.log').get('configFileFound'), False)
    official = codeql.parent
    original = m.capacity.bundle.bundle_closure(official, m.VERSION)
    original['codeql/suite-helpers'] = m.candidate.suite_helper(official)
    test.assertEqual(len(original), 14)
    published = official / 'qlpacks/codeql/rust-queries/0.1.46'
    published_before = m.candidate.regular_snapshot(published, end)
    suite_file = published / 'codeql-suites/rust-code-scanning.qls'
    run([codeql, 'resolve', 'queries', suite_file, '--format=json'], 'official-suite', 30)
    suite = dict(query_root=str(published), suite_sha256=m.capacity.digest_until(suite_file, end),
                 queries=m.capacity.suite_entries(m.read_json(output / 'official-suite.log'), published))
    before = {name: m.candidate.regular_snapshot(Path(pack['root']), end)
              for name, pack in original.items()}
    closure = m.prepare_libraries(args, original, run)
    negative = m.verify_negative_projection_identity(original, work, output, end)
    expected_files = {
        'candidate-rust-all/codeql/rust/internal/typeinference/TypeInference.qll':
            'db190947441166d6a24217f4dd3bdeafdf8965ca4e11998eba54080bbf91016b',
        'candidate-rust-all/codeql/rust/internal/PathResolution.qll':
            '222a5a43252a4055a66b38d70b2177668e3ebdbd918de67eeb7e9c206cc9ebbf',
        'candidate-rust-all/codeql/rust/frameworks/stdlib/core.model.yml':
            '4a2534bf0f566206409fa9a9f24eca3b2744c8f79b33409689f680bb4f504734',
        'candidate-typeinference/codeql/typeinference/internal/TypeInference.qll':
            '8b32c4d95cfd39f0fef5d365e27b3a0d734ae815272dc714fc60d40de56edc33',
        'candidate-target-typeinference/codeql/typeinference/internal/TypeInference.qll':
            '8f8ac05fb73efc3d7ebfd3d9ab452e90c13d61c7e651379f57f88b4400080ca9',
        'candidate-negative-typeinference/codeql/typeinference/internal/TypeInference.qll':
            'f51a925b05f1a8315248d04cda44cd12cfb12fd7ae424138898805118ffd9f09',
    }
    test.assertEqual({name: m.capacity.digest_until(work / name, end) for name in expected_files}, expected_files)
    v3 = m.read_json(output / 'lines-model-identity.json')
    test.assertEqual(v3['variant'], 'qualifier-lines-v3')
    test.assertEqual(v3['candidate_files'][m.comparison.TYPE_FILE]['sha256'],
                     'd502d88bafabca4c3b41e173c362ce80660bd8c096ee95e53a47f124deaf33fb')
    v6 = m.read_json(output / 'constraint-matching-identity.json')
    test.assertEqual(v6['variant'], 'qualifier-lines-constraint-matching-v6')
    test.assertEqual(v6['base_variant'], 'qualifier-lines-shared-trait-v5')
    test.assertEqual(v6['before_files'][m.candidate.SHARED_FILE]['sha256'],
                     '4a65a578b82c8fa739c2a802b2cc6763cebc34cb67be9008fd396d5dbb5bd47b')
    target = m.read_json(output / 'target-qualified-identity.json')
    test.assertEqual(target['variant'], 'qualifier-lines-constraint-target-qualified-v7')
    test.assertEqual(target['base_variant'], 'qualifier-lines-constraint-matching-v6')
    test.assertEqual(target['changed_files'], [m.candidate.SHARED_FILE])
    test.assertEqual(target['base_receipts_sha256'], {'constraint-matching-identity.json':
                     m.capacity.digest_until(output / 'constraint-matching-identity.json', end)})
    test.assertEqual(negative['variant'], 'qualifier-lines-negative-projection-v8')
    test.assertEqual(negative['base_variant'], 'qualifier-lines-constraint-target-qualified-v7')
    test.assertEqual(negative['before_files'], target['candidate_files'])
    test.assertEqual(negative['base_receipts_sha256'], {'target-qualified-identity.json':
                     m.capacity.digest_until(output / 'target-qualified-identity.json', end)})
    test.assertEqual(negative['tree_permissions']['v7'], negative['tree_permissions']['candidate'])
    records, _ = m.source_queries(official, work, suite, end)
    frozen = m.input_snapshot(args, original, end)
    m.resolve_queries(args, records, closure, run)
    test.assertEqual(len(records), 37)
    resolutions = m.read_json(output / 'library-resolution.json')
    test.assertEqual(len(resolutions), 37)
    for receipt in resolutions:
        paths = receipt['actual']['libraryPath']
        test.assertEqual(len(paths), 15)  # Fourteen dependencies plus the query root.
        test.assertIn(str(work / 'candidate-rust-all'), paths)
        test.assertIn(str(work / 'candidate-negative-typeinference'), paths)
        test.assertNotIn(str(work / 'candidate-target-typeinference'), paths)
        test.assertNotIn(str(work / 'candidate-typeinference'), paths)
        test.assertNotIn(original['codeql/rust-all']['root'], paths)
        test.assertNotIn(original['codeql/typeinference']['root'], paths)
    test.assertEqual(m.verify_negative_projection_identity(original, work, output, end), negative)
    test.assertEqual(m.input_snapshot(args, original, end), frozen)
    test.assertEqual({name: m.candidate.regular_snapshot(Path(pack['root']), end)
                      for name, pack in original.items()}, before)
    test.assertEqual(sorted(m.read_json(output / 'lines-model-identity.json')['changed_files']),
                     sorted([m.comparison.TYPE_FILE, m.candidate.MODEL_FILE]))
    m.save(args, 'binding-validation.json', dict(
        variant=m.PRODUCTION_VARIANT, base_variant=m.PRODUCTION_BASE_VARIANT, queries=records, closure=closure,
        input_snapshot=frozen, published_queries=published_before))
    def verify_after(check_end):
        test.assertEqual(m.verify_negative_projection_identity(original, work, output, check_end), negative)
        test.assertEqual(m.input_snapshot(args, original, check_end), frozen)
        test.assertEqual(m.candidate.regular_snapshot(published, check_end), published_before)
    return verify_after, {name: work / name for name in
                         ('candidate-rust-all', 'candidate-typeinference', 'candidate-target-typeinference',
                          'candidate-negative-typeinference', 'default-queries')}


@unittest.skipUnless(os.environ.get('OASIS7_CODEQL_PRODUCTION_CLI'), 'explicit pinned CodeQL integration input required')
class RealCodeQL(unittest.TestCase):
    def test_exact_default_source_pack_and_actual_repaired_dependency_resolution(self):
        """Standalone production consumer; the paired gate calls its helper once."""
        codeql = Path(os.environ['OASIS7_CODEQL_PRODUCTION_CLI']).resolve()
        with tempfile.TemporaryDirectory() as name:
            folder = Path(name).resolve()
            work, output = folder / 'work', folder / 'evidence'
            work.mkdir(); output.mkdir()
            end = time.monotonic() + 600
            args = SimpleNamespace(codeql=codeql, source=HERE.parents[1], work=work, output=output)
            run = m.Runner(args, {}, end)
            verify, _ = validate_real_production_bindings(self, codeql, work, output, end=end, run=run)
            verify(end)


NEGATIVE_PROJECTION_QL = '''import rust as R
import codeql.typeinference.internal.TypeInference as TI

module I1 implements TI::InputSig1<R::Location> {
  newtype TAtom = Atom(string name) {
    name = ["Pair", "i32", "bool", "T", "P0", "P1", "unknown"]
  }
  class Type extends TAtom {
    Type() { this = Atom(_) }
    TypeParameter getPositionalTypeParameter(int i) {
      this = Atom("Pair") and
      (i = 0 and result = Atom("P0") or i = 1 and result = Atom("P1"))
    }
    TypeParameter getATypeParameter() { result = this.getPositionalTypeParameter(_) }
    string toString() { this = Atom(result) }
    R::Location getLocation() { none() }
  }
  class TypeParameter extends Type {
    TypeParameter() { this = Atom(["T", "P0", "P1"]) }
  }
  class PseudoType extends Type {
    PseudoType() { this = Atom("unknown") }
  }
  newtype TAbs = Abs();
  class TypeAbstraction extends TAbs {
    TypeAbstraction() { this = Abs() }
    TypeParameter getATypeParameter() { result = Atom("T") }
    string toString() { result = "abs" }
    R::Location getLocation() { none() }
  }
  int getTypeParameterId(TypeParameter tp) {
    tp = Atom("T") and result = 0
    or tp = Atom("P0") and result = 1
    or tp = Atom("P1") and result = 2
  }
  int getTypePathLimit() { result = 1 }
}
module M1 = TI::Make1<R::Location, I1>;
M1::TypePath pathAt(int slot) {
  slot = -1 and result = M1::TypePath::nil()
  or slot = 0 and
    result = M1::TypePath::cons(any(I1::TypeParameter p | p = I1::Atom("P0")),
                              M1::TypePath::nil())
  or slot = 1 and
    result = M1::TypePath::cons(any(I1::TypeParameter p | p = I1::Atom("P1")),
                              M1::TypePath::nil())
}
newtype TMention = MentionKind(string name) { name = ["single", "concrete", "repeat"] }
class Mention extends TMention {
  Mention() { this = MentionKind(_) }
  I1::Type getTypeAt(M1::TypePath path) {
    path = pathAt(-1) and
    (this = MentionKind("concrete") and result = I1::Atom("i32")
     or this = MentionKind(["single", "repeat"]) and result = I1::Atom("Pair"))
    or this = MentionKind("single") and path = pathAt(0) and result = I1::Atom("T")
    or this = MentionKind("repeat") and path = pathAt([0, 1]) and result = I1::Atom("T")
  }
  string toString() { this = MentionKind(result) }
  R::Location getLocation() { none() }
}
module I2 implements M1::InputSig2<Mention> {
  Mention getATypeParameterConstraint(I1::TypeParameter tp) { none() }
  predicate conditionSatisfiesConstraint(
    I1::TypeAbstraction abs, Mention condition, Mention constraint, boolean transitive
  ) { none() }
  predicate typeAbstractionHasAmbiguousConstraintAt(
    I1::TypeAbstraction abs, I1::Type constraint, M1::TypePath path
  ) { none() }
  predicate typeParameterIsFunctionallyDetermined(I1::TypeParameter tp) { none() }
}
module M2 = M1::Make2<Mention, I2>;
newtype TApp = AppCase(string name) {
  name = ["single-missing", "single-pseudo", "concrete-different",
          "repeat-equal", "repeat-different", "repeat-missing"]
}
class App extends TApp {
  App() { this = AppCase(_) }
  I1::Type getTypeAt(M1::TypePath path) {
    path = pathAt(-1) and
    (this = AppCase("concrete-different") and result = I1::Atom("bool")
     or this != AppCase("concrete-different") and result = I1::Atom("Pair"))
    or this = AppCase("single-pseudo") and path = pathAt(0) and result = I1::Atom("unknown")
    or this = AppCase(["repeat-equal", "repeat-different", "repeat-missing"]) and
       path = pathAt(0) and result = I1::Atom("i32")
    or this = AppCase("repeat-equal") and path = pathAt(1) and result = I1::Atom("i32")
    or this = AppCase("repeat-different") and path = pathAt(1) and result = I1::Atom("bool")
  }
  string toString() { this = AppCase(result) }
  R::Location getLocation() { none() }
}
module InstInput implements M2::IsInstantiationOfInputSig<App, Mention> {
  predicate potentialInstantiationOf(App app, I1::TypeAbstraction abs, Mention constraint) {
    abs = I1::Abs() and
    (app = AppCase(["single-missing", "single-pseudo"]) and constraint = MentionKind("single")
     or app = AppCase("concrete-different") and constraint = MentionKind("concrete")
     or app = AppCase(["repeat-equal", "repeat-different", "repeat-missing"]) and
        constraint = MentionKind("repeat"))
  }
}
module Inst = M2::IsInstantiationOf<App, Mention, InstInput>;
boolean positive(App app) {
  if Inst::isInstantiationOf(app, _, _) then result = true else result = false
}
boolean negativeAt(App app, int slot) {
  slot = [-1, 0, 1] and
  if Inst::isNotInstantiationOf(app, _, _, pathAt(slot)) then result = true else result = false
}
from App app
select app.toString() as caseName, positive(app) as positive,
  negativeAt(app, -1) as negativeRoot, negativeAt(app, 0) as negativeP0,
  negativeAt(app, 1) as negativeP1
'''


@unittest.skipUnless(os.environ.get('OASIS7_CODEQL_NEGATIVE_PROJECTION_CLI'),
                     'explicit pinned CodeQL negative-projection semantic input required')
class RealNegativeProjectionQL(unittest.TestCase):
    def test_real_v7_and_candidate_match_independent_six_case_oracle(self):
        """Separate opt-in: an existing small finalized Rust DB, not the 37-query gate."""
        codeql = Path(os.environ['OASIS7_CODEQL_NEGATIVE_PROJECTION_CLI']).resolve()
        self.assertTrue(os.environ.get('OASIS7_CODEQL_NEGATIVE_PROJECTION_DATABASE'),
                        'the semantic opt-in requires an explicit finalized small Rust fixture database')
        database = Path(os.environ['OASIS7_CODEQL_NEGATIVE_PROJECTION_DATABASE']).resolve()
        self.assertTrue((database / 'codeql-database.yml').is_file())
        expected = [
            ['concrete-different', False, True, False, False],
            ['repeat-different', False, False, True, True],
            ['repeat-equal', True, False, False, False],
            ['repeat-missing', False, False, False, False],
            ['single-missing', True, False, False, False],
            ['single-pseudo', True, False, False, False],
        ]
        with tempfile.TemporaryDirectory() as name:
            root = Path(name).resolve()
            work, output, query = root / 'work', root / 'evidence', root / 'query'
            for path in (work, output, query): path.mkdir()
            end = time.monotonic() + 600
            args = SimpleNamespace(codeql=codeql, source=root, work=work, output=output,
                                   mode=m.action_deadline.NEGATIVE_PROJECTION_MODE)
            run = m.Runner(args, {}, end)
            run([codeql, 'version', '--format=json'], 'version', 20)
            self.assertEqual(m.read_json(output / 'version.log')['version'], m.VERSION)
            self.assertIs(m.read_json(output / 'version.log').get('configFileFound'), False)
            original = m.capacity.bundle.bundle_closure(codeql.parent, m.VERSION)
            original['codeql/suite-helpers'] = m.candidate.suite_helper(codeql.parent)
            projected = m.prepare_libraries(args, original, run)
            baseline = m.candidate.research_closure(original, work)
            self.assertEqual(baseline['codeql/typeinference']['root'], str(work / 'candidate-target-typeinference'))
            self.assertEqual(projected['codeql/typeinference']['root'], str(work / 'candidate-negative-typeinference'))
            pack = 'oasis7/negative-projection-semantic-fixture'
            (query / 'qlpack.yml').write_text('name: ' + pack + '\nversion: 0.0.0\n'
                'dependencies:\n  codeql/rust-all: 0.2.25\n  codeql/typeinference: 0.0.42\n'
                '  codeql/suite-helpers: 1.0.61\n  codeql/util: 2.0.48\nextractor: rust\n')
            ql = query / 'Semantics.ql'; ql.write_text(NEGATIVE_PROJECTION_QL)
            observed = {}
            for variant, closure in (('v7', baseline), ('negative', projected)):
                with self.subTest(variant=variant):
                    options = m.query_options(closure, work, variant)
                    run([codeql, 'resolve', 'library-path', '--query=' + str(ql),
                         '--format=json', *options], variant + '-resolve', 20)
                    m.capacity.bundle.verify_library_path(m.read_json(output / (variant + '-resolve.log')),
                                                          query, 'Semantics', closure, pack)
                    bqrs = work / (variant + '.bqrs')
                    run([codeql, 'query', 'run', ql, '--database=' + str(database),
                         '--output=' + str(bqrs), '--threads=2', '--ram=3072', *options], variant + '-run')
                    run([codeql, 'bqrs', 'info', bqrs, '--format=json'], variant + '-info', 20)
                    run([codeql, 'bqrs', 'decode', bqrs, '--format=json'], variant + '-decoded', 20)
                    decoded = m.read_json(output / (variant + '-decoded.log'))
                    m.capacity.verify_decoded(m.read_json(output / (variant + '-info.log')), decoded)
                    observed[variant] = sorted(decoded['#select']['tuples'])
                    self.assertEqual(observed[variant], expected)
            self.assertEqual(observed['v7'], observed['negative'])
            self.assertEqual(ql.read_text(), NEGATIVE_PROJECTION_QL)
            m.verify_negative_projection_identity(original, work, output, end)


if __name__ == '__main__':
    unittest.main()
