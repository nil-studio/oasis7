#!/usr/bin/env python3
"""Exercise diagnostic Git identities, fresh-runner restoration, and coverage guards."""
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location('preview', ROOT / 'scripts/ci-fast-preview.py')
PREVIEW = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PREVIEW)


def command(repo, *args):
    return subprocess.check_output(['git', '-C', str(repo), *args], text=True, stderr=subprocess.PIPE).strip()


def fixture(directory):
    repo = Path(directory) / 'repo'
    repo.mkdir()
    command(repo, 'init', '-q', '-b', 'main')
    command(repo, 'config', 'user.name', 'CI fixture')
    command(repo, 'config', 'user.email', 'fixture@example.invalid')
    for path in PREVIEW.SAMPLE_PATHS:
        destination = repo / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text('pub fn state_contract() {}\n')
    (repo / 'scripts').mkdir()
    (repo / 'scripts/ci-required-scope.json').write_text('{}\n')
    (repo / 'README.md').write_text('test fixture\n')
    command(repo, 'add', '.')
    command(repo, 'commit', '-qm', 'baseline')
    return repo, command(repo, 'rev-parse', 'HEAD')


def valid_plan(source):
    return {'scope': 'targeted', 'execution_profile': 'pr-fast',
            'fast_execution_version': 1, 'ordinary_execution': 'inline',
            'test_policy_changed': False, 'fast_plan_exceptions': [],
            'rust_baseline_checks': [], 'oasis7_required_wasm_packages': [],
            'groups': sorted(PREVIEW.EXPECTED_GROUPS),
            'source_sha': source, 'tested_sha': source, 'policy_sha': source,
            'ordinary_fast_targets': [dict(package=package, target_kind=kind, target_name=name)
                for package, kind, name in sorted(PREVIEW.EXPECTED_TARGETS)]}


