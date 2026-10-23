#!/usr/bin/env python3
"""Offline actual-orchestrator tests; child programs are isolated fixture stubs."""
import json
import importlib.util
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import unittest

SCRIPTS = Path(__file__).resolve().parent


class AttemptWrapperTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        scripts = self.root / 'scripts'
        scripts.mkdir()
        shutil.copy2(SCRIPTS / 'p2p-real-env-observability-monitor.sh', scripts)
        helper = SCRIPTS / 'p2p-real-env-observability-attempt.py'
        if helper.exists():
            shutil.copy2(helper, scripts)
        self.out = self.root / 'out'
        self.env = dict(os.environ)
        self.children = []
        self.addCleanup(self.stop_children)
        source = r'''
import json, os, pathlib, shutil, sys, time
args = sys.argv[1:]
def arg(name): return args[args.index(name) + 1]
if sys.argv[0] == '-c': sys.argv = sys.argv[1:]
name = pathlib.Path(sys.argv[0]).name
out = pathlib.Path(arg('--out-dir')) if '--out-dir' in args else None
fixtures = pathlib.Path(os.environ['ATTEMPT_TEST_FIXTURES']) if os.environ.get('ATTEMPT_TEST_FIXTURES') else None
if 'snapshot' in name:
    if os.environ.get('ATTEMPT_TEST_MARKER'):
        pathlib.Path(os.environ['ATTEMPT_TEST_MARKER']).write_text('started')
    while os.environ.get('ATTEMPT_TEST_BLOCK') and not pathlib.Path(os.environ['ATTEMPT_TEST_BLOCK']).exists():
        time.sleep(.01)
    if os.environ.get('ATTEMPT_TEST_FAIL') == 'snapshot': sys.exit(7)
    base = out / 'fixture'
    for label in ['local_node', 'sequencer_ecs', 'storage_ecs']:
        node = base / 'nodes' / label
        (node / 'samples' / 'sample-001').mkdir(parents=True)
        (node / 'status.json').write_text('{}')
        (node / 'samples' / 'sample-001' / 'status.json').write_text('{}')
    (base / 'summary.json').write_text('{}')
    if fixtures:
        value = json.loads((fixtures / 'snapshot_summary.json').read_text())
        value['claim_status'] = 'blocked'
        (base / 'summary.json').write_text(json.dumps(value))
        for label in ['local_node', 'sequencer_ecs', 'storage_ecs']:
            source = fixtures / (label.replace('_node', '') + '_status.json')
            if label == 'local_node': source = fixtures / 'local_status.json'
            if label == 'sequencer_ecs': source = fixtures / 'sequencer_status.json'
            if label == 'storage_ecs': source = fixtures / 'storage_status.json'
            shutil.copy2(source, base / 'nodes' / label / 'status.json')
elif 'host-monitor' in name:
    (out / 'fixture').mkdir(parents=True)
    (out / 'fixture' / 'summary.json').write_text('{}')
    if fixtures: shutil.copy2(fixtures / 'host_summary.json', out / 'fixture' / 'summary.json')
    if os.environ.get('ATTEMPT_TEST_FAIL') == 'host': sys.exit(7)
elif 'traffic-monitor' in name or 'wasm-metrics' in name:
    out.mkdir(parents=True, exist_ok=True)
    (out / 'latest_summary.json').write_text('{}')
    if fixtures:
        filename = 'traffic_summary.json' if 'traffic-monitor' in name else {'local_node': 'local_wasm_summary.json', 'sequencer_ecs': 'sequencer_wasm_summary.json', 'storage_ecs': 'storage_wasm_summary.json'}[arg('--node-label')]
        shutil.copy2(fixtures / filename, out / 'latest_summary.json')
else:
    pathlib.Path(arg('--summary-json')).write_text(json.dumps({'overall': {'status': 'pass'}, 'run_id': arg('--run-id'), 'run_dir': arg('--run-dir')}))
    pathlib.Path(arg('--summary-md')).write_text('# fixture pass\n')
'''
        for name in ['p2p-real-env-triad-snapshot.sh', 'p2p-real-env-host-monitor.sh',
                     'p2p-real-env-traffic-monitor.sh', 'oasis7-node-wasm-metrics-monitor.sh',
                     'p2p-real-env-observability-summary.py']:
            target = scripts / name
            target.write_text(('#!/usr/bin/env bash\nexec ' + sys.executable + ' -c ' + __import__('shlex').quote(source) + ' \"$0\" \"$@\"\n') if 'wasm-metrics' in name else '#!' + sys.executable + '\n' + source)
            target.chmod(0o755)

    def stop_children(self):
        for child in self.children:
            if child.poll() is None:
                os.killpg(child.pid, signal.SIGKILL)
            child.communicate(timeout=5)

    def start(self, env=None, arguments=None):
        child = subprocess.Popen(['bash', str(self.root / 'scripts' / 'p2p-real-env-observability-monitor.sh'),
                                  '--out-dir', str(self.out), *(arguments if arguments is not None else ['--local-status-url', 'fixture'])],
                                 env=env or self.env, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                 start_new_session=True)
        self.children.append(child)
        return child

    def wait_file(self, path):
        deadline = time.monotonic() + 5
        while not path.exists() and time.monotonic() < deadline:
            time.sleep(.01)
        self.assertTrue(path.exists(), 'child fixture did not reach collector barrier')

    def test_new_attempt_publishes_pending_before_first_collector_and_sigkill(self):
        self.out.mkdir()
        (self.out / 'latest_summary.json').write_text(json.dumps({'overall': {'status': 'pass'}}))
        marker, release = self.root / 'started', self.root / 'release'
        env = dict(self.env, ATTEMPT_TEST_MARKER=str(marker), ATTEMPT_TEST_BLOCK=str(release))
        child = self.start(env)
        self.wait_file(marker)
        report = json.loads((self.out / 'latest_summary.json').read_text())
        self.assertEqual(report.get('attempt', {}).get('result'), 'pending')
        os.killpg(child.pid, signal.SIGKILL)
        child.communicate(timeout=5)
        self.assertEqual(json.loads((self.out / 'latest_summary.json').read_text())['attempt']['result'], 'pending')
        self.assertEqual(self.canonical(1)['latest_attempt']['result'], 'pending')

    def complete(self, child, expected=0):
        stdout, stderr = child.communicate(timeout=10)
        self.assertEqual(child.returncode, expected, stderr.decode())
        return stdout

    def canonical(self, expected=0, markdown=False):
        result = subprocess.run([sys.executable, str(self.root / 'scripts' / 'p2p-real-env-observability-attempt.py'),
            'read', '--out-dir', str(self.out), '--format', 'markdown' if markdown else 'json'], capture_output=True)
        self.assertEqual(result.returncode, expected, result.stderr.decode())
        return result.stdout.decode() if markdown else json.loads(result.stdout)

    def test_success_then_failure_and_recovery_keep_independent_success_reference(self):
        self.complete(self.start())
        first = self.canonical()
        self.assertEqual(first['latest_attempt']['sequence'], 1)
        self.assertEqual(first['last_success']['sequence'], 1)
        self.assertTrue(first['collection_attempt']['execution_succeeded'])
        self.complete(self.start(dict(self.env, ATTEMPT_TEST_FAIL='snapshot')), 1)
        failed = self.canonical(1)
        self.assertEqual(failed['latest_attempt']['sequence'], 2)
        self.assertEqual(failed['latest_attempt']['result'], 'failed')
        self.assertEqual(failed['last_success']['sequence'], 1)
        self.assertFalse(failed['current_measurement_available'])
        self.complete(self.start())
        recovered = self.canonical()
        self.assertEqual(recovered['latest_attempt']['sequence'], 3)
        self.assertEqual(recovered['last_success']['sequence'], 3)
        self.assertIn('Last successful sequence: `3`', self.canonical(markdown=True))
        self.assertEqual(len(list((self.out / 'runs').iterdir())), 3)

    def test_slow_old_success_cannot_overwrite_new_failure(self):
        marker, release = self.root / 'a-started', self.root / 'a-release'
        old = self.start(dict(self.env, ATTEMPT_TEST_MARKER=str(marker), ATTEMPT_TEST_BLOCK=str(release)))
        self.wait_file(marker)
        self.complete(self.start(dict(self.env, ATTEMPT_TEST_FAIL='snapshot')), 1)
        before = (self.out / 'attempt_state.json').read_bytes()
        release.touch()
        self.complete(old, 3)
        self.assertEqual((self.out / 'attempt_state.json').read_bytes(), before)
        self.assertEqual(self.canonical(1)['latest_attempt']['result'], 'failed')

    def test_slow_old_success_cannot_overwrite_new_pending(self):
        first_marker, first_release = self.root / 'a-started', self.root / 'a-release'
        second_marker, second_release = self.root / 'b-started', self.root / 'b-release'
        old = self.start(dict(self.env, ATTEMPT_TEST_MARKER=str(first_marker), ATTEMPT_TEST_BLOCK=str(first_release)))
        self.wait_file(first_marker)
        newer = self.start(dict(self.env, ATTEMPT_TEST_MARKER=str(second_marker), ATTEMPT_TEST_BLOCK=str(second_release)))
        self.wait_file(second_marker)
        first_release.touch()
        self.complete(old, 3)
        current = self.canonical(1)
        self.assertEqual(current['latest_attempt']['sequence'], 2)
        self.assertEqual(current['latest_attempt']['result'], 'pending')
        second_release.touch()
        self.complete(newer)

    def test_missing_or_corrupt_sequence_state_blocks_before_collectors(self):
        self.complete(self.start())
        state = self.out / 'attempt_state.json'
        original = state.read_bytes()
        for mutation in ['missing', 'corrupt']:
            if mutation == 'missing': state.unlink()
            else: state.write_text('{broken')
            marker = self.root / ('blocked-' + mutation)
            self.complete(self.start(dict(self.env, ATTEMPT_TEST_MARKER=str(marker))), 2)
            self.assertFalse(marker.exists())
            self.assertFalse(self.canonical(2)['current_measurement_available'])
            state.write_bytes(original)

    def test_publication_failure_starts_no_collectors_and_reader_fails_closed(self):
        self.out.mkdir()
        (self.out / 'latest_summary.md').mkdir()
        marker = self.root / 'must-not-start'
        self.complete(self.start(dict(self.env, ATTEMPT_TEST_MARKER=str(marker))), 2)
        self.assertFalse(marker.exists())
        self.assertFalse(self.canonical(2)['current_measurement_available'])

    def test_terminal_state_write_failure_cannot_validate_new_cached_success(self):
        helper_path = SCRIPTS / 'p2p-real-env-observability-attempt.py'
        spec = importlib.util.spec_from_file_location('attempt_helper', helper_path)
        helper = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(helper)
        self.out.mkdir()
        reserved = helper.begin(self.out, [])
        report = Path(reserved['run_dir']) / 'report'
        report.mkdir()
        (report / 'latest_summary.json').write_text(json.dumps({'overall': {'status': 'pass'}, 'run_id': str(reserved['sequence']) + '-' + reserved['attempt_id'], 'run_dir': reserved['run_dir']}))
        (report / 'latest_summary.md').write_text('# pass')
        write = helper.atomic_write
        def fail_terminal(path, data):
            if path.name == helper.STATE and json.loads(data)['latest_attempt']['result'] == 'success':
                raise OSError('injected terminal state write failure')
            return write(path, data)
        helper.atomic_write = fail_terminal
        with self.assertRaises(OSError):
            helper.finish(self.out, reserved['sequence'], reserved['attempt_id'], [])
        state = json.loads((self.out / helper.STATE).read_text())
        self.assertEqual(state['latest_attempt']['result'], 'pending')
        self.assertIsNone(state['last_success'])
        self.assertFalse(self.canonical(2)['current_measurement_available'])

    def test_same_clock_runs_have_unique_directories_and_configuration_digests(self):
        self.complete(self.start())
        first = self.canonical()
        self.complete(self.start(arguments=['--local-status-url', 'http://offline.invalid/status?token=never-store-this-secret']))
        second = self.canonical()
        self.assertNotEqual(first['latest_attempt']['run_directory'], second['latest_attempt']['run_directory'])
        self.assertNotEqual(first['latest_attempt']['scope_fingerprint'], second['latest_attempt']['scope_fingerprint'])
        self.assertNotIn('never-store-this-secret', (self.out / 'attempt_state.json').read_text())

    def test_real_merger_blocked_profile_is_preserved_after_successful_execution(self):
        shutil.copy2(SCRIPTS / 'p2p-real-env-observability-summary.py', self.root / 'scripts')
        env = dict(self.env, ATTEMPT_TEST_FIXTURES=str(SCRIPTS.parent / 'fixtures' / 'p2p_real_env_observability'))
        self.complete(self.start(env, arguments=[]))
        report = self.canonical()
        self.assertEqual(report['latest_attempt']['result'], 'success')
        self.assertTrue(report['collection_attempt']['execution_succeeded'])
        self.assertEqual(report['overall']['status'], 'blocked')
        self.assertFalse(report['traffic']['current_measurement_available'])
        self.assertNotIn('current_measurement_available', report)
        original = json.loads((Path(report['run_dir']) / 'report/latest_summary.json').read_text())
        self.assertEqual(report['as_of_unix_ms'], original['as_of_unix_ms'])
        self.assertEqual(report['canonical_state']['validation_scope'], 'attempt_order_and_artifact_integrity')
        self.assertGreaterEqual(report['canonical_state']['read_at_unix_ms'], report['as_of_unix_ms'])
        self.assertIn('blocked', self.canonical(markdown=True))

    def test_real_merger_host_failure_preserves_report_diagnostics(self):
        self.complete(self.start())
        shutil.copy2(SCRIPTS / 'p2p-real-env-observability-summary.py', self.root / 'scripts')
        env = dict(self.env, ATTEMPT_TEST_FIXTURES=str(SCRIPTS.parent / 'fixtures' / 'p2p_real_env_observability'), ATTEMPT_TEST_FAIL='host')
        self.complete(self.start(env, arguments=[]), 1)
        report = self.canonical(1)
        self.assertIn('nodes', report)
        self.assertEqual(report['snapshot']['claim_status'], 'blocked')
        self.assertEqual(report['last_success']['sequence'], 1)
        self.assertEqual(report['overall']['status'], 'blocked')
        self.assertIn('host_failed', report['overall']['alerts'])
        self.assertFalse(report['current_measurement_available'])
        self.assertIn('## local_node', self.canonical(1, markdown=True))

    def test_failed_collectors_discard_invalid_or_failed_summary_source(self):
        for mutation in ('wrong_run', 'malformed', 'bad_alerts', 'summary_failed'):
            with self.subTest(mutation=mutation):
                helper = self.helper()
                reserved = helper.begin(self.out, [])
                directory = Path(reserved['run_dir']) / 'report'
                directory.mkdir()
                source = {'overall': {'status':'pass'}, 'run_id': str(reserved['sequence']) + '-' + reserved['attempt_id'], 'run_dir': reserved['run_dir'], 'nodes': {'must_not_adopt': {}}}
                if mutation == 'wrong_run': source['run_id'] = 'wrong'
                if mutation == 'bad_alerts': source['overall']['alerts'] = [{}]
                (directory / 'latest_summary.json').write_text('{' if mutation == 'malformed' else json.dumps(source))
                (directory / 'latest_summary.md').write_text('# must_not_adopt')
                errors = [{'stage':'summary' if mutation == 'summary_failed' else 'host', 'exit_code':7, 'error':'collector_failed'}]
                self.assertEqual(helper.finish(self.out, reserved['sequence'], reserved['attempt_id'], errors), 1)
                report = self.canonical(1)
                self.assertNotIn('nodes', report)
                self.assertIsNone(report['last_success'])
                self.assertNotIn('must_not_adopt', self.canonical(1, markdown=True))

    def helper(self):
        spec = importlib.util.spec_from_file_location('attempt_helper_extra', SCRIPTS / 'p2p-real-env-observability-attempt.py')
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module

    def test_collector_profile_separation_and_legacy_default(self):
        helper = self.helper()
        reserved = helper.begin(self.out, [])
        before = {p.name: p.read_bytes() for p in self.out.iterdir() if p.is_file()}
        for operation in (lambda: helper.begin(self.out, [], collector='host'),
                          lambda: helper.finish(self.out, reserved['sequence'], reserved['attempt_id'], [], collector='host'),
                          lambda: helper.read(self.out, collector='host')):
            with self.assertRaises(helper.AttemptError): operation()
            self.assertEqual(before, {p.name: p.read_bytes() for p in self.out.iterdir() if p.is_file()})
        for name in (helper.ANCHOR, helper.STATE):
            path = self.out / name
            value = json.loads(path.read_text())
            value.pop('collector', None)
            if name == helper.STATE: value['latest_attempt'].pop('collector', None)
            path.write_text(json.dumps(value))
        self.assertEqual(helper.begin(self.out, [])['sequence'], 2)
        with self.assertRaises(helper.AttemptError): helper.begin(self.out, [], collector='host')

    def test_host_store_profile_corruption_fails_before_mutation(self):
        helper = self.helper()
        reserved = helper.begin(self.out, [], collector='host')
        for target in ('anchor', 'state', 'attempt'):
            with self.subTest(target=target):
                path = self.out / (helper.ANCHOR if target == 'anchor' else helper.STATE)
                original = path.read_bytes()
                value = json.loads(original)
                (value['latest_attempt'] if target == 'attempt' else value).pop('collector')
                path.write_text(json.dumps(value))
                before = {p.name: p.read_bytes() for p in self.out.iterdir() if p.is_file()}
                for operation in (lambda: helper.begin(self.out, [], collector='host'),
                                  lambda: helper.finish(self.out, reserved['sequence'], reserved['attempt_id'], [], collector='host'),
                                  lambda: helper.read(self.out, collector='host')):
                    with self.assertRaises(helper.AttemptError): operation()
                    self.assertEqual(before, {p.name: p.read_bytes() for p in self.out.iterdir() if p.is_file()})
                path.write_bytes(original)
        with self.assertRaises(helper.AttemptError): helper.begin(self.out, [])

    def test_host_own_source_identity_schema_and_collection_history(self):
        helper = self.helper()
        for mutation in ('valid', 'wrong_run', 'wrong_schema', 'bad_complete', 'missing', 'incomplete', 'host_failed'):
            with self.subTest(mutation=mutation):
                root = self.root / mutation
                reserved = helper.begin(root, [], collector='host')
                pending, _, code = helper.read(root, collector='host')
                self.assertEqual(code, 1)
                self.assertFalse(pending['aggregate']['complete'])
                report = Path(reserved['run_dir']) / 'report'
                report.mkdir()
                nodes = {label: {'available': True, 'sample_count': 1, 'success_sample_count': 1,
                    'failed_sample_count': 0, 'latest_attempt': {'result': 'success', 'sample_fetch_ok': True},
                    'last_success': {'sample_index': 1}} for label in ('local_node', 'sequencer_ecs', 'storage_ecs')}
                source = {'schema_version': 2, 'run_id': str(reserved['sequence']) + '-' + reserved['attempt_id'],
                    'run_dir': reserved['run_dir'], 'nodes': nodes, 'aggregate': {'complete': True,
                    'expected_sample_count': 3, 'attempted_count': 3, 'failed_sample_count': 0,
                    'failed_nodes': [], 'node_count': 3}}
                if mutation == 'wrong_run': source['run_id'] = 'another'
                if mutation == 'wrong_schema': source['schema_version'] = 1
                if mutation == 'bad_complete': source['nodes']['local_node']['failed_sample_count'] = 1
                if mutation in ('incomplete', 'host_failed'):
                    source['nodes']['local_node'].update(available=False, success_sample_count=0, failed_sample_count=1,
                        latest_attempt={'result': 'failed', 'sample_fetch_ok': False}, last_success=None)
                    source['aggregate'].update(complete=False, failed_sample_count=1, failed_nodes=['local_node'], node_count=2)
                if mutation != 'missing':
                    (report / 'latest_summary.json').write_text(json.dumps(source))
                    (report / 'latest_summary.md').write_text('# host source')
                expected = 0 if mutation == 'valid' else 1
                self.assertEqual(helper.finish(root, reserved['sequence'], reserved['attempt_id'],
                    [{'stage': 'host', 'exit_code': 1, 'error': 'collector_failed'}] if mutation == 'host_failed' else [], collector='host'), expected)
                value, _, code = helper.read(root, collector='host')
                self.assertEqual(code, expected)
                if expected:
                    self.assertFalse(value['aggregate']['complete'])
                    if mutation == 'host_failed':
                        self.assertEqual(value['nodes'], nodes)
                        self.assertNotIn('summary_failed', value['overall']['alerts'])
                    else:
                        self.assertIn('summary_failed', value['overall']['alerts'])
                else:
                    self.assertEqual(value['nodes'], nodes)
                    self.assertEqual(value['last_success']['sequence'], 1)
                    self.assertEqual(value['overall']['status'], 'pass')

    def test_snapshot_source_inventory_and_execution_profile(self):
        helper = self.helper()
        for mutation in ('valid', 'diagnostic_blocked', 'failed', 'incomplete', 'wrong_inventory', 'bad_count', 'bad_sample', 'wrong_schema', 'duplicate', 'bad_label', 'missing_fetch', 'bool_count', 'config_mismatch', 'config_run', 'config_world'):
            with self.subTest(mutation=mutation):
                root = self.root / mutation
                reserved = helper.begin(root, [], collector='snapshot')
                pending, _, code = helper.read(root, collector='snapshot')
                self.assertEqual(code, 1)
                self.assertEqual(pending['claim_status'], 'blocked')
                run = Path(reserved['run_dir'])
                config = {'samples': 1, 'nodes': {'one': {'label': 'one'}}, 'run_id': str(reserved['sequence']) + '-' + reserved['attempt_id'], 'run_dir': reserved['run_dir'], 'world_id': 'world'}
                (run / 'config.json').write_text(json.dumps(config))
                report = run / 'report'
                report.mkdir()
                source = {'schema_version': 2, 'run_id': str(reserved['sequence']) + '-' + reserved['attempt_id'],
                    'run_dir': reserved['run_dir'], 'world_id': 'world', 'claim_status': 'pass_candidate', 'failure_signatures': [],
                    'collection': {'node_labels': ['one'], 'samples_per_node': 1, 'expected_sample_count': 1,
                        'attempted_count': 1, 'failed_sample_count': 0, 'complete': True},
                    'nodes': {'one': {'sample_count': 1, 'success_sample_count': 1, 'failed_sample_count': 0}},
                    'totals': {'sample_record_count': 1, 'node_count': 1},
                    'samples': [{'label': 'one', 'sample_index': 1, 'healthz_fetch_ok': True, 'status_fetch_ok': True}]}
                if mutation == 'diagnostic_blocked': source.update(claim_status='blocked', failure_signatures=['node_not_ready'])
                if mutation in ('failed', 'incomplete'):
                    source['collection'].update(complete=False, failed_sample_count=1)
                    source['nodes']['one'].update(success_sample_count=0, failed_sample_count=1)
                    source['samples'][0]['status_fetch_ok'] = False
                if mutation == 'wrong_inventory': source['collection']['node_labels'] = ['other']
                if mutation == 'bad_count': source['nodes']['one']['sample_count'] = 2
                if mutation == 'bad_sample': source['samples'][0]['sample_index'] = 2
                if mutation == 'wrong_schema': source['schema_version'] = 1
                if mutation == 'duplicate': source['samples'].append(dict(source['samples'][0]))
                if mutation == 'bad_label': source['samples'][0]['label'] = []
                if mutation == 'missing_fetch': source['samples'][0].pop('healthz_fetch_ok')
                if mutation == 'bool_count': source['collection']['expected_sample_count'] = True
                if mutation == 'config_mismatch': config['samples'] = 2
                if mutation == 'config_run': config['run_id'] = 'another'
                if mutation == 'config_world': config['world_id'] = 'another'
                (run / 'config.json').write_text(json.dumps(config))
                (report / 'latest_summary.json').write_text(json.dumps(source))
                (report / 'latest_summary.md').write_text('# snapshot source')
                errors = [{'stage': 'snapshot', 'exit_code': 1, 'error': 'collector_failed'}] if mutation == 'failed' else []
                expected = 0 if mutation in ('valid', 'diagnostic_blocked') else 1
                self.assertEqual(helper.finish(root, reserved['sequence'], reserved['attempt_id'], errors, collector='snapshot'), expected)
                value, _, code = helper.read(root, collector='snapshot')
                self.assertEqual(code, expected)
                if not expected:
                    self.assertEqual(value['claim_status'], source['claim_status'])
                    self.assertEqual(value['last_success']['sequence'], 1)
                else:
                    self.assertEqual(value['claim_status'], 'blocked')
                    self.assertFalse(value['current_measurement_available'])
                    self.assertIsNone(value['last_success'])
                    if mutation == 'failed': self.assertEqual(value['nodes'], source['nodes'])
                for other in ('host', 'observability'):
                    with self.assertRaises(helper.AttemptError): helper.read(root, collector=other)

    def test_snapshot_profile_binding_scope_and_unknown_reader(self):
        helper = self.helper()
        reserved = helper.begin(self.out, [], collector='snapshot')
        for target in ('anchor', 'state', 'attempt'):
            path = self.out / (helper.ANCHOR if target == 'anchor' else helper.STATE)
            original = path.read_bytes()
            value = json.loads(original)
            (value['latest_attempt'] if target == 'attempt' else value).pop('collector')
            path.write_text(json.dumps(value))
            before = {p.name: p.read_bytes() for p in self.out.iterdir() if p.is_file()}
            for operation in (lambda: helper.begin(self.out, [], collector='snapshot'),
                              lambda: helper.finish(self.out, reserved['sequence'], reserved['attempt_id'], [], collector='snapshot'),
                              lambda: helper.read(self.out, collector='snapshot')):
                with self.assertRaises(helper.AttemptError): operation()
                self.assertEqual(before, {p.name: p.read_bytes() for p in self.out.iterdir() if p.is_file()})
            path.write_bytes(original)
        # Snapshot code is bound separately from host/default code, and file inputs bind content.
        config = self.root / 'scope-config'
        config.write_text('one')
        first = helper.scope_fingerprint([str(config)], 'snapshot')
        config.write_text('two')
        self.assertNotEqual(first, helper.scope_fingerprint([str(config)], 'snapshot'))
        self.assertNotEqual(helper.scope_fingerprint([], 'host'), helper.scope_fingerprint([], 'snapshot'))
        result = subprocess.run([sys.executable, str(SCRIPTS / 'p2p-real-env-observability-attempt.py'),
            'read', '--collector', 'snapshot', '--out-dir', str(self.root / 'missing')], capture_output=True, text=True)
        self.assertEqual(result.returncode, 2)
        unknown = json.loads(result.stdout)
        self.assertEqual(unknown['claim_status'], 'blocked')
        self.assertFalse(unknown['current_measurement_available'])
        self.assertFalse(unknown['collection']['complete'])

    def test_anchor_loss_and_versioned_orphans_never_reset_sequence(self):
        self.complete(self.start())
        anchor = self.out / '.observability-initialized'
        original = anchor.read_bytes()
        anchor.unlink()
        marker = self.root / 'anchor-missing'
        self.complete(self.start(dict(self.env, ATTEMPT_TEST_MARKER=str(marker))), 2)
        self.assertFalse(marker.exists())
        (self.out / 'attempt_state.json').unlink()
        self.complete(self.start(dict(self.env, ATTEMPT_TEST_MARKER=str(marker))), 2)
        self.assertFalse(marker.exists())
        self.assertFalse(anchor.exists())

    def test_anchor_only_initialization_failure_blocks_reinitialization(self):
        helper = self.helper()
        write = helper.atomic_write
        def fail_initial_state(path, data):
            if path.name == helper.STATE: raise OSError('injected initial sequence persistence failure')
            return write(path, data)
        helper.atomic_write = fail_initial_state
        with self.assertRaises(OSError): helper.begin(self.out, [])
        self.assertTrue((self.out / helper.ANCHOR).exists())
        self.assertFalse((self.out / helper.STATE).exists())
        marker = self.root / 'must-not-reinitialize'
        self.complete(self.start(dict(self.env, ATTEMPT_TEST_MARKER=str(marker))), 2)
        self.assertFalse(marker.exists())

    def test_fixed_clock_reservations_and_lock_inode_guard(self):
        helper = self.helper()
        from unittest.mock import patch
        with patch.object(helper.time, 'time_ns', return_value=123_000_000):
            first = helper.begin(self.out, [])
            second = helper.begin(self.out, [])
        self.assertEqual(first['started_at_unix_ms'], second['started_at_unix_ms'])
        self.assertNotEqual(first['run_dir'], second['run_dir'])
        self.assertEqual(second['sequence'], first['sequence'] + 1)
        with self.assertRaises(helper.AttemptError):
            with helper.locked(self.out) as check:
                (self.out / helper.LOCK).rename(self.out / 'old-lock')
                (self.out / helper.LOCK).touch(mode=0o600)
                check()

    def test_sequence_rollback_in_state_or_state_and_anchor_blocks_collection(self):
        self.complete(self.start())
        state_path = self.out / 'attempt_state.json'
        anchor_path = self.out / '.observability-initialized'
        first_state, first_anchor = state_path.read_bytes(), anchor_path.read_bytes()
        self.complete(self.start())
        second_state, second_anchor = state_path.read_bytes(), anchor_path.read_bytes()
        for rollback_anchor in [False, True]:
            state_path.write_bytes(first_state)
            anchor_path.write_bytes(first_anchor if rollback_anchor else second_anchor)
            marker = self.root / ('rollback-' + str(rollback_anchor))
            self.complete(self.start(dict(self.env, ATTEMPT_TEST_MARKER=str(marker))), 2)
            self.assertFalse(marker.exists())
            self.assertFalse(self.canonical(2)['current_measurement_available'])
            self.assertEqual(state_path.read_bytes(), first_state)
        state_path.write_bytes(second_state)
        anchor_path.write_bytes(second_anchor)

    def test_initialized_lock_loss_or_replacement_blocks_collection(self):
        self.complete(self.start())
        lock = self.out / '.observability-attempt.lock'
        lock.unlink()
        marker = self.root / 'lock-lost'
        self.complete(self.start(dict(self.env, ATTEMPT_TEST_MARKER=str(marker))), 2)
        self.assertFalse(marker.exists())
        self.assertFalse(lock.exists(), 'initialized lock must not be recreated')
        lock.touch(mode=0o600)
        self.complete(self.start(dict(self.env, ATTEMPT_TEST_MARKER=str(marker))), 2)
        self.assertFalse(marker.exists())
        self.assertFalse(self.canonical(2)['current_measurement_available'])

    def test_replacement_lock_with_reused_device_and_inode_still_blocks(self):
        self.complete(self.start())
        lock = self.out / '.observability-attempt.lock'
        anchor = self.out / '.observability-initialized'
        value = json.loads(anchor.read_text())
        lock.unlink()
        lock.touch(mode=0o600)
        # Model immediate inode reuse deterministically: only the recorded device
        # and inode are made indistinguishable, preserving the old lock token.
        metadata = lock.stat()
        value['lock_identity']['device'] = metadata.st_dev
        value['lock_identity']['inode'] = metadata.st_ino
        anchor.write_text(json.dumps(value))
        marker = self.root / 'reused-inode-must-not-collect'
        self.complete(self.start(dict(self.env, ATTEMPT_TEST_MARKER=str(marker))), 2)
        self.assertFalse(marker.exists())
        self.assertEqual(self.canonical(2)['attempt']['result'], 'unknown')

    def test_bound_lock_content_cannot_change_in_place(self):
        self.complete(self.start())
        lock = self.out / '.observability-attempt.lock'
        original = lock.read_bytes()
        identity = (lock.stat().st_dev, lock.stat().st_ino)
        for payload in [b'', b'partial-token', (str(__import__('uuid').uuid4()) + '\n').encode()]:
            lock.write_bytes(payload)
            self.assertEqual((lock.stat().st_dev, lock.stat().st_ino), identity)
            marker = self.root / 'changed-token-must-not-collect'
            self.complete(self.start(dict(self.env, ATTEMPT_TEST_MARKER=str(marker))), 2)
            self.assertFalse(marker.exists())
            self.assertEqual(self.canonical(2)['attempt']['result'], 'unknown')
            self.assertEqual(lock.read_bytes(), payload, 'bound lock must not be silently reinitialized')
        lock.write_bytes(original)
        self.assertEqual(self.canonical()['attempt']['result'], 'success')

    def test_copied_token_on_different_lock_inode_is_still_rejected(self):
        self.complete(self.start())
        lock = self.out / '.observability-attempt.lock'
        token = lock.read_bytes()
        original_inode = lock.stat().st_ino
        lock.rename(self.out / 'retained-original-lock')
        lock.touch(mode=0o600)
        lock.write_bytes(token)
        self.assertNotEqual(lock.stat().st_ino, original_inode)
        marker = self.root / 'copied-token-must-not-collect'
        self.complete(self.start(dict(self.env, ATTEMPT_TEST_MARKER=str(marker))), 2)
        self.assertFalse(marker.exists())
        self.assertEqual(self.canonical(2)['attempt']['result'], 'unknown')

    def test_pre_anchor_lock_initialization_preserves_valid_token_and_refuses_partial(self):
        helper = self.helper()
        root = self.root / 'pre-anchor'
        root.mkdir()
        lock = root / helper.LOCK
        lock.touch(mode=0o600)
        reserved = helper.begin(root, [])
        token = lock.read_bytes()
        self.assertEqual(len(token), 37)
        anchor = json.loads((root / helper.ANCHOR).read_text())
        self.assertEqual(anchor['lock_identity']['token'], token.decode().strip())
        self.assertEqual(reserved['sequence'], 1)
        # Full pre-anchor token after a crash can be re-synced without changing it.
        orphan = self.root / 'full-token-before-anchor'
        orphan.mkdir()
        (orphan / helper.LOCK).touch(mode=0o600)
        (orphan / helper.LOCK).write_bytes(token)
        helper.begin(orphan, [])
        self.assertEqual((orphan / helper.LOCK).read_bytes(), token)
        broken = self.root / 'partial-token-before-anchor'
        broken.mkdir()
        (broken / helper.LOCK).touch(mode=0o600)
        (broken / helper.LOCK).write_bytes(b'partial')
        with self.assertRaises(helper.AttemptError):
            helper.begin(broken, [])
        self.assertFalse((broken / helper.ANCHOR).exists())
        self.assertFalse((broken / helper.STATE).exists())
        self.assertEqual((broken / helper.LOCK).read_bytes(), b'partial')

    def test_anchor_without_token_cannot_migrate_or_collect(self):
        self.complete(self.start())
        anchor = self.out / '.observability-initialized'
        value = json.loads(anchor.read_text())
        value['lock_identity'].pop('token')
        anchor.write_text(json.dumps(value))
        marker = self.root / 'legacy-anchor-must-not-collect'
        self.complete(self.start(dict(self.env, ATTEMPT_TEST_MARKER=str(marker))), 2)
        self.assertFalse(marker.exists())
        self.assertEqual(self.canonical(2)['attempt']['result'], 'unknown')

    def test_concurrent_initializers_keep_one_token_and_distinct_reservations(self):
        marker = self.root / 'concurrent-started'
        release = self.root / 'concurrent-release'
        env = dict(self.env, ATTEMPT_TEST_MARKER=str(marker), ATTEMPT_TEST_BLOCK=str(release))
        first, second = self.start(env), self.start(env)
        self.wait_file(marker)
        deadline = time.monotonic() + 5
        state_path = self.out / 'attempt_state.json'
        while time.monotonic() < deadline:
            if state_path.exists() and json.loads(state_path.read_text())['last_sequence'] == 2:
                break
            time.sleep(.01)
        self.assertEqual(json.loads(state_path.read_text())['last_sequence'], 2)
        token = (self.out / '.observability-attempt.lock').read_bytes()
        self.assertEqual(json.loads((self.out / '.observability-initialized').read_text())['lock_identity']['token'], token.decode().strip())
        release.touch()
        for child in (first, second):
            child.communicate(timeout=10)
        self.assertEqual(sorted([first.returncode, second.returncode]), [0, 3])
        self.assertEqual((self.out / '.observability-attempt.lock').read_bytes(), token)
        self.assertEqual(self.canonical()['attempt']['sequence'], 2)

    def test_canonical_read_of_uninitialized_root_never_creates_or_initializes_lock(self):
        self.out.mkdir()
        sentinel = self.out / 'sentinel'
        sentinel.write_bytes(b'unchanged')
        before = {path.name: (path.read_bytes(), path.stat().st_mtime_ns) for path in self.out.iterdir()}
        root_mtime = self.out.stat().st_mtime_ns
        self.assertEqual(self.canonical(2)['attempt']['result'], 'unknown')
        self.assertFalse((self.out / '.observability-attempt.lock').exists())
        self.assertEqual(self.out.stat().st_mtime_ns, root_mtime)
        self.assertEqual({path.name: (path.read_bytes(), path.stat().st_mtime_ns) for path in self.out.iterdir()}, before)
        lock = self.out / '.observability-attempt.lock'
        lock.touch(mode=0o600)
        before = {path.name: (path.read_bytes(), path.stat().st_mtime_ns) for path in self.out.iterdir()}
        root_mtime = self.out.stat().st_mtime_ns
        self.assertEqual(self.canonical(2)['attempt']['result'], 'unknown')
        self.assertEqual(lock.read_bytes(), b'')
        self.assertEqual(self.out.stat().st_mtime_ns, root_mtime)
        self.assertEqual({path.name: (path.read_bytes(), path.stat().st_mtime_ns) for path in self.out.iterdir()}, before)

    def test_old_other_run_summary_is_not_successful_completion(self):
        marker, release = self.root / 'wrong-run-started', self.root / 'wrong-run-release'
        child = self.start(dict(self.env, ATTEMPT_TEST_MARKER=str(marker), ATTEMPT_TEST_BLOCK=str(release)))
        self.wait_file(marker)
        # Make the merger produce valid JSON from a different run despite exit zero.
        merger = self.root / 'scripts' / 'p2p-real-env-observability-summary.py'
        source = merger.read_text().replace("'run_id': arg('--run-id')", "'run_id': 'older-unrelated-run'")
        merger.write_text(source)
        release.touch()
        self.complete(child, 1)
        report = self.canonical(1)
        self.assertEqual(report['latest_attempt']['error'], 'summary_failed')
        self.assertIsNone(report['last_success'])

    def test_invalid_anchor_json_shapes_have_bounded_unknown_reader_output(self):
        self.complete(self.start())
        anchor = self.out / '.observability-initialized'
        original = anchor.read_bytes()
        for value in [[], None, 1, True, 'bad-anchor']:
            anchor.write_text(json.dumps(value))
            report = self.canonical(2)
            self.assertEqual(report['attempt']['result'], 'unknown')
            marker = self.root / 'invalid-anchor-collector'
            self.complete(self.start(dict(self.env, ATTEMPT_TEST_MARKER=str(marker))), 2)
            self.assertFalse(marker.exists())
        anchor.write_bytes(original)

    def test_output_alias_resolves_to_one_physical_attempt_store(self):
        alias = self.root / 'output-alias'
        self.out.mkdir()
        alias.symlink_to(self.out, target_is_directory=True)
        self.out = alias
        self.complete(self.start())
        first = self.canonical()
        self.assertEqual(first['attempt']['sequence'], 1)
        self.assertEqual(Path(first['attempt']['run_directory']).parts[0], 'runs')
        self.assertEqual(first['run_dir'], str(alias.resolve() / first['attempt']['run_directory']))
        self.out = alias.resolve()
        self.complete(self.start())
        self.assertEqual(self.canonical()['attempt']['sequence'], 2)

    def test_corrupt_last_success_identity_is_not_validated_history(self):
        self.complete(self.start())
        state = self.out / 'attempt_state.json'
        value = json.loads(state.read_text())
        value['last_success']['attempt_id'] = []
        state.write_text(json.dumps(value))
        self.assertEqual(self.canonical(2)['attempt']['result'], 'unknown')
        marker = self.root / 'bad-success-must-not-collect'
        self.complete(self.start(dict(self.env, ATTEMPT_TEST_MARKER=str(marker))), 2)
        self.assertFalse(marker.exists())

    def test_internal_run_symlink_never_creates_outside_artifacts(self):
        self.complete(self.start())
        runs = self.out / 'runs'
        runs.rename(self.out / 'saved-runs')
        outside = self.root / 'outside'
        outside.mkdir()
        sentinel = outside / 'sentinel'
        sentinel.write_text('unchanged')
        runs.symlink_to(outside, target_is_directory=True)
        anchor = (self.out / '.observability-initialized').read_bytes()
        marker = self.root / 'symlink-must-not-collect'
        self.complete(self.start(dict(self.env, ATTEMPT_TEST_MARKER=str(marker))), 2)
        self.assertFalse(marker.exists())
        self.assertEqual(sorted(path.name for path in outside.iterdir()), ['sentinel'])
        self.assertEqual(sentinel.read_text(), 'unchanged')
        self.assertEqual((self.out / '.observability-initialized').read_bytes(), anchor)

    def test_output_parent_fsync_failure_prevents_collector_authorization(self):
        helper = self.helper()
        nested = self.root / 'new-parent' / 'new-store'
        synchronize = helper.sync_directory
        calls = []
        def fail_parent(path):
            calls.append(path)
            if path == self.root:
                raise OSError('injected output parent fsync failure')
            return synchronize(path)
        helper.sync_directory = fail_parent
        with self.assertRaises(OSError):
            helper.begin(nested, [])
        self.assertIn(self.root / 'new-parent', calls)
        self.assertIn(self.root, calls)
        self.assertFalse((nested / helper.STATE).exists())
        self.assertFalse((nested / helper.ANCHOR).exists())
        # Exercise the real shell entry, injecting only the component's fsync
        # boundary in this isolated copied helper, never a production env hook.
        copied = self.root / 'scripts' / 'p2p-real-env-observability-attempt.py'
        copied.write_text(copied.read_text().replace(
            '    os.fsync(fd)', '    raise OSError("injected output-parent fsync failure")', 1))
        marker = self.root / 'parent-fsync-must-not-collect'
        self.complete(self.start(dict(self.env, ATTEMPT_TEST_MARKER=str(marker))), 2)
        self.assertFalse(marker.exists())
        self.assertFalse((self.out / 'attempt_state.json').exists())

    def test_reader_rejects_cached_generation_mismatch(self):
        self.complete(self.start())
        (self.out / 'latest_summary.json').write_text('{"overall":{"status":"pass"}}')
        self.assertFalse(self.canonical(2)['current_measurement_available'])



if __name__ == '__main__':
    unittest.main()
