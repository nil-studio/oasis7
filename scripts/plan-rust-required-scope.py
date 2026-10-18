#!/usr/bin/env python3
"""Select ordinary CI groups from Git data. No network, task state or candidate code execution."""
import argparse
import fnmatch
import json
import posixpath
import re
import subprocess
try:
    import tomllib
except ModuleNotFoundError:
    raise SystemExit("Python 3.11 or newer is required; use scripts/plan-rust-required-scope.sh to discover it")
from pathlib import Path

PLATFORM_GROUPS = ('windows_rollout', 'macos_package_contract', 'fleet_health')
SUPPORT_PROFILES = (
    'oasis7_agent_api', 'oasis7_agent_harness', 'oasis7_client_api', 'oasis7_game_client', 'oasis7_model_provider',
    'oasis7_client_storage', 'oasis7_signer_protocol',
    'oasis7_local_signer', 'oasis7_launcher_ui', 'oasis7_proto',
    'oasis7_wasm_abi', 'oasis7_wasm_build', 'oasis7_wasm_router',
    'oasis7_wasm_sdk', 'oasis7_wasm_store', 'pixel_world_bridge',
    'oasis7_wasm_executor', 'oasis7_secure_envelope', 'oasis7_context',
    'oasis7_client_launcher', 'wasm_build_suite', 'wasm_module_observe',
)
CONTROL = ('Cargo.toml', 'Cargo.lock', '**/Cargo.toml', '**/Cargo.lock', '**/build.rs',
           '.cargo/**', 'rust-toolchain*', '.github/**', 'scripts/ci-*', 'scripts/build-*', '**/world/artifacts/**',
           'scripts/plan-rust-required-scope*', 'scripts/document_corpus.py')


class UnknownInputsError(ValueError):
    """Unregistered paths are a policy gap, not a request for broader tests."""

    def __init__(self, paths):
        super().__init__('unknown CI inputs: ' + json.dumps(sorted(set(paths)))
                         + '; register ownership and validation in the trusted baseline '
                         + 'before introducing these paths')


def git(*args):
    return subprocess.check_output(['git', *args], stderr=subprocess.DEVNULL)


def changed_paths(base, head):
    """Both rename/copy endpoints; Git NUL records preserve whitespace and punctuation."""
    records = git('diff', '--name-status', '--find-renames', '-z', base, head).split(b'\0')
    if records.pop() != b'':
        raise ValueError('incomplete name-status diff')
    result = []
    while records:
        status = records.pop(0).decode('ascii')
        count = 2 if status.startswith(('R', 'C')) else 1
        if len(records) < count:
            raise ValueError('incomplete rename record')
        for _ in range(count):
            result.append(records.pop(0).decode('utf-8', 'surrogateescape'))
    return list(dict.fromkeys(result))


def independent_target_input(path, directory, manifest):
    """These Cargo target roots do not form part of the package library."""
    relative = posixpath.relpath(path, directory or '.')
    roots = ('src/bin/', 'tests/', 'examples/', 'benches/')
    if not relative.startswith(roots):
        return False
    library = manifest.get('lib', {}).get('path', 'src/lib.rs')
    return not library.startswith(roots)


