#!/usr/bin/env python3
"""Select ordinary CI groups from Git data. No network, task state or candidate code execution."""
import argparse
import fnmatch
import hashlib
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
EXECUTION_PROFILES = ('pr-fast', 'main-fast', 'periodic-full', 'candidate-full')
FAST_PLAN_VERSION = 1
FAST_TEST_INVENTORY_VERSION = 1
CHECK_INVENTORY_VERSION = 1
FORMAL_SUITES = ('runtime-protocol', 'world-history', 'healthy-set', 'storage-recovery', 'lifecycle-cli')
SUPPORT_PROFILES = (
    'oasis7_agent_api', 'oasis7_agent_harness', 'oasis7_browser_game_client', 'oasis7_client_api', 'oasis7_game_client', 'oasis7_model_provider',
    'oasis7_client_storage', 'oasis7_signer_protocol',
    'oasis7_custody_core', 'oasis7_custody_worker',
    'oasis7_local_signer', 'oasis7_launcher_ui', 'oasis7_proto',
    'oasis7_wasm_abi', 'oasis7_wasm_build', 'oasis7_wasm_router',
    'oasis7_wasm_sdk', 'oasis7_wasm_store', 'pixel_world_bridge',
    'oasis7_wasm_executor', 'oasis7_secure_envelope', 'oasis7_context',
    'oasis7_client_launcher', 'wasm_build_suite', 'wasm_module_observe',
)
# The distfs group runs these lib harnesses with Cargo defaults on Ubuntu.
# This proves native target predicates only, never expanded feature activation.
# Keep synchronized with run_oasis7_distfs_tests and rust.yml's distfs runner.
NATIVE_DEFAULT_TEST_PACKAGES = frozenset((
    'oasis7_cas', 'oasis7_formal_protocol', 'oasis7_distfs',
))
CONTROL = ('Cargo.toml', 'Cargo.lock', '**/Cargo.toml', '**/Cargo.lock', '**/build.rs',
           '.cargo/**', 'rust-toolchain*', '.github/**', 'scripts/ci-*', 'scripts/build-*', '**/world/artifacts/**',
           'scripts/plan-rust-required-scope*', 'scripts/document_corpus.py')


class UnknownInputsError(ValueError):
    """Unregistered paths are a policy gap, not a request for broader tests."""

    def __init__(self, paths):
        super().__init__('unknown CI inputs: ' + json.dumps(sorted(set(paths)))
                         + '; register ownership and validation in the trusted baseline '
                         + 'before introducing these paths')


class TestPolicyError(ValueError):
    """An ignored test needs a trusted real execution path, not just a new tag."""


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


def known_test_target_root(path, directory, manifest):
    """Only standalone Cargo roots can prove that no external mod cfg applies."""
    relative = posixpath.relpath(path, directory or '.')
    package, library = manifest.get('package', {}), manifest.get('lib', {})
    if (('lib' in manifest or package.get('autolib', True)) and library.get('test', True)
            and library.get('harness', True)
            and relative == posixpath.normpath(library.get('path', 'src/lib.rs'))):
        return True
    for kind in ('bin', 'test'):
        targets = manifest.get(kind, [])
        if any(target.get('path') and relative == posixpath.normpath(target['path'])
               and target.get('test', True) and target.get('harness', True)
               and not target.get('required-features') for target in targets):
            return True
        # Explicit declarations can override automatic target test settings.
        if targets or not package.get('auto' + kind + 's', True):
            continue
        pattern = (r'src/(?:main\.rs|bin/[^/.][^/]*(?:\.rs|/main\.rs))'
                   if kind == 'bin' else r'tests/[^/.][^/]*(?:\.rs|/main\.rs)')
        if re.fullmatch(pattern, relative):
            return True
    return False


def rust_literal_path(value):
    """Decode exact unescaped/raw path literals; escaped paths stay uncertain."""
    plain = re.fullmatch(r'"([^"\\]+)"', value)
    raw = re.fullmatch(r'r(#+)"(.*)"\1', value)
    if value.startswith('r"') and value.endswith('"'):
        return value[2:-1]
    return plain.group(1) if plain else raw.group(2) if raw else None


def rust_source_inputs(source, return_tokens=False, with_module_paths=False):
    """Read literal tokens and real include calls without expanding candidate macros.

    Rust fixture strings are a single token, not another source file. Unknown
    real expressions stay unresolved; comments (including nested blocks), strings
    and character literals must never hide a later real call.
    """
    tokens = []
    position = 0
    raw_string = re.compile(r'(?:br|cr|r)(#*)"')
    identifier = re.compile(r'(?:r#)?(?:[^\W\d]|_)\w*')
    literal_suffix = re.compile(r'(?:[^\W\d]|_)\w*')
    number_literal = re.compile(r'[0-9][0-9_]*(?:\.[0-9_]+)?(?:[eE][+-]?[0-9_]+)?')
    character = re.compile(r"(?:b)?'(?:[^'\\\r\n]|\\(?:[nrt0\\'\"]|x[0-9a-fA-F]{2}|u\{[0-9a-fA-F_]+\}))'")

    def append_literal(kind, start, end):
        # Literal suffixes are part of the token, even in arbitrary macro input.
        # For example, discard!("example"include_str!(...)) contains no include.
        suffix = literal_suffix.match(source, end)
        if suffix:
            end = suffix.end()
            while end < len(source) and ('_' + source[end]).isidentifier():
                end += 1
            kind = 'suffixed_literal'
        tokens.append((kind, source[start:end], start))
        return end

    while position < len(source):
        start = position
        if source[position].isspace():
            position += 1
            continue
        if source.startswith('//', position):
            end = source.find('\n', position + 2)
            position = len(source) if end < 0 else end
            continue
        if source.startswith('/*', position):
            depth = 1
            position += 2
            while depth and position < len(source):
                if source.startswith('/*', position):
                    depth += 1
                    position += 2
                elif source.startswith('*/', position):
                    depth -= 1
                    position += 2
                else:
                    position += 1
            if depth:
                raise ValueError('unterminated Rust block comment')
            continue
        raw = raw_string.match(source, position)
        if raw:
            closing = '"' + raw.group(1)
            end = source.find(closing, raw.end())
            if end < 0:
                raise ValueError('unterminated Rust raw string')
            position = append_literal('string', start, end + len(closing))
            continue
        quote = position + int(source.startswith(('b"', 'c"'), position))
        if source[quote] == '"':
            position = quote + 1
            while position < len(source) and source[position] != '"':
                position += 2 if source[position] == '\\' else 1
            if position >= len(source):
                raise ValueError('unterminated Rust string')
            position = append_literal('string', start, position + 1)
            continue
        char = character.match(source, position)
        if char:
            position = append_literal('character', start, char.end())
            continue
        if source[position].isascii() and source[position].isdigit():
            number = number_literal.match(source, position)
            position = append_literal('number', start, number.end())
            continue
        name = identifier.match(source, position)
        if name:
            position = name.end()
            # Python's \w omits combining marks accepted in Rust identifiers.
            # Do not split a longer identifier into a false include macro.
            while position < len(source) and ('_' + source[position]).isidentifier():
                position += 1
            tokens.append(('identifier', source[start:position], start))
            continue
        # A lifetime's apostrophe is punctuation, not a greedy quoted string.
        tokens.append(('punctuation', source[position], position))
        position += 1

    closing = {}
    stack = []
    pairs = {')': '(', ']': '[', '}': '{'}
    for index, (kind, value, _) in enumerate(tokens):
        if kind != 'punctuation':
            continue
        if value in ('(', '[', '{'):
            stack.append(index)
        elif value in pairs:
            if not stack or tokens[stack[-1]][1] != pairs[value]:
                raise ValueError('unbalanced Rust token delimiters')
            closing[stack.pop()] = index
    if stack:
        raise ValueError('unclosed Rust token delimiter')
    if return_tokens:
        return tokens, closing

    # Decode only forms whose Rust path value is exact. Escapes and other
    # expressions remain conservative instead of using Python/JSON semantics.
    plain_literal = re.compile(r'"([^"\\\r\n]*)"')
    literals = []
    calls = []
    for index, (kind, value, start) in enumerate(tokens):
        if kind == 'string':
            literal = re.fullmatch(r'(?:b|c)?"([^"\\\r\n]*)"', value)
            raw = raw_string.match(value)
            if literal:
                literals.append(literal.group(1))
            elif raw:
                literals.append(value[raw.end():-(len(raw.group(1)) + 1)])
        name = value.removeprefix('r#')
        if (kind != 'identifier' or name not in ('include', 'include_str', 'include_bytes')
                or index + 1 >= len(tokens) or tokens[index + 1][1] != '!'):
            continue
        end = closing.get(index + 2)
        call = {'macro': name, 'kind': 'unknown', 'path': None,
                'line': source.count('\n', 0, start) + 1}
        call['expression'] = source[start:tokens[end][2] + 1] if end is not None else value + '!'
        # Keep canonical std/core builtins; a foreign macro with the same name
        # has no builtin/OUT_DIR authority. A field's single colon is not a path.
        cursor = index - 1
        namespace = []
        while cursor > 0 and tokens[cursor - 1][1] == tokens[cursor][1] == ':':
            cursor -= 2
            if cursor < 0 or tokens[cursor][0] != 'identifier':
                break
            namespace.append(tokens[cursor][1].removeprefix('r#'))
            cursor -= 1
        qualified = ((index > 1 and tokens[index - 2][1] == tokens[index - 1][1] == ':'
                      and namespace not in (['std'], ['core']))
                     or (cursor >= 0 and tokens[cursor][1] == '$'))
        if end is not None and not qualified:
            arguments = tokens[index + 3:end]
            if arguments and arguments[-1][1] == ',':
                arguments = arguments[:-1]
            expression = ''.join(token[1] for token in arguments)
            literal = plain_literal.fullmatch(expression) if len(arguments) == 1 else None
            concatenated = re.fullmatch(
                r'concat!\(env!\("(CARGO_MANIFEST_DIR|OUT_DIR)",?\),"([^"\\\r\n]+)",?\)',
                expression)
            if literal:
                call.update(kind='literal', path=literal.group(1))
            elif concatenated:
                call.update(kind='manifest' if concatenated.group(1) == 'CARGO_MANIFEST_DIR' else 'generated',
                            path=concatenated.group(2))
        calls.append(call)
    if with_module_paths:
        uncertain_module_paths = False
        for index, (_, token, _) in enumerate(tokens[:-1]):
            if token != '#' or index + 1 not in closing or tokens[index + 1][1] != '[':
                continue
            values = tuple(value.removeprefix('r#') if kind == 'identifier' else value
                           for kind, value, _ in tokens[index + 2:closing[index + 1]])
            # Inline #[path] changes the directory of descendant modules. Keep
            # its unresolved edges even when every literal is individually exact.
            cursor = closing[index + 1] + 1
            while cursor < len(tokens) and tokens[cursor][1] not in ('mod', '{', '}', ';'):
                if tokens[cursor][1] == '#' and cursor + 1 in closing:
                    cursor = closing[cursor + 1] + 1
                else:
                    cursor += 1
            inline_path = (values and values[0] == 'path' and cursor + 2 < len(tokens)
                           and tokens[cursor][1] == 'mod' and tokens[cursor + 2][1] == '{')
            if values and (values[0] == 'path' and (len(values) != 3 or values[1] != '='
                                                  or rust_literal_path(values[2]) is None)
                           or values[0] == 'cfg_attr' and 'path' in values or inline_path):
                uncertain_module_paths = True
        return literals, calls, uncertain_module_paths
    return literals, calls


