#!/usr/bin/env python3
"""Fetch pinned distributions, then build/install them without dependency resolution."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import urllib.parse
import urllib.request


def artifacts():
    rows = json.loads(Path(__file__).with_name('artifacts.json').read_text())
    if len(rows) != 18 or len({row['filename'] for row in rows}) != len(rows):
        raise ValueError('unexpected distribution inventory')
    for row in rows:
        url = urllib.parse.urlsplit(row['url'])
        if (url.scheme != 'https' or url.netloc != 'files.pythonhosted.org'
                or url.query or url.fragment or Path(url.path).name != row['filename']
                or '/' in row['filename'] or row['group'] not in ('sdk', 'build')
                or not re.fullmatch(r'[a-f0-9]{64}', row['sha256'])
                or type(row['size']) is not int or not 0 < row['size'] <= 20 * 1024 * 1024):
            raise ValueError('invalid pinned distribution')
    return rows


def fetch(destination):
    destination.mkdir()
    for row in artifacts():
        target = destination / row['filename']
        digest, count = hashlib.sha256(), 0
        with urllib.request.urlopen(row['url'], timeout=120) as response, target.open('xb') as output:
            if urllib.parse.urlsplit(response.url).netloc != 'files.pythonhosted.org':
                raise ValueError('distribution redirected outside PyPI files')
            while chunk := response.read(1024 * 1024):
                count += len(chunk)
                if count > row['size']:
                    raise ValueError('distribution exceeds its locked size')
                digest.update(chunk)
                output.write(chunk)
        if count != row['size'] or digest.hexdigest() != row['sha256']:
            raise ValueError('distribution integrity mismatch')


def install(destination):
    rows = artifacts()
    for row in rows:
        source = destination / row['filename']
        if source.stat().st_size != row['size'] or hashlib.sha256(source.read_bytes()).hexdigest() != row['sha256']:
            raise ValueError('offline input differs from its lock')
    subprocess.run(['python3', '-m', 'venv', '/tmp/oss-build'], check=True)
    pip = ['/tmp/oss-build/bin/python', '-m', 'pip']
    subprocess.run([*pip, 'install', '--no-index', '--no-deps',
                    *(str(destination / row['filename']) for row in rows if row['group'] == 'build')], check=True)
    wheels = Path('/tmp/oss-wheels')
    wheels.mkdir()
    # Build isolation is explicitly disabled: only the three locked build wheels
    # above are available. Docker also runs this entire phase with network=none.
    subprocess.run([*pip, 'wheel', '--no-index', '--no-deps', '--no-build-isolation',
                    '--wheel-dir', str(wheels),
                    *(str(destination / row['filename']) for row in rows if row['group'] == 'sdk')], check=True)
    subprocess.run(['python3', '-m', 'venv', '--without-pip', '/opt/oasis7-oss'], check=True)
    subprocess.run([*pip, '--python', '/opt/oasis7-oss/bin/python', 'install', '--no-index', '--no-deps',
                    *(str(path) for path in sorted(wheels.glob('*.whl')))], check=True)
    subprocess.run([*pip, '--python', '/opt/oasis7-oss/bin/python', 'check'], check=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation', choices=('fetch', 'install'))
    parser.add_argument('destination', type=Path)
    args = parser.parse_args()
    (fetch if args.operation == 'fetch' else install)(args.destination)
