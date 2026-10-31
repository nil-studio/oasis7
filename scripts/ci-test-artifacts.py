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
FORMAL_AUTHORITY = None
SHARDS = ('runtime-protocol', 'world-history', 'healthy-set', 'storage-recovery', 'lifecycle-cli')
DEFAULT_SNAPSHOT_REGRESSIONS = (
    ('viewer-snapshot-tests', 'snapshot_progress::'),
    ('snapshot-tests', 'snapshot_player_gameplay_execution_state_backfills_from_legacy_fields'),
)
PORTABLE_REQUIRED_PACKAGES = ('oasis7_agent_api', 'oasis7_simulator', 'oasis7_viewer_codec', 'oasis7_viewer_runtime')
# These roots contain shipped Viewer subprocess consumers or their integration
# fixtures. Other roots do not need a second, unrelated Viewer binary build.
VIEWER_REQUIRED_CONSUMERS = frozenset(('oasis7', 'oasis7_simulator', 'oasis7_chain_runtime',
                                     'oasis7_launcher_backend', 'oasis7_viewer_runtime'))
FAST_PROFILES = frozenset(('pr-fast', 'main-fast', 'periodic-full', 'candidate-full'))


def fast_plan(plan):
    """Old trusted planners retain the complete previous execution inventory."""
    if plan is None or (isinstance(plan, dict) and 'fast_plan_version' not in plan):
        return None
    if (not isinstance(plan, dict) or type(plan.get('fast_plan_version')) is not int
            or plan.get('fast_plan_version') != 1 or plan.get('execution_profile') not in FAST_PROFILES
            or plan.get('scope') not in ('targeted', 'full', 'explicit-full')
            or not isinstance(plan.get('groups'), list)
            or any(not isinstance(group, str) for group in plan['groups'])
            or len(plan['groups']) != len(set(plan['groups']))
            or type(plan.get('test_policy_changed', False)) is not bool):
        raise ValueError('unsupported fast execution plan')
    return plan


def plan_list(plan, key, inventory, *, nonempty=True):
    value = plan.get(key)
    if (not isinstance(value, list) or (nonempty and not value)
            or any(not isinstance(item, str) for item in value)
            or len(value) != len(set(value)) or not set(value) <= set(inventory)):
        raise ValueError('invalid execution selection: ' + key)
    return value


def required_packages(plan=None):
    command = next(args for name, args, _ in build_variants('ordinary') if name == 'required')
    inventory = [command[index + 1] for index, value in enumerate(command) if value == '-p']
    plan = fast_plan(plan)
    if plan is None:
        return inventory
    selected = plan_list(plan, 'oasis7_required_packages', inventory)
    if 'oasis7_required' not in plan.get('groups', []):
        raise ValueError('ordinary build is not selected by plan')
    if plan.get('scope') in ('full', 'explicit-full') or plan['execution_profile'] in ('periodic-full', 'candidate-full'):
        return inventory
    return selected


def required_wasm_packages(plan=None):
    plan = fast_plan(plan)
    if plan is None:
        return list(PORTABLE_REQUIRED_PACKAGES)
    selected = plan_list(plan, 'oasis7_required_wasm_packages', PORTABLE_REQUIRED_PACKAGES, nonempty=False)
    if plan.get('scope') in ('full', 'explicit-full') or plan['execution_profile'] in ('periodic-full', 'candidate-full'):
        return list(PORTABLE_REQUIRED_PACKAGES)
    return selected


def package_features(packages, plan=None, *, include_full=False):
    """Activate only features actually owned by a selected Cargo package."""
    workspace = tomllib.loads((ROOT / 'Cargo.toml').read_text())['workspace']
    manifests = {}
    for member in workspace['members']:
        for directory in ROOT.glob(member):
            manifest = tomllib.loads((directory / 'Cargo.toml').read_text())
            manifests[manifest['package']['name']] = manifest
    expanded = include_full or (fast_plan(plan) is not None and plan.get('test_policy_changed', False))
    features = []
    for package in packages:
        if package not in manifests:
            raise ValueError('selected package has no workspace manifest: ' + package)
        available = manifests[package].get('features', {})
        # An edited full=>required feature edge cannot silently drop the
        # existing required integration tests during a policy transition.
        tiers = ('test_tier_required', 'test_tier_full') if expanded else ('test_tier_required',)
        features.extend(package + '/' + tier for tier in tiers if tier in available)
        if expanded:
            features.extend(package + '/' + feature for feature in ('wasmtime', 'viewer_live_integration') if feature in available)
    return features


