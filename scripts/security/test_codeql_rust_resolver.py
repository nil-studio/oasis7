#!/usr/bin/env python3
"""Resolver framing, persistence acknowledgements and real owned-process failures."""
import contextlib
import ctypes
import hashlib
import json
import os
from pathlib import Path
import signal
import struct
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

import codeql_rust_resolver as m


def frame(value, raw=b''):
    return (json.dumps(value, separators=(',', ':')) + '\n').encode() + raw


def fake_runtime_session(observer, output, response, *, fail_index=None):
    """Existing consumer fixtures retain real parent validation/persistence/ACK."""
    output = Path(output)
    data = bytearray()
    data.extend(observer.feed('stdout', frame(dict(kind='server', adapter_pid=123, pid=124, pgid=123))))
    for index, record in enumerate(observer.plan['records']):
        started = observer.plan['started'] if index == 0 else time.monotonic()
        deadline = min(started + m.COMMAND_SECONDS, observer.plan['end'] - m.CLEANUP_SECONDS)
        data.extend(observer.feed('stdout', frame(dict(kind='command', index=index, started=started, deadline=deadline))))
        if index == fail_index:
            raise TimeoutError('injected resolver failure')
        raw = json.dumps(response(record)).encode()
        data.extend(observer.feed('stdout', frame(dict(kind='response', index=index,
                              bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest(), nul=True), raw)))
        output.write_bytes(data)
        observer.poll(observer.plan['end'] - m.CLEANUP_SECONDS)
        if os.read(observer.read_fd, 4) != struct.pack('!I', index):
            raise AssertionError('parent did not acknowledge the persisted index')
    terminal = dict(kind='terminal', error=None, index=len(observer.plan['records']) - 1,
                    completed=len(observer.plan['records']), acks=len(observer.plan['records']), shutdown=True,
                    server=dict(pid=124, pgid=123, exit=0, stdout_eof=True, stderr_eof=True,
                                remaining=[], cleanup_errors=[], reaped_orphans=[]))
    data.extend(observer.feed('stdout', frame(terminal), final=True))
    observer.feed('stderr', b'', final=True)
    output.write_bytes(data)
    return dict(exit=0, pid=123, pgid=123, cleanup_errors=[], evidence_errors=[],
                failure_kind=None, output_observation=observer.summary())


FAKE_CLI = r'''
import json,os,pathlib,signal,subprocess,sys,time
if sys.argv[1:] != ['execute','cli-server']: sys.exit(91)
work=pathlib.Path(os.environ['FAKE_WORK'])
plan=json.loads(pathlib.Path(os.environ['FAKE_REQUEST']).read_text())
mode=os.environ.get('FAKE_MODE','success')
calls=work/'calls.jsonl'
def record(argv):
    with calls.open('a') as f:f.write(json.dumps(argv)+'\n')
def tail(_signum,_frame):
    os.write(2,b'term-tail\n')
    sys.exit(7)
if mode=='term_tail':signal.signal(signal.SIGTERM,tail)
if mode=='orphan':signal.signal(signal.SIGTERM,signal.SIG_IGN)
pending=bytearray()
while True:
    byte=sys.stdin.buffer.read(1)
    if not byte:sys.exit(0)
    if byte!=b'\0':pending.extend(byte);continue
    argv=json.loads(pending);pending.clear();record(argv)
    if argv==['shutdown']:
        if mode=='shutdown_tail':os.write(1,b'extra')
        sys.exit(0)
    if argv[:2]!=['resolve','library-path']:sys.exit(92)
    query=next(x.split('=',1)[1] for x in argv if x.startswith('--query='))
    item=next(x for x in plan['records'] if x['path']==query)
    if mode in ('hang','term_tail','orphan'):
        if mode=='orphan':
            child=subprocess.Popen([sys.executable,'-c','import signal,time;signal.signal(signal.SIGTERM,signal.SIG_IGN);time.sleep(120)'])
            (work/'orphan.pid').write_text(str(child.pid))
        (work/'started').write_text(str(os.getpid()))
        while True:time.sleep(.05)
    if mode=='exit_zero':sys.exit(0)
    if mode=='nonzero':sys.exit(7)
    if mode=='overflow':
        for _ in range(4):os.write(2,b'x'*65536)
    closure=plan['closures'][item['kind']]
    value=dict(libraryPath=[item['root']]+[p['root'] for p in closure.values()],
               dbscheme=closure['codeql/rust-all']['root']+'/rust.dbscheme',
               compilationCache=[plan['work']+'/resolve-compilation-cache'],
               relativeName=item['pack']+'/'+item['relative'],qlPackName=item['pack'])
    if mode=='bad_closure':value['libraryPath'][1]='/wrong/stock'
    if mode=='bad_cache':value['compilationCache']=['/wrong/cache']
    if mode=='bad_query':value['relativeName']='wrong/query.ql'
    raw=json.dumps(value).encode()
    if mode=='invalid_json':raw=b'{broken'
    if mode=='extra_nul':raw+=b'\0'
    if mode=='no_nul':os.write(1,raw);sys.exit(0)
    wire=raw+b'\0'
    if mode=='split':
        for pos in range(0,len(wire),7):os.write(1,wire[pos:pos+7])
    else:os.write(1,wire)
'''


