#!/usr/bin/env python3
"""Candidate observation contracts with real pipes, seals and owned children."""
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

import codeql_rust_candidate_profile as profile
import codeql_rust_diagnostic_runtime as runtime

HERE = Path(__file__).resolve().parent
# Public synthetic input: fake producer source may contain it; observed evidence must not.
SYNTHETIC_CANARY = 'RAW_CANARY_NEVER_PERSIST_937542'
HEADER = {'type': 'LOG_HEADER', 'logVersion': '0.5.0', 'codeqlVersion': '2.27.4'}
FOOTER = {'type': 'LOG_FOOTER'}
RAW = (json.dumps(HEADER) + '\n' + json.dumps({'type': 'PREDICATE_STARTED', 'predicateName': SYNTHETIC_CANARY})
       + '\n' + json.dumps(FOOTER)).encode()
QUERY = dict(path='/frozen/Query.ql', relative='Query.ql', pack='fixture/profile', sha256='a'*64)


class CaptureTests(unittest.TestCase):
    def capture(self, *args):
        item = profile.Capture(*args)
        self.addCleanup(item.close)
        return item

    def test_real_memfd_seals_identity_and_last_object_without_newline(self):
        capture = self.capture()
        capture.feed(RAW[:23]); capture.feed(RAW[23:]); capture.finish(True, True)
        report = capture.summary()
        self.assertEqual(report['status'], 'CAPTURED')
        self.assertEqual(report['log_boundary_status'], 'COMPLETE')
        self.assertEqual(report['retained_bytes'], len(RAW))
        self.assertEqual(report['retained_sha256'], hashlib.sha256(RAW).hexdigest())
        self.assertEqual(Path(capture.path).read_bytes(), RAW)
        with self.assertRaises(OSError): os.write(capture.fd, b'x')
        with self.assertRaises(OSError): os.ftruncate(capture.fd, 0)
        self.assertFalse(os.get_inheritable(capture.fd))

    def test_memory_overflow_keeps_exact_prefix_and_observed_drain_hash(self):
        capture = self.capture(4)
        capture.feed(b'abcd')
        capture.feed(b'ef')
        capture.feed(b'gh'); capture.finish(True, True)
        report = capture.summary()
        self.assertEqual(report['status'], 'INCOMPLETE')
        self.assertEqual(report['error'], 'MEMORY_LIMIT')
        self.assertEqual(report['retained_bytes'], 4)
        self.assertEqual(report['observed_bytes'], 8)
        self.assertEqual(report['discarded_bytes'], 4)
        self.assertEqual(report['retained_sha256'], hashlib.sha256(b'abcd').hexdigest())
        self.assertEqual(report['observed_sha256'], hashlib.sha256(b'abcdefgh').hexdigest())
        self.assertEqual(Path(capture.path).read_bytes(), b'abcd')

    def test_crossing_memory_limit_never_resumes_retention_for_smaller_chunks(self):
        capture = self.capture(4)
        capture.feed(b'abc'); capture.feed(b'de'); capture.feed(b'f')
        capture.finish(True, True)
        report = capture.summary()
        self.assertEqual(report['error'], 'MEMORY_LIMIT')
        self.assertEqual(report['retained_bytes'], 3)
        self.assertEqual(report['discarded_bytes'], 3)
        self.assertEqual(report['observed_sha256'], hashlib.sha256(b'abcdef').hexdigest())
        self.assertEqual(Path(capture.path).read_bytes(), b'abc')
        self.assertEqual(report['log_boundary_status'], 'PARTIAL')

    def test_positive_short_write_reports_the_actual_retained_identity(self):
        capture = self.capture()
        real_write = os.write
        with patch.object(os, 'write', side_effect=lambda fd, data: real_write(fd, data[:5])):
            with self.assertRaises(profile.ObservationError): capture.feed(RAW)
        capture.finish(True, True)
        report = capture.summary()
        self.assertEqual(report['retained_bytes'], 5)
        self.assertEqual(report['retained_sha256'], hashlib.sha256(RAW[:5]).hexdigest())
        self.assertEqual(Path(capture.path).read_bytes(), RAW[:5])
        self.assertEqual(report['error'], 'SHORT_MEMORY_WRITE')

    def test_missing_footer_truncation_duplicate_header_and_bad_version_are_partial(self):
        for raw in (json.dumps(HEADER).encode(), RAW[:-1], json.dumps(HEADER).encode()+b'\n'+RAW,
                    RAW.replace(b'0.5.0', b'0.4.0')):
            with self.subTest(raw_sha256=hashlib.sha256(raw).hexdigest()):
                capture = self.capture(); capture.feed(raw); capture.finish(True, True)
                self.assertEqual(capture.summary()['log_boundary_status'], 'PARTIAL')
                capture.close()

    def test_false_eof_or_cleanup_cannot_be_reported_as_complete(self):
        for eof, cleanup in ((False, True), (True, False)):
            capture = self.capture(); capture.feed(RAW); capture.finish(eof, cleanup)
            self.assertEqual(capture.summary()['status'], 'INCOMPLETE')
            capture.close()


class OwnedProcessTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name); self.out = self.root / 'evidence'; self.out.mkdir()
        self.cap = profile.Capture(); self.addCleanup(self.cap.close)
        self.saved_caps = runtime.CAP, runtime.WRITE_CAP
        runtime.CAP, runtime.WRITE_CAP = 64*1024**2, 63*1024**2
        self.addCleanup(self.restore_caps)

    def restore_caps(self): runtime.CAP, runtime.WRITE_CAP = self.saved_caps

    def child(self, body):
        path = self.root / 'child.py'; path.write_text(body)
        return [sys.executable, str(path), '{EVALUATOR_FIFO}']

    def execute(self, body, timeout=8):
        return runtime.run(self.child(body), self.root, self.out / 'batch.log', time.monotonic()+10,
                           timeout, self.out, os.environ.copy(), file_limit=None,
                           evaluator=self.root/'events.jsonl', evaluator_capture=self.cap,
                           observer=profile.Progress([QUERY]), restricted_output=True)

    def assert_private(self):
        self.assertFalse((self.root/'events.jsonl').exists())
        self.assertFalse((self.root/'events.fifo').exists())
        for path in self.out.rglob('*'):
            if path.is_file(): self.assertNotIn(SYNTHETIC_CANARY.encode(), path.read_bytes(), str(path))

    def test_interleaved_three_streams_never_persist_raw_text(self):
        body = ('import os,sys\n'
                f'fd=os.open(sys.argv[1],os.O_WRONLY);os.write(fd,{RAW!r});os.close(fd)\n'
                f'os.write(1,{(SYNTHETIC_CANARY+chr(10)).encode()!r})\n'
                f'os.write(2,{(SYNTHETIC_CANARY+chr(10)).encode()!r})\n'
                'os.write(2,b"Compiling query plan for /frozen/Query.ql.\\n")\n'
                'os.write(2,b"[1/1 comp 2ms] Compiled /frozen/Query.ql.\\n")\n')
        result = self.execute(body)
        self.assertEqual(result['exit'], 0)
        self.assertEqual(result['evaluator_capture']['log_boundary_status'], 'COMPLETE')
        self.assertEqual(result['output_observation']['events']['COMPILE_DONE'], 1)
        self.assertTrue(all(v['final'] for v in result['output_observation']['streams'].values()))
        self.assert_private()

    def test_memory_limit_drains_beyond_pipe_capacity_and_preserves_normal_exit(self):
        self.cap.close(); self.cap = profile.Capture(4096); self.addCleanup(self.cap.close)
        prefix = (json.dumps(HEADER)+'\n').encode()
        event = (json.dumps(dict(type='PREDICATE_STARTED', predicateName=SYNTHETIC_CANARY,
                                payload='x'*1024))+'\n').encode()
        suffix = json.dumps(FOOTER).encode()
        raw = prefix + event*256 + suffix
        completed = self.root/'producer-completed'
        body = ('import os,sys\nfrom pathlib import Path\nfd=os.open(sys.argv[1],os.O_WRONLY)\n'
                f'os.write(fd,{prefix!r})\n'
                f'for _ in range(256): os.write(fd,{event!r})\n'
                f'os.write(fd,{suffix!r});os.close(fd)\n'
                f'Path({str(completed)!r}).write_text("completed")\n')
        result = self.execute(body)
        self.assertEqual(result['exit'], 0)
        self.assertIsNone(result['execution_failure_kind'])
        self.assertFalse(result['cleanup_errors'])
        self.assertEqual(completed.read_text(), 'completed')
        report = result['evaluator_capture']
        self.assertEqual(report['status'], 'INCOMPLETE')
        self.assertEqual(report['error'], 'MEMORY_LIMIT')
        self.assertTrue(report['transport_eof'])
        self.assertTrue(report['cleanup_ok'])
        self.assertEqual(report['observed_bytes'], len(raw))
        self.assertEqual(report['observed_sha256'], hashlib.sha256(raw).hexdigest())
        self.assertLessEqual(report['retained_bytes'], 4096)
        self.assertEqual(report['discarded_bytes'], len(raw)-report['retained_bytes'])
        self.assertEqual(Path(self.cap.path).read_bytes(), raw[:report['retained_bytes']])
        self.assert_private()

    def test_memory_limit_does_not_replace_a_later_child_exit_failure(self):
        self.cap.close(); self.cap = profile.Capture(4); self.addCleanup(self.cap.close)
        body = ('import os,sys\nfd=os.open(sys.argv[1],os.O_WRONLY)\n'
                f'os.write(fd,{RAW!r});os.close(fd)\nsys.exit(7)\n')
        with self.assertRaises(runtime.PhaseError) as caught:
            self.execute(body)
        result = caught.exception.result
        self.assertEqual(result['exit'], 7)
        self.assertEqual(result['execution_failure_kind'], 'child_exit')
        self.assertEqual(result['evaluator_capture']['error'], 'MEMORY_LIMIT')
        self.assertFalse(result['cleanup_errors'])
        self.assert_private()

    def test_timeout_preserves_term_handler_footer_and_original_failure(self):
        prefix = (json.dumps(HEADER)+'\n').encode()
        body = ('import os,sys,signal,time\nfd=os.open(sys.argv[1],os.O_WRONLY)\n'
                f'os.write(fd,{prefix!r})\n'
                'def stop(*_):\n'
                f' os.write(fd,{json.dumps(FOOTER).encode()!r});os.close(fd);sys.exit(0)\n'
                'signal.signal(signal.SIGTERM,stop)\ntime.sleep(60)\n')
        with self.assertRaises(runtime.PhaseError) as caught: self.execute(body, timeout=4.1)
        result = caught.exception.result
        self.assertEqual(result['execution_failure_kind'], 'timeout')
        self.assertTrue(caught.exception.clean_timeout)
        self.assertEqual(result['evaluator_capture']['log_boundary_status'], 'COMPLETE')
        self.assert_private()

    def test_parser_exception_does_not_escape_through_error_receipts(self):
        class Reject(profile.Lines):
            def line(self, name, data):
                json.loads(data)  # the resulting exception must not expose its input
        argv = self.child(f'import os,sys\nos.write(1,{(SYNTHETIC_CANARY+chr(10)).encode()!r})\n')
        with self.assertRaises(runtime.PhaseError) as caught:
            runtime.run(argv[:-1], self.root, self.out/'reject.log', time.monotonic()+8, 8,
                        self.out, os.environ.copy(), observer=Reject(), restricted_output=True)
        self.assertEqual(caught.exception.result['failure_kind'], 'unsafe')
        self.assert_private()

    def test_existing_fifo_is_not_deleted_and_no_child_is_started(self):
        fifo = self.root/'events.fifo'; os.mkfifo(fifo)
        with self.assertRaises(runtime.PhaseError) as caught: self.execute('raise RuntimeError("must not start")\n')
        self.assertIsNone(caught.exception.result['pid'])
        self.assertTrue(fifo.exists()); fifo.unlink()

    def test_final_receipt_crossing_end_is_failure(self):
        real_save = runtime.save
        deadline = time.monotonic()+4
        def late_save(path, value, root, terminal=False):
            real_save(path, value, root, terminal)
            if path.name.endswith('.result.json'):
                clock[0] = deadline+1
        clock = [time.monotonic()]
        with patch.object(runtime, 'save', side_effect=late_save), patch.object(runtime.time, 'monotonic', side_effect=lambda: clock[0]):
            with self.assertRaises(runtime.PhaseError) as caught:
                runtime.run([sys.executable,'-c','pass'],self.root,self.out/'late.log',deadline,4,
                            self.out,os.environ.copy(),observer=profile.Lines(),restricted_output=True)
        self.assertEqual(caught.exception.result['error'], 'RECEIPT_EXCEEDED_FIXED_DEADLINE')
        self.assertEqual(json.loads((self.out/'late.log.result.json').read_text())['failure_kind'], 'unsafe')

    def test_sigterm_and_repeat_interrupt_close_owned_process_and_memfd(self):
        child = self.root/'waiting-child.py'
        child.write_text('import os,sys,time\nfd=os.open(sys.argv[1],os.O_WRONLY)\n'
                         f'os.write(fd,{(json.dumps(HEADER)+chr(10)).encode()!r})\n'
                         'open(sys.argv[2],"w").write(str(os.getpid()))\ntime.sleep(60)\n')
        supervisor = self.root/'supervisor.py'
        supervisor.write_text('import sys,os,time,json\nfrom pathlib import Path\n'
            f'sys.path.insert(0,{str(HERE)!r})\nimport codeql_rust_candidate_profile as p\nimport codeql_rust_diagnostic_runtime as r\n'
            f'root=Path({str(self.root)!r});out=root/"evidence";c=p.Capture()\n'
            'try:\n'
            ' r.run([sys.executable,str(root/"waiting-child.py"),"{EVALUATOR_FIFO}",str(root/"ready")],root,out/"cancel.log",time.monotonic()+12,12,out,os.environ.copy(),evaluator=root/"cancel-events.jsonl",evaluator_capture=c,observer=p.Lines(),restricted_output=True)\n'
            'except r.PhaseError: pass\n'
            'finally:\n'
            ' c.close();(out/"closed.json").write_text(json.dumps({"closed":c.fd is None}))\n')
        proc = subprocess.Popen([sys.executable, str(supervisor)], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.addCleanup(lambda: proc.kill() if proc.poll() is None else None)
        until = time.monotonic()+4
        while not (self.root/'ready').exists() and proc.poll() is None and time.monotonic()<until:
            time.sleep(.01)
        self.assertTrue((self.root/'ready').exists())
        proc.send_signal(signal.SIGTERM)
        time.sleep(.01)
        if proc.poll() is None: proc.send_signal(signal.SIGTERM)
        stdout, stderr = proc.communicate(timeout=5)
        self.assertEqual(proc.returncode, 0, stderr.decode(errors='replace'))
        result = json.loads((self.out/'cancel.log.result.json').read_text())
        self.assertEqual(result['failure_kind'], 'cancelled')
        self.assertFalse(result['cleanup_errors'])
        self.assertTrue(json.loads((self.out/'closed.json').read_text())['closed'])
        with self.assertRaises(ProcessLookupError): os.kill(result['pid'], 0)

    def test_sigterm_between_real_popen_and_registration_cannot_leak_child(self):
        real_popen = subprocess.Popen
        children = []
        def during_popen(*args, **kwargs):
            child = real_popen(*args, **kwargs); children.append(child)
            os.kill(os.getpid(), signal.SIGTERM)
            return child
        with patch.object(runtime.subprocess, 'Popen', side_effect=during_popen):
            with self.assertRaises(runtime.PhaseError) as caught:
                runtime.run([sys.executable,'-c','import time; time.sleep(60)'],self.root,self.out/'race.log',
                            time.monotonic()+8,8,self.out,os.environ.copy(),observer=profile.Lines(),restricted_output=True)
        self.assertEqual(caught.exception.result['failure_kind'],'cancelled')
        self.assertEqual(caught.exception.result['pid'],children[0].pid)
        self.assertIsNotNone(children[0].poll())
        with self.assertRaises(ProcessLookupError): os.kill(children[0].pid,0)

    def test_sigterm_during_last_result_write_is_not_success(self):
        real_save=runtime.save; signalled=[]
        def late_cancel(path,value,root,terminal=False):
            real_save(path,value,root,terminal)
            if path.name.endswith('.result.json') and not signalled:
                signalled.append(True);os.kill(os.getpid(),signal.SIGTERM)
        with patch.object(runtime,'save',side_effect=late_cancel):
            with self.assertRaises(runtime.PhaseError) as caught:
                runtime.run([sys.executable,'-c','pass'],self.root,self.out/'late-cancel.log',
                            time.monotonic()+8,8,self.out,os.environ.copy(),observer=profile.Lines(),restricted_output=True)
        self.assertEqual(caught.exception.result['failure_kind'],'cancelled')
        self.assertTrue(json.loads((self.out/'late-cancel.log.result.json').read_text())['cancelled'])


class BudgetTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name); self.work = self.root/'work'; self.work.mkdir()
        self.out = self.root/'out'; self.out.mkdir()
        self.caps = runtime.CAP, runtime.WRITE_CAP
        runtime.CAP, runtime.WRITE_CAP = 64*1024**2, 63*1024**2
        self.addCleanup(self.restore)

    def restore(self): runtime.CAP, runtime.WRITE_CAP = self.caps

    def observation(self):
        value = profile.Observation(self.out, self.work, [QUERY]); self.addCleanup(value.close)
        value.start(); return value

    def test_raw_memory_output_and_terminal_reservations_share_original_cap(self):
        observation = self.observation()
        self.assertEqual(runtime.CAP, 16*1024**2)
        self.assertEqual(runtime.WRITE_CAP, 15*1024**2)
        with observation.output_budget():
            self.assertEqual(runtime.CAP, 8*1024**2)
            self.assertEqual(runtime.WRITE_CAP, 7*1024**2)
        self.assertEqual(runtime.CAP, 16*1024**2)
        observation.close(); self.assertEqual(runtime.CAP, 64*1024**2)

    def test_insufficient_shared_capacity_rejects_before_memfd_allocation(self):
        with (self.out/'core').open('wb') as stream: stream.truncate(24*1024**2)
        with patch.object(profile, 'Capture') as capture:
            with self.assertRaises(profile.ObservationError): self.observation()
            capture.assert_not_called()
        self.assertEqual(runtime.CAP, 64*1024**2)

    def test_unsafe_or_low_time_never_spawns_summary(self):
        for safe, remaining, expected in ((False, 100, 'NOT_REACHED_UNSAFE_FAILURE'),
                                          (True, 19, 'NOT_REACHED_FIXED_DEADLINE')):
            observation = self.observation()
            observation.capture.feed(RAW); observation.capture.finish(True, True)
            observation.batch_safe = safe
            with patch.object(runtime, 'run') as run:
                observation.summarize('/cli', self.root, {}, time.monotonic()+remaining, admitted=True)
                run.assert_not_called()
            self.assertEqual(observation.summary_result['status'], expected)
            observation.close()
            for path in observation.out.iterdir(): path.unlink()
            observation.out.rmdir()

    def test_memory_limit_never_summarizes_the_retained_prefix(self):
        real_capture = profile.Capture
        with patch.object(profile, 'Capture', side_effect=lambda: real_capture(4)):
            observation = self.observation()
        observation.capture.feed(RAW); observation.capture.finish(True, True)
        observation.batch_safe = True
        with patch.object(runtime, 'run') as run:
            observation.summarize('/cli', self.root, {}, time.monotonic()+100, admitted=True)
            run.assert_not_called()
        self.assertEqual(observation.summary_result['status'], 'NOT_REACHED_MEMORY_LIMIT')
        self.assertEqual(observation.receipt()['status'], 'INCOMPLETE')
        self.assertEqual(runtime.CAP, 16*1024**2)
        observation.close(); self.assertEqual(runtime.CAP, 64*1024**2)


