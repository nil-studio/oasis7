#!/usr/bin/env python3
"""Candidate source binding, single-batch results, failure and absolute-clock contracts."""
import time
_PAIR_ENTRY_STARTED = time.monotonic()

import contextlib
import csv
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import signal
import stat
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch
from test_codeql_rust_resolver import fake_runtime_session
import codeql_rust_capacity_layout as capacity_layout

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
spec = importlib.util.spec_from_file_location('candidate_workload', HERE / 'codeql-rust-candidate-workload.py')
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


class WitnessColumns(unittest.TestCase):
    def parse(self, name, text):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'decoded.csv'
            path.write_text(text)
            return m.witness_rows(name, path)

    def test_named_metric_columns_preserve_actual_decoded_rows(self):
        # Captured CodeQL 2.27.4 PeerCandidate output from run 42832373527.
        text = '"label","present"\n"derive-df",1\n"derive-ast",1\n"closure-flow",1\n'
        wanted = [['derive-df', '1'], ['derive-ast', '1'], ['closure-flow', '1']]
        for name in ('PeerCandidate', 'PeerLegacy', 'Driver', 'Image'):
            with self.subTest(name=name): self.assertEqual(self.parse(name, text), wanted)

    def test_unnamed_expression_columns_and_empty_results_reach_the_oracle(self):
        for name in ('CallsCandidate', 'CallsLegacy'):
            with self.subTest(name=name):
                self.assertEqual(self.parse(name, '"col0","col1","col2","col3"\n"a",42,"call","b"\n'),
                                 [['a', '42', 'call', 'b']])
        self.assertEqual(self.parse('Files', '"col0","col1"\n"a","b"\n'), [['a', 'b']])
        self.assertEqual(self.parse('PeerCandidate', '"label","present"\n'), [])
        self.assertEqual(self.parse('CallsCandidate', '"col0","col1","col2","col3"\n'), [])

    def test_wrong_missing_duplicate_or_reordered_columns_and_unknown_tables_are_rejected(self):
        cases = [
            ('PeerCandidate', ''), ('PeerCandidate', '"derive-ast",1\n'),
            ('PeerCandidate', 'col0,col1\n'), ('PeerLegacy', 'present,label\n'),
            ('Driver', 'label,label\n'), ('Image', 'label,present,extra\n'),
            ('Files', 'label,present\n'), ('Files', 'col1,col0\n'),
            ('CallsCandidate', 'col0,col1,col2\n'), ('CallsLegacy', 'col0,col1,col3,col2\n'),
            ('CallsUnknown', 'col0,col1,col2,col3\n'), ('Unknown', 'label,present\n'),
        ]
        for name, text in cases:
            with self.subTest(name=name, text=text):
                with self.assertRaisesRegex(RuntimeError, 'public witness decode schema differs'):
                    self.parse(name, text)


class Budget(unittest.TestCase):
    def budget(self):
        return dict(mode=m.MODE, job_budget_seconds=5400, fixed_job_deadline=6400,
                    analysis_allocated_at=1000, analysis_allocated_minutes=75, analysis_deadline=5500,
                    post_analysis_reserve_seconds=600, helper_terminal_and_upload_reserve_seconds=120)

    def test_elapsed_preparation_and_extraction_consume_the_same_clock(self):
        value = self.budget()
        for now in (1000, 2400, 5000, 5499, 5600):
            analysis, job = m.shared_budget(value, 6400, 5500, now, 20)
            self.assertEqual(analysis, 20 + 5500 - now)
            self.assertEqual(job, 20 + 6400 - now)
        for remaining in (5400, 5100, 4800, 660, 600):
            result = m.capacity.analysis_budget(m.MODE, remaining, m.capacity.CAPACITY_SOURCE)
            self.assertLessEqual(result['analysis_allocated_minutes'], 75)
            self.assertLessEqual(result['analysis_allocated_minutes'] * 60 + 600, remaining)
        for source in (None, m.capacity.PREVIOUS_CAPACITY_SOURCE):
            with self.assertRaises(ValueError): m.capacity.analysis_budget(m.MODE, 5400, source)

    def test_unbound_or_extended_clocks_and_nonfinite_values_are_rejected(self):
        for field, bad in (('mode', 'capacity'), ('job_budget_seconds', 9000), ('fixed_job_deadline', 6500),
                           ('analysis_deadline', 5501), ('analysis_allocated_at', 3000),
                           ('analysis_allocated_minutes', 76), ('analysis_allocated_minutes', True),
                           ('post_analysis_reserve_seconds', 0), ('helper_terminal_and_upload_reserve_seconds', 0)):
            with self.subTest(field=field, bad=bad):
                value = self.budget(); value[field] = bad
                with self.assertRaises(RuntimeError): m.shared_budget(value, 6400, 5500, 2000, 20)
        for bad in (float('nan'), float('inf'), 0, 7000):
            with self.assertRaises(RuntimeError): m.shared_budget(self.budget(), bad, 5500, 2000, 20)

    def test_seal_charges_identity_and_hash_work_before_margin(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name); (root / 'observation.json').write_text('{}')
            terminal = dict(status='CANDIDATE_RESEARCH_COMPLETED')
            with patch.object(m.time, 'time', return_value=1000):
                m.seal(root, terminal, 'run', 1270, time.monotonic() + 60)
                self.assertEqual(terminal['status'], 'CANDIDATE_RESEARCH_COMPLETED')
            terminal['status'] = 'CANDIDATE_RESEARCH_COMPLETED'
            # Earlier work may have had 280 seconds; completion actually has 260.
            with patch.object(m.time, 'time', return_value=1020):
                m.seal(root, terminal, 'run', 1280, time.monotonic() + 60)
                self.assertEqual(terminal['status'], 'CANDIDATE_RESEARCH_NEAR_LIMIT')
            m.comparison.verify_seal(root, time.monotonic() + 60)

    def test_raw_evidence_admission_is_aggregate_bounded_and_rejects_symlinks(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name); out = root / 'out'; out.mkdir()
            first, oversized = root / 'first.bqrs', root / 'oversized.bqrs'
            first.write_bytes(b'first'); oversized.write_bytes(b'x' * 4096)
            with patch.object(m.runtime, 'WRITE_CAP', 2048), patch.object(m.runtime, 'CAP', 4096):
                with self.assertRaisesRegex(RuntimeError, 'private evidence bound'):
                    m.preserve_results({'00': first, '01': oversized}, out, time.monotonic() + 60)
                self.assertEqual((out / 'result-00.bqrs').read_bytes(), b'first')
                self.assertFalse((out / 'result-01.bqrs').exists())
                self.assertLessEqual(m.runtime.output_bytes(out), 2048)
            link = root / 'linked.bqrs'; link.symlink_to(first)
            with self.assertRaisesRegex(RuntimeError, 'unsafe result'):
                m.preserve_results({'02': link}, out, time.monotonic() + 60)


class ArityPatch(unittest.TestCase):
    def test_incremental_patch_has_its_own_identity_and_preserves_the_legacy_patch(self):
        self.assertEqual(m.runtime.digest(HERE / 'codeql-rust-qualifier-arity.patch'), m.ARITY_PATCH_SHA)
        self.assertEqual(m.runtime.digest(HERE / 'codeql-rust-omitted-qualifier.patch'), m.comparison.PATCH_SHA)
        self.assertNotEqual(m.TYPE_CANDIDATE_SHA, m.comparison.CANDIDATE_SHA)
        self.assertEqual(m.VARIANT, 'qualifier-lines-v3')

    def test_shared_patch_has_separate_research_and_component_identities(self):
        self.assertEqual(m.runtime.digest(HERE / 'codeql-rust-shared-constraint.patch'), m.SHARED_PATCH_SHA)
        self.assertEqual(m.V5_VARIANT, 'qualifier-lines-shared-trait-v5')
        self.assertEqual(m.WORKLOAD_VARIANT, 'qualifier-lines-constraint-matching-v6')
        self.assertEqual(m.VARIANT, 'qualifier-lines-v3')
        self.assertEqual(m.runtime.digest(HERE / 'codeql-rust-trait-visibility.patch'), m.TRAIT_PATCH_SHA)

    def test_constraint_patch_keeps_its_source_and_fixed_v5_base_separate(self):
        self.assertEqual(m.CONSTRAINT_UPSTREAM, '1c4fea90af949111bc1c625f1e72de56a12c7712')
        self.assertEqual(m.CONSTRAINT_API_PATCH_SHA, 'ba47e346672cd8281b3a6a3d94647b0d00d733a273311799b0533dc4b0fd37db')
        self.assertEqual(m.CONSTRAINT_UPSTREAM_PATCH_SHA, '3880e57052c74d8af7f155a7abc8b721eef0725463897647809fd10b28d3a0ba')
        self.assertEqual(m.runtime.digest(HERE / 'codeql-rust-constraint-matching.patch'),
                         'e6563cf895df0f662444795cf5c8f2358804784d667ccce4b11fa336ffde7db8')
        self.assertEqual(m.runtime.digest(HERE / 'codeql-rust-constraint-matching.patch'), m.CONSTRAINT_PATCH_SHA)
        self.assertEqual(m.capacity.bundle.SHARED_CANDIDATE_SHA,
                         '4a65a578b82c8fa739c2a802b2cc6763cebc34cb67be9008fd396d5dbb5bd47b')
        self.assertEqual(m.CONSTRAINT_CANDIDATE_SHA,
                         '8b32c4d95cfd39f0fef5d365e27b3a0d734ae815272dc714fc60d40de56edc33')

    def test_target_patch_has_its_own_research_identity_without_relabelling_v6(self):
        self.assertEqual(m.WORKLOAD_VARIANT, 'qualifier-lines-constraint-matching-v6')
        self.assertEqual(m.RESEARCH_VARIANT, 'qualifier-lines-constraint-target-qualified-v7')
        self.assertEqual(m.runtime.digest(HERE / 'codeql-rust-target-qualified.patch'),
                         '96dcb5da3818ce129924cc9f80df366cf1517613acc19145df27ea2a65eaed2d')
        self.assertEqual(m.TARGET_QUALIFIED_PATCH_SHA, m.runtime.digest(HERE / 'codeql-rust-target-qualified.patch'))
        self.assertEqual(m.TARGET_QUALIFIED_SHA,
                         '8f8ac05fb73efc3d7ebfd3d9ab452e90c13d61c7e651379f57f88b4400080ca9')


