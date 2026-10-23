#!/usr/bin/env python3
"""Offline integration proof for the real snapshot wrapper publication boundary."""
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import time
import unittest

SOURCE = Path(__file__).resolve().parent

class SnapshotDurable(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / 'scripts').mkdir()
        for name in ('p2p-real-env-triad-snapshot.sh', 'p2p-real-env-observability-attempt.py'):
            shutil.copy2(SOURCE / name, self.root / 'scripts' / name)
        if os.environ.get('SNAPSHOT_BASELINE_REF'):
            for name in ('p2p-real-env-triad-snapshot.sh', 'p2p-real-env-observability-attempt.py'):
                (self.root/'scripts'/name).write_bytes(subprocess.check_output(['git','show',os.environ['SNAPSHOT_BASELINE_REF']+':scripts/'+name],cwd=SOURCE.parent))
        self.bin = self.root / 'bin'; self.bin.mkdir()
        self.write('systemctl', '#!/bin/sh\necho active\n')
        self.write('curl', r'''#!/usr/bin/env python3
import json, os, pathlib, sys, time
root = pathlib.Path(os.environ['TEST_ROOT'])
(root / 'probed').touch()
if os.environ.get('BARRIER'):
    while not (root / 'release').exists(): time.sleep(.02)
if os.environ.get('FAIL'): sys.exit(22)
if os.environ.get('MALFORMED'): print('{}'); sys.exit()
print(json.dumps({'ok': False if os.environ.get('UNHEALTHY') else True} if sys.argv[-1].endswith('health') else {'node_id':'offline','world_id':'offline','running':True}))
''')
        self.env = dict(os.environ, TEST_ROOT=str(self.root), PATH=str(self.bin)+os.pathsep+os.environ['PATH'], PYTHONDONTWRITEBYTECODE='1')
        self.out = self.root / 'out'
    def write(self, name, content):
        path = self.bin / name; path.write_text(content); path.chmod(0o755)
    def args(self, *extra):
        return ['bash', str(self.root/'scripts/p2p-real-env-triad-snapshot.sh'), '--samples','1','--interval-secs','1','--out-dir',str(self.out),'--world-id','offline','--node','label=one,mode=local,service=offline,status_url=http://offline/status,health_url=http://offline/health,env_file=/offline/missing',*extra]
    def run_snapshot(self, extra=(), **env):
        return subprocess.run(self.args(*extra), env=dict(self.env, **env), text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    def read(self):
        result = subprocess.run(['python3',str(self.root/'scripts/p2p-real-env-observability-attempt.py'),'read','--collector','snapshot','--out-dir',str(self.out)],env=self.env,text=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        return result, json.loads(result.stdout)
    def test_default_no_argument_entry_begins_without_array_error(self):
        self.write('ssh','#!/bin/sh\nexit 255\n')
        self.write('sshpass','#!/bin/sh\nexit 255\n')
        self.write('sleep','#!/bin/sh\nexit 0\n')
        result=subprocess.run(['bash',str(self.root/'scripts/p2p-real-env-triad-snapshot.sh')],env=self.env,text=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        self.assertNotIn('unbound variable',result.stderr)
        state=json.loads((self.root/'.tmp/p2p_real_env_triad/attempt_state.json').read_text())
        self.assertEqual(state['latest_attempt']['sequence'],1)
        self.assertEqual(state['latest_attempt']['result'],'failed')
    def test_invalid_cli_never_writes_or_probes(self):
        for extra in (('--samples','0'),('--samples','000'),('--samples','999999999999'),('--samples','１２'),('--samples','0','--samples','1'),('--node','label=..,mode=local,service=x,status_url=x,health_url=x,env_file=x'),('--node','label=one,mode=local,service=x,status_url=x,health_url=x,env_file=x'),('--node','label=bad,mode=local,service=x,status_url=x,health_url=x,env_file=x,password_env=bad-name'),('--out-dir',''),('--bogus','x'),('--samples',)):
            with self.subTest(extra=extra):
                result=self.run_snapshot(extra)
                self.assertNotEqual(result.returncode,0)
                self.assertFalse(self.out.exists(), result.stderr)
                self.assertFalse((self.root/'probed').exists())
    def test_invalid_occurrence_cannot_be_overridden(self):
        result=self.run_snapshot(('--samples','0','--samples','1'))
        self.assertNotEqual(result.returncode,0)
        self.assertFalse(self.out.exists())
    def test_success_failure_history_recovery_and_unique_runs(self):
        self.assertEqual(self.run_snapshot().returncode,0)
        _, first=self.read(); self.assertTrue(first['collection']['complete'])
        self.assertEqual(first['collection']['node_labels'],['one'])
        self.assertNotEqual(self.run_snapshot(FAIL='1').returncode,0)
        code, failed=self.read(); self.assertNotEqual(code.returncode,0)
        self.assertEqual(failed['claim_status'],'blocked')
        self.assertFalse(failed['collection']['complete'])
        self.assertEqual(failed['last_success']['sequence'],1)
        self.assertEqual(self.run_snapshot(UNHEALTHY='1').returncode,0)
        _, recovered=self.read(); self.assertTrue(recovered['collection']['complete'])
        self.assertEqual(recovered['latest_attempt']['sequence'],3)
        self.assertEqual(len(list((self.out/'runs').iterdir())),3)
    def test_pending_before_first_probe_and_kill(self):
        process=subprocess.Popen(self.args(),env=dict(self.env,BARRIER='1'),stdout=subprocess.PIPE,stderr=subprocess.PIPE,start_new_session=True)
        self.addCleanup(lambda: process.poll() is None and os.killpg(process.pid,signal.SIGKILL))
        deadline=time.monotonic()+10
        while not (self.root/'probed').exists() and time.monotonic()<deadline: time.sleep(.02)
        self.assertTrue((self.root/'probed').exists())
        _, pending=self.read(); self.assertEqual(pending['latest_attempt']['result'],'pending')
        self.assertEqual(pending['claim_status'],'blocked')
        os.killpg(process.pid,signal.SIGKILL); process.communicate()
        _, pending=self.read(); self.assertEqual(pending['latest_attempt']['result'],'pending')
    def test_older_success_cannot_replace_new_failure(self):
        process=subprocess.Popen(self.args(),env=dict(self.env,BARRIER='1'),stdout=subprocess.PIPE,stderr=subprocess.PIPE,start_new_session=True)
        self.addCleanup(lambda: process.poll() is None and os.killpg(process.pid,signal.SIGKILL))
        deadline=time.monotonic()+10
        while not (self.root/'probed').exists() and time.monotonic()<deadline: time.sleep(.02)
        self.assertNotEqual(self.run_snapshot(FAIL='1').returncode,0)
        (self.root/'release').touch(); process.communicate(timeout=15)
        self.assertNotEqual(process.returncode,0)
        _, latest=self.read(); self.assertEqual(latest['latest_attempt']['sequence'],2)
        self.assertEqual(latest['latest_attempt']['result'],'failed')
    def test_initial_publication_failure_never_probes(self):
        self.out.mkdir()
        (self.out/'latest_summary.json').mkdir()
        result=self.run_snapshot()
        self.assertNotEqual(result.returncode,0)
        self.assertFalse((self.root/'probed').exists())
        result, report=self.read(); self.assertNotEqual(result.returncode,0)
        self.assertEqual(report['claim_status'],'blocked')
    def test_fixed_clock_older_success_cannot_replace_new_pending(self):
        self.write('date', '#!/bin/sh\necho 2026-01-01T00:00:00Z\n')
        first=subprocess.Popen(self.args(),env=dict(self.env,BARRIER='1'),stdout=subprocess.PIPE,stderr=subprocess.PIPE,start_new_session=True)
        self.addCleanup(lambda: first.poll() is None and os.killpg(first.pid,signal.SIGKILL))
        deadline=time.monotonic()+10
        while not (self.root/'probed').exists() and time.monotonic()<deadline: time.sleep(.02)
        (self.root/'probed').unlink()
        second=subprocess.Popen(self.args(),env=dict(self.env,BARRIER='1'),stdout=subprocess.PIPE,stderr=subprocess.PIPE,start_new_session=True)
        self.addCleanup(lambda: second.poll() is None and os.killpg(second.pid,signal.SIGKILL))
        deadline=time.monotonic()+10
        while not (self.root/'probed').exists() and time.monotonic()<deadline: time.sleep(.02)
        os.killpg(second.pid,signal.SIGKILL); second.communicate()
        (self.root/'release').touch(); first.communicate(timeout=15)
        self.assertNotEqual(first.returncode,0)
        _, latest=self.read(); self.assertEqual(latest['latest_attempt']['sequence'],2)
        self.assertEqual(latest['latest_attempt']['result'],'pending')
        self.assertEqual(len(list((self.out/'runs').iterdir())),2)
    def test_broken_store_rejected_before_probe(self):
        for fault in ('anchor','state','rollback','digest','wrong_collector'):
            with self.subTest(fault=fault):
                self.out=self.root/fault
                self.assertEqual(self.run_snapshot().returncode,0)
                state_path=self.out/'attempt_state.json'; state=json.loads(state_path.read_text())
                if fault == 'anchor': (self.out/'.observability-initialized').unlink()
                elif fault == 'state': state_path.write_text('{')
                elif fault == 'rollback':
                    state['last_sequence']=0; state_path.write_text(json.dumps(state))
                elif fault == 'digest':
                    (self.out/state['publication']['json_path']).write_text('{}')
                else:
                    anchor=self.out/'.observability-initialized'; value=json.loads(anchor.read_text()); value['collector']='host'; anchor.write_text(json.dumps(value))
                if (self.root/'probed').exists(): (self.root/'probed').unlink()
                if fault != 'digest':
                    self.assertNotEqual(self.run_snapshot().returncode,0)
                    self.assertFalse((self.root/'probed').exists())
                result, report=self.read(); self.assertNotEqual(result.returncode,0)
                self.assertEqual(report['claim_status'],'blocked')
    def test_later_failed_sample_replaces_raw_status(self):
        self.write('curl', r'''#!/usr/bin/env python3
import json, os, pathlib, sys
root=pathlib.Path(os.environ['TEST_ROOT']); counter=root/'counter'
n=int(counter.read_text())+1 if counter.exists() else 1; counter.write_text(str(n))
if n > 2: sys.exit(22)
print(json.dumps({'ok':True} if sys.argv[-1].endswith('health') else {'node_id':'offline','world_id':'offline','running':True}))
''')
        self.write('sleep','#!/bin/sh\nexit 0\n')
        self.assertNotEqual(self.run_snapshot(('--samples','2')).returncode,0)
        _, report=self.read()
        raw=json.loads((Path(report['run_dir'])/'nodes/one/status.json').read_text())
        self.assertIn('fetch_error',raw)
        self.assertEqual(report['nodes']['one']['success_sample_count'],1)
        self.assertEqual(report['nodes']['one']['failed_sample_count'],1)
    def test_terminal_publication_error_never_promotes_success(self):
        self.assertEqual(self.run_snapshot().returncode,0)
        real_python=shutil.which('python3')
        self.write('python3', '#!'+real_python+'\nimport os, pathlib, sys\nif "finish" in sys.argv and "--collector" in sys.argv:\n p=pathlib.Path(os.environ["TEST_ROOT"])/"out/latest_summary.json"; p.unlink(); p.mkdir()\nos.execv('+repr(real_python)+', ['+repr(real_python)+']+sys.argv[1:])\n')
        self.assertNotEqual(self.run_snapshot().returncode,0)
        state=json.loads((self.out/'attempt_state.json').read_text())
        self.assertEqual(state['last_success']['sequence'],1)
        result, report=self.read(); self.assertNotEqual(result.returncode,0)
        self.assertEqual(report['claim_status'],'blocked')
    def test_actual_outer_consumes_own_snapshot_and_wasm_paths(self):
        for name in ('p2p-real-env-observability-monitor.sh','p2p-real-env-observability-summary.py'):
            shutil.copy2(SOURCE/name,self.root/'scripts'/name)
        # Child peers are offline fixtures; snapshot and merger remain real.
        fixtures=SOURCE.parent/'fixtures/p2p_real_env_observability'
        shutil.copytree(fixtures,self.root/'fixtures')
        for name, source in (('host-monitor','host_summary.json'),('traffic-monitor','traffic_summary.json')):
            path=self.root/'scripts'/('p2p-real-env-'+name+'.sh')
            path.write_text('#!/usr/bin/env bash\nset -eu\nwhile [[ $# -gt 0 ]]; do if [[ "$1" == --out-dir ]]; then out=$2; fi; shift; done\nmkdir -p "$out/legacy"\ncp fixtures/'+source+' "$out/latest_summary.json"\ncp fixtures/'+source+' "$out/legacy/summary.json"\n'); path.chmod(0o755)
        path=self.root/'scripts/oasis7-node-wasm-metrics-monitor.sh'
        path.write_text('#!/usr/bin/env bash\nset -eu\nwhile [[ $# -gt 0 ]]; do case "$1" in --status-sample-dir) input=$2;; --out-dir) out=$2;; esac; shift; done\nmkdir -p "$out"\nprintf "%s\\n" "$input" >> "$TEST_ROOT/wasm-inputs"\ncp fixtures/local_wasm_summary.json "$out/latest_summary.json"\n'); path.chmod(0o755)
        self.write('ssh', """#!/bin/sh
case "$*" in *systemctl*) echo active;; *healthz*) echo '{"ok":true}';; *curl*) echo '{"node_id":"remote","world_id":"offline","running":true}';; *) exit 1;; esac
""")
        outer=self.root/'outer'
        args=['bash',str(self.root/'scripts/p2p-real-env-observability-monitor.sh'),'--out-dir',str(outer),'--samples','1','--interval-secs','1','--traffic-samples','1','--traffic-interval-secs','1','--local-status-url','http://offline/status','--local-health-url','http://offline/health','--world-id','offline']
        result=subprocess.run(args,env=self.env,text=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        self.assertEqual(result.returncode,0,result.stderr)
        state=json.loads((outer/'attempt_state.json').read_text()); own=outer/state['latest_attempt']['run_directory']
        child=json.loads((own/'snapshot/canonical_summary.json').read_text())
        self.assertTrue(child['collection']['complete'])
        self.assertTrue(Path(child['run_dir']).resolve().is_relative_to((own/'snapshot/runs').resolve()))
        inputs=(self.root/'wasm-inputs').read_text().splitlines()
        self.assertEqual(len(inputs),3)
        for label in ('local_node','sequencer_ecs','storage_ecs'):
            raw=own/'wasm/input'/label/'001.json'
            self.assertEqual(json.loads(raw.read_text())['world_id'],'offline')
        canonical=json.loads((outer/'latest_summary.json').read_text())
        self.assertEqual(canonical['nodes']['local_node']['node_id'],'offline')
        self.assertEqual(canonical['snapshot']['claim_status'],child['claim_status'])
        # A versioned child corrupted after collection cannot recover through its
        # same-run compatibility summary, and cannot feed raw status to WASM.
        original=self.root/'scripts/p2p-real-env-triad-snapshot.sh'
        shutil.copy2(original,self.root/'scripts/snapshot-real.sh')
        original.write_text("""#!/usr/bin/env bash
set -eu
bash ./scripts/snapshot-real.sh "$@"
while [[ $# -gt 0 ]]; do if [[ "$1" == --out-dir ]]; then out=$2; fi; shift; done
printf '{' > "$out/attempt_state.json"
""")
        result=subprocess.run(args,env=self.env,text=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        self.assertNotEqual(result.returncode,0)
        state=json.loads((outer/'attempt_state.json').read_text()); own=outer/state['latest_attempt']['run_directory']
        child=json.loads((own/'snapshot/canonical_summary.json').read_text())
        self.assertEqual(child['claim_status'],'blocked')
        for label in ('local_node','sequencer_ecs','storage_ecs'):
            self.assertEqual(list((own/'wasm/input'/label).iterdir()),[])
        canonical=json.loads((outer/'latest_summary.json').read_text())
        self.assertEqual(canonical['overall']['status'],'blocked')
    def test_malformed_and_lost_lock_fail_closed(self):
        self.assertNotEqual(self.run_snapshot(MALFORMED='1',PYTHONOPTIMIZE='1').returncode,0)
        _, failed=self.read(); self.assertFalse(failed['collection']['complete'])
        (self.out/'.observability-attempt.lock').unlink()
        (self.root/'probed').unlink()
        self.assertNotEqual(self.run_snapshot().returncode,0)
        self.assertFalse((self.root/'probed').exists())
        result, unknown=self.read(); self.assertNotEqual(result.returncode,0)
        self.assertEqual(unknown['claim_status'],'blocked')

if __name__ == '__main__': unittest.main()
