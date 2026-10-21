#!/usr/bin/env python3
"""Explicit, conservative Git resource cleanup; dry run unless --execute."""
import argparse
import json
import hashlib
import os
import pathlib
import subprocess
import ctypes
import importlib.util
import io
import marshal
import types
import errno
import re
import sys
import time
from urllib.parse import urlsplit

class CleanupError(RuntimeError):
    pass

def _run(args, check=True, timeout=10):
    try:
        result = subprocess.run(args, text=True, capture_output=True, timeout=timeout)
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise CleanupError(str(exc)) from exc
    if check and result.returncode:
        raise CleanupError(result.stderr.strip() or result.stdout.strip())
    return result

def git(repo, *args):
    return _run(['git', '-C', str(repo), *args]).stdout.strip()

def _native_process_identity(pid):
    if sys.platform.startswith('linux'):
        raw = pathlib.Path(f'/proc/{pid}/stat').read_text()
        fields = raw[raw.rindex(')') + 2:].split()
        return (pid, int(fields[19]), fields[0])
    if sys.platform == 'darwin':
        class BSDInfo(ctypes.Structure):
            _fields_ = [('ids', ctypes.c_uint32 * 12), ('comm', ctypes.c_char * 16),
                        ('name', ctypes.c_char * 32), ('tail', ctypes.c_uint32 * 6),
                        ('sec', ctypes.c_uint64), ('usec', ctypes.c_uint64)]
        lib = ctypes.CDLL('/usr/lib/libproc.dylib', use_errno=True)
        info = BSDInfo()
        result = lib.proc_pidinfo(pid, 3, 0, ctypes.byref(info), ctypes.sizeof(info))
        if result != ctypes.sizeof(info) or info.ids[3] != pid or not info.sec:
            raise ValueError('short or invalid libproc identity')
        return (pid, (info.sec, info.usec), str(info.ids[1]))
    raise ValueError('unsupported native process identity')


def process_identity(pid, deadline=None):
    """Native start identity; only ESRCH proves an unreadable process gone."""
    for attempt in range(3):
        if deadline is not None and time.monotonic() >= deadline:
            raise CleanupError('process inspection budget exhausted')
        try:
            return _native_process_identity(pid)
        except (OSError, ValueError, IndexError):
            try:
                os.kill(pid, 0)
            except OSError as exc:
                if exc.errno == errno.ESRCH:
                    return None
                raise CleanupError(f'PID {pid}: identity unreadable ({exc.strerror})') from exc
            if attempt < 2:
                # An exiting process can briefly answer kill(0) after native
                # identity has vanished. Re-read native start identity so PID
                # reuse produces fresh evidence; never infer exit from name.
                remaining = .01 if deadline is None else min(.01, max(0, deadline - time.monotonic()))
                time.sleep(remaining)
    raise CleanupError(f'PID {pid}: alive but native identity unreadable')


def snapshot_processes(deadline):
    result = _run(['ps', '-axo', 'pid=,uid='], timeout=min(2, max(.01, deadline-time.monotonic())))
    identities = {}
    seen = set()
    for line in result.stdout.splitlines():
        if time.monotonic() >= deadline:
            raise CleanupError('process inspection budget exhausted')
        try:
            pid_text, uid_text = line.split()
            pid, uid = int(pid_text), int(uid_text)
        except ValueError as exc:
            raise CleanupError('malformed ps PID') from exc
        # Darwin ps renders uid_t through signed 32-bit display (for
        # example nobody=-2). Normalize that native representation only.
        if uid < 0:
            if sys.platform != 'darwin' or uid < -(2 ** 31):
                raise CleanupError('invalid ps UID')
            uid += 2 ** 32
        if uid >= 2 ** 32:
            raise CleanupError('invalid ps UID')
        if pid < 0 or pid in seen:
            raise CleanupError('invalid or duplicate ps identity')
        seen.add(pid)
        if uid != os.getuid():
            continue
        identity = process_identity(pid, deadline)
        if identity is not None:
            if pid in identities:
                raise CleanupError('duplicate ps PID')
            identities[pid] = identity
    if not identities:
        raise CleanupError('empty process census')
    return identities