def cargo_graph(ref, generated_inputs=()):
    """Resolve workspace path edges including dev/build/target and renamed dependencies.

    Optional dependencies are included conservatively, so no feature combination is lost.
    Unsupported/ambiguous workspace relationships widen selection rather than reject work.
    """
    records = git('ls-tree', '-r', '-z', ref).split(b'\0')
    entries = []
    blob_modes = {}
    tracked_paths = set()
    for record in records:
        if not record:
            continue
        identity, path = record.split(b'\t', 1)
        mode, kind, oid = identity.split()
        tracked_paths.add(path.decode('utf-8', 'surrogateescape'))
        if kind == b'blob' and (path.endswith(b'Cargo.toml') or path.endswith(b'.rs')):
            entries.append((oid, path.decode('utf-8', 'surrogateescape')))
            blob_modes[path.decode('utf-8', 'surrogateescape')] = mode
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
    # Trust only reviewed, self-contained producers whose repository inputs are
    # covered by CONTROL. Check the actual build entry and bytes at every ref;
    # a stale file left behind after switching generators is not authoritative.
    generated_outputs = {}
    for entry in generated_inputs:
        if entry['package'] not in manifests:
            continue
        directory, manifest = manifests[entry['package']]
        build = manifest['package'].get('build', 'build.rs')
        if not isinstance(build, str) or not build:
            continue
        producer = posixpath.normpath(posixpath.join(directory, build))
        if (producer != entry['producer']
                or (directory and not producer.startswith(directory + '/'))
                or not any(fnmatch.fnmatchcase(producer, pattern) for pattern in CONTROL)):
            continue
        contents = blobs.get(producer)
        if (contents is None or blob_modes.get(producer) not in (b'100644', b'100755')
                or hashlib.sha256(contents).hexdigest() != entry['producer_sha256']):
            continue
        generated_outputs[entry['package']] = set(entry['outputs'])
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
    source_inputs = {}

    def inputs(path):
        if path not in source_inputs:
            try:
                source_inputs[path] = rust_source_inputs(blobs[path].decode('utf-8', 'replace'), with_module_paths=True)
            except ValueError as error:
                raise ValueError(f'{path}: {error}') from error
        return source_inputs[path]

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
            literals, includes = inputs(path)[:2]
            for token in literals:
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
            if (any(call['kind'] not in ('literal', 'manifest') for call in includes)
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
    included_sources, unresolved_source_owners, uncertain_includes = set(), set(), set()

    for path in paths:
        owners = [n for n, (d, _) in manifests.items()
                  if (d and path.startswith(d + '/')) or (not d and path.startswith(('src/', 'tests/', 'examples/', 'benches/')))]
        owners = sorted(set(owners) | {reader for directory, readers in library_readers.items()
                                       if (not directory or path.startswith(directory + '/')) for reader in readers})
        if not path.endswith('.rs') or not owners:
            continue
        literals, includes = inputs(path)[:2]
        library_owners = set(owners) if path not in leaf_inputs else set()
        for call in includes:
            if call['macro'] == 'include':
                if call['kind'] == 'literal':
                    included_sources.add(posixpath.normpath(posixpath.join(posixpath.dirname(path), call['path'])))
                else:
                    unresolved_source_owners.update(owners)
                    if call['kind'] == 'unknown':
                        uncertain_includes.add(path)
                continue
            if call['kind'] == 'literal':
                candidate = posixpath.normpath(posixpath.join(posixpath.dirname(path), call['path']))
                embedded.setdefault(candidate, set()).update(owners)
                library_embeds.setdefault(candidate, set()).update(library_owners)
            elif call['kind'] == 'manifest':
                for owner in owners:
                    directory = manifests[owner][0]
                    candidate = posixpath.normpath(directory + '/' + call['path'].lstrip('/'))
                    embedded.setdefault(candidate, set()).add(owner)
                    if owner in library_owners:
                        library_embeds.setdefault(candidate, set()).add(owner)
            else:
                for owner in owners:
                    # Each reader has its own OUT_DIR; one registration cannot
                    # exempt another owner or cancel a different unknown call.
                    known = (call['kind'] == 'generated'
                             and call['path'].startswith('/')
                             and call['path'][1:] in generated_outputs.get(owner, set()))
                    if not known:
                        embedded.setdefault('__unresolved_embed__', set()).add(owner)
                        expression = ' '.join(call['expression'].split())[:160]
                        embedded.setdefault('__unresolved_embed_sources__', set()).add(
                            f"{path}:{call['line']} ({owner}): {expression}")
        # Tests often read fixtures through manifest-relative or repo-relative literals.
        for token in literals:
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
    embedded['__leaf_inputs__'] = leaf_inputs
    embedded['__library_directories__'] = library_directories
    embedded['__library_readers__'] = library_readers
    embedded['__library_embeds__'] = library_embeds
    # Reuse the exact Git blobs for bounded module-context proofs in fast mode.
    embedded['__rust_sources__'] = {path: content for path, content in blobs.items() if path.endswith('.rs')}
    embedded['__rust_include_targets__'] = included_sources
    embedded['__unresolved_rust_include_owners__'] = unresolved_source_owners
    embedded['__uncertain_module_sources__'] = {path for path, details in source_inputs.items() if details[2]} | uncertain_includes
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
    scoped = config.get('scoped_control_paths', {})
    if not isinstance(scoped, dict):
        raise ValueError('invalid scoped control inventory')
    for path, selected in scoped.items():
        if (not isinstance(path, str) or not path.startswith('.github/')
                or path.endswith('/') or posixpath.normpath(path) != path
                or any(character in path for character in ('\\', '*', '?', '[', ']'))
                or '..' in path.split('/') or any(ord(character) < 32 for character in path)
                or not isinstance(selected, list)
                or any(not isinstance(group, str) or group not in groups for group in selected)
                or len(selected) != len(set(selected))):
            raise ValueError('invalid scoped control path or groups')
    for selected in config['package_groups'].values():
        if not selected or not set(selected).issubset(groups):
            raise ValueError('invalid package groups')
    for rule in config['rules']:
        if not rule.get('match') or not set(rule.get('groups', [])).issubset(groups):
            raise ValueError('invalid directory mapping')
        excluded = rule.get('exclude', [])
        if not isinstance(excluded, list) or any(not isinstance(pattern, str) or not pattern for pattern in excluded):
            raise ValueError('invalid directory exclusions')
    profile_paths = config.get('workspace_support_profile_paths', {})
    if not isinstance(profile_paths, dict) or (profile_paths and 'workspace_support' not in groups):
        raise ValueError('invalid workspace support profile paths')
    for profile, patterns in profile_paths.items():
        if profile not in SUPPORT_PROFILES or not isinstance(patterns, list) or not patterns:
            raise ValueError('invalid workspace support profile mapping')
        if any(
                not isinstance(pattern, str) or not pattern
                or posixpath.isabs(pattern) or posixpath.normpath(pattern) != pattern
                or '\\' in pattern or '..' in pattern.split('/')
                for pattern in patterns):
            raise ValueError('invalid workspace support profile path')
        if len(patterns) != len(set(patterns)):
            raise ValueError('duplicate workspace support profile path')
    generated = config.get('generated_inputs', [])
    if not isinstance(generated, list):
        raise ValueError('invalid generated input inventory')
    packages = set()
    for entry in generated:
        if (not isinstance(entry, dict)
                or set(entry) != {'package', 'producer', 'producer_sha256', 'outputs'}):
            raise ValueError('invalid generated input registration')
        package, producer = entry['package'], entry['producer']
        digest, outputs = entry['producer_sha256'], entry['outputs']
        if (not isinstance(package, str) or not package or package in packages
                or not isinstance(producer, str) or not producer
                or posixpath.isabs(producer) or posixpath.normpath(producer) != producer
                or '\\' in producer or '..' in producer.split('/')
                or not isinstance(digest, str) or not re.fullmatch(r'[0-9a-f]{64}', digest)
                or not isinstance(outputs, list) or not outputs
                or any(not isinstance(name, str) or name in ('.', '..')
                       or not re.fullmatch(r'[A-Za-z0-9_.-]+', name) for name in outputs)
                or len(outputs) != len(set(outputs))):
            raise ValueError('invalid generated input identity')
        packages.add(package)
    if 'fast_scope_version' in config:
        if config['fast_scope_version'] != FAST_PLAN_VERSION:
            raise ValueError('unknown fast scope version')
        for key in ('oasis7_required_packages', 'oasis7_required_wasm_packages'):
            inventory = config.get(key)
            if (not isinstance(inventory, list) or not inventory
                    or any(not isinstance(name, str) or not re.fullmatch(r'[A-Za-z0-9_-]+', name)
                           for name in inventory)
                    or len(inventory) != len(set(inventory))):
                raise ValueError('invalid ' + key)
        risk_rules = config.get('formal_risk_rules')
        if not isinstance(risk_rules, list):
            raise ValueError('missing formal risk rules')
        for rule in risk_rules:
            if (not isinstance(rule, dict) or not isinstance(rule.get('match'), list)
                    or not rule['match'] or not isinstance(rule.get('suites'), list)
                    or not set(rule['suites']).issubset(FORMAL_SUITES)
                    or len(rule['suites']) != len(set(rule['suites']))
                    or not isinstance(rule.get('reason'), str) or not rule['reason']):
                raise ValueError('invalid formal risk rule')
            for pattern in rule['match']:
                if (not isinstance(pattern, str) or not pattern or posixpath.isabs(pattern)
                        or '..' in pattern.split('/') or '\\' in pattern):
                    raise ValueError('invalid formal risk path')
        fast_paths = config.get('command_center_fast_paths', [])
        if (not isinstance(fast_paths, list) or any(
                not isinstance(pattern, str) or not pattern.startswith('tools/oasis7-command-center/')
                or '..' in pattern.split('/') or '\\' in pattern for pattern in fast_paths)):
            raise ValueError('invalid command center fast paths')
    validate_fast_test_inventory(config)
    return config


def valid_fast_declaration(item):
    if not isinstance(item, str):
        return False
    item = item.strip()
    native = '#[cfg(not(target_arch = "wasm32"))]'
    if item.startswith(native):
        item = item[len(native):].strip()
    identifier = r'[A-Za-z_][A-Za-z0-9_]*'
    module = r'(?:pub(?:\(super\))?\s+)?mod\s+' + identifier + r'\s*;'
    names = identifier + r'(?:\s*,\s*' + identifier + r')*\s*,?'
    export = (r'pub\s+use\s+(?:' + identifier + r'::)+'
              r'(?:' + identifier + r'|\{\s*' + names + r'\s*\})\s*;')
    return re.fullmatch(module + '|' + export, item) is not None


def validate_fast_test_inventory(config):
    """Only an explicit trusted inventory may replace package-wide tests."""
    fields = {'fast_test_inventory_version', 'ordinary_fast_targets', 'ordinary_fast_rules'}
    if not fields.intersection(config):
        return
    if (not fields.issubset(config)
            or type(config['fast_test_inventory_version']) is not int
            or config['fast_test_inventory_version'] != FAST_TEST_INVENTORY_VERSION
            or config.get('fast_scope_version') != FAST_PLAN_VERSION):
        raise ValueError('invalid fast test inventory version')
    targets = config['ordinary_fast_targets']
    rules = config['ordinary_fast_rules']
    if not isinstance(targets, dict) or not targets or not isinstance(rules, list) or not rules:
        raise ValueError('invalid fast test targets or rules')
    identities = set()
    for key, target in targets.items():
        if (not isinstance(key, str) or not re.fullmatch(r'[a-z0-9_-]+', key)
                or not isinstance(target, dict)
                or set(target) != {'package', 'target_kind', 'target_name', 'default_features', 'features', 'cases'}):
            raise ValueError('invalid fast test target')
        if (target['package'] not in config['oasis7_required_packages']
                or target['target_kind'] not in ('lib', 'test')
                or not isinstance(target['target_name'], str)
                or not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_-]*', target['target_name'])
                or type(target['default_features']) is not bool):
            raise ValueError('invalid fast test target identity')
        identity = (target['package'], target['target_kind'], target['target_name'])
        if identity in identities:
            raise ValueError('duplicate fast test target identity')
        identities.add(identity)
        for field in ('features', 'cases'):
            values = target[field]
            if (not isinstance(values, list) or (field == 'cases' and not values)
                    or any(not isinstance(value, str) or not value or value.startswith('-')
                           or any(character.isspace() for character in value) for value in values)
                    or len(values) != len(set(values))):
                raise ValueError('invalid fast test ' + field)
        if (any(not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_-]*', feature) for feature in target['features'])
                or not target['default_features'] and 'default' in target['features']):
            raise ValueError('invalid fast test features')
    seen_paths = set()
    for rule in rules:
        if (not isinstance(rule, dict) or set(rule) - {'paths', 'targets', 'reason', 'added_declarations'}
                or not {'paths', 'targets', 'reason'}.issubset(rule)
                or not isinstance(rule['paths'], list) or not rule['paths']
                or not isinstance(rule['targets'], list) or not rule['targets']
                or any(not isinstance(key, str) or key not in targets for key in rule['targets'])
                or len(rule['targets']) != len(set(rule['targets']))
                or not isinstance(rule['reason'], str) or not rule['reason']):
            raise ValueError('invalid fast test coverage rule')
        for path in rule['paths']:
            if (not isinstance(path, str) or not path.endswith('.rs')
                    or posixpath.isabs(path) or posixpath.normpath(path) != path
                    or '..' in path.split('/') or any(c in path for c in ('\\', '*', '?', '[', ']'))
                    or any(ord(c) < 32 for c in path) or path in seen_paths):
                raise ValueError('invalid or duplicate fast test input path')
            seen_paths.add(path)
        declarations = rule.get('added_declarations')
        if declarations is not None and (
                not isinstance(declarations, list) or not declarations
                or any(not valid_fast_declaration(item) for item in declarations)
                or len(declarations) != len(set(declarations))):
            raise ValueError('invalid fast test declaration additions')


