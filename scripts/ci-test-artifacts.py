#!/usr/bin/env python3
"""Build once and run native test executables without Cargo on consumers."""
import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import time
import tomllib

ROOT = Path(__file__).resolve().parent.parent
CONFIG = ROOT / 'scripts/ci-formal-process-suites.json'
SHARDS = ('runtime-protocol', 'world-history', 'healthy-set', 'storage-recovery', 'lifecycle-cli')
DEFAULT_SNAPSHOT_REGRESSIONS = (
    ('viewer-snapshot-tests', 'snapshot_progress::'),
    ('snapshot-tests', 'snapshot_player_gameplay_execution_state_backfills_from_legacy_fields'),
)


def output(command):
    return subprocess.check_output(command, text=True, cwd=ROOT).strip()


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def inventory(path, ignored=False, env=None):
    command = [str(path), '--list', '--format', 'terse']
    if ignored:
        command.append('--ignored')
    text = subprocess.check_output(command, text=True, env=env, timeout=120)
    return sorted(line[:-6] for line in text.splitlines() if line.endswith(': test'))


def resolve_suites(config, binaries):
    suites = {name: list(entries) for name, entries in config['suites'].items()}
    assigned = set()
    for entries in suites.values():
        for entry in entries:
            key = (entry['binary'], entry['name'])
            binary = binaries[entry['binary']]
            if key in assigned or entry['name'] not in binary['ignored_tests']:
                raise ValueError(f'duplicate or missing ignored acceptance: {key}')
            assigned.add(key)
    for binary, names in config.get('auxiliary_entries', {}).items():
        for name in names:
            if name not in binaries[binary]['tests'] or (binary, name) in assigned:
                raise ValueError(f'missing or independently scheduled auxiliary: {binary}: {name}')
    for binary, filters in config['legacy_ignored_filters'].items():
        expected = {name for name in binaries[binary]['ignored_tests']
                    if any(part in name for part in filters)}
        actual = {name for owner, name in assigned if owner == binary}
        if expected != actual:
            raise ValueError(f'ignored coverage changed: {binary}: {expected ^ actual}')
    # Preserve the three original formal commands with their binary environment.
    # These are deliberately retained until equivalence to ordinary tests is proven.
    for binary, suite in config['environment_formal_batches'].items():
        ordinary = set(binaries[binary]['tests']) - set(binaries[binary]['ignored_tests'])
        for name in sorted(ordinary):
            if 'formal' in name:
                suites[suite].append(dict(binary=binary, name=name, ignored=False))
    for suite, scripts in config['shell_tests'].items():
        suites[suite].extend(dict(script=script) for script in scripts)
    return suites


def resolve_shards(config, suites):
    """Move whole cases; logical suite ownership and coverage remain unchanged."""
    overrides = {(binary, name): shard for binary, names in config.get('case_shards', {}).items()
                 for name, shard in names.items()}
    if set(overrides.values()) - set(SHARDS):
        raise ValueError('unknown physical shard')
    result = {name: [] for name in SHARDS}
    used, assigned = set(), set()
    for suite, entries in suites.items():
        for entry in entries:
            key = (entry.get('binary', 'script'), entry.get('name', entry.get('script')))
            if key in assigned:
                raise ValueError(f'case scheduled more than once: {key}')
            assigned.add(key)
            shard = overrides.get(key, suite)
            if key in overrides:
                used.add(key)
            if shard not in result:
                raise ValueError(f'unknown physical shard: {shard}')
            result[shard].append(dict(entry, suite=suite))
    if used != set(overrides):
        raise ValueError(f'shard override does not name an actual case: {set(overrides) - used}')
    return result


def build_variants(scope):
    required_features = ['test_tier_required']
    # Separate variants preserve the old snapshot regressions' default features.
    variants = [
        ('viewer', ['build', '-p', 'oasis7_viewer_runtime', '--bin', 'oasis7_viewer_live', '--features', 'test_tier_required'], required_features),
        ('required', ['test', '-p', 'oasis7', '-p', 'oasis7_simulator', '-p', 'oasis7_runtime', '-p', 'oasis7_world_state', '-p', 'oasis7_world_projection', '-p', 'oasis7_world_service', '-p', 'scenario_test_runner', '-p', 'oasis7_chain_runtime', '-p', 'oasis7_client_cli', '-p', 'oasis7_launcher_backend', '-p', 'oasis7_newapi_bridge_service', '-p', 'oasis7_ops_tools', '-p', 'oasis7_provider_local_bridge', '-p', 'oasis7_provider_tools', '-p', 'oasis7_viewer_runtime', '-p', 'oasis7_observability', '-p', 'oasis7_hosted_access', '-p', 'oasis7_letai_credit_adapter', '-p', 'oasis7_launcher_config', '-p', 'oasis7_wasm_materializer', '-p', 'oasis7_game_client', '-p', 'oasis7_cas', '-p', 'oasis7_formal_protocol', '--tests', '--features', 'test_tier_required', '--no-run'], required_features),
        ('runtime', ['build', '-p', 'oasis7_chain_runtime', '--bin', 'oasis7_chain_runtime', '--features', 'test_tier_required'], required_features),
        ('endpoint', ['build', '-p', 'oasis7_distfs', '--bin', 'oasis7_durability_endpoint', '--release'], []),
        ('distfs-tests', ['test', '-p', 'oasis7_distfs', '--lib', '--no-run'], []),
        ('snapshot-tests', ['test', '-p', 'oasis7', '--lib', '--no-run'], []),
        ('viewer-snapshot-tests', ['test', '-p', 'oasis7_viewer_runtime', '--lib', '--no-run'], []),
    ]
    if scope == 'all':
        return variants
    if scope == 'ordinary':
        return [v for v in variants if v[0] in ('viewer', 'required', 'snapshot-tests', 'viewer-snapshot-tests')]
    if scope in ('formal', 'storage'):
        # Keep both core libraries and the exact Chain binary harness. Chain
        # configuration library tests belong to the ordinary required variant;
        # integration tests, viewer executables and snapshot variants run elsewhere.
        # The CI-only marker reproduces shared dependency features from all
        # required roots without compiling their unrelated harnesses.
        features = ['test_tier_required', 'ci_formal_required_features']
        core = ('formal-required', ['test', '-p', 'oasis7', '-p', 'oasis7_chain_runtime',
                '--lib', '--bins', '--features', ','.join(features), '--no-run'], features)
        result = [core, *(v for v in variants if v[0] in ('runtime', 'endpoint', 'distfs-tests'))]
        if scope == 'storage':
            result.append(('storage-libs', ['test', '-p', 'oasis7_cas', '-p', 'oasis7_formal_protocol',
                '-p', 'oasis7_runtime', '-p', 'oasis7_world_state', '-p', 'oasis7_world_projection', '-p', 'oasis7_world_service', '--lib', '--no-run'], []))
        return result
    raise ValueError('unknown build scope: ' + scope)


