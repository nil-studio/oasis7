#!/usr/bin/env python3
"""无特权的 apt 生命周期合同与真实子进程回归。绝不调用 apt/dpkg。"""
import importlib.util
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

PATH = Path(__file__).with_name('ci-apt-install.py')
spec = importlib.util.spec_from_file_location('ci_apt', PATH)
apt = importlib.util.module_from_spec(spec)
spec.loader.exec_module(apt)
TRANSIENT = ('E: Failed to fetch http://mirror.example/index Temporary failure resolving '
             "'mirror.example'\nE: Some index files failed to download. "
             'They have been ignored, or old ones used instead.\n')
RUNNER_MIRRORS = (
    b'http://azure.archive.ubuntu.com/ubuntu/\tpriority:1\n'
    b'https://archive.ubuntu.com/ubuntu/\tpriority:2\n'
    b'https://security.ubuntu.com/ubuntu/\tpriority:3\n')
CANONICAL_FIRST_MIRRORS = (
    b'https://archive.ubuntu.com/ubuntu/\tpriority:1\n'
    b'https://security.ubuntu.com/ubuntu/\tpriority:2\n'
    b'http://azure.archive.ubuntu.com/ubuntu/\tpriority:3\n')
PREFERRED_MIRRORS = (
    b'https://mirrors.edge.kernel.org/ubuntu/\tpriority:1\n'
    b'https://archive.ubuntu.com/ubuntu/\tpriority:2\n'
    b'https://security.ubuntu.com/ubuntu/\tpriority:3\n'
    b'http://azure.archive.ubuntu.com/ubuntu/\tpriority:4\n')

FAKE = r'''
import json, os, signal, subprocess, sys, time
from pathlib import Path
root = Path(os.environ['FIXTURE'])
mode = os.environ['MODE']
child = '--child' in sys.argv
stage = 'child' if child else ('update' if 'update' in sys.argv else 'install')
if child or mode in ('ignore', 'escape', 'leader_exit', 'install_hang'):
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
    signal.signal(signal.SIGINT, signal.SIG_IGN)
signal.alarm(15)  # bounded fixture fallback even when an assertion fails
with (root / 'events').open('a') as out:
    out.write(json.dumps(dict(pid=os.getpid(), stage=stage, args=sys.argv[1:],
                             lc=os.environ.get('LC_ALL'), frontend=os.environ.get('DEBIAN_FRONTEND'))) + '\n')
if child:
    while True: time.sleep(.05)
if mode in ('zombie_success', 'zombie_error', 'zombie_transient'):
    pid = os.fork()
    if pid == 0:
        os._exit(0)
    deadline = time.monotonic() + 3
    while Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()[0] != 'Z':
        if time.monotonic() >= deadline:
            raise RuntimeError('fixture child did not become a zombie')
        time.sleep(.001)
    with (root / 'events').open('a') as out:
        out.write(json.dumps(dict(pid=pid, stage='zombie', state='Z')) + '\n')
    # Deliberately leave the exited child for the helper's subreaper to adopt.
    if mode == 'zombie_error' and stage == 'install':
        sys.exit(42)
if stage == 'install' and mode == 'install_error':
    print(os.environ['TRANSIENT'], flush=True)
    sys.exit(100)
if stage == 'update':
    attempts = len([x for x in (root / 'events').read_text().splitlines() if json.loads(x)['stage'] == 'update'])
    if mode == 'permanent':
        print('E: The repository does not have a Release file.', flush=True)
        sys.exit(100)
    if mode in ('transient', 'exhaust', 'zombie_transient') and (mode == 'exhaust' or attempts == 1):
        print(os.environ['TRANSIENT'], flush=True)
        sys.exit(100)
    if mode == 'unknown':
        print('unrecognized failure', flush=True)
        sys.exit(100)
    if mode == 'wrong_exit':
        print(os.environ['TRANSIENT'], flush=True)
        sys.exit(42)
    if mode == 'overflow':
        print('x' * (2 * 1024 * 1024), flush=True)
    if mode in ('escape', 'leader_exit'):
        subprocess.Popen([sys.executable, __file__, '--child'], start_new_session=True)
        while not any(json.loads(x)['stage'] == 'child' for x in (root / 'events').read_text().splitlines()):
            time.sleep(.005)
        if mode == 'leader_exit': sys.exit(0)
    if mode in ('hang', 'ignore', 'escape'):
        while True: time.sleep(.05)
if stage == 'install' and mode == 'install_hang':
    while True: time.sleep(.05)
sys.exit(0)
'''
RUNNER = r'''
import importlib.util, os, sys
s = importlib.util.spec_from_file_location('ci_apt_fixture', sys.argv[1])
m = importlib.util.module_from_spec(s); s.loader.exec_module(m)
i = m.Installer(float(sys.argv[3]), command=(sys.executable, sys.argv[2]), backoff=float(sys.argv[4]))
if os.environ['MODE'] == 'backpressure':
    for _ in range(1000): i.log('x' * 8192)
sys.exit(i.install(['pkg-config', 'libudev-dev']))
'''


