#!/usr/bin/env python3
"""Self-contained workflow guard and timeout contract tests."""
from pathlib import Path
import re
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent


def workflow_jobs(workflow):
    jobs = dict(re.findall(r'^  ([a-z][a-z0-9_-]*):\n(.*?)(?=^  [a-z][a-z0-9_-]*:|\Z)',
                          workflow.split('\njobs:\n', 1)[1], re.M | re.S))
    anchors = {}
    for job in jobs.values():
        match = re.search(r'^    steps: &([a-z0-9-]+)\n(.*)', job, re.M | re.S)
        if match:
            assert match[1] not in anchors, 'duplicate step sequence anchor'
            anchors[match[1]] = match[2]
    jobs = {name: re.sub(r'^    steps: &[a-z0-9-]+\n', '    steps:\n',
                        re.sub(r'^    steps: \*([a-z0-9-]+)\n',
                               lambda match: '    steps:\n' + anchors[match[1]], job, flags=re.M), flags=re.M)
            for name, job in jobs.items()}
    # Resolve individual step aliases as well as complete step sequences. Keep
    # the old observation alias opaque: contracts inspect that step separately.
    definitions = re.findall(
        r'^      - &([a-z0-9-]+)\n(.*?)(?=^      - |^\S|^  [a-z]|\Z)',
        workflow, re.M | re.S)
    step_anchors = dict(definitions)
    assert len(step_anchors) == len(definitions), 'duplicate step anchor'
    assert not set(step_anchors).intersection(anchors), 'ambiguous step anchor'
    known_steps = set(step_anchors)
    step_anchors.pop('engineering-job-observation', None)
    for name, body in step_anchors.items():
        assert body.startswith('        '), f'invalid step anchor {name}'
        step_anchors[name] = '      - ' + body.removeprefix('        ')
    for name, job in jobs.items():
        for alias in re.findall(r'^      - \*([a-z0-9-]+)\n', job, re.M):
            assert alias in known_steps, f'undefined step alias {alias}'
        job = re.sub(r'^      - &([a-z0-9-]+)\n(.*?)(?=^      - |\Z)',
                     lambda match: step_anchors.get(match[1], match[0]), job, flags=re.M | re.S)
        jobs[name] = re.sub(r'^      - \*([a-z0-9-]+)\n',
                           lambda match: step_anchors.get(match[1], match[0]), job, flags=re.M)
    return jobs


JOBS = workflow_jobs((ROOT / '.github/workflows/rust.yml').read_text())


def effective_job_condition(condition, values, ancestor_results, *, cancelled=False):
    """Include Actions' implicit success() instead of evaluating only written YAML."""
    expression = condition.strip().removeprefix('>-').strip()
    if not re.search(r'\b(?:always|success|failure|cancelled)\s*\(', expression):
        expression = 'success() && (' + expression + ')'
    values = {
        'needs.fast-preflight.result': 'success',
        **values,
        'always()': True,
        'success()': not cancelled and all(result == 'success' for result in ancestor_results),
        'failure()': 'failure' in ancestor_results,
        'cancelled()': cancelled,
    }
    for token, value in sorted(values.items(), key=lambda item: -len(item[0])):
        expression = expression.replace(token, repr(value))
    expression = re.sub(r'!(?!=)', 'not ', expression.replace('&&', ' and ').replace('||', ' or '))
    return bool(eval(' '.join(expression.split()), {'__builtins__': {}},
                     {'contains': lambda collection, item: item in collection}))


