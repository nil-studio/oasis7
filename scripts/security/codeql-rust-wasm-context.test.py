#!/usr/bin/env python3
"""WASM context contracts; a real CLI fixture is explicitly opt-in."""
import base64
import copy
from contextlib import ExitStack, redirect_stdout
import datetime
import email.utils
import hashlib
import importlib.util
import io
import json
import math
import os
from pathlib import Path
import signal
import sys
import tarfile
import tempfile
import textwrap
import threading
import time
import types
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
import codeql_rust_candidate_profile as candidate_profile

ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / '.github/workflows/codeql-rust-wasm-context.yml'
TEXT = WORKFLOW.read_text()
BOOTSTRAP = textwrap.dedent(TEXT.split('          # BEGIN_WASM_CONTEXT_BOOTSTRAP\n', 1)[1]
                           .split('          # END_WASM_CONTEXT_BOOTSTRAP', 1)[0])
b = types.ModuleType('wasm_context_bootstrap_test')
exec(compile(BOOTSTRAP, str(WORKFLOW), 'exec'), b.__dict__)
spec = importlib.util.spec_from_file_location('wasm_context_test', Path(__file__).with_name('codeql-rust-wasm-context.py'))
d = importlib.util.module_from_spec(spec); spec.loader.exec_module(d)
w = d.witnesses


def clock_receipt(seconds=1800):
    mono, wall = time.monotonic(), time.time()
    return {'maximum_job_seconds': 1800, 'deadline_monotonic': mono + seconds,
            'deadline_wall': wall + seconds, 'prepare_end_monotonic': mono + seconds - 1020,
            'prepare_end_wall': wall + seconds - 1020}


def success_rows():
    rows = [[row[0], 1, 1, 3] if row[-1] else [row[0], 1, 0, 0] for row in w.MACROS]
    rows.extend([[row[0], 1, 1, 0] for row in w.CALLS]); rows.append(['wasm-role-cfg', 1, 1, 0])
    return {'#select': {'columns': [{'name': 'label', 'kind': 'String'}, {'name': 'syntax', 'kind': 'Int'},
                                   {'name': 'positive', 'kind': 'Int'}, {'name': 'negative', 'kind': 'Int'}], 'tuples': rows}}


class BootstrapTests(unittest.TestCase):
    def test_default_off_trusted_queue_and_artifact_boundary(self):
        self.assertIn('default: false', TEXT)
        self.assertIn("inputs.execute == true && github.ref == format('refs/heads/{0}', github.event.repository.default_branch)", TEXT)
        self.assertIn('timeout-minutes: 30', TEXT)
        self.assertIn('contents: read\n  actions: read', TEXT)
        self.assertIn('group: oasis7-codeql-scan-slot-1\n      queue: max\n      cancel-in-progress: false', TEXT)
        self.assertIn('path: ${{ runner.temp }}/rust-wasm-context/evidence', TEXT)
        self.assertEqual(set(b.HELPERS) | {'workflow.yml'}, set(d.HELPERS))
        self.assertIn('codeql_rust_candidate_profile.py', b.HELPERS)
        self.assertIn('codeql-rust-shared-constraint.patch', b.HELPERS)
        self.assertNotIn('codeql-rust-candidate-workload.py', b.HELPERS)
        self.assertNotIn('upload-sarif', TEXT)
        self.assertNotIn('security-events: write', TEXT)
        self.assertNotIn('database-upload', TEXT)
        self.assertEqual(b.SOURCE, d.SOURCE)

    def test_actual_job_clock_and_exact_tool_handoff(self):
        with tempfile.TemporaryDirectory() as temporary:
            environment = {'RUNNER_TEMP': temporary, 'GITHUB_SHA': '1' * 40, 'GITHUB_REPOSITORY': b.REPOSITORY,
                           'GITHUB_RUN_ID': '42', 'GITHUB_RUN_ATTEMPT': '1', 'GH_TOKEN': 'not-disclosed',
                           'GITHUB_EVENT_NAME': 'workflow_dispatch', 'GITHUB_REF': 'refs/heads/main',
                           'WASM_EXECUTE': 'true', 'WASM_TRUSTED_REF': 'refs/heads/main'}
            files = {f'scripts/security/{name}': (name + '\n').encode() for name in b.HELPERS}
            files[b.WORKFLOW] = b'name: fixture\n'
            blobs = {b.blob_id(raw): raw for raw in files.values()}
            tree = [{'path': name, 'type': 'blob', 'mode': '100644', 'sha': b.blob_id(raw)} for name, raw in files.items()]
            class API:
                def open(self, request, timeout):
                    url = request.full_url
                    if '/jobs?' in url:
                        started = datetime.datetime.now(datetime.timezone.utc) - datetime.timedelta(seconds=5)
                        value = {'jobs': [{'name': b.JOB_NAME, 'status': 'in_progress', 'head_sha': environment['GITHUB_SHA'],
                                          'run_id': 42, 'run_attempt': 1, 'id': 7, 'started_at': started.isoformat()}]}
                    elif '/git/commits/' in url: value = {'sha': environment['GITHUB_SHA'], 'tree': {'sha': 'a' * 40}}
                    elif '/git/trees/' in url: value = {'sha': 'a' * 40, 'truncated': False, 'tree': tree}
                    else:
                        oid = url.rsplit('/', 1)[1]; raw = blobs[oid]
                        value = {'sha': oid, 'encoding': 'base64', 'size': len(raw), 'content': base64.b64encode(raw).decode()}
                    response = io.BytesIO(json.dumps(value).encode()); response.status = 200
                    response.headers = {'Date': email.utils.formatdate(usegmt=True)}
                    return response
            calls = []
            def handoff(session, clock, identity, api, received):
                calls.append((session, clock, identity))
                self.assertEqual(clock['maximum_job_seconds'], 1800)
                self.assertAlmostEqual(clock['deadline_monotonic'] - clock['prepare_end_monotonic'], 1020)
                self.assertLess(clock['deadline_wall'] - time.time(), 1800)
                self.assertEqual(identity['tool_sha'], environment['GITHUB_SHA'])
                self.assertEqual(set(identity['files']), set(d.HELPERS))
                self.assertNotIn('not-disclosed', (session / 'evidence/bootstrap-http.json').read_text())
                return 0
            self.assertEqual(b.bootstrap(environment, opener=API(), handoff=handoff), 0)
            self.assertEqual(len(calls), 1)

    def test_clock_binding_fails_closed_for_missing_date_wrong_source_and_old_budget(self):
        api = types.SimpleNamespace(environment={'GITHUB_RUN_ID': '42', 'GITHUB_RUN_ATTEMPT': '1'},
                                    tool_sha='1' * 40, end=time.monotonic() + 20)
        api.get = lambda _kind: ({'jobs': []}, None)
        with self.assertRaises(b.Rejected): b.bind_clock(api)
        receipt = clock_receipt(); receipt['maximum_job_seconds'] = 1200
        with self.assertRaises(d.Rejected): d.Clock(receipt)
        for key in ('deadline_wall', 'deadline_monotonic'):
            receipt = clock_receipt(); receipt[key] = float('nan')
            with self.assertRaises(d.Rejected): d.Clock(receipt)


