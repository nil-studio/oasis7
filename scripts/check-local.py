#!/usr/bin/env python3
"""Explicit local checks using the repository's trusted planner and existing driver.

quick runs the fixed baseline; affected selects required groups; formal requires
a clean source and an explicit required/full tier. Unsupported execution groups
are blocked before launch. --dry-run plans only and never reports validation.
"""
import argparse
import contextlib
from collections import Counter
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tempfile
import time
import tomllib

from ci_process_runner import run as run_process

spec = importlib.util.spec_from_file_location('local_input', Path(__file__).with_name('ci-local-input.py'))
inputs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(inputs)
Blocked = inputs.Blocked

# These are audited driver capabilities, not path-selection rules. New groups
# remain selected in the report and block execution until their write and
# evidence contracts are supported. Never turn an unknown group into a skip.
SAFE_GROUPS = frozenset(('baseline', 'codex_agent_config_validation',
                         'rust_baseline', 'consensus', 'distfs', 'node', 'net',
                         'pixel_world_bridge', 'oasis7_required', 'workspace_support',
                         'doc_checker_contracts', 'cargo_tooling_contracts', 'compile_metrics',
                         'viewer_js_required', 'viewer_performance_report'))
RUST_GROUPS = frozenset(('rust_baseline', 'consensus', 'distfs', 'node', 'net',
                         'pixel_world_bridge', 'oasis7_required', 'workspace_support'))
ENGINEERING_CHECKS = {
    'doc_checker_contracts': ('resolve-product-doc-ci-range.test.py', 'doc-governance-scope.test.py',
        'document-corpus-inventory-check.test.py', 'product-doc-governance-check.test.py',
        'product-doc-content-check.test.py', 'doc-core-content-check.test.py', 'doc-content-warnings.test.py',
        'system-design-traceability-check.test.py', 'product-doc-content-callers.test.sh',
        'doc-governance-check.test.sh', 'find-python-with-module.test.sh'),
    'cargo_tooling_contracts': ('ci-bin-ownership.test.py', 'cargo-dev-windows-toolchain.test.sh',
        'cargo-dev-worktree-isolation.test.sh', 'new-task-worktree.test.sh',
        'check-launcher-p2p-dependency-surface.test.sh', 'cargo-dev-architecture.test.py',
        'runtime-module-routing-perf-harness.test.py', 'cargo-dev-lib.test.sh',
        'check-standalone-tool-lockfiles.test.sh', 'check-standalone-tool-lockfiles.sh'),
    'compile_metrics': ('ci-compile-metrics-contract.test.sh', 'ci-ecs-build-metrics.test.py', 'ci-ecs-cache.test.py'),
}
VIEWER_GROUPS = frozenset(('viewer_js_required', 'viewer_performance_report'))
CARGO_GROUPS = RUST_GROUPS | VIEWER_GROUPS
PLAN_GROUPS = frozenset(('oasis7_required', 'rust_baseline', 'distfs'))
UNSUPPORTED = {
    'formal_process': 'formal process environment and execution evidence need a local adapter',
    'windows_rollout': 'requires the separate Windows runner',
    'macos_package_contract': 'requires the separate macOS runner',
    'fleet_health': 'requires the separate fleet environment',
    'command_center': 'has no group implementation in the local required driver',
}


# Local capabilities only: the trusted planner still owns the selected profiles.
# These canonical native commands write target/ or their own temporary fixtures.
SUPPORT_PROFILES = frozenset((
    'oasis7_agent_harness', 'oasis7_browser_game_client', 'oasis7_client_api',
    'oasis7_model_provider', 'oasis7_client_storage', 'oasis7_signer_protocol',
    'oasis7_custody_core', 'oasis7_launcher_ui', 'oasis7_proto',
    'oasis7_wasm_build', 'oasis7_wasm_sdk', 'oasis7_wasm_store',
    'pixel_world_bridge', 'oasis7_secure_envelope', 'oasis7_context',
    'oasis7_client_launcher',
))
SUPPORT_CLIPPY_PROFILES = frozenset((
    'oasis7_agent_harness', 'oasis7_browser_game_client', 'oasis7_client_storage',
    'oasis7_model_provider', 'oasis7_signer_protocol',
))
UNSUPPORTED_SUPPORT = {
    'oasis7_agent_api': 'signed-read conformance still installs npm dependencies',
    'oasis7_dev_test_worker': 'Worker verification still installs tools and npm dependencies',
    'oasis7_custody_worker': 'Worker provisioning and generated outputs need a local adapter',
    'oasis7_game_client': 'cross-UID fixture requires a separate hosted execution contract',
    'wasm_build_suite': 'real WASM cases need a preflight for their installed target',
    'wasm_module_observe': 'real WASM cases need a preflight for their installed target',
    'oasis7_local_signer': 'installer fixture and ignored-case execution need a local adapter',
    'oasis7_wasm_abi': 'canonical library tests include an ignored performance probe',
    'oasis7_wasm_router': 'canonical library tests include an ignored performance probe',
    'oasis7_wasm_executor': 'canonical wasmtime tests include an ignored performance probe',
}


class Failed(ValueError):
    pass


class NoChecks(ValueError):
    pass


class Parser(argparse.ArgumentParser):
    def error(self, message):
        raise Blocked(message)


def now():
    return datetime.now(timezone.utc).isoformat()


def requested(argv, name, default=None):
    for index, value in enumerate(argv):
        if value.startswith(name + '='):
            return value.split('=', 1)[1]
        if value == name and index + 1 < len(argv):
            return argv[index + 1]
    return default


