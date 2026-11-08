import hashlib
import json
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('adapters', ROOT / 'scripts/engineering_telemetry_adapters.py')
a = importlib.util.module_from_spec(spec); spec.loader.exec_module(a)

class AdapterTests(unittest.TestCase):
    def test_actions_does_not_copy_secrets(self):
        raw = b'{"jobs":[{"id":4,"started_at":"2026-01-01T00:00:00Z","completed_at":"2026-01-01T00:01:00Z","conclusion":"failure","name":"secret /home/user"}]}'
        rows, missing, digest = a.project('actions-jobs', raw)
        self.assertEqual(rows[0]['execution_result'], 'failure')
        self.assertNotIn('secret', repr(rows)); self.assertTrue(missing)
        self.assertEqual(digest, hashlib.sha256(raw).hexdigest())

    def test_fractional_timestamp_order_and_unusual_conclusion(self):
        raw = b'{"jobs":[{"id":5,"started_at":"2026-01-01T00:00:00.5Z","completed_at":"2026-01-01T00:00:00Z","conclusion":"stale"}]}'
        rows, missing, _ = a.project("actions-jobs", raw)
        self.assertEqual(rows[0]["execution_result"], "stale")
        self.assertIn("reversed job timestamps", missing)

    def test_process_no_guessed_identity(self):
        rows, missing, _ = a.project('process-jsonl', b'{"test_name":"x", "elapsed_seconds":3, "exit_code":1, "passed":false}\n')
        self.assertEqual(rows[0]['exit_code'], 1); self.assertNotIn('job_id', rows[0]); self.assertTrue(missing)

    def test_process_producer_utc_offset_and_missing_time(self):
        raw = b'{"started_at":"2026-11-03T15:37:25.123456+00:00","elapsed_seconds":3,"exit_code":0,"passed":true}\n'
        rows, missing, _ = a.project("process-jsonl", raw)
        self.assertEqual(rows[0]["started_at"], "2026-11-03T15:37:25.123456+00:00")
        self.assertNotIn("invalid or missing process started_at", missing)
        for invalid in (None, "2026-01-01T00:00:00", "2026-01-01T00:00:00+01:00"):
            import json
            rows, missing, _ = a.project("process-jsonl", (json.dumps({"started_at": invalid, "elapsed_seconds": 1, "exit_code": 0}) + "\n").encode())
            self.assertIsNone(rows[0]["started_at"])
            self.assertIn("invalid or missing process started_at", missing)

    def test_schema_and_budget_fail_closed(self):
        with self.assertRaises(ValueError): a.project('build-manifest', b'{"schema_version":3}')
        with self.assertRaises(ValueError): a.project('compile-metrics', b'{}')
        with self.assertRaises(ValueError): a.project('actions-jobs', b' ' * (a.MAX_BYTES + 1))
        with self.assertRaises(ValueError): a.project('unknown', b'{}')
        with self.assertRaises(ValueError): a.project('actions-jobs', b"[" * 2000 + b"]" * 2000)


