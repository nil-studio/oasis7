#!/usr/bin/env python3
"""Bound a browser install and its sudo apt descendants without elevating the browser."""
import argparse
import importlib.util
import math
import os
from pathlib import Path
import pwd
import signal
import sys
import time

spec = importlib.util.spec_from_file_location('ci_apt', Path(__file__).with_name('ci-apt-install.py'))
apt = importlib.util.module_from_spec(spec)
spec.loader.exec_module(apt)


def caller_command(command, path):
    """Only the original non-root sudo caller may run the browser installer."""
    if os.geteuid() != 0:
        raise RuntimeError('supervisor requires sudo for owned apt descendant cleanup')
    try:
        uid = int(os.environ['SUDO_UID'])
        gid = int(os.environ['SUDO_GID'])
        user = pwd.getpwuid(uid)
    except (KeyError, ValueError) as error:
        raise RuntimeError('verified sudo caller required') from error
    if uid == 0 or user.pw_name != os.environ.get('SUDO_USER') or user.pw_gid != gid:
        raise RuntimeError('non-root sudo caller identity mismatch')
    if not command or not Path(command[0]).is_absolute() or not path:
        raise RuntimeError('absolute installer executable and caller PATH required')
    # runuser performs the credential/group transition without a Python preexec_fn
    # (Installer owns a logging thread). Browser/npm code never runs as root.
    return ['/usr/sbin/runuser', '--user', user.pw_name, '--preserve-environment', '--',
            '/usr/bin/env', '-i', 'DEBIAN_FRONTEND=noninteractive', 'LC_ALL=C',
            'DEBCONF_NONINTERACTIVE_SEEN=true', 'APT_LISTCHANGES_FRONTEND=none',
            f'HOME={user.pw_dir}', f'USER={user.pw_name}',
            f'LOGNAME={user.pw_name}', f'PATH={path}', *command]


def supervise(command, budget):
    apt.establish_owner()
    installer = apt.Installer(budget)
    previous = {sig: signal.signal(sig, installer.cancel)
                for sig in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP)}
    try:
        status, _, valid = installer.run_command('browser-install', command)
        success = valid and status == 0 and not installer.cancelled
        installer.log(f'browser_install_ready={str(success).lower()}')
        if success:
            return 0
        if installer.cancelled:
            return 128 + installer.cancelled
        return status if status is not None and 0 < status < 256 else 1
    finally:
        for sig, handler in previous.items():
            signal.signal(sig, handler)
        installer.log.flush(time.monotonic() + 0.1)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--budget-seconds', type=float, required=True)
    parser.add_argument('--caller-path', required=True)
    parser.add_argument('command', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if not math.isfinite(args.budget_seconds) or args.budget_seconds <= 0:
        parser.error('budget must be finite and positive')
    command = args.command[1:] if args.command[:1] == ['--'] else args.command
    try:
        return supervise(caller_command(command, args.caller_path), args.budget_seconds)
    except (OSError, RuntimeError) as error:
        print(f'browser_install_ready=false failure={type(error).__name__}: {error}', flush=True)
        return 1


if __name__ == '__main__':
    sys.exit(main())
