#!/usr/bin/env python3
"""Admission and isolation contracts for optional private-library observations."""
import importlib.util
import contextlib
import csv
import hashlib
import json
import os
from pathlib import Path
import tempfile
import time
import sys
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
spec = importlib.util.spec_from_file_location('comparison', HERE / 'codeql-rust-qualifier-comparison.py')
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


class Comparison(unittest.TestCase):
    def test_fixed_window_includes_preparation_and_keeps_job_margin(self):
        for remaining, expected in [(5400, 900), (1800, 900), (1200, 750), (509, 59), (450, 0), (-1, 0)]:
            seconds, end = m.allocation(10000 + remaining, 10000, 123)
            self.assertEqual((seconds, end), (expected, 123 + expected))
            if seconds:
                self.assertGreaterEqual(remaining - seconds, 450)
        for invalid in (5401, float('inf'), float('nan')):
            with self.assertRaises(RuntimeError):
                m.allocation(10000 + invalid, 10000, 123)

    def terminals(self):
        source, deadline = Path('/source'), 12345
        common = dict(mode='capacity', source=m.capacity.CAPACITY_SOURCE, source_root=str(source),
                      fixed_job_deadline=deadline, analyze_outcome='success')
        captured = dict(common, phase='capture', status='CAPTURED', evidence_status='COMPLETE',
                        cargo_guard={'status': 'PASS'}, coverage_capture={'status': 'PASS'},
                        checkout_gate='PASS', lock_observation_gate='PASS', resource_gate='PASS')
        names = set(m.capacity.semantic_queries(source))
        finished = dict(common, phase='finish', status='FAILED', verification_admitted=True,
                        default_result_verification='PASS', public_semantics='FAIL',
                        public_semantic_tables={n: {} for n in names - set(m.NAMES)},
                        public_semantic_failures={n: 'callee target row missing/duplicate' for n in m.NAMES},
                        after_identity={'source': 'NOT_REACHED', 'official': 'NOT_REACHED'})
        return captured, finished, source, deadline

    def research_terminals(self):
        captured, finished, source, deadline = self.terminals()
        for value in (captured, finished):
            value.update(mode='qualifier-research', expected_bundle_version='2.27.4')
        finished.update(status='EXTRACTION_READY', assessment_status='RESEARCH_READY',
                        extraction_checkpoint='PASS', archived_source='PASS',
                        default_result_verification='NOT_REQUESTED', public_semantics='NOT_REQUESTED',
                        after_identity={'source': 'UNCHANGED', 'official': 'UNCHANGED'})
        finished.pop('public_semantic_tables'); finished.pop('public_semantic_failures')
        return captured, finished, source, deadline

    def test_independent_research_has_a_bounded_distinct_window(self):
        for remaining, expected in ((9000, 7200), (7650, 7200), (4800, 4350), (509, 59), (450, 0)):
            seconds, end = m.allocation(10000 + remaining, 10000, 123, mode='qualifier-research')
            self.assertEqual((seconds, end), (expected, 123 + expected))
            self.assertGreaterEqual(remaining - seconds, 450)
        for invalid in (9001, float('inf'), float('nan')):
            with self.assertRaises(RuntimeError):
                m.allocation(10000 + invalid, 10000, 123, mode='qualifier-research')

    def test_extraction_checkpoint_is_separate_from_capacity_admission(self):
        m.admit(*self.research_terminals(), mode='qualifier-research')
        with self.assertRaises(RuntimeError):
            m.admit(*self.research_terminals())
        with self.assertRaises(RuntimeError):
            m.admit(*self.terminals(), mode='qualifier-research')
        for side, key, value in [(0, 'expected_bundle_version', '2.27.3'), (0, 'evidence_status', 'PARTIAL'),
                (0, 'cargo_guard', {'status': 'FAIL'}), (1, 'status', 'RESEARCH_INCOMPLETE'),
                (1, 'extraction_checkpoint', 'INCOMPLETE'), (1, 'archived_source', 'FAIL'),
                (1, 'after_identity', {'source': 'UNCHANGED', 'official': 'CHANGED'}),
                (1, 'default_result_verification', 'PASS'), (1, 'public_semantics', 'PASS')]:
            with self.subTest(key=key):
                args = list(self.research_terminals()); args[side][key] = value
                with self.assertRaises(RuntimeError):
                    m.admit(*args, mode='qualifier-research')

    def test_callee_failure_is_admitted_but_incomplete_or_changed_inputs_are_not(self):
        args = self.terminals()
        m.admit(*args)
        for side, key, value in [(0, 'lock_observation_gate', 'FAIL'), (0, 'resource_gate', 'FAIL'),
                (0, 'evidence_status', 'PARTIAL'), (0, 'cargo_guard', {'status': 'FAIL'}),
                (0, 'coverage_capture', {'status': 'FAIL'}), (0, 'source', 'b' * 40),
                (1, 'analyze_outcome', 'cancelled'), (1, 'verification_admitted', False),
                (1, 'default_result_verification', 'INCOMPLETE'), (1, 'public_semantics', 'INCOMPLETE'),
                (1, 'public_semantic_tables', {})]:
            with self.subTest(key=key):
                changed = list(self.terminals())
                changed[side][key] = value
                with self.assertRaises(RuntimeError):
                    m.admit(*changed)

    def test_library_substitution_does_not_mutate_official_closure(self):
        official = {'codeql/rust-all': {'root': '/official/rust-all', 'version': '0.2.25'},
                    'codeql/dataflow': {'root': '/official/dataflow', 'version': '1.0.0'}}
        candidate = m.candidate_closure(official, Path('/private/rust-all'))
        self.assertEqual(official['codeql/rust-all']['root'], '/official/rust-all')
        self.assertEqual(candidate['codeql/rust-all']['root'], '/private/rust-all')
        self.assertEqual(candidate['codeql/dataflow'], official['codeql/dataflow'])
        self.assertNotIn('/official/rust-all', [v['root'] for v in candidate.values()])

    def test_zero_baseline_rows_allow_candidate_but_wrong_targets_and_regressions_fail(self):
        source = Path('/source')
        provenance = {'candidate': '/cargo/libp2p-0.56', 'legacy': '/cargo/libp2p-0.54'}
        observed = {'official': {}, 'candidate': {}}
        for name in m.NAMES:
            context = 'candidate' if name == 'CallsCandidate' else 'legacy'
            rows = [[str(source / m.capacity.model.BASE / context / 'src/adapter.rs'), '42',
                     'with_existing_identity', provenance[context] + '/src/builder/phase/identity.rs']]
            observed['official'][name] = m.observation(name, [], source, provenance)
            observed['candidate'][name] = m.observation(name, rows, source, provenance)
            with self.assertRaises(RuntimeError):
                m.observation(name, rows * 2, source, provenance)
            wrong = [row[:] for row in rows]
            wrong[0][3] = '/cargo/foreign/src/identity.rs'
            with self.assertRaises(RuntimeError):
                m.observation(name, wrong, source, provenance)
        self.assertEqual(m.compare(observed)['restored'], list(m.NAMES))
        observed['official'] = observed['candidate']
        self.assertEqual(m.compare(observed)['preserved'], list(m.NAMES))
        observed['candidate'] = {n: {'contract': 'MISSING', 'rows': []} for n in m.NAMES}
        with self.assertRaises(RuntimeError):
            m.compare(observed)

    def test_official_seal_rejects_changed_missing_extra_and_symlink_files(self):
        for change in ('none', 'changed', 'missing', 'extra', 'symlink'):
            with self.subTest(change=change), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                (root / 'terminal.json').write_text('{"status":"FAILED"}\n')
                digest = hashlib.sha256((root / 'terminal.json').read_bytes()).hexdigest()
                (root / 'hashes.json').write_text(json.dumps({'terminal.json': digest}))
                if change == 'changed': (root / 'terminal.json').write_text('changed')
                elif change == 'missing': (root / 'terminal.json').unlink()
                elif change == 'extra': (root / 'new').write_text('extra')
                elif change == 'symlink': (root / 'alias').symlink_to(root / 'terminal.json')
                if change == 'none':
                    self.assertEqual(m.verify_seal(root, time.monotonic() + 10)['terminal.json'], digest)
                else:
                    with self.assertRaises(RuntimeError):
                        m.verify_seal(root, time.monotonic() + 10)

    def test_patch_input_is_the_reviewed_behavior_only_patch(self):
        data = (HERE / 'codeql-rust-omitted-qualifier.patch').read_bytes()
        self.assertEqual(hashlib.sha256(data).hexdigest(), m.PATCH_SHA)
        self.assertNotIn(b'researchSelfStage', data)
        self.assertEqual(data.count(b'--- a/'), 1)

    def test_actual_entry_runs_both_variants_and_preserves_original_failure(self):
        cases = [('capacity', 'none'), ('capacity', 'timeout')]
        cases += [('qualifier-research', failure) for failure in ('none', 'timeout', 'compile', 'decode', 'metadata')]
        for mode, failure in cases:
            research, interrupt = mode == 'qualifier-research', failure != 'none'
            with self.subTest(mode=mode, failure=failure), tempfile.TemporaryDirectory() as directory, contextlib.ExitStack() as stack:
                root = Path(directory)
                source, runner, official = root / 'source', root / 'runner', root / 'official'
                for path in (source, runner, official): path.mkdir()
                frozen, out, work, db = (runner / name for name in ('frozen', 'out', 'work', 'db'))
                frozen.mkdir(); db.mkdir()
                (db / 'codeql-database.yml').write_text('---\nfinalised: true\n')
                library = official / 'rust-all'
                type_file = library / m.TYPE_FILE
                type_file.parent.mkdir(parents=True)
                type_file.write_text('old')
                (library / 'qlpack.yml').write_text('official manifest')
                closure = {'codeql/rust-all': {'root': str(library), 'version': '0.2.25'}}
                captured, finished, _, _ = self.research_terminals() if research else self.terminals()
                finished['database_metadata_sha256'] = hashlib.sha256((db / 'codeql-database.yml').read_bytes()).hexdigest()
                deadline = time.time() + (9000 if research else 1800)
                for value in (captured, finished):
                    value.update(source_root=str(source), fixed_job_deadline=deadline)
                resources = {'CODEQL_THREADS': 4, 'CODEQL_RAM': 4096}
                provenance = {'candidate': '/cargo/libp2p-0.56', 'legacy': '/cargo/libp2p-0.54'}
                inputs = {'capacity-capture-terminal.json': captured, 'capacity-finish-terminal.json': finished,
                          'resources.json': resources, 'library-closure.json': closure,
                          'libp2p-provenance.json': provenance,
                          'semantic-query-inputs.json': {n: hashlib.sha256(t.encode()).hexdigest()
                                                       for n, t in m.capacity.semantic_queries(source).items()}}
                for name, value in inputs.items(): (frozen / name).write_text(json.dumps(value))
                hashes = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in frozen.iterdir()}
                (frozen / 'hashes.json').write_text(json.dumps(hashes))
                before = {p.name: p.read_bytes() for p in frozen.iterdir()}
                calls = []
                def run(argv, cwd, output, end, timeout, evidence, env, **kwargs):
                    calls.append(argv)
                    self.assertFalse({'GH_TOKEN', 'GITHUB_TOKEN', 'RUSTC_WRAPPER'} & env.keys())
                    self.assertLessEqual(end, time.monotonic() + (m.RESEARCH_SECONDS if research else m.MAX_SECONDS) - m.AUDIT_RESERVE)
                    self.assertEqual(cwd, source)
                    output.write_text('')
                    if argv[0] == 'patch':
                        Path(argv[-2]).write_text('new')
                    elif argv[1:3] == ['resolve', 'database']:
                        output.write_text(json.dumps(dict(sourceLocationPrefix=str(source), languages=['rust'], datasetFolder=str(db / 'db-rust'))))
                    elif argv[1:3] == ['resolve', 'library-path']:
                        q = Path(next(v.split('=', 1)[1] for v in argv if v.startswith('--query=')))
                        packs = next(v.split('=', 1)[1] for v in argv if v.startswith('--additional-packs=')).split(os.pathsep)
                        pack = (q.parent / 'qlpack.yml').read_text().splitlines()[0].split(': ', 1)[1]
                        output.write_text(json.dumps(dict(libraryPath=[str(q.parent), *packs], dbscheme=packs[0] + '/rust.dbscheme',
                            compilationCache=[], relativeName=pack + '/' + q.name, qlPackName=pack)))
                    elif argv[1:3] == ['query', 'run']:
                        self.assertIn('--no-default-compilation-cache', argv)
                        for option in ('--compilation-cache=', '--common-caches='):
                            self.assertTrue(Path(next(v.split('=', 1)[1] for v in argv if v.startswith(option))).is_relative_to(work))
                        if interrupt and output.name == 'candidate-CallsLegacy-run.log':
                            raise TimeoutError('injected candidate timeout')
                        Path(next(v.split('=', 1)[1] for v in argv if v.startswith('--output='))).write_bytes(b'bqrs fixture')
                    elif argv[1:3] == ['database', 'run-queries']:
                        self.assertTrue(research)
                        self.assertEqual(argv[3], str(db))
                        self.assertLessEqual(timeout, m.VARIANT_SECONDS)
                        self.assertNotIn('--no-rerun', argv)
                        self.assertIn('--no-default-compilation-cache', argv)
                        for option in ('--compilation-cache=', '--common-caches='):
                            self.assertTrue(Path(next(v.split('=', 1)[1] for v in argv if v.startswith(option))).is_relative_to(work))
                        queries = [Path(v) for v in argv[4:] if v.endswith('.ql')]
                        self.assertEqual([q.stem for q in queries], list(m.NAMES))
                        pack = (queries[0].parent / 'qlpack.yml').read_text().splitlines()[0].split(': ', 1)[1]
                        variant = 'candidate' if output.name == 'candidate-batch.log' else 'official'
                        self.assertEqual(pack, m.PACK + '-' + variant)
                        if variant == 'candidate':
                            self.assertEqual(len(json.loads((out / 'observations.json').read_text())['official']), 2)
                            if failure == 'compile': raise RuntimeError('injected compilation failure')
                        results = db / 'results' / pack
                        results.mkdir(parents=True)
                        for q in queries:
                            (results / (q.stem + '.bqrs')).write_bytes((variant + ' bqrs fixture').encode())
                            if variant == 'candidate' and failure == 'timeout':
                                raise TimeoutError('injected candidate timeout')
                        if variant == 'candidate' and failure == 'metadata':
                            (db / 'codeql-database.yml').write_text('---\nfinalised: true\nchanged: true\n')
                    elif argv[1:3] == ['bqrs', 'decode']:
                        b = Path(argv[3])
                        self.assertTrue(b.is_file())
                        if failure == 'decode' and output.name == 'candidate-CallsCandidate-decode.log':
                            raise RuntimeError('injected decode failure')
                        with output.open('w') as stream:
                            writer = csv.writer(stream); writer.writerow(['col0', 'col1', 'col2', 'col3'])
                            if b.parent.name == 'candidate' or b.parent.name.endswith('-candidate'):
                                context = 'candidate' if b.stem == 'CallsCandidate' else 'legacy'
                                writer.writerow([str(source / m.capacity.model.BASE / context / 'src/adapter.rs'), '42',
                                                 'with_existing_identity', provenance[context] + '/src/builder/phase/identity.rs'])
                    else:
                        self.fail('unexpected command: ' + repr(argv))
                argv = ['comparison', '--codeql', str(official / 'codeql'), '--source', str(source),
                        '--database', str(db), '--official-evidence', str(frozen), '--output', str(out),
                        '--work', str(work), '--deadline', str(deadline), '--mode', mode]
                for item in [patch.object(sys, 'argv', argv), patch.dict(os.environ, {'RUNNER_TEMP': str(runner), 'GH_TOKEN': 'hidden'}),
                        patch.object(m.runtime, 'run', side_effect=run), patch.object(m.capacity, 'resources', return_value=resources),
                        patch.object(m.capacity.bundle, 'bundle_closure', return_value=closure),
                        patch.object(m.capacity, 'after_identity', return_value={'source': 'UNCHANGED', 'official': 'UNCHANGED'}),
                        patch.object(m, 'OFFICIAL_SHA', hashlib.sha256(b'old').hexdigest()),
                        patch.object(m, 'CANDIDATE_SHA', hashlib.sha256(b'new').hexdigest())]:
                    stack.enter_context(item)
                self.assertEqual(m.main(), 1 if interrupt else 0)
                terminal = json.loads((out / 'comparison-terminal.json').read_text())
                self.assertEqual(terminal['status'], 'INCOMPLETE' if interrupt else 'COMPARISON_COMPLETED')
                self.assertEqual(terminal['official_terminal_status'], 'EXTRACTION_READY' if research else 'FAILED')
                self.assertEqual(terminal['official_public_semantics'], 'NOT_REQUESTED' if research else 'FAIL')
                self.assertEqual(terminal['production_capacity'], 'UNKNOWN')
                self.assertEqual(terminal['full_rust_coverage'], 'HOLD')
                self.assertEqual({p.name: p.read_bytes() for p in frozen.iterdir()}, before)
                self.assertEqual(type_file.read_text(), 'old')
                observations = json.loads((out / 'observations.json').read_text())
                self.assertEqual(len(observations['official']), 2)
                expected_candidate = (0 if failure in ('timeout', 'compile', 'decode') else 2) if research else (1 if interrupt else 2)
                self.assertEqual(len(observations['candidate']), expected_candidate)
                self.assertEqual(len([a for a in calls if a[1:3] == ['query', 'run']]), 0 if research else 4)
                self.assertEqual(len([a for a in calls if a[1:3] == ['database', 'run-queries']]), 2 if research else 0)
                if research:
                    self.assertEqual(len(list(out.glob('official-*.bqrs'))), 2)
                    self.assertEqual(len(list(out.glob('candidate-*.bqrs'))), 1 if failure == 'timeout' else 0 if failure == 'compile' else 2)
                    self.assertEqual({p.read_bytes() for p in out.glob('official-*.bqrs')}, {b'official bqrs fixture'})
                    if failure in ('timeout', 'compile'):
                        self.assertFalse(any('candidate-' in str(a[3]) for a in calls if a[1:3] == ['bqrs', 'decode']))
                    if failure == 'metadata': self.assertIn('metadata changed', terminal['integrity_error'])