class RuntimeBindings(unittest.TestCase):
    def setUp(self):
        import engineering_telemetry as b
        self.b = b
        self.row = dict(schema_version='oasis7-engineering-job/v1', repository='owner/repo', run_id=7, run_attempt=2,
            job_key='select', workflow_sha='d'*40, matrix={}, needs={}, target_source_sha='a'*40,
            target_tested_sha='b'*40, target_policy_sha='c'*40, checkout_sha='a'*40,
            configuration_digests={'checkout-test-config':'e'*64}, manifests=[], missing_reasons=[])
        self.job = dict(id=11, run_id=7, run_attempt=2, head_sha='a'*40, name='misleading display name', conclusion='success',
            started_at='2026-01-01T00:00:00Z', completed_at='2026-01-01T00:00:04Z', steps=[dict(number=5,
            name=b.STEP, conclusion='success', started_at='2026-01-01T00:00:02Z', completed_at='2026-01-01T00:00:03Z')])
        self.run = dict(id=7,run_attempt=2,head_sha='a'*40,repository={'full_name':'owner/repo'})

    def log(self, row=None):
        return ("2026-01-01T00:00:02.000Z ##[group]Run python3 - <<'PY_OBSERVATION' # oasis7-engineering-job-observation/v1\n"
                '2026-01-01T00:00:02.001Z ##[endgroup]\n'
                '2026-01-01T00:00:02.100Z '+self.b.MARKER+json.dumps(row or self.row)+'\n').encode()

    def test_numeric_job_binding_uses_runtime_key_not_display_name(self):
        row, source=self.b.parse_job_log(self.log(),self.job,'owner/repo',7,2)
        self.assertEqual(row['job_key'],'select'); self.assertEqual(source['step_number'],5)
        self.assertEqual(row['checkout_sha'],'a'*40)
        self.assertNotEqual(row['checkout_sha'],row['target_tested_sha'])

    def test_marker_must_be_unique_and_inside_dedicated_step(self):
        for raw in (self.log()+self.log(),self.log().replace(b'##[group]Run',b'ordinary Run'),
                    self.log().replace(b'00:00:02.100Z',b'00:00:09.100Z')):
            with self.assertRaises(ValueError):self.b.parse_job_log(raw,self.job,'owner/repo',7,2)
        for field,value in (('run_attempt',1),('run_id',True),('configuration_digests',{'password':'secret'}),('matrix',{'password':'secret'})):
            row=dict(self.row);row[field]=value
            with self.subTest(field=field),self.assertRaises(ValueError):self.b.parse_job_log(self.log(row),self.job,'owner/repo',7,2)

    def test_workflow_source_and_symbolic_needs_are_explicit(self):
        workflow=b'name: Test\njobs:\n  select:\n    steps: []\n  formal-process:\n    needs: [select]\n    steps: []\n'
        self.assertEqual(self.b.workflow_dependencies(workflow),{'select':[],'formal-process':['select']})
        for raw in (workflow.replace(b'[select]',b'${{ matrix.needs }}'),workflow.replace(b'[select]',b'[unknown]')):
            with self.assertRaises(ValueError):self.b.workflow_dependencies(raw)
        report=self.b.bind(self.run,[{'jobs':[self.job]}],{11:self.log()},workflow,'d'*40,True)
        self.assertEqual(report['jobs'][0]['job_id'],11)
        self.assertIn('runtime_job_binding_unavailable:formal-process',report['missing_reasons'])
        self.assertEqual(report['analysis']['status'],'unavailable')
        self.assertNotIn('misleading',json.dumps(report))

    def test_manifest_content_binding_keeps_transport_and_validation_unknown(self):
        import copy
        m=dict(direction='produced',label='shard',manifest_sha256='f'*64,source_sha='a'*40,tested_sha='b'*40,
               producer_run_id='7',producer_attempt='2',config_digest='e'*64)
        self.row['manifests']=[m]
        consumer=copy.deepcopy(self.row);consumer.update(job_key='formal-process',checkout_sha=None,needs={'select':'success'})
        consumer['manifests'][0]['direction']='observed-input'
        job2=dict(self.job,id=12)
        workflow=b'name: Test\njobs:\n  select:\n    steps: []\n  formal-process:\n    needs: [select]\n    steps: []\n'
        report=self.b.bind(self.run,[{'jobs':[self.job,job2]}],{11:self.log(),12:self.log(consumer)},workflow,'d'*40,True)
        edge=report['artifact_bindings'][0]
        self.assertEqual((edge['producer_job_id'],edge['consumer_job_id'],edge['content_binding']),(11,12,'bound'))
        self.assertIsNone(edge['artifact_id']);self.assertEqual(edge['validation_result'],'unknown')
        consumer['manifests'][0]['producer_attempt']='1'
        report=self.b.bind(self.run,[{'jobs':[self.job,job2]}],{11:self.log(),12:self.log(consumer)},workflow,'d'*40,True)
        self.assertIsNone(report['artifact_bindings'][0]['producer_job_id'])

    def test_emitter_is_same_code_in_workflow_and_cannot_gate(self):
        workflow=(ROOT/'.github/workflows/rust.yml').read_text()
        step=self.b.workflow_step()
        self.assertEqual(workflow.count(step),1)
        self.assertIn('continue-on-error: true',step);self.assertIn('if: always() && !cancelled()',step)
        self.assertIn('timeout-minutes: 1',step)
        compile(self.b.EMITTER,'runtime observation','exec')

    def test_conflicts_remove_all_instances_and_multiline_needs_is_rejected(self):
        workflow=b'name: Test\njobs:\n  select:\n    steps: []\n  formal-process:\n    needs: [select]\n    steps: []\n'
        for jobs in ([self.job,dict(self.job,id=12)],[self.job,self.job]):
            report=self.b.bind(self.run,[{'jobs':jobs}],{11:self.log(),12:self.log()},workflow,'d'*40,True)
            self.assertEqual(report['jobs'],[])
        with self.assertRaises(ValueError):
            self.b.workflow_dependencies(workflow.replace(b'needs: [select]',b'needs:\n      - select'))
        with self.assertRaises(ValueError):
            self.b.workflow_dependencies(workflow.replace(b'needs: [select]',b'<<: *shared\n    needs: [select]'))
        for bad in (workflow.replace(b'needs: [select]',b'"needs": [select]'), workflow.replace(b'    needs: [select]\n    steps: []',b'    steps: []\n    needs: [select]')):
            with self.assertRaises(ValueError):self.b.workflow_dependencies(bad)

    def test_log_window_rejects_late_marker_and_early_header(self):
        self.job['steps'].append(dict(number=6,name='Post cleanup',started_at='2026-01-01T00:00:20Z'))
        for raw in (self.log().replace(b'00:00:02.100Z',b'00:00:10.100Z'),
                    self.log().replace(b'00:00:02.000Z',b'00:00:00.000Z')):
            with self.assertRaises(ValueError):self.b.parse_job_log(raw,self.job,'owner/repo',7,2)

    def test_same_second_step_is_bound_by_log_group_and_cleanup_fence(self):
        self.job['steps'][0]['completed_at']='2026-01-01T00:00:02Z'
        self.job['steps'].append(dict(number=6,name='Post cleanup',started_at='2026-01-01T00:00:02Z'))
        row,_=self.b.parse_job_log(self.log()+b'2026-01-01T00:00:02.200Z Post job cleanup.\n',self.job,'owner/repo',7,2)
        self.assertEqual(row['job_key'],'select')
        bad=self.log().replace(b'2026-01-01T00:00:02.100Z '+self.b.MARKER.encode(),
                              b'2026-01-01T00:00:02.050Z Post job cleanup.\n2026-01-01T00:00:02.100Z '+self.b.MARKER.encode())
        with self.assertRaises(ValueError):self.b.parse_job_log(bad,self.job,'owner/repo',7,2)
        with self.assertRaises(ValueError):self.b.parse_job_log(self.log().replace(b'00:00:02.000Z',b'00:00:02.150Z'),self.job,'owner/repo',7,2)

    def test_cross_second_completion_requires_exact_fence_before_next_step(self):
        self.job['steps'].append(dict(number=6,name='Post cleanup',started_at='2026-01-01T00:00:03Z'))
        raw=self.log().replace(b'00:00:02.100Z',b'00:00:03.004Z')
        fenced=raw+b'2026-01-01T00:00:03.019Z Post job cleanup.\n'
        row,_=self.b.parse_job_log(fenced,self.job,'owner/repo',7,2)
        self.assertEqual(row['job_key'],'select')
        completed=fenced.replace(b'Post job cleanup.',b'Cleaning up orphan processes')
        self.b.parse_job_log(completed,self.job,'owner/repo',7,2)
        for bad in (raw, fenced.replace(b'00:00:03.019Z',b'00:00:04.019Z'),
                    fenced.replace(b'00:00:03.004Z',b'00:00:04.004Z'),
                    raw.replace(b'2026-01-01T00:00:03.004Z',
                        b'2026-01-01T00:00:03.001Z Post job cleanup.\n2026-01-01T00:00:03.004Z')):
            with self.assertRaises(ValueError):self.b.parse_job_log(bad,self.job,'owner/repo',7,2)

    def test_live_transport_fetches_exact_job_and_workflow_and_bounds_failure(self):
        import base64
        import engineering_telemetry as t
        workflow=b'name: Test\njobs:\n  select:\n    steps: []\n  formal-process:\n    needs: [select]\n    steps: []\n'
        responses={
            'actions/runs/7/attempts/2':self.run,
            'actions/runs/7/attempts/2/jobs?per_page=100&page=1':{'total_count':1,'jobs':[self.job]},
            'actions/jobs/11/logs':self.log(),
            'contents/.github/workflows/rust.yml?ref='+('d'*40):{'encoding':'base64','content':base64.b64encode(workflow).decode()}}
        seen=[]
        def transport(url,limit,timeout):
            key=url.split('/repos/owner/repo/')[1];seen.append(key);value=responses[key]
            return 200,None,value if isinstance(value,bytes) else json.dumps(value).encode()
        result=self.b.collect_live('owner/repo',7,2,t.CollectionBudget(),transport)
        self.assertEqual(result['jobs'][0]['job_id'],11)
        self.assertEqual(len(seen),4);self.assertEqual(result['transport'],'live')
        result=self.b.collect_live('owner/repo',7,2,t.CollectionBudget(requests=1),transport)
        self.assertEqual(result['collection_status'],'partial');self.assertEqual(result['jobs'],[])
        responses['actions/runs/7/attempts/2']=dict(self.run,id=99)
        seen.clear()
        result=self.b.collect_live('owner/repo',7,2,t.CollectionBudget(),transport)
        self.assertEqual(result['missing_reasons'],['run_identity_mismatch']);self.assertEqual(len(seen),1)
        def failure(url,limit,timeout):return 403,None,b''
        result=self.b.collect_live('owner/repo',7,2,t.CollectionBudget(),failure)
        self.assertEqual(result['missing_reasons'],['http_status_403'])

    def test_real_emitter_without_checkout_and_with_manifest(self):
        import os,subprocess,sys,tempfile,hashlib
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);(root/'native-tests').mkdir()
            manifest=dict(schema_version=2,source_sha='a'*40,tested_sha='b'*40,producer_run_id='7',producer_attempt='2',config_sha256='e'*64,private_path='/private/password=DO_NOT_COPY')
            raw=json.dumps(manifest).encode();(root/'native-tests/manifest.json').write_bytes(raw)
            env=dict(os.environ,GITHUB_WORKSPACE=directory,RUNNER_TEMP=directory,GITHUB_JOB='formal-process-tests',GITHUB_REPOSITORY='owner/repo',
                     GITHUB_RUN_ID='7',GITHUB_RUN_ATTEMPT='2',TELEMETRY_WORKFLOW_SHA='d'*40,TELEMETRY_MATRIX='{"shard":"world-history"}',
                     TELEMETRY_NEEDS='{}',TELEMETRY_SOURCE_SHA='a'*40,TELEMETRY_TESTED_SHA='b'*40,TELEMETRY_POLICY_SHA='c'*40)
            proc=subprocess.run([sys.executable,'-c',self.b.EMITTER],env=env,capture_output=True,text=True,check=True)
            row=json.loads(proc.stdout.removeprefix(self.b.MARKER))
            self.b.validate_observation(row,'owner/repo',7,2)
            self.assertIsNone(row['checkout_sha']);self.assertEqual(row['manifests'][0]['manifest_sha256'],hashlib.sha256(raw).hexdigest())
            self.assertNotIn('DO_NOT_COPY',proc.stdout)
            manifest['producer_run_id']='password=DO_NOT_COPY'
            (root/'native-tests/manifest.json').write_text(json.dumps(manifest))
            proc=subprocess.run([sys.executable,'-c',self.b.EMITTER],env=env,capture_output=True,text=True,check=True)
            self.assertNotIn('DO_NOT_COPY',proc.stdout)
            self.assertEqual(json.loads(proc.stdout.removeprefix(self.b.MARKER))['manifests'],[])
            (root/'native-tests/manifest.json').unlink();(root/'native-tests/manifest.json').symlink_to(root/'outside.json');(root/'outside.json').write_bytes(raw)
            proc=subprocess.run([sys.executable,'-c',self.b.EMITTER],env=env,capture_output=True,text=True,check=True)
            row=json.loads(proc.stdout.removeprefix(self.b.MARKER));self.assertEqual(row['manifests'],[])
            self.assertIn('manifest_observation_unavailable',row['missing_reasons'])


if __name__ == '__main__': unittest.main()
