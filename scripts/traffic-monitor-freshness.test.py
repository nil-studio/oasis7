#!/usr/bin/env python3
import importlib.util
import sys
import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
spec = importlib.util.spec_from_file_location('traffic_summary', Path(__file__).with_name('traffic-monitor-summary.py'))
summary = importlib.util.module_from_spec(spec)
spec.loader.exec_module(summary)


def record(minute, ok=True, **extra):
    return dict(captured_at_unix_ms=minute * 60_000, status_fetch_ok=ok,
                node_id='n', world_id='w', status_observed_at_unix_ms=minute * 60_000,
                **extra)


class FreshnessTests(unittest.TestCase):
    def test_latest_failure_cannot_borrow_success_clock_or_full_coverage(self):
        records = [record(0), record(10)] + [record(m, False) for m in range(11, 31)]
        result = summary.summarize_record_set(records, 10, 3, as_of=30 * 60_000)
        self.assertFalse(result['available'])
        self.assertFalse(result['window']['full_window_covered'])
        self.assertEqual(result['window']['max_sample_gap_ms'], 600_000)

    def reduce(self, records, minute=10, **kwargs):
        return summary.summarize_record_set(records, 10, 3, as_of=minute * 60_000, **kwargs)

    def test_all_failed_and_recovery_preserve_separate_success(self):
        failed = self.reduce([record(9, False), record(10, False)])
        self.assertEqual(failed['freshness']['state'], 'failed')
        self.assertIsNone(failed['last_success'])
        recovered = self.reduce([record(0), record(5, False), record(10)])
        self.assertTrue(recovered['available'])
        self.assertEqual(recovered['last_success']['captured_at_unix_ms'], 600_000)
        self.assertFalse(recovered['window']['full_window_covered'])
        self.assertEqual(recovered['window']['failed_sample_count'], 1)

    def test_contiguous_window_and_sparse_endpoints(self):
        full = self.reduce([record(m) for m in range(11)])
        self.assertTrue(full['window']['full_window_covered'])
        sparse = self.reduce([record(0), record(10)])
        self.assertFalse(sparse['window']['full_window_covered'])
        self.assertEqual(sparse['window']['max_sample_gap_ms'], 600_000)

    def test_stale_capture_and_stale_http200_producer_independent(self):
        self.assertEqual(self.reduce([record(8)])['freshness']['state'], 'stale')
        row = record(10)
        row['status_observed_at_unix_ms'] = 5 * 60_000
        result = self.reduce([row])
        self.assertTrue(result['available'])
        self.assertEqual(result['freshness']['producer']['state'], 'stale')
        rows = [record(m) for m in range(11)]
        rows[-1]['status_observed_at_unix_ms'] = 300_000
        self.assertFalse(self.reduce(rows)['window']['full_window_covered'])
        row.pop('status_observed_at_unix_ms')
        self.assertEqual(self.reduce([row])['freshness']['producer']['state'], 'unknown')

    def test_clock_regression_future_and_invalid_clock_fail_closed(self):
        self.assertIn('collector_clock_regression', self.reduce([record(9), record(8), record(10)])['freshness']['reasons'])
        result = self.reduce([record(10), record(11)])
        self.assertFalse(result['available'])
        self.assertIn('future_capture_time', result['freshness']['reasons'])
        self.assertTrue(self.reduce([record(10), record(11)], historical=True)['available'])
        regressed = self.reduce([record(9), record(8, False)])
        self.assertEqual(regressed['latest_attempt']['captured_at_unix_ms'], 480_000)
        self.assertEqual(regressed['latest_attempt']['result'], 'failed')
        invalid = record(10)
        invalid['captured_at_unix_ms'] = None
        unknown = self.reduce([record(9), invalid])
        self.assertEqual(unknown['latest_attempt']['result'], 'unknown')
        self.assertIsNone(unknown['latest_attempt']['captured_at_unix_ms'])
        for value in [None, True, -1, 1.5, '600000']:
            row = record(10)
            row['captured_at_unix_ms'] = value
            self.assertEqual(self.reduce([row])['freshness']['state'], 'unknown')

    def test_identity_and_counter_reset_prevent_full_window_and_cross_reset_delta(self):
        for field in ['node_id', 'world_id']:
            rows = [record(m) for m in range(11)]
            rows[-1][field] = 'changed'
            result = self.reduce(rows)
            self.assertFalse(result['window']['full_window_covered'])
            self.assertEqual(result['window']['compatible_sample_count'], 1)
        rows = [record(m) for m in range(11)]
        for index, row in enumerate(rows):
            row['traffic'] = {'libp2p_replication': {'observed_since_unix_ms': 1,
                'totals': {'inbound': {'messages': index if index < 10 else 0}}}}
        result = self.reduce(rows)
        self.assertIn('counter_reset', result['freshness']['reasons'])
        self.assertEqual(result['window']['compatible_sample_count'], 1)

    def test_live_discovery_gauges_can_drop_without_resetting_traffic_window(self):
        rows = [record(minute) for minute in range(11)]
        for index, row in enumerate(rows):
            row['traffic'] = {'libp2p_replication': {
                'observed_since_unix_ms': 1,
                'scope': 'application_payload_with_substream_wire_bytes',
                'totals': {'inbound': {'messages': index, 'payload_bytes': index * 100},
                           'outbound': {'messages': index, 'payload_bytes': index * 100}},
                'discovery_work': {'admitted': index, 'rejected_window': 0, 'rejected_inflight': 0,
                    'inflight': 32 if index < 5 else 0, 'window_starts': 128 if index < 5 else 0,
                    'max_window_starts': 128, 'window_ms': 60_000, 'max_inflight': 32,
                    'max_proxy_attempts': 4, 'max_list_ids': 128, 'max_list_bytes': 16_384,
                    'max_cooldown_entries': 1024}}}
        result = self.reduce(rows)
        self.assertTrue(result['window']['full_window_covered'])
        self.assertNotIn('counter_reset', result['freshness']['reasons'])
        self.assertEqual(result['window']['compatible_sample_count'], 11)
        self.assertEqual(result['traffic']['libp2p_replication']['totals']['inbound']['payload_bytes'], 1000)

    def invoke(self, directory, records, layout='single-node', as_of=600_000):
        directory = Path(directory)
        history = directory / 'history.ndjson'
        history.write_text(''.join(json.dumps(row) + '\n' for row in records))
        original = history.read_bytes()
        command = [sys.executable, str(Path(__file__).with_name('traffic-monitor-summary.py')),
                   '--layout', layout, '--history-path', str(history), '--summary-json', str(directory / 'summary.json'),
                   '--summary-md', str(directory / 'summary.md'), '--window-minutes', '10',
                   '--history-retention-minutes', '1', '--top-n', '3', '--as-of-unix-ms', str(as_of)]
        subprocess.run(command, check=True, capture_output=True)
        self.assertEqual(history.read_bytes(), original)
        return json.loads((directory / 'summary.json').read_text())

    def test_historical_asof_readonly_and_triad_common_clock(self):
        with tempfile.TemporaryDirectory() as directory:
            result = self.invoke(directory, [record(0), record(10), record(20)])
            self.assertEqual(result['node']['last_success']['captured_at_unix_ms'], 600_000)
            self.assertEqual(result['history_pruned_count'], 0)
            self.invoke(directory, [record(0), record(9), record(10, False)])
            self.assertIn('Historical diagnostics below', (Path(directory) / 'summary.md').read_text())
            rows = []
            for label in ['local_node', 'sequencer_ecs', 'storage_ecs']:
                rows += [dict(record(m), label=label) for m in [0, 9, 10]]
            result = self.invoke(directory, rows, layout='triad')
            self.assertEqual(result['schema_version'], 2)
            self.assertEqual(result['freshness_policy']['version'], 1)
            self.assertTrue(all(node['as_of_unix_ms'] == result['as_of_unix_ms'] for node in result['nodes'].values()))

    def test_production_retention_handles_out_of_order_without_healing_clock(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'history'
            path.write_text(''.join(json.dumps(record(m)) + '\n' for m in [2, 0, 1]))
            rows, total = summary.prune_and_load_records(path, 1, as_of=120_000)
            self.assertEqual(total, 3)
            self.assertEqual([row['captured_at_unix_ms'] for row in rows], [120_000, 60_000])
            result = self.reduce(rows, minute=2)
            self.assertIn('collector_clock_regression', result['freshness']['reasons'])

    def test_collectors_record_invalid_http_success_and_completion_per_node(self):
        scripts = Path(__file__).parent
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binaries = root / 'bin'
            binaries.mkdir()
            stubs = {
                'curl': '#!/bin/sh\nprintf "%s\n" "${MOCK_STATUS:-not-json}"\n',
                'ssh': '#!/bin/sh\ncase "$*" in *"curl -fsS"*) printf \'%s\\n\' "$MOCK_STATUS";; *) printf \'%s\\n\' \'{"available":false}\';; esac\n',
                'ip': '#!/bin/sh\nexit 1\n',
                'date': '#!/bin/sh\ncase "$1" in +%s) n=$(cat "$MOCK_CLOCK" 2>/dev/null || echo "$MOCK_EPOCH"); n=$((n+1)); echo "$n" > "$MOCK_CLOCK"; echo "$n";; *) echo fixture-time;; esac\n',
            }
            for name, body in stubs.items():
                target = binaries / name
                target.write_text(body)
                target.chmod(0o755)
            # Execute real normalization but inject a subsecond completion clock. This
            # fails if collectors truncate via date +%s rather than retain the helper's ms.
            python_stub = binaries / 'python3'
            python_stub.write_text('#!/bin/sh\nif [ "${3:-}" = "--capture-time" ]; then\n'
                + '"$REAL_PYTHON" "$1" "$2" || exit $?\n'
                + 'n=$(cat "$MOCK_COMPLETION_CLOCK" 2>/dev/null || echo "$MOCK_COMPLETION_BASE"); n=$((n+1)); echo "$n" > "$MOCK_COMPLETION_CLOCK"; echo "$n"\n'
                + 'else exec "$REAL_PYTHON" "$@"; fi\n')
            python_stub.chmod(0o755)
            env = dict(os.environ, PATH=str(binaries) + os.pathsep + os.environ['PATH'],
                       MOCK_CLOCK=str(root / 'clock'), MOCK_EPOCH=str(int(__import__('time').time())), MOCK_STATUS='not-json',
                       REAL_PYTHON=sys.executable, MOCK_COMPLETION_CLOCK=str(root / 'completion-clock'),
                       MOCK_COMPLETION_BASE=str(int(__import__('time').time()) * 1000 + 123))
            malformed = ['not-json', '', '[]']
            for key, value in [('traffic', 'oops'), ('consensus', []), ('storage', 2), ('reward_runtime', False),
                               ('traffic', {'libp2p_replication': 'bad-lane'})]:
                malformed.append(json.dumps(dict(ok=True, node_id='n', world_id='w', **{key: value})))
            for index, payload in enumerate(malformed):
                env['MOCK_STATUS'] = payload
                out = root / ('single-' + str(index))
                completed = subprocess.run(['bash', str(scripts / 'oasis7-node-traffic-monitor.sh'), '--out-dir', str(out),
                                '--network-interface', 'fixture'], env=env, capture_output=True)
                self.assertEqual(completed.returncode, 0, completed.stderr.decode())
                row = json.loads((out / 'history.ndjson').read_text())
                self.assertFalse(row['status_fetch_ok'])
                self.assertEqual(row['fetch_error'], 'invalid_status_response')
            env['MOCK_STATUS'] = json.dumps({'ok': True, 'node_id': 'n', 'world_id': 'w', 'observed_at_unix_ms': 123})
            out = root / 'triad'
            completed = subprocess.run(['bash', str(scripts / 'p2p-real-env-traffic-monitor.sh'), '--out-dir', str(out),
                            '--samples', '1'], env=env, capture_output=True)
            self.assertEqual(completed.returncode, 0, completed.stderr.decode())
            rows = [json.loads(line) for line in (out / 'history.ndjson').read_text().splitlines()]
            self.assertEqual(len(rows), 3)
            self.assertEqual(len({row['captured_at_unix_ms'] for row in rows}), 3)
            self.assertTrue(all(row['captured_at_unix_ms'] % 1000 != 0 for row in rows))
            self.assertTrue(all(row['captured_at_unix_ms'] >= int(env['MOCK_COMPLETION_BASE']) for row in rows))
            self.assertTrue(all(row['status_observed_at_unix_ms'] == 123 for row in rows))

    def test_wrapper_historical_replay_never_collects_or_changes_history(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binaries = root / 'bin'
            binaries.mkdir()
            for name in ['curl', 'ssh']:
                target = binaries / name
                target.write_text('#!/bin/sh\necho unexpected-network >&2\nexit 91\n')
                target.chmod(0o755)
            env = dict(os.environ, PATH=str(binaries) + os.pathsep + os.environ['PATH'])
            for script in ['oasis7-node-traffic-monitor.sh', 'p2p-real-env-traffic-monitor.sh']:
                history = root / (script + '.history')
                rows = [dict(record(m), label='local_node') for m in [0, 10, 20]]
                history.write_text(''.join(json.dumps(row) + '\n' for row in rows))
                original = history.read_bytes()
                out = root / (script + '.out')
                completed = subprocess.run(['bash', str(Path(__file__).with_name(script)),
                    '--summary-only', '--as-of-unix-ms', '600000', '--history-retention-minutes', '1',
                    '--history-path', str(history), '--out-dir', str(out)], env=env, capture_output=True)
                self.assertEqual(completed.returncode, 0, completed.stderr.decode())
                self.assertNotIn(b'unexpected-network', completed.stderr)
                self.assertEqual(history.read_bytes(), original)
                result = json.loads((out / 'latest_summary.json').read_text())
                self.assertEqual(result['as_of_unix_ms'], 600000)
                node = result.get('node') or result['nodes']['local_node']
                self.assertEqual(node['last_success']['captured_at_unix_ms'], 600000)

    def test_invalid_options_fail_before_any_output_write(self):
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory) / 'must-not-exist'
            for script in ['oasis7-node-traffic-monitor.sh', 'p2p-real-env-traffic-monitor.sh']:
                result = subprocess.run(['bash', str(Path(__file__).with_name(script)), '--out-dir', str(out),
                                         '--max-sample-gap-secs', '0'], capture_output=True)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(out.exists())
                result = subprocess.run(['bash', str(Path(__file__).with_name(script)), '--out-dir', str(out),
                    '--summary-only', '--as-of-unix-ms', str(10**20)], capture_output=True)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(out.exists())
            result = subprocess.run([sys.executable, str(Path(__file__).with_name('traffic-monitor-summary.py')),
                '--layout', 'single-node', '--history-path', str(out / 'missing'), '--summary-json', str(out / 'json'),
                '--summary-md', str(out / 'md'), '--window-minutes', '10', '--history-retention-minutes', '60',
                '--top-n', '3', '--as-of-unix-ms', str(10**20)], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(out.exists())


if __name__ == '__main__':
    unittest.main()
