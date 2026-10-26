#!/usr/bin/env python3
"""Freeze external capacity tools and check the real pre-init collection root."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import time


TOOL_FILES = (
    'codeql_rust_cfg_graph.py',
    'codeql-rust-capacity.py',
    'codeql-rust-cargo-check.py',
    'codeql-plan.py',
    'codeql-rust-refactored-source.py',
    'codeql-rust-model-diagnostic.py',
    'codeql_rust_diagnostic_runtime.py',
    'codeql_rust_capacity_layout.py',
    'codeql_rust_capacity_evidence.py',
    'codeql_rust_cargo_guard.py',
)
ACTION_SHA = '6f2ac81f5cd81fe351a197ba0b00d91d03cd2aee'
FILE_LIMIT = 4 * 1024**2
RECEIPT_LIMIT = 128 * 1024


class LayoutError(RuntimeError):
    pass


def _sha(value):
    return hashlib.sha256(value).hexdigest()


def _read_regular(path, limit=FILE_LIMIT):
    """Read a small, stable regular file without following a tool symlink."""
    path = Path(path).absolute()
    if path.resolve() != path:
        raise LayoutError('UNSAFE_TOOL_PATH:' + str(path))
    initial = path.stat(follow_symlinks=False)
    if not stat.S_ISREG(initial.st_mode) or initial.st_size > limit:
        raise LayoutError('UNSAFE_TOOL_FILE:' + str(path))
    # O_NONBLOCK also covers replacement by a FIFO between stat and open.
    with os.fdopen(os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK), 'rb') as stream:
        before = os.fstat(stream.fileno())
        if not stat.S_ISREG(before.st_mode) or before.st_size > limit:
            raise LayoutError('UNSAFE_TOOL_FILE:' + str(path))
        data = stream.read(limit + 1)
        after = os.fstat(stream.fileno())
    current = path.stat(follow_symlinks=False)
    identity = lambda item: (item.st_dev, item.st_ino, item.st_size, item.st_mtime_ns)
    if (len(data) > limit or identity(initial) != identity(before)
            or identity(before) != identity(after) or identity(after) != identity(current)):
        raise LayoutError('TOOL_CHANGED_WHILE_READING:' + str(path))
    return data


def _save(out, name, value):
    out = Path(out).absolute()
    if out.resolve() != out:
        raise LayoutError('UNSAFE_OUTPUT_PATH')
    out.mkdir(parents=True, exist_ok=True)
    data = (json.dumps(value, indent=2, sort_keys=True) + '\n').encode()
    if len(data) > RECEIPT_LIMIT:
        raise LayoutError('LAYOUT_RECEIPT_TOO_LARGE')
    target = out / name
    if target.is_symlink():
        raise LayoutError('UNSAFE_OUTPUT_RECEIPT')
    temporary = out / (name + '.tmp-' + str(os.getpid()))
    try:
        with temporary.open('xb') as stream:
            stream.write(data)
        os.replace(temporary, target)
    finally:
        temporary.unlink(missing_ok=True)


def _paths(source, tools):
    source = Path(source).resolve()
    requested_tools = Path(tools).absolute()
    tools = requested_tools.resolve()
    workspace = os.environ.get('GITHUB_WORKSPACE')
    runner = os.environ.get('RUNNER_TEMP')
    cwd = Path.cwd().resolve()
    workspace = Path(workspace).resolve() if workspace else None
    runner = Path(runner).resolve() if runner else None
    paths = {'cwd': str(cwd), 'source': str(source), 'tools': str(tools),
             'github_workspace': str(workspace) if workspace else None,
             'runner_temp': str(runner) if runner else None,
             'executing_helper': str(Path(__file__).resolve())}
    errors = []
    if cwd != source:
        errors.append('CWD_SOURCE_MISMATCH')
    if workspace != source:
        errors.append('WORKSPACE_SOURCE_MISMATCH')
    if tools.is_relative_to(source):
        errors.append('TOOLS_INSIDE_SOURCE')
    if runner is None or tools == runner or not tools.is_relative_to(runner):
        errors.append('TOOLS_OUTSIDE_RUNNER_TEMP')
    if tools != requested_tools:
        errors.append('TOOLS_PATH_IS_SYMLINK')
    if not source.is_dir():
        errors.append('SOURCE_NOT_DIRECTORY')
    return paths, errors


def _git(source, end, *args):
    remaining = end - time.monotonic()
    if remaining <= 0:
        raise LayoutError('NOT_REACHED_FIXED_DEADLINE')
    env = {key: value for key, value in os.environ.items()
           if key not in ('GH_TOKEN', 'GITHUB_TOKEN')}
    env.update(GIT_OPTIONAL_LOCKS='0', GIT_TERMINAL_PROMPT='0')
    result = subprocess.run(['git', '--no-optional-locks', '-C', str(source), *args],
                            env=env, capture_output=True, timeout=min(10, remaining))
    if result.returncode:
        raise LayoutError('TOOL_GIT_COMMAND_FAILED:' + args[0])
    if len(result.stdout) > RECEIPT_LIMIT:
        raise LayoutError('TOOL_GIT_OUTPUT_TOO_LARGE')
    return result.stdout


def _tracked_tools(source, end):
    selected = ['scripts/security/' + name for name in TOOL_FILES]
    tree = {}
    for entry in _git(source, end, 'ls-tree', '-rz', 'HEAD', '--', *selected).split(b'\0'):
        if entry:
            meta, name = entry.decode().split('\t', 1)
            mode, kind, oid = meta.split()
            if name in tree or kind != 'blob' or mode not in ('100644', '100755'):
                raise LayoutError('UNSAFE_TRACKED_TOOL:' + name)
            tree[name] = (mode, oid)
    if set(tree) != set(selected):
        raise LayoutError('TRACKED_TOOL_SET_DIFFERS')
    index = {}
    for entry in _git(source, end, 'ls-files', '--stage', '-z', '--', *selected).split(b'\0'):
        if entry:
            meta, name = entry.decode().split('\t', 1)
            mode, oid, stage = meta.split()
            if name in index or stage != '0':
                raise LayoutError('UNMERGED_TOOL_INDEX')
            index[name] = (mode, oid)
    if index != tree:
        raise LayoutError('TOOL_INDEX_DIFFERS_FROM_HEAD')
    return tree


def stage(source, tools, out, end, tool_sha):
    paths, errors = _paths(source, tools)
    source, tools = Path(paths['source']), Path(paths['tools'])
    value = {'schema': 'oasis7/rust-capacity-tool-snapshot/v1', 'status': 'FAIL',
             'tool_sha': tool_sha, 'actual_tool_sha': None,
             'source_root': str(source), 'tools_root': str(tools),
             'paths': paths, 'files': {}, 'errors': errors}
    try:
        if not re.fullmatch(r'[0-9a-f]{40}', tool_sha):
            raise LayoutError('INVALID_TOOL_SHA')
        if Path(__file__).resolve() != source / 'scripts/security' / Path(__file__).name:
            errors.append('STAGE_HELPER_NOT_FROM_WORKSPACE')
        if errors:
            return value
        actual = _git(source, end, 'rev-parse', '--verify', 'HEAD').decode().strip()
        value['actual_tool_sha'] = actual
        if actual != tool_sha:
            raise LayoutError('TOOL_HEAD_MISMATCH')
        top = _git(source, end, 'rev-parse', '--show-toplevel').decode().strip()
        if Path(top).resolve() != source:
            raise LayoutError('TOOL_REPOSITORY_ROOT_MISMATCH')
        tree = _tracked_tools(source, end)
        payloads = {}
        for name in TOOL_FILES:
            if time.monotonic() >= end:
                raise LayoutError('NOT_REACHED_FIXED_DEADLINE')
            relative = 'scripts/security/' + name
            mode, oid = tree[relative]
            data = _read_regular(source / relative)
            # This workflow's frozen Git commits use the SHA-1 object format.
            blob = hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()
            if blob != oid:
                raise LayoutError('TOOL_GIT_BLOB_MISMATCH:' + name)
            payloads[name] = data
            value['files'][name] = {'source_path': str(source / relative),
                                    'snapshot_path': str(tools / name), 'git_mode': mode,
                                    'git_blob_oid': oid, 'bytes': len(data),
                                    'source_sha256': _sha(data)}
        if tools.exists() and (not tools.is_dir() or any(tools.iterdir())):
            raise LayoutError('TOOLS_DESTINATION_NOT_EMPTY')
        tools.mkdir(parents=True, exist_ok=True)
        for name, data in payloads.items():
            target = tools / name
            with target.open('xb') as stream:
                stream.write(data)
            target.chmod(0o444)
            copied = _read_regular(target)
            value['files'][name]['snapshot_sha256'] = _sha(copied)
            if copied != data:
                raise LayoutError('TOOL_COPY_MISMATCH:' + name)
        value['status'] = 'PASS'
    except Exception as exc:
        errors.append(str(exc)[:2048])
    finally:
        _save(out, 'tool-snapshot.json', value)
    return value


def _check_snapshot(tools, out, paths):
    errors, observed = [], {}
    try:
        frozen = json.loads(_read_regular(Path(out) / 'tool-snapshot.json', RECEIPT_LIMIT))
        expected_sha = os.environ.get('GITHUB_SHA', '')
        if (not re.fullmatch(r'[0-9a-f]{40}', expected_sha)
                or frozen.get('tool_sha') != expected_sha
                or frozen.get('actual_tool_sha') != expected_sha):
            errors.append('SNAPSHOT_WORKFLOW_SHA_MISMATCH')
        if frozen.get('schema') != 'oasis7/rust-capacity-tool-snapshot/v1' or frozen.get('status') != 'PASS':
            errors.append('SNAPSHOT_NOT_COMPLETE')
        if frozen.get('tools_root') != str(tools):
            errors.append('SNAPSHOT_TOOLS_ROOT_MISMATCH')
        if frozen.get('source_root') != paths['github_workspace']:
            errors.append('SNAPSHOT_WORKSPACE_MISMATCH')
        files = frozen.get('files', {})
        if not isinstance(files, dict) or set(files) != set(TOOL_FILES):
            raise LayoutError('SNAPSHOT_TOOL_SET_DIFFERS')
        for name in TOOL_FILES:
            try:
                item = files[name]
                data = _read_regular(tools / name)
                observed[name] = {'sha256': _sha(data), 'bytes': len(data)}
                if (item.get('source_sha256') != _sha(data)
                        or item.get('snapshot_sha256') != _sha(data)
                        or item.get('bytes') != len(data)):
                    errors.append('TOOL_HASH_MISMATCH:' + name)
                if (item.get('snapshot_path') != str(tools / name)
                        or item.get('source_path') != str(Path(frozen['source_root']) / 'scripts/security' / name)):
                    errors.append('TOOL_SNAPSHOT_PATH_MISMATCH:' + name)
            except Exception as exc:
                errors.append('TOOL_READ_FAILED:' + name + ':' + str(exc)[:512])
    except Exception as exc:
        errors.append('SNAPSHOT_READ_FAILED:' + str(exc)[:1024])
    return errors, observed


def check(source, tools, out, end, source_sha):
    paths, errors = _paths(source, tools)
    source, tools = Path(paths['source']), Path(paths['tools'])
    if Path(__file__).resolve() != tools / Path(__file__).name:
        errors.append('CHECK_HELPER_NOT_FROM_EXTERNAL_SNAPSHOT')
    if not re.fullmatch(r'[0-9a-f]{40}', source_sha):
        errors.append('INVALID_SOURCE_SHA')
    snapshot_errors, files = _check_snapshot(tools, out, paths)
    errors.extend(snapshot_errors)
    identity = {'status': 'NOT_REACHED', 'reason': 'tool snapshot is not trusted'}
    if not snapshot_errors:
        try:
            # Import only after the complete external module snapshot was verified.
            from codeql_rust_capacity_evidence import capture_identity
            identity = capture_identity(source, Path(out), end, source_sha, prefix='isolation')
        except Exception as exc:
            identity = {'status': 'PARTIAL', 'error': repr(exc)[:2048]}
    if identity.get('status') != 'PASS':
        errors.append('BUSINESS_IDENTITY_NOT_PASS')
    value = {'schema': 'oasis7/rust-capacity-collection-layout/v1',
             'status': 'FAIL' if errors else 'PASS', 'source_sha': source_sha,
             'paths': paths, 'tool_snapshot_gate': 'FAIL' if snapshot_errors else 'PASS',
             'observed_tools': files, 'identity': identity, 'errors': errors,
             'expected_action_trace_cwd': {
                 'path': str(source), 'observed': False, 'action_sha': ACTION_SHA,
                 'basis': 'Pinned analyze Action passes process.cwd() to trace-command; '
                          'pre-init check observes this step cwd only. Actual analyze argv '
                          'must be retained in extraction evidence.',
             }}
    _save(out, 'collection-layout.json', value)
    return value


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest='phase', required=True)
    for phase in ('stage', 'check'):
        command = subparsers.add_parser(phase)
        for name in ('source', 'tools', 'output'):
            command.add_argument('--' + name, type=Path, required=True)
        command.add_argument('--deadline', type=float, required=True)
        command.add_argument('--tool-sha' if phase == 'stage' else '--source-sha', required=True)
    args = parser.parse_args(argv)
    end = time.monotonic() + args.deadline - time.time() - 120
    action = stage if args.phase == 'stage' else check
    expected = args.tool_sha if args.phase == 'stage' else args.source_sha
    result = action(args.source, args.tools, args.output, end, expected)
    print(json.dumps({'phase': args.phase, 'status': result['status'], 'errors': result['errors']}))
    return 0 if result['status'] == 'PASS' else 1


if __name__ == '__main__':
    sys.exit(main())