class MirrorPreparation(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory(prefix='ci-apt-mirrors-')
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.mirrors = self.root / 'apt-mirrors.txt'
        self.mirrors.write_bytes(RUNNER_MIRRORS)

    def test_known_runner_list_prefers_independent_https_without_changing_signed_sources(self):
        self.assertEqual(len(RUNNER_MIRRORS), 144)
        source = self.root / 'ubuntu.sources'
        original_source = ('Types: deb\nURIs: mirror+file:/etc/apt/apt-mirrors.txt\n'
                           'Suites: noble noble-updates noble-backports noble-security\n'
                           'Components: main restricted universe multiverse\n'
                           'Architectures: amd64\nSigned-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg\n')
        source.write_text(original_source)
        third_party = self.root / 'vendor.list'
        third_party.write_text('deb [signed-by=/vendor/key.gpg] https://vendor.example/packages stable main\n')
        original_vendor = third_party.read_bytes()
        self.mirrors.chmod(0o640)
        original_stat = self.mirrors.stat()
        self.assertEqual(apt.prepare_runner_mirrors(self.mirrors), 'preferred_independent_https')
        self.assertEqual(self.mirrors.read_bytes(), PREFERRED_MIRRORS)
        current = self.mirrors.stat()
        self.assertEqual((current.st_mode, current.st_uid, current.st_gid),
                         (original_stat.st_mode, original_stat.st_uid, original_stat.st_gid))
        self.assertEqual(source.read_text(), original_source)
        self.assertEqual(third_party.read_bytes(), original_vendor)
        self.assertEqual(apt.prepare_runner_mirrors(self.mirrors), 'already_preferred')
        self.assertEqual(self.mirrors.stat().st_ino, current.st_ino, 'idempotent preparation rewrote the file')

    def test_known_prior_preparation_upgrades_without_losing_any_runner_source(self):
        self.mirrors.write_bytes(CANONICAL_FIRST_MIRRORS)
        self.assertEqual(apt.prepare_runner_mirrors(self.mirrors), 'preferred_independent_https')
        self.assertEqual(self.mirrors.read_bytes(), PREFERRED_MIRRORS)
        original_hosts = {line.split()[0] for line in RUNNER_MIRRORS.splitlines()}
        preferred_hosts = {line.split()[0] for line in self.mirrors.read_bytes().splitlines()}
        self.assertEqual(preferred_hosts - original_hosts, {b'https://mirrors.edge.kernel.org/ubuntu/'})
        self.assertTrue(original_hosts < preferred_hosts)

    def test_missing_custom_mixed_and_lookalike_sources_remain_unchanged(self):
        for content in (b'', RUNNER_MIRRORS[:-1], RUNNER_MIRRORS + b'https://custom.example/\n',
                        RUNNER_MIRRORS.replace(b'archive.ubuntu.com/', b'archive.ubuntu.com.attacker.example/'),
                        RUNNER_MIRRORS.replace(b'priority:1', b'priority:9'),
                        PREFERRED_MIRRORS.replace(b'kernel.org/', b'kernel.org.attacker.example/'),
                        PREFERRED_MIRRORS[:-1], CANONICAL_FIRST_MIRRORS + b'https://custom.example/\n',
                        RUNNER_MIRRORS.replace(b'https://', b'http://'), b'x' * 8192):
            with self.subTest(content=content[:80]):
                self.mirrors.write_bytes(content)
                original = self.mirrors.stat()
                self.assertEqual(apt.prepare_runner_mirrors(self.mirrors), 'unchanged_unrecognized')
                self.assertEqual(self.mirrors.read_bytes(), content)
                self.assertEqual(self.mirrors.stat().st_ino, original.st_ino)
        self.mirrors.unlink()
        self.assertEqual(apt.prepare_runner_mirrors(self.mirrors), 'missing')
        self.assertFalse(self.mirrors.exists())

    def test_symlinks_and_non_regular_files_are_rejected_without_following_them(self):
        link = self.root / 'link'
        link.symlink_to(self.mirrors)
        with self.assertRaises(OSError):
            apt.prepare_runner_mirrors(link)
        self.assertEqual(self.mirrors.read_bytes(), RUNNER_MIRRORS)
        fifo = self.root / 'fifo'
        os.mkfifo(fifo)
        with self.assertRaisesRegex(RuntimeError, 'regular file'):
            apt.prepare_runner_mirrors(fifo)

    def test_replace_failure_preserves_the_original_and_removes_only_owned_temporary_file(self):
        sentinel = self.root / '.ci-apt-mirrors-unrelated'
        sentinel.write_text('keep')
        with patch.object(apt.os, 'replace', side_effect=PermissionError('fixture')):
            with self.assertRaises(PermissionError):
                apt.prepare_runner_mirrors(self.mirrors)
        self.assertEqual(self.mirrors.read_bytes(), RUNNER_MIRRORS)
        self.assertEqual(list(self.root.glob('.ci-apt-mirrors-*')), [sentinel])
        self.assertEqual(sentinel.read_text(), 'keep')

    def test_preparation_cannot_start_apt_after_consuming_the_budget_or_cancellation(self):
        with patch.object(apt.time, 'monotonic', return_value=10.0) as clock:
            installer = apt.Installer(1, mirror_list=self.mirrors)
            def prepare(path):
                clock.return_value = 11.0
                return 'already_preferred'
            with patch.object(apt, 'prepare_runner_mirrors', side_effect=prepare), \
                 patch.object(apt.subprocess, 'Popen', side_effect=AssertionError('apt must not start')):
                self.assertFalse(installer.prepare_sources())
                self.assertEqual(installer.run('update', ['update']), (None, '', False))
            clock.return_value = 20.0
            installer = apt.Installer(1, mirror_list=self.mirrors)
            installer.cancelled = signal.SIGTERM
            with patch.object(apt, 'prepare_runner_mirrors', side_effect=AssertionError('no source write')):
                self.assertFalse(installer.prepare_sources())

    @unittest.skipUnless(sys.platform == 'linux', 'Linux pidfd/subreaper contract')
    def test_native_and_prepare_only_cli_share_preparation_and_propagate_failure(self):
        fake = self.root / 'fake.py'
        fake.write_text('import os\nfrom pathlib import Path\n'
                        'assert Path(os.environ["MIRROR_GUARD"]).read_text() == os.environ["EXPECTED_MIRRORS"]\n'
                        + FAKE)
        code = r'''
import importlib.util, os, sys
from pathlib import Path
s = importlib.util.spec_from_file_location('ci_apt_cli_fixture', sys.argv[1])
m = importlib.util.module_from_spec(s); s.loader.exec_module(m)
m.MIRROR_LIST = Path(sys.argv[2])
m.os.geteuid = lambda: 0  # CLI precondition only; no privilege or real apt/dpkg
fake = sys.argv[3]
installer = m.Installer
m.Installer = lambda budget, **kw: installer(budget, command=(sys.executable, fake), **kw)
if os.environ['PREPARE_ERROR'] == '1':
    m.prepare_runner_mirrors = lambda path: (_ for _ in ()).throw(PermissionError('fixture'))
arguments = ['--prepare-sources-only'] if os.environ['PREPARE_ONLY'] == '1' else ['--', 'pkg-config']
sys.argv = ['ci-apt', '--budget-seconds', '2', *arguments]
sys.exit(m.main())
'''
        events = self.root / 'events'
        for only in (False, True):
            for failure in (False, True):
                with self.subTest(prepare_only=only, failure=failure):
                    self.mirrors.write_bytes(RUNNER_MIRRORS)
                    events.unlink(missing_ok=True)
                    result = subprocess.run([sys.executable, '-c', code, str(PATH), str(self.mirrors), str(fake)],
                        env=dict(os.environ, FIXTURE=str(self.root), MODE='success', TRANSIENT=TRANSIENT,
                                 MIRROR_GUARD=str(self.mirrors), EXPECTED_MIRRORS=PREFERRED_MIRRORS.decode(),
                                 PREPARE_ONLY=str(int(only)), PREPARE_ERROR=str(int(failure))),
                        text=True, capture_output=True, timeout=5)
                    self.assertEqual(result.returncode, int(failure), result.stdout + result.stderr)
                    self.assertNotIn('Traceback', result.stderr)
                    self.assertEqual(self.mirrors.read_bytes(), RUNNER_MIRRORS if failure else PREFERRED_MIRRORS)
                    self.assertEqual(events.exists(), not only and not failure)
                    if events.exists():
                        self.assertEqual([json.loads(line)['stage'] for line in events.read_text().splitlines()],
                                         ['update', 'install'])
                    if only:
                        self.assertNotIn('dependencies_ready=true', result.stdout)


class ErrorClassification(unittest.TestCase):
    def test_explicit_transient_update_errors(self):
        for reason in ('Temporary failure resolving', 'Connection timed out',
                       'Could not connect to', 'Connection failed'):
            with self.subTest(reason=reason):
                self.assertTrue(apt.transient_update('E: Failed to fetch http://mirror/index ' + reason))

    def test_unknown_permanent_and_mixed_errors_do_not_retry(self):
        for line in ('E: Unable to locate package x', 'E: Invalid operation',
                     'E: Permission denied', 'E: Could not get lock /var/lib/dpkg/lock',
                     'E: Hash Sum mismatch', 'E: GPG error: invalid signature',
                     'W: The repository does not have a Release file',
                     'E: HTTP 404 Not Found', 'E: something unknown'):
            with self.subTest(line=line):
                self.assertFalse(apt.transient_update(line))
                self.assertFalse(apt.transient_update(TRANSIENT + line))
        self.assertFalse(apt.transient_update(''))
        self.assertFalse(apt.transient_update('Err:1 mirror\n  Temporary failure resolving'))


@unittest.skipUnless(sys.platform == 'linux', 'Linux pidfd/subreaper contract')
class Lifecycle(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix='ci-apt-fixture-')
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.fake = self.root / 'fake.py'
        self.fake.write_text(FAKE)
        self.processes = []
        self.addCleanup(self.cleanup)

    def cleanup(self):
        for proc in self.processes:
            if proc.poll() is None:
                proc.kill()
                proc.wait(timeout=3)
            for stream in (proc.stdout, proc.stderr):
                if stream: stream.close()
        # A failed regression must not leave its own fake command running.
        for event in self.events():
            pid = event['pid']
            try:
                fd = os.pidfd_open(pid)
                try:
                    if str(self.fake).encode() in Path(f'/proc/{pid}/cmdline').read_bytes():
                        signal.pidfd_send_signal(fd, signal.SIGKILL)
                finally:
                    os.close(fd)
            except (FileNotFoundError, ProcessLookupError):
                pass

    def events(self):
        path = self.root / 'events'
        return [json.loads(x) for x in path.read_text().splitlines()] if path.exists() else []

    def start(self, mode, budget=1.2, backoff=.03):
        proc = subprocess.Popen([sys.executable, '-c', RUNNER, str(PATH), str(self.fake),
                                 str(budget), str(backoff)],
                                env=dict(os.environ, FIXTURE=str(self.root), MODE=mode,
                                         TRANSIENT=TRANSIENT),
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                text=True, start_new_session=True)
        self.processes.append(proc)
        return proc

    def finish(self, proc, started, expect=1, budget=1.2):
        stdout, stderr = proc.communicate(timeout=budget + 3)
        self.assertEqual(proc.returncode, expect, stdout + stderr)
        self.assertLess(time.monotonic() - started, budget + 1.0, stdout + stderr)
        self.assertNotIn('Traceback', stderr)
        for event in self.events():
            self.assertFalse(Path(f"/proc/{event['pid']}").exists(), event)
        return stdout

    def run_mode(self, mode, expect=1, budget=1.2, backoff=.03):
        started = time.monotonic()
        proc = self.start(mode, budget=budget, backoff=backoff)
        return self.finish(proc, started, expect, budget)

    def test_normal_completion_preserves_packages_and_apt_controls(self):
        output = self.run_mode('success', expect=0)
        self.assertIn('dependencies_ready=true', output)
        self.assertEqual([e['stage'] for e in self.events()], ['update', 'install'])
        install = self.events()[-1]
        self.assertEqual(install['args'][-5:], ['install', '-y', '--', 'pkg-config', 'libudev-dev'])
        for event in self.events():
            for option in ('Acquire::Retries=0', 'Acquire::http::Timeout=20',
                           'Acquire::https::Timeout=20', 'DPkg::Lock::Timeout=20',
                           'APT::Update::Error-Mode=any'):
                self.assertIn(option, event['args'])
            self.assertEqual((event['lc'], event['frontend']), ('C', 'noninteractive'))

    def test_transient_update_recovers_with_bounded_retry(self):
        output = self.run_mode('transient', expect=0)
        self.assertEqual([e['stage'] for e in self.events()], ['update', 'update', 'install'])
        self.assertIn('retry=1', output)

    def test_adopted_zombies_are_reaped_before_successful_completion(self):
        output = self.run_mode('zombie_success', expect=0)
        self.assertIn('dependencies_ready=true', output)
        self.assertNotIn('leftover_descendants', output)
        self.assertEqual([e['stage'] for e in self.events()],
                         ['update', 'zombie', 'install', 'zombie'])
        self.assertTrue(all(e['state'] == 'Z' for e in self.events() if e['stage'] == 'zombie'))

    def test_reaping_adopted_zombies_preserves_failed_leader_status(self):
        output = self.run_mode('zombie_error')
        self.assertIn('stage=install attempt=1 exit=42 reason=exit', output)
        self.assertIn('dependencies_ready=false', output)
        self.assertNotIn('leftover_descendants', output)
        self.assertEqual([e['stage'] for e in self.events()],
                         ['update', 'zombie', 'install', 'zombie'])

    def test_reaping_adopted_zombies_preserves_transient_update_retry(self):
        output = self.run_mode('zombie_transient', expect=0)
        self.assertIn('stage=update attempt=1 exit=100 reason=exit', output)
        self.assertIn('retry=1', output)
        self.assertIn('dependencies_ready=true', output)
        self.assertEqual([e['stage'] for e in self.events()],
                         ['update', 'zombie', 'update', 'zombie', 'install', 'zombie'])

    def test_transient_retry_exhaustion_blocks_install(self):
        self.run_mode('exhaust')
        self.assertEqual([e['stage'] for e in self.events()], ['update'] * 3)

    def test_permanent_unknown_and_unexpected_exit_fail_without_retry(self):
        for mode in ('permanent', 'unknown', 'wrong_exit', 'overflow'):
            with self.subTest(mode=mode):
                (self.root / 'events').unlink(missing_ok=True)
                self.run_mode(mode)
                self.assertEqual([e['stage'] for e in self.events()], ['update'])

    def test_install_failure_never_retries_even_network_diagnostic(self):
        self.run_mode('install_error')
        self.assertEqual([e['stage'] for e in self.events()], ['update', 'install'])

    def test_timeout_cleans_hanging_and_term_ignoring_commands(self):
        for mode in ('hang', 'ignore', 'escape', 'install_hang'):
            with self.subTest(mode=mode):
                (self.root / 'events').unlink(missing_ok=True)
                self.assertIn('reason=timeout', self.run_mode(mode))
                self.assertLessEqual(sum(e['stage'] == 'install' for e in self.events()), 1)

    def test_parent_exit_is_not_install_completion_and_detached_child_is_reaped(self):
        output = self.run_mode('leader_exit')
        self.assertIn('reason=leftover_descendants', output)
        self.assertEqual([e['stage'] for e in self.events()], ['update', 'child'])

    def test_backoff_is_inside_total_budget(self):
        self.run_mode('exhaust', budget=.6, backoff=2)
        self.assertEqual([e['stage'] for e in self.events()], ['update'])

    def test_cancellation_kills_only_owned_descendants(self):
        sentinel = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(15)'])
        self.addCleanup(lambda: (sentinel.kill(), sentinel.wait()) if sentinel.poll() is None else None)
        started = time.monotonic()
        proc = self.start('escape', budget=3)
        until = time.monotonic() + 2
        while not any(e['stage'] == 'child' for e in self.events()):
            self.assertLess(time.monotonic(), until)
            time.sleep(.01)
        proc.send_signal(signal.SIGTERM)
        output = self.finish(proc, started, 128 + signal.SIGTERM, budget=3)
        self.assertIn('reason=cancelled', output)
        self.assertIsNone(sentinel.poll(), 'unrelated process was signalled')

    def test_stdout_backpressure_does_not_extend_deadline(self):
        started = time.monotonic()
        proc = self.start('backpressure', budget=.7)
        proc.wait(timeout=2)  # Deliberately do not consume the log pipe while it runs.
        self.finish(proc, started, expect=0, budget=.7)

    def test_main_exception_diagnostic_respects_blocked_stdout_budget(self):
        code = r'''
import importlib.util, os, sys
s = importlib.util.spec_from_file_location('ci_apt_main_fixture', sys.argv[1])
m = importlib.util.module_from_spec(s); s.loader.exec_module(m)
os.set_blocking(1, False)
try:
    while True: os.write(1, b'x' * 8192)
except BlockingIOError:
    pass
os.set_blocking(1, True)
m.os.geteuid = lambda: 0  # only the CLI precondition, never elevate or run apt
m.Installer.install = lambda self, packages: (_ for _ in ()).throw(RuntimeError('fixture'))
sys.argv = ['ci-apt', '--budget-seconds', '.2', '--', 'pkg-config']
sys.exit(m.main())
'''
        started = time.monotonic()
        proc = subprocess.Popen([sys.executable, '-c', code, str(PATH)],
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.processes.append(proc)
        proc.wait(timeout=1.2)  # Reading here would hide a blocking exception print.
        self.finish(proc, started, expect=1, budget=.2)

    def test_missing_command_fails_before_install(self):
        self.fake.unlink()
        self.run_mode('success')
        self.assertEqual(self.events(), [])


if __name__ == '__main__':
    unittest.main()
