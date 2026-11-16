"""Run inside standard GDB; observe the original Node process tree, without injection."""
import hashlib
import json
import os
import time

import gdb

MAX_RECORDS = 256
MAX_MAP_BYTES = 1024 * 1024
FAULT_SIGNALS = ('SIGSEGV', 'SIGBUS', 'SIGILL', 'SIGFPE', 'SIGABRT')


class Observation:
    def __init__(self, config):
        self.config = config
        self.root = None
        self.root_exit = None
        self.root_signal = None
        self.root_done = False
        self.targets = {}
        self.rejected_targets = set()
        self.errors = []
        self.count = 0
        descriptor = os.open(config['records'], os.O_WRONLY | os.O_CREAT | os.O_EXCL
                             | os.O_NOFOLLOW | os.O_CLOEXEC, 0o600)
        self.stream = os.fdopen(descriptor, 'w', buffering=1)

    def error(self, reason):
        if reason not in self.errors and len(self.errors) < 8:
            self.errors.append(reason)

    def write(self, event, **fields):
        if self.count >= MAX_RECORDS - 1 and event != 'complete':
            self.error('record_limit')
            return
        try:
            self.stream.write(json.dumps(dict(event=event, observed_monotonic_ns=time.monotonic_ns(),
                                              **fields), sort_keys=True) + '\n')
            self.stream.flush()
            self.count += 1
        except OSError:
            self.error('record_write_failed')

    @staticmethod
    def integer(expression):
        try:
            return int(gdb.parse_and_eval(expression))
        except (gdb.error, ValueError, TypeError):
            return None

    def discover(self, unused=None):
        try:
            for inferior in gdb.inferiors():
                if inferior.pid <= 0:
                    continue
                key = (inferior.num, inferior.pid)
                if inferior.num == 1 and self.root is None:
                    self.root = key
                    self.write('root_started', inferior=key[0], pid=key[1])
                if key in self.targets or key in self.rejected_targets:
                    continue
                try:
                    actual = os.readlink(f'/proc/{inferior.pid}/exe')
                except OSError:
                    continue
                if actual == self.config['executable']:
                    try:
                        with open(f'/proc/{inferior.pid}/exe', 'rb') as stream:
                            actual_digest = hashlib.file_digest(stream, 'sha256').hexdigest()
                    except OSError:
                        self.rejected_targets.add(key)
                        self.error('engine_identity_unavailable')
                        continue
                    if actual_digest != self.config['engine_sha256']:
                        self.rejected_targets.add(key)
                        self.error('engine_identity_mismatch')
                        continue
                    self.targets[key] = False
                    self.write('runtime_started', inferior=key[0], pid=key[1], engine_sha256=actual_digest)
        except (gdb.error, OSError):
            self.error('process_discovery_failed')

    def mappings(self, pid, registers):
        addresses = dict(registers, r13_slot=(registers['r13'] + 8)
                         if registers['r13'] is not None else None)
        matches = {key: None for key in addresses}
        base = None
        pc_in_engine = False
        try:
            with open(f'/proc/{pid}/maps', 'rb') as stream:
                raw = stream.read(MAX_MAP_BYTES + 1)
            if len(raw) > MAX_MAP_BYTES or not raw.endswith(b'\n'):
                return False, matches, None
            for line in raw.decode().splitlines():
                columns = line.split(None, 5)
                low, high = (int(value, 16) for value in columns[0].split('-'))
                offset = int(columns[2], 16)
                engine = len(columns) == 6 and columns[5] == self.config['executable']
                if engine:
                    candidate = low - offset
                    base = candidate if base is None else min(base, candidate)
                    pc_in_engine |= registers['pc'] is not None and low <= registers['pc'] < high
                for label, value in addresses.items():
                    if value is not None and low <= value < high:
                        matches[label] = dict(start=hex(low), end=hex(high), permissions=columns[1])
            offset = registers['pc'] - base if pc_in_engine and base is not None else None
            return True, matches, hex(offset) if offset is not None else None
        except (OSError, UnicodeError, ValueError, IndexError):
            return False, matches, None

    def stopped(self, event):
        self.discover()
        if not isinstance(event, gdb.SignalEvent) or event.stop_signal not in FAULT_SIGNALS:
            return
        try:
            inferior = gdb.selected_inferior()
            key = (inferior.num, inferior.pid)
            if key not in self.targets:
                return
            registers = {name: self.integer('$' + name) for name in ('pc', 'r13', 'rbp', 'rsp')}
            code = self.integer('$_siginfo.si_code')
            # si_addr is meaningful for a synchronous hardware fault, not for
            # raise()/kill() unions such as SI_USER or SI_TKILL.
            fault_address = self.integer('$_siginfo._sifields._sigfault.si_addr') \
                if code is not None and code > 0 and event.stop_signal in ('SIGSEGV', 'SIGBUS', 'SIGILL', 'SIGFPE') else None
            complete, mappings, offset = self.mappings(key[1], registers)
            self.write('signal', inferior=key[0], pid=key[1], signal=event.stop_signal,
                       thread=list(gdb.selected_thread().ptid),
                       si_code=code, si_addr=hex(fault_address) if fault_address is not None else None,
                       registers={name: hex(value) if value is not None else None
                                  for name, value in registers.items()},
                       maps_complete=complete, mappings=mappings, pc_offset=offset)
        except (gdb.error, OSError, ValueError, TypeError):
            self.error('signal_capture_failed')

    def exited(self, event):
        # Fix the terminal state in this callback. Global convenience variables
        # must never be read later and assigned to a different inferior.
        try:
            number = event.inferior.num
            code = getattr(event, 'exit_code', None)
            terminal_signal = self.integer('$_exitsignal') if code is None else None
            if self.root is not None and number == self.root[0]:
                self.root_exit = code
                self.root_signal = terminal_signal
                self.root_done = True
                if code is None:
                    self.error('root_exit_unknown')
                self.write('root_exited', inferior=number, pid=self.root[1], exit_code=code,
                           exit_signal=terminal_signal)
            for key, ended in tuple(self.targets.items()):
                if key[0] == number and not ended:
                    self.targets[key] = True
                    self.write('runtime_exited', inferior=number, pid=key[1],
                               exit_code=code, exit_signal=terminal_signal)
        except (gdb.error, ValueError, TypeError):
            self.error('exit_capture_failed')

    def run(self):
        self.write('session', schema=self.config['schema'], context=self.config['context'],
                   attempt=self.config['attempt'], engine_sha256=self.config['engine_sha256'],
                   gdb_version=gdb.VERSION)
        gdb.events.new_objfile.connect(self.discover)
        gdb.events.cont.connect(self.discover)
        gdb.events.stop.connect(self.stopped)
        gdb.events.exited.connect(self.exited)
        try:
            for command in ('set pagination off', 'set confirm off', 'set startup-with-shell off',
                            'set disable-randomization off', 'set print thread-events off',
                            'set print frame-arguments none', 'set follow-fork-mode parent',
                            'set detach-on-fork off', 'set follow-exec-mode same',
                            'set non-stop off', 'set schedule-multiple on',
                            'handle all nostop noprint pass', 'handle SIGINT nostop noprint pass',
                            'handle ' + ' '.join(FAULT_SIGNALS) + ' stop print pass'):
                gdb.execute(command, to_string=True)
            # --nx normally starts in replay mode, which behaves as off during
            # this ordinary run (we never record/replay). The setter requires
            # a live native target on some versions, so verify without setting.
            if gdb.parameter('scheduler-locking') not in ('off', 'replay'):
                raise gdb.error('scheduler locking must permit normal execution')
            command = 'run'
            for unused in range(4096):
                gdb.execute(command, to_string=True)
                if self.root_done:
                    break
                live = [inferior for inferior in gdb.inferiors()
                        if inferior.pid > 0 and inferior.threads()]
                if not live:
                    self.error('root_exit_missing')
                    break
                # Keep the stopped thread selected so continue delivers its
                # pending signal to the original recipient. Switch only after
                # an exit has left the current inferior without a process.
                current = gdb.selected_inferior()
                if current.pid <= 0 or not current.threads():
                    gdb.execute('inferior ' + str(live[0].num), to_string=True)
                command = 'continue'
            else:
                self.error('stop_limit')
        except gdb.error:
            self.error('debugger_command_failed')
        self.discover()
        self.write('complete', errors=self.errors,
                   open_runtimes=[list(key) for key, ended in self.targets.items() if not ended])
        self.stream.close()
        # Preserve the original root Node status. The outer consumer separately
        # rejects incomplete evidence or any recorded workerd failure/restart.
        status = self.root_exit if self.root_exit is not None else \
            128 + self.root_signal if self.root_signal is not None else 125
        gdb.execute('quit ' + str(status))


with open(os.environ['OASIS7_WORKERD_GDB_CONFIG']) as config_stream:
    Observation(json.load(config_stream)).run()
