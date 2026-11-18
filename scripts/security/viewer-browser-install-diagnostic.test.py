import importlib.util
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import textwrap
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
            root = Path(tmp).resolve(); (root/'sources.list.d').mkdir()
            (root/'sources.list').write_text('deb https://user:secret@archive.ubuntu.com/ubuntu?token=secret noble main\n')
            (root/'auth.conf').write_text('private-auth-secret')
            result = diag.observer.source_snapshot(root)
            self.assertIn('archive.ubuntu.com', str(result))
            self.assertNotIn('secret', str(result)); self.assertNotIn('auth.conf', str(result))

    def test_failed_diagnostic_cannot_turn_install_failure_green(self):
        action = (HERE.parent.parent/'.github/actions/setup-ci-browser/action.yml').read_text()
        steps = {block.splitlines()[0].split('name: ', 1)[1]: block
                 for block in re.findall(r'^    - name: .*?(?=^    - |\Z)', action, re.M | re.S)}
        install = steps['Install missing browser system dependencies']
        diagnostic = steps['Capture browser installation failure']
        self.assertIn('  using: composite\n', action)
        self.assertIn("      if: failure() && inputs.browser == 'agent'\n", diagnostic)
        for name in ('Install locked browser tools without lifecycle scripts',
                     'Verify browser downloads and inspect complete dependencies',
                     'Install missing browser system dependencies',
                     'Verify and launch the actual pinned browser'):
            step = steps[name]
            self.assertNotIn('continue-on-error:', step)
            self.assertNotIn('      if:', step)  # Default success(): no activation after failure.
            self.assertLess(action.index(step), action.index(diagnostic))
        self.assertIn('      shell: bash\n', install)
        self.assertIn('      shell: bash\n', diagnostic)

        def script(step):
            source = step.split('      run: ', 1)[1]
            return textwrap.dedent(source[2:]) if source.startswith('|\n') else source.strip()

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            prepared = root/'oasis7-ci-browser-agent'
            prepared.mkdir()
            (prepared/'apt-arguments.txt').write_text('--\nfixture-browser-package\n')
            trace = root/'commands'
            stub = '''sudo() { printf 'install:%s\\n' "$*" >> "$TRACE"; return "$INSTALL_STATUS"; }
python3() { printf 'diagnostic:%s\\n' "$*" >> "$TRACE"; return "$DIAGNOSTIC_STATUS"; }
'''
            for install_status in (1, 100):
                for diagnostic_status in (0, 7):
                    with self.subTest(install=install_status, diagnostic=diagnostic_status):
                        trace.write_text('')
                        environment = dict(os.environ, RUNNER_TEMP=str(root), BROWSER_KIND='agent',
                            TRACE=str(trace), INSTALL_STATUS=str(install_status),
                            DIAGNOSTIC_STATUS=str(diagnostic_status))
                        command = ['bash', '--noprofile', '--norc', '-e', '-u', '-o', 'pipefail', '-c']
                        failed = subprocess.run(command + [stub + script(install)], cwd=root,
                            env=environment, capture_output=True, text=True, timeout=5)
                        observed = subprocess.run(command + [stub + script(diagnostic)], cwd=root,
                            env=environment, capture_output=True, text=True, timeout=5)
                        # Run the actual step scripts separately, as composite
                        # actions do. A later successful observation does not
                        # erase this failed, non-continue-on-error install step.
                        self.assertEqual(failed.returncode, install_status, failed.stderr)
                        self.assertEqual(observed.returncode, 0, observed.stderr)
                        self.assertEqual('Browser install diagnostic unavailable' in observed.stdout,
                                         diagnostic_status != 0)
                        calls = trace.read_text().splitlines()
                        self.assertEqual(len(calls), 2)
                        self.assertTrue(calls[0].startswith(
                            'install:-n /usr/bin/python3 scripts/ci-apt-install.py '))
                        self.assertTrue(calls[0].endswith(' -- fixture-browser-package'))
                        self.assertEqual(calls[1],
                            'diagnostic:scripts/security/viewer-browser-install-diagnostic.py '
                            'output/viewer-browser-install/diagnostic.json')
        self.assertIn('worker_redirect_chain', (HERE/'viewer-browser-install-diagnostic.py').read_text())

    def test_viewer_callers_keep_failed_composite_install_blocking(self):
        workflow = (HERE.parent.parent/'.github/workflows/rust.yml').read_text()
        jobs = dict(re.findall(r'^  ([\w-]+):\n(.*?)(?=^  [\w-]+:\n|\Z)', workflow, re.M | re.S))
        for name in ('viewer-js-required', 'viewer-performance-report'):
            with self.subTest(job=name):
                job = jobs[name]
                self.assertNotIn('continue-on-error:', job.split('    steps:', 1)[0])
                installs = [step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                            if 'uses: ./.github/actions/setup-ci-browser' in step]
                self.assertEqual(len(installs), 1)
                self.assertIn('          browser: agent\n', installs[0])
                self.assertNotIn('continue-on-error:', installs[0])
                self.assertNotIn('        if:', installs[0])


if __name__ == '__main__': unittest.main()
