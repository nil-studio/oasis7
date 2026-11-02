#!/usr/bin/env python3
"""Measure a candidate fast policy on current code, without authorizing a PR gate."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parent.parent
POLICY_INPUTS = frozenset((
    '.github/workflows/rust.yml', 'scripts/ci-required-scope.json',
    'scripts/plan-rust-required-scope.py', 'scripts/ci-tests.sh',
    'scripts/ci-test-artifacts.py', 'scripts/ci-fast-preview.py',
))
SAMPLE_PATHS = (
    'crates/oasis7_runtime/src/world/incremental_commit.rs',
    'crates/oasis7_world_state/src/incremental_fixed_economy_records.rs',
)
COMMENT = b'\n// Synthetic CI budget preview baseline; production behavior is unchanged.\n'
EXPECTED_GROUPS = frozenset(('baseline', 'rust_baseline', 'oasis7_required'))
EXPECTED_TARGETS = frozenset((
    ('oasis7_runtime', 'lib', 'oasis7_runtime'),
    ('oasis7_runtime', 'test', 'pr_fast_incremental'),
    ('oasis7_world_state', 'lib', 'oasis7_world_state'),
    ('oasis7_world_state', 'test', 'pr_fast_reducers'),
))


def git(repo, *args, data=None, env=None):
    return subprocess.check_output(['git', '-C', str(repo), *args], input=data,
                                   env=env, stderr=subprocess.PIPE)


def resolve(repo, ref):
    return git(repo, 'rev-parse', '--verify', '--end-of-options', ref + '^{commit}').decode().strip()


def synthetic_baseline(repo, source):
    """Create only Git objects, leaving the candidate checkout and index intact."""
    timestamp = git(repo, 'show', '-s', '--format=%ct', source).decode().strip()
    environment = dict(os.environ, GIT_AUTHOR_NAME='CI budget preview',
        GIT_AUTHOR_EMAIL='ci-preview@example.invalid', GIT_COMMITTER_NAME='CI budget preview',
        GIT_COMMITTER_EMAIL='ci-preview@example.invalid',
        GIT_AUTHOR_DATE=timestamp + ' +0000', GIT_COMMITTER_DATE=timestamp + ' +0000')
    with tempfile.TemporaryDirectory(prefix='oasis7-fast-preview-') as directory:
        environment['GIT_INDEX_FILE'] = str(Path(directory) / 'index')
        git(repo, 'read-tree', source, env=environment)
        for path in SAMPLE_PATHS:
            entry = git(repo, 'ls-tree', source, '--', path).decode().strip()
            if not entry or entry.split()[0:2] != ['100644', 'blob']:
                raise ValueError('preview requires a tracked regular source file: ' + path)
            original = git(repo, 'show', source + ':' + path)
            blob = git(repo, 'hash-object', '-w', '--stdin', data=original + COMMENT).decode().strip()
            git(repo, 'update-index', '--cacheinfo', '100644', blob, path, env=environment)
        tree = git(repo, 'write-tree', env=environment).decode().strip()
        commit = git(repo, 'commit-tree', tree, '-p', source, env=environment,
                     data=b'Synthetic comment-only baseline for CI budget preview\n').decode().strip()
    return commit, tree


def validate_plan(plan, source):
    if (not isinstance(plan, dict) or plan.get('scope') != 'targeted'
            or plan.get('execution_profile') != 'pr-fast'
            or type(plan.get('fast_execution_version')) is not int
            or plan['fast_execution_version'] != 1
            or plan.get('ordinary_execution') != 'inline'
            or plan.get('test_policy_changed') is not False
            or plan.get('fast_plan_exceptions') != []
            or plan.get('rust_baseline_checks') != []
            or plan.get('oasis7_required_wasm_packages') != []):
        raise ValueError('preview did not select the expected bounded native execution')
    groups = plan.get('groups')
    if not isinstance(groups, list) or len(groups) != len(EXPECTED_GROUPS) or set(groups) != EXPECTED_GROUPS:
        raise ValueError('preview selected unexpected groups; do not silently omit work')
    for key in ('source_sha', 'tested_sha', 'policy_sha'):
        if plan.get(key) != source:
            raise ValueError('preview source/tested/policy identity mismatch: ' + key)
    targets = plan.get('ordinary_fast_targets')
    if not isinstance(targets, list) or not targets:
        raise ValueError('preview has no executable target inventory')
    identities = {(item.get('package'), item.get('target_kind'), item.get('target_name'))
                  for item in targets if isinstance(item, dict)}
    if len(targets) != len(EXPECTED_TARGETS) or identities != EXPECTED_TARGETS:
        raise ValueError('preview must execute the complete expected behavioral-owner inventory')


def write_outputs(path, values):
    if path:
        with Path(path).open('a') as output:
            for key, value in values.items():
                value = str(value)
                if '\n' in value or '\r' in value:
                    raise ValueError('invalid preview output value')
                output.write(key + '=' + value + '\n')


def plan_preview(repo, base_ref, head_ref, output, github_output=None):
    base, source = resolve(repo, base_ref), resolve(repo, head_ref)
    if resolve(repo, 'HEAD') != source:
        raise ValueError('preview must use its exact candidate checkout')
    changed = set(git(repo, 'diff', '--name-only', '-z', base, source).decode().split('\0'))
    if not changed.intersection(POLICY_INPUTS):
        write_outputs(github_output, {'enabled': 'false'})
        print('No CI policy change: candidate fast preview is not selected.')
        return None
    synthetic, tree = synthetic_baseline(repo, source)
    output = Path(output).resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run([sys.executable, str(repo / 'scripts/plan-rust-required-scope.py'),
                    '--profile', 'pr-fast', '--base-ref', synthetic, '--head-ref', source,
                    '--test-ref', source, '--policy-ref', source,
                    '--config', str(repo / 'scripts/ci-required-scope.json'),
                    '--formal-config', str(repo / 'scripts/ci-formal-process-suites.json'),
                    '--output', str(output)], cwd=repo, check=True)
    selected = json.loads(output.read_text())
    validate_plan(selected, source)
    metadata = {
        'schema_version': 1, 'authority': 'diagnostic-only',
        'workload': 'synthetic-comment-diff', 'cache': 'cold-no-restore',
        'actual_pr_base_sha': base, 'source_sha': source, 'tested_sha': source,
        'policy_sha': source, 'source_tree': git(repo, 'rev-parse', source + '^{tree}').decode().strip(),
        'synthetic_base_sha': synthetic, 'synthetic_base_tree': tree,
        'paths': list(SAMPLE_PATHS),
        'limitation': 'Current real tests and compilation footprint; not PR authorization, regression quality, or P95 evidence.',
    }
    metadata_path = Path(str(output) + '.metadata.json')
    metadata_path.write_text(json.dumps(metadata, ensure_ascii=False, indent=2) + '\n')
    write_outputs(github_output, {'enabled': 'true', 'source_sha': source, 'tested_sha': source,
        'synthetic_base_sha': synthetic, 'metadata_path': metadata_path, 'plan_path': output})
    print(json.dumps(metadata, ensure_ascii=False))
    return metadata


def restore_baseline(repo, metadata):
    if (not isinstance(metadata, dict) or metadata.get('schema_version') != 1
            or metadata.get('authority') != 'diagnostic-only'
            or metadata.get('workload') != 'synthetic-comment-diff'
            or metadata.get('paths') != list(SAMPLE_PATHS)):
        raise ValueError('invalid diagnostic preview metadata')
    source = resolve(repo, 'HEAD')
    if any(metadata.get(key) != source for key in ('source_sha', 'tested_sha', 'policy_sha')):
        raise ValueError('preview checkout identity mismatch')
    if git(repo, 'rev-parse', source + '^{tree}').decode().strip() != metadata.get('source_tree'):
        raise ValueError('preview source tree mismatch')
    synthetic, tree = synthetic_baseline(repo, source)
    if synthetic != metadata.get('synthetic_base_sha') or tree != metadata.get('synthetic_base_tree'):
        raise ValueError('preview synthetic baseline identity mismatch')
    print('Reconstructed diagnostic-only baseline ' + synthetic)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo-root', type=Path, default=ROOT)
    commands = parser.add_subparsers(dest='command', required=True)
    plan = commands.add_parser('plan')
    plan.add_argument('--base-ref', required=True)
    plan.add_argument('--head-ref', required=True)
    plan.add_argument('--output', required=True)
    plan.add_argument('--github-output')
    restore = commands.add_parser('restore')
    restore.add_argument('--metadata', required=True)
    args = parser.parse_args()
    if args.command == 'plan':
        plan_preview(args.repo_root.resolve(), args.base_ref, args.head_ref, args.output, args.github_output)
    else:
        restore_baseline(args.repo_root.resolve(), json.loads(Path(args.metadata).read_text()))


if __name__ == '__main__':
    main()