class StablePrJobTimeoutContractTests(unittest.TestCase):
    def test_stable_pr_jobs_declare_bounded_timeout_minutes(self):
        expected_limits = {
            '.github/workflows/rust.yml': {
                'periodic-admission': 3,
                'main-fast-admission': 3,
                'fast-preflight': 5,
                'cache-bootstrap-maintenance': 5,
                'cache-maintenance': 5,
                'formal-process': 5,
                'workspace-support': 5,
                'required-gate': 5,
                'explicit-full-gate': 5,
                'select': 5,
                'pr-governance-preview': 5,
                'pr-fast-preview-plan': 5,
                'pr-fast-preview': 20,
                'baseline': 10,
                'codex-agent-config-validation': 5,
                'macos-package-contract': 5,
                'site-quality': 5,
                'packaging-contracts': 5,
                'scenario-regression': 11,
                'compile-metrics': 5,
                'cargo-tooling-contracts': 5,
                'windows-rollout': 7,
                'fleet-health': 13,
                'doc-checker-contracts': 10,
                'workflow-governance': 16,
                'custody-native-contracts': 12,
                'consensus': 5,
                'net': 10,
                'node': 10,
                'pixel-world-bridge': 5,
                'custody-node': 19,
                'custody-native': 17,
                'custody-wasm-production': 25,
                'distfs': 16,
                'rust-baseline': 32,
                'launcher-web': 10,
                'viewer-js-required': 14,
                'viewer-performance-report': 15,
            },
            '.github/workflows/pr-boundary-gate.yml': {
                'boundary-check': 3,
            },
            '.github/workflows/codeql.yml': {
                'plan': 5,
            },
            '.github/workflows/wasm-determinism-gate.yml': {
                'plan-wasm-determinism-scope': 5,
            },
            '.github/workflows/pr-fast-preview.yml': {
                'pr-fast-preview-plan': 5,
                'pr-fast-preview': 20,
            },
            '.github/workflows/command-center.yml': {
                'desktop': 16,
            },
            '.github/workflows/daily-github-release.yml': {
                'create-release': 5,
            },
            '.github/workflows/pages.yml': {
                'deploy': 5,
            },
            '.github/workflows/ecs-packages.yml': {
                'web': 18,
                'native': 135,
                'native-trusted': 135,
                'cache-maintenance': 5,
                'build-metrics': 150,
                'package': 20,
                'publish-preview': 10,
            },
            '.github/workflows/hosted-login-linux-build.yml': {
                'build-linux-x86-64': 30,
                'build-viewer': 16,
                'assemble': 5,
                'warm-linux-release-deps': 75,
            },
            '.github/workflows/ci-base-images.yml': {
                'build': 40,
                'publish': 10,
            },
            '.github/workflows/native-cache-diagnostic.yml': {
                'diagnose': 25,
            },
            '.github/workflows/storage-acceptance.yml': {
                'diagnostics-probe-admission': 2,
                'diagnostics-probe': 5,
                'build-components': 125,
                'build': 10,
                'verify': 130,
                'attempt-release-reference': 50,
                'capacity-long': 360,
                'full-node-long': 360,
                'summary': 5,
            },
            '.github/workflows/wasm-darwin-docker-evidence.yml': {
                'collect-darwin-docker-summaries': 90,
                'verify-with-linux-summaries': 30,
            },
            '.github/workflows/rust-cache.yml': {
                'browser-archive-cache': 6,
                'worker-build-cache': 12,
                'shared-native-cache': 120,
                'rust-image-native-cache': 20,
                'rust-image-component-cache': 35,
                'web-wasm-cache': 22,
            },
        }
        retiring = {
            '.github/workflows/pr-fast-preview.yml',
            '.github/workflows/command-center.yml',
            '.github/workflows/native-cache-diagnostic.yml',
            '.github/workflows/wasm-darwin-docker-evidence.yml',
        }
        retiring_rust_jobs = {
            'command-center', 'consensus', 'viewer-performance-report', 'pixel-world-bridge',
            'codex-agent-config-validation', 'compile-metrics', 'site-quality',
            'doc-checker-contracts', 'cargo-tooling-contracts', 'windows-rollout',
            'macos-package-contract',
        }
        for rel_path, limits in expected_limits.items():
            if rel_path in retiring and not (ROOT / rel_path).is_file():
                continue
            jobs = workflow_jobs((ROOT / rel_path).read_text())
            for job_name, max_minutes in limits.items():
                if rel_path == '.github/workflows/rust.yml' and job_name in retiring_rust_jobs and job_name not in jobs:
                    continue
                with self.subTest(workflow=rel_path, job=job_name):
                    self.assertIn(job_name, jobs)
                    match = re.search(r'^    timeout-minutes:\s*(\d+)\s*$', jobs[job_name], re.M)
                    self.assertIsNotNone(match, f'{rel_path}:{job_name} must declare explicit timeout-minutes')
                    actual = int(match.group(1))
                    self.assertGreater(actual, 0)
                    self.assertLessEqual(actual, max_minutes)