def read_open_files(pids, path, deadline, result=None):
    if result is None:
        result = _run(['lsof', '-n', '-P', '-F', 'pfnt', '-p', ','.join(map(str, pids))],
                  check=False, timeout=min(10, max(.01, deadline-time.monotonic())))
    covered, busy = set(), set()
    pid, fd, kind, name = None, None, None, None
    nonfiles = {'PIPE', 'NPOLICY', 'NEXUS', 'KQUEUE', 'unix', 'IPv4', 'IPv6', 'systm', 'CHAN', 'sock'}
    target = path.resolve()
    def finish():
        if fd is None:
            return
        if kind is None or (name is None and kind not in nonfiles):
            raise CleanupError(f'PID {pid}: incomplete descriptor')
        if kind in nonfiles:
            return
        # macOS POSIX semaphore names live in the sem_open namespace,
        # not the filesystem. Require the native lsof type and a single
        # namespace component; unknown forms remain ambiguous.
        if sys.platform == 'darwin' and kind == 'PSXSEM' and fd.isdecimal() and name and re.fullmatch(r'/[^/]+', name):
            return
        if kind not in {'REG', 'DIR', 'CHR', 'BLK', 'FIFO', 'LINK'}:
            raise CleanupError(f'PID {pid}: unknown descriptor type {kind}')
        # Linux lsof names anonymous pipe descriptors as FIFO/pipe. A
        # filename whitelist would hide named FIFOs: verify the exact native
        # fd symlink, which uses pipe:[inode], before treating it as non-file.
        if sys.platform.startswith('linux') and kind == 'FIFO' and name == 'pipe' and fd.isdecimal():
            try:
                native = os.readlink(f'/proc/{pid}/fd/{fd}')
            except OSError as exc:
                raise CleanupError(f'PID {pid}: anonymous pipe identity unreadable') from exc
            if re.fullmatch(r'pipe:\[[0-9]+\]', native):
                return
            raise CleanupError(f'PID {pid}: FIFO/pipe native identity mismatch')
        if not name or not name.startswith('/'):
            raise CleanupError(f'PID {pid}: ambiguous file path')
        value = re.sub(r' \((deleted|revoked)\)$', '', name)
        try:
            pathlib.Path(value).resolve().relative_to(target)
            busy.add(pid)
        except ValueError:
            pass
        if fd == 'cwd':
            covered.add(pid)
    for line in result.stdout.splitlines():
        if not line:
            continue
        field, value = line[0], line[1:]
        if field == 'p':
            finish()
            try:
                pid = int(value)
            except ValueError as exc:
                raise CleanupError('invalid lsof PID') from exc
            if pid not in pids:
                raise CleanupError('unexpected lsof PID')
            fd = kind = name = None
        elif field == 'f':
            finish()
            if pid is None or not value:
                raise CleanupError('invalid lsof descriptor')
            fd, kind, name = value, None, None
        elif field == 't' and fd is not None and kind is None and value:
            kind = value
        elif field == 'n' and fd is not None and name is None:
            name = value
        else:
            raise CleanupError('unknown or malformed lsof field')
    finish()
    # lsof exit 1 can mean no surviving requested PIDs; identity reconciliation
    # below decides coverage. Other errors and warnings remain unknown.
    if result.returncode not in (0, 1) or result.stderr.strip():
        raise CleanupError('lsof failed or reported incomplete visibility: ' + result.stderr.strip())
    return covered, busy


def visible_target_holders(path, deadline):
    """Observe all-UID visible holders; absence is not global visibility proof."""
    result = _run(['lsof', '-n', '-P', '-F', 'pfnt', '+D', str(path.resolve())], check=False,
                  timeout=min(10, max(.01, deadline - time.monotonic())))
    if result.returncode not in (0, 1) or result.stderr.strip():
        raise CleanupError('target-use visibility unavailable: ' + result.stderr.strip())
    holders = set()
    for line in result.stdout.splitlines():
        if line.startswith('p'):
            if not re.fullmatch(r'p[1-9][0-9]*', line):
                raise CleanupError('malformed target-use PID')
            holders.add(int(line[1:]))
    _, busy = read_open_files(holders, path, deadline, result=result)
    if holders != busy:
        raise CleanupError('target-use descriptor evidence incomplete or outside target')
    for pid in busy:
        # Gone collectors/holders are harmless; denied target-related native
        # identity is unknown, never an exemption for foreign users.
        if process_identity(pid, deadline) is not None:
            return pid
    return None


def inspect_process_use(path):
    if sys.platform not in ('linux', 'darwin'):
        return {'state': 'unknown', 'reason': 'unsupported process inspection platform'}
    deadline = time.monotonic() + 30
    try:
        current = {pid: value[:2] for pid, value in snapshot_processes(deadline).items()}
        evidence = {}
        for _ in range(2):
            holder = visible_target_holders(path, deadline)
            if holder is not None:
                return {'state': 'busy', 'reason': f'visible PID {holder} holds target cwd/file'}
            pending = {pid: identity for pid, identity in current.items() if evidence.get(pid) != identity}
            if pending:
                covered, busy = read_open_files(pending, path, deadline)
                after = {pid: value[:2] for pid, value in snapshot_processes(deadline).items()}
                for pid in busy:
                    if pid in pending and after.get(pid) == pending[pid]:
                        return {'state': 'busy', 'reason': f'PID {pid} holds target cwd/file'}
                for pid in covered:
                    if pid in pending and after.get(pid) == pending[pid]:
                        evidence[pid] = pending[pid]
                current = after
            if time.monotonic() >= deadline:
                raise CleanupError('process inspection budget exhausted')
            if all(evidence.get(pid) == identity for pid, identity in current.items()):
                return {'state': 'clear_observed', 'reason': 'no use observed among current-user processes or visible all-user target holders'}
        missing = [str(pid) for pid, identity in current.items() if evidence.get(pid) != identity]
        raise CleanupError('uncovered live process identities: ' + ', '.join(missing))
    except CleanupError as exc:
        return {'state': 'unknown', 'reason': str(exc)}


