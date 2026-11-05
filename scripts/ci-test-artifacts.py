#!/usr/bin/env python3
"""Build once and run native test executables without Cargo on consumers."""
import argparse
from concurrent.futures import FIRST_COMPLETED, ThreadPoolExecutor, wait
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import re
import shutil
import signal
import stat
import subprocess
import sys
import tarfile
import tempfile
import threading
import time
import tomllib

import shlex

class ProviderArtifact:
    subprocess = subprocess
    MODULE = 'module.runtime.local-test-provider'
    MANIFEST = 'crates/oasis7_builtin_wasm_modules/local_test_provider/Cargo.toml'
    TARGET = 'wasm32-unknown-unknown'
    DIRECTORY = 'preview-provider'
    VARIABLES = ('OASIS7_PREVIEW_TEST_PROVIDER_WASM', 'OASIS7_PREVIEW_TEST_PROVIDER_METADATA')


    @staticmethod
    def digest(path):
        return hashlib.sha256(path.read_bytes()).hexdigest()


    @staticmethod
    def checked(root, relative):
        if not isinstance(relative, str) or not relative or Path(relative).is_absolute() or any(p in ('', '.', '..') for p in relative.split('/')):
            raise ValueError('invalid provider artifact path')
        path = root / relative
        cursor = root
        for part in relative.split('/'):
            cursor /= part
            if cursor.is_symlink():
                raise ValueError('provider artifact symlink')
        if not path.is_file() or not path.resolve().is_relative_to(root.resolve()):
            raise ValueError('provider material missing or outside root')
        return path


    @staticmethod
    def verify_metadata(root, bundle, paths):
        subprocess.run([str(paths['checker']), '--verify-provider-artifact', str(root),
                        str(paths['wasm']), str(paths['metadata']), str(paths['receipt'])],
                       cwd=root, check=True, env=dict(os.environ, LD_LIBRARY_PATH=str(bundle / ProviderArtifact.DIRECTORY / 'lib') + os.pathsep + os.environ.get('LD_LIBRARY_PATH', '')))


    @staticmethod
    def tree_digest(root):
        hasher = hashlib.sha256(subprocess.check_output(['git', 'diff', '--binary', 'HEAD'], cwd=root))
        hasher.update(subprocess.check_output(['git', 'rev-parse', 'HEAD^{tree}'], cwd=root))
        for name in sorted(subprocess.check_output(['git', 'ls-files', '--others', '--exclude-standard', '-z'], cwd=root).decode().split('\0')):
            if name:
                hasher.update(name.encode() + b'\0')
                hasher.update(ProviderArtifact.checked(root, name).read_bytes())
        return hasher.hexdigest()


    @staticmethod
    def identity(root):
        tested = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
        return {'tested_sha': tested, 'source_sha': os.environ.get('SOURCE_SHA', tested), 'checkout_tree_sha256': ProviderArtifact.tree_digest(root)}


    @staticmethod
    def build(root, bundle, expected):
        directory = bundle / ProviderArtifact.DIRECTORY
        directory.mkdir(parents=True, exist_ok=False)
        before = ProviderArtifact.identity(root)
        if any(before[key] != expected[key] for key in ('tested_sha', 'source_sha')):
            raise ValueError('provider producer source differs from native identity')
        (root / 'target').mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(prefix='ci-preview-provider-', dir=root / 'target') as temporary:
            subprocess.run(['bash', str(root / 'scripts/build-wasm-module.sh'), '--candidate-build',
                            '--manifest-path', ProviderArtifact.MANIFEST, '--out-dir', temporary,
                            '--module-id', ProviderArtifact.MODULE, '--profile', 'release'], cwd=root, check=True)
            for suffix in ('.wasm', '.metadata.json', '.build-receipt.json'):
                shutil.copy2(Path(temporary) / (ProviderArtifact.MODULE + suffix), directory / (ProviderArtifact.MODULE + suffix))
        files = {kind: f'{ProviderArtifact.DIRECTORY}/{ProviderArtifact.MODULE}{suffix}' for kind, suffix in
                 (('wasm', '.wasm'), ('metadata', '.metadata.json'), ('receipt', '.build-receipt.json'))}
        subprocess.run(['cargo', 'build', '--locked', '-p', 'oasis7_distfs', '--features',
                        'sync-builtin-wasm-identity', '--bin', 'sync_builtin_wasm_identity'],
                       cwd=root, check=True, env={key:value for key,value in os.environ.items() if key != 'RUSTC_WRAPPER'})
        checker = 'sync_builtin_wasm_identity' + ('.exe' if os.name == 'nt' else '')
        files['checker'] = f'{ProviderArtifact.DIRECTORY}/{checker}'
        shutil.copy2(root / 'target/debug' / checker, bundle / files['checker'])
        libraries = []
        if sys.platform == 'linux':
            linked = subprocess.run(['ldd', str(bundle / files['checker'])], text=True, capture_output=True, check=True)
            for line in linked.stdout.splitlines():
                fields = line.split()
                if '=>' in fields and len(fields) > 2 and fields[2].startswith(str(root / 'target') + '/'):
                    library = Path(fields[2])
                    relative = f'{ProviderArtifact.DIRECTORY}/lib/{library.name}'
                    (directory / 'lib').mkdir(exist_ok=True)
                    shutil.copy2(library, bundle / relative)
                    libraries.append({'path': relative, 'sha256': ProviderArtifact.digest(bundle / relative)})
        if ProviderArtifact.identity(root) != before:
            raise ValueError('provider source changed during official build')
        record = dict(before, libraries=libraries, files={kind: {'path': name, 'sha256': ProviderArtifact.digest(ProviderArtifact.checked(bundle, name))}
                                       for kind, name in files.items()})
        ProviderArtifact.validate(root, bundle, record, expected)
        return record


    @staticmethod
    def validate(root, bundle, record, expected):
        if not isinstance(record, dict) or any(record.get(key) != expected[key] for key in ('tested_sha', 'source_sha')):
            raise ValueError('provider tested/source identity mismatch')
        if record.get('checkout_tree_sha256') != ProviderArtifact.tree_digest(root):
            raise ValueError('provider checkout tree differs')
        if set(record.get('files', {})) != {'wasm', 'metadata', 'receipt', 'checker'}:
            raise ValueError('provider artifact pair incomplete')
        libraries = record.get('libraries')
        if not isinstance(libraries, list):
            raise ValueError('provider checker libraries missing')
        seen = set()
        for library in libraries:
            name = library.get('path', '')
            if not name.startswith(ProviderArtifact.DIRECTORY + '/lib/') or name in seen or ProviderArtifact.digest(ProviderArtifact.checked(bundle, name)) != library.get('sha256'):
                raise ValueError('provider checker library identity mismatch')
            seen.add(name)
        paths = {}
        for kind, suffix in (('wasm', '.wasm'), ('metadata', '.metadata.json'), ('receipt', '.build-receipt.json'), ('checker', '')):
            entry = record['files'][kind]
            name = ('sync_builtin_wasm_identity' + ('.exe' if os.name == 'nt' else '')) if kind == 'checker' else ProviderArtifact.MODULE + suffix
            if entry.get('path') != f'{ProviderArtifact.DIRECTORY}/{name}':
                raise ValueError('provider artifact role/path mismatch')
            paths[kind] = ProviderArtifact.checked(bundle, entry['path'])
            if ProviderArtifact.digest(paths[kind]) != entry['sha256']:
                raise ValueError('provider artifact hash mismatch')
        if not os.access(paths['checker'], os.X_OK):
            raise ValueError('provider checker is not executable')
        ProviderArtifact.verify_metadata(root, bundle, paths)
        return dict(zip(ProviderArtifact.VARIABLES, (str(paths[kind].resolve()) for kind in ('wasm', 'metadata'))))