def only_registered_declarations_added(path, comparisons, declarations):
    """Permit additive wiring, never removal/replacement of consumer APIs."""
    alternatives = [re.escape(re.sub(r'\s+', '', item)) for item in declarations]
    allowed = re.compile('(?:' + '|'.join(alternatives) + ')+')
    for before, after in comparisons:
        patch = git('diff', '--no-ext-diff', '--unified=0', before, after, '--', path).decode()
        added = []
        for line in patch.splitlines():
            if line.startswith(('---', '+++')):
                continue
            if line.startswith('-') and line[1:].strip():
                return False
            if line.startswith('+'):
                added.append(line[1:])
        text = re.sub(r'\s+', '', '\n'.join(added))
        if text and allowed.fullmatch(text) is None:
            return False
    return True


def ordinary_fast_selection(config, paths, path_affected, path_rules, selected_core, comparisons):
    """Every affected native/Formal input needs a bounded, trusted contract."""
    if config.get('fast_test_inventory_version') != FAST_TEST_INVENTORY_VERSION:
        return None
    mappings = {path: rule for rule in config['ordinary_fast_rules'] for path in rule['paths']}
    selected, covered, reasons = set(), set(), []
    for path in paths:
        groups = set(path_rules.get(path, ()))
        groups.update(group for package in path_affected.get(path, ())
                      for group in config['package_groups'].get(package, ()))
        if not groups.intersection({'oasis7_required', 'formal_process'}):
            continue
        rule = mappings.get(path)
        if rule is None or (rule.get('added_declarations') is not None
                and not only_registered_declarations_added(path, comparisons, rule['added_declarations'])):
            return None
        selected.update(rule['targets'])
        covered.add(path)
        reasons.append('fast target contract ' + rule['reason'] + ': ' + path)
    if not selected:
        return None
    targets = [config['ordinary_fast_targets'][key] for key in sorted(selected)]
    if not {target['package'] for target in targets}.issubset(selected_core):
        return None
    return targets, covered, reasons