def subreaper_state():
    value = ctypes.c_int()
    if ctypes.CDLL(None).prctl(37, ctypes.byref(value), 0, 0, 0) != 0:
        raise RuntimeError('could not read subreaper')
    return value.value


class Resolver(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.work, self.out = self.root/'work', self.root/'evidence'
        self.work.mkdir(); self.out.mkdir()
        self.cli = self.root/'codeql'
        self.cli.write_text('#!'+sys.executable+'\n'+FAKE_CLI)
        self.cli.chmod(0o700)
        self.query_root = self.work/'queries'; self.query_root.mkdir()
        closure = {}
        for index in range(14):
            name = 'codeql/rust-all' if index == 0 else 'codeql/library-'+str(index)
            root = self.work/('library-'+str(index)); root.mkdir()
            closure[name] = dict(root=str(root))
        self.closures = dict(default=closure)
        self.records = []
        for index in range(37):
            path = self.query_root/(str(index)+'.ql')
            path.write_text('select '+str(index)+'\n')
            self.records.append(dict(path=str(path), root=str(self.query_root), relative=path.name,
                                     pack='synthetic/resolver', id='query-'+str(index),
                                     sha256=hashlib.sha256(path.read_bytes()).hexdigest()))
        self.calls = []
        self.original_subreaper = subreaper_state()
        self.original_fds = set(os.listdir('/proc/self/fd'))
        self.addCleanup(lambda: self.assertEqual(self.original_subreaper, subreaper_state()))

    def invoke(self, mode='success', seconds=30):
        env = dict(os.environ, FAKE_MODE=mode, FAKE_WORK=str(self.work),
                   FAKE_REQUEST=str(self.out/'resolver-request.json'), PYTHONDONTWRITEBYTECODE='1')
        end = time.monotonic()+seconds
        def run(argv, label, limit=None, **options):
            self.calls.append((argv, label, options))
            return m.runtime.run(argv, self.work, self.out/(label+'.log'), end,
                                 max(0, end-time.monotonic()), self.out, env, file_limit=None, **options)
        return m.resolve(self.cli, self.records, self.closures, self.work, self.out,
                         run, end=end, mode='production')

    def sent(self):
        path = self.work/'calls.jsonl'
        return [json.loads(x) for x in path.read_text().splitlines()] if path.exists() else []

    def report(self):
        return json.loads((self.out/'resolver-session.json').read_text())

    def assert_closed(self):
        report = self.report()
        self.assertTrue(report['parent_subreaper_restored'])
        self.assertEqual(subreaper_state(), self.original_subreaper)
        self.assertEqual(set(os.listdir('/proc/self/fd')), self.original_fds)
        process = report['runtime']
        if process and process.get('pid'):
            self.assertFalse(process['cleanup_errors'])
            with self.assertRaises(ProcessLookupError):
                os.killpg(process['pid'], 0)

    def test_actual_child_one_server_all_37_responses_and_persisted_acks(self):
        result = self.invoke('split')
        self.assertEqual(len(result), 37)
        self.assertEqual(len(self.calls), 1)
        self.assertTrue(self.calls[0][2]['restricted_output'])
        self.assertTrue(self.calls[0][2]['reap_orphans'])
        self.assertEqual(len(self.sent()), 38)
        self.assertEqual(self.sent()[-1], ['shutdown'])
        report = self.report()
        self.assertEqual(report['status'], 'COMPLETE')
        self.assertEqual(report['terminal']['server']['exit'], 0)
        self.assertTrue(report['ack_pipe_eof'])
        self.assertTrue(all(x['nul'] and x['persisted'] and x['ack'] for x in report['commands']))
        self.assertTrue(all('exit' not in x for x in report['commands']))
        self.assertEqual(len(list(self.out.glob('resolve-*.log'))), 37)
        self.assert_closed()

    def test_invalid_response_stops_before_next_command(self):
        # Each case owns an independent fake server and filesystem, never CodeQL.
        for mode in ('bad_closure', 'bad_cache', 'bad_query', 'invalid_json', 'extra_nul', 'no_nul', 'exit_zero', 'nonzero'):
            with self.subTest(mode=mode):
                fixture = Resolver('test_actual_child_one_server_all_37_responses_and_persisted_acks')
                fixture.setUp()
                try:
                    with self.assertRaises(m.ResolverError):
                        fixture.invoke(mode)
                    self.assertEqual(len(fixture.sent()), 1)
                    self.assertEqual(fixture.report()['acknowledged'], 0)
                    fixture.assert_closed()
                finally:
                    fixture.doCleanups()

    def test_parent_response_write_failure_does_not_ack_or_send_next(self):
        real_write = m.runtime.evidence_write
        def fail(path, data, root, terminal=False):
            if Path(path).name == 'resolve-00.log':
                raise OSError('injected disk failure')
            return real_write(path, data, root, terminal=terminal)
        with patch.object(m.runtime, 'evidence_write', side_effect=fail):
            with self.assertRaises(m.ResolverError):
                self.invoke()
        self.assertEqual(len(self.sent()), 1)
        self.assertEqual(self.report()['acknowledged'], 0)
        self.assertEqual(self.report()['error'], 'RUNTIME_FAILURE')
        self.assertEqual(self.report()['runtime']['execution_failure_kind'], 'unsafe')
        self.assertEqual(self.report()['observer_error'], 'IO_ERROR')
        self.assert_closed()

    def test_parent_index_write_failure_keeps_raw_but_does_not_ack(self):
        real_save = m.runtime.save
        def fail(path, value, root, terminal=False):
            if Path(path).name == 'library-resolution.json':
                raise OSError('injected index failure')
            return real_save(path, value, root, terminal=terminal)
        with patch.object(m.runtime, 'save', side_effect=fail):
            with self.assertRaises(m.ResolverError):
                self.invoke()
        self.assertTrue((self.out/'resolve-00.log').is_file())
        self.assertEqual(len(self.sent()), 1)
        self.assertEqual(self.report()['acknowledged'], 0)
        self.assert_closed()

    def test_wrong_duplicate_and_eof_ack_stop_the_child(self):
        for mode in ('wrong', 'duplicate', 'eof'):
            with self.subTest(mode=mode):
                fixture = Resolver('test_actual_child_one_server_all_37_responses_and_persisted_acks')
                fixture.setUp()
                original = m.Observation.poll
                injected = [False]
                def poll(observer, end):
                    if observer.pending_ack is not None and not injected[0]:
                        injected[0] = True
                        if mode == 'wrong':
                            observer.pending_ack = struct.pack('!I', 1)
                        elif mode == 'duplicate':
                            observer.pending_ack *= 2
                        else:
                            os.close(observer.write_fd); observer.write_fd = None
                            observer.pending_ack = None
                    return original(observer, end)
                try:
                    with patch.object(m.Observation, 'poll', poll):
                        with self.assertRaises(m.ResolverError):
                            fixture.invoke()
                    self.assertTrue(injected[0])
                    self.assertEqual(len(fixture.sent()), 1)
                    fixture.assert_closed()
                finally:
                    fixture.doCleanups()

    def test_ack_short_write_eagain_and_interrupted_retry_without_duplicate_command(self):
        real_write = os.write
        injected = []
        actual_poll = m.Observation.poll
        def poll(observer, end):
            def write(fd, data):
                if fd == observer.write_fd and len(injected) < 3:
                    injected.append(len(injected))
                    if len(injected) == 1:
                        return real_write(fd, data[:1])
                    if len(injected) == 2:
                        raise BlockingIOError()
                    raise InterruptedError()
                return real_write(fd, data)
            with patch.object(m.os, 'write', side_effect=write):
                return actual_poll(observer, end)
        with patch.object(m.Observation, 'poll', poll):
            self.invoke()
        self.assertEqual(injected, [0, 1, 2])
        self.assertEqual(len(self.sent()), 38)
        self.assert_closed()

    def test_expired_admission_spawns_nothing_and_closes_fds(self):
        with self.assertRaises(m.ResolverError):
            self.invoke(seconds=1)
        self.assertEqual(self.calls, [])
        self.assert_closed()

    def test_timeout_retains_term_tail_and_original_failure(self):
        with self.assertRaises(m.ResolverError):
            self.invoke('term_tail', seconds=9)
        self.assertEqual(len(self.sent()), 1)
        self.assertIn(b'term-tail', (self.out/'resolver-session.log.stderr').read_bytes())
        self.assertIn(self.report()['runtime']['execution_failure_kind'], ('unsafe', 'timeout', 'child_exit'))
        self.assertIn(self.report()['observer_error'], ('DEADLINE', 'CANCELLED'))
        self.assert_closed()

    def test_inflight_adapter_kill_is_reaped_by_parent_with_its_descendants(self):
        original = m.Observation.poll
        killed = []
        def poll(observer, end):
            if observer.server and (self.work/'orphan.pid').exists() and not killed:
                killed.append(observer.server['adapter_pid'])
                os.kill(killed[0], signal.SIGKILL)
            return original(observer, end)
        with patch.object(m.Observation, 'poll', poll):
            with self.assertRaises(m.ResolverError):
                self.invoke('orphan', seconds=15)
        self.assertEqual(len(killed), 1)
        self.assertEqual(len(self.sent()), 1)
        self.assertEqual(self.report()['runtime']['exit'], -signal.SIGKILL)
        orphan = int((self.work/'orphan.pid').read_text())
        with self.assertRaises(ProcessLookupError):
            os.kill(orphan, 0)
        self.assert_closed()

    def test_unassigned_shutdown_tail_cannot_report_complete(self):
        with self.assertRaises(m.ResolverError):
            self.invoke('shutdown_tail')
        self.assertEqual(len(self.sent()), 38)
        self.assertEqual(self.report()['status'], 'INCOMPLETE')
        self.assert_closed()

    def test_inflight_parent_cancellation_stops_sequence_and_reaps_group(self):
        original = m.Observation.poll
        cancelled = []
        def poll(observer, end):
            if (self.work/'started').exists() and not cancelled:
                cancelled.append(True)
                os.kill(os.getpid(), signal.SIGTERM)
            return original(observer, end)
        with patch.object(m.Observation, 'poll', poll):
            with self.assertRaises(m.ResolverError):
                self.invoke('hang')
        self.assertTrue(cancelled)
        self.assertEqual(len(self.sent()), 1)
        self.assertEqual(self.report()['error'], 'CANCELLED')
        self.assertEqual(self.report()['acknowledged'], 0)
        self.assert_closed()

    def test_spawn_failure_keeps_zero_prefix_and_restores_parent_state(self):
        with patch.object(m.runtime.subprocess, 'Popen', side_effect=OSError('injected spawn failure')):
            with self.assertRaises(m.ResolverError):
                self.invoke()
        self.assertEqual(self.sent(), [])
        self.assertEqual(self.report()['acknowledged'], 0)
        self.assertIsNone(self.report()['runtime']['pid'])
        self.assert_closed()

    def test_output_registration_failure_never_polls_or_acknowledges(self):
        original = Path.open
        polled = []
        def opened(path, *args, **kwargs):
            if path.name == 'resolver-session.log.stderr' and args and args[0] == 'xb':
                raise OSError('injected registration failure')
            return original(path, *args, **kwargs)
        with patch.object(Path, 'open', opened), patch.object(m.Observation, 'poll', side_effect=lambda *_: polled.append(True)):
            with self.assertRaises(m.ResolverError):
                self.invoke()
        self.assertEqual(polled, [])
        self.assertEqual(self.report()['acknowledged'], 0)
        self.assertLessEqual(len(self.sent()), 1)
        self.assert_closed()

    def test_runtime_projection_write_failure_prevents_pending_ack(self):
        original = m.runtime.output_bytes
        def size(root):
            # This is reached after feed persisted the raw response and index,
            # but before runtime flushes its projected control output.
            if (self.out/'library-resolution.json').exists():
                return m.runtime.WRITE_CAP
            return original(root)
        with patch.object(m.runtime, 'output_bytes', side_effect=size):
            with self.assertRaises(m.ResolverError):
                self.invoke()
        self.assertEqual(len(self.sent()), 1)
        self.assertEqual(self.report()['validated_responses'], 1)
        self.assertEqual(self.report()['acknowledged'], 0)
        process = json.loads((self.out/'resolver-session.log.result.json').read_text())
        self.assertEqual(process['execution_failure_kind'], 'unsafe')
        self.assertEqual(process['error'], 'OBSERVATION_ERROR')
        self.assertEqual(self.report()['error'], 'RUNTIME_FAILURE')
        self.assertEqual(self.report()['runtime']['execution_failure_kind'], process['execution_failure_kind'])
        self.assertEqual(self.report()['runtime']['error'], process['error'])
        self.assertIn(self.report()['observer_error'], ('CANCELLED', 'CONTROL_EOF'))
        self.assert_closed()

    def test_terminal_poll_keeps_last_command_closing_deadline_without_ack(self):
        # The terminal may arrive during the already-reserved cleanup interval.
        original = m.Observation._event
        checked = []
        def event(observer, value):
            result = original(observer, value)
            if value.get('kind') == 'terminal':
                end = observer.current['deadline']
                with patch.object(m.time, 'monotonic', return_value=end-.01):
                    observer.poll(observer.plan['end'])
                with patch.object(m.time, 'monotonic', return_value=end+.01):
                    with self.assertRaisesRegex(m.ResolverError, 'DEADLINE'):
                        observer.poll(observer.plan['end'])
                checked.append(True)
            return result
        with patch.object(m.Observation, '_event', event):
            with self.assertRaises(m.ResolverError):
                self.invoke()
        self.assertEqual(checked, [True])
        self.assertEqual(len(self.sent()), 38)
        self.assert_closed()

    def test_runtime_return_after_last_command_deadline_cannot_complete(self):
        real_clock, real_run = time.monotonic, m.runtime.run
        offset = [0.0]
        def run(*args, **kwargs):
            result = real_run(*args, **kwargs)
            observer = kwargs['observer']
            # The shared end remains in the future; only this command's
            # original closing deadline has elapsed after runtime returns.
            offset[0] = observer.current['deadline']+.01-real_clock()
            self.assertLess(real_clock()+offset[0], observer.plan['end'])
            return result
        with patch.object(m.time, 'monotonic', side_effect=lambda: real_clock()+offset[0]), patch.object(m.runtime, 'run', run):
            with self.assertRaisesRegex(m.ResolverError, 'DEADLINE'):
                self.invoke()
        self.assertEqual(self.report()['status'], 'INCOMPLETE')
        self.assertEqual(self.report()['error'], 'DEADLINE')
        self.assert_closed()

    def test_final_receipt_crossing_either_cutoff_is_a_failure_receipt(self):
        for cutoff in ('command', 'shared'):
            with self.subTest(cutoff=cutoff):
                fixture = Resolver('test_actual_child_one_server_all_37_responses_and_persisted_acks')
                fixture.setUp()
                real_clock, real_write = time.monotonic, m.runtime.evidence_write
                offset, writes = [0.0], []
                def write(path, data, root, terminal=False):
                    result = real_write(path, data, root, terminal=terminal)
                    if Path(path).name == 'resolver-session.json':
                        report = json.loads(data)
                        writes.append(report['status'])
                        if len(writes) == 1:
                            end = report['commands'][-1]['deadline'] if cutoff == 'command' else report['absolute_end']
                            offset[0] = end+.01-real_clock()
                    return result
                try:
                    with patch.object(m.time, 'monotonic', side_effect=lambda: real_clock()+offset[0]), patch.object(m.runtime, 'evidence_write', write):
                        with self.assertRaisesRegex(m.ResolverError, 'DEADLINE'):
                            fixture.invoke()
                    self.assertEqual(writes, ['COMPLETE', 'INCOMPLETE'])
                    self.assertEqual(fixture.report()['status'], 'INCOMPLETE')
                    self.assertEqual(fixture.report()['error'], 'DEADLINE')
                    fixture.assert_closed()
                finally:
                    fixture.doCleanups()

    def test_late_failure_receipt_retains_the_existing_runtime_primary(self):
        real_clock, real_write = time.monotonic, m.runtime.evidence_write
        offset, writes = [0.0], []
        def write(path, data, root, terminal=False):
            result = real_write(path, data, root, terminal=terminal)
            if Path(path).name == 'resolver-session.json':
                report = json.loads(data)
                writes.append((report['status'], report['error']))
                if len(writes) == 1:
                    offset[0] = report['commands'][-1]['deadline']+.01-real_clock()
            return result
        with patch.object(m.time, 'monotonic', side_effect=lambda: real_clock()+offset[0]), patch.object(m.runtime, 'evidence_write', write):
            with self.assertRaisesRegex(m.ResolverError, 'ADAPTER_EXIT'):
                self.invoke('bad_cache')
        self.assertEqual(writes, [('INCOMPLETE', 'ADAPTER_EXIT'), ('INCOMPLETE', 'ADAPTER_EXIT')])
        self.assertEqual(self.report()['runtime']['execution_failure_kind'], 'child_exit')
        self.assertEqual(self.report()['observer_error'], 'COMPILATION_CACHE')
        self.assertTrue(self.report()['receipt_deadline_exceeded'])
        self.assert_closed()

    def test_global_evidence_limit_never_admits_another_query(self):
        with patch.object(m.runtime, 'WRITE_CAP', 100000), patch.object(m.runtime, 'CAP', 300000):
            with self.assertRaises(m.ResolverError):
                self.invoke('overflow')
            self.assertLessEqual(m.runtime.output_bytes(self.out), 300000)
        self.assertEqual(len(self.sent()), 1)
        self.assertEqual(self.report()['acknowledged'], 0)
        self.assert_closed()

    def test_parent_persistence_uses_existing_command_deadline(self):
        original = m.runtime.save
        def saved(path, value, root, terminal=False):
            result = original(path, value, root, terminal=terminal)
            if Path(path).name == 'library-resolution.json':
                plan = json.loads((self.out/'resolver-request.json').read_text())
                work_end = plan['end'] - 2*m.CLEANUP_SECONDS
                time.sleep(max(0, work_end-time.monotonic()+.02))
            return result
        with patch.object(m.runtime, 'save', side_effect=saved):
            with self.assertRaises(m.ResolverError):
                self.invoke(seconds=9)
        self.assertEqual(len(self.sent()), 1)
        self.assertEqual(self.report()['error'], 'RUNTIME_FAILURE')
        self.assertEqual(self.report()['runtime']['execution_failure_kind'], 'unsafe')
        self.assertEqual(self.report()['observer_error'], 'DEADLINE')
        self.assertEqual(self.report()['acknowledged'], 0)
        self.assert_closed()

    def test_ack_wait_does_not_restart_current_command_deadline(self):
        original = m.Observation.poll
        def poll(observer, end):
            pending = observer.pending_ack
            observer.pending_ack = None
            try:
                return original(observer, end)
            finally:
                observer.pending_ack = pending
        with patch.object(m.Observation, 'poll', poll):
            with self.assertRaises(m.ResolverError):
                self.invoke(seconds=9)
        self.assertEqual(len(self.sent()), 1)
        self.assertIn(self.report()['runtime']['execution_failure_kind'], ('unsafe', 'child_exit'))
        self.assertEqual(self.report()['observer_error'], 'DEADLINE')
        self.assertEqual(self.report()['validated_responses'], 1)
        self.assertEqual(self.report()['acknowledged'], 0)
        self.assert_closed()

    def test_query_change_after_child_validation_is_not_acknowledged(self):
        original = m.Observation._response
        def response(observer):
            Path(observer.plan['records'][0]['path']).write_text('changed query\n')
            return original(observer)
        with patch.object(m.Observation, '_response', response):
            with self.assertRaises(m.ResolverError):
                self.invoke()
        self.assertEqual(len(self.sent()), 1)
        self.assertEqual(self.report()['error'], 'RUNTIME_FAILURE')
        self.assertEqual(self.report()['runtime']['execution_failure_kind'], 'unsafe')
        self.assertEqual(self.report()['observer_error'], 'QUERY_IDENTITY')
        self.assertEqual(self.report()['acknowledged'], 0)
        self.assert_closed()

    def test_preexisting_parent_subreaper_state_is_preserved(self):
        libc = ctypes.CDLL(None)
        old = subreaper_state()
        self.assertEqual(libc.prctl(36, 1, 0, 0, 0), 0)
        try:
            self.invoke()
            self.assertEqual(self.report()['parent_subreaper_original'], 1)
            self.assertEqual(subreaper_state(), 1)
        finally:
            self.assertEqual(libc.prctl(36, old, 0, 0, 0), 0)
        self.assert_closed()

    def test_existing_session_cannot_be_restarted_or_have_its_evidence_overwritten(self):
        self.invoke()
        before = {path.name: hashlib.sha256(path.read_bytes()).hexdigest()
                  for path in self.out.iterdir() if path.is_file()}
        with self.assertRaisesRegex(m.ResolverError, 'SESSION_ALREADY_EXISTS'):
            self.invoke()
        self.assertEqual(len(self.calls), 1)
        self.assertEqual(before, {path.name: hashlib.sha256(path.read_bytes()).hexdigest()
                                  for path in self.out.iterdir() if path.is_file()})
        self.assert_closed()

    def test_terminal_identity_types_and_prefix_cannot_substitute_for_real_server(self):
        # Mutate only the adapter control projection; the server still actually exits.
        mutations = (
            lambda v: v['server'].__setitem__('pid', v['server']['pid']+1),
            lambda v: v['server'].__setitem__('pgid', v['server']['pgid']+1),
            lambda v: v['server'].__setitem__('exit', False),
            lambda v: v['server'].__setitem__('stdout_eof', 1),
            lambda v: v['server'].__setitem__('unknown', True),
            lambda v: v.__setitem__('index', True),
            lambda v: v.__setitem__('completed', 36),
            lambda v: v.__setitem__('acks', 35),
        )
        original = m.Observation._event
        for change in mutations:
            with self.subTest(change=mutations.index(change)):
                fixture = Resolver('test_actual_child_one_server_all_37_responses_and_persisted_acks')
                fixture.setUp()
                def event(observer, value):
                    if value.get('kind') == 'terminal':
                        change(value)
                    return original(observer, value)
                try:
                    with patch.object(m.Observation, '_event', event):
                        with self.assertRaises(m.ResolverError):
                            fixture.invoke()
                    self.assertEqual(fixture.report()['status'], 'INCOMPLETE')
                    fixture.assert_closed()
                finally:
                    fixture.doCleanups()


if __name__ == '__main__':
    unittest.main()
