#!/usr/bin/env python3
"""Plan affected checks for a clean committed tree; this first slice never runs checks."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


class Blocked(Exception):
    pass


def git(root, *args):
    env = os.environ.copy()
    for key in ('GIT_DIR', 'GIT_WORK_TREE', 'GIT_INDEX_FILE', 'GIT_EXTERNAL_DIFF'):
        env.pop(key, None)
    env['GIT_OPTIONAL_LOCKS'] = '0'
    try:
        return subprocess.check_output(['git', '-C', str(root), *args], stderr=subprocess.PIPE, env=env)
    except (OSError, subprocess.CalledProcessError) as exc:
        raise Blocked(f'Git command failed: {args[0]}: {exc}') from exc


def commit(root, ref):
    if not ref or ref.startswith('-'):
        raise Blocked('invalid ref')
    return git(root, 'rev-parse', '--verify', '--end-of-options', ref + '^{commit}').decode('ascii').strip()


def state(root):
    head = commit(root, 'HEAD')
    flags = git(root, 'ls-files', '-v', '-z').split(b'\0')
    if flags.pop() != b'' or any(not record or record[:1] == b'S' or record[:1].islower() for record in flags):
        raise Blocked('assume-unchanged or skip-worktree index entries are unsupported')
    if git(root, 'ls-files', '--unmerged', '-z'):
        raise Blocked('unmerged index entries')
    status = git(root, '-c', 'core.filemode=true', '-c', 'core.fsmonitor=false',
                 '-c', 'core.untrackedCache=false', 'status', '--porcelain=v1', '-z', '--untracked-files=all', '--ignore-submodules=none')
    if status:
        raise Blocked('dirty or untracked inputs are not supported by this planning slice')
    index = os.fsdecode(git(root, 'rev-parse', '--git-path', 'index').rstrip(b'\n'))
    index_path = Path(index)
    if not index_path.is_absolute():
        index_path = root / index_path
    try:
        digest = hashlib.sha256(index_path.read_bytes()).hexdigest()
    except OSError as exc:
        raise Blocked(f'cannot read index: {exc}') from exc
    return head, digest


def plan(root, base_ref):
    before = state(root)
    # The local origin/main ref is the trust anchor. Never execute policy from
    # an arbitrary candidate commit supplied as a comparison base.
    remote = commit(root, 'refs/remotes/origin/main')
    if base_ref is None:
        bases = git(root, 'merge-base', '--all', before[0], remote).decode('ascii').splitlines()
        if len(bases) != 1:
            raise Blocked('HEAD and origin/main must have a unique merge base')
        base = bases[0]
        base_source = 'merge-base(origin/main,HEAD)'
    else:
        base = commit(root, base_ref)
        base_source = base_ref
    try:
        git(root, 'merge-base', '--is-ancestor', base, remote)
    except Blocked as exc:
        raise Blocked('base is not an ancestor of local origin/main; refusing untrusted policy') from exc
    with tempfile.TemporaryDirectory(prefix='oasis7-local-plan-') as directory:
        temp = Path(directory)
        for name in ('plan-rust-required-scope.py', 'ci-required-scope.json', 'ci-formal-process-suites.json'):
            (temp / name).write_bytes(git(root, 'show', f'{base}:scripts/{name}'))
        argv = [sys.executable, str(temp / 'plan-rust-required-scope.py'),
                '--base-ref', base, '--head-ref', before[0], '--policy-ref', base,
                '--profile', 'pr-fast', '--config', str(temp / 'ci-required-scope.json'),
                '--formal-config', str(temp / 'ci-formal-process-suites.json')]
        env = os.environ.copy()
        for key in ('GIT_DIR', 'GIT_WORK_TREE', 'GIT_INDEX_FILE', 'GIT_EXTERNAL_DIFF'):
            env.pop(key, None)
        env['GIT_OPTIONAL_LOCKS'] = '0'
        try:
            result = subprocess.run(argv, cwd=root, env=env, stdout=subprocess.PIPE,
                                    stderr=subprocess.PIPE, check=False)
        except OSError as exc:
            raise Blocked(f'cannot start trusted planner: {exc}') from exc
        if result.returncode:
            raise Blocked(f'trusted planner failed ({result.returncode}): '
                          + result.stderr.decode('utf-8', 'replace').strip())
        try:
            planned = json.loads(result.stdout)
        except (ValueError, UnicodeDecodeError) as exc:
            raise Blocked('trusted planner returned invalid JSON') from exc
    if state(root) != before or commit(root, 'refs/remotes/origin/main') != remote:
        raise Blocked('stale_input: HEAD, index or trust anchor changed while planning')
    return {'schema_version': 1, 'status': 'planned', 'validated': False,
            'profile': 'affected', 'source_sha': before[0], 'base_sha': base,
            'base_source': base_source, 'policy_sha': base, 'trust_anchor_sha': remote,
            'planner_sha256': hashlib.sha256(git(root, 'show', f'{base}:scripts/plan-rust-required-scope.py')).hexdigest(),
            'scope': 'committed-only', 'dirty': False, 'untracked': False,
            'planner_command': ['python3', 'trusted-base:scripts/plan-rust-required-scope.py',
                                '--base-ref', base, '--head-ref', before[0],
                                '--policy-ref', base, '--profile', 'pr-fast'],
            'checks_executed': 0, 'plan': planned,
            'limitations': ['No checks were executed.', 'Only clean committed input is supported.',
                            'Ignored or external runtime inputs and other platforms are not validated.',
                            'Policy trust assumes the local origin/main ref is trustworthy.']}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--profile', choices=('affected',), required=True)
    parser.add_argument('--dry-run', action='store_true', required=True)
    parser.add_argument('--base-ref')
    parser.add_argument('--format', choices=('text', 'json'), default='text')
    args = parser.parse_args()
    try:
        root = Path(os.fsdecode(git(Path.cwd(), 'rev-parse', '--show-toplevel').rstrip(b'\n')))
        result = plan(root, args.base_ref)
    except (Blocked, OSError, UnicodeError) as exc:
        print(f'blocked: {exc}', file=sys.stderr)
        return 2
    if args.format == 'json':
        print(json.dumps(result, ensure_ascii=True, sort_keys=True))
    else:
        print(f"planned, not validated; source={result['source_sha']} base={result['base_sha']} policy={result['policy_sha']}")
        print('Checks executed: 0')
        print('Groups:', ', '.join(result['plan'].get('groups', [])))
        print('Full result:')
        print(json.dumps(result, ensure_ascii=True, sort_keys=True, indent=2))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
