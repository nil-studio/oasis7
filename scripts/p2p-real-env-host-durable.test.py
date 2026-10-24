#!/usr/bin/env python3
"""Actual standalone host collector lifecycle, using offline probe/SSH fixtures."""
import importlib.util
import json
import os
from pathlib import Path
import signal
import shutil
import subprocess
import time
import unittest

spec = importlib.util.spec_from_file_location("host_failures", Path(__file__).with_name("p2p-real-env-host-failure.test.py"))
fixtures = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixtures)


def complete(command, env, expected=0):
    result = subprocess.run(command, env=env, capture_output=True, text=True)
    assert result.returncode == expected, (result.returncode, result.stdout, result.stderr)
    return result


def canonical(root, expected=0):
    command = ['python3', str(root/'scripts/p2p-real-env-observability-attempt.py'), 'read', '--collector', 'host', '--out-dir', str(root/'out')]
    return json.loads(complete(command, os.environ, expected).stdout)


def pause_probe(root):
    probe = root/'scripts/p2p-real-env-node-host-sample.sh'
    original = probe.read_text()
    probe.write_text(original.replace('import json,os,pathlib,sys,shlex', 'import json,os,pathlib,sys,shlex,time\nif os.environ.get("HOST_GATE"):\n gate=pathlib.Path(os.environ["HOST_GATE"]); gate.with_suffix(".entered").touch()\n while not gate.exists(): time.sleep(.01)'))


def start_paused(root, env, command, name="release"):
    gate = root/name
    process = subprocess.Popen(command, env=dict(env, HOST_GATE=str(gate)), stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, start_new_session=True)
    deadline = time.monotonic() + 5
    while not gate.with_suffix('.entered').exists() and time.monotonic() < deadline:
        if process.poll() is not None: raise AssertionError(process.communicate())
        time.sleep(.01)
    assert gate.with_suffix('.entered').exists()
    return process, gate