class SummaryTests(unittest.TestCase):
    def row(self, **changes):
        value = dict(raHash='physical-relation-1', predicateName=SYNTHETIC_CANARY, appearsAs={SYNTHETIC_CANARY: {QUERY['path']: [1,2]}},
                     evaluationStrategy='EXTENSIONAL', completionTime='2026-11-11T00:00:01Z',
                     dependencies={SYNTHETIC_CANARY:'dependency-id'}, millis=100, position=SYNTHETIC_CANARY,
                     queryCausingWork=QUERY['path'], resultSize=5)
        value.update(changes); return value

    def project(self, rows, *, header=None, suffix=b'', stderr=b''):
        header = header or dict(summaryLogVersion='0.4.0', codeqlVersion='2.27.4', startTime='2026-11-11T00:00:00Z')
        data = '\n'.join(json.dumps(value) for value in [header,*rows]).encode()+suffix
        observer = profile.Summary([QUERY])
        for index in range(0,len(data),7): self.assertEqual(observer.feed('stdout', data[index:index+7]), b'')
        observer.feed('stdout', b'', final=True)
        observer.feed('stderr', stderr, final=True)
        report = observer.summary()
        self.assertEqual(report['streams']['stdout']['bytes'], len(data))
        self.assertEqual(report['streams']['stdout']['sha256'], hashlib.sha256(data).hexdigest())
        self.assertNotIn(SYNTHETIC_CANARY, json.dumps(report))
        return report

    def member(self, **changes):
        value = self.row(raHash='physical-member', evaluationStrategy='IN_LAYER',
                         predicateIterationMillis=[3,-1,7], deltaSizes=[1,-1,2], mainHash='physical-layer',
                         pipelineRuns=[{'raReference':SYNTHETIC_CANARY}])
        del value['millis']; del value['dependencies']
        value.update(changes); return value

    def test_real_observed_shapes_separate_literal_time_and_known_iteration_subtotal(self):
        report = self.project([self.row(), self.member()])
        self.assertEqual(report['status'], 'PROJECTED')
        self.assertEqual(report['accepted_records'], 2)
        self.assertEqual(report['negative_iteration_markers'], 1)
        literal = report['rankings']['official_millis']; iterations = report['rankings']['known_iteration_subtotal']
        self.assertEqual(len(literal), 1); self.assertEqual(len(iterations), 1)
        self.assertEqual(literal[0]['reported_millis'], 100)
        self.assertEqual(iterations[0]['known_iteration_subtotal'], 10)
        self.assertIsNone(iterations[0]['reported_millis'])
        self.assertEqual(iterations[0]['negative_iteration_markers'], 1)
        self.assertEqual(iterations[0]['first_requiring_query'], {'index':0,'query_sha256':'a'*64})
        self.assertEqual(iterations[0]['appears_in_query_indexes'], [0])

    def test_all_unknown_iterations_are_not_zero_cost(self):
        report = self.project([self.row(),self.member(predicateIterationMillis=[-1,-1])])
        self.assertEqual(report['untimed_records'],1)
        self.assertEqual(report['negative_iteration_markers'],2)
        self.assertEqual(report['rankings']['known_iteration_subtotal'],[])

    def test_shared_metadata_and_member_are_one_timing_role(self):
        member=self.member()
        sentinel={key:member[key] for key in ('completionTime','raHash','predicateName','appearsAs')}
        sentinel.update(evaluationStrategy='SENTINEL_EMPTY',sentinelRaHash='sentinel-id')
        report=self.project([sentinel,member])
        self.assertEqual(report['accepted_records'],2)
        self.assertEqual(report['reused_ra_hash_records'],1)
        self.assertEqual(len(report['rankings']['known_iteration_subtotal']),1)

    def test_reused_ra_hash_is_not_an_event_id_and_does_not_drop_official_records(self):
        for second in (self.row(), self.row(predicateName=SYNTHETIC_CANARY+'-another-name')):
            with self.subTest(second_name=second['predicateName']):
                report=self.project([self.row(),second])
                self.assertEqual(report['status'],'PROJECTED')
                self.assertEqual(report['reused_ra_hash_records'],1)
                self.assertEqual(report['accepted_records'],2)
                ranking=report['rankings']['official_millis']
                self.assertEqual({r['record'] for r in ranking},{1,2})
                self.assertEqual([r['reported_millis'] for r in ranking],[100,100])

    def test_unknown_shape_bad_type_nested_fields_and_bad_encoding_never_escape(self):
        mutations=[dict(millis=True),dict(millis=-1),dict(extra=SYNTHETIC_CANARY),dict(raHash=None),
                   dict(dependencies={SYNTHETIC_CANARY:17}),dict(appearsAs={SYNTHETIC_CANARY:{QUERY['path']:[True]}}),
                   dict(pipelineRuns=[{'unknown':SYNTHETIC_CANARY}]),dict(position='\ud800')]
        for mutation in mutations:
            with self.subTest(fields=sorted(mutation)):
                report=self.project([self.row(**mutation)])
                self.assertEqual(report['status'],'PARTIAL')
                self.assertEqual(report['accepted_records'],0)
        for suffix in (b'\n{"x":"'+SYNTHETIC_CANARY.encode()+b'",', b'\n\xff'+SYNTHETIC_CANARY.encode(),
                       b'\n{"millis":NaN}', b'\n{"x":1,"x":2}'):
            report=self.project([self.row()],suffix=suffix)
            self.assertEqual(report['status'],'PARTIAL')
            self.assertGreater(report['invalid_records'],0)

    def test_wrong_version_and_duplicate_header_remain_partial(self):
        header=dict(summaryLogVersion='0.4.0',codeqlVersion='2.27.3',startTime='now')
        self.assertEqual(self.project([self.row()],header=header)['status'],'PARTIAL')
        good=dict(summaryLogVersion='0.4.0',codeqlVersion='2.27.4',startTime='now')
        report=self.project([self.row(),good])
        self.assertEqual(report['status'],'PARTIAL')
        self.assertGreater(report['invalid_records'],0)

    def test_unobserved_incomplete_record_keeps_marker_without_claiming_compatible_shape(self):
        value=self.row(completionType='INCOMPLETE');del value['completionTime']
        report=self.project([value])
        self.assertEqual(report['status'],'PARTIAL')
        self.assertEqual(report['incomplete_predicates'],1)
        self.assertEqual(report['unknown_shapes'],1)
        self.assertEqual(report['rankings']['official_millis'],[])

    def test_missing_footer_stderr_and_unobserved_null_delta_are_partial(self):
        report=self.project([self.row()],stderr=b'ERROR: Reached end of log file before footer was found.\n')
        self.assertEqual(report['status'],'PARTIAL');self.assertTrue(report['input_footer_missing'])
        report=self.project([self.member(deltaSizes=[None])])
        self.assertEqual(report['status'],'PARTIAL')
        self.assertEqual(report['unobserved_delta_null_entries'],1)

    def test_pipeline_failure_or_exception_cannot_be_lost_in_implicit_success(self):
        report=self.project([self.member(pipelineRuns=[dict(raReference=SYNTHETIC_CANARY,completionType='FAILURE',exceptionName=SYNTHETIC_CANARY)])])
        self.assertEqual(report['status'],'PARTIAL')
        self.assertEqual(report['pipeline_non_success'],1)
        self.assertEqual(report['exception_records'],1)

    def test_real_pipe_limit_failures_keep_observed_bytes_and_sticky_partial_after_eof(self):
        prefix = (json.dumps(dict(summaryLogVersion='0.4.0',codeqlVersion='2.27.4',startTime='now'))+
                  '\n'+json.dumps(self.row())+'\n').encode()
        for mode in ('bytes','line','records'):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as folder:
                root=Path(folder);out=root/'out';out.mkdir();ready=root/'parsed'
                extra=(SYNTHETIC_CANARY+'x'*2048+'\n').encode() if mode=='line' else (json.dumps(self.row(raHash='new-ra'))+'\n').encode()
                child=root/'producer.py'
                child.write_text('import os,time\nfrom pathlib import Path\n'+f'os.write(1,{prefix!r})\n'+
                    f'ready=Path({str(ready)!r})\nend=time.monotonic()+3\n'+
                    'while not ready.exists() and time.monotonic()<end:time.sleep(.01)\n'+f'os.write(1,{extra!r})\n')
                class Marked(profile.Summary):
                    def line(observer,name,data):
                        result=super().line(name,data)
                        if observer.accepted: ready.touch()
                        return result
                observer=Marked([QUERY])
                key,limit={'bytes':('RAW_CAP',len(prefix)+1),'line':('LINE_CAP',len(prefix)+1),
                           'records':('MAX_SUMMARY_RECORDS',1)}[mode]
                with patch.object(profile,key,limit):
                    with self.assertRaises(runtime.PhaseError) as caught:
                        runtime.run([sys.executable,str(child)],root,out/'summary.log',time.monotonic()+8,8,out,
                                    os.environ.copy(),observer=observer,restricted_output=True)
                report=caught.exception.result['output_observation']
                self.assertEqual(report['status'],'PARTIAL')
                self.assertIsNotNone(report['feed_failure'])
                stream=report['streams']['stdout']
                self.assertTrue(stream['final'])
                self.assertEqual(stream['bytes'],len(prefix)+len(extra))
                self.assertEqual(stream['sha256'],hashlib.sha256(prefix+extra).hexdigest())
                self.assertEqual(stream['processed_prefix_bytes'],len(prefix))
                for path in out.rglob('*'):
                    if path.is_file():self.assertNotIn(SYNTHETIC_CANARY.encode(),path.read_bytes())


if __name__ == '__main__':
    unittest.main()
