#!/usr/bin/env python3
"""Manual candidate public-model diagnostics; no security scanning or SARIF upload."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import time

import codeql_rust_diagnostic_runtime as runtime

BUNDLES = {
    '2.27.2': {'rust_all': '0.2.23', 'sha256': 'acf2050189aeede5b9aa566f8fb382b02ac1ace72f534778f68b06fce5371cea'},
    '2.27.3': {'rust_all': '0.2.24', 'sha256': '218a19c49a4e44dee236d2cc55e20cda03ac4f37ddc581b1b0e5fff2e7912237'},
}


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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--codeql', type=Path, required=True)
    parser.add_argument('--bundle-version', choices=BUNDLES, default='2.27.2')
    parser.add_argument('--business-source', type=Path, required=True)
    parser.add_argument('--probe', choices=('node-only', 'endpoints', 'peer-df', 'driver-non-target', 'driver-target', 'calls', 'full', 'trait-scope-plan', 'successor-keyed-plan', 'successor-keyed-eval', 'files-inventory', 'peer-ast-inventory'), default='node-only')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--work', type=Path, required=True)
    parser.add_argument('--deadline', type=float, required=True, help='fixed first-preparation Unix deadline, at most1080s ahead')
    args = parser.parse_args()
    compile_only = args.probe in ("trait-scope-plan", "successor-keyed-plan")
    if (compile_only or args.probe in ("successor-keyed-eval", "files-inventory", "peer-ast-inventory")) and args.bundle_version != "2.27.3":
        parser.error("isolated library probes require complete official 2.27.3")
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
        if compile_only:
            import codeql_rust_trait_scope_plan as scope_plan
            scope_plan.execute(args, env, closure, verify_library_path)
            terminal['status'] = 'COMPILE_ONLY_REQUIRES_PLAN_REVIEW'
        else:
            for toolchain in ('1.96.0', '1.98.1'):
                runtime.run(['rustup', 'run', toolchain, 'rustc', '-vV'], args.work,
                            args.output / ('rustc-' + toolchain + '.log'), args.deadline-90, 20, args.output, env)
            spec = importlib.util.spec_from_file_location('refactored', Path(__file__).with_name('codeql-rust-refactored-source.py'))
            model = importlib.util.module_from_spec(spec); spec.loader.exec_module(model)
            if args.probe == 'peer-ast-inventory':
                import codeql_rust_peer_ast_inventory as prefix
                model.execute(args, env, closure, verify_library_path,
                    comparison=lambda *values: prefix.execute(*values, model))
                terminal['status'] = 'PEER_AST_INVENTORY_RECORDED'
            elif args.probe in ('successor-keyed-eval', 'files-inventory'):
                import codeql_rust_successor_evaluation as comparison
                files_only = args.probe == 'files-inventory'
                args.probe = 'files-inventory' if files_only else 'full'
                model.execute(args, env, closure, verify_library_path,
                    comparison=lambda *values: comparison.execute(*values, model, files_only=files_only))
                terminal['status'] = 'FILES_INVENTORY_RECORDED' if files_only else 'COMPARISON_RECORDED_REQUIRES_REVIEW'
            else:
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
