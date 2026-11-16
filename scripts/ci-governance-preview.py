#!/usr/bin/env python3
"""Compare complete governance execution on one fixed checkout; never authorize CI."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time


def git(root, *args):
    return subprocess.check_output(['git', '-C', str(root), *args], text=True).strip()


def benchmark_plan(root, tested):
    spec = importlib.util.spec_from_file_location(
        'governance_preview_planner', root / 'scripts/plan-rust-required-scope.py')
    planner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(planner)
    config = json.loads((root / 'scripts/ci-required-scope.json').read_text())
    planner.validate_python_control_contracts(config)
    controls = planner.python_control_plan(config, ['scripts/ci-workflow.test.py'])
    if controls.get('python_control_paths') != ['scripts/ci-workflow.test.py']:
        raise ValueError('missing fixed governance control scenario')
    groups = sorted({'baseline', 'workflow_governance', *controls['python_control_tests']})
    return dict(scope='targeted', groups=groups, source_sha=tested, tested_sha=tested,
                policy_sha=tested, **controls)


def legacy_tools(root, base, output):
    for name in ('ci-python-control-contracts.py', 'ci-tests.sh'):
        source = subprocess.check_output(['git', '-C', str(root), 'show', base + ':scripts/' + name])
        (output / name).write_bytes(source)
    runner = output / 'ci-python-control-contracts.py'
    help_text = subprocess.check_output([sys.executable, '-I', str(runner), '--help'], text=True)
    return '--governance' not in help_text


def comparison_commands(root, output, tested):
    plan = output / 'plan.json'
    common = ['run', '--repo-root', str(root), '--policy-sha', tested, '--plan', str(plan)]
    shell = r'''set -euo pipefail
repo_root="$1"; driver_dir="$2"; ci_python="$3"; tier=required
source "$4"
cd "$repo_root"
run_workflow_governance_contract_tests
'''
    return [
        ('legacy-controls', [sys.executable, str(output / 'ci-python-control-contracts.py'), *common]),
        ('legacy-complete-driver', ['bash', '-c', shell, 'legacy-governance', str(root),
                                   str(root / 'scripts'), sys.executable, str(output / 'ci-tests.sh')]),
        ('legacy-extra-contracts', ['bash', '-e', '-u', '-o', 'pipefail', '-c',
                                   'python3 scripts/package-source-plan.test.py\n'
                                   'python3 scripts/safe-git-archive.test.py\n'
                                   'node scripts/cache-permission-probe.test.cjs']),
        ('candidate-complete-owner', [sys.executable, str(root / 'scripts/ci-python-control-contracts.py'),
                                      *common, '--governance']),
    ]


def run_comparison(commands, root, output, environment, report):
    result_path = output / 'result.json'
    report.update(complete=False, stages=[])
    result_path.write_text(json.dumps(report, indent=2) + '\n')
    for name, command in commands:
        started = time.monotonic()
        print('governance_preview_stage=' + name, flush=True)
        with (output / (name + '.log')).open('w') as stream:
            completed = subprocess.run(command, cwd=root, env=environment,
                                       stdout=stream, stderr=subprocess.STDOUT, timeout=1800)
        stage = dict(name=name, seconds=round(time.monotonic() - started, 3),
                     exit_code=completed.returncode)
        report['stages'].append(stage)
        result_path.write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps(stage), flush=True)
        if completed.returncode:
            print((output / (name + '.log')).read_text(errors='replace')[-16000:], flush=True)
            raise RuntimeError('incomplete governance comparison: ' + name)
    if [name for name, _ in commands] != [
            'legacy-controls', 'legacy-complete-driver', 'legacy-extra-contracts', 'candidate-complete-owner']:
        raise ValueError('incomplete comparison stage inventory')
    report.update(complete=True,
                  legacy_seconds=round(sum(stage['seconds'] for stage in report['stages'][:3]), 3),
                  candidate_seconds=report['stages'][3]['seconds'])
    report['saved_seconds'] = round(report['legacy_seconds'] - report['candidate_seconds'], 3)
    result_path.write_text(json.dumps(report, indent=2) + '\n')
    return report


def compare(root, base, source, tested, output):
    for revision in (base, source, tested):
        if not re.fullmatch(r'[0-9a-f]{40}', revision):
            raise ValueError('comparison requires fixed commit identities')
        if git(root, 'rev-parse', '--verify', '--end-of-options', revision + '^{commit}') != revision:
            raise ValueError('comparison revision is not a commit')
    if git(root, 'rev-parse', 'HEAD') != tested:
        raise ValueError('comparison checkout identity mismatch')
    if source != tested:
        parents = git(root, 'show', '-s', '--format=%P', tested).split()
        if len(parents) != 2 or parents[1] != source:
            raise ValueError('comparison requires the actual target/source merge')
    if output == root or root in output.parents:
        raise ValueError('diagnostic output must be outside the checkout')
    output.mkdir(parents=True, exist_ok=False)
    report = dict(authority='diagnostic-only', actual_base_sha=base, actual_source_sha=source,
                  tested_sha=tested, benchmark_policy_sha=tested, tree=git(root, 'rev-parse', 'HEAD^{tree}'),
                  workload='registered-workflow-control-scenario-and-complete-governance',
                  order='legacy-then-candidate', cache_restored=False,
                  limitation='Sequential same-runner replay, with warm-order effects; not PR authorization or P95.',
                  complete=False)
    (output / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
    if not legacy_tools(root, base, output):
        report['not_applicable'] = 'base_already_single_owner'
        (output / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
        return report
    plan = benchmark_plan(root, tested)
    (output / 'plan.json').write_text(json.dumps(plan, indent=2) + '\n')
    environment = dict(os.environ, SOURCE_SHA=tested, PYTHONDONTWRITEBYTECODE='1', CI_VERBOSE='1')
    return run_comparison(comparison_commands(root, output, tested), root, output, environment, report)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo-root', type=Path, required=True)
    parser.add_argument('--base-sha', required=True)
    parser.add_argument('--source-sha', required=True)
    parser.add_argument('--tested-sha', required=True)
    parser.add_argument('--output-dir', type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(compare(args.repo_root.resolve(), args.base_sha, args.source_sha,
                             args.tested_sha, args.output_dir.resolve()), indent=2))


if __name__ == '__main__':
    main()