class ReleasePackagingJobTimeoutContractTests(unittest.TestCase):
    def test_release_packaging_workflows_declare_bounded_timeout_minutes_for_all_jobs(self):
        expected_limits = {
            '.github/workflows/testnet-packages.yml': {
                'plan': 5,
                'build-web-dist': 25,
                'package-native': 180,
                'package-native-trusted': 180,
                'package-installers': 90,
            },
            '.github/workflows/mainnet-packages.yml': {
                'plan': 5,
                'release_gate_runtime_core': 40,
                'release_gate_runtime_support': 25,
                'release_gate_runtime_sync': 35,
                'release_gate_web': 25,
                'release_gate_soak': 25,
                'release_gate': 5,
                'build-web-dist': 20,
                'package-native': 180,
                'package-index': 5,
            },
            '.github/workflows/release-packages.yml': {
                'cache-bootstrap-maintenance': 5,
                'cache-maintenance': 5,
                'plan': 5,
                'release_gate_runtime_core': 40,
                'release_gate_runtime_support': 25,
                'release_gate_runtime_sync': 35,
                'release_gate_web': 25,
                'release_gate_soak': 25,
                'release_gate': 5,
                'build-web-dist': 20,
                'package-native': 180,
                'publish-release': 10,
            },
        }
        for rel_path, limits in expected_limits.items():
            jobs = workflow_jobs((ROOT / rel_path).read_text())
            self.assertEqual(set(jobs.keys()), set(limits.keys()), rel_path)
            for job_name, max_minutes in limits.items():
                with self.subTest(workflow=rel_path, job=job_name):
                    match = re.search(r'^    timeout-minutes:\s*(\d+)\s*$', jobs[job_name], re.M)
                    self.assertIsNotNone(match, f'{rel_path}:{job_name} must declare explicit timeout-minutes')
                    actual = int(match.group(1))
                    self.assertGreater(actual, 0)
                    self.assertLessEqual(actual, max_minutes)