class HostDurableTests(unittest.TestCase):
    def test_pending_is_durable_before_first_probe_and_survives_kill(self):
        with fixtures.host_fixture() as (root, env, command):
            probe = root / "scripts/p2p-real-env-node-host-sample.sh"
            probe.write_text("#!/usr/bin/env python3\nimport os,pathlib,time\nr=pathlib.Path(os.environ['STUB_ROOT']); (r/'probe-entered').touch(); time.sleep(30)\n")
            process = subprocess.Popen(command, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
            try:
                deadline = time.monotonic() + 5
                while not (root/'probe-entered').exists() and time.monotonic() < deadline:
                    time.sleep(.01)
                self.assertTrue((root/'probe-entered').exists())
                state = json.loads((root/'out/attempt_state.json').read_text())
                self.assertEqual(state['latest_attempt']['result'], 'pending')
                self.assertEqual(state['last_sequence'], 1)
            finally:
                os.killpg(process.pid, signal.SIGKILL)
                process.communicate()
            state = json.loads((root/'out/attempt_state.json').read_text())
            self.assertEqual(state['latest_attempt']['result'], 'pending')

    def test_same_clock_runs_are_unique_and_recovery_retains_success(self):
        with fixtures.host_fixture() as (root, env, command):
            date = root/'bin/date'
            date.write_text('#!/bin/sh\ncase "$1" in +%s) echo 1234567890;; *) echo 2020-01-01T00:00:00+00:00;; esac\n')
            date.chmod(0o755)
            complete(command, env)
            first = canonical(root)
            complete(command, dict(env, FAILURES=json.dumps({'local_node:3':'fetch'})), 1)
            failed = canonical(root, 1)
            self.assertEqual(failed['attempt']['sequence'], 2)
            self.assertEqual(failed['last_success']['sequence'], 1)
            self.assertFalse(failed['aggregate']['complete'])
            complete(command, env)
            recovered = canonical(root)
            self.assertEqual(recovered['attempt']['sequence'], 3)
            self.assertEqual(recovered['last_success']['sequence'], 3)
            self.assertEqual(len(list((root/'out/runs').iterdir())), 3)
            self.assertNotEqual(first['run_dir'], recovered['run_dir'])

    def test_older_completion_cannot_replace_newer_failed_or_pending(self):
        for newer_pending in (False, True):
            with self.subTest(newer_pending=newer_pending), fixtures.host_fixture() as (root, env, command):
                pause_probe(root)
                older, gate = start_paused(root, env, command)
                newer = None
                try:
                    if newer_pending:
                        newer, _ = start_paused(root, env, command, "release-newer")
                        expected = 'pending'
                    else:
                        complete(command, dict(env, FAILURES=json.dumps({'local_node:1':'fetch'})), 1)
                        expected = 'failed'
                    before = (root/'out/attempt_state.json').read_bytes()
                    gate.touch()
                    stdout, stderr = older.communicate(timeout=10)
                    self.assertEqual(older.returncode, 3, (stdout, stderr))
                    self.assertEqual((root/'out/attempt_state.json').read_bytes(), before)
                    report = canonical(root, 1)
                    self.assertEqual(report['attempt']['sequence'], 2)
                    self.assertEqual(report['attempt']['result'], expected)
                finally:
                    for process in (older, newer):
                        if process and process.poll() is None:
                            os.killpg(process.pid, signal.SIGKILL)
                            process.communicate()

    def test_invalid_cli_never_writes_or_probes(self):
        with fixtures.host_fixture() as (root, env, command):
            invalid = [['--samples', '0'], ['--samples', '2147483648'], ['--samples', '١'], ['--samples', '-1'], ['--samples', '1.0'], ['--samples', 'bad', '--samples', '1'], ['--samples'], ['--local-service', ''], ['--out-dir'], ['--unknown', 'x']]
            for arguments in invalid:
                complete(command + arguments, env, 2)
                self.assertFalse((root/'out').exists(), arguments)
                self.assertFalse((root/'local_node.count').exists(), arguments)
            complete(command + ['--samples', '0001'], env)
            self.assertEqual(canonical(root)['aggregate']['expected_sample_count'], 3)

    def test_broken_store_stops_before_probe_and_reads_unknown(self):
        for mode in ('lock', 'state', 'rollback', 'digest', 'cache', 'publication_write'):
            with self.subTest(mode=mode), fixtures.host_fixture() as (root, env, command):
                complete(command, env)
                anchor = (root/'out/.observability-initialized').read_bytes()
                state = (root/'out/attempt_state.json').read_bytes()
                complete(command, env)
                before = (root/'local_node.count').read_bytes()
                if mode == 'lock': (root/'out/.observability-attempt.lock').unlink()
                if mode == 'state': (root/'out/attempt_state.json').unlink()
                if mode == 'rollback':
                    (root/'out/.observability-initialized').write_bytes(anchor)
                    (root/'out/attempt_state.json').write_bytes(state)
                if mode == 'digest':
                    current = json.loads((root/'out/attempt_state.json').read_text())
                    (root/'out'/current['publication']['json_path']).write_text('{}')
                if mode == 'cache': (root/'out/latest_summary.json').write_text('{}')
                if mode == 'publication_write':
                    (root/'out/latest_summary.md').unlink()
                    (root/'out/latest_summary.md').mkdir()
                canonical(root, 2)
                if mode in ('lock', 'state', 'rollback', 'publication_write'):
                    result = subprocess.run(command, env=env, capture_output=True, text=True)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertEqual((root/'local_node.count').read_bytes(), before)

    def test_summary_write_failure_records_failed_attempt(self):
        with fixtures.host_fixture() as (root, env, command):
            summary = root/'scripts/p2p-real-env-host-summary.py'
            original = summary.read_text()
            summary.write_text(original.replace("if __name__ == \"__main__\":", "if __name__ == \"__main__\":\n    if '--history-path' in sys.argv: sys.exit(9)"))
            complete(command, env, 1)
            report = canonical(root, 1)
            self.assertEqual(report['attempt']['errors'][0]['stage'], 'summary')

    def test_outer_real_host_and_real_merger_preserve_failed_collection(self):
        spec = importlib.util.spec_from_file_location('outer_fixture', Path(__file__).with_name('p2p-real-env-observability-attempt.test.py'))
        outer_module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(outer_module)
        outer = outer_module.AttemptWrapperTests('runTest')
        outer.setUp()
        self.addCleanup(outer.doCleanups)
        for failures in ({}, {'local_node:1':'fetch'}):
            with self.subTest(failures=failures), fixtures.host_fixture(failures) as (root, env, command):
                scripts = root/'scripts'
                for name in ('p2p-real-env-triad-snapshot.sh', 'p2p-real-env-traffic-monitor.sh', 'oasis7-node-wasm-metrics-monitor.sh'):
                    shutil.copy2(outer.root/'scripts'/name, scripts/name)
                shutil.copy2(fixtures.ROOT/'scripts/p2p-real-env-observability-summary.py', scripts)
                env = dict(env, ATTEMPT_TEST_FIXTURES=str(fixtures.ROOT/'fixtures/p2p_real_env_observability'))
                outer_command = [command[0], str(scripts/'p2p-real-env-observability-monitor.sh'), '--out-dir', str(root/'outer'), '--samples', '1', '--interval-secs', '1', '--local-service', 'local_node.service', '--sequencer-service', 'sequencer_ecs.service', '--storage-service', 'storage_ecs.service']
                complete(outer_command, env, int(bool(failures)))
                state = json.loads((root/'outer/attempt_state.json').read_text())
                run = root/'outer'/state['latest_attempt']['run_directory']
                host = json.loads((run/'host/canonical_summary.json').read_text())
                self.assertTrue(host['canonical_state']['validated'])
                self.assertEqual(host['aggregate']['complete'], not failures)
                merged = json.loads((run/'report/latest_summary.json').read_text())
                self.assertEqual(merged['host']['collection_complete'], not failures)
                self.assertEqual(state['latest_attempt']['result'], 'failed' if failures else 'success')

    def test_outer_dangling_protocol_artifact_never_uses_legacy_child(self):
        spec = importlib.util.spec_from_file_location('outer_symlink_fixture', Path(__file__).with_name('p2p-real-env-observability-attempt.test.py'))
        outer_module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(outer_module)
        for artifact in ('.observability-initialized', 'attempt_state.json'):
            with self.subTest(artifact=artifact):
                outer = outer_module.AttemptWrapperTests('runTest')
                outer.setUp()
                try:
                    host = outer.root/'scripts/p2p-real-env-host-monitor.sh'
                    host.write_text(host.read_text().replace("(out / 'fixture').mkdir(parents=True)", "(out / 'fixture').mkdir(parents=True)\n    (out / " + repr(artifact) + ").symlink_to(out / 'missing-target')"))
                    outer.complete(outer.start(arguments=[]), 1)
                    state = outer.canonical(1)
                    run = outer.out/state['attempt']['run_directory']
                    report = json.loads((run/'host/canonical_summary.json').read_text())
                    self.assertEqual(report['attempt']['result'], 'unknown')
                    self.assertFalse(report['aggregate']['complete'])
                finally:
                    outer.doCleanups()

    def test_store_loss_or_write_failure_during_collection_fails_closed(self):
        for mode in ('lock', 'state', 'publication_write'):
            with self.subTest(mode=mode), fixtures.host_fixture() as (root, env, command):
                complete(command, env)
                self.assertEqual(canonical(root)['last_success']['sequence'], 1)
                pause_probe(root)
                process, gate = start_paused(root, env, command)
                try:
                    if mode == 'lock': (root/'out/.observability-attempt.lock').unlink()
                    if mode == 'state': (root/'out/attempt_state.json').unlink()
                    if mode == 'publication_write':
                        (root/'out/latest_summary.md').unlink()
                        (root/'out/latest_summary.md').mkdir()
                    gate.touch()
                    stdout, stderr = process.communicate(timeout=10)
                    self.assertNotEqual(process.returncode, 0, (stdout, stderr))
                    report = canonical(root, 2)
                    self.assertEqual(report['attempt']['result'], 'unknown')
                    self.assertFalse(report['aggregate']['complete'])
                finally:
                    if process.poll() is None:
                        os.killpg(process.pid, signal.SIGKILL)
                        process.communicate()


if __name__ == '__main__':
    unittest.main()
