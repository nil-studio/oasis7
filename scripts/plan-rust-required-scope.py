#!/usr/bin/env python3
"""Select ordinary CI groups from Git data. No network, task state or candidate code execution."""
import argparse
from bisect import bisect_right
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
from types import MappingProxyType

PLATFORM_GROUPS = ('windows_rollout', 'macos_package_contract', 'fleet_health')
EXECUTION_PROFILES = ('pr-fast', 'main-fast', 'periodic-full', 'candidate-full')
FAST_PLAN_VERSION = 1
FAST_TEST_INVENTORY_VERSION = 1
FAST_EXECUTION_VERSION = 1
ORDINARY_PARTITION_VERSION = 1
ORDINARY_PARTITION_PACKAGES = frozenset(('oasis7_runtime', 'oasis7_world_state'))
DISTFS_CLIPPY_CONTRACT_VERSION = 1
DISTFS_CLIPPY_TARGETS = frozenset((
    ('oasis7_runtime', 'lib', 'oasis7_runtime'),
    ('oasis7_runtime', 'test', 'pr_fast_incremental'),
    ('oasis7_world_state', 'lib', 'oasis7_world_state'),
    ('oasis7_world_state', 'test', 'pr_fast_reducers'),
))
ARCHITECTURE_BUILD_POLICY_VERSION = 1
CHECK_INVENTORY_VERSION = 1
PYTHON_CONTROL_CONTRACT_VERSION = 1
ADDITIVE_GOVERNANCE_REGISTRATION_VERSION = 1
CUSTODY_EXECUTION_VERSION = 1
CUSTODY_NODE_SHARDS = (0, 1, 2, 3)
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
           'scripts/plan-rust-required-scope*', 'scripts/document_corpus.py',
           'scripts/ci_process_runner.py')


class UnknownInputsError(ValueError):
    """Unregistered paths are a policy gap, not a request for broader tests."""

    def __init__(self, paths):
        super().__init__('unknown CI inputs: ' + json.dumps(sorted(set(paths)))
                         + '; register ownership and validation in the trusted baseline '
                         + 'before introducing these paths')


class TestPolicyError(ValueError):
    """An ignored test needs a trusted real execution path, not just a new tag."""


def git(*args, repo=None):
    return subprocess.check_output(['git', *(['-C', str(repo)] if repo is not None else []), *args],
                                   stderr=subprocess.DEVNULL)


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


def additive_governance_registration(config, comparisons, paths, *, repo=None):
    """Prove one additive diagnostic registration using immutable Git inputs.

    This is deliberately not a shell parser or a general ci-tests.sh exception.
    The trusted policy must opt in, every old literal invocation must survive in
    order, and the complete script outside the one fixed function is identical.
    The caller must retain Cargo ownership/embedded-consumer and other full
    fallbacks; this proof never removes a selected Rust consumer.
    """
    driver = 'scripts/ci-tests.sh'
    version = config.get('additive_governance_registration_version')
    if (driver not in paths or type(version) is not int
            or version != ADDITIVE_GOVERNANCE_REGISTRATION_VERSION):
        return None
    marker = 'run_workflow_governance_operational_contract_tests() {\n'
    command = re.compile(r'  run python3 \./(scripts/security/'
                         r'(?:[A-Za-z0-9_-]+\.test|test_[A-Za-z0-9_]+)\.py)\n')

    def read_git(*args):
        return subprocess.check_output(['git', *(['-C', str(repo)] if repo is not None else []),
                                        *args], stderr=subprocess.DEVNULL)

    def regular_blob(ref, path):
        record = read_git('ls-tree', '-z', ref, '--', path).split(b'\0')
        if len(record) != 2 or record[1] != b'':
            raise ValueError('registration requires one tracked regular file')
        metadata, name = record[0].split(b'\t', 1)
        mode, kind, oid = metadata.decode('ascii').split()
        if name.decode('utf-8') != path or kind != 'blob' or mode not in ('100644', '100755'):
            raise ValueError('registration requires a tracked regular file')
        return mode, oid

    def driver_parts(ref):
        mode, oid = regular_blob(ref, driver)
        if int(read_git('cat-file', '-s', oid)) > 256 * 1024:
            raise ValueError('unbounded diagnostic registration script')
        text = read_git('cat-file', 'blob', oid).decode('utf-8')
        if text.count(marker) != 1:
            raise ValueError('ambiguous diagnostic registration function')
        before, rest = text.split(marker)
        if before and not before.endswith('\n'):
            raise ValueError('registration function must start on its own line')
        body, after = rest.split('\n}\n', 1)
        lines = (body + '\n').splitlines(keepends=True)
        if (not lines or any(command.fullmatch(line) is None for line in lines)
                or len(lines) != len(set(lines))):
            raise ValueError('registration body must contain unique literal test invocations')
        return mode, before, lines, after

    def mapped_groups(path):
        scoped = config.get('scoped_control_paths', {}).get(path)
        # Keep mixtures with other drivers/planners full even when that Python
        # control has its own contract. Only already-scoped diagnostic workflows
        # may accompany this one shell registration exception.
        if (any(fnmatch.fnmatchcase(path, pattern) for pattern in CONTROL)
                and (scoped is None or not path.startswith('.github/'))):
            return None
        selected = set(scoped or ())
        matched = scoped is not None
        for rule in config['rules']:
            if (any(fnmatch.fnmatchcase(path, pattern) for pattern in rule['match'])
                    and not any(fnmatch.fnmatchcase(path, pattern) for pattern in rule.get('exclude', []))):
                if rule.get('full', False):
                    return None
                matched = True
                selected.update(rule.get('groups', ()))
        selected.update(python_control_inputs(config).get(path, {}))
        if any(fnmatch.fnmatchcase(path, pattern)
               for patterns in config.get('workspace_support_profile_paths', {}).values()
               for pattern in patterns):
            return None
        if not matched or selected - {'baseline', 'workflow_governance'}:
            return None
        return selected

    try:
        if any(mapped_groups(path) is None for path in paths if path != driver):
            return None
        additions = set()
        if not comparisons:
            return None
        for before, after in comparisons:
            old_mode, old_prefix, old, old_suffix = driver_parts(before)
            new_mode, new_prefix, new, new_suffix = driver_parts(after)
            if (old_mode, old_prefix, old_suffix) != (new_mode, new_prefix, new_suffix):
                return None
            # A subsequence preserves every existing registration and its order.
            pending = iter(old)
            expected = next(pending, None)
            for line in new:
                if line == expected:
                    expected = next(pending, None)
            if expected is not None:
                return None
            for line in new:
                if line not in old:
                    target = command.fullmatch(line).group(1)
                    regular_blob(after, target)
                    if mapped_groups(target) != {'workflow_governance'}:
                        return None
                    additions.add(target)
        return sorted(additions) or None
    except (OSError, subprocess.CalledProcessError, ValueError, KeyError, TypeError, UnicodeError):
        return None


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


def production_target_roots(sources, directories, manifests):
    """Actual lib/bin roots, independently of test harness and feature settings."""
    roots, explicit = set(), set()
    for package, manifest in manifests.items():
        if package not in directories:
            continue
        directory = directories[package]
        metadata = manifest.get('package', {})
        if 'lib' in manifest or metadata.get('autolib', True):
            root = posixpath.normpath(posixpath.join(directory,
                                     manifest.get('lib', {}).get('path', 'src/lib.rs')))
            roots.add(root)
            if 'path' in manifest.get('lib', {}):
                explicit.add(root)
        for target in manifest.get('bin', []):
            candidates = ([target['path']] if 'path' in target else
                          ['src/main.rs', 'src/bin/' + target['name'] + '.rs',
                           'src/bin/' + target['name'] + '/main.rs'])
            roots.update(posixpath.normpath(posixpath.join(directory, candidate)) for candidate in candidates)
            if 'path' in target:
                explicit.add(posixpath.normpath(posixpath.join(directory, target['path'])))
        if metadata.get('autobins', True):
            prefix = directory + '/' if directory else ''
            roots.update(candidate for candidate in sources if candidate.startswith(prefix) and re.fullmatch(
                r'src/(?:main\.rs|bin/[^/.][^/]*(?:\.rs|/main\.rs))', candidate[len(prefix):]))
    # An explicit non-.rs or otherwise unparsed root is still a possible
    # production reader. Its absence must not prove portable exclusion.
    return roots.intersection(sources) | explicit


def build_script_roots(sources, directories, manifests):
    roots = set()
    for package, manifest in manifests.items():
        if package not in directories:
            continue
        build = manifest.get('package', {}).get('build', 'build.rs')
        if build is False:
            continue
        root = posixpath.normpath(posixpath.join(directories[package],
                                 build if isinstance(build, str) else 'build.rs'))
        if root in sources or isinstance(manifest.get('package', {}).get('build'), str):
            roots.add(root)
    return roots


def default_feature_closure(manifest, *, initial=None):
    features = manifest.get('features', {})
    enabled, pending = set(), (list(initial) if initial is not None else
                               ['default'] if 'default' in features else [])
    while pending:
        name = pending.pop()
        if name in enabled or name not in features:
            continue
        enabled.add(name)
        values = features[name]
        if not isinstance(values, list) or any(not isinstance(value, str) for value in values):
            raise ValueError('invalid Cargo default feature closure')
        pending.extend(value for value in values if value in features)
    return enabled


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


class GitSourceCache:
    """One selection's immutable blob bytes and context-free Rust input scans.

    Paths, modes, manifests, generated-output permissions and consumer edges are
    still interpreted separately for every Git tree. No ref name or worktree
    content is a cache key, and no state survives a select() invocation.
    """

    def __init__(self, *, repo=None):
        self.repo = repo
        self.blobs = {}
        self.rust_inputs = {}

    def load(self, oids):
        missing = list(dict.fromkeys(oid for oid in oids if oid not in self.blobs))
        if not missing:
            return
        data = subprocess.check_output(['git', *(['-C', str(self.repo)] if self.repo is not None else []),
                                        'cat-file', '--batch'],
                                       input=b''.join(oid + b'\n' for oid in missing))
        fetched, offset = {}, 0
        for oid in missing:
            end = data.find(b'\n', offset)
            fields = data[offset:end].split() if end >= offset else []
            if (len(fields) != 3 or fields[:2] != [oid, b'blob']
                    or not fields[2].isdigit()):
                raise ValueError('invalid Git blob batch header')
            size = int(fields[2])
            start, offset = end + 1, end + 1 + size
            if data[offset:offset + 1] != b'\n':
                raise ValueError('incomplete Git blob batch body')
            fetched[oid] = data[start:offset]
            offset += 1
        if offset != len(data):
            raise ValueError('unexpected trailing Git blob batch data')
        self.blobs.update(fetched)

    def inputs(self, oid):
        if oid not in self.rust_inputs:
            literals, calls, uncertain = rust_source_inputs(
                self.blobs[oid].decode('utf-8', 'replace'), with_module_paths=True)
            self.rust_inputs[oid] = (tuple(literals),
                                    tuple(MappingProxyType(call) for call in calls), uncertain)
        return self.rust_inputs[oid]


