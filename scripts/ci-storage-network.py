#!/usr/bin/env python3
"""Owned loopback-only netem profiles for disposable Linux CI runners."""
import argparse
import json
import os
from pathlib import Path
import platform
import subprocess

HANDLE = '7a35:'
PROFILES = {
    'delay': ['delay', '2ms', '1ms', 'distribution', 'normal'],
    'loss': ['delay', '2ms', 'loss', 'random', '0.2%', 'duplicate', '0.1%',
             'reorder', '1%', '50%'],
}


def qdiscs():
    return json.loads(subprocess.check_output(['tc', '-j', '-s', 'qdisc', 'show', 'dev', 'lo'], text=True))


def owned(entries):
    return [entry for entry in entries if entry.get('handle') == HANDLE and entry.get('kind') == 'netem']


def operate(action, profile, report):
    if platform.system() != 'Linux' or os.environ.get('GITHUB_ACTIONS') != 'true':
        raise ValueError('netem requires an explicitly dispatched disposable GitHub Linux runner')
    entries = qdiscs()
    if action == 'setup':
        if any(entry.get('kind') != 'noqueue' for entry in entries):
            raise ValueError('refusing to replace an existing loopback queue discipline')
        subprocess.run(['sudo', 'tc', 'qdisc', 'add', 'dev', 'lo', 'root', 'handle', HANDLE,
                        'netem', 'limit', '10000', *PROFILES[profile]], check=True)
        entries = qdiscs()
        if len(owned(entries)) != 1:
            raise ValueError('requested netem profile was not installed')
    elif action == 'check':
        selected = owned(entries)
        if len(selected) != 1 or selected[0].get('packets', 0) <= 0:
            raise ValueError('no observed test traffic through the owned netem discipline')
        if profile == 'loss' and selected[0].get('drops', 0) <= 0:
            raise ValueError('loss profile did not observe a packet drop; acceptance inconclusive')
    elif action == 'cleanup':
        if owned(entries):
            subprocess.run(['sudo', 'tc', 'qdisc', 'del', 'dev', 'lo', 'root', 'handle', HANDLE], check=True)
        if owned(qdiscs()):
            raise ValueError('owned network fault discipline remains installed')
    result = dict(schema_version=1, action=action, profile=profile, device='lo', handle=HANDLE,
                  parameters=PROFILES[profile], seed=None, kernel=platform.release(), qdiscs=entries,
                  scope='single-host packet fault model; not independent failure domains or production network performance')
    report.parent.mkdir(parents=True, exist_ok=True)
    report.write_text(json.dumps(result, indent=2) + '\n')
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['setup', 'check', 'cleanup'])
    parser.add_argument('--profile', choices=PROFILES, required=True)
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    operate(args.action, args.profile, args.report)