def required_arguments(plan=None, phase='native'):
    if phase == 'wasm':
        packages = required_wasm_packages(plan)
        return [value for package in packages for value in ('-p', package)] + (
            ['--lib', '--target', 'wasm32-unknown-unknown', '--locked'] if packages else [])
    packages = required_packages(plan)
    if phase == 'packages':
        return packages
    if phase != 'native':
        raise ValueError('unknown ordinary command phase')
    if fast_plan(plan) is None:
        command = next(args for name, args, _ in build_variants('ordinary') if name == 'required')
        return command[1:-1]
    args = [value for package in packages for value in ('-p', package)] + ['--tests']
    features = package_features(packages, plan)
    return args + (['--features', ','.join(features)] if features else [])


def formal_suites(plan=None):
    plan = fast_plan(plan)
    if plan is None:
        return list(SHARDS)
    selected = plan_list(plan, 'formal_suites', SHARDS, nonempty=False)
    if bool(selected) != ('formal_process' in plan.get('groups', [])):
        raise ValueError('formal suite selection disagrees with selected groups')
    if plan.get('scope') in ('full', 'explicit-full') or plan['execution_profile'] in ('periodic-full', 'candidate-full'):
        if not selected:
            raise ValueError('complete profile is missing formal suites')
        return list(SHARDS)
    return selected


def formal_configuration():
    candidate = json.loads(CONFIG.read_text())
    if FORMAL_AUTHORITY is None:
        return candidate
    trusted = json.loads(FORMAL_AUTHORITY.read_text())
    if trusted.get('schema_version') != 2 or candidate.get('schema_version') != 2:
        raise ValueError('unsupported trusted Formal inventory schema')
    result = json.loads(json.dumps(trusted))
    owners = {}
    for suite, entries in trusted['suites'].items():
        for entry in entries:
            owners[(entry['binary'], entry['name'])] = (suite, entry)
    candidate_cases = set()
    for suite, entries in candidate['suites'].items():
        if suite not in SHARDS:
            raise ValueError('unknown logical Formal suite: ' + suite)
        for entry in entries:
            key = (entry['binary'], entry['name'])
            if key in candidate_cases:
                raise ValueError('duplicate candidate Formal case: ' + str(key))
            candidate_cases.add(key)
            if key in owners:
                if owners[key] != (suite, entry):
                    raise ValueError('candidate changes a trusted Formal case identity: ' + str(key))
            else:
                result['suites'].setdefault(suite, []).append(entry)
                owners[key] = (suite, entry)
    # A candidate can add coverage and dependencies. Removing its local entry
    # cannot remove the trusted expected case, ignored filter, batch or script.
    for key in ('binary_dependencies', 'script_dependencies', 'shell_tests',
                'legacy_ignored_filters', 'auxiliary_entries'):
        for owner, entries in candidate.get(key, {}).items():
            result.setdefault(key, {}).setdefault(owner, [])
            result[key][owner] = list(dict.fromkeys([*result[key][owner], *entries]))
    for binary, suite in candidate['environment_formal_batches'].items():
        previous = result['environment_formal_batches'].get(binary)
        if previous is not None and previous != suite:
            raise ValueError('candidate changes a trusted Formal batch owner: ' + binary)
        result['environment_formal_batches'][binary] = suite
    # Placement may change for whole cases only. The trusted selected physical
    # shard set is checked below before building or packaging any executable.
    for binary, entries in candidate.get('case_shards', {}).items():
        result.setdefault('case_shards', {}).setdefault(binary, {}).update(entries)
    return result


def formal_authority_digest():
    return digest(FORMAL_AUTHORITY) if FORMAL_AUTHORITY is not None else None


def formal_shards(plan=None):
    selected = set(formal_suites(plan))
    config = formal_configuration()
    # Logical ownership stays independent from the placement of whole cases.
    suites = {suite: entries for suite, entries in config['suites'].items() if suite in selected}
    overrides = config.get('case_shards', {})
    shards = {overrides.get(entry['binary'], {}).get(entry['name'], suite)
              for suite, entries in suites.items() for entry in entries}
    shards.update(suite for suite, entries in config['shell_tests'].items() if suite in selected and entries)
    shards.update(suite for suite in config['environment_formal_batches'].values() if suite in selected)
    if shards - set(SHARDS):
        raise ValueError('unknown physical Formal shard')
    if fast_plan(plan) is not None and 'formal_shards' in plan:
        expected = set(plan_list(plan, 'formal_shards', SHARDS, nonempty=False))
        if not shards <= expected:
            raise ValueError('Formal coverage requires a shard outside the trusted plan')
    return [shard for shard in SHARDS if shard in shards]