def artifact_logical_name(variant, package, target, test):
    """Formal aliases identify package, target kind and name, never name alone."""
    name, kinds = target['name'], tuple(target['kind'])
    if variant == 'storage-libs':
        return 'storage-lib-' + name
    if variant not in ('required', 'formal-required'):
        return variant
    if test:
        aliases = {
            ('oasis7_chain_runtime', ('lib',), 'oasis7_chain_runtime'): 'runtime-tests',
            ('oasis7', ('lib',), 'oasis7'): 'oasis7-lib-tests',
        }
        return aliases.get((package, kinds, name),
                           'required-test-' + package + '-' + '-'.join(kinds) + '-' + name)
    return 'required-executable-' + package + '-' + '-'.join(kinds) + '-' + name


def build_environment(scope):
    env = dict(os.environ)
    env.pop('RUSTC_WRAPPER', None)
    if scope == 'storage':
        # Keep test checks while avoiding debug-code cost in multi-hour workloads.
        # Release endpoint policy is independent and remains unchanged.
        for profile in ('DEV', 'TEST'):
            env[f'CARGO_PROFILE_{profile}_OPT_LEVEL'] = '2'
            env[f'CARGO_PROFILE_{profile}_DEBUG_ASSERTIONS'] = 'true'
            env[f'CARGO_PROFILE_{profile}_OVERFLOW_CHECKS'] = 'true'
    return env


def build_cache_dependencies(scope):
    """Compile the producer's exact variants without copying/running a bundle."""
    env = build_environment(scope)
    for name, args, _ in build_variants(scope):
        command = ['cargo', *args, '--locked']
        print('cache_build_variant=' + name + ' command=' + ' '.join(command), flush=True)
        subprocess.run(command, env=env, cwd=ROOT, check=True)


def validate_profile(logical, binary, scope=None):
    if logical == 'endpoint':
        profile = binary['profile']
        if (binary['target'] != 'oasis7_durability_endpoint' or binary['test']
                or profile['opt_level'] != '3' or profile['debug_assertions']
                or Path(binary['original_executable']).parent.name != 'release'):
            raise ValueError('expected optimized release endpoint artifact')
    elif scope == 'storage':
        profile = binary.get('profile', {})
        if (profile.get('opt_level') != '2' or profile.get('debug_assertions') is not True
                or profile.get('overflow_checks') is not True):
            raise ValueError('expected optimized storage artifact with assertions and overflow checks')