def _process_mentions_path(path):
    result = inspect_process_use(path)
    if result['state'] == 'unknown':
        raise CleanupError(result['reason'])
    return result['state'] == 'busy'


def worktrees(repo):
    records = []
    for row in git(repo, 'worktree', 'list', '--porcelain', '-z').split('\0\0'):
        fields = {}
        for entry in row.split('\0'):
            if entry:
                key, _, value = entry.partition(' ')
                fields[key] = value
        if fields:
            records.append(fields)
    return records


def oid(repo, ref):
    return git(repo, 'rev-parse', '--verify', ref + '^{commit}')


def ancestor(repo, older, newer):
    result = _run(['git', '-C', str(repo), 'merge-base', '--is-ancestor', older, newer], check=False)
    if result.returncode not in (0, 1):
        raise CleanupError('cannot verify ancestry')
    return result.returncode == 0


def reconstructable_pycache(path, item):
    """Accept only exact current-ABI compiled content of a tracked local source."""
    relative = pathlib.PurePosixPath(item)
    match = re.fullmatch(r'(.+)\.' + re.escape(sys.implementation.cache_tag) + r'(?:\.opt-([012]))?\.pyc', relative.name)
    if relative.parent.name != '__pycache__' or not match:
        return False
    source_relative = relative.parent.parent / (match.group(1) + '.py')
    source, cached = path / source_relative, path / relative
    for relative_file in (relative, source_relative):
        current = path
        for component in relative_file.parts:
            current = current / component
            if current.is_symlink():
                return False
    tracked = _run(['git', '-C', str(path), 'ls-files', '--error-unmatch', '--', str(source_relative)], check=False)
    if tracked.returncode or not source.is_file() or not cached.is_file():
        return False
    if source.stat().st_size > 16 * 1024 * 1024 or cached.stat().st_size > 16 * 1024 * 1024:
        return False
    raw, source_bytes = cached.read_bytes(), source.read_bytes()
    if len(raw) < 16 or raw[:4] != importlib.util.MAGIC_NUMBER:
        return False
    flags = int.from_bytes(raw[4:8], 'little')
    if flags not in (0, 1, 3):
        return False
    if flags and raw[8:16] != importlib.util.source_hash(source_bytes):
        return False
    if not flags and int.from_bytes(raw[12:16], 'little') != len(source_bytes):
        return False
    try:
        payload = io.BytesIO(raw[16:])
        code = marshal.load(payload)
        expected = compile(source_bytes, str(source), 'exec', dont_inherit=True,
                           optimize=int(match.group(2) or '0'))
        # Compare code and recursively require source filename metadata too;
        # caches with extra embedded material are not disposable. No cached
        # bytecode is executed.
        def same_source_metadata(value):
            filename = pathlib.Path(value.co_filename)
            return filename.is_absolute() and filename.resolve() == source.resolve() and all(
                same_source_metadata(item) for item in value.co_consts if isinstance(item, types.CodeType))
        return isinstance(code, types.CodeType) and not payload.read() and same_source_metadata(code) and code == expected
    except (ValueError, EOFError, TypeError, SyntaxError, RecursionError, OSError, RuntimeError):
        return False


def user_material(repo, path, records):
    if git(path, 'status', '--porcelain', '--untracked-files=all'):
        raise CleanupError('uncommitted or untracked work retained')
    canonical = pathlib.Path(records[0]['worktree']).resolve()
    cache = canonical.parent / '.oasis7-cache' / 'cargo-target'
    ignored = git(path, 'ls-files', '--others', '--ignored', '--exclude-standard', '-z').split('\0')
    for item in filter(None, ignored):
        if reconstructable_pycache(path, item):
            continue
        if item == 'config.toml':
            source, copy = canonical / item, path / item
            if canonical == path or source.is_symlink() or copy.is_symlink() or not source.is_file() or source.read_bytes() != copy.read_bytes():
                raise CleanupError('modified or unverifiable ignored config retained')
        elif item.rstrip('/') == 'target' and (path / 'target').is_symlink():
            destination = (path / 'target').resolve()
            try:
                destination.relative_to(cache.resolve())
            except ValueError as exc:
                raise CleanupError('target does not point into known external build cache') from exc
            if not destination.is_dir() or destination == cache.resolve() or destination.is_relative_to(path):
                raise CleanupError('target cache identity unclear')
        else:
            raise CleanupError('unknown ignored user material retained: ' + item)