def validate_plan_identity(plan):
    plan = fast_plan(plan)
    if plan is None:
        return
    architecture = plan.get('architecture', {})
    if not isinstance(architecture, dict):
        raise ValueError('invalid execution architecture identity')
    expected = {'tested': output(['git', 'rev-parse', 'HEAD']),
                'source': os.environ.get('SOURCE_SHA', output(['git', 'rev-parse', 'HEAD']))}
    for key, actual in expected.items():
        claims = [container[field] for container, field in ((plan, key + '_sha'), (architecture, key))
                  if field in container]
        if not claims or any(claimed != actual for claimed in claims):
            raise ValueError('execution plan identity mismatch: ' + key)


def snapshot_regressions(manifest):
    plan = fast_plan(manifest.get('execution_plan'))
    if plan is None:
        return DEFAULT_SNAPSHOT_REGRESSIONS
    packages = set(required_packages(plan))
    owners = {'viewer-snapshot-tests': 'oasis7_viewer_runtime', 'snapshot-tests': 'oasis7'}
    return tuple((owner, pattern) for owner, pattern in DEFAULT_SNAPSHOT_REGRESSIONS if owners[owner] in packages)


def validate_ordinary_selection(manifest):
    plan = fast_plan(manifest.get('execution_plan'))
    if plan is None or manifest.get('scope') != 'ordinary':
        return
    expected = set(required_packages(plan))
    actual = {entry.get('package') for entry in manifest['binaries'].values() if entry.get('ordinary')}
    if actual != expected:
        raise ValueError('ordinary test package inventory mismatch')
    if not any(entry.get('ordinary') and set(entry['tests']) - set(entry['ignored_tests'])
               for entry in manifest['binaries'].values()):
        raise ValueError('selected ordinary packages have no executable behavior tests')


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


def build_variants(scope, plan=None):
    required_features = ['test_tier_required']
    # Separate variants preserve the old snapshot regressions' default features.
    variants = [
        ('viewer', ['build', '-p', 'oasis7_viewer_runtime', '--bin', 'oasis7_viewer_live', '--features', 'test_tier_required'], required_features),
        ('required', ['test', '-p', 'oasis7', '-p', 'oasis7_simulator', '-p', 'oasis7_runtime', '-p', 'oasis7_world_state', '-p', 'oasis7_world_projection', '-p', 'oasis7_world_service', '-p', 'scenario_test_runner', '-p', 'oasis7_chain_runtime', '-p', 'oasis7_client_cli', '-p', 'oasis7_protocol_tools', '-p', 'oasis7_launcher_backend', '-p', 'oasis7_newapi_bridge_service', '-p', 'oasis7_ops_tools', '-p', 'oasis7_provider_local_bridge', '-p', 'oasis7_provider_tools', '-p', 'oasis7_viewer_runtime', '-p', 'oasis7_viewer_codec', '-p', 'oasis7_observability', '-p', 'oasis7_hosted_access', '-p', 'oasis7_letai_credit_adapter', '-p', 'oasis7_launcher_config', '-p', 'oasis7_wasm_materializer', '-p', 'oasis7_game_client', '-p', 'oasis7_browser_game_client', '-p', 'oasis7_cas', '-p', 'oasis7_formal_protocol', '--tests', '--features', 'test_tier_required', '--no-run'], required_features),
        ('runtime', ['build', '-p', 'oasis7_chain_runtime', '--bin', 'oasis7_chain_runtime', '--features', 'test_tier_required'], required_features),
        ('endpoint', ['build', '-p', 'oasis7_distfs', '--bin', 'oasis7_durability_endpoint', '--release'], []),
        ('distfs-tests', ['test', '-p', 'oasis7_distfs', '--lib', '--no-run'], []),
        ('snapshot-tests', ['test', '-p', 'oasis7', '--lib', '--no-run'], []),
        ('viewer-snapshot-tests', ['test', '-p', 'oasis7_viewer_runtime', '--lib', '--no-run'], []),
    ]
    if scope == 'all':
        return variants
    if scope == 'ordinary':
        if fast_plan(plan) is not None:
            packages = set(required_packages(plan))
            native_args = required_arguments(plan)
            features = native_args[native_args.index('--features') + 1].split(',') if '--features' in native_args else []
            selected = [('required', ['test', *native_args, '--no-run'], features)]
            if packages & VIEWER_REQUIRED_CONSUMERS:
                viewer_features = package_features(['oasis7_viewer_runtime'], plan)
                selected.insert(0, ('viewer', ['build', '-p', 'oasis7_viewer_runtime', '--bin', 'oasis7_viewer_live',
                                               '--features', ','.join(viewer_features)], viewer_features))
            owners = {'snapshot-tests': 'oasis7', 'viewer-snapshot-tests': 'oasis7_viewer_runtime'}
            selected.extend(v for v in variants if owners.get(v[0]) in packages)
            return selected
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


