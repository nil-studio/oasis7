#!/usr/bin/env python3
"""Linux 临时 CI runner 的有界 apt 准备；调用方通过 sudo 提供清理权限。"""
import argparse
import ctypes
import importlib.util
import math
import os
from pathlib import Path
import queue
import re
import selectors
import signal
import subprocess
import sys
import threading
import time

_dns_spec = importlib.util.spec_from_file_location('ci_apt_dns', Path(__file__).with_name('ci-apt-dns.py'))
_dns = importlib.util.module_from_spec(_dns_spec)
_dns_spec.loader.exec_module(_dns)

MAX_OUTPUT = 1024 * 1024
MAX_ATTEMPTS = 3
OPTIONS = ['-o', 'Acquire::Retries=0', '-o', 'Acquire::http::Timeout=20',
           '-o', 'Acquire::https::Timeout=20', '-o', 'DPkg::Lock::Timeout=20',
           '-o', 'APT::Update::Error-Mode=any']


def transient_update(output):
    """只接受明确的网络错误；未知诊断和混合永久错误拒绝重试。"""
    transient = r'(Temporary failure resolving|Connection timed out|Could not connect to|Connection failed)'
    found = False
    for line in output.splitlines():
        if line.startswith(('E:', 'W:', 'Err:')):
            if line.startswith('Err:'):
                continue  # apt 在下一行输出具体原因，最终 E: 必须再次给出原因。
            if re.search(transient, line) and 'Failed to fetch' in line:
                found = True
            elif line not in (
                    'E: Some index files failed to download. They have been ignored, or old ones used instead.',
                    'W: Some index files failed to download. They have been ignored, or old ones used instead.'):
                return False
    return found


class Log:
    """控制台背压不能阻止期限和清理；只缓冲有限的阶段诊断。"""
    def __init__(self):
        self.items = queue.Queue(maxsize=64)
        self.worker = threading.Thread(target=self.write, daemon=True)
        self.worker.start()

    def write(self):
        while True:
            item = self.items.get()
            try:
                os.write(1, (item + '\n').encode())
            except OSError:
                pass
            finally:
                self.items.task_done()

    def __call__(self, message):
        try:
            self.items.put_nowait('ci-apt ' + message)
        except queue.Full:
            pass

    def flush(self, deadline):
        while self.items.unfinished_tasks and time.monotonic() < deadline:
            time.sleep(0.005)


def descendants():
    # /proc/<pid>/task/<tid>/children requires CONFIG_CHECKPOINT_RESTORE.
    # The standard PPid field also works on kernels without that option.
    parents = {}
    for entry in Path('/proc').iterdir():
        if not entry.name.isdigit():
            continue
        try:
            fields = (entry / 'stat').read_text().rsplit(')', 1)[1].split()
        except (FileNotFoundError, ProcessLookupError):
            continue
        parents.setdefault(int(fields[1]), []).append(int(entry.name))
    result = set()
    pending = parents.get(os.getpid(), []).copy()
    while pending:
        pid = pending.pop()
        if pid not in result:
            result.add(pid)
            pending.extend(parents.get(pid, []))
    return result


def establish_owner():
    if sys.platform != 'linux' or not hasattr(os, 'pidfd_open') or not hasattr(signal, 'pidfd_send_signal'):
        raise RuntimeError('Linux pidfd ownership unavailable')
    if descendants():
        raise RuntimeError('helper must have exclusive child ownership')
    libc = ctypes.CDLL(None, use_errno=True)
    if libc.prctl(36, 1, 0, 0, 0) != 0:  # PR_SET_CHILD_SUBREAPER
        raise OSError(ctypes.get_errno(), 'cannot establish subreaper')
    value = ctypes.c_int()
    if libc.prctl(37, ctypes.byref(value), 0, 0, 0) != 0 or value.value != 1:
        raise RuntimeError('subreaper verification failed')
    # Verify proc access and pidfd support before any package operation.
    Path('/proc/self/stat').read_text()
    fd = os.pidfd_open(os.getpid())
    os.close(fd)


def send_owned(sig):
    for pid in descendants():
        try:
            fd = os.pidfd_open(pid)
        except ProcessLookupError:
            continue
        try:
            # Revalidate ancestry after opening the stable process handle.
            if pid in descendants():
                signal.pidfd_send_signal(fd, sig)
        except ProcessLookupError:
            pass
        finally:
            os.close(fd)


def reap():
    while True:
        try:
            pid, _ = os.waitpid(-1, os.WNOHANG)
        except ChildProcessError:
            return
        if not pid:
            return


