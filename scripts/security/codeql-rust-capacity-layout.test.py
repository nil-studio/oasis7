#!/usr/bin/env python3
"""Real-checkout regressions for the capacity job's collection boundary."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import unittest


HERE = Path(__file__).resolve().parent
LAYOUT = 'codeql_rust_capacity_layout.py'
FILES = (
    'codeql-rust-capacity.py',
    'codeql-rust-cargo-check.py',
    'codeql-plan.py',
    'codeql-rust-refactored-source.py',
    'codeql-rust-model-diagnostic.py',
    'codeql_rust_diagnostic_runtime.py',
    LAYOUT,
    'codeql_rust_capacity_evidence.py',
    'codeql_rust_cargo_guard.py',
)


class CapacityLayout(unittest.TestCase):
    def setUp(self):
        self.assertTrue((HERE / LAYOUT).is_file(), 'isolation helper is not implemented')
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source = self.root / 'workspace'
        self.source.mkdir()
        self.runner = self.root / 'runner'
        self.runner.mkdir()
        self.tools = self.runner / 'helper'
        self.out = self.runner / 'evidence'
        self.env = dict(os.environ, GITHUB_WORKSPACE=str(self.source),
                        RUNNER_TEMP=str(self.runner), PYTHONDONTWRITEBYTECODE='1')
        self.git('init', '--quiet')
        self.git('config', 'user.name', 'Capacity test')
        self.git('config', 'user.email', 'capacity@example.invalid')
        directory = self.source / 'scripts/security'
        directory.mkdir(parents=True)
        for name in FILES:
            shutil.copyfile(HERE / name, directory / name)
        self.git('add', '.')
        self.git('commit', '--quiet', '-m', 'Tool snapshot')
        self.tool_sha = self.git('rev-parse', 'HEAD').strip()
        self.env['GITHUB_SHA'] = self.tool_sha
        self.git('rm', '--quiet', '-r', 'scripts')
        (self.source / 'business.rs').write_text('fn main() {}\n')
        (self.source / '.gitignore').write_text('internal-tools/\n')
        self.git('add', '.')
        self.git('commit', '--quiet', '-m', 'Frozen business source')
        self.source_sha = self.git('rev-parse', 'HEAD').strip()
        self.git('checkout', '--quiet', self.tool_sha)

    def git(self, *args, cwd=None):
        return subprocess.check_output(['git', *args], cwd=cwd or self.source,
                                       env=self.env, text=True)

    def invoke(self, phase, *, source=None, tools=None, cwd=None, script=None, sha=None,
               timeout=30):
        source = source or self.source
        tools = tools or self.tools
        script = script or (self.source / 'scripts/security' / LAYOUT
                            if phase == 'stage' else tools / LAYOUT)
        sha = sha or (self.tool_sha if phase == 'stage' else self.source_sha)
        return subprocess.run(
            [sys.executable, str(script), phase, '--source', str(source),
             '--tools', str(tools), '--output', str(self.out),
             '--deadline', str(time.time() + 300),
             '--tool-sha' if phase == 'stage' else '--source-sha', sha],
            cwd=cwd or self.source, env=self.env, capture_output=True, text=True,
            timeout=timeout,
        )

    def stage_and_switch(self):
        result = self.invoke('stage')
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.git('checkout', '--quiet', self.source_sha)

    def receipt(self):
        return json.loads((self.out / 'collection-layout.json').read_text())

    def test_external_snapshot_survives_same_root_business_checkout(self):
        self.stage_and_switch()
        self.assertFalse((self.source / 'scripts/security' / LAYOUT).exists())
        result = self.invoke('check')
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        value = self.receipt()
        self.assertEqual(value['status'], 'PASS')
        self.assertEqual(value['identity']['actual_head'], self.source_sha)
        self.assertEqual(value['identity']['clean_gate'], 'PASS')
        self.assertEqual(value['paths']['cwd'], str(self.source))
        self.assertEqual(value['paths']['executing_helper'], str(self.tools / LAYOUT))
        self.assertEqual(value['expected_action_trace_cwd']['path'], str(self.source))
        self.assertFalse(value['expected_action_trace_cwd']['observed'])
        snapshot = json.loads((self.out / 'tool-snapshot.json').read_text())
        self.assertEqual(snapshot['tool_sha'], self.tool_sha)
        self.assertEqual(set(snapshot['files']), set(FILES))
        for name, item in snapshot['files'].items():
            actual = hashlib.sha256((self.tools / name).read_bytes()).hexdigest()
            self.assertEqual(item['source_sha256'], actual)
            self.assertEqual(item['snapshot_sha256'], actual)

    def test_old_nested_layout_is_rejected_even_with_correct_action_repository_root(self):
        result = self.invoke('stage')
        self.assertEqual(result.returncode, 0, result.stderr)
        nested = self.source / 'capacity-source'
        nested.mkdir()
        self.git('init', '--quiet', cwd=nested)
        self.git('config', 'user.name', 'Capacity test', cwd=nested)
        self.git('config', 'user.email', 'capacity@example.invalid', cwd=nested)
        (nested / 'business.rs').write_text('fn main() {}\n')
        self.git('add', '.', cwd=nested)
        self.git('commit', '--quiet', '-m', 'Nested business', cwd=nested)
        sha = self.git('rev-parse', 'HEAD', cwd=nested).strip()
        config = {'repositoryRoot': str(nested), 'languages': ['rust'], 'buildMode': 'none'}
        (self.out / 'action-config.json').write_text(json.dumps(config))
        result = self.invoke('check', source=nested, sha=sha)
        self.assertNotEqual(result.returncode, 0)
        value = self.receipt()
        self.assertEqual(value['identity']['status'], 'PASS')
        self.assertEqual(value['status'], 'FAIL')
        self.assertIn('CWD_SOURCE_MISMATCH', value['errors'])
        self.assertEqual(config['repositoryRoot'], str(nested))

    def test_tools_inside_source_are_rejected(self):
        self.stage_and_switch()
        inside = self.source / 'internal-tools'
        shutil.copytree(self.tools, inside)
        result = self.invoke('check', tools=inside)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('TOOLS_INSIDE_SOURCE', self.receipt()['errors'])

    def test_staging_inside_source_is_rejected_before_copy(self):
        inside = self.source / 'internal-tools'
        result = self.invoke('stage', tools=inside)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(inside.exists())
        value = json.loads((self.out / 'tool-snapshot.json').read_text())
        self.assertIn('TOOLS_INSIDE_SOURCE', value['errors'])

    def test_modified_external_tool_is_rejected_and_receipt_survives(self):
        self.stage_and_switch()
        changed = self.tools / 'codeql-plan.py'
        changed.chmod(0o644)
        changed.write_bytes(changed.read_bytes() + b'\n# changed\n')
        result = self.invoke('check')
        self.assertNotEqual(result.returncode, 0)
        value = self.receipt()
        self.assertEqual(value['status'], 'FAIL')
        self.assertIn('TOOL_HASH_MISMATCH:codeql-plan.py', value['errors'])
        self.assertEqual(value['paths']['source'], str(self.source))

    def test_bad_source_identity_still_preserves_layout_and_tool_observations(self):
        self.stage_and_switch()
        result = self.invoke('check', sha=self.tool_sha)
        self.assertNotEqual(result.returncode, 0)
        value = self.receipt()
        self.assertEqual(value['identity']['actual_head'], self.source_sha)
        self.assertEqual(value['identity']['identity_gate'], 'FAIL')
        self.assertEqual(value['tool_snapshot_gate'], 'PASS')
        self.assertEqual(value['paths']['tools'], str(self.tools))

    def test_stage_rejects_working_tree_tool_not_matching_frozen_git_blob(self):
        changed = self.source / 'scripts/security/codeql-plan.py'
        changed.write_bytes(changed.read_bytes() + b'\n# unstaged tool change\n')
        result = self.invoke('stage')
        self.assertNotEqual(result.returncode, 0)
        snapshot = json.loads((self.out / 'tool-snapshot.json').read_text())
        self.assertEqual(snapshot['status'], 'FAIL')
        self.assertIn('TOOL_GIT_BLOB_MISMATCH:codeql-plan.py', snapshot['errors'])

    def test_stage_rejects_fifo_tool_without_blocking_and_preserves_receipt(self):
        changed = self.source / 'scripts/security/codeql-plan.py'
        changed.unlink()
        os.mkfifo(changed)
        result = self.invoke('stage', timeout=2)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(self.tools.exists())
        snapshot = json.loads((self.out / 'tool-snapshot.json').read_text())
        self.assertEqual(snapshot['status'], 'FAIL')
        self.assertIn('UNSAFE_TOOL_FILE:' + str(changed), snapshot['errors'])


if __name__ == '__main__':
    unittest.main()
