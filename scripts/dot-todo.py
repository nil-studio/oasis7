#!/usr/bin/env python3
"""Single-executor, local TODO snapshot transitions; never executes task text."""
import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile


class Stop(Exception):
    """Input or state cannot safely advance."""


def digest(value):
    return hashlib.sha256(json.dumps(value, ensure_ascii=False, sort_keys=True,
                                     separators=(',', ':'), allow_nan=False).encode()).hexdigest()


def require(condition, message):
    if not condition:
        raise Stop(message)


def nonempty(value):
    return isinstance(value, str) and bool(value.strip())


def strings(value):
    return isinstance(value, list) and all(nonempty(x) for x in value)


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'duplicate JSON key')
        result[key] = value
    return result


def read_json(path):
    return json.loads(path.read_text(encoding='utf-8'), object_pairs_hook=unique_object,
                      parse_constant=lambda _: (_ for _ in ()).throw(Stop('invalid number')))


def validate(state):
    require(isinstance(state, dict) and set(state) == {'schema', 'revision', 'tasks', 'checksum'},
            'invalid snapshot fields')
    require(type(state['schema']) is int and state['schema'] == 1, 'unsupported schema')
    require(type(state['revision']) is int and state['revision'] >= 0, 'invalid revision')
    require(state['checksum'] == digest({k: v for k, v in state.items() if k != 'checksum'}),
            'snapshot checksum mismatch')
    require(isinstance(state['tasks'], list), 'invalid tasks')
    ids = set()
    active = 0
    for task in state['tasks']:
        require(isinstance(task, dict) and set(task) == {'id', 'text', 'status', 'history'},
                'invalid task fields')
        require(nonempty(task['id']) and task['id'] not in ids and nonempty(task['text']),
                'invalid or duplicate task identity')
        ids.add(task['id'])
        require(task['status'] in {'pending', 'claimed', 'running', 'completed', 'blocked', 'unknown'},
                'invalid task status')
        active += task['status'] in {'claimed', 'running', 'unknown'}
        require(isinstance(task['history'], list), 'invalid history')
        previous = 'pending'
        for entry in task['history']:
            validate_entry(entry, previous)
            previous = entry['status']
        require(previous == task['status'], 'history/status mismatch')
    require(active <= 1, 'multiple active tasks')
    require(sum(len(t['history']) for t in state['tasks']) == state['revision'],
            'revision/history mismatch')


def validate_entry(entry, previous):
    require(isinstance(entry, dict) and set(entry) == {'status', 'evidence', 'remaining'},
            'invalid history entry')
    status = entry['status']
    allowed = {'pending': {'claimed'}, 'claimed': {'running', 'blocked', 'unknown'},
               'running': {'completed', 'blocked', 'unknown'},
               'unknown': {'completed', 'blocked'}, 'blocked': {'completed'}, 'completed': set()}
    require(isinstance(status, str) and status in allowed[previous], 'unsafe transition')
    require(strings(entry['evidence']) and strings(entry['remaining']), 'invalid evidence/remaining')
    if status in {'completed', 'blocked', 'unknown'}:
        require(bool(entry['evidence']), 'result evidence required')
        require(bool(entry['remaining']) == (status != 'completed'), 'remaining obligations mismatch')


def seal(state):
    state['checksum'] = digest({k: v for k, v in state.items() if k != 'checksum'})
    return state


@contextmanager
def locked(path):
    # Persistent sidecar inode: never unlink it, including after a failed invocation.
    import fcntl
    fd = os.open(str(path) + '.lock', os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
    try:
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise Stop('local snapshot is locked') from error
        yield
    finally:
        os.close(fd)


def atomic_write(path, state):
    fd, name = tempfile.mkstemp(prefix='.' + path.name + '-', dir=path.parent)
    try:
        with os.fdopen(fd, 'w', encoding='utf-8') as stream:
            json.dump(state, stream, ensure_ascii=False, indent=2, allow_nan=False)
            stream.write('\n')
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(name, path)
        directory = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        if os.path.exists(name):
            os.unlink(name)


def run(args):
    raw = Path(args.state).absolute()
    require(not raw.is_symlink(), 'snapshot symlink is unsupported')
    path = raw.resolve(strict=True)
    with locked(path):
        state = read_json(path)
        validate(state)
        require(state['revision'] == args.revision and state['checksum'] == args.checksum,
                'snapshot conflicts with expected revision/checksum; restore and reconcile')
        selected = None
        if args.command == 'claim':
            require(not any(t['status'] in {'claimed', 'running', 'unknown'} for t in state['tasks']),
                    'existing active/unknown task: reconcile original operation; do not retry')
            selected = next((t for t in state['tasks'] if t['status'] == 'pending'), None)
            require(selected is not None, 'no pending task')
            entry = {'status': 'claimed', 'evidence': [], 'remaining': []}
        elif args.command in {'start', 'result'}:
            selected = next((t for t in state['tasks'] if t['id'] == args.id), None)
            require(selected is not None, 'unknown task ID')
            if args.command == 'start':
                entry = {'status': 'running', 'evidence': [], 'remaining': []}
            else:
                result = read_json(Path(args.result))
                require(isinstance(result, dict) and set(result) ==
                        {'id', 'text_checksum', 'status', 'evidence', 'remaining'}, 'invalid result fields')
                require(result['id'] == selected['id'] and result['text_checksum'] == digest(selected['text']),
                        'result does not match original obligation')
                require(result['status'] in {'completed', 'blocked', 'unknown'}, 'invalid result status')
                entry = {k: result[k] for k in ('status', 'evidence', 'remaining')}
        if selected is not None:
            validate_entry(entry, selected['status'])
            selected['status'] = entry['status']
            selected['history'].append(entry)
            state['revision'] += 1
            seal(state)
            validate(state)
            atomic_write(path, state)
        return {'snapshot': state, 'selected': selected,
                'text_checksum': digest(selected['text']) if selected else None}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--state', required=True, help='existing private local snapshot')
    parser.add_argument('--revision', required=True, type=int, help='trusted expected revision')
    parser.add_argument('--checksum', required=True, help='trusted expected snapshot checksum')
    parser.add_argument('--format', choices=['json', 'text'], default='json')
    commands = parser.add_subparsers(dest='command', required=True)
    commands.add_parser('show')
    commands.add_parser('claim')
    commands.add_parser('start').add_argument('--id', required=True)
    result = commands.add_parser('result')
    result.add_argument('--id', required=True)
    result.add_argument('--result', required=True, help='dot-verified result JSON')
    args = parser.parse_args()
    try:
        output = run(args)
        if args.format == 'json':
            print(json.dumps(output, ensure_ascii=False, indent=2))
        else:
            state = output['snapshot']
            print(f"revision={state['revision']} checksum={state['checksum']}")
            for task in state['tasks']:
                print(f"[{task['status']}] {task['id']}: {task['text']}")
                if task['history']:
                    for remaining in task['history'][-1]['remaining']:
                        print(f'  remaining: {remaining}')
    except (Stop, OSError, ValueError, TypeError, KeyError, ImportError) as error:
        print(f'STOP: {error}. Re-read and reconcile before any external action.', file=sys.stderr)
        return 2
    return 0


if __name__ == '__main__':
    sys.exit(main())