class Workload(unittest.TestCase):
    """Real filesystem and oracles; only external CLI and hosted checkout observations are substituted."""
    @classmethod
    def setUpClass(cls):
        cls._template_dir = tempfile.TemporaryDirectory()
        cls.addClassCleanup(cls._template_dir.cleanup)
        troot = Path(cls._template_dir.name).resolve()
        tsource, trunner = troot / 'source', troot / 'runner'
        tsource.mkdir(); trunner.mkdir()
        tofficial, tdb, tcheckpoint = [trunner / n for n in ('bundle', 'db', 'official-evidence')]
        for p in (tofficial, tdb, tcheckpoint):
            p.mkdir()
        (trunner / 'github-output').write_text('')
        published = tofficial / 'qlpacks/codeql/rust-queries/0.1.46'
        (published / 'queries/support').mkdir(parents=True)
        (published / 'queries/support/Stats.qll').write_text('predicate support() { any() }\n')
        cls._entries = []
        for i in range(37):
            path = published / ('queries/Q' + str(i).zfill(2) + '.ql')
            path.write_text('/** @id rust/example-' + str(i) + ' */\nimport support.Stats\nselect 1\n')
            cls._entries.append(dict(relative=str(path.relative_to(published)), id='rust/example-' + str(i), sha256=m.runtime.digest(path)))
            path.with_suffix('.qlx').write_text('compiled stock; must never be copied')
        (published / 'queries/NonDefault.ql').write_text('select 2\n')
        (published / '.codeql/libraries/stock').mkdir(parents=True)
        (published / '.codeql/libraries/stock/TypeInference.qll').write_text('embedded stock')
        for name in ('qlpack.yml', 'codeql-pack.lock.yml', '.packinfo'):
            (published / name).write_text('published binding')
        (published / 'codeql-suites').mkdir()
        (published / 'codeql-suites/rust-code-scanning.qls').write_text('- queries: queries\n')
        cls._suite_sha = m.runtime.digest(published / 'codeql-suites/rust-code-scanning.qls')
        cls._entries_sha = hashlib.sha256(json.dumps(cls._entries, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
        suite_helper = tofficial / 'qlpacks/codeql/suite-helpers/1.0.61'
        suite_helper.mkdir(parents=True)
        (suite_helper / 'qlpack.yml').write_text('name: codeql/suite-helpers\nversion: 1.0.61\nlibrary: false\ndependencies: {}\n')
        cls._closure_specs = {}
        for name in ['rust-all', 'typeinference', *['fixture-' + str(i) for i in range(11)]]:
            version = '0.0.42' if name == 'typeinference' else '0.2.25'
            rel_root = Path('qlpacks/codeql') / name / version
            root = tofficial / rel_root
            root.mkdir(parents=True)
            (root / 'qlpack.yml').write_text('library: true\nname: codeql/' + name + '\nversion: ' + version + '\ndependencies: {}\n')
            cls._closure_specs['codeql/' + name] = (
                dict(name='codeql/' + name, version=version, dependencies={}, manifest_sha256=m.runtime.digest(root / 'qlpack.yml')),
                rel_root,
            )
        shared = tofficial / cls._closure_specs['codeql/typeinference'][1]
        (shared / m.SHARED_FILE).parent.mkdir(parents=True)
        (shared / m.SHARED_FILE).write_text('stock-shared')
        (shared / 'Unchanged.qll').write_text('unchanged shared support')
        original = tofficial / cls._closure_specs['codeql/rust-all'][1]
        (original / m.comparison.TYPE_FILE).parent.mkdir(parents=True)
        (original / m.comparison.TYPE_FILE).write_text('stock')
        (original / m.TRAIT_PATH_FILE).write_text('stock-path')
        cls._core_bytes = b'extensions:\n' + m.MODEL_MARKER + b'      # Existing fixed models\n'
        (original / m.MODEL_FILE).parent.mkdir(parents=True)
        (original / m.MODEL_FILE).write_bytes(cls._core_bytes)
        (original / 'rust.dbscheme').write_text('schema')
        (tdb / 'codeql-database.yml').write_text('language: rust\nfinalised: true\n')
        cls._db_metadata_sha = m.runtime.digest(tdb / 'codeql-database.yml')
        cls._template_root = troot

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        shutil.copytree(self._template_root, self.root, dirs_exist_ok=True)
        self.source, self.runner = self.root / 'source', self.root / 'runner'
        self.official, self.db, self.checkpoint = [self.runner / n for n in ('bundle', 'db', 'official-evidence')]
        self.out, self.work = self.runner / 'candidate-evidence', self.runner / 'candidate-work'
        self.output = self.runner / 'github-output'
        self.published = self.official / 'qlpacks/codeql/rust-queries/0.1.46'
        self.entries = [dict(entry) for entry in self._entries]
        self.suite_sha = self._suite_sha
        self.entries_sha = self._entries_sha
        self.suite = dict(query_root=str(self.published), suite_sha256=self.suite_sha, queries=self.entries)
        self.suite_helper = self.official / 'qlpacks/codeql/suite-helpers/1.0.61'
        self.closure = {
            key: dict(meta, dependencies=dict(meta['dependencies']), root=str(self.official / rel_root))
            for key, (meta, rel_root) in self._closure_specs.items()
        }
        self.core_bytes = self._core_bytes
        self.deadline = time.time() + 5000
        self.budget = dict(mode=m.MODE, job_budget_seconds=5400, fixed_job_deadline=self.deadline,
                           analysis_allocated_at=time.time() - 10, analysis_allocated_minutes=73,
                           analysis_deadline=self.deadline - 640, post_analysis_reserve_seconds=600,
                           helper_terminal_and_upload_reserve_seconds=120)
        self.resources = dict(CODEQL_RAM=4096, CODEQL_THREADS=2)
        self.protected = dict(source='UNCHANGED', official='UNCHANGED', additional_official_paths=[])
        common = dict(mode=m.MODE, source=m.capacity.CAPACITY_SOURCE, source_root=str(self.source),
                      fixed_job_deadline=self.deadline, analyze_outcome='success', expected_bundle_version='2.27.4')
        self.capture = dict(common, phase='capture', status='CAPTURED', evidence_status='COMPLETE',
                            cargo_guard={'status': 'PASS'}, coverage_capture={'status': 'PASS'}, checkout_gate='PASS',
                            lock_observation_gate='PASS', resource_gate='PASS')
        self.finish = dict(common, phase='finish', status='CANDIDATE_EXTRACTION_READY', assessment_status='CANDIDATE_RESEARCH_READY',
                           verification_admitted=True, extraction_checkpoint='PASS', archived_source='PASS',
                           default_result_verification='NOT_REQUESTED', public_semantics='NOT_REQUESTED', after_identity=self.protected,
                           absolute_action_deadline={'status': 'PASS'},
                           database_metadata_sha256=self._db_metadata_sha)
        self.provenance = dict(candidate='/cargo/git/libp2p-0.57.0', legacy='/cargo/registry/libp2p-0.56.0')
        for name, value in [('capacity-budget', self.budget), ('default-suite', self.suite), ('library-closure', self.closure),
                            ('resources', self.resources), ('libp2p-provenance', self.provenance),
                            ('capacity-prepare-terminal', dict(common, status='PREPARED', image_contract_version=m.IMAGE_CONTRACT)),
                            ('capacity-capture-terminal', self.capture), ('capacity-finish-terminal', self.finish),
                            ('semantic-query-contract', m.capacity.semantic_contract(self.source, image_version=m.IMAGE_CONTRACT)),
                            ('semantic-query-inputs', {n: hashlib.sha256(t.encode()).hexdigest()
                                                       for n, t in m.capacity.semantic_queries(self.source, image_version=m.IMAGE_CONTRACT).items()})]:
            self.write(self.checkpoint / (name + '.json'), value)
        self.write(self.checkpoint / 'hashes.json', {p.name: m.runtime.digest(p) for p in self.checkpoint.iterdir()})
        self.calls = []; self.failed_stage = self.mutate = self.oracle_failure = self.bad_decode = self.bad_header = None
        self.clean_batch_timeout = self.bad_summary = self.cancel_after_batch = False
        self.stack = contextlib.ExitStack(); self.addCleanup(self.stack.close)
        for target, key, value in ((m, 'DEFAULT_ENTRIES_SHA', self.entries_sha), (m, 'SUITE_SHA', self.suite_sha),
                                   (m, 'MODEL_OFFICIAL_SHA', hashlib.sha256(self.core_bytes).hexdigest()),
                                   (m, 'MODEL_CANDIDATE_SHA', hashlib.sha256(self.core_bytes.replace(m.MODEL_MARKER,m.MODEL_MARKER+m.MODEL_LINE)).hexdigest()),
                                   (m.comparison, 'OFFICIAL_SHA', hashlib.sha256(b'stock').hexdigest()),
                                   (m.comparison, 'CANDIDATE_SHA', hashlib.sha256(b'candidate').hexdigest()),
                                   (m, 'TYPE_CANDIDATE_SHA', hashlib.sha256(b'arity-candidate').hexdigest()),
                                   (m, 'TRAIT_TYPE_SHA', hashlib.sha256(b'trait-candidate').hexdigest()),
                                   (m, 'TRAIT_PATH_OFFICIAL_SHA', hashlib.sha256(b'stock-path').hexdigest()),
                                   (m, 'TRAIT_PATH_SHA', hashlib.sha256(b'trait-path').hexdigest()),
                                   (m, 'CONSTRAINT_CANDIDATE_SHA', hashlib.sha256(b'constraint-matching-shared').hexdigest()),
                                   (m, 'TARGET_QUALIFIED_SHA', hashlib.sha256(b'target-qualified-shared').hexdigest()),
                                   (m.capacity.bundle, 'SHARED_OFFICIAL_SHA', hashlib.sha256(b'stock-shared').hexdigest()),
                                   (m.capacity.bundle, 'SHARED_CANDIDATE_SHA', hashlib.sha256(b'helper-shared').hexdigest())):
            self.stack.enter_context(patch.object(target, key, value))
        self.stack.enter_context(patch.dict(os.environ, RUNNER_TEMP=str(self.runner), GITHUB_OUTPUT=str(self.output)))
        self.stack.enter_context(patch.object(m.capacity, 'after_identity', return_value=self.protected))
        self.stack.enter_context(patch.object(m.capacity, 'resources', return_value=self.resources))
        self.stack.enter_context(patch.object(m.capacity.bundle, 'bundle_closure', return_value=self.closure))
        self.stack.enter_context(patch.object(m.comparison, 'prepare_candidate', side_effect=self.prepare_candidate))
        self.stack.enter_context(patch.object(m.runtime, 'run', side_effect=self.cli))

    def write(self, path, value): path.write_text(json.dumps(value))

    def prepare_candidate(self, closure, work, out, end, run, *, copy_end):
        self.assertLessEqual(copy_end, end)
        private = work / 'candidate-rust-all'
        shutil.copytree(Path(closure['codeql/rust-all']['root']), private)
        (private / m.comparison.TYPE_FILE).write_text('candidate')
        return private

    def rows(self, name):
        expected = m.capacity.expected(name,image_version=m.IMAGE_CONTRACT)
        if expected is not None: return [[key, str(value)] for key, value in expected.items()]
        if name == 'Files':
            model = m.capacity.model
            return [[str(self.source / p), 'source'] for p in (model.DRIVER, model.BASE + '/common/src/protocol.rs', model.EXPECT,
                                                             model.BASE + '/candidate/src/adapter.rs', model.BASE + '/legacy/src/adapter.rs')]
        context = 'candidate' if name == 'CallsCandidate' else 'legacy'
        return [[str(self.source / m.capacity.model.BASE / context / 'src/adapter.rs'), '42', 'with_existing_identity',
                 self.provenance[context] + '/src/builder/phase/identity.rs']]

    def cli(self, argv, cwd, output, end, timeout, out, env, file_limit=None, **observation):
        name = output.name[:-4]
        self.calls.append((name, argv, end, timeout, file_limit))
        self.assertIsNone(file_limit, 'full database must not inherit the small probe file limit')
        self.assertLessEqual(end - time.monotonic(), self.deadline - time.time() - 119)
        self.assertNotIn('GH_TOKEN', env); self.assertNotIn('GITHUB_TOKEN', env)
        if name == 'apply-arity-patch':
            (self.work / 'candidate-rust-all' / m.comparison.TYPE_FILE).write_text('arity-candidate')
            output.write_text('applied fixed arity fixture')
        elif name == 'apply-shared-helper-patch':
            (self.work / 'candidate-typeinference' / m.SHARED_FILE).write_text('helper-shared')
            output.write_text('applied fixed shared helper fixture')
        elif name == 'apply-trait-visibility-patch':
            (self.work / 'candidate-rust-all' / m.comparison.TYPE_FILE).write_text('trait-candidate')
            (self.work / 'candidate-rust-all' / m.TRAIT_PATH_FILE).write_text('trait-path')
            output.write_text('applied fixed trait visibility fixture')
        elif name == 'apply-constraint-matching-patch':
            self.assertEqual(argv, ['patch', '--batch', '--forward', '--no-backup-if-mismatch', '--fuzz=0',
                                   '-p1', '-d', str(self.work / 'candidate-typeinference'),
                                   '-i', str(HERE / 'codeql-rust-constraint-matching.patch')])
            self.assertEqual((self.work / 'candidate-typeinference' / m.SHARED_FILE).read_text(), 'helper-shared')
            self.assertEqual(m.read_json(self.out / 'shared-helper-identity.json')['variant'], 'qualifier-lines-shared-trait-v5')
            self.assertEqual(m.read_json(self.out / 'trait-visibility-identity.json')['variant'], 'qualifier-lines-shared-trait-v5')
            (self.work / 'candidate-typeinference' / m.SHARED_FILE).write_text('constraint-matching-shared')
            output.write_text('applied fixed ConstraintMatching fixture')
        elif name == 'apply-target-qualified-patch':
            target = self.work / 'candidate-target-typeinference' / m.SHARED_FILE
            self.assertEqual(argv, ['patch', '--batch', '--forward', '--no-backup-if-mismatch', '--fuzz=0',
                                   str(target), str(HERE / 'codeql-rust-target-qualified.patch')])
            self.assertEqual(target.read_text(), 'constraint-matching-shared')
            m.verify_workload_identity(self.closure, self.work, self.out, end)
            target.write_text('target-qualified-shared')
            output.write_text('applied fixed target-qualified fixture')
        elif name == 'private-default-suite': self.write(output, [str(self.work / 'default-queries' / r['relative']) for r in self.entries])
        elif name == 'resolver-session':
            self.assertTrue(observation['restricted_output'])
            self.assertTrue(observation['reap_orphans'])
            def response(record):
                closure = m.read_json(self.out / 'selected-closures.json')[record['kind']]
                return dict(libraryPath=[record['root'], *[v['root'] for v in closure.values()]],
                            dbscheme=str(self.work / 'candidate-rust-all/rust.dbscheme'),
                            compilationCache=[str(self.work / 'resolve-compilation-cache')],
                            relativeName=record['pack'] + '/' + record['relative'], qlPackName=record['pack'])
            fail = int(self.failed_stage[8:]) if self.failed_stage and self.failed_stage.startswith('resolve-') else None
            return fake_runtime_session(observation['observer'], output, response, fail_index=fail)
        elif name.startswith('resolve-'):
            path = Path(next(a.split('=', 1)[1] for a in argv if a.startswith('--query=')))
            kind = 'default' if path.is_relative_to(self.work / 'default-queries') else 'witness'
            root = self.work / ('default-queries' if kind == 'default' else 'public-witnesses')
            pack = m.DEFAULT_PACK if kind == 'default' else m.WITNESS_PACK
            closure = m.read_json(self.out / 'selected-closures.json')[kind]
            self.write(output, dict(libraryPath=[str(root), *[v['root'] for v in closure.values()]],
                                   dbscheme=str(self.work / 'candidate-rust-all/rust.dbscheme'),
                                   compilationCache=[str(self.work / 'resolve-compilation-cache')],
                                   relativeName=pack + '/' + str(path.relative_to(root)), qlPackName=pack))
        elif name == 'database-identity': self.write(output, dict(sourceLocationPrefix=str(self.source), languages=['rust'], datasetFolder=str(self.db / 'db-rust')))
        elif name == 'candidate-workload-batch':
            self.assertTrue(observation['restricted_output'])
            self.assertIn('--evaluator-log-level=3', argv)
            self.assertIn('--no-tuple-counting', argv)
            self.assertEqual(m.runtime.CAP, m.candidate_profile.OUTPUT_CAP-m.candidate_profile.NUMERIC_RESERVE +
                             m.runtime.output_bytes(out)-m.runtime.output_bytes(output.parent))
            records = m.read_json(self.out / 'query-inputs.json')
            paths = m.result_paths(records, self.db)
            for label, path in paths.items():
                path.parent.mkdir(parents=True, exist_ok=True); path.write_bytes(('BQRS-' + label).encode())
                if self.failed_stage == name: break
            output.write_text('batch output')
            capture = observation['evaluator_capture']
            events = [dict(type='LOG_HEADER', logVersion='0.5.0', codeqlVersion='2.27.4'),
                      dict(type='PREDICATE_STARTED', appearsAs={'TypeInference::inferType/2#fixture': [1]},
                           predicateType='RECURSIVE_INTENSIONAL', layerId=0),
                      dict(type='PIPELINE_STARTED', predicateStartEvent=1, generationNumber=1),
                      dict(type='PIPELINE_COMPLETED', startEvent=2, resultSize=1),
                      dict(type='PREDICATE_COMPLETED', startEvent=1, resultSize=1), dict(type='LOG_FOOTER')]
            capture.feed(b'\n'.join(json.dumps(dict(event, eventId=index, nanoTime=index*100,
                time='2026-01-01T00:00:00Z')).encode() for index, event in enumerate(events)))
            capture.finish(True, True)
            for stream in ('stdout','stderr'): observation['observer'].feed(stream, b'', final=True)
            if self.mutate == 'candidate': (self.work / 'candidate-rust-all' / m.comparison.TYPE_FILE).write_text('mutated')
            if self.mutate == 'model': (self.work / 'candidate-rust-all' / m.MODEL_FILE).write_text('mutated')
            if self.mutate == 'query': (self.work / 'default-queries/queries/Q00.ql').write_text('mutated')
            if self.mutate == 'database': (self.db / 'codeql-database.yml').write_text('mutated')
            if self.mutate == 'shared': (self.work / 'candidate-typeinference' / m.SHARED_FILE).write_text('mutated')
            if self.mutate == 'target': (self.work / 'candidate-target-typeinference' / m.SHARED_FILE).write_text('mutated')
            if self.mutate == 'target-receipt':
                receipt = m.read_json(self.out / 'target-qualified-identity.json')
                receipt['base_variant'] = 'forged'
                self.write(self.out / 'target-qualified-identity.json', receipt)
            if self.mutate == 'trait-path': (self.work / 'candidate-rust-all' / m.TRAIT_PATH_FILE).write_text('mutated')
        elif name == 'official-summary':
            self.assertTrue(observation['restricted_output'])
            self.assertEqual(argv[1:3], ['generate','log-summary'])
            self.assertEqual(timeout, 20)
            self.assertGreater(m.runtime.output_bytes(self.out / 'candidate-observation'), 0)
            self.assertTrue((self.out / 'raw-results.json').is_file())
            self.assertTrue((self.out / 'database_metadata-run-after.json').is_file())
            value = dict(raHash='fixture-ra', predicateName='fixture-predicate', appearsAs={},
                         evaluationStrategy='EXTENSIONAL', completionTime='2026-11-11T00:00:00Z',
                         dependencies={}, millis=7, position='', queryCausingWork='fixture-query', resultSize=1)
            if self.bad_summary: value['unrecognized'] = 'RAW_CANARY_UNKNOWN_SUMMARY'
            observer = observation['observer']
            self.assertFalse(observer.replay.start_attempted)
            observer.poll(end - m.runtime.CLEANUP_SECONDS - .75)
            replay_hash = hashlib.sha256()
            replay_bytes = 0
            with open(argv[-1], 'rb', buffering=0) as replay_input:
                while chunk := replay_input.read(65536):
                    replay_hash.update(chunk)
                    replay_bytes += len(chunk)
            self.assertEqual(replay_bytes, observer.replay.capture.observed)
            self.assertEqual(replay_hash.digest(), observer.replay.capture.observed_hash.digest())
            data = (json.dumps(dict(summaryLogVersion='0.4.0', codeqlVersion='2.27.4', startTime='2026-11-11T00:00:00Z'))+
                    '\n'+json.dumps(value)).encode()
            output.write_bytes(observer.feed('stdout', data, final=True))
            observer.feed('stderr', b'', final=True)
        elif name.startswith('info-'): self.write(output, {'result-sets': [{'name': '#select', 'rows': 1}]})
        elif name.startswith('decoded-'): self.write(output, {'#select': {'tuples': [] if self.bad_decode == name else [[1]]}})
        elif name == 'interpret-default-results':
            self.write(self.work / 'default-results.sarif', dict(version='2.1.0', runs=[dict(invocations=[{'executionSuccessful': True}], results=[{'message': {'text': 'local finding'}}])]))
            output.write_text('interpreted')
        elif name.startswith('decode-'):
            query = name[7:]; rows = self.rows(query)
            # Actual CodeQL 2.27.4 BQRS columns: named metric variables retain
            # label/present; expression-only Files and Calls use colN.
            headers = {
                'PeerCandidate': ['label', 'present'], 'PeerLegacy': ['label', 'present'],
                'Driver': ['label', 'present'], 'Image': ['label', 'present'],
                'Files': ['col0', 'col1'],
                'CallsCandidate': ['col0', 'col1', 'col2', 'col3'],
                'CallsLegacy': ['col0', 'col1', 'col2', 'col3'],
            }
            if query == self.bad_header: headers[query] = ['unexpected', 'columns']
            with output.open('w') as stream:
                writer = csv.writer(stream); writer.writerow(headers[query])
                writer.writerows([] if query == self.oracle_failure else rows)
        else: self.fail('unrecognized CLI stage ' + name)
        if name == 'candidate-workload-batch' and self.cancel_after_batch:
            os.kill(os.getpid(), signal.SIGTERM)
        if name == self.failed_stage:
            if name == 'candidate-workload-batch' and self.clean_batch_timeout:
                raise m.runtime.PhaseError(dict(exit=143, failure_kind='timeout', cleanup_errors=[], evidence_errors=[],
                                               error='injected command timeout'))
            raise TimeoutError('injected command timeout')
        return dict(exit=0, argv=argv, pid=1, cleanup_errors=[], evidence_errors=[])

    def invoke(self, phase, **outcomes):
        argv = [str(HERE / 'codeql-rust-candidate-workload.py'), phase, '--codeql', str(self.official / 'codeql'),
                '--source', str(self.source), '--database', str(self.db), '--official-evidence', str(self.checkpoint),
                '--output', str(self.out), '--work', str(self.work), '--deadline', str(self.deadline),
                '--analysis-deadline', str(self.budget['analysis_deadline'])]
        if phase == 'run':
            for stage in ('candidate-prepare', 'extraction', 'capture', 'checkpoint', 'official-artifact'):
                argv.extend(['--' + stage + '-outcome', outcomes.get(stage, 'success')])
        with patch.object(sys, 'argv', argv): return m.main()

    def prepare(self):
        self.assertEqual(self.invoke('prepare'), 0)
        m.comparison.verify_seal(self.out, time.monotonic() + 30)

    def test_44_actual_bindings_use_one_owned_resolver_session(self):
        self.prepare()
        self.assertEqual(len(m.read_json(self.out / 'library-resolution.json')), 44)
        sessions = [call for call in self.calls if call[0] == 'resolver-session']
        independent = [call for call in self.calls if call[1][1:3] == ['resolve', 'library-path']]
        self.assertEqual(len(sessions), 1)
        self.assertEqual(independent, [])

    def test_original_source_copy_preserves_support_and_discards_published_bindings(self):
        self.prepare()
        target = self.work / 'default-queries'
        self.assertEqual((target / 'queries/support/Stats.qll').read_bytes(), (self.published / 'queries/support/Stats.qll').read_bytes())
        self.assertFalse(any(target.rglob('*.qlx'))); self.assertFalse((target / '.codeql').exists())
        self.assertFalse((target / 'codeql-pack.lock.yml').exists()); self.assertFalse((target / '.packinfo').exists())
        self.assertTrue((target / 'queries/NonDefault.ql').is_file())
        records = m.read_json(self.out / 'query-inputs.json')
        self.assertEqual(len(records), 44); self.assertFalse(any(r['path'].endswith('NonDefault.ql') for r in records))
        self.assertEqual(len(m.read_json(self.out / 'library-resolution.json')), 44)
        with self.assertRaises(RuntimeError): m.capacity.bundle.pack_manifest(self.suite_helper / 'qlpack.yml')

    def test_single_44_query_batch_all_results_and_seven_oracles_with_local_only_sarif(self):
        self.prepare(); self.assertEqual(self.invoke('run'), 0)
        batches = [call for call in self.calls if call[0] == 'candidate-workload-batch']
        self.assertEqual(len(batches), 1)
        _, argv, end, _, _ = batches[0]
        self.assertIn('--no-use-precompiled', argv); self.assertIn('--no-default-compilation-cache', argv)
        queries = argv[argv.index('--') + 2:]
        self.assertEqual(queries, [r['path'] for r in m.read_json(self.out / 'query-inputs.json')])
        self.assertLessEqual(end - time.monotonic(), self.budget['analysis_deadline'] - time.time() + .1)
        interpreted = next(c[1] for c in self.calls if c[0] == 'interpret-default-results')
        self.assertEqual(len(interpreted[interpreted.index('--') + 2:]), 37)
        terminal = m.read_json(self.out / 'candidate-run-terminal.json')
        self.assertEqual(terminal['status'], 'CANDIDATE_RESEARCH_COMPLETED')
        self.assertEqual(terminal['candidate_variant'],'qualifier-lines-constraint-target-qualified-v7')
        self.assertEqual(terminal['image_contract_version'],'image-v2')
        self.assertEqual(m.read_json(self.out/'semantic-Image-terminal.json')['image_contract_version'],'image-v2')
        model=m.read_json(self.out/'lines-model-identity.json')
        self.assertEqual(m.read_json(self.out/'arity-patch-identity.json'), m.arity_identity())
        self.assertEqual(model['candidate_files'][m.comparison.TYPE_FILE]['sha256'],
                         hashlib.sha256(b'arity-candidate').hexdigest())
        self.assertEqual(model['changed_files'],sorted([m.MODEL_FILE,m.comparison.TYPE_FILE]))
        self.assertEqual(set(model['official_files']),set(model['candidate_files']))
        shared = m.read_json(self.out / 'shared-helper-identity.json')
        self.assertEqual(shared['variant'], 'qualifier-lines-shared-trait-v5')
        self.assertEqual(shared['base_variant'], 'qualifier-lines-v3')
        self.assertEqual(shared['changed_files'], [m.SHARED_FILE])
        self.assertEqual(shared['candidate_files'][m.SHARED_FILE]['sha256'], hashlib.sha256(b'helper-shared').hexdigest())
        self.assertEqual(set(shared['official_files']), set(shared['candidate_files']))
        trait = m.read_json(self.out / 'trait-visibility-identity.json')
        self.assertEqual(trait['variant'], 'qualifier-lines-shared-trait-v5')
        self.assertEqual(trait['base_variant'], 'qualifier-lines-v3')
        self.assertEqual(trait['before_files'], model['candidate_files'])
        self.assertEqual(trait['changed_files'], sorted((m.comparison.TYPE_FILE, m.TRAIT_PATH_FILE)))
        self.assertEqual(trait['candidate_files'][m.comparison.TYPE_FILE]['sha256'], m.TRAIT_TYPE_SHA)
        self.assertEqual(trait['candidate_files'][m.TRAIT_PATH_FILE]['sha256'], m.TRAIT_PATH_SHA)
        self.assertEqual(trait['base_receipts_sha256'], {name: m.runtime.digest(self.out / name) for name in
                         ('arity-patch-identity.json', 'lines-model-identity.json')})
        constraint = m.read_json(self.out / 'constraint-matching-identity.json')
        self.assertEqual(constraint['variant'], 'qualifier-lines-constraint-matching-v6')
        self.assertEqual(constraint['base_variant'], 'qualifier-lines-shared-trait-v5')
        self.assertEqual(constraint['upstream_commit'], '1c4fea90af949111bc1c625f1e72de56a12c7712')
        self.assertEqual(constraint['patch_sha256'], m.CONSTRAINT_PATCH_SHA)
        self.assertEqual(constraint['before_files'], shared['candidate_files'])
        self.assertEqual(constraint['changed_files'], [m.SHARED_FILE])
        self.assertEqual(constraint['candidate_files'][m.SHARED_FILE]['sha256'],
                         hashlib.sha256(b'constraint-matching-shared').hexdigest())
        self.assertEqual(set(constraint['before_files']), set(constraint['candidate_files']))
        self.assertEqual(constraint['base_receipts_sha256'], {name: m.runtime.digest(self.out / name) for name in
                         ('shared-helper-identity.json', 'trait-visibility-identity.json')})
        target = m.read_json(self.out / 'target-qualified-identity.json')
        self.assertEqual(target['variant'], m.RESEARCH_VARIANT)
        self.assertEqual(target['base_variant'], m.WORKLOAD_VARIANT)
        self.assertEqual(target['patch_sha256'], m.TARGET_QUALIFIED_PATCH_SHA)
        self.assertEqual(target['before_files'], constraint['candidate_files'])
        self.assertEqual(target['changed_files'], [m.SHARED_FILE])
        self.assertEqual(target['candidate_files'][m.SHARED_FILE]['sha256'], m.TARGET_QUALIFIED_SHA)
        self.assertEqual(target['base_receipts_sha256'], {
            'constraint-matching-identity.json': m.runtime.digest(self.out / 'constraint-matching-identity.json')})
        m.verify_workload_identity(self.closure, self.work, self.out, time.monotonic() + 30)
        self.assertIn('candidate-target-typeinference', m.private_inputs(self.work, time.monotonic() + 30))
        with self.assertRaisesRegex(RuntimeError, 'fixed single-file helper patch'):
            m.verify_v5_identity(self.closure, self.work, self.out, time.monotonic() + 30)
        closures = m.read_json(self.out / 'selected-closures.json')
        for closure in closures.values():
            self.assertEqual(closure['codeql/typeinference']['root'], str(self.work / 'candidate-target-typeinference'))
        self.assertEqual((Path(self.closure['codeql/typeinference']['root']) / m.SHARED_FILE).read_text(), 'stock-shared')
        self.assertEqual(terminal['production_capacity'], 'UNKNOWN'); self.assertEqual(terminal['full_rust_coverage'], 'HOLD')
        self.assertEqual(terminal['final_hosted_job_margin'], 'NOT_OBSERVED')
        self.assertEqual(terminal['performance_observation']['status'], 'COMPLETE')
        self.assertEqual(terminal['performance_observation']['numeric_status'], 'OBSERVED')
        observation = m.read_json(self.out / 'candidate-observation/observation-terminal.json')
        self.assertEqual(observation['capture']['version'], 'gzip-capture-v1')
        self.assertEqual(observation['capture']['encoding'], 'gzip')
        self.assertEqual(observation['capture']['memory_reserve_bytes'], 48 * 1024**2)
        self.assertEqual(observation['capture']['encoded_limit_bytes'], 47 * 1024**2)
        self.assertEqual(observation['summary']['replay']['status'], 'COMPLETE')
        self.assertTrue(observation['summary']['replay']['consumer_eof'])
        self.assertEqual(observation['batch_parent_cpu']['status'], 'MEASURED')
        self.assertEqual(observation['batch_parent_cpu']['intervals'], 1)
        feed_cost = observation['capture']['feed_cost']['buckets']
        self.assertEqual(sum(bucket['input_bytes'] for bucket in feed_cost.values()), observation['capture']['observed_bytes'])
        self.assertEqual(feed_cost['CAP_CROSSING']['wall']['status'], 'NOT_OBSERVED')
        self.assertLessEqual((self.out / 'candidate-observation/observation-terminal.json').stat().st_size,
                             m.runtime.MAX_CONTROL_RECEIPT)
        numeric = self.out / 'candidate-observation/numeric-projection.json'
        self.assertLessEqual(numeric.stat().st_size, m.runtime.MAX_CONTROL_RECEIPT)
        self.assertEqual(m.read_json(numeric)['pipeline_tail'][0]['pipeline_result_size'], 1)
        self.assertEqual(sum(c[0]=='official-summary' for c in self.calls), 1)
        self.assertEqual(len(terminal['public_semantic_tables']), 7)
        self.assertEqual(len(m.read_json(self.out / 'default-query-results.json')), 37)
        self.assertTrue((self.work / 'default-results.sarif').exists()); self.assertFalse(any(self.out.rglob('*.sarif')))
        self.assertFalse(any('local finding' in p.read_text(errors='replace') for p in self.out.rglob('*') if p.is_file()))
        self.assertLessEqual(m.runtime.output_bytes(self.out), m.PRIVATE_CAP)
        m.comparison.verify_seal(self.out, time.monotonic() + 30)

    def test_failed_batch_keeps_partial_bqrs_and_stops_before_info_decode_or_interpret(self):
        self.prepare(); self.failed_stage = 'candidate-workload-batch'
        self.assertEqual(self.invoke('run'), 1)
        names = [c[0] for c in self.calls]
        self.assertFalse(any(n.startswith(('info-', 'decoded-', 'decode-', 'interpret-')) for n in names))
        self.assertTrue((self.out / 'result-00.bqrs').exists())
        raw = m.read_json(self.out / 'raw-results.json')
        self.assertEqual(raw['01']['status'], 'NOT_PRODUCED')
        terminal = m.read_json(self.out / 'candidate-run-terminal.json')
        self.assertEqual(terminal['primary_failure']['stage'], self.failed_stage)
        self.assertNotIn('official-summary', names)

    def test_clean_timeout_preserves_primary_failure_and_allows_one_bounded_summary(self):
        self.prepare(); self.failed_stage='candidate-workload-batch'; self.clean_batch_timeout=True
        self.assertEqual(self.invoke('run'),1)
        terminal=m.read_json(self.out/'candidate-run-terminal.json')
        self.assertEqual(terminal['primary_failure']['stage'],'candidate-workload-batch')
        self.assertEqual(terminal['batch']['status'],'INCOMPLETE')
        self.assertEqual(terminal['public_semantics'],'NOT_REACHED')
        self.assertEqual(sum(c[0]=='official-summary' for c in self.calls),1)

    def test_core_result_preservation_failure_blocks_summary_after_clean_timeout(self):
        self.prepare(); self.failed_stage='candidate-workload-batch'; self.clean_batch_timeout=True
        with patch.object(m,'preserve_results',side_effect=RuntimeError('injected core evidence failure')):
            self.assertEqual(self.invoke('run'),1)
        terminal=m.read_json(self.out/'candidate-run-terminal.json')
        self.assertEqual(terminal['primary_failure']['stage'],'candidate-workload-batch')
        self.assertEqual(terminal['raw_result_preservation'],'INCOMPLETE')
        self.assertTrue(any(e['stage']=='raw-results' for e in terminal['secondary_errors']))
        self.assertFalse(any(c[0]=='official-summary' for c in self.calls))

    def test_unknown_summary_keeps_computation_and_oracles_separate(self):
        self.prepare(); self.bad_summary=True
        self.assertEqual(self.invoke('run'),1)
        terminal=m.read_json(self.out/'candidate-run-terminal.json')
        self.assertEqual(terminal['batch']['status'],'COMPUTED')
        self.assertEqual(terminal['default_result_verification'],'PASS')
        self.assertEqual(terminal['public_semantics'],'PASS')
        self.assertEqual(terminal['primary_failure']['stage'],'performance-observation')
        self.assertEqual(terminal['performance_observation']['summary_status'],'PARTIAL')
        for path in self.out.rglob('*'):
            if path.is_file(): self.assertNotIn(b'RAW_CANARY_UNKNOWN_SUMMARY',path.read_bytes())
        m.comparison.verify_seal(self.out,time.monotonic()+30)

    def test_memory_limit_preserves_44_results_and_oracles_without_completing_research(self):
        self.prepare()
        real_capture = m.candidate_profile.CompressedCapture
        with patch.object(m.candidate_profile, 'CompressedCapture', side_effect=lambda **kwargs: real_capture(4, **kwargs)):
            self.assertEqual(self.invoke('run'), 1)
        terminal = m.read_json(self.out/'candidate-run-terminal.json')
        self.assertEqual(terminal['status'], 'CANDIDATE_RESEARCH_INCOMPLETE')
        self.assertEqual(terminal['batch']['status'], 'COMPUTED')
        self.assertEqual(terminal['raw_result_preservation'], 'COMPLETE')
        self.assertEqual(len(list(self.out.glob('result-*.bqrs'))), 44)
        self.assertEqual(terminal['default_result_verification'], 'PASS')
        self.assertEqual(terminal['public_semantics'], 'PASS')
        self.assertEqual(len(terminal['public_semantic_tables']), 7)
        self.assertEqual(len(m.read_json(self.out/'default-query-results.json')), 37)
        self.assertEqual(terminal['primary_failure']['stage'], 'performance-observation')
        self.assertEqual(terminal['performance_observation']['capture_status'], 'INCOMPLETE')
        self.assertEqual(terminal['performance_observation']['summary_status'], 'NOT_REACHED_MEMORY_LIMIT')
        self.assertEqual(terminal['performance_observation']['numeric_status'], 'OBSERVED')
        self.assertTrue(m.read_json(self.out/'candidate-observation/numeric-projection.json')['raw_memory_limit_seen'])
        self.assertEqual(sum(c[0]=='candidate-workload-batch' for c in self.calls), 1)
        self.assertFalse(any(c[0]=='official-summary' for c in self.calls))
        self.assertLessEqual(m.runtime.output_bytes(self.out), m.PRIVATE_CAP)
        m.comparison.verify_seal(self.out,time.monotonic()+30)

    def test_capture_close_failure_keeps_original_failure_and_still_seals(self):
        self.prepare(); self.failed_stage='candidate-workload-batch'; self.clean_batch_timeout=True
        real_close=m.candidate_profile.Capture.close
        failed=[]
        def close(capture):
            real_close(capture)
            if not failed:
                failed.append(True);raise OSError('injected close error')
        with patch.object(m.candidate_profile.Capture,'close',close):
            self.assertEqual(self.invoke('run'),1)
        terminal=m.read_json(self.out/'candidate-run-terminal.json')
        self.assertEqual(terminal['primary_failure']['stage'],'candidate-workload-batch')
        self.assertTrue(any(e['stage']=='performance-observation-cleanup' for e in terminal['secondary_errors']))
        m.comparison.verify_seal(self.out,time.monotonic()+30)

    def test_failed_first_closeout_write_still_closes_capture_and_restores_budget(self):
        self.prepare(); captures=[]
        real_start=m.candidate_profile.Observation.start
        real_save=m.runtime.save
        def start(observation):
            real_start(observation);captures.append(observation.capture)
        def save(path,value,root,terminal=False):
            if path.name=='candidate-run-terminal.json' and value.get('status')=='CANDIDATE_RESEARCH_COMPLETED':
                raise RuntimeError('injected terminal write failure')
            return real_save(path,value,root,terminal)
        with patch.object(m.candidate_profile.Observation,'start',start),patch.object(m.runtime,'save',side_effect=save):
            with self.assertRaisesRegex(RuntimeError,'injected terminal write failure'): self.invoke('run')
        self.assertTrue(captures)
        self.assertTrue(all(c.fd is None for c in captures))
        self.assertEqual(m.runtime.CAP,m.PRIVATE_CAP)

    def test_cancel_after_batch_prevents_default_decode_and_optional_summary(self):
        self.prepare();self.cancel_after_batch=True
        self.assertEqual(self.invoke('run'),1)
        terminal=m.read_json(self.out/'candidate-run-terminal.json')
        self.assertTrue(terminal['cancelled'])
        self.assertEqual(terminal['batch']['status'],'COMPUTED')
        self.assertFalse(any(c[0].startswith(('info-','decode-','decoded-','official-summary')) for c in self.calls))

    def test_cancel_during_seal_updates_terminal_and_manifest_to_incomplete(self):
        self.prepare();real_seal=m.seal
        def seal(out,terminal,phase,deadline,end):
            real_seal(out,terminal,phase,deadline,end)
            if phase=='run':os.kill(os.getpid(),signal.SIGTERM)
        with patch.object(m,'seal',side_effect=seal):self.assertEqual(self.invoke('run'),1)
        terminal=m.read_json(self.out/'candidate-run-terminal.json')
        self.assertTrue(terminal['cancelled'])
        self.assertEqual(terminal['status'],'CANDIDATE_RESEARCH_INCOMPLETE')
        self.assertEqual(terminal['primary_failure']['error'],'CANCELLED')
        m.comparison.verify_seal(self.out,time.monotonic()+30)

    def test_oracle_mismatch_records_all_seven_without_retrying_batch(self):
        self.prepare(); self.oracle_failure = 'CallsCandidate'
        self.assertEqual(self.invoke('run'), 1)
        terminal = m.read_json(self.out / 'candidate-run-terminal.json')
        self.assertEqual(terminal['public_semantics'], 'FAIL')
        self.assertEqual(set(terminal['public_semantic_failures']), {'CallsCandidate'})
        self.assertEqual(terminal['primary_failure']['stage'], 'semantic-CallsCandidate')
        self.assertEqual(len(terminal['public_semantic_tables']), 6)
        self.assertEqual(sum(c[0] == 'candidate-workload-batch' for c in self.calls), 1)
        self.assertTrue((self.out / 'semantic-Image-terminal.json').exists())

    def test_recorded_image_flow_failure_remains_a_semantic_failure(self):
        self.prepare()
        original_rows = self.rows
        def recorded_rows(name):
            if name == 'Image':
                # Exact historical v1 observations cannot be accepted as v2.
                return [['format-original','1'],['format-replacement','1'],['original-flow','0'],
                        ['replacement-flow','0'],['contains-line-callee','1'],['replacement-line-callee','1'],
                        ['whole-line-flow','0'],['output-line-flow','0'],['wrong-function-flow','0']]
            return original_rows(name)

        with patch.object(self, 'rows', side_effect=recorded_rows):
            self.assertEqual(self.invoke('run'), 1)
        terminal = m.read_json(self.out / 'candidate-run-terminal.json')
        self.assertEqual(terminal['default_result_verification'], 'PASS')
        self.assertEqual(terminal['public_semantics'], 'FAIL')
        self.assertEqual(set(terminal['public_semantic_failures']), {'Image'})
        self.assertEqual(terminal['primary_failure']['stage'], 'semantic-Image')
        self.assertEqual(set(terminal['public_semantic_tables']),
                         {'PeerCandidate', 'CallsCandidate', 'Driver', 'Files', 'PeerLegacy', 'CallsLegacy'})
        self.assertEqual(sum(c[0] == 'candidate-workload-batch' for c in self.calls), 1)
        self.assertEqual(m.read_json(self.out / 'semantic-Image-terminal.json')['status'], 'FAIL')
        m.comparison.verify_seal(self.out, time.monotonic() + 30)

    def test_incomplete_decode_stops_before_remaining_results(self):
        self.prepare(); self.bad_decode = 'decoded-00'
        self.assertEqual(self.invoke('run'), 1)
        self.assertFalse(any(c[0] in ('info-01', 'interpret-default-results', 'decode-Image') for c in self.calls))
        self.assertEqual(m.read_json(self.out / 'candidate-run-terminal.json')['default_result_verification'], 'INCOMPLETE')

    def test_public_schema_error_stops_before_the_oracle_and_remaining_decodes(self):
        self.prepare(); self.bad_header = 'PeerCandidate'
        self.assertEqual(self.invoke('run'), 1)
        terminal = m.read_json(self.out / 'candidate-run-terminal.json')
        self.assertEqual(terminal['primary_failure']['stage'], 'decode-PeerCandidate')
        self.assertIn('public witness decode schema differs', terminal['primary_failure']['error'])
        self.assertEqual(terminal['public_semantics'], 'INCOMPLETE')
        self.assertEqual(terminal['public_semantic_tables'], {})
        self.assertEqual(terminal['public_semantic_failures'], {})
        self.assertFalse(any(c[0] == 'decode-CallsCandidate' for c in self.calls))
        self.assertTrue((self.out / 'result-43.bqrs').exists())
        m.comparison.verify_seal(self.out, time.monotonic() + 30)

    def test_failed_upstream_stage_never_admits_any_workload_subprocess(self):
        self.prepare(); before = len(self.calls)
        self.assertEqual(self.invoke('run', extraction='failure'), 1)
        self.assertEqual(len(self.calls), before)
        terminal = m.read_json(self.out / 'candidate-run-terminal.json')
        self.assertIn('prior stage incomplete', terminal['primary_failure']['error'])
        m.comparison.verify_seal(self.out, time.monotonic() + 30)

    def test_failure_during_layout_is_sealed_and_cannot_admit_later_queries(self):
        self.failed_stage = 'resolve-00'
        self.assertEqual(self.invoke('prepare'), 1)
        self.assertEqual(m.read_json(self.out / 'candidate-prepare-terminal.json')['primary_failure']['stage'], 'resolve-00')
        before = len(self.calls)
        self.assertEqual(self.invoke('run', **{'candidate-prepare': 'failure', 'extraction': 'skipped'}), 1)
        self.assertEqual(before, len(self.calls))
        m.comparison.verify_seal(self.out, time.monotonic() + 30)

    def test_candidate_mutation_during_successful_batch_fails_after_identity(self):
        self.prepare(); self.mutate = 'candidate'
        self.assertEqual(self.invoke('run'), 1)
        terminal = m.read_json(self.out / 'candidate-run-terminal.json')
        self.assertEqual(terminal['primary_failure']['stage'], 'private_inputs-after')

    def test_private_query_mutation_before_batch_is_not_admitted(self):
        self.prepare(); (self.work / 'default-queries/queries/Q00.ql').write_text('changed')
        before = len(self.calls); self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)

    def test_target_receipt_mutation_during_batch_fails_final_revalidation(self):
        self.prepare(); self.mutate = 'target-receipt'
        self.assertEqual(self.invoke('run'), 1)
        terminal = m.read_json(self.out / 'candidate-run-terminal.json')
        self.assertEqual(terminal['private_inputs_after'], 'UNCHANGED')
        self.assertEqual(terminal['primary_failure']['stage'], 'target_identity-after')
        self.assertNotEqual(terminal['status'], 'CANDIDATE_RESEARCH_COMPLETED')

    def test_target_tree_mutation_during_batch_is_not_admitted(self):
        self.prepare(); self.mutate = 'target'
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(m.read_json(self.out / 'candidate-run-terminal.json')['primary_failure']['stage'],
                         'private_inputs-after')

    def test_v6_preparation_cannot_resume_as_target_research(self):
        self.prepare()
        prepared = m.read_json(self.out / 'candidate-prepare-terminal.json')
        prepared['candidate_variant'] = m.WORKLOAD_VARIANT
        m.seal(self.out, prepared, 'prepare', self.deadline, time.monotonic() + 30)
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('preparation receipt differs',
                      m.read_json(self.out / 'candidate-run-terminal.json')['primary_failure']['error'])

    def test_missing_target_receipt_blocks_queries_after_resealing(self):
        self.prepare()
        (self.out / 'target-qualified-identity.json').unlink()
        prepared = m.read_json(self.out / 'candidate-prepare-terminal.json')
        m.seal(self.out, prepared, 'prepare', self.deadline, time.monotonic() + 30)
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('target-qualified-identity.json',
                      m.read_json(self.out / 'candidate-run-terminal.json')['primary_failure']['error'])

    def test_target_v6_fallback_is_rejected_before_queries(self):
        self.prepare()
        closures = m.read_json(self.out / 'selected-closures.json')
        for selected in closures.values():
            selected['codeql/typeinference']['root'] = str(self.work / 'candidate-typeinference')
        self.write(self.out / 'selected-closures.json', closures)
        prepared = m.read_json(self.out / 'candidate-prepare-terminal.json')
        m.seal(self.out, prepared, 'prepare', self.deadline, time.monotonic() + 30)
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('workload binding differs',
                      m.read_json(self.out / 'candidate-run-terminal.json')['primary_failure']['error'])

    def test_target_receipt_cannot_forge_the_retained_v6_baseline(self):
        self.prepare()
        receipt = m.read_json(self.out / 'target-qualified-identity.json')
        receipt['before_files'][m.SHARED_FILE]['sha256'] = m.TARGET_QUALIFIED_SHA
        self.write(self.out / 'target-qualified-identity.json', receipt)
        with self.assertRaisesRegex(RuntimeError, 'target-qualified identity differs'):
            m.verify_target_qualified_identity(self.closure, self.work, self.out, time.monotonic() + 30)

    def test_target_overlay_rejects_extra_missing_and_mode_changed_files(self):
        self.prepare()
        root = self.work / 'candidate-target-typeinference'
        support = root / 'Unchanged.qll'
        original, mode = support.read_bytes(), support.stat().st_mode
        for mutation in ('extra', 'missing', 'mode'):
            with self.subTest(mutation=mutation):
                if mutation == 'extra': (root / 'Extra.qll').write_text('unapproved')
                elif mutation == 'missing': support.unlink()
                else: support.chmod(mode ^ 0o100)
                try:
                    with self.assertRaisesRegex(RuntimeError, 'fixed target-qualified overlay'):
                        m.verify_target_qualified_identity(self.closure, self.work, self.out, time.monotonic() + 30)
                finally:
                    if mutation == 'extra': (root / 'Extra.qll').unlink()
                    elif mutation == 'missing': support.write_bytes(original)
                    support.chmod(mode)

    def test_target_permissions_cannot_follow_a_mutated_v6_permission_baseline(self):
        self.prepare()
        paths = [self.work / name / m.SHARED_FILE for name in
                 ('candidate-typeinference', 'candidate-target-typeinference')]
        modes = [path.stat().st_mode for path in paths]
        for path, mode in zip(paths, modes): path.chmod(mode ^ 0o020)
        try:
            # Old v6 records only Git execute bits; the new layer also binds all
            # permission bits to the actual unchanged official shared package.
            m.verify_workload_identity(self.closure, self.work, self.out, time.monotonic() + 30)
            with self.assertRaisesRegex(RuntimeError, 'fixed target-qualified overlay'):
                m.verify_target_qualified_identity(self.closure, self.work, self.out, time.monotonic() + 30)
        finally:
            for path, mode in zip(paths, modes): path.chmod(mode)

    def test_wrong_target_patch_stops_before_copy_or_process_and_retains_real_v6(self):
        with patch.object(m, 'TARGET_QUALIFIED_PATCH_SHA', '0' * 64):
            self.assertEqual(self.invoke('prepare'), 1)
        self.assertFalse(any(call[0] == 'apply-target-qualified-patch' for call in self.calls))
        self.assertFalse((self.work / 'candidate-target-typeinference').exists())
        self.assertFalse((self.out / 'target-qualified-identity.json').exists())
        m.verify_workload_identity(self.closure, self.work, self.out, time.monotonic() + 30)

    def test_target_copy_permission_drift_is_rejected_before_patch_process(self):
        original_copy = shutil.copytree
        def copy(source, destination, *args, **options):
            result = original_copy(source, destination, *args, **options)
            if Path(destination).name == 'candidate-target-typeinference':
                path = Path(destination) / m.SHARED_FILE
                path.chmod(path.stat().st_mode ^ 0o020)
            return result
        with patch.object(shutil, 'copytree', side_effect=copy):
            self.assertEqual(self.invoke('prepare'), 1)
        self.assertFalse(any(call[0] == 'apply-target-qualified-patch' for call in self.calls))
        self.assertFalse((self.out / 'target-qualified-identity.json').exists())
        m.verify_workload_identity(self.closure, self.work, self.out, time.monotonic() + 30)

    def test_v2_preparation_is_rejected_even_with_a_valid_evidence_seal(self):
        self.prepare()
        prepared = m.read_json(self.out / 'candidate-prepare-terminal.json')
        prepared['candidate_variant'] = 'qualifier-lines-v2'
        old_result = self.out / 'old-v2-result.bqrs'
        old_result.write_bytes(b'unverified old v2 result')
        m.seal(self.out, prepared, 'prepare', self.deadline, time.monotonic() + 30)
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('preparation receipt differs',
                      m.read_json(self.out / 'candidate-run-terminal.json')['primary_failure']['error'])
        self.assertEqual(old_result.read_bytes(), b'unverified old v2 result')
        self.assertEqual(m.read_json(self.out / 'candidate-prepare-terminal.json')['candidate_variant'],
                         'qualifier-lines-v2')

    def test_v3_preparation_cannot_be_resumed_as_the_shared_helper_workload(self):
        self.prepare()
        prepared = m.read_json(self.out / 'candidate-prepare-terminal.json')
        prepared['candidate_variant'] = 'qualifier-lines-v3'
        old_result = self.out / 'old-v3-result.bqrs'
        old_result.write_bytes(b'unverified old v3 result')
        m.seal(self.out, prepared, 'prepare', self.deadline, time.monotonic() + 30)
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('preparation receipt differs',
                      m.read_json(self.out / 'candidate-run-terminal.json')['primary_failure']['error'])
        self.assertEqual(old_result.read_bytes(), b'unverified old v3 result')

    def test_old_qualifier_bytes_cannot_be_admitted_under_the_new_variant(self):
        self.prepare()
        (self.work / 'candidate-rust-all' / m.comparison.TYPE_FILE).write_text('candidate')
        self.write(self.out / 'private-inputs-before.json', m.private_inputs(self.work, time.monotonic() + 30))
        prepared = m.read_json(self.out / 'candidate-prepare-terminal.json')
        m.seal(self.out, prepared, 'prepare', self.deadline, time.monotonic() + 30)
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('workload binding differs',
                      m.read_json(self.out / 'candidate-run-terminal.json')['primary_failure']['error'])

    def test_missing_arity_receipt_blocks_queries_even_after_evidence_resealing(self):
        self.prepare()
        (self.out / 'arity-patch-identity.json').unlink()
        prepared = m.read_json(self.out / 'candidate-prepare-terminal.json')
        m.seal(self.out, prepared, 'prepare', self.deadline, time.monotonic() + 30)
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('arity-patch-identity.json',
                      m.read_json(self.out / 'candidate-run-terminal.json')['primary_failure']['error'])

    def test_v4_preparation_cannot_be_resumed_as_the_trait_workload(self):
        self.prepare()
        prepared = m.read_json(self.out / 'candidate-prepare-terminal.json')
        prepared['candidate_variant'] = 'qualifier-lines-shared-v4'
        m.seal(self.out, prepared, 'prepare', self.deadline, time.monotonic() + 30)
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('preparation receipt differs',
                      m.read_json(self.out / 'candidate-run-terminal.json')['primary_failure']['error'])

    def test_v5_preparation_cannot_be_resumed_as_the_constraint_workload(self):
        self.prepare()
        prepared = m.read_json(self.out / 'candidate-prepare-terminal.json')
        prepared['candidate_variant'] = 'qualifier-lines-shared-trait-v5'
        m.seal(self.out, prepared, 'prepare', self.deadline, time.monotonic() + 30)
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('preparation receipt differs',
                      m.read_json(self.out / 'candidate-run-terminal.json')['primary_failure']['error'])

    def test_missing_constraint_receipt_blocks_queries_after_resealing(self):
        self.prepare()
        (self.out / 'constraint-matching-identity.json').unlink()
        prepared = m.read_json(self.out / 'candidate-prepare-terminal.json')
        m.seal(self.out, prepared, 'prepare', self.deadline, time.monotonic() + 30)
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('constraint-matching-identity.json',
                      m.read_json(self.out / 'candidate-run-terminal.json')['primary_failure']['error'])

    def test_v5_shared_bytes_cannot_be_admitted_after_resealing_inputs(self):
        self.prepare()
        (self.work / 'candidate-typeinference' / m.SHARED_FILE).write_text('helper-shared')
        self.write(self.out / 'private-inputs-before.json', m.private_inputs(self.work, time.monotonic() + 30))
        prepared = m.read_json(self.out / 'candidate-prepare-terminal.json')
        m.seal(self.out, prepared, 'prepare', self.deadline, time.monotonic() + 30)
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('fixed ConstraintMatching overlay',
                      m.read_json(self.out / 'candidate-run-terminal.json')['primary_failure']['error'])

    def test_forged_v5_shared_base_cannot_hide_an_extra_change_after_resealing(self):
        self.prepare()
        (self.work / 'candidate-typeinference/Unchanged.qll').write_text('unapproved change')
        actual = m.regular_snapshot(self.work / 'candidate-typeinference', time.monotonic() + 30)
        shared = m.read_json(self.out / 'shared-helper-identity.json')
        shared['official_files']['Unchanged.qll'] = shared['candidate_files']['Unchanged.qll'] = actual['Unchanged.qll']
        self.write(self.out / 'shared-helper-identity.json', shared)
        constraint = m.read_json(self.out / 'constraint-matching-identity.json')
        constraint['before_files'] = shared['candidate_files']
        constraint['candidate_files'] = actual
        constraint['base_receipts_sha256']['shared-helper-identity.json'] = m.runtime.digest(self.out / 'shared-helper-identity.json')
        self.write(self.out / 'constraint-matching-identity.json', constraint)
        self.write(self.out / 'private-inputs-before.json', m.private_inputs(self.work, time.monotonic() + 30))
        prepared = m.read_json(self.out / 'candidate-prepare-terminal.json')
        m.seal(self.out, prepared, 'prepare', self.deadline, time.monotonic() + 30)
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('fixed single-file helper patch',
                      m.read_json(self.out / 'candidate-run-terminal.json')['primary_failure']['error'])

    def test_constraint_overlay_rejects_file_set_and_mode_changes(self):
        self.prepare()
        private = self.work / 'candidate-typeinference'
        for relative, payload in (('Extra.qll', b'extra'), ('Unchanged.qll', None)):
            with self.subTest(relative=relative):
                target = private / relative
                before = target.read_bytes() if target.exists() else None
                if payload is None: target.unlink()
                else: target.write_bytes(payload)
                try:
                    with self.assertRaisesRegex(RuntimeError, 'fixed ConstraintMatching overlay'):
                        m.verify_workload_identity(self.closure, self.work, self.out, time.monotonic() + 30)
                finally:
                    if before is None: target.unlink()
                    else: target.write_bytes(before)
        target = private / m.SHARED_FILE
        mode = target.stat().st_mode
        target.chmod(mode ^ 0o100)
        try:
            with self.assertRaisesRegex(RuntimeError, 'fixed ConstraintMatching overlay'):
                m.verify_workload_identity(self.closure, self.work, self.out, time.monotonic() + 30)
        finally: target.chmod(mode)
        m.verify_workload_identity(self.closure, self.work, self.out, time.monotonic() + 30)

    def test_wrong_constraint_patch_stops_before_its_process(self):
        with patch.object(m, 'CONSTRAINT_PATCH_SHA', '0' * 64):
            self.assertEqual(self.invoke('prepare'), 1)
        self.assertFalse(any(call[0] == 'apply-constraint-matching-patch' for call in self.calls))
        self.assertFalse((self.out / 'constraint-matching-identity.json').exists())
        self.assertEqual((self.work / 'candidate-typeinference' / m.SHARED_FILE).read_text(), 'helper-shared')
        m.verify_v5_identity(self.closure, self.work, self.out, time.monotonic() + 30)

    def test_missing_trait_receipt_blocks_queries_after_resealing(self):
        self.prepare()
        (self.out / 'trait-visibility-identity.json').unlink()
        prepared = m.read_json(self.out / 'candidate-prepare-terminal.json')
        m.seal(self.out, prepared, 'prepare', self.deadline, time.monotonic() + 30)
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('trait-visibility-identity.json',
                      m.read_json(self.out / 'candidate-run-terminal.json')['primary_failure']['error'])

    def test_old_path_bytes_block_queries_even_with_fresh_inputs_and_seal(self):
        self.prepare()
        (self.work / 'candidate-rust-all' / m.TRAIT_PATH_FILE).write_text('stock-path')
        self.write(self.out / 'private-inputs-before.json', m.private_inputs(self.work, time.monotonic() + 30))
        prepared = m.read_json(self.out / 'candidate-prepare-terminal.json')
        m.seal(self.out, prepared, 'prepare', self.deadline, time.monotonic() + 30)
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('fixed trait visibility annotation patch',
                      m.read_json(self.out / 'candidate-run-terminal.json')['primary_failure']['error'])

    def test_forged_base_cannot_hide_an_extra_change_after_resealing(self):
        self.prepare()
        (self.work / 'candidate-rust-all/rust.dbscheme').write_text('changed')
        model = m.read_json(self.out / 'lines-model-identity.json')
        changed = m.regular_snapshot(self.work / 'candidate-rust-all', time.monotonic() + 30)['rust.dbscheme']
        model['official_files']['rust.dbscheme'] = model['candidate_files']['rust.dbscheme'] = changed
        self.write(self.out / 'lines-model-identity.json', model)
        self.write(self.out / 'private-inputs-before.json', m.private_inputs(self.work, time.monotonic() + 30))
        prepared = m.read_json(self.out / 'candidate-prepare-terminal.json')
        m.seal(self.out, prepared, 'prepare', self.deadline, time.monotonic() + 30)
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('two-file model',
                      m.read_json(self.out / 'candidate-run-terminal.json')['primary_failure']['error'])

    def test_trait_tree_changes_and_old_type_bytes_are_rejected(self):
        self.prepare()
        private = self.work / 'candidate-rust-all'
        original = Path(self.closure['codeql/rust-all']['root'])
        for relative, payload in ((m.comparison.TYPE_FILE, b'arity-candidate'),
                                  ('rust.dbscheme', b'changed'), ('Extra.qll', b'extra')):
            with self.subTest(relative=relative):
                target = private / relative
                before = target.read_bytes() if target.exists() else None
                target.write_bytes(payload)
                try:
                    with self.assertRaises(RuntimeError):
                        m.verify_workload_identity(self.closure, self.work, self.out, time.monotonic() + 30)
                finally:
                    if before is None: target.unlink()
                    else: target.write_bytes(before)
        target = private / m.TRAIT_PATH_FILE
        mode = target.stat().st_mode
        target.chmod(mode ^ 0o100)
        try:
            with self.assertRaises(RuntimeError):
                m.verify_workload_identity(self.closure, self.work, self.out, time.monotonic() + 30)
        finally: target.chmod(mode)
        m.verify_workload_identity(self.closure, self.work, self.out, time.monotonic() + 30)
        with self.assertRaisesRegex(RuntimeError, 'two-file model'):
            m.verify_candidate_identity(original, private, self.out, time.monotonic() + 30)

    def test_wrong_trait_patch_stops_before_its_process(self):
        with patch.object(m, 'TRAIT_PATCH_SHA', '0' * 64):
            self.assertEqual(self.invoke('prepare'), 1)
        self.assertFalse(any(call[0] == 'apply-trait-visibility-patch' for call in self.calls))
        self.assertFalse((self.out / 'trait-visibility-identity.json').exists())
        self.assertEqual((self.work / 'candidate-rust-all' / m.comparison.TYPE_FILE).read_text(), 'arity-candidate')
        self.assertEqual((self.work / 'candidate-rust-all' / m.TRAIT_PATH_FILE).read_text(), 'stock-path')

    def test_path_mutation_during_batch_fails_after_identity(self):
        self.prepare(); self.mutate = 'trait-path'
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(m.read_json(self.out / 'candidate-run-terminal.json')['primary_failure']['stage'], 'private_inputs-after')

    def test_shared_original_bytes_cannot_be_admitted_after_resealing_inputs(self):
        self.prepare()
        (self.work / 'candidate-typeinference' / m.SHARED_FILE).write_text('stock-shared')
        self.write(self.out / 'private-inputs-before.json', m.private_inputs(self.work, time.monotonic() + 30))
        prepared = m.read_json(self.out / 'candidate-prepare-terminal.json')
        m.seal(self.out, prepared, 'prepare', self.deadline, time.monotonic() + 30)
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('fixed ConstraintMatching overlay',
                      m.read_json(self.out / 'candidate-run-terminal.json')['primary_failure']['error'])

    def test_shared_extra_change_is_rejected_even_with_fresh_snapshots_and_seal(self):
        self.prepare()
        (self.work / 'candidate-typeinference/Unchanged.qll').write_text('unapproved change')
        self.write(self.out / 'private-inputs-before.json', m.private_inputs(self.work, time.monotonic() + 30))
        prepared = m.read_json(self.out / 'candidate-prepare-terminal.json')
        m.seal(self.out, prepared, 'prepare', self.deadline, time.monotonic() + 30)
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('fixed ConstraintMatching overlay',
                      m.read_json(self.out / 'candidate-run-terminal.json')['primary_failure']['error'])

    def test_shared_official_closure_fallback_is_rejected_before_queries(self):
        self.prepare()
        closures = m.read_json(self.out / 'selected-closures.json')
        for closure in closures.values():
            closure['codeql/typeinference'] = self.closure['codeql/typeinference']
        self.write(self.out / 'selected-closures.json', closures)
        prepared = m.read_json(self.out / 'candidate-prepare-terminal.json')
        m.seal(self.out, prepared, 'prepare', self.deadline, time.monotonic() + 30)
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('workload binding differs',
                      m.read_json(self.out / 'candidate-run-terminal.json')['primary_failure']['error'])

    def test_missing_shared_receipt_is_rejected_after_evidence_resealing(self):
        self.prepare()
        (self.out / 'shared-helper-identity.json').unlink()
        prepared = m.read_json(self.out / 'candidate-prepare-terminal.json')
        m.seal(self.out, prepared, 'prepare', self.deadline, time.monotonic() + 30)
        before = len(self.calls)
        self.assertEqual(self.invoke('run'), 1)
        self.assertEqual(len(self.calls), before)
        self.assertIn('shared-helper-identity.json',
                      m.read_json(self.out / 'candidate-run-terminal.json')['primary_failure']['error'])

    def test_wrong_shared_patch_identity_stops_before_its_patch_process(self):
        with patch.object(m.capacity.bundle, 'SHARED_PATCH_SHA', '0' * 64):
            self.assertEqual(self.invoke('prepare'), 1)
        self.assertFalse(any(call[0] == 'apply-shared-helper-patch' for call in self.calls))
        self.assertFalse((self.work / 'candidate-typeinference').exists())
        self.assertFalse((self.out / 'shared-helper-identity.json').exists())

    def test_shared_mutation_during_batch_fails_after_identity(self):
        self.prepare(); self.mutate = 'shared'
        self.assertEqual(self.invoke('run'), 1)
        terminal = m.read_json(self.out / 'candidate-run-terminal.json')
        self.assertEqual(terminal['primary_failure']['stage'], 'private_inputs-after')

    def test_wrong_arity_patch_identity_stops_before_the_patch_process(self):
        with patch.object(m, 'ARITY_PATCH_SHA', '0' * 64):
            self.assertEqual(self.invoke('prepare'), 1)
        self.assertEqual(self.calls, [])
        self.assertFalse((self.out / 'arity-patch-identity.json').exists())
        self.assertEqual((self.work / 'candidate-rust-all' / m.comparison.TYPE_FILE).read_text(), 'candidate')

    def test_v1_contract_is_rejected_before_candidate_preparation(self):
        contract=m.capacity.semantic_contract(self.source)
        self.write(self.checkpoint/'semantic-query-contract.json',contract)
        self.write(self.checkpoint/'semantic-query-inputs.json',contract['query_sha256'])
        self.assertEqual(self.invoke('prepare'),1)
        self.assertEqual(self.calls,[])
        self.assertFalse(self.work.exists())

    def test_missing_contract_is_rejected_before_candidate_preparation(self):
        (self.checkpoint/'semantic-query-contract.json').unlink()
        self.assertEqual(self.invoke('prepare'),1)
        self.assertEqual(self.calls,[])
        self.assertFalse(self.work.exists())

    def test_wrong_frozen_image_hash_is_rejected_before_candidate_preparation(self):
        inputs=m.read_json(self.checkpoint/'semantic-query-inputs.json');inputs['Image']='0'*64
        self.write(self.checkpoint/'semantic-query-inputs.json',inputs)
        self.assertEqual(self.invoke('prepare'),1)
        self.assertEqual(self.calls,[])

    def test_wrong_core_input_or_target_is_rejected(self):
        original=Path(self.closure['codeql/rust-all']['root'])
        (original/m.MODEL_FILE).write_bytes(self.core_bytes+b'changed\n')
        self.assertEqual(self.invoke('prepare'),1)
        self.assertEqual(self.calls,[])
        self.assertFalse((self.out/'lines-model-identity.json').exists())

    def test_wrong_core_target_is_rejected_before_writing_the_candidate(self):
        with patch.object(m,'MODEL_CANDIDATE_SHA','0'*64):
            self.assertEqual(self.invoke('prepare'),1)
        self.assertEqual((self.work/'candidate-rust-all'/m.MODEL_FILE).read_bytes(),self.core_bytes)
        self.assertEqual(self.calls,[])

    def test_third_candidate_change_is_not_admitted(self):
        prepare=self.prepare_candidate
        def with_extra(*args,**kwargs):
            private=prepare(*args,**kwargs);(private/'rust.dbscheme').write_text('changed')
            return private
        with patch.object(m.comparison,'prepare_candidate',side_effect=with_extra):
            self.assertEqual(self.invoke('prepare'),1)
        self.assertEqual(self.calls,[])
        self.assertIn('two-file model',m.read_json(self.out/'candidate-prepare-terminal.json')['primary_failure']['error'])

    def test_missing_qualifier_change_is_not_admitted(self):
        prepare=self.prepare_candidate
        def without_qualifier(*args,**kwargs):
            private=prepare(*args,**kwargs);(private/m.comparison.TYPE_FILE).write_text('stock')
            return private
        with patch.object(m.comparison,'prepare_candidate',side_effect=without_qualifier):
            self.assertEqual(self.invoke('prepare'),1)
        self.assertEqual(self.calls,[])

    def test_core_model_mutation_before_batch_is_not_admitted(self):
        self.prepare();(self.work/'candidate-rust-all'/m.MODEL_FILE).write_text('changed')
        before=len(self.calls)
        self.assertEqual(self.invoke('run'),1)
        self.assertEqual(len(self.calls),before)

    def test_core_model_mutation_during_batch_fails_after_identity(self):
        self.prepare();self.mutate='model'
        self.assertEqual(self.invoke('run'),1)
        self.assertEqual(m.read_json(self.out/'candidate-run-terminal.json')['primary_failure']['stage'],'private_inputs-after')

    def test_missing_checkpoint_verification_or_wrong_bundle_is_rejected(self):
        m.admit_checkpoint(self.capture, self.finish, self.source, self.deadline)
        for side, key, value in [('capture', 'evidence_status', 'PARTIAL'), ('capture', 'expected_bundle_version', '2.27.3'),
                                 ('finish', 'status', 'EXTRACTION_READY'), ('finish', 'public_semantics', 'PASS'),
                                 ('finish', 'verification_admitted', False)]:
            captured, finished = dict(self.capture), dict(self.finish)
            (captured if side == 'capture' else finished)[key] = value
            with self.assertRaises(RuntimeError): m.admit_checkpoint(captured, finished, self.source, self.deadline)


def validate_real_candidate_bindings(test, codeql, work, out, *, end, run):
    """The original 44 consumer, now bound to the separately verified target tree."""
    if run.end != end:
        raise RuntimeError('candidate binding deadline differs from its caller')
    source = run.args.source
    run([codeql, 'version', '--format=json'], 'version', 20)
    test.assertEqual(m.read_json(out / 'version.log')['version'], '2.27.4')
    test.assertIs(m.read_json(out / 'version.log').get('configFileFound'), False)
    official = m.capacity.bundle.bundle_closure(codeql.parent, '2.27.4')
    protected = dict(official, **{'codeql/suite-helpers': m.suite_helper(codeql.parent)})
    before = {key: m.regular_snapshot(Path(value['root']), end) for key, value in protected.items()}
    m.prepare_candidate(official, work, out, end, run, copy_end=end)
    m.prepare_shared_helper(official, work, out, end, run, copy_end=end)
    m.prepare_trait_visibility(official, work, out, end, run)
    m.verify_v5_identity(official, work, out, end)
    test.assertEqual(m.capacity.digest_until(work / 'candidate-typeinference' / m.SHARED_FILE, end),
                     '4a65a578b82c8fa739c2a802b2cc6763cebc34cb67be9008fd396d5dbb5bd47b')
    m.prepare_constraint_matching(official, work, out, end, run)
    m.verify_workload_identity(official, work, out, end)
    test.assertEqual(m.capacity.digest_until(work / 'candidate-typeinference' / m.SHARED_FILE, end),
                     '8b32c4d95cfd39f0fef5d365e27b3a0d734ae815272dc714fc60d40de56edc33')
    m.prepare_target_qualified(official, work, out, end, run)
    target = m.verify_target_qualified_identity(official, work, out, end)
    witness = m.research_closure(official, work)
    closures = dict(witness=witness, default=dict(witness, **{'codeql/suite-helpers': m.suite_helper(codeql.parent)}))
    test.assertEqual(len(witness), 13); test.assertEqual(len(closures['default']), 14)
    published = codeql.parent / 'qlpacks/codeql/rust-queries/0.1.46'
    published_before = m.regular_snapshot(published, end)
    suite_file = published / 'codeql-suites/rust-code-scanning.qls'
    run([codeql, 'resolve', 'queries', suite_file, '--format=json'], 'official-suite', 30)
    suite = dict(query_root=str(published), suite_sha256=m.capacity.digest_until(suite_file, end),
                 queries=m.capacity.suite_entries(m.read_json(out / 'official-suite.log'), published))
    frozen = m.capacity.semantic_contract(source, image_version=m.IMAGE_CONTRACT)['query_sha256']
    records = m.source_layout(codeql.parent, work, out, source, suite, frozen, end)
    private = m.private_inputs(work, end)
    m.resolve_layout(codeql, records, closures, work, out, run, end=end)
    test.assertEqual(m.private_inputs(work, end), private)
    resolved = m.read_json(out / 'library-resolution.json')
    test.assertEqual(len(records), 44); test.assertEqual(len(resolved), 44)
    for result in resolved:
        test.assertIn(str(work / 'candidate-target-typeinference'), result['actual']['libraryPath'])
        test.assertNotIn(str(work / 'candidate-typeinference'), result['actual']['libraryPath'])
        test.assertNotIn(official['codeql/typeinference']['root'], result['actual']['libraryPath'])
    test.assertEqual({key: m.regular_snapshot(Path(value['root']), end) for key, value in protected.items()}, before)
    test.assertEqual(m.read_json(out / 'shared-helper-identity.json')['changed_files'], [m.SHARED_FILE])
    trait = m.read_json(out / 'trait-visibility-identity.json')
    test.assertEqual(trait['candidate_files'][m.comparison.TYPE_FILE]['sha256'], m.TRAIT_TYPE_SHA)
    test.assertEqual(trait['candidate_files'][m.TRAIT_PATH_FILE]['sha256'], m.TRAIT_PATH_SHA)
    constraint = m.read_json(out / 'constraint-matching-identity.json')
    test.assertEqual(constraint['before_files'], m.read_json(out / 'shared-helper-identity.json')['candidate_files'])
    test.assertEqual(constraint['candidate_files'], m.regular_snapshot(work / 'candidate-typeinference', end))
    with test.assertRaisesRegex(RuntimeError, 'fixed single-file helper patch'):
        m.verify_v5_identity(official, work, out, end)
    with test.assertRaisesRegex(RuntimeError, 'two-file model'):
        m.verify_candidate_identity(Path(official['codeql/rust-all']['root']), work / 'candidate-rust-all', out, end)
    m.runtime.save(out / 'binding-validation.json', dict(
        variant=m.RESEARCH_VARIANT, queries=records, closures=closures,
        private_inputs=private, official_inputs=before,
        published_queries=published_before, target_identity=target), out)
    def verify_after(check_end):
        test.assertEqual(m.verify_target_qualified_identity(official, work, out, check_end), target)
        test.assertEqual(m.private_inputs(work, check_end), private)
        test.assertEqual({key: m.regular_snapshot(Path(value['root']), check_end)
                          for key, value in protected.items()}, before)
        test.assertEqual(m.regular_snapshot(published, check_end), published_before)
    return verify_after, {name: work / name for name in private}


PAIR_SECONDS, PAIR_CLOSEOUT_SECONDS = 600, 60
PAIR_EVIDENCE_CAP = 64 * 1024**2


def binding_module(name, path):
    module_spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(module_spec)
    module_spec.loader.exec_module(module)
    return module


def binding_check(end, cancelled=None):
    m.capacity.check_time(end)
    if cancelled and cancelled['cancelled']:
        raise m.runtime.CancelledError('paired binding validation cancelled')


def binding_cleanup(owner, end):
    """Reuse the repository's ancestry/pidfd operations, including setsid children."""
    initial = sorted(owner.descendants())
    for sig, seconds in m.runtime.CLEANUP_SIGNALS:
        binding_check(end)
        owner.send_owned(sig)
        until = min(end, time.monotonic() + seconds)
        while owner.descendants():
            owner.reap()
            if time.monotonic() >= until:
                break
            time.sleep(min(.02, max(0, until - time.monotonic())))
        owner.reap()
    binding_check(end)
    remaining = sorted(owner.descendants())
    if remaining:
        raise RuntimeError('paired binding descendants remain: ' + repr(remaining))
    return dict(initial_pids=initial, remaining_pids=remaining)


class BindingStreams:
    """Unmodified raw bytes plus explicit pipe EOF for non-resolver commands."""
    def __init__(self):
        self.eof = dict(stdout=False, stderr=False)

    def feed(self, name, data, final=False):
        if final:
            self.eof[name] = True
        return data

    def poll(self, end):
        binding_check(end)

    def summary(self):
        return dict(eof=dict(self.eof))


class BindingRunner:
    """Only the local 44/37 gate: retain the existing runtime and resolver argv."""
    def __init__(self, args, end, cancelled, owner):
        self.args, self.end, self.cancelled, self.owner = args, end, cancelled, owner
        self.terminal = {}
        self.env = dict(PATH='/usr/bin:/bin', HOME=os.environ['HOME'], LANG='C.UTF-8', TZ='UTC',
                        PYTHONDONTWRITEBYTECODE='1', TMPDIR=str(args.work / 'tmp'),
                        XDG_CACHE_HOME=str(args.work / 'cache'), CODEQL_THREADS='2', CODEQL_RAM='3072',
                        JAVA_TOOL_OPTIONS='-Xmx3072m -XX:ActiveProcessorCount=2')
        for name in ('tmp', 'cache'):
            (args.work / name).mkdir()
        m.runtime.save(args.output / 'process-environment.json', self.env, args.output)

    def __call__(self, argv, name, limit=None, *, observer=None,
                 restricted_output=True, reap_orphans=True):
        binding_check(self.end, self.cancelled)
        if not restricted_output:
            raise RuntimeError('paired binding commands require bounded raw capture')
        observed = observer or BindingStreams()
        failure = None
        try:
            result = m.runtime.run(list(map(str, argv)), self.args.source,
                self.args.output / (name + '.log'), self.end,
                min(self.end - time.monotonic(), limit or self.end - time.monotonic()),
                self.args.output, self.env, file_limit=None, observer=observed,
                restricted_output=True, reap_orphans=True)
            if (not isinstance(result.get('pid'), int) or result['pid'] <= 0 or result['exit'] != 0
                    or result['error'] or result['failure_kind'] or result['cleanup_errors']
                    or result['evidence_errors'] or result.get('cancelled')
                    or not all(observed.eof.values())):
                raise RuntimeError('paired binding runtime was not completely successful')
        except BaseException as exc:
            failure = exc
        try:
            cleanup = binding_cleanup(self.owner, self.end)
            m.runtime.save(self.args.output / (name + '.owner.json'), cleanup, self.args.output)
            if cleanup['initial_pids']:
                raise RuntimeError('paired binding descendants outlived runtime cleanup')
            binding_check(self.end, self.cancelled)
        except BaseException as exc:
            failure = failure or exc
        if failure:
            raise failure
        return result


def binding_repository_file(path, end):
    binding_check(end)
    data = capacity_layout._read_regular(path)
    return dict(sha256=hashlib.sha256(data).hexdigest(), bytes=len(data), mode=path.stat().st_mode & 0o7777)


def binding_tools(codeql, end):
    names = [*capacity_layout.tool_files(production_default=True), 'codeql-rust-candidate-workload.test.py',
             'codeql-rust-production.test.py',
             'test_codeql_rust_resolver.py']
    paths = [HERE / name for name in names] + [ROOT / 'scripts/ci-apt-install.py', ROOT / 'scripts/ci-apt-dns.py']
    identities = {}
    for path in paths:
        identities[str(path.relative_to(ROOT))] = binding_repository_file(path, end)
    cli = {}
    for path in (codeql, codeql.parent / 'tools/codeql.jar'):
        binding_check(end)
        if path.resolve() != path or not path.is_file():
            raise RuntimeError('unsafe pinned binding CLI input')
        before = path.stat()
        cli[str(path.relative_to(codeql.parent))] = dict(
            sha256=m.capacity.digest_until(path, end), bytes=before.st_size, mode=before.st_mode & 0o7777)
        after = path.stat()
        if ((after.st_dev, after.st_ino, after.st_size, after.st_mode, after.st_mtime_ns, after.st_ctime_ns)
                != (before.st_dev, before.st_ino, before.st_size, before.st_mode, before.st_mtime_ns, before.st_ctime_ns)):
            raise RuntimeError('binding CLI input changed while hashing')
    return dict(repository=identities, cli=cli,
                java=m.regular_snapshot(codeql.parent / 'tools/linux64/java', end))


def binding_archive(root, destination, evidence, end):
    """Preserve complete private source bytes before removing owned work/cache."""
    before = m.regular_snapshot(root, end)
    sizes = {name: (root / name).stat().st_size for name in before}
    if m.runtime.output_bytes(evidence) + sum(sizes.values()) > PAIR_EVIDENCE_CAP - 1024**2:
        raise RuntimeError('paired binding evidence cap before input copy')
    for name in before:
        binding_check(end)
        target = destination / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(root / name, target)
        if target.stat().st_size != sizes[name]:
            raise RuntimeError('paired binding input size changed during copy')
    if m.regular_snapshot(destination, end) != before or m.regular_snapshot(root, end) != before:
        raise RuntimeError('paired binding preserved input differs')


def binding_remove_work(output, identity, end):
    """Remove only the new work inode; check the common end before every operation."""
    removed = 0
    binding_check(end)
    parent = os.open(output, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    def remove(fd, depth):
        nonlocal removed
        binding_check(end)
        if depth > 100:
            raise RuntimeError('paired binding cleanup depth')
        for name in sorted(os.listdir(fd)):
            binding_check(end)
            item = os.stat(name, dir_fd=fd, follow_symlinks=False)
            if stat.S_ISDIR(item.st_mode):
                child = os.open(name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
                try:
                    if os.fstat(child) != item:
                        raise RuntimeError('paired binding cleanup inode changed')
                    remove(child, depth + 1)
                finally:
                    os.close(child)
                binding_check(end)
                current = os.stat(name, dir_fd=fd, follow_symlinks=False)
                if (current.st_dev, current.st_ino) != (item.st_dev, item.st_ino):
                    raise RuntimeError('paired binding cleanup directory changed')
                os.rmdir(name, dir_fd=fd)
            else:
                os.unlink(name, dir_fd=fd)
            removed += 1
            if removed > 50000:
                raise RuntimeError('paired binding cleanup entry limit')
    try:
        current = os.stat('work', dir_fd=parent, follow_symlinks=False)
        if (current.st_dev, current.st_ino) != identity:
            raise RuntimeError('paired binding owned work changed')
        child = os.open('work', os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=parent)
        try:
            if (os.fstat(child).st_dev, os.fstat(child).st_ino) != identity:
                raise RuntimeError('paired binding opened work changed')
            remove(child, 0)
        finally:
            os.close(child)
        binding_check(end)
        current = os.stat('work', dir_fd=parent, follow_symlinks=False)
        if (current.st_dev, current.st_ino) != identity:
            raise RuntimeError('paired binding owned work changed')
        os.rmdir('work', dir_fd=parent)
        binding_check(end)
        if (output / 'work').exists():
            raise RuntimeError('paired binding work reappeared')
        return dict(status='REMOVED', entries=removed + 1)
    finally:
        os.close(parent)


def binding_seal(evidence, terminal, end, cancelled):
    marker = evidence / 'ADMISSION.json'
    try:
        binding_check(end, cancelled)
        if any((evidence / name).exists() for name in ('ADMISSION.json', 'terminal.json', 'hashes.json')):
            raise RuntimeError('paired binding seal files must be new')
        m.runtime.save(evidence / 'terminal.json', terminal, evidence, terminal=True)
        m.runtime.save(marker, dict(status='PAIR_BINDINGS_COMPLETE',
            require_outer_exit_zero=True, counts=[44, 37]), evidence, terminal=True)
        hashes = {str(path.relative_to(evidence)): m.capacity.digest_until(path, end)
                  for path in sorted(evidence.rglob('*')) if path.is_file()}
        m.runtime.save(evidence / 'hashes.json', hashes, evidence, terminal=True)
        if m.runtime.output_bytes(evidence) > PAIR_EVIDENCE_CAP:
            raise RuntimeError('paired binding final evidence cap')
        sealed = m.comparison.verify_seal(evidence, end)
        binding_check(end, cancelled)
        return sealed
    except BaseException:
        marker.unlink(missing_ok=True)
        raise


def run_real_binding_pair(test, codeql, output):
    """Fixed local protocol; no database, compilation, evaluation or retry."""
    started = _PAIR_ENTRY_STARTED
    end, work_end = started + PAIR_SECONDS, started + PAIR_SECONDS - PAIR_CLOSEOUT_SECONDS
    if (codeql.resolve() != codeql or not codeql.is_file() or output.resolve() != output
            or output.exists() or not output.parent.is_dir()
            or any(output.is_relative_to(root) or root.is_relative_to(output)
                   for root in (ROOT, codeql.parent))):
        raise RuntimeError('paired binding output must be new and outside protected roots')
    output.mkdir(mode=0o700)
    work, evidence = output / 'work', output / 'evidence'
    work.mkdir(); evidence.mkdir()
    identity = (work.stat().st_dev, work.stat().st_ino)
    owner = None
    subreaper = None
    try:
        with m.runtime.cancellation_guard() as cancelled:
            binding_check(work_end, cancelled)
            owner = binding_module('binding_owner', ROOT / 'scripts/ci-apt-install.py')
            if owner.descendants():
                raise RuntimeError('paired binding entry requires exclusive child ownership')
            subreaper = m.resolver.Subreaper()
            before = binding_tools(codeql, work_end)
            m.runtime.save(evidence / 'tools-before.json', before, evidence)
            for relative in before['repository']:
                binding_check(work_end, cancelled)
                destination = evidence / 'tools' / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(ROOT / relative, destination)
                test.assertEqual(binding_repository_file(destination, work_end), before['repository'][relative])
            production = binding_module('production_binding_test', HERE / 'codeql-rust-production.test.py')
            contract = dict(started=started, end=end, work_end=work_end, total_seconds=600,
                closeout_seconds=60, counts=[44, 37], variants=[m.RESEARCH_VARIANT, production.m.PRODUCTION_VARIANT],
                codeql=str(codeql), process_file_limit=None,
                runtime_evidence_cap_per_consumer=m.runtime.CAP,
                final_pair_evidence_cap=PAIR_EVIDENCE_CAP,
                cap_scope='per-consumer runtime writes; combined pair is final admission, not physical peak',
                resource_request=dict(CODEQL_THREADS=2, CODEQL_RAM=3072),
                java_options='-Xmx3072m -XX:ActiveProcessorCount=2; not a process RSS limit',
                no_database_compilation_or_evaluation=True)
            m.runtime.save(evidence / 'contract.json', contract, evidence)
            validations = []
            for name, validate in (('target44', validate_real_candidate_bindings),
                                   ('production37', production.validate_real_production_bindings)):
                binding_check(work_end, cancelled)
                private, out = work / name, evidence / name
                private.mkdir(); out.mkdir()
                source = private / 'source'; source.mkdir()
                args = SimpleNamespace(codeql=codeql, source=source, work=private, output=out)
                run = BindingRunner(args, work_end, cancelled, owner)
                verify, archives = validate(test, codeql, private, out, end=work_end, run=run)
                validations.append((name, verify, archives))
            binding_check(end, cancelled)
            for name, verify, archives in validations:
                verify(end)
                for relative, root in archives.items():
                    binding_archive(root, evidence / 'inputs' / name / relative, evidence, end)
                verify(end)
            test.assertEqual(binding_tools(codeql, end), before)
            for relative, expected in before['repository'].items():
                test.assertEqual(binding_repository_file(evidence / 'tools' / relative, end), expected)
            cleanup = binding_cleanup(owner, end)
            test.assertEqual(cleanup['initial_pids'], [])
            removed = binding_remove_work(output, identity, end)
            subreaper.close(); subreaper = None
            binding_check(end, cancelled)
            terminal = dict(status='PAIR_BINDINGS_COMPLETE', counts=[44, 37],
                work_removed=removed, children=cleanup, inputs='UNCHANGED',
                elapsed_seconds=time.monotonic() - started)
            binding_seal(evidence, terminal, end, cancelled)
            binding_check(end, cancelled)
    except BaseException as exc:
        (evidence / 'ADMISSION.json').unlink(missing_ok=True)
        try:
            if owner is not None:
                binding_cleanup(owner, end)
            if work.exists():
                binding_remove_work(output, identity, end)
            binding_check(end)
            m.runtime.save(evidence / 'INCOMPLETE.json',
                dict(status='INCOMPLETE', error=repr(exc), elapsed_seconds=time.monotonic() - started),
                evidence, terminal=True)
        except BaseException:
            pass  # No new deadline, post-deadline hashes, or success marker.
        raise
    finally:
        if subreaper is not None:
            subreaper.close()


class BindingPairProtocol(unittest.TestCase):
    """Synthetic source consumers and Python children; never start CodeQL."""
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.bundle = self.root / 'bundle'; self.bundle.mkdir()
        self.codeql = self.bundle / 'codeql'; self.codeql.write_text('not executable')
        self.output = self.root / 'pair'
        self.events, self.verifications = [], []
        self.clock = [1000.0]

    def consumer(self, label, after=None):
        def validate(test, codeql, work, out, *, end, run):
            self.events.append((label, end, run.end))
            private = work / 'private'; private.mkdir()
            (private / 'input.ql').write_bytes(label.encode())
            (out / 'raw.log').write_bytes((label + '\n').encode())
            def verify(check_end):
                self.verifications.append((label, check_end))
                self.assertEqual((private / 'input.ql').read_bytes(), label.encode())
            if after:
                after()
            return verify, dict(private=private)
        return validate

    def invoke_pair(self, candidate=None, production=None, tools=None):
        original_module = binding_module
        def load(name, path):
            if name == 'production_binding_test':
                return SimpleNamespace(validate_real_production_bindings=production or self.consumer('production37'),
                    m=SimpleNamespace(PRODUCTION_VARIANT='qualifier-lines-negative-projection-v8'))
            return original_module(name, path)
        with (patch(__name__ + '.binding_module', side_effect=load),
              patch(__name__ + '._PAIR_ENTRY_STARTED', 1000),
              patch(__name__ + '.validate_real_candidate_bindings', candidate or self.consumer('target44')),
              patch(__name__ + '.binding_tools', side_effect=tools or (lambda *_: dict(repository={}))),
              patch.object(time, 'monotonic', side_effect=lambda: self.clock[0])):
            run_real_binding_pair(self, self.codeql, self.output)

    def test_pair_executes_each_consumer_once_with_one_clock_and_preserves_raw_inputs(self):
        self.invoke_pair()
        self.assertEqual(self.events, [('target44', 1540, 1540), ('production37', 1540, 1540)])
        self.assertEqual(self.verifications, [('target44', 1600)] * 2 + [('production37', 1600)] * 2)
        evidence = self.output / 'evidence'
        self.assertFalse((self.output / 'work').exists())
        sealed = m.comparison.verify_seal(evidence, time.monotonic() + 5)
        self.assertIn('ADMISSION.json', sealed)
        for name in ('target44', 'production37'):
            self.assertEqual((evidence / name / 'raw.log').read_bytes(), (name + '\n').encode())
            self.assertEqual((evidence / 'inputs' / name / 'private/input.ql').read_bytes(), name.encode())
        self.assertEqual(m.read_json(evidence / 'contract.json')['work_end'], 1540)
        self.assertEqual(m.read_json(evidence / 'contract.json')['variants'],
                         ['qualifier-lines-constraint-target-qualified-v7', 'qualifier-lines-negative-projection-v8'])

    def real_tool_inputs(self):
        repo = self.root / 'repository'
        helpers = repo / 'scripts/security'; helpers.mkdir(parents=True)
        # The explicit patch fixture is independent of the production tool list.
        names = {*capacity_layout.tool_files(production_default=True),
                 'codeql-rust-candidate-workload.test.py', 'codeql-rust-production.test.py',
                 'test_codeql_rust_resolver.py', 'codeql-rust-negative-projection.patch'}
        for name in names:
            shutil.copy2(HERE / name, helpers / name)
        for name in ('ci-apt-install.py', 'ci-apt-dns.py'):
            shutil.copy2(ROOT / 'scripts' / name, repo / 'scripts' / name)
        for name in ('tools/codeql.jar', 'tools/linux64/java/bin/java'):
            path = self.bundle / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b'identity fixture; not executable')
        patch_file = helpers / 'codeql-rust-negative-projection.patch'
        original = patch_file.read_bytes()
        expected = dict(sha256=hashlib.sha256(original).hexdigest(), bytes=len(original),
                        mode=patch_file.stat().st_mode & 0o7777)
        actual_binding_tools, observed = binding_tools, []
        def actual_tools(codeql, end):
            value = actual_binding_tools(codeql, end)
            observed.append(value)
            return value
        return repo, helpers, patch_file, original, expected, actual_tools, observed

    def test_pair_freezes_and_seals_the_actual_negative_patch(self):
        repo, helpers, _, original, expected, tools, observed = self.real_tool_inputs()
        with patch(__name__ + '.ROOT', repo), patch(__name__ + '.HERE', helpers):
            self.invoke_pair(tools=tools)
        key = 'scripts/security/codeql-rust-negative-projection.patch'
        self.assertEqual(len(observed), 2)
        self.assertEqual([value['repository'][key] for value in observed], [expected, expected])
        evidence = self.output / 'evidence'
        self.assertEqual(m.read_json(evidence / 'tools-before.json')['repository'][key], expected)
        self.assertEqual((evidence / 'tools' / key).read_bytes(), original)
        self.assertIn('tools/' + key, m.comparison.verify_seal(evidence, time.monotonic() + 5))

    def test_actual_negative_patch_change_prevents_pair_sealing(self):
        repo, helpers, patch_file, original, expected, tools, observed = self.real_tool_inputs()
        def mutate():
            patch_file.write_bytes(original + b'changed fixture\n')
        with patch(__name__ + '.ROOT', repo), patch(__name__ + '.HERE', helpers):
            with self.assertRaises(AssertionError):
                self.invoke_pair(production=self.consumer('production37', after=mutate), tools=tools)
        key = 'scripts/security/codeql-rust-negative-projection.patch'
        self.assertEqual(self.events, [('target44', 1540, 1540), ('production37', 1540, 1540)])
        self.assertEqual(len(observed), 2)
        self.assertEqual(observed[0]['repository'][key], expected)
        self.assertNotEqual(observed[1]['repository'][key]['sha256'], expected['sha256'])
        evidence = self.output / 'evidence'
        self.assertEqual((evidence / 'tools' / key).read_bytes(), original)
        self.assertFalse((evidence / 'ADMISSION.json').exists())
        self.assertFalse((evidence / 'hashes.json').exists())
        self.assertEqual(m.read_json(evidence / 'INCOMPLETE.json')['status'], 'INCOMPLETE')

    def test_second_consumer_gets_no_fresh_window_after_preparation_exhausts_work_time(self):
        def exhaust(): self.clock[0] = 1540
        with self.assertRaises(TimeoutError):
            self.invoke_pair(candidate=self.consumer('target44', exhaust))
        self.assertEqual(self.events, [('target44', 1540, 1540)])
        self.assertFalse((self.output / 'evidence/ADMISSION.json').exists())
        self.assertFalse((self.output / 'work').exists())
        self.assertEqual(m.read_json(self.output / 'evidence/INCOMPLETE.json')['status'], 'INCOMPLETE')

    def test_time_before_the_pair_function_is_charged_to_the_module_origin(self):
        self.clock[0] = 1500
        self.invoke_pair()
        self.assertEqual(self.events, [('target44', 1540, 1540), ('production37', 1540, 1540)])
        self.assertEqual(m.read_json(self.output / 'evidence/terminal.json')['elapsed_seconds'], 500)

    def test_source_identity_change_prevents_sealing_both_completed_consumers(self):
        values = iter((dict(repository={}, identity=1), dict(repository={}, identity=2)))
        with self.assertRaises(AssertionError):
            self.invoke_pair(tools=lambda *_: next(values))
        self.assertEqual(len(self.events), 2)
        self.assertFalse((self.output / 'evidence/ADMISSION.json').exists())
        self.assertFalse((self.output / 'evidence/hashes.json').exists())

    def test_seal_write_or_readback_failure_cannot_leave_admission(self):
        for failed in ('terminal.json', 'ADMISSION.json', 'hashes.json', 'readback'):
            with self.subTest(failed=failed), tempfile.TemporaryDirectory(dir=self.root) as name:
                evidence = Path(name); (evidence / 'raw.log').write_bytes(b'complete original output')
                original = m.runtime.save
                def save(path, *args, **kwargs):
                    if path.name == failed:
                        raise OSError('injected write failure')
                    return original(path, *args, **kwargs)
                with contextlib.ExitStack() as stack:
                    stack.enter_context(patch.object(m.runtime, 'save', side_effect=save))
                    if failed == 'readback':
                        stack.enter_context(patch.object(m.comparison, 'verify_seal', side_effect=OSError('readback')))
                    with self.assertRaises(OSError):
                        binding_seal(evidence, {}, time.monotonic() + 5, dict(cancelled=False))
                self.assertFalse((evidence / 'ADMISSION.json').exists())
                self.assertEqual((evidence / 'raw.log').read_bytes(), b'complete original output')

    def test_late_cancel_or_deadline_after_readback_revokes_admission(self):
        for fault in ('cancel', 'deadline'):
            with self.subTest(fault=fault), tempfile.TemporaryDirectory(dir=self.root) as name:
                evidence = Path(name); (evidence / 'raw.log').write_bytes(b'full')
                state, clock = dict(cancelled=False), [1000]
                original = m.comparison.verify_seal
                def readback(root, end):
                    value = original(root, end)
                    if fault == 'cancel': state['cancelled'] = True
                    else: clock[0] = 1010
                    return value
                with (patch.object(time, 'monotonic', side_effect=lambda: clock[0]),
                      patch.object(m.comparison, 'verify_seal', side_effect=readback)):
                    with self.assertRaises((m.runtime.CancelledError, TimeoutError)):
                        binding_seal(evidence, {}, 1010, state)
                self.assertFalse((evidence / 'ADMISSION.json').exists())

    def test_combined_cap_rejects_final_seal_and_copy_admission_without_truncating_raw(self):
        evidence = self.root / 'evidence'; evidence.mkdir()
        original = b'x' * 4096; (evidence / 'raw.log').write_bytes(original)
        with patch(__name__ + '.PAIR_EVIDENCE_CAP', 1024):
            with self.assertRaisesRegex(RuntimeError, 'final evidence cap'):
                binding_seal(evidence, {}, time.monotonic() + 5, dict(cancelled=False))
        self.assertFalse((evidence / 'ADMISSION.json').exists())
        self.assertEqual((evidence / 'raw.log').read_bytes(), original)
        source = self.root / 'private'; source.mkdir(); (source / 'input').write_bytes(b'xx')
        destination = evidence / 'inputs'
        with patch(__name__ + '.PAIR_EVIDENCE_CAP', 1024**2):
            with self.assertRaisesRegex(RuntimeError, 'before input copy'):
                binding_archive(source, destination, evidence, time.monotonic() + 5)
        self.assertFalse(destination.exists())

    def test_owned_cleanup_never_follows_a_work_symlink(self):
        self.output.mkdir(); work = self.output / 'work'; work.mkdir()
        target = self.root / 'protected'; target.mkdir(); (target / 'keep').write_text('keep')
        (work / 'link').symlink_to(target)
        identity = (work.stat().st_dev, work.stat().st_ino)
        self.assertEqual(binding_remove_work(self.output, identity, time.monotonic() + 5)['status'], 'REMOVED')
        self.assertEqual((target / 'keep').read_text(), 'keep')

    def test_python_child_retains_raw_eof_and_rejects_setsided_descendant_even_after_exit_zero(self):
        owner = binding_module('synthetic_binding_owner', ROOT / 'scripts/ci-apt-install.py')
        subreaper = m.resolver.Subreaper()
        end = time.monotonic() + 15
        self.addCleanup(subreaper.close)
        self.addCleanup(lambda: binding_cleanup(owner, time.monotonic() + 5))
        for label, program in (
            ('complete', "import sys; print('original stdout'); print('original stderr', file=sys.stderr)"),
            ('escaped', "import subprocess,sys; p=subprocess.Popen([sys.executable,'-c','import time;time.sleep(60)'],start_new_session=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL); print(p.pid,flush=True)")):
            root = self.root / label; root.mkdir()
            work, out, source = (root / name for name in ('work', 'evidence', 'source'))
            for folder in (work, out, source): folder.mkdir()
            args = SimpleNamespace(work=work, output=out, source=source)
            with patch.dict(os.environ, CODEQL_JAVA_HOME='/untrusted', _JAVA_OPTIONS='-Xmx99g'):
                run = BindingRunner(args, end, dict(cancelled=False), owner)
            self.assertNotIn('CODEQL_JAVA_HOME', run.env); self.assertNotIn('_JAVA_OPTIONS', run.env)
            if label == 'complete':
                result = run([sys.executable, '-c', program], label)
                self.assertEqual(result['output_observation']['eof'], dict(stdout=True, stderr=True))
                self.assertEqual((out / (label + '.log')).read_text(), 'original stdout\n')
                self.assertEqual((out / (label + '.log.stderr')).read_text(), 'original stderr\n')
            else:
                with self.assertRaisesRegex(RuntimeError, 'outlived runtime cleanup'):
                    run([sys.executable, '-c', program], label)
                self.assertEqual(m.read_json(out / (label + '.log.result.json'))['exit'], 0)
                self.assertTrue(m.read_json(out / (label + '.owner.json'))['initial_pids'])
                self.assertEqual(owner.descendants(), set())


@unittest.skipUnless(os.environ.get('OASIS7_CODEQL_CANDIDATE_CLI'), 'explicit pinned CodeQL integration input required')
class RealCodeQL(unittest.TestCase):
    def test_actual_patches_and_all_44_shared_library_bindings(self):
        """One persisted 44-target → 37-production gate, sharing a single clock."""
        if os.environ.get('OASIS7_CODEQL_PRODUCTION_CLI'):
            self.fail('the paired gate must not also enable the standalone production test')
        codeql = Path(os.environ['OASIS7_CODEQL_CANDIDATE_CLI']).absolute()
        output = Path(os.environ['OASIS7_CODEQL_BINDING_OUTPUT']).absolute()
        run_real_binding_pair(self, codeql, output)


class Workflow(unittest.TestCase):
    def test_separate_manual_path_uses_one_clock_and_private_evidence(self):
        text = (ROOT / '.github/workflows/codeql-rust-capacity.yml').read_text()
        self.assertIn('- candidate-workload-research', text)
        self.assertIn("timeout-minutes: ${{ inputs.mode == 'official-default-research' && 360 || inputs.mode == 'qualifier-research' && 150 || 90 }}", text)
        self.assertIn("skip-queries: ${{ inputs.mode == 'qualifier-research' || inputs.mode == 'candidate-workload-research' }}", text)
        self.assertLess(text.index('- id: candidate_prepare'), text.index('- id: analyze'))
        self.assertLess(text.index('- id: official_artifact'), text.index('- id: candidate_workload'))
        self.assertEqual(text.count('--analysis-deadline "$CAPACITY_ANALYSIS_DEADLINE"'), 2)
        self.assertNotIn('extraction_minutes', text)
        self.assertIn("NODE_OPTIONS: ${{ inputs.mode == 'candidate-workload-research'", text)
        self.assertIn('codeql-rust-action-deadline.cjs', text)
        self.assertIn('|| env.NODE_OPTIONS', text)
        run = text.split('      - id: candidate_workload', 1)[1].split('      - id: qualifier_comparison', 1)[0]
        self.assertIn("if: always() && inputs.mode == 'candidate-workload-research'", run)
        self.assertIn('path: ${{ runner.temp }}/rust-candidate-workload-evidence', run)
        self.assertNotIn('path: ${{ runner.temp }}/rust-candidate-workload-data', run)
        self.assertNotIn('security-events:', text); self.assertNotIn('upload-sarif', text)
        self.assertIn('upload: never', text); self.assertIn('upload-database: false', text)
        self.assertIn('codeql-rust-candidate-workload.test.py', (ROOT / 'scripts/ci-tests.sh').read_text())


if __name__ == '__main__': unittest.main()