def report_directory(root, requested_path):
    if requested_path is None:
        return Path(tempfile.mkdtemp(prefix='oasis7-local-check-'))
    parent = Path(requested_path).resolve()
    if parent.is_relative_to(root):
        relative = parent.relative_to(root).as_posix()
        if any(parent.is_relative_to(root / scope) for scope in ('doc', 'scripts', '.agents', 'skills')):
            raise Blocked('report directory cannot be inside a baseline checker input directory')
        if relative == '.' or inputs.git(root, 'ls-files', '-z', '--', relative):
            raise Blocked('report directory cannot contain tracked source')
        try:
            inputs.git(root, 'check-ignore', '-q', '--', relative + '/')
        except Blocked as error:
            raise Blocked('--output-dir inside the worktree must be ignored') from error
    parent.mkdir(parents=True, exist_ok=True)
    return Path(tempfile.mkdtemp(prefix='local-check-', dir=parent))


def execution_environment():
    env = inputs.environment()
    # No inherited artifact/selection/deployment switches or CI event range.
    for key in list(env):
        if key in ('OASIS7_CI_PROCESS_OWNER', 'OASIS7_CI_PROCESS_ANCESTORS'):
            continue
        if key.startswith(('OASIS7_', 'GITHUB_', 'CARGO_TARGET_', 'CARGO_PROFILE_',
                           'AGENT_BROWSER_', 'AGENT_VISUAL_')) or key in (
                'CI', 'SOURCE_SHA', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER',
                'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTUP_TOOLCHAIN',
                'CARGO_BUILD_TARGET_DIR', 'CARGO_BUILD_RUSTFLAGS',
                'PIXEL_WORLD_BEVY_RENDER_PROBE_OUT_DIR', 'PIXEL_WORLD_BEVY_PIXEL_PROBE_OUT_DIR',
                'WASM_BINDGEN_BIN'):
            env.pop(key)
    env.update(PYTHONDONTWRITEBYTECODE='1', RUSTUP_AUTO_INSTALL='0',
               GIT_CONFIG_COUNT='1', GIT_CONFIG_KEY_0='core.quotePath', GIT_CONFIG_VALUE_0='false')
    return env


def invoke(argv, root, env, directory, name, timeout):
    log = directory / (name + '.log')
    journal = directory / (name + '.jsonl')
    try:
        with log.open('xb') as stream, contextlib.redirect_stdout(sys.stderr):
            outcome = run_process(argv, root, env, 'local', name, journal,
                                  timeout_seconds=timeout, log_stream=stream)
    except OSError as error:
        raise Failed(f'cannot start command or persist its output: {error}') from error
    return dict(outcome, argv=argv, cwd=str(root), log=str(log), journal=str(journal))


def plan_from_baseline(root, base, snapshot, args, directory, result):
    """Run baseline-pinned policy and return public group names and planner path.

    The baseline establishes policy integrity; the selection is report evidence.
    """
    policy = directory / 'policy'
    policy.mkdir()
    blobs = {}
    for name in ('plan-rust-required-scope.py', 'ci-required-scope.json', 'ci-formal-process-suites.json'):
        blobs[name] = inputs.git(root, 'show', f'{base}:scripts/{name}')
        (policy / name).write_bytes(blobs[name])
    config = json.loads(blobs['ci-required-scope.json'])
    known = config.get('groups')
    if not isinstance(known, list) or any(not isinstance(group, str) for group in known):
        raise Blocked('trusted policy has no valid group inventory')
    result.update(policy_sha=base, policy=dict(status='used', sha=base,
                  files_sha256={name: inputs.sha256(data) for name, data in blobs.items()}))
    planner = policy / 'plan-rust-required-scope.py'
    env = inputs.environment()
    argv = [sys.executable, str(planner), '--base-ref', base,
            '--head-ref', snapshot['summary']['source_sha'], '--policy-ref', base,
            '--profile', 'candidate-full' if args.tier == 'full' else 'pr-fast',
            '--config', str(policy / 'ci-required-scope.json'),
            '--formal-config', str(policy / 'ci-formal-process-suites.json'),
            '--output', str(directory / 'plan.json')]
    dirty = snapshot['summary']['dirty'] or snapshot['summary']['untracked'] or any(
        item['origin'] == 'referenced' for item in snapshot['summary']['changes'])
    if dirty:
        if b'LOCAL_INPUTS_VERSION = 1' not in blobs['plan-rust-required-scope.py']:
            raise Blocked('trusted baseline planner has no local snapshot capability; rebase after its merge')
        index_tree, worktree_tree, env = inputs.private_trees(root, snapshot, directory)
        local = dict(schema_version=1, source_sha=snapshot['summary']['source_sha'],
                     index_tree=index_tree, worktree_tree=worktree_tree,
                     changes=snapshot['summary']['changes'])
        path = directory / 'local-inputs.json'
        path.write_text(json.dumps(local, ensure_ascii=True, sort_keys=True) + '\n')
        argv += ['--local-inputs', str(path)]
        result['input'].update(index_tree=index_tree, worktree_tree=worktree_tree)
    outcome = invoke(argv, root, env, directory, 'planner', args.planner_timeout_seconds)
    result['planner'] = dict(outcome, sha256=inputs.sha256(blobs['plan-rust-required-scope.py']))
    if outcome['cancelled']:
        raise KeyboardInterrupt
    if not outcome['passed']:
        if outcome['timeout']:
            raise Failed('trusted planner timeout')
        raise Blocked('trusted planner rejected the input; see planner log')
    try:
        plan = json.loads((directory / 'plan.json').read_bytes())
    except (OSError, ValueError) as error:
        raise Failed('trusted planner did not write a valid plan') from error
    groups, reasons = plan.get('groups'), plan.get('reasons')
    if (not isinstance(groups, list) or any(group not in known for group in groups)
            or len(groups) != len(set(groups)) or not isinstance(reasons, list)
            or any(not isinstance(reason, str) for reason in reasons)):
        raise Failed('trusted planner returned invalid groups or reasons')
    result.update(plan=plan, scope=plan.get('scope'),
                  not_selected_groups=[dict(name=group, reason='not selected by trusted planner')
                                       for group in known if group not in groups])
    return groups, planner