class DependabotCoverageGuardTests(unittest.TestCase):
    EXCLUDED_NPM_DIRECTORIES = {
        '/.github/actions/cache-sdk': 'vendored fixed actions/cache 55cc SDK closure; update patch/provenance together',
    }
    EXCLUDED_CARGO_DIRECTORIES = {
        **({
            '/crates/oasis7': 'retired umbrella crate removed from workspace members (#6152); pending physical deletion',
        } if (ROOT / 'crates/oasis7/Cargo.toml').is_file() else {}),
        '/tools/wasm_build_suite/templates/minimal_module': 'zero-dependency minimal template',
        '/vendor/if-watch-3.2.2': 'vendored crate patched by root Cargo.toml',
        '/vendor/libp2p-quic-0.15.0': 'vendored crate patched by root Cargo.toml',
        '/vendor/libp2p-swarm-0.49.0': 'vendored crate patched by root Cargo.toml',
    }
    EXCLUDED_DOCKER_DIRECTORIES = {
        '/docker/ci-base': 'UBUNTU_BASE ARG is workflow-supplied with no Dockerfile default',
    }

    @staticmethod
    def _parse_dependabot_directories(text):
        updates = {}
        current_ecosystem = None
        in_directories = False
        for raw_line in text.splitlines():
            stripped = raw_line.split('#', 1)[0].rstrip()
            if not stripped.strip():
                continue
            eco_match = re.match(r'^  -\s+package-ecosystem:\s*["\']?([^"\'\s]+)["\']?\s*$', stripped)
            if eco_match:
                current_ecosystem = eco_match.group(1)
                updates.setdefault(current_ecosystem, [])
                in_directories = False
                continue
            if current_ecosystem is None:
                continue
            dir_match = re.match(r'^    directory:\s*["\']?([^"\'\s]+)["\']?\s*$', stripped)
            if dir_match:
                updates[current_ecosystem].append(dir_match.group(1))
                in_directories = False
                continue
            if re.match(r'^    directories:\s*$', stripped):
                in_directories = True
                continue
            if in_directories:
                item_match = re.match(r'^      -\s*["\']?([^"\'\s]+)["\']?\s*$', stripped)
                if item_match:
                    updates[current_ecosystem].append(item_match.group(1))
                    continue
                if re.match(r'^    \S', stripped):
                    in_directories = False
        return updates

    @staticmethod
    def _tracked_manifest_directories(filename_pattern):
        output = subprocess.check_output(['git', '-C', str(ROOT), 'ls-files', '-z'])
        dirs = set()
        for raw_path in output.decode('utf-8').split('\0'):
            if not raw_path:
                continue
            name = raw_path.rsplit('/', 1)[-1]
            if not re.fullmatch(filename_pattern, name):
                continue
            parent = raw_path.rsplit('/', 1)[0] if '/' in raw_path else ''
            dirs.add('/' if not parent else '/' + parent)
        return dirs

    @staticmethod
    def _matches_any(directory, patterns):
        import fnmatch
        return any(fnmatch.fnmatchcase(directory, pattern) for pattern in patterns)

    @staticmethod
    def _cargo_workspace_member_directories(workspace_dir):
        import fnmatch
        import tomllib
        manifest = ROOT / (workspace_dir.lstrip('/') + '/Cargo.toml' if workspace_dir != '/' else 'Cargo.toml')
        data = tomllib.loads(manifest.read_text(encoding='utf-8'))
        workspace = data.get('workspace')
        if not isinstance(workspace, dict):
            return set()
        members = workspace.get('members') or []
        base = '' if workspace_dir == '/' else workspace_dir.lstrip('/')
        member_dirs = set()
        for member in members:
            rel = f'{base}/{member}' if base else member
            if any(ch in rel for ch in '*?['):
                for candidate in DependabotCoverageGuardTests._tracked_manifest_directories(r'Cargo\.toml'):
                    if fnmatch.fnmatchcase(candidate.lstrip('/'), rel):
                        member_dirs.add(candidate)
            else:
                member_dirs.add('/' + rel.strip('/'))
        return member_dirs

    def _assert_ecosystem_coverage(self, ecosystem, manifest_dirs, configured_patterns, excluded):
        import fnmatch
        self.assertGreater(len(configured_patterns), 0, f'{ecosystem} must declare at least one directory')

        for pattern in configured_patterns:
            matched = [d for d in manifest_dirs if fnmatch.fnmatchcase(d, pattern)]
            self.assertGreater(
                len(matched),
                0,
                f'.github/dependabot.yml {ecosystem} entry {pattern!r} does not match any tracked manifest',
            )

        excluded_set = set(excluded)
        stale_exclusions = sorted(excluded_set - manifest_dirs)
        self.assertEqual(
            stale_exclusions,
            [],
            f'Stale {ecosystem} exclusion allowlist entries: {stale_exclusions}',
        )

        overlapping = sorted(d for d in excluded_set if self._matches_any(d, configured_patterns))
        self.assertEqual(
            overlapping,
            [],
            f'{ecosystem} directories are both covered by dependabot.yml and excluded: {overlapping}',
        )

        uncovered = sorted(
            d for d in manifest_dirs
            if d not in excluded_set and not self._matches_any(d, configured_patterns)
        )
        self.assertEqual(
            uncovered,
            [],
            f'Uncovered {ecosystem} manifest directories (add to .github/dependabot.yml or explicit exclusion allowlist): {uncovered}',
        )

    def test_dependabot_covers_all_non_excluded_manifests(self):
        dependabot_path = ROOT / '.github/dependabot.yml'
        updates = self._parse_dependabot_directories(dependabot_path.read_text(encoding='utf-8'))

        self.assertIn('/', updates.get('github-actions', []), 'github-actions must cover /')

        npm_dirs = self._tracked_manifest_directories(r'package\.json')
        self._assert_ecosystem_coverage(
            'npm',
            npm_dirs,
            updates.get('npm', []),
            self.EXCLUDED_NPM_DIRECTORIES,
        )

        all_cargo_dirs = self._tracked_manifest_directories(r'Cargo\.toml')
        cargo_patterns = updates.get('cargo', [])
        covered_cargo_roots = {d for d in all_cargo_dirs if self._matches_any(d, cargo_patterns)}
        workspace_members = set()
        for ws_root in covered_cargo_roots:
            workspace_members.update(self._cargo_workspace_member_directories(ws_root))
        standalone_cargo_dirs = all_cargo_dirs - workspace_members

        redundant_member_patterns = sorted(
            p for p in cargo_patterns
            if p in workspace_members
        )
        self.assertEqual(
            redundant_member_patterns,
            [],
            f'Redundant cargo entries already covered by a parent workspace: {redundant_member_patterns}',
        )
        self._assert_ecosystem_coverage(
            'cargo',
            standalone_cargo_dirs,
            cargo_patterns,
            self.EXCLUDED_CARGO_DIRECTORIES,
        )

        docker_dirs = self._tracked_manifest_directories(r'Dockerfile(\.[^/]+)?')
        self._assert_ecosystem_coverage(
            'docker',
            docker_dirs,
            updates.get('docker', []),
            self.EXCLUDED_DOCKER_DIRECTORIES,
        )


