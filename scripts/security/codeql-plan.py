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

# Exact engineering bundle consumed by the Rust validator and Python CI summary.
# This registers input ownership, not a scan exclusion or an enabled batch.
WASM_CANDIDATE_INPUTS = {
    'crates/oasis7_runtime/src/world/artifacts/builtin_wasm_candidate_baseline.v1.json',
    'crates/oasis7_runtime/src/world/artifacts/m1_builtin_modules.source-rule2-candidate.identity.json',
    'crates/oasis7_runtime/src/world/artifacts/m1_builtin_modules.source-rule2-candidate.sha256',
    'crates/oasis7_runtime/src/world/artifacts/m4_builtin_modules.source-rule2-candidate.identity.json',
    'crates/oasis7_runtime/src/world/artifacts/m4_builtin_modules.source-rule2-candidate.sha256',
    'crates/oasis7_runtime/src/world/artifacts/m5_builtin_modules.source-rule2-candidate.identity.json',
    'crates/oasis7_runtime/src/world/artifacts/m5_builtin_modules.source-rule2-candidate.sha256',
}


# Exact policy fixtures consumed by Rust validation and actual workerd tests.
# Neighboring JSON inputs remain unregistered and fail closed.
WORKERS_POLICY_INPUTS = {
    'crates/oasis7_dev_test_worker/tests/fixtures/policy.json',
    'crates/oasis7_dev_test_worker/tests/fixtures/policy-large-input.json',
    'crates/oasis7_dev_test_worker/tests/fixtures/policy-large-bindings.json',
}


# Exact non-executable GitHub issue forms/chooser metadata. This is not a
# directory or YAML exclusion; possible embedded/build consumers still apply.
ISSUE_METADATA_INPUTS = {
    '.github/ISSUE_TEMPLATE/code-development.yml',
    '.github/ISSUE_TEMPLATE/config.yml',
    '.github/ISSUE_TEMPLATE/decision.yml',
    '.github/ISSUE_TEMPLATE/defect.yml',
    '.github/ISSUE_TEMPLATE/document.yml',
    '.github/ISSUE_TEMPLATE/test.yml',
}

# Exact non-Rust/non-Markdown files in fixed root vendor patches. Neighboring or
# version-bumped paths remain unregistered and fail closed until reviewed.
VENDOR_RUST_METADATA_INPUTS = {
    'vendor-if-watch-3.2.2/Cargo.toml.orig',
    'vendor-libp2p-quic-0.14.0/.gitignore',
    'vendor-libp2p-quic-0.14.0/LICENSE',
    'vendor-libp2p-swarm-0.48.0/LICENSE',
}

VENDOR_PROVENANCE_INPUTS = {
    'vendor-if-watch-3.2.2/upstream-source-sha256.json',
    'vendor-libp2p-rendezvous-0.18.1/LICENSE',
    'vendor-libp2p-rendezvous-0.18.1/local-patch-sha256.json',
    'vendor-libp2p-rendezvous-0.18.1/src/generated/rpc.proto',
    'vendor-libp2p-rendezvous-0.18.1/upstream-source-sha256.json',
    'vendor-libp2p-swarm-0.48.0/upstream-source-sha256.json',
}


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


def inventory(root, oid, *, with_blobs=False):
    entries = git(root, 'ls-tree', '-rz', oid).split(b'\0')
    files, symlinks, blobs = [], set(), {}
    for entry in entries:
        if not entry:
            continue
        meta, name = entry.split(b'\t', 1)
        path = paths(name)[0]
        files.append(path)
        if meta.startswith(b'120000 '):
            symlinks.add(path)
        elif with_blobs:
            parts = meta.split()
            if len(parts) == 3 and parts[1] == b'blob':
                blobs[path] = parts[2]
    return (files, symlinks, blobs) if with_blobs else (files, symlinks)