def preflight(root, args, result, env):
    reasons = []
    groups = [group['name'] for group in result['groups']]
    for group in result['groups']:
        if group['name'] not in SAFE_GROUPS:
            reason = UNSUPPORTED.get(group['name'], 'local write/evidence contract is not yet audited')
            group.update(status='blocked', reason=reason)
            reasons.append(f'{group["name"]}: {reason}')
    if args.tier == 'full':
        reasons.append('formal full includes source-writing/provisioning commands without a read-only adapter')
    plan = result.get('plan', {})
    if 'workspace_support' in groups:
        support = next(group for group in result['groups'] if group['name'] == 'workspace_support')
        problems = []
        profiles = plan.get('workspace_support_profiles')
        if (type(plan.get('workspace_support_profile_version')) is not int
                or plan['workspace_support_profile_version'] != 1):
            problems.append('trusted support profile version 1 is required')
        if (not isinstance(profiles, list) or not profiles
                or any(not isinstance(profile, str) for profile in profiles)
                or len(set(profiles)) != len(profiles)):
            problems.append('trusted support profiles must be a nonempty unique string list')
        else:
            support['profiles'] = profiles
            for profile in profiles:
                if profile not in SUPPORT_PROFILES:
                    problems.append(profile + ': ' + UNSUPPORTED_SUPPORT.get(
                        profile, 'local support write/evidence contract is not yet audited'))
            if 'oasis7_context' in profiles:
                for tool in ('/usr/bin/git', '/usr/bin/python3'):
                    if not os.access(tool, os.X_OK):
                        problems.append('context test prerequisite unavailable: ' + tool)
        # The existing driver expands full scope to its complete current inventory,
        # and policy changes add targets/features. Neither may narrow to this list.
        if plan.get('scope') != 'targeted' or plan.get('test_policy_changed') is not False:
            problems.append('expanded support targets/features and full scope need a local adapter')
        if problems:
            support.update(status='blocked', reason='; '.join(problems))
            reasons.extend('workspace_support: ' + problem for problem in problems)
    full_feature = any('test_tier_full' in target.get('features', [])
                       for target in plan.get('ordinary_fast_targets', []))
    if 'oasis7_required' in groups and (plan.get('scope') in ('full', 'explicit-full')
                                      or plan.get('test_policy_changed') or full_feature):
        reasons.append('expanded ordinary tests require Docker-built provider/builtin resources; no local provisioning adapter')
    if sys.platform != 'linux' or not Path('/proc').is_dir():
        reasons.append('local execution currently requires Linux /proc for transitive owned-process cleanup')
    driver = root / 'scripts/ci-tests.sh'
    if not driver.is_file() or b'OASIS7_LOCAL_CHECK_COMMAND_RUNNER' not in driver.read_bytes():
        reasons.append('checked-out driver lacks the local command receipt capability')
    required_tools = {'bash', 'git'}
    if set(groups) & ENGINEERING_CHECKS.keys():
        required_tools.update(('awk', 'grep', 'sed', 'mktemp'))
    if 'compile_metrics' in groups:
        required_tools.update(('zstdmt', 'tar', 'ruby'))
        if platform.machine() != 'x86_64':
            reasons.append('compile_metrics requires x86_64 Linux for its real Rust fixture')
        if shutil.which('ruby', path=env.get('PATH')) is not None:
            if subprocess.run(['ruby', '-ryaml', '-rjson', '-e', 'exit 0'], cwd=root, env=env,
                              capture_output=True, timeout=15).returncode:
                reasons.append('compile_metrics requires Ruby with yaml/json; no automatic installation')
    for name in sorted(required_tools):
        if shutil.which(name, path=env.get('PATH')) is None:
            reasons.append(f'missing tool: {name}')
    if set(groups) & {'baseline', 'doc_checker_contracts'}:
        finder = root / 'scripts/find-python-with-module.sh'
        try:
            check = subprocess.run(['bash', str(finder), 'markdown_it'], cwd=root, env=env,
                                   capture_output=True, timeout=15)
            if check.returncode:
                reasons.append('missing tool: Python with markdown_it (no automatic installation)')
            elif 'doc_checker_contracts' in groups:
                document_python = Path(os.fsdecode(check.stdout).strip())
                if not document_python.is_absolute() or not document_python.is_file():
                    reasons.append('document contract Python discovery did not return an executable path')
                elif subprocess.run([str(document_python), '-c', 'import tomllib, markdown_it'],
                                    cwd=root, env=env, capture_output=True, timeout=15).returncode:
                    reasons.append('document contracts require one Python with tomllib and markdown_it')
                else:
                    result['configuration']['document_python'] = str(document_python.resolve())
        except (OSError, subprocess.SubprocessError) as error:
            reasons.append(f'document checker prerequisite unavailable: {error}')
    if set(groups) & (CARGO_GROUPS | {'cargo_tooling_contracts', 'compile_metrics'}):
        for name in ('cargo', 'rustc', *(['rustup'] if set(groups) & CARGO_GROUPS else [])):
            if shutil.which(name, path=env.get('PATH')) is None:
                reasons.append(f'missing tool: {name}')
    if set(groups) & {'cargo_tooling_contracts', 'compile_metrics'}:
        for name in ('cargo', 'rustc'):
            if shutil.which(name, path=env.get('PATH')) is not None:
                check = subprocess.run([name, '--version'], cwd=root, env=env, capture_output=True, timeout=15)
                if check.returncode:
                    reasons.append(f'engineering contracts require an available {name} toolchain; no automatic installation')
    if 'cargo_tooling_contracts' in groups:
        result.setdefault('limitations', []).append(
            'cargo_tooling_contracts uses Linux applicability: the Windows-only fallback and optional '
            'sccache cache-hit proof are not local acceptance evidence.')
    if set(groups) & (CARGO_GROUPS | {'cargo_tooling_contracts', 'compile_metrics'}):
        if 'rust_baseline' in groups:
            if args.offline:
                reasons.append('offline RustSec advisory readiness is not yet supported; no network command started')
            if shutil.which('cargo-deny', path=env.get('PATH')) is None:
                reasons.append('missing pinned cargo-deny; local checks never install it')
            elif subprocess.run(['cargo-deny', '--version'], env=env, capture_output=True,
                                text=True, timeout=15).stdout.strip() != 'cargo-deny 0.19.9':
                reasons.append('cargo-deny 0.19.9 is required; local checks never upgrade it')
        # Target is the existing worktree target, never a fresh namespace to
        # evade a lock. Symlinks/external overrides need an explicit write audit.
        target = root / 'target'
        if target.is_symlink():
            reasons.append('target symlink is not an audited local output directory')
        try:
            inputs.git(root, 'check-ignore', '-q', '--', 'target/')
        except Blocked:
            reasons.append('Cargo target/ must be ignored')
        if (root / 'Cargo.lock').is_symlink():
            reasons.append('Cargo.lock symlink is not supported')
        # Cargo reads ancestor and user configuration, not just environment
        # overrides. Resolve its actual target without changing that target or
        # bypassing a shared build lock, and retain external config identities.
        configurations = {}
        cargo_home = Path(env.get('CARGO_HOME', str(Path.home() / '.cargo')))
        if not cargo_home.is_absolute():
            cargo_home = root / cargo_home
        for directory in (*root.parents, root):
            for name in ('config', 'config.toml'):
                path = directory / '.cargo' / name
                configurations[str(path)] = inputs.sha256(path.read_bytes()) if path.is_file() else None
        for name in ('config', 'config.toml'):
            path = cargo_home / name
            configurations[str(path)] = inputs.sha256(path.read_bytes()) if path.is_file() else None
        result['cargo_configuration'] = configurations
        for name in configurations:
            if configurations[name] is None:
                continue
            config = tomllib.loads(Path(name).read_text())
            build = config.get('build', {})
            if any(key in build for key in ('rustc-wrapper', 'rustc-workspace-wrapper', 'rustc')):
                reasons.append(f'unreviewed Cargo compiler override: {name}')
        if set(groups) & (CARGO_GROUPS | {'cargo_tooling_contracts'}) and shutil.which('cargo', path=env.get('PATH')) is not None:
            argv = ['cargo', 'metadata', '--no-deps', '--format-version', '1', '--locked', '--offline']
            metadata = subprocess.run(argv, cwd=root, env=env, capture_output=True, text=True, timeout=30)
            result['cargo_preflight'] = dict(argv=argv, cwd=str(root), exit_code=metadata.returncode)
            if metadata.returncode:
                reasons.append('locked offline Cargo metadata/toolchain unavailable: ' + metadata.stderr.strip()[-1500:])
            else:
                resolved = Path(json.loads(metadata.stdout)['target_directory']).resolve()
                result['configuration']['cargo_target'] = str(resolved)
                if resolved != (root / 'target').resolve() or (root / 'target').is_symlink():
                    reasons.append('Cargo resolved an external/nonstandard target directory; no build started')
    if set(groups) & VIEWER_GROUPS:
        viewer_preflight(root, result, env, reasons)
    if reasons:
        result['preconditions'] = reasons
        raise Blocked('; '.join(reasons))