def _module_git(gitdir, *args):
    result = _run(['git', '--git-dir', str(gitdir), '--work-tree', str(gitdir), *args])
    if result.stderr.strip():
        raise CleanupError('submodule metadata inspection warned: ' + result.stderr.strip())
    return result.stdout.strip()


def _module_cache_layout(root, known):
    """Retain cached repositories that no current gitlink can account for."""
    if not root.exists():
        return
    if root.is_symlink() or not root.is_dir():
        raise CleanupError('ambiguous submodule cache retained')
    for child in root.iterdir():
        if child.is_symlink():
            raise CleanupError('symlinked submodule Git data retained')
        if child in known:
            continue
        if child.is_dir() and any(child in target.parents for target in known):
            _module_cache_layout(child, known)
        else:
            raise CleanupError('unknown submodule cache retained: ' + str(child))


def _module_config(gitdir):
    """Accept generated clone configuration, retaining repository-local overrides."""
    raw = _run(['git', 'config', '--file', str(gitdir / 'config'), '-z', '--list']).stdout
    branches = set(_module_git(gitdir, 'for-each-ref', '--format=%(refname:strip=2)',
                               'refs/heads').splitlines())
    declarations = _run(['git', '--git-dir', str(gitdir), '--work-tree', str(gitdir),
                         'config', '-z', '--blob', 'HEAD:.gitmodules', '--get-regexp',
                         r'^submodule\..*\.path$'], check=False)
    names = set()
    if declarations.returncode == 0:
        names = {row.split('\n', 1)[0][len('submodule.'):-len('.path')]
                 for row in filter(None, declarations.stdout.split('\0'))}
    bools = {'core.filemode', 'core.bare', 'core.logallrefupdates', 'core.ignorecase',
             'core.precomposeunicode', 'core.symlinks', 'remote.origin.promisor'}
    seen = set()
    for row in filter(None, raw.split('\0')):
        key, _, value = row.partition('\n')
        allowed = False
        if key in bools:
            allowed = value in {'true', 'false'}
        elif key == 'core.repositoryformatversion':
            allowed = value in {'0', '1'}
        elif key == 'extensions.objectformat':
            allowed = value in {'sha1', 'sha256'}
        elif key == 'core.worktree':
            allowed = bool(value)
        elif key in {'remote.origin.url', 'remote.origin.partialclonefilter'}:
            allowed = bool(value) and '\n' not in value
        elif key == 'remote.origin.fetch':
            allowed = re.fullmatch(r'\+?refs/heads/([^:]+):refs/remotes/origin/\1', value) is not None
        else:
            for name in branches:
                if key == f'branch.{name}.remote' and value == 'origin':
                    allowed = True
                if key == f'branch.{name}.merge' and value == f'refs/heads/{name}':
                    allowed = True
            for name in names:
                if key == f'submodule.{name}.active' and value in {'true', 'false'}:
                    allowed = True
                if key == f'submodule.{name}.url' and value and '\n' not in value:
                    allowed = True
        if not allowed or key in seen:
            raise CleanupError('custom or unknown submodule configuration retained: ' + key)
        seen.add(key)


