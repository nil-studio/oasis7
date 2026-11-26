"""One frozen two-case observation; never an ordinary Node-suite receipt."""
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import time

SOURCE = '9d572001f2bcd92ff8d03a619d268e920137d812'
HEAD = 'codex/engineering-node-seed-two-case-diagnostic-corrected'
BASE = 'codex/engineering-portable-cache-container-cutover'
FROM = 'Lock corrected two exact Node seed diagnostic cases'
TO = 'Observe corrected two exact Node seed cases with original assertions'
CASES = tuple('tests::network_gap_sync_provider_routing_tests::runtime_network_replication_gap_sync_blocks_without_generic_fallback_after_provider_route_' + suffix for suffix in ('not_found', 'unavailable'))
ROOT = Path(os.environ['GITHUB_WORKSPACE']).resolve()
OUT = ROOT / 'node-seed-diagnostic-output'


def git(*args):
    return subprocess.check_output(['git', '-C', str(ROOT), *args], text=True, timeout=10).strip()


def admission():
    event = json.loads(Path(os.environ['GITHUB_EVENT_PATH']).read_text())
    pr = event['pull_request']
    expected = {'GITHUB_REPOSITORY': 'nil-studio/oasis7', 'GITHUB_EVENT_NAME': 'pull_request',
                'GITHUB_HEAD_REF': HEAD, 'GITHUB_BASE_REF': BASE, 'GITHUB_JOB': 'node-seed-exact',
                'RUNNER_OS': 'Linux', 'RUNNER_ARCH': 'X64', 'RUNNER_ENVIRONMENT': 'github-hosted',
                'GITHUB_RUN_ATTEMPT': '1', 'CODE_SHA': SOURCE}
    if (any(os.environ.get(k) != v for k, v in expected.items()) or (os.getuid(), os.getgid()) != (1001, 1001)
            or event.get('action') != 'edited' or event.get('changes', {}).get('title', {}).get('from') != FROM
            or pr.get('title') != TO or pr.get('draft') is not True
            or any(pr[role]['repo']['full_name'] != 'nil-studio/oasis7' for role in ('head', 'base'))
            or pr['head']['sha'] != os.environ['CONTROLLER_SHA'] or git('rev-parse', 'HEAD') != SOURCE):
        raise ValueError('not the exact owned single-use diagnostic')
    if git('diff', '--name-only', SOURCE, os.environ['CONTROLLER_SHA']).splitlines() != [
            '.github/actions/node-seed-exact-diagnostic/run.py', '.github/workflows/node-seed-exact-diagnostic.yml']:
        raise ValueError('unexpected controller delta')


def sample(pid, elapsed):
    result = {'elapsed_seconds': round(elapsed, 3)}
    try:
        stat = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
        result.update(state=stat[0], cpu_ticks=int(stat[11]) + int(stat[12]), rss_pages=int(stat[21]))
        threads = []
        for task in sorted(Path(f'/proc/{pid}/task').iterdir())[:64]:
            comm = (task / 'comm').read_text().strip()
            if comm.startswith('aw-node-'):
                wchan = (task / 'wchan').read_text().strip()
                threads.append({'tid': int(task.name), 'name': comm[:32], 'wchan': wchan[:80]})
        result['node_threads'] = threads
    except (FileNotFoundError, PermissionError, ProcessLookupError):
        result['process_or_thread_unavailable'] = True
    return result


def child_environment():
    return {k: v for k, v in os.environ.items()
            if not re.search(r'TOKEN|SECRET|PASSWORD|CREDENTIAL', k, re.I)
            and not k.startswith(('GITHUB_', 'ACTIONS_', 'INPUT_'))}


def group_inventory(pgid):
    members = []
    for item in Path('/proc').iterdir():
        if item.name.isdigit():
            try:
                if item.stat().st_uid == os.getuid() and os.getpgid(int(item.name)) == pgid:
                    try:
                        fields = (item / 'stat').read_text().rsplit(')', 1)[1].split()
                        state, identity = fields[0], fields[19]
                        if fields[2] != str(pgid) or not identity.isdigit():
                            state, identity = '?', None
                    except (PermissionError, IndexError):
                        state, identity = '?', None
                    members.append({'pid': int(item.name), 'state': state, 'starttime': identity})
            except (FileNotFoundError, ProcessLookupError, PermissionError):
                pass
    return sorted(members, key=lambda row: row['pid'])


def group_members(pgid):
    # Match ci_process_runner.owned_processes: terminated zombies cannot be
    # signalled or execute code. Unknown and every live state remain blockers.
    return [row['pid'] for row in group_inventory(pgid) if row['state'] != 'Z']