def viewer_preflight(root, result, env, reasons):
    """Inspect already prepared tools; this path never installs or builds."""
    tools = {}
    for name in ('node', 'npm', 'agent-browser'):
        path = shutil.which(name, path=env.get('PATH'))
        if path is None:
            reasons.append(f'missing prepared Viewer tool: {name}; no automatic installation')
        else:
            tools[name] = str(Path(path).resolve())
    chrome = next((shutil.which(name, path=env.get('PATH')) for name in
                   ('chromium', 'chromium-browser', 'google-chrome', 'google-chrome-stable')
                   if shutil.which(name, path=env.get('PATH'))), None)
    if chrome is None:
        reasons.append('missing prepared Chromium executable on PATH; no browser installation')
    else:
        tools['chromium'] = str(Path(chrome).resolve())
    result['viewer_preflight'] = []

    def inspect(argv):
        try:
            check = subprocess.run(argv, cwd=root, env=dict(env, OASIS7_LOCAL_CHECK='1'),
                                   capture_output=True, text=True, timeout=30)
        except (OSError, subprocess.SubprocessError) as error:
            reasons.append(f'Viewer prerequisite could not be inspected: {argv[0]}: {error}')
            check = subprocess.CompletedProcess(argv, None, '', str(error))
        result['viewer_preflight'].append(dict(argv=argv, cwd=str(root), exit_code=check.returncode))
        return check

    for name, path in tools.items():
        check = inspect([path, '--version'])
        if check.returncode != 0 or not check.stdout.strip():
            reasons.append(f'prepared Viewer tool is unusable: {name}')
        elif name == 'agent-browser' and check.stdout.strip() != 'agent-browser 0.37.1':
            reasons.append('prepared agent-browser 0.37.1 is required; no automatic upgrade')
    if shutil.which('rustup', path=env.get('PATH')):
        check = inspect(['rustup', 'target', 'list', '--installed'])
        if check.returncode != 0 or 'wasm32-unknown-unknown' not in check.stdout.splitlines():
            reasons.append('missing installed wasm32-unknown-unknown target; no rustup installation')
    if 'node' in tools and 'npm' in tools:
        check = inspect(['bash', str(root / 'scripts/viewer-dependency-preflight.sh'),
                         '--root', str(root), '--purpose', 'all'])
        if check.returncode != 0:
            reasons.append('Viewer npm dependencies are not prepared: ' + check.stderr.strip()[-1500:])
    helper = root / 'scripts/ensure-wasm-bindgen-cli.sh'
    if not helper.is_file() or b'--check' not in helper.read_bytes():
        reasons.append('Viewer wasm-bindgen helper lacks its read-only check capability')
    else:
        check = inspect(['bash', str(helper), '--check', '--print-bin'])
        candidate = Path(check.stdout.strip())
        if (check.returncode != 0 or not candidate.is_absolute() or not candidate.is_file()
                or not os.access(candidate, os.X_OK)):
            reasons.append('locked wasm-bindgen CLI is not prepared; no automatic installation')
        else:
            tools['wasm_bindgen'] = str(candidate)
    result['viewer_tools'] = tools


