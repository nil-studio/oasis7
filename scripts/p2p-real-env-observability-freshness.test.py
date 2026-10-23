#!/usr/bin/env python3
"""Offline reducer -> merged JSON/Markdown freshness regressions."""
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent



class MergeFreshnessTests(unittest.TestCase):
    def traffic(self):
        records = [dict(label=label, captured_at_unix_ms=t, status_fetch_ok=True,
                        node_id='n', world_id='w', status_observed_at_unix_ms=t, traffic={})
                   for label in ('local_node', 'sequencer_ecs', 'storage_ecs') for t in (0, 60_000)]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'history.jsonl').write_text(''.join(json.dumps(row) + '\n' for row in records))
            subprocess.run([sys.executable, str(ROOT / 'scripts/traffic-monitor-summary.py'),
                            '--layout', 'triad', '--history-path', str(root / 'history.jsonl'),
                            '--summary-json', str(root / 'traffic.json'), '--summary-md', str(root / 'traffic.md'),
                            '--window-minutes', '1', '--history-retention-minutes', '60', '--top-n', '3',
                            '--as-of-unix-ms', '60000'], check=True)
            return json.loads((root / 'traffic.json').read_text())

    def merge(self, traffic, as_of):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'traffic.json').write_text(json.dumps(traffic))
            (root / 'empty.json').write_text('{}')
            (root / 'snapshot.json').write_text('{"claim_status":"pass_candidate"}')
            cmd = [sys.executable, str(ROOT / 'scripts/p2p-real-env-observability-summary.py'),
                   '--as-of-unix-ms', str(as_of), '--snapshot-summary', str(root / 'snapshot.json'),
                   '--host-summary', str(root / 'empty.json'), '--traffic-summary', str(root / 'traffic.json'),
                   '--summary-json', str(root / 'summary.json'), '--summary-md', str(root / 'summary.md')]
            for label in ('local', 'sequencer', 'storage'):
                cmd += [f'--{label}-wasm-summary', str(root / 'empty.json'),
                        f'--{label}-status-json', str(ROOT / f'fixtures/p2p_real_env_observability/{label}_status.json')]
            subprocess.run(cmd, check=True)
            return json.loads((root / 'summary.json').read_text()), (root / 'summary.md').read_text()

    def test_positive_exact_boundary_and_merge_time_expiry(self):
        traffic = self.traffic()
        fresh, markdown = self.merge(traffic, 60_000)
        module = fresh['nodes']['local_node']['modules']['traffic_control_plane']
        self.assertEqual(module['freshness']['state'], 'fresh')
        self.assertTrue(module['current_measurement_available'])
        self.assertNotIn('traffic_freshness_unknown', module['alerts'])
        stale, markdown = self.merge(traffic, 120_001)
        node = stale['nodes']['local_node']
        self.assertEqual(node['traffic']['freshness']['sample_age_ms'], 60_001)
        self.assertEqual(node['traffic']['freshness']['state'], 'stale')
        self.assertEqual(node['traffic']['freshness']['producer']['state'], 'stale')
        self.assertFalse(node['traffic']['current_measurement_available'])
        self.assertEqual(node['traffic']['reported_window']['end_unix_ms'], 60_000)
        self.assertIn('traffic_window_uncovered', node['alerts'])
        self.assertNotEqual(stale['overall']['status'], 'pass_candidate')
        self.assertIn('Traffic freshness: `stale` producer=`stale`', markdown)
        self.assertIn('historical window values', markdown)

    def test_failure_future_inconsistency_and_unknown_policy(self):
        for change, expected in (
            (lambda n: n['latest_attempt'].update(result='failed', error='curl_failed'), 'failed'),
            (lambda n: n.update(as_of_unix_ms=60_001), 'unknown'),
            (lambda n: n['latest_attempt'].update(captured_at_unix_ms=60_001), 'unknown'),
            (lambda n: n['last_success'].update(status_observed_at_unix_ms=60_001), 'fresh'),
        ):
            traffic = self.traffic()
            change(traffic['nodes']['local_node'])
            merged, _ = self.merge(traffic, 60_000)
            node = merged['nodes']['local_node']
            self.assertEqual(node['traffic']['freshness']['state'], expected)
            self.assertFalse(node['traffic']['current_measurement_available'])
            self.assertTrue(node['alerts'])
        for policy in ({}, {'version': 1, 'max_sample_age_ms': 0, 'max_sample_gap_ms': 1}):
            traffic = self.traffic()
            traffic['freshness_policy'] = policy
            merged, _ = self.merge(traffic, 60_000)
            self.assertEqual(merged['nodes']['local_node']['traffic']['freshness']['state'], 'unknown')
        merged, _ = self.merge(self.traffic(), 59_999)
        self.assertEqual(merged['nodes']['local_node']['traffic']['freshness']['state'], 'unknown')

    def test_future_as_of_rejected_before_outputs_or_collection(self):
        future = str(time.time_ns() // 1_000_000 + 3_600_000)
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory) / 'must-not-exist'
            result = subprocess.run(['bash', str(ROOT / 'scripts/p2p-real-env-observability-monitor.sh'),
                                     '--as-of-unix-ms', future, '--out-dir', str(out)],
                                    capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('current UTC time', result.stderr)
            self.assertFalse(out.exists())
            result = subprocess.run([sys.executable, str(ROOT / 'scripts/p2p-real-env-observability-summary.py'),
                                     '--as-of-unix-ms', future,
                                     '--snapshot-summary', 'unused', '--host-summary', 'unused',
                                     '--traffic-summary', 'unused', '--local-wasm-summary', 'unused',
                                     '--sequencer-wasm-summary', 'unused', '--storage-wasm-summary', 'unused',
                                     '--summary-json', str(out), '--summary-md', str(out)],
                                    capture_output=True, text=True)
            self.assertEqual(result.returncode, 2)
            self.assertIn('current UTC time', result.stderr)
            self.assertFalse(out.exists())

    def test_outer_historical_pair_rejected_before_collection(self):
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory) / 'must-not-exist'
            result = subprocess.run(['bash', str(ROOT / 'scripts/p2p-real-env-observability-monitor.sh'),
                                     '--as-of-unix-ms', '60000', '--out-dir', str(out)],
                                    capture_output=True, text=True)
            self.assertEqual(result.returncode, 2)
            self.assertIn('--traffic-history-path', result.stderr)
            self.assertFalse(out.exists())

    def test_outer_routes_freshness_only_to_traffic_and_report(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            scripts = root / 'scripts'
            scripts.mkdir()
            shutil.copy(ROOT / 'scripts/p2p-real-env-observability-monitor.sh', scripts)
            (scripts / 'p2p-real-env-observability-summary.py').symlink_to(ROOT / 'scripts/p2p-real-env-observability-summary.py')
            stub = root / 'stub.py'
            stub.write_text("""import json,sys,shutil
from pathlib import Path
name=sys.argv[1]; args=sys.argv[2:]; root=Path(__file__).parent
(root/(name+'.args')).write_text(json.dumps(args))
out=Path(args[args.index('--out-dir')+1]); out.mkdir(parents=True,exist_ok=True)
fixtures=Path(ROOT)/'fixtures/p2p_real_env_observability'
if 'snapshot' in name:
 out=out/'run';out.mkdir();shutil.copy(fixtures/'snapshot_summary.json',out/'summary.json')
 for label,prefix in [('local_node','local'),('sequencer_ecs','sequencer'),('storage_ecs','storage')]:
  node=out/'nodes'/label;node.mkdir(parents=True);shutil.copy(fixtures/(prefix+'_status.json'),node/'status.json')
elif 'host' in name:
 out=out/'run';out.mkdir();shutil.copy(fixtures/'host_summary.json',out/'summary.json')
elif 'traffic' in name:
 shutil.copy(fixtures/'traffic_summary.json',out/'latest_summary.json')
else:
 shutil.copy(fixtures/'local_wasm_summary.json',out/'latest_summary.json')
""".replace('ROOT', repr(str(ROOT))))
            for name in ('p2p-real-env-triad-snapshot.sh', 'p2p-real-env-host-monitor.sh',
                         'oasis7-node-wasm-metrics-monitor.sh'):
                path = scripts / name
                path.write_text('#!/usr/bin/env bash\nexec ' + sys.executable + ' "$(dirname "$0")/../stub.py" "$(basename "$0")" "$@"\n')
                path.chmod(0o755)
            # Use the real traffic collector; summary-only must accept the route and never sample.
            for name in ('traffic-monitor-summary.py', 'traffic_monitor_freshness.py'):
                shutil.copy(ROOT / 'scripts' / name, scripts / name)
            shutil.copy(ROOT / 'scripts/p2p-real-env-traffic-monitor.sh', scripts / 'traffic-real.sh')
            wrapper = scripts / 'p2p-real-env-traffic-monitor.sh'
            wrapper.write_text('#!/usr/bin/env bash\n' + sys.executable +
                               ' -c \'import json,sys;from pathlib import Path;Path(sys.argv[1]).write_text(json.dumps(sys.argv[2:]))\' "$(dirname "$0")/../p2p-real-env-traffic-monitor.sh.args" "$@"\n'
                               + 'exec bash "$(dirname "$0")/traffic-real.sh" "$@"\n')
            wrapper.chmod(0o755)
            history = root / 'saved.ndjson'
            history.write_text(''.join(json.dumps(dict(label=label, captured_at_unix_ms=t,
                                      status_fetch_ok=True, node_id='n', world_id='w',
                                      status_observed_at_unix_ms=t, traffic={})) + '\n'
                                      for label in ('local_node', 'sequencer_ecs', 'storage_ecs')
                                      for t in (0, 60_000, 120_000)))
            original = (history.read_bytes(), history.stat().st_mtime_ns)
            bin_dir = root / 'bin'
            bin_dir.mkdir()
            for command in ('curl', 'ssh', 'sshpass'):
                guard = bin_dir / command
                guard.write_text('#!/usr/bin/env bash\necho network-called >> "' + str(root / 'network-called') + '"\nexit 99\n')
                guard.chmod(0o755)
            env = dict(os.environ, PATH=str(bin_dir) + os.pathsep + os.environ['PATH'])
            subprocess.run(['bash', str(scripts / 'p2p-real-env-observability-monitor.sh'),
                            '--as-of-unix-ms', '60000', '--traffic-history-path', str(history), '--max-sample-age-secs', '31',
                            '--max-sample-gap-secs', '17', '--local-status-url', 'http://offline.invalid', '--out-dir', str(root / 'out')],
                           check=True, capture_output=True, env=env)
            self.assertEqual((history.read_bytes(), history.stat().st_mtime_ns), original)
            self.assertFalse((root / 'network-called').exists())
            for name in ('p2p-real-env-triad-snapshot.sh', 'p2p-real-env-host-monitor.sh'):
                args = json.loads((root / (name + '.args')).read_text())
                self.assertNotIn('--as-of-unix-ms', args)
                self.assertNotIn('--max-sample-age-secs', args)
                self.assertNotIn('--max-sample-gap-secs', args)
            args = json.loads((root / 'p2p-real-env-traffic-monitor.sh.args').read_text())
            self.assertIn('--summary-only', args)
            self.assertEqual(args[args.index('--history-path') + 1], str(history))
            for flag, value in (('--as-of-unix-ms', '60000'), ('--max-sample-age-secs', '31'), ('--max-sample-gap-secs', '17')):
                self.assertEqual(args[args.index(flag) + 1], value)
            self.assertEqual(json.loads((root / 'out/latest_summary.json').read_text())['as_of_unix_ms'], 60000)

    def test_producer_unknown_and_legacy_never_green(self):
        traffic = self.traffic()
        traffic['nodes']['local_node']['freshness']['producer']['state'] = 'unknown'
        merged, _ = self.merge(traffic, 60_000)
        self.assertIn('traffic_producer_unknown', merged['nodes']['local_node']['alerts'])
        self.assertFalse(merged['nodes']['local_node']['modules']['traffic_control_plane']['window_full_covered'])
        legacy = json.loads((ROOT / 'fixtures/p2p_real_env_observability/traffic_summary.json').read_text())
        merged, markdown = self.merge(legacy, 60_000)
        self.assertEqual(merged['traffic']['aggregate']['total_payload_bytes'], 780646)
        self.assertFalse(merged['traffic']['current_measurement_available'])
        self.assertIn('traffic_freshness_unknown', merged['nodes']['sequencer_ecs']['alerts'])
        self.assertIn('Traffic freshness: `unknown`', markdown)


if __name__ == '__main__':
    unittest.main()