def run(argv, label, seconds):
    started = time.monotonic()
    observation = {'label': label, 'samples': [], 'timed_out': False}
    with (OUT / (label + '.stdout')).open('wb') as stdout, (OUT / (label + '.stderr')).open('wb') as stderr:
        process = subprocess.Popen(argv, cwd=ROOT, env=child_environment(), stdout=stdout, stderr=stderr,
                                   start_new_session=True)
        try:
            while process.poll() is None:
                elapsed = time.monotonic() - started
                if elapsed >= seconds or max(os.fstat(stdout.fileno()).st_size, os.fstat(stderr.fileno()).st_size) > 16000000:
                    observation['timed_out'] = True
                    observation['abort_reason'] = 'deadline' if elapsed >= seconds else 'output_limit'
                    break
                if label != 'compile' and len(observation['samples']) < 32 and elapsed >= len(observation['samples']) * .5:
                    observation['samples'].append(sample(process.pid, elapsed))
                time.sleep(.05)
        finally:
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait(timeout=3)
        remaining = group_members(process.pid)
        if remaining:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            deadline = time.monotonic() + 1
            while remaining and time.monotonic() < deadline:
                time.sleep(.05)
                remaining = group_members(process.pid)
        observation.update(exit_code=process.returncode, seconds=round(time.monotonic() - started, 3),
                           remaining_process_group=remaining,
                           process_group_after_cleanup=group_inventory(process.pid))
    (OUT / (label + '.json')).write_text(json.dumps(observation, indent=2) + '\n')
    return observation


def main():
    admission()
    OUT.mkdir(mode=0o700)
    receipt = {'probe_only': True, 'full_node_suite_passed': False, 'source': SOURCE,
               'controller': os.environ['CONTROLLER_SHA'], 'run': os.environ['GITHUB_RUN_ID'],
               'image': 'ghcr.io/nil-studio/oasis7/ci-rust-build@sha256:bc886637f7bf153dfb60df5c8f56e82949a8f4d9ec2ffd571750eae2fe1a795f',
               'uid': os.getuid(), 'gid': os.getgid(), 'cases': [], 'cache_write': False}
    original_lock = (ROOT / 'Cargo.lock').read_bytes()
    try:
        build = run(['env', '-u', 'RUSTC_WRAPPER', 'cargo', 'test', '-p', 'oasis7_node', '--lib',
                     '--no-run', '--message-format=json'], 'compile', 300)
        receipt['compile'] = build
        if build['exit_code'] != 0 or build['timed_out'] or build['remaining_process_group']:
            receipt['compile_error_tail'] = (OUT / 'compile.stderr').read_bytes()[-16000:].decode('utf8', errors='replace')
            raise ValueError('compile failed before cases')
        artifacts = [json.loads(line) for line in (OUT / 'compile.stdout').read_text().splitlines()]
        binaries = {Path(row['executable']).resolve() for row in artifacts
                    if row.get('reason') == 'compiler-artifact' and row.get('executable')
                    and row.get('target', {}).get('name') == 'oasis7_node'
                    and row.get('target', {}).get('src_path') == str(ROOT / 'crates/oasis7_node/src/lib.rs')
                    and row.get('target', {}).get('kind') == ['lib'] and row.get('profile', {}).get('test') is True}
        if len(binaries) != 1:
            raise ValueError('expected exactly one actual Node test ELF')
        binary = binaries.pop()
        if not binary.is_relative_to(ROOT / 'target/debug/deps') or not binary.is_file() or binary.is_symlink():
            raise ValueError('unexpected test binary path')
        digest = hashlib.sha256()
        with binary.open('rb') as stream:
            size = os.fstat(stream.fileno()).st_size
            if size > 512000000 or stream.read(4) != b'\x7fELF':
                raise ValueError('expected bounded Linux ELF')
            stream.seek(0)
            for block in iter(lambda: stream.read(1048576), b''):
                digest.update(block)
        receipt.update(binary_sha256=digest.hexdigest(), binary_bytes=size)
        for index, case in enumerate(CASES):
            listing = subprocess.check_output([str(binary), case, '--exact', '--list'], cwd=ROOT,
                                              env=child_environment(), text=True, timeout=10)
            if listing.strip() != case + ': test\n\n1 test, 0 benchmarks':
                raise ValueError('exact test inventory mismatch')
            observation = run([str(binary), case, '--exact', '--nocapture'], f'case-{index}', 60)
            receipt['cases'].append({'case': case, **observation})
            if observation['timed_out'] or observation['remaining_process_group']:
                raise ValueError('case supervisor did not complete cleanly')
            if observation['exit_code'] == 0 and not re.search(
                    r'test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured; 648 filtered out;',
                    (OUT / f'case-{index}.stdout').read_text()):
                raise ValueError('expected one original unignored case to execute')
        if any(row['exit_code'] != 0 for row in receipt['cases']):
            raise ValueError('original exact case failed; retain its original snapshot')
        if (ROOT / 'Cargo.lock').read_bytes() != original_lock:
            raise ValueError('Cargo.lock changed')
        receipt['two_exact_cases_passed'] = True
    except Exception as error:
        receipt['error'] = str(error)
        raise
    finally:
        (OUT / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
        # Compiler output is not needed in the public diagnostic artifact.
        for name in ('compile.stdout', 'compile.stderr'):
            (OUT / name).unlink(missing_ok=True)


if __name__ == '__main__':
    main()
