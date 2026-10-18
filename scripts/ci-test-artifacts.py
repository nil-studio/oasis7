#!/usr/bin/env python3
"""Build once and run native test executables without Cargo on consumers."""
import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
CONFIG = ROOT / 'scripts/ci-formal-process-suites.json'
SHARDS = ('runtime-protocol', 'world-history', 'healthy-set', 'storage-recovery', 'lifecycle-cli')


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
        ('required', ['test', '-p', 'oasis7', '-p', 'oasis7_simulator', '-p', 'oasis7_runtime', '-p', 'oasis7_world_service', '-p', 'scenario_test_runner', '-p', 'oasis7_chain_runtime', '-p', 'oasis7_client_cli', '-p', 'oasis7_launcher_backend', '-p', 'oasis7_newapi_bridge_service', '-p', 'oasis7_ops_tools', '-p', 'oasis7_provider_local_bridge', '-p', 'oasis7_provider_tools', '-p', 'oasis7_viewer_runtime', '-p', 'oasis7_observability', '-p', 'oasis7_hosted_access', '-p', 'oasis7_letai_credit_adapter', '-p', 'oasis7_launcher_config', '-p', 'oasis7_wasm_materializer', '-p', 'oasis7_game_client', '-p', 'oasis7_cas', '-p', 'oasis7_formal_protocol', '--tests', '--features', 'test_tier_required', '--no-run'], required_features),
        ('runtime', ['build', '-p', 'oasis7_chain_runtime', '--bin', 'oasis7_chain_runtime', '--features', 'test_tier_required'], required_features),
        ('endpoint', ['build', '-p', 'oasis7_distfs', '--bin', 'oasis7_durability_endpoint', '--release'], []),
        ('distfs-tests', ['test', '-p', 'oasis7_distfs', '--lib', '--no-run'], []),
        ('snapshot-tests', ['test', '-p', 'oasis7', '--lib', '--no-run'], []),
    ]
    if scope == 'all':
        return variants
    if scope == 'ordinary':
        return [v for v in variants if v[0] in ('viewer', 'required', 'snapshot-tests')]
    if scope == 'formal':
        # Keep the bin harness: the runtime has no library target. Ordinary
        # integration tests, viewer executables and snapshot variants run elsewhere.
        # The CI-only marker reproduces shared dependency features from all
        # required roots without compiling their unrelated harnesses.
        features = ['test_tier_required', 'ci_formal_required_features']
        core = ('formal-required', ['test', '-p', 'oasis7', '-p', 'oasis7_chain_runtime',
                '--lib', '--bins', '--features', ','.join(features), '--no-run'], features)
        return [core, *(v for v in variants if v[0] in ('runtime', 'endpoint', 'distfs-tests'))]
    raise ValueError('unknown build scope: ' + scope)


def validate_profile(logical, binary):
    if logical == 'endpoint':
        profile = binary['profile']
        if (binary['target'] != 'oasis7_durability_endpoint' or binary['test']
                or profile['opt_level'] != '3' or profile['debug_assertions']
                or Path(binary['original_executable']).parent.name != 'release'):
            raise ValueError('expected optimized release endpoint artifact')


def build(bundle, scope='all'):
    bundle.mkdir(parents=True, exist_ok=False)
    (bundle / 'bin').mkdir()
    (bundle / 'lib').mkdir()
    env = dict(os.environ)
    env.pop('RUSTC_WRAPPER', None)
    binaries, timings = {}, []
    variants = build_variants(scope)
    for variant, args, features in variants:
        started = time.monotonic()
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
                executable = artifact.get('executable')
                if artifact.get('reason') != 'compiler-artifact' or not executable:
                    continue
                target = artifact['target']
                test = artifact['profile']['test']
                name = target['name']
                logical = variant
                if variant in ('required', 'formal-required'):
                    if not test:
                        logical = 'required-executable-' + name
                    else:
                        logical = {'oasis7_chain_runtime': 'runtime-tests', 'oasis7': 'oasis7-lib-tests'}.get(name, 'required-test-' + '-'.join(target['kind']) + '-' + name)
                if logical in binaries:
                    raise ValueError(f'duplicate executable: {logical}')
                source = Path(executable)
                destination = bundle / 'bin' / logical
                shutil.copy2(source, destination)
                # Cargo uses the package directory as test cwd, and exposes its binaries.
                manifest = Path(artifact['manifest_path'])
                binaries[logical] = dict(path=str(destination.relative_to(bundle)), sha256=digest(destination),
                    profile=artifact['profile'], features=artifact['features'], requested_features=features,
                    test=test, target=name, original_executable=str(source), cwd=str(manifest.parent.relative_to(ROOT)),
                    ordinary=variant == 'required' and test, variant=variant,
                    bytes=destination.stat().st_size, libraries=[])
                validate_profile(logical, binaries[logical])
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
        timings.append(dict(variant=variant, elapsed_seconds=round(time.monotonic() - started, 3)))
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
                    or name in ('viewer', 'snapshot-tests') or name.startswith('required-executable-')}
        if 'snapshot-tests' not in required:
            raise ValueError('missing default-feature snapshot harness')
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


def load(bundle, suite=None, shard=None):
    manifest = json.loads((bundle / 'manifest.json').read_text())
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
            validate_profile(name, entry)
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
        binary = manifest['binaries']['snapshot-tests']
        for pattern in ['snapshot_progress::', 'snapshot_player_gameplay_execution_state_backfills_from_legacy_fields']:
            names = [name for name in binary['tests'] if pattern in name and name not in binary['ignored_tests']]
            if not names:
                raise ValueError('missing snapshot regression: ' + pattern)
            entries.append(dict(binary='snapshot-tests', filter=pattern, count=len(names)))
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
    parser.add_argument('action', choices=['build', 'run', 'pack', 'cache-budget'])
    parser.add_argument('--repo-root', type=Path, default=ROOT)
    parser.add_argument('--bundle', type=Path)
    parser.add_argument('--scope', choices=['all', 'formal', 'ordinary'], default='all')
    parser.add_argument('--suite', default='ordinary')
    parser.add_argument('--shard', choices=SHARDS)
    parser.add_argument('--split-formal', action='store_true')
    parser.add_argument('--output', type=Path)
    parser.add_argument('--github-output', type=Path)
    parser.add_argument('--max-bytes', type=int, default=8 * 1024 ** 3)
    parser.add_argument('--results', type=Path)
    args = parser.parse_args()
    ROOT = args.repo_root.resolve()
    CONFIG = ROOT / 'scripts/ci-formal-process-suites.json'
    if args.action == 'cache-budget':
        report = cache_budget(ROOT / 'target', args.max_bytes)
        print('formal_workspace_cache=' + json.dumps(report))
        if args.github_output:
            with args.github_output.open('a') as stream:
                stream.write('save=' + str(report['save']).lower() + '\n')
        if os.environ.get('GITHUB_STEP_SUMMARY'):
            with Path(os.environ['GITHUB_STEP_SUMMARY']).open('a') as stream:
                stream.write(f"\nFormal workspace cache: {report['raw_bytes']} bytes; limit {report['max_bytes']}; eligible to save: {report['save']}.\n")
        return
    if not args.bundle:
        parser.error('--bundle is required for build, run and pack')
    if args.action == 'build':
        build(args.bundle.resolve(), args.scope)
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
