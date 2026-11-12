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
        for name in ['rust-all', 'util', *['fixture-' + str(i) for i in range(11)]]:
            version = '0.2.25' if name != 'util' else '2.0.48'
            root = self.official / 'qlpacks/codeql' / name / version
            root.mkdir(parents=True)
            manifest = root / 'qlpack.yml'
            manifest.write_text('library: true\nname: codeql/' + name + '\nversion: ' + version + '\ndependencies: {}\n')
            self.closure['codeql/' + name] = dict(name='codeql/' + name, version=version, dependencies={},
                                                manifest_sha256=m.runtime.digest(manifest), root=str(root))
        self.original = Path(self.closure['codeql/rust-all']['root'])
        (self.original / m.comparison.TYPE_FILE).parent.mkdir(parents=True)
        (self.original / m.comparison.TYPE_FILE).write_text('stock')
        (self.original / m.candidate.MODEL_FILE).parent.mkdir(parents=True)
        self.core = b'extensions:\n' + m.candidate.MODEL_MARKER + b'      # Existing models\n'
        (self.original / m.candidate.MODEL_FILE).write_bytes(self.core)
        (self.original / 'rust.dbscheme').write_text('schema')
        helper = self.official / 'qlpacks/codeql/suite-helpers/1.0.61'
        helper.mkdir(parents=True)
        (helper / 'qlpack.yml').write_text('name: codeql/suite-helpers\nversion: 1.0.61\nlibrary: false\ndependencies: {}\n')
        self.calls, self.guard_manifests = [], []
        self.bad_closure = self.query_failure = self.mutate_source = self.missing_result = False
        self.guard_status = 'PASS'
        self.stack = ExitStack()
        self.addCleanup(self.stack.close)
        for target, name, value in (
                (m.candidate, 'DEFAULT_ENTRIES_SHA', self.entries_sha),
                (m.candidate, 'SUITE_SHA', self.suite_sha),
                (m.comparison, 'OFFICIAL_SHA', sha(b'stock')),
                (m.comparison, 'CANDIDATE_SHA', sha(b'patched')),
                (m.candidate, 'TYPE_CANDIDATE_SHA', sha(b'arity-patched')),
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
            def __call__(self, argv, name, limit=None):
                terminal['stage'] = name
                args_list = list(map(str, argv))
                fixture.calls.append((name, args_list))
                value = {}
                if name == 'version':
                    value = {'version': m.VERSION}
                elif name == 'official-default-suite':
                    value = [str(fixture.published / record['relative']) for record in fixture.entries]
                elif name == 'apply-behavior-patch':
                    (args.work / 'candidate-rust-all' / m.comparison.TYPE_FILE).write_text('patched')
                elif name == 'apply-arity-patch':
                    (args.work / 'candidate-rust-all' / m.comparison.TYPE_FILE).write_text('arity-patched')
                elif name == 'production-default-suite':
                    value = [str(args.work / 'default-queries' / record['relative']) for record in fixture.entries]
                elif name.startswith('resolve-'):
                    closure = m.read_json(args.output / 'selected-closure.json')
                    query = Path(next(item.split('=', 1)[1] for item in args_list if item.startswith('--query=')))
                    root = args.work / 'default-queries'
                    paths = [pack['root'] for pack in closure.values()]
                    if fixture.bad_closure:
                        paths.remove(str(args.work / 'candidate-rust-all'))
                        paths.append(str(fixture.original))
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
                elif name.startswith('info-'):
                    value = {'result-sets': [{'name': '#select', 'rows': 0}]}
                elif name.startswith('decoded-'):
                    value = {'#select': {'tuples': []}}
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
        if phase == 'run':
            argv += ['--extraction-outcome', outcome]
        with patch.dict(os.environ, self.env), redirect_stdout(io.StringIO()):
            return m.main(argv)

    def terminal(self, phase):
        return json.loads((self.output / ('production-' + phase + '.json')).read_text())

    def prepare_ok(self):
        self.assertEqual(self.invoke('prepare'), 0, self.terminal('prepare'))

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
        self.assertEqual(self.terminal('prepare')['variant'], 'qualifier-lines-v3')
        self.assertEqual(m.read_json(self.output / 'arity-patch-identity.json'), m.candidate.arity_identity())
        self.assertEqual((self.work / 'candidate-rust-all' / m.comparison.TYPE_FILE).read_text(), 'arity-patched')
        self.assertNotEqual(self.source_sha, m.capacity.CAPACITY_SOURCE)
        self.assertIn('analysis_minutes=', self.github_output.read_text())
        self.assertIn('node_options=--require=', self.github_output.read_text())
        self.assertIn('CARGO_HOME=', self.github_env.read_text())

    def test_actual_resolver_stock_substitution_blocks_preparation(self):
        self.bad_closure = True
        self.assertEqual(self.invoke('prepare'), 1)
        self.assertIn('dependency closure', self.terminal('prepare')['primary_failure']['error'])
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
        self.assertEqual(len(m.read_json(self.output / 'default-results.json')), 37)

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

    def test_v2_preparation_cannot_be_promoted_to_the_current_variant(self):
        self.prepare_ok()
        prepared = self.terminal('prepare')
        prepared['variant'] = 'qualifier-lines-v2'
        (self.output / 'production-prepare.json').write_text(json.dumps(prepared))
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('preparation identity', self.terminal('run')['primary_failure']['error'])
        self.assertEqual(self.terminal('prepare')['variant'], 'qualifier-lines-v2')
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
        self.assertIn('two-file model', self.terminal('run')['primary_failure']['error'])
        self.assertFalse(self.sarif.exists())

    def test_missing_arity_receipt_blocks_production_queries(self):
        self.prepare_ok()
        (self.output / 'arity-patch-identity.json').unlink()
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('arity-patch-identity.json', self.terminal('run')['primary_failure']['error'])
        self.assertFalse(self.sarif.exists())

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


@unittest.skipUnless(os.environ.get('OASIS7_CODEQL_PRODUCTION_CLI'), 'explicit pinned CodeQL integration input required')
class RealCodeQL(unittest.TestCase):
    def test_exact_default_source_pack_and_actual_repaired_dependency_resolution(self):
        """No Cargo or extraction fixture: validate the real compiler's selected inputs."""
        codeql = Path(os.environ['OASIS7_CODEQL_PRODUCTION_CLI']).resolve()
        with tempfile.TemporaryDirectory() as name:
            folder = Path(name).resolve()
            work, output = folder / 'work', folder / 'evidence'
            work.mkdir(); output.mkdir()
            args = SimpleNamespace(codeql=codeql, source=HERE.parents[1], work=work, output=output)
            end = time.monotonic() + 600
            run = m.Runner(args, {}, end)
            run([codeql, 'version', '--format=json'], 'version', 20)
            self.assertEqual(m.read_json(output / 'version.log')['version'], m.VERSION)
            official = codeql.parent
            original = m.capacity.bundle.bundle_closure(official, m.VERSION)
            original['codeql/suite-helpers'] = m.candidate.suite_helper(official)
            self.assertEqual(len(original), 14)
            published = official / 'qlpacks/codeql/rust-queries/0.1.46'
            suite_file = published / 'codeql-suites/rust-code-scanning.qls'
            run([codeql, 'resolve', 'queries', suite_file, '--format=json'], 'official-suite', 30)
            suite = dict(query_root=str(published), suite_sha256=m.runtime.digest(suite_file),
                         queries=m.capacity.suite_entries(m.read_json(output / 'official-suite.log'), published))
            before = m.candidate.regular_snapshot(Path(original['codeql/rust-all']['root']), end)
            private = m.candidate.prepare_candidate(original, work, output, end, run, copy_end=end)
            m.candidate.verify_candidate_identity(Path(original['codeql/rust-all']['root']), private, output, end)
            # Reverse only the reviewed equivalent arity encapsulation. The full
            # remaining source must match the independently pinned legacy file.
            current = (private / m.comparison.TYPE_FILE).read_text()
            helper = ('/** Checks arity after both the call and its candidate target are bound. */\n'
                      'bindingset[ce, target]\npragma[inline_late]\n'
                      'private predicate omittedConcreteInherentQualifierArityMatches(CallExpr ce, Function target) {\n'
                      '  ce.getNumberOfSyntacticArguments() = target.getNumberOfParamsInclSelf()\n}\n\n')
            self.assertEqual(current.count(helper), 1)
            restored = current.replace(helper, '', 1)
            call = '    omittedConcreteInherentQualifierArityMatches(ce, target) and\n'
            self.assertEqual(restored.count(call), 1)
            restored = restored.replace(call,
                '    ce.getNumberOfSyntacticArguments() = target.getNumberOfParamsInclSelf() and\n', 1)
            self.assertEqual(sha(restored.encode()), m.comparison.CANDIDATE_SHA)
            records, _ = m.source_queries(official, work, suite, end)
            closure = m.comparison.candidate_closure(original, private)
            m.resolve_queries(args, records, closure, run)
            self.assertEqual(len(records), 37)
            self.assertEqual(len(m.read_json(output / 'library-resolution.json')), 37)
            self.assertEqual(m.candidate.regular_snapshot(Path(original['codeql/rust-all']['root']), end), before)
            self.assertEqual(sorted(m.read_json(output / 'lines-model-identity.json')['changed_files']),
                             sorted([m.comparison.TYPE_FILE, m.candidate.MODEL_FILE]))


if __name__ == '__main__':
    unittest.main()