def viewer_commands(group, bindgen):
    """The complete existing driver calls, independent of its result counts."""
    build = ('./scripts/build-viewer-software-safe.sh',)
    if group == 'viewer_performance_report':
        return [('bash', './scripts/viewer-performance-report-only-contract.test.sh'), build,
                ('env', 'WASM_BINDGEN_BIN=' + bindgen, 'npm', '--prefix', 'crates/oasis7_viewer',
                 'run', 'build:viewer:visual-test'), ('./scripts/viewer-performance-report-only.sh',)]
    return [('./scripts/viewer-dependency-preflight.test.sh',),
            ('bash', './scripts/ensure-wasm-bindgen-cli.test.sh'),
            ('npm', '--prefix', 'crates/oasis7_viewer', 'run', 'test:frontend-structure'),
            ('node', '--test', '--test-reporter=tap', 'crates/oasis7_viewer/scripts/software-safe-feedback-contract.test.mjs'),
            ('node', '--test', '--test-reporter=tap', 'crates/oasis7_viewer/scripts/gameplay-attraction-scenario.test.mjs'),
            ('python', 'scripts/tests/test_industrial_starter_evidence.py'),
            ('./scripts/copy-viewer-web-dist.test.sh',),
            ('./scripts/agent-browser-viewer-dist-freshness-test.sh',),
            ('bash', './scripts/agent-browser-lifecycle-contract.test.sh'),
            ('bash', './scripts/viewer-prompt-control-regression.test.sh'),
            ('bash', './scripts/viewer-prompt-control-race.test.sh'),
            ('bash', './scripts/viewer-prompt-control-race-identity.test.sh'),
            ('node', '--test', '--test-reporter=tap', 'crates/oasis7_viewer/scripts/agent-browser-visual-runner-lifecycle.test.mjs'),
            ('./scripts/bundle-freshness-lib.test.sh',),
            ('npm', '--prefix', 'crates/oasis7_viewer', 'run', 'test:ui', '--', '--reporter=tap-flat'), build,
            ('npm', '--prefix', 'crates/oasis7_viewer', 'run', 'build:viewer:bundle', '--', '--mode', 'visual-test'),
            ('node', 'crates/oasis7_viewer/scripts/browser-functional-regression.mjs', '--skip-build'),
            ('node', 'crates/oasis7_viewer/scripts/viewer-auth-browser-security-smoke.mjs')]


def viewer_complete(commands, group, bindgen):
    actual = []
    for item in commands:
        argv = list(item['argv'])
        if argv and Path(argv[0]).name.startswith('python'):
            argv[0] = 'python'
        actual.append(tuple(argv))
    return Counter(actual) == Counter(viewer_commands(group, bindgen))


def baseline_complete(commands, base, head):
    # Fixed baseline dispatch is part of the quick contract. A partial driver
    # cannot report success merely because its remaining commands returned 0.
    actual = [item['argv'] for item in commands]
    scripts = ('unified-world-code-terminology-scan.test.sh', 'doc-governance-check.sh',
               'lint-skills.sh', 'check-windows-paths.sh', 'check-script-executable-bits.sh',
               'validate-codex-agent-config.py')
    for script in scripts:
        if not any(any(arg.endswith('/' + script) for arg in command) for command in actual):
            return False
    return (any(command == ['git', 'diff', '--check', base, head] for command in actual)
            and any(command[:3] == ['git', 'diff', '--check'] and '--cached' in command for command in actual)
            and any(command == ['git', 'diff', '--check', '--no-ext-diff', '--no-textconv'] for command in actual)
            and any(len(command) == 2 and command[1] == '-' for command in actual))