class ClockAdmissionTests(unittest.TestCase):
    """Real bootstrap HTTP decoding with controlled external responses and time."""
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(); self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.wall = datetime.datetime(2026, 11, 12, 16, 0, tzinfo=datetime.timezone.utc).timestamp()
        self.elapsed = 0.; self.requests = []; self.bodies = []
        self.environment = {'RUNNER_TEMP': str(self.root), 'GITHUB_SHA': '1' * 40,
            'GITHUB_REPOSITORY': b.REPOSITORY, 'GITHUB_RUN_ID': '42', 'GITHUB_RUN_ATTEMPT': '1',
            'GH_TOKEN': 'TOKEN_MUST_NOT_BE_WRITTEN', 'GITHUB_EVENT_NAME': 'workflow_dispatch',
            'GITHUB_REF': 'refs/heads/main', 'WASM_EXECUTE': 'true', 'WASM_TRUSTED_REF': 'refs/heads/main'}
        self.started = datetime.datetime.fromtimestamp(self.wall - 5, datetime.timezone.utc).isoformat()
        self.job = {'name': b.JOB_NAME, 'status': 'in_progress', 'id': 7, 'run_id': 42,
                    'run_attempt': 1, 'head_sha': '1' * 40, 'started_at': self.started}

    def timeline(self):
        stack = ExitStack()
        stack.enter_context(patch.object(b.time, 'monotonic', side_effect=lambda: 100 + self.elapsed))
        stack.enter_context(patch.object(b.time, 'time', side_effect=lambda: self.wall + self.elapsed))
        stack.enter_context(patch.object(b.time, 'sleep', side_effect=self.advance))
        return stack

    def advance(self, seconds): self.elapsed += seconds

    def opener(self, responses, *, latency=.25, date='automatic'):
        owner = self
        class HTTP:
            def open(self, request, timeout):
                index = len(owner.requests); owner.requests.append((request.full_url, timeout))
                owner.advance(latency)
                value = responses[min(index, len(responses) - 1)]
                raw = json.dumps(value).encode(); owner.bodies.append(raw)
                response = io.BytesIO(raw); response.status = 200
                response.headers = {'Date': email.utils.formatdate(owner.wall + owner.elapsed, usegmt=True)
                                    if date == 'automatic' else date}
                return response
        return HTTP()

    def test_queued_and_unpublished_start_bind_only_the_original_job_budget(self):
        queued = dict(self.job, status='queued', started_at=None)
        starting = dict(self.job, started_at=None)
        with self.timeline():
            api = b.API(self.environment, self.opener([{'jobs': [queued]}, {'jobs': [starting]}, {'jobs': [self.job]}]))
            clock = b.bind_clock(api)
        self.assertEqual(len(self.requests), 3)
        self.assertEqual(api.clock_admission['retry_count'], 2)
        self.assertEqual([o['job_status'] for o in api.clock_admission['observations']],
                         ['queued', 'in_progress', 'in_progress'])
        self.assertEqual(clock['started_at'], self.started)
        self.assertLessEqual(clock['deadline_wall'], self.wall - 5 + 1800)
        self.assertLessEqual(clock['prepare_end_wall'], self.wall - 5 + 780)
        self.assertEqual(clock['deadline_monotonic'] - clock['prepare_end_monotonic'], 1020)
        self.assertEqual([r['sha256'] for r in api.receipts], [hashlib.sha256(raw).hexdigest() for raw in self.bodies])
        self.assertTrue(all(r['http_status'] == 200 and r['body_observation'] == 'EOF_OBSERVED' for r in api.receipts))

    def test_retry_and_slow_success_share_one_twenty_second_deadline(self):
        queued = {'jobs': [dict(self.job, status='queued', started_at=None)]}
        for responses, latency in (([queued], .25), ([{'jobs': [self.job]}], 20.1)):
            with self.subTest(latency=latency):
                self.elapsed = 0.; self.requests = []; self.bodies = []
                with self.timeline():
                    api = b.API(self.environment, self.opener(responses, latency=latency))
                    with self.assertRaises(b.Rejected): b.bind_clock(api)
                self.assertEqual(api.clock_admission['error_code'], 'CLOCK_DEADLINE')
                self.assertLessEqual(self.elapsed, max(20., latency))
                self.assertEqual(api.end, 120.)
                self.assertLessEqual(len(self.requests), 20)
                self.assertTrue(all(timeout <= 20 - (index * (1 + latency))
                                    for index, (_, timeout) in enumerate(self.requests)))

    def test_retry_cannot_switch_to_another_job_id(self):
        responses = [{'jobs': [dict(self.job, status='queued', started_at=None)]},
                     {'jobs': [dict(self.job, id=8)]}]
        with self.timeline():
            api = b.API(self.environment, self.opener(responses))
            with self.assertRaises(b.Rejected): b.bind_clock(api)
        self.assertEqual(len(self.requests), 2)
        self.assertEqual(api.clock_admission['error_code'], 'CLOCK_IDENTITY')

    def test_invalid_identity_count_state_or_clock_never_retries(self):
        cases = [({'jobs': []}, 'automatic'),
                 ({'jobs': [dict(self.job, status='queued'), self.job]}, 'automatic')]
        for key, value in (('run_id', 99), ('run_attempt', 2), ('head_sha', '2' * 40), ('id', True),
                           ('name', 'another job'), ('status', 'completed'),
                           ('started_at', 'private invalid date'),
                           ('started_at', datetime.datetime.fromtimestamp(self.wall + 1, datetime.timezone.utc).isoformat()),
                           ('started_at', datetime.datetime.fromtimestamp(self.wall - 120, datetime.timezone.utc).isoformat())):
            cases.append(({'jobs': [dict(self.job, **{key: value})]}, 'automatic'))
        missing = dict(self.job); del missing['started_at']
        cases.extend([({'jobs': [missing]}, 'automatic'), ({'jobs': [self.job]}, None),
                      ({'jobs': [self.job]}, 'invalid date'),
                      ({'jobs': [self.job]}, 'Thu, 12 Nov 2026 16:00:00')])
        for value, date in cases:
            with self.subTest(value=value, date=date):
                self.elapsed = 0.; self.requests = []
                with self.timeline():
                    api = b.API(self.environment, self.opener([value], date=date))
                    with self.assertRaises(b.Rejected): b.bind_clock(api)
                self.assertEqual(len(self.requests), 1)
                self.assertEqual(api.clock_admission['retry_count'], 0)
                self.assertEqual(api.clock_admission['status'], 'REJECTED')
                self.assertNotEqual(api.clock_admission['error_code'], 'CLOCK_DEADLINE')

    def test_rejected_clock_first_writes_only_whitelisted_fields(self):
        secret = 'RAW_CLOCK_RESPONSE_MUST_NOT_BE_WRITTEN'
        for index, field in enumerate(('name', 'status', 'started_at', 'head_sha')):
            with self.subTest(field=field):
                root = self.root / str(index); root.mkdir()
                environment = dict(self.environment, RUNNER_TEMP=str(root))
                value = {'jobs': [dict(self.job, **{field: secret}, untrusted_message=secret)], 'message': secret}
                writes = []; original = b.save_bootstrap; stdout = io.StringIO()
                def save(out, name, value):
                    writes.append(json.dumps(value)); return original(out, name, value)
                with self.timeline(), patch.object(b, 'save_bootstrap', side_effect=save), redirect_stdout(stdout):
                    code = b.bootstrap(environment, opener=self.opener([value]), handoff=lambda *args: self.fail('unsafe handoff'))
                self.assertEqual(code, 1)
                for text in writes + [stdout.getvalue()]:
                    self.assertNotIn(secret, text); self.assertNotIn(environment['GH_TOKEN'], text)
                terminal = json.loads((root / 'rust-wasm-context/evidence/bootstrap-terminal.json').read_text())
                self.assertTrue(terminal['error_code'].startswith('CLOCK_'))
                self.assertEqual(terminal['clock_admission']['status'], 'REJECTED')
                self.assertFalse((root / 'rust-wasm-context/evidence/job-clock.json').exists())

    def test_clock_evidence_write_failure_never_hands_off_or_leaks_error_text(self):
        original = Path.write_bytes; stdout = io.StringIO()
        def write(path, data):
            if path.name == 'job-clock.json': raise OSError('RAW_WRITE_ERROR_MUST_NOT_BE_WRITTEN')
            return original(path, data)
        with self.timeline(), patch.object(Path, 'write_bytes', new=write), redirect_stdout(stdout):
            code = b.bootstrap(self.environment, opener=self.opener([{'jobs': [self.job]}]),
                               handoff=lambda *args: self.fail('unrecorded clock handoff'))
        self.assertEqual(code, 1)
        terminal = json.loads((self.root / 'rust-wasm-context/evidence/bootstrap-terminal.json').read_text())
        self.assertEqual(terminal['error_code'], 'BOOTSTRAP_EVIDENCE_WRITE')
        self.assertNotIn('RAW_WRITE_ERROR', stdout.getvalue())
        self.assertNotIn('RAW_WRITE_ERROR', json.dumps(terminal))


