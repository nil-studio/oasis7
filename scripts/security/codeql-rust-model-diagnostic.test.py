#!/usr/bin/env python3
"""Local process/transport and official-input contract tests; no CodeQL execution."""
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import time
import unittest
from unittest.mock import patch, Mock

import codeql_rust_diagnostic_runtime as r

P = Path(__file__).with_name('codeql-rust-model-diagnostic.py')
spec = importlib.util.spec_from_file_location('diagnostic', P)
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)


class Runtime(unittest.TestCase):
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


class OfficialInputs(unittest.TestCase):
    def test_real_saved_official_manifest_closure_is_exact(self):
        # Small fixture uses the official manifest grammar and two separate versions.
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            for version,rust in [('2.27.2','0.2.23'),('2.27.3','0.2.24')]:
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


class Workflow(unittest.TestCase):
    def test_literal_manual_has_no_scan_or_required_override(self):
        root=P.resolve().parents[2];text=(root/'.github/workflows/rust.yml').read_text()
        job=text.split('  codeql-rust-model-diagnostic:\n',1)[1].split('\n  select:',1)[0]
        self.assertIn("github.event_name == 'workflow_dispatch' && inputs.run_mode == 'codeql_rust_model_diagnostic'",job)
        self.assertIn('timeout-minutes: 20',job)
        self.assertIn('permissions:\n      contents: read',job)
        for forbidden in ('security-events','GH_TOKEN','GITHUB_TOKEN','upload-sarif','codeql/analyze','--admin','continue-on-error'):
            self.assertNotIn(forbidden,job)
        self.assertIn("if: github.event_name != 'workflow_dispatch' || inputs.run_mode == 'full'",text)
        self.assertIn("if: always() && (github.event_name == 'pull_request' || github.event_name == 'push')",text)
        for version,pin in m.BUNDLES.items():self.assertIn(version,job);self.assertIn(pin['sha256'],job)
        self.assertIn(' + 1080 ',job);self.assertIn('64 * 1024**2',job)
        for test in ('codeql-rust-model-diagnostic.test.py','codeql-rust-refactored-source.test.py'):
            self.assertIn('run python3 ./scripts/security/'+test,(root/'scripts/ci-tests.sh').read_text())


if __name__=='__main__':unittest.main()
