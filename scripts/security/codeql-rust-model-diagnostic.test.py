#!/usr/bin/env python3
"""Local process/transport and official-input contract tests; no CodeQL execution."""
import importlib.util
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch, Mock

import codeql_rust_diagnostic_runtime as r

P = Path(__file__).with_name('codeql-rust-model-diagnostic.py')
spec = importlib.util.spec_from_file_location('diagnostic', P)
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)


def workflow_job(text, name):
    matches = re.findall(r'^  ' + re.escape(name) + r':\n(.*?)(?=^  [A-Za-z_][A-Za-z0-9_-]*:|\Z)',
                         text, re.M | re.S)
    if len(matches) != 1:
        raise ValueError('expected one workflow job: ' + name)
    return matches[0]


class ResourceTimeline(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.now = 100.0
        self.clock = patch.object(r.time, 'monotonic', side_effect=lambda: self.now)
        self.clock.start(); self.addCleanup(self.clock.stop)
        self.timeline = r.ResourceTimeline(42, self.root)
        self.write_pressure(100)

    def stat(self, folder, pid, start, cpu, pgid=42, faults=(3, 2)):
        folder.mkdir(parents=True, exist_ok=True)
        fields = ['0'] * 22
        for index, value in {0: 'R', 2: pgid, 7: faults[0], 9: faults[1],
                             11: cpu, 12: 2, 19: start, 21: 1}.items():
            fields[index] = str(value)
        (folder / 'stat').write_text(str(pid) + ' (private ) process name) ' + ' '.join(fields))

    def process(self, pid=100, start=20, cpu=10, io=100, faults=(3, 2), threads=None, pgid=42):
        folder = self.root / str(pid)
        self.stat(folder, pid, start, cpu, pgid, faults)
        (folder / 'io').write_text(''.join(name + ': ' + str(io) + '\n' for name in sorted(r.IO_KEYS)))
        for tid, thread_start, thread_cpu in threads or [(pid, start, cpu)]:
            self.stat(folder / 'task' / str(tid), tid, thread_start, thread_cpu, pgid)
        return folder

    def write_pressure(self, value):
        folder = self.root / 'pressure'; folder.mkdir(exist_ok=True)
        for name in ('cpu', 'memory', 'io'):
            (folder / name).write_text(''.join(kind + ' avg10=0.00 avg60=0.00 avg300=0.00 total=' +
                str(value) + '\n' for kind in ('some', 'full')))

    def poll(self, available=None):
        data = self.timeline.poll(10000, self.timeline.MAX_BYTES if available is None else available)
        if data is None:
            return None
        self.assertLessEqual(len(data), self.timeline.MAX_RECORD)
        self.assertNotIn(b'private', data)
        self.timeline.written(data)
        self.now += self.timeline.PERIOD
        return json.loads(data)

    def test_process_thread_io_fault_and_system_pressure_deltas_share_clock(self):
        self.process(threads=[(100, 20, 10), (101, 21, 3)])
        first = self.poll()
        self.assertIsNone(first['interval_seconds'])
        self.assertIsNone(first['processes'][0]['cpu_delta_ticks'])
        self.assertEqual(first['processes'][0]['threads'], [[100, 20, 12, None], [101, 21, 5, None]])
        self.process(cpu=40, io=117, faults=(8, 4), threads=[(100, 20, 35), (101, 21, 8)])
        self.write_pressure(120)
        second = self.poll(); process = second['processes'][0]
        self.assertEqual(second['interval_seconds'], 15)
        self.assertEqual(second['started_monotonic'], 115)
        self.assertGreater(second['started_wall'], 0)
        self.assertGreaterEqual(second['finished_monotonic'], second['started_monotonic'])
        self.assertEqual(process['cpu_delta_ticks'], 30)
        self.assertEqual(process['fault_deltas'], {'minor_faults': 5, 'major_faults': 2})
        self.assertEqual(process['io_deltas'], {name: 17 for name in r.IO_KEYS})
        self.assertEqual([row[3] for row in process['threads']], [25, 5])
        self.assertEqual(second['pressure_scope'], 'system')
        self.assertEqual(second['system_pressure']['cpu']['delta_us'], {'some': 20})
        self.assertNotIn('full', second['system_pressure']['cpu']['total_us'])
        self.assertEqual(self.timeline.result()['records'], 2)

    def test_pid_and_tid_reuse_and_foreign_group_never_share_deltas(self):
        self.process(); self.poll()
        self.process(start=30, cpu=100, threads=[(100, 30, 100)]);
        reused = self.poll()['processes'][0]
        self.assertIsNone(reused['cpu_delta_ticks'])
        self.assertIsNone(reused['threads'][0][3])
        self.process(start=30, cpu=120, threads=[(100, 31, 120)])
        thread_reused = self.poll()['processes'][0]
        self.assertEqual(thread_reused['cpu_delta_ticks'], 20)
        self.assertIsNone(thread_reused['threads'][0][3])
        self.process(start=30, pgid=77)
        self.assertEqual(self.poll()['processes'], [])
        self.process(start=30, cpu=130)
        self.assertIsNone(self.poll()['processes'][0]['cpu_delta_ticks'])

    def test_parent_identity_change_discards_the_entire_combination(self):
        folder = self.process(); self.poll()
        real_stat = self.timeline.stat
        calls = 0
        def changed(path, pid, end):
            nonlocal calls
            value = real_stat(path, pid, end)
            if path == folder:
                calls += 1
                if calls == 2:
                    value['start_ticks'] += 1
            return value
        with patch.object(self.timeline, 'stat', side_effect=changed):
            row = self.poll()
        self.assertEqual(row['processes'], [])
        self.assertIn('PROCESS_IDENTITY_CHANGED', row['limits'])
        self.assertIsNone(self.poll()['processes'][0]['cpu_delta_ticks'])

    def test_missing_or_bad_counters_clear_only_their_previous_sample(self):
        folder = self.process(); self.poll()
        (folder / 'io').write_bytes(b'private\xff')
        (folder / 'task/100/stat').unlink()
        (self.root / 'pressure/memory').write_text('some avg10=0 total=10\n')
        missing = self.poll(); process = missing['processes'][0]
        self.assertIsNone(process['io'])
        self.assertEqual(process['threads'], [])
        self.assertNotIn('memory', missing['system_pressure'])
        self.process(cpu=40, io=999); self.write_pressure(400)
        restored = self.poll(); process = restored['processes'][0]
        self.assertEqual(process['cpu_delta_ticks'], 30)
        self.assertTrue(all(value is None for value in process['io_deltas'].values()))
        self.assertIsNone(process['threads'][0][3])
        self.assertTrue(all(value is None for value in restored['system_pressure']['memory']['delta_us'].values()))

    def test_decreasing_counters_are_unknown_not_zero(self):
        self.process(cpu=100, io=100, faults=(100, 100)); self.poll()
        self.process(cpu=10, io=10, faults=(10, 10)); self.write_pressure(10)
        row = self.poll(); process = row['processes'][0]
        self.assertEqual(row['status'], 'PARTIAL')
        self.assertIsNone(process['cpu_delta_ticks'])
        self.assertTrue(all(value is None for value in process['fault_deltas'].values()))
        self.assertTrue(all(value is None for value in process['io_deltas'].values()))
        self.assertIsNone(process['threads'][0][3])
        self.assertIsNone(row['system_pressure']['io']['delta_us']['some'])
        self.assertGreater(row['counter_regressions'], 0)

    def test_process_thread_directory_and_read_bounds_are_explicit(self):
        self.process(threads=[(tid, 20, 10) for tid in range(100, 170)])
        row = self.poll()
        self.assertEqual(len(row['processes'][0]['threads']), 64)
        self.assertFalse(row['processes'][0]['threads_complete'])
        self.assertIn('THREAD_LIMIT', row['limits'])
        for pid in range(200, 210):
            self.process(pid=pid)
        row = self.poll()
        self.assertEqual(len(row['processes']), 8)
        self.assertIn('PROCESS_LIMIT', row['limits'])
        with patch.object(self.timeline, 'MAX_PROC_ENTRIES', 1):
            self.assertIn('PROCESS_DIRECTORY_LIMIT', self.poll()['limits'])
        (self.root / '100/io').write_bytes(b'x' * 4097)
        # Selection is an observation subset, so isolate this PID for the bad-read assertion.
        for pid in range(200, 210):
            shutil.rmtree(self.root / str(pid))
        row = self.poll()
        self.assertIsNone(row['processes'][0]['io'])
        self.assertGreater(row['read_errors'], 0)

    def test_sample_record_and_total_byte_limits_stop_only_extra_observation(self):
        self.process()
        with patch.object(self.timeline, 'MAX_SAMPLES', 1):
            self.poll()
            self.assertIsNone(self.poll())
        self.assertEqual(self.timeline.result()['stopped'], 'SAMPLE_LIMIT')
        for attribute, limit, expected in (('MAX_RECORD', 1, 'RECORD_LIMIT'), ('MAX_BYTES', 1, 'EVIDENCE_LIMIT')):
            self.timeline = r.ResourceTimeline(42, self.root)
            with patch.object(self.timeline, attribute, limit):
                self.assertIsNone(self.poll())
            self.assertEqual(self.timeline.result()['stopped'], expected)
            self.assertEqual(self.timeline.result()['bytes'], 0)
        self.timeline = r.ResourceTimeline(42, self.root)
        self.assertIsNone(self.poll(available=1))
        self.assertEqual(self.timeline.result()['stopped'], 'EVIDENCE_LIMIT')

    def test_soft_deadline_stops_reads_and_absolute_deadline_does_not_renew(self):
        self.process()
        real_stat = self.timeline.stat
        def slow(*args):
            value = real_stat(*args)
            self.now += .1
            return value
        with patch.object(self.timeline, 'stat', side_effect=slow):
            row = self.poll()
        self.assertIn('TIME_LIMIT', row['limits'])
        self.assertEqual(row['processes'], [])
        with patch.object(self.timeline, 'collect') as collect:
            self.assertIsNone(self.timeline.poll(self.now, 10000))
        collect.assert_not_called()
        self.assertEqual(self.timeline.result()['stopped'], 'FIXED_DEADLINE')

    def test_discarded_process_record_does_not_upgrade_persisted_unknown(self):
        self.assertEqual(self.poll()['processes'], [])
        self.process()
        with patch.object(self.timeline, 'MAX_RECORD', 1):
            self.assertIsNone(self.poll())
        self.assertEqual(self.timeline.result()['records'], 1)
        self.assertEqual(self.timeline.result()['observed_process_samples'], 0)
        self.assertEqual(self.timeline.result()['status'], 'UNAVAILABLE')


class Runtime(unittest.TestCase):
    def test_optional_timeline_observes_real_owned_process_and_threads(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(r.ResourceTimeline, 'PERIOD', .1):
            root = Path(directory)
            program = ('import threading,time\n'
                       'def work():\n'
                       ' end=time.monotonic()+.8\n'
                       ' while time.monotonic()<end: sum(range(1000))\n'
                       't=threading.Thread(target=work);t.start();work();t.join()\n')
            result = r.run([sys.executable, '-c', program], root, root/'run.log',
                           time.monotonic()+10, 5, root, dict(os.environ), resource_details=True)
            rows = [json.loads(line) for line in (root/'run.log.resources.jsonl').read_text().splitlines()]
            owned = [process for row in rows for process in row['processes'] if process['pid'] == result['pid']]
            self.assertGreaterEqual(result['resource_timeline']['records'], 2)
            self.assertTrue(any(len(process['threads']) >= 2 for process in owned))
            self.assertTrue(any((process['cpu_delta_ticks'] or 0) > 0 for process in owned))
            self.assertEqual(result['resource_timeline']['bytes'], (root/'run.log.resources.jsonl').stat().st_size)
            self.assertEqual(result['exit'], 0)

    def test_sampling_error_preserves_real_timeout_and_cleanup(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(
                r.ResourceTimeline, 'collect', side_effect=RuntimeError('private proc text')):
            root = Path(directory)
            with self.assertRaises(r.PhaseError) as failed:
                r.run([sys.executable, '-c', 'import time;time.sleep(10)'], root, root/'run.log',
                      time.monotonic()+5, .4, root, dict(os.environ), resource_details=True)
            result = failed.exception.result
            self.assertTrue(failed.exception.clean_timeout)
            self.assertEqual(result['failure_kind'], 'timeout')
            self.assertEqual(result['resource_timeline']['stopped'], 'SAMPLING_ERROR')
            self.assertEqual(result['cleanup_errors'], [])
            self.assertNotIn('private proc text', json.dumps(result))

    def test_actual_fifo_keeps_structured_stdout_and_stderr_distinct(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            program = 'import sys,json; print("stdout"); print("stderr",file=sys.stderr); f=open(sys.argv[1],"w"); f.write(json.dumps({"type":"LOG_HEADER"})+"\\n"); f.close()'
            value = r.run([sys.executable, '-c', program, '{EVALUATOR_FIFO}'], root,
                root/'stdout.log', time.monotonic()+5, 3, root, dict(os.environ), evaluator=root/'events.jsonl')
            self.assertEqual(value['exit'], 0)
            self.assertEqual((root/'stdout.log').read_text(), 'stdout\n')
            self.assertEqual((root/'stdout.log.stderr').read_text(), 'stderr\n')
            self.assertEqual(json.loads((root/'events.jsonl').read_text()), {'type':'LOG_HEADER'})
            self.assertFalse((root/'events.fifo').exists())
            self.assertNotIn('resource_timeline', value)
            self.assertFalse((root/'stdout.log.resources.jsonl').exists())

    def test_timeout_preserves_partial_event_and_reaps_group(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            ready=root/'ready'
            program='import sys,time,pathlib,signal; f=open(sys.argv[1],"w"); f.write("{\\"type\\":\\"LOG_HEADER\\"}\\n"); f.flush(); signal.signal(signal.SIGTERM,lambda *_:(f.write("{\\"type\\":\\"CLEANUP_MARKER\\"}\\n"),f.flush(),sys.exit(0))); pathlib.Path(sys.argv[2]).touch(); time.sleep(20)'
            actual_popen=r.subprocess.Popen
            def ready_popen(*args,**kwargs):
                process=actual_popen(*args,**kwargs)
                try:
                    end=time.monotonic()+1
                    while not ready.exists() and time.monotonic()<end:time.sleep(.01)
                    self.assertTrue(ready.exists(),'dummy must write FIFO header before watchdog exercise')
                    return process
                except BaseException:
                    process.kill();process.wait();raise
            started=time.monotonic()
            with self.assertRaises(r.PhaseError) as caught,patch.object(r.subprocess,'Popen',side_effect=ready_popen):
                r.run([sys.executable,'-c',program,'{EVALUATOR_FIFO}',str(ready)],root,root/'run.log',
                    time.monotonic()+5,2,root,dict(os.environ),evaluator=root/'events.jsonl')
            self.assertGreaterEqual(time.monotonic()-started,2)
            self.assertTrue(caught.exception.clean_timeout)
            lines=(root/'events.jsonl').read_text().splitlines()
            self.assertEqual(json.loads(lines[0]),{'type':'LOG_HEADER'})
            self.assertEqual(json.loads(lines[-1]),{'type':'CLEANUP_MARKER'})
            self.assertEqual(caught.exception.result['cleanup_errors'],[])

    def test_owned_group_reap_preserves_leader_status_after_poll_race(self):
        class StopBeforeRead:
            def poll(self, end):
                raise TimeoutError('force cleanup before the runtime polls the leader')

            def feed(self, stream, chunk, final=False):
                return chunk

            def summary(self):
                return {'status': 'TEST_FORCED_TIMEOUT'}

        for mode, expected in (('exit', 23), ('signal', -signal.SIGKILL)):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                ready, release = root/'ready', root/'release'
                program = '''import os, signal, sys, time
from pathlib import Path
ready, release = map(Path, sys.argv[1:3])
def terminate(*_):
    while not release.exists():
        time.sleep(.001)
    if sys.argv[3] == 'signal':
        os.kill(os.getpid(), signal.SIGKILL)
    os._exit(23)
signal.signal(signal.SIGTERM, terminate)
ready.touch()
while True:
    signal.pause()
'''
                actual_popen = r.subprocess.Popen
                observed = []

                def synchronized_popen(*args, **kwargs):
                    process = actual_popen(*args, **kwargs)
                    try:
                        end = time.monotonic()+2
                        while not ready.exists() and time.monotonic() < end:
                            time.sleep(.001)
                        self.assertTrue(ready.exists(), 'child must install its TERM gate')
                        actual_poll = process.poll

                        def poll_before_exit():
                            value = actual_poll()
                            if not observed:
                                self.assertIsNone(value)
                                release.touch()
                                # Observe a real exit without reaping it. The immediately
                                # following group wait must own this exact wait status.
                                end = time.monotonic()+2
                                exited = None
                                while exited is None and time.monotonic() < end:
                                    exited = os.waitid(os.P_PID, process.pid,
                                        os.WEXITED | os.WNOWAIT | os.WNOHANG)
                                    if exited is None:
                                        time.sleep(.001)
                                self.assertIsNotNone(exited, 'child must reach its real exit')
                                observed.append(exited)
                            return value

                        process.poll = poll_before_exit
                        return process
                    except BaseException:
                        process.kill(); process.wait()
                        raise

                with self.assertRaises(r.PhaseError) as caught, \
                        patch.object(r.subprocess, 'Popen', side_effect=synchronized_popen):
                    r.run([sys.executable, '-c', program, str(ready), str(release), mode],
                        root, root/'run.log', time.monotonic()+8, 8, root, dict(os.environ),
                        observer=StopBeforeRead(), restricted_output=True, reap_orphans=True)
                receipt = caught.exception.result
                self.assertEqual(len(observed), 1)
                self.assertEqual(observed[0].si_pid, receipt['pid'])
                self.assertEqual(observed[0].si_status, abs(expected))
                self.assertEqual(receipt['failure_kind'], 'timeout', receipt)
                self.assertEqual(receipt['error'], 'TIMEOUT')
                self.assertEqual(receipt['exit'], expected)
                self.assertEqual(receipt['cleanup_errors'], [])
                self.assertEqual(receipt['evidence_errors'], [])
                self.assertEqual(json.loads((root/'run.log.result.json').read_text())['exit'], expected)
                with self.assertRaises(ProcessLookupError):
                    os.killpg(receipt['pgid'], 0)

    def test_cap_is_checked_before_write_and_not_allowed_summary(self):
        with tempfile.TemporaryDirectory() as directory,patch.object(r,'WRITE_CAP',512):
            root=Path(directory)
            with self.assertRaises(r.PhaseError) as caught:
                r.run([sys.executable,'-c','print("x"*10000)'],root,root/'run.log',
                    time.monotonic()+5,3,root,dict(os.environ))
            self.assertFalse(caught.exception.clean_timeout)
            self.assertLess((root/'run.log').stat().st_size,512)
            self.assertTrue((root/'run.log.result.json').exists())
            with patch.object(r,'run') as spawn:
                self.assertEqual(r.official_summaries('codeql',root/'missing',root,root,
                    time.monotonic()+5,{},caught.exception)['status'],'NOT_RUN_UNSAFE_FAILURE')
                spawn.assert_not_called()

    def test_existing_fifo_is_not_deleted_or_spawned(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);foreign=root/'events.fifo';foreign.write_text('foreign')
            with self.assertRaises(r.PhaseError),patch.object(r.subprocess,'Popen') as spawn:
                r.run(['not-run'],root,root/'run.log',time.monotonic()+3,1,root,{},evaluator=root/'events.jsonl')
            spawn.assert_not_called();self.assertEqual(foreign.read_text(),'foreign')

    def test_no_budget_means_zero_spawn(self):
        with tempfile.TemporaryDirectory() as directory,patch.object(r.subprocess,'Popen') as spawn:
            root=Path(directory)
            with self.assertRaises(r.PhaseError):r.run(['not-run'],root,root/'run.log',0,1,root,{})
            spawn.assert_not_called()

    def test_summary_failure_has_no_subsequent_summary(self):
        failure=r.PhaseError({'failure_kind':'unsafe','cleanup_errors':[]})
        with tempfile.TemporaryDirectory() as directory,patch.object(r,'run',side_effect=failure) as spawn:
            root=Path(directory);value=r.official_summaries('cli',root/'e.jsonl',root,root,time.monotonic()+10,{})
            self.assertEqual(spawn.call_count,1);self.assertEqual(value['overall']['status'],'UNKNOWN')
        unsafe=r.PhaseError({'failure_kind':'timeout','cleanup_errors':['unreaped']})
        with patch.object(r,'run') as spawn:
            r.official_summaries('cli',Path('x'),Path('.'),Path('.'),time.monotonic()+10,{},unsafe)
            spawn.assert_not_called()

    def test_proc_start_identity_and_sampled_peak_not_pid_alias(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);folder=root/'123';folder.mkdir()
            def write(start,cpu,rss,pgid=42):
                fields=['0']*22;fields[0]='R';fields[2]=str(pgid);fields[11]=str(cpu);fields[12]='2';fields[19]=str(start);fields[21]=str(rss)
                (folder/'stat').write_text('123 (worker name) '+' '.join(fields))
                (folder/'io').write_text('read_bytes: 7\nwrite_bytes: 9\n')
            sampler=r.GroupSamples(42,root);write(100,3,2);sampler.sample();write(101,8,1);sampler.sample()
            value=sampler.result();self.assertEqual(len(value['processes']),2)
            self.assertEqual(value['observed_peak_group_rss_bytes'],2*os.sysconf('SC_PAGE_SIZE'))
            write(102,99,9,77);sampler.sample();self.assertEqual(len(sampler.identities),2)

    def test_proc_reuse_between_stat_and_io_is_discarded_unknown(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);(root/'123').mkdir()
            fields=['0']*22;fields[0]='R';fields[2]='42';fields[11]='10';fields[12]='2';fields[19]='100';fields[21]='2'
            first='123 (worker) '+' '.join(fields);fields[19]='101';second='123 (other) '+' '.join(fields)
            sampler=r.GroupSamples(42,root)
            with patch.object(Path,'read_text',side_effect=[first,'read_bytes: 999\n',second]):value=sampler.sample()
            self.assertEqual(sampler.identities,{})
            self.assertIsNone(value['group_rss_bytes'])
            self.assertIsNone(sampler.result()['observed_group_own_cpu_seconds'])
            self.assertIsNone(sampler.result()['observed_peak_group_rss_bytes'])
            with patch.object(Path,'read_text',side_effect=OSError('proc vanished')):sampler.sample()
            self.assertEqual(sampler.result()['counter_validity'],'UNKNOWN')
        self.assertIsNone(r.GroupSamples(42,Path('/nonexistent/diagnostic-proc')).result()['observed_peak_group_rss_bytes'])

    def test_257_identity_saturation_is_incomplete_not_process_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);folder=root/'123';folder.mkdir()
            sampler=r.GroupSamples(42,root)
            def write(start,cpu,pgid=42):
                fields=['0']*22;fields[0]='R';fields[2]=str(pgid);fields[11]=str(cpu);fields[19]=str(start);fields[21]='2'
                (folder/'stat').write_text('123 (worker) '+' '.join(fields))
                (folder/'io').write_text('read_bytes: 7\n')
            for start in range(257):
                write(start,1);sampler.sample()
            value=sampler.result()
            self.assertEqual(len(value['processes']),256)
            self.assertEqual(value['skipped_identity_observations'],1)
            self.assertEqual(value['counter_validity'],'INCOMPLETE_IDENTITY_SATURATION')
            self.assertIsNone(value['observed_group_own_cpu_seconds'])
            self.assertGreater(value['observed_tracked_identity_cpu_seconds'],0)
            write(0,99);sampler.sample()
            self.assertEqual(sampler.identities['123:0:42']['cpu_ticks_observed_max'],99)
            write(999,999,77);sampler.sample()
            self.assertEqual(len(sampler.identities),256)
            self.assertEqual(sampler.skipped_identity_observations,1)
            self.assertEqual(sampler.result()['observed_peak_group_rss_bytes'],2*os.sysconf('SC_PAGE_SIZE'))

    def test_terminal_reserve_frozen_size_admission(self):
        value=r.admit_terminal_snapshots({'source':'f'*64},{'library':'e'*64})
        self.assertLessEqual(value['required_bytes'],r.CAP-r.WRITE_CAP)
        with self.assertRaises(RuntimeError):r.admit_terminal_snapshots({'huge':'x'*(1024**2)},{})

    def test_shared_event_dedup_does_not_sum_appearances(self):
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'events'
            events=[{'type':'LOG_HEADER'},{'type':'PREDICATE_STARTED','eventId':1,'raHash':'hash','predicateName':'p','nanoTime':10,'appearsAs':{'p':[1,2,3]}},
                {'type':'PREDICATE_COMPLETED','startEvent':1,'nanoTime':20,'resultSize':7},{'type':'LOG_FOOTER'}]
            path.write_text('\n'.join(json.dumps(e) for e in events))
            value=r.stage_observation(path);self.assertEqual(len(value['completed_once_per_event']),1)
            self.assertEqual(value['completed_once_per_event'][0]['elapsed_ns'],10)
            path.write_text('not-json');self.assertEqual(r.stage_observation(path)['status'],'UNKNOWN_SCHEMA')


class SharedHelper(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory(); self.addCleanup(directory.cleanup)
        self.root = Path(directory.name).resolve()
        self.original, self.private = self.root / 'official', self.root / 'private'
        self.original.mkdir()
        manifest = self.original / 'qlpack.yml'
        manifest.write_text('name: codeql/typeinference\nversion: 0.0.42\nlibrary: true\ndependencies: {}\n')
        self.file = 'codeql/typeinference/internal/TypeInference.qll'
        (self.original / self.file).parent.mkdir(parents=True)
        (self.original / self.file).write_bytes(b'official helper\n')
        (self.original / 'Support.qll').write_bytes(b'unchanged\n')
        self.patch_file = self.root / 'helper.patch'; self.patch_file.write_bytes(b'fixed fixture patch\n')
        self.entry = dict(m.pack_manifest(manifest), root=str(self.original))
        self.calls = []
        for name, path in [('SHARED_OFFICIAL_SHA', self.original / self.file), ('SHARED_PATCH_SHA', self.patch_file)]:
            patcher = patch.object(m, name, r.digest(path)); patcher.start(); self.addCleanup(patcher.stop)
        patcher = patch.object(m, 'SHARED_CANDIDATE_SHA', hashlib.sha256(b'private helper\n').hexdigest())
        patcher.start(); self.addCleanup(patcher.stop)

    def snapshot(self, root):
        result = {}
        for path in sorted(root.rglob('*')):
            if path.is_symlink() or not (path.is_dir() or path.is_file()): raise RuntimeError('unsafe fixture')
            if path.is_file():
                result[path.relative_to(root).as_posix()] = {'sha256': r.digest(path), 'mode': path.stat().st_mode & 0o111}
        return result

    def apply_patch(self, argv, label, timeout):
        self.calls.append((list(map(str, argv)), label, timeout))
        self.assertEqual(list(map(str, argv)), ['patch', '--batch', '--fuzz=0', str(self.private / self.file), str(self.patch_file)])
        self.assertEqual((label, timeout), ('apply-shared-helper-patch', 20))
        (self.private / self.file).write_bytes(b'private helper\n')

    def prepare(self):
        return m.prepare_shared_helper(self.entry, self.private, self.patch_file,
                                      snapshot=self.snapshot, copy_file=shutil.copy2, file_digest=r.digest, run=self.apply_patch)

    def test_fixed_patch_preserves_complete_official_pack_and_mode(self):
        before = self.snapshot(self.original)
        identity = self.prepare()
        self.assertEqual(self.snapshot(self.original), before)
        self.assertEqual(identity['official_files'], before)
        self.assertEqual(identity['changed_files'], [self.file])
        self.assertEqual(identity['candidate_files'][self.file]['sha256'], m.SHARED_CANDIDATE_SHA)
        self.assertEqual(identity['candidate_files'][self.file]['mode'], before[self.file]['mode'])
        self.assertEqual(set(identity), {'upstream_commit', 'patch_sha256', 'changed_files', 'official_files', 'candidate_files'})
        self.assertEqual(len(self.calls), 1)

    def test_saved_snapshot_validation_preserves_the_legacy_contract(self):
        identity = self.prepare()
        official = self.snapshot(self.original)
        saved = json.loads(json.dumps(identity['candidate_files']))
        self.assertEqual(m.shared_snapshot_identity(official, saved), identity)
        saved['Support.qll']['sha256'] = hashlib.sha256(b'forged intermediate').hexdigest()
        with self.assertRaisesRegex(RuntimeError, 'fixed single-file helper patch'):
            m.shared_snapshot_identity(official, saved)
        self.assertEqual(self.snapshot(self.original), official)

    def test_wrong_patch_base_or_manifest_never_spawns_patch(self):
        for fault in ('patch', 'base', 'version', 'manifest'):
            with self.subTest(fault=fault):
                original_patch = self.patch_file.read_bytes()
                original_base = (self.original / self.file).read_bytes()
                original_entry = dict(self.entry)
                try:
                    if fault == 'patch': self.patch_file.write_bytes(b'changed patch')
                    elif fault == 'base': (self.original / self.file).write_bytes(b'changed base')
                    elif fault == 'version': self.entry['version'] = '0.0.43'
                    else: self.entry['manifest_sha256'] = '0' * 64
                    with self.assertRaises(RuntimeError): self.prepare()
                    self.assertEqual(self.calls, [])
                    self.assertFalse(self.private.exists())
                finally:
                    self.patch_file.write_bytes(original_patch)
                    (self.original / self.file).write_bytes(original_base)
                    self.entry = original_entry

    def test_private_difference_cannot_expand_or_fall_back_to_stock(self):
        self.prepare()
        target = self.private / self.file
        for fault in ('stock', 'extra', 'removed', 'support', 'mode', 'symlink'):
            with self.subTest(fault=fault):
                original = {p: (self.private / p).read_bytes() for p in self.snapshot(self.private)}
                mode = target.stat().st_mode
                try:
                    if fault == 'stock': target.write_bytes(b'official helper\n')
                    elif fault == 'extra': (self.private / 'Extra.qll').write_bytes(b'extra')
                    elif fault == 'removed': (self.private / 'Support.qll').unlink()
                    elif fault == 'support': (self.private / 'Support.qll').write_bytes(b'changed')
                    elif fault == 'mode': target.chmod(mode ^ 0o100)
                    else: (self.private / 'Extra.qll').symlink_to(self.original / self.file)
                    with self.assertRaises(RuntimeError):
                        m.shared_helper_identity(self.original, self.private, snapshot=self.snapshot)
                finally:
                    (self.private / 'Extra.qll').unlink(missing_ok=True)
                    for path, data in original.items(): (self.private / path).write_bytes(data)
                    target.chmod(mode)

    def test_copy_does_not_admit_overlapping_or_existing_private_roots(self):
        for private in (self.original, self.original / 'nested', self.root):
            with self.subTest(private=private), self.assertRaises(RuntimeError):
                m.prepare_shared_helper(self.entry, private, self.patch_file,
                                        snapshot=self.snapshot, copy_file=shutil.copy2, file_digest=r.digest, run=self.apply_patch)
        self.assertEqual(self.calls, [])


class OfficialInputs(unittest.TestCase):
    def test_real_saved_official_manifest_closure_is_exact(self):
        # Small fixture uses the official manifest grammar and separate bundle versions.
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            for version,rust in [('2.27.2','0.2.23'),('2.27.3','0.2.24'),('2.27.4','0.2.25')]:
                bundle=root/version
                for name,v,deps in [('rust-all',rust,'\n  codeql/util: 1.0.0'),('util','1.0.0',' {}')]:
                    folder=bundle/'qlpacks/codeql'/name/v;folder.mkdir(parents=True)
                    (folder/'qlpack.yml').write_text(f'library: true\nname: codeql/{name}\nversion: {v}\ndependencies:{deps}\n')
                closure=m.bundle_closure(bundle,version)
                self.assertEqual(closure['codeql/rust-all']['version'],rust)
                self.assertTrue(all(str(bundle) in p['root'] for p in closure.values()))
            bad=root/'bad.yml';bad.write_text('library: true\nname: codeql/x\nversion: 1.0.0\ndependencies:\n  codeql/y: ^1.0.0\n')
            with self.assertRaises(RuntimeError):m.pack_manifest(bad)

    def test_actual_resolver_rejects_mixed_bundle_or_unknown_schema(self):
        closure={'codeql/rust-all':{'root':'/bundle/rust-all','version':'0.2.23'}}
        value={'libraryPath':['/query','/bundle/rust-all'],'dbscheme':'/bundle/rust-all/rust.dbscheme',
            'compilationCache':[],'relativeName':'oasis7/rust-model-diagnostic/Test.ql','qlPackName':'oasis7/rust-model-diagnostic'}
        m.verify_library_path(value,Path('/query'),'Test',closure)
        for changed in [dict(value,libraryPath=['/query','/other/rust-all']),dict(value,extra=True)]:
            with self.assertRaises(RuntimeError):m.verify_library_path(changed,Path('/query'),'Test',closure)

    def test_capacity_bundle_is_not_admitted_by_legacy_model_cli(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            result=subprocess.run([sys.executable,str(P),'--codeql',str(root/'codeql'),'--bundle-version','2.27.4',
                '--business-source',str(root/'source'),'--output',str(root/'out'),'--work',str(root/'work'),
                '--deadline',str(time.time()+300)],capture_output=True,text=True,timeout=10)
            self.assertEqual(result.returncode,2)
            self.assertIn("invalid choice: '2.27.4'",result.stderr)
            self.assertFalse((root/'out').exists())
            self.assertFalse((root/'work').exists())


class Workflow(unittest.TestCase):
    def assert_diagnostic_job_contract(self, job):
        self.assertIn("github.event_name == 'workflow_dispatch' && inputs.run_mode == 'codeql_rust_model_diagnostic'",job)
        self.assertIn('timeout-minutes: 20',job)
        self.assertIn('permissions:\n      contents: read',job)
        for forbidden in ('security-events','GH_TOKEN','GITHUB_TOKEN','upload-sarif','codeql/analyze','--admin','continue-on-error'):
            self.assertNotIn(forbidden,job)

    def test_native_extraction_uses_mold_within_the_fixed_preparation_budget(self):
        text=(P.resolve().parents[2]/'.github/workflows/rust.yml').read_text()
        job=workflow_job(text, 'codeql-rust-model-diagnostic')
        steps=re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
        setups=[i for i, step in enumerate(steps)
                if 'uses: ./.github/actions/setup-linux-mold' in step]
        self.assertEqual(len(setups), 1)
        setup=setups[0]
        # Rust extraction can compile build.rs and procedural macros even with
        # build-mode=none; the QL-only plan must retain its no-Rust preparation.
        self.assertIn("if: inputs.model_probe != 'trait-scope-plan'\n", steps[setup])
        self.assertIn('MODEL_DEADLINE=', steps[0])
        self.assertLess(0, setup)
        self.assertIn('ref: ${{ github.workflow_sha }}', steps[setup-1])
        self.assertIn('sparse-checkout: .github/actions/setup-linux-mold\n', steps[setup-1])
        self.assertNotIn('path:', steps[setup-1])
        self.assertIn('ref: ${{ github.workflow_sha }}', steps[setup+1])
        self.assertNotIn('sparse-checkout:', steps[setup+1])
        self.assertNotIn('path:', steps[setup+1])
        self.assertIn('ref: 49b25e1ab631ffb433bce88709fa5601720cfc17\n', steps[setup+2])
        self.assertIn('path: model-source\n', steps[setup+2])
        for checkout in (steps[setup-1], steps[setup+1], steps[setup+2]):
            self.assertIn('uses: actions/checkout@', checkout)
            self.assertIn('persist-credentials: false\n', checkout)
        for boundary in ('Set up selected immutable Rust CodeQL bundle',
                         'Diagnose selected public-model partition'):
            self.assertLess(setup, next(i for i, step in enumerate(steps) if boundary in step))

    def test_literal_manual_has_no_scan_or_required_override(self):
        root=P.resolve().parents[2];text=(root/'.github/workflows/rust.yml').read_text()
        job=workflow_job(text, 'codeql-rust-model-diagnostic')
        self.assert_diagnostic_job_contract(job)
        # Evaluate the real selection guard instead of requiring its historical
        # one-line spelling. Schedule-only admission may precede select.
        condition=re.search(r'^    if: (.*?)(?=^    [a-z-]+:|\Z)',
                            workflow_job(text, 'select'), re.M | re.S)[1]
        condition=condition.strip().removeprefix('>-').strip()
        for mode, expected in (('codeql_rust_model_diagnostic', False), ('full', True)):
            expression=condition
            values={'always()':True, 'cancelled()':False, 'github.event_name':'workflow_dispatch',
                    'inputs.run_mode':mode, 'needs.periodic-admission.result':'skipped',
                    'needs.periodic-admission.outputs.run_full':''}
            for token, value in values.items(): expression=expression.replace(token, repr(value))
            expression=re.sub(r'!(?!=)', 'not ', expression.replace('&&', ' and ').replace('||', ' or '))
            with self.subTest(mode=mode):
                self.assertEqual(bool(eval(' '.join(expression.split()), {'__builtins__':{}}, {})), expected)
        self.assertIn(
            "if: >-\n"
            "      always() && (github.event_name == 'push' ||\n"
            "      (github.event_name == 'pull_request' &&\n"
            "       (github.event.action != 'labeled' ||\n"
            "        (github.event.label.name == 'ci-priority' &&\n"
            "         github.event.pull_request.head.repo.full_name == github.repository &&\n"
            "         !github.event.pull_request.draft))))",
            text,
        )
        self.assertEqual(m.MODEL_BUNDLE_VERSIONS,('2.27.2','2.27.3'))
        for version in m.MODEL_BUNDLE_VERSIONS:self.assertIn(version,job);self.assertIn(m.BUNDLES[version]['sha256'],job)
        self.assertIn(' + 1080 ',job);self.assertIn('64 * 1024**2',job)
        for test in ('codeql-rust-model-diagnostic.test.py','codeql-rust-refactored-source.test.py'):
            self.assertIn('run python3 ./scripts/security/'+test,(root/'scripts/ci-tests.sh').read_text())

    def test_job_boundary_excludes_neighbors_but_keeps_own_security_checks(self):
        text=(P.resolve().parents[2]/'.github/workflows/rust.yml').read_text()
        job=workflow_job(text, 'codeql-rust-model-diagnostic')
        for neighbor in ('periodic-admission', 'different_job', 'select'):
            fixture=('jobs:\n  codeql-rust-model-diagnostic:\n' + job
                     + '\n  ' + neighbor + ':\n    env:\n      GH_TOKEN: neighbor-only\n')
            with self.subTest(neighbor=neighbor):
                extracted=workflow_job(fixture, 'codeql-rust-model-diagnostic')
                self.assertEqual(extracted.strip(), job.strip())
                self.assert_diagnostic_job_contract(extracted)
        for forbidden in ('security-events','GH_TOKEN','GITHUB_TOKEN','upload-sarif',
                          'codeql/analyze','--admin','continue-on-error'):
            with self.subTest(forbidden=forbidden), self.assertRaises(AssertionError):
                self.assert_diagnostic_job_contract(job + '\n    probe: ' + forbidden + '\n')
        for invalid in ('jobs:\n  other:\n    runs-on: ubuntu-24.04\n',
                        'jobs:\n  codeql-rust-model-diagnostic:\n' + job
                        + '\n  codeql-rust-model-diagnostic:\n' + job):
            with self.assertRaises(ValueError): workflow_job(invalid, 'codeql-rust-model-diagnostic')


if __name__=='__main__':unittest.main()