provider_artifact = ProviderArtifact

def preview_provider_required(manifest):
    plan = manifest.get('execution_plan') or {}
    expanded = plan.get('test_policy_changed', False) or plan.get('scope') in ('full', 'explicit-full') or plan.get('execution_profile') in ('periodic-full', 'candidate-full')
    selection = manifest.get('selection')
    ordinary = selection == {'suite': 'ordinary'} or (selection is None and manifest.get('scope') == 'ordinary')
    provider_consumer = any(binary.get("package") == "oasis7_chain_runtime"
                            and binary.get("test")
                            and "test_tier_full" in binary.get("features", [])
                            for binary in manifest.get("binaries", {}).values())
    return bool(expanded and ordinary and provider_consumer)


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
RUST_BASELINE_CHECKS = ('newapi_accounting', 'standalone_tool_lockfiles')


def fast_plan(plan):
    """Old trusted planners retain the complete previous execution inventory."""
    if (isinstance(plan, dict) and 'fast_execution_version' in plan
            and type(plan['fast_execution_version']) is not int):
        raise ValueError('invalid fast execution capability')
    if plan is None or (isinstance(plan, dict) and 'fast_plan_version' not in plan):
        if isinstance(plan, dict) and plan.get('fast_execution_version') == 1:
            raise ValueError('inline execution requires a compatible fast plan')
        return None
    if (not isinstance(plan, dict) or type(plan.get('fast_plan_version')) is not int
            or plan.get('fast_plan_version') != 1 or plan.get('execution_profile') not in FAST_PROFILES
            or plan.get('scope') not in ('targeted', 'full', 'explicit-full')
            or not isinstance(plan.get('groups'), list)
            or any(not isinstance(group, str) for group in plan['groups'])
            or len(plan['groups']) != len(set(plan['groups']))
            or type(plan.get('test_policy_changed', False)) is not bool):
        raise ValueError('unsupported fast execution plan')
    if 'fast_execution_version' in plan:
        version = plan['fast_execution_version']
        if version == 1:
            if (plan.get('ordinary_execution') != 'inline'
                    or plan['execution_profile'] not in ('pr-fast', 'main-fast')
                    or plan['scope'] != 'targeted' or plan.get('test_policy_changed', False)
                    or 'oasis7_required' not in plan['groups']
                    or type(plan.get('fast_test_inventory_version')) is not int
                    or plan['fast_test_inventory_version'] != 1
                    or not isinstance(plan.get('ordinary_fast_targets'), list)
                    or not plan['ordinary_fast_targets']):
                raise ValueError('inline execution requires a bounded ordinary fast inventory')
            plan_list(plan, 'rust_baseline_checks', RUST_BASELINE_CHECKS, nonempty=False)
            clippy_group = plan.get('ordinary_clippy_group', 'oasis7_required')
            if clippy_group not in ('oasis7_required', 'rust_baseline'):
                raise ValueError('invalid ordinary Clippy group')
            if clippy_group == 'rust_baseline' and 'rust_baseline' not in plan['groups']:
                raise ValueError('ordinary Clippy owner is not a selected required group')
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
    if phase == 'clippy-group':
        selected = fast_plan(plan)
        if selected is not None and selected.get('fast_execution_version') == 1:
            ordinary_fast_targets(selected)
            return [selected.get('ordinary_clippy_group', 'oasis7_required')]
        return ['oasis7_required']
    if phase == 'baseline-checks':
        selected = fast_plan(plan)
        if selected is not None and selected.get('fast_execution_version') == 1:
            ordinary_fast_targets(selected)
            return selected['rust_baseline_checks']
        return list(RUST_BASELINE_CHECKS)
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


def target_inventory(plan, version_key, targets_key):
    if type(plan[version_key]) is not int or plan[version_key] != 1:
        raise ValueError('unsupported ordinary target inventory: ' + version_key)
    targets = plan.get(targets_key)
    if not isinstance(targets, list) or not targets:
        raise ValueError('missing ordinary target inventory: ' + targets_key)
    packages = set(required_packages(plan))
    identities = set()
    fields = {'package', 'target_kind', 'target_name', 'default_features', 'features', 'cases'}
    for target in targets:
        if (not isinstance(target, dict) or set(target) != fields
                or not isinstance(target['package'], str) or target['package'] not in packages
                or target['target_kind'] not in ('lib', 'test')
                or not isinstance(target['target_name'], str)
                or re.fullmatch(r'[A-Za-z_][A-Za-z0-9_-]*', target['target_name']) is None
                or type(target['default_features']) is not bool):
            raise ValueError('invalid ordinary target identity')
        for key in ('features', 'cases'):
            values = target[key]
            if (not isinstance(values, list) or (key == 'cases' and not values)
                    or any(not isinstance(value, str) or not value or '\n' in value or '\r' in value for value in values)
                    or len(values) != len(set(values))
                    or (key == 'features' and any(re.fullmatch(r'[A-Za-z_][A-Za-z0-9_-]*', value) is None for value in values))):
                raise ValueError('invalid ordinary target ' + key)
        if not target['default_features'] and 'default' in target['features']:
            raise ValueError('ordinary target explicitly enables disabled default features')
        identity = (target['package'], target['target_kind'], target['target_name'])
        if identity in identities:
            raise ValueError('duplicate ordinary target identity')
        identities.add(identity)
    return targets


