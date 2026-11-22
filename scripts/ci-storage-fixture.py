"""Real filesystem and process prerequisites for the attempt-release image consumer."""
import fcntl
import json
import os
from pathlib import Path
import signal
import stat
import subprocess
import sys
import tempfile
import time
import uuid

from ci_process_runner import run


def filesystem_probe(root, results, expected_device, expected_inode):
    root, results = Path(root), Path(results)
    info = root.lstat()
    if (root.resolve() != root or not stat.S_ISDIR(info.st_mode)
            or info.st_uid != os.getuid() or stat.S_IMODE(info.st_mode) != 0o700
            or (info.st_dev, info.st_ino) != (expected_device, expected_inode)):
        raise ValueError('fixture must be the private runner-owned bind with its original identity')
    if results.parent != root or results.stat().st_dev != info.st_dev:
        raise ValueError('fixture and sampled results must share one filesystem')
    temporary = root / 'tmp'
    temporary.mkdir(mode=0o700, exist_ok=True)
    if (temporary.is_symlink() or temporary.stat().st_dev != info.st_dev
            or stat.S_IMODE(temporary.stat().st_mode) != 0o700
            or temporary.stat().st_uid != os.getuid()):
        raise ValueError('fixture temporary directory changed filesystem')
    filesystem = subprocess.check_output(
        ['stat', '-f', '-c', '%T', str(root)], text=True, timeout=5).strip()
    if filesystem in ('overlayfs', 'tmpfs'):
        raise ValueError('fixture must retain the runner filesystem, not overlay or tmpfs')
    with tempfile.TemporaryDirectory(dir=temporary, prefix='preflight-') as work:
        work = Path(work)
        source, linked = work / 'original', work / 'linked'
        with source.open('wb') as stream:
            stream.write(b'original fixture bytes')
            stream.flush()
            os.fsync(stream.fileno())
            fcntl.flock(stream.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
            # A fresh process must see the actual kernel lock, not a Python mock.
            code = ('import fcntl,sys; f=open(sys.argv[1],"rb"); '
                    '\ntry: fcntl.flock(f,fcntl.LOCK_EX|fcntl.LOCK_NB)'
                    '\nexcept BlockingIOError: sys.exit(0)'
                    '\nsys.exit(1)')
            subprocess.run([sys.executable, '-c', code, str(source)], check=True, timeout=10)
        os.link(source, linked)
        original, alias = source.stat(), linked.stat()
        if ((original.st_dev, original.st_ino) != (alias.st_dev, alias.st_ino)
                or original.st_nlink != 2 or linked.read_bytes() != b'original fixture bytes'):
            raise ValueError('fixture hardlink identity was not preserved')
        allocated = original.st_blocks * 512
        source.rename(work / 'published')
        fd = os.open(work, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(fd)
        finally:
            os.close(fd)
    return dict(device=info.st_dev, inode=info.st_ino, uid=info.st_uid, filesystem=filesystem,
                mode='0700', allocated_probe_bytes=allocated,
                hardlink=True, flock=True, file_fsync=True, directory_fsync=True)


def process_probe():
    marker = uuid.uuid4().hex
    child = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(30)'],
                             env=dict(os.environ, OASIS7_STORAGE_SAMPLE_OWNER=marker))
    try:
        proc = Path('/proc') / str(child.pid)
        if (proc.stat().st_uid != os.getuid()
                or ('OASIS7_STORAGE_SAMPLE_OWNER=' + marker).encode()
                not in (proc / 'environ').read_bytes().split(b'\0')):
            raise ValueError('same-UID marked child is not observable through proc')
        if os.readlink(proc / 'ns/pid') != os.readlink('/proc/self/ns/pid'):
            raise ValueError('driver and fixture child do not share a PID namespace')
        status = (proc / 'status').read_text()
        if f'PPid:\t{os.getpid()}\n' not in status or 'VmRSS:' not in status:
            raise ValueError('fixture child has unexpected parent identity')
        child.kill()
        if child.wait(timeout=10) != -signal.SIGKILL:
            raise ValueError('real child kill/wait did not observe SIGKILL')
    finally:
        if child.poll() is None:
            child.kill()
        child.wait(timeout=10)
    return dict(uid=os.getuid(), parent_pid=os.getpid(), child_pid=child.pid,
                shared_pid_namespace=True, proc_marker_visible=True, sigkill_reaped=True)


def preflight(root, results, bundle, manifest, env):
    """Fail closed before running the unchanged four-phase acceptance case."""
    report = dict(passed=False)
    started = time.monotonic()
    path = results / 'fixture-preflight.json'
    try:
        report['filesystem'] = filesystem_probe(
            root, results, int(os.environ['OASIS7_STORAGE_FIXTURE_DEVICE']),
            int(os.environ['OASIS7_STORAGE_FIXTURE_INODE']))
        temporary = str(Path(root) / 'tmp')
        if env.get('TMPDIR') != temporary or tempfile.gettempdir() != temporary:
            raise ValueError('driver and children must use the runner fixture temporary directory')
        report['process'] = process_probe()
        binary = manifest['binaries']['runtime-tests']
        suffix = '::native_environment_original_loaded_bytes_verify'
        names = [name for name in binary['tests'] if name.endswith(suffix)]
        if len(names) != 1 or names[0] in binary['ignored_tests']:
            raise ValueError('native environment preflight inventory changed')
        cwd = Path(__file__).resolve().parents[1] / binary['cwd']
        with (results / 'native-preflight.log').open('w') as stream:
            result = run([str(bundle / binary['path']), '--exact', names[0],
                          '--test-threads=1', '--nocapture'], cwd,
                         dict(env, CARGO_MANIFEST_DIR=str(cwd)),
                         'attempt-release-preflight', names[0], results / 'preflight-results.jsonl',
                         expected_count=1, timeout_seconds=2400, log_stream=stream)
        report['native'] = result
        if (result.get('passed') is not True or result.get('cleanup_result') != 'clean'
                or result.get('count_ok') is not True or result.get('leaked_processes')
                or result.get('exit_code') != 0 or result.get('timeout') or result.get('cancelled')):
            raise ValueError('actual native environment capture preflight failed')
        report['passed'] = True
        return report
    except Exception as error:
        report['error'] = str(error)
        raise
    finally:
        report['elapsed_seconds'] = time.monotonic() - started
        path.write_text(json.dumps(report, indent=2) + '\n')
