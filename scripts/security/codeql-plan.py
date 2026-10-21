#!/usr/bin/env python3
"""Stdlib-only advisory routing. Run trusted base code/policy, never candidate imports."""
import argparse
import hashlib
import json
import pathlib
import posixpath
import re
import subprocess
import sys

UNITS = [('actions-repo', 'actions'), ('python-repo', 'python'),
         ('javascript-repo', 'javascript-typescript'), ('rust-repo', 'rust')]


def batch_modes(raw):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError("duplicate batch key")
            result[key] = value
        return result
    value = json.loads(raw or "{}", object_pairs_hook=unique)
    allowed = {unit + "/" + profile for unit, _ in UNITS for profile in ("default", "extended")}
    if not isinstance(value, dict) or any(key not in allowed or mode not in ("baseline", "observe")
                                        or (key.endswith("/extended") and mode == "observe")
                                        for key, mode in value.items()):
        raise ValueError("invalid CodeQL batch modes")
    return value


def git(root, *args):
    result = subprocess.run(['git', '-C', str(root), *args], capture_output=True)
    if result.returncode:
        raise ValueError('Git input unreadable: ' + result.stderr.decode('utf-8', 'replace').strip())
    return result.stdout


def paths(data):
    values = data.decode('utf-8', 'strict').split('\0')
    result = [v for v in values if v]
    for path in result:
        if path.startswith('/') or '..' in pathlib.PurePosixPath(path).parts:
            raise ValueError('unsafe repository path')
    return result


def inventory(root, oid):
    entries = git(root, 'ls-tree', '-rz', oid).split(b'\0')
    files, symlinks = [], set()
    for entry in entries:
        if not entry:
            continue
        meta, name = entry.split(b'\t', 1)
        path = paths(name)[0]
        files.append(path)
        if meta.startswith(b'120000 '):
            symlinks.add(path)
    return files, symlinks


def changes(root, base, head):
    tokens = paths(git(root, 'diff', '--name-status', '-z', '--find-renames', base, head, '--'))
    result = []
    index = 0
    while index < len(tokens):
        status = tokens[index]
        count = 2 if status[0] in 'RC' else 1
        if status[0] not in 'AMDTURC' or index + count >= len(tokens):
            raise ValueError('incomplete Git diff')
        result.extend(tokens[index + 1:index + count + 1])
        index += count + 1
    return sorted(set(result))