def ordinary_fast_targets(plan=None):
    """A versioned target inventory narrows only an unambiguous fast plan."""
    plan = fast_plan(plan)
    if (plan is None or 'fast_test_inventory_version' not in plan
            or plan['scope'] != 'targeted'
            or plan['execution_profile'] not in ('pr-fast', 'main-fast')
            or plan.get('test_policy_changed', False)):
        return None
    return target_inventory(plan, 'fast_test_inventory_version', 'ordinary_fast_targets')


def ordinary_full_fast_targets(plan=None):
    """Full runs retain the original inventory and add exact fast feature variants."""
    plan = fast_plan(plan)
    if (plan is None or 'full_fast_test_inventory_version' not in plan
            or plan['execution_profile'] not in ('periodic-full', 'candidate-full')):
        return None
    return target_inventory(plan, 'full_fast_test_inventory_version', 'ordinary_full_fast_targets')


def target_native_arguments(target):
    args = ['-p', target['package']]
    args += ['--lib'] if target['target_kind'] == 'lib' else ['--test', target['target_name']]
    if not target['default_features']:
        args.append('--no-default-features')
    if target['features']:
        args += ['--features', ','.join(target['features'])]
    return args


def required_native_commands(plan=None):
    targets = ordinary_fast_targets(plan)
    if targets is not None:
        return [target_native_arguments(target) for target in targets]
    return [required_arguments(plan)] + [target_native_arguments(target)
                                       for target in ordinary_full_fast_targets(plan) or []]


def target_build_key(target):
    return target['package'], target['default_features'], tuple(target['features'])


def target_build_variants(targets, prefix, combine=False):
    if not combine:
        return [(prefix + str(index), ['test', *target_native_arguments(target), '--no-run'], target['features'])
                for index, target in enumerate(targets or [])]
    # Never unify features across package roots. Only targets of the same
    # package with the exact same feature request share a Cargo invocation.
    groups = {}
    for index, target in enumerate(targets or []):
        groups.setdefault(target_build_key(target), []).append((index, target))
    variants = []
    for members in groups.values():
        index, first = members[0]
        args = ['test', '-p', first['package']]
        for _, target in members:
            args += ['--lib'] if target['target_kind'] == 'lib' else ['--test', target['target_name']]
        if not first['default_features']:
            args.append('--no-default-features')
        if first['features']:
            args += ['--features', ','.join(first['features'])]
        variants.append((prefix + str(index), args + ['--no-run'], first['features']))
    return variants


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
    # Concurrency is an allowance, not added coverage: candidate declarations
    # must not widen the BASE's exact case/resource policy.
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
    if ordinary_fast_targets(plan) is not None:
        return ()
    if plan is None:
        return DEFAULT_SNAPSHOT_REGRESSIONS
    packages = set(required_packages(plan))
    owners = {'viewer-snapshot-tests': 'oasis7_viewer_runtime', 'snapshot-tests': 'oasis7'}
    return tuple((owner, pattern) for owner, pattern in DEFAULT_SNAPSHOT_REGRESSIONS if owners[owner] in packages)


def validate_target_inventory(targets, binaries):
    expected = {(target['package'], (target['target_kind'],), target['target_name']): target for target in targets}
    actual = set()
    for binary in binaries:
        identity = (binary.get('package'), tuple(binary.get('target_kind', [])), binary.get('target'))
        if identity not in expected or identity in actual or not binary.get('test'):
            raise ValueError('ordinary target inventory mismatch')
        target = expected[identity]
        actual.add(identity)
        if (binary.get('requested_features') != target['features']
                or binary.get('default_features') is not target['default_features']
                or not set(target['features']) <= set(binary.get('features', []))
                or (not target['default_features'] and 'default' in binary.get('features', []))):
            raise ValueError('ordinary target feature inventory mismatch')
        behavior = set(binary.get('tests', [])) - set(binary.get('ignored_tests', []))
        if not set(target['cases']) <= behavior:
            raise ValueError('ordinary target minimum case inventory mismatch')
    if actual != set(expected):
        raise ValueError('ordinary target inventory mismatch')


def validate_ordinary_selection(manifest):
    plan = fast_plan(manifest.get('execution_plan'))
    if plan is None:
        return
    supplements = ordinary_full_fast_targets(plan)
    if manifest.get('scope') != 'ordinary' and not (manifest.get('scope') == 'all' and supplements is not None):
        return
    binaries = [entry for entry in manifest['binaries'].values() if entry.get('ordinary')]
    targets = ordinary_fast_targets(plan)
    if targets is not None:
        validate_target_inventory(targets, binaries)
        return
    if supplements is not None:
        validate_target_inventory(supplements, [entry for entry in binaries
                                  if entry.get('variant', '').startswith('required-full-fast-')])
        # Supplementary no-default-feature harnesses cannot replace any of the
        # original required packages, including the same package's default build.
        binaries = [entry for entry in binaries if not entry.get('variant', '').startswith('required-full-fast-')]
    expected = set(required_packages(plan))
    actual = {entry.get('package') for entry in binaries}
    if actual != expected:
        raise ValueError('ordinary test package inventory mismatch')
    if not any(set(entry['tests']) - set(entry['ignored_tests']) for entry in binaries):
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


