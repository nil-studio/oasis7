#!/usr/bin/env python3
"""WASM context trust, budget, first-write and semantic contracts. No extraction."""
import base64
import copy
from contextlib import ExitStack, redirect_stdout
import datetime
import email.utils
import hashlib
import importlib.util
import io
import json
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
        api = types.SimpleNamespace(environment={'GITHUB_RUN_ID': '42', 'GITHUB_RUN_ATTEMPT': '1'}, tool_sha='1' * 40)
        api.get = lambda _kind: ({'jobs': []}, None)
        with self.assertRaises(b.Rejected): b.bind_clock(api)
        receipt = clock_receipt(); receipt['maximum_job_seconds'] = 1200
        with self.assertRaises(d.Rejected): d.Clock(receipt)
        for key in ('deadline_wall', 'deadline_monotonic'):
            receipt = clock_receipt(); receipt[key] = float('nan')
            with self.assertRaises(d.Rejected): d.Clock(receipt)


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
        closure = {'codeql/' + ('rust-all' if i == 0 else 'library' + str(i)):
                   {'name': 'codeql/' + ('rust-all' if i == 0 else 'library' + str(i)),
                    'version': '0.2.25' if i == 0 else '1.0.0', 'root': str(session / 'work/codeql/qlpacks' / str(i)),
                    'dependencies': {}} for i in range(13)}
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
            elif phase == 'codeql-version': raw = b'{"version":"2.27.4"}'
            elif phase.startswith('query-library'):
                raw = d.encoded({'libraryPath': [str(work / 'query')] + [p['root'] for p in closure.values()],
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
            elif phase == 'witness-query':
                target = database / 'results' / w.PACK / (w.NAME + '.bqrs'); target.parent.mkdir(parents=True); target.write_bytes(b'BQRS fixture')
                if fault == 'source-mutated': (source / 'Cargo.lock').write_bytes(b'changed lock')
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
