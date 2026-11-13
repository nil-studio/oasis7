#!/usr/bin/env python3
"""Manual candidate public-model diagnostics; no security scanning or SARIF upload."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import shutil
import time

import codeql_rust_diagnostic_runtime as runtime

BUNDLES = {
    '2.27.2': {'rust_all': '0.2.23', 'sha256': 'acf2050189aeede5b9aa566f8fb382b02ac1ace72f534778f68b06fce5371cea'},
    '2.27.3': {'rust_all': '0.2.24', 'sha256': '218a19c49a4e44dee236d2cc55e20cda03ac4f37ddc581b1b0e5fff2e7912237'},
    '2.27.4': {'rust_all': '0.2.25', 'sha256': '1c2f867094b80e43c9a0e511090e57e0ea3bf107600b4b3763c52b9020b4ec4b'},
}
# Shared closure metadata does not expand the historical model diagnostic CLI.
MODEL_BUNDLE_VERSIONS = ('2.27.2', '2.27.3')
SHARED_FILE = 'codeql/typeinference/internal/TypeInference.qll'
SHARED_OFFICIAL_SHA = '2a6d0a6be96444593e6294cb848633256f228f0ff137d42357f1ee4b92724378'
SHARED_CANDIDATE_SHA = '4a65a578b82c8fa739c2a802b2cc6763cebc34cb67be9008fd396d5dbb5bd47b'
SHARED_PATCH_SHA = '6b5a4cd3443caca8e994f05aa3c57e34b23c6fe3dfdc27851eb5f5c6ebf3c36c'
SHARED_UPSTREAM = '34c3f5b74d3acee241d57c596044de3f1a0c8918'


def pack_manifest(path):
    """Strict subset of the frozen official library manifest, not a general YAML loader."""
    text = path.read_text()
    def field(name):
        values = re.findall(r'^' + name + r': ([a-zA-Z0-9/_.-]+)$', text, re.M)
        if len(values) != 1:
            raise RuntimeError('unsupported official manifest field: ' + name)
        return values[0]
    if field('library') != 'true':
        raise RuntimeError('dependency is not an official library')
    match = re.search(r'^dependencies:\n((?:[ \t].*\n)*)', text, re.M)
    dependencies = {}
    declarations = re.findall(r'^dependencies:.*$', text, re.M)
    if len(declarations) != 1 or (not match and declarations != ['dependencies: {}']):
        raise RuntimeError('unsupported official dependency declaration')
    if match:
        for line in match.group(1).splitlines():
            dep = re.fullmatch(r'  (codeql/[a-z-]+): (\d+\.\d+\.\d+)', line)
            if not dep or dep[1] in dependencies:
                raise RuntimeError('unsupported or duplicate exact library dependency')
            dependencies[dep[1]] = dep[2]
    return {'name': field('name'), 'version': field('version'), 'dependencies': dependencies,
            'manifest_sha256': runtime.digest(path)}


def bundle_closure(bundle, version):
    selected, pending = {}, [('codeql/rust-all', BUNDLES[version]['rust_all'])]
    while pending:
        name, wanted = pending.pop()
        if name in selected:
            if selected[name]['version'] != wanted:
                raise RuntimeError('conflicting official library versions')
            continue
        if len(selected) >= 32:
            raise RuntimeError('official dependency closure bound exceeded')
        root = bundle / 'qlpacks' / name / wanted
        manifest = pack_manifest(root / 'qlpack.yml')
        if manifest['name'] != name or manifest['version'] != wanted or root.is_symlink():
            raise RuntimeError('official pack path/name/version differs')
        selected[name] = dict(manifest, root=str(root))
        pending.extend(manifest['dependencies'].items())
    return selected


def verify_library_path(value, query, name, closure, pack_name="oasis7/rust-model-diagnostic"):
    keys = {'libraryPath', 'dbscheme', 'compilationCache', 'relativeName', 'qlPackName'}
    if not isinstance(value, dict) or set(value) != keys:
        raise RuntimeError('unknown actual library-path schema')
    paths = value['libraryPath']
    expected = {p['root'] for p in closure.values()} | {str(query)}
    if not isinstance(paths, list) or not all(isinstance(p, str) for p in paths) or len(paths) != len(set(paths)) or set(paths) != expected:
        raise RuntimeError('actual library dependency closure differs from selected bundle')
    if value['dbscheme'] != str(Path(closure['codeql/rust-all']['root']) / 'rust.dbscheme'):
        raise RuntimeError('actual dbscheme differs')
    if value['qlPackName'] != pack_name or value['relativeName'] != f'{pack_name}/{name}.ql':
        raise RuntimeError('actual query identity differs')
    if not isinstance(value['compilationCache'], list) or not all(isinstance(p, str) for p in value['compilationCache']):
        raise RuntimeError('unknown actual compilation cache schema')


def shared_helper_identity(original, private, *, snapshot):
    """Fixed shared helper only; each caller supplies its bounded regular-file inventory."""
    if any(root.resolve() != root or not root.is_dir() for root in (original, private)):
        raise RuntimeError('unsafe shared helper root')
    return shared_snapshot_identity(snapshot(original), snapshot(private))


def shared_snapshot_identity(official_files, candidate_files):
    """Validate the fixed helper against actual or saved intermediate snapshots."""
    changed = sorted(p for p in set(official_files) | set(candidate_files)
                     if official_files.get(p) != candidate_files.get(p))
    if (not 0 < len(official_files) <= 8192 or set(official_files) != set(candidate_files)
            or changed != [SHARED_FILE]
            or official_files.get(SHARED_FILE, {}).get('sha256') != SHARED_OFFICIAL_SHA
            or candidate_files.get(SHARED_FILE, {}).get('sha256') != SHARED_CANDIDATE_SHA
            or official_files[SHARED_FILE]['mode'] != candidate_files[SHARED_FILE]['mode']):
        raise RuntimeError('shared candidate differs from the fixed single-file helper patch')
    return dict(upstream_commit=SHARED_UPSTREAM, patch_sha256=SHARED_PATCH_SHA, changed_files=changed,
                official_files=official_files, candidate_files=candidate_files)


def prepare_shared_helper(entry, private, patch_file, *, snapshot, copy_file, file_digest, run):
    """Reuse caller deadlines/supervision; never patch the official package or choose another patch."""
    original = Path(entry['root'])
    if (original.resolve() != original or not original.is_dir() or private.resolve() != private
            or private.exists() or private.is_relative_to(original) or original.is_relative_to(private)
            or patch_file.resolve() != patch_file or not patch_file.is_file()):
        raise RuntimeError('unsafe shared helper preparation paths')
    official_files = snapshot(original)
    if (entry.get('name') != 'codeql/typeinference' or entry.get('version') != '0.0.42'
            or pack_manifest(original / 'qlpack.yml') != {k: v for k, v in entry.items() if k != 'root'}
            or not 0 < len(official_files) <= 8192
            or file_digest(patch_file) != SHARED_PATCH_SHA
            or official_files.get(SHARED_FILE, {}).get('sha256') != SHARED_OFFICIAL_SHA):
        raise RuntimeError('shared helper patch or official type-inference identity differs')
    shutil.copytree(original, private, copy_function=copy_file)
    run(['patch', '--batch', '--fuzz=0', private / SHARED_FILE, patch_file], 'apply-shared-helper-patch', 20)
    identity = shared_helper_identity(original, private, snapshot=snapshot)
    if identity['official_files'] != official_files:
        raise RuntimeError('official shared package changed during preparation')
    return identity


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--codeql', type=Path, required=True)
    parser.add_argument('--bundle-version', choices=MODEL_BUNDLE_VERSIONS, default='2.27.2')
    parser.add_argument('--business-source', type=Path, required=True)
    parser.add_argument('--probe', choices=('node-only', 'endpoints', 'peer-df', 'driver-non-target', 'driver-target', 'calls', 'full', 'trait-scope-plan'), default='node-only')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--work', type=Path, required=True)
    parser.add_argument('--deadline', type=float, required=True, help='fixed first-preparation Unix deadline, at most1080s ahead')
    args = parser.parse_args()
    if args.probe == "trait-scope-plan" and args.bundle_version != "2.27.3":
        parser.error("trait-scope-plan requires complete official 2.27.3")
    remaining = args.deadline - time.time()
    if not 0 < remaining <= 1080:
        parser.error('invalid fixed total deadline')
    args.fixed_wall_deadline = args.deadline
    args.deadline = time.monotonic() + remaining
    args.codeql = args.codeql.resolve()
    args.output.mkdir(exist_ok=False)
    args.work.mkdir(exist_ok=False)
    terminal = {'status': 'FAILED', 'probe': args.probe, 'full_rust_coverage': 'HOLD'}
    env = {k: v for k, v in os.environ.items() if k not in ('GH_TOKEN', 'GITHUB_TOKEN', 'RUSTC_WRAPPER')}
    env.update(CARGO_NET_OFFLINE='true', RUSTUP_TOOLCHAIN='1.96.0')
    try:
        runtime.save(args.output / 'identity.json', {'run': os.environ.get('GITHUB_RUN_ID'),
            'attempt': os.environ.get('GITHUB_RUN_ATTEMPT'), 'source': os.environ.get('GITHUB_SHA'),
            'bundle': BUNDLES[args.bundle_version], 'bundle_version': args.bundle_version,
            'fixed_wall_deadline': args.fixed_wall_deadline, 'platform': platform.platform(),
            'script_sha256': runtime.digest(Path(__file__))}, args.output)
        runtime.run([str(args.codeql), 'version', '--format=json'], args.work,
                    args.output / 'codeql-version.json', args.deadline-90, 20, args.output, env)
        if json.loads((args.output / 'codeql-version.json').read_text())['version'] != args.bundle_version:
            raise RuntimeError('actual CLI version differs from selected bundle')
        closure = bundle_closure(args.codeql.parent, args.bundle_version)
        runtime.save(args.output / 'bundle-library-closure.json', closure, args.output)
        if args.probe == "trait-scope-plan":
            import codeql_rust_trait_scope_plan as scope_plan
            scope_plan.execute(args, env, closure, verify_library_path)
            terminal['status'] = 'COMPILE_ONLY_REQUIRES_PLAN_REVIEW'
        else:
            for toolchain in ('1.96.0', '1.98.1'):
                runtime.run(['rustup', 'run', toolchain, 'rustc', '-vV'], args.work,
                            args.output / ('rustc-' + toolchain + '.log'), args.deadline-90, 20, args.output, env)
            spec = importlib.util.spec_from_file_location('refactored', Path(__file__).with_name('codeql-rust-refactored-source.py'))
            model = importlib.util.module_from_spec(spec); spec.loader.exec_module(model)
            model.execute(args, env, closure, verify_library_path)
            terminal['status'] = 'SELECTED_PROBE_COMPLETED'
    except BaseException as exc:
        terminal['error'] = repr(exc)[:8192]
        raise
    finally:
        runtime.save(args.output / 'terminal.json', terminal, args.output, terminal=True)
        hashes = {str(p.relative_to(args.output)): runtime.digest(p) for p in args.output.rglob('*') if p.is_file()}
        if len(hashes) > 256 or any(len(name) > 160 for name in hashes) or len((json.dumps(hashes,indent=2)+'\n').encode()) > runtime.MAX_HASH_RECEIPT:
            raise RuntimeError('frozen hash index size admission exceeded')
        runtime.save(args.output / 'evidence-hashes.json', hashes, args.output, terminal=True)


if __name__ == '__main__':
    main()
