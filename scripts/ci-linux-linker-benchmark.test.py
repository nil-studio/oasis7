#!/usr/bin/env python3
"""Contracts for frozen linker measurements; no production build or downloads."""
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tarfile
import tempfile
import time
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).with_name('ci-linux-linker-benchmark.py')
SPEC = importlib.util.spec_from_file_location('linker_benchmark', SCRIPT)
BENCH = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BENCH)


def archive(path, files):
    with tarfile.open(path, 'w') as output:
        for name, content, kind in files:
            member = tarfile.TarInfo(name)
            member.type = kind
            member.size = len(content) if kind == tarfile.REGTYPE else 0
            if kind in (tarfile.SYMTYPE, tarfile.LNKTYPE):
                member.linkname = '../../outside'
            output.addfile(member, io.BytesIO(content) if kind == tarfile.REGTYPE else None)


def alive(pid):
    try:
        value = Path(f'/proc/{pid}/stat').read_text()
    except FileNotFoundError:
        return False
    return value[value.rindex(')') + 2:].split()[0] != 'Z'


class Contracts(unittest.TestCase):
    def test_normalization_removes_linker_and_compiler_cache_overrides(self):
        poisoned = {'RUSTFLAGS': '-Clinker=false', 'CARGO_ENCODED_RUSTFLAGS': 'wrong',
                    'CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER': '/old/mold-gcc',
                    'CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS': '-Copt-level=3',
                    'RUSTC_WRAPPER': 'sccache', 'RUSTC_WORKSPACE_WRAPPER': 'custom',
                    'CARGO_BUILD_RUSTC_WRAPPER': 'other', 'CARGO_BUILD_TARGET': 'wasm32',
                    'CARGO_PROFILE_DEV_OPT_LEVEL': '3', 'LLD_REPRODUCE': '/wrong.tar',
                    'CARGO_HOME': '/shared/registry', 'PATH': '/tools'}
        with patch.dict(os.environ, poisoned, clear=True):
            result = BENCH.normalized_env(Path('/owned/target'), 2)
            self.assertEqual(dict(os.environ), poisoned)
        for key in set(poisoned) - {'CARGO_HOME', 'PATH'}:
            self.assertNotIn(key, result)
        self.assertEqual(result['CARGO_TARGET_DIR'], '/owned/target')
        self.assertEqual(result['CARGO_PROFILE_DEV_DEBUG'], '0')
        self.assertEqual(result['CARGO_PROFILE_DEV_LTO'], 'false')
        self.assertEqual(result['CARGO_INCREMENTAL'], '0')
        self.assertEqual(result['CARGO_HOME'], '/shared/registry')

    def test_archive_rejects_escaping_links_duplicates_and_unclosed_response(self):
        good = ('capture/response.txt', b'--chroot .\n-o original\nfile.o\n', tarfile.REGTYPE)
        cases = [
            [good, ('../outside', b'bad', tarfile.REGTYPE)],
            [good, ('/outside', b'bad', tarfile.REGTYPE)],
            [good, ('capture/alias', b'', tarfile.SYMTYPE)],
            [good, ('capture/alias', b'', tarfile.LNKTYPE)],
            [good, good],
            [('capture/response.txt', b'-o output file.o\n', tarfile.REGTYPE)],
            [('capture/response.txt', b'--chroot . @other.rsp\n', tarfile.REGTYPE)],
            [('capture/response.txt', b'--chroot . ../outside.o\n', tarfile.REGTYPE)],
            [good, ('capture/' + BENCH.OUTPUT, b'input collision', tarfile.REGTYPE)],
        ]
        for files in cases:
            with self.subTest(files=files), tempfile.TemporaryDirectory() as raw:
                root = Path(raw)
                archive(root / 'capture.tar', files)
                with self.assertRaises(BENCH.MeasurementError):
                    BENCH.unpack_inputs(root / 'capture.tar', root / 'inputs')
                self.assertFalse((root / 'outside').exists())

    def test_frozen_manifest_ignores_only_the_owned_output_and_detects_input_change(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            archive(root / 'capture.tar', [
                ('capture/response.txt', b'--chroot . -o original file.o\n', tarfile.REGTYPE),
                ('capture/file.o', b'actual input', tarfile.REGTYPE)])
            bundle = BENCH.unpack_inputs(root / 'capture.tar', root / 'inputs')
            before = BENCH.input_manifest(bundle)
            (bundle / BENCH.OUTPUT).write_bytes(b'new binary')
            self.assertEqual(BENCH.input_manifest(bundle), before)
            (bundle / 'file.o').write_bytes(b'changed input')
            self.assertNotEqual(BENCH.input_manifest(bundle), before)

    def test_statistics_keep_all_balanced_pairs_and_reject_invalid_samples(self):
        self.assertEqual(BENCH.sample_order(4), [
            (1, 'lld'), (1, 'mold'), (2, 'mold'), (2, 'lld'),
            (3, 'lld'), (3, 'mold'), (4, 'mold'), (4, 'lld')])
        values = {'lld': [10, 8, 6, 4], 'mold': [5, 6, 3, 3]}
        samples = [{'phase': 'warmup', 'pair': 0, 'linker': 'lld', 'wall_seconds': 1000}]
        samples += [{'phase': 'measured', 'pair': pair, 'linker': name,
                     'wall_seconds': values[name][pair - 1], 'success': True, 'artifact': {'sha256': name}}
                    for pair, name in BENCH.sample_order(4)]
        report = BENCH.compare(samples, 4)
        self.assertEqual(report['linkers']['lld']['median_seconds'], 7)
        self.assertEqual(report['linkers']['mold']['median_seconds'], 4)
        self.assertEqual(report['median_paired_saved_seconds'], 2.5)
        self.assertEqual(report['median_paired_mold_change_pct'], -37.5)
        for bad in (samples[:-1], [*samples, samples[-1]], [*samples[:1], *reversed(samples[1:])]):
            with self.assertRaises(BENCH.MeasurementError):
                BENCH.compare(bad, 4)
        for key, value in (('success', False), ('wall_seconds', 0), ('wall_seconds', float('inf'))):
            changed = [dict(s) for s in samples]
            changed[1][key] = value
            with self.assertRaises(BENCH.MeasurementError):
                BENCH.compare(changed, 4)
        changed = [dict(s) for s in samples]
        del changed[1]['artifact']
        with self.assertRaises(BENCH.MeasurementError):
            BENCH.compare(changed, 4)

    @unittest.skipUnless(sys.platform.startswith('linux'), 'Linux process-group contract')
    def test_wall_waits_for_fork_child_pipe_eof(self):
        code = 'import os,time; p=os.fork(); os._exit(0) if p else None; time.sleep(0.2); print("child-finished")'
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            result, stdout, _ = BENCH.invoke([sys.executable, '-c', code], root, dict(os.environ),
                                            time.monotonic() + 5, root / 'fork')
            self.assertTrue(result['success'])
            self.assertIn(b'child-finished', stdout)
            self.assertGreaterEqual(result['wall_seconds'], 0.15)

    @unittest.skipUnless(sys.platform.startswith('linux'), 'Linux process-group contract')
    def test_timeout_kills_owned_fork_child_and_retains_failure(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            pidfile = root / 'child.pid'
            code = ('import os,time,pathlib; p=os.fork(); os._exit(0) if p else None; '
                    f'pathlib.Path({str(pidfile)!r}).write_text(str(os.getpid())); time.sleep(60)')
            foreign = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'])
            try:
                result, _, _ = BENCH.invoke([sys.executable, '-c', code], root, dict(os.environ),
                                            time.monotonic() + 5, root / 'timeout', limit=0.3)
                self.assertFalse(result['success'])
                self.assertTrue(result['timed_out'])
                self.assertFalse(alive(int(pidfile.read_text())))
                self.assertIsNone(foreign.poll())
                record = json.loads((root / 'timeout.invocation.json').read_text())
                self.assertTrue(record['timed_out'])
            finally:
                foreign.terminate()
                foreign.wait(timeout=5)

    def test_failed_summary_never_presents_speedup(self):
        text = BENCH.summary({'success': False, 'samples': [], 'error': 'missing output'})
        self.assertIn('FAIL', text)
        self.assertIn('No performance conclusion', text)
        self.assertNotIn('Median paired mold change', text)


class RealRust(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not sys.platform.startswith('linux') or not shutil.which('rustc') or not shutil.which('cargo'):
            raise unittest.SkipTest('requires installed Linux Rust 1.96.0 and mold; does not install tools')
        version = subprocess.check_output(['rustc', '-vV'], text=True)
        if 'release: 1.96.0\n' not in version or 'host: x86_64-unknown-linux-gnu' not in version:
            raise unittest.SkipTest('fixed x86_64 GNU Rust 1.96.0 contract')
        hint = os.environ.get('CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER')
        cls.mold = Path(hint).parent / 'mold' if hint else Path(shutil.which('mold') or '/missing/mold')
        if not cls.mold.is_file():
            raise unittest.SkipTest('mold is not installed; use the existing CI setup action')

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='linker-contract-')
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.repo = self.root / 'repo'
        (self.repo / 'src').mkdir(parents=True)
        (self.repo / 'Cargo.toml').write_text('[package]\nname="oasis7_newapi_bridge_service"\n'
                                             'version="0.0.0"\nedition="2024"\n[workspace]\n')
        (self.repo / 'src/main.rs').write_text('fn main() { assert_eq!(std::env::args().nth(1).as_deref(), '
                                              'Some("--help")); println!("Usage: fixture [options]"); }\n')
        commands = [['git', 'init', '-q'], ['cargo', 'generate-lockfile', '--offline'], ['git', 'add', '.'],
                    ['git', '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
                     'commit', '-qm', 'fixture: native link']]
        for command in commands:
            subprocess.run(command, cwd=self.repo, check=True, capture_output=True)
        self.source = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=self.repo, text=True).strip()

    def command(self, out):
        return [sys.executable, str(SCRIPT), '--repo', str(self.repo), '--out-dir', str(out),
                '--mold', str(self.mold), '--pairs', '2']

    def test_same_inputs_real_lld_and_mold_produce_executable_binaries(self):
        out = self.root / 'report'
        completed = subprocess.run(self.command(out), capture_output=True, text=True, timeout=60)
        self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)
        report = json.loads((out / 'report.json').read_text())
        self.assertTrue(report['success'])
        self.assertEqual(report['source'], self.source)
        self.assertEqual(len(report['samples']), 6)
        self.assertEqual(report['comparison']['linkers']['lld']['samples'], 2)
        self.assertEqual(report['comparison']['linkers']['mold']['samples'], 2)
        self.assertTrue((out / 'capture.tar').is_file())
        self.assertTrue((out / 'inputs.json').is_file())
        self.assertEqual(len({s['artifact']['help_stdout_sha256'] for s in report['samples']}), 1)
        for sample in report['samples']:
            self.assertTrue(sample['success'])
            self.assertIn('LLD' if sample['linker'] == 'lld' else 'mold 3.0.0',
                          sample['artifact']['linker_comment'])
        worktrees = subprocess.check_output(['git', 'worktree', 'list', '--porcelain'], cwd=self.repo, text=True)
        self.assertEqual(worktrees.count('worktree '), 1)
        self.assertEqual(subprocess.check_output(['git', 'status', '--porcelain'], cwd=self.repo), b'')

    def test_sigterm_preserves_report_and_stops_owned_prefetch_tree(self):
        out, tools = self.root / 'cancelled', self.root / 'tools'
        tools.mkdir()
        pidfile = self.root / 'worker.pid'
        fake = tools / 'cargo'
        fake.write_text('#!' + sys.executable + '\nimport os,sys,time,pathlib\n'
                        'if sys.argv[1:] == ["-V"]: print("cargo fixture"); sys.exit(0)\n'
                        'if sys.argv[1] != "fetch": sys.exit(9)\n'
                        'p=os.fork()\nif p: os._exit(0)\n'
                        f'pathlib.Path({str(pidfile)!r}).write_text(str(os.getpid()))\n'
                        'time.sleep(60)\n')
        fake.chmod(0o755)
        env = {**os.environ, 'PATH': str(tools) + os.pathsep + os.environ['PATH']}
        process = subprocess.Popen(self.command(out), stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
        try:
            deadline = time.monotonic() + 10
            while not pidfile.exists() and process.poll() is None and time.monotonic() < deadline:
                time.sleep(0.01)
            self.assertTrue(pidfile.exists())
            process.send_signal(signal.SIGTERM)
            stdout, stderr = process.communicate(timeout=10)
            self.assertNotEqual(process.returncode, 0, stdout + stderr)
            report = json.loads((out / 'report.json').read_text())
            self.assertFalse(report['success'])
            self.assertIn('signal', report['error'])
            self.assertNotIn('comparison', report)
            self.assertFalse(alive(int(pidfile.read_text())))
            self.assertTrue((out / 'logs/prefetch.invocation.json').is_file())
            worktrees = subprocess.check_output(['git', 'worktree', 'list', '--porcelain'], cwd=self.repo, text=True)
            self.assertEqual(worktrees.count('worktree '), 1)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait(timeout=5)


if __name__ == '__main__':
    unittest.main()