def build(bundle, scope='all', cache_state_path=None, plan=None):
    validate_plan_identity(plan)
    if scope != 'ordinary' and FORMAL_AUTHORITY is not None:
        formal_shards(plan)
    bundle.mkdir(parents=True, exist_ok=False)
    (bundle / 'bin').mkdir()
    (bundle / 'lib').mkdir()
    env = build_environment(scope)
    cache_state = None
    if cache_state_path:
        selection = 'libraries' if scope == 'ordinary' else 'all'
        try:
            cache_state = cache_begin(ROOT, selection=selection)
        except (OSError, ValueError) as error:
            cache_state = dict(schema_version=CACHE_SCHEMA, checkout=str(ROOT),
                               target=str((ROOT / 'target').resolve()), selection=selection,
                               complete=False, error=str(error))
    binaries, timings = {}, []
    variants = build_variants(scope, plan) if plan is not None else build_variants(scope)
    for variant, args, features in variants:
        started = time.monotonic()
        local_units = local_fresh = dependency_units = dependency_fresh = 0
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
                if artifact.get('reason') == 'compiler-artifact':
                    # Count unit observations in this invocation, not unique
                    # crates: features, targets and profiles can produce more.
                    if Path(artifact['manifest_path']).is_relative_to(ROOT):
                        local_units += 1
                        local_fresh += bool(artifact['fresh'])
                    else:
                        dependency_units += 1
                        dependency_fresh += bool(artifact['fresh'])
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
                            local_units=local_units, local_fresh=local_fresh, local_rebuilt=local_units - local_fresh,
                            dependency_units=dependency_units, dependency_fresh=dependency_fresh,
                            dependency_rebuilt=dependency_units - dependency_fresh))
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
    if fast_plan(plan) is not None:
        manifest['execution_plan'] = plan
    if FORMAL_AUTHORITY is not None:
        manifest['formal_authority_sha256'] = formal_authority_digest()
    list_env = dict(env, LD_LIBRARY_PATH=str(bundle / 'lib') + os.pathsep + env.get('LD_LIBRARY_PATH', ''))
    for logical, binary in binaries.items():
        if binary['test']:
            binary['tests'] = inventory(bundle / binary['path'], env=list_env)
            binary['ignored_tests'] = inventory(bundle / binary['path'], True, list_env)
            binary['test_list_sha256'] = hashlib.sha256(json.dumps(binary['tests']).encode()).hexdigest()
    config = formal_configuration()
    manifest['suites'] = resolve_suites(config, binaries) if scope != 'ordinary' else {}
    if scope != 'ordinary':
        resolve_shards(config, manifest['suites'])
    manifest['libraries'] = {str(p.relative_to(bundle)): digest(p) for p in (bundle / 'lib').iterdir()}
    manifest['included_binaries'] = sorted(binaries)
    manifest['selection'] = None
    validate_ordinary_selection(manifest)
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
    plan = fast_plan(manifest.get('execution_plan'))
    selected = set(formal_suites(plan))
    resolved = resolve_shards(config, manifest['suites'])
    if plan is not None and 'formal_shards' in plan:
        expected = set(plan_list(plan, 'formal_shards', SHARDS, nonempty=False))
        actual = {name for name, entries in resolved.items() if any(entry['suite'] in selected for entry in entries)}
        if not actual <= expected:
            raise ValueError('resolved Formal coverage requires a shard outside the trusted plan')
    if shard:
        return [entry for entry in resolved[shard] if entry['suite'] in selected]
    if suite not in selected:
        raise ValueError('logical suite was not selected: ' + str(suite))
    return [dict(entry, suite=suite) for entry in manifest['suites'][suite]]