class Installer:
    def __init__(self, budget, command=('/usr/bin/apt-get',), backoff=1, hosted_dns_recovery=False):
        self.started = time.monotonic()
        self.end = self.started + budget
        self.reserve = min(2.0, budget / 4)
        self.work_end = self.end - self.reserve
        self.command = list(command)
        self.backoff = backoff
        self.cancelled = 0
        self.log = Log()
        self.dns = _dns.Recovery(self, hosted_dns_recovery)

    def cancel(self, signum, _frame):
        self.cancelled = signum

    def cleanup(self, proc):
        start = time.monotonic()
        term_end = min(self.end - self.reserve / 2, start + self.reserve / 2)
        # poll() owns the leader's wait status before reaping adopted children.
        while True:
            proc.poll()
            if proc.returncode is not None:
                reap()
            if not descendants():
                self.log(f'cleanup=complete elapsed={time.monotonic() - start:.3f}s')
                return True
            send_owned(signal.SIGTERM if time.monotonic() < term_end else signal.SIGKILL)
            if time.monotonic() >= self.end:
                self.log('cleanup=incomplete budget_exhausted=true')
                return False
            time.sleep(min(0.01, max(0, self.end - time.monotonic())))

    def run(self, stage, args, attempt=1):
        return self.run_command(stage, self.dns.prefix + self.command + OPTIONS + args, attempt)

    def run_command(self, stage, command, attempt=1, report_output=True):
        start = time.monotonic()
        if self.cancelled or start >= self.work_end:
            return None, '', False
        self.log(f'stage={stage} attempt={attempt} event=start elapsed={start - self.started:.3f}s')
        env = dict(os.environ, DEBIAN_FRONTEND='noninteractive', LC_ALL='C',
                   DEBCONF_NONINTERACTIVE_SEEN='true', APT_LISTCHANGES_FRONTEND='none')
        proc = subprocess.Popen(command, stdin=subprocess.DEVNULL,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                start_new_session=True, env=env)
        output = bytearray()
        overflow = False
        status = None
        reason = 'exit'
        try:
            with selectors.DefaultSelector() as selector:
                os.set_blocking(proc.stdout.fileno(), False)
                selector.register(proc.stdout, selectors.EVENT_READ)
                while True:
                    if self.cancelled or time.monotonic() >= self.work_end:
                        reason = 'cancelled' if self.cancelled else 'timeout'
                        break
                    for key, _ in selector.select(min(0.02, max(0, self.work_end - time.monotonic()))):
                        data = os.read(key.fd, 65536)
                        if not data:
                            selector.unregister(key.fileobj)
                        elif len(output) + len(data) <= MAX_OUTPUT:
                            output.extend(data)
                        else:
                            overflow = True
                    status = proc.poll()
                    if status is not None:
                        # poll() has preserved the leader's status. Reap adopted
                        # zombies before deciding whether any descendants remain live.
                        reap()
                        # A successful leader with live descendants is not a completed install.
                        if descendants():
                            reason = 'leftover_descendants'
                            break
                        if not selector.get_map():
                            break
        finally:
            clean = self.cleanup(proc)
            proc.stdout.close()
        text = output.decode('utf-8', errors='replace')
        self.log(f'stage={stage} attempt={attempt} exit={status} reason={reason} elapsed={time.monotonic() - start:.3f}s output_truncated={overflow}')
        if report_output and (status != 0 or reason != 'exit'):
            self.log('diagnostic=' + repr(text[-4096:]))
        valid = clean and reason == 'exit' and not overflow and time.monotonic() < self.end
        return status, text, valid

    def install(self, packages):
        establish_owner()
        previous = {s: signal.signal(s, self.cancel) for s in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP)}
        try:
            if self.dns.enabled:
                eligible = self.dns.eligible()
                self.log(f'dns_recovery_opt_in=true eligible={eligible} reason={self.dns.reason}')
            for attempt in range(1, MAX_ATTEMPTS + 1):
                status, output, valid = self.run('update', ['update'], attempt=attempt)
                if status == 0 and valid:
                    break
                if not (valid and status == 100 and transient_update(output) and attempt < MAX_ATTEMPTS):
                    return 128 + self.cancelled if self.cancelled else 1
                self.dns.try_enable(output)
                delay = self.backoff * attempt
                self.log(f'stage=update retry={attempt} backoff={delay:.3f}s')
                until = time.monotonic() + delay
                while time.monotonic() < until:
                    if self.cancelled or time.monotonic() >= self.work_end:
                        return 128 + self.cancelled if self.cancelled else 1
                    time.sleep(min(0.02, max(0, until - time.monotonic())))
            status, _, valid = self.run('install', ['install', '-y', '--', *packages])
            success = status == 0 and valid and not self.cancelled
            self.log(f'dependencies_ready={str(success).lower()} elapsed={time.monotonic() - self.started:.3f}s')
            return 0 if success else (128 + self.cancelled if self.cancelled else 1)
        finally:
            try:
                self.dns.close()
            finally:
                for sig, handler in previous.items():
                    signal.signal(sig, handler)
                self.log.flush(min(self.end, time.monotonic() + 0.1))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--budget-seconds', type=float, required=True)
    parser.add_argument('--hosted-dns-recovery', action='store_true',
                        help='opt in only for verified disposable GitHub-hosted Ubuntu runners')
    parser.add_argument('packages', nargs='+')
    args = parser.parse_args()
    if not math.isfinite(args.budget_seconds) or args.budget_seconds <= 0:
        parser.error('budget must be finite and positive')
    if os.geteuid() != 0:
        parser.error('run with sudo -n /usr/bin/python3 on a disposable Linux CI runner')
    installer = Installer(args.budget_seconds, hosted_dns_recovery=args.hosted_dns_recovery)
    try:
        return installer.install(args.packages)
    except (OSError, RuntimeError) as error:
        installer.log(f'failure={type(error).__name__}: {error}')
        installer.log.flush(min(installer.end, time.monotonic() + 0.1))
        return 1


if __name__ == '__main__':
    sys.exit(main())
