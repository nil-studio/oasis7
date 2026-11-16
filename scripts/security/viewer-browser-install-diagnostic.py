#!/usr/bin/env python3
"""Failure-only read observations; never repairs APT or changes install status."""
import json
from pathlib import Path
import subprocess
import sys
import time

import codeql_rust_native_observer as observer

HOSTS = ('azure.archive.ubuntu.com', 'archive.ubuntu.com', 'security.ubuntu.com')


def collect(run=subprocess.run, sources=observer.source_snapshot):
    result = {'scope': 'post-failure observation, not proof of the earlier failure cause',
              'source_configuration': sources(), 'dns': [], 'apt_version': [],
              'worker_redirect_chain': 'unknown: original installer did not emit worker frames'}
    try:
        p = run(['apt-get', '--version'], capture_output=True, timeout=2, check=False)
        result['apt_version'] = [observer.project(line, 'apt-version') for line in p.stdout[:8192].splitlines()
                                 if observer.project(line, 'apt-version').get('kind') == 'APT_VERSION']
    except (OSError, subprocess.TimeoutExpired):
        result['apt_version_status'] = 'unavailable'
    for host in HOSTS:
        started = time.monotonic()
        try:
            p = run(['getent', 'ahosts', host], stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL, timeout=2, check=False)
            row = {'host': host, 'status': 'resolved' if p.returncode == 0 else 'unresolved', 'exit': p.returncode}
        except (OSError, subprocess.TimeoutExpired):
            row = {'host': host, 'status': 'unavailable'}
        row['elapsed_seconds'] = time.monotonic() - started
        result['dns'].append(row)
    return result


if __name__ == '__main__':
    if len(sys.argv) != 2:
        raise SystemExit('usage: viewer-browser-install-diagnostic.py output.json')
    out = Path(sys.argv[1]); out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(collect(), indent=2) + '\n')
