#!/usr/bin/env python3
"""Outer CLI routing tests with strict consumer parsers and offline integrations."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import shlex
import subprocess
import sys
import unittest

SCRIPTS = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('attempt_routing_fixture', SCRIPTS / 'p2p-real-env-observability-attempt.test.py')
fixture_module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture_module)

COMMON = {'--samples', '--interval-secs', '--ssh-timeout-secs', '--out-dir'}
TARGETS = {'--sequencer-target', '--storage-target'}
SERVICES = {'--local-service', '--sequencer-service', '--storage-service'}
STATUS = {'--local-status-url', '--sequencer-status-url', '--storage-status-url'}
SNAPSHOT_ONLY = {'--world-id', '--local-health-url', '--local-env-file', '--sequencer-health-url',
                 '--sequencer-env-file', '--sequencer-public-status-url', '--sequencer-public-health-url',
                 '--storage-health-url', '--storage-env-file', '--storage-public-status-url', '--storage-public-health-url'}
HOST_ONLY = {'--local-storage-path', '--sequencer-storage-path', '--storage-storage-path'}
TRAFFIC_ONLY = {'--window-minutes', '--top-n', '--history-retention-minutes', '--as-of-unix-ms',
                '--max-sample-age-secs', '--max-sample-gap-secs', '--history-path', '--summary-only'}


class RoutingTests(unittest.TestCase):
    def setUp(self):
        self.fixture = fixture_module.AttemptWrapperTests('runTest')
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        self.root = self.fixture.root
        self.scripts = self.root / 'scripts'
        self.log = self.root / 'argv'
        self.log.mkdir()
        self.fixture.env['ROUTING_ARGV_LOG'] = str(self.log)
        for name, allowed in [
            ('p2p-real-env-triad-snapshot.sh', COMMON | TARGETS | SERVICES | STATUS | SNAPSHOT_ONLY),
            ('p2p-real-env-host-monitor.sh', COMMON | TARGETS | SERVICES | HOST_ONLY),
            ('p2p-real-env-traffic-monitor.sh', COMMON | TARGETS | STATUS | TRAFFIC_ONLY),
        ]:
            path = self.scripts / name
            validation = '''
allowed = ALLOWED
index = 0
while index < len(args):
    flag = args[index]
    if flag not in allowed:
        print('strict consumer rejects ' + flag, file=sys.stderr)
        sys.exit(2)
    index += 1 if flag == '--summary-only' else 2
pathlib.Path(os.environ['ROUTING_ARGV_LOG'], pathlib.Path(sys.argv[0]).name + '.json').write_text(json.dumps(args))
'''.replace('ALLOWED', repr(allowed))
            path.write_text(path.read_text().replace('args = sys.argv[1:]\n', 'args = sys.argv[1:]\n' + validation, 1))

    def argv(self, name):
        return json.loads((self.log / ('p2p-real-env-' + name + '.sh.json')).read_text())

    def test_status_urls_are_sent_only_to_snapshot_and_traffic(self):
        args = ['--local-status-url', 'http://offline.invalid/custom']
        self.fixture.complete(self.fixture.start(arguments=args))
        self.assertIn('--local-status-url', self.argv('triad-snapshot'))
        self.assertIn('--local-status-url', self.argv('traffic-monitor'))
        self.assertNotIn('--local-status-url', self.argv('host-monitor'))

    def test_all_configured_routes_and_duplicate_values_preserve_consumer_order(self):
        mapping = {flag: ('value with spaces' if 'path' in flag or 'env-file' in flag else 'fixture-' + flag[2:])
                   for flag in TARGETS | SERVICES | STATUS | SNAPSHOT_ONLY | HOST_ONLY}
        args = ['--samples', '08', '--interval-secs', '09', '--traffic-samples', '01',
                '--traffic-interval-secs', '02', '--window-minutes', '010', '--ssh-timeout-secs', '08',
                '--top-n', '03', '--history-retention-minutes', '0120',
                '--max-sample-age-secs', '060', '--max-sample-gap-secs', '0120']
        for flag, value in sorted(mapping.items()): args += [flag, value]
        args += ['--local-service', 'second-service', '--top-n', '04']
        self.fixture.complete(self.fixture.start(arguments=args))
        snapshots = self.argv('triad-snapshot')
        hosts = self.argv('host-monitor')
        traffic = self.argv('traffic-monitor')
        for flag, value in mapping.items():
            for argv, expected in [(snapshots, flag in TARGETS | SERVICES | STATUS | SNAPSHOT_ONLY),
                                   (hosts, flag in TARGETS | SERVICES | HOST_ONLY),
                                   (traffic, flag in TARGETS | STATUS)]:
                self.assertEqual(flag in argv, expected, flag)
                if expected: self.assertEqual(argv[argv.index(flag) + 1], value)
        self.assertEqual([snapshots[i + 1] for i, flag in enumerate(snapshots) if flag == '--local-service'],
                         [mapping['--local-service'], 'second-service'])
        self.assertEqual([hosts[i + 1] for i, flag in enumerate(hosts) if flag == '--local-service'],
                         [mapping['--local-service'], 'second-service'])
        self.assertEqual([traffic[i + 1] for i, flag in enumerate(traffic) if flag == '--top-n'], ['3', '4'])
        self.assertEqual(snapshots[snapshots.index('--samples') + 1], '8')
        self.assertEqual(hosts[hosts.index('--interval-secs') + 1], '9')
        self.assertEqual(traffic[traffic.index('--history-retention-minutes') + 1], '120')
        self.assertEqual(traffic[traffic.index('--max-sample-gap-secs') + 1], '120')

    def invalid(self, args):
        child = self.fixture.start(arguments=args)
        self.fixture.complete(child, 2)
        self.assertFalse(self.fixture.out.exists(), repr(args))
        self.assertEqual(list(self.log.iterdir()), [])

    def test_unknown_node_alias_and_raw_child_flags_rejected_before_attempt(self):
        for args in [['--node', '{}'], ['--observer-status-url', 'fixture'], ['--summary-only'],
                     ['--history-path', 'fixture'], ['--invented', 'x'], ['positional']]:
            self.invalid(args)

    def test_missing_empty_or_flag_value_rejected_before_attempt(self):
        numeric = {'--samples', '--interval-secs', '--traffic-samples', '--traffic-interval-secs', '--window-minutes',
                   '--ssh-timeout-secs', '--top-n', '--history-retention-minutes', '--max-sample-age-secs',
                   '--max-sample-gap-secs', '--as-of-unix-ms'}
        options = TARGETS | SERVICES | STATUS | SNAPSHOT_ONLY | HOST_ONLY | numeric | {'--out-dir', '--traffic-history-path'}
        for flag in sorted(options):
            for tail in [[], [''], ['--help']]:
                self.invalid([flag] + tail)

    def test_invalid_numeric_unicode_and_overflow_are_rejected_before_attempt(self):
        for flag in ['--samples', '--interval-secs', '--traffic-samples', '--traffic-interval-secs', '--window-minutes',
                     '--ssh-timeout-secs', '--top-n', '--history-retention-minutes', '--max-sample-age-secs', '--max-sample-gap-secs']:
            for value in ['0', '-1', '1.5', '٠٨', '2147483648']:
                self.invalid([flag, value])
        self.invalid(['--as-of-unix-ms', '9223372036854775808'])
        self.invalid(['--samples', 'bad', '--samples', '1'])

    def test_help_and_bash3_empty_consumer_arrays_do_not_create_artifacts(self):
        child = self.fixture.start(arguments=['--help'])
        self.fixture.complete(child)
        self.assertFalse(self.fixture.out.exists())
        # Pin the executable even when Homebrew Bash precedes /bin in PATH.
        # macOS /bin/bash exercises Bash 3.2; Linux uses its system Bash.
        binaries = self.root / 'system-bash-bin'
        binaries.mkdir()
        (binaries / 'bash').symlink_to('/bin/bash')
        env = dict(self.fixture.env, PATH=str(binaries) + os.pathsep + os.environ['PATH'])
        self.fixture.complete(self.fixture.start(env, arguments=[]))
        self.assertEqual(self.fixture.canonical()['attempt']['result'], 'success')

    def test_literal_metacharacters_are_argv_data_not_outer_shell_code(self):
        sentinel = self.root / 'must-not-execute'
        literal = "http://offline.invalid/$(touch " + str(sentinel) + ");'quoted';\"double\""
        storage = "data/$(touch " + str(sentinel) + ");'quoted path'"
        self.fixture.complete(self.fixture.start(arguments=['--local-status-url', literal,
                                                           '--local-storage-path', storage]))
        for name in ['triad-snapshot', 'traffic-monitor']:
            argv = self.argv(name)
            self.assertEqual(argv[argv.index('--local-status-url') + 1], literal)
        argv = self.argv('host-monitor')
        self.assertEqual(argv[argv.index('--local-storage-path') + 1], storage)
        self.assertFalse(sentinel.exists())

    def test_invalid_arguments_leave_existing_store_and_consumers_untouched(self):
        self.fixture.complete(self.fixture.start(arguments=[]))
        def snapshot():
            return {str(path.relative_to(self.fixture.out)): (path.read_bytes() if path.is_file() else None,
                    path.stat().st_mtime_ns) for path in self.fixture.out.rglob('*')}
        original = snapshot()
        for log in self.log.iterdir(): log.unlink()
        for args in [['--unknown', 'x'], ['--samples', '٠٨'], ['--node', '{}'], ['--local-status-url', '']]:
            self.fixture.complete(self.fixture.start(arguments=args), 2)
            self.assertEqual(snapshot(), original)
            self.assertEqual(list(self.log.iterdir()), [])

    def test_real_host_and_traffic_parsers_accept_configured_routes_offline(self):
        for name in ['p2p-real-env-host-monitor.sh', 'p2p-real-env-host-summary.py', 'p2p-real-env-observability-attempt.py',
                     'p2p-real-env-traffic-monitor.sh', 'traffic-monitor-summary.py', 'traffic_monitor_freshness.py']:
            shutil.copy2(SCRIPTS / name, self.scripts)
        env_body = 'hostname=offline\ncpu_cores=1\nload1=0\nload5=0\nload15=0\nmem_total_bytes=1024\nmem_available_bytes=1024\n'
        env_body += "service_active_state=active\nservice_sub_state=running\nservice_main_pid=0\nruntime_pid=''\n"
        for key in ['storage_total_bytes', 'storage_used_bytes', 'storage_available_bytes', 'storage_used_percent']:
            env_body += key + "=''\n"
        for prefix in ['wrapper', 'runtime']:
            for key in ['pcpu', 'pmem', 'elapsed_secs', 'nlwp', 'psr']:
                env_body += prefix + '_' + key + "=''\n"
        sample = self.scripts / 'p2p-real-env-node-host-sample.sh'
        sample.write_text("#!/bin/sh\ncat <<'HOSTENV'\n" + env_body + "HOSTENV\nprintf 'storage_path=%q\\n' \"$2\"\n")
        sample.chmod(0o755)
        binaries = self.root / 'bin'
        binaries.mkdir()
        status = json.dumps({'ok': True, 'node_id': 'offline', 'world_id': 'world'})
        for name, body in {
            'curl': '#!' + sys.executable + '\nimport json,os,sys\n'
                + 'with open(os.environ["ROUTING_CURL_LOG"], "a") as log: log.write(json.dumps(sys.argv[1:]) + "\\n")\n'
                + 'print(' + repr(status) + ')\n',
            'ip': '#!/bin/sh\nexit 0\n',
            'ssh': '#!' + sys.executable + '\nimport json,os,sys,shlex\n'
                + 'with open(os.environ["ROUTING_SSH_LOG"], "a") as log: log.write(json.dumps(sys.argv[1:]) + "\\n")\n'
                + 'command = sys.argv[-1]\n'
                + 'print(' + repr(env_body) + ' + "storage_path=" + shlex.quote(shlex.split(command)[4]) + "\\n", end="") if "bash -s --" in command else print('
                + repr(status) + ' if "curl -fsS" in command else ' + repr('{"available":false}') + ')\n',

        }.items():
            path = binaries / name
            path.write_text(body)
            path.chmod(0o755)
        env = dict(self.fixture.env, PATH=str(binaries) + os.pathsep + os.environ['PATH'],
                   ROUTING_SSH_LOG=str(self.root / 'ssh-argv'), ROUTING_CURL_LOG=str(self.root / 'curl-argv'))
        env.pop('P2PARCH6_SEQ_SSH_PASSWORD', None)
        env.pop('P2PARCH6_STORAGE_SSH_PASSWORD', None)
        args = ['--samples', '1', '--traffic-samples', '1', '--local-status-url', 'http://offline.invalid/local',
                '--sequencer-status-url', 'http://offline.invalid/seq', '--storage-status-url', 'http://offline.invalid/storage',
                '--local-storage-path', '/offline/local space', '--sequencer-storage-path', '/offline/seq space',
                '--storage-storage-path', '/offline/storage space', '--local-service', 'offline-local.service',
                '--sequencer-target', 'fixture@seq.invalid', '--storage-target', 'fixture@storage.invalid',
                '--top-n', '3', '--history-retention-minutes', '120']
        self.fixture.complete(self.fixture.start(env, arguments=args))
        report = self.fixture.canonical()
        run = self.fixture.out / report['attempt']['run_directory']
        host = next((run / 'host').glob('runs/*/report/latest_summary.json'))
        host_summary = json.loads(host.read_text())
        self.assertEqual(host_summary['nodes']['local_node']['latest']['storage']['path'], '/offline/local space')
        traffic = json.loads((run / 'traffic' / 'latest_summary.json').read_text())
        self.assertEqual(traffic['nodes']['local_node']['latest']['status_url'], 'http://offline.invalid/local')
        self.assertEqual(traffic['top_n'], 3)
        ssh_calls = [json.loads(line) for line in (self.root / 'ssh-argv').read_text().splitlines()]
        self.assertTrue(any('fixture@seq.invalid' in call and '/offline/seq space' in shlex.split(call[-1]) for call in ssh_calls))
        self.assertTrue(any('fixture@storage.invalid' in call and '/offline/storage space' in shlex.split(call[-1]) for call in ssh_calls))
        self.assertTrue(any('http://offline.invalid/seq' in call[-1] for call in ssh_calls))
        curl_calls = [json.loads(line) for line in (self.root / 'curl-argv').read_text().splitlines()]
        self.assertTrue(any('http://offline.invalid/local' in call for call in curl_calls))



if __name__ == '__main__':
    unittest.main()
