#!/usr/bin/env python3
"""Bounded native observation regressions; no network, real apt or Rust."""
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import time
import unittest
from contextlib import redirect_stdout
from io import StringIO
from unittest.mock import patch

import codeql_rust_diagnostic_runtime as runtime
import codeql_rust_native_observer as observation

P = Path(__file__).with_name('codeql-rust-native-diagnostic.py')
spec = importlib.util.spec_from_file_location('native_diagnostic', P)
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)


def quote(value):
    return ''.join('%%%02x' % b if b <= 32 or b >= 127 or b == 37 else chr(b)
                   for b in value.encode())


def frame(code, fields, direction='<-'):
    text = code + '\n' + '\n'.join(key + ': ' + value for key, value in fields)
    return (' ' + direction + ' http:' + quote(text) + '\n').encode()


class NativeDiagnostic(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name); self.out = self.root / 'out'; self.out.mkdir()
        cap, write_cap = runtime.CAP, runtime.WRITE_CAP
        self.addCleanup(lambda: (setattr(runtime, 'CAP', cap), setattr(runtime, 'WRITE_CAP', write_cap)))

    def observer(self, phase='native-install'):
        return observation.StreamObserver(phase)

    def records(self, observer, chunks, stream='stderr', final=True):
        data = b''.join(observer.feed(stream, chunk) for chunk in chunks)
        if final: data += observer.feed(stream, b'', final=True)
        return [json.loads(line) for line in data.splitlines()]

    def test_worker_frames_strip_secrets_and_preserve_raw_offsets(self):
        secret = 'private-test-credential'
        first = frame('200 URI Start', [('URI', 'http://user:' + secret + '@azure.archive.ubuntu.com/ubuntu/pool/a.deb?token=' + secret), ('Size', '123')])
        last = frame('400 URI Failure', [('URI', 'https://archive.ubuntu.com/ubuntu/pool/a.deb'),
            ('Message', 'Connection timed out. Authorization: Bearer ' + secret), ('Transient-Failure', 'true')]).rstrip(b'\n')
        chunks = [first[:27], first[27:90], first[90:] + last[:13], last[13:]]
        rows = self.records(self.observer(), chunks)
        self.assertEqual([r['event']['code'] for r in rows], [200, 400])
        self.assertEqual([r['raw_offset'] for r in rows], [0, len(first)])
        self.assertEqual([r['raw_length'] for r in rows], [len(first), len(last)])
        self.assertEqual(rows[1]['event']['message_classes'], ['timeout'])
        self.assertNotIn(secret, json.dumps(rows))
        self.assertNotIn('user', json.dumps(rows))
        self.assertTrue(all(r['received_last_monotonic'] >= r['received_first_monotonic'] for r in rows))

    def test_configuration_decodes_exactly_twice_and_projects_allowed_values(self):
        items = [('Config-Item', quote(k) + '=' + quote(v)) for k, v in (
            ('Acquire::Retries', '0'), ('Acquire::http::Timeout', '20'),
            ('Acquire::https::Timeout', '15'), ('Acquire::http::Proxy', 'http://u:secret@proxy.example:8080/?secret'),
            ('APT::Authentication', 'secret'), ('Acquire::http::archive.ubuntu.com::Timeout', '30'),
            ('Acquire::http::Timeout::archive.ubuntu.com', '99'))]
        rows = self.records(self.observer(), [frame('601 Configuration', items, '->')])
        config = rows[0]['event']['config']
        self.assertEqual(config['acquire::retries'], '0')
        self.assertEqual(config['acquire::https::timeout'], '15')
        self.assertEqual(config['acquire::http::archive.ubuntu.com::timeout'], '30')
        self.assertNotIn('acquire::http::timeout::archive.ubuntu.com',config)
        self.assertEqual(config['acquire::http::proxy']['host'], 'proxy.example')
        self.assertNotIn('secret', json.dumps(rows))
        self.assertNotIn('authentication', json.dumps(rows))

    def test_unknown_overlong_and_crlf_lines_never_fall_back_to_raw_text(self):
        observer = self.observer()
        data = b'Authorization: opaque-secret\r\n' + b'private' * 50000 + b'\n' + b'opaque-secret'
        rows = self.records(observer, [data[i:i+8192] for i in range(0, len(data), 8192)])
        self.assertEqual(len(rows), 3)
        self.assertEqual(rows[1]['event']['kind'], 'OVERLONG')
        self.assertEqual(sum(r['raw_length'] for r in rows), len(data))
        self.assertNotIn('opaque-secret', json.dumps(rows))
        self.assertLess(len(json.dumps(rows)), 4096)
        self.assertEqual(observer.summary()['raw_bytes']['stderr'], len(data))

    def test_unterminated_line_uses_last_byte_time_not_flush_time(self):
        observer = self.observer()
        with patch.object(observation.time,'monotonic',side_effect=[100,200]), \
             patch.object(observation.time,'time',side_effect=[1000,2000]):
            self.assertEqual(observer.feed('stderr',b'unknown tail'),b'')
            row = json.loads(observer.feed('stderr',b'',final=True))
        self.assertEqual(row['received_last_monotonic'],100)
        self.assertEqual(row['received_last_wall'],1000)

    def test_real_timeout_final_drain_uses_same_projection(self):
        child = self.root / 'child.py'
        child.write_text('import signal,sys,time\n'
            'def stop(*args):\n print("Authorization: tail-secret",file=sys.stderr,flush=True);sys.exit(0)\n'
            'signal.signal(signal.SIGTERM,stop)\nprint("ready",flush=True)\ntime.sleep(20)\n')
        with self.assertRaises(runtime.PhaseError) as failure:
            runtime.run([sys.executable, str(child)], self.root, self.out/'phase', time.monotonic()+5,
                .3, self.out, dict(os.environ), observer=self.observer())
        self.assertEqual(failure.exception.result['failure_kind'], 'timeout')
        self.assertEqual(failure.exception.result['cleanup_errors'], [])
        rows = [json.loads(line) for line in (self.out/'phase.stderr').read_text().splitlines()]
        self.assertTrue(rows)
        self.assertNotIn('tail-secret', ''.join(p.read_text() for p in self.out.iterdir()))

    def test_projection_expansion_respects_shared_cap_and_keeps_receipt(self):
        with patch.object(runtime, 'CAP', 1024**2), patch.object(runtime, 'WRITE_CAP', 4096):
            with self.assertRaises(runtime.PhaseError) as failure:
                runtime.run([sys.executable, '-c', 'print("x\\n"*2000)'], self.root, self.out/'phase',
                    time.monotonic()+5, 3, self.out, dict(os.environ), observer=self.observer())
        self.assertIn('evidence budget', failure.exception.result['error'])
        self.assertTrue((self.out/'phase.result.json').is_file())
        self.assertLess(runtime.output_bytes(self.out), 1024**2)

    def test_partial_snapshot_reports_unreadable_and_does_not_follow_links(self):
        archives = self.root/'archives'; partial = archives/'partial'; partial.mkdir(parents=True)
        (partial/'pkg_1_amd64.deb').write_bytes(b'x'*7)
        (partial/'link_1_amd64.deb').symlink_to(self.root/'secret')
        rows = observation.archive_snapshot((archives, partial))
        self.assertEqual(rows[1]['files'][0]['size'], 7)
        self.assertEqual(rows[1]['non_regular'], 1)
        with patch.object(observation.os, 'scandir', side_effect=PermissionError('private-detail')):
            rows = observation.archive_snapshot((partial,))
        self.assertEqual(rows[0]['status'], 'UNREADABLE')
        self.assertNotIn('private-detail', json.dumps(rows))
        self.assertNotIn('files', rows[0])

    def test_source_projection_omits_auth_query_and_unrelated_fields(self):
        apt = self.root/'apt'; (apt/'sources.list.d').mkdir(parents=True)
        (apt/'sources.list.d/ubuntu.sources').write_text('Types: deb\nURIs: https://user:source-secret@archive.ubuntu.com/ubuntu?source-secret\nSuites: noble noble-updates\nSigned-By: source-secret\n')
        (apt/'apt-mirrors.txt').write_text('http://azure.archive.ubuntu.com/ubuntu\nhttps://archive.ubuntu.com/ubuntu\n')
        data = observation.source_snapshot(apt)
        self.assertNotIn('source-secret', json.dumps(data))
        self.assertIn('archive.ubuntu.com', json.dumps(data))
        self.assertIn('sha256', json.dumps(data))

    def test_fixed_native_chain_has_one_install_and_stops_after_exit_124(self):
        for exitcode in (0, 124):
            with self.subTest(exitcode=exitcode):
                out = self.out/str(exitcode); out.mkdir()
                calls=[]
                def run(argv, cwd, output, end, timeout, root, env, **kw):
                    calls.append((argv, end, kw))
                    if 'install' in argv and exitcode:
                        raise runtime.PhaseError({'failure_kind':'child_exit','exit':124})
                controller = observation.NativeObservation(out, dict(os.environ))
                with patch.object(m.native.runtime, 'run', side_effect=run):
                    if exitcode:
                        with self.assertRaises(runtime.PhaseError):
                            m.native.prepare_native(self.root, out, time.monotonic()+600, {}, maximum_seconds=360, observation=controller)
                    else:
                        m.native.prepare_native(self.root, out, time.monotonic()+600, {}, maximum_seconds=360, observation=controller)
                self.assertEqual(len(calls), 2 if exitcode else 4)
                self.assertEqual(sum('install' in argv for argv, _, _ in calls), 1)
                self.assertTrue(all('observer' in kw for _, _, kw in calls))
                self.assertTrue(all('Debug::pkgAcquire::Worker=1' in argv for argv, _, _ in calls[:2]))
                self.assertFalse(any(any(x in argv for x in ('cargo','rustc','rustup','codeql')) for argv,_,_ in calls))
                self.assertEqual(json.loads((out/'native-budget.json').read_text())['maximum_seconds'],360)

    def test_entry_with_real_fake_commands_seals_success_and_install_failure(self):
        commands = self.root/'bin'; commands.mkdir()
        for name, body in {
            'apt-get': 'print("apt 2.8.1 (amd64)")',
            'sudo': 'import os,sys\nassert "GH_TOKEN" not in os.environ\n'
                    'print("Get:1 http://u:fixture-secret@archive.ubuntu.com/ubuntu noble/main pkg 1")\n'
                    'sys.exit(int(os.environ["FIXTURE_EXIT"]) if "install" in sys.argv else 0)',
            'dpkg-query': 'print("libgtk-3-dev\\t3.24.41-1")',
            'pkg-config': 'print("3.24.41")',
            **{name: 'raise SystemExit("unexpected compiler invocation")' for name in ('cargo', 'rustc', 'rustup', 'codeql')}
        }.items():
            path = commands/name; path.write_text('#!'+sys.executable+'\n'+body+'\n'); path.chmod(0o755)
        for code in (0, 124):
            out = self.out/str(code); out.mkdir()
            env = dict(os.environ, PATH=str(commands)+os.pathsep+os.environ['PATH'],
                       GH_TOKEN='fixture-secret', FIXTURE_EXIT=str(code))
            with patch.object(m, 'identity', return_value={'tool_sha':'a'*40}), \
                 patch.object(observation, 'source_snapshot', return_value={'files':[]}), \
                 patch.object(observation.NativeObservation, 'poll'), redirect_stdout(StringIO()):
                result = m.diagnose(self.root, out, time.time()+590, 'a'*40, env)
            terminal = json.loads((out/'native-terminal.json').read_text())
            self.assertEqual(result, int(bool(code)))
            self.assertEqual(terminal['status'], 'NATIVE_INCOMPLETE' if code else 'NATIVE_READY')
            self.assertEqual(terminal['rust_codeql_execution'], 'NOT_REQUESTED')
            if code:
                self.assertEqual(terminal['error']['exit'], 124)
                self.assertFalse((out/'native-packages.stdout').exists())
                self.assertEqual(json.loads((out/'native-install.stdout.result.json').read_text())['exit'],124)
            else:
                self.assertTrue((out/'native-modules.stdout.result.json').exists())
            seal = json.loads((out/'sha256.json').read_text())
            self.assertEqual(set(seal), {p.name for p in out.iterdir()}-{'sha256.json'})
            for name, digest in seal.items(): self.assertEqual(runtime.digest(out/name), digest)
            self.assertNotIn('fixture-secret', ''.join(p.read_text() for p in out.iterdir()))

    def test_expired_job_does_not_spawn_and_still_seals_failure(self):
        with patch.object(m, 'identity') as identity, patch.object(runtime, 'run') as run, redirect_stdout(StringIO()):
            result = m.diagnose(self.root, self.out, time.time()-1, 'a'*40, {})
        self.assertEqual(result,1); identity.assert_not_called(); run.assert_not_called()
        self.assertEqual(json.loads((self.out/'native-terminal.json').read_text())['error']['stage'],'admission')
        self.assertTrue((self.out/'sha256.json').exists())

    def test_literal_worker_protocol_examples_and_malformed_frames(self):
        data = (b' -> http:601%20Configuration%0aConfig-Item:%20Acquire::Retries=0'
                b'%0aConfig-Item:%20X-Canary=a%253db%250aPRIVATE%0a%0a\n'
                b' <- mirror+file:103%20Redirect%0aURI:%20mirror+file:/etc/apt/apt-mirrors.txt/a.deb'
                b'%0aNew-URI:%20https://archive.ubuntu.com/ubuntu/a.deb\n'
                b' <- http:102%20Status%0aURI:%20http://archive.ubuntu.com/ubuntu/a.deb'
                b'%0aMessage:%20Waiting%20for%20headers%0a%20literal%2520%20and%20+\n'
                b' <- http:200%2xURI%20Start\n')
        rows = self.records(self.observer(), [data[i:i+7] for i in range(0,len(data),7)])
        self.assertEqual(rows[0]['event']['config'], {'acquire::retries':'0'})
        self.assertEqual(rows[1]['event']['New-URI']['scheme'],'https')
        self.assertEqual(rows[2]['event']['message_classes'],['waiting_headers'])
        self.assertEqual(rows[3]['event']['kind'],'UNKNOWN_FORMAT')
        self.assertNotIn('PRIVATE',json.dumps(rows))

    def test_https_port_is_not_an_http_status_code(self):
        rows = self.records(self.observer(), [frame('400 URI Failure', [
            ('Message', 'Connection failed [IP: 203.0.113.8 443]'), ('FailReason', 'ConnectionRefused')]),
            frame('400 URI Failure', [('Message', '404 Not Found [IP: 203.0.113.8 443]'), ('FailReason', 'HttpError404')])])
        self.assertNotIn('http_status_code', rows[0]['event'])
        self.assertEqual(rows[1]['event']['http_status_code'],404)

    def test_source_limits_are_not_reported_as_complete_observations(self):
        apt = self.root/'apt'; parts = apt/'sources.list.d'; parts.mkdir(parents=True)
        for index in range(33):
            (parts/(str(index)+'.sources')).write_text('Suites: noble\n'*1025)
        value = observation.source_snapshot(apt)
        self.assertEqual(value['listing_status'], 'PARTIAL_LIMIT')
        source_files = [row for row in value['files'] if row['path'].endswith('.sources')]
        self.assertEqual(len(source_files),32)
        self.assertTrue(all(row['status'] == 'PARTIAL_LIMIT' for row in source_files))

    def test_manual_workflow_bounds_and_explicit_test_registration(self):
        repo = P.parents[2]
        workflow = (repo/'.github/workflows/codeql-rust-native-diagnostic.yml').read_text()
        for token in ('default: false', 'inputs.execute == true', 'github.event.repository.default_branch',
            'timeout-minutes: 10', 'local_now+600-age', 'contents: read', 'actions: read',
            'if: always()', 'timeout-minutes: 1', 'oasis7-codeql-scan-slot-1', 'persist-credentials: false'):
            self.assertIn(token, workflow)
        for token in ('rustup ', 'codeql-action/', 'security-events:', 'upload-sarif', 'pull_request:', 'schedule:'):
            self.assertNotIn(token, workflow)
        self.assertIn('run python3 ./scripts/security/'+Path(__file__).name, (repo/'scripts/ci-tests.sh').read_text())
        scope = json.loads((repo/'scripts/ci-required-scope.json').read_text())
        self.assertEqual(scope['scoped_control_paths']['.github/workflows/codeql-rust-native-diagnostic.yml'], ['workflow_governance'])


if __name__ == '__main__':
    unittest.main()
