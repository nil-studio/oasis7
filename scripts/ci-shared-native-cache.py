#!/usr/bin/env python3
"""Build the complete trusted native dependency cache without running tests.

Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7.
This is an optional cache producer, never a verification or release receipt.
Upload authorization and measured archive admission remain separate.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
import sys


sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent.parent


def load(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'scripts' / (name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


artifacts = load('ci-test-artifacts')
planner = load('plan-rust-required-scope')


def build_variants(root):
    """One target directory, including every context formerly warmed by full."""
    artifacts.ROOT = root
    config = json.loads((root / 'scripts/ci-required-scope.json').read_text())
    planner.validate_config(config)
    # Validate the entire trusted inventory before the first Cargo invocation.
    # This is not a narrowed PR plan and grants no candidate write authority.
    targets = artifacts.target_inventory({
        'version': config['fast_test_inventory_version'],
        'targets': list(config['ordinary_fast_targets'].values()),
    }, 'version', 'targets')
    packages = [arg for package in artifacts.required_packages() for arg in ('-p', package)]
    variants = []

    def add(name, args, environment=None):
        variants.append((name, ['cargo', *args, '--locked'], environment or {}))

    def tier(name, features, selectors):
        add(name + '-viewer', ['build', '-p', 'oasis7_viewer_runtime', '--bin',
                              'oasis7_viewer_live', '--features', features])
        add(name, ['test', *packages, *selectors, '--features', features, '--no-run'])

    tier('full', 'test_tier_full,wasmtime,viewer_live_integration', ['--tests'])
    for package in sorted(planner.SUPPORT_PROFILES):
        features = artifacts.package_features([package], include_full=True)
        args = ['test', '-p', package, '--all-targets', '--no-run']
        if features:
            args += ['--features', ','.join(features)]
        environment = ({'OASIS7_WASM_BUILD_STD': '0'}
                       if package in ('wasm_build_suite', 'wasm_module_observe') else {})
        add('support-' + package, args, environment)
    # The fixture's two provider filters share one exact compile context. Its
    # actual three behavior checks and the production guard remain in full.
    add('fixture-world', ['test', '-p', 'oasis7', '--features', 'test_tier_full', '--no-run'])
    add('fixture-provider', ['test', '-p', 'oasis7_provider_tools', '--bin',
                            'oasis7_llm_agent_demo', '--features', 'test_tier_full', '--no-run'])
    add('production-native', ['build', '-p', 'oasis7_chain_runtime', '-p', 'oasis7_launcher_backend',
                             '--bin', 'oasis7_chain_runtime', '--bin', 'oasis7_game_launcher',
                             '--features', 'wasmtime'])
    tier('required-wasmtime', 'test_tier_required,wasmtime', ['--lib', '--bins'])
    for name, args, _ in artifacts.build_variants('ordinary'):
        add('ordinary-' + name, args)
    add('support-wasmtime', ['test', *packages, '--lib', '--bins', '--features', 'wasmtime', '--no-run'])
    add('runtime', ['build', '-p', 'oasis7_chain_runtime', '--bin', 'oasis7_chain_runtime'])
    add('hydrate', ['build', '-p', 'oasis7_distfs', '--bin', 'hydrate_builtin_wasm'])
    add('sync', ['build', '-p', 'oasis7_distfs', '--features', 'sync-builtin-wasm-identity',
                '--bin', 'sync_builtin_wasm_identity'])
    # Wider feature unions cannot warm these exact no-default contexts. Reuse
    # the consumer's grouping, which combines only one package/feature policy.
    for name, args, _ in artifacts.target_build_variants(targets, 'fast-', combine=True):
        add(name, args)
    return variants


def build(root):
    variants = build_variants(root)
    environment = artifacts.build_environment('ordinary')
    for name, command, overrides in variants:
        print('cache_build_variant=' + name + ' command=' + ' '.join(command), flush=True)
        subprocess.run(command, env=environment | overrides, cwd=root, check=True)
    print(f'cache_build_complete variants={len(variants)} verification=false', flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo-root', type=Path, default=ROOT)
    args = parser.parse_args()
    build(args.repo_root.resolve())


if __name__ == '__main__':
    main()
