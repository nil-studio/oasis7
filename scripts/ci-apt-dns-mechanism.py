#!/usr/bin/env python3
"""Branch-only hosted experiment; deliberately NOT evidence of automatic failover."""
import argparse
import importlib.util
import math
import os
from pathlib import Path
import signal
import sys
import time

spec = importlib.util.spec_from_file_location('ci_apt_mechanism', Path(__file__).with_name('ci-apt-install.py'))
apt = importlib.util.module_from_spec(spec)
spec.loader.exec_module(apt)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--budget-seconds', type=float, required=True)
    args = parser.parse_args()
    if not math.isfinite(args.budget_seconds) or not 0 < args.budget_seconds <= 600 or os.geteuid() != 0:
        parser.error('requires root and a finite 0..600 second shared budget')
    installer = apt.Installer(args.budget_seconds)
    recovery = apt._dns.Recovery(installer, True)
    installer.dns = recovery
    apt.establish_owner()
    previous = {sig: signal.signal(sig, installer.cancel) for sig in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP)}
    try:
        installer.log('mechanism_only=true automatic_failover_not_exercised=true')
        eligible = recovery.eligible()
        installer.log(f'mechanism_eligible={eligible} reason={recovery.reason}')
        if not eligible:
            return 1
        if not recovery.claim_lock():
            installer.log('mechanism_refused=exclusive_lock_unavailable')
            return 1
        if not recovery.default_routes():
            installer.log('mechanism_refused=' + recovery.reason)
            return 1
        prefix = recovery.namespace_prefix()
        probe = [sys.executable, str(Path(__file__).with_name('ci-apt-dns.py')), '--probe']
        status, _, valid = installer.run_command('mechanism-nss', prefix + probe)
        if not valid or status != 0 or installer.cancelled:
            return 128 + installer.cancelled if installer.cancelled else 1
        installer.log('mechanism_namespace_nss=passed host_resolver_modified=false')
        recovery.prefix = prefix
        # This experiment selected a namespace explicitly. Do not turn it into
        # an automatic recovery trigger or claim the default resolver failed.
        recovery.enabled = False
        return installer.install(['pkg-config'])
    except (OSError, RuntimeError) as error:
        installer.log('mechanism_failure=' + type(error).__name__)
        return 1
    finally:
        try:
            recovery.close()
        finally:
            for sig, handler in previous.items():
                signal.signal(sig, handler)
            installer.log.flush(min(installer.end, time.monotonic() + 0.1))


if __name__ == '__main__':
    sys.exit(main())