class FleetHealthSubcontractsExecutionTests(unittest.TestCase):
    @staticmethod
    def _step_conditions(job):
        steps = re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
        result = {}
        for step in steps:
            name_match = re.search(r'^      - name: (.+)$', step, re.M)
            if not name_match:
                continue
            cond_match = re.search(r'^        if: (.*?)(?=^        [a-z-]+:|\Z)', step, re.M | re.S)
            if cond_match:
                result[name_match.group(1)] = cond_match.group(1)
        return result

    @staticmethod
    def _eval_matrix_os(expr, values):
        body = expr.strip().removeprefix('${{').removesuffix('}}').strip()
        full_oses = ['ubuntu-24.04', 'windows-2022', 'macos-15']
        win_only = ['windows-2022']
        mapping = {
            'fromJSON(\'["ubuntu-24.04","windows-2022","macos-15"]\')': full_oses,
            'fromJSON(\'["windows-2022"]\')': win_only,
            **values,
        }
        for token, val in sorted(mapping.items(), key=lambda item: -len(item[0])):
            body = body.replace(token, repr(val))
        # Evaluate GitHub Actions && / || short-circuit value semantics via Python and/or
        body = re.sub(r'!(?!=)', 'not ', body.replace('&&', ' and ').replace('||', ' or '))
        return eval(
            ' '.join(body.split()),
            {'__builtins__': {}},
            {'join': lambda items, sep=',': sep.join(items or ())},
        )

    def test_fleet_health_matrix_and_subcontract_steps_slice_and_fallback_safely(self):
        fleet = JOBS['fleet-health']
        matrix_expr = re.search(r'^\s+os: (\$\{\{.*\}\})$', fleet, re.M).group(1)
        conds = self._step_conditions(fleet)

        def base_vals(*, scope='targeted', ver=1, checks=(), oses=(), run_fleet='true', run_win='false'):
            return {
                'needs.select.outputs.run_fleet_health': run_fleet,
                'needs.select.outputs.run_windows_rollout': run_win,
                'fromJSON(needs.select.outputs.plan).scope': scope,
                'fromJSON(needs.select.outputs.plan).contract_job_consolidation_version': 5,
                'fromJSON(needs.select.outputs.plan).fleet_health_subcontracts_version': ver,
                'fromJSON(needs.select.outputs.plan).fleet_health_checks': list(checks),
                'fromJSON(needs.select.outputs.plan).fleet_health_oses': list(oses),
            }

        # 1) Targeted subcontract matrix.os slicing + full/legacy/empty fallbacks
        self.assertEqual(
            self._eval_matrix_os(matrix_expr, base_vals(checks=['host_cleanup'], oses=['ubuntu-24.04', 'macos-15'])),
            ['ubuntu-24.04', 'macos-15'],
        )
        self.assertEqual(
            self._eval_matrix_os(matrix_expr, base_vals(checks=['macos_signer'], oses=['macos-15'])),
            ['macos-15'],
        )
        self.assertEqual(
            self._eval_matrix_os(matrix_expr, base_vals(run_fleet='false', run_win='true', checks=[], oses=['windows-2022'])),
            ['windows-2022'],
        )
        for full_scope in ('full', 'explicit-full'):
            self.assertEqual(
                self._eval_matrix_os(matrix_expr, base_vals(scope=full_scope, checks=['macos_signer'], oses=['macos-15'])),
                ['ubuntu-24.04', 'windows-2022', 'macos-15'],
            )
        self.assertEqual(
            self._eval_matrix_os(matrix_expr, base_vals(ver=None, checks=[], oses=[])),
            ['ubuntu-24.04', 'windows-2022', 'macos-15'],
        )
        self.assertEqual(
            self._eval_matrix_os(matrix_expr, base_vals(run_fleet='false', run_win='false', checks=[], oses=[])),
            ['windows-2022'],
        )

        # 2) Step gating across subcontracts: host_cleanup skips Rust on Linux & macOS
        cleanup_vals = base_vals(checks=['host_cleanup'], oses=['ubuntu-24.04', 'macos-15'])
        for runner_os in ('Linux', 'macOS'):
            vals = cleanup_vals | {'runner.os': runner_os}
            self.assertFalse(effective_job_condition(conds['Execute selected group'], vals, ('success',)))
            self.assertFalse(effective_job_condition(conds['Prepare native filesystem contract Rust toolchain'], vals, ('success',)))
            self.assertFalse(effective_job_condition(conds['Verify hosted access cross-process ownership on native host'], vals, ('success',)))
            self.assertFalse(effective_job_condition(conds['Verify private context filesystem and caller on macOS'], vals, ('success',)))
            self.assertFalse(effective_job_condition(conds['Verify local signer contracts on macOS'], vals, ('success',)))
            self.assertTrue(effective_job_condition(conds['Verify cleanup on native host'], vals, ('success',)))
            self.assertEqual(
                effective_job_condition(conds['Ensure Linux process inspection dependency'], vals, ('success',)),
                runner_os == 'Linux',
            )

        # 3) Mixed macos_signer + host_cleanup installs Rust ONLY on macOS, not Linux
        mixed_vals = base_vals(checks=['macos_signer', 'host_cleanup'], oses=['ubuntu-24.04', 'macos-15'])
        self.assertFalse(effective_job_condition(
            conds['Prepare native filesystem contract Rust toolchain'],
            mixed_vals | {'runner.os': 'Linux'}, ('success',),
        ))
        self.assertTrue(effective_job_condition(
            conds['Prepare native filesystem contract Rust toolchain'],
            mixed_vals | {'runner.os': 'macOS'}, ('success',),
        ))
        self.assertTrue(effective_job_condition(
            conds['Verify local signer contracts on macOS'],
            mixed_vals | {'runner.os': 'macOS'}, ('success',),
        ))

        # 4) fleet_script runs Python collector without Rust toolchain on all 3 OSes; windows_rollout-only runs only Windows rollout
        win_rollout_vals = base_vals(run_fleet='false', run_win='true', checks=[], oses=['windows-2022']) | {'runner.os': 'Windows'}
        self.assertTrue(effective_job_condition(conds['Execute consolidated Windows rollout contract'], win_rollout_vals, ('success',)))
        self.assertFalse(effective_job_condition(conds['Execute selected group'], win_rollout_vals, ('success',)))
        self.assertFalse(effective_job_condition(conds['Prepare native filesystem contract Rust toolchain'], win_rollout_vals, ('success',)))
        self.assertFalse(effective_job_condition(conds['Verify hosted access cross-process ownership on native host'], win_rollout_vals, ('success',)))

        fleet_script_vals = base_vals(checks=['fleet_script'], oses=['ubuntu-24.04', 'windows-2022', 'macos-15'])
        for runner_os in ('Linux', 'Windows', 'macOS'):
            vals = fleet_script_vals | {'runner.os': runner_os}
            self.assertTrue(effective_job_condition(conds['Execute selected group'], vals, ('success',)))
            self.assertFalse(effective_job_condition(conds['Prepare native filesystem contract Rust toolchain'], vals, ('success',)))
            self.assertFalse(effective_job_condition(conds['Verify hosted access cross-process ownership on native host'], vals, ('success',)))
            self.assertFalse(effective_job_condition(conds['Verify cleanup on native host'], vals, ('success',)))

        # 5) Legacy BASE (ver=None) and full/explicit-full run all platform-appropriate steps
        for fallback_vals in (
            base_vals(ver=None, checks=[], oses=[]),
            base_vals(scope='full', checks=[], oses=[]),
            base_vals(scope='explicit-full', checks=[], oses=[]),
        ):
            for runner_os in ('Linux', 'Windows', 'macOS'):
                vals = fallback_vals | {'runner.os': runner_os}
                self.assertTrue(effective_job_condition(conds['Execute selected group'], vals, ('success',)))
                self.assertTrue(effective_job_condition(conds['Prepare native filesystem contract Rust toolchain'], vals, ('success',)))
                self.assertTrue(effective_job_condition(conds['Verify hosted access cross-process ownership on native host'], vals, ('success',)))
                self.assertEqual(
                    effective_job_condition(conds['Verify local signer contracts on macOS'], vals, ('success',)),
                    runner_os == 'macOS',
                )
                self.assertEqual(
                    effective_job_condition(conds['Verify cleanup on native host'], vals, ('success',)),
                    runner_os != 'Windows',
                )