def read_blobs(root, blob_oids):
    unique = list(dict.fromkeys(blob_oids))
    if not unique:
        return {}
    proc = subprocess.run(['git', '-C', str(root), 'cat-file', '--batch'],
                          input=b''.join(oid + b'\n' for oid in unique), capture_output=True)
    if proc.returncode != 0:
        raise ValueError('embedded input blob batch unreadable')
    data = proc.stdout
    fetched, offset = {}, 0
    for oid in unique:
        end = data.find(b'\n', offset)
        fields = data[offset:end].split() if end >= offset else []
        if len(fields) != 3 or fields[:2] != [oid, b'blob'] or not fields[2].isdigit():
            raise ValueError('invalid embedded input blob header')
        size = int(fields[2])
        start, offset = end + 1, end + 1 + size
        if data[offset:offset + 1] != b'\n':
            raise ValueError('incomplete embedded input blob body')
        fetched[oid] = data[start:offset]
        offset += 1
    if offset != len(data):
        raise ValueError('unexpected trailing embedded input blob data')
    return fetched


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
    scope = getattr(args, 'maintenance_scope', 'all')
    if scope not in ('all', 'non-rust', 'rust') or (args.event == 'pull_request' and scope != 'all'):
        raise ValueError('invalid maintenance scope for event')
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
    base_files, base_links, base_blobs = inventory(root, ids['merge_base'], with_blobs=True)
    if ids['merge_base'] == ids['source_head']:
        head_files, head_links, head_blobs = base_files, base_links, base_blobs
    else:
        head_files, head_links, head_blobs = inventory(root, ids['source_head'], with_blobs=True)
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
    trees = [(ids['merge_base'], base_files, base_blobs)]
    if ids['source_head'] != ids['merge_base']:
        trees.append((ids['source_head'], head_files, head_blobs))
    matched_sources = []
    for oid, files, blobs in trees:
        build_scripts |= any(pathlib.PurePosixPath(p).name == 'build.rs' and not p.startswith('third_party/') for p in files)
        matches = subprocess.run(['git', '-C', str(root), 'grep', '-I', '-l', '-z', '-E',
                                  'include_(str|bytes)!', oid, '--', '*.rs'], capture_output=True)
        if matches.returncode not in (0, 1):
            raise ValueError('embedded input inventory unreadable')
        for entry in paths(matches.stdout):
            source = entry.split(':', 1)[1]
            if source.startswith('third_party/'):
                continue
            blob_oid = blobs.get(source)
            if blob_oid is None:
                raise ValueError('embedded input blob missing from tree inventory')
            matched_sources.append((source, blob_oid))
    blob_contents = read_blobs(root, [blob_oid for _, blob_oid in matched_sources])
    for source, blob_oid in matched_sources:
        content = blob_contents[blob_oid].decode('utf-8', 'strict')
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
        elif name == '.gitattributes':
            all_units('Git checkout attributes: ' + path)
        elif path in VENDOR_RUST_METADATA_INPUTS:
            # Metadata travels with these fixed Rust dependencies; do not treat
            # neighboring unclassified files as registered or excluded.
            add('rust-repo', path)
        elif path in VENDOR_PROVENANCE_INPUTS:
            # Consumed by scripts/libp2p-security-source.test.py and paired with
            # the fixed Rust vendor packages; neighboring files stay closed.
            add('python-repo', path)
            add('rust-repo', path)
        elif suffix == '.rs' or name in ('Cargo.toml', 'Cargo.lock', 'rust-toolchain', 'rust-toolchain.toml') or path.startswith('.cargo/'):
            add('rust-repo', path)
        elif suffix in ('.py', '.pyi') or name in ('pyproject.toml', 'requirements.txt', 'Pipfile', 'Pipfile.lock', 'poetry.lock', 'uv.lock', 'setup.cfg', 'setup.py'):
            add('python-repo', path)
        elif path == 'crates/oasis7_viewer/viewer_terminal_shell.css':
            # Exact Viewer HTML/build/test input; other CSS stays unregistered.
            # Possible Rust consumers are still added below.
            add('javascript-repo', path)
        elif suffix in ('.js', '.jsx', '.ts', '.tsx', '.mjs', '.cjs', '.html', '.htm', '.vue', '.svelte', '.hbs', '.ejs') or name in ('package.json', 'package-lock.json', 'yarn.lock', 'pnpm-lock.yaml', 'tsconfig.json'):
            add('javascript-repo', path)
        elif path.startswith('.github/workflows/') or name in ('action.yml', 'action.yaml'):
            add('actions-repo', path)
        elif path in ('scripts/ci-required-scope.json', 'scripts/cargo-dev-architecture.json'):
            # Fixed CI policies consumed by the Python planning/architecture
            # tools and rust.yml's selection or execution. This is not a blanket
            # JSON registration; possible Rust consumers are still added below.
            add('actions-repo', path)
            add('python-repo', path)
        elif path == 'deny.toml':
            # Exact cargo-deny policy consumed by CI shell/Actions, not Rust source.
            # Preserve possible embedded/build consumers below and batch limits.
            add('actions-repo', path)
        elif path in ('.dockerignore', 'docker/wasm-builder/Dockerfile'):
            # Fixed canonical builder inputs consumed by Actions, Python context
            # contracts and Rust producer recipes. Keep the consumer union below.
            add('actions-repo', path)
            add('python-repo', path)
            add('rust-repo', path)
        elif path in ('docker/ci-base/Dockerfile', 'docker/ci-base/build.sh',
                      'docker/ci-base/runtime-packages.txt', 'docker/ci-base/build-packages.txt'):
            add('actions-repo', path)
        elif path in ISSUE_METADATA_INPUTS:
            # GitHub renders these as form data, not executable Actions YAML.
            pass
        elif path in WASM_CANDIDATE_INPUTS:
            add('python-repo', path)
            add('rust-repo', path)
        elif path == 'tools/scenario_test_runner/scenarios/triad_p2p.yaml':
            # Exact data fixture read by the Rust scenario runner, not Actions YAML.
            add('rust-repo', path)
        elif path in WORKERS_POLICY_INPUTS:
            add('javascript-repo', path)
            add('rust-repo', path)
        elif path == 'crates/oasis7_dev_test_worker/wrangler.toml':
            # Exact deployment input for the JS ingress and Rust Wasm/DO class.
            # Other TOML remains unregistered; this does not enable any batch.
            add('javascript-repo', path)
            add('rust-repo', path)
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
        elif (scope == 'rust' and unit != 'rust-repo') or (scope == 'non-rust' and unit == 'rust-repo'):
            not_selected[unit] = 'maintenance_scope'
        else:
            selected.append(unit)
    matrix = []
    for index, (unit, language) in enumerate(UNITS):
        if unit in selected:
            matrix.append(dict(unit=unit, language=language, category=f'oasis7/{unit}/{args.profile}',
                               slot=(index + args.pr_number) % args.max_slots, build_mode='none',
                               queries='' if args.profile == 'default' else 'security-extended',
                               timeout_minutes=90 if language == 'rust' and args.profile == 'default' and args.event != 'pull_request'
                               else (45 if language == 'rust' or args.profile == 'extended' else 20)))
    return dict(schema='oasis7-codeql-plan/v1', status='disabled' if not enabled or (candidates and not selected) else ('selected' if selected else 'not_applicable'),
                profile=args.profile, maintenance_scope=scope, identity=ids, policy=dict(path=str(args.policy), sha256=hashlib.sha256(raw).hexdigest()),
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
    parser.add_argument('--maintenance-scope', choices=['all', 'non-rust', 'rust'], default='all')
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