FORMAL_COMPONENTS = ('formal-debug', 'formal-endpoint')


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
        return variants + target_build_variants(ordinary_full_fast_targets(plan), 'required-full-fast-')
    if scope == 'ordinary':
        targets = ordinary_fast_targets(plan)
        if targets is not None:
            return target_build_variants(targets, 'required-fast-',
                                         combine=plan.get('fast_execution_version') == 1)
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
            return selected + target_build_variants(ordinary_full_fast_targets(plan), 'required-full-fast-')
        return [v for v in variants if v[0] in ('viewer', 'required', 'snapshot-tests', 'viewer-snapshot-tests')]
    if scope in ('formal', 'storage', *FORMAL_COMPONENTS):
        # Keep both core libraries and the exact Chain binary harness. Chain
        # configuration library tests belong to the ordinary required variant;
        # integration tests, viewer executables and snapshot variants run elsewhere.
        # The CI-only marker reproduces shared dependency features from all
        # required roots without compiling their unrelated harnesses.
        features = ['test_tier_required', 'ci_formal_required_features']
        core = ('formal-required', ['test', '-p', 'oasis7', '-p', 'oasis7_chain_runtime',
                '--lib', '--bins', '--features', ','.join(features), '--no-run'], features)
        result = [core, *(v for v in variants if v[0] in ('runtime', 'endpoint', 'distfs-tests'))]
        if scope in FORMAL_COMPONENTS:
            return [variant for variant in result if (variant[0] == 'endpoint') == (scope == 'formal-endpoint')]
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
    if variant.startswith('required-full-fast-'):
        # Preserve both feature variants of a package/target in full bundles.
        return variant + '-' + artifact_logical_name('required', package, target, test)
    if variant.startswith('required-fast-'):
        return artifact_logical_name('required', package, target, test)
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
    if scope in ('formal', 'storage', *FORMAL_COMPONENTS):
        # Formal keeps the original cheap debug profile. Optimizing its entire
        # dependency graph and large test harnesses greatly increases cold cost.
        # Storage retains its established profile; release endpoint is separate.
        for profile in ('DEV', 'TEST'):
            env[f'CARGO_PROFILE_{profile}_OPT_LEVEL'] = '2' if scope == 'storage' else '0'
            env[f'CARGO_PROFILE_{profile}_DEBUG_ASSERTIONS'] = 'true'
            env[f'CARGO_PROFILE_{profile}_OVERFLOW_CHECKS'] = 'true'
    return env


def compiler_environment(env):
    """Inputs shared by split producers; omit credentials and execution-only knobs."""
    flags = {'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTDOCFLAGS', 'CARGO_ENCODED_RUSTDOCFLAGS',
             'RUSTC', 'RUSTDOC', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER', 'CARGO_INCREMENTAL',
             'CC', 'CXX', 'AR', 'CFLAGS', 'CXXFLAGS', 'CPPFLAGS', 'LDFLAGS', 'LIBRARY_PATH', 'CPATH'}
    prefixes = ('CARGO_PROFILE_', 'CARGO_TARGET_', 'CARGO_BUILD_', 'CC_', 'CXX_', 'AR_', 'PKG_CONFIG', 'OASIS7_WASM')
    return {key: value for key, value in sorted(env.items())
            if (key in flags or key.startswith(prefixes)) and key != 'CARGO_BUILD_JOBS'}


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
    elif scope in ('formal', 'formal-debug', 'storage'):
        profile = binary.get('profile', {})
        expected_opt = '2' if scope == 'storage' else '0'
        if (profile.get('opt_level') != expected_opt or profile.get('debug_assertions') is not True
                or profile.get('overflow_checks') is not True):
            policy = 'optimized storage' if scope == 'storage' else 'unoptimized ' + scope
            raise ValueError('expected ' + policy + ' artifact with assertions and overflow checks')


class BuildObservation:
    """Optional diagnostics; never take over Cargo output or its exit result."""

    def __init__(self, variant, directory=None, timing_directory=None):
        self.variant = variant
        self.directory = directory / variant if directory else None
        self.timing_directory = timing_directory
        self.process = self.observer = None
        self.before = {}
        self.started = time.monotonic()
        self.report = dict(variant=variant, status='running', cargo_exit_code=None, timings=[])

    def warn(self, error):
        print(f'::warning::Build diagnostics for {self.variant}: {type(error).__name__}', flush=True)

    def timing_files(self):
        if self.timing_directory is None:
            return {}
        return {path: (path.stat().st_mtime_ns, path.stat().st_size)
                for path in self.timing_directory.glob('cargo-timing*.html')
                if path.is_file() and not path.is_symlink()}

    def write(self):
        if self.directory is not None:
            try:
                self.report['elapsed_seconds'] = round(time.monotonic() - self.started, 3)
                temporary = self.directory / 'build.json.tmp'
                temporary.write_text(json.dumps(self.report, indent=2) + '\n')
                temporary.replace(self.directory / 'build.json')
            except OSError as error:
                self.warn(error)

    def __enter__(self):
        if self.directory is not None:
            try:
                self.directory.mkdir(parents=True, exist_ok=False)
                self.before = self.timing_files()
            except OSError as error:
                self.warn(error)
                self.directory = None
            else:
                self.write()
                print('native_build_start=' + json.dumps(self.report), flush=True)
        return self

    def attach(self, process):
        self.process = process
        if self.directory is not None:
            try:
                self.observer = subprocess.Popen([
                    sys.executable, str(Path(__file__).with_name('package-ecs-resource-monitor.py')),
                    '--out', str(self.directory / 'resources.json'), '--pid', str(process.pid),
                    '--label', self.variant, '--heartbeat-seconds', '20'], cwd=ROOT)
            except OSError as error:
                self.warn(error)

    def __exit__(self, kind, value, traceback):
        if self.observer is not None:
            try:
                if self.observer.poll() is None:
                    self.observer.terminate()
                try:
                    self.observer.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    self.observer.kill()  # Only our observer, never the build.
                    self.observer.wait(timeout=3)
                if self.observer.returncode:
                    print(f'::warning::Build resource observer exited {self.observer.returncode}', flush=True)
            except (OSError, subprocess.SubprocessError) as error:
                self.warn(error)
        if self.directory is not None:
            try:
                for path, identity in self.timing_files().items():
                    if self.before.get(path) != identity:
                        shutil.copy2(path, self.directory / path.name)
                        self.report['timings'].append(path.name)
            except OSError as error:
                self.warn(error)
            self.report['cargo_exit_code'] = self.process.returncode if self.process else None
            self.report['status'] = 'success' if kind is None and self.report['cargo_exit_code'] == 0 else 'failure'
            if kind is not None:
                self.report['error_type'] = kind.__name__
            self.write()
            print('native_build_end=' + json.dumps(self.report), flush=True)
        return False


