#!/usr/bin/env python3
"""Optional fixed-profile ECS workspace cache; cache failures never skip Cargo.

Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7.
"""
import argparse
import hashlib
import importlib.util
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
WORKFLOW = '.github/workflows/ecs-packages.yml'


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'scripts' / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


units = load('ecs_units', 'ci-test-artifacts.py')
policy = load('ecs_policy', 'ci-cache-policy.py')
archive_codec = load('ecs_archive', 'ci-cache-archive.py')
STAGING_BYTES = policy.FAMILIES['ecs']['staging_bytes']
FAMILY_BYTES = policy.FAMILIES['ecs']['max_bytes']
GENERATIONS = policy.FAMILIES['ecs']['keep_generations']
ECS_KEY = policy.FAMILIES['ecs']['managed_pattern']
ARCHIVE_NAME = archive_codec.ARCHIVE_NAME
MANIFEST_BYTES = archive_codec.MANIFEST_BYTES
MAX_FILES = archive_codec.MAX_FILES


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
             'scripts/package-ecs.py', 'scripts/ci-test-artifacts.py',
             'scripts/ci-cache-policy.py', 'scripts/ci-cache-archive.py'}
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


quota_bytes = policy.quota_bytes
configured_budget = policy.configured_budget
capacity_context = policy.capacity_context
save_policy = policy.save_policy


def api_client():
    maintenance = load('ecs_maintenance', 'ci-cache-maintenance.py')
    return maintenance, maintenance.GitHubApi(os.environ.get('GITHUB_REPOSITORY'), os.environ.get('GH_TOKEN'))


def archive_manifest(metadata):
    return archive_codec.archive_manifest(metadata, max_bytes=STAGING_BYTES)


def pack_archive(staged, destination):
    return archive_codec.pack_archive(staged, destination, max_bytes=STAGING_BYTES)


def unpack_archive(archive, destination):
    return archive_codec.unpack_archive(archive, destination, max_bytes=STAGING_BYTES)


def upload_bound(archive):
    return archive_codec.upload_bound(archive, root=ROOT)


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
