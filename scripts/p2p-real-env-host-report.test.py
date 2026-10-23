#!/usr/bin/env python3
"""Offline host failure contracts through the actual report merger."""
import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parent
FIXTURES = SCRIPTS.parent / 'fixtures/p2p_real_env_observability'
LABELS = ('local_node', 'sequencer_ecs', 'storage_ecs')

class HostReportTests(unittest.TestCase):
    def host(self):
        host = json.loads((FIXTURES / 'host_summary.json').read_text())
        host['schema_version'] = 2
        host['aggregate'].update(complete=True, expected_sample_count=3, attempted_count=3, failed_sample_count=0, failed_nodes=[])
        for node in host['nodes'].values():
            node.update(sample_count=1, success_sample_count=1, failed_sample_count=0, available=True,
                        latest_attempt={'result': 'success'}, last_success=copy.deepcopy(node['latest']))
        return host

    def merge(self, host):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'host.json').write_text(json.dumps(host))
            snapshot = json.loads((FIXTURES / 'snapshot_summary.json').read_text())
            snapshot.update(claim_status='pass_candidate', failure_signatures=[])
            (root / 'snapshot.json').write_text(json.dumps(snapshot))
            args = ['--snapshot-summary', str(root / 'snapshot.json'), '--host-summary', str(root / 'host.json')]
            for flag, filename in [('traffic-summary','traffic_summary.json'), ('local-wasm-summary','local_wasm_summary.json'), ('sequencer-wasm-summary','sequencer_wasm_summary.json'), ('storage-wasm-summary','storage_wasm_summary.json'), ('local-status-json','local_status.json'), ('sequencer-status-json','sequencer_status.json'), ('storage-status-json','storage_status.json')]:
                args += ['--'+flag, str(FIXTURES / filename)]
            subprocess.run([sys.executable, str(SCRIPTS / 'p2p-real-env-observability-summary.py'), *args, '--run-id', 'fixture', '--run-dir', str(root), '--summary-json', str(root / 'report.json'), '--summary-md', str(root / 'report.md')], check=True)
            return json.loads((root / 'report.json').read_text()), (root / 'report.md').read_text()

    def test_latest_failure_preserves_history_and_diagnostics(self):
        host = self.host()
        host['aggregate'].update(complete=False, failed_sample_count=1, failed_nodes=['local_node'])
        node = host['nodes']['local_node']
        node.update(available=False, latest={}, success_sample_count=0, failed_sample_count=1,
                    latest_attempt={'result':'failed', 'sample_fetch_ok':False, 'stage':'fetch', 'fetch_error':'ssh_failed'}, alerts=['host_sample_failed'])
        report, markdown = self.merge(host)
        self.assertEqual(report['overall']['status'], 'blocked')
        self.assertEqual(report['nodes']['local_node']['host']['latest_attempt']['fetch_error'], 'ssh_failed')
        self.assertTrue(report['nodes']['local_node']['host']['last_success'])
        self.assertIsNone(report['nodes']['local_node']['host']['runtime_cpu_percent'])
        self.assertNotIn('runtime_process_missing', report['nodes']['local_node']['alerts'])
        self.assertIn('ssh_failed', markdown)

    def test_missing_inconsistent_and_unknown_schemas_block(self):
        for change in ['complete', 'node', 'count', 'unknown']:
            with self.subTest(change=change):
                host = self.host()
                if change == 'complete': host['aggregate'].pop('complete')
                if change == 'node': host['nodes'].pop('local_node')
                if change == 'count': host['nodes']['local_node']['sample_count'] = 0
                if change == 'unknown': host['schema_version'] = 99
                report, _ = self.merge(host)
                self.assertEqual(report['overall']['status'], 'blocked')

    def test_complete_and_legacy_reports_remain_compatible(self):
        host = self.host()
        report, _ = self.merge(host)
        self.assertNotEqual(report['overall']['status'], 'blocked')
        host.pop('schema_version')
        report, _ = self.merge(host)
        self.assertNotEqual(report['overall']['status'], 'blocked')

if __name__ == '__main__': unittest.main()