def build(bundle, scope='all', cache_state_path=None, plan=None, diagnostics_dir=None):
    validate_plan_identity(plan)
    if scope != 'ordinary' and FORMAL_AUTHORITY is not None:
        formal_shards(plan)
    bundle.mkdir(parents=True, exist_ok=False)
    (bundle / 'bin').mkdir()
    (bundle / 'lib').mkdir()
    env = build_environment(scope)
    timing_directory = None
    if diagnostics_dir is not None:
        diagnostics_dir = Path(diagnostics_dir).resolve()
        try:
            diagnostics_dir.mkdir(parents=True, exist_ok=False)
        except OSError as error:
            print(f'::warning::Build diagnostics unavailable: {type(error).__name__}', flush=True)
            diagnostics_dir = None
        if diagnostics_dir is not None:
            try:
                metadata = json.loads(output(['cargo', 'metadata', '--locked', '--no-deps', '--format-version=1']))
                timing_directory = Path(metadata['target_directory']) / 'cargo-timings'
            except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
                print(f'::warning::Cargo timing directory unavailable: {type(error).__name__}', flush=True)
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
    target_specs = {}
    if scope in ('ordinary', 'all'):
        for prefix, targets in (('required-fast-', ordinary_fast_targets(plan)),
                                ('required-full-fast-', ordinary_full_fast_targets(plan))):
            target_specs.update({prefix + str(index): target for index, target in enumerate(targets or [])})
    for variant, args, features in variants:
        members = {variant: target_specs[variant]} if variant in target_specs else {}
        if (members and scope == 'ordinary' and plan is not None
                and plan.get('fast_execution_version') == 1):
            key = target_build_key(target_specs[variant])
            members = {name: target for name, target in target_specs.items()
                       if name.startswith('required-fast-') and target_build_key(target) == key}
        started = time.monotonic()
        local_units = local_fresh = dependency_units = dependency_fresh = 0
        command = ['cargo', *args, '--locked', '--message-format=json-render-diagnostics']
        if diagnostics_dir is not None:
            command.append('--timings')
        print('+ ' + ' '.join(command), flush=True)
        with BuildObservation(variant, diagnostics_dir, timing_directory) as observation, \
                subprocess.Popen(command, stdout=subprocess.PIPE, text=True, env=env, cwd=ROOT) as process:
            observation.attach(process)
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
                artifact_variant = variant
                if members and test:
                    matching = [name for name, spec in members.items()
                                if (spec['package'], [spec['target_kind']], spec['target_name']) ==
                                   (package, target['kind'], target['name'])]
                    if len(matching) != 1:
                        raise ValueError('unexpected test target in Cargo build: ' + variant)
                    artifact_variant = matching[0]
                logical = artifact_logical_name(artifact_variant, package, target, test)
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
                    ordinary=(variant == 'required' or artifact_variant in target_specs) and test, variant=artifact_variant,
                    bytes=destination.stat().st_size, libraries=[])
                if artifact_variant in target_specs:
                    binaries[logical]['default_features'] = target_specs[artifact_variant]['default_features']
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
        if len(members) > 1:
            timings[-1]['target_variants'] = list(members)
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
    component = scope in FORMAL_COMPONENTS
    manifest['suites'] = resolve_suites(config, binaries) if scope != 'ordinary' and not component else {}
    if component:
        manifest['formal_component_version'] = 1
        manifest['compiler_environment'] = compiler_environment(env)
    elif scope == 'formal':
        manifest['formal_execution_version'] = 1
    if scope != 'ordinary' and not component:
        resolve_shards(config, manifest['suites'])
    manifest['libraries'] = {str(p.relative_to(bundle)): digest(p) for p in (bundle / 'lib').iterdir()}
    manifest['included_binaries'] = sorted(binaries)
    manifest['selection'] = None
    if scope in ('ordinary', 'all') and preview_provider_required(dict(manifest, selection={'suite': 'ordinary'})):
        manifest['preview_provider'] = provider_artifact.build(ROOT, bundle, manifest)
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


def complete_bundle_scope(manifest):
    scope = manifest.get('scope')
    if scope is None and manifest.get('schema_version', 1) == 1:
        return  # Original manifests predate named scopes and execute serially.
    if scope in FORMAL_COMPONENTS:
        raise ValueError('partial Formal component must be validated and merged before consumption')
    if scope not in ('all', 'ordinary', 'formal', 'storage'):
        raise ValueError('unknown native bundle scope')
    if 'formal_execution_version' in manifest and type(manifest['formal_execution_version']) is not int:
        raise ValueError('invalid Formal execution version')


def formal_component_targets(scope):
    """Resolve the actual checkout's complete lib/bin selectors without compiling."""
    metadata = json.loads(output(['cargo', 'metadata', '--locked', '--no-deps', '--format-version=1']))
    packages = {package['name']: package for package in metadata['packages']}
    expected = {}
    for variant, args, features in build_variants(scope):
        roots = [args[index + 1] for index, arg in enumerate(args) if arg == '-p']
        for root in roots:
            package = packages[root]
            available = package['features']
            enabled = set(features) & set(available)
            if '--no-default-features' not in args and 'default' in available:
                enabled.add('default')
            pending = list(enabled)
            while pending:
                for feature in available[pending.pop()]:
                    if feature in available and feature not in enabled:
                        enabled.add(feature)
                        pending.append(feature)
            for target in package['targets']:
                kinds = target['kind']
                selected = ('--lib' in args and kinds == ['lib']) or (
                    kinds == ['bin'] and ('--bins' in args or (
                        '--bin' in args and args[args.index('--bin') + 1] == target['name'])))
                if not selected or not set(target.get('required-features', [])) <= enabled:
                    continue
                test = args[0] == 'test'
                logical = artifact_logical_name(variant, root, target, test)
                if logical in expected:
                    raise ValueError('duplicate Formal component target: ' + logical)
                expected[logical] = dict(package=root, target=target['name'], target_kind=kinds,
                    variant=variant, test=test, ordinary=False, requested_features=features,
                    cwd=str(Path(package['manifest_path']).parent.relative_to(ROOT)),
                    enabled_features=sorted(enabled), available_features=set(available))
    if not expected:
        raise ValueError('Formal component has no selected Cargo targets')
    return expected