class BudgetTests(unittest.TestCase):
    def test_shared_window_never_resets_and_retains_cleanup_upload(self):
        clock = d.Clock(clock_receipt(1500)); end = clock.admit()
        self.assertLessEqual(end, clock.end - 120)
        self.assertAlmostEqual(end - time.monotonic(), 900, delta=.1)
        with self.assertRaises(d.Rejected): clock.admit()
        old_end = clock.end
        with patch.object(d.time, 'time', return_value=clock.wall + 1):
            with self.assertRaises(d.Rejected): clock.check()
        self.assertEqual(clock.end, old_end)

    def test_late_admission_does_not_gain_new_minutes(self):
        clock = d.Clock(clock_receipt(1019))
        with self.assertRaises(d.Rejected): clock.admit()
        self.assertIsNone(clock.analysis_end)


class WitnessTests(unittest.TestCase):
    def test_positive_negative_and_cfg_are_all_required(self):
        value = success_rows(); self.assertEqual(w.validate(value)['status'], 'PASS')
        for index in range(9):
            bad = copy.deepcopy(value); bad['#select']['tuples'][index][1] = 0
            self.assertEqual(w.validate(bad)['status'], 'INCOMPLETE')
        for index in (4, 5, 6, 7, 8):
            bad = copy.deepcopy(value); bad['#select']['tuples'][index][3] = 1
            self.assertEqual(w.validate(bad)['status'], 'INCOMPLETE')
        for index in (0, 1, 2, 3, 6, 7, 8):
            bad = copy.deepcopy(value); bad['#select']['tuples'][index][2] = 0
            self.assertEqual(w.validate(bad)['status'], 'INCOMPLETE')
        bad = copy.deepcopy(value); bad['#select']['tuples'][-1] = ['wasm-role-cfg', 2, 1, 0]
        self.assertEqual(w.validate(bad)['wasm_role_cfg'], 'INCOMPLETE')

    def test_missing_duplicate_unknown_or_malformed_rows_rejected(self):
        for mutation in ('missing', 'duplicate', 'unknown', 'negative', 'boolean', 'columns', 'table'):
            value = success_rows(); rows = value['#select']['tuples']
            if mutation == 'missing': rows.pop()
            elif mutation == 'duplicate': rows[-1] = rows[0]
            elif mutation == 'unknown': rows[0][0] = 'untrusted output'
            elif mutation == 'negative': rows[0][1] = -1
            elif mutation == 'boolean': rows[0][1] = True
            elif mutation == 'columns': value['#select']['columns'][0]['name'] = 'unexpected'
            else: value['another'] = {}
            with self.subTest(mutation=mutation), self.assertRaises(ValueError): w.validate(value)

    def test_public_ast_macro_and_static_anchors_stay_distinct(self):
        query = w.query(Path('/fixed-source'))
        self.assertEqual(query.count('import '), 1)
        for api in ('getAttributeMacroExpansion()', 'getADeriveMacroExpansion()', 'getStaticTarget()', 'getACfgOption()',
                    'getParentNode+()', 'isFromMacroExpansion()'):
            self.assertIn(api, query)
        self.assertIn('c.getLocation().getStartLine() = 65', query)
        self.assertIn('c.getLocation().getStartLine() = 110', query)
        self.assertNotIn('getTargetName()', query)
        self.assertNotIn('DataFlow', query)
        self.assertIn('exists(n.getName()) implies', query)

    def test_frozen_source_rejects_changes_and_symlinks(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); folder = root / w.DIRECTORY; folder.mkdir(parents=True)
            (folder / 'anchor.rs').write_bytes(b'fn original() {}\n')
            identity = {'anchor.rs': hashlib.sha256((folder / 'anchor.rs').read_bytes()).hexdigest()}
            with patch.object(w, 'SOURCE_FILES', identity):
                self.assertEqual(len(w.verify_source(root)), 1)
                (folder / 'anchor.rs').write_bytes(b'fn changed() {}\n')
                with self.assertRaises(ValueError): w.verify_source(root)
                (folder / 'anchor.rs').unlink(); (folder / 'anchor.rs').symlink_to('/etc/hosts')
                with self.assertRaises(ValueError): w.verify_source(root)


class ExtractionContracts(unittest.TestCase):
    def test_root_source_selection_and_exact_guarded_role_command(self):
        source, work = Path('/fixed/source'), Path('/fixed/work')
        args = d.extractor_arguments(source, work, work / 'locked-cargo-home/bin/cargo')
        self.assertEqual(args[1:5], ['check', '--locked', '--offline', '--manifest-path'])
        self.assertEqual(args[args.index('--features') + 1], 'oasis7_custody_worker/recovery-fence')
        self.assertEqual(args[args.index('--target') + 1], 'wasm32-unknown-unknown')
        self.assertEqual(args[args.index('--manifest-path') + 1], '/fixed/source/Cargo.toml')
        self.assertIn('--lib', args); self.assertIn('--no-default-features', args)
        self.assertNotIn('--workspace', args); self.assertNotIn('--all-features', args)
        index = d.index_arguments(work / 'database', source)
        self.assertIn('--working-dir=/fixed/source', index)
        self.assertEqual(sum(a.startswith('--include=') for a in index), 6)
        self.assertTrue(all('*.rs' in a for a in index if a.startswith('--include=')))

    def test_actual_cli_index_must_match_six_packages_not_a_requested_label(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); source = root / 'source'; source.mkdir()
            before = {'files': {}}
            for name in d.LOCAL_PACKAGES:
                path = 'crates/' + name + '/src/lib.rs'
                target = source / path; target.parent.mkdir(parents=True); target.write_text('')
                before['files'][path] = {'sha256': '1' * 64}
            expected = d.selected_files(source, before)
            db = root / 'database'; (db / 'working').mkdir(parents=True)
            selected = db / 'working/files-to-index123.list'
            selected.write_text(''.join(str(source / p) + '\n' for p in expected))
            self.assertEqual(d.observe_index(db, source, expected)['selected_count'], 6)
            selected.write_text(str(source / next(iter(expected))) + '\n')
            with self.assertRaises(d.Rejected): d.observe_index(db, source, expected)
            selected.write_text('/outside/source.rs\n')
            with self.assertRaises(d.Rejected): d.observe_index(db, source, expected)

    def test_guard_requires_fresh_extractor_calls_and_exact_build_script(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); out = root / 'evidence'; out.mkdir()
            audit = out / 'cargo-guard-calls'; audit.mkdir(); source = root / 'source'
            evidence = d.metadata.Evidence(out)
            args = d.extractor_arguments(source, root, root / 'guard/bin/cargo')
            base = {'status': 'PASS', 'exit': 0, 'locked': True, 'rustup_toolchain': '1.99.0',
                    'cwd_sha256': d.sha256(os.fsencode(source)), 'manifest_path_sha256': d.sha256(os.fsencode(source / 'Cargo.toml')),
                    'wasm_version_projection': {'status': 'VALIDATED_PROJECTED'}, 'actual_cargo_version': 'fixture'}
            pre = dict(base, operation='metadata', full_metadata=True)
            # UUID order is intentionally opposite chronological order.
            (audit / ('f' * 32 + '.json')).write_bytes(d.encoded(pre))
            preflight = {p.name: d.sha256(p.read_bytes()) for p in audit.glob('*.json')}
            verdict = {'status': 'PASS', 'calls': 1, 'full_metadata_calls': 1}
            with patch.object(d.guard, 'inspect', return_value=verdict), patch.object(d.metadata, 'parse_version', return_value={}):
                got = d.inspect_guard(evidence, 0, 0, source, preflight, args)
                self.assertEqual(got['status'], 'INCOMPLETE')
                (audit / ('1' * 32 + '.json')).write_bytes(d.encoded(pre))
                build = dict(base, operation='check', full_metadata=False, argv_sha256=d.sha256(json.dumps(args[1:]).encode()))
                check_path = audit / ('2' * 32 + '.json'); check_path.write_bytes(d.encoded(build))
                self.assertEqual(d.inspect_guard(evidence, 0, 0, source, preflight, args)['status'], 'PASS')
                build['argv_sha256'] = '0' * 64; check_path.write_bytes(d.encoded(build))
                self.assertEqual(d.inspect_guard(evidence, 0, 0, source, preflight, args)['status'], 'INCOMPLETE')
                (audit / next(iter(preflight))).write_text('{}')
                with self.assertRaises(d.Rejected): d.inspect_guard(evidence, 0, 0, source, preflight, args)


class OwnedProcessTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.root = Path(self.temp.name)
        self.out = self.root / 'evidence'; self.out.mkdir(); self.evidence = d.metadata.Evidence(self.out)
        self.clock = d.Clock(clock_receipt()); d.metadata.set_subreaper()

    def tearDown(self): self.temp.cleanup()

    def run_child(self, code, **kwargs):
        return d.run_owned('database-index', [sys.executable, '-c', code], self.root, dict(os.environ),
                           self.clock, time.monotonic() + 10, self.evidence, {'cwd': str(self.root)}, **kwargs)

    def test_success_and_failure_never_first_write_raw_streams_or_argv(self):
        secret = 'RAW_PRIVATE_OUTPUT_SENTINEL'
        raw, record = self.run_child('import sys;print("' + secret + '");print("stderr-secret",file=sys.stderr)', capture=True)
        self.assertIn(secret.encode(), raw)
        self.assertEqual(record['streams']['stdout']['observation'], 'EOF_COMPLETE')
        self.assertNotIn(secret, (self.out / 'database-index.result.json').read_text())
        self.assertNotIn('stderr-secret', (self.out / 'database-index.result.json').read_text())
        with self.assertRaises(d.CommandFailed) as error:
            self.run_child('import sys;print("' + secret + '");sys.exit(7)', capture=True)
        self.assertEqual(error.exception.receipt['exit'], 7)
        self.assertEqual(error.exception.receipt['failure_kind'], 'child_exit')
        self.assertNotIn(secret, (self.out / 'database-index.result.json').read_text())

    def test_deadline_reaps_real_process_group_and_reports_timeout(self):
        started = time.monotonic()
        code = 'import subprocess,sys,time;subprocess.Popen([sys.executable,"-c","import time;time.sleep(30)"]);time.sleep(30)'
        with self.assertRaises(d.CommandFailed) as error:
            d.run_owned('database-index', [sys.executable, '-c', code], self.root, dict(os.environ),
                        self.clock, time.monotonic() + 4.3, self.evidence, {'cwd': str(self.root)})
        receipt = error.exception.receipt
        self.assertEqual(receipt['failure_kind'], 'timeout')
        self.assertFalse(receipt['cleanup_errors'])
        with self.assertRaises(ProcessLookupError): os.killpg(receipt['pgid'], 0)
        self.assertLess(time.monotonic() - started, 5)

    def test_capture_overflow_fails_without_persisting_prefix(self):
        with patch.object(d, 'MAX_STDOUT', 4), self.assertRaises(d.CommandFailed) as error:
            self.run_child('print("private raw long output")', capture=True)
        self.assertEqual(error.exception.receipt['failure_kind'], 'output_limit')
        self.assertNotIn('private raw', (self.out / 'database-index.result.json').read_text())

    def test_term_cancellation_keeps_receipt_and_reaps_child(self):
        previous = signal.signal(signal.SIGTERM, d.interruption)
        timer = threading.Timer(.1, lambda: os.kill(os.getpid(), signal.SIGTERM))
        try:
            timer.start()
            with self.assertRaises(d.CommandFailed) as error:
                self.run_child('import time;time.sleep(30)')
            self.assertEqual(error.exception.receipt['failure_kind'], 'cancelled')
            self.assertFalse(error.exception.receipt['cleanup_errors'])
            with self.assertRaises(ProcessLookupError): os.killpg(error.exception.receipt['pgid'], 0)
        finally:
            timer.cancel(); signal.signal(signal.SIGTERM, previous)

    def test_existing_metadata_mode_is_unchanged(self):
        self.assertNotIn('database-index', d.metadata.PHASES)
        with self.assertRaises(d.metadata.Rejected):
            d.metadata.Clock(clock_receipt())
        self.assertEqual(d.metadata.CAP, 64 * 1024**2)