def build(bundle, scope='all', cache_state_path=None):
    bundle.mkdir(parents=True, exist_ok=False)
    (bundle / 'bin').mkdir()
    (bundle / 'lib').mkdir()
    env = build_environment(scope)
    cache_state = None
    if cache_state_path:
        try:
            cache_state = cache_begin(ROOT)
        except (OSError, ValueError) as error:
            cache_state = dict(schema_version=CACHE_SCHEMA, checkout=str(ROOT),
                               target=str((ROOT / 'target').resolve()), complete=False, error=str(error))
    binaries, timings = {}, []
    variants = build_variants(scope)
    for variant, args, features in variants:
        started = time.monotonic()
        local_units = local_fresh = 0
        command = ['cargo', *args, '--locked', '--message-format=json-render-diagnostics']
        print('+ ' + ' '.join(command), flush=True)
        with subprocess.Popen(command, stdout=subprocess.PIPE, text=True, env=env, cwd=ROOT) as process:
            for line in process.stdout:
                try:
                    artifact = json.loads(line)
                except ValueError:
                    continue
                if artifact.get('reason') == 'compiler-message':
                    print(artifact['message'].get('rendered', ''), end='', flush=True)
                if cache_state is not None and not cache_state.get('error'):
                    try:
                        cache_record(cache_state, artifact)
                    except (OSError, ValueError, KeyError, TypeError) as error:
                        cache_state['error'] = str(error)
                if (artifact.get('reason') == 'compiler-artifact'
                        and Path(artifact['manifest_path']).is_relative_to(ROOT)):
                    local_units += 1
                    local_fresh += bool(artifact['fresh'])
                executable = artifact.get('executable')
                if artifact.get('reason') != 'compiler-artifact' or not executable:
                    continue
                target = artifact['target']
                test = artifact['profile']['test']
                name = target['name']
                manifest = Path(artifact['manifest_path'])
                package = tomllib.loads(manifest.read_text())['package']['name']
                logical = artifact_logical_name(variant, package, target, test)
                if logical in binaries:
                    raise ValueError(f'duplicate executable: {logical}')
                source = Path(executable)
                destination = bundle / 'bin' / logical
                shutil.copy2(source, destination)
                # Cargo uses the package directory as test cwd, and exposes its binaries.
                binaries[logical] = dict(path=str(destination.relative_to(bundle)), sha256=digest(destination),
                    profile=artifact['profile'], features=artifact['features'], requested_features=features,
                    test=test, package=package, target=name, target_kind=target['kind'],
                    original_executable=str(source), cwd=str(manifest.parent.relative_to(ROOT)),
                    ordinary=variant == 'required' and test, variant=variant,
                    bytes=destination.stat().st_size, libraries=[])
                validate_profile(logical, binaries[logical], scope)
                if sys.platform == 'linux':
                    linked = subprocess.run(['ldd', str(source)], text=True, capture_output=True)
                    for line in linked.stdout.splitlines():
                        fields = line.split()
                        if '=>' in fields and len(fields) > 2 and fields[2].startswith(str(ROOT / 'target') + '/'):
                            library = Path(fields[2])
                            shutil.copy2(library, bundle / 'lib' / library.name)
                            binaries[logical]['libraries'].append('lib/' + library.name)
            if process.wait() != 0:
                raise RuntimeError(f'build failed: {variant}')
        timings.append(dict(variant=variant, elapsed_seconds=round(time.monotonic() - started, 3),
                            local_units=local_units, local_fresh=local_fresh, local_rebuilt=local_units - local_fresh))
        print('native_build=' + json.dumps(timings[-1]), flush=True)
    manifest = dict(schema_version=2, scope=scope, tested_sha=output(['git', 'rev-parse', 'HEAD']),
        source_sha=os.environ.get('SOURCE_SHA', output(['git', 'rev-parse', 'HEAD'])),
        producer_run_id=os.environ.get('GITHUB_RUN_ID', 'local'),
        producer_attempt=os.environ.get('GITHUB_RUN_ATTEMPT', '1'),
        rust_toolchain=os.environ.get('RUST_TOOLCHAIN', 'local'),
        rustc_version=output(['rustc', '--version']),
        target_triple=output(['rustc', '-vV']).split('host: ')[1].splitlines()[0],
        runner_image=os.environ.get('ImageOS', sys.platform), checkout=str(ROOT), binaries=binaries,
        config_sha256=digest(CONFIG), build_timings=timings)
    list_env = dict(env, LD_LIBRARY_PATH=str(bundle / 'lib') + os.pathsep + env.get('LD_LIBRARY_PATH', ''))
    for logical, binary in binaries.items():
        if binary['test']:
            binary['tests'] = inventory(bundle / binary['path'], env=list_env)
            binary['ignored_tests'] = inventory(bundle / binary['path'], True, list_env)
            binary['test_list_sha256'] = hashlib.sha256(json.dumps(binary['tests']).encode()).hexdigest()
    config = json.loads(CONFIG.read_text())
    manifest['suites'] = resolve_suites(config, binaries) if scope != 'ordinary' else {}
    if scope != 'ordinary':
        resolve_shards(config, manifest['suites'])
    manifest['libraries'] = {str(p.relative_to(bundle)): digest(p) for p in (bundle / 'lib').iterdir()}
    manifest['included_binaries'] = sorted(binaries)
    manifest['selection'] = None
    (bundle / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    if cache_state is not None:
        try:
            if not cache_state.get('error'):
                cache_finish(cache_state)
        except (OSError, ValueError) as error:
            cache_state['error'] = str(error)
        try:
            cache_state_path.parent.mkdir(parents=True, exist_ok=True)
            cache_state_path.write_text(json.dumps(cache_state, sort_keys=True) + '\n')
        except OSError as error:
            print('formal_workspace_cache=' + json.dumps(dict(save=False, reason=str(error))), flush=True)


def selected_entries(manifest, config, suite=None, shard=None):
    if bool(suite) == bool(shard):
        raise ValueError('select exactly one logical suite or physical shard')
    if shard:
        return resolve_shards(config, manifest['suites'])[shard]
    return [dict(entry, suite=suite) for entry in manifest['suites'][suite]]


def dependency_closure(manifest, config, suite=None, shard=None):
    binaries = manifest['binaries']
    if suite == 'ordinary':
        required = {name for name, binary in binaries.items()
                    if (binary.get('ordinary') and set(binary['tests']) - set(binary['ignored_tests']))
                    or name in ('viewer', 'snapshot-tests', 'viewer-snapshot-tests') or name.startswith('required-executable-')}
        for owner, _ in DEFAULT_SNAPSHOT_REGRESSIONS:
            if owner not in required:
                raise ValueError('missing default-feature snapshot harness: ' + owner)
        return required
    required = set()
    for entry in selected_entries(manifest, config, suite, shard):
        if 'script' in entry:
            try:
                required.update(config['script_dependencies'][entry['script']])
            except KeyError as error:
                raise ValueError('missing shell dependency declaration') from error
        else:
            required.add(entry['binary'])
    pending = list(required)
    while pending:
        name = pending.pop()
        if name not in binaries:
            raise ValueError('missing executable dependency: ' + name)
        for dependency in config.get('binary_dependencies', {}).get(name, []):
            if dependency not in required:
                required.add(dependency)
                pending.append(dependency)
    return required


def checked_file(bundle, name, sha):
    path = bundle / name
    if not path.resolve().is_relative_to(bundle.resolve()) or not path.is_file() or digest(path) != sha:
        raise ValueError('missing, invalid or corrupt executable/library')
    return path


def pack(bundle, archive_path, suite=None, shard=None):
    """Publish one consumption closure; the producer's complete inventory remains auditable."""
    manifest = json.loads((bundle / 'manifest.json').read_text())
    config = json.loads(CONFIG.read_text())
    included = dependency_closure(manifest, config, suite, shard)
    if not included:
        raise ValueError('empty native artifact closure')
    manifest['included_binaries'] = sorted(included)
    manifest['selection'] = {'shard': shard} if shard else {'suite': suite}
    libraries = {name for logical in included for name in manifest['binaries'][logical].get('libraries', [])}
    manifest['libraries'] = {name: manifest['libraries'][name] for name in sorted(libraries)}
    files = [(manifest['binaries'][name]['path'], manifest['binaries'][name]['sha256']) for name in sorted(included)]
    files += list(manifest['libraries'].items())
    checked = [(name, sha, checked_file(bundle, name, sha)) for name, sha in files]
    manifest_bytes = (json.dumps(manifest, indent=2) + '\n').encode()
    archive_path.parent.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()
    # Low compression reduces network traffic without a new tool dependency.
    # Equal bytes use tar hardlink records, never hardlinks to the mutable Cargo target.
    with archive_path.open('wb') as destination, gzip.GzipFile(fileobj=destination, mode='wb', compresslevel=1, mtime=0) as compressed:
        with tarfile.open(fileobj=compressed, mode='w|') as archive:
            metadata = tarfile.TarInfo('manifest.json')
            metadata.size, metadata.mode = len(manifest_bytes), 0o644
            archive.addfile(metadata, io.BytesIO(manifest_bytes))
            seen = {}
            for name, sha, path in checked:
                metadata = archive.gettarinfo(str(path), arcname=name)
                identity = (sha, metadata.mode)
                if identity in seen:
                    metadata.type, metadata.linkname, metadata.size = tarfile.LNKTYPE, seen[identity], 0
                    archive.addfile(metadata)
                else:
                    seen[identity] = name
                    with path.open('rb') as stream:
                        archive.addfile(metadata, stream)
    report = dict(selection=manifest['selection'], raw_bytes=len(manifest_bytes) + sum(p.stat().st_size for _, _, p in checked),
                  compressed_bytes=archive_path.stat().st_size, elapsed_seconds=round(time.monotonic() - started, 3),
                  binaries=[dict(name=name, bytes=manifest['binaries'][name].get('bytes', (bundle / manifest['binaries'][name]['path']).stat().st_size),
                                 sha256=manifest['binaries'][name]['sha256'],
                                 test_count=len(manifest['binaries'][name].get('tests', [])),
                                 ignored_count=len(manifest['binaries'][name].get('ignored_tests', []))) for name in sorted(included)])
    archive_path.with_suffix('.size.json').write_text(json.dumps(report, indent=2) + '\n')
    print('native_artifact=' + json.dumps(report), flush=True)
    if os.environ.get('GITHUB_STEP_SUMMARY'):
        with Path(os.environ['GITHUB_STEP_SUMMARY']).open('a') as summary:
            summary.write(f"\n### Native artifact: {shard or suite}\n\nRaw bytes: {report['raw_bytes']}; compressed bytes: {report['compressed_bytes']}; packaging seconds: {report['elapsed_seconds']}.\n\n| Binary | Bytes | Tests | Ignored |\n| --- | ---: | ---: | ---: |\n")
            for entry in sorted(report['binaries'], key=lambda entry: -entry['bytes']):
                summary.write(f"| {entry['name']} | {entry['bytes']} | {entry['test_count']} | {entry['ignored_count']} |\n")
    return report


def cache_budget(target, maximum=8 * 1024 ** 3):
    # Read-only accounting. An oversized disposable cache must not fail tests
    # or delete Cargo fingerprints to force it under the limit.
    size = sum(path.stat().st_size for path in target.rglob('*') if path.is_file()) if target.is_dir() else 0
    return dict(raw_bytes=size, max_bytes=maximum, save=0 < size <= maximum)


CACHE_SCHEMA = 2


def _cache_path(root, name):
    path = Path(name)
    if (not isinstance(name, str) or not name or name == '.' or path.is_absolute()
            or path.as_posix() != name or '..' in path.parts or '\\' in name):
        raise ValueError('invalid cache path')
    current = root
    for part in path.parts:
        current = current / part
        if current.is_symlink():
            raise ValueError('cache paths must not contain symlinks')
    return current


def _cache_walk(root, excluded=()):
    files, directories = {}, {}
    if root.is_symlink() or not root.is_dir():
        raise ValueError('cache tree must be an existing directory, not a symlink')
    def inaccessible(error):
        raise error
    for directory, names, entries in os.walk(root, followlinks=False, onerror=inaccessible):
        relative = Path(directory).relative_to(root)
        if relative == Path('.'):
            names[:] = [name for name in names if name not in excluded]
            entries = [name for name in entries if name not in excluded]
        for name in sorted(names + entries):
            path = Path(directory) / name
            info = path.lstat()
            key = path.relative_to(root).as_posix()
            if stat.S_ISREG(info.st_mode):
                files[key] = info
            elif stat.S_ISDIR(info.st_mode):
                directories[key] = info
            else:
                raise ValueError('cache tree contains a symlink or non-regular input: ' + key)
    return files, directories


def _cache_file(path, info=None):
    info = info or path.stat()
    return dict(bytes=info.st_size, mode=stat.S_IMODE(info.st_mode),
                mtime_ns=info.st_mtime_ns, sha256=digest(path))


def _cache_inputs(root):
    # Include ignored/untracked build inputs too. Never traverse Git internals
    # or the separately managed Cargo target (which may be a prepared symlink).
    files, _ = _cache_walk(root, excluded=('.git', 'target'))
    return {name: _cache_file(root / name, info) for name, info in files.items()}


def cache_begin(root):
    root = root.resolve()
    return dict(schema_version=CACHE_SCHEMA, checkout=str(root),
                target=str((root / 'target').resolve()), inputs=_cache_inputs(root),
                packages={}, files=[], directories=[], complete=False)


def _cache_relative(state, path):
    path = Path(path)
    target = Path(state['target'])
    alias = Path(state['checkout']) / 'target'
    if path.is_relative_to(alias):
        path = target / path.relative_to(alias)
    name = path.relative_to(target).as_posix()
    _cache_path(target, name)
    return name


def cache_record(state, message):
    """Select concrete Cargo units, including both build-script unit hashes."""
    reason = message.get('reason')
    target_root = Path(state['target'])
    if reason == 'compiler-artifact':
        manifest = Path(message['manifest_path'])
        root = Path(state['checkout'])
        if not manifest.is_relative_to(root):
            return
        manifest = _cache_path(root, manifest.relative_to(root).as_posix())
        package = tomllib.loads(manifest.read_text())['package']['name']
        state['packages'][message['package_id']] = package
        filenames = [_cache_relative(state, name) for name in message['filenames']]
        if not filenames:
            raise ValueError('local Cargo artifact has no files')
        target_name = message['target']['name'].replace('-', '_')
        if 'custom-build' in message['target']['kind']:
            directory = Path(filenames[0]).parent
            if directory.parent.name != 'build':
                raise ValueError('unsupported build-script layout')
            unit_hash = directory.name.rsplit('-', 1)[-1]
            profile = directory.parent.parent
            state['directories'].append(directory.as_posix())
        else:
            hashed = []
            pattern = re.compile(r'(?:lib)?' + re.escape(target_name) + r'-([0-9a-f]{16})(?:\.|$)')
            for name in filenames:
                path = Path(name)
                match = pattern.fullmatch(path.name) or pattern.match(path.name)
                if path.parent.name == 'deps' and match:
                    hashed.append((path.parent.parent, match.group(1)))
            if not hashed:
                # Cargo reports only the public path for a normal binary. Its
                # concrete deps output is a hard link, not a guessed crate hash.
                public = target_root / filenames[0]
                profile = Path(filenames[0]).parent
                for candidate in (target_root / profile / 'deps').glob(target_name + '-*'):
                    match = pattern.fullmatch(candidate.name)
                    if match and candidate.is_file() and os.path.samefile(public, candidate):
                        hashed.append((profile, match.group(1)))
            if len(set(hashed)) != 1:
                raise ValueError('cannot identify a unique local Cargo output unit')
            profile, unit_hash = hashed[0]
            for candidate in (target_root / profile / 'deps').iterdir():
                if any(candidate.name == prefix or candidate.name.startswith(prefix + '.')
                       for prefix in (target_name + '-' + unit_hash, 'lib' + target_name + '-' + unit_hash)):
                    state['files'].append(_cache_relative(state, candidate))
            for name in filenames:
                path = target_root / name
                if path.parent.name != 'deps' and path.with_suffix('.d').is_file():
                    state['files'].append(_cache_relative(state, path.with_suffix('.d')))
        if not re.fullmatch(r'[0-9a-f]{16}', unit_hash):
            raise ValueError('unsupported Cargo unit hash')
        state['files'].extend(filenames)
        state['directories'].append((profile / '.fingerprint' / (package + '-' + unit_hash)).as_posix())
    elif reason == 'build-script-executed' and message['package_id'] in state['packages']:
        directory = Path(_cache_relative(state, message['out_dir'])).parent
        package = state['packages'][message['package_id']]
        if directory.parent.name != 'build' or not re.fullmatch(re.escape(package) + r'-[0-9a-f]{16}', directory.name):
            raise ValueError('unsupported build-script output layout')
        state['directories'] += [directory.as_posix(),
                                 (directory.parent.parent / '.fingerprint' / directory.name).as_posix()]


def cache_finish(state):
    if _cache_inputs(Path(state['checkout'])) != state['inputs']:
        raise ValueError('source inputs changed during the build')
    state['files'] = sorted(set(state['files']))
    state['directories'] = sorted(set(state['directories']))
    state['complete'] = True


def _cache_sizes(files):
    seen, unique, categories = set(), 0, {}
    for name, info in files.items():
        inode = (info.st_dev, info.st_ino)
        if inode not in seen:
            unique += info.st_size
            seen.add(inode)
        parts = Path(name).parts
        category = '/'.join(parts[:2]) if len(parts) > 2 else parts[0] + '/public'
        if len(parts) > 2 and parts[1] == 'deps':
            category += '/' + (Path(name).suffix.lstrip('.') or 'executable')
        categories[category] = categories.get(category, 0) + info.st_size
    return dict(raw_bytes=sum(info.st_size for info in files.values()), unique_bytes=unique,
                bytes_by_category=dict(sorted(categories.items())))


def _cache_copy(source, destination, files, directories):
    """Copy complete units, preserving internal hard links and exact timestamps."""
    written, inodes = [], {}
    try:
        for name in directories:
            _cache_path(destination, name).mkdir(parents=True, exist_ok=True)
        for name, metadata in files.items():
            original, copied = _cache_path(source, name), _cache_path(destination, name)
            copied.parent.mkdir(parents=True, exist_ok=True)
            info = original.stat()
            inode = (info.st_dev, info.st_ino)
            written.append(copied)
            copied.unlink(missing_ok=True)
            if inode in inodes:
                os.link(inodes[inode], copied)
            else:
                shutil.copy2(original, copied)
                inodes[inode] = copied
            copied.chmod(metadata['mode'])
            os.utime(copied, ns=(metadata['mtime_ns'], metadata['mtime_ns']))
        for name in sorted(directories, key=lambda value: len(Path(value).parts), reverse=True):
            copied, metadata = _cache_path(destination, name), directories[name]
            copied.chmod(metadata['mode'])
            os.utime(copied, ns=(metadata['mtime_ns'], metadata['mtime_ns']))
    except OSError:
        # Any partial unit loses its restored fingerprint as well. A disposable
        # cache cannot leave a partially restored unit looking fresh to Cargo.
        try:
            for copied in written:
                copied.unlink(missing_ok=True)
        except OSError as error:
            raise RuntimeError('cannot discard a partial Cargo cache restore') from error
        raise


def _cache_location(cache, root, target):
    if cache.is_symlink():
        raise ValueError('cache staging directory is a symlink')
    cache = cache.resolve()
    if any(cache.is_relative_to(path) or path.is_relative_to(cache) for path in (root, target)):
        raise ValueError('cache staging must be outside checkout and target')
    return cache


def cache_stage(state, cache, maximum=8 * 1024 ** 3, measure_only=False):
    started = time.monotonic()
    report = dict(save=False, max_bytes=maximum, measure_only=measure_only)
    try:
        root, target = Path(state['checkout']), Path(state['target'])
        cache = _cache_location(cache, root, target)
        all_files, _ = _cache_walk(target)
        target_sizes = _cache_sizes(all_files)
        report.update({'target_' + key: value for key, value in target_sizes.items()})
        if state['schema_version'] != CACHE_SCHEMA or not state.get('complete'):
            raise ValueError(state.get('error', 'incomplete Cargo cache state'))
        if not measure_only and _cache_inputs(root) != state['inputs']:
            raise ValueError('source inputs changed since the build')
        selected = set(state['files'])
        directories = {}
        for name in state['directories']:
            path = _cache_path(target, name)
            if not path.is_dir():
                raise ValueError('missing Cargo unit directory: ' + name)
            files, children = _cache_walk(path)
            selected.update((Path(name) / child).as_posix() for child in files)
            for child, info in {name: path.stat(), **{
                    (Path(name) / child).as_posix(): info for child, info in children.items()}}.items():
                directories[child] = dict(mode=stat.S_IMODE(info.st_mode), mtime_ns=info.st_mtime_ns)
        sizes = _cache_sizes({name: all_files[name] for name in sorted(selected)})
        report.update(sizes, local_packages=len(state['packages']), cargo_unit_directories=len(state['directories']),
                      file_count=len(selected), eligible=0 < sizes['raw_bytes'] <= maximum)
        if measure_only:
            # Readers report the real selection without hashing/copying payload,
            # changing source timestamps, or preparing an unwritable cache.
            return report
        if not report['eligible']:
            report['reason'] = 'workspace selection exceeds budget or is empty'
            return report
        files = {name: _cache_file(_cache_path(target, name), all_files[name]) for name in sorted(selected)}
        if max(item['mtime_ns'] for item in [*files.values(), *directories.values(), *state['inputs'].values()]) >= time.time_ns():
            raise ValueError('future cache timestamp')
        if cache.exists():
            shutil.rmtree(cache)
        cache.mkdir(parents=True)
        _cache_copy(target, cache / 'target', files, directories)
        if _cache_inputs(root) != state['inputs']:
            raise ValueError('source inputs changed while staging')
        manifest = dict(schema_version=CACHE_SCHEMA, checkout=str(root), target=str(target),
                        inputs=state['inputs'], files=files, directories=directories)
        (cache / 'manifest.json').write_text(json.dumps(manifest, sort_keys=True) + '\n')
        report['save'] = True
    except (OSError, ValueError, KeyError, TypeError) as error:
        report['reason'] = str(error)
    finally:
        report['elapsed_seconds'] = round(time.monotonic() - started, 3)
    return report


def cache_restore(root, cache, maximum=8 * 1024 ** 3):
    started = time.monotonic()
    report = dict(restored=False)
    written = False
    try:
        root = root.resolve()
        target = (root / 'target').resolve()
        cache = _cache_location(cache, root, target)
        manifest = json.loads(_cache_path(cache, 'manifest.json').read_text())
        if (manifest['schema_version'] != CACHE_SCHEMA or manifest['checkout'] != str(root)
                or manifest['target'] != str(target)):
            raise ValueError('cache checkout, target or schema mismatch')
        files, directories, inputs = manifest['files'], manifest['directories'], manifest['inputs']
        if not all(isinstance(value, dict) and value for value in (files, directories, inputs)):
            raise ValueError('cache manifest requires complete file, directory and source maps')
        actual, _ = _cache_walk(cache)
        if set(actual) != {'manifest.json', *('target/' + name for name in files)}:
            raise ValueError('cache payload is incomplete or has extra files')
        current = _cache_inputs(root)
        if set(current) != set(inputs):
            raise ValueError('source input paths were added, removed or renamed')
        now = time.time_ns()
        for metadata in [*files.values(), *directories.values(), *inputs.values()]:
            if (not isinstance(metadata, dict) or type(metadata['mtime_ns']) is not int or not 0 < metadata['mtime_ns'] < now
                    or type(metadata['mode']) is not int or metadata['mode'] & ~0o777):
                raise ValueError('invalid cache timestamp or mode')
        for metadata in [*files.values(), *inputs.values()]:
            if (type(metadata['bytes']) is not int or metadata['bytes'] < 0
                    or not isinstance(metadata['sha256'], str) or not re.fullmatch('[0-9a-f]{64}', metadata['sha256'])):
                raise ValueError('invalid cache file length or checksum')
        if not 0 < sum(item['bytes'] for item in files.values()) <= maximum:
            raise ValueError('cache payload exceeds budget or is empty')
        for name, metadata in files.items():
            path = _cache_path(cache / 'target', name)
            info = path.stat()
            if (info.st_size != metadata['bytes'] or stat.S_IMODE(info.st_mode) != metadata['mode']
                    or digest(path) != metadata['sha256']):
                raise ValueError('cache payload checksum, size or mode mismatch: ' + name)
            destination = _cache_path(target, name)
            if destination.exists() and (not destination.is_file() or digest(destination) != metadata['sha256']):
                raise ValueError('cache overlaps a different existing Cargo output: ' + name)
        for name, metadata in directories.items():
            path = _cache_path(cache / 'target', name)
            if not path.is_dir() or stat.S_IMODE(path.stat().st_mode) != metadata['mode']:
                raise ValueError('missing cache unit directory or directory mode mismatch')
            destination = _cache_path(target, name)
            if destination.exists():
                existing, _ = _cache_walk(destination)
                if any((Path(name) / child).as_posix() not in files for child in existing):
                    raise ValueError('cache overlaps an incomplete existing Cargo unit')
        _cache_copy(cache / 'target', target, files, directories)
        written = True
        changed = 0
        # Validation rejects future metadata; this stamp is strictly newer than
        # every restored Cargo freshness reference, including build-script runs.
        stamp = time.time_ns()
        for name, metadata in inputs.items():
            same = all(current[name][key] == metadata[key] for key in ('bytes', 'mode', 'sha256'))
            changed += not same
            path = _cache_path(root, name)
            value = metadata['mtime_ns'] if same else stamp
            os.utime(path, ns=(value, value))
        report.update(restored=True, changed_inputs=changed, matched_inputs=len(inputs) - changed,
                      raw_bytes=sum(item['bytes'] for item in files.values()))
    except (OSError, ValueError, KeyError, TypeError) as error:
        if written:
            for name in files:
                _cache_path(target, name).unlink(missing_ok=True)
        report['reason'] = str(error)
    finally:
        report['elapsed_seconds'] = round(time.monotonic() - started, 3)
    return report


def load(bundle, suite=None, shard=None):
    manifest = json.loads((bundle / 'manifest.json').read_text())
    if manifest.get('scope') == 'storage' and manifest.get('schema_version') != 2:
        raise ValueError('expected schema 2 storage artifact')
    config = json.loads(CONFIG.read_text())
    expected = dict(tested_sha=output(['git', 'rev-parse', 'HEAD']),
                    source_sha=os.environ.get('SOURCE_SHA', output(['git', 'rev-parse', 'HEAD'])),
                    producer_run_id=os.environ.get('GITHUB_RUN_ID', 'local'), checkout=str(ROOT),
                    config_sha256=digest(CONFIG),
                    rust_toolchain=os.environ.get('RUST_TOOLCHAIN', 'local'),
                    runner_image=os.environ.get('ImageOS', sys.platform))
    for key, value in expected.items():
        if manifest.get(key) != value:
            raise ValueError(f'artifact identity mismatch: {key}')
    included = set(manifest.get('included_binaries', manifest['binaries']))
    if suite or shard:
        selection = {'shard': shard} if shard else {'suite': suite}
        if manifest.get('selection') not in (None, selection):
            raise ValueError('artifact selection mismatch')
        required = dependency_closure(manifest, config, suite, shard)
        if not required <= included or (manifest.get('selection') and required != included):
            raise ValueError('artifact executable closure mismatch')
        included = required
    selected = {name: manifest['binaries'][name] for name in sorted(included)}
    for name, entry in selected.items():
        path = checked_file(bundle, entry['path'], entry['sha256'])
        if not os.access(path, os.X_OK):
            raise ValueError('missing, invalid or corrupt executable')
        if manifest.get('schema_version') == 2:
            validate_profile(name, entry, manifest.get('scope'))
    # env!(CARGO_BIN_EXE_*) embeds Cargo's original absolute binary path.
    # Restore only recorded executables under this checkout's target directory.
    for entry in selected.values():
        original = Path(entry['original_executable'])
        if not entry['test'] and original.is_relative_to(ROOT / 'target'):
            original.parent.mkdir(parents=True, exist_ok=True)
            if not original.exists() or digest(original) != entry['sha256']:
                shutil.copy2(bundle / entry['path'], original)
    for name, sha in manifest['libraries'].items():
        path = bundle / name
        if not path.resolve().is_relative_to(bundle.resolve()) or digest(path) != sha:
            raise ValueError('invalid dynamic library')
    env = dict(os.environ)
    env['LD_LIBRARY_PATH'] = str(bundle / 'lib') + os.pathsep + env.get('LD_LIBRARY_PATH', '')
    for variable, logical in {
        'PRE2_VIEWER_BINARY': 'viewer', 'OASIS7_CHAIN_RUNTIME_BIN': 'runtime',
        'OASIS7_FORMAL_ENDPOINT_TEST_BIN': 'endpoint',
        'OASIS7_NODE_LIFECYCLE_TEST_BIN': 'runtime-tests',
        'OASIS7_FORMAL_NODE_TEST_BIN': 'runtime',
    }.items():
        if logical in selected:
            env[variable] = str(bundle / selected[logical]['path'])
    # Validate actual downloaded harnesses; the manifest alone is not coverage proof.
    for binary in selected.values():
        if binary['test']:
            if inventory(bundle / binary['path'], env=env) != binary['tests'] or inventory(bundle / binary['path'], True, env) != binary['ignored_tests']:
                raise ValueError('downloaded test inventory mismatch')
    if manifest.get('scope') != 'ordinary' and resolve_suites(config, manifest['binaries']) != manifest['suites']:
        raise ValueError('suite inventory mismatch')
    if manifest.get('scope') != 'ordinary' and manifest.get('schema_version') == 2:
        resolve_shards(config, manifest['suites'])
    return manifest, env


def execute(bundle, suite, results, shard=None):
    from ci_process_runner import run
    manifest, env = load(bundle, suite if not shard else None, shard)
    if suite == 'ordinary' and not shard:
        entries = [dict(binary=name, batch=True) for name, binary in manifest['binaries'].items() if binary['ordinary'] and set(binary['tests']) - set(binary['ignored_tests'])]
        for owner, pattern in DEFAULT_SNAPSHOT_REGRESSIONS:
            if owner not in manifest['binaries']:
                raise ValueError('missing default-feature snapshot harness: ' + owner)
            binary = manifest['binaries'][owner]
            names = [name for name in binary['tests'] if pattern in name and name not in binary['ignored_tests']]
            if not names:
                raise ValueError('missing snapshot regression: ' + pattern)
            entries.append(dict(binary=owner, filter=pattern, count=len(names)))
        # Original ordinary environment did not contain formal-process child paths.
        for variable in list(env):
            if variable.startswith('OASIS7_FORMAL_') or variable in ('OASIS7_CHAIN_RUNTIME_BIN', 'OASIS7_NODE_LIFECYCLE_TEST_BIN'):
                env.pop(variable)
    else:
        entries = selected_entries(manifest, json.loads(CONFIG.read_text()), suite if not shard else None, shard)
    failed = False
    for entry in entries:
        logical_suite = entry.get('suite', suite)
        if 'script' in entry:
            command = ['bash', str(ROOT / 'scripts' / entry['script'])]
            cwd, count = ROOT, None
            name = entry['script']
        else:
            binary = manifest['binaries'][entry['binary']]
            command = [str(bundle / binary['path'])]
            name = entry.get('name', entry['binary'])
            if entry.get('batch'):
                count = len(set(binary['tests']) - set(binary['ignored_tests']))
            elif 'filter' in entry:
                command.append(entry['filter'])
                name, count = entry['filter'], entry['count']
            else:
                command += ['--exact', name]
                if entry['ignored']:
                    command.append('--ignored')
                count = 1
            if logical_suite != 'ordinary':
                command += ['--test-threads=1', '--nocapture']
            cwd = ROOT / binary['cwd']
        test_env = dict(env, CARGO_MANIFEST_DIR=str(cwd))
        if shard:
            # The generic runner owns cleanup and all result checks. Keep shard
            # metadata in this execution entry point, after its complete result.
            with tempfile.TemporaryDirectory(prefix='oasis7-ci-result-') as temporary:
                result = run(command, cwd, test_env, logical_suite, name,
                             Path(temporary) / 'result.jsonl', expected_count=count)
            result = dict(result, shard=shard)
            results.parent.mkdir(parents=True, exist_ok=True)
            with results.open('a') as stream:
                stream.write(json.dumps(result) + '\n')
        else:
            result = run(command, cwd, test_env, logical_suite, name, results, expected_count=count)
        failed |= not result['passed']
        if result['cleanup_result'] != 'clean':
            break
    if failed:
        raise RuntimeError(f'{shard or suite} failed; see {results}')


def main():
    global ROOT, CONFIG
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['build', 'run', 'pack', 'cache-build', 'cache-budget', 'cache-stage', 'cache-restore'])
    parser.add_argument('--repo-root', type=Path, default=ROOT)
    parser.add_argument('--bundle', type=Path)
    parser.add_argument('--scope', choices=['all', 'formal', 'ordinary', 'storage'], default='all')
    parser.add_argument('--suite', default='ordinary')
    parser.add_argument('--shard', choices=SHARDS)
    parser.add_argument('--split-formal', action='store_true')
    parser.add_argument('--output', type=Path)
    parser.add_argument('--github-output', type=Path)
    parser.add_argument('--max-bytes', type=int, default=8 * 1024 ** 3)
    parser.add_argument('--cache', type=Path)
    parser.add_argument('--cache-state', type=Path)
    parser.add_argument('--measure-only', action='store_true')
    parser.add_argument('--results', type=Path)
    args = parser.parse_args()
    ROOT = args.repo_root.resolve()
    CONFIG = ROOT / 'scripts/ci-formal-process-suites.json'
    if args.measure_only and args.action != 'cache-stage':
        parser.error('--measure-only requires cache-stage')
    if args.action == 'cache-build':
        build_cache_dependencies(args.scope)
        return
    if args.action in ('cache-budget', 'cache-stage', 'cache-restore'):
        if args.action == 'cache-budget':
            report = cache_budget(ROOT / 'target', args.max_bytes)
        else:
            if not args.cache:
                parser.error('cache-stage and cache-restore require --cache')
            if args.action == 'cache-restore':
                report = cache_restore(ROOT, args.cache, args.max_bytes)
            else:
                if not args.cache_state:
                    parser.error('cache-stage requires --cache-state')
                try:
                    state = json.loads(args.cache_state.read_text())
                    if state['checkout'] != str(ROOT) or state['target'] != str((ROOT / 'target').resolve()):
                        raise ValueError('cache build state belongs to a different checkout or target')
                    report = cache_stage(state, args.cache, args.max_bytes, args.measure_only)
                except (OSError, ValueError, KeyError, TypeError) as error:
                    report = dict(save=False, reason=str(error))
        print('formal_workspace_cache=' + json.dumps(report))
        if args.github_output:
            with args.github_output.open('a') as stream:
                stream.write('save=' + str(report.get('save', False)).lower() + '\n')
        if os.environ.get('GITHUB_STEP_SUMMARY'):
            with Path(os.environ['GITHUB_STEP_SUMMARY']).open('a') as stream:
                stream.write('\n### Formal workspace cache: ' + args.action + '\n\n```json\n' + json.dumps(report, indent=2) + '\n```\n')
        return
    if not args.bundle:
        parser.error('--bundle is required for build, run and pack')
    if args.action == 'build':
        build(args.bundle.resolve(), args.scope, args.cache_state)
    elif args.action == 'pack':
        if not args.output:
            parser.error('pack requires --output')
        if args.split_formal:
            for shard in SHARDS:
                pack(args.bundle.resolve(), args.output / (shard + '.tar.gz'), shard=shard)
        else:
            pack(args.bundle.resolve(), args.output, None if args.shard else args.suite, args.shard)
    else:
        execute(args.bundle.resolve(), args.suite, args.results or ROOT / 'output/formal-process/results.jsonl', args.shard)


if __name__ == '__main__':
    main()
