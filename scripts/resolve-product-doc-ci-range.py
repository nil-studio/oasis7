#!/usr/bin/env python3
"""Resolve the baseline job's tested documentation range; never change Git state."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys


def oid(value, label):
    if not isinstance(value, str) or not re.fullmatch(r'[0-9a-fA-F]{40}', value) or value == '0' * 40:
        raise ValueError(f'{label} must be a nonzero full 40-character commit OID')
    return value.lower()


def git(root, *args):
    result = subprocess.run(['git', '-C', str(root), *args], text=True, capture_output=True)
    if result.returncode:
        raise ValueError(f'Git lookup failed: {" ".join(args)}')
    return result.stdout.strip()


def resolve(root, event_name, event, tested_oid, *, push_range_base=None):
    tested = oid(tested_oid, 'tested OID')
    if git(root, 'rev-parse', '--verify', 'HEAD^{commit}') != tested:
        raise ValueError('checkout HEAD does not match tested OID')
    if not isinstance(event, dict):
        raise ValueError('event must be an object')
    if event_name == 'pull_request':
        pr = event.get('pull_request')
        if not isinstance(pr, dict) or not isinstance(pr.get('head'), dict) or not isinstance(pr.get('base'), dict):
            raise ValueError('pull_request event must provide base/head identities')
        # Validate the event identity, but use the actual tested merge parent,
        # which can advance beyond the event's original base without this PR.
        oid(pr['base'].get('sha'), 'event PR base')
        source = oid(pr['head'].get('sha'), 'event PR head')
        parents = git(root, 'show', '-s', '--format=%P', tested).split()
        if len(parents) != 2:
            raise ValueError('tested PR checkout must have exactly two parents')
        if parents[1] != source:
            raise ValueError('tested merge second parent does not match event PR head')
        base, head = parents[0], tested
    elif event_name == 'push':
        base = oid(event.get('before'), 'push before')
        head = oid(event.get('after'), 'push after')
        if head != tested:
            raise ValueError('push after does not match tested OID')
        if push_range_base:
            widened = oid(push_range_base, 'selected push range base')
            git(root, 'cat-file', '-e', widened + '^{commit}')
            ancestor = subprocess.run(['git', '-C', str(root), 'merge-base', '--is-ancestor', widened, base],
                                      text=True, capture_output=True)
            if ancestor.returncode != 0:
                raise ValueError('selected push range base must be an ancestor of push before')
            base = widened
    else:
        raise ValueError(f'unsupported CI event: {event_name}')
    for commit in (base, head):
        git(root, 'cat-file', '-e', commit + '^{commit}')
    return base, head


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo-root', required=True, type=Path)
    parser.add_argument('--event-name', required=True)
    parser.add_argument('--event-path', required=True, type=Path)
    parser.add_argument('--tested-oid', required=True)
    args = parser.parse_args()
    try:
        event = json.loads(args.event_path.read_text(encoding='utf-8'))
        base, head = resolve(args.repo_root, args.event_name, event, args.tested_oid,
                             push_range_base=os.environ.get('SELECT_RANGE_BASE', '').strip() or None)
    except (OSError, ValueError) as error:
        print(f'product-doc-ci-range: error: {error}', file=sys.stderr)
        return 2
    # Emit nothing until every check passes, so failures cannot export a partial range.
    print(f'OASIS7_PRODUCT_DOC_BASE={base}')
    print(f'OASIS7_PRODUCT_DOC_HEAD={head}')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
