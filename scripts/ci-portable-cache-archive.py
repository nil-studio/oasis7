#!/usr/bin/env python3
"""Finite portable cache layout and bounded, non-extracting archive validation."""
import argparse
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import signal
import stat
import subprocess
import sys
import tarfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
CWD = Path('/home/runner/work/oasis7/oasis7')
TEMP = Path('/home/runner/work/_temp')


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), ROOT / 'scripts' / f'{name}.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def describe(contract, component):
    if contract == 'worker-build' and component == 'worker-build':
        helper = load('ci-worker-build')
        root = Path('/home/runner/.cache/oasis7/worker-build') / helper.VERSION
        key = helper.cache_key(os.environ, root)
        max_bytes, max_files = helper.MAX_CACHE_BYTES, helper.MAX_CACHE_ENTRIES
        job = 'worker-build-cache'
    elif contract == 'browser-archive' and component in ('fonts-noto-cjk', 'fonts-freefont-ttf', 'chrome'):
        helper = load('ci-browser-deps')
        meta = helper.metadata(component, TEMP / 'oasis7-browser-archives')
        root, key = Path(meta['root']), meta['key']
        max_bytes, max_files, job = helper.MAX_CACHE_BYTES, 4, 'browser-archive-cache'
    else:
        raise ValueError('unknown portable archive contract')
    return dict(contract=contract, component=component, root=str(root), key=key, cwd=str(CWD),
                member=os.path.relpath(root, CWD), max_bytes=max_bytes, max_files=max_files, job=job)


def no_symlinks(path):
    for parent in (path, *path.parents):
        try:
            info = parent.lstat()
        except FileNotFoundError:
            continue
        if stat.S_ISLNK(info.st_mode):
            raise ValueError('archive destination uses a symlink')
        if parent != path and not stat.S_ISDIR(info.st_mode):
            raise ValueError('archive destination ancestor is not a directory')


class BoundedReader:
    def __init__(self, stream, maximum):
        self.stream, self.remaining = stream, maximum

    def read(self, size=-1):
        # Include headers/PAX/long names and padding in the same hard bound.
        data = self.stream.read(min(size if size >= 0 else self.remaining + 1, self.remaining + 1))
        self.remaining -= len(data)
        if self.remaining < 0:
            raise ValueError('decompressed archive exceeds limit')
        return data


def inspect_archive(archive, compression, layout, *, exact_file=False):
    if compression != 'zstd-without-long':
        raise ValueError('portable archives require the original zstd-without-long method')
    no_symlinks(Path(layout['root']))
    no_symlinks(Path(archive))
    flags = os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW
    fd = os.open(archive, flags)
    process = None
    try:
        info = os.fstat(fd)
        limit = layout['max_bytes'] + 1024 * 1024
        if not stat.S_ISREG(info.st_mode) or not 0 < info.st_size <= limit:
            raise ValueError('invalid compressed archive size/type')
        # Already-open regular fd prevents path replacement/FIFO races at launch.
        environment = {key: os.environ[key] for key in ('PATH', 'LANG', 'LC_ALL') if key in os.environ}
        process = subprocess.Popen(['zstd', '-d', '-q', '-c', '--memory=256MB', '--'], stdin=fd, stdout=subprocess.PIPE,
                                   stderr=subprocess.DEVNULL, env=environment)
        bounded = BoundedReader(process.stdout, limit)
        seen, files, payload = {}, 0, 0
        prefix = layout['member']
        with tarfile.open(fileobj=bounded, mode='r|') as reader:
            for entry in reader:
                if set(entry.pax_headers) - {'path', 'size', 'mtime', 'atime', 'ctime'}:
                    raise ValueError('unsupported PAX extension')
                name = entry.name.rstrip('/')
                if (not name or any(ord(c) < 32 or ord(c) == 127 for c in name)
                        or name.startswith('/') or '\\' in name
                        or not (name == prefix or name.startswith(prefix + '/'))):
                    raise ValueError('archive member outside exact legacy prefix')
                suffix = name[len(prefix) + 1:] if name != prefix else ''
                if suffix and any(part in ('', '.', '..') for part in suffix.split('/')):
                    raise ValueError('unsafe archive member suffix')
                if (name in seen or not (entry.isdir() or entry.isreg()) or entry.sparse is not None
                        or (exact_file and (suffix or not entry.isreg()))
                        or (not exact_file and not suffix and not entry.isdir())
                        or (entry.isreg() and any(item.startswith(name + '/') for item in seen))):
                    raise ValueError('duplicate or unsupported archive member')
                for ancestor in PurePosixPath(suffix).parents:
                    ancestor_name = prefix + ('/' + str(ancestor) if str(ancestor) != '.' else '')
                    if seen.get(ancestor_name) == 'file':
                        raise ValueError('archive file/directory conflict')
                seen[name] = 'dir' if entry.isdir() else 'file'
                if len(seen) > layout['max_files'] + 256:
                    raise ValueError('too many archive entries')
                destination = Path(layout['root']) / suffix
                no_symlinks(destination)
                if entry.isreg():
                    files += 1
                    payload += entry.size
                    if files > layout['max_files'] or payload > layout['max_bytes'] or entry.size < 0:
                        raise ValueError('archive payload exceeds contract')
                # Advance streaming parser through each payload under the same bound.
        while bounded.read(64 * 1024):
            pass
        if process.wait(timeout=5) != 0 or files == 0:
            raise ValueError('invalid or empty compressed archive')
        return dict(files=files, payload_bytes=payload, entries=len(seen))
    finally:
        os.close(fd)
        if process is not None:
            if process.poll() is None:
                process.kill()
            process.wait(timeout=5)
            process.stdout.close()


def interrupted(_signal, _frame):
    raise ValueError('archive inspection interrupted')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--contract', required=True)
    parser.add_argument('--component', required=True)
    parser.add_argument('--archive')
    parser.add_argument('--compression')
    args = parser.parse_args()
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGALRM, interrupted)
    signal.alarm(60)
    layout = describe(args.contract, args.component)
    result = layout if args.archive is None else inspect_archive(args.archive, args.compression, layout)
    print(json.dumps(result, sort_keys=True))


if __name__ == '__main__':
    try:
        if '--supervise' in sys.argv:
            import tempfile
            from ci_process_runner import run
            with tempfile.TemporaryDirectory(prefix='portable-inspect-') as temporary:
                args = [arg for arg in sys.argv[1:] if arg != '--supervise']
                outcome = run([sys.executable, __file__, *args], ROOT, dict(os.environ),
                              'portable-cache', 'archive-contract', Path(temporary) / 'result.jsonl',
                              timeout_seconds=60)
                sys.exit(0 if outcome['passed'] else 1)
        main()
    except (ValueError, OSError, tarfile.TarError, subprocess.SubprocessError) as error:
        print(f'portable archive rejected: {error}', file=sys.stderr)
        sys.exit(1)
