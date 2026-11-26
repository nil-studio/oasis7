#!/usr/bin/env python3
"""CanonicalWorkspaceOutputs and OutputBackpressure regressions split from ci-formal-process.test.py."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def module(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), HERE / (name + '.py'))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


artifacts = module('ci-test-artifacts')
from ci_process_runner import run, _Capture, READ_BYTES, SUMMARY_LINE_BYTES


class CanonicalWorkspaceOutputs(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name).resolve()
        self.root = self.base / 'checkout'
        self.root.mkdir()
        self.manifest = artifacts.ProviderArtifact.MANIFEST
        self.project = self.root / Path(self.manifest).parent
        (self.project / 'src').mkdir(parents=True)
        (self.project / 'Cargo.toml').write_text(
            '[workspace]\n[package]\nname="canonical_cache_fixture"\nversion="0.1.0"\nedition="2021"\n')
        (self.project / 'src/lib.rs').write_text('pub fn value() -> u8 { 7 }\n')
        fingerprint = 'debug/.fingerprint/fixture-' + 'a' * 16
        self.payload = ['debug/deps/libfixture-' + 'a' * 16 + '.rlib', fingerprint + '/lib-fixture']
        for name in self.payload:
            path = self.root / 'target' / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('original native unit')
        self.fingerprint = fingerprint

    def state(self):
        state = artifacts.cache_begin(self.root, selection='libraries')
        state['files'], state['directories'] = self.payload, [self.fingerprint]
        return state

    def register(self, state, metadata=None):
        metadata = metadata or dict(workspace_root=str(self.project), target_directory=str(self.project / 'target'))
        with patch.object(artifacts.subprocess, 'run', return_value=subprocess.CompletedProcess(
                [], 0, stdout=json.dumps(metadata))) as execute:
            artifacts.cache_register_canonical_targets(state, [self.manifest])
        return execute

    def output(self):
        path = self.project / 'target/wasm32-unknown-unknown/release/module.wasm'
        path.parent.mkdir(parents=True)
        path.write_bytes(b'owned compiler output')

    def test_real_standalone_cargo_outputs_do_not_become_native_source_inputs(self):
        self.assertNotIn('CARGO_TARGET_DIR', os.environ)
        environment = {key: value for key, value in os.environ.items() if key != 'RUSTC_WRAPPER'}
        subprocess.run(['cargo', 'generate-lockfile', '--offline'], cwd=self.project,
                       check=True, env=environment, capture_output=True)
        state = self.state()
        artifacts.cache_register_canonical_targets(state, [self.manifest])
        subprocess.run(['cargo', 'build', '--locked', '--offline', '--lib'], cwd=self.project,
                       check=True, env=environment, capture_output=True)
        changed = set(artifacts._cache_inputs(self.root)) - set(state['inputs'])
        self.assertTrue(changed)
        prefix = (Path(self.manifest).parent / 'target').as_posix() + '/'
        self.assertTrue(all(name.startswith(prefix) for name in changed))
        artifacts.cache_finish(state)
        self.assertTrue(state['complete'])
        cache = self.base / 'cache'
        report = artifacts.cache_stage(state, cache)
        self.assertTrue(report['save'], report)
        saved = json.loads((cache / 'manifest.json').read_text())
        self.assertNotIn('generated_cargo_targets', saved)
        self.assertEqual(saved['inputs'], state['inputs'])
        # A reader still validates the complete clean checkout; no output
        # allowlist is supplied by the long-lived archive.
        shutil.rmtree(self.project / 'target')
        shutil.rmtree(self.root / 'target')
        self.assertTrue(artifacts.cache_restore(self.root, cache)['restored'])

    def test_existing_inputs_are_never_removed_by_output_registration(self):
        self.output()
        state = self.state()
        with self.assertRaisesRegex(ValueError, 'captured source input'):
            self.register(state)
        self.assertNotIn('generated_cargo_targets', state)
        self.assertEqual(artifacts._cache_inputs(self.root), state['inputs'])

    def test_outputs_created_by_the_native_phase_cannot_be_registered_later(self):
        state = self.state()
        self.output()
        with self.assertRaisesRegex(ValueError, 'existed before registration'):
            self.register(state)
        self.assertNotIn('generated_cargo_targets', state)
        with self.assertRaisesRegex(ValueError, 'source inputs changed'):
            artifacts.cache_finish(state)

    def test_metadata_cannot_create_new_inputs_and_then_declare_them_outputs(self):
        state = self.state()
        def metadata(*args, **keywords):
            self.output()
            return subprocess.CompletedProcess([], 0, stdout=json.dumps(dict(
                workspace_root=str(self.project), target_directory=str(self.project / 'target'))))
        with patch.object(artifacts.subprocess, 'run', side_effect=metadata), \
             self.assertRaisesRegex(ValueError, 'existed before registration'):
            artifacts.cache_register_canonical_targets(state, [self.manifest])
        self.assertNotIn('generated_cargo_targets', state)
        with self.assertRaisesRegex(ValueError, 'source inputs changed'):
            artifacts.cache_finish(state)

    def test_real_source_mutation_and_unknown_nested_output_still_fail_closed(self):
        for name in ('src/lib.rs', 'other/target/output', '.ignored-input'):
            with self.subTest(name=name):
                state = self.state()
                self.register(state)
                path = self.project / name
                path.parent.mkdir(parents=True, exist_ok=True)
                existed = path.exists()
                original = path.read_bytes() if existed else None
                path.write_text('changed actual input')
                with self.assertRaisesRegex(ValueError, 'source inputs changed during the build'):
                    artifacts.cache_finish(state)
                self.assertFalse(state['complete'])
                if existed:
                    path.write_bytes(original)
                else:
                    path.unlink()

    def test_metadata_must_prove_the_exact_default_owned_target(self):
        state = self.state()
        for metadata in (dict(workspace_root=str(self.root), target_directory=str(self.project / 'target')),
                         dict(workspace_root=str(self.project), target_directory=str(self.base / 'outside')),
                         dict(workspace_root=str(self.project), target_directory=str(self.project / 'src'))):
            with self.subTest(metadata=metadata), self.assertRaisesRegex(ValueError, 'unsupported output layout'):
                self.register(state, metadata)
            self.assertNotIn('generated_cargo_targets', state)
        with patch.dict(os.environ, CARGO_TARGET_DIR=str(self.project / 'target')), \
             self.assertRaisesRegex(ValueError, 'original default target'):
            self.register(state)
        with patch.object(artifacts.subprocess, 'run', side_effect=subprocess.TimeoutExpired('cargo', 30)), \
             self.assertRaisesRegex(ValueError, 'cannot verify'):
            artifacts.cache_register_canonical_targets(state, [self.manifest])
        self.assertNotIn('generated_cargo_targets', state)

    def test_reader_of_state_rejects_forged_and_swapped_output_roots(self):
        state = self.state()
        self.register(state)
        artifacts.cache_finish(state)
        for records in ({'other/Cargo.toml': 'other/target'}, {self.manifest: 'src'},
                        {self.manifest: []}):
            with self.subTest(records=records):
                forged = dict(state, generated_cargo_targets=records)
                self.assertFalse(artifacts.cache_stage(forged, self.base / 'cache')['save'])
        outside = self.base / 'outside'
        outside.mkdir()
        (self.project / 'target').symlink_to(outside, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, 'symlink'):
            artifacts.cache_finish(state)
        # Re-establish the genuinely completed capture to exercise the stage
        # parser independently of its incomplete-state rejection.
        state['complete'] = True
        for measure in (True, False):
            report = artifacts.cache_stage(state, self.base / 'cache', measure_only=measure)
            self.assertFalse(report['save'])
            self.assertIn('symlink', report['reason'])
        self.assertEqual(list(outside.iterdir()), [])

    def test_registration_and_stage_recheck_symlinks_before_metadata_and_after_copy(self):
        state = self.state()
        outside = self.base / 'outside'
        outside.mkdir()
        output = self.project / 'target'
        output.symlink_to(outside, target_is_directory=True)
        with patch.object(artifacts.subprocess, 'run') as execute, \
             self.assertRaisesRegex(ValueError, 'symlink'):
            artifacts.cache_register_canonical_targets(state, [self.manifest])
        execute.assert_not_called()
        output.unlink()
        self.register(state)
        artifacts.cache_finish(state)
        original = artifacts._cache_copy
        def swap(*args):
            original(*args)
            output.symlink_to(outside, target_is_directory=True)
        with patch.object(artifacts, '_cache_copy', side_effect=swap):
            report = artifacts.cache_stage(state, self.base / 'cache')
        self.assertFalse(report['save'])
        self.assertIn('symlink', report['reason'])
        self.assertEqual(list(outside.iterdir()), [])

    def test_optional_cache_metadata_failure_keeps_both_original_material_builds(self):
        config = self.root / 'suites.json'
        config.write_text('{}')
        state_path = self.base / 'state.json'
        with patch.object(artifacts, 'ROOT', self.root), patch.object(artifacts, 'CONFIG', config), \
             patch.object(artifacts, 'build_variants', return_value=[]), \
             patch.object(artifacts, 'output', return_value='host: fixture'), \
             patch.object(artifacts, 'preview_provider_required', return_value=True), \
             patch.object(artifacts.BuiltinTestArtifact, 'required', return_value=True), \
             patch.object(artifacts, 'cache_register_canonical_targets', side_effect=ValueError('unavailable')), \
             patch.object(artifacts.provider_artifact, 'build', return_value={}) as provider, \
             patch.object(artifacts.BuiltinTestArtifact, 'build', return_value={}) as builtin:
            bundle = self.base / 'bundle'
            artifacts.build(bundle, 'ordinary', state_path)
        provider.assert_called_once()
        builtin.assert_called_once()
        self.assertEqual(json.loads((bundle / 'manifest.json').read_text())['builtin_test_resources'], {})
        state = json.loads(state_path.read_text())
        self.assertFalse(state['complete'])
        self.assertEqual(state['error'], 'unavailable')

    def test_real_build_registers_only_selected_material_roles_before_their_builds(self):
        catalog = {self.manifest, *(f'crates/oasis7_builtin_wasm_modules/{crate}/Cargo.toml'
                                   for crate in artifacts.BuiltinTestArtifact.MODULES.values())}
        for scope in ('ordinary', 'formal'):
            with self.subTest(scope=scope):
                for manifest in catalog:
                    path = self.root / manifest
                    path.parent.mkdir(parents=True, exist_ok=True)
                    if not path.exists():
                        path.write_text('[workspace]\n')
                    if (path.parent / 'target').exists():
                        shutil.rmtree(path.parent / 'target')
                config = self.root / 'suites.json'
                config.write_text(json.dumps(dict(suites={}, legacy_ignored_filters={},
                    environment_formal_batches={}, shell_tests={})))
                events = []
                def metadata(command, **keywords):
                    path = Path(command[-1])
                    self.assertEqual(command[1:6], ['metadata', '--locked', '--offline', '--no-deps', '--format-version=1'])
                    self.assertEqual(keywords['timeout'], 30)
                    events.append('metadata')
                    return subprocess.CompletedProcess(command, 0, stdout=json.dumps(dict(
                        workspace_root=str(path.parent), target_directory=str(path.parent / 'target'))))
                def material(kind):
                    def build(*args):
                        events.append(kind)
                        manifests = {self.manifest} if kind == 'provider' else catalog - {self.manifest}
                        for manifest in manifests:
                            output = self.root / Path(manifest).parent / 'target/module.wasm'
                            output.parent.mkdir(parents=True)
                            output.write_bytes(b'original material compiler output')
                        return {}
                    return build
                with patch.object(artifacts, 'ROOT', self.root), patch.object(artifacts, 'CONFIG', config), \
                     patch.object(artifacts, 'build_variants', return_value=[]), \
                     patch.object(artifacts, 'output', return_value='host: fixture'), \
                     patch.object(artifacts, 'preview_provider_required', return_value=True), \
                     patch.object(artifacts.BuiltinTestArtifact, 'required', return_value=True), \
                     patch.object(artifacts.provider_artifact, 'build', side_effect=material('provider')), \
                     patch.object(artifacts.BuiltinTestArtifact, 'build', side_effect=material('builtin')), \
                     patch.object(artifacts.subprocess, 'run', side_effect=metadata):
                    state_path = self.base / (scope + '.json')
                    artifacts.build(self.base / scope, scope, state_path)
                state = json.loads(state_path.read_text())
                self.assertTrue(state['complete'], state.get('error'))
                selected = catalog - ({self.manifest} if scope == 'formal' else set())
                self.assertEqual(set(state['generated_cargo_targets']), selected)
                self.assertEqual(events, ['metadata'] * len(selected) +
                                 (['provider', 'builtin'] if scope == 'ordinary' else ['builtin']))


class OutputBackpressure(unittest.TestCase):
    def test_queued_packets_keep_original_descriptor_after_caller_closes_it(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            program = ('import contextlib,os,stat,sys,time\nfrom pathlib import Path\n'
                f'sys.path.insert(0,{str(HERE)!r})\nfrom ci_process_runner import run,live_output\n'
                'read_fd,write_fd=os.pipe()\n'
                'with os.fdopen(write_fd,"w") as destination, contextlib.redirect_stdout(destination):\n'
                ' run([sys.executable,"-c",\'import os; [os.write(1,b"x"*8191+b"\\\\n") for _ in range(512)]; '
                'print("test result: ok. 1 passed; 0 failed; 0 ignored;")\'],'
                f'Path({str(root)!r}),os.environ,"probe","lifetime",Path({str(root / "results")!r}),'
                'expected_count=1,timeout_seconds=3)\n'
                f'with open({str(root / "sentinel")!r},"w") as sentinel:\n'
                ' sentinel.write("untouched")\n'
                ' relay=live_output()\n'
                ' with relay.queue.mutex:\n'
                '  assert relay.queue.queue\n'
                '  assert all(stat.S_ISFIFO(os.fstat(packet[3]).st_mode) for packet in relay.queue.queue)\n'
                ' os.close(read_fd)\n'
                ' deadline=time.monotonic()+2\n'
                ' while relay.queue.unfinished_tasks and time.monotonic()<deadline: time.sleep(0.01)\n'
                ' assert not relay.queue.unfinished_tasks\n')
            process = subprocess.run([sys.executable, '-c', program], capture_output=True, timeout=6)
            self.assertEqual(process.returncode, 0, process.stderr)
            self.assertEqual((root / 'sentinel').read_text(), 'untouched')
            self.assertTrue(json.loads((root / 'results').read_text())['passed'])

    def test_explicit_log_write_failure_cannot_pass(self):
        import contextlib
        import ci_process_runner as runner
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with (root / 'log').open('w') as log, (root / 'console').open('w') as console:
                identity = os.fstat(log.fileno())
                original = runner.write_fd
                def fail_log(fd, data):
                    current = os.fstat(fd)
                    if (current.st_dev, current.st_ino) == (identity.st_dev, identity.st_ino):
                        raise OSError('injected persistent log failure')
                    return original(fd, data)
                with contextlib.redirect_stdout(console), patch.object(runner, 'write_fd', side_effect=fail_log):
                    result = runner.run([sys.executable, '-c',
                        'print("test result: ok. 1 passed; 0 failed; 0 ignored;")'],
                        root, os.environ, 'probe', 'log-failure', root / 'results',
                        expected_count=1, timeout_seconds=3, log_stream=log)
            self.assertFalse(result['passed'])
            self.assertFalse(result['log_complete'])
            self.assertFalse(result['count_ok'])
            self.assertEqual(result['output_read_error'], 'OSError')
            self.assertEqual(result['output_bytes'], 48)
            self.assertEqual(result['cleanup_result'], 'clean')

    def test_each_run_uses_current_redirected_file(self):
        import contextlib
        from ci_process_runner import run
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for index in range(3):
                destination = root / f'output-{index}'
                with destination.open('w') as stream, contextlib.redirect_stdout(stream):
                    result = run([sys.executable, '-c',
                        f'print("destination-{index}"); print("test result: ok. 1 passed; 0 failed; 0 ignored;")'],
                        root, os.environ, 'probe', str(index), root / 'results',
                        expected_count=1, timeout_seconds=3)
                self.assertTrue(result['passed'])
                content = destination.read_text()
                self.assertIn(f'destination-{index}', content)
                self.assertIn('test result: ok.', content)
                for other in range(index):
                    self.assertNotIn(f'destination-{other}', content)

    def test_explicit_log_is_complete_with_undrained_console(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            log, results = root / 'durable.log', root / 'results'
            child = ('import os; [os.write(1,b"x"*8191+b"\\n") for _ in range(512)]; '
                     'print(\'STORAGE_CAPACITY_REPORT={"status":"passed","executed_heights":10}\'); '
                     'print(\'STORAGE_FULL_NODE_REPORT={"status":"passed"}\'); '
                     'print("test result: ok. 1 passed; 0 failed; 0 ignored;")')
            program = ('import os,sys\nfrom pathlib import Path\n'
                f'sys.path.insert(0,{str(HERE)!r})\nfrom ci_process_runner import run\n'
                f'with open({str(log)!r},"w") as stream:\n'
                f' run([sys.executable,"-c",{child!r}],Path({str(root)!r}),os.environ,'
                f'"probe","durable",Path({str(results)!r}),expected_count=1,timeout_seconds=3,log_stream=stream)\n')
            process = subprocess.Popen([sys.executable, '-c', program], stdout=subprocess.PIPE,
                                       stderr=subprocess.STDOUT)
            try:
                process.wait(timeout=5)
                timely = True
            except subprocess.TimeoutExpired:
                timely = False
            finally:
                output, _ = process.communicate(timeout=5)
            self.assertTrue(timely)
            self.assertEqual(process.returncode, 0, output[-4000:])
            result = json.loads(results.read_text())
            self.assertTrue(result['passed'] and result['log_complete'] and result['count_ok'])
            self.assertGreater(result['output_suppressed_bytes'], 0)
            content = log.read_bytes()
            self.assertEqual(len(content), result['output_bytes'])
            self.assertIn(b'STORAGE_CAPACITY_REPORT={"status":"passed","executed_heights":10}', content)
            self.assertIn(b'STORAGE_FULL_NODE_REPORT={"status":"passed"}', content)
            self.assertTrue(content.endswith(b'test result: ok. 1 passed; 0 failed; 0 ignored;\n'))

    def test_drained_live_output_preserves_heartbeat_and_summary(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            results = root / 'results.jsonl'
            program = (
                'import os,sys\nfrom pathlib import Path\n'
                f'sys.path.insert(0,{str(HERE)!r})\nfrom ci_process_runner import run\n'
                'run([sys.executable,"-c",\'import time; time.sleep(0.15); '
                'print("test result: ok. 1 passed; 0 failed; 0 ignored;")\'],'
                f'Path({str(root)!r}),os.environ,"probe","healthy",Path({str(results)!r}),'
                'expected_count=1,timeout_seconds=2,heartbeat_seconds=0.05)\n'
            )
            process = subprocess.run([sys.executable, '-c', program], capture_output=True, timeout=4)
            self.assertEqual(process.returncode, 0, process.stderr)
            self.assertIn(b'suite=probe test=healthy', process.stdout)
            self.assertIn(b'test result: ok. 1 passed;', process.stdout)
            record = json.loads(results.read_text())
            self.assertTrue(record['passed'])
            self.assertEqual(record['output_suppressed_bytes'], 0)
            self.assertEqual(record['output_pending_bytes'], 0)

    def test_summary_parser_handles_chunk_boundaries_without_retaining_long_lines(self):
        capture = _Capture()
        for piece in [b'test result: o', b'k. 4 passed; 0 fail', b'ed; 2 ignored;\n']:
            capture.feed(piece)
        self.assertEqual(capture.summary, (4, 0, 2))
        for _ in range(1024):
            capture.feed(b'x' * READ_BYTES)
            self.assertLessEqual(len(capture.line), SUMMARY_LINE_BYTES)
        capture.feed(b'\ntest result: ok. 5 passed; 0 failed; 0 ignored;\n')
        self.assertEqual(capture.summary, (5, 0, 0))
        capture.feed(b'test result: ok. ' + b'9' * READ_BYTES)
        self.assertIsNone(capture.summary, 'an oversized later summary cannot reuse an old count')

    def test_repeated_runs_share_one_bounded_relay_when_stdout_is_not_drained(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            results, observations = root / 'results.jsonl', root / 'observations.json'
            child = ('import os; print("test result: ok. 1 passed; 0 failed; 0 ignored;",flush=True); '
                     '[os.write(1,b"x"*8191+b"\\n") for _ in range(512)]')
            program = (
                'import json,os,sys,threading\nfrom pathlib import Path\n'
                f'sys.path.insert(0,{str(HERE)!r})\n'
                'from ci_process_runner import run,live_output,READ_BYTES,LIVE_CHUNKS\n'
                'before=threading.active_count()\n'
                'for i in range(3):\n'
                f' run([sys.executable,"-c",{child!r}],Path({str(root)!r}),os.environ,'
                f'"probe",str(i),Path({str(results)!r}),expected_count=1,timeout_seconds=2,heartbeat_seconds=0.05)\n'
                'relay=live_output()\n'
                f'Path({str(observations)!r}).write_text(json.dumps(dict('
                'threads=threading.active_count()-before,queued=relay.queue.qsize(),'
                'max_chunks=LIVE_CHUNKS,max_packet=max(len(p[0]) for p in relay.queue.queue),'
                'chunk_bytes=READ_BYTES)))\n'
            )
            process = subprocess.Popen([sys.executable, '-c', program], stdout=subprocess.PIPE,
                                       stderr=subprocess.STDOUT)
            try:
                process.wait(timeout=6)
                timely = True
            except subprocess.TimeoutExpired:
                timely = False
            finally:
                output, _ = process.communicate(timeout=10)
            self.assertTrue(timely, 'repeated runs must not wait for blocked relay threads')
            self.assertEqual(process.returncode, 0, output[-4000:])
            records = [json.loads(line) for line in results.read_text().splitlines()]
            self.assertEqual(len(records), 3)
            self.assertTrue(all(record['passed'] and record['count_ok'] for record in records))
            self.assertTrue(all(record['output_suppressed_bytes'] > 0 for record in records))
            self.assertTrue(all(record['output_bytes'] == 512 * 8192 + 48 for record in records))
            self.assertTrue(all(record['output_suppressed_bytes'] + record['output_pending_bytes']
                                <= record['output_bytes'] for record in records))
            stats = json.loads(observations.read_text())
            self.assertEqual(stats['threads'], 1, 'only one relay survives across all runs')
            self.assertLessEqual(stats['queued'], stats['max_chunks'])
            self.assertLessEqual(stats['max_packet'], stats['chunk_bytes'])

    def test_undrained_stdout_cannot_block_deadline_result_or_owned_cleanup(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            results, pids = root / 'results.jsonl', root / 'pids.json'
            child = (
                'import json,os,subprocess,sys,time; '
                'other=subprocess.Popen([sys.executable,"-c","import time; time.sleep(30)"]); '
                f'open({str(pids)!r},"w").write(json.dumps([os.getpid(),other.pid])); '
                'print("test result: ok. 1 passed; 0 failed; 0 ignored;",flush=True); '
                '[os.write(1,b"x"*8191+b"\\n") for _ in range(1024)]; time.sleep(30)'
            )
            program = (
                'import os,sys\nfrom pathlib import Path\n'
                f'sys.path.insert(0,{str(HERE)!r})\n'
                'from ci_process_runner import run\n'
                f'run([sys.executable,"-c",{child!r}],Path({str(root)!r}),os.environ,'
                f'"probe","blocked-output",Path({str(results)!r}),expected_count=1,'
                'timeout_seconds=0.25,heartbeat_seconds=0.05)\n'
            )
            process = subprocess.Popen([sys.executable, '-c', program], stdout=subprocess.PIPE,
                                       stderr=subprocess.STDOUT)
            try:
                # Deliberately never drain the live output until the runner has
                # exited. Its deadline/result/cleanup must not need our reader.
                process.wait(timeout=4)
                timely = True
            except subprocess.TimeoutExpired:
                timely = False
            finally:
                # Also release an unfixed runner, so the red regression leaves
                # no sleeping child or blocked pipe behind.
                output, _ = process.communicate(timeout=10)
            self.assertTrue(timely, 'live stdout prevented the deadline and runner exit')
            self.assertEqual(process.returncode, 0, output[-4000:])
            record = json.loads(results.read_text().splitlines()[-1])
            self.assertTrue(record['timeout'])
            self.assertFalse(record['passed'])
            self.assertTrue(record['count_ok'], 'summary parsing must continue independently')
            self.assertEqual(record['cleanup_result'], 'clean')
            self.assertLess(record['elapsed_seconds'], 3)
            self.assertGreater(record['output_suppressed_bytes'], 0)
            for pid in json.loads(pids.read_text()):
                state = subprocess.run(['ps', '-p', str(pid), '-o', 'stat='],
                                       capture_output=True, text=True).stdout.strip()
                self.assertTrue(not state or state.startswith('Z'), (pid, state))

    def test_child_exit_zero_observed_after_deadline_cannot_pass(self):
        original_popen = subprocess.Popen
        def delayed_observation(*args, **kwargs):
            child = original_popen(*args, **kwargs)
            # A real child completes successfully after the deadline while the
            # supervisor cannot observe it yet (as with a stalled output call).
            child.wait(timeout=2)
            return child
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with patch('ci_process_runner.subprocess.Popen', side_effect=delayed_observation):
                result = run([sys.executable, '-c',
                              'import time; time.sleep(0.15); print("test result: ok. 1 passed; 0 failed; 0 ignored;")'],
                             root, os.environ, 'probe', 'late-success', root / 'results.jsonl',
                             expected_count=1, timeout_seconds=0.05, heartbeat_seconds=0.01)
            self.assertEqual(result['exit_code'], 0)
            self.assertTrue(result['count_ok'])
            self.assertTrue(result['timeout'])
            self.assertFalse(result['passed'])


if __name__ == '__main__':
    unittest.main()
