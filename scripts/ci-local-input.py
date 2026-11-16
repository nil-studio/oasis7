"""Read-only local input capture and private Git trees for the shared planner."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import stat
import subprocess
import tomllib


class Blocked(ValueError):
    pass


def environment():
    # A caller's alternate index/object store, replace refs or injected Git
    # configuration must not retarget the source or the local trust anchor.
    env = {key: value for key, value in os.environ.items() if not key.startswith('GIT_')}
    env.update(GIT_OPTIONAL_LOCKS='0', GIT_NO_REPLACE_OBJECTS='1')
    return env


def git(root, *args, data=None, env=None):
    try:
        return subprocess.check_output(
            ['git', '-C', str(root), '-c', 'core.fsmonitor=false', '-c', 'core.filemode=true', *args],
            input=data, stderr=subprocess.PIPE, env=env or environment(), timeout=30)
    except (OSError, subprocess.SubprocessError) as error:
        detail = getattr(error, 'stderr', b'').decode('utf-8', 'replace').strip()
        raise Blocked(f'git {args[0]} failed: {detail or error}') from error


def commit(root, ref):
    if not ref or ref.startswith('-'):
        raise Blocked('invalid revision')
    return git(root, 'rev-parse', '--verify', '--end-of-options', ref + '^{commit}').decode().strip()


def baseline(root, requested):
    head, anchor = commit(root, 'HEAD'), commit(root, 'refs/remotes/origin/main')
    if requested:
        base = commit(root, requested)
    else:
        bases = git(root, 'merge-base', '--all', head, anchor).decode().splitlines()
        if len(bases) != 1:
            raise Blocked('HEAD and local origin/main must have one unique merge base')
        base = bases[0]
    try:
        git(root, 'merge-base', '--is-ancestor', base, anchor)
    except Blocked as error:
        raise Blocked('base is not a trusted ancestor of local origin/main') from error
    return head, anchor, base


def paths(blob):
    return [os.fsdecode(value) for value in blob.split(b'\0') if value]


def changes(root, origin, *comparison):
    records = paths(git(root, 'diff', '--no-ext-diff', '--no-textconv', '--name-status',
                        '-z', '--find-renames', '--find-copies', *comparison, '--'))
    result = []
    index = 0
    while index < len(records):
        status, path = records[index:index + 2]
        index += 2
        entry = dict(origin=origin, status=status, path=path)
        if status[0] in 'RC':
            entry.update(old_path=path, path=records[index])
            index += 1
        result.append(entry)
    return result


def file_bytes(root, path):
    target = root / path
    try:
        details = target.lstat()
    except FileNotFoundError:
        return None
    # Never follow a directory symlink to capture an unrelated external tree.
    if any(parent.is_symlink() for parent in target.parents if parent != root and root in parent.parents):
        raise Blocked(f'symlink directory input is unsupported: {path!r}')
    if stat.S_ISLNK(details.st_mode):
        raise Blocked(f'symlink input is unsupported until its transitive read set is audited: {path!r}')
    if not stat.S_ISREG(details.st_mode):
        raise Blocked(f'nonregular input is unsupported: {path!r}')
    if not details.st_mode & 0o444:
        raise Blocked(f'unreadable input: {path!r}')
    return ('100755' if details.st_mode & 0o111 else '100644'), target.read_bytes()


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def blob_oid(data):
    return hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()


def capture(root, base, extra=()):
    head = commit(root, 'HEAD')
    flags = paths(git(root, 'ls-files', '-v', '-z'))
    if any(not entry or entry[0] == 'S' or entry[0].islower() for entry in flags):
        raise Blocked('assume-unchanged and skip-worktree inputs are unsupported')
    if git(root, 'ls-files', '--unmerged', '-z'):
        raise Blocked('unmerged index entries')
    index_path = Path(os.fsdecode(git(root, 'rev-parse', '--git-path', 'index').rstrip(b'\n')))
    if not index_path.is_absolute():
        index_path = root / index_path
    index_before = index_path.read_bytes()
    refs_before = git(root, 'show-ref', '--head', '-d')
    entries = {}
    for record in git(root, 'ls-files', '--stage', '-z').split(b'\0'):
        if not record:
            continue
        identity, raw_path = record.split(b'\t', 1)
        mode, oid, stage = identity.decode().split()
        if stage != '0' or set(oid) == {'0'}:
            raise Blocked('unresolved or intent-to-add index entry')
        entries[os.fsdecode(raw_path)] = (mode, oid)
    untracked = paths(git(root, 'ls-files', '--others', '--exclude-standard', '-z'))
    observed = set(entries) | set(untracked) | set(extra)
    # Baseline checkers explicitly walk these directories, including ignored
    # documents/skills. Generated Python bytecode is not a checker input.
    for relative in ('doc', '.agents/skills', 'skills', 'scripts'):
        start = root / relative
        if not start.is_dir():
            continue
        for directory, dirs, names in os.walk(start, followlinks=False):
            if Path(directory).is_symlink() or any((Path(directory) / name).is_symlink() for name in dirs):
                raise Blocked(f'symlink directory in baseline read scope: {directory}')
            dirs[:] = [name for name in dirs if name != '__pycache__']
            for name in names:
                if not name.endswith(('.pyc', '.pyo')):
                    observed.add((Path(directory) / name).relative_to(root).as_posix())
    files, submodules = {}, {}
    for path in sorted(observed):
        if entries.get(path, (None,))[0] == '160000':
            # Submodules are not traversed by this entry. A changed checkout
            # cannot be represented by its recorded gitlink alone.
            status = git(root, 'status', '--porcelain=v1', '--ignore-submodules=none', '--', path)
            if status:
                raise Blocked(f'dirty or uninitialized changed submodule: {path}')
            submodules[path] = entries[path]
            continue
        value = file_bytes(root, path)
        if value is not None:
            files[path] = value
    attributes = git(root, 'check-attr', '-z', '--stdin', 'filter', 'working-tree-encoding',
                     data=b''.join(os.fsencode(path) + b'\0' for path in sorted(observed)))
    attrs = paths(attributes)
    for index in range(0, len(attrs), 3):
        if attrs[index + 2] not in ('unspecified', 'unset'):
            raise Blocked(f'unsupported Git content transformation: {attrs[index]!r} ({attrs[index + 1]})')
    origins = changes(root, 'committed', base, head)
    origins += changes(root, 'staged', '--cached', head)
    # Compare raw bytes to the index, not Git's filtered worktree view. EOL
    # normalization must not erase input that the real checker/compiler reads.
    for path, (mode, oid) in entries.items():
        if mode == '160000':
            continue
        value = files.get(path)
        if value is None:
            origins.append(dict(origin='unstaged', status='D', path=path))
        elif value[0] != mode or blob_oid(value[1]) != oid:
            origins.append(dict(origin='unstaged', status='M', path=path))
    origins += [dict(origin='untracked', status='??', path=path) for path in untracked]
    origins += [dict(origin='referenced', status='A', path=path)
                for path in sorted(set(files) - set(entries) - set(untracked))]
    identity = {path: dict(mode=mode, sha256=sha256(data)) for path, (mode, data) in files.items()}
    if (commit(root, 'HEAD') != head or index_path.read_bytes() != index_before
            or git(root, 'show-ref', '--head', '-d') != refs_before):
        raise Blocked('HEAD, refs or index changed while capturing input')
    summary = dict(source_sha=head, index_sha256=sha256(index_before),
                   refs_sha256=sha256(refs_before),
                   entries_sha256=sha256(json.dumps(entries, ensure_ascii=True, sort_keys=True).encode()),
                   git_attributes_sha256=sha256(attributes),
                   files_sha256=sha256(json.dumps(identity, ensure_ascii=True, sort_keys=True).encode()),
                   file_count=len(files), submodules=submodules, changes=origins,
                   dirty=any(item['origin'] in ('staged', 'unstaged') for item in origins),
                   untracked=bool(untracked), referenced_paths=sorted(set(extra)),
                   absent_referenced_paths=sorted(set(extra) - set(files)),
                   scope='tracked, index, nonignored untracked and baseline directory inputs')
    return dict(summary=summary, files=files, entries=entries, submodules=submodules)


def referenced_inputs(root, snapshot, planner_path):
    """Use the trusted Rust tokenizer; this adds input bytes, never selection rules."""
    spec = importlib.util.spec_from_file_location('local_trusted_scope', planner_path)
    planner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(planner)
    files = snapshot['files']
    manifests = {}
    for path, (_, data) in files.items():
        if Path(path).name == 'Cargo.toml':
            try:
                manifests[str(Path(path).parent)] = tomllib.loads(data.decode())
            except (ValueError, UnicodeError) as error:
                raise Blocked(f'unreadable Cargo manifest: {path}: {error}') from error
    extra = set()

    def include(path, owner, required=False):
        candidate = Path(os.path.normpath(path))
        if candidate.is_absolute() or '..' in candidate.parts:
            if required:
                raise Blocked(f'external referenced input in {owner}: {path!r}')
            return
        relative = candidate.as_posix()
        target = root / candidate
        if target.is_file() or target.is_symlink():
            if relative not in files:
                extra.add(relative)
        elif required:
            # A source file can name a fixture behind an inactive feature. Keep
            # its absence in the immutable read set instead of guessing Rust
            # cfg here. If the selected compiler actually needs it, that real
            # command fails; creating it during this run makes the input stale.
            extra.add(relative)

    for directory, manifest in manifests.items():
        for member in manifest.get('workspace', {}).get('members', []):
            if Path(member).is_absolute() or '..' in Path(member).parts:
                raise Blocked(f'external workspace member in {directory}: {member!r}')
            for member_dir in (root / directory).glob(member):
                include((member_dir / 'Cargo.toml').relative_to(root).as_posix(), directory, required=True)
        targets = [manifest.get('lib', {})]
        for kind in ('bin', 'test', 'example', 'bench'):
            targets.extend(manifest.get(kind, []))
        build = manifest.get('package', {}).get('build')
        declared = [target['path'] for target in targets if 'path' in target]
        if isinstance(build, str):
            declared.append(build)
        elif build is not False and (root / directory / 'build.rs').exists():
            declared.append('build.rs')
        for path in declared:
            include(str(Path(directory) / path), directory, required=True)
            parent = Path(os.path.normpath(str(Path(directory) / path))).parent
            if parent.is_absolute() or '..' in parent.parts:
                raise Blocked(f'external Cargo target path in {directory}: {path!r}')
            # A custom target's adjacent module tree is an actual compiler
            # input even when its directory is ignored by Git.
            for current, dirs, names in os.walk(root / parent, followlinks=False):
                if Path(current).is_symlink() or any((Path(current) / name).is_symlink() for name in dirs):
                    raise Blocked(f'symlink directory in Cargo target read scope: {current}')
                dirs[:] = [name for name in dirs if name not in ('target', 'node_modules', '.git', '__pycache__')]
                for name in names:
                    if name.endswith('.rs'):
                        include((Path(current) / name).relative_to(root).as_posix(), directory, required=True)
        # Rust module lookup can read ignored source without an include macro.
        # Capture every .rs in the manifest's source/target directories instead
        # of maintaining a second Rust module resolver beside the planner.
        for relative in ('src', 'tests', 'examples', 'benches'):
            for parent, dirs, names in os.walk(root / directory / relative, followlinks=False):
                if Path(parent).is_symlink() or any((Path(parent) / name).is_symlink() for name in dirs):
                    raise Blocked(f'symlink directory in Rust read scope: {parent}')
                dirs[:] = [name for name in dirs if name not in ('target', 'node_modules', '__pycache__')]
                for name in names:
                    if name.endswith('.rs'):
                        include((Path(parent) / name).relative_to(root).as_posix(), directory, required=True)
        tables = [manifest, manifest.get('workspace', {}), *manifest.get('target', {}).values()]
        for table in tables:
            for section in ('dependencies', 'dev-dependencies', 'build-dependencies', 'patch'):
                for dependency in table.get(section, {}).values():
                    candidates = dependency.values() if section == 'patch' and isinstance(dependency, dict) else [dependency]
                    for value in candidates:
                        if isinstance(value, dict) and 'path' in value:
                            include(str(Path(directory) / value['path'] / 'Cargo.toml'),
                                    str(Path(directory) / 'Cargo.toml'), required=True)
    for path, (_, data) in files.items():
        if not path.endswith('.rs'):
            continue
        owners = [directory for directory in manifests if Path(path).is_relative_to(directory)]
        owner = max(owners, key=len) if owners else '.'
        try:
            source = data.decode('utf-8', 'replace')
            literals, calls, _ = planner.rust_source_inputs(source, with_module_paths=True)
            # The second token view is needed only for path attributes; avoid
            # tokenizing every unrelated Rust file twice.
            tokens = planner.rust_source_inputs(source, return_tokens=True)[0] if 'path' in source and '#' in source else []
        except ValueError as error:
            raise Blocked(f'uncertain Rust inputs in {path}: {error}') from error
        for index in range(len(tokens) - 5):
            if [value.removeprefix('r#') if kind == 'identifier' else value
                for kind, value, _ in tokens[index:index + 4]] == ['#', '[', 'path', '=']:
                value = planner.rust_literal_path(tokens[index + 4][1])
                if value is None:
                    raise Blocked(f'uncertain module path in {path}')
                include(str(Path(path).parent / value), path, required=True)
        for call in calls:
            if call['kind'] == 'literal':
                include(str(Path(path).parent / call['path']), path, required=True)
            elif call['kind'] == 'manifest':
                include(str(Path(owner) / call['path'].lstrip('/')), path, required=True)
            # Dynamic/generated include expressions remain the planner's
            # conservative policy; they never authorize targeted local success.
        for literal in literals:
            if ('/' in literal or literal.endswith('.rs')) and '\\' not in literal and len(literal) <= 512:
                for directory in ('.', owner, str(Path(path).parent)):
                    include(str(Path(directory) / literal), path)
    return extra


def private_trees(root, snapshot, directory):
    if git(root, 'rev-parse', '--show-object-format').strip() != b'sha1':
        raise Blocked('the shared planner requires SHA-1 Git identities')
    objects = directory / 'objects'
    objects.mkdir()
    common = Path(os.fsdecode(git(root, 'rev-parse', '--git-common-dir').rstrip(b'\n')))
    if not common.is_absolute():
        common = root / common
    env = environment()
    env.update(GIT_OBJECT_DIRECTORY=str(objects),
               GIT_ALTERNATE_OBJECT_DIRECTORIES=json.dumps(str((common / 'objects').resolve()), ensure_ascii=False))

    def tree(entries):
        directories = {}
        for path, entry in entries.items():
            current = directories
            parts = path.split('/')
            for part in parts[:-1]:
                current = current.setdefault(part, {})
            current[parts[-1]] = entry

        def write(children):
            records = []
            for name, value in children.items():
                if isinstance(value, dict):
                    mode, oid = '040000', write(value)
                else:
                    mode, oid = value
                kind = 'tree' if mode == '040000' else 'commit' if mode == '160000' else 'blob'
                records.append(f'{mode} {kind} {oid}\t'.encode() + os.fsencode(name) + b'\0')
            return git(root, 'mktree', '-z', '--missing', data=b''.join(records), env=env).decode().strip()
        return write(directories)

    worktree = dict(snapshot['submodules'])
    for path, (mode, data) in snapshot['files'].items():
        oid = blob_oid(data)
        if snapshot['entries'].get(path, (None, None))[1] != oid:
            actual = git(root, 'hash-object', '-w', '--stdin', data=data, env=env).decode().strip()
            if actual != oid:
                raise Blocked('captured blob identity mismatch')
        worktree[path] = mode, oid
    return tree(snapshot['entries']), tree(worktree), env