def support_complete(commands, profiles):
    expected = [('test', profile) for profile in profiles]
    expected.extend(('clippy', profile) for profile in profiles if profile in SUPPORT_CLIPPY_PROFILES)
    actual = []
    for item in commands:
        command = item['argv']
        # Match the canonical direct Cargo invocation, not a wrapper mentioning
        # Cargo or an additional package/workspace selector hidden in its args.
        if command[:4] != ['env', '-u', 'RUSTC_WRAPPER', 'cargo']:
            return False
        args = command[4:]
        if args[:1] == ['--verbose']:
            args = args[1:]
        if '--' in args:
            args = args[:args.index('--')]
        if not args or args[0] not in ('test', 'clippy'):
            return False
        selectors = [arg for arg in args[1:] if arg in ('--workspace', '--all')
                     or arg.startswith(('--package', '--exclude', '-p'))]
        if selectors != ['-p'] or args.index('-p') + 1 == len(args):
            return False
        actual.append((args[0], args[args.index('-p') + 1]))
    return sorted(actual) == sorted(expected)


def execute(root, args, result, directory, env):
    driver = root / 'scripts/ci-tests.sh'
    command_runner = Path(__file__).with_name('ci-local-check-command.py').resolve()
    result['driver'] = dict(status='not_started', path=str(driver), sha256=inputs.sha256(driver.read_bytes()),
                            runner_sha256=inputs.sha256(command_runner.read_bytes()))
    preflight(root, args, result, env)
    result['driver']['status'] = 'running'
    result['groups_expected'] = len(result['groups'])
    check_counts = dict(baseline=10, codex_agent_config_validation=1,
                        **{name: len(checks) for name, checks in ENGINEERING_CHECKS.items()})
    result['checks_expected'] = (sum(check_counts[group['name']] for group in result['groups'])
                                 if all(group['name'] in check_counts for group in result['groups']) else None)
    for index, group in enumerate(result['groups']):
        group_dir = directory / f'group-{index:02d}-{group["name"]}'
        group_dir.mkdir()
        argv = ['bash', str(driver), 'required', '--group', group['name']]
        if group['name'] == 'workspace_support':
            argv += ['--support-plan-file', str(directory / 'plan.json')]
        if group['name'] in PLAN_GROUPS:
            argv += ['--plan', str(directory / 'plan.json')]
        group_env = dict(env, OASIS7_LOCAL_CHECK='1', OASIS7_LOCAL_CHECK_COMMAND_RUNNER=str(command_runner),
                         OASIS7_LOCAL_CHECK_RUN_DIR=str(group_dir), OASIS7_LOCAL_CHECK_GROUP=group['name'],
                         OASIS7_PRODUCT_DOC_BASE=result['base_sha'], OASIS7_PRODUCT_DOC_HEAD=result['source_sha'])
        if group['name'] == 'doc_checker_contracts':
            # Fixture CLIs also invoke literal python/python3. Bind the same
            # discovered interpreter throughout the group, without installing
            # dependencies or changing the caller's PATH outside this process.
            binaries = group_dir / 'python-bin'
            binaries.mkdir()
            for name in ('python', 'python3'):
                (binaries / name).symlink_to(result['configuration']['document_python'])
            group_env['PATH'] = str(binaries) + os.pathsep + group_env.get('PATH', '')
        with contextlib.ExitStack() as owned:
            if group['name'] in VIEWER_GROUPS:
                viewer_output = group_dir / 'viewer'
                viewer_output.mkdir(mode=0o700)
                if viewer_output.resolve() != viewer_output or viewer_output.is_symlink():
                    raise Blocked('Viewer output directory is not canonical')
                sockets = owned.enter_context(tempfile.TemporaryDirectory(prefix='o7v-', dir='/tmp'))
                group_env.update(OASIS7_VIEWER_LOCAL_OUTPUT_ROOT=str(viewer_output),
                                 AGENT_BROWSER_BIN=result['viewer_tools']['agent-browser'],
                                 AGENT_BROWSER_EXECUTABLE_PATH=result['viewer_tools']['chromium'],
                                 AGENT_BROWSER_SOCKET_DIR=sockets,
                                 WASM_BINDGEN_BIN=result['viewer_tools']['wasm_bindgen'])
                group['viewer_output'] = str(viewer_output)
            outcome = invoke(argv, root, group_env, group_dir, 'driver', args.timeout_seconds)
        group.update(outcome)
        commands = [json.loads(path.read_bytes()) for path in sorted((group_dir / 'commands').glob('*.json'))]
        commands.sort(key=lambda item: item['started_at'])
        group['commands'] = commands
        result['checks_executed'] += sum(item['status'] != 'running' for item in commands)
        known = all(isinstance(item.get('tests_executed'), int) for item in commands)
        group['tests_executed'] = sum(item['tests_executed'] for item in commands) if known else None
        group['tests_expected'] = sum(item['tests_expected'] for item in commands) if known else None
        group['business_tests_executed'] = sum(item.get('business_tests_executed', 0) or 0 for item in commands) if known else None
        if outcome['cancelled']:
            group.update(status='cancelled', reason='interrupted; owned processes cleaned up')
            raise KeyboardInterrupt
        if not outcome['passed']:
            if any(item['status'] == 'blocked' for item in commands):
                group.update(status='blocked', reason='command prerequisite unavailable')
                raise Blocked(f'{group["name"]}: command prerequisite unavailable; see command log')
            incomplete = any(item['status'] == 'incomplete' for item in commands)
            group.update(status='incomplete' if incomplete else 'failed',
                         reason='timeout' if outcome['timeout'] else 'driver or command failed')
            raise Failed(f'{group["name"]}: {group["reason"]}; see command logs')
        if not commands:
            group.update(status='incomplete', reason='driver executed no recorded checks')
            raise Failed(f'{group["name"]}: no command completion evidence')
        if any(item['status'] != 'passed' for item in commands) or not known:
            group.update(status='incomplete', reason='required command evidence failed, skipped or unknown')
            raise Failed(f'{group["name"]}: incomplete command evidence')
        if group['name'] == 'baseline' and not baseline_complete(commands, result['base_sha'], result['source_sha']):
            group.update(status='incomplete', reason='baseline command inventory is incomplete')
            raise Failed('baseline did not execute its complete input/check inventory')
        if group['name'] == 'workspace_support' and not support_complete(
                commands, result['plan']['workspace_support_profiles']):
            group.update(status='incomplete', reason='support command inventory is incomplete or unexpected')
            raise Failed('workspace_support: support command inventory is incomplete or unexpected')
        expected_commands = ENGINEERING_CHECKS.get(group['name'])
        if expected_commands is not None and (len(commands) != len(expected_commands) or any(
                sum(any(arg.endswith('/' + script) for arg in item['argv']) for item in commands) != 1
                for script in expected_commands)):
            group.update(status='incomplete', reason='engineering contract command inventory is incomplete')
            raise Failed(f'{group["name"]}: incomplete command inventory')
        if group['name'] in VIEWER_GROUPS and not viewer_complete(commands, group['name'], result['viewer_tools']['wasm_bindgen']):
            group.update(status='incomplete', reason='Viewer command inventory is incomplete or duplicated')
            raise Failed('Viewer did not execute its complete source/build/browser inventory')
        if group['name'] in (RUST_GROUPS - {'rust_baseline'}) | ENGINEERING_CHECKS.keys() and not group['tests_executed']:
            group.update(status='incomplete', reason='selected test group executed zero cases')
            raise Failed(f'{group["name"]}: zero selected cases executed')
        group.update(status='passed', reason='all selected commands completed with execution evidence')
    result['driver']['status'] = 'completed'
    result.update(status='passed', exit_code=0, validated=True,
                  reason='all checks in the selected local profile passed')


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    output_format = requested(argv, '--format', 'text')
    result = dict(schema_version=1, profile=requested(argv, '--profile'), tier=requested(argv, '--tier'),
                  status='blocked', exit_code=2, validated=False, reason='', started_at=now(),
                  source_sha=None, base_sha=None, policy_sha=None,
                  planner=dict(status='not_used'), policy=dict(status='not_used'),
                  driver=dict(status='not_started'), input=dict(status='not_inspected'),
                  groups=[], not_selected_groups=[], checks_expected=0, checks_executed=0,
                  tests_expected=0, tests_executed=0, business_tests_executed=0, logs={},
                  allowed_writes=['ignored worktree build outputs', 'owned temporary files and report directory',
                                  'locked Cargo dependency/advisory caches when network is allowed'],
                  limitations=['Local results do not replace hosted CI, other platforms or real-world acceptance.',
                               'Unsupported selected groups block the whole execution; no silent narrowing.'])
    start = time.monotonic()
    before = root = directory = base = None
    extra = set()
    try:
        parser = Parser(description=__doc__)
        parser.add_argument('--profile', choices=('quick', 'affected', 'formal'), required=True)
        parser.add_argument('--tier', choices=('required', 'full'))
        parser.add_argument('--base-ref', help='trusted ancestor of local origin/main; no automatic fetch')
        parser.add_argument('--output-dir', help='parent for a unique report; external or ignored directory')
        parser.add_argument('--format', choices=('text', 'json'), default='text')
        parser.add_argument('--dry-run', action='store_true', help='plan only; validated=false')
        parser.add_argument('--offline', action='store_true', help='forbid Cargo dependency network access')
        parser.add_argument('--timeout-seconds', type=float, default=2400, help='deadline per driver group')
        parser.add_argument('--planner-timeout-seconds', type=float, default=120)
        args = parser.parse_args(argv)
        if (args.profile == 'formal') != (args.tier is not None):
            raise Blocked('formal requires explicit --tier required|full; other profiles reject --tier')
        if any(not math.isfinite(value) or value <= 0 for value in (args.timeout_seconds, args.planner_timeout_seconds)):
            raise Blocked('deadlines must be positive and finite')
        root = Path(os.fsdecode(inputs.git(Path.cwd(), 'rev-parse', '--show-toplevel').rstrip(b'\n'))).resolve()
        directory = report_directory(root, args.output_dir)
        result['logs'] = dict(directory=str(directory), report=str(directory / 'result.json'))
        head, anchor, base = inputs.baseline(root, args.base_ref)
        result.update(source_sha=head, trust_anchor_sha=anchor, base_sha=base,
                      base_source=args.base_ref or 'merge-base(HEAD,local origin/main)')
        before = inputs.capture(root, base)
        result['input'] = dict(before['summary'])
        if before['summary']['source_sha'] != head or inputs.commit(root, 'refs/remotes/origin/main') != anchor:
            raise Blocked('HEAD or trust anchor changed between baseline resolution and input capture')
        if args.profile == 'formal' and (result['input']['dirty'] or result['input']['untracked']):
            raise Blocked('formal requires clean tracked and nonignored untracked source')
        if args.profile == 'quick':
            groups = ['baseline']
            result.update(scope='fixed-baseline', planner=dict(status='not_used', reason='quick does not select Rust impact'),
                          policy=dict(status='not_used', reason='fixed baseline is local feedback only'),
                          selection_reasons=['quick always runs the existing required baseline'],
                          not_selected_groups=[dict(name=name, reason='outside quick profile') for name in
                                               ('Rust fmt/compile', 'business tests', 'browser', 'services', 'other platforms')])
        else:
            first_plan = directory / 'planning-0'
            first_plan.mkdir()
            groups, planner_path = plan_from_baseline(root, base, before, args, first_plan, result)
            if set(groups) & (CARGO_GROUPS | {'cargo_tooling_contracts'}):
                for attempt in range(1, 33):
                    found = inputs.referenced_inputs(root, before, planner_path)
                    if not found - extra:
                        break
                    extra.update(found)
                    extended = inputs.capture(root, base, extra)
                    if (any(extended['files'].get(path) != value for path, value in before['files'].items())
                            or any(extended['summary'][key] != before['summary'][key] for key in
                                   ('source_sha', 'index_sha256', 'refs_sha256', 'entries_sha256'))):
                        raise Blocked('input changed while resolving referenced files')
                    same_bytes = extended['files'] == before['files']
                    before = extended
                    result['input'].update(before['summary'])
                    if same_bytes:
                        # Missing optional fixtures add absence identities, not
                        # new bytes or dependencies. The same plan still applies.
                        break
                    next_plan = directory / f'planning-{attempt}'
                    next_plan.mkdir()
                    groups, planner_path = plan_from_baseline(root, base, before, args, next_plan, result)
                else:
                    raise Blocked('referenced input closure did not converge')
            (directory / 'plan.json').write_text(json.dumps(result['plan'], ensure_ascii=True, sort_keys=True) + '\n')
            result['selection_reasons'] = result['plan']['reasons']
        result['groups'] = [dict(name=group, status='not_run', reason='dry_run' if args.dry_run else 'pending',
                                 tests_expected=None if group in CARGO_GROUPS or group in ENGINEERING_CHECKS else 0,
                                 tests_executed=0, business_tests_executed=0)
                            for group in groups]
        result['groups_expected'] = len(groups)
        result['checks_expected'] = 10 if groups == ['baseline'] else None
        if not groups:
            raise NoChecks('trusted planner selected no checks')
        if args.dry_run:
            result.update(status='planned', exit_code=0, reason='explicit dry-run; no checks executed')
        else:
            # Current line-oriented document consumers cannot safely represent
            # these names. The planner still supports them; execution is honest.
            if any(any(char in path for char in '\n\r\t"\\') or any(0xDC80 <= ord(char) <= 0xDCFF for char in path)
                   for path in before['files']):
                raise Blocked('baseline document consumers do not support control/quoted/non-UTF8 paths')
            env = execution_environment()
            if args.offline:
                env['CARGO_NET_OFFLINE'] = 'true'
            result['configuration'] = dict(offline=args.offline, rustc_wrapper=None,
                                           cargo_target='worktree target/', automatic_tool_installation=False)
            if inputs.capture(root, base, extra)['summary'] != before['summary']:
                raise Blocked('input changed before execution')
            execute(root, args, result, directory, env)
    except NoChecks as error:
        result.update(status='no_checks', exit_code=3, reason=str(error))
    except Blocked as error:
        result.update(status='blocked', exit_code=2, reason=str(error))
    except KeyboardInterrupt:
        result.update(status='cancelled', exit_code=130, reason='interrupted; partial logs retained')
    except (Failed, OSError, ValueError, subprocess.SubprocessError) as error:
        result.update(status='incomplete' if any(group['status'] == 'incomplete' for group in result['groups'])
                      else 'failed', exit_code=1, reason=str(error))
    finally:
        if before is not None:
            try:
                after = inputs.capture(root, base, extra)
                if after['summary'] != before['summary']:
                    result['stale_reason'] = 'HEAD, refs, index or actual checker inputs changed during this run'
            except (Blocked, OSError, ValueError) as error:
                result['stale_reason'] = f'cannot verify final input identity: {error}'
            if result.get('stale_reason') and result['status'] != 'cancelled':
                result.update(status='stale_input', exit_code=2, reason=result['stale_reason'])
        for path, digest in result.get('cargo_configuration', {}).items():
            try:
                unchanged = ((inputs.sha256(Path(path).read_bytes()) if Path(path).is_file() else None) == digest)
            except OSError:
                unchanged = False
            if not unchanged and result['status'] != 'cancelled':
                result.update(status='stale_input', exit_code=2, reason=f'Cargo configuration changed: {path}')
    if result['status'] != 'passed':
        result['validated'] = False
        for group in result['groups']:
            if group['status'] == 'not_run':
                group['reason'] = 'not executed because ' + result['status']
    for field in ('tests_expected', 'tests_executed', 'business_tests_executed'):
        values = [group.get(field, 0) for group in result['groups']]
        result[field] = sum(values) if all(isinstance(value, int) for value in values) else None
    result.update(finished_at=now(), duration_seconds=time.monotonic() - start)
    if directory is not None:
        temporary = directory / 'result.tmp'
        try:
            temporary.write_text(json.dumps(result, ensure_ascii=True, sort_keys=True, indent=2) + '\n')
            os.replace(temporary, directory / 'result.json')
        except OSError as error:
            result.update(status='failed', exit_code=1, validated=False, reason=f'cannot persist report: {error}')
            result['logs']['report'] = None
            try:
                temporary.unlink(missing_ok=True)
            except OSError:
                pass
    if output_format == 'json':
        print(json.dumps(result, ensure_ascii=True, sort_keys=True))
    else:
        print(f"{result['status']}: {result['reason']}")
        print(f"Checks executed: {result['checks_executed']}; test cases: {result['tests_executed']}; "
              f"business test cases: {result['business_tests_executed']}; validated={str(result['validated']).lower()}")
        print('Full result:')
        print(json.dumps(result, ensure_ascii=True, sort_keys=True, indent=2))
    return result['exit_code']


if __name__ == '__main__':
    raise SystemExit(main())