def resolve_commit(ref):
    return git('rev-parse', '--verify', ref + '^{commit}').decode().strip()


def formal_shards(suites, inventory):
    """Logical suites keep all their cases even when balancing moved a case."""
    if not suites:
        return []
    if inventory is None:
        return sorted(FORMAL_SUITES)
    if not isinstance(inventory, dict) or set(inventory.get('suites', {})) != set(FORMAL_SUITES):
        raise ValueError('invalid trusted formal suite inventory')
    overrides = inventory.get('case_shards', {})
    result = set(suites)
    for suite in suites:
        for entry in inventory['suites'][suite]:
            shard = overrides.get(entry.get('binary'), {}).get(entry.get('name'), suite)
            if shard not in FORMAL_SUITES:
                raise ValueError('unknown trusted formal shard')
            result.add(shard)
    return sorted(result)


def changed_test_policy(base, source, paths, feature_owners, feature_packages, available_features,
                        ordinary_roots=(), source_inputs=None, narrow_features_by_path=None,
                        narrow_unavailable=None, selected_targets=None):
    """Detect selection-affecting edits without executing candidate Rust.

    Full-feature execution preserves a required->full move. Semantic retirement
    of a removed/renamed case remains part of independent review; this is not a
    Rust parser or a claim that a replacement proves the old behavior.
    """
    rust_paths = [path for path in paths if path.endswith('.rs')]
    if not rust_paths:
        return [], False, []
    policy_paths = []
    production_cfg_paths = []
    enabled_features = {'test_tier_required', 'test_tier_full', 'wasmtime', 'viewer_live_integration'}

    def applies_to(owner, test_owner):
        return owner == test_owner or (owner[0] in ('mod', 'module') and
               (not owner[1] or test_owner[1].startswith(owner[1] + '::')))

    def ordinary_attribute(values, native_linux=False):
        return (values == ('test',)
                or enabled_gate(values, native_linux, set())
                or (len(values) == 3 and values[:2] == ('path', '=')
                    and rust_literal_path(values[2]) is not None)
                or (values[:4] == ('tokio', ':', ':', 'test')
                    and (len(values) == 4 or (values[4:5] == ('(',) and values[-1] == ')')))
                or values[0] in ('allow', 'warn', 'deny', 'forbid', 'expect', 'doc', 'should_panic'))

    def attributes(source, native_linux=False):
        tokens, closing = rust_source_inputs(source, return_tokens=True)
        selection, ignored, test_owners = set(), set(), set()
        annotations, ordinary_context = set(), set()
        modules = [(index + 2, closing[index + 2], tokens[index + 1][1].removeprefix('r#'))
                   for index, (_, value, _) in enumerate(tokens[:-2])
                   if value == 'mod' and tokens[index + 1][0] == 'identifier'
                   and tokens[index + 2][1] == '{' and index + 2 in closing]
        module_braces = {start for start, _, _ in modules}

        def module_path(index):
            return tuple(name for start, end, name in modules if start < index < end)

        # Qualified lexical ownership prevents two modules' regression() cases
        # from cancelling out in the before/after attribute comparison. Repeated
        # item names under mutually exclusive cfgs get separate occurrences too.
        item_owners, occurrences = {}, {}
        item_kinds = {'fn', 'mod', 'struct', 'enum', 'trait', 'type', 'const', 'static', 'impl', 'use'}
        for index, (_, value, _) in enumerate(tokens[:-1]):
            if value not in item_kinds and not (value == 'include' and tokens[index + 1][1] == '!'):
                continue
            name = '::'.join((*module_path(index), tokens[index + 1][1].removeprefix('r#')))
            key = value, name
            occurrence = occurrences.get(key, 0)
            occurrences[key] = occurrence + 1
            item_owners[index] = (*key, occurrence)
            if not any(start < index < stop and start not in module_braces
                       for start, stop in closing.items()):
                ordinary_context.add(item_owners[index])
        # A local `let` cfg is not an item. Absolute token offsets are not
        # lexical identities: inserting an unrelated struct must not make an
        # unchanged macOS statement look like a newly gated Linux test. Keep
        # this exception narrow: direct free-function-body lets only, with complete
        # statement tokens and their enclosing qualified function as identity.
        # Changed expressions/initializers, new statements and moved scopes
        # remain additions and go through the existing validated-driver check.
        function_bodies = []
        for start, owner in item_owners.items():
            if owner[0] != 'fn':
                continue
            cursor = start + 2
            while cursor < len(tokens) and tokens[cursor][1] not in ('{', ';'):
                if cursor in closing and tokens[cursor][1] in ('(', '['):
                    cursor = closing[cursor] + 1
                else:
                    cursor += 1
            if cursor in closing and tokens[cursor][1] == '{':
                function_bodies.append((cursor, closing[cursor], owner))
        statement_owners, statement_occurrences = {}, {}
        for start, (_, value, _) in enumerate(tokens):
            if value != 'let':
                continue
            bodies = [(left, right, owner) for left, right, owner in function_bodies
                      if left < start < right]
            if not bodies:
                continue
            left, right, function = min(bodies, key=lambda body: body[1] - body[0])
            if any(opening < left < right < stop and tokens[opening][1] == '{'
                   and opening not in module_braces for opening, stop in closing.items()):
                continue  # Impl/trait/nested-function/macro owners remain conservative.
            if any(left < opening < start < stop <= right and tokens[opening][1] == '{'
                   for opening, stop in closing.items()):
                continue  # Nested blocks/macros retain conservative handling.
            end = start + 1
            while end < right and tokens[end][1] != ';':
                if end in closing:
                    end = closing[end] + 1
                else:
                    end += 1
            if end >= right:
                continue
            identity = hashlib.sha256(json.dumps(
                [token for _, token, _ in tokens[start:end + 1]],
                ensure_ascii=True, separators=(',', ':')).encode()).hexdigest()
            key = function, identity
            occurrence = statement_occurrences.get(key, 0)
            statement_occurrences[key] = occurrence + 1
            statement_owners[start] = ('statement', f'{function[1]}#{function[2]}::let:{identity}', occurrence)

        for index, (_, token, _) in enumerate(tokens):
            inner = index + 1 < len(tokens) and tokens[index + 1][1] == '!'
            opening = index + 2 if inner else index + 1
            if token != '#' or opening not in closing or tokens[opening][1] != '[':
                continue
            end = closing[opening]
            values = tuple(value.removeprefix('r#') if kind == 'identifier' else value
                           for kind, value, _ in tokens[opening + 1:end])
            if not values:
                continue
            cursor = end + 1
            while cursor < len(tokens) and cursor not in item_owners and tokens[cursor][1] not in ('{', ';'):
                if tokens[cursor][1] == '#' and cursor + 1 in closing:
                    cursor = closing[cursor + 1] + 1
                    continue
                cursor += 1
            owner = (('module', '::'.join(module_path(index)), 0) if inner else
                     item_owners.get(cursor, ('unresolved', '::'.join(module_path(index)), cursor)))
            if owner[0] == 'unresolved':
                target = end + 1
                while target < len(tokens) and tokens[target][1] == '#' and target + 1 in closing:
                    target = closing[target + 1] + 1
                owner = statement_owners.get(target, owner)
            annotations.add((values, owner))
            if values[0] not in ('cfg', 'cfg_attr', 'ignore') and 'test' not in values:
                continue
            if values[0] == 'cfg_attr':
                # Lint/derive attributes do not select test execution. A nested
                # cfg/cfg_attr/test/ignore payload does and needs proof below.
                nesting = 0
                comma = None
                for offset, value in enumerate(values[2:], 2):
                    if value == ',' and nesting == 0:
                        comma = offset
                        break
                    nesting += (value == '(') - (value == ')')
                if comma is not None and not set(values[comma + 1:]) & {'cfg', 'cfg_attr', 'test', 'ignore'}:
                    continue
            selection.add((values, owner))
            if values[0] not in ('cfg', 'cfg_attr') and 'test' in values:
                test_owners.add(owner)
            if values[0] == 'ignore':
                ignored.add(owner)
        ordinary_items = {owner for owner in ordinary_context
                          if all(ordinary_attribute(attribute, native_linux)
                                 for attribute, target in annotations if applies_to(target, owner))}
        return (selection, ignored, test_owners, set(item_owners.values()),
                test_owners & ordinary_items, ordinary_items, annotations,
                tokens, closing, item_owners, modules, ordinary_context)

    def test_related(item, test_owners, path):
        attribute, owner = item
        if attribute[0] not in ('cfg', 'cfg_attr'):
            return True
        if any('test_tier_' in token for token in attribute):
            return True
        if owner in test_owners:
            return True
        if owner[0] in ('mod', 'module') and any(
                not owner[1] or test_owner[1].startswith(owner[1] + '::') for test_owner in test_owners):
            return True
        short_name = owner[1].rsplit('::', 1)[-1]
        return (('test' in attribute and owner[0] != 'fn')
                or '/tests/' in path or path.endswith(('_tests.rs', '/tests.rs'))
                or (owner[0] == 'mod'
                    and (short_name == 'tests' or short_name.startswith('test_') or short_name.endswith('_tests'))))

    def enabled_gate(attribute, native_linux, enabled=None):
        # Accept only a known expression that is actually true in the expanded
        # driver feature and native target combination. Mentioning an allowed
        # feature is insufficient: all(feature="test_tier_full", any()) is false.
        if len(attribute) < 4 or attribute[0:2] != ('cfg', '(') or attribute[-1] != ')':
            return False
        values = attribute[2:-1]
        actual_features = enabled_features if enabled is None else enabled

        def expression(cursor):
            if cursor >= len(values):
                raise ValueError('missing cfg predicate')
            value = values[cursor]
            if value == 'test':
                return True, cursor + 1
            if native_linux and value in ('unix', 'windows'):
                return value == 'unix', cursor + 1
            if value == 'feature' and values[cursor + 1:cursor + 2] == ('=',):
                if cursor + 2 >= len(values):
                    raise ValueError('missing cfg feature')
                return (True if values[cursor + 2].strip('"') in actual_features else None), cursor + 3
            native_values = {'target_os': 'linux', 'target_family': 'unix',
                             'target_arch': 'x86_64', 'target_pointer_width': '64',
                             'target_endian': 'little', 'target_env': 'gnu'}
            if native_linux and value in native_values and values[cursor + 1:cursor + 2] == ('=',):
                if cursor + 2 >= len(values):
                    raise ValueError('missing target cfg value')
                return values[cursor + 2].strip('"') == native_values[value], cursor + 3
            if value not in ('all', 'any', 'not') or values[cursor + 1:cursor + 2] != ('(',):
                raise ValueError('unsupported cfg predicate')
            cursor += 2
            predicates = []
            while cursor < len(values) and values[cursor] != ')':
                result, cursor = expression(cursor)
                predicates.append(result)
                if cursor < len(values) and values[cursor] == ',':
                    cursor += 1
                elif cursor >= len(values) or values[cursor] != ')':
                    raise ValueError('invalid cfg expression')
            if cursor >= len(values):
                raise ValueError('unclosed cfg expression')
            if value == 'not':
                if len(predicates) != 1:
                    raise ValueError('invalid negated cfg expression')
                return (None if predicates[0] is None else not predicates[0]), cursor + 1
            if value == 'all':
                result = False if False in predicates else (None if None in predicates else True)
            else:
                result = True if True in predicates else (None if None in predicates else False)
            return result, cursor + 1

        try:
            enabled, end = expression(0)
            return enabled is True and end == len(values)
        except ValueError:
            return False

    def source_at(ref, path):
        try:
            return git('show', f'{ref}:{path}').decode('utf-8', 'replace')
        except subprocess.CalledProcessError:
            return ''  # Both rename endpoints and additions/deletions are present.

    source_inputs = source_inputs or {}
    sources = source_inputs.get('sources', {})
    directories = source_inputs.get('directories', {})
    parsed_sources, parent_cache, context_cache = {}, {}, {}

    def source_owners(path):
        return {name for name, directory in directories.items()
                if path.startswith(directory + '/') or not directory}

    def parsed(path, native):
        key = path, native
        if key not in parsed_sources:
            parsed_sources[key] = attributes(sources[path].decode('utf-8', 'replace'), native)
        return parsed_sources[key]

    def parents(path, native, selected_features=None):
        """Find every literal mod/include reference without expanding macros.

        Only same-package, unambiguous static references can prove ordinary
        execution. The file's own inner attributes are checked by its caller.
        """
        key = path, native, selected_features
        if key in parent_cache:
            return parent_cache[key]
        owners = source_owners(path)
        stem = (posixpath.basename(posixpath.dirname(path)) if path.endswith('/mod.rs')
                else posixpath.basename(path).removesuffix('.rs'))
        name = re.compile(rb'(?<![A-Za-z0-9_])' + re.escape(stem.encode()) + rb'(?![A-Za-z0-9_])')
        candidates = [candidate for candidate, contents in sources.items()
                      if candidate in source_inputs.get('uncertain_modules', ())
                      or ((source_owners(candidate) == owners or posixpath.basename(path).encode() in contents)
                          and (name.search(contents) or posixpath.basename(path).encode() in contents))]
        references = []
        for parent in candidates:
            info = parsed(parent, native)
            annotations, tokens, closing, items, modules, context = info[6:]
            directory = posixpath.dirname(parent)
            module_directory = (directory if parent in ordinary_roots
                                or posixpath.basename(parent) in ('lib.rs', 'main.rs', 'mod.rs')
                                or parent in source_inputs.get('includes', ())
                                else parent.removesuffix('.rs'))
            for index, owner in items.items():
                kind = tokens[index][1]
                inline = tuple(name for start, end, name in modules if start < index < end)
                attributes_here = [attribute for attribute, target in annotations if applies_to(target, owner)]
                known = owner in context and (selected_features is not None or source_owners(parent) == owners)
                targets = []
                if kind == 'mod' and tokens[index + 2][1] == ';':
                    name = tokens[index + 1][1].removeprefix('r#')
                    inherited_path = any(attribute[0] in ('path', 'cfg_attr') and 'path' in attribute
                                         for attribute, target in annotations
                                         if target != owner and applies_to(target, owner))
                    path_attrs = [attribute for attribute, target in annotations
                                  if target == owner and attribute[0] == 'path']
                    conditional_paths = [attribute for attribute, target in annotations
                                         if target == owner and attribute[0] == 'cfg_attr' and 'path' in attribute]
                    if conditional_paths:
                        # Neither an alternate path nor its inherited cfg may
                        # be inferred from whichever filename happens to exist.
                        targets.append(path)
                        known = False
                    if path_attrs:
                        if (len(path_attrs) == 1 and len(path_attrs[0]) == 3
                                and rust_literal_path(path_attrs[0][2]) is not None and not inline):
                            targets.append(posixpath.normpath(posixpath.join(directory, rust_literal_path(path_attrs[0][2]))))
                        else:
                            # A possible nonliteral parent cannot disappear
                            # just because another parent is easy to resolve.
                            targets.append(path)
                            known = False
                    else:
                        prefix = posixpath.join(module_directory, *inline, name)
                        targets.extend(target for target in (prefix + '.rs', prefix + '/mod.rs') if target in sources)
                        known &= len(targets) == 1
                    if inherited_path:
                        targets.append(path)
                        known = False
                elif kind == 'include':
                    if selected_features is None:
                        known &= not inline
                    end = closing.get(index + 2)
                    values = [value for _, value, _ in tokens[index + 3:end]] if end is not None else []
                    if values and values[-1] == ',':
                        values.pop()
                    if len(values) == 1 and rust_literal_path(values[0]) is not None:
                        value = rust_literal_path(values[0])
                        targets.append(posixpath.normpath(posixpath.join(directory, value)))
                        if selected_features is None:
                            known &= '/' not in value and '\\' not in value
                    else:
                        targets.append(path)
                        known = False
                if path in targets:
                    references.append((parent, kind, known and all(
                        ordinary_attribute(attribute, native)
                        or (selected_features is not None and enabled_gate(attribute, native, selected_features))
                        for attribute in attributes_here), attributes_here))
        parent_cache[key] = references
        return references

    def ordinary_source_context(path, native, visiting=frozenset()):
        key = path, native
        if key in context_cache:
            return context_cache[key]
        if path in visiting:
            return False
        if source_inputs.get('uncertain_modules'):
            return False
        if path not in sources:
            return path in ordinary_roots
        if source_owners(path) & set(source_inputs.get('unresolved_includes', ())):
            return False
        references = parents(path, native)
        # Mixing include! with an external mod changes the implicit directory
        # for nested modules; no one convenient context can authorise the other.
        mixed = len({kind for _, kind, _, _ in references}) > 1
        enabled = bool(path in ordinary_roots or references) and not mixed
        enabled &= all(known and ordinary_source_context(parent, native, visiting | {path})
                       for parent, _, known, _ in references)
        context_cache[key] = enabled
        return enabled

    def inherited_gates(path, native, visiting=frozenset()):
        if path in visiting or path not in sources:
            return set()
        result = set()
        for parent, _, _, annotations in parents(path, native):
            result.update(attribute for attribute in annotations if attribute[0] in ('cfg', 'cfg_attr'))
            result.update(inherited_gates(parent, native, visiting | {path}))
        return result

    def selected_target_root(target):
        package = target['package']
        manifest = source_inputs.get('manifests', {}).get(package)
        directory = directories.get(package)
        if not isinstance(manifest, dict) or directory is None:
            return None
        name = target['target_name']
        if target['target_kind'] == 'lib':
            declaration = manifest.get('lib', {})
            if declaration.get('name', package.replace('-', '_')) != name:
                return None
            path = declaration.get('path', 'src/lib.rs')
        else:
            declarations = manifest.get('test', [])
            matches = [item for item in declarations if item.get('name') == name]
            if len(matches) > 1 or declarations and not matches:
                return None
            declaration = matches[0] if matches else {}
            if not matches and not manifest.get('package', {}).get('autotests', True):
                return None
            path = declaration.get('path', 'tests/' + name + '.rs')
        root = posixpath.normpath(posixpath.join(directory, path))
        if (root not in sources or root not in ordinary_roots
                or not declaration.get('test', True) or not declaration.get('harness', True)
                or not set(declaration.get('required-features', ())).issubset(target['features'])):
            return None
        return root

    selected_context_cache = {}

    def selected_source_context(path, target, visiting=frozenset()):
        """One proven route to the selected target is sufficient for shared tests.

        This proof is used only after every affected input has matched trusted
        fast rules. It does not change the legacy all-parent ownership proof.
        """
        root = selected_target_root(target)
        features = frozenset(target['features'])
        key = path, root, features
        if key in selected_context_cache:
            return selected_context_cache[key]
        if (root is None or path in visiting or path not in sources
                or source_inputs.get('uncertain_modules')
                or source_owners(path) & set(source_inputs.get('unresolved_includes', ()))):
            return False
        annotations = parsed(path, True)[6]
        inner = [attribute for attribute, owner in annotations if owner == ('module', '', 0)]
        if any(not (ordinary_attribute(attribute, True) or enabled_gate(attribute, True, features))
               for attribute in inner):
            return False
        result = path == root or any(
            known and selected_source_context(parent, target, visiting | {path})
            for parent, _, known, _ in parents(path, True, features))
        selected_context_cache[key] = result
        return result

    def native_context(path):
        owners = feature_owners.get(path, set())
        if path in ordinary_roots or owners and owners.issubset(feature_packages):
            return True
        # A package-level default driver covers only its executed lib tree,
        # not arbitrary bins/targets. Prove the literal module path to a known
        # root under native predicates before granting target knowledge.
        return bool(owners and owners.issubset(feature_packages | NATIVE_DEFAULT_TEST_PACKAGES)
                    and ordinary_source_context(path, True))

    analyses = []
    for path in rust_paths:
        before, after = source_at(base, path), source_at(source, path)
        if before != after:
            native = native_context(path)
            analyses.append((path, attributes(before, native), attributes(after, native)))
    # Keep both rename endpoints and removals in the review evidence. A new
    # marker is exempt only when this change contains no retiring test owner.
    has_removed_tests = any(before[2] - after[2] for _, before, after in analyses)
    for path, before, after in analyses:
        before_attrs, old_ignored, old_test_owners, old_items, _ = before[:5]
        after_attrs, new_ignored, new_test_owners, _, ordinary_owners = after[:5]
        test_owners = old_test_owners | new_test_owners
        changes = before_attrs ^ after_attrs
        native = native_context(path)
        ordinary_source = ordinary_source_context(path, native)
        # Unchanged full/WASM gates still matter when the enclosed body changes.
        # Attribute diffs alone cannot prove that a narrower driver runs it.
        if narrow_unavailable is not None and path in (narrow_features_by_path or {}):
            gates = {attribute for attribute, _ in after_attrs if attribute[0] in ('cfg', 'cfg_attr')}
            gates.update(inherited_gates(path, True))
            if any(not enabled_gate(attribute, True, narrow_features_by_path[path]) for attribute in gates):
                narrow_unavailable.add(path)
        if selected_targets and path in (narrow_features_by_path or {}):
            # A legacy root is not evidence that the chosen small target runs a
            # new case. Prove one static route to an actually selected root.
            ordinary_source = any(selected_source_context(path, target) for target in selected_targets)
        new_ordinary = ordinary_owners - old_items if ordinary_source and not has_removed_tests else set()
        benign_additions = {item for item in after_attrs - before_attrs if item[1] in new_ordinary}
        for attribute, owner in after_attrs - before_attrs:
            if (ordinary_source and not has_removed_tests
                    and ordinary_attribute(attribute, native)
                    and ((owner in after[5] and owner not in old_items)
                         or (owner == ('module', '', 0) and not source_at(base, path)))):
                benign_additions.add((attribute, owner))
        changes -= benign_additions
        test_changes = {item for item in changes if test_related(item, test_owners, path)}
        added_attrs = {item for item in after_attrs - before_attrs if test_related(item, test_owners, path)}
        added_attrs.update(item for item in benign_additions if item[0][0] in ('cfg', 'cfg_attr'))
        # A new case inherits unchanged module/file gates too. They must be
        # executable under the same expanded combination as a newly added gate.
        added_attrs.update((attribute, owner) for attribute, owner in after_attrs
                           if attribute[0] in ('cfg', 'cfg_attr')
                           and any(applies_to(owner, test) for test in new_test_owners - old_test_owners))
        if new_test_owners - old_test_owners:
            added_attrs.update((attribute, ('module', '', 0)) for attribute in inherited_gates(path, native))
        if test_changes:
            policy_paths.append(path)
        if changes - test_changes:
            production_cfg_paths.append(path)
        conditional_ignore = any('ignore' in attribute and attribute[0] == 'cfg_attr'
                                 for attribute, _ in added_attrs)
        newly_ignored = new_ignored - old_ignored
        # A short function name does not establish a Cargo binary + qualified
        # case identity. Existing registered ignored cases are unchanged here;
        # a new ignore needs a separately provable runner mapping.
        if conditional_ignore or newly_ignored:
            raise TestPolicyError('new ignored test lacks a trusted execution mapping: ' + path
                                  + '; keep it executable or register its actual ignored-test runner')
        for attribute, _ in added_attrs:
            if attribute[0] not in ('cfg', 'cfg_attr'):
                continue
            features = {attribute[index + 2].strip('"') for index, value in enumerate(attribute[:-2])
                        if value == 'feature' and attribute[index + 1] == '='}
            mapped_native_driver = (bool(feature_owners.get(path))
                                    and feature_owners[path].issubset(feature_packages))
            ordinary_gate = enabled_gate(attribute, native, set())
            if not ordinary_gate and features and not mapped_native_driver:
                raise TestPolicyError('changed test feature has no expanded-feature driver for its package: ' + path)
            actual_features = set(enabled_features)
            defaults = []
            for package in feature_owners.get(path, ()):
                table = available_features.get(package, {})
                # A set-only legacy caller provides declarations, not activation.
                table = table if isinstance(table, dict) else {}
                active, pending = set(), ['default']
                while pending:
                    feature = pending.pop()
                    if feature in active or feature not in table:
                        continue
                    active.add(feature)
                    pending.extend(table[feature])
                defaults.append(active)
            if defaults:
                # Every source owner must execute the inherited condition.
                actual_features.update(set.intersection(*defaults))
            if not ordinary_gate and not enabled_gate(attribute, mapped_native_driver, actual_features):
                raise TestPolicyError('changed cfg is outside the validated test feature combinations: ' + path)
            if features and (not feature_owners.get(path) or any(
                    not features.issubset(available_features.get(package, set()))
                    for package in feature_owners[path])):
                raise TestPolicyError('changed test feature is not declared by its executing package: ' + path)
    return sorted(policy_paths), False, sorted(production_cfg_paths)


