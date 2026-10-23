#!/usr/bin/env python3
"""Actual traffic wrapper lifecycle with offline curl and SSH fixtures."""
import contextlib
import importlib.util
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import sys
import subprocess
import tempfile
import time
import unittest

SCRIPTS = Path(__file__).resolve().parent

@contextlib.contextmanager
def traffic_fixture():
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory).resolve()
        scripts = root / 'scripts'; scripts.mkdir()
        for name in ('p2p-real-env-traffic-monitor.sh', 'p2p-real-env-observability-attempt.py', 'traffic-monitor-summary.py', 'traffic_monitor_freshness.py'):
            shutil.copy2(SCRIPTS / name, scripts / name)
        binary = root / 'bin'; binary.mkdir()
        source = r'''#!/usr/bin/env python3
import json,os,pathlib,sys,time
root=pathlib.Path(os.environ['TRAFFIC_ROOT'])
if os.environ.get('TRAFFIC_GATE'):
 gate=pathlib.Path(os.environ['TRAFFIC_GATE']); gate.with_suffix('.entered').touch()
 while not gate.exists(): time.sleep(.01)
if os.environ.get('TRAFFIC_FAIL'): sys.exit(7)
if pathlib.Path(sys.argv[0]).name == 'ssh' and 'iface=' in sys.argv[-1]:
 print('{"available":false}'); sys.exit()
now=int(time.time()*1000)
print(json.dumps({'ok':True,'node_id':'offline-node','world_id':'offline-world','observed_at_unix_ms':now,'traffic':{'udp_gossip':{'observed_since_unix_ms':now-1000},'libp2p_replication':{'observed_since_unix_ms':now-1000}}}))
'''
        for name in ('curl','ssh'):
            executable = binary/name; executable.write_text(source); executable.chmod(0o755)
        env = dict(os.environ, PATH=str(binary)+os.pathsep+os.environ['PATH'], TRAFFIC_ROOT=str(root))
        env.pop('P2PARCH6_SEQ_SSH_PASSWORD', None); env.pop('P2PARCH6_STORAGE_SSH_PASSWORD', None)
        command=['bash',str(scripts/'p2p-real-env-traffic-monitor.sh'),'--out-dir',str(root/'out'),'--samples','1','--interval-secs','1']
        yield root,env,command

def complete(command, env, expected=0):
    result = subprocess.run(command, env=env, capture_output=True, text=True)
    assert result.returncode == expected, (result.returncode, result.stdout, result.stderr)
    return result


def canonical(root, expected=0, out=None):
    return json.loads(complete([sys.executable, str(root/'scripts/p2p-real-env-observability-attempt.py'), 'read', '--collector', 'traffic', '--out-dir', str(out or root/'out')], os.environ, expected).stdout)