def cargo_graph(ref):
    """Resolve workspace path edges including dev/build/target and renamed dependencies.

    Optional dependencies are included conservatively, so no feature combination is lost.
    Unsupported/ambiguous workspace relationships widen selection rather than reject work.
    """
    records = git('ls-tree', '-r', '-z', ref).split(b'\0')
    entries = []
    tracked_paths = set()
    for record in records:
        if not record:
            continue
        identity, path = record.split(b'\t', 1)
        mode, kind, oid = identity.split()
        tracked_paths.add(path.decode('utf-8', 'surrogateescape'))
        if kind == b'blob' and (path.endswith(b'Cargo.toml') or path.endswith(b'.rs')):
            entries.append((oid, path.decode('utf-8', 'surrogateescape')))
    data = subprocess.check_output(['git', 'cat-file', '--batch'], input=b''.join(oid + b'\n' for oid, _ in entries))
    blobs = {}
    offset = 0
    for _, path in entries:
        end = data.index(b'\n', offset)
        size = int(data[offset:end].split()[2])
        offset = end + 1
        blobs[path] = data[offset:offset + size]
        offset += size + 1
    paths = list(blobs)
    root = tomllib.loads(blobs['Cargo.toml'].decode())
    workspace = root.get('workspace', {})
    members = workspace.get('members', [])
    excludes = workspace.get('exclude', [])
    manifests = {}
    for path in paths:
        if not path.endswith('Cargo.toml'):
            continue
        directory = posixpath.dirname(path)
        if path != 'Cargo.toml' and (not any(fnmatch.fnmatchcase(directory, m.rstrip('/')) for m in members)
                                  or any(fnmatch.fnmatchcase(directory, e.rstrip('/')) for e in excludes)):
            continue
        manifest = tomllib.loads(blobs[path].decode())
        package = manifest.get('package')
        if not package:
            continue
        name = package['name']
        if name in manifests:
            raise ValueError('duplicate package name')
        manifests[name] = (directory, manifest)
    if not manifests:
        raise ValueError('no workspace packages')
    by_directory = {directory: name for name, (directory, _) in manifests.items()}
    reverse = {name: set() for name in manifests}
    workspace_dependencies = workspace.get('dependencies', {})
    for name, (directory, manifest) in manifests.items():
        tables = [manifest, *manifest.get('target', {}).values()]
        for table in tables:
            for kind in ('dependencies', 'dev-dependencies', 'build-dependencies'):
                for alias, dependency in table.get(kind, {}).items():
                    if not isinstance(dependency, dict):
                        continue
                    inherited = dependency.get('workspace') is True
                    if inherited:
                        if alias not in workspace_dependencies:
                            raise ValueError('missing inherited dependency')
                        dependency = workspace_dependencies[alias]
                    if not isinstance(dependency, dict) or 'path' not in dependency:
                        continue
                    target_directory = posixpath.normpath(posixpath.join('' if inherited else directory, dependency['path']))
                    target = by_directory.get(target_directory)
                    if target is None:
                        # External local packages may be shared generators/consumers.
                        raise ValueError('path dependency outside resolved workspace')
                    if dependency.get('package', target) != target:
                        raise ValueError('path dependency identity mismatch')
                    reverse[target].add(name)
    # A library may explicitly import a nominally independent target source.
    # Widen the whole referenced directory, including implicit child modules.
    library_directories = set()
    library_readers = {}
    source_owners = {path: {n for n, (d, _) in manifests.items()
                          if d and path.startswith(d + '/')}
                     for path in paths if path.endswith('.rs')}
    while True:
        before = (set(library_directories), {d: frozenset(readers) for d, readers in library_readers.items()})
        for path, owners in source_owners.items():
            source = blobs[path].decode('utf-8', 'replace')
            owners = owners | {reader for d, readers in library_readers.items()
                               if (not d or path.startswith(d + '/')) for reader in readers}
            library_owners = {n for n in owners
                              if not independent_target_input(path, *manifests[n])
                              or any(not d or path.startswith(d + '/') for d in library_directories)}
            if not library_owners:
                continue
            for token in re.findall(r'"([^"\n]+)"', source):
                for owner in library_owners:
                    directory = manifests[owner][0]
                    for root in ('', directory, posixpath.dirname(path)):
                        candidate = posixpath.normpath(posixpath.join(root, token.lstrip('/')))
                        if candidate in source_owners:
                            parent = posixpath.dirname(candidate)
                            library_directories.add(parent)
                            library_readers.setdefault(parent, set()).update(library_owners)
            if re.search(r'\bmod\s+bin\s*[;{]', source):
                for owner in library_owners:
                    library_directories.add(manifests[owner][0] + '/src/bin')
            # A computed include/path can reach any target source. Keep all edges.
            includes = len(re.findall(r'include(?:_str|_bytes)?!\s*\(', source))
            literal_includes = len(re.findall(r'include(?:_str|_bytes)?!\s*\(\s*"[^"\n]+"\s*,?\s*\)', source))
            manifest_includes = len(re.findall(r'include(?:_str|_bytes)?!\s*\(\s*concat!\s*\(\s*env!\s*\(\s*"CARGO_MANIFEST_DIR"\s*\)\s*,\s*"[^"\n]+"\s*,?\s*\)\s*,?\s*\)', source))
            if (includes > literal_includes + manifest_includes
                    or re.search(r'#\[(?:path|cfg_attr\([^;\n]*,\s*path)\s*=\s*(?![\s"])[A-Za-z_]', source)):
                for owner in library_owners:
                    library_directories.add(manifests[owner][0])
        if (library_directories, {d: frozenset(readers) for d, readers in library_readers.items()}) == before:
            break
    leaf_inputs = {path for path, owners in source_owners.items() if owners
                   and all(independent_target_input(path, *manifests[n]) for n in owners)
                   and not any(not d or path.startswith(d + '/') for d in library_directories)}
    # Embedded inputs retain whether their reader is a library or a leaf target.
    embedded = {}
    library_embeds = {}

    for path in paths:
        owners = [n for n, (d, _) in manifests.items()
                  if (d and path.startswith(d + '/')) or (not d and path.startswith(('src/', 'tests/', 'examples/', 'benches/')))]
        owners = sorted(set(owners) | {reader for directory, readers in library_readers.items()
                                       if (not directory or path.startswith(directory + '/')) for reader in readers})
        if not path.endswith('.rs') or not owners:
            continue
        source = blobs[path].decode('utf-8', 'replace')
        library_owners = set(owners) if path not in leaf_inputs else set()
        literal = list(re.finditer(r'include_(?:str|bytes)!\s*\(\s*"([^"\n]+)"\s*,?\s*\)', source))
        concatenated = list(re.finditer(r'include_(?:str|bytes)!\s*\(\s*concat!\s*\(\s*env!\s*\(\s*"CARGO_MANIFEST_DIR"\s*\)\s*,\s*"([^"\n]+)"\s*,?\s*\)\s*,?\s*\)', source))
        for match in literal:
            candidate = posixpath.normpath(posixpath.join(posixpath.dirname(path), match.group(1)))
            embedded.setdefault(candidate, set()).update(owners)
            library_embeds.setdefault(candidate, set()).update(library_owners)
        for match in concatenated:
            for owner in owners:
                directory = manifests[owner][0]
                candidate = posixpath.normpath(directory + '/' + match.group(1).lstrip('/'))
                embedded.setdefault(candidate, set()).add(owner)
                if owner in library_owners:
                    library_embeds.setdefault(candidate, set()).add(owner)
        # Tests often read fixtures through manifest-relative or repo-relative literals.
        for token in re.findall(r'"([^"\n]+)"', source):
            if '/' not in token or '\\' in token or len(token) > 512:
                continue
            for owner in owners:
                directory = manifests[owner][0]
                for root in ('', directory, posixpath.dirname(path)):
                    candidate = posixpath.normpath(posixpath.join(root, token.lstrip('/')))
                    if candidate in tracked_paths:
                        embedded.setdefault(candidate, set()).add(owner)
                        if owner in library_owners:
                            library_embeds.setdefault(candidate, set()).add(owner)
        if len(re.findall(r'include_(?:str|bytes)!', source)) > len(literal) + len(concatenated):
            embedded.setdefault('__unresolved_embed__', set()).update(owners)
    embedded['__leaf_inputs__'] = leaf_inputs
    embedded['__library_directories__'] = library_directories
    embedded['__library_readers__'] = library_readers
    embedded['__library_embeds__'] = library_embeds
    return manifests, reverse, embedded