def dependency_closure(manifest, config, suite=None, shard=None):
    binaries = manifest['binaries']
    if suite == 'ordinary':
        required = {name for name, binary in binaries.items()
                    if (binary.get('ordinary') and set(binary['tests']) - set(binary['ignored_tests']))
                    or name in ('viewer', 'snapshot-tests', 'viewer-snapshot-tests') or name.startswith('required-executable-')}
        for owner, _ in snapshot_regressions(manifest):
            if owner not in required:
                raise ValueError('missing default-feature snapshot harness: ' + owner)
        if (fast_plan(manifest.get('execution_plan')) is not None
                and set(required_packages(manifest['execution_plan'])) & VIEWER_REQUIRED_CONSUMERS
                and 'viewer' not in required):
            raise ValueError('missing selected Viewer process dependency')
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


def pack(bundle, archive_path, suite=None, shard=None, plan=None):
    """Publish one consumption closure; the producer's complete inventory remains auditable."""
    manifest = json.loads((bundle / 'manifest.json').read_text())
    if manifest.get('execution_plan') != fast_plan(plan):
        raise ValueError('artifact execution plan mismatch')
    if manifest.get('formal_authority_sha256') != formal_authority_digest():
        raise ValueError('artifact Formal authority mismatch')
    validate_ordinary_selection(manifest)
    config = formal_configuration()
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
CACHE_LIBRARY_KINDS = frozenset(('lib', 'rlib', 'dylib', 'cdylib', 'staticlib', 'proc-macro'))


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


def cache_begin(root, selection='all'):
    if selection not in ('all', 'libraries'):
        raise ValueError('unknown Cargo cache selection: ' + str(selection))
    root = root.resolve()
    return dict(schema_version=CACHE_SCHEMA, checkout=str(root),
                target=str((root / 'target').resolve()), inputs=_cache_inputs(root),
                selection=selection, packages={}, input_owners={}, files=[], directories=[], complete=False)


def _cache_relative(state, path):
    path = Path(path)
    target = Path(state['target'])
    alias = Path(state['checkout']) / 'target'
    if path.is_relative_to(alias):
        path = target / path.relative_to(alias)
    name = path.relative_to(target).as_posix()
    _cache_path(target, name)
    return name


def _cache_defer_unhashed_library(state, message, package, filenames):
    """Local dylib/cdylib outputs do not carry Cargo's unit hash."""
    kinds = set(message['target']['kind'])
    if (message['profile']['test'] is not False or not kinds <= CACHE_LIBRARY_KINDS
            or not kinds.intersection(('dylib', 'cdylib'))):
        return False
    name = message['target']['name'].replace('-', '_')
    pattern = re.compile(r'(?:lib)?' + re.escape(name) + r'\.(?:rlib|rmeta|so|dylib|dll|dll\.lib|dll\.a|lib|a|pdb)')
    paths = [Path(filename) for filename in filenames]
    profiles = {p.parent.parent if p.parent.name == 'deps' else p.parent for p in paths}
    if len(profiles) != 1 or any(not pattern.fullmatch(p.name) for p in paths):
        raise ValueError('unsupported unhashed library output layout: ' + package + '/' + name)
    profile = profiles.pop()
    # Cargo replaces these outputs and their package-scoped fingerprint when
    # subsequent feature variants run. Retain the last observation, not a
    # union of incompatible versions of the same public filenames.
    key = (profile / name).as_posix()
    pending = state.setdefault('unhashed_libraries', {})
    entry = dict(package=package, name=name, profile=profile.as_posix(),
                 filenames=filenames, features=message['features'],
                 manifest_path=message['manifest_path'])
    if key in pending and (pending[key]['manifest_path'] != entry['manifest_path']
                           or pending[key]['package'] != package):
        raise ValueError('unhashed library output collision: ' + package + '/' + name)
    pending[key] = entry
    return True


