#!/usr/bin/env python3
"""Browser supervisor contracts; no package manager, network or privilege changes."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import signal
import sys
import tempfile
import time
import unittest
from types import SimpleNamespace
from unittest.mock import patch, Mock

PATH = Path(__file__).with_name('ci-browser-install.py')
spec = importlib.util.spec_from_file_location('browser', PATH)
browser = importlib.util.module_from_spec(spec)
spec.loader.exec_module(browser)


class BrowserInstallTests(unittest.TestCase):
    def test_command_construction_preserves_verified_user_and_exact_arguments(self):
        user = SimpleNamespace(pw_name='runner', pw_gid=1001, pw_dir='/home/runner')
        with patch.object(browser.os, 'geteuid', return_value=0), \
             patch.object(browser.pwd, 'getpwuid', return_value=user), \
             patch.dict(os.environ, SUDO_UID='1001', SUDO_GID='1001', SUDO_USER='runner'):
            command = browser.caller_command(['/opt/node', '/tmp/cli.js', 'install', '--with-deps'], '/opt/bin:/usr/bin')
        self.assertEqual(command[:6], ['/usr/sbin/runuser', '--user', 'runner', '--preserve-environment', '--', '/usr/bin/env'])
        self.assertEqual(command[6], '-i')
        self.assertIn('HOME=/home/runner', command)
        self.assertEqual(command[-4:], ['/opt/node', '/tmp/cli.js', 'install', '--with-deps'])

    def test_root_mismatched_and_non_sudo_callers_fail_closed(self):
        user = SimpleNamespace(pw_name='runner', pw_gid=1001, pw_dir='/home/runner')
        for uid, gid, name in [('0', '1001', 'runner'), ('1001', '0', 'runner'), ('1001', '1001', 'other'), ('bad', '1001', 'runner')]:
            with self.subTest(uid=uid, gid=gid, name=name), \
                 patch.object(browser.os, 'geteuid', return_value=0), \
                 patch.object(browser.pwd, 'getpwuid', return_value=user), \
                 patch.dict(os.environ, SUDO_UID=uid, SUDO_GID=gid, SUDO_USER=name):
                with self.assertRaises(RuntimeError):
                    browser.caller_command(['/opt/node'], '/usr/bin')
        with patch.object(browser.os, 'geteuid', return_value=1001):
            with self.assertRaises(RuntimeError):
                browser.caller_command(['/opt/node'], '/usr/bin')

    def run_fixture(self, code, budget=2):
        # Run supervision in a fresh process, because subreaper ownership is per process.
        harness = ('import importlib.util,sys; '
                   's=importlib.util.spec_from_file_location("b",sys.argv[1]); '
                   'b=importlib.util.module_from_spec(s); s.loader.exec_module(b); '
                   'sys.exit(b.supervise([sys.executable,"-c",sys.argv[2]],float(sys.argv[3])))')
        return subprocess.run([sys.executable, '-c', harness, str(PATH), code, str(budget)],
                              capture_output=True, text=True, timeout=10)

    def test_success_and_failure_exit_are_not_relabelled(self):
        good = self.run_fixture('pass')
        bad = self.run_fixture('raise SystemExit(42)')
        self.assertEqual(good.returncode, 0, good.stdout + good.stderr)
        self.assertIn('browser_install_ready=true', good.stdout)
        self.assertEqual(bad.returncode, 42, bad.stdout + bad.stderr)
        self.assertIn('browser_install_ready=false', bad.stdout)

    @unittest.skipUnless(sys.platform == 'linux', 'Linux pidfd/subreaper contract')
    def test_timeout_kills_escaped_descendant_and_remains_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            record = Path(directory) / 'child'
            child = ('import os,signal,time; signal.signal(signal.SIGTERM,signal.SIG_IGN); '
                     f'open({str(record)!r},"w").write(str(os.getpid())); time.sleep(30)')
            parent = ('import subprocess,sys,time; '
                      f'subprocess.Popen([sys.executable,"-c",{child!r}],start_new_session=True); '
                      'time.sleep(30)')
            start = time.monotonic()
            result = self.run_fixture(parent, 1.5)
            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertLess(time.monotonic() - start, 5)
            self.assertIn('reason=timeout', result.stdout)
            self.assertIn('cleanup=complete', result.stdout)
            self.assertIn('browser_install_ready=false', result.stdout)
            self.assertTrue(record.exists(), result.stderr)
            self.assertFalse(Path('/proc', record.read_text()).exists(), result.stdout)

    def test_zero_exit_with_incomplete_cleanup_never_passes(self):
        installer = Mock(cancelled=0)
        installer.run_command.return_value = (0, '', False)
        with patch.object(browser.apt, 'establish_owner'), \
             patch.object(browser.apt, 'Installer', return_value=installer):
            self.assertEqual(browser.supervise(['/never-executed'], 1), 1)
        installer.log.assert_called_with('browser_install_ready=false')

    def test_successful_leader_with_live_descendant_is_not_success(self):
        child = 'import time; time.sleep(30)'
        parent = ('import subprocess,sys; '
                  f'subprocess.Popen([sys.executable,"-c",{child!r}],start_new_session=True)')
        result = self.run_fixture(parent, 2)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn('reason=leftover_descendants', result.stdout)
        self.assertIn('cleanup=complete', result.stdout)

    def test_cancellation_cleans_owned_child_and_returns_signal_status(self):
        with tempfile.TemporaryDirectory() as directory:
            record = Path(directory) / 'ready'
            child = ('import os,time; '
                     f'open({str(record)!r},"w").write(str(os.getpid())); time.sleep(30)')
            harness = ('import importlib.util,sys; '
                       's=importlib.util.spec_from_file_location("b",sys.argv[1]); '
                       'b=importlib.util.module_from_spec(s); s.loader.exec_module(b); '
                       'sys.exit(b.supervise([sys.executable,"-c",sys.argv[2]],5))')
            proc = subprocess.Popen([sys.executable, '-c', harness, str(PATH), child],
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            try:
                deadline = time.monotonic() + 3
                while not record.exists() and time.monotonic() < deadline:
                    time.sleep(.01)
                self.assertTrue(record.exists())
                proc.send_signal(signal.SIGTERM)
                out, err = proc.communicate(timeout=7)
                self.assertEqual(proc.returncode, 143, out + err)
                self.assertIn('reason=cancelled', out)
                self.assertIn('cleanup=complete', out)
                self.assertFalse(Path('/proc', record.read_text()).exists())
            finally:
                if proc.poll() is None:
                    proc.kill()
                proc.communicate()

    def test_helper_has_registered_non_privileged_contracts(self):
        root = PATH.parent.parent
        config = json.loads((root / 'scripts/ci-required-scope.json').read_text())
        for path in ('scripts/ci-browser-install.py', 'scripts/ci-browser-install.test.py'):
            self.assertEqual(config['scoped_control_paths'][path], ['workflow_governance'])
            self.assertIn('scripts/ci-browser-install.test.py', config['python_control_contracts'][path]['tests'])
        self.assertIn('scripts/ci-browser-install.test.py', config['workflow_governance_python_tests'])


if __name__ == '__main__':
    unittest.main()
