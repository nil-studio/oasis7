#!/usr/bin/env python3
"""Optional fixed-profile ECS workspace cache; cache failures never skip Cargo.

Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7.
"""
import argparse
import gzip
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tarfile
import tempfile
import time

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent.parent
STAGING_BYTES = 6 * 1024 ** 3
FAMILY_BYTES = 6 * 1024 ** 3
GENERATIONS = 2
ARCHIVE_NAME = 'workspace-units.tar.gz'
MANIFEST_BYTES = 64 * 1024 ** 2
MAX_FILES = 20_000
ECS_KEY = re.compile(r'ecs-workspace-v[12]-[0-9a-f]{64}-[0-9a-f]{40}\Z', re.ASCII)
WORKFLOW = '.github/workflows/ecs-packages.yml'


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'scripts' / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


units = load('ecs_units', 'ci-test-artifacts.py')


def temporary(path):
    path = path.resolve()
    target = (ROOT / 'target').resolve()
    if any(path.is_relative_to(base) or base.is_relative_to(path) for base in (ROOT, target)):
        raise ValueError('ECS cache state and reports must be outside checkout and target')
    return path


def build(command, env, state_path):
    state_path = temporary(state_path)
    started = time.monotonic()
    state = None
    try:
        state = units.cache_begin(ROOT)
    except (OSError, ValueError) as error:
        print('ECS cache recording unavailable: ' + str(error), flush=True)
    local = []
    with subprocess.Popen([*command, '--message-format=json-render-diagnostics'], cwd=ROOT,
                          env=env, stdout=subprocess.PIPE, text=True) as process:
        for line in process.stdout:
            try:
                message = json.loads(line)
            except ValueError:
                print(line, end='', flush=True)
                continue
            if message.get('reason') == 'compiler-message':
                print(message['message'].get('rendered', ''), end='', flush=True)
            if (message.get('reason') == 'compiler-artifact'
                    and Path(message['manifest_path']).is_relative_to(ROOT)):
                local.append({key: message.get(key) for key in
                              ('package_id', 'target', 'profile', 'features', 'fresh')})
            if state is not None:
                try:
                    units.cache_record(state, message)
                except (OSError, ValueError, KeyError, TypeError) as error:
                    state['error'] = str(error)
        status = process.wait()
    if status:
        raise subprocess.CalledProcessError(status, command)
    if state is not None and not state.get('error'):
        try:
            units.cache_finish(state)
        except (OSError, ValueError) as error:
            state['error'] = str(error)
    try:
        state_path.parent.mkdir(parents=True, exist_ok=True)
        state_path.write_text(json.dumps(state or {}, sort_keys=True) + '\n')
        state_path.with_name('ecs-units.json').write_text(json.dumps(dict(
            elapsed_seconds=time.monotonic() - started, units=local,
            fresh=sum(bool(unit['fresh']) for unit in local), rebuilt=sum(not unit['fresh'] for unit in local)), indent=2) + '\n')
    except OSError as error:
        print('ECS cache report unavailable: ' + str(error), flush=True)


def identity():
    # Cargo source contents are validated by the unit manifest; source SHA is
    # deliberately separate from the compatible build-configuration identity.
    metadata = json.loads(subprocess.check_output(
        ['cargo', 'metadata', '--locked', '--no-deps', '--format-version=1'], cwd=ROOT, text=True))
    files = {'Cargo.lock', '.github/workflows/ecs-packages.yml', 'scripts/ci-ecs-cache.py',
             'scripts/package-ecs.py', 'scripts/ci-test-artifacts.py'}
    files.update(str(Path(package['manifest_path']).relative_to(ROOT)) for package in metadata['packages'])
    files.update(str(path.relative_to(ROOT)) for path in (ROOT / '.cargo').rglob('*') if path.is_file())
    files.update(name for name in ('rust-toolchain', 'rust-toolchain.toml') if (ROOT / name).is_file())
    configuration = dict(rustc=subprocess.check_output(['rustc', '-vV'], text=True),
                         cargo=subprocess.check_output(['cargo', '-V'], text=True),
                         profile='ecs', features='default', target='native',
                         environment={key: value for key, value in os.environ.items()
                                      if (key.startswith(('CARGO_', 'RUST', 'CC_', 'CXX_', 'AR_', 'PKG_CONFIG'))
                                          or key in ('CC', 'CXX', 'AR', 'CFLAGS', 'CXXFLAGS', 'CPPFLAGS',
                                                     'LDFLAGS', 'LIBRARY_PATH', 'CPATH'))
                                      and key not in ('CARGO_BUILD_JOBS', 'CARGO_TERM_COLOR', 'CARGO_NET_RETRY')},
                         files={name: units.digest(ROOT / name) for name in sorted(files)})
    digest = hashlib.sha256(json.dumps(configuration, sort_keys=True).encode()).hexdigest()
    prefix = 'ecs-workspace-v2-' + digest + '-'
    source = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    return dict(prefix=prefix, key=prefix + source)