def full_plan(config, profile, source_head, test_head=None, policy_ref=None, formal_inventory=None):
    """Expand the trusted full inventory, independently of a commit diff."""
    validate_config(config)
    if profile not in ('periodic-full', 'candidate-full'):
        raise ValueError('not a full execution profile')
    if config.get('fast_scope_version') != FAST_PLAN_VERSION:
        raise ValueError('full profiles require a trusted versioned inventory')
    source = resolve_commit(source_head)
    tested = resolve_commit(test_head or source_head)
    policy = resolve_commit(policy_ref or source_head)
    groups = sorted(config['groups'])
    suites = list(FORMAL_SUITES) if 'formal_process' in groups else []
    plan = {
        'scope': 'explicit-full', 'groups': groups,
        'resources': sorted({resource for group in groups for resource in config['resources'][group]}),
        'architecture': {'version': 1, 'mode': 'full', 'packages': [],
                         'base': policy, 'source': source, 'tested': tested},
        'workspace_support_profile_version': 1, 'workspace_support_profiles': sorted(SUPPORT_PROFILES),
        'execution_profile': profile, 'fast_plan_version': FAST_PLAN_VERSION,
        'check_inventory_version': CHECK_INVENTORY_VERSION,
        'source_sha': source, 'tested_sha': tested, 'policy_sha': policy,
        'oasis7_required_packages': sorted(config['oasis7_required_packages']),
        'oasis7_required_wasm_packages': sorted(config['oasis7_required_wasm_packages']),
        'formal_suites': sorted(suites), 'formal_shards': formal_shards(suites, formal_inventory),
        'command_center_package': True, 'test_policy_changed': False, 'test_policy_paths': [],
        'fast_plan_exceptions': [], 'reasons': ['complete trusted inventory; no changed-files filter'],
    }
    if config.get('fast_test_inventory_version') == FAST_TEST_INVENTORY_VERSION:
        plan['full_fast_test_inventory_version'] = FAST_TEST_INVENTORY_VERSION
        plan['ordinary_full_fast_targets'] = [config['ordinary_fast_targets'][key]
                                               for key in sorted(config['ordinary_fast_targets'])]
    return plan


