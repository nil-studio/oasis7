#!/usr/bin/env python3
"""Candidate source binding, single-batch results, failure and absolute-clock contracts."""
import contextlib
import csv
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import signal
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

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


class Workload(unittest.TestCase):
    """Real filesystem and oracles; only external CLI and hosted checkout observations are substituted."""
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.source, self.runner = self.root / 'source', self.root / 'runner'
        self.source.mkdir(); self.runner.mkdir()
        self.official, self.db, self.checkpoint = [self.runner / n for n in ('bundle', 'db', 'official-evidence')]
        for p in (self.official, self.db, self.checkpoint): p.mkdir()
        self.out, self.work = self.runner / 'candidate-evidence', self.runner / 'candidate-work'
        self.output = self.runner / 'github-output'; self.output.write_text('')
        self.published = self.official / 'qlpacks/codeql/rust-queries/0.1.46'
        (self.published / 'queries/support').mkdir(parents=True)
        (self.published / 'queries/support/Stats.qll').write_text('predicate support() { any() }\n')
        self.entries = []
        for i in range(37):
            path = self.published / ('queries/Q' + str(i).zfill(2) + '.ql')
            path.write_text('/** @id rust/example-' + str(i) + ' */\nimport support.Stats\nselect 1\n')
            self.entries.append(dict(relative=str(path.relative_to(self.published)), id='rust/example-' + str(i), sha256=m.runtime.digest(path)))
            path.with_suffix('.qlx').write_text('compiled stock; must never be copied')
        (self.published / 'queries/NonDefault.ql').write_text('select 2\n')
        (self.published / '.codeql/libraries/stock').mkdir(parents=True)
        (self.published / '.codeql/libraries/stock/TypeInference.qll').write_text('embedded stock')
        for name in ('qlpack.yml', 'codeql-pack.lock.yml', '.packinfo'): (self.published / name).write_text('published binding')
        (self.published / 'codeql-suites').mkdir()
        (self.published / 'codeql-suites/rust-code-scanning.qls').write_text('- queries: queries\n')
        self.suite_sha = m.runtime.digest(self.published / 'codeql-suites/rust-code-scanning.qls')
        self.entries_sha = hashlib.sha256(json.dumps(self.entries, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
        self.suite = dict(query_root=str(self.published), suite_sha256=self.suite_sha, queries=self.entries)
        self.suite_helper = self.official / 'qlpacks/codeql/suite-helpers/1.0.61'
        self.suite_helper.mkdir(parents=True)
        (self.suite_helper / 'qlpack.yml').write_text('name: codeql/suite-helpers\nversion: 1.0.61\nlibrary: false\ndependencies: {}\n')
        self.closure = {}
        for name in ['rust-all', *['fixture-' + str(i) for i in range(12)]]:
            root = self.official / 'qlpacks/codeql' / name / '0.2.25'; root.mkdir(parents=True)
            (root / 'qlpack.yml').write_text('library: true\nname: codeql/' + name + '\nversion: 0.2.25\ndependencies: {}\n')
            self.closure['codeql/' + name] = dict(name='codeql/' + name, version='0.2.25', dependencies={}, manifest_sha256=m.runtime.digest(root / 'qlpack.yml'), root=str(root))
        original = Path(self.closure['codeql/rust-all']['root'])
        (original / m.comparison.TYPE_FILE).parent.mkdir(parents=True)
        (original / m.comparison.TYPE_FILE).write_text('stock')
        self.core_bytes=b'extensions:\n'+m.MODEL_MARKER+b'      # Existing fixed models\n'
        (original / m.MODEL_FILE).parent.mkdir(parents=True)
        (original / m.MODEL_FILE).write_bytes(self.core_bytes)
        (original / 'rust.dbscheme').write_text('schema')
        (self.db / 'codeql-database.yml').write_text('language: rust\nfinalised: true\n')
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
                           database_metadata_sha256=m.runtime.digest(self.db / 'codeql-database.yml'))
        self.provenance = dict(candidate='/cargo/git/libp2p-0.57.0', legacy='/cargo/registry/libp2p-0.56.0')
        for name, value in [('capacity-budget', self.budget), ('default-suite', self.suite), ('library-closure', self.closure),
                            ('resources', self.resources), ('libp2p-provenance', self.provenance),
                            ('capacity-prepare-terminal', dict(common, status='PREPARED', image_contract_version=m.IMAGE_CONTRACT)),
                            ('capacity-capture-terminal', self.capture), ('capacity-finish-terminal', self.finish),
                            ('semantic-query-contract', m.capacity.semantic_contract(self.source,image_version=m.IMAGE_CONTRACT)),
                            ('semantic-query-inputs', {n: hashlib.sha256(t.encode()).hexdigest() for n, t in m.capacity.semantic_queries(self.source,image_version=m.IMAGE_CONTRACT).items()})]:
            self.write(self.checkpoint / (name + '.json'), value)
        self.write(self.checkpoint / 'hashes.json', {p.name: m.runtime.digest(p) for p in self.checkpoint.iterdir()})
        self.calls = []; self.failed_stage = self.mutate = self.oracle_failure = self.bad_decode = self.bad_header = None
        self.clean_batch_timeout = self.bad_summary = self.cancel_after_batch = False
        self.stack = contextlib.ExitStack(); self.addCleanup(self.stack.close)
        for target, key, value in ((m, 'DEFAULT_ENTRIES_SHA', self.entries_sha), (m, 'SUITE_SHA', self.suite_sha),
                                   (m, 'MODEL_OFFICIAL_SHA', hashlib.sha256(self.core_bytes).hexdigest()),
                                   (m, 'MODEL_CANDIDATE_SHA', hashlib.sha256(self.core_bytes.replace(m.MODEL_MARKER,m.MODEL_MARKER+m.MODEL_LINE)).hexdigest()),
                                   (m.comparison, 'OFFICIAL_SHA', hashlib.sha256(b'stock').hexdigest()),
                                   (m.comparison, 'CANDIDATE_SHA', hashlib.sha256(b'candidate').hexdigest())):
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
        if name == 'private-default-suite': self.write(output, [str(self.work / 'default-queries' / r['relative']) for r in self.entries])
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
            self.assertEqual(m.runtime.CAP, m.candidate_profile.OUTPUT_CAP +
                             m.runtime.output_bytes(out)-m.runtime.output_bytes(output.parent))
            records = m.read_json(self.out / 'query-inputs.json')
            paths = m.result_paths(records, self.db)
            for label, path in paths.items():
                path.parent.mkdir(parents=True, exist_ok=True); path.write_bytes(('BQRS-' + label).encode())
                if self.failed_stage == name: break
            output.write_text('batch output')
            capture = observation['evaluator_capture']
            capture.feed((json.dumps(dict(type='LOG_HEADER', logVersion='0.5.0', codeqlVersion='2.27.4'))+
                          '\n'+json.dumps(dict(type='LOG_FOOTER'))).encode())
            capture.finish(True, True)
            for stream in ('stdout','stderr'): observation['observer'].feed(stream, b'', final=True)
            if self.mutate == 'candidate': (self.work / 'candidate-rust-all' / m.comparison.TYPE_FILE).write_text('mutated')
            if self.mutate == 'model': (self.work / 'candidate-rust-all' / m.MODEL_FILE).write_text('mutated')
            if self.mutate == 'query': (self.work / 'default-queries/queries/Q00.ql').write_text('mutated')
            if self.mutate == 'database': (self.db / 'codeql-database.yml').write_text('mutated')
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
        return dict(exit=0, argv=argv)

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
        self.assertEqual(terminal['candidate_variant'],'qualifier-lines-v2')
        self.assertEqual(terminal['image_contract_version'],'image-v2')
        self.assertEqual(m.read_json(self.out/'semantic-Image-terminal.json')['image_contract_version'],'image-v2')
        model=m.read_json(self.out/'lines-model-identity.json')
        self.assertEqual(model['changed_files'],sorted([m.MODEL_FILE,m.comparison.TYPE_FILE]))
        self.assertEqual(set(model['official_files']),set(model['candidate_files']))
        self.assertEqual(terminal['production_capacity'], 'UNKNOWN'); self.assertEqual(terminal['full_rust_coverage'], 'HOLD')
        self.assertEqual(terminal['final_hosted_job_margin'], 'NOT_OBSERVED')
        self.assertEqual(terminal['performance_observation']['status'], 'COMPLETE')
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
        real_capture = m.candidate_profile.Capture
        with patch.object(m.candidate_profile, 'Capture', side_effect=lambda: real_capture(4)):
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
