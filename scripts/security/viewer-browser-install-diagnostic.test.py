import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
spec = importlib.util.spec_from_file_location('diagnostic', HERE/'viewer-browser-install-diagnostic.py')
diag = importlib.util.module_from_spec(spec); spec.loader.exec_module(diag)


class Contracts(unittest.TestCase):
    def test_fixed_readonly_commands_and_no_address_or_error_output(self):
        calls = []
        def run(argv, **kwargs):
            calls.append(argv); self.assertEqual(kwargs['timeout'], 2)
            return subprocess.CompletedProcess(argv, 0, stdout=b'apt 2.8.1 (amd64)\nsecret-not-a-version\n', stderr=b'secret')
        r = diag.collect(run, lambda: {'files': []})
        self.assertEqual(calls, [['apt-get', '--version']] + [['getent', 'ahosts', h] for h in diag.HOSTS])
        self.assertEqual(r['apt_version'][0]['version'], '2.8.1')
        self.assertNotIn('secret', str(r))
        self.assertTrue(all(row['status'] == 'resolved' for row in r['dns']))

    def test_timeout_unavailable_and_original_redirect_unknown(self):
        def run(argv, **kwargs): raise subprocess.TimeoutExpired(argv, 2)
        r = diag.collect(run, lambda: {})
        self.assertTrue(all(row['status'] == 'unavailable' for row in r['dns']))
        self.assertTrue(r['worker_redirect_chain'].startswith('unknown'))

    def test_source_projection_excludes_credentials_and_auth_files(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); (root/'sources.list.d').mkdir()
            (root/'sources.list').write_text('deb https://user:secret@archive.ubuntu.com/ubuntu?token=secret noble main\n')
            (root/'auth.conf').write_text('private-auth-secret')
            result = diag.observer.source_snapshot(root)
            self.assertNotIn('secret', str(result)); self.assertNotIn('auth.conf', str(result))

    def test_failed_diagnostic_cannot_turn_install_failure_green(self):
        for status in [1, 100]:
            p = subprocess.run(['bash', '-c', f'if (exit {status}); then exit 0; else install_status=$?; (exit 7) || true; exit "$install_status"; fi'])
            self.assertEqual(p.returncode, status)
        workflow = (HERE.parent.parent/'.github/workflows/rust.yml').read_text()
        self.assertIn('exit "$install_status"', workflow)
        self.assertIn('worker_redirect_chain', (HERE/'viewer-browser-install-diagnostic.py').read_text())


if __name__ == '__main__': unittest.main()