def _cache_finish_unhashed_libraries(state):
    """Resolve complete fingerprints only after Cargo has finished writing them."""
    target = Path(state['target'])
    selected, directories = [], []
    for entry in state.get('unhashed_libraries', {}).values():
        package, name, profile = entry['package'], entry['name'], Path(entry['profile'])
        owner = package + '/' + name
        fingerprint_root = _cache_path(target, (profile / '.fingerprint').as_posix())
        marker = 'lib-' + name
        candidates = []
        for candidate in fingerprint_root.iterdir():
            if not re.fullmatch(re.escape(package) + r'-[0-9a-f]{16}', candidate.name):
                continue
            relative = candidate.relative_to(target).as_posix()
            candidate = _cache_path(target, relative)
            if not candidate.is_dir():
                continue
            # Test/build-script fingerprints are not this library's owner.
            if not _cache_path(target, relative + '/' + marker).exists():
                continue
            unit_hash = candidate.name.rsplit('-', 1)[-1]
            # Check-mode and other hashed library units can coexist in the
            # dependency cache. Their concrete outputs identify them, so they
            # must not be assigned to this unhashed artifact.
            deps = _cache_path(target, (profile / 'deps').as_posix())
            hashed = re.compile(r'(?:lib)?' + re.escape(name) + '-' + unit_hash + r'(?:\..+)?')
            if any(hashed.fullmatch(p.name) for p in deps.iterdir()):
                continue
            candidates.append(relative)
        if len(candidates) != 1:
            raise ValueError('cannot identify a unique unhashed Cargo library unit: ' + owner)
        directory = candidates[0]
        for suffix in (marker, marker + '.json', 'dep-' + marker, 'invoked.timestamp'):
            path = _cache_path(target, directory + '/' + suffix)
            if not path.is_file():
                raise ValueError('incomplete unhashed Cargo library fingerprint: ' + owner)
        metadata = json.loads(_cache_path(target, directory + '/' + marker + '.json').read_text())
        features = metadata.get('features') if isinstance(metadata, dict) else None
        if not isinstance(features, str) or json.loads(features) != entry['features']:
            raise ValueError('unhashed Cargo library features changed: ' + owner)
        for filename in entry['filenames']:
            output = _cache_path(target, filename)
            if not output.is_file():
                raise ValueError('missing unhashed Cargo library output: ' + filename)
            selected.append(filename)
            # JSON may report the uplifted path. Preserve the concrete deps
            # output as well, but only when Cargo's hard link proves ownership.
            concrete = profile / 'deps' / output.name
            if Path(filename) != concrete:
                path = _cache_path(target, concrete.as_posix())
                if not path.is_file() or not os.path.samefile(output, path):
                    raise ValueError('unbound unhashed Cargo library output: ' + filename)
                selected.append(concrete.as_posix())
            public_dep = output.with_suffix('.d')
            if output.parent.name != 'deps' and public_dep.exists():
                selected.append(_cache_relative(state, public_dep))
        dep_info = (profile / 'deps' / (name + '.d')).as_posix()
        if not _cache_path(target, dep_info).is_file():
            raise ValueError('missing unhashed Cargo library dep-info: ' + owner)
        selected.append(dep_info)
        directories.append(directory)
    # No partial set is committed if any pending unit failed validation.
    state['files'].extend(selected)
    state['directories'].extend(directories)


def cache_record(state, message):
    """Select concrete Cargo units, including both build-script unit hashes."""
    reason = message.get('reason')
    target_root = Path(state['target'])
    if reason == 'compiler-artifact':
        manifest = Path(message['manifest_path'])
        root = Path(state['checkout'])
        if not manifest.is_relative_to(root):
            return
        manifest_name = manifest.relative_to(root).as_posix()
        manifest = _cache_path(root, manifest_name)
        generators = state.setdefault('input_owners', {}).setdefault(manifest_name, [])
        if set(message['target']['kind']).intersection(('custom-build', 'proc-macro')):
            source = Path(message['target']['src_path']).relative_to(root).as_posix()
            _cache_path(root, source)
            if source not in generators:
                generators.append(source)
        if state.get('selection', 'all') == 'libraries':
            kinds = set(message['target']['kind'])
            if ('custom-build' not in kinds and (message['profile']['test'] is not False
                    or not kinds or not kinds <= CACHE_LIBRARY_KINDS)):
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
            if not hashed and _cache_defer_unhashed_library(state, message, package, filenames):
                return
            if len(set(hashed)) != 1:
                raise ValueError('cannot identify a unique local Cargo output unit: ' + package + '/' + target_name)
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
    state['complete'] = False
    if _cache_inputs(Path(state['checkout'])) != state['inputs']:
        raise ValueError('source inputs changed during the build')
    _cache_finish_unhashed_libraries(state)
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
        report['selection'] = state.get('selection', 'all')
        if report['selection'] not in ('all', 'libraries'):
            raise ValueError('unknown Cargo cache selection')
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
                        inputs=state['inputs'], files=files, directories=directories,
                        input_owners=state.get('input_owners'))
        (cache / 'manifest.json').write_text(json.dumps(manifest, sort_keys=True) + '\n')
        report['save'] = True
    except (OSError, ValueError, KeyError, TypeError) as error:
        report['reason'] = str(error)
    finally:
        report['elapsed_seconds'] = round(time.monotonic() - started, 3)
    return report