MAX_PYTHON_TEST_SCRIPT_LINES = 2500


def _is_python_test_script(path: Path) -> bool:
    name = path.name
    return name.endswith('.test.py') or (name.startswith('test_') and name.endswith('.py'))


def find_oversized_python_test_scripts(
    scripts_dir: Path,
    *,
    max_lines: int = MAX_PYTHON_TEST_SCRIPT_LINES,
) -> tuple[int, list[tuple[str, int]]]:
    """Scan scripts_dir recursively and return (scanned_count, [(rel_path, line_count), ...])."""
    scanned = 0
    violations: list[tuple[str, int]] = []
    for path in sorted(scripts_dir.rglob('*.py')):
        if '__pycache__' in path.parts or not path.is_file():
            continue
        if not _is_python_test_script(path):
            continue
        scanned += 1
        line_count = len(path.read_text(encoding='utf-8').splitlines())
        if line_count > max_lines:
            rel = path.relative_to(scripts_dir).as_posix()
            violations.append((rel, line_count))
    return scanned, violations


class PythonTestScriptLineCapGuardTests(unittest.TestCase):
    """Fail-closed ceiling preventing oversized monolithic Python test scripts (Refs #7068)."""

    def test_all_scripts_python_test_files_stay_within_line_cap(self):
        scanned, violations = find_oversized_python_test_scripts(ROOT / 'scripts')
        self.assertGreaterEqual(
            scanned,
            50,
            f'expected >= 50 Python test scripts under scripts/, got {scanned}',
        )
        self.assertEqual(
            violations,
            [],
            'Oversized Python test scripts exceed the '
            f'{MAX_PYTHON_TEST_SCRIPT_LINES}-line cap (split into modular subsuites; see #7068):\n'
            + '\n'.join(
                f'  scripts/{rel}: {count} lines (limit <= {MAX_PYTHON_TEST_SCRIPT_LINES})'
                for rel, count in violations
            ),
        )

    def test_line_cap_guard_accepts_files_at_exact_limit_and_ignores_non_test_files(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'ok.test.py').write_text('x = 1\n' * MAX_PYTHON_TEST_SCRIPT_LINES, encoding='utf-8')
            nested = root / 'security'
            nested.mkdir()
            (nested / 'test_ok.py').write_text('x = 1\n' * MAX_PYTHON_TEST_SCRIPT_LINES, encoding='utf-8')
            (root / 'helper_not_a_test.py').write_text(
                'x = 1\n' * (MAX_PYTHON_TEST_SCRIPT_LINES + 500),
                encoding='utf-8',
            )
            scanned, violations = find_oversized_python_test_scripts(root)
            self.assertEqual(scanned, 2)
            self.assertEqual(violations, [])

    def test_line_cap_guard_rejects_oversized_top_level_and_nested_test_scripts(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'oversized.test.py').write_text(
                'x = 1\n' * (MAX_PYTHON_TEST_SCRIPT_LINES + 1),
                encoding='utf-8',
            )
            nested = root / 'security'
            nested.mkdir()
            (nested / 'test_nested_oversized.py').write_text(
                'x = 1\n' * (MAX_PYTHON_TEST_SCRIPT_LINES + 42),
                encoding='utf-8',
            )
            scanned, violations = find_oversized_python_test_scripts(root)
            self.assertEqual(scanned, 2)
            self.assertEqual(
                violations,
                [
                    ('oversized.test.py', MAX_PYTHON_TEST_SCRIPT_LINES + 1),
                    ('security/test_nested_oversized.py', MAX_PYTHON_TEST_SCRIPT_LINES + 42),
                ],
            )


if __name__ == '__main__':
    unittest.main()