def reverse_consumers(changed, reverse):
    result = set(changed)
    pending = list(changed)
    while pending:
        for consumer in reverse[pending.pop()]:
            if consumer not in result:
                result.add(consumer)
                pending.append(consumer)
    return result


def validate_config(config):
    groups = config['groups']
    if (not isinstance(groups, list) or len(groups) != len(set(groups))
            or not {'baseline', 'rust_baseline', *PLATFORM_GROUPS}.issubset(groups)):
        raise ValueError('invalid group inventory')
    if set(config['resources']) != set(groups):
        raise ValueError('missing resource mapping')
    for selected in config['package_groups'].values():
        if not selected or not set(selected).issubset(groups):
            raise ValueError('invalid package groups')
    for rule in config['rules']:
        if not rule.get('match') or not set(rule.get('groups', [])).issubset(groups):
            raise ValueError('invalid directory mapping')
    return config


def select(config, base, source_head, test_head=None, additions=()):
    validate_config(config)
    groups = set(config['groups'])
    chosen = {'baseline'}
    reasons = ['baseline always runs']
    paths = []
    affected = set()
    unknown_paths = []
    full = False
    support_rule = False
    try:
        # Push/source-only checks retain the event range. For PR merge checks,
        # exclude target history already present in the actual first parent.
        graph_refs = [base, source_head, test_head or source_head]
        paths = changed_paths(base, source_head)
        if test_head and test_head != source_head:
            source_oid = git('rev-parse', source_head + '^{commit}').decode().strip()
            base_oid = git('rev-parse', base + '^{commit}').decode().strip()
            parents = git('show', '-s', '--format=%P', test_head).decode().split()
            if len(parents) != 2 or parents[1] != source_oid:
                raise ValueError('tested commit is not a target/source merge')
            target = parents[0]
            if git('merge-base', base_oid, target).decode().strip() != base_oid:
                raise ValueError('event base is not an ancestor of tested target')
            forks = git('merge-base', '--all', target, source_oid).decode().split()
            if len(forks) != 1:
                raise ValueError('ambiguous source/target merge base')
            source_base = forks[0]
            paths = list(dict.fromkeys(changed_paths(source_base, source_oid)
                                       + changed_paths(target, test_head)))
            graph_refs.extend([source_base, target])
        graphs = [cargo_graph(ref) for ref in dict.fromkeys(graph_refs)]
        for path in paths:
            controlled = any(fnmatch.fnmatchcase(path, pattern) for pattern in CONTROL)
            if controlled:
                full = True
                reasons.append('shared build or CI control changed: ' + path)
            matched = controlled
            for manifests, reverse, embedded in graphs:
                owners = {name for name, (directory, _) in manifests.items()
                          if directory and path.startswith(directory + '/')}
                owners.update(embedded.get(path, ()))
                registered_owners = set(owners)
                imported_readers = {reader for directory, readers in embedded.get('__library_readers__', {}).items()
                                    if (not directory or path.startswith(directory + '/')) for reader in readers}
                owners.update(imported_readers)
                if owners:
                    # Inferred directory readers widen tests; they do not register new inputs.
                    matched |= bool(registered_owners)
                    leaf = all(independent_target_input(path, *manifests[n]) for n in owners) and not any(
                        (not d or path.startswith(d + '/')) for d in embedded.get('__library_directories__', ()))
                    # An external fixture has no direct source owner; its readers
                    # determine whether reverse consumers are affected.
                    direct = {name for name, (directory, _) in manifests.items()
                              if directory and path.startswith(directory + '/')}
                    library_owners = (set() if leaf else direct) | imported_readers | embedded.get('__library_embeds__', {}).get(path, set())
                    affected.update(owners)
                    affected.update(reverse_consumers(library_owners, reverse))
            for rule in config['rules']:
                if any(fnmatch.fnmatchcase(path, pattern) for pattern in rule['match']):
                    matched = True
                    chosen.update(rule.get('groups', []))
                    support_rule |= 'workspace_support' in rule.get('groups', [])
                    full |= rule.get('full', False)
                    reasons.append(rule['reason'] + ': ' + path)
            if any(embedded.get('__unresolved_embed__') for _, _, embedded in graphs) and not any(
                    directory and path.startswith(directory + '/') for manifests, _, _ in graphs
                    for directory, _ in manifests.values()):
                full = True
                reasons.append('unresolved embedded consumer input: ' + path)
            if not matched:
                unknown_paths.append(path)
        for package in sorted(affected):
            mapped = config['package_groups'].get(package)
            if not mapped:
                full = True
                reasons.append('unmapped consumer: ' + package)
            else:
                chosen.update(mapped)
        if affected:
            chosen.add('rust_baseline')
            reasons.append('changed packages and reverse consumers: ' + ', '.join(sorted(affected)))
    except (OSError, subprocess.CalledProcessError, ValueError, KeyError, TypeError) as error:
        full = True
        reasons.append('uncertain Git/dependency inputs: ' + str(error))
    if unknown_paths:
        raise UnknownInputsError(unknown_paths)
    if full:
        chosen = set(groups)
        reasons.append('ordinary required fallback')
    if 'operational_contracts' in chosen:
        chosen.update(('windows_rollout', 'fleet_health'))
    if 'packaging_contracts' in chosen:
        chosen.add('macos_package_contract')
    if not set(additions).issubset(groups):
        raise ValueError('additional request names unknown group')
    chosen.update(additions)
    if 'operational_contracts' in chosen:
        chosen.update(('windows_rollout', 'fleet_health'))
    if 'packaging_contracts' in chosen:
        chosen.add('macos_package_contract')
    resources = sorted({resource for group in chosen for resource in config['resources'][group]})
    support_profiles = []
    if 'workspace_support' in chosen:
        support_profiles = sorted(set(affected).intersection(SUPPORT_PROFILES))
        # Non-package rules and explicit group requests retain the complete group.
        # Control/uncertain inputs must never narrow the old command inventory.
        unmapped_support = any(
            'workspace_support' in config['package_groups'].get(package, [])
            and package not in SUPPORT_PROFILES for package in affected)
        if full or support_rule or unmapped_support or not support_profiles or 'workspace_support' in additions:
            support_profiles = sorted(SUPPORT_PROFILES)
    return {'groups': sorted(chosen), 'resources': resources,
            'workspace_support_profiles': support_profiles,
            'workspace_support_profile_version': 1,
            'reasons': list(dict.fromkeys(reasons)), 'scope': 'full' if full else 'targeted'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--base-ref', required=True)
    parser.add_argument('--head-ref', required=True, help='source HEAD')
    parser.add_argument('--test-ref', help='actual tested merge/tree commit')
    parser.add_argument('--config', default=str(Path(__file__).with_name('ci-required-scope.json')))
    parser.add_argument('--add-group', action='append', default=[])
    parser.add_argument('--github-output')
    parser.add_argument('--output')
    args = parser.parse_args()
    config = json.loads(Path(args.config).read_text())
    try:
        plan = select(config, args.base_ref, args.head_ref, args.test_ref, args.add_group)
    except UnknownInputsError as error:
        parser.exit(2, f"CI scope planning failed: {error}\n")
    encoded = json.dumps(plan, ensure_ascii=True, separators=(',', ':'))
    if args.output:
        Path(args.output).write_text(encoded + '\n')
    if args.github_output:
        with Path(args.github_output).open('a') as output:
            output.write('plan=' + encoded + '\n')
            for group in config['groups']:
                output.write(f'run_{group}=' + str(group in plan['groups']).lower() + '\n')
    print(encoded)


if __name__ == '__main__':
    main()
