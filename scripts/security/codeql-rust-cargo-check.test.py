#!/usr/bin/env python3
"""Fixed check replay and failure-evidence boundary regressions; no Rust build."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

P = Path(__file__).with_name('codeql-rust-cargo-check.py')
spec = importlib.util.spec_from_file_location('single_check', P)
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)


class SingleCheck(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source = self.root / 'source'; self.work = self.root / 'work'; self.out = self.root / 'out'
        self.work.mkdir(); self.out.mkdir()
        self.lock = self.source / Path(m.MANIFEST).with_name('Cargo.lock')
        self.lock.parent.mkdir(parents=True); self.lock.write_bytes(b'version = 4\n')

    def fake_cargo(self, body):
        cargo = self.root / 'cargo'
        cargo.write_text('#!' + sys.executable + '\n' + body)
        cargo.chmod(0o755)
        return str(cargo)

    def test_native_preparation_stops_at_missing_pkgconfig_module(self):
        calls=[]
        def run(argv, *args, **kwargs):
            calls.append(argv)
            if argv[0] == 'pkg-config':
                raise m.runtime.PhaseError({"failure_kind":"child_exit","exit":1})
            return {"exit":0}
        with patch.object(m.runtime, "run", side_effect=run):
            with self.assertRaises(m.runtime.PhaseError):
                m.prepare_native(self.source,self.out,time.monotonic()+800,dict(os.environ))
        self.assertEqual(len(calls),4)
        self.assertFalse(any("check" in x for x in calls))

    def test_native_preparation_is_bounded_and_probes_original_missing_modules(self):
        calls=[]
        with patch.object(m.runtime, 'run', side_effect=lambda *a, **kw: calls.append((a,kw))):
            m.prepare_native(self.source,self.out,time.monotonic()+800,dict(os.environ))
        self.assertEqual(len(calls),4)
        self.assertEqual(calls[0][0][0][:3],['sudo','-n','timeout'])
        self.assertEqual(len({args[3] for args,_ in calls}),1)
        self.assertLessEqual(calls[0][0][4],60)
        self.assertLessEqual(calls[1][0][4],120)
        for package in ('libgtk-3-dev','libwebkit2gtk-4.1-dev','libdbus-1-dev'):
            self.assertIn(package,calls[1][0][0])
        modules=calls[-1][0][0]
        for module in ('gdk-3.0 >= 3.22','dbus-1 >= 1.6','javascriptcoregtk-4.1 >= 2.38'):
            self.assertIn(module,modules)
        self.assertTrue(all(kw['file_limit'] is None for _,kw in calls))

    def test_native_observations_cannot_overrun_preparation_deadline(self):
        calls=[]
        def run(*args, **kwargs):
            calls.append(args)
        # After update and install, the shared preparation clock has expired.
        with patch.object(m.time,'monotonic',side_effect=[100,100,100,281]), \
                patch.object(m.runtime,'run',side_effect=run):
            with self.assertRaisesRegex(RuntimeError,'native observation budget'):
                m.prepare_native(self.source,self.out,1000,dict(os.environ))
        self.assertEqual(len(calls),2)

    def test_reconstruction_matches_the_original_recorded_hash(self):
        args = m.check_argv(Path('/home/runner/work/oasis7/oasis7'),
                            Path('/home/runner/work/_temp/rust-capacity-databases/rust/working'))
        self.assertEqual(hashlib.sha256(json.dumps(args).encode()).hexdigest(),
                         'ae8fc3d8d27bc6d0e9686f6ce5f9932442f35b7713d59fd503a7ffc8779750ed')

    def test_nonzero_check_preserves_separate_stderr_and_locks(self):
        cargo = self.fake_cargo('import os,sys\nassert "--locked" in sys.argv\n'
                                'assert os.environ["RUSTUP_TOOLCHAIN"]=="1.99.0"\n'
                                'assert "GH_TOKEN" not in os.environ\n'
                                'print("compiler message")\nprint("specific build failure",file=sys.stderr)\n'
                                'sys.exit(101)\n')
        value = m.single_check(self.source, self.work, self.out, time.monotonic()+40,
                               dict(os.environ, GH_TOKEN='must-not-reach-child'), cargo)
        self.assertEqual(value['check_invocations'], 1)
        self.assertEqual(value['process']['exit'], 101)
        self.assertEqual(value['process']['failure_kind'], 'child_exit')
        self.assertEqual(value['process']['cleanup_errors'], [])
        self.assertEqual((self.out/'check.stdout').read_text(), 'compiler message\n')
        self.assertEqual((self.out/'check.stdout.stderr').read_text(), 'specific build failure\n')
        self.assertTrue(value['source_lock_unchanged'] and value['copied_lock_unchanged'])

    def test_copy_updates_are_observed_without_modifying_source(self):
        cargo = self.fake_cargo('import os,pathlib\npathlib.Path(os.environ["CARGO_RESOLVER_LOCKFILE_PATH"]).write_text("changed")\n')
        value = m.single_check(self.source,self.work,self.out,time.monotonic()+40,dict(os.environ),cargo)
        self.assertFalse(value['copied_lock_unchanged'])
        self.assertTrue(value['source_lock_unchanged'])

    def test_expired_budget_does_not_start_check(self):
        cargo = self.fake_cargo('raise AssertionError("must not start")\n')
        value = m.single_check(self.source,self.work,self.out,time.monotonic(),dict(os.environ),cargo)
        self.assertIsNone(value['process']['pid'])
        self.assertEqual(value['process']['failure_kind'], 'timeout')
        self.assertEqual(value['check_invocations'],0)

    def test_unreadable_post_check_lock_cannot_erase_child_failure(self):
        cargo=self.fake_cargo('import os,pathlib,sys\npathlib.Path(os.environ["CARGO_RESOLVER_LOCKFILE_PATH"]).unlink()\nsys.exit(101)\n')
        observed={}
        value=m.single_check(self.source,self.work,self.out,time.monotonic()+40,dict(os.environ),cargo,observed)
        self.assertIs(value,observed)
        self.assertEqual(value['check_invocations'],1)
        self.assertEqual(value['process']['exit'],101)
        self.assertIsNone(value['copied_lock_unchanged'])
        self.assertIn('copied_lock_unchanged',value['lock_errors'])
        self.assertEqual(m.check_status(value['process']),'CHECK_FAILED')

    def test_version_identity_is_not_just_a_release_label(self):
        text = 'release: 1.99.0\ncommit-hash: '+m.CARGO_COMMIT+'\nhost: x86_64-unknown-linux-gnu\n'
        self.assertTrue(m.version_matches(text))
        self.assertFalse(m.version_matches(text.replace(m.CARGO_COMMIT,'0'*40)))
        self.assertFalse(m.version_matches(text.replace('x86_64','aarch64')))
        self.assertTrue(m.rustc_matches('release: 1.99.0\nhost: x86_64-unknown-linux-gnu\n'))
        self.assertFalse(m.rustc_matches('release: 1.98.0\nhost: x86_64-unknown-linux-gnu\n'))

    def test_cleanup_error_is_not_a_successful_check(self):
        self.assertEqual(m.check_status({'exit':0,'cleanup_errors':['live child'],'error':'cleanup'}),'CHECK_INCOMPLETE')

    def test_manual_workflow_has_no_codeql_or_privileged_execution(self):
        text=(P.parents[2]/'.github/workflows/codeql-rust-cargo-check.yml').read_text()
        for required in ('default: false','inputs.execute == true','timeout-minutes: 15',
                         'start+900','ref: '+m.SOURCE,'rustup toolchain install 1.99.0',
                         'include-hidden-files: true','contents: read','actions: read'):
            self.assertIn(required,text)
        for forbidden in ('github/codeql-action/','security-events: write','upload-sarif',
                          'pull_request:','schedule:','CARGO_TARGET_DIR:'):
            self.assertNotIn(forbidden,text)


if __name__ == '__main__':
    unittest.main()