def cargo_graph(ref, generated_inputs=(), *, source_cache=None, repo=None):
    """Resolve workspace path edges including dev/build/target and renamed dependencies.

    Optional dependencies are included conservatively, so no feature combination is lost.
    Unsupported/ambiguous workspace relationships widen selection rather than reject work.
    """
    records = git('ls-tree', '-r', '-z', ref, repo=repo).split(b'\0')
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
    source_cache = source_cache if source_cache is not None else GitSourceCache(repo=repo)
    source_cache.load(oid for oid, _ in entries)
    blob_oids = {path: oid for oid, path in entries}
    blobs = {path: source_cache.blobs[oid] for oid, path in entries}
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
    host_execution_targets = {name for name, (_, manifest) in manifests.items()
                              if manifest.get('lib', {}).get('proc-macro') is True
                              or manifest.get('lib', {}).get('proc_macro') is True
                              or 'proc-macro' in manifest.get('lib', {}).get('crate-type', [])
                              or 'proc-macro' in manifest.get('lib', {}).get('crate_type', [])}
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
                    if kind == 'build-dependencies':
                        host_execution_targets.add(target)
    # A library may explicitly import a nominally independent target source.
    # Widen the whole referenced directory, including implicit child modules.
    library_directories = set()
    library_readers = {}
    source_inputs = {}

    def inputs(path):
        if path not in source_inputs:
            try:
                source_inputs[path] = source_cache.inputs(blob_oids[path])
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
    data_readers, unresolved_data_readers = {}, set()
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
                data_readers.setdefault(candidate, set()).add(path)
            elif call['kind'] == 'manifest':
                for owner in owners:
                    directory = manifests[owner][0]
                    candidate = posixpath.normpath(directory + '/' + call['path'].lstrip('/'))
                    embedded.setdefault(candidate, set()).add(owner)
                    if owner in library_owners:
                        library_embeds.setdefault(candidate, set()).add(owner)
                    data_readers.setdefault(candidate, set()).add(path)
            else:
                for owner in owners:
                    # Each reader has its own OUT_DIR; one registration cannot
                    # exempt another owner or cancel a different unknown call.
                    known = (call['kind'] == 'generated'
                             and call['path'].startswith('/')
                             and call['path'][1:] in generated_outputs.get(owner, set()))
                    if not known:
                        unresolved_data_readers.add(path)
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
                        data_readers.setdefault(candidate, set()).add(path)
    embedded['__leaf_inputs__'] = leaf_inputs
    embedded['__library_directories__'] = library_directories
    embedded['__library_readers__'] = library_readers
    embedded['__library_embeds__'] = library_embeds
    # A native-only library can still run on the host as a code generator for
    # a portable consumer. Proc macros also execute on the host through normal
    # dependency edges. Preserve both kinds of transitive generator provenance.
    embedded['__host_execution_inputs__'] = ({package for package in manifests
        if reverse_consumers({package}, reverse) & host_execution_targets} if host_execution_targets else set())
    embedded['__rust_data_readers__'] = data_readers
    embedded['__unresolved_rust_data_readers__'] = unresolved_data_readers
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
        python_control = (exact_python_script(path)
                          and any(fnmatch.fnmatchcase(path, pattern) for pattern in CONTROL))
        if (not isinstance(path, str) or not (path.startswith('.github/') or python_control)
                or path.endswith('/') or posixpath.normpath(path) != path
                or any(character in path for character in ('\\', '*', '?', '[', ']'))
                or '..' in path.split('/') or any(ord(character) < 32 for character in path)
                or not isinstance(selected, list)
                or any(not isinstance(group, str) or group not in groups for group in selected)
                or len(selected) != len(set(selected))):
            raise ValueError('invalid scoped control path or groups')
    validate_python_control_contracts(config)
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
    if 'architecture_build_policy_version' in config:
        version = config['architecture_build_policy_version']
        if (type(version) is not int or version == ARCHITECTURE_BUILD_POLICY_VERSION
                and config.get('fast_execution_version') != FAST_EXECUTION_VERSION):
            raise ValueError('invalid architecture build policy capability')
    if 'workspace_support_partition_version' in config:
        if (type(config['workspace_support_partition_version']) is not int
                or config['workspace_support_partition_version'] != 1
                or config.get('fast_scope_version') != FAST_PLAN_VERSION):
            raise ValueError('invalid workspace support partition capability')
    if 'custody_execution_version' in config:
        if (type(config['custody_execution_version']) is not int
                or config['custody_execution_version'] != CUSTODY_EXECUTION_VERSION
                or type(config.get('workspace_support_partition_version')) is not int
                or config['workspace_support_partition_version'] != 1):
            raise ValueError('invalid custody execution capability')
    if 'ordinary_artifact_partition_version' in config:
        if (type(config['ordinary_artifact_partition_version']) is not int
                or config['ordinary_artifact_partition_version'] != 1
                or config.get('fast_scope_version') != FAST_PLAN_VERSION):
            raise ValueError('invalid ordinary artifact partition capability')
    if 'ordinary_partition_version' in config:
        if (type(config['ordinary_partition_version']) is not int
                or config['ordinary_partition_version'] != ORDINARY_PARTITION_VERSION
                or config.get('fast_execution_version') != FAST_EXECUTION_VERSION):
            raise ValueError('invalid ordinary partition capability')
    if 'distfs_clippy_contract_version' in config:
        if (type(config['distfs_clippy_contract_version']) is not int
                or config['distfs_clippy_contract_version'] != DISTFS_CLIPPY_CONTRACT_VERSION
                or config.get('fast_execution_version') != FAST_EXECUTION_VERSION
                or config.get('ordinary_partition_version') != ORDINARY_PARTITION_VERSION
                or config.get('architecture_build_policy_version') != ARCHITECTURE_BUILD_POLICY_VERSION):
            raise ValueError('invalid DistFS Clippy capability')
    validate_fast_test_inventory(config)
    validate_fast_support_contracts(config)
    return config


def exact_python_script(path):
    return (isinstance(path, str) and path.startswith('scripts/') and path.endswith('.py')
            and posixpath.normpath(path) == path and '..' not in path.split('/')
            and not any(character in path for character in ('\\', '*', '?', '[', ']'))
            and not any(ord(character) < 32 for character in path))


def validate_python_control_contracts(config):
    scoped = config.get('scoped_control_paths', {})
    python_paths = {path for path in scoped if path.endswith('.py')}
    fields = {'python_control_contract_version', 'python_control_contracts'}
    if not python_paths and not fields.intersection(config):
        return
    contracts = config.get('python_control_contracts')
    version = config.get('python_control_contract_version')
    if (type(version) is not int or version != PYTHON_CONTROL_CONTRACT_VERSION
            or 'workflow_governance' not in config['groups']
            or not isinstance(contracts, dict) or not contracts or set(contracts) != python_paths):
        raise ValueError('invalid Python control contract capability or inventory')
    for path, contract in contracts.items():
        if (not exact_python_script(path) or not isinstance(contract, dict)
                or set(contract) != {'group', 'tests'}
                or not isinstance(contract['group'], str)
                or contract['group'] not in scoped[path]
                or not isinstance(contract['tests'], list) or not contract['tests']
                or any(not exact_python_script(test) or not test.endswith('.test.py')
                       for test in contract['tests'])
                or len(set(contract['tests'])) != len(contract['tests'])):
            raise ValueError('invalid Python control contract path, group or tests')


def python_control_inputs(config):
    """A minimum test must protect its own edits as well as its source edits."""
    inputs = {}
    if config.get('python_control_contract_version') != PYTHON_CONTROL_CONTRACT_VERSION:
        return inputs
    for path, contract in config['python_control_contracts'].items():
        for entry in [path, *contract['tests']]:
            inputs.setdefault(entry, {}).setdefault(contract['group'], set()).update(contract['tests'])
    return inputs