class Preview(unittest.TestCase):
    def test_current_source_unchanged_and_diagnostic_baseline_exact(self):
        with tempfile.TemporaryDirectory() as directory:
            repo, source = fixture(directory)
            before_index = command(repo, 'write-tree')
            synthetic, tree = PREVIEW.synthetic_baseline(repo, source)
            self.assertEqual(command(repo, 'rev-parse', 'HEAD'), source)
            self.assertEqual(command(repo, 'status', '--porcelain'), '')
            self.assertEqual(command(repo, 'write-tree'), before_index)
            self.assertEqual(command(repo, 'rev-parse', synthetic + '^'), source)
            self.assertEqual(command(repo, 'rev-parse', synthetic + '^{tree}'), tree)
            self.assertEqual(set(command(repo, 'diff', '--name-only', synthetic, source).splitlines()),
                             set(PREVIEW.SAMPLE_PATHS))
            for path in PREVIEW.SAMPLE_PATHS:
                self.assertEqual(PREVIEW.git(repo, 'show', synthetic + ':' + path),
                                 PREVIEW.git(repo, 'show', source + ':' + path) + PREVIEW.COMMENT)
            self.assertEqual(PREVIEW.synthetic_baseline(repo, source), (synthetic, tree))

    def test_fresh_checkout_reconstructs_same_objects_and_rejects_identity_drift(self):
        with tempfile.TemporaryDirectory() as directory:
            repo, source = fixture(directory)
            synthetic, tree = PREVIEW.synthetic_baseline(repo, source)
            clone = Path(directory) / 'consumer'
            subprocess.run(['git', 'clone', '-q', '--no-local', str(repo), str(clone)], check=True)
            missing = subprocess.run(['git', '-C', str(clone), 'cat-file', '-e', synthetic],
                                     capture_output=True)
            self.assertNotEqual(missing.returncode, 0)
            metadata = dict(schema_version=1, authority='diagnostic-only', workload='synthetic-comment-diff',
                            paths=list(PREVIEW.SAMPLE_PATHS), source_sha=source, tested_sha=source,
                            policy_sha=source, source_tree=command(repo, 'rev-parse', source + '^{tree}'),
                            synthetic_base_sha=synthetic, synthetic_base_tree=tree)
            PREVIEW.restore_baseline(clone, metadata)
            self.assertEqual(command(clone, 'rev-parse', synthetic), synthetic)
            for key in ('source_sha', 'tested_sha', 'policy_sha', 'source_tree', 'synthetic_base_sha', 'synthetic_base_tree'):
                altered = dict(metadata, **{key: '0' * 40})
                with self.subTest(key=key), self.assertRaises(ValueError):
                    PREVIEW.restore_baseline(clone, altered)
            with self.assertRaises(ValueError):
                PREVIEW.restore_baseline(clone, dict(metadata, authority='required'))

    def test_no_policy_change_explicitly_disables_preview(self):
        with tempfile.TemporaryDirectory() as directory:
            repo, base = fixture(directory)
            (repo / 'README.md').write_text('ordinary documentation change\n')
            command(repo, 'commit', '-qam', 'docs')
            output = Path(directory) / 'github-output'
            self.assertIsNone(PREVIEW.plan_preview(repo, base, 'HEAD', Path(directory) / 'plan.json', output))
            self.assertEqual(output.read_text(), 'enabled=false\n')
            self.assertFalse((Path(directory) / 'plan.json').exists())

    def test_real_cli_runs_candidate_planner_but_keeps_distinct_authority(self):
        with tempfile.TemporaryDirectory() as directory:
            repo, base = fixture(directory)
            # The actual subprocess parses and reports the identities received
            # by the candidate planner; it never receives the real PR base.
            planner = """import argparse,json,pathlib
p=argparse.ArgumentParser()
for key in ('profile','base-ref','head-ref','test-ref','policy-ref','config','formal-config','output'):
 p.add_argument('--'+key,required=True)
a=p.parse_args()
assert a.base_ref != a.head_ref
assert a.head_ref == a.test_ref == a.policy_ref
plan=PLAN
for key in ('source_sha','tested_sha','policy_sha'): plan[key]=a.head_ref
pathlib.Path(a.output).write_text(json.dumps(plan))
""".replace('PLAN', repr(valid_plan('unused')))
            (repo / 'scripts/plan-rust-required-scope.py').write_text(planner)
            command(repo, 'add', '.')
            command(repo, 'commit', '-qm', 'candidate policy')
            source = command(repo, 'rev-parse', 'HEAD')
            output = Path(directory) / 'plan.json'
            github_output = Path(directory) / 'github-output'
            metadata = PREVIEW.plan_preview(repo, base, source, output, github_output)
            self.assertEqual(metadata['authority'], 'diagnostic-only')
            self.assertEqual(metadata['source_sha'], source)
            self.assertEqual(metadata['actual_pr_base_sha'], base)
            self.assertNotEqual(metadata['synthetic_base_sha'], base)
            self.assertEqual(json.loads(Path(str(output) + '.metadata.json').read_text()), metadata)
            self.assertIn('enabled=true\n', github_output.read_text())
            self.assertEqual(command(repo, 'status', '--porcelain'), '')
            with self.assertRaises(ValueError):
                PREVIEW.plan_preview(repo, base, base, output, github_output)

    def test_missing_extra_or_changed_work_never_becomes_a_smaller_benchmark(self):
        source = 'a' * 40
        original = valid_plan(source)
        PREVIEW.validate_plan(original, source)
        mutations = (
            ('scope', 'full'), ('execution_profile', 'candidate-full'),
            ('ordinary_execution', 'artifacts'), ('fast_execution_version', True),
            ('test_policy_changed', True), ('fast_plan_exceptions', ['risk']),
            ('rust_baseline_checks', ['newapi_accounting']),
            ('oasis7_required_wasm_packages', ['oasis7_viewer_codec']),
            ('groups', ['baseline', 'oasis7_required']),
            ('groups', sorted(PREVIEW.EXPECTED_GROUPS) + ['launcher_web']),
            ('ordinary_fast_targets', original['ordinary_fast_targets'][:-1]),
            ('source_sha', 'b' * 40), ('tested_sha', 'b' * 40), ('policy_sha', 'b' * 40),
        )
        for key, value in mutations:
            plan = copy.deepcopy(original)
            plan[key] = value
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                PREVIEW.validate_plan(plan, source)


if __name__ == '__main__':
    unittest.main()
