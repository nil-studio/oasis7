#!/usr/bin/env python3
"""Optional ordinary/Formal workspace cache with shared bounded admission.

Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7.
Cargo and the existing test artifacts remain authoritative on every cache miss.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys
import tarfile
import tempfile
import time

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent.parent
WORKFLOW = '.github/workflows/rust.yml'
WRITERS = {'ordinary': 'oasis7-test-build-trusted', 'formal': 'formal-test-build-trusted'}
VERSIONS = {'ordinary': 'v1', 'formal': 'v3'}


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'scripts' / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


units = load('workspace_units', 'ci-test-artifacts.py')
archive = load('workspace_archive', 'ci-cache-archive.py')
ARCHIVE_NAME = archive.COMPACT_ARCHIVE_NAME
policy = load('workspace_policy', 'ci-cache-policy.py')
maintenance = load('workspace_maintenance', 'ci-cache-maintenance.py')


def temporary(path):
    return units._cache_location(path, ROOT, (ROOT / 'target').resolve())


def identity(scope):
    metadata = json.loads(subprocess.check_output(
        ['cargo', 'metadata', '--locked', '--no-deps', '--format-version=1'], cwd=ROOT, text=True))
    # Hash effective compiler inputs and restore semantics, not orchestration
    # or capacity policy. Writer authorization still checks WORKFLOW separately.
    files = {'Cargo.toml', 'Cargo.lock', 'scripts/ci-workspace-cache.py',
             'scripts/ci-cache-archive.py', 'scripts/ci-test-artifacts.py',
             'scripts/ci-formal-process-suites.json', 'scripts/ci-rust-cache-toolchains.py'}
    files.update(str(Path(package['manifest_path']).relative_to(ROOT)) for package in metadata['packages'])
    files.update(str(path.relative_to(ROOT)) for path in (ROOT / '.cargo').rglob('*') if path.is_file())
    files.update(name for name in ('rust-toolchain', 'rust-toolchain.toml') if (ROOT / name).is_file())
    env = units.build_environment(scope)
    wasm = env.get('OASIS7_WASM_TOOLCHAIN')
    configuration = dict(scope=scope, selection='libraries' if scope == 'ordinary' else 'all',
        platform=dict(os=platform.system(), arch=platform.machine(), image=env.get('ImageOS')),
        rustc=subprocess.check_output(['rustc', '-vV'], cwd=ROOT, text=True),
        cargo=subprocess.check_output(['cargo', '-V'], cwd=ROOT, text=True),
        wasm_rustc=subprocess.check_output(['rustc', '+' + wasm, '-vV'], cwd=ROOT, text=True) if wasm else None,
        variants=units.build_variants(scope),
        environment={key: value for key, value in env.items()
                     if (key.startswith(('CARGO_', 'RUST', 'OASIS7_WASM', 'CC_', 'CXX_', 'AR_', 'PKG_CONFIG'))
                         or key in ('CC', 'CXX', 'AR', 'CFLAGS', 'CXXFLAGS', 'CPPFLAGS',
                                    'LDFLAGS', 'LIBRARY_PATH', 'CPATH'))
                     and key not in ('CARGO_BUILD_JOBS', 'CARGO_TERM_COLOR', 'CARGO_NET_RETRY')},
        files={name: units.digest(ROOT / name) for name in sorted(files)})
    digest = hashlib.sha256(json.dumps(configuration, sort_keys=True).encode()).hexdigest()
    source = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    if not re.fullmatch('[0-9a-f]{40}', source):
        raise ValueError('workspace source is not a complete Git commit')
    prefix = f'{scope}-workspace-{VERSIONS[scope]}-{digest}-'
    return dict(prefix=prefix, key=prefix + source)


def current_key(scope, key):
    return policy.is_current_key(key) and policy.family_for_key(key) == scope


def trusted_writer(scope, key):
    repository, sha = os.environ.get('GITHUB_REPOSITORY', ''), os.environ.get('GITHUB_SHA', '')
    event = os.environ.get('GITHUB_EVENT_NAME', '')
    if (os.environ.get('GITHUB_ACTIONS') != 'true' or os.environ.get('GITHUB_REF') != 'refs/heads/main'
            or os.environ.get('GITHUB_JOB') != WRITERS[scope]
            or event not in ('push', 'schedule', 'workflow_dispatch')
            or os.environ.get('GITHUB_WORKFLOW_SHA') != sha or not re.fullmatch('[0-9a-f]{40}', sha)
            or os.environ.get('GITHUB_WORKFLOW_REF') != f'{repository}/{WORKFLOW}@refs/heads/main'
            or not current_key(scope, key) or not key.endswith('-' + sha)
            or os.environ.get('ACTIONS_CACHE_MODE', 'write') not in ('write', 'write-only')):
        raise ValueError('workspace staging requires the matching current trusted main Rust writer')
    if event == 'workflow_dispatch':
        try:
            payload = json.loads(Path(os.environ.get('GITHUB_EVENT_PATH', '')).read_text())
            full = payload.get('inputs', {}).get('run_mode') == 'full'
        except (OSError, ValueError, AttributeError):
            full = False
        if not full:
            raise ValueError('manual workspace staging requires run_mode=full')


def api_client():
    return maintenance.GitHubApi(os.environ.get('GITHUB_REPOSITORY'), os.environ.get('CACHE_INVENTORY_TOKEN'))


def read_state(args):
    state = json.loads(temporary(args.state).read_text())
    if (state.get('checkout') != str(ROOT) or state.get('target') != str((ROOT / 'target').resolve())
            or state.get('selection', 'all') != ('libraries' if args.scope == 'ordinary' else 'all')):
        raise ValueError('workspace state has a different checkout, target or unit selection')
    return state


def bounded_library_state(state, maximum, report):
    """Retain whole recorded units, never an arbitrary prefix of their files.

    Ordinary full runs can collect many feature variants. Prefer the most
    recently built units (the full-fast variants run last), then stable paths.
    This is a packing heuristic, not an assertion about compilation savings.
    Generators, unhashed outputs and cross-unit hard links stay together.
    """
    if (state.get('schema_version') != units.CACHE_SCHEMA or state.get('complete') is not True
            or state.get('selection') != 'libraries' or type(maximum) is not int or maximum <= 0):
        raise ValueError('bounded selection requires a complete library state and positive budget')
    target = Path(state['target'])
    files, directories = set(state['files']), set(state['directories'])
    fingerprints, builds = {}, set()
    for name in directories:
        path = Path(name)
        units._cache_path(target, name)
        match = re.fullmatch(r'(.+)-([0-9a-f]{16})', path.name)
        if not match or path.parent.name not in ('.fingerprint', 'build'):
            raise ValueError('unsupported bounded Cargo unit directory: ' + name)
        unit = (path.parent.parent.as_posix(), match[2])
        if path.parent.name == 'build':
            builds.add(name)
        elif unit in fingerprints:
            raise ValueError('ambiguous bounded Cargo unit hash: ' + name)
        else:
            fingerprints[unit] = name
    # The existing recorder has already resolved the unique unhashed library
    # fingerprint. Pin every observed unit of that package rather than guessing
    # which public filename belongs to which feature variant.
    unhashed, pinned_packages = set(), set()
    for entry in state.get('unhashed_libraries', {}).values():
        profile = Path(entry['profile'])
        pinned_packages.add((profile.as_posix(), entry['package']))
        for filename in entry['filenames']:
            output = Path(filename)
            unhashed.update((filename, (profile / 'deps' / output.name).as_posix()))
            if output.parent.name != 'deps':
                unhashed.add(output.with_suffix('.d').as_posix())
        unhashed.add((profile / 'deps' / (entry['name'] + '.d')).as_posix())
    groups = {name: dict(files=set(), directories={name}) for name in fingerprints.values()}
    support = dict(files=set(), directories=set(builds))
    build_units = {(Path(name).parent.parent.as_posix(), Path(name).name) for name in builds}
    pinned = set()
    for unit, name in fingerprints.items():
        package = Path(name).name.rsplit('-', 1)[0]
        if (unit[0], package) in pinned_packages or (unit[0], Path(name).name) in build_units:
            pinned.add(name)
    # Public library aliases have no hash. Bind them through the same inode
    # as a recorded deps output, then attach only their recorded dep-info.
    public_owners, recorded_inodes = {}, {}
    for name in files:
        path = Path(name)
        info = units._cache_path(target, name).stat()
        match = re.fullmatch(r'.+-([0-9a-f]{16})(?:\..+)?', path.name)
        unit = (path.parent.parent.as_posix(), match[1]) if match else None
        if path.parent.name == 'deps' and unit in fingerprints:
            key = (path.parent.parent.as_posix(), info.st_dev, info.st_ino)
            recorded_inodes.setdefault(key, set()).add(fingerprints[unit])
    for name in files:
        path = Path(name)
        if path.parent.name == 'deps' or path.suffix == '.d' or name in unhashed:
            continue
        info = units._cache_path(target, name).stat()
        owners = recorded_inodes.get((path.parent.as_posix(), info.st_dev, info.st_ino), set()).copy()
        if len(owners) > 1:
            raise ValueError('ambiguous bounded public Cargo output: ' + name)
        if owners:
            owner = owners.pop()
            public_owners[name] = owner
            dep_info = path.with_suffix('.d').as_posix()
            if dep_info in files:
                previous = public_owners.setdefault(dep_info, owner)
                if previous != owner:
                    raise ValueError('ambiguous bounded public Cargo dep-info: ' + dep_info)
    for name in files:
        path = Path(name)
        units._cache_path(target, name)
        if any(path.is_relative_to(Path(directory)) for directory in builds) or name in unhashed:
            support['files'].add(name)
            continue
        if name in public_owners:
            groups[public_owners[name]]['files'].add(name)
            continue
        match = re.fullmatch(r'.+-([0-9a-f]{16})(?:\..+)?', path.name)
        unit = (path.parent.parent.as_posix(), match[1]) if match else None
        if path.parent.name != 'deps' or unit not in fingerprints:
            raise ValueError('unbound bounded Cargo output: ' + name)
        groups[fingerprints[unit]]['files'].add(name)
    # An unassigned fingerprint is allowed only for a pinned generator or
    # unhashed library, not as a fabricated complete unit without outputs.
    if any(not group['files'] and name not in pinned for name, group in groups.items()):
        raise ValueError('bounded Cargo fingerprint has no recorded output')
    groups['support'] = support
    pinned.add('support')
    inodes = {}
    for name, group in groups.items():
        selected = {file: units._cache_path(target, file).stat() for file in group['files']}
        for directory in group['directories']:
            path = units._cache_path(target, directory)
            children, _ = units._cache_walk(path)
            selected.update({(Path(directory) / child).as_posix(): info for child, info in children.items()})
        group['bytes'] = sum(info.st_size for info in selected.values())
        group['stamp'] = max((info.st_mtime_ns for info in selected.values()), default=0)
        for info in selected.values():
            inodes.setdefault((info.st_dev, info.st_ino), set()).add(name)
    # Do not break aliases crossing two otherwise separate fingerprint units.
    for owners in inodes.values():
        if len(owners) > 1:
            pinned.update(owners)
    retained = set(pinned)
    used = sum(groups[name]['bytes'] for name in retained)
    if used > maximum:
        raise ValueError('mandatory Cargo support units exceed bounded staging budget')
    ordered = sorted(set(groups) - pinned, key=lambda name: (-groups[name]['stamp'], name))
    for name in ordered:
        if used + groups[name]['bytes'] <= maximum:
            retained.add(name)
            used += groups[name]['bytes']
    chosen_files = sorted({file for name in retained for file in groups[name]['files']})
    chosen_dirs = sorted({directory for name in retained for directory in groups[name]['directories']})
    if not chosen_files or not chosen_dirs:
        raise ValueError('no complete Cargo library unit fits bounded staging budget')
    omitted = sorted(set(groups) - retained)
    report.update(selection_policy='whole-units-newest-v1', selection_budget_bytes=maximum,
                  selection_candidate_bytes=sum(group['bytes'] for group in groups.values()),
                  selection_retained_bytes=used, retained_unit_groups=len(retained - {'support'}),
                  omitted_unit_groups=len(omitted), omitted_unit_samples=omitted[:50],
                  omitted_unit_samples_truncated=len(omitted) > 50)
    # Preserve the complete input snapshot and ownership map. An omitted unit
    # has no restored fingerprint; Cargo remains responsible for rebuilding it
    # and propagating its invalidation to all dependent units.
    return dict(state, files=chosen_files, directories=chosen_dirs)


def measure_cache(args, report):
    state = read_state(args)
    report.update(units.cache_stage(state, temporary(args.cache),
                  policy.FAMILIES[args.scope]['staging_bytes'], measure_only=True), save=False)
    report['full_selection_raw_bytes'] = report.get('raw_bytes')
    if (args.scope == 'ordinary' and not report.get('eligible')
            and report.get('raw_bytes', 0) > policy.FAMILIES['ordinary']['staging_bytes']):
        state = bounded_library_state(state, policy.FAMILIES['ordinary']['staging_bytes'], report)
        report.update(units.cache_stage(state, temporary(args.cache),
                      policy.FAMILIES['ordinary']['staging_bytes'], measure_only=True), save=False)
    return state


def stage_cache(args, budget, report):
    key = identity(args.scope)['key']
    trusted_writer(args.scope, key)
    # The trusted schedule/manual-full workflow also writes the shared native
    # dependency cache later. Keep its rotation space out of workspace uploads;
    # the dependency writer still admits its actual final archive independently.
    reserved = (policy.DEPENDENCY_ROTATION_HEADROOM_BYTES
                if os.environ.get('GITHUB_EVENT_NAME') in ('schedule', 'workflow_dispatch') else 0)
    report['reserved_repository_bytes'] = reserved
    maximum = policy.FAMILIES[args.scope]['staging_bytes']
    cache = temporary(args.cache)
    state = measure_cache(args, report)
    if not report.get('eligible'):
        report.setdefault('reason', 'workspace selection exceeds staging budget or is empty')
        return
    policy.capacity_context(maintenance, api_client(), budget, report)
    if not report['available_repository_bytes']:
        report.update(save=False, reason='repository cache capacity gap')
        return
    cache.mkdir(parents=True, exist_ok=True)
    if any(path.name != ARCHIVE_NAME or not path.is_file() or path.is_symlink() for path in cache.iterdir()):
        raise ValueError('workspace archive directory contains unexpected files')
    payload = cache / ARCHIVE_NAME
    payload.unlink(missing_ok=True)
    report['pack_attempts'] = []
    selection_maximum = maximum
    # At most three local packs; never retry remote writes or capacity failures.
    # Raw bytes do not predict compression. A measured upload miss narrows the
    # ordinary unit selection and is checked again by the same archive bounds.
    for attempt in range(3 if args.scope == 'ordinary' else 1):
        with tempfile.TemporaryDirectory(prefix='workspace-stage-', dir=cache.parent) as directory:
            staged = Path(directory) / 'units'
            result = units.cache_stage(state, staged, maximum)
            report.update(result, save=False)
            if not result.get('save'):
                return
            report.update(archive.pack_archive(staged, payload, maximum, compact=True))
        report.update(archive.upload_bound(payload, root=ROOT))
        bound = report['upload_bound_bytes']
        limit = policy.FAMILIES[args.scope]['max_upload_bytes']
        report['pack_attempts'].append(dict(raw_bytes=report['raw_bytes'], upload_bound_bytes=bound))
        if bound <= limit or args.scope != 'ordinary' or attempt == 2:
            break
        selection_maximum = min(selection_maximum - 1, report['raw_bytes'] * limit * 9 // (bound * 10))
        payload.unlink(missing_ok=True)
        state = bounded_library_state(state, selection_maximum, report)
    # The preflight observation is not a reservation; staging can take minutes.
    entries = policy.capacity_context(maintenance, api_client(), budget, report)
    report.update(policy.save_policy(entries, key, report['upload_bound_bytes'],
                                     report['effective_repository_budget_bytes'],
                                     reserved_repository_bytes=reserved))
    if units._cache_inputs(ROOT) != state['inputs']:
        raise ValueError('source inputs changed while packing the workspace cache')
    report['save'] = report['capacity_allowed']
    if not report['save']:
        payload.unlink(missing_ok=True)


def restore_cache(args, report):
    report.update(restored=False, requested_key=args.requested_key, matched_key=args.matched_key,
                  action_cache_hit={'true': True, 'false': False, '': None}.get(args.cache_hit))
    if (not current_key(args.scope, args.requested_key) or not current_key(args.scope, args.matched_key)
            or args.requested_key.rsplit('-', 1)[0] != args.matched_key.rsplit('-', 1)[0]):
        raise ValueError('workspace cache miss or incompatible action matched key')
    maximum = policy.FAMILIES[args.scope]['staging_bytes']
    cache = temporary(args.cache)
    if not cache.is_dir() or set(path.name for path in cache.iterdir()) != {ARCHIVE_NAME}:
        raise ValueError('workspace cache archive is missing or contains extra files')
    with tempfile.TemporaryDirectory(prefix='workspace-restore-', dir=cache.parent) as directory:
        report.update(archive.unpack_archive(cache / ARCHIVE_NAME, Path(directory), maximum))
        report.update(units.cache_restore(ROOT, Path(directory), maximum))


def observe_save(scope, key, report):
    report.update(saved_confirmed=False, requested_key=key)
    if not current_key(scope, key):
        raise ValueError('workspace key is outside the current family schema')
    entries = maintenance.validate_inventory(api_client().list_caches())
    matches = [entry for entry in entries if entry['key'] == key and entry['ref'] == 'refs/heads/main']
    report.update(inventory_complete=True, matching_entries=len(matches))
    if len(matches) != 1 or matches[0]['size_in_bytes'] <= 0:
        report['reason'] = 'saved cache is missing, empty or has ambiguous versions'
        return
    entry = matches[0]
    report.update(saved_confirmed=True, cache_id=entry['id'], cache_version=entry['version'],
                  remote_compressed_bytes=entry['size_in_bytes'], created_at=entry['created_at'])


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=('identity', 'preflight', 'measure', 'restore', 'stage', 'observe-save'))
    parser.add_argument('--scope', choices=tuple(WRITERS), required=True)
    parser.add_argument('--cache', type=Path)
    parser.add_argument('--state', type=Path)
    parser.add_argument('--report', type=Path)
    parser.add_argument('--repository-budget-bytes', default=os.environ.get('CI_CACHE_REPOSITORY_BUDGET_BYTES'))
    parser.add_argument('--requested-key')
    parser.add_argument('--matched-key')
    parser.add_argument('--cache-hit', default='')
    parser.add_argument('--key')
    args = parser.parse_args(argv)
    started = time.monotonic()
    report = dict(mode=args.mode, scope=args.scope, save=False)
    try:
        if args.mode == 'identity':
            report.update(identity(args.scope))
        elif args.mode == 'preflight':
            policy.capacity_context(maintenance, api_client(), policy.configured_budget(args.repository_budget_bytes), report)
        elif args.mode == 'measure':
            measure_cache(args, report)
        elif args.mode == 'restore':
            restore_cache(args, report)
        elif args.mode == 'stage':
            stage_cache(args, policy.configured_budget(args.repository_budget_bytes), report)
        elif args.mode == 'observe-save':
            observe_save(args.scope, args.key, report)
    except (OSError, ValueError, KeyError, TypeError, EOFError, tarfile.TarError, subprocess.SubprocessError) as error:
        print('Workspace cache unavailable; full build remains enabled: ' + str(error))
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