class Workflow(unittest.TestCase):
    def test_default_closed_and_after_official_upload(self):
        text = (ROOT / '.github/workflows/codeql-rust-capacity.yml').read_text()
        declaration = text.split('      compare_qualifier_patch:\n', 1)[1].split('\n      mode:', 1)[0]
        self.assertIn('default: false', declaration)
        upload = text.index('      - id: official_artifact\n')
        compare = text.index('      - id: qualifier_comparison\n')
        self.assertLess(upload, compare)
        step = text[compare:].split('      - name: Upload private qualifier comparison', 1)[0]
        self.assertIn("inputs.mode == 'capacity'", step)
        self.assertIn('inputs.compare_qualifier_patch == true', step)
        self.assertIn("steps.official_artifact.outcome == 'success'", step)
        self.assertIn('if: always()', step)
        self.assertIn("inputs.mode == 'qualifier-research' || (inputs.mode == 'capacity' && inputs.compare_qualifier_patch == true)", step)
        self.assertIn("inputs.mode == 'qualifier-research' && 121 || 16", step)
        self.assertIn('--mode "$CAPACITY_MODE"', step)
        self.assertIn("skip-queries: ${{ inputs.mode == 'qualifier-research' || inputs.mode == 'candidate-workload-research' }}", text)
        self.assertNotIn('continue-on-error', text)
        self.assertIn('rust-qualifier-comparison-${{ github.run_id }}', text[compare:])
        self.assertIn('path: ${{ runner.temp }}/rust-qualifier-evidence', text[compare:])


if __name__ == '__main__':
    unittest.main()
