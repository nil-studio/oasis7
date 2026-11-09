#!/usr/bin/env python3
"""Collector tests use fake Cargo streams; they do not claim Rust cache reuse."""
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('diagnostic', Path(__file__).with_name('ci-cache-diagnostic.py'))
diagnostic = importlib.util.module_from_spec(spec)
spec.loader.exec_module(diagnostic)


def primary_env():
    return 'name: Rust\nenv:\n' + ''.join('  ' + key + ': "1"\n' for key in sorted(diagnostic.ENV_KEYS)) + '\ncache-mode: read\n'


def artifact(path, fresh):
    return json.dumps(dict(reason='compiler-artifact', manifest_path=path, fresh=fresh)).encode() + b'\n'


class CollectorTests(unittest.TestCase):
    def test_literal_primary_environment(self):
        self.assertEqual(set(diagnostic.workflow_environment(primary_env())), diagnostic.ENV_KEYS)

    def test_environment_drift_fails_closed(self):
        for text in (primary_env().replace('CARGO_INCREMENTAL', 'UNEXPECTED'),
                     primary_env().replace('"1"', '${{ secrets.TOKEN }}', 1),
                     primary_env().replace('"1"', 'false # inline', 1),
                     primary_env() + 'env:\n  X: 1\n', 'name: Rust\n'):
            with self.subTest(text=text), self.assertRaises(ValueError):
                diagnostic.workflow_environment(text)

    def test_duplicate_environment_key(self):
        with self.assertRaises(ValueError):
            diagnostic.workflow_environment(primary_env().replace('env:\n', 'env:\n  CARGO_INCREMENTAL: 0\n'))

    def test_json_counts_compile_units_not_unique_crates(self):
        observer = diagnostic.Observation(Path('/repo'))
        observer.line('stdout', artifact('/repo/crates/a/Cargo.toml', False))
        observer.line('stdout', artifact('/cargo/registry/a/Cargo.toml', True))
        observer.line('stdout', artifact('/cargo/registry/a/Cargo.toml', False))
        observer.line('stdout', artifact('/repo-sibling/a/Cargo.toml', False))
        self.assertEqual(observer.units, dict(local_rebuilt=1, dependency_fresh=1, dependency_rebuilt=2))
        self.assertTrue(observer.report()['unit_counts_complete'])

    def test_bad_artifact_is_not_a_cache_hit(self):
        observer = diagnostic.Observation(Path('/repo'))
        observer.line('stdout', artifact('/repo/a/Cargo.toml', 'false'))
        observer.line('stdout', artifact('relative/Cargo.toml', True))
        observer.line('stdout', b'not-json\n')
        self.assertFalse(observer.report()['unit_counts_complete'])
        self.assertEqual(observer.discarded['invalid_stdout_records'], 3)

    def test_empty_output_is_not_reuse_evidence(self):
        self.assertFalse(diagnostic.Observation(Path('/repo')).report()['unit_counts_complete'])

    def test_only_allowlisted_fingerprint_fields_are_retained(self):
        observer = diagnostic.Observation(Path('/repo'))
        secret = 'SUPER_PRIVATE_CANARY'
        lines = [f'INFO cargo::core::compiler::fingerprint: fingerprint error for serde v1.0.229/Build/TargetInner {{ {secret} }}',
                 f'INFO cargo::core::compiler::fingerprint: dirty: EnvVarChanged {{ previous: "{secret}", value: "{secret}" }}',
                 f'INFO cargo::core::compiler::fingerprint: dirty: NewUnknownReason {{ value: "{secret}" }}',
                 f'INFO cargo::core::compiler::fingerprint:     err: failed to read /private/{secret}',
                 f'build-script secret={secret}']
        for line in lines:
            observer.line('stderr', line.encode())
        data = observer.report()
        self.assertNotIn(secret, json.dumps(data))
        self.assertEqual(data['samples'][0], dict(reason='fingerprint_error', package='serde', version='1.0.229'))
        self.assertEqual(data['reason_line_observations']['EnvVarChanged'], 1)
        self.assertEqual(data['reason_line_observations']['unclassified_dirty'], 1)
        self.assertEqual(data['reason_line_observations']['fingerprint_read_error'], 1)

    def test_samples_are_bounded(self):
        observer = diagnostic.Observation(Path('/repo'))
        for _ in range(1000):
            observer.line('stderr', b'cargo::core::compiler::fingerprint: dirty: FeaturesChanged')
        report = observer.report()
        self.assertEqual(len(report['samples']), diagnostic.SAMPLE_LIMIT)
        self.assertTrue(report['samples_truncated'])
        self.assertEqual(report['reason_line_observations']['FeaturesChanged'], 1000)

    def call_fake(self, source, seconds=3, **limits):
        with tempfile.TemporaryDirectory() as root:
            return diagnostic.observe([sys.executable, '-c', source], Path(root), dict(os.environ),
                                      time.monotonic() + seconds, **limits)

    def test_nonzero_exit_is_preserved(self):
        report = self.call_fake('raise SystemExit(7)')
        self.assertEqual(report['exit_code'], 7)
        self.assertFalse(report['timed_out'])

    def test_streaming_json_and_final_line_without_newline(self):
        record = artifact('/cargo/a/Cargo.toml', True).decode().strip()
        report = self.call_fake('import sys; sys.stdout.write(' + repr(record) + ')')
        self.assertEqual(report['unit_observations'], dict(dependency_fresh=1))
        self.assertTrue(report['unit_counts_complete'])

    def test_two_full_pipes_do_not_deadlock(self):
        report = self.call_fake("import os\nfor _ in range(100):\n os.write(1,b'not-json\\n'*1000)\n os.write(2,b'private-token\\n'*1000)\n", stream_limit=10000)
        self.assertEqual(report['exit_code'], 0)
        self.assertIn('stdout_bytes_over_limit', report['discarded'])
        self.assertIn('stderr_bytes_over_limit', report['discarded'])
        self.assertNotIn('private-token', json.dumps(report))

    def test_oversized_line_does_not_erase_following_record(self):
        record = artifact('/cargo/a/Cargo.toml', False).decode()
        report = self.call_fake('import sys; sys.stdout.write("x"*10000+"\\n"+' + repr(record) + ')', line_limit=1024)
        self.assertEqual(report['unit_observations'], dict(dependency_rebuilt=1))
        self.assertEqual(report['discarded']['stdout_oversized_lines'], 1)
        self.assertFalse(report['unit_counts_complete'])

    def test_byte_limit_does_not_parse_partial_record(self):
        record = artifact('/cargo/a/Cargo.toml', False).decode()
        report = self.call_fake('import sys; sys.stdout.write(' + repr(record) + ')', stream_limit=20)
        self.assertEqual(report['unit_observations'], {})
        self.assertFalse(report['unit_counts_complete'])

    def test_timeout_is_not_success(self):
        report = self.call_fake('import time; time.sleep(20)', seconds=0.2)
        self.assertEqual(report['exit_code'], 124)
        self.assertTrue(report['timed_out'])
        self.assertLess(report['elapsed_seconds'], 3)

    def test_closed_pipes_do_not_bypass_deadline(self):
        report = self.call_fake('import os,time; os.close(1); os.close(2); time.sleep(20)', seconds=0.2)
        self.assertEqual(report['exit_code'], 124)

    def test_child_holding_pipe_is_bounded_after_parent_exit(self):
        source = 'import subprocess,sys; subprocess.Popen([sys.executable,"-c","import time; time.sleep(20)"])'
        report = self.call_fake(source, seconds=0.2)
        self.assertEqual(report['exit_code'], 124)

    def test_workspace_cache_path_and_read_only_contract(self):
        path = Path(__file__).resolve().parent.parent / '.github/workflows/native-cache-diagnostic.yml'
        text = path.read_text()
        # actions/cache versions include the archive path: a renamed diagnostic
        # directory would miss the production cache despite an identical key.
        self.assertIn('${{ runner.temp }}/${{ inputs.scope }}-workspace-cache/workspace-units.tar.zst', text)
        self.assertIn('--cache "$RUNNER_TEMP/${SCOPE}-workspace-cache"', text)
        self.assertIn('cache-mode: read', text)
        self.assertIn('save-if: false', text)
        self.assertNotIn('actions/cache/save@', text)
        self.assertNotIn('schedule:', text)
        self.assertIn("github.event_name == 'workflow_dispatch' && github.ref == 'refs/heads/main'", text)

    def test_real_workflow_env_when_checkout_available(self):
        path = Path(__file__).resolve().parent.parent / '.github/workflows/rust.yml'
        if not path.exists():
            self.skipTest('primary workflow not in isolated collector fixture')
        self.assertEqual(set(diagnostic.workflow_environment(path.read_text())), diagnostic.ENV_KEYS)


class DiagnosticPresenceTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name);self.target=self.root/'target';self.target.mkdir()

    def value(self,filenames,**changes):
        obj=dict(reason='compiler-artifact',manifest_path=str(self.root/'Cargo.toml'),fresh=False,
                 target={'name':'example','kind':['lib']},features=['test_tier_required'],
                 profile={'opt_level':'0','test':False},filenames=[str(self.target/p) for p in filenames])
        obj.update(changes);return obj

    def test_absent_present_and_partial_are_distinct(self):
        (self.target/'a.rlib').touch()
        snapshot=diagnostic.snapshot_outputs(self.root)
        for names,expected in [(['a.rlib'],'present'),(['b.rlib'],'absent'),(['a.rlib','b.rlib'],'partial')]:
            result=diagnostic.output_presence([str(self.target/p) for p in names],snapshot)
            self.assertEqual(result['prebuild_outputs'],expected)

    def test_prebuild_snapshot_not_after_compilation(self):
        snapshot=diagnostic.snapshot_outputs(self.root);(self.target/'a.rlib').touch()
        self.assertEqual(diagnostic.output_presence([str(self.target/'a.rlib')],snapshot)['prebuild_outputs'],'absent')

    def test_incomplete_or_external_paths_are_unknown(self):
        snapshot=diagnostic.snapshot_outputs(self.root)
        for files in ([],[str(self.root/'outside')],['relative'],[str(self.target/'../outside')],[None]):
            self.assertEqual(diagnostic.output_presence(files,snapshot)['prebuild_outputs'],'unknown')
        self.assertEqual(diagnostic.output_presence([str(self.target/'a')],(self.target,set(),False))['prebuild_outputs'],'unknown')

    def test_inventory_limit_never_claims_absence(self):
        for n in range(2):(self.target/str(n)).touch()
        self.assertFalse(diagnostic.snapshot_outputs(self.root,limit=1)[2])

    def test_symlink_inventory_is_not_followed(self):
        outside=self.root/'private';outside.mkdir();(outside/'secret').touch()
        (self.target/'link').symlink_to(outside,target_is_directory=True)
        target,files,complete=diagnostic.snapshot_outputs(self.root)
        self.assertFalse(complete);self.assertNotIn('link/secret',files)

    def test_rebuilt_unit_observation_preserves_features_without_paths(self):
        observer=diagnostic.Observation(self.root,diagnostic.snapshot_outputs(self.root))
        observer.line('stdout',json.dumps(self.value(['a.rlib'])).encode())
        report=observer.report();self.assertEqual(report['rebuilt_output_observations'],{'local_absent':1})
        sample=report['rebuilt_unit_samples'][0]
        self.assertEqual(sample['features'],['test_tier_required']);self.assertEqual(sample['target'],'example')
        self.assertNotIn(str(self.root),json.dumps(report));self.assertNotIn('a.rlib',json.dumps(report))

    def test_existing_outputs_do_not_mean_fresh(self):
        (self.target/'a.rlib').touch();observer=diagnostic.Observation(self.root,diagnostic.snapshot_outputs(self.root))
        observer.line('stdout',json.dumps(self.value(['a.rlib'])).encode())
        report=observer.report();self.assertEqual(report['unit_observations'],{'local_rebuilt':1})
        self.assertEqual(report['rebuilt_output_observations'],{'local_present':1})

    def test_unsafe_unit_metadata_is_not_emitted(self):
        value=self.value(['a'],target={'name':'/private/token','kind':['lib']},features=['/private/key'])
        self.assertEqual(diagnostic.safe_unit_identity(value),{})

    def test_samples_bounded_and_counts_remain_complete(self):
        observer=diagnostic.Observation(self.root,diagnostic.snapshot_outputs(self.root))
        for _ in range(diagnostic.SAMPLE_LIMIT+1):observer.line('stdout',json.dumps(self.value(['a'])).encode())
        report=observer.report();self.assertEqual(len(report['rebuilt_unit_samples']),diagnostic.SAMPLE_LIMIT // 2)
        self.assertTrue(report['rebuilt_unit_samples_truncated']);self.assertTrue(report['unit_counts_complete'])
        self.assertEqual(report['rebuilt_output_observations']['local_absent'],diagnostic.SAMPLE_LIMIT+1)

    def test_old_artifact_records_keep_counts_but_presence_is_unknown(self):
        observer=diagnostic.Observation(self.root)
        observer.line('stdout',json.dumps(dict(reason='compiler-artifact',manifest_path=str(self.root/'Cargo.toml'),fresh=False)).encode())
        self.assertEqual(observer.report()['unit_observations'],{'local_rebuilt':1})
        self.assertEqual(observer.report()['rebuilt_output_observations'],{'local_unknown':1})



if __name__ == '__main__':
    unittest.main()