def python_control_plan(config, paths=None):
    """Only the supplied trusted inventory can select immutable control tests."""
    inputs = python_control_inputs(config)
    if not inputs:
        return {}
    contracts = config['python_control_contracts']
    selected = sorted(contracts if paths is None else set(paths).intersection(inputs))
    tests = {}
    for path in selected:
        for group, contract_tests in inputs[path].items():
            tests.setdefault(group, set()).update(contract_tests)
    return {'python_control_contract_version': PYTHON_CONTROL_CONTRACT_VERSION,
            'python_control_paths': selected,
            'python_control_tests': {group: sorted(paths) for group, paths in sorted(tests.items())}}


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
    if not (fields | {'fast_execution_version', 'ordinary_fast_component_rules'}).intersection(config):
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
        if (not isinstance(rule, dict) or set(rule) - {'paths', 'targets', 'reason', 'added_declarations', 'static_module_additions'}
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
        if 'static_module_additions' in rule and (
                rule['static_module_additions'] is not True
                or config.get('fast_execution_version') != FAST_EXECUTION_VERSION
                or not any(targets[key]['target_kind'] == 'lib' for key in rule['targets'])):
            raise ValueError('static module additions require a complete component library target')
    component_rules = config.get('ordinary_fast_component_rules')
    if 'fast_execution_version' not in config:
        if component_rules is not None:
            raise ValueError('component rules require trusted fast execution capability')
        return
    if (type(config['fast_execution_version']) is not int
            or config['fast_execution_version'] != FAST_EXECUTION_VERSION
            or not isinstance(component_rules, list) or not component_rules):
        raise ValueError('invalid fast execution capability')
    for rule in component_rules:
        if (not isinstance(rule, dict) or set(rule) != {'match', 'exclude', 'targets', 'reason'}
                or not isinstance(rule['match'], list) or not rule['match']
                or not isinstance(rule['exclude'], list)
                or not isinstance(rule['targets'], list) or not rule['targets']
                or any(not isinstance(key, str) or key not in targets for key in rule['targets'])
                or len(rule['targets']) != len(set(rule['targets']))
                or not isinstance(rule['reason'], str) or not rule['reason']):
            raise ValueError('invalid fast component rule')
        for pattern in rule['match'] + rule['exclude']:
            if (not isinstance(pattern, str) or not pattern or posixpath.isabs(pattern)
                    or posixpath.normpath(pattern) != pattern or '..' in pattern.split('/')
                    or '\\' in pattern or any(ord(character) < 32 for character in pattern)):
                raise ValueError('invalid fast component pattern')


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


def added_static_module_sources(path, comparisons, *, native_root=False):
    """Return newly wired regular sources, or None for any wider token change.

    The native mode is only used after proving an actual selected Cargo lib
    root. It requires an exact exclusion gate on every new module; the existing
    static_module_additions contract continues to allow no attributes at all.
    """
    added_sources = set()
    native_gate = ('#', '[', 'cfg', '(', 'not', '(', 'target_arch', '=', '"wasm32"', ')', ')', ']')
    for before, after in comparisons:
        patch = git('diff', '--no-ext-diff', '--unified=0', before, after, '--', path).decode()
        added_lines, line = set(), 0
        for change in patch.splitlines():
            if change.startswith(('---', '+++')):
                continue
            hunk = re.match(r'@@ -\d+(?:,\d+)? \+(\d+)(?:,\d+)? @@', change)
            if hunk:
                line = int(hunk[1])
            elif change.startswith('-') and change[1:].strip():
                return None
            elif change.startswith('+'):
                added_lines.add(line)
                line += 1
            elif change.startswith(' '):
                line += 1
        try:
            source = git('show', after + ':' + path).decode()
            tokens, closing = rust_source_inputs(source, return_tokens=True)
            previous, _ = rust_source_inputs(git('show', before + ':' + path).decode(), return_tokens=True)
        except (subprocess.CalledProcessError, UnicodeDecodeError, ValueError):
            return None
        starts = [0, *(match.end() for match in re.finditer('\n', source))]
        additions = {index for index, (_, _, position) in enumerate(tokens)
                     if bisect_right(starts, position) in added_lines}
        inserted = set(additions)
        modules, index = [], 0
        while index < len(tokens):
            cursor = index
            # An existing attribute must not become attached to the new item.
            boundary = index == 0 or tokens[index - 1][1] in (';', '}')
            if native_root:
                boundary &= tuple(value for _, value, _ in tokens[cursor:cursor + len(native_gate)]) == native_gate
                cursor += len(native_gate)
            if boundary and cursor < len(tokens) and tokens[cursor][1] == 'pub':
                cursor += 1
                if cursor + 2 < len(tokens) and tokens[cursor][1] == '(':
                    if (tokens[cursor + 1][1] not in ('crate', 'self', 'super')
                            or tokens[cursor + 2][1] != ')'):
                        boundary = False
                    cursor += 3
            if (boundary and cursor + 2 < len(tokens) and tokens[cursor][1] == 'mod'
                    and tokens[cursor + 1][0] == 'identifier' and tokens[cursor + 2][1] == ';'
                    and set(range(index, cursor + 3)).issubset(additions)):
                modules.append(tokens[cursor + 1][1].removeprefix('r#'))
                additions.difference_update(range(index, cursor + 3))
                index = cursor + 3
            else:
                # Skip complete nested bodies, attributes and macro arguments.
                index = closing.get(index, index) + 1
        if (additions or [(kind, value) for index, (kind, value, _) in enumerate(tokens) if index not in inserted]
                != [(kind, value) for kind, value, _ in previous]):
            return None
        directory = (posixpath.dirname(path) if native_root or posixpath.basename(path) in ('lib.rs', 'main.rs', 'mod.rs')
                     else path.removesuffix('.rs'))
        for name in modules:
            candidates = [directory + '/' + name + '.rs', directory + '/' + name + '/mod.rs']
            if git('ls-tree', '-z', before, '--', *candidates):
                return None  # Mounting old untested source needs the original expanded checks.
            entries = [entry for entry in git('ls-tree', '-z', after, '--', *candidates).split(b'\0') if entry]
            if len(entries) != 1 or entries[0].split(b'\t', 1)[0].split()[:2] != [b'100644', b'blob']:
                return None
            added_sources.add(entries[0].split(b'\t', 1)[1].decode())
    return added_sources


def only_static_modules_added(path, comparisons):
    """Permit new root-level modules whose regular source is new in each diff."""
    return added_static_module_sources(path, comparisons) is not None


def validate_fast_support_contracts(config):
    fields = {'ordinary_support_contract_version', 'ordinary_fast_support_contracts'}
    if not fields.intersection(config):
        return
    contracts = config.get('ordinary_fast_support_contracts')
    if (type(config.get('ordinary_support_contract_version')) is not int
            or config['ordinary_support_contract_version'] != 1
            or config.get('fast_execution_version') != FAST_EXECUTION_VERSION
            or not isinstance(contracts, list) or not contracts):
        raise ValueError('invalid ordinary support contract capability')
    mappings = {path: rule for rule in config['ordinary_fast_rules'] for path in rule['paths']}
    owners = set()
    for contract in contracts:
        if not isinstance(contract, dict) or set(contract) != {
                'package', 'source', 'entry', 'callers', 'tests', 'deferred_consumers'}:
            raise ValueError('invalid ordinary support contract')
        owner = contract['package']
        if (not isinstance(owner, str) or owner not in SUPPORT_PROFILES or owner in owners
                or 'workspace_support' not in config['package_groups'].get(owner, ())
                or not isinstance(contract['source'], str)
                or not re.fullmatch(re.escape('crates/' + owner + '/src/') + r'[a-z_]+\.rs', contract['source'])
                or not isinstance(contract['entry'], str)
                or not re.fullmatch(r'[a-z_][a-z0-9_]*', contract['entry'])):
            raise ValueError('invalid ordinary support adapter identity')
        owners.add(owner)
        callers = contract['callers']
        if (not isinstance(callers, dict) or set(callers) != {
                f'crates/{owner}/src/commands.rs', f'crates/{owner}/src/bin/{owner}.rs'}
                or any(not isinstance(value, str) or not re.fullmatch(r'[0-9a-f]{64}', value)
                       for value in callers.values())):
            raise ValueError('support adapter requires reviewed dispatch and executable callers')
        tests, consumers = contract['tests'], contract['deferred_consumers']
        if (not isinstance(tests, list) or not tests
                or any(not isinstance(path, str) or not re.fullmatch(
                    re.escape('crates/' + owner + '/tests/') + r'[a-z_]+\.rs', path) for path in tests)
                or len(tests) != len(set(tests))
                or not isinstance(consumers, list)
                or any(not isinstance(name, str) or name not in SUPPORT_PROFILES or name == owner
                       or 'workspace_support' not in config['package_groups'].get(name, ()) for name in consumers)
                or len(consumers) != len(set(consumers))):
            raise ValueError('invalid ordinary support input or consumer inventory')
        for path in [contract['source'], *tests]:
            rule = mappings.get(path)
            if rule is None or 'added_declarations' in rule or rule.get('static_module_additions'):
                raise ValueError('support adapter requires an exact ordinary target contract')
            targets = [config['ordinary_fast_targets'][key] for key in rule['targets']]
            if (any(target['package'] != owner or target['default_features'] is not True
                    or target['features'] for target in targets)
                    or not any(target['target_kind'] == 'lib' for target in targets)
                    or not {posixpath.basename(test)[:-3] for test in tests} <= {
                        target['target_name'] for target in targets if target['target_kind'] == 'test'}):
                raise ValueError('support adapter must retain its default library and complete CLI targets')


def private_adapter_boundary(contract, comparisons, source_inputs):
    """Only the body of the audited private entry may change.

    Rust impls inside a function still affect the crate. Unknown macros, new
    items, attributes and unsafe/linkage cannot borrow this leaf's contract.
    This deliberately bounded proof does not attempt general Rust semantics.
    """
    path, entry = contract['source'], contract['entry']
    parent = posixpath.dirname(path) + '/lib.rs'
    module = posixpath.basename(path)[:-3]
    parsed_cache = {}

    def parsed_source(contents):
        if contents not in parsed_cache:
            tokens, closing = rust_source_inputs(contents.decode(), return_tokens=True)
            values = [value.removeprefix('r#') if kind == 'identifier' else value
                      for kind, value, _ in tokens]
            parsed_cache[contents] = tokens, closing, values
        return parsed_cache[contents]

    def macro_scopes(tokens, closing, values):
        ranges = []
        for index, value in enumerate(values[:-1]):
            if value != '!':
                continue
            opening = index + 1
            name = values[index - 1] if index else ''
            if name == 'macro_rules':
                opening += 1
            if opening < len(tokens) and values[opening] in ('(', '[', '{'):
                ranges.append((name, opening, closing.get(opening, len(tokens))))
        return ranges

    def boundary(ref):
        inputs = source_inputs.get(resolve_commit(ref), {})
        sources = inputs.get('sources', {})
        if (not sources or inputs.get('uncertain_modules')
                or any(caller not in sources or hashlib.sha256(sources[caller]).hexdigest() != digest
                       for caller, digest in contract['callers'].items())):
            return None
        owner = contract['package']
        consumers = reverse_consumers({owner}, inputs.get('reverse', {})) | {owner}
        if consumers.intersection(inputs.get('unresolved_includes', ())):
            return None
        local_macros = {package for package, manifest in inputs.get('manifests', {}).items()
                        if manifest.get('lib', {}).get('proc-macro') is True}
        if consumers.intersection(reverse_consumers(local_macros, inputs.get('reverse', {}))):
            return None  # Unreviewed local token generators can synthesize hidden callers.
        directories = [inputs['directories'][package] + '/' for package in consumers
                       if package in inputs.get('directories', {})]
        workspace_directories = [directory + '/' for directory in inputs.get('directories', {}).values() if directory]
        included = set(inputs.get('includes', ()))
        assertions = {'assert', 'assert_eq', 'assert_ne', 'debug_assert', 'debug_assert_eq', 'debug_assert_ne'}
        # Reject new wrappers, re-exports and aliased imports in any real reverse
        # consumer, including ones already present in the target branch history.
        # The only audited public dispatch and executable stay byte-pinned until
        # their topology is explicitly reviewed and re-enrolled in trusted data.
        private_references = {}
        for candidate, contents in sources.items():
            if not any(candidate.startswith(directory) for directory in workspace_directories) and candidate not in included:
                continue
            # Ordinary local assertions below are known expression macros, not
            # arbitrary module forwarding. A local override invalidates that proof.
            if b'macro_use' in contents or b'assert' in contents or b'macro_rules' in contents:
                assertion_tokens, assertion_closing, assertion_values = parsed_source(contents)
                scopes = macro_scopes(assertion_tokens, assertion_closing, assertion_values)
                in_use = False
                for index, value in enumerate(assertion_values):
                    if value == 'use':
                        in_use = True
                    elif value == ';':
                        in_use = False
                    if (value == 'macro_use' or (in_use and value in assertions)
                            or (assertion_values[index:index + 2] == ['macro_rules', '!']
                                and (index + 2 >= len(assertion_values)
                                     or assertion_tokens[index + 2][0] != 'identifier'
                                     or assertion_values[index + 2] in assertions))
                            or (value in assertions and assertion_values[index + 1:index + 2] != ['!']
                                and any(name not in assertions and start < index < end
                                        for name, start, end in scopes))):
                        return None
            if module.encode() not in contents and b'commands' not in contents:
                continue
            parsed, closing, candidate_values = parsed_source(contents)
            count = candidate_values.count(module)
            if count:
                private_references[candidate] = count
            if candidate in contract['callers']:
                continue
            macro_ranges = macro_scopes(parsed, closing, candidate_values)
            local_commands = any(candidate_values[index:index + 2] == ['mod', 'commands']
                                 for index in range(len(candidate_values) - 2))
            reverse_consumer = any(candidate.startswith(directory) for directory in directories)
            in_use = False
            for index, value in enumerate(candidate_values):
                if value == 'use':
                    in_use = True
                elif value == ';':
                    in_use = False
                qualified = (candidate_values[index + 1:index + 3] == [':', ':']
                             or candidate_values[max(0, index - 2):index] == [':', ':'])
                # A macro crate need not depend on the crate named by its
                # expansion. Inspect every workspace source, while recognizing
                # unrelated, explicitly local commands modules outside consumers.
                unrelated_local = (not reverse_consumer and local_commands
                                   and candidate_values[index + 1:index + 3] == [':', ':']
                                   and index + 3 < len(parsed) and parsed[index + 3][0] == 'identifier'
                                   and candidate_values[index + 3:index + 4] != ['run']
                                   and candidate_values[max(0, index - 2):index] != [':', ':'])
                enclosing = [name for name, start, end in macro_ranges if start < index < end]
                # Standard assertions consume expressions (including indexed
                # values and struct fields), never a bare module parameter.
                # Qualified module calls are still rejected independently.
                assertion_value = enclosing and all(name in assertions for name in enclosing)
                if value == 'commands' and (in_use or (qualified and not unrelated_local)
                                             or (enclosing and not assertion_value)):
                    return None
        dispatch = f'crates/{owner}/src/commands.rs'
        if private_references != {parent: 1, dispatch: 1}:
            return None
        tokens, closing, values = parsed_source(sources[path])
        if (set(values).intersection({'#', 'impl', 'trait', 'mod', 'extern', 'unsafe',
                                     'macro_rules', 'macro', 'static', 'const', 'union'})
                or any(value == '!' and values[index + 1] in ('(', '[', '{')
                       for index, value in enumerate(values[:-1]))):
            return None
        functions = [i for i, value in enumerate(values) if value == 'fn']
        if len(functions) != 1:
            return None
        index = functions[0]
        if values[index - 4:index + 2] != ['pub', '(', 'crate', ')', 'fn', entry]:
            return None
        imports = values[:index - 4]
        while imports:
            if imports[0] != 'use' or ';' not in imports:
                return None
            imports = imports[imports.index(';') + 1:]
        opening = values.index('{', index)
        if closing.get(opening) != len(values) - 1:
            return None
        _, _, parent_values = parsed_source(sources[parent])
        mounts = [i for i in range(len(parent_values) - 2)
                  if parent_values[i:i + 3] == ['mod', module, ';']]
        if len(mounts) != 1 or parent_values[mounts[0] - 7:mounts[0]] != [
                '#', '[', 'cfg', '(', 'unix', ')', ']']:
            return None
        return values[:opening + 1]

    try:
        for before, after in comparisons:
            original, candidate = boundary(before), boundary(after)
            if original is None or original != candidate:
                return False
    except (ValueError, KeyError, UnicodeError, subprocess.CalledProcessError):
        return False
    return True


def ordinary_fast_selection(config, paths, path_affected, path_rules, selected_core, comparisons, source_inputs=None):
    """Every affected native/Formal input needs a bounded, trusted contract."""
    if config.get('fast_test_inventory_version') != FAST_TEST_INVENTORY_VERSION:
        return None
    mappings = {path: rule for rule in config['ordinary_fast_rules'] for path in rule['paths']}
    component_rules = (config.get('ordinary_fast_component_rules', [])
                       if config.get('fast_execution_version') == FAST_EXECUTION_VERSION else [])
    support = config.get('ordinary_fast_support_contracts', [])
    support_owners = {contract['package'] for contract in support}
    for contract in support:
        if (set(paths).intersection([contract['source'], *contract['tests']])
                and not private_adapter_boundary(contract, comparisons, source_inputs or {})):
            return None
    selected, covered, reasons, path_features = set(), set(), [], {}
    for path in paths:
        groups = set(path_rules.get(path, ()))
        groups.update(group for package in path_affected.get(path, ())
                      for group in config['package_groups'].get(package, ()))
        if (not groups.intersection({'oasis7_required', 'formal_process'})
                and not support_owners.intersection(path_affected.get(path, ()))):
            continue
        rule = mappings.get(path)
        wiring = rule is not None and ('added_declarations' in rule or rule.get('static_module_additions'))
        if wiring:
            registered = ('added_declarations' in rule
                          and only_registered_declarations_added(path, comparisons, rule['added_declarations']))
            static = rule.get('static_module_additions') and only_static_modules_added(path, comparisons)
            if not registered and not static:
                return None  # A wider component cannot bypass an API-wiring contract.
        matches = ([rule] if rule is not None else []) + [candidate for candidate in component_rules
                   if path.endswith('.rs')
                   and any(fnmatch.fnmatchcase(path, pattern) for pattern in candidate['match'])
                   and not any(fnmatch.fnmatchcase(path, pattern) for pattern in candidate['exclude'])]
        if not matches:
            return None
        path_targets = {key for candidate in matches for key in candidate['targets']}
        selected.update(path_targets)
        covered.add(path)
        # Exact additive wiring does not edit unrelated gated bodies elsewhere
        # in a root file. Keep its existing declaration-only proof intact.
        if not wiring:
            path_features[path] = set.intersection(*(set(config['ordinary_fast_targets'][key]['features'])
                                                    for key in path_targets))
        reasons.extend('fast target contract ' + candidate['reason'] + ': ' + path for candidate in matches)
    if not selected:
        return None
    targets = [config['ordinary_fast_targets'][key] for key in sorted(selected)]
    if not {target['package'] for target in targets}.issubset(selected_core):
        return None
    return targets, covered, reasons, path_features


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
    for binary, suite in inventory.get('environment_formal_batches', {}).items():
        if suite in suites:
            explicit = {entry.get('name') for entries in inventory['suites'].values()
                        for entry in entries if entry.get('binary') == binary}
            destinations = {shard for name, shard in overrides.get(binary, {}).items() if name not in explicit}
            if destinations - set(FORMAL_SUITES):
                raise ValueError('unknown trusted formal shard')
            result.update(destinations)
    return sorted(result)


# Successful wasm compile profiles already executed by main30ff verify.sh.
# Each tuple is a separate --no-default-features invocation, never a union.
WORKER_WASM_COMPILE_FEATURES = (
    ('identity',), ('owner',), ('executor',), ('recovery-fence',),
    ('owner', 'approval-harness'), ('identity', 'native-runtime-harness'),
)


WORKER_SIGNER_FEATURE = 'execution-signer-harness'
WORKER_SIGNER_GUARD_MESSAGE = 'execution-signer-harness requires executor and is not a deployment profile'


def worker_local_feature_closure(manifest, initial):
    """Reuse the existing local fixed point, without injecting defaults.

    Dependency forwarding cannot be guessed into a local cfg proof. A known
    non-optional dependency's foreign feature has no local feature edge; optional
    forwarding/implicit features require a richer Cargo model and stay rejected.
    """
    features = manifest.get('features', {})
    if not isinstance(features, dict) or any(name not in features for name in initial):
        raise TestPolicyError('Worker compile profile feature is not declared')
    try:
        enabled = default_feature_closure(manifest, initial=initial)
    except (ValueError, TypeError):
        raise TestPolicyError('invalid Worker local feature closure') from None
    dependencies = {}
    tables = [manifest]
    targets = manifest.get('target', {})
    if not isinstance(targets, dict):
        raise TestPolicyError('unknown Worker target dependency closure')
    tables.extend(targets.values())
    for table in tables:
        if not isinstance(table, dict):
            raise TestPolicyError('unknown Worker dependency closure')
        # Build/dev edges do not supply production package feature references.
        entries = table.get('dependencies', {})
        if not isinstance(entries, dict):
            raise TestPolicyError('unknown Worker dependency closure')
        for name, declaration in entries.items():
            if not isinstance(declaration, (str, dict)):
                raise TestPolicyError('unknown Worker dependency declaration')
            optional = isinstance(declaration, dict) and declaration.get('optional', False)
            if type(optional) is not bool:
                raise TestPolicyError('unknown Worker dependency optionality')
            dependencies.setdefault(name, set()).add(optional)
    for name in enabled:
        for edge in features[name]:
            if edge in features:
                continue
            if edge.startswith('dep:'):
                if dependencies.get(edge[4:]) != {True}:
                    raise TestPolicyError('unmodeled Worker namespaced dependency feature')
                continue  # dep: enables the dependency, not a local feature.
            if '/' in edge:
                dependency, foreign = edge.split('/', 1)
                if (not foreign or '/' in foreign or dependency.endswith('?')
                        or dependencies.get(dependency) != {False}):
                    raise TestPolicyError('unmodeled Worker optional dependency feature forwarding')
                continue  # Known non-optional foreign feature: no local cfg edge.
            raise TestPolicyError('unmodeled Worker implicit local feature')
    return enabled


def worker_signer_profile_enabled(manifest):
    features = manifest.get('features', {})
    if WORKER_SIGNER_FEATURE not in features:
        return False
    defaults = (worker_local_feature_closure(manifest, ('default',))
                if 'default' in features else set())
    if features[WORKER_SIGNER_FEATURE] != [] or WORKER_SIGNER_FEATURE in defaults:
        raise TestPolicyError('Worker signer compile feature must be empty and default-off')
    if any(WORKER_SIGNER_FEATURE in worker_local_feature_closure(manifest, profile)
           for profile in WORKER_WASM_COMPILE_FEATURES):
        raise TestPolicyError('Worker signer compile feature requires explicit opt-in')
    return True


def worker_wasm_compile_features(manifest):
    profiles = list(WORKER_WASM_COMPILE_FEATURES)
    if worker_signer_profile_enabled(manifest):
        profiles.append(('executor', WORKER_SIGNER_FEATURE))
    return profiles


def worker_wasm_compile_checks(manifest):
    def command(features):
        return ['env', '-u', 'RUSTC_WRAPPER', 'cargo', 'check', '-p',
                'oasis7_custody_worker', '--target', 'wasm32-unknown-unknown',
                '--no-default-features', '--features', ','.join(features), '--locked']
    checks = [(command(features), None) for features in worker_wasm_compile_features(manifest)]
    if worker_signer_profile_enabled(manifest):
        checks.extend((command((role, WORKER_SIGNER_FEATURE)), WORKER_SIGNER_GUARD_MESSAGE)
                      for role in ('identity', 'owner', 'recovery-fence'))
    return checks


def run_worker_wasm_compile_checks(manifest, runner=subprocess.run):
    for profile in worker_wasm_compile_features(manifest):
        worker_local_feature_closure(manifest, profile)
    for command, expected_error in worker_wasm_compile_checks(manifest):
        print('+ ' + ' '.join(command), flush=True)
        if expected_error is None:
            runner(command, check=True)
        else:
            result = runner(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
            print(result.stdout, end='')
            if result.returncode == 0 or expected_error not in result.stdout:
                raise ValueError('Worker signer negative compile contract failed')


def worker_exact_signer_guard(source, attribute, owner):
    # Only this literal compile_error receives failed-build evidence. Nothing
    # behind a rejected profile is treated as compiled production Rust.
    if owner[0] != 'unresolved':
        return False
    guard = ('#[cfg(all(feature="execution-signer-harness",not(feature="executor")))] '
             'compile_error!("' + WORKER_SIGNER_GUARD_MESSAGE + '");')
    expected = [token[1] for token in rust_source_inputs(guard, return_tokens=True)[0]]
    values = [token[1] for token in rust_source_inputs(source, return_tokens=True)[0]]
    canonical_attribute = tuple(expected[2:expected.index(']')])
    if attribute != canonical_attribute:
        return False
    occurrences = [i for i in range(len(values))
                   if tuple(values[i:i + len(attribute)]) == attribute]
    if len(occurrences) != 1:
        return False
    # The current attribute starts after exactly '# ['. Bind the complete item
    # at that position; a legitimate guard elsewhere is never evidence here.
    start = occurrences[0] - 2
    return start >= 0 and values[start:start + len(expected)] == expected


def changed_test_policy(base, source, paths, feature_owners, feature_packages, available_features,
                        ordinary_roots=(), source_inputs=None, narrow_features_by_path=None,
                        narrow_unavailable=None, selected_targets=None, required_source_paths=(),
                        portable_paths=None, worker_wasm_compile=False, native_root_deltas=None,
                        pixel_platform_driver=False):
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
        lexical_annotations = []
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
        function_bodies, function_starts = [], {}
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
                function_starts[cursor] = start
        statement_owners, statement_occurrences, statement_context = {}, {}, {}
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
            if function in ordinary_context:
                statement_context[statement_owners[start]] = (function, function_starts[left], start)

        # An unresolved field/statement inside an unchanged method must not get
        # a new identity merely because an earlier module or item was inserted.
        # Bind it to the complete function tokens, lexical owner and relative
        # position. Any body/initializer/scope edit still requires expanded
        # coverage; this does not prove that an unknown gate is executable.
        reverse_closing = {end: start for start, end in closing.items()}

        def declaration_start(start):
            while start:
                previous = tokens[start - 1][1]
                if previous in ('pub', 'async', 'const', 'unsafe', 'extern', 'default'):
                    start -= 1
                elif (tokens[start - 1][0] == 'string' and start > 1
                      and tokens[start - 2][1] == 'extern'):
                    start -= 2
                elif previous in (']', ')') and start - 1 in reverse_closing:
                    opening = reverse_closing[start - 1]
                    prefix = '#' if previous == ']' else 'pub'
                    if opening == 0 or tokens[opening - 1][1] != prefix:
                        break
                    start = opening - 1
                else:
                    break
            return start

        plain_impls, impl_occurrences = {}, {}
        for start, owner in item_owners.items():
            if owner[0] != 'impl':
                continue
            end = start + 1
            while end < len(tokens) and tokens[end][1] not in ('{', ';'):
                end += 1
            header = tokens[start + 1:end]
            if (end not in closing or tokens[end][1] != '{'
                    or len(header) % 3 != 1
                    or any(kind != 'identifier' if offset % 3 == 0 else value != ':'
                           for offset, (kind, value, _) in enumerate(header))):
                continue  # Generic/trait/opaque impl contexts stay conservative.
            key = (module_path(start), tuple(value for _, value, _ in tokens[declaration_start(start):end]))
            occurrence = impl_occurrences.get(key, 0)
            impl_occurrences[key] = occurrence + 1
            plain_impls[end] = key, occurrence

        stable_functions, function_occurrences = {}, {}
        for left, right, _ in function_bodies:
            start = function_starts[left]
            parents = [opening for opening, stop in closing.items()
                       if opening < start < right < stop and opening not in module_braces]
            if len(parents) > 1 or parents and parents[0] not in plain_impls:
                continue  # Nested functions, traits and macro token trees.
            start = declaration_start(start)
            modules_here = tuple(item_owners[opening - 2] for opening, stop, _ in modules
                                 if opening < start < stop)
            context = (modules_here, plain_impls.get(parents[0]) if parents else None,
                       tuple(value for _, value, _ in tokens[start:left]))
            occurrence = function_occurrences.get(context, 0)
            function_occurrences[context] = occurrence + 1
            identity = hashlib.sha256(json.dumps(
                (context, occurrence, [value for _, value, _ in tokens[left:right + 1]]),
                ensure_ascii=True, separators=(',', ':')).encode()).hexdigest()
            stable_functions[left] = start, right, identity

        macro_ranges = [(opening, stop) for opening, stop in closing.items()
                        if (opening and tokens[opening - 1][1] == '!')
                        or (opening > 1 and tokens[opening - 2][1] == '!'
                            and tokens[opening - 1][0] == 'identifier')]
        statement_context = {owner: context for owner, context in statement_context.items()
                             if not any(left < context[2] < right for left, right in macro_ranges)}

        def enclosing_function_owner(index, fallback):
            bodies = [left for left, right, _ in function_bodies if left < index < right]
            if not bodies or any(left < index < right for left, right in macro_ranges):
                return fallback
            scope = stable_functions.get(max(bodies))
            if scope is None:
                return fallback
            start, _, identity = scope
            return 'body', identity, index - start

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
                owner = statement_owners.get(target, enclosing_function_owner(index, owner))
            annotations.add((values, owner))
            lexical_annotations.append((values, owner, index, cursor, inner))
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
                tokens, closing, item_owners, modules, ordinary_context, lexical_annotations, statement_context)

    def lexical_attributes(info, position):
        # Keep stable named/occurrence owners for edit comparison. Compilation
        # additionally needs the actual module instance, not a same-name prefix.
        tokens, closing, modules = info[7], info[8], info[10]
        scoped = []
        for attribute, owner, index, target, inner in info[12]:
            if inner:
                containers = [(left, right) for left, right in closing.items()
                              if tokens[left][1] == '{' and left < index < right]
                left, right = min(containers, key=lambda pair: pair[1] - pair[0]) if containers else (-1, len(tokens))
                applies = left < position < right
            else:
                applies = target == position or any(
                    left == target + 2 and left < position < right
                    for left, right, _ in modules)
            if applies:
                scoped.append((attribute, owner))
        return scoped

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

    def gate_value(attribute, native_linux, enabled=None, wasm_compile=False, test_build=True):
        # Accept only a known expression that is actually true in the expanded
        # driver feature and native target combination. Mentioning an allowed
        # feature is insufficient: all(feature="test_tier_full", any()) is false.
        if len(attribute) < 4 or attribute[0:2] != ('cfg', '(') or attribute[-1] != ')':
            return None
        values = attribute[2:-1]
        actual_features = enabled_features if enabled is None else enabled

        def expression(cursor):
            if cursor >= len(values):
                raise ValueError('missing cfg predicate')
            value = values[cursor]
            if value == 'test':
                return test_build and not wasm_compile, cursor + 1
            if wasm_compile and value in ('unix', 'windows'):
                return False, cursor + 1
            if native_linux and value in ('unix', 'windows'):
                return value == 'unix', cursor + 1
            if value == 'feature' and values[cursor + 1:cursor + 2] == ('=',):
                if cursor + 2 >= len(values):
                    raise ValueError('missing cfg feature')
                name = values[cursor + 2].strip('"')
                if wasm_compile and name in available_features.get('oasis7_custody_worker', ()):
                    return name in actual_features, cursor + 3
                return (True if name in actual_features else None), cursor + 3
            native_values = {'target_os': 'linux', 'target_family': 'unix',
                             'target_arch': 'x86_64', 'target_pointer_width': '64',
                             'target_endian': 'little', 'target_env': 'gnu'}
            if wasm_compile:
                native_values = {'target_os': 'unknown', 'target_family': 'wasm',
                                 'target_arch': 'wasm32', 'target_pointer_width': '32',
                                 'target_endian': 'little', 'target_env': ''}
            if (native_linux or wasm_compile) and value in native_values and values[cursor + 1:cursor + 2] == ('=',):
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
            return enabled if end == len(values) else None
        except ValueError:
            return None

    def enabled_gate(attribute, native_linux, enabled=None):
        return gate_value(attribute, native_linux, enabled) is True

    def source_at(ref, path):
        try:
            return git('show', f'{ref}:{path}').decode('utf-8', 'replace')
        except subprocess.CalledProcessError:
            return ''  # Both rename endpoints and additions/deletions are present.

    source_inputs = source_inputs or {}
    sources = source_inputs.get('sources', {})
    directories = source_inputs.get('directories', {})
    production_roots = production_target_roots(sources, directories, source_inputs.get('manifests', {}))
    host_roots = build_script_roots(sources, directories, source_inputs.get('manifests', {}))
    parsed_sources, parent_cache, context_cache, reference_cache = {}, {}, {}, {}
    ordinary_structure_cache = {}

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
        references, provenance = [], []
        for parent in candidates:
            info = parsed(parent, native)
            annotations, tokens, closing, items, modules, context = info[6:12]
            directory = posixpath.dirname(parent)
            module_directory = (directory if parent in ordinary_roots or parent in production_roots or parent in host_roots
                                or posixpath.basename(parent) in ('lib.rs', 'main.rs', 'mod.rs')
                                or parent in source_inputs.get('includes', ())
                                else parent.removesuffix('.rs'))
            for index, owner in items.items():
                kind = tokens[index][1]
                inline = tuple(name for start, end, name in modules if start < index < end)
                scoped = lexical_attributes(info, index)
                attributes_here = [attribute for attribute, _ in scoped]
                known = owner in context and (selected_features is not None or source_owners(parent) == owners)
                opaque = any(
                    attribute[0] not in ('cfg', 'cfg_attr', 'path') and not ordinary_attribute(attribute, native)
                    for attribute in attributes_here)
                targets, resolved = [], True
                if kind == 'mod' and tokens[index + 2][1] == ';':
                    name = tokens[index + 1][1].removeprefix('r#')
                    inherited_path = any(attribute[0] in ('path', 'cfg_attr') and 'path' in attribute
                                         for attribute, target in scoped
                                         if target != owner)
                    path_attrs = [attribute for attribute, target in annotations
                                  if target == owner and attribute[0] == 'path']
                    conditional_paths = [attribute for attribute, target in annotations
                                         if target == owner and attribute[0] == 'cfg_attr' and 'path' in attribute]
                    if conditional_paths:
                        # Neither an alternate path nor its inherited cfg may
                        # be inferred from whichever filename happens to exist.
                        targets.append(path)
                        resolved = False
                        known = False
                        opaque = True
                    if path_attrs:
                        if (len(path_attrs) == 1 and len(path_attrs[0]) == 3
                                and rust_literal_path(path_attrs[0][2]) is not None and not inline):
                            targets.append(posixpath.normpath(posixpath.join(directory, rust_literal_path(path_attrs[0][2]))))
                        else:
                            # A possible nonliteral parent cannot disappear
                            # just because another parent is easy to resolve.
                            targets.append(path)
                            resolved = False
                            known = False
                            # Escaped literal/inline placement remains expanded-only;
                            # the complete inherited gate chain is still checked.
                            opaque |= not (len(path_attrs) == 1 and len(path_attrs[0]) == 3
                                           and path_attrs[0][2].startswith(('"', 'r')))
                    else:
                        prefix = posixpath.join(module_directory, *inline, name)
                        targets.extend(target for target in (prefix + '.rs', prefix + '/mod.rs') if target in sources)
                        resolved &= len(targets) == 1
                        known &= len(targets) == 1
                        opaque |= len(targets) != 1 and not inherited_path
                    if inherited_path:
                        targets.append(path)
                        resolved = False
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
                        resolved = False
                        known = False
                        opaque = True
                if path in targets and not (parent == path and not known and not opaque):
                    references.append((parent, kind, known and all(
                        ordinary_attribute(attribute, native)
                        or (selected_features is not None and enabled_gate(attribute, native, selected_features))
                        for attribute in attributes_here), attributes_here))
                    provenance.append((parent, known, opaque, resolved, tuple(
                        attribute for attribute in attributes_here if attribute[0] in ('cfg', 'cfg_attr'))))
        reference_cache[key] = provenance
        parent_cache[key] = references
        return references

    def ordinary_source_structure(path, native, visiting=frozenset()):
        key = path, native
        if key in ordinary_structure_cache:
            return ordinary_structure_cache[key]
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
        enabled &= all(known and resolved and not opaque
                       and ordinary_source_structure(parent, native, visiting | {path})
                       for parent, known, opaque, resolved, _ in reference_cache[(path, native, None)])
        ordinary_structure_cache[key] = enabled
        return enabled

    def inherited_paths(path, native, visiting=frozenset(), roots=None):
        # AND within a concrete Cargo-root path, never across alternative mounts.
        # Opaque references/cycles cannot be hidden by a second convenient path.
        if path in visiting:
            return [((), False, True, frozenset())]
        result = []
        if path in (ordinary_roots if roots is None else roots):
            result.append(((), True, False, frozenset(source_owners(path))))
        if path not in sources:
            return result
        parents(path, native)  # Fill the same exact-reference provenance cache.
        for parent, known, opaque, resolved, gates in reference_cache[(path, native, None)]:
            if parent in visiting and not resolved:
                continue
            for previous, proven, uncertain, packages in inherited_paths(parent, native, visiting | {path}, roots):
                result.append((previous + gates, proven and known, uncertain or opaque, packages))
        return result

    def ordinary_source_context(path, native):
        key = path, native
        if key in context_cache:
            return context_cache[key]
        context_cache[key] = False
        # Keep every reference closed and unambiguous before evaluating cfgs.
        # A convenient executable route cannot hide a different unknown owner,
        # include context, disabled harness or unresolved/cyclic mount.
        if not ordinary_source_structure(path, native):
            return False
        defaults = source_inputs.get('default_features', {})
        executable = False
        for gates, proven, opaque, packages in inherited_paths(path, native):
            features = set.intersection(*(set(defaults.get(package, ())) for package in packages)) if packages else set()
            values = [gate_value(gate, native, features) for gate in gates]
            if not proven or opaque or any(value is None for value in values):
                return False
            # A production-only mount is a false alternative in a test build,
            # not a condition on a separate real test mount. Unknown or expanded
            # features above still prevent the ordinary-addition exemption.
            executable |= all(values)
        context_cache[key] = executable
        return executable

    def inherited_gates(path, native, visiting=frozenset()):
        # Narrow inventory keeps its conservative ancestor check; the legacy
        # policy below evaluates concrete alternative execution paths instead.
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

    def native_only_source(path, *, native_root_delta=False):
        """Prove every production mount is absent from a non-test WASM build.

        A reverse dependency alone is neither proof of portability nor proof of
        native-only ownership. Keep portable checks for unknown cfgs, alternate
        mounts, includes and cycles. Only literal, already resolved exclusion
        gates establish this optimization. A candidate annotation alone cannot
        authorize deferral: production cfg edits expand the plan below, and a
        native root delta requires a separate additive module/source proof.
        """
        if (path not in sources or source_inputs.get('uncertain_modules')
                or (production_roots | host_roots) - sources.keys()
                or source_owners(path) & set(source_inputs.get('unresolved_includes', ()))
                or (source_owners(path) | set(feature_owners.get(path, ())))
                & set(source_inputs.get('host_execution_inputs', ()))):
            return False
        excluded = {
            ('cfg', '(', 'not', '(', 'target_arch', '=', '"wasm32"', ')', ')'),
            ('cfg', '(', 'test', ')'),
            ('cfg', '(', 'unix', ')'),  # False for the fixed wasm32-unknown-unknown target.
        }

        def excluded_mounts(current, visiting=frozenset()):
            if (current in visiting or current not in sources
                    or source_owners(current) & set(source_inputs.get('unresolved_includes', ()))
                    or source_owners(current) & set(source_inputs.get('host_execution_inputs', ()))):
                return False
            # A build script is a host executable, including when it imports
            # another target's native module directly instead of through Cargo.
            if inherited_paths(current, True, roots=host_roots):
                return False
            routes = inherited_paths(current, True, roots=production_roots)
            # An independent integration target is absent from production builds.
            # This is separate from, and cannot replace, executable source proof.
            absent = (all(proven and not opaque and not packages & set(source_inputs.get('host_execution_inputs', ()))
                          and ((native_root_delta and current == path) or any(gate in excluded for gate in gates))
                          for gates, proven, opaque, packages in routes) if routes else
                      current in ordinary_roots and current not in production_roots)
            # include_str!/include_bytes! are dependency edges only. They never
            # authorize Rust execution, but portable readers prohibit deferral.
            return absent and all(excluded_mounts(reader, visiting | {current})
                                  for reader in source_inputs.get('data_readers', {}).get(current, ()))

        return excluded_mounts(path) and all(excluded_mounts(reader)
                                            for reader in source_inputs.get('unresolved_data_readers', ()))

    def native_only_root_additions(path):
        # A portable lib root can change only its native module wiring. Prove
        # the actual delta and the new sources, without treating the whole root
        # as excluded or trusting a path/name convention. Alternate code mounts
        # remain conservative; data/host readers are checked by the same proof
        # as ordinary native-only sources, including readers of the root text.
        targets = [target for target in selected_targets or ()
                   if target['target_kind'] == 'lib' and selected_target_root(target) == path
                   and selected_source_context(path, target)]
        if not targets or parents(path, True):
            return False
        modules = added_static_module_sources(path, [(base, source)], native_root=True)
        if not modules or not modules.issubset(required_source_paths):
            return False
        return (all(any(selected_source_context(module, target) for target in targets)
                    and native_only_source(module) for module in modules)
                and native_only_source(path, native_root_delta=True))

    def inherited_execution(path, native):
        combinations = []
        paths = inherited_paths(path, native)
        defaults = source_inputs.get('default_features', {})
        for gates, proven, opaque, packages in paths:
            if opaque:
                raise TestPolicyError('changed cfg is outside the validated test feature combinations (unresolved or cyclic module path): ' + path)
            ordinary = set.intersection(*(set(defaults.get(package, ())) for package in packages)) if packages else set()
            expanded = ordinary | enabled_features
            def conjunction(features):
                values = [gate_value(gate, native, features) for gate in gates]
                return False if False in values else (None if None in values else True)
            # A declared but unexecuted feature/unknown predicate is not a
            # disabled alternative. It must retain the existing policy error.
            if any(gate_value(gate, native, ordinary) is None
                   and gate_value(gate, native, expanded) is None for gate in gates):
                raise TestPolicyError('changed cfg is outside the validated test feature combinations: ' + path)
            ordinary_value, expanded_value = conjunction(ordinary), conjunction(expanded)
            if ordinary_value is not True and expanded_value is not True:
                if proven and ordinary_value is False and expanded_value is False:
                    continue
                raise TestPolicyError('changed cfg is outside the validated test feature combinations: ' + path)
            combinations.append((gates, proven, ordinary, expanded))
        if paths and not combinations:
            raise TestPolicyError('changed cfg is outside the validated test feature combinations: ' + path)
        return combinations

    def native_context(path):
        owners = feature_owners.get(path, set())
        if path in ordinary_roots or owners and owners.issubset(feature_packages):
            return True
        # A package-level default driver covers only its executed lib tree,
        # not arbitrary bins/targets. Prove the literal module path to a known
        # root under native predicates before granting target knowledge.
        return bool(owners and owners.issubset(feature_packages | NATIVE_DEFAULT_TEST_PACKAGES)
                    and ordinary_source_context(path, True))

    def worker_compilation_proves(path, attribute, owner, test_owners):
        # A successful compile is evidence only for production Rust. It cannot
        # execute #[test], an ignored case, or a native/opaque module route.
        package = 'oasis7_custody_worker'
        if (not worker_wasm_compile or source_owners(path) != {package}
                or attribute[0] != 'cfg' or owner[0] == 'unresolved'
                or any(applies_to(owner, test) for test in test_owners)):
            return False
        # Every applicable selection gate belongs to one concrete item. Include
        # unchanged local attributes, inline ancestors and file-inner gates;
        # separate successful tuples cannot witness their conjunction. The
        # selection parser excludes cfg_attr payloads that are only lint/derive.
        info = parsed(path, True)
        positions = [index if inner else target
                     for gate, target_owner, index, target, inner in info[12]
                     if gate == attribute and target_owner == owner]
        module_braces = {left for left, _, _ in info[10]}
        if (not positions or (owner[0] != 'module' and owner not in info[11])
                or any(left < position < right and info[7][left][1] == '{'
                       and left not in module_braces
                       for position in positions for left, right in info[8].items())):
            return False  # Nested fn/impl/trait/macro scopes have no compile proof.
        local_combinations = [[gate for gate, target in lexical_attributes(info, position)
                               if (gate, target) in info[0] and gate[0] in ('cfg', 'cfg_attr')]
                              for position in positions]
        routes = inherited_paths(path, True, roots=production_roots)
        manifest = source_inputs.get('manifests', {}).get(package, {})
        for local_gates in local_combinations:
            witnessed = False
            for features in worker_wasm_compile_features(manifest):
                if not set(features).issubset(available_features.get(package, ())):
                    continue
                enabled = worker_local_feature_closure(manifest, features)
                if any(gate_value(gate, False, enabled, True) is not True
                       for gate in local_gates):
                    continue
                if any(proven and not opaque and packages == {package}
                       and all(gate_value(gate, False, enabled, True) is True
                               for gate in gates)
                       for gates, proven, opaque, packages in routes):
                    witnessed = True
                    break
            if not witnessed:
                return False
        return True

    def production_import_proves(path, attribute, owner):
        # Only a direct sibling-module alias, mounted in both real Cargo
        # contexts, may use the production (test=false) compilation tuple.
        # This never witnesses a fn/mod test execution gate.
        if owner[0] != "use" or attribute[0] != "cfg" or "feature" in attribute:
            return False
        info = parsed(path, True)
        tokens, closing, items = info[7], info[8], info[9]
        starts = [index for index, item in items.items() if item == owner]
        if len(starts) != 1:
            return False
        start = starts[0]
        if any(left < start < right and tokens[left][1] == "{" for left, right in closing.items()):
            return False
        declaration = [value for _, value, _ in tokens[start:start + 8]]
        if (len(declaration) != 8 or declaration[:4] != ["use", "super", ":", ":"]
                or declaration[5] != "as" or declaration[7] != ";"
                or tokens[start + 4][0] != "identifier" or tokens[start + 6][0] != "identifier"):
            return False
        if gate_value(attribute, True, set(), test_build=False) is not True or gate_value(attribute, True, set()) is not False:
            return False
        local = [gate for gate, _ in lexical_attributes(info, start)]
        if any(gate[0] == "cfg_attr" or (gate[0] == "cfg" and (
                "feature" in gate or gate_value(gate, True, set(), test_build=False) is not True))
                or (gate[0] not in ("cfg", "cfg_attr") and not ordinary_attribute(gate, True))
                for gate in local):
            return False
        def tuple_features(packages, test_build):
            defaults = source_inputs.get("default_features", {})
            features = set.intersection(*(set(defaults.get(package, ())) for package in packages)) if packages else set()
            declared = set.intersection(*(set(available_features.get(package, ())) for package in packages)) if packages else set()
            if test_build and packages and packages.issubset(feature_packages):
                features |= enabled_features & declared
            return features
        def compile_route(roots, test_build):
            routes = inherited_paths(path, True, roots=roots)
            if not routes:
                return False
            values = []
            for gates, proven, opaque, packages in routes:
                features = tuple_features(packages, test_build)
                current = [gate_value(gate, True, features, test_build=test_build) for gate in gates]
                if not proven or opaque or any(value is None for value in current):
                    return False
                values.append(all(current))
            # Alternative literal mounts may be disabled in this tuple; one
            # actual route must execute, and no unresolved route is hidden.
            return any(values)
        if not compile_route(ordinary_roots, True) or not compile_route(production_roots, False):
            return False
        parents(path, True)  # Populate literal provenance, independently of cfg truth.
        provenance = reference_cache[(path, True, None)]
        if any(not known or opaque or not resolved for _, known, opaque, resolved, _ in provenance):
            return False
        references = sorted({parent for parent, _, _, _, gates in provenance
                             if all(gate_value(gate, True, tuple_features(source_owners(parent), False), test_build=False) is True for gate in gates)})
        if not references:
            return False
        # The imported sibling must have a literal declaration and actual source;
        # an unresolved import cannot become a compiler-coverage exemption.
        name = declaration[4]
        for parent in references:
            parent_info = parsed(parent, True)
            matches = []
            for index, item in parent_info[9].items():
                if item[0] != "mod" or parent_info[7][index + 1][1] != name:
                    continue
                if any(left < index < right and parent_info[7][left][1] == "{" for left, right in parent_info[8].items()):
                    continue
                gates = [gate for gate, _ in lexical_attributes(parent_info, index) if gate[0] in ("cfg", "cfg_attr")]
                if any(gate_value(gate, True, tuple_features(source_owners(parent), False), test_build=False) is not True for gate in gates):
                    continue
                if parent_info[7][index + 2][1] == "{":
                    matches.append(True)
                else:
                    paths = [gate for gate, target in parent_info[6] if target == item and gate[0] == "path"]
                    if paths and not (len(paths) == 1 and len(paths[0]) == 3 and rust_literal_path(paths[0][2]) is not None):
                        continue
                    directory = posixpath.dirname(parent) if parent in production_roots or posixpath.basename(parent) in ("mod.rs", "main.rs", "lib.rs") else parent.removesuffix(".rs")
                    candidates = ([posixpath.normpath(posixpath.join(posixpath.dirname(parent), rust_literal_path(paths[0][2])))]
                        if len(paths) == 1 and len(paths[0]) == 3 and rust_literal_path(paths[0][2]) is not None
                        else [posixpath.join(directory,name + ".rs"),posixpath.join(directory,name,"mod.rs")])
                    matches.append(sum(candidate in sources for candidate in candidates) == 1)
            if matches != [True]:
                return False
        return True

    def new_pixel_platform_statement(path, attribute, owner, old_items, test_owners):
        # The trusted pixel group executes default-feature native lib tests AND
        # a wasm32 production check. This witnesses only new direct free-function
        # statements, never a test or a change to an existing function's gates.
        package = 'pixel_world_bridge'
        predicates = {
            ('cfg', '(', 'target_arch', '=', '"wasm32"', ')'),
            ('cfg', '(', 'not', '(', 'target_arch', '=', '"wasm32"', ')', ')'),
        }
        if (not pixel_platform_driver or source_owners(path) != {package}
                or attribute not in predicates or not ordinary_source_structure(path, True)):
            return False
        info = parsed(path, True)
        context = info[13].get(owner)
        if context is None:
            return False
        function, function_start, position = context
        if function in old_items or function in test_owners:
            return False
        local = set(lexical_attributes(info, function_start) + lexical_attributes(info, position))
        local.update(item for item in info[6] if item[1] == owner)
        if any(gate[0] == 'cfg_attr' or (gate[0] != 'cfg' and not ordinary_attribute(gate, True))
               for gate, _ in local):
            return False
        gates_here = [gate for gate, _ in local if gate[0] == 'cfg']
        features = set(source_inputs.get('default_features', {}).get(package, ()))
        routes = inherited_paths(path, True)
        witnessed = False
        for gates, proven, opaque, packages in routes:
            if not proven or opaque or packages != {package}:
                return False
            for wasm in (False, True):
                values = [gate_value(gate, not wasm, features, wasm, test_build=not wasm)
                          for gate in (*gates, *gates_here)]
                if any(value is None for value in values):
                    return False
                witnessed |= all(values)
        return witnessed

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
        if path in required_source_paths:
            selected_source = bool(selected_targets) and any(
                selected_source_context(path, target) for target in selected_targets)
            if not selected_source and narrow_unavailable is not None:
                narrow_unavailable.add(path)
            if portable_paths is not None and not (selected_source and native_only_source(path)):
                if selected_source and native_only_root_additions(path):
                    if native_root_deltas is not None:
                        native_root_deltas.add(path)
                else:
                    portable_paths.add(path)
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
        production_additions = {item for item in after_attrs - before_attrs
                                if not has_removed_tests and new_pixel_platform_statement(
                                    path, *item, old_items, test_owners)}
        for attribute, owner in after_attrs - before_attrs:
            if (ordinary_source and not has_removed_tests
                    and ordinary_attribute(attribute, native)
                    and ((owner in after[5] and owner not in old_items)
                         or (owner == ('module', '', 0) and not source_at(base, path)))):
                benign_additions.add((attribute, owner))
        changes -= benign_additions | production_additions
        test_changes = {item for item in changes if test_related(item, test_owners, path)}
        added_attrs = {item for item in after_attrs - before_attrs if test_related(item, test_owners, path)}
        added_attrs.update(item for item in benign_additions if item[0][0] in ('cfg', 'cfg_attr'))
        # A new case inherits unchanged module/file gates too. They must be
        # executable under the same expanded combination as a newly added gate.
        added_attrs.update((attribute, owner) for attribute, owner in after_attrs
                           if attribute[0] in ('cfg', 'cfg_attr')
                           and any(applies_to(owner, test) for test in new_test_owners - old_test_owners))
        inherited = inherited_execution(path, native) if new_test_owners - old_test_owners else []
        inherited_defaults = set.intersection(*(entry[2] for entry in inherited)) if inherited else set()
        inherited_expanded = inherited_defaults | enabled_features
        # Default features come from the executing packages' Cargo manifests,
        # not a new allowed-feature name or whichever path happened to pass.
        for gates, _, ordinary_features, expanded_features in inherited:
            for attribute in gates:
                features = {attribute[index + 2].strip('"') for index, value in enumerate(attribute[:-2])
                            if value == 'feature' and attribute[index + 1] == '='}
                if features and (not feature_owners.get(path) or any(
                        not features.issubset(available_features.get(package, set())) for package in feature_owners[path])):
                    raise TestPolicyError('changed cfg is outside the validated test feature combinations (feature not declared by its executing package): ' + path)
                mapped = bool(feature_owners.get(path)) and feature_owners[path].issubset(feature_packages)
                if (not enabled_gate(attribute, native, ordinary_features)
                        and not enabled_gate(attribute, mapped, expanded_features)):
                    raise TestPolicyError('changed cfg is outside the validated test feature combinations: ' + path)
                if attribute[0] == 'cfg_attr' and 'ignore' in attribute:
                    raise TestPolicyError('new ignored test lacks a trusted execution mapping: ' + path)
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
        for attribute, owner in added_attrs:
            if production_import_proves(path, attribute, owner):
                production_cfg_paths.append(path)
                continue
            worker = 'oasis7_custody_worker'
            manifest = source_inputs.get('manifests', {}).get(worker, {})
            worker_root = posixpath.join(directories.get(worker, ''), 'src/lib.rs')
            if (worker_wasm_compile and path == worker_root
                    and source_owners(path) == {worker}
                    and worker_signer_profile_enabled(manifest)
                    and worker_exact_signer_guard(source_at(source, path), attribute, owner)):
                production_cfg_paths.append(path)
                continue
            if worker_compilation_proves(path, attribute, owner, test_owners):
                production_cfg_paths.append(path)
                continue
            if attribute[0] not in ('cfg', 'cfg_attr'):
                continue
            features = {attribute[index + 2].strip('"') for index, value in enumerate(attribute[:-2])
                        if value == 'feature' and attribute[index + 1] == '='}
            mapped_native_driver = (bool(feature_owners.get(path))
                                    and feature_owners[path].issubset(feature_packages))
            ordinary_gate = enabled_gate(attribute, native, inherited_defaults)
            if not ordinary_gate and features and not mapped_native_driver:
                raise TestPolicyError('changed test feature has no expanded-feature driver for its package: ' + path)
            if not ordinary_gate and not enabled_gate(attribute, native, inherited_expanded):
                raise TestPolicyError('changed cfg is outside the validated test feature combinations: ' + path)
            if features and (not feature_owners.get(path) or any(
                    not features.issubset(available_features.get(package, set()))
                    for package in feature_owners[path])):
                raise TestPolicyError('changed cfg is outside the validated test feature combinations (feature not declared by its executing package): ' + path)
    return sorted(policy_paths), False, sorted(production_cfg_paths)


ORDINARY_ARTIFACT_PARTITIONS = ['controls', 'runtime', 'component-libs', 'integrations-0', 'integrations-1']


def complete_ordinary_partitions(config, plan):
    if (config.get('ordinary_artifact_partition_version') == 1
            and 'oasis7_required' in plan['groups']
            and plan.get('ordinary_execution') != 'inline'
            and plan['scope'] in ('full', 'explicit-full')):
        plan.update(ordinary_artifact_partition_version=1,
                    ordinary_partitions=list(ORDINARY_ARTIFACT_PARTITIONS))
    if config.get('workspace_support_partition_version') == 1 and 'workspace_support' in plan['groups']:
        plan.update(workspace_support_partition_version=1,
                    workspace_support_partitions=support_partitions(plan['workspace_support_profiles']))
        if (config.get('custody_execution_version') == CUSTODY_EXECUTION_VERSION
                and 'oasis7_custody_worker' in plan['workspace_support_profiles']
                and plan.get('test_policy_changed') is False):
            # Policy edits keep the legacy support driver's expanded-feature
            # --all-targets tail; the split build contract does not replace it.
            plan.update(custody_execution_version=CUSTODY_EXECUTION_VERSION,
                        custody_node_shards=list(CUSTODY_NODE_SHARDS))


def support_partitions(profiles):
    return sorted({'custody' if profile == 'oasis7_custody_worker' else 'other' for profile in profiles})


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
    plan.update(python_control_plan(config))
    complete_ordinary_partitions(config, plan)
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
    support_owners = {contract['package'] for contract in config.get('ordinary_fast_support_contracts', [])}
    if (not conservative and selected_core
            and ('oasis7_required' in chosen or support_owners.intersection(affected))
            and 'oasis7_required' not in additions
            and not any('oasis7_required' in groups for groups in path_rules.values())):
        native_fast_candidate = ordinary_fast_selection(
            config, paths, path_affected, path_rules, selected_core, comparisons or [(base, source)], source_inputs)
    narrow_features_by_path, narrow_unavailable = {}, set()
    execution_capable = config.get('fast_execution_version') == FAST_EXECUTION_VERSION
    portable_paths, native_root_comparisons = set(), []
    if config.get('fast_test_inventory_version') == FAST_TEST_INVENTORY_VERSION:
        for rule in config['ordinary_fast_rules']:
            if 'added_declarations' not in rule and not rule.get('static_module_additions'):
                features = set.intersection(*(set(config['ordinary_fast_targets'][key]['features'])
                                              for key in rule['targets']))
                narrow_features_by_path.update((path, features) for path in rule['paths'])
    if native_fast_candidate is not None:
        narrow_features_by_path.update(native_fast_candidate[3])
    for before, after in comparisons or [(base, source)]:
        comparison_native_roots = set()
        changed_policy, changed_ignored, production_cfg = changed_test_policy(
            before, after, paths, feature_owners or {},
            set(config['oasis7_required_packages']) | set(SUPPORT_PROFILES),
            (feature_manifests or {}).get(resolve_commit(after), {}),
            (test_target_roots or {}).get(resolve_commit(after), set()),
            (source_inputs or {}).get(resolve_commit(after), {}),
            narrow_features_by_path, narrow_unavailable,
            native_fast_candidate[0] if native_fast_candidate else None,
            native_fast_candidate[1] if execution_capable and native_fast_candidate else (),
            portable_paths if execution_capable else None,
            worker_wasm_compile=(conservative or 'oasis7_custody_worker'
                                 in plan.get('workspace_support_profiles', ())),
            native_root_deltas=comparison_native_roots,
            pixel_platform_driver=('pixel_world_bridge' in chosen
                                   and 'pixel_world_bridge' in config['package_groups'].get('pixel_world_bridge', ())))
        native_root_comparisons.append(comparison_native_roots)
        policy_paths.update(changed_policy)
        ignored_risk |= changed_ignored
        production_cfg_paths.update(production_cfg)
    # A successful source diff cannot erase an unproved tested-merge diff,
    # including an empty delta after a merge resolution restores the old tree.
    native_root_deltas = set().union(*native_root_comparisons)
    for proven in native_root_comparisons:
        portable_paths.update(native_root_deltas - proven)
    policy_paths = sorted(policy_paths)
    if narrow_unavailable:
        plan['reasons'].extend('changed source is not proven executable by the selected fast target/features; expanded coverage: ' + path
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
    if native_fast_candidate is not None and execution_capable and not conservative and not policy_paths and not ignored_risk:
        # Independent integration inputs may originally select only support.
        # Promotion follows the same source/target/policy proof as library edits.
        chosen.update(('rust_baseline', 'oasis7_required'))
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

    if native_fast is not None and execution_capable:
        plan['fast_execution_version'] = FAST_EXECUTION_VERSION
        plan['ordinary_execution'] = 'inline'
        if {'rust_baseline', 'oasis7_required'}.issubset(chosen):
            plan['ordinary_clippy_group'] = 'rust_baseline'
        baseline_checks = []
        if 'oasis7_newapi_bridge_service' in affected:
            baseline_checks.append('newapi_accounting')
        if any(path.startswith(('tools/', 'crates/oasis7_builtin_wasm_modules/'))
               or path.startswith('scripts/check-standalone-tool-lockfiles') for path in paths):
            baseline_checks.append('standalone_tool_lockfiles')
        plan['rust_baseline_checks'] = baseline_checks
        native_paths = native_fast[1] - portable_paths
        plan['reasons'].extend('new native-only library module wiring; existing portable tokens unchanged: ' + path
                               for path in sorted(native_root_deltas & native_paths))
        portable_consumers = {package for path in paths if path not in native_paths
                              for package in path_affected.get(path, ())}
        deferred_wasm = wasm - portable_consumers
        wasm -= deferred_wasm
        if deferred_wasm:
            plan['reasons'].append('all changed production input deltas are native/test-only; portable checks stay in full: '
                                   + ', '.join(sorted(deferred_wasm)))
        launcher_paths = {path for path in paths if 'launcher_web' in path_rules.get(path, ()) or any(
            'launcher_web' in config['package_groups'].get(package, ())
            for package in path_affected.get(path, ()))}
        if ('launcher_web' in chosen and 'launcher_web' not in additions and launcher_paths
                and launcher_paths.issubset(native_paths)
                and not any('launcher_web' in groups for groups in path_rules.values())):
            chosen.remove('launcher_web')
            plan['reasons'].append('all launcher consumer inputs are proven native/test-only; web packaging stays in full')

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
                matches = [rule for rule in config['formal_risk_rules']
                           if any(fnmatch.fnmatchcase(path, pattern) for pattern in rule['match'])]
                if not matches and native_fast is not None and path in native_fast[1]:
                    plan['reasons'].append('bounded native contract; complete process regression stays in full: ' + path)
                    continue
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
    if (native_fast is not None and execution_capable and not exceptions and not additions
            and 'workspace_support' in chosen
            and not any('workspace_support' in groups for groups in path_rules.values())
            and not any(fnmatch.fnmatchcase(path, pattern) for path in paths
                        for patterns in config.get('workspace_support_profile_paths', {}).values()
                        for pattern in patterns)):
        profiles = set(plan['workspace_support_profiles'])
        handoffs, deferred = set(), set()
        native_paths = native_fast[1] - portable_paths
        for contract in config.get('ordinary_fast_support_contracts', []):
            owner = contract['package']
            owner_inputs = {path for path in paths if owner in path_affected.get(path, ())}
            covered = native_paths.intersection([contract['source'], *contract['tests']])
            if owner not in profiles or not owner_inputs or not owner_inputs.issubset(covered):
                continue
            handoffs.add(owner)
            for consumer in profiles.intersection(contract['deferred_consumers']):
                inputs = {path for path in paths if consumer in path_affected.get(path, ())}
                if inputs and inputs.issubset(covered):
                    deferred.add(consumer)
        if handoffs:
            plan.update(ordinary_support_contract_version=1,
                        ordinary_support_handoffs=sorted(handoffs), ordinary_support_deferred=sorted(deferred),
                        workspace_support_profiles=sorted(profiles - handoffs - deferred))
            plan['reasons'].append('private native adapter contract: inline library/CLI tests for '
                                   + ', '.join(sorted(handoffs)) + '; full consumer regression deferred for '
                                   + ', '.join(sorted(deferred)) + '; all original all-targets Clippy retained in rust_baseline')
            if not plan['workspace_support_profiles']:
                chosen.remove('workspace_support')
    plan.update(groups=sorted(chosen),
                resources=sorted({resource for group in chosen for resource in config['resources'][group]}),
                execution_profile=profile, fast_plan_version=FAST_PLAN_VERSION,
                check_inventory_version=CHECK_INVENTORY_VERSION,
                source_sha=resolve_commit(source), tested_sha=resolve_commit(tested or source),
                policy_sha=resolve_commit(policy_ref or base),
                affected_packages=sorted(affected),
                oasis7_required_packages=sorted(selected_core), oasis7_required_wasm_packages=sorted(wasm),
                formal_suites=sorted(suites), formal_shards=formal_shards(suites, formal_inventory),
                command_center_package=center_package,
                test_policy_changed=bool(policy_paths), test_policy_paths=policy_paths,
                fast_plan_exceptions=exceptions)
    # Architecture graphs remain complete. Only a final, exception-free ordinary
    # plan may defer their extra profile builds to full; a mixed support input
    # cannot borrow an unrelated component's fast target contract.
    architecture = plan['architecture']
    inputs = set(architecture['packages'])
    owners = {target['package'] for target in plan.get('ordinary_fast_targets', ())}
    required_groups = {'baseline', 'rust_baseline', 'oasis7_required'}
    extra_groups = set(plan['groups']) - required_groups
    consumer_groups = {group: sorted(package for package in affected - inputs
                                    if group in config['package_groups'].get(package, ()))
                       for group in sorted(extra_groups)}
    # Native reverse-consumer jobs retain their complete existing checks. A
    # direct path contract or an explicit request is a separate input, not proof
    # that an extra group only exercises an already-covered component change.
    native_consumers = (extra_groups <= {'consensus', 'distfs', 'net', 'node'}
                        and all(consumer_groups.values())
                        and not any(extra_groups.intersection(groups) for groups in path_rules.values()))
    if (config.get('architecture_build_policy_version') == ARCHITECTURE_BUILD_POLICY_VERSION
            and plan.get('fast_execution_version') == FAST_EXECUTION_VERSION
            and plan.get('ordinary_execution') == 'inline' and plan['scope'] == 'targeted'
            and architecture['mode'] == 'targeted' and not additions
            and required_groups.issubset(plan['groups']) and native_consumers
            and not plan['test_policy_changed'] and not plan['fast_plan_exceptions']
            and not plan['formal_suites'] and not plan['oasis7_required_wasm_packages']
            and not any(fnmatch.fnmatchcase(path, pattern) for path in paths for pattern in CONTROL)
            and inputs and inputs.issubset(owners)):
        architecture.update(build_policy_version=ARCHITECTURE_BUILD_POLICY_VERSION,
                            build_policy='ordinary-targets')
        if consumer_groups:
            architecture['consumer_groups'] = consumer_groups
        plan['reasons'].append('all architecture inputs have mandatory fast targets; '
                               'extra profile compilation stays in full, all dependency graphs remain required')
    if (config.get('ordinary_partition_version') == ORDINARY_PARTITION_VERSION
            and plan.get('fast_execution_version') == FAST_EXECUTION_VERSION
            and plan.get('ordinary_execution') == 'inline'
            and plan.get('ordinary_clippy_group') == 'rust_baseline'
            and plan['scope'] == 'targeted' and not plan['test_policy_changed']
            and not plan['oasis7_required_wasm_packages']):
        packages = sorted({target['package'] for target in plan['ordinary_fast_targets']})
        plan.update(ordinary_partition_version=ORDINARY_PARTITION_VERSION,
                    ordinary_partitions=packages if set(packages) == ORDINARY_PARTITION_PACKAGES else ['all'])
    if (config.get('distfs_clippy_contract_version') == DISTFS_CLIPPY_CONTRACT_VERSION
            and plan.get('ordinary_partitions') == sorted(ORDINARY_PARTITION_PACKAGES)
            and architecture.get('build_policy') == 'ordinary-targets'
            and {'distfs', 'rust_baseline', 'oasis7_required'}.issubset(plan['groups'])
            and not plan['fast_plan_exceptions'] and not plan['formal_suites']
            and {(target['package'], target['target_kind'], target['target_name'])
                 for target in plan['ordinary_fast_targets']} == DISTFS_CLIPPY_TARGETS
            and all(target['default_features'] is False and target['features'] == ['test_tier_required']
                    for target in plan['ordinary_fast_targets'])):
        plan.update(distfs_clippy_contract_version=DISTFS_CLIPPY_CONTRACT_VERSION,
                    distfs_clippy_group='rust_baseline')
        plan['reasons'].append('complete DistFS/CAS/Formal strict Clippy stays required in rust_baseline; '
                               'all original DistFS group test targets remain serial')
    plan['reasons'] = list(dict.fromkeys(plan['reasons']))
    complete_ordinary_partitions(config, plan)
    return plan


def select(config, base, source_head, test_head=None, additions=(), profile=None,
           formal_inventory=None, policy_ref=None):
    validate_config(config)
    python_inputs = python_control_inputs(config)
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
        source_cache = GitSourceCache()
        for ref in dict.fromkeys(graph_refs):
            try:
                graph = cargo_graph(ref, config.get('generated_inputs', []), source_cache=source_cache)
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
                                 & ({'consensus', 'distfs', 'node', 'net'}
                                    | ({'pixel_world_bridge'} if package == 'pixel_world_bridge' else set()))
                                 and posixpath.relpath(path, directory or '.')
                                 == manifest.get('lib', {}).get('path', 'src/lib.rs')))}
                    source_inputs[resolve_commit(ref)] = {
                        'sources': sources,
                        'reverse': graph[1],
                        'directories': {package: directory for package, (directory, _) in graph[0].items()},
                        'manifests': {package: manifest for package, (_, manifest) in graph[0].items()},
                        'default_features': {package: default_feature_closure(manifest)
                                             for package, (_, manifest) in graph[0].items()
                                             if package in config.get('oasis7_required_packages', ())
                                             or package == 'pixel_world_bridge'},
                        'includes': graph[2].get('__rust_include_targets__', ()),
                        'unresolved_includes': graph[2].get('__unresolved_rust_include_owners__', ()),
                        'uncertain_modules': graph[2].get('__uncertain_module_sources__', ()),
                        'data_readers': graph[2].get('__rust_data_readers__', {}),
                        'unresolved_data_readers': graph[2].get('__unresolved_rust_data_readers__', ()),
                        'host_execution_inputs': graph[2].get('__host_execution_inputs__', ()),
                    }
            except (OSError, subprocess.CalledProcessError, ValueError, KeyError, TypeError) as error:
                # Keep usable graphs to establish ownership even when another
                # registered input cannot be parsed. Uncertainty is not registration.
                full = True
                reasons.append('uncertain Git/dependency inputs: ' + str(error))
        reported_embeds = False
        governance_registrations = additive_governance_registration(config, policy_comparisons, paths)
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
            elif path == 'scripts/ci-tests.sh' and governance_registrations:
                chosen.add('workflow_governance')
                path_rules[path].add('workflow_governance')
                reasons.append('proved additive diagnostic test registration: '
                               + ', '.join(governance_registrations))
            elif controlled:
                full = True
                reasons.append('shared build or CI control changed: ' + path)
            contract_groups = python_inputs.get(path, {})
            if contract_groups:
                chosen.update(contract_groups)
                path_rules[path].update(contract_groups)
                support_rule |= 'workspace_support' in contract_groups
                reasons.append('trusted Python control contract input changed: ' + path)
            matched = controlled or bool(contract_groups)
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
        if governance_registrations and (affected or chosen - {'baseline', 'workflow_governance'}):
            full = True
            reasons.append('diagnostic registration mixed with product or other validation inputs')
    except (OSError, subprocess.CalledProcessError, ValueError, KeyError, TypeError) as error:
        full = True
        reasons.append('uncertain Git/dependency inputs: ' + str(error))
    if unknown_paths:
        raise UnknownInputsError(unknown_paths)
    python_controls = python_control_plan(config, paths)
    if python_controls.get('python_control_paths'):
        chosen.add('workflow_governance')
        reasons.append('trusted Python control contracts run in workflow_governance')
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
    plan.update(python_controls)
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
                output.write('ordinary_partitions=' + json.dumps(plan.get('ordinary_partitions', ['all']), separators=(',', ':')) + '\n')
                output.write('workspace_support_partitions=' + json.dumps(plan.get('workspace_support_partitions', ['all']), separators=(',', ':')) + '\n')
                output.write('formal_shards=' + json.dumps(plan['formal_shards'], separators=(',', ':')) + '\n')
                output.write('command_center_package=' + str(plan['command_center_package']).lower() + '\n')
    print(encoded)


if __name__ == '__main__':
    main()