def fast_plan(config, plan, base, source, tested, paths, affected, path_affected,
              path_rules, additions, profile, formal_inventory=None, policy_ref=None,
              comparisons=None, feature_owners=None, feature_manifests=None, test_target_roots=None,
              source_inputs=None):
    if config.get('fast_scope_version') != FAST_PLAN_VERSION:
        # The version capability comes from BASE. Candidate metadata cannot opt in.
        raise ValueError('fast profiles require trusted baseline fast_scope_version')
    chosen = set(plan['groups'])
    conservative = plan['scope'] == 'full'
    exceptions = []
    policy_paths, ignored_risk, production_cfg_paths = set(), False, set()
    selected_core = affected & set(config['oasis7_required_packages'])
    native_fast_candidate = None
    if (not conservative and 'oasis7_required' in chosen and selected_core
            and 'oasis7_required' not in additions
            and not any('oasis7_required' in groups for groups in path_rules.values())):
        native_fast_candidate = ordinary_fast_selection(
            config, paths, path_affected, path_rules, selected_core, comparisons or [(base, source)])
    narrow_features_by_path, narrow_unavailable = {}, set()
    if config.get('fast_test_inventory_version') == FAST_TEST_INVENTORY_VERSION:
        for rule in config['ordinary_fast_rules']:
            if 'added_declarations' not in rule:
                features = set.intersection(*(set(config['ordinary_fast_targets'][key]['features'])
                                              for key in rule['targets']))
                narrow_features_by_path.update((path, features) for path in rule['paths'])
    for before, after in comparisons or [(base, source)]:
        changed_policy, changed_ignored, production_cfg = changed_test_policy(
            before, after, paths, feature_owners or {},
            set(config['oasis7_required_packages']) | set(SUPPORT_PROFILES),
            (feature_manifests or {}).get(resolve_commit(after), {}),
            (test_target_roots or {}).get(resolve_commit(after), set()),
            (source_inputs or {}).get(resolve_commit(after), {}),
            narrow_features_by_path, narrow_unavailable,
            native_fast_candidate[0] if native_fast_candidate else None)
        policy_paths.update(changed_policy)
        ignored_risk |= changed_ignored
        production_cfg_paths.update(production_cfg)
    policy_paths = sorted(policy_paths)
    if narrow_unavailable:
        plan['reasons'].extend('changed source has an existing gate outside fast features; expanded coverage: ' + path
                               for path in sorted(narrow_unavailable))
        policy_paths = sorted(set(policy_paths) | narrow_unavailable)
    if production_cfg_paths:
        # Production platform/feature edits are not test retirement. Preserve
        # the old necessary groups and platform jobs instead of rejecting code.
        conservative = True
        chosen = set(config['groups'])
        plan['scope'] = 'full'
        plan['architecture']['mode'] = 'full'
        plan['workspace_support_profiles'] = sorted(SUPPORT_PROFILES)
        plan['reasons'].extend('production cfg changed; conservative platform coverage: ' + path
                               for path in sorted(production_cfg_paths))
    if conservative:
        exceptions.append('shared control or uncertain impact; original necessary groups retained')
    if policy_paths:
        exceptions.append('test execution policy changed; full features and independent retirement review required')
        plan['reasons'].extend('test execution policy changed: ' + path for path in policy_paths)
    core_inventory = set(config['oasis7_required_packages'])
    selected_core = affected & core_inventory
    core_fallback = (conservative or 'oasis7_required' in additions
                     or any('oasis7_required' in groups for groups in path_rules.values())
                     or not selected_core)
    if 'oasis7_required' not in chosen:
        selected_core, wasm = set(), set()
    elif core_fallback:
        selected_core, wasm = core_inventory, set(config['oasis7_required_wasm_packages'])
    else:
        wasm = affected & set(config['oasis7_required_wasm_packages'])

    native_fast = native_fast_candidate if not core_fallback and not policy_paths and not ignored_risk else None
    if native_fast is not None:
        plan['fast_test_inventory_version'] = FAST_TEST_INVENTORY_VERSION
        plan['ordinary_fast_targets'] = native_fast[0]
        plan['reasons'].extend(native_fast[2])

    suites = set()
    if 'formal_process' in chosen:
        if conservative or 'formal_process' in additions or ignored_risk:
            suites.update(FORMAL_SUITES)
        else:
            for path in paths:
                path_groups = set(path_rules.get(path, ()))
                path_groups.update(group for package in path_affected.get(path, ())
                                   for group in config['package_groups'].get(package, ()))
                if 'formal_process' not in path_groups:
                    continue
                if native_fast is not None and path in native_fast[1]:
                    plan['reasons'].append('bounded native contract; complete process regression stays in full: ' + path)
                    continue
                matches = [rule for rule in config['formal_risk_rules']
                           if any(fnmatch.fnmatchcase(path, pattern) for pattern in rule['match'])]
                if not matches:
                    suites.update(FORMAL_SUITES)
                    plan['reasons'].append('no trusted narrow Formal mapping: ' + path)
                for rule in matches:
                    suites.update(rule['suites'])
                    plan['reasons'].append('Formal risk ' + rule['reason'] + ': ' + path)
            if not paths:
                suites.update(FORMAL_SUITES)
        if not suites:
            chosen.remove('formal_process')
            plan['reasons'].append('Formal infrastructure unchanged; affected behavior stays in ordinary tests')
        else:
            exceptions.append('necessary Formal risk suites: ' + ', '.join(sorted(suites)))

    center_paths = [path for path in paths if
                    'command_center' in path_rules.get(path, ()) or any(
                        'command_center' in config['package_groups'].get(package, ())
                        for package in path_affected.get(path, ()))]
    center_package = ('command_center' in chosen and (conservative or 'command_center' in additions
                      or not center_paths or any(not any(fnmatch.fnmatchcase(path, pattern)
                          for pattern in config.get('command_center_fast_paths', [])) for path in center_paths)))
    plan.update(groups=sorted(chosen),
                resources=sorted({resource for group in chosen for resource in config['resources'][group]}),
                execution_profile=profile, fast_plan_version=FAST_PLAN_VERSION,
                check_inventory_version=CHECK_INVENTORY_VERSION,
                source_sha=resolve_commit(source), tested_sha=resolve_commit(tested or source),
                policy_sha=resolve_commit(policy_ref or base),
                oasis7_required_packages=sorted(selected_core), oasis7_required_wasm_packages=sorted(wasm),
                formal_suites=sorted(suites), formal_shards=formal_shards(suites, formal_inventory),
                command_center_package=center_package,
                test_policy_changed=bool(policy_paths), test_policy_paths=policy_paths,
                fast_plan_exceptions=exceptions)
    plan['reasons'] = list(dict.fromkeys(plan['reasons']))
    return plan


