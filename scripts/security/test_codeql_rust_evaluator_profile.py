import ast
import re
import importlib.util
import json
import io
import sys
import time
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch
import codeql_rust_evaluator_profile as profile


class ProfileTests(unittest.TestCase):
    HEADER = json.dumps(dict(summaryLogVersion='observed',codeqlVersion='2.27.3',startTime='2026-10-29T15:29:13Z')).encode()+b'\n'
    def test_log_complete_partial_and_truncated(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d)/'events'
            self.assertEqual(profile.log_completeness(p)['status'],'NOT_REACHED')
            for text, expected in [('', 'PARTIAL'), ('{}\n','PARTIAL'),
                ('{"type":"LOG_HEADER"}\n{"type":"LOG_FOOTER"}\n','COMPLETE'),
                ('{"type":"LOG_HEADER"}\n{','PARTIAL'),
                ('{"type":"LOG_HEADER"}\n{"type":"LOG_HEADER"}\n{"type":"LOG_FOOTER"}\n','PARTIAL')]:
                p.write_text(text)
                self.assertEqual(profile.log_completeness(p)['status'],expected)

    def exercise(self, failure=None, corrupt=False, deadline_delta=3300, analysis_delta=2400, partial=False, summary_failed=False, config_changed=False):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);out=root/'out';out.mkdir()
            cli=root/'codeql';cli.write_text('cli')
            qr=root/'qlpacks/codeql/rust-queries/1';(qr/'codeql-suites').mkdir(parents=True)
            suite=qr/'codeql-suites/rust-code-scanning.qls';suite.write_text('official')
            q=qr/'real.ql';q.write_text('real')
            entries=[dict(relative='real.ql',id='real',sha256=profile.runtime.digest(q))]
            frozen=dict(query_root=str(qr),suite_sha256=profile.runtime.digest(suite),queries=entries)
            (out/'default-suite.json').write_text(json.dumps(frozen))
            (root/'db/temp').mkdir(parents=True)
            (root/'db/temp/config-queries.qls').write_text('actual Action suite')
            if corrupt:q.write_text('changed')
            args=SimpleNamespace(codeql=cli,source=root,output=out,database=root/'db',deadline=10000+deadline_delta,analysis_end=10000+analysis_delta,threads=4,ram=14570)
            calls=[]
            def run(argv,*a,**kw):
                calls.append(argv)
                if 'resolve' in argv:
                    target='profile-action-suite.log' if 'config-queries.qls' in argv[3] else 'profile-resolved-suite.log'
                    (out/target).write_text('[1]' if config_changed and target=='profile-action-suite.log' else '[]')
                else:
                    self.assertEqual(profile.runtime.WRITE_CAP,184*1024**2)
                    kw['evaluator'].write_text('{"type":"LOG_HEADER"}\n'+('' if failure or partial else '{"type":"LOG_FOOTER"}\n'))
                    receipt=dict(argv=[str(out/'official-evaluator.fifo') if a=='{EVALUATOR_FIFO}' else a for a in argv],cwd=str(root),exit=0,error=None,cleanup_errors=[],evidence_errors=[],pid=123,pgid=123,started_wall=10000,elapsed=1)
                    if failure:receipt.update(failure)
                    (out/'profile-query.log.result.json').write_text(json.dumps(receipt))
                    if failure:raise profile.runtime.PhaseError(receipt)
            def summaries(*a):
                self.assertEqual(profile.runtime.WRITE_CAP,248*1024**2)
                self.assertLessEqual(a[4],130)
                return {'status':'PARTIAL' if summary_failed else 'GENERATED_PROJECTED_NOT_EVALUATION_COMPLETION'}
            cap=SimpleNamespace(suite_entries=lambda value,*a:[] if value==[1] else entries)
            with patch.object(profile.time,'time',return_value=10000),patch.object(profile.time,'monotonic',return_value=100),patch.object(profile.runtime,'run',side_effect=run),patch.object(profile,'bounded_summary',side_effect=summaries) as sm:
                profile.execute(args,cap)
                (out/'capacity-capture-terminal.json').write_text(json.dumps(dict(checkout_gate='PASS',resource_gate='PASS',lock_observation_gate='PASS',core_receipt_gate='PASS')))
                (out/'profile-after-identity.json').write_text(json.dumps(dict(source='UNCHANGED',official='UNCHANGED')))
                frozen_query=(out/'profile-query-terminal.json').read_bytes()
                rc=profile.summarize(args)
                self.assertEqual((out/'profile-query-terminal.json').read_bytes(),frozen_query)
            return rc,json.loads((out/'profile-terminal.json').read_text()),calls,sm.call_count

    def test_complete_official_workload_no_decode(self):
        rc,v,calls,summaries=self.exercise()
        self.assertEqual(rc,0);self.assertEqual(v['status'],'PROFILE_COMPLETED_QUERIES_NOT_VERIFIED')
        self.assertEqual(summaries,1)
        cmd=calls[-1]
        self.assertIn('run-queries',cmd);self.assertIn('{EVALUATOR_FIFO}',cmd)
        self.assertIn('--no-tuple-counting',cmd)
        self.assertNotIn('decode',str(calls));self.assertNotIn('upload',str(calls))
        self.assertEqual(v['full_rust_coverage'],'HOLD')

    def test_timeout_preserves_primary_failure_and_partial_log(self):
        failure=dict(failure_kind='timeout',cleanup_errors=[],evidence_errors=[],error='deadline')
        rc,v,_,_=self.exercise(failure)
        self.assertEqual(rc,1);self.assertEqual(v['failure']['failure_kind'],failure['failure_kind'])
        self.assertEqual(v['evaluator_log']['status'],'PARTIAL')
        self.assertEqual(v['status'],'INCOMPLETE')

    def test_partial_log_or_summary_failure_is_not_green(self):
        for options in ({'partial':True},{'summary_failed':True}):
            rc,v,_,_=self.exercise(**options)
            self.assertEqual(rc,1)
            self.assertEqual(v['query_status'],'COMPLETED_NOT_VERIFIED')
            self.assertEqual(v['status'],'INCOMPLETE')

    def test_actual_action_suite_drift_rejects_query(self):
        rc,v,calls,summaries=self.exercise(config_changed=True)
        self.assertEqual(rc,1);self.assertEqual(summaries,0)
        self.assertTrue(all('run-queries' not in cmd for cmd in calls))

    def test_unsafe_query_does_not_admit_summary(self):
        rc,v,_,summaries=self.exercise(dict(failure_kind='unsafe',cleanup_errors=['owned group unknown'],evidence_errors=[],error='unsafe'))
        self.assertEqual(rc,1);self.assertEqual(summaries,0)
        self.assertEqual(v['failure']['failure_kind'],'unsafe')

    def test_changed_query_rejected_before_spawn(self):
        rc,v,calls,summaries=self.exercise(corrupt=True)
        self.assertEqual(rc,1);self.assertEqual(calls,[]);self.assertEqual(summaries,0)

    def test_invalid_time_gates_zero_spawn(self):
        for job,analysis in [(3300,2401),(3300,-1),(2500,2000)]:
            rc,_,calls,_=self.exercise(deadline_delta=job,analysis_delta=analysis)
            self.assertEqual(rc,1);self.assertEqual(calls,[])

    def test_official_summary_refuses_unsafe_failure(self):
        failure=profile.runtime.PhaseError(dict(failure_kind='unsafe',cleanup_errors=[],evidence_errors=[]))
        with patch.object(profile.runtime,'run') as run:
            v=profile.runtime.official_summaries('cli',Path('missing'),Path('.'),Path('.'),999999,{},failure)
        self.assertEqual(v['status'],'NOT_RUN_UNSAFE_FAILURE');run.assert_not_called()

    @staticmethod
    def complete_record(key='one',millis=10):
        return dict(appearsAs={},completionTime='2026-10-29T00:00:00Z',dependencies={},evaluationStrategy='COMPUTE_SIMPLE',
                    millis=millis,position='source:1',predicateName='known',queryCausingWork='query.ql',raHash=key,resultSize=1)

    @staticmethod
    def partial_record(key='partial',times=None):
        return dict(appearsAs={},completionType='INCOMPLETE',deltaSizes=[],evaluationStrategy='NAMED_LOCAL',mainHash='parent',
                    position='source:1',predicateIterationMillis=[20,-1,30] if times is None else times,predicateName='partial',queryCausingWork='query.ql',ra={},raHash=key)

    @staticmethod
    def metadata_record():
        return dict(appearsAs={},completionTime='2026-10-29T00:00:00Z',evaluationStrategy='SENTINEL_EMPTY',isCached=True,
                    predicateName='metadata',raHash='cache',sentinelRaHash='sentinel')

    def test_projection_ranks_physical_records_and_preserves_unknown(self):
        values=[self.metadata_record(),self.complete_record(),self.partial_record()]
        result=profile.project_summary(io.BytesIO(self.HEADER+('\n'.join(json.dumps(v) for v in values)).encode()))
        self.assertEqual(result['records'],3);self.assertEqual(result['untimed_metadata_records'],1)
        row=result['rankings']['known_iteration_subtotal'][0]
        self.assertEqual(row['known_iteration_millis'],50);self.assertEqual(row['negative_marker_entries'],1)
        self.assertEqual(result['rankings']['official_millis'][0]['reported_millis'],10)
        self.assertNotIn('ra',row);self.assertNotIn('appearsAs',row)
        for bad in ([True],[-2],[2**63]):
            with self.assertRaises(ValueError):profile.project_summary(io.BytesIO(self.HEADER+json.dumps(self.partial_record(times=bad)).encode()))

    def test_all_observed_schema_shapes_have_separate_cost_classes(self):
        catalog=json.loads(Path(__file__).with_name('codeql_summary_schema_2_27_3.json').read_text())
        rows=[]
        for i,signature in enumerate(catalog['signatures']):
            if 'codeqlVersion' in signature['types']:continue
            row={k:{'str':'sample','int':1,'bool':True,'dict':{},'list':[]}[t] for k,t in signature['types'].items()}
            row['raHash']=str(i)
            row['evaluationStrategy']=json.loads(next(iter(signature['evaluationStrategy_counts'])))
            if 'completionType' in row:row['completionType']='INCOMPLETE'
            if 'predicateIterationMillis' in row:row['predicateIterationMillis']=[2,-1,3]
            rows.append(row)
        result=profile.project_summary(io.BytesIO(self.HEADER+('\n'.join(json.dumps(v) for v in rows)).encode()))
        self.assertEqual(result['records'],18);self.assertEqual(result['untimed_metadata_records'],4)
        self.assertEqual(result['timing_records'],14);self.assertEqual(result['incomplete_timing_records'],4)

    def test_owned_worker_reads_header_and_reaps_schema_failures(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);cli=root/'official-cli-fixture'
            for good in (True,False):
                out=root/str(good);out.mkdir()
                prefix=self.HEADER if good else b'{"unexpected":true}\n'
                payload=prefix+json.dumps(self.complete_record()).encode()+b'\n'
                cli.write_text('#!'+sys.executable+'\nimport sys\nsys.stdout.buffer.write('+repr(payload)+')\nsys.stdout.buffer.flush()\n')
                cli.chmod(0o700)
                result=profile.bounded_summary(cli,root/'raw',root,out,time.monotonic()+15,{})
                if good:
                    self.assertEqual(result['status'],'GENERATED_PROJECTED_NOT_EVALUATION_COMPLETION')
                    report=json.loads((out/'official-evaluator-summary-projection.json').read_text())
                    self.assertEqual(report['rankings']['official_millis'][0]['reported_millis'],10)
                else:
                    self.assertEqual(result['status'],'PARTIAL');self.assertEqual(result['failure']['cleanup_errors'],[])

    def test_metadata_and_timing_can_share_hash_without_double_counting(self):
        timed=dict(self.partial_record(),evaluationStrategy='IN_LAYER',pipelineRuns=[])
        meta=self.metadata_record()
        meta['raHash']=timed['raHash'];meta['predicateName']=timed['predicateName']
        raw=self.HEADER+b'\n'.join(json.dumps(v).encode() for v in (meta,timed))
        result=profile.project_summary(io.BytesIO(raw))
        self.assertEqual(result['records'],2)
        self.assertEqual(result['untimed_metadata_records'],1)
        self.assertEqual(result['timing_records'],1)
        self.assertEqual(result['metadata_timing_shared_hashes'],1)
        self.assertEqual(len(result['rankings']['known_iteration_subtotal']),1)
        for rows in ((meta,meta,timed),(timed,timed),(meta,dict(timed,predicateName='other')),(meta,dict(self.complete_record(),raHash=timed['raHash'],predicateName=timed['predicateName']))):
            with self.assertRaises(ValueError):
                profile.project_summary(io.BytesIO(self.HEADER+b'\n'.join(json.dumps(v).encode() for v in rows)))

    def test_projection_serialized_output_cap_includes_escaping(self):
        report={'top':[dict(raHash=str(i)+'x'*8190,predicateName='\x00'*8192,position='p'*8192,completionType='INCOMPLETE') for i in range(100)]}
        with self.assertRaises(ValueError):profile.encoded_projection(report)
        self.assertLess(len(profile.encoded_projection({'top':[]})),1024**2)

    def test_projection_rejects_duplicate_truncated_and_oversized_records(self):
        line=json.dumps(self.complete_record()).encode()+b'\n'
        for raw in (self.HEADER+line+line,self.HEADER+line+b'{',b'[]',b'',line,self.HEADER+self.HEADER+line):
            with self.assertRaises((ValueError,AttributeError)):profile.project_summary(io.BytesIO(raw))
        cached=self.metadata_record()
        with self.assertRaises(ValueError):profile.project_summary(io.BytesIO(self.HEADER+json.dumps(cached).encode()))
        for flag in (False,None,'true'):
            cached['isCached']=flag
            with self.assertRaises(ValueError):profile.project_summary(io.BytesIO(self.HEADER+json.dumps(cached).encode()+b'\n'+line))
        cached=self.metadata_record();cached['unexpected']=True
        with self.assertRaises(ValueError):profile.project_summary(io.BytesIO(self.HEADER+json.dumps(cached).encode()+b'\n'+line))
        with self.assertRaises(ValueError):profile.project_summary(io.BytesIO(self.HEADER+line),record_limit=8)
        with self.assertRaises(ValueError):profile.project_summary(io.BytesIO(self.HEADER+line),total_limit=8)

    def test_missing_query_terminal_records_not_reached(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d)
            with patch.object(profile.runtime,'run') as run:
                self.assertEqual(profile.summarize(SimpleNamespace(output=root)),1)
            run.assert_not_called()
            self.assertEqual(json.loads((root/'profile-summary-terminal.json').read_text())['status'],'NOT_REACHED_QUERY_TERMINAL_MISSING')

    def test_oversized_summary_preserves_core_bytes(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);cli=root/'fake-cli'
            cli.write_text('#!'+sys.executable+'\nimport sys\nsys.stdout.write("x"*1048576)\nsys.stdout.flush()\n')
            cli.chmod(0o700)
            core=root/'lock-receipt.json';core.write_bytes(b'protected primary evidence')
            before=core.read_bytes();prior=profile.runtime.WRITE_CAP
            used=profile.runtime.output_bytes(root)
            with patch.object(profile,'SUMMARY_OUTPUT_BUDGET',524288),patch.object(profile,'__file__',str(cli)):
                result=profile.bounded_summary(cli,root/'raw',root,root,time.monotonic()+10,{})
            self.assertEqual(result['status'],'PARTIAL')
            self.assertEqual(result['failure']['failure_kind'],'unsafe')
            self.assertIn('evidence budget',result['failure']['error'])
            self.assertLessEqual(profile.runtime.output_bytes(root)-used,524288)
            self.assertEqual(core.read_bytes(),before)
            profile.runtime.save(root/'sealed-terminal.json',{'status':'PARTIAL_SUMMARY_CORE_PRESERVED'},root,terminal=True)
            profile.runtime.save(root/'hashes.json',{p.name:profile.runtime.digest(p) for p in root.iterdir() if p.is_file()},root,terminal=True)
            self.assertTrue((root/'hashes.json').is_file())
            self.assertEqual(profile.runtime.WRITE_CAP,prior)
            self.assertFalse((root/'official-evaluator-summary-predicates.log').exists())
            self.assertLessEqual((root/'official-evaluator-summary-projection.json').stat().st_size,524288)

    def test_embedded_python_clock_and_capture_binding(self):
        s=(Path(__file__).parents[2]/'.github/workflows/codeql-rust-cfg-diagnostic.yml').read_text().split('  evaluator_profile:',1)[1]
        blocks=re.findall(r"python3 - <<'(PY|PYCODE)'\n(.*?)\n          \1",s,re.S)
        self.assertEqual(len(blocks),4)
        self.assertLess(s.index('- id: integrity'),s.index('- id: capture'))
        self.assertLess(s.index('- id: capture'),s.index('- id: summary'))
        self.assertLess(s.index('- id: summary'),s.index('- id: evidence'))
        self.assertIn('--phase evaluate',s)
        self.assertIn('--phase summarize',s)
        for _,block in blocks:
            ast.parse('\n'.join(line[10:] for line in block.splitlines()))
        self.assertIn("profile-analysis-clock.json",s)
        self.assertIn("'allocated_seconds':analysis_end-analysis_start",s)
        self.assertNotIn("'analysis_allocated_minutes':40",s)
        self.assertIn('--analyze-outcome "${{ steps.graph.outcome }}"',s)
        self.assertIn("'codeql_rust_evaluator_profile.py'",(Path(__file__).with_name('codeql_rust_capacity_layout.py')).read_text())

    def test_workflow_bounds_source_isolation_no_upload(self):
        s=(Path(__file__).parents[2]/'.github/workflows/codeql-rust-cfg-diagnostic.yml').read_text().split('  evaluator_profile:',1)[1]
        for value in ('timeout-minutes: 55','budget=3300','analysis_start+2400',"CAPACITY_DEADLINE'])-600",'e205ebe902d9fd3240cb3cb6ff6e36cbf84c00e6','--analysis-end "$PROFILE_ANALYSIS_END"','persist-credentials: false'):
            self.assertIn(value,s)
        full=(Path(__file__).parents[2]/'.github/workflows/codeql-rust-cfg-diagnostic.yml').read_text()
        self.assertIn('inputs.profile != true',full)
        self.assertIn('inputs.profile == true',full)
        self.assertEqual(full.split('permissions:',1)[0].count('default: false'),3)
        for value in ('codeql-action/analyze@','upload-sarif','security-events:',"'query_check'",'steps.query_check'):
            self.assertNotIn(value,s)


if __name__=='__main__':unittest.main()