def plan(args):
    batches = batch_modes(getattr(args, 'batch_modes', '{}'))
    root = pathlib.Path(args.repo_root).resolve()
    raw = pathlib.Path(args.policy).read_bytes()
    policy = json.loads(raw)
    if policy.get('schema') != 'oasis7-codeql-policy/v1' or policy.get('units') != [dict(unit=u, language=l) for u, l in UNITS]:
        raise ValueError('invalid fixed-unit policy')
    # Exclusions are fixed: a candidate policy must not silently widen them.
    exclusions = ['third_party/', 'target/', 'node_modules/', 'dist/', '.pm/']
    if policy.get('excluded_prefixes') != exclusions:
        raise ValueError('unapproved exclusion policy')
    ids = {}
    for key, ref in [('event_base', args.base), ('source_head', args.head), ('checkout', args.checkout or 'HEAD')]:
        if ref.startswith('-'):
            raise ValueError('invalid Git identity')
        ids[key] = git(root, 'rev-parse', '--verify', ref + '^{commit}').decode().strip()
    ids['merge_base'] = git(root, 'merge-base', ids['event_base'], ids['source_head']).decode().strip()
    base_files, base_links = inventory(root, ids['merge_base'])
    head_files, head_links = inventory(root, ids['source_head'])
    changed = changes(root, ids['merge_base'], ids['source_head'])
    if set(changed) & (base_links | head_links):
        raise ValueError('changed symbolic link requires explicit coverage review')
    manifests = {side: [p for p in files if pathlib.PurePosixPath(p).name == 'Cargo.toml' and not p.startswith('third_party/')]
                 for side, files in [('base', base_files), ('head', head_files)]}
    reasons = {u: [] for u, _ in UNITS}
    fallbacks = []
    def add(unit, reason):
        reasons[unit].append(reason)
    def all_units(reason):
        fallbacks.append(reason)
        for unit in reasons:
            add(unit, reason)
    # Literal includes are resolved relative to the source file. Dynamic generation
    # cannot be proven from text, so any build.rs conservatively links data inputs.
    embedded = set()
    build_scripts = False
    dynamic_includes = False
    action_consumers = any(p.startswith('.github/workflows/') or pathlib.PurePosixPath(p).name in ('action.yml', 'action.yaml')
                           for p in base_files + head_files if not p.startswith(tuple(exclusions)))
    for oid, files in [(ids['merge_base'], base_files), (ids['source_head'], head_files)]:
        build_scripts |= any(pathlib.PurePosixPath(p).name == 'build.rs' and not p.startswith('third_party/') for p in files)
        matches = subprocess.run(['git', '-C', str(root), 'grep', '-I', '-l', '-z', '-E',
                                  'include_(str|bytes)!', oid, '--', '*.rs'], capture_output=True)
        if matches.returncode not in (0, 1):
            raise ValueError('embedded input inventory unreadable')
        for entry in paths(matches.stdout):
            source = entry.split(':', 1)[1]
            if source.startswith('third_party/'):
                continue
            content = git(root, 'show', oid + ':' + source).decode('utf-8', 'strict')
            literal_matches = list(re.finditer(r'include_(?:str|bytes)!\s*\(\s*"([^"\n]+)"', content))
            dynamic_includes |= len(literal_matches) < len(re.findall(r'include_(?:str|bytes)!', content))
            for match in literal_matches:
                embedded.add(posixpath.normpath(str(pathlib.PurePosixPath(source).parent / match[1])))
    for path in changed:
        name = pathlib.PurePosixPath(path).name
        suffix = pathlib.PurePosixPath(path).suffix.lower()
        if path.startswith(tuple(exclusions)):
            continue
        if path.startswith('scripts/security/') or path == '.github/workflows/codeql.yml':
            all_units('scan configuration: ' + path)
        elif suffix == '.rs' or name in ('Cargo.toml', 'Cargo.lock', 'rust-toolchain', 'rust-toolchain.toml') or path.startswith('.cargo/'):
            add('rust-repo', path)
        elif suffix in ('.py', '.pyi') or name in ('pyproject.toml', 'requirements.txt', 'Pipfile', 'Pipfile.lock', 'poetry.lock', 'uv.lock', 'setup.cfg', 'setup.py'):
            add('python-repo', path)
        elif suffix in ('.js', '.jsx', '.ts', '.tsx', '.mjs', '.cjs', '.html', '.htm', '.vue', '.svelte', '.hbs', '.ejs') or name in ('package.json', 'package-lock.json', 'yarn.lock', 'pnpm-lock.yaml', 'tsconfig.json'):
            add('javascript-repo', path)
        elif path.startswith('.github/workflows/') or name in ('action.yml', 'action.yaml'):
            add('actions-repo', path)
        elif suffix not in ('.md', '.rst', '.txt', '.adoc', '.sh'):
            raise ValueError('unregistered input: ' + path)
        # build.rs can read arbitrary repository data or invoke generators. No
        # suffix proves non-consumption, including documents and scripts. Keep
        # old-side consumers when the build script itself has been removed.
        if path in embedded or dynamic_includes or build_scripts:
            add('rust-repo', 'embedded or possible build input: ' + path)
        # Shell is not a CodeQL language. A script can still change a workflow
        # boundary; dynamic commands prevent a complete literal consumer graph.
        # With any old/new workflow or local action, conservatively select Actions.
        if suffix == '.sh' and action_consumers:
            add('actions-repo', 'workflow or possible action input: ' + path)
    if args.full:
        all_units('full maintenance scan')
    enabled = args.mode != 'off'
    if args.event != 'pull_request':
        default_oid = git(root, 'rev-parse', '--verify', args.default_branch + '^{commit}').decode().strip()
        if enabled and (ids['source_head'] != default_oid or ids['checkout'] != default_oid):
            raise ValueError('maintenance requires trusted default branch checkout')
    elif args.mode == 'baseline':
        enabled = False
    candidates = [u for u, _ in UNITS if reasons[u]]
    selected, not_selected = [], {}
    for unit, _ in UNITS:
        mode = batches.get(unit + "/" + args.profile)
        if not enabled:
            not_selected[unit] = "global_or_event_disabled"
        elif unit not in candidates:
            not_selected[unit] = "not_affected"
        elif mode is None:
            not_selected[unit] = "batch_disabled"
        elif args.event == "pull_request" and (args.profile != "default" or mode != "observe"):
            not_selected[unit] = "pr_baseline_only"
        else:
            selected.append(unit)
    matrix = []
    for index, (unit, language) in enumerate(UNITS):
        if unit in selected:
            matrix.append(dict(unit=unit, language=language, category=f'oasis7/{unit}/{args.profile}',
                               slot=(index + args.pr_number) % args.max_slots, build_mode='none',
                               queries='' if args.profile == 'default' else 'security-extended',
                               timeout_minutes=45 if language == 'rust' or args.profile == 'extended' else 20))
    return dict(schema='oasis7-codeql-plan/v1', status='disabled' if not enabled or (candidates and not selected) else ('selected' if selected else 'not_applicable'),
                profile=args.profile, identity=ids, policy=dict(path=str(args.policy), sha256=hashlib.sha256(raw).hexdigest()),
                completeness=dict(complete=True, diff_paths=changed, manifest_inventory=manifests),
                candidate_units=candidates, batch_modes=batches, not_selected_reasons=not_selected,
                selected_units=selected, has_units=bool(selected), reasons={u: sorted(set(v)) for u, v in reasons.items()},
                fallback_reasons=sorted(set(fallbacks)), expected_coverage=policy['coverage_note'], matrix=dict(include=matrix))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for arg in ('repo-root', 'base', 'head', 'policy', 'output'):
        parser.add_argument('--' + arg, required=True)
    parser.add_argument('--checkout')
    parser.add_argument('--batch-modes', default='{}')
    parser.add_argument('--profile', choices=['default', 'extended'], default='default')
    parser.add_argument('--mode', choices=['off', 'baseline', 'observe'], default='off')
    parser.add_argument('--event', choices=['pull_request', 'schedule', 'workflow_dispatch'], default='pull_request')
    parser.add_argument('--default-branch', default='main')
    parser.add_argument('--full', action='store_true')
    parser.add_argument('--pr-number', type=int, default=0)
    parser.add_argument('--max-slots', type=int, choices=[1, 2], default=2)
    args = parser.parse_args()
    try:
        result = plan(args)
        pathlib.Path(args.output).write_text(json.dumps(result, indent=2) + '\n')
    except (ValueError, OSError, UnicodeError, json.JSONDecodeError) as exc:
        print('CodeQL planner error: ' + str(exc), file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
