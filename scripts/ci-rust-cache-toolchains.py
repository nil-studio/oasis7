#!/usr/bin/env python3
"""Keep cache fingerprints independent of unrelated hosted runner toolchains.

Run after installing the job's pinned compilers and before rust-cache. This is
for disposable GitHub-hosted Linux runners, never local or self-hosted Rustup.
"""
import argparse
import os
import re
import subprocess

PIN = re.compile(r'(?:\d+\.\d+\.\d+|nightly-\d{4}-\d{2}-\d{2})\Z', re.ASCII)


def rustup(*args):
    return subprocess.check_output(['rustup', *args], text=True).strip()


def installed_toolchains():
    lines = rustup('toolchain', 'list').splitlines()
    names = [line.split()[0] for line in lines if line.strip()]
    if not names or len(names) != len(set(names)):
        raise ValueError('expected an unambiguous installed Rust toolchain set')
    return set(names)


def normalize(pins):
    if (os.environ.get('GITHUB_ACTIONS') != 'true'
            or os.environ.get('RUNNER_ENVIRONMENT') != 'github-hosted'
            or os.environ.get('RUNNER_OS') != 'Linux'):
        raise ValueError('cache toolchain preparation requires a disposable GitHub-hosted Linux runner')
    if not pins or any(not PIN.fullmatch(pin) for pin in pins) or len(set(pins)) != len(pins):
        raise ValueError('supply unique exact Rust versions or dated nightly versions')

    installed = installed_toolchains()
    required = set()
    # Validate the complete keep set before the first uninstall. Rustup run
    # requires an installed toolchain; it does not install an unpinned fallback.
    for pin in pins:
        version = rustup('run', pin, 'rustc', '-vV')
        fields = dict(line.split(': ', 1) for line in version.splitlines() if ': ' in line)
        host = fields.get('host', '')
        if not re.fullmatch(r'[A-Za-z0-9_]+(?:-[A-Za-z0-9_]+)+', host):
            raise ValueError('pinned compiler did not report a valid host triple')
        name = pin + '-' + host
        if name not in installed:
            raise ValueError('required toolchain is not installed: ' + name)
        if not pin.startswith('nightly-') and fields.get('release') != pin:
            raise ValueError('pinned stable compiler release does not match: ' + pin)
        if pin.startswith('nightly-') and not fields.get('release', '').endswith('-nightly'):
            raise ValueError('dated nightly did not resolve to a nightly compiler')
        required.add(name)
    active = rustup('show', 'active-toolchain').split()[0]
    if active not in required:
        raise ValueError('active toolchain is outside the required cache set: ' + active)

    for name in sorted(installed - required):
        rustup('toolchain', 'uninstall', name)
    if installed_toolchains() != required:
        raise ValueError('installed Rust toolchain set changed during cache preparation')
    print('rust_cache_toolchains=' + ','.join(sorted(required)), flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('toolchains', nargs='+')
    args = parser.parse_args()
    try:
        normalize(args.toolchains)
    except (ValueError, subprocess.CalledProcessError, OSError, IndexError) as error:
        parser.exit(1, f'Rust cache toolchain preparation failed: {error}\n')


if __name__ == '__main__':
    main()
