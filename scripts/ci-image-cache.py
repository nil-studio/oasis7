#!/usr/bin/env python3
"""Warm the actual component CI compile contexts in their matching image.

Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7.
No test process or validation receipt is produced. Trusted writer identity and
final compressed-archive capacity admission remain in the cache workflow.
"""
import argparse
import os
from pathlib import Path
import subprocess


ROOT = Path(__file__).resolve().parent.parent
PROFILES = ('native', 'node', 'net', 'launcher', 'tooling')


def build_variants(root, profile):
    """Keep separate feature contexts; wider unions do not warm the readers."""
    if profile not in PROFILES:
        raise ValueError('unknown image cache compile profile')
    variants = []

    def cargo(name, action, selectors, environment=None):
        command = ['cargo', action, '--locked', *selectors]
        if action == 'test':
            command.append('--no-run')
        variants.append((name, command, environment or {}))

    def native(name, selectors):
        # Check warms the dependency metadata used by the consumer's Clippy;
        # test --no-run warms its separate test-codegen dependency context.
        cargo(name + '-check', 'check', selectors)
        cargo(name + '-test', 'test', selectors)

    if profile == 'native':
        native('consensus', ['-p', 'oasis7_consensus', '--lib'])
        native('storage-libs', ['-p', 'oasis7_cas', '-p', 'oasis7_formal_protocol', '--lib'])
        native('distfs', ['-p', 'oasis7_distfs', '--lib', '--bin', 'oasis7_durability_endpoint'])
        if (root / 'crates/oasis7_distfs/tests/incremental_industry_jobs.rs').is_file():
            native('distfs-industry', ['-p', 'oasis7_distfs', '--test', 'incremental_industry_jobs'])
    elif profile == 'node':
        native('node', ['-p', 'oasis7_node', '--lib'])
    elif profile == 'net':
        native('net', ['-p', 'oasis7_net', '--lib'])
        cargo('if-watch', 'test', ['--manifest-path', './vendor-if-watch-3.2.2/Cargo.toml',
                                  '--target-dir', './target', '--all-features', '--lib'])
        cargo('rendezvous', 'test', ['--manifest-path', './vendor-libp2p-rendezvous-0.18.1/Cargo.toml',
                                    '--target-dir', './target', '--lib'])
        native('net-libp2p', ['-p', 'oasis7_net', '--features', 'libp2p', '--lib'])
        wasm = {'CC_wasm32_unknown_unknown': 'clang', 'AR_wasm32_unknown_unknown': 'llvm-ar'}
        cargo('net-wasm', 'check', ['-p', 'oasis7_net', '--no-default-features',
                                   '--target', 'wasm32-unknown-unknown'], wasm)
        cargo('node-wasm', 'check', ['-p', 'oasis7_node', '--features', 'libp2p',
                                    '--target', 'wasm32-unknown-unknown'], wasm)
    elif profile == 'launcher':
        # index.html selects this bin with default features; Trunk's --release
        # build uses the root workspace target and the same stable WASM target.
        cargo('launcher-wasm-release', 'build', ['-p', 'oasis7_client_launcher',
              '--bin', 'oasis7_client_launcher', '--release', '--target', 'wasm32-unknown-unknown'])
    else:
        # Tooling's real repository work resolves the standalone lockfiles.
        # Its temporary Rust fixtures are local and do not share root targets.
        # Reuse the consumer's discovery, keeping root and standalone resolution
        # locked, without inventing compilation solely to populate this cache.
        variants.append(('tooling-locked-metadata', ['bash', 'scripts/check-standalone-tool-lockfiles.sh'], {
            'OASIS7_LOCAL_CHECK': '1', 'OASIS7_CI_RUN_RUST_BASELINE': 'true',
            'OASIS7_STANDALONE_TOOL_REPO_ROOT': str(root),
        }))
    return variants


def build(root, profile):
    variants = build_variants(root, profile)
    environment = {key: value for key, value in os.environ.items() if key != 'RUSTC_WRAPPER'}
    for name, command, overrides in variants:
        print('image_cache_variant=' + name + ' command=' + ' '.join(command), flush=True)
        subprocess.run(command, cwd=root, env=environment | overrides, check=True)
    print(f'image_cache_complete profile={profile} variants={len(variants)} verification=false', flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--profile', required=True, choices=PROFILES)
    parser.add_argument('--repo-root', type=Path, default=ROOT)
    args = parser.parse_args()
    build(args.repo_root.resolve(), args.profile)


if __name__ == '__main__':
    main()