def _module_material(gitdir):
    """Keep nonstandard metadata, pending operations and unpublished objects."""
    _module_config(gitdir)
    cached = _run(['git', '--git-dir', str(gitdir), '--work-tree', str(gitdir),
                   'diff-index', '--cached', '--quiet', 'HEAD', '--'], check=False)
    if cached.returncode:
        raise CleanupError('staged or unreadable submodule index retained')
    templates = pathlib.Path(_run(['git', '--exec-path']).stdout.strip()).parent.parent / 'share/git-core/templates'
    defaults = {'description', 'info/exclude'}
    metadata = {'HEAD', 'config', 'index', 'packed-refs', 'shallow', 'FETCH_HEAD', 'ORIG_HEAD'}
    object_file = re.compile(r'objects/(?:[0-9a-f]{2}/[0-9a-f]{38,62}|pack/pack-[0-9a-f]{40,64}\.(?:pack|idx|rev|bitmap|mtimes|promisor))')
    for directory, dirs, files in os.walk(gitdir, followlinks=False):
        parent = pathlib.Path(directory)
        # Each nested modules directory is independently checked against gitlinks.
        if parent == gitdir:
            dirs[:] = [name for name in dirs if name != 'modules']
        for name in dirs + files:
            target = parent / name
            if target.is_symlink():
                raise CleanupError('symlinked submodule metadata retained')
            relative = target.relative_to(gitdir).as_posix()
            if target.is_dir():
                if relative == 'branches' and not any(target.iterdir()):
                    continue
                if (relative in {'info', 'hooks', 'refs', 'logs', 'objects', 'objects/info', 'objects/pack'}
                        or relative.startswith(('refs/', 'logs/'))
                        or re.fullmatch(r'objects/[0-9a-f]{2}', relative)):
                    continue
                raise CleanupError('unknown submodule Git directory retained: ' + relative)
            if relative in defaults or relative.startswith('hooks/'):
                sample = templates / relative
                if (not sample.is_file() or sample.is_symlink() or
                        target.read_bytes() != sample.read_bytes()):
                    raise CleanupError('custom submodule template/hook retained: ' + relative)
            elif relative == 'COMMIT_EDITMSG':
                if target.read_bytes().strip() != _module_git(gitdir, 'log', '-1', '--format=%B').encode():
                    raise CleanupError('uncommitted submodule message retained')
            elif relative in metadata or object_file.fullmatch(relative):
                continue
            elif relative.startswith(('refs/', 'logs/')):
                # Commit/ref validation below still checks these histories.
                if relative.endswith('.lock'):
                    raise CleanupError('submodule reference lock retained')
                raw = target.read_bytes()
                if relative.startswith('logs/'):
                    entry = rb'[0-9a-f]{40,64} [0-9a-f]{40,64} [^\n<>]* <[^\n<>]*> [0-9]+ [+-][0-9]{4}(?:\t[^\n]*)?\n'
                    if any(not re.fullmatch(entry, line) for line in raw.splitlines(keepends=True)):
                        raise CleanupError('unrecognized submodule reflog material retained')
                elif not re.fullmatch(rb'(?:[0-9a-f]{40,64}|ref: refs/[^\s]+)\n?', raw):
                    raise CleanupError('unrecognized submodule reference material retained')
            else:
                raise CleanupError('unknown submodule Git material retained: ' + relative)
    # Missing objects, corrupt stores and unreachable local work all stop removal.
    fsck = _run(['git', '--git-dir', str(gitdir), '--work-tree', str(gitdir),
                 'fsck', '--full', '--no-reflogs', '--unreachable'], check=False, timeout=30)
    if fsck.returncode or fsck.stdout.strip() or fsck.stderr.strip():
        raise CleanupError('unreachable or unverifiable submodule objects retained')


def inspect_submodules(path, allow_submodules=False):
    """Check active and dormant module data before the single-force exception."""
    root_gitdir = pathlib.Path(git(path, 'rev-parse', '--absolute-git-dir')).resolve()
    result = []

    def visit(gitdir, worktree):
        links = {}
        for row in _module_git(gitdir, 'ls-tree', '-rz', 'HEAD').split('\0'):
            if row:
                info, name = row.split('\t', 1)
                mode, _, commit = info.split()
                if mode == '160000':
                    links[name] = commit
        cache = gitdir / 'modules'
        if not links and not cache.exists():
            return
        if not allow_submodules:
            raise CleanupError('submodule worktree retained; use --allow-submodules after verification')
        names = {}
        config = _run(['git', '--git-dir', str(gitdir), 'config', '-z', '--blob',
                       'HEAD:.gitmodules', '--get-regexp', r'^submodule\..*\.path$'], check=False)
        if config.returncode not in (0, 1):
            raise CleanupError('submodule declarations unavailable')
        for row in filter(None, config.stdout.split('\0')):
            key, value = row.split('\n', 1)
            if value in names:
                raise CleanupError('duplicate submodule path retained')
            names[value] = key[len('submodule.'):-len('.path')]
        known = set()
        for name, commit in sorted(links.items()):
            module_name = names.get(name)
            if module_name is None:
                raise CleanupError('gitlink has no submodule declaration')
            for value in (name, module_name):
                relative = pathlib.PurePosixPath(value)
                if (not value or relative.is_absolute() or
                        any(part in {'.', '..', '.git'} for part in relative.parts)):
                    raise CleanupError('ambiguous submodule path/name retained')
            child = worktree / name
            current = worktree
            for component in pathlib.PurePosixPath(name).parts:
                current = current / component
                if current.is_symlink():
                    raise CleanupError('symlinked submodule worktree retained')
            backend = cache / module_name
            # Do not follow a symlink in the repository-private module hierarchy.
            current = cache
            for component in pathlib.PurePosixPath(module_name).parts:
                current = current / component
                if current.is_symlink():
                    raise CleanupError('symlinked submodule Git data retained')
            marker = child / '.git'
            initialized = marker.exists() or marker.is_symlink()
            if initialized:
                actual = pathlib.Path(git(child, 'rev-parse', '--absolute-git-dir')).resolve()
                if marker.is_symlink() or actual != backend or not backend.is_dir():
                    raise CleanupError('external or missing submodule Git data retained')
                if git(child, 'status', '--porcelain', '--untracked-files=all', '--ignored',
                       '--ignore-submodules=none'):
                    raise CleanupError('submodule user material retained: ' + str(child))
            elif child.exists() and (not child.is_dir() or any(child.iterdir())):
                raise CleanupError('uninitialized submodule user material retained')
            state = {'path': str(child), 'gitlink': commit, 'gitdir': None}
            if backend.exists():
                if not backend.is_dir():
                    raise CleanupError('invalid submodule Git data retained')
                known.add(backend)
                common = pathlib.Path(_module_git(backend, 'rev-parse', '--path-format=absolute',
                                                  '--git-common-dir')).resolve()
                if common != backend:
                    raise CleanupError('shared submodule Git data retained')
                checkouts = _module_git(backend, 'worktree', 'list', '--porcelain', '-z')
                roots = [row[len('worktree '):] for row in checkouts.split('\0') if row.startswith('worktree ')]
                if len(roots) != 1:
                    raise CleanupError('submodule linked worktrees retained')
                configured = _run(['git', '--git-dir', str(backend), '--work-tree', str(backend),
                                   'config', '--get', 'core.worktree'], check=False)
                if configured.returncode not in (0, 1) or (initialized and configured.returncode != 0):
                    raise CleanupError('submodule working directory identity unavailable')
                if configured.returncode == 0 and (backend / configured.stdout.strip()).resolve() != child:
                    raise CleanupError('external submodule working directory retained')
                if _module_git(backend, 'rev-parse', '--verify', 'HEAD^{commit}') != commit:
                    raise CleanupError('submodule HEAD does not match parent gitlink')
                if _module_git(backend, 'rev-list', '--reflog', '--all', 'HEAD', '--not', '--remotes'):
                    raise CleanupError('unpublished submodule history retained')
                _module_material(backend)
                state['gitdir'] = str(backend)
                visit(backend, child)
            elif initialized:
                raise CleanupError('missing submodule Git data retained')
            result.append(state)
        _module_cache_layout(cache, known)

    visit(root_gitdir, path)
    return sorted(result, key=lambda item: item['path'])