class WitnessProgressTests(unittest.TestCase):
    """Real pipe bytes and cleanup exercise the fixed-query first-write boundary."""
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(); self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name); out = self.root / 'evidence'; out.mkdir()
        self.secret = 'UNAPPROVED_PROGRESS_RAW_CANARY'
        query = self.root / self.secret / (w.NAME + '.ql'); query.parent.mkdir()
        query.write_bytes(b'select "UNAPPROVED_QUERY_TEXT_CANARY"\n')
        self.query = query; self.digest = hashlib.sha256(query.read_bytes()).hexdigest()
        self.evidence = d.metadata.Evidence(out); self.clock = d.Clock(clock_receipt())
        d.metadata.set_subreaper()
        self.start = ('Compiling query plan for ' + str(query) + '.\n').encode()
        self.compiled = ('[1/1 comp 0.01s] Compiled ' + w.PACK + '/' + w.NAME + '.ql.\n').encode()
        self.evaluate = ('Starting evaluation of ' + w.PACK + '/' + w.NAME + '.ql.\n').encode()
        self.done = ('[1/1 eval 0.02s] Evaluation done; writing results to ' + w.PACK + '/' + w.NAME + '.bqrs.\n').encode()

    def run_query(self, program, *, end=None):
        writes = []; original = Path.write_bytes; output = io.StringIO()
        def save(path, data):
            if path.is_relative_to(self.evidence.root): writes.append(data)
            return original(path, data)
        try:
            with patch.object(Path, 'write_bytes', new=save), redirect_stdout(output):
                return d.run_owned('witness-query', [sys.executable, '-c', 'import os,signal,sys,time\n' + program, str(self.query)],
                    self.root, dict(os.environ), self.clock, end or time.monotonic() + 10,
                    self.evidence, {'cwd': str(self.root)})[1]
        finally:
            first_writes = b''.join(writes) + output.getvalue().encode()
            self.assertNotIn(self.secret.encode(), first_writes)
            self.assertNotIn(b'UNAPPROVED_QUERY_TEXT_CANARY', first_writes)

    def assert_streams(self, receipt, stdout, stderr):
        for name, raw in (('stdout', stdout), ('stderr', stderr)):
            actual = receipt['streams'][name]; progress = receipt['query_progress']['streams'][name]
            self.assertEqual(actual['bytes'], len(raw)); self.assertEqual(actual['sha256'], hashlib.sha256(raw).hexdigest())
            self.assertEqual(actual['observation'], 'EOF_COMPLETE')
            self.assertEqual(progress['bytes'], actual['bytes']); self.assertEqual(progress['sha256'], actual['sha256'])
            self.assertTrue(progress['final'])

    def test_chunked_streams_project_only_frozen_query_events_and_bounded_times(self):
        stdout = self.start + self.secret.encode() + b'\nCompiling query plan for /untrusted/Other.ql.\n'
        stderr = self.compiled + self.evaluate + self.done
        original_read = os.read
        with patch.object(d.os, 'read', side_effect=lambda fd, size: original_read(fd, min(size, 11))):
            receipt = self.run_query(f'os.write(1, {stdout!r})\nos.write(2, {stderr!r})\n')
        progress = receipt['query_progress']
        self.assertEqual(progress['status'], 'OBSERVED'); self.assertEqual(progress['query_sha256'], self.digest)
        self.assertEqual(progress['events'], {event: 1 for event in ('COMPILE_START', 'COMPILE_DONE', 'EVALUATE_START', 'EVALUATE_DONE')})
        self.assertEqual(set(progress['timing']), set(progress['events']))
        for timing in progress['timing'].values():
            self.assertEqual(len(timing), 4); self.assertTrue(all(math.isfinite(value) for value in timing.values()))
            self.assertGreaterEqual(timing['first_received_monotonic'], receipt['started_monotonic'])
            self.assertGreaterEqual(timing['last_received_monotonic'], timing['first_received_monotonic'])
        self.assertEqual(progress['streams']['stdout']['rejected'], 2)
        self.assert_streams(receipt, stdout, stderr)

    def test_unterminated_tail_is_hashed_but_never_promoted_to_an_event(self):
        stdout = self.start + self.compiled.rstrip(b'\n')
        receipt = self.run_query(f'os.write(1, {stdout!r})\n')
        progress = receipt['query_progress']
        self.assertEqual(progress['events']['COMPILE_START'], 1)
        self.assertEqual(progress['events']['COMPILE_DONE'], 0)
        self.assertNotIn('COMPILE_DONE', progress['timing'])
        self.assertEqual(progress['streams']['stdout']['processed_prefix_bytes'], len(self.start))
        self.assertEqual(progress['streams']['stdout']['rejected'], 1)
        self.assert_streams(receipt, stdout, b'')

    def test_timeout_preserves_original_failure_and_projects_term_handler_tail(self):
        program = (f'def stop(*_):\n os.write(2, {self.evaluate!r})\n sys.exit(0)\n'
                   f'signal.signal(signal.SIGTERM, stop)\nos.write(1, {self.start!r})\ntime.sleep(20)\n')
        started = time.monotonic()
        with self.assertRaises(d.CommandFailed) as failed:
            self.run_query(program, end=started + 4.4)
        receipt = failed.exception.receipt
        self.assertEqual(receipt['failure_kind'], 'timeout'); self.assertEqual(receipt['cleanup_errors'], [])
        self.assertEqual(receipt['query_progress']['events']['EVALUATE_START'], 1)
        self.assertEqual(receipt['query_progress']['events']['EVALUATE_DONE'], 0)
        self.assert_streams(receipt, self.start, self.evaluate)
        self.assertLess(time.monotonic() - started, 5)
        with self.assertRaises(ProcessLookupError): os.killpg(receipt['pgid'], 0)

    def test_nonzero_exit_keeps_progress_and_original_failure(self):
        stderr = (self.secret + '\n').encode()
        with self.assertRaises(d.CommandFailed) as failed:
            self.run_query(f'os.write(1, {self.start!r})\nos.write(2, {stderr!r})\nsys.exit(7)\n')
        receipt = failed.exception.receipt
        self.assertEqual(receipt['failure_kind'], 'child_exit'); self.assertEqual(receipt['exit'], 7)
        self.assertEqual(receipt['query_progress']['events']['COMPILE_START'], 1)
        self.assert_streams(receipt, self.start, stderr)

    def test_projection_failure_is_separate_and_never_makes_a_command_successful(self):
        for exit_code in (0, 7):
            with self.subTest(exit_code=exit_code), patch.object(candidate_profile.Progress, 'line', side_effect=ValueError(self.secret)):
                with self.assertRaises(d.CommandFailed) as failed:
                    self.run_query(f'os.write(1, {self.start!r})\nsys.exit({exit_code})\n')
                receipt = failed.exception.receipt
                self.assertEqual(receipt['failure_kind'], 'child_exit' if exit_code else 'progress_observation')
                self.assertEqual(receipt['exit'], exit_code)
                self.assertEqual(receipt['query_progress']['status'], 'INCOMPLETE')
                self.assertEqual(receipt['query_progress']['error_code'], 'PROGRESS_REJECTED')
                self.assert_streams(receipt, self.start, b'')

    def test_cleanup_output_and_receipt_limits_preserve_the_primary_failure(self):
        tail = (self.secret.encode() * 100)[:1025]
        program = (f'def stop(*_):\n os.write(2, {tail!r})\n sys.exit(0)\n'
                   f'signal.signal(signal.SIGTERM, stop)\nos.write(1, {self.start!r})\ntime.sleep(20)\n')
        with self.subTest(limit='cleanup-output'), patch.object(d, 'MAX_STREAM', 1024):
            with self.assertRaises(d.CommandFailed) as failed:
                self.run_query(program, end=time.monotonic() + 4.4)
            self.assertEqual(failed.exception.receipt['failure_kind'], 'timeout')
            self.assertEqual(failed.exception.receipt['streams']['stderr']['observation'], 'OBSERVED_PREFIX_ONLY')
            self.assertFalse(failed.exception.receipt['query_progress']['streams']['stderr']['final'])
        oversized = {'fixture': 'x' * d.MAX_PROCESS_RECEIPT}
        with self.subTest(limit='receipt'), patch.object(d.runtime.GroupSamples, 'result', return_value=oversized):
            with self.assertRaises(d.CommandFailed) as failed:
                self.run_query(f'os.write(1, {self.start!r})\nsys.exit(7)\n')
            self.assertEqual(failed.exception.receipt['failure_kind'], 'child_exit')
            self.assertEqual(failed.exception.receipt['resource_observation']['status'], 'UNAVAILABLE_RECEIPT_LIMIT')

    def test_cleanup_cancellation_keeps_the_existing_exit_in_memory_and_on_disk(self):
        previous = signal.signal(signal.SIGTERM, d.interruption); cleanup = d.cleanup_group
        def interrupt_after_cleanup(proc):
            result = cleanup(proc); os.kill(os.getpid(), signal.SIGTERM); return result
        try:
            with patch.object(d, 'cleanup_group', side_effect=interrupt_after_cleanup):
                with self.assertRaises(d.CommandFailed) as failed:
                    self.run_query(f'os.write(1, {self.start!r})\nsys.exit(7)\n')
            receipt = failed.exception.receipt
            saved = json.loads((self.evidence.root / 'witness-query.result.json').read_text())
            self.assertEqual(receipt['failure_kind'], 'child_exit'); self.assertEqual(receipt['exit'], 7)
            self.assertEqual(saved['failure_kind'], receipt['failure_kind'])
        finally:
            signal.signal(signal.SIGTERM, previous)