def _cache_input_refresh(root, inputs, current, owners, report):
    """Preserve unaffected mtimes; make path changes visible to Cargo itself."""
    for name in inputs:
        _cache_path(root, name)
        if Path(name).parts[0] in ('.git', 'target'):
            raise ValueError('invalid cache source input: ' + name)
    added, removed = set(current) - set(inputs), set(inputs) - set(current)
    report['input_path_changes'] = {
        key: dict(count=len(paths), paths=sorted(paths)[:50], truncated=len(paths) > 50)
        for key, paths in (('added', added), ('removed', removed))
    }
    if not added and not removed:
        return set(), set()
    # Old snapshots have no observed Cargo ownership. Keep their old closed
    # behavior on a path-set change, rather than guessing from directory names.
    if not isinstance(owners, dict) or not owners:
        raise ValueError('path changes require Cargo input ownership metadata')
    roots, generators = set(), set()
    for manifest, sources in owners.items():
        _cache_path(root, manifest)
        if (Path(manifest).name != 'Cargo.toml' or manifest not in inputs
                or manifest not in current or not isinstance(sources, list)):
            raise ValueError('invalid or changed Cargo input owner: ' + manifest)
        roots.add(Path(manifest).parent)
        for source in sources:
            _cache_path(root, source)
            if source not in inputs:
                raise ValueError('unrecorded Cargo generator input: ' + source)
            generators.add(source)
    delta = added | removed
    if any(Path(name).name in ('Cargo.toml', 'Cargo.lock', 'rust-toolchain', 'rust-toolchain.toml')
           or '.cargo' in Path(name).parts for name in delta):
        raise ValueError('Cargo configuration paths changed')
    old_dirs = {parent for name in inputs for parent in Path(name).parents}
    new_dirs = {parent for name in current for parent in Path(name).parents}
    common_dirs = old_dirs & new_dirs
    scopes, directories = set(), set()
    for name in delta:
        path = Path(name)
        owners_for_path = [owner for owner in roots if path.is_relative_to(owner)]
        if owners_for_path:
            # Package-wide promotion also catches newly ambiguous Rust module
            # layouts and auto-discovered targets absent from old dep-info.
            scope = max(owners_for_path, key=lambda owner: len(owner.parts))
        else:
            # Retain every input, including documents and ignored files. A
            # previously observed sibling subtree is the conservative boundary
            # for an input outside Cargo packages; a new top-level tree widens
            # to the checkout root, never to an unproven exclusion list.
            scope = next(parent for parent in path.parents if parent in common_dirs)
        scopes.add(scope)
        directories.update(parent for parent in path.parents if parent in new_dirs)
    # Build scripts and proc macros can observe directory membership outside
    # their package. Recompile their observed entry sources on every path-set
    # change, leaving Cargo to propagate dirtiness to dependent units.
    refresh = {name for name in current if name in generators
               or any(Path(name).is_relative_to(scope) for scope in scopes)}
    report['refreshed_input_scopes'] = sorted(scope.as_posix() for scope in scopes)[:50]
    report['refreshed_input_scope_count'] = len(scopes)
    report['refreshed_generator_inputs'] = len(generators & set(current))
    return refresh, directories


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
        now = time.time_ns()
        for metadata in [*files.values(), *directories.values(), *inputs.values()]:
            if (not isinstance(metadata, dict) or type(metadata['mtime_ns']) is not int or not 0 < metadata['mtime_ns'] < now
                    or type(metadata['mode']) is not int or metadata['mode'] & ~0o777):
                raise ValueError('invalid cache timestamp or mode')
        for metadata in [*files.values(), *inputs.values()]:
            if (type(metadata['bytes']) is not int or metadata['bytes'] < 0
                    or not isinstance(metadata['sha256'], str) or not re.fullmatch('[0-9a-f]{64}', metadata['sha256'])):
                raise ValueError('invalid cache file length or checksum')
        refresh, input_directories = _cache_input_refresh(
            root, inputs, current, manifest.get('input_owners'), report)
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
        changed = matched = refreshed = 0
        # Validation rejects future metadata; this stamp is strictly newer than
        # every restored Cargo freshness reference, including build-script runs.
        stamp = time.time_ns()
        for name in current:
            metadata = inputs.get(name)
            same = metadata is not None and all(
                current[name][key] == metadata[key] for key in ('bytes', 'mode', 'sha256'))
            changed += not same
            matched += same
            dirty = not same or name in refresh
            refreshed += dirty
            path = _cache_path(root, name)
            value = stamp if dirty else metadata['mtime_ns']
            os.utime(path, ns=(value, value))
        # A removed input stays absent. Stamp surviving parent directories so
        # rerun-if-changed directory observers see removals as well as additions.
        for directory in input_directories:
            path = root if directory == Path('.') else _cache_path(root, directory.as_posix())
            if path.is_dir():
                os.utime(path, ns=(stamp, stamp))
        report.update(restored=True, changed_inputs=changed, matched_inputs=matched,
                      refreshed_inputs=refreshed, raw_bytes=sum(item['bytes'] for item in files.values()))
    except (OSError, ValueError, KeyError, TypeError) as error:
        if written:
            for name in files:
                _cache_path(target, name).unlink(missing_ok=True)
        report['reason'] = str(error)
    finally:
        report['elapsed_seconds'] = round(time.monotonic() - started, 3)
    return report