def save_allowed(entries, key, upload_bytes, repository_limit_bytes=None):
    return save_policy(entries, key, upload_bytes, repository_limit_bytes)['capacity_allowed']


def quota_bytes(value):
    limit = value.get('max_cache_size_gb') if isinstance(value, dict) else None
    if type(limit) is not int or limit <= 0:
        raise ValueError('repository cache quota unavailable or invalid')
    return limit * 1000 ** 3


def configured_budget(value):
    if value in (None, ''):
        return None
    if not isinstance(value, str) or not re.fullmatch(r'[1-9][0-9]{0,18}', value, re.ASCII):
        raise ValueError('configured repository budget must be positive decimal bytes')
    return int(value)


def api_client():
    maintenance = load('ecs_maintenance', 'ci-cache-maintenance.py')
    return maintenance, maintenance.GitHubApi(os.environ.get('GITHUB_REPOSITORY'), os.environ.get('GH_TOKEN'))


def capacity_context(maintenance, api, budget, report):
    """A verified workflow budget is distinct from the live repository quota."""
    report.pop('quota_api_diagnostics', None)
    report.update(configured_repository_budget_bytes=budget, live_repository_limit_bytes=None,
                  effective_repository_budget_bytes=None, capacity_basis=None,
                  inventory_complete=False, observed_cache_count=None, observed_repository_bytes=None,
                  available_repository_bytes=None)
    entries = maintenance.validate_inventory(api.list_caches())
    report.update(inventory_complete=True, observed_cache_count=len(entries),
                  observed_repository_bytes=sum(entry['size_in_bytes'] for entry in entries))
    try:
        live = quota_bytes(api.request('GET', '/actions/cache/storage-limit'))
    except maintenance.MaintenanceApiError as error:
        report['quota_api_diagnostics'] = error.diagnostics
        # A permission-denied quota read does not invalidate the independently
        # verified, main-controlled budget. Unknown errors and rate limits do.
        if (budget is None or error.status != 403 or error.rate_limited
                or error.diagnostics['category'] not in ('integration_access', 'personal_token_access')):
            raise
        effective, basis = budget, 'verified_configured_budget'
    else:
        report['live_repository_limit_bytes'] = live
        effective = min(live, budget) if budget is not None else live
        basis = 'live_quota_and_configured_budget' if budget is not None else 'live_quota'
    report.update(effective_repository_budget_bytes=effective, capacity_basis=basis,
                  available_repository_bytes=max(0, effective - report['observed_repository_bytes']))
    return entries


def save_policy(entries, key, upload_bytes, repository_limit_bytes):
    if not ECS_KEY.fullmatch(key or '') or not key.startswith('ecs-workspace-v2-'):
        raise ValueError('ECS cache key is outside the current managed schema')
    if type(upload_bytes) is not int or upload_bytes <= 0:
        raise ValueError('cache upload bound must be positive bytes')
    family = [entry for entry in entries if entry['ref'] == 'refs/heads/main'
              and ECS_KEY.fullmatch(entry['key'])]
    compatible = [entry for entry in family if entry['key'].rsplit('-', 1)[0] == key.rsplit('-', 1)[0]]
    observed = sum(entry['size_in_bytes'] for entry in entries)
    projected = observed + upload_bytes
    report = dict(capacity_allowed=False, effective_repository_budget_bytes=repository_limit_bytes,
                  observed_repository_bytes=observed, projected_repository_bytes=projected,
                  new_payload_upload_bound_bytes=upload_bytes,
                  projected_family_bytes=sum(entry['size_in_bytes'] for entry in family) + upload_bytes,
                  family_limit_bytes=FAMILY_BYTES, compatible_generations=len(compatible),
                  rotation_required=len(compatible) >= GENERATIONS,
                  compatible_cache_ids=sorted(entry['id'] for entry in compatible))
    if type(repository_limit_bytes) is not int or repository_limit_bytes <= 0:
        report['reason'] = 'repository cache quota unavailable or invalid'
    elif projected > repository_limit_bytes:
        report['reason'] = 'repository cache capacity gap'
    elif any(entry['key'] == key for entry in family):
        report['reason'] = 'existing immutable key'
    elif len(compatible) >= GENERATIONS:
        report['reason'] = 'compatible generation limit'
    elif report['projected_family_bytes'] > FAMILY_BYTES:
        report['reason'] = 'ECS main family budget'
    else:
        report['capacity_allowed'] = True
    return report