@unittest.skipUnless(os.environ.get('OASIS7_CODEQL_WASM_SHARED_CLI'), 'explicit pinned CodeQL resolver input required')
class RealSharedResolverTests(unittest.TestCase):
    def test_fixed_shared_patch_and_actual_private_or_stock_library_resolution(self):
        """Apply and resolve only: no database, compilation or query evaluation."""
        codeql = Path(os.environ['OASIS7_CODEQL_WASM_SHARED_CLI']).resolve()
        with tempfile.TemporaryDirectory() as temporary:
            work = Path(temporary).resolve(); out = work / 'evidence'; out.mkdir()
            evidence = d.metadata.Evidence(out)
            clock = d.Clock(clock_receipt(seconds=1140))  # At most 120 seconds left for this fixture.
            env = {key: value for key, value in os.environ.items() if key not in ('GH_TOKEN', 'GITHUB_TOKEN')}
            calls = []
            def command(phase, argv, cwd, end, *, capture=False, timeout=None):
                calls.append(phase)
                return d.run_owned(phase, list(map(str, argv)), cwd, env, clock, end, evidence,
                                   {'cwd': str(cwd)}, capture=capture, timeout=timeout)[0]
            bundle_before = d.tree_identity(codeql.parent, clock, clock.prepare)
            base = ['--log-to-stderr', '--loglevel=ERROR', '--common-caches=' + str(work / 'common-cache')]
            raw = command('codeql-version', [codeql, 'version', '--format=json', *base], work, clock.prepare,
                          capture=True, timeout=20)
            self.assertEqual(d.parse_json(raw)['version'], '2.27.4')
            official = d.model.bundle_closure(codeql.parent, '2.27.4')
            self.assertEqual(len(official), 13)
            selected, identity = d.prepare_shared_helper(official, work, clock, evidence, command)
            self.assertEqual([name for name in official if selected[name] != official[name]], ['codeql/typeinference'])
            self.assertEqual(identity['changed_files'], [d.model.SHARED_FILE])
            self.assertEqual(identity['official_files'][d.model.SHARED_FILE]['sha256'], d.model.SHARED_OFFICIAL_SHA)
            self.assertEqual(identity['candidate_files'][d.model.SHARED_FILE]['sha256'], d.model.SHARED_CANDIDATE_SHA)
            source = Path('/home/runner/work/_temp/rust-wasm-context/work/source')
            query = d.prepare_query(source, work, selected)
            query_path = query / (w.NAME + '.ql')
            self.assertEqual(d.sha256(query_path.read_bytes()), '8de9207bb99817e37a8354f2330e1e94c958ae434275d63368a296d96e10c6af')
            query_before = d.tree_identity(query, clock, clock.prepare)
            for phase, closure in (('query-library', selected), ('query-library-after', official)):
                raw = command(phase, [codeql, 'resolve', 'library-path', '--query=' + str(query_path), '--format=json',
                              *d.query_options(work, closure), '--log-to-stderr', '--loglevel=ERROR'],
                              work, clock.prepare, capture=True, timeout=20)
                actual = d.parse_json(raw)
                if closure is selected:
                    d.model.verify_library_path(actual, query, w.NAME, selected, w.PACK)
                    self.assertEqual(actual['compilationCache'], [str(work / 'compilation-cache')])
                    self.assertIn(str(work / 'candidate-typeinference'), actual['libraryPath'])
                    self.assertNotIn(official['codeql/typeinference']['root'], actual['libraryPath'])
                else:
                    self.assertIn(official['codeql/typeinference']['root'], actual['libraryPath'])
                    with self.assertRaises(RuntimeError):
                        d.model.verify_library_path(actual, query, w.NAME, selected, w.PACK)
            self.assertEqual(d.shared_helper_identity(Path(official['codeql/typeinference']['root']),
                work / 'candidate-typeinference', clock, clock.prepare), identity)
            self.assertEqual(d.tree_identity(query, clock, clock.prepare), query_before)
            self.assertEqual(d.tree_identity(codeql.parent, clock, clock.prepare), bundle_before)
            self.assertEqual(calls, ['codeql-version', 'apply-shared-helper-patch', 'query-library', 'query-library-after'])


@unittest.skipUnless(os.environ.get('OASIS7_CODEQL_WASM_PROGRESS_CLI') and os.environ.get('OASIS7_CODEQL_WASM_PROGRESS_RUST_BIN'),
                     'explicit pinned CodeQL/Rust integration inputs required')
class RealProgressTests(unittest.TestCase):
    def test_fixed_cli_flags_and_four_events_through_the_real_supervisor(self):
        """A single-file Rust fixture proves CLI event format, not WASM semantics."""
        codeql = Path(os.environ['OASIS7_CODEQL_WASM_PROGRESS_CLI']).resolve()
        rust_bin = Path(os.environ['OASIS7_CODEQL_WASM_PROGRESS_RUST_BIN']).resolve()
        with tempfile.TemporaryDirectory() as temporary:
            work = Path(temporary).resolve(); source = work / 'source'; source.mkdir()
            (source / 'lib.rs').write_text('pub fn fixture() {}\n')
            (source / 'Cargo.toml').write_text('[package]\nname="progress_fixture"\nversion="0.0.0"\nedition="2021"\n[lib]\npath="lib.rs"\n')
            output = Path(os.environ.get('OASIS7_CODEQL_WASM_PROGRESS_EVIDENCE', str(work / 'evidence'))).resolve()
            output.mkdir(parents=True, exist_ok=True); evidence = d.metadata.Evidence(output)
            clock = d.Clock(clock_receipt()); end = time.monotonic() + 120
            env = {k: v for k, v in os.environ.items() if k not in ('GH_TOKEN', 'GITHUB_TOKEN', 'RUSTC_WRAPPER',
                                                                 'RUSTC_WORKSPACE_WRAPPER', 'RUSTUP_TOOLCHAIN')}
            env.update(PATH=str(rust_bin) + os.pathsep + env['PATH'], RUSTC=str(rust_bin / 'rustc'),
                       CARGO_HOME=str(work / 'cargo-home'), CARGO_TARGET_DIR=str(work / 'target'), CARGO_NET_OFFLINE='true')
            base = ['--log-to-stderr', '--loglevel=ERROR', '--common-caches=' + str(work / 'common-cache')]
            def run(phase, argv, capture=False):
                return d.run_owned(phase, [str(codeql), *map(str, argv)], source, env, clock, end,
                                   evidence, {'cwd': str(source)}, capture=capture)
            raw, _ = run('codeql-version', ['version', '--format=json', *base], capture=True)
            self.assertEqual(json.loads(raw)['version'], d.VERSION)
            for kind in ('cargo', 'rustc'):
                raw, _ = d.run_owned(kind + '-version', [str(rust_bin / kind), '-Vv'], source, env, clock, end,
                                     evidence, {'cwd': str(source)}, capture=True)
                self.assertEqual(d.metadata.parse_version(raw, kind)['release'], '1.99.0')
            database = work / 'database'
            run('database-init', ['database', 'init', '--language=rust', '--build-mode=none',
                                 '--source-root=' + str(source), *base, '--', database])
            run('database-index', ['database', 'index-files', '--language=rust', '--threads=2', '--ram=3072',
                '--working-dir=' + str(source), '--no-follow-symlinks', '--include=lib.rs', *base, '--', database])
            run('database-finalize', ['database', 'finalize', '--threads=2', '--ram=3072', *base, '--', database])
            closure = d.model.bundle_closure(codeql.parent, d.VERSION)
            query = d.prepare_query(source, work, closure) / (w.NAME + '.ql')
            query.write_text('select "progress-fixture"\n')
            _, receipt = run('witness-query', ['database', 'run-queries', '--no-use-precompiled', '--threads=2', '--ram=3072',
                *d.query_options(work, closure), '--loglevel=ERROR', '--verbosity=progress', '--', database, query])
            progress = receipt['query_progress']
            self.assertEqual(progress['status'], 'OBSERVED')
            self.assertEqual(progress['events'], {event: 1 for event in ('COMPILE_START', 'COMPILE_DONE', 'EVALUATE_START', 'EVALUATE_DONE')})
            result = database / 'results' / w.PACK / (w.NAME + '.bqrs')
            raw, _ = run('witness-decode', ['bqrs', 'decode', result, '--format=json', *base], capture=True)
            self.assertEqual(json.loads(raw)['#select']['tuples'], [['progress-fixture']])
            evidence.save('smoke-summary.json', {'status': 'PASS', 'codeql_version': d.VERSION, 'query_invocations': 1,
                'flags': ['--loglevel=ERROR', '--verbosity=progress'],
                'events': progress['events'], 'query_sha256': progress['query_sha256'], 'decoded_rows': 1,
                'scope': 'Actual CLI events on a single-file Rust fixture DB; not WASM semantics.'})