def load(bundle, suite=None, shard=None, plan=None):
    validate_plan_identity(plan)
    manifest = json.loads((bundle / 'manifest.json').read_text())
    if manifest.get('execution_plan') != fast_plan(plan):
        raise ValueError('artifact execution plan mismatch')
    validate_ordinary_selection(manifest)
    if manifest.get('scope') == 'storage' and manifest.get('schema_version') != 2:
        raise ValueError('expected schema 2 storage artifact')
    config = formal_configuration()
    expected = dict(tested_sha=output(['git', 'rev-parse', 'HEAD']),
                    source_sha=os.environ.get('SOURCE_SHA', output(['git', 'rev-parse', 'HEAD'])),
                    producer_run_id=os.environ.get('GITHUB_RUN_ID', 'local'), checkout=str(ROOT),
                    config_sha256=digest(CONFIG),
                    formal_authority_sha256=formal_authority_digest(),
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


def execute(bundle, suite, results, shard=None, plan=None):
    from ci_process_runner import run
    manifest, env = load(bundle, suite if not shard else None, shard, plan)
    if suite == 'ordinary' and not shard:
        entries = [dict(binary=name, batch=True) for name, binary in manifest['binaries'].items() if binary['ordinary'] and set(binary['tests']) - set(binary['ignored_tests'])]
        for owner, pattern in snapshot_regressions(manifest):
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
        entries = selected_entries(manifest, formal_configuration(), suite if not shard else None, shard)
    if not entries:
        raise ValueError('selected test execution is empty')
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
    global ROOT, CONFIG, FORMAL_AUTHORITY
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['build', 'run', 'pack', 'shards', 'required-args', 'cache-build', 'cache-budget', 'cache-stage', 'cache-restore'])
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
    parser.add_argument('--plan', type=Path, help='the same trusted selection used by producer and consumers')
    parser.add_argument('--formal-config', type=Path, help='trusted baseline Formal minimum inventory')
    parser.add_argument('--phase', choices=['native', 'wasm', 'packages'], default='native')
    args = parser.parse_args()
    ROOT = args.repo_root.resolve()
    CONFIG = ROOT / 'scripts/ci-formal-process-suites.json'
    FORMAL_AUTHORITY = args.formal_config.resolve() if args.formal_config else None
    plan = json.loads(args.plan.read_text()) if args.plan else None
    if args.action == 'shards':
        print(json.dumps(formal_shards(plan)))
        return
    if args.action == 'required-args':
        validate_plan_identity(plan)
        print('\n'.join(required_arguments(plan, args.phase)))
        return
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
        build(args.bundle.resolve(), args.scope, args.cache_state, plan)
    elif args.action == 'pack':
        if not args.output:
            parser.error('pack requires --output')
        if args.split_formal:
            for shard in formal_shards(plan):
                pack(args.bundle.resolve(), args.output / (shard + '.tar.gz'), shard=shard, plan=plan)
        else:
            pack(args.bundle.resolve(), args.output, None if args.shard else args.suite, args.shard, plan)
    else:
        execute(args.bundle.resolve(), args.suite, args.results or ROOT / 'output/formal-process/results.jsonl', args.shard, plan)


if __name__ == '__main__':
    main()