def github_repository(remote):
    """Resolve only an exact GitHub host and owner/repository origin."""
    if '://' in remote:
        parsed = urlsplit(remote)
        if parsed.scheme not in {'https', 'http', 'ssh', 'git'} or parsed.hostname != 'github.com' or parsed.query or parsed.fragment:
            raise CleanupError('origin repository identity unavailable for PR proof')
        repository = parsed.path.removeprefix('/')
    else:
        match = re.fullmatch(r'(?:[^@/:]+@)?github\.com:([^?#]+)', remote)
        if not match:
            raise CleanupError('origin repository identity unavailable for PR proof')
        repository = match.group(1)
    repository = repository.removesuffix('.git')
    if not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', repository) or any(part in {'.', '..'} for part in repository.split('/')):
        raise CleanupError('origin repository identity unavailable for PR proof')
    return repository


def delivery(repo, expected, base_ref, pr):
    if base_ref not in {'refs/remotes/origin/main', 'refs/heads/main'}:
        raise CleanupError('invalid main base ref')
    try:
        base = oid(repo, base_ref)
        origin = oid(repo, 'refs/remotes/origin/main')
    except CleanupError as exc:
        raise CleanupError('main baseline missing; fetch normally before cleanup') from exc
    if base_ref == 'refs/heads/main' and not ancestor(repo, base, origin):
        raise CleanupError('local main has not been delivered to origin/main')
    if not ancestor(repo, expected, base):
        if pr is None:
            raise CleanupError('no main ancestry or explicit merged PR proof')
        remote = git(repo, 'remote', 'get-url', 'origin')
        slug = github_repository(remote)
        # GitHub may redirect an old origin after a repository transfer/rename.
        repository = json.loads(_run(['gh', 'api', f'repos/{slug}']).stdout)
        canonical = repository.get('full_name')
        if not isinstance(canonical, str):
            raise CleanupError('canonical origin repository identity unavailable')
        slug = github_repository('https://github.com/' + canonical)
        data = json.loads(_run(['gh', 'api', f'repos/{slug}/pulls/{pr}']).stdout)
        merged = data.get('merge_commit_sha')
        if (data.get('merged') is not True or data.get('head', {}).get('sha') != expected
                or data.get('base', {}).get('ref') != 'main'
                or data.get('base', {}).get('repo', {}).get('full_name', '').lower() != slug.lower()
                or not isinstance(merged, str) or not ancestor(repo, merged, base)):
            raise CleanupError('PR merged/head/base/repository proof mismatch')
    return base, origin