class FullFlowTests(unittest.TestCase):
    """Small independent fake external tools; real budgets/guard/seal/final transitions."""
    def flow(self, fault=None):
        temporary = tempfile.TemporaryDirectory(); self.addCleanup(temporary.cleanup)
        root = Path(temporary.name); session = root / 'session'; session.mkdir()
        (session / 'evidence').mkdir(); tool_dir = session / 'tools'; tool_dir.mkdir()
        identity = {'tool_sha': '1' * 40, 'files': {}}
        for name in d.HELPERS:
            src = WORKFLOW if name == 'workflow.yml' else Path(__file__).with_name(name)
            raw = src.read_bytes(); (tool_dir / name).write_bytes(raw)
            identity['files'][name] = {'path': str(src.relative_to(ROOT)), 'bytes': len(raw),
                                     'git_blob': d.metadata.git_blob(raw), 'sha256': d.sha256(raw)}
        host = root / 'host'; host.mkdir()
        for name in ('cargo', 'rustup'):
            p = host / name; p.write_text('#!/bin/sh\nexit 0\n'); p.chmod(0o700)
        environment = {'PATH': str(host), 'GITHUB_SHA': identity['tool_sha']}
        members = ['crates/' + name for name in d.LOCAL_PACKAGES] + ['crates/member' + str(i) for i in range(47)]
        source_files = {'Cargo.toml': ('[workspace]\nmembers = ' + json.dumps(members) + '\n').encode(),
                        'Cargo.lock': b'version = 4\n[[package]]\nname = "fixture"\nversion = "0.0.0"\n'}
        for member in members:
            name = member.split('/')[-1]
            source_files[member + '/Cargo.toml'] = ('[package]\nname = "' + name + '"\nversion = "0.0.0"\n').encode()
            source_files[member + '/src/lib.rs'] = b'pub fn fixture() {}\n'
        tree = {p: {'path': p, 'type': 'blob', 'mode': '100644', 'sha': d.metadata.git_blob(raw), 'size': len(raw)}
                for p, raw in source_files.items()}
        archive = io.BytesIO()
        with tarfile.open(fileobj=archive, mode='w:gz') as stream:
            folder = tarfile.TarInfo('fixture'); folder.type = tarfile.DIRTYPE; stream.addfile(folder)
            for name, raw in source_files.items():
                info = tarfile.TarInfo('fixture/' + name); info.size = len(raw); info.mode = 0o644
                stream.addfile(info, io.BytesIO(raw))
        api = types.SimpleNamespace(get=lambda *args: ({}, None), archive=lambda: archive.getvalue(), receipts=[])
        clock = clock_receipt(); clock['tool_sha'] = identity['tool_sha']
        closure = {}
        for i, name in enumerate(['rust-all', 'typeinference', *['library' + str(n) for n in range(11)]]):
            version = '0.2.25' if name == 'rust-all' else '0.0.42' if name == 'typeinference' else '1.0.0'
            closure['codeql/' + name] = {'name': 'codeql/' + name, 'version': version,
                'root': str(session / 'work/codeql/qlpacks' / str(i)), 'dependencies': {}}
        shared = Path(closure['codeql/typeinference']['root'])
        shared_file = d.model.SHARED_FILE
        stock_shared, private_shared = b'official shared fixture\n', b'private shared fixture\n'
        commit = d.metadata.CARGO_COMMIT
        versions = {'cargo': ('cargo 1.99.0 (' + commit[:9] + ' 2026-09-01)\nrelease: 1.99.0\ncommit-hash: ' + commit +
                             '\ncommit-date: 2026-09-01\nhost: x86_64-unknown-linux-gnu\n').encode(),
                    'rustc': b'rustc 1.99.0 (123456789 2026-09-01)\nbinary: rustc\nrelease: 1.99.0\ncommit-hash: 1234567890123456789012345678901234567890\ncommit-date: 2026-09-01\nhost: x86_64-unknown-linux-gnu\n'}
        calls = []
        def record(operation, argv, source, out):
            config = (out / 'cargo-guard.json').read_bytes(); audit = out / 'cargo-guard-calls'
            count = int((audit / '.counter').read_text()) + 1
            (audit / '.counter').write_text(f'{count:016d}')
            value = {'status': 'PASS', 'exit': 0, 'locked': True, 'operation': operation, 'full_metadata': operation == 'metadata',
                     'argv_sha256': d.sha256(json.dumps(argv).encode()), 'cwd_sha256': d.sha256(os.fsencode(source)),
                     'manifest_path_sha256': d.sha256(os.fsencode(source / 'Cargo.toml')), 'rustup_toolchain': '1.99.0',
                     'config_sha256': d.sha256(config), 'actual_cargo_version': versions['cargo'].decode(),
                     'wasm_version_projection': {'status': 'VALIDATED_PROJECTED'}}
            (audit / (f'{count:032x}' + '.json')).write_bytes(d.encoded(value))
        def external(phase, argv, cwd, env, active_clock, end, evidence, semantic, **kwargs):
            calls.append((phase, end, cwd)); active_clock.check(end)
            result = {'phase': phase, 'exit': 0, 'failure_kind': None, 'cleanup_errors': []}
            evidence.save(phase + '.result.json', result)
            work = session / 'work'; source = work / 'source'; database = work / 'database'
            raw = b''
            if phase == 'bundle-download': (work / 'bundle.tar.zst').write_bytes(b'fixture bundle')
            elif phase == 'bundle-unpack':
                (work / 'codeql').mkdir(); (work / 'codeql/codeql').write_bytes(b'fixture CLI')
                for entry in closure.values():
                    folder = Path(entry['root']); folder.mkdir(parents=True)
                    manifest = folder / 'qlpack.yml'
                    manifest.write_text('name: ' + entry['name'] + '\nversion: ' + entry['version'] + '\nlibrary: true\ndependencies: {}\n')
                    entry['manifest_sha256'] = d.runtime.digest(manifest)
                (shared / shared_file).parent.mkdir(parents=True)
                (shared / shared_file).write_bytes(stock_shared)
            elif phase == 'codeql-version': raw = b'{"version":"2.27.4"}'
            elif phase == 'apply-shared-helper-patch':
                self.assertEqual(argv, ['patch', '--batch', '--fuzz=0', str(work / 'candidate-typeinference' / shared_file),
                                       str(Path(d.__file__).with_name('codeql-rust-shared-constraint.patch'))])
                self.assertEqual(end, active_clock.prepare); self.assertEqual(kwargs['timeout'], 20)
                if fault == 'shared-unprepared':
                    result.update(exit=1, failure_kind='exit')
                    evidence.save(phase + '.result.json', result)
                    raise d.CommandFailed(result)
                (work / 'candidate-typeinference' / shared_file).write_bytes(private_shared)
            elif phase.startswith('query-library'):
                actual = dict(closure)
                if fault != 'shared-fallback':
                    actual['codeql/typeinference'] = dict(actual['codeql/typeinference'], root=str(work / 'candidate-typeinference'))
                raw = d.encoded({'libraryPath': [str(work / 'query')] + [p['root'] for p in actual.values()],
                                 'dbscheme': closure['codeql/rust-all']['root'] + '/rust.dbscheme',
                                 'compilationCache': [str(work / 'compilation-cache')], 'qlPackName': w.PACK,
                                 'relativeName': w.PACK + '/' + w.NAME + '.ql'})
            elif phase == 'toolchain-install':
                base = work / 'rustup/toolchains' / d.TOOLCHAIN
                for path in ('bin/cargo', 'bin/rustc', 'lib/rustlib/multirust-channel-manifest.toml',
                             'lib/rustlib/src/rust/library/Cargo.toml', 'lib/rustlib/' + d.TARGET + '/lib/libstd-fixture.rlib'):
                    target = base / path; target.parent.mkdir(parents=True, exist_ok=True); target.write_bytes(b'fixed tool')
            elif phase.startswith('cargo-version'): raw = versions['cargo']
            elif phase.startswith('rustc-version'): raw = versions['rustc']
            elif phase == 'metadata': record('metadata', argv[1:], source, evidence.root); raw = b'{}'
            elif phase == 'database-init':
                self.assertIn('--build-mode=none', argv)
                (database / 'working').mkdir(parents=True)
            elif phase == 'database-index':
                paths = sorted(source / (member + '/src/lib.rs') for member in members[:6])
                (database / 'working/files-to-index.list').write_text(''.join(str(p) + '\n' for p in paths))
                if fault != 'preflight-only':
                    record('metadata', ['metadata', '--locked'], source, evidence.root)
                    record('check', json.loads(env['CODEQL_EXTRACTOR_RUST_OPTION_BUILD_SCRIPT_COMMAND'])[1:], source, evidence.root)
            elif phase == 'database-finalize': (database / 'codeql-database.yml').write_text('name: fixture\nfinalised: true\n')
            elif phase == 'database-identity':
                raw = d.encoded({'sourceLocationPrefix': str(source), 'languages': ['rust'], 'datasetFolder': str(database / 'db-rust')})
                if fault == 'shared-before-query': (work / 'candidate-typeinference' / shared_file).write_bytes(stock_shared)
            elif phase == 'witness-query':
                self.assertIn('--verbosity=progress', argv)
                self.assertIn('--loglevel=ERROR', argv)
                self.assertNotIn('--log-to-stderr', argv)
                target = database / 'results' / w.PACK / (w.NAME + '.bqrs'); target.parent.mkdir(parents=True); target.write_bytes(b'BQRS fixture')
                if fault == 'source-mutated': (source / 'Cargo.lock').write_bytes(b'changed lock')
                if fault in ('query-timeout', 'shared-mutated-timeout'):
                    if fault == 'shared-mutated-timeout': (work / 'candidate-typeinference' / shared_file).write_bytes(stock_shared)
                    result.update(exit=143, failure_kind='timeout')
                    evidence.save(phase + '.result.json', result)
                    raise d.CommandFailed(result)
            elif phase == 'witness-decode':
                value = success_rows()
                if fault == 'missing-macro': value['#select']['tuples'][0][2] = 0
                raw = d.encoded(value)
            return raw, result
        saved_handlers = {sig: signal.getsignal(sig) for sig in (signal.SIGALRM, signal.SIGTERM, signal.SIGINT)}
        try:
            with ExitStack() as stack:
                stack.enter_context(patch.object(d.metadata, 'verify_git_tree', return_value=tree))
                stack.enter_context(patch.object(d.witnesses, 'verify_source', return_value={'fixture': True}))
                stack.enter_context(patch.object(d.model, 'bundle_closure', return_value=closure))
                stack.enter_context(patch.object(d.metadata, 'metadata_graph', return_value=({'packages': []}, {'status': 'ROLE_CONTEXT_OBSERVED'})))
                stack.enter_context(patch.object(d, 'BUNDLE_SHA', d.sha256(b'fixture bundle')))
                stack.enter_context(patch.object(d.model, 'SHARED_OFFICIAL_SHA', d.sha256(stock_shared)))
                stack.enter_context(patch.object(d.model, 'SHARED_CANDIDATE_SHA', d.sha256(private_shared)))
                stack.enter_context(patch.object(d, 'run_owned', side_effect=external))
                if fault == 'seal-failure':
                    stack.enter_context(patch.object(d.metadata.Evidence, 'seal', side_effect=OSError('private exception sentinel')))
                stack.enter_context(redirect_stdout(io.StringIO()))
                code = d.diagnose(session, clock, identity, api, environment)
        finally:
            signal.setitimer(signal.ITIMER_REAL, 0)
            for sig, handler in saved_handlers.items(): signal.signal(sig, handler)
        terminal = json.loads((session / 'evidence/terminal.json').read_text())
        return code, terminal, calls, session

    def test_full_flow_seals_only_evidence_and_uses_one_analysis_deadline(self):
        code, terminal, calls, session = self.flow()
        self.assertEqual(code, 0)
        self.assertEqual(terminal['status'], 'WASM_CONTEXT_SEMANTICS_OBSERVED')
        self.assertEqual(terminal['identity'], 'PASS'); self.assertEqual(terminal['guard'], 'PASS')
        self.assertEqual(terminal['library_variant'], 'wasm-shared-helper-v1')
        out = session / 'evidence'
        official = json.loads((out / 'official-library-closure.json').read_text())
        selected = json.loads((out / 'library-closure.json').read_text())
        self.assertEqual([name for name in official if selected[name] != official[name]], ['codeql/typeinference'])
        self.assertEqual(selected['codeql/rust-all'], official['codeql/rust-all'])
        shared_identity = json.loads((out / 'shared-helper-identity.json').read_text())
        self.assertEqual(shared_identity['variant'], terminal['library_variant'])
        self.assertEqual(shared_identity['changed_files'], [d.model.SHARED_FILE])
        self.assertEqual(json.loads((out / 'helpers-after.json').read_text())['shared']['variant'], terminal['library_variant'])
        analysis = [end for phase, end, _ in calls if phase in ('metadata', 'database-init', 'database-index',
                    'database-finalize', 'database-identity', 'witness-query', 'witness-decode', 'query-library-after')]
        self.assertEqual(len(set(analysis)), 1)
        self.assertEqual(analysis[0], terminal['analysis_deadline_monotonic'] - 60)
        for phase, _, cwd in calls:
            if phase.startswith('database-') or phase.startswith('witness-'):
                self.assertEqual(cwd, session / 'work/source')
        self.assertEqual(terminal['production_capacity'], 'UNKNOWN'); self.assertEqual(terminal['full_rust_coverage'], 'HOLD')
        seal = json.loads((session / 'evidence/sha256.json').read_text())
        self.assertFalse(any('database/' in p or p.endswith(('.bqrs', '.rs', '.sarif')) for p in seal['files']))
        for name, expected in seal['files'].items():
            self.assertEqual(d.sha256((session / 'evidence' / name).read_bytes()), expected['sha256'])

    def test_preflight_alone_never_starts_witness_query(self):
        code, terminal, calls, _ = self.flow('preflight-only')
        self.assertEqual(code, 1); self.assertEqual(terminal['guard'], 'INCOMPLETE')
        self.assertNotIn('witness-query', [phase for phase, _, _ in calls])
        self.assertEqual(terminal['witness_query'], 'NOT_RUN')

    def test_shared_preparation_fallback_and_prequery_mutation_fail_closed(self):
        for fault in ('shared-unprepared', 'shared-fallback', 'shared-before-query'):
            with self.subTest(fault=fault):
                code, terminal, calls, _ = self.flow(fault)
                self.assertEqual(code, 1)
                self.assertEqual(terminal['identity'], 'INCOMPLETE')
                self.assertEqual(terminal['witness_query'], 'NOT_RUN')
                self.assertNotIn('witness-query', [phase for phase, _, _ in calls])
                if fault == 'shared-unprepared':
                    self.assertEqual(terminal['command_failure']['phase'], 'apply-shared-helper-patch')

    def test_shared_final_identity_cannot_replace_query_timeout(self):
        for fault in ('query-timeout', 'shared-mutated-timeout'):
            with self.subTest(fault=fault):
                code, terminal, _, session = self.flow(fault)
                self.assertEqual(code, 1)
                self.assertEqual(terminal['status'], 'WASM_CONTEXT_INCOMPLETE')
                self.assertEqual(terminal['witness_query'], 'INCOMPLETE')
                self.assertEqual(terminal['command_failure'],
                                 {'phase': 'witness-query', 'exit': 143, 'failure_kind': 'timeout', 'cleanup_errors': []})
                if fault == 'query-timeout':
                    self.assertEqual(terminal['identity'], 'PASS')
                    self.assertIn('shared', json.loads((session / 'evidence/helpers-after.json').read_text()))
                else:
                    self.assertEqual(terminal['identity'], 'INCOMPLETE')
                    self.assertIn('final_error_class', terminal)

    def test_oracle_or_after_identity_failure_retains_computed_fact(self):
        for fault in ('missing-macro', 'source-mutated'):
            with self.subTest(fault=fault):
                code, terminal, calls, _ = self.flow(fault)
                self.assertEqual(code, 1); self.assertEqual(terminal['status'], 'WASM_CONTEXT_INCOMPLETE')
                self.assertIn('witness-query', [phase for phase, _, _ in calls])
                self.assertEqual(terminal['extraction'], 'FINALIZED_CONTEXT_OBSERVED')
                self.assertEqual(terminal['macros'], 'INCOMPLETE' if fault == 'missing-macro' else 'PASS')

    def test_seal_failure_never_reports_complete_or_leaks_exception_text(self):
        code, terminal, _calls, session = self.flow('seal-failure')
        self.assertEqual(code, 1); self.assertEqual(terminal['status'], 'WASM_CONTEXT_INCOMPLETE')
        self.assertEqual(terminal['witness_query'], 'PASS')
        self.assertEqual(terminal['final_phase'], 'seal'); self.assertEqual(terminal['evidence'], 'INCOMPLETE')
        self.assertNotIn('private exception sentinel', (session / 'evidence/terminal.json').read_text())
        self.assertFalse((session / 'evidence/sha256.json').exists())


if __name__ == '__main__': unittest.main()