def merge_formal(debug, endpoint, bundle, plan=None):
    """Join two complete build roles only after validating every input artifact."""
    validate_plan_identity(plan)
    configuration = formal_configuration()
    tested = output(['git', 'rev-parse', 'HEAD'])
    identity = dict(schema_version=2, tested_sha=tested,
        source_sha=os.environ.get('SOURCE_SHA', tested), producer_run_id=os.environ.get('GITHUB_RUN_ID', 'local'),
        checkout=str(ROOT), config_sha256=digest(CONFIG), formal_authority_sha256=formal_authority_digest(),
        rust_toolchain=os.environ.get('RUST_TOOLCHAIN', 'local'), rustc_version=output(['rustc', '--version']),
        target_triple=output(['rustc', '-vV']).split('host: ')[1].splitlines()[0],
        runner_image=os.environ.get('ImageOS', sys.platform), execution_plan=fast_plan(plan),
        compiler_environment=compiler_environment(build_environment('formal')))
    current_attempt = os.environ.get('GITHUB_RUN_ATTEMPT', '1')
    if not re.fullmatch(r'[1-9][0-9]*', current_attempt):
        raise ValueError('invalid current producer attempt')
    binaries, libraries, components, timings, inputs, component_names = {}, {}, {}, {}, {}, {}
    for scope, source in zip(FORMAL_COMPONENTS, (debug, endpoint)):
        source = Path(source)
        manifest_path = ProviderArtifact.checked(source, 'manifest.json')
        manifest = json.loads(manifest_path.read_text())
        if (manifest.get('scope') != scope or type(manifest.get('formal_component_version')) is not int
                or manifest['formal_component_version'] != 1 or manifest.get('selection') is not None
                or manifest.get('suites') != {}):
            raise ValueError('invalid Formal component role or completeness')
        for key, expected in identity.items():
            if manifest.get(key) != expected:
                raise ValueError('Formal component identity mismatch: ' + key)
        attempt = manifest.get('producer_attempt')
        if (not isinstance(attempt, str) or not re.fullmatch(r'[1-9][0-9]*', attempt)
                or int(attempt) > int(current_attempt)):
            raise ValueError('invalid or future Formal component attempt')
        expected = formal_component_targets(scope)
        component_names[scope] = list(expected)
        actual = manifest['binaries']
        if set(actual) != set(expected) or manifest.get('included_binaries') != sorted(expected):
            raise ValueError('incomplete or expanded Formal component target inventory')
        variants = [name for name, _, _ in build_variants(scope)]
        if [entry['variant'] for entry in manifest['build_timings']] != variants:
            raise ValueError('Formal component build contexts differ')
        for entry in manifest['build_timings']:
            if entry['variant'] in timings:
                raise ValueError('duplicate Formal component build context')
            timings[entry['variant']] = entry
        needed_libraries = set()
        for logical, spec in expected.items():
            entry = actual[logical]
            enabled, available = spec.pop('enabled_features'), spec.pop('available_features')
            if any(entry.get(key) != value for key, value in spec.items()):
                raise ValueError('Formal component target identity mismatch: ' + logical)
            features = entry.get('features')
            if (not isinstance(features, list) or any(not isinstance(feature, str) for feature in features)
                    or len(features) != len(set(features)) or not set(enabled) <= set(features)
                    or not set(features) <= available):
                raise ValueError('Formal component target features differ: ' + logical)
            if (logical in binaries or entry.get('path') != 'bin/' + logical
                    or entry['profile'].get('test') is not entry['test']
                    or not Path(entry['original_executable']).is_absolute()
                    or not Path(entry['original_executable']).resolve().is_relative_to((ROOT / 'target').resolve())):
                raise ValueError('duplicate or invalid Formal component executable: ' + logical)
            validate_profile(logical, entry, scope)
            path = ProviderArtifact.checked(source, entry['path'])
            if (digest(path) != entry['sha256'] or path.stat().st_size != entry['bytes']
                    or not os.access(path, os.X_OK)):
                raise ValueError('invalid Formal component executable bytes or mode')
            tests, ignored = entry.get('tests', []), entry.get('ignored_tests', [])
            if (not isinstance(tests, list) or any(not isinstance(name, str) or not name for name in tests)
                    or tests != sorted(set(tests)) or not isinstance(ignored, list)
                    or any(not isinstance(name, str) or not name for name in ignored)
                    or ignored != sorted(set(ignored)) or not set(ignored) <= set(tests)):
                raise ValueError('invalid Formal component case inventory')
            if entry['test'] and entry.get('test_list_sha256') != hashlib.sha256(json.dumps(tests).encode()).hexdigest():
                raise ValueError('Formal component test inventory digest mismatch')
            binaries[logical] = entry
            inputs[entry['path']] = (path, entry['sha256'])
            needed_libraries.update(entry['libraries'])
        if set(manifest['libraries']) != needed_libraries:
            raise ValueError('Formal component dynamic library closure mismatch')
        for name, sha in manifest['libraries'].items():
            if not name.startswith('lib/') or len(Path(name).parts) != 2:
                raise ValueError('invalid Formal component library path')
            path = ProviderArtifact.checked(source, name)
            if digest(path) != sha or (name in libraries and libraries[name] != sha):
                raise ValueError('invalid or conflicting Formal component library')
            libraries[name], inputs[name] = sha, (path, sha)
        components[scope] = dict(producer_attempt=attempt, manifest_sha256=digest(manifest_path))
    # Validate the entire minimum inventory before listing or copying any binary.
    suites = resolve_suites(configuration, binaries)
    resolve_shards(configuration, suites)
    for scope, source in zip(FORMAL_COMPONENTS, (debug, endpoint)):
        env = dict(os.environ, LD_LIBRARY_PATH=str(Path(source) / 'lib') + os.pathsep + os.environ.get('LD_LIBRARY_PATH', ''))
        for logical in component_names[scope]:
            entry = binaries[logical]
            if entry['test'] and (inventory(inputs[entry['path']][0], env=env) != entry['tests']
                    or inventory(inputs[entry['path']][0], True, env) != entry['ignored_tests']):
                raise ValueError('Formal component actual test inventory mismatch')
    manifest = dict(identity, scope='formal', producer_attempt=current_attempt,
        formal_execution_version=1, components=components, binaries=binaries, libraries=libraries,
        suites=suites, included_binaries=sorted(binaries), selection=None,
        build_timings=[timings[name] for name, _, _ in build_variants('formal')])
    # Partial and corrupt inputs leave no consumable output. Never share mutable
    # Cargo targets between the two producers or hardlink their input artifacts.
    for path, sha in inputs.values():
        if digest(path) != sha:
            raise ValueError('Formal component changed during validation')
    bundle.mkdir(parents=True, exist_ok=False)
    (bundle / 'bin').mkdir()
    (bundle / 'lib').mkdir()
    try:
        for name, (source, sha) in inputs.items():
            shutil.copy2(source, bundle / name)
            if digest(bundle / name) != sha:
                raise ValueError('Formal component changed during copy')
        (bundle / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    except BaseException:
        shutil.rmtree(bundle)
        raise
    print('formal_merge=' + json.dumps(dict(components=components, binaries=len(binaries),
                                           cases=sum(len(entries) for entries in suites.values()))), flush=True)


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
                and ordinary_fast_targets(manifest['execution_plan']) is None
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
    complete_bundle_scope(manifest)
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
    if preview_provider_required(manifest):
        provider_artifact.validate(ROOT, bundle, manifest.get('preview_provider'), manifest)
        files += [(entry['path'], entry['sha256']) for entry in manifest['preview_provider']['files'].values()]
        files += [(entry['path'], entry['sha256']) for entry in manifest['preview_provider']['libraries']]
    else:
        manifest.pop('preview_provider', None)
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
    complete_bundle_scope(manifest)
    if manifest.get('execution_plan') != fast_plan(plan):
        raise ValueError('artifact execution plan mismatch')
    validate_ordinary_selection(manifest)
    if manifest.get('scope') in ('formal', 'storage') and (
            type(manifest.get('schema_version')) is not int or manifest['schema_version'] != 2):
        raise ValueError('expected schema 2 ' + manifest['scope'] + ' artifact')
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
            # Execution capability controls scheduling, never profile identity.
            # Original Formal bundles also used opt0; old opt2 must not be
            # accepted by stripping or changing the execution-version marker.
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
    for variable in provider_artifact.VARIABLES:
        env.pop(variable, None)
    if preview_provider_required(manifest):
        env.update(provider_artifact.validate(ROOT, bundle, manifest.get('preview_provider'), manifest))
    elif 'preview_provider' in manifest:
        raise ValueError('unexpected provider material outside expanded ordinary execution')
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


def native_cpu_budget():
    try:
        cpus = max(1, len(os.sched_getaffinity(0)))
    except (AttributeError, OSError):
        cpus = max(1, os.cpu_count() or 1)
    # Hosted Linux exposes its current container quota here. An unreadable or
    # unlimited quota retains affinity; a fractional CPU still needs one worker.
    try:
        fields = Path('/sys/fs/cgroup/cpu.max').read_text().split()
    except OSError:
        fields = None
    if fields is not None:
        try:
            quota, period = fields
            period = int(period)
            limit = None if quota == 'max' else int(quota)
            if period <= 0 or (limit is not None and limit <= 0):
                raise ValueError()
        except ValueError as error:
            raise ValueError('invalid visible CPU quota') from error
        return cpus if limit is None else min(cpus, max(1, limit // period))
    for directory in ('cpu', 'cpu,cpuacct'):
        root = Path('/sys/fs/cgroup') / directory
        try:
            quota = (root / 'cpu.cfs_quota_us').read_text()
            period = (root / 'cpu.cfs_period_us').read_text()
        except OSError:
            continue
        try:
            quota, period = int(quota), int(period)
            if period <= 0 or quota == 0 or quota < -1:
                raise ValueError()
        except ValueError as error:
            raise ValueError('invalid visible CPU quota') from error
        return cpus if quota == -1 else min(cpus, max(1, quota // period))
    return cpus


def formal_parallel_cases(config):
    """Only trusted exact identities can spend less than the whole CPU budget."""
    policy = config.get('parallel_cases', {})
    if not isinstance(policy, dict):
        raise ValueError('invalid Formal parallel case policy')
    result = {}
    for binary, cases in policy.items():
        if not isinstance(binary, str) or not binary or not isinstance(cases, dict):
            raise ValueError('invalid Formal parallel case owner')
        for name, weight in cases.items():
            if (not isinstance(name, str) or not name or any(character in name for character in '\r\n\0')
                    or type(weight) is not int or weight not in (1, 2)):
                raise ValueError('invalid Formal parallel case weight or identity')
            result[(binary, name)] = weight
    return result


def execute_inline_jobs(jobs, results, *, formal=False, shard=None):
    """Run complete validated jobs, with one receipt and owner per child."""
    from ci_process_runner import run
    cpus = native_cpu_budget()
    workers = min(4 if formal else 2, cpus, len(jobs))
    threads = 1 if formal else max(1, cpus // workers)
    # An existing lower limit remains useful; never multiply libtest's normal
    # CPU-sized pool by the number of concurrently running harnesses.
    for job in jobs:
        configured = job['env'].get('RUST_TEST_THREADS')
        if configured is not None:
            try:
                requested = int(configured)
            except ValueError as error:
                raise ValueError('invalid RUST_TEST_THREADS') from error
            if requested < 1:
                raise ValueError('invalid RUST_TEST_THREADS')
            threads = min(threads, requested)
    stop = threading.Event()
    interrupted = []
    handlers = {}
    failures, recorded = [], set()
    next_job = 0
    weights = [min(cpus, job.get('cpu_weight', cpus)) if formal else threads for job in jobs]
    if any(type(weight) is not int or weight < 1 for weight in weights):
        raise ValueError('invalid execution CPU weight')
    results.parent.mkdir(parents=True, exist_ok=True)
    label = 'formal' if formal else 'ordinary'
    prefix = 'formal_execution=' if formal else 'native_execution='
    print(prefix + json.dumps(dict(mode='bounded-cases' if formal else 'inline', workers=workers,
          cpu_budget=cpus, test_threads=threads, selected=len(jobs), weights=weights)), flush=True)

    def interrupt(signum, frame):
        interrupted.append(signum)
        stop.set()

    def invoke(index, receipt):
        job = jobs[index]
        if stop.is_set():
            raise InterruptedError('cancelled before starting harness')
        env = dict(job['env'], RUST_TEST_THREADS=str(threads))
        result = run(job['command'], job['cwd'], env, job['suite'], job['name'],
                     receipt, expected_count=job['count'], cancellation_event=stop)
        if not result.get('passed'):
            stop.set()
        return result

    def collect(index, future, receipt):
        # A worker return or exit alone cannot stand in for completed execution.
        result = future.result()
        if receipt.stat().st_size > 1024 * 1024:
            raise ValueError('oversized harness result')
        lines = receipt.read_text().splitlines()
        if len(lines) != 1:
            raise ValueError('missing or duplicate harness result')
        record = json.loads(lines[0])
        job = jobs[index]
        if (record != result or record.get('suite') != job['suite']
                or record.get('test_name') != job['name']
                or (job['count'] is not None and type(record.get('expected_count')) is not int)
                or record['expected_count'] != job['count']
                or type(record.get('passed')) is not bool):
            raise ValueError('harness result identity mismatch')
        if record['passed'] and (record.get('exit_code') != 0 or record.get('count_ok') is not True
                or record.get('timeout') is not False or record.get('cancelled') is not False
                or record.get('cleanup_result') != 'clean' or record.get('leaked_processes') is not False
                or record.get('output_read_error') is not None or record.get('log_complete') is False):
            raise ValueError('harness result reports success without clean complete execution')
        if formal:
            record = dict(record, binary=job.get('binary'))
            if shard:
                record['shard'] = shard
        with results.open('a') as stream:
            stream.write(json.dumps(record) + '\n')
        recorded.add(index)
        if not record['passed']:
            failures.append(job['name'])
            stop.set()

    try:
        if threading.current_thread() is threading.main_thread():
            for signum in (signal.SIGTERM, signal.SIGINT):
                handlers[signum] = signal.signal(signum, interrupt)
        with tempfile.TemporaryDirectory(prefix='oasis7-ci-native-results-') as temporary:
            with ThreadPoolExecutor(max_workers=workers) as pool:
                pending = {}
                try:
                    while pending or (next_job < len(jobs) and not stop.is_set()):
                        while next_job < len(jobs) and len(pending) < workers and not stop.is_set():
                            used = sum(weights[index] for index, _ in pending.values())
                            if used + weights[next_job] > cpus:
                                break
                            receipt = Path(temporary) / (str(next_job) + '.jsonl')
                            pending[pool.submit(invoke, next_job, receipt)] = (next_job, receipt)
                            next_job += 1
                        if not pending:
                            break
                        done, _ = wait(pending, timeout=0.1, return_when=FIRST_COMPLETED)
                        for future in done:
                            index, receipt = pending.pop(future)
                            try:
                                collect(index, future, receipt)
                            except BaseException as error:
                                # SystemExit(0), a missing receipt, and a reader
                                # failure must all remain failed executions.
                                failures.append(jobs[index]['name'] + ': ' + type(error).__name__ + ': ' + str(error))
                                stop.set()
                finally:
                    # Unexpected supervisor errors must also stop active
                    # children before the pool waits for their cleanup.
                    stop.set()
    finally:
        for signum, handler in handlers.items():
            signal.signal(signum, handler)
    complete = recorded == set(range(len(jobs)))
    print(prefix + json.dumps(dict(mode='bounded-cases' if formal else 'inline', selected=len(jobs),
          started=next_job, recorded=len(recorded), failures=failures,
          interrupted=interrupted, complete=complete)), flush=True)
    if failures or interrupted or not complete:
        raise RuntimeError(f'{label} failed; see {results}')


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
    formal = manifest.get('scope') == 'formal' and manifest.get('formal_execution_version') == 1
    resources = formal_parallel_cases(formal_configuration()) if formal else {}
    inline = (suite == 'ordinary' and not shard and fast_plan(plan) is not None
              and plan.get('fast_execution_version') == 1)
    if inline:
        # All manifests and actual inventories have passed load() validation.
        # Start complete component libraries before short integration harnesses
        # so the latter do not delay the longest runners. Keep each target and
        # the original relative order within both kinds.
        entries.sort(key=lambda entry: manifest['binaries'][entry['binary']]['target_kind'] != ['lib'])
    jobs = []
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
        job = dict(command=command, cwd=cwd, env=test_env, suite=logical_suite,
                   name=name, count=count, binary=entry.get('binary'))
        if (entry.get('binary'), name) in resources:
            job['cpu_weight'] = resources[(entry['binary'], name)]
        jobs.append(job)
    if formal or inline:
        execute_inline_jobs(jobs, results, formal=formal, shard=shard)
        return
    failed = False
    for job in jobs:
        command, cwd, test_env = job['command'], job['cwd'], job['env']
        logical_suite, name, count = job['suite'], job['name'], job['count']
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
    parser.add_argument('action', choices=['prepare-provider', 'build', 'merge-formal', 'run', 'pack', 'shards', 'required-args', 'cache-build', 'cache-budget', 'cache-stage', 'cache-restore'])
    parser.add_argument('--repo-root', type=Path, default=ROOT)
    parser.add_argument('--bundle', type=Path)
    parser.add_argument('--scope', choices=['all', 'formal', 'ordinary', 'storage', *FORMAL_COMPONENTS], default='all')
    parser.add_argument('--debug-bundle', type=Path)
    parser.add_argument('--endpoint-bundle', type=Path)
    parser.add_argument('--suite', default='ordinary')
    parser.add_argument('--shard', choices=SHARDS)
    parser.add_argument('--split-formal', action='store_true')
    parser.add_argument('--output', type=Path)
    parser.add_argument('--github-output', type=Path)
    parser.add_argument('--max-bytes', type=int, default=8 * 1024 ** 3)
    parser.add_argument('--cache', type=Path)
    parser.add_argument('--cache-state', type=Path)
    parser.add_argument('--build-diagnostics', type=Path, help='new directory for per-Cargo timing and resource reports')
    parser.add_argument('--measure-only', action='store_true')
    parser.add_argument('--results', type=Path)
    parser.add_argument('--plan', type=Path, help='the same trusted selection used by producer and consumers')
    parser.add_argument('--formal-config', type=Path, help='trusted baseline Formal minimum inventory')
    parser.add_argument('--phase', choices=['native', 'native-commands', 'wasm', 'packages', 'baseline-checks', 'clippy-group'], default='native')
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
        if args.phase == 'native-commands':
            print(json.dumps(required_native_commands(plan)))
        else:
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
        parser.error('--bundle is required for build, merge-formal, run and pack')
    if args.action == 'prepare-provider':
        if not args.output:
            parser.error('prepare-provider requires --output for its environment file')
        bundle = args.bundle.resolve()
        bundle.mkdir(parents=True, exist_ok=True)
        record = ProviderArtifact.build(ROOT, bundle, ProviderArtifact.identity(ROOT))
        (bundle / 'preview-provider.json').write_text(json.dumps(record) + '\n')
        environment = ProviderArtifact.validate(ROOT, bundle, record, ProviderArtifact.identity(ROOT))
        args.output.write_text(''.join('export ' + key + '=' + shlex.quote(value) + '\n' for key, value in environment.items()))
        args.output.chmod(0o600)
    elif args.action == 'build':
        build(args.bundle.resolve(), args.scope, args.cache_state, plan, args.build_diagnostics)
    elif args.action == 'merge-formal':
        if not args.debug_bundle or not args.endpoint_bundle:
            parser.error('merge-formal requires --debug-bundle and --endpoint-bundle')
        merge_formal(args.debug_bundle.resolve(), args.endpoint_bundle.resolve(), args.bundle.resolve(), plan)
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