def inspect(repo, path, branch, expected, base_ref='refs/remotes/origin/main', pr=None, allow_submodules=False):
    repo, path = repo.resolve(), path.resolve()
    if branch in {'main', 'master'}:
        raise CleanupError('primary branch retained')
    records = worktrees(repo)
    if not records or path == pathlib.Path(records[0]['worktree']).resolve() or not path.is_dir():
        raise CleanupError('target must be an existing separate worktree')
    wanted = next((r for r in records if r.get('worktree') == str(path)), None)
    if not wanted or wanted.get('branch') != 'refs/heads/' + branch or wanted.get('HEAD') != expected or oid(repo, 'refs/heads/' + branch) != expected or oid(path, 'HEAD') != expected:
        raise CleanupError('worktree path, branch, or expected HEAD mismatch')
    if 'locked' in wanted or 'prunable' in wanted:
        raise CleanupError('locked or ambiguous worktree retained')
    if pathlib.Path(git(path, 'rev-parse', '--path-format=absolute', '--git-common-dir')).resolve() != pathlib.Path(git(repo, 'rev-parse', '--path-format=absolute', '--git-common-dir')).resolve():
        raise CleanupError('repository identity mismatch')
    user_material(repo, path, records)
    modules = inspect_submodules(path, allow_submodules)
    base, origin = delivery(repo, expected, base_ref, pr)
    upstream = _run(['git', '-C', str(path), 'rev-parse', '--symbolic-full-name', '@{upstream}'], check=False)
    ref = upstream.stdout.strip()
    if upstream.returncode == 0 and ref not in {'refs/remotes/origin/main', 'refs/heads/main'}:
        tip = _run(['git', '-C', str(repo), 'rev-parse', '--verify', ref], check=False)
        if tip.returncode == 0 and tip.stdout.strip() != expected:
            raise CleanupError('independent remote topic changed; retained')
    if _process_mentions_path(path):
        raise CleanupError('worktree is in use')
    for module in modules:
        if module['gitdir'] and _process_mentions_path(pathlib.Path(module['gitdir'])):
            raise CleanupError('submodule Git data is in use')
    return {'worktree': str(path), 'branch': branch, 'head': expected, 'base_ref': base_ref,
            'submodules': modules, 'submodule_gitdirs': [m['gitdir'] for m in modules if m['gitdir']],
            'base_oid': base, 'origin_main_oid': origin, 'process_state': 'clear_observed',
            'process_scope': 'current_user_and_visible_all_user_target_holders'}


def _removal_identity(repo, path, branch, expected, base_ref, report):
    # Recheck identity and materials immediately after the potentially slow process scan.
    records = worktrees(repo)
    wanted = next((r for r in records if r.get('worktree') == str(path.resolve())), {})
    if wanted.get('HEAD') != expected or wanted.get('branch') != 'refs/heads/' + branch or 'locked' in wanted or 'prunable' in wanted or oid(path, 'HEAD') != expected or oid(repo, 'refs/heads/' + branch) != expected or oid(repo, base_ref) != report['base_oid']:
        raise CleanupError('resource identity changed before deletion')
    if pathlib.Path(git(path, 'rev-parse', '--path-format=absolute', '--git-common-dir')).resolve() != pathlib.Path(git(repo, 'rev-parse', '--path-format=absolute', '--git-common-dir')).resolve():
        raise CleanupError('repository identity changed before deletion')
    if base_ref == 'refs/heads/main' and oid(repo, 'refs/remotes/origin/main') != report['origin_main_oid']:
        raise CleanupError('origin main changed before deletion')
    return records


def execute(repo, path, branch, expected, base_ref, pr, report, allow_submodules=False):
    repo, path = repo.resolve(), path.resolve()
    fresh = inspect(repo, path, branch, expected, base_ref, pr, allow_submodules)
    if (fresh['base_oid'] != report['base_oid'] or fresh['origin_main_oid'] != report['origin_main_oid']
            or fresh['submodules'] != report['submodules']):
        raise CleanupError('base changed before deletion; retained')
    records = _removal_identity(repo, path, branch, expected, base_ref, report)
    user_material(repo, path, records)
    modules = inspect_submodules(path, allow_submodules)
    if modules != fresh['submodules']:
        raise CleanupError('submodule identity changed before deletion')
    if modules:
        # No force for deinit: any Git-detected changes still stop cleanup.
        git(path, 'submodule', 'deinit', '--all')
        records = _removal_identity(repo, path, branch, expected, base_ref, report)
        user_material(repo, path, records)
        if inspect_submodules(path, allow_submodules) != modules:
            raise CleanupError('submodule identity changed during deinitialization')
        git(repo, 'worktree', 'remove', '--force', str(path.resolve()))
        report['submodule_force_remove'] = True
    else:
        git(repo, 'worktree', 'remove', str(path.resolve()))
    report['worktree_removed'] = True
    if any(r.get('branch') == 'refs/heads/' + branch for r in worktrees(repo)):
        raise CleanupError('branch checked out elsewhere; partial cleanup retained branch')
    commands = ['start', f'verify {base_ref} {report["base_oid"]}']
    if base_ref == 'refs/heads/main':
        commands.append(f'verify refs/remotes/origin/main {report["origin_main_oid"]}')
    commands += [f'delete refs/heads/{branch} {expected}', 'prepare', 'commit', '']
    result = subprocess.run(['git', '-C', str(repo), 'update-ref', '--stdin'], input='\n'.join(commands), text=True, capture_output=True, timeout=10)
    if result.returncode:
        raise CleanupError('ref transaction failed; partial cleanup retained branch: ' + result.stderr.strip())
    remaining = _run(['git', '-C', str(repo), 'show-ref', '--verify', '--quiet', 'refs/heads/' + branch], check=False)
    if remaining.returncode != 1:
        report['branch_state'] = 'retained/recreated'
        raise CleanupError('branch retained/recreated after deletion')
    report['branch_removed'] = True