def archive_manifest(metadata):
    def unique_object(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError('duplicate cache manifest field')
            result[key] = value
        return result
    if len(metadata) > MANIFEST_BYTES:
        raise ValueError('cache manifest exceeds its byte limit')
    try:
        manifest = json.loads(metadata, object_pairs_hook=unique_object)
    except RecursionError:
        raise ValueError('cache manifest nesting exceeds the JSON parser limit') from None
    if not isinstance(manifest, dict):
        raise ValueError('cache manifest must be an object')
    files, directories = manifest.get('files'), manifest.get('directories')
    if (not isinstance(files, dict) or not 0 < len(files) <= MAX_FILES
            or not isinstance(directories, dict) or len(directories) > MAX_FILES):
        raise ValueError('cache manifest has invalid file or directory counts')
    size = 0
    for name, metadata in [*files.items(), *directories.items()]:
        units._cache_path(Path('/cache-path-validation'), name)
        if (not isinstance(metadata, dict) or type(metadata.get('mode')) is not int
                or metadata['mode'] & ~0o777):
            raise ValueError('cache manifest has an invalid file mode')
    for metadata in files.values():
        if type(metadata.get('bytes')) is not int or metadata['bytes'] < 0:
            raise ValueError('cache manifest has an invalid file size')
        size += metadata['bytes']
    if not 0 < size <= STAGING_BYTES:
        raise ValueError('cache logical payload exceeds the staging budget or is empty')
    return manifest


def pack_archive(staged, destination):
    """Use short flat USTAR names: the unit manifest retains the original paths.

    No links, sparse records or PAX extensions are necessary. Duplicate Cargo
    hardlinks are regular payloads and count against the logical-byte bound.
    """
    started = time.monotonic()
    if (staged / 'manifest.json').stat().st_size > MANIFEST_BYTES:
        raise ValueError('cache manifest exceeds its byte limit')
    metadata = (staged / 'manifest.json').read_bytes()
    manifest = archive_manifest(metadata)
    # A failed compression never leaves a partial file at the action's path.
    with tempfile.TemporaryDirectory(prefix='ecs-pack-', dir=destination.parent.parent) as directory:
        pending = Path(directory) / ARCHIVE_NAME
        with pending.open('wb') as output, gzip.GzipFile(fileobj=output, mode='wb', compresslevel=1,
                                                       mtime=0, filename='') as compressed:
            with tarfile.open(fileobj=compressed, mode='w|', format=tarfile.USTAR_FORMAT) as archive:
                header = tarfile.TarInfo('manifest.json')
                header.size, header.mode = len(metadata), 0o600
                archive.addfile(header, io.BytesIO(metadata))
                for index, name in enumerate(sorted(manifest['files'])):
                    source = units._cache_path(staged / 'target', name)
                    header = tarfile.TarInfo(f'files/{index:06d}')
                    header.size, header.mode = manifest['files'][name]['bytes'], 0o600
                    with source.open('rb') as stream:
                        archive.addfile(header, stream)
        pending.replace(destination)
    return dict(archive_bytes=destination.stat().st_size,
                pack_seconds=round(time.monotonic() - started, 3))


def read_exact(stream, length):
    value = stream.read(length)
    if len(value) != length:
        raise ValueError('truncated ECS cache archive')
    return value


def read_header(stream, expected):
    # Read the raw header before tarfile can follow GNU/PAX/sparse extensions or
    # allocate an attacker-controlled extension buffer.
    header = tarfile.TarInfo.frombuf(read_exact(stream, tarfile.BLOCKSIZE), 'utf-8', 'strict')
    if (header.name != expected or not header.isreg() or header.type != tarfile.REGTYPE
            or header.linkname or header.pax_headers or header.sparse is not None):
        raise ValueError('unexpected, duplicate or unsafe ECS archive member')
    return header


def read_padding(stream, size):
    padding = read_exact(stream, -size % tarfile.BLOCKSIZE)
    if any(padding):
        raise ValueError('invalid ECS archive padding')


def unpack_archive(archive, destination):
    started = time.monotonic()
    if (archive.is_symlink() or not archive.is_file() or not 0 < archive.stat().st_size
            <= STAGING_BYTES + MANIFEST_BYTES + MAX_FILES * 1024):
        raise ValueError('ECS archive is missing, unsafe or too large')
    with gzip.open(archive, 'rb') as stream:
        header = read_header(stream, 'manifest.json')
        if not 0 < header.size <= MANIFEST_BYTES:
            raise ValueError('cache manifest exceeds its byte limit')
        metadata = read_exact(stream, header.size)
        manifest = archive_manifest(metadata)
        read_padding(stream, header.size)
        (destination / 'manifest.json').write_bytes(metadata)
        for index, name in enumerate(sorted(manifest['files'])):
            header = read_header(stream, f'files/{index:06d}')
            expected = manifest['files'][name]
            if header.size != expected['bytes']:
                raise ValueError('ECS archive member differs from the bounded manifest size')
            path = units._cache_path(destination / 'target', name)
            path.parent.mkdir(parents=True, exist_ok=True)
            remaining = header.size
            with path.open('xb') as output:
                while remaining:
                    value = read_exact(stream, min(remaining, 1024 ** 2))
                    output.write(value)
                    remaining -= len(value)
            path.chmod(expected['mode'])
            read_padding(stream, header.size)
        # USTAR termination is two zero blocks, padded to one 10 KiB record.
        # Also consume the gzip trailer; truncated or concatenated payloads
        # cannot reach Cargo validation with a partial archive.
        ending = stream.read(tarfile.RECORDSIZE + 1)
        if not 2 * tarfile.BLOCKSIZE <= len(ending) <= tarfile.RECORDSIZE or any(ending):
            raise ValueError('invalid or oversized ECS archive termination')
        for name in sorted(manifest['directories'], key=lambda name: len(Path(name).parts)):
            units._cache_path(destination / 'target', name).mkdir(parents=True, exist_ok=True)
        for name in sorted(manifest['directories'], key=lambda name: len(Path(name).parts), reverse=True):
            units._cache_path(destination / 'target', name).chmod(manifest['directories'][name]['mode'])
    return dict(archive_bytes=archive.stat().st_size, unpack_seconds=round(time.monotonic() - started, 3))


def upload_bound(archive):
    """Bound the pinned Linux actions/cache wrapper around exactly one file.

    Match the pinned action's resolvePaths (L1431), compression choice (L1500)
    and --posix/-P/-C/--files-from tar arguments (L2616):
    https://github.com/actions/cache/blob/5a3ec84eff668545956fd18022155c47e93e2684/dist/save-only/index.js
    GNU tar is measured rather than assuming path/PAX/header overhead. Only
    atime/ctime can change before save; their bounded timestamp strings can add
    at most one PAX data block, hence one additional 20-block tar record.
    ZSTD_COMPRESSBOUND alone only promises a one-pass bound. This contract uses
    the audited standard zstdmt CLI (v1.5.5, no intermediate flushes, no --long),
    which fills streaming blocks; it does not apply to arbitrary flush callers.
    The final ceiling also takes the larger zlib generic deflateBound, covering
    the gzip fallback (fixed/stored blocks plus the 18-byte gzip wrapper).
    """
    if os.environ.get('TAR_OPTIONS') or os.environ.get('GZIP'):
        raise ValueError('custom tar/gzip options are outside the cache upload size contract')
    version = subprocess.check_output(['tar', '--version'], text=True)
    if 'GNU tar' not in version.splitlines()[0]:
        raise ValueError('ECS cache upload sizing requires GNU tar')
    workspace = Path(os.environ.get('GITHUB_WORKSPACE', ROOT)).resolve()
    relative = os.path.relpath(archive, workspace)
    if len(os.fsencode(relative)) > 4096 or relative.startswith('-') or any(ord(c) < 32 for c in relative):
        raise ValueError('ECS cache path is outside the upload size contract')
    with tempfile.TemporaryDirectory(prefix='ecs-wrapper-', dir=archive.parent.parent) as directory:
        manifest = Path(directory) / 'manifest.txt'
        manifest.write_text(relative + '\n')
        with tempfile.TemporaryFile() as errors:
            with subprocess.Popen(['tar', '--posix', '-cf', '-', '--exclude', 'cache.tzst', '-P',
                                   '-C', str(workspace), '--files-from', str(manifest)],
                                  stdout=subprocess.PIPE, stderr=errors) as process:
                total = 0
                while data := process.stdout.read(1024 ** 2):
                    total += len(data)
                if process.wait():
                    raise ValueError('GNU tar cache wrapper sizing failed')
    bounded_tar = total + tarfile.RECORDSIZE
    zstd = bounded_tar + (bounded_tar >> 8) + max(0, ((128 << 10) - bounded_tar) >> 11)
    fixed = bounded_tar + (bounded_tar >> 3) + (bounded_tar >> 8) + (bounded_tar >> 9) + 4
    stored = bounded_tar + (bounded_tar >> 5) + (bounded_tar >> 7) + (bounded_tar >> 11) + 7
    return dict(outer_tar_bytes=total, outer_tar_bound_bytes=bounded_tar,
                upload_bound_bytes=max(zstd, fixed + 18, stored + 18))


def trusted_writer(key):
    repository, sha = os.environ.get('GITHUB_REPOSITORY', ''), os.environ.get('GITHUB_SHA', '')
    if (os.environ.get('GITHUB_ACTIONS') != 'true' or os.environ.get('GITHUB_REF') != 'refs/heads/main'
            or os.environ.get('GITHUB_JOB') != 'native-trusted'
            or os.environ.get('GITHUB_EVENT_NAME') != 'workflow_dispatch'
            or os.environ.get('GITHUB_WORKFLOW_SHA') != sha or not re.fullmatch('[0-9a-f]{40}', sha)
            or os.environ.get('GITHUB_WORKFLOW_REF') != f'{repository}/{WORKFLOW}@refs/heads/main'
            or not key.endswith('-' + sha)
            or os.environ.get('ACTIONS_CACHE_MODE', 'write') not in ('write', 'write-only')):
        raise ValueError('ECS workspace staging requires the current trusted main native writer')


def stage_cache(args, budget, report):
    key = identity()['key']
    preview = bool(getattr(args, 'preview', False))
    report['preview'] = preview
    if not preview:
        trusted_writer(key)
    cache = units._cache_location(args.cache, ROOT, (ROOT / 'target').resolve())
    cache.mkdir(parents=True, exist_ok=True)
    if any(path.name != ARCHIVE_NAME or not path.is_file() or path.is_symlink() for path in cache.iterdir()):
        raise ValueError('ECS archive directory contains unexpected files')
    archive = cache / ARCHIVE_NAME
    archive.unlink(missing_ok=True)
    state = json.loads(temporary(args.state).read_text())
    if state.get('checkout') != str(ROOT) or state.get('target') != str((ROOT / 'target').resolve()):
        raise ValueError('ECS cache state belongs to a different checkout or target')
    report.update(units.cache_stage(state, cache, STAGING_BYTES, measure_only=True), save=False)
    if not report.get('eligible'):
        return
    if not preview:
        maintenance, api = api_client()
        capacity_context(maintenance, api, budget, report)
    with tempfile.TemporaryDirectory(prefix='ecs-units-', dir=cache.parent) as directory:
        staged = Path(directory) / 'units'
        result = units.cache_stage(state, staged, STAGING_BYTES)
        report.update(result, save=False)
        if not result.get('save'):
            return
        report.update(pack_archive(staged, archive))
    report.update(upload_bound(archive))
    # Copying and compression can be significant. Do not reuse the preflight
    # inventory or its capacity observation as a reservation for this save.
    maintenance, api = api_client()
    entries = capacity_context(maintenance, api, budget, report)
    report.update(save_policy(entries, key, report['upload_bound_bytes'],
                              report['effective_repository_budget_bytes']))
    if units._cache_inputs(ROOT) != state['inputs']:
        raise ValueError('source inputs changed while packing the ECS cache')
    report['would_save'] = report['capacity_allowed']
    report['save'] = report['capacity_allowed'] and not preview
    if not report['save'] and not preview:
        archive.unlink(missing_ok=True)


def restore_cache(args, report):
    report.update(restored=False, requested_key=args.requested_key, matched_key=args.matched_key,
                  action_cache_hit={'true': True, 'false': False, '': None}.get(args.cache_hit))
    if args.requested_key is not None or args.matched_key is not None:
        if (not ECS_KEY.fullmatch(args.requested_key or '') or not ECS_KEY.fullmatch(args.matched_key or '')
                or not args.requested_key.startswith('ecs-workspace-v2-')
                or args.requested_key.rsplit('-', 1)[0] != args.matched_key.rsplit('-', 1)[0]):
            raise ValueError('ECS cache miss or incompatible action matched key')
    cache = units._cache_location(args.cache, ROOT, (ROOT / 'target').resolve())
    if not cache.is_dir() or set(path.name for path in cache.iterdir()) != {ARCHIVE_NAME}:
        raise ValueError('ECS cache archive is missing or contains extra files')
    with tempfile.TemporaryDirectory(prefix='ecs-restore-', dir=cache.parent) as directory:
        report.update(unpack_archive(cache / ARCHIVE_NAME, Path(directory)))
        report.update(units.cache_restore(ROOT, Path(directory), STAGING_BYTES))


def observe_save(key, report):
    report.update(saved_confirmed=False, requested_key=key)
    if not ECS_KEY.fullmatch(key or '') or not key.startswith('ecs-workspace-v2-'):
        raise ValueError('ECS cache key is outside the current managed schema')
    maintenance, api = api_client()
    entries = maintenance.validate_inventory(api.list_caches())
    matches = [entry for entry in entries if entry['key'] == key and entry['ref'] == 'refs/heads/main']
    report.update(inventory_complete=True, matching_entries=len(matches))
    if len(matches) != 1 or matches[0]['size_in_bytes'] <= 0:
        report['reason'] = 'saved cache is missing, empty or has ambiguous versions'
        return
    entry = matches[0]
    report.update(saved_confirmed=True, cache_id=entry['id'], cache_version=entry['version'],
                  remote_compressed_bytes=entry['size_in_bytes'], created_at=entry['created_at'])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=('identity', 'preflight', 'restore', 'stage', 'observe-save'))
    parser.add_argument('--cache', type=Path)
    parser.add_argument('--state', type=Path)
    parser.add_argument('--report', type=Path)
    parser.add_argument('--repository-budget-bytes', default=os.environ.get('ECS_CACHE_REPOSITORY_BUDGET_BYTES'))
    parser.add_argument('--requested-key')
    parser.add_argument('--matched-key')
    parser.add_argument('--cache-hit', default='')
    parser.add_argument('--key')
    parser.add_argument('--preview', action='store_true', help='measure complete local staging; never permit remote save')
    args = parser.parse_args()
    started = time.monotonic()
    report = dict(mode=args.mode, save=False)
    try:
        if args.mode == 'identity':
            report.update(identity())
        elif args.mode == 'preflight':
            maintenance, api = api_client()
            capacity_context(maintenance, api, configured_budget(args.repository_budget_bytes), report)
        elif args.mode == 'restore':
            restore_cache(args, report)
        elif args.mode == 'observe-save':
            observe_save(args.key, report)
        elif args.mode == 'stage':
            stage_cache(args, configured_budget(args.repository_budget_bytes), report)
    except (OSError, ValueError, KeyError, TypeError, EOFError, tarfile.TarError, subprocess.SubprocessError) as error:
        print('ECS cache unavailable; full build remains enabled: ' + str(error))
        report.update(save=False, reason=str(error))
        if hasattr(error, 'diagnostics'):
            report['api_diagnostics'] = error.diagnostics
    report['operation_seconds'] = round(time.monotonic() - started, 3)
    if args.report:
        try:
            temporary(args.report).write_text(json.dumps(report, indent=2) + '\n')
        except (OSError, ValueError):
            report['report_write_failed'] = True
    if os.environ.get('GITHUB_OUTPUT'):
        try:
            with open(os.environ['GITHUB_OUTPUT'], 'a') as stream:
                for key, value in report.items():
                    if key in ('save', 'prefix', 'key', 'saved_confirmed', 'upload_bound_bytes'):
                        stream.write(f'{key}={str(value).lower() if isinstance(value, bool) else value}\n')
        except OSError:
            report['output_write_failed'] = True
    print(json.dumps(report, sort_keys=True))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