def select(config, base, source_head, test_head=None, additions=(), profile=None,
           formal_inventory=None, policy_ref=None):
    validate_config(config)
    groups = set(config['groups'])
    chosen = {'baseline'}
    reasons = ['baseline always runs']
    paths = []
    affected = set()
    architecture_inputs = set()
    unknown_paths = []
    full = False
    support_rule = False
    path_support_profiles = set()
    path_affected = {}
    path_rules = {}
    feature_owners = {}
    feature_manifests = {}
    test_target_roots = {}
    source_inputs = {}
    policy_comparisons = [(base, source_head)]
    try:
        # Push/source-only checks retain the event range. For PR merge checks,
        # exclude target history already present in the actual first parent.
        graph_refs = [base, source_head, test_head or source_head]
        paths = changed_paths(base, source_head)
        try:
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
                policy_comparisons = [(source_base, source_oid), (target, test_head)]
                graph_refs.extend([source_base, target])
        except (OSError, subprocess.CalledProcessError, ValueError, KeyError, TypeError) as error:
            full = True
            reasons.append('uncertain tested merge inputs: ' + str(error))
        graphs = []
        for ref in dict.fromkeys(graph_refs):
            try:
                graph = cargo_graph(ref, config.get('generated_inputs', []))
                graphs.append(graph)
                sources = graph[2].pop('__rust_sources__', {})
                if profile is not None:
                    if graph[2].get('__uncertain_module_sources__'):
                        full = True
                        reasons.extend('uncertain Rust module input: ' + path
                                       for path in sorted(graph[2]['__uncertain_module_sources__']))
                    feature_manifests[resolve_commit(ref)] = {
                        package: manifest.get('features', {})
                        for package, (_, manifest) in graph[0].items()}
                    test_target_roots[resolve_commit(ref)] = {
                        path for path in sources for package, (directory, manifest) in graph[0].items()
                        if known_test_target_root(path, directory, manifest)
                        and (package in config.get('oasis7_required_packages', ())
                             or (set(config['package_groups'].get(package, ()))
                                 & {'consensus', 'distfs', 'node', 'net'}
                                 and posixpath.relpath(path, directory or '.')
                                 == manifest.get('lib', {}).get('path', 'src/lib.rs')))}
                    source_inputs[resolve_commit(ref)] = {
                        'sources': sources,
                        'directories': {package: directory for package, (directory, _) in graph[0].items()},
                        'manifests': {package: manifest for package, (_, manifest) in graph[0].items()},
                        'includes': graph[2].get('__rust_include_targets__', ()),
                        'unresolved_includes': graph[2].get('__unresolved_rust_include_owners__', ()),
                        'uncertain_modules': graph[2].get('__uncertain_module_sources__', ()),
                    }
            except (OSError, subprocess.CalledProcessError, ValueError, KeyError, TypeError) as error:
                # Keep usable graphs to establish ownership even when another
                # registered input cannot be parsed. Uncertainty is not registration.
                full = True
                reasons.append('uncertain Git/dependency inputs: ' + str(error))
        reported_embeds = False
        for path in paths:
            path_affected[path] = set()
            path_rules[path] = set()
            feature_owners[path] = set()
            controlled = any(fnmatch.fnmatchcase(path, pattern) for pattern in CONTROL)
            scoped_groups = config.get('scoped_control_paths', {}).get(path)
            if scoped_groups is not None:
                chosen.update(scoped_groups)
                path_rules[path].update(scoped_groups)
                support_rule |= 'workspace_support' in scoped_groups
                reasons.append('scoped CI control changed: ' + path)
            elif controlled:
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
                    feature_owners[path].update(owners)
                    # Inferred directory readers widen tests; they do not register new inputs.
                    matched |= bool(registered_owners)
                    leaf = all(independent_target_input(path, *manifests[n]) for n in owners) and not any(
                        (not d or path.startswith(d + '/')) for d in embedded.get('__library_directories__', ()))
                    # An external fixture has no direct source owner; its readers
                    # determine whether reverse consumers are affected.
                    direct = {name for name, (directory, _) in manifests.items()
                              if directory and path.startswith(directory + '/')}
                    library_owners = (set() if leaf else direct) | imported_readers | embedded.get('__library_embeds__', {}).get(path, set())
                    architecture_inputs.update(owners)
                    affected.update(owners)
                    affected.update(reverse_consumers(library_owners, reverse))
                    path_affected[path].update(owners)
                    path_affected[path].update(reverse_consumers(library_owners, reverse))
            for rule in config['rules']:
                if (any(fnmatch.fnmatchcase(path, pattern) for pattern in rule['match'])
                        and not any(fnmatch.fnmatchcase(path, pattern) for pattern in rule.get('exclude', []))):
                    matched = True
                    chosen.update(rule.get('groups', []))
                    path_rules[path].update(rule.get('groups', []))
                    support_rule |= 'workspace_support' in rule.get('groups', [])
                    full |= rule.get('full', False)
                    reasons.append(rule['reason'] + ': ' + path)
            # Add an audited cross-package test consumer without flattening the
            # support group. This does not register ownership for unknown paths.
            for support_profile, patterns in config.get('workspace_support_profile_paths', {}).items():
                if any(fnmatch.fnmatchcase(path, pattern) for pattern in patterns):
                    chosen.add('workspace_support')
                    path_support_profiles.add(support_profile)
                    reasons.append('workspace support profile consumer ' + support_profile + ': ' + path)
            if any(embedded.get('__unresolved_embed__') for _, _, embedded in graphs) and not any(
                    directory and path.startswith(directory + '/') for manifests, _, _ in graphs
                    for directory, _ in manifests.values()):
                full = True
                reasons.append('unresolved embedded consumer input: ' + path)
                if not reported_embeds:
                    reasons.extend('unresolved Rust embed: ' + detail for detail in sorted({
                        detail for _, _, embedded in graphs
                        for detail in embedded.get('__unresolved_embed_sources__', ())}))
                    reported_embeds = True
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
        support_profiles = sorted(set(affected).intersection(SUPPORT_PROFILES) | path_support_profiles)
        # Non-package rules and explicit group requests retain the complete group.
        # Control/uncertain inputs must never narrow the old command inventory.
        unmapped_support = any(
            'workspace_support' in config['package_groups'].get(package, [])
            and package not in SUPPORT_PROFILES for package in affected)
        if full or support_rule or unmapped_support or not support_profiles or 'workspace_support' in additions:
            support_profiles = sorted(SUPPORT_PROFILES)
    plan = {'groups': sorted(chosen), 'resources': resources,
            'architecture': {'version': 1, 'mode': 'full' if full or additions or any(
                fnmatch.fnmatchcase(path, 'scripts/cargo-dev*') for path in paths) else 'targeted',
                'packages': sorted(architecture_inputs), 'base': base, 'source': source_head,
                'tested': test_head or source_head},
            'workspace_support_profiles': support_profiles,
            'workspace_support_profile_version': 1,
            'reasons': list(dict.fromkeys(reasons)), 'scope': 'full' if full else 'targeted'}
    if profile is not None:
        if profile not in ('pr-fast', 'main-fast'):
            raise ValueError('select requires a fast execution profile')
        return fast_plan(config, plan, base, source_head, test_head, paths, affected,
                         path_affected, path_rules, additions, profile, formal_inventory, policy_ref,
                         policy_comparisons, feature_owners, feature_manifests, test_target_roots, source_inputs)
    return plan


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--base-ref')
    parser.add_argument('--head-ref', required=True, help='source HEAD')
    parser.add_argument('--test-ref', help='actual tested merge/tree commit')
    parser.add_argument('--profile', choices=EXECUTION_PROFILES)
    parser.add_argument('--policy-ref', help='fixed trusted workflow/policy commit')
    parser.add_argument('--formal-config', default=str(Path(__file__).with_name('ci-formal-process-suites.json')))
    parser.add_argument('--config', default=str(Path(__file__).with_name('ci-required-scope.json')))
    parser.add_argument('--add-group', action='append', default=[])
    parser.add_argument('--github-output')
    parser.add_argument('--output')
    args = parser.parse_args()
    config = json.loads(Path(args.config).read_text())
    try:
        formal_inventory = (json.loads(Path(args.formal_config).read_text())
                            if args.profile and Path(args.formal_config).is_file() else None)
        if args.profile in ('periodic-full', 'candidate-full'):
            if args.add_group:
                raise ValueError('full inventory does not accept additional group requests')
            plan = full_plan(config, args.profile, args.head_ref, args.test_ref,
                             args.policy_ref or args.base_ref, formal_inventory)
        else:
            if not args.base_ref:
                raise ValueError('--base-ref is required for changed-files planning')
            plan = select(config, args.base_ref, args.head_ref, args.test_ref, args.add_group,
                          args.profile, formal_inventory, args.policy_ref)
    except (UnknownInputsError, TestPolicyError, ValueError) as error:
        parser.exit(2, f"CI scope planning failed: {error}\n")
    encoded = json.dumps(plan, ensure_ascii=True, separators=(',', ':'))
    if args.output:
        Path(args.output).write_text(encoded + '\n')
    if args.github_output:
        with Path(args.github_output).open('a') as output:
            output.write('plan=' + encoded + '\n')
            for group in config['groups']:
                output.write(f'run_{group}=' + str(group in plan['groups']).lower() + '\n')
            if args.profile:
                output.write('formal_shards=' + json.dumps(plan['formal_shards'], separators=(',', ':')) + '\n')
                output.write('command_center_package=' + str(plan['command_center_package']).lower() + '\n')
    print(encoded)


if __name__ == '__main__':
    main()