def cargo_cache_followup(repo, worktree):
    cache_script = pathlib.Path(__file__).with_name('cargo-cache.py')
    command = [sys.executable, str(cache_script), 'gc', '--repo-root', str(repo), '--json']
    result = {'preview_command': command, 'default_retention_days': 7}
    try:
        preview = _run(command, timeout=120)
        data = json.loads(preview.stdout)
        namespace = 'worktree-' + hashlib.sha256(str(worktree.resolve()).encode()).hexdigest()[:12]
        result['targets'] = [r for r in data['targets'] if r.get('owner') == str(worktree.resolve()) or pathlib.Path(r['target']).parent.name == namespace]
        targets = {r['target'] for r in result['targets']}
        result['actions'] = [a for a in data['actions'] if a['target'] in targets]
    except (CleanupError, ValueError, KeyError) as exc:
        result['report_error'] = str(exc)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__, epilog='Process snapshots cover current-user processes and visible all-user target holders; invisible foreign use is outside this observation domain. Current-ABI pycache requires matching tracked source content. Unsupported cache ABI is retained. Stop known users and run outside the target. Never fetches or uses sudo. A single force removal is allowed only for verified submodule worktrees with --allow-submodules; all other safeguards still apply. Exit: 0 eligible/success, 1 blocked/partial, 2 invalid arguments.')
    parser.add_argument('--repo-root', type=pathlib.Path, default=pathlib.Path.cwd())
    parser.add_argument('--worktree', type=pathlib.Path, required=True)
    parser.add_argument('--branch', required=True)
    parser.add_argument('--expected-head', required=True, help='full expected commit OID')
    parser.add_argument('--base-ref', choices=['refs/remotes/origin/main', 'refs/heads/main'], default='refs/remotes/origin/main')
    parser.add_argument('--pr', type=int, help='explicit PR number for squash/rebase merged proof')
    parser.add_argument('--allow-submodules', action='store_true', help='verify recursive submodule data and allow one force remove after non-force deinit')
    parser.add_argument('--execute', action='store_true')
    args = parser.parse_args()
    if not re.fullmatch(r'[0-9a-f]{40}|[0-9a-f]{64}', args.expected_head) or (args.pr is not None and args.pr <= 0):
        parser.error('expected-head must be full OID; PR must be positive')
    if _run(['git', 'check-ref-format', 'refs/heads/' + args.branch], check=False).returncode:
        parser.error('invalid branch name')
    report = dict(eligible=False, blocked_reasons=[], executed=args.execute, worktree_removed=False, branch_removed=False, submodule_force_remove=False)
    try:
        report.update(inspect(args.repo_root, args.worktree, args.branch, args.expected_head, args.base_ref, args.pr, args.allow_submodules))
        report['eligible'] = True
        if args.execute:
            execute(args.repo_root, args.worktree, args.branch, args.expected_head, args.base_ref, args.pr, report, args.allow_submodules)
    except (CleanupError, OSError, ValueError, subprocess.TimeoutExpired) as exc:
        report['eligible'] = False
        report['blocked_reasons'].append(str(exc))
    # Read actual remaining resources even on partial failure.
    try:
        report['worktree_registered'] = any(r.get('worktree') == str(args.worktree.resolve()) for r in worktrees(args.repo_root))
        report['worktree_exists'] = args.worktree.exists()
        branch_readback = _run(['git', '-C', str(args.repo_root), 'show-ref', '--verify', '--quiet', 'refs/heads/' + args.branch], check=False)
        if branch_readback.returncode not in (0, 1):
            raise CleanupError('branch readback unavailable')
        report['branch_exists'] = branch_readback.returncode == 0
        if args.execute and report['eligible'] and (report['worktree_registered'] or report['worktree_exists'] or report['branch_exists']):
            report['eligible'] = False
            report['blocked_reasons'].append('resource retained/recreated during final readback')
            report['branch_removed'] = not report['branch_exists']
    except CleanupError as exc:
        report['readback_error'] = str(exc)
        report['eligible'] = False
    if report['worktree_removed']:
        # Reporting is independent of Git deletion success; never delete external caches here.
        report['cargo_cache_followup'] = cargo_cache_followup(args.repo_root, args.worktree)
    print(json.dumps(report))
    return 0 if report['eligible'] else 1

if __name__ == '__main__':
    sys.exit(main())
