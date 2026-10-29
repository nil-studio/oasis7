import ast
import re
import importlib.util
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch
import codeql_rust_evaluator_profile as profile


class ProfileTests(unittest.TestCase):
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
                    if failure:raise profile.runtime.PhaseError(failure)
            def summaries(*a):
                self.assertEqual(profile.runtime.WRITE_CAP,248*1024**2)
                self.assertLessEqual(a[4],160)
                return {'overall':'UNKNOWN' if summary_failed else 'GENERATED_NOT_EVALUATION_COMPLETION','predicates':'GENERATED_NOT_EVALUATION_COMPLETION'}
            cap=SimpleNamespace(suite_entries=lambda value,*a:[] if value==[1] else entries)
            with patch.object(profile.time,'time',return_value=10000),patch.object(profile.time,'monotonic',return_value=100),patch.object(profile.runtime,'run',side_effect=run),patch.object(profile.runtime,'official_summaries',side_effect=summaries) as sm:
                rc=profile.execute(args,cap)
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
        self.assertEqual(rc,1);self.assertEqual(v['failure'],failure)
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

    def test_embedded_python_clock_and_capture_binding(self):
        s=(Path(__file__).parents[2]/'.github/workflows/codeql-rust-cfg-diagnostic.yml').read_text().split('  evaluator_profile:',1)[1]
        blocks=re.findall(r"python3 - <<'(PY|PYCODE)'\n(.*?)\n          \1",s,re.S)
        self.assertEqual(len(blocks),4)
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
        self.assertEqual(full.split('permissions:',1)[0].count('default: false'),2)
        for value in ('codeql-action/analyze@','upload-sarif','security-events:',"'query_check'",'steps.query_check'):
            self.assertNotIn(value,s)


if __name__=='__main__':unittest.main()