class TrafficDurableTests(unittest.TestCase):
    def test_actual_legacy_wrapper_upgrades_in_place_without_adopting_old_success(self):
        # Byte-for-byte predecessor from 74ffd31be78e34165378783f42cfe2fc0814880f,
        # retained for shallow CI checkouts; digest guards the extracted source.
        legacy = SCRIPTS/'p2p-real-env-traffic-monitor.legacy-fixture.sh'
        self.assertEqual(hashlib.sha256(legacy.read_bytes()).hexdigest(), '9e64781288421107bf71d9493c9d8a34d4efe5ca93fc0cfd2caf38a5c886843e')
        with traffic_fixture() as (root,env,command):
            wrapper=root/'scripts/p2p-real-env-traffic-monitor.sh'
            current=wrapper.read_bytes()
            wrapper.write_bytes(legacy.read_bytes())
            complete(command,env)
            out=root/'out'
            legacy_runs=list((out/'runs').iterdir())
            self.assertEqual(len(legacy_runs),1)
            self.assertRegex(legacy_runs[0].name,r'^[0-9]{8}-[0-9]{6}$')
            legacy_evidence={str(path.relative_to(out)):path.read_bytes() for path in legacy_runs[0].rglob('*') if path.is_file()}
            old_history=(out/'history.ndjson').read_bytes()
            self.assertEqual(len(old_history.splitlines()),3)
            self.assertFalse((out/'attempt_state.json').exists())
            wrapper.write_bytes(current)
            process,release=self.start_paused(root,env,command)
            try:
                pending=canonical(root,1)
                self.assertEqual(pending['attempt']['sequence'],1)
                self.assertEqual(pending['attempt']['result'],'pending')
                self.assertIsNone(pending['last_success'])
                self.assertEqual((out/'history.ndjson').read_bytes(),old_history)
                release.touch()
                output=process.communicate(timeout=10)
                self.assertEqual(process.returncode,0,output)
            finally:
                self.stop_process(process)
            first=canonical(root)
            self.assertEqual(first['attempt']['sequence'],1)
            self.assertEqual(first['last_success']['sequence'],1)
            self.assertEqual(first['run_id'],'1-'+first['attempt']['attempt_id'])
            self.assertEqual(len((out/'history.ndjson').read_bytes().splitlines()),6)
            complete(command,env)
            second=canonical(root)
            self.assertEqual(second['attempt']['sequence'],2)
            self.assertEqual(second['last_success']['sequence'],2)
            self.assertEqual(len((out/'history.ndjson').read_bytes().splitlines()),9)
            for relative,payload in legacy_evidence.items():
                self.assertEqual((out/relative).read_bytes(),payload)

    def test_invalid_cli_never_writes(self):
        with traffic_fixture() as (root, env, command):
            for invalid in (['--samples','0'],['--samples','bad','--samples','1'],['--samples','2147483648'],['--samples','١'],['--samples'],['--out-dir',''],['--as-of-unix-ms','0'],['--max-sample-age-secs','-1'],['--top-n','1.5'],['--unknown','x']):
                complete(command+invalid, env, 2)
                self.assertFalse((root/'out').exists(), invalid)

    def test_invalid_history_ownership_never_starts_pending_or_probes(self):
        with traffic_fixture() as (root,env,command):
            regular=root/'saved.ndjson'; regular.write_text(''); regular.chmod(0o600)
            symbolic=root/'symbolic'; symbolic.symlink_to(regular)
            hard=root/'hard'; os.link(regular,hard)
            unsafe=root/'unsafe'; unsafe.write_text(''); unsafe.chmod(0o666)
            for path in (root/'out/attempt_state.json',root/'config.json',symbolic,hard,unsafe):
                complete(command+['--history-path',str(path)],env,2)
                self.assertFalse((root/'out').exists(),path)

    def test_clock_collision_recovery_and_failure_history(self):
        with traffic_fixture() as (root,env,command):
            date=root/'bin/date'; date.write_text('#!/bin/sh\necho 2020-01-01T00:00:00+00:00\n'); date.chmod(0o755)
            complete(command,env)
            first=canonical(root)
            complete(command,dict(env,TRAFFIC_FAIL='yes'),1)
            failed=canonical(root,1)
            self.assertEqual(failed['attempt']['sequence'],2)
            self.assertEqual(failed['last_success']['sequence'],1)
            self.assertFalse(failed['collection']['complete'])
            complete(command,env)
            recovered=canonical(root)
            self.assertEqual(recovered['attempt']['sequence'],3)
            self.assertEqual(recovered['last_success']['sequence'],3)
            self.assertNotEqual(first['run_dir'],recovered['run_dir'])
            self.assertEqual(len(list((root/'out/runs').iterdir())),3)

    def test_historical_replay_is_read_only_and_no_network(self):
        with traffic_fixture() as (root,env,command):
            complete(command,env)
            history=root/'out/history.ndjson'
            original=(history.read_bytes(),history.stat().st_mtime_ns)
            complete(command+['--out-dir',str(root/'replay'),'--summary-only','--history-path',str(history),'--as-of-unix-ms','0'],dict(env,TRAFFIC_FAIL='yes'))
            self.assertEqual((history.read_bytes(),history.stat().st_mtime_ns),original)
            report=canonical(root,out=root/'replay')
            self.assertEqual(report['collection']['mode'],'history_replay')
            self.assertEqual(report['as_of_unix_ms'],0)

    def test_empty_replay_history_is_failed_without_mutation(self):
        with traffic_fixture() as (root,env,command):
            complete(command,env)
            empty=root/'empty.ndjson'; empty.write_text(''); empty.chmod(0o600)
            original=empty.stat().st_mtime_ns
            complete(command+['--summary-only','--history-path',str(empty)],dict(env,TRAFFIC_FAIL='yes'),1)
            report=canonical(root,1)
            self.assertEqual(report['attempt']['result'],'failed')
            self.assertEqual(report['last_success']['sequence'],1)
            self.assertFalse(report['collection']['complete'])
            self.assertEqual((empty.read_bytes(),empty.stat().st_mtime_ns),(b'',original))

    def start_paused(self, root, env, command, name='release'):
        gate = root/name
        process = subprocess.Popen(command, env=dict(env, TRAFFIC_GATE=str(gate)), stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, start_new_session=True)
        self.addCleanup(self.stop_process, process)
        deadline = time.monotonic()+5
        while not gate.with_suffix('.entered').exists() and time.monotonic()<deadline:
            if process.poll() is not None: self.fail(str(process.communicate()))
            time.sleep(.01)
        self.assertTrue(gate.with_suffix('.entered').exists())
        return process, gate

    @staticmethod
    def stop_process(process):
        if process.poll() is None: os.killpg(process.pid, signal.SIGKILL)
        process.communicate()

    def test_old_completion_cannot_replace_newer_failed_or_pending(self):
        for pending in (False, True):
            with self.subTest(pending=pending), traffic_fixture() as (root,env,command):
                older,release=self.start_paused(root,env,command)
                if pending:
                    newer,_=self.start_paused(root,env,command,'newer')
                else:
                    complete(command,dict(env,TRAFFIC_FAIL='yes'),1)
                before=(root/'out/attempt_state.json').read_bytes()
                release.touch()
                output=older.communicate(timeout=10)
                self.assertEqual(older.returncode,3,output)
                self.assertEqual((root/'out/attempt_state.json').read_bytes(),before)
                report=canonical(root,1)
                self.assertEqual(report['attempt']['sequence'],2)
                self.assertEqual(report['attempt']['result'],'pending' if pending else 'failed')
                self.assertFalse(report['collection']['complete'])

    def test_shared_history_between_output_roots_has_no_lost_updates(self):
        with traffic_fixture() as (root,env,command):
            history=root/'shared/history.ndjson'
            older,release=self.start_paused(root,env,command+['--history-path',str(history)])
            complete(command+['--out-dir',str(root/'second'),'--history-path',str(history)],env)
            release.touch()
            output=older.communicate(timeout=10)
            self.assertEqual(older.returncode,0,output)
            rows=[json.loads(line) for line in history.read_text().splitlines()]
            self.assertEqual(len(rows),6)
            self.assertEqual(len({row['run_id'] for row in rows}),2)
            self.assertEqual(len(Path(canonical(root)['run_dir']).joinpath('history.ndjson').read_text().splitlines()),6)

    def test_broken_store_prevents_probe_and_reports_unknown(self):
        for fault in ('state','lock','digest','cache','write'):
            with self.subTest(fault=fault), traffic_fixture() as (root,env,command):
                complete(command,env)
                out=root/'out'
                if fault=='state': (out/'attempt_state.json').unlink()
                if fault=='lock': (out/'.observability-attempt.lock').unlink()
                if fault=='digest':
                    state=json.loads((out/'attempt_state.json').read_text()); (out/state['publication']['json_path']).write_text('{}')
                if fault=='cache': (out/'latest_summary.json').write_text('{}')
                if fault=='write':
                    (out/'latest_summary.md').unlink(); (out/'latest_summary.md').mkdir()
                self.assertEqual(canonical(root,2)['attempt']['result'],'unknown')
                if fault in ('state','lock','write'):
                    gate=root/'release'
                    result=subprocess.run(command,env=dict(env,TRAFFIC_GATE=str(gate)),capture_output=True,timeout=10)
                    self.assertNotEqual(result.returncode,0)
                    self.assertFalse(gate.with_suffix('.entered').exists())

    def test_history_and_summary_write_failures_publish_failed(self):
        for fault in ('history','summary'):
            with self.subTest(fault=fault), traffic_fixture() as (root,env,command):
                process,release=self.start_paused(root,env,command)
                state=json.loads((root/'out/attempt_state.json').read_text())
                run=root/'out'/state['latest_attempt']['run_directory']
                if fault=='history': (run/'history.ndjson').mkdir()
                else: (run/'report/latest_summary.json').mkdir()
                release.touch()
                output=process.communicate(timeout=10)
                self.assertEqual(process.returncode,1,output)
                self.assertFalse(canonical(root,1)['collection']['complete'])

    def test_real_outer_reads_validated_traffic_and_preserves_failure(self):
        spec=importlib.util.spec_from_file_location('traffic_outer_fixture',SCRIPTS/'p2p-real-env-observability-attempt.test.py')
        module=importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
        for failed in (False,True):
            with self.subTest(failed=failed),traffic_fixture() as (root,env,command):
                outer=module.AttemptWrapperTests('runTest'); outer.setUp()
                try:
                    scripts=root/'scripts'
                    for name in ('p2p-real-env-triad-snapshot.sh','p2p-real-env-host-monitor.sh','oasis7-node-wasm-metrics-monitor.sh'):
                        shutil.copy2(outer.root/'scripts'/name,scripts/name)
                    for name in ('p2p-real-env-observability-monitor.sh','p2p-real-env-observability-summary.py'):
                        shutil.copy2(SCRIPTS/name,scripts/name)
                    environment=dict(env,ATTEMPT_TEST_FIXTURES=str(SCRIPTS.parent/'fixtures/p2p_real_env_observability'))
                    if failed: environment['TRAFFIC_FAIL']='yes'
                    outer_command=['bash',str(scripts/'p2p-real-env-observability-monitor.sh'),'--out-dir',str(root/'outer'),'--traffic-samples','1','--traffic-interval-secs','1']
                    complete(outer_command,environment,int(failed))
                    state=json.loads((root/'outer/attempt_state.json').read_text())
                    run=root/'outer'/state['latest_attempt']['run_directory']
                    traffic=json.loads((run/'traffic/canonical_summary.json').read_text())
                    self.assertTrue(traffic['canonical_state']['validated'])
                    self.assertEqual(traffic['collection']['complete'],not failed)
                    merged=json.loads((run/'report/latest_summary.json').read_text())
                    self.assertEqual(merged['traffic']['collection_complete'],not failed)
                finally: outer.doCleanups()

    def test_outer_dangling_traffic_protocol_never_falls_back(self):
        spec=importlib.util.spec_from_file_location('traffic_outer_legacy',SCRIPTS/'p2p-real-env-observability-attempt.test.py')
        module=importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
        for artifact in ('.observability-initialized','attempt_state.json'):
            with self.subTest(artifact=artifact):
                outer=module.AttemptWrapperTests('runTest'); outer.setUp()
                try:
                    child=outer.root/'scripts/p2p-real-env-traffic-monitor.sh'
                    child.write_text(child.read_text().replace("(out / 'latest_summary.json').write_text('{}')", "(out / 'latest_summary.json').write_text('{}')\n    (out / "+repr(artifact)+").symlink_to(out/'missing')"))
                    outer.complete(outer.start(arguments=[]),1)
                    state=outer.canonical(1)
                    run=outer.out/state['attempt']['run_directory']
                    child_report=json.loads((run/'traffic/canonical_summary.json').read_text())
                    self.assertEqual(child_report['attempt']['result'],'unknown')
                    self.assertFalse(child_report['collection']['complete'])
                finally: outer.doCleanups()

    def test_pending_precedes_first_probe_and_survives_sigkill(self):
        with traffic_fixture() as (root,env,command):
            gate=root/'release'
            process=subprocess.Popen(command,env=dict(env,TRAFFIC_GATE=str(gate)),stdout=subprocess.PIPE,stderr=subprocess.PIPE,start_new_session=True)
            try:
                deadline=time.monotonic()+5
                while not gate.with_suffix('.entered').exists() and time.monotonic()<deadline:
                    if process.poll() is not None: self.fail(str(process.communicate()))
                    time.sleep(.01)
                self.assertTrue(gate.with_suffix('.entered').exists())
                state=json.loads((root/'out/attempt_state.json').read_text())
                self.assertEqual(state['latest_attempt']['result'],'pending')
            finally:
                if process.poll() is None: os.killpg(process.pid,signal.SIGKILL)
                process.communicate()
            self.assertEqual(json.loads((root/'out/attempt_state.json').read_text())['latest_attempt']['result'],'pending')

if __name__ == '__main__': unittest.main()
