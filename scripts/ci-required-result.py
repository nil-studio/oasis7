#!/usr/bin/env python3
"""Validate selection and actual Actions results; every expected result is explicit."""
import argparse
import json
import re
from pathlib import Path


LEGACY_V1_CONSOLIDATED_GROUPS = frozenset({
    'codex_agent_config_validation',
    'macos_package_contract',
    'compile_metrics',
})


def _consolidation_overrides(config, plan):
    if not isinstance(plan, dict) or 'contract_job_consolidation_version' not in plan:
        return None
    version = plan['contract_job_consolidation_version']
    config_version = config.get('contract_job_consolidation_version')
    if type(version) is not int or version not in (1, 2):
        raise ValueError('unsupported contract job consolidation version')
    if type(config_version) is not int or config_version not in (1, 2) or version > config_version:
        raise ValueError('untrusted contract job consolidation capability')
    overrides = config.get('group_jobs')
    if not isinstance(overrides, dict) or not overrides:
        raise ValueError('missing consolidated group job mapping')
    default_jobs = {group.replace('_', '-') for group in config['groups']}
    for group, job in overrides.items():
        if (group not in config['groups'] or not isinstance(job, str)
                or job not in default_jobs or job == group.replace('_', '-')):
            raise ValueError('invalid consolidated group job mapping')
    if version == 1 and config_version == 2:
        return {group: job for group, job in overrides.items() if group in LEGACY_V1_CONSOLIDATED_GROUPS}
    return dict(overrides)


def _direct_workspace_support_gate(config, plan):
    if not isinstance(plan, dict) or 'workspace_support_direct_gate_version' not in plan:
        return False
    version = plan['workspace_support_direct_gate_version']
    if type(version) is not int or version != 1:
        raise ValueError('unsupported direct workspace support gate version')
    if config.get('workspace_support_direct_gate_version') != 1:
        raise ValueError('untrusted direct workspace support gate capability')
    if ('workspace_support' not in plan.get('groups', ())
            or plan.get('scope') == 'explicit-full'
            or plan.get('custody_execution_version') == 1
            or plan.get('workspace_support_partition_version') != 1
            or plan.get('workspace_support_partitions') != ['other']):
        raise ValueError('invalid direct workspace support gate plan')
    return True


def configured_group_jobs(config, plan=None):
    jobs = {group: group.replace('_', '-') for group in config['groups']}
    overrides = _consolidation_overrides(config, plan)
    if overrides:
        jobs.update(overrides)
    if _direct_workspace_support_gate(config, plan):
        jobs['workspace_support'] = 'workspace-support-other'
    return jobs


def retired_group_jobs(config, plan=None):
    retired = set()
    overrides = _consolidation_overrides(config, plan)
    if overrides:
        active = set(configured_group_jobs(config, plan).values())
        retired.update({group.replace('_', '-') for group in overrides} - active)
    if _direct_workspace_support_gate(config, plan):
        retired.add('workspace-support')
    return retired


def validate(plan, needs, group_jobs, *, retired_jobs=()):
    if not isinstance(needs, dict) or not all(isinstance(entry, dict) for entry in needs.values()):
        raise ValueError('invalid needs results')
    if needs.get('select', {}).get('result') != 'success':
        raise ValueError('select did not succeed')
    chosen = plan.get('groups') if isinstance(plan, dict) else None
    if (not isinstance(chosen, list) or not all(isinstance(group, str) for group in chosen)
            or len(chosen) != len(set(chosen)) or 'baseline' not in chosen
            or set(chosen) - set(group_jobs)):
        raise ValueError('invalid selected groups, missing baseline or duplicate groups')
    for group, job in group_jobs.items():
        result = needs.get(job, {}).get('result')
        allowed = {'success'} if group in chosen else {'skipped', 'success'}
        if result not in allowed:
            raise ValueError(f'{group}: unexpected result {result!r}')
    retired = set(retired_jobs)
    for job in retired:
        if job not in needs or needs.get(job, {}).get('result') != 'skipped':
            raise ValueError(f'{job}: retired consolidated job must be present and skipped')
    expected_jobs = {'select', *group_jobs.values(), *retired}
    if ('workspace_support' in group_jobs and 'workspace-support-other' in needs
            and 'workspace-support-other' not in expected_jobs):
        expected_jobs.add('workspace-support-other')
        if needs['workspace-support-other'].get('result') not in {'skipped', 'success'}:
            raise ValueError('workspace-support-other: unexpected result '
                             f"{needs['workspace-support-other'].get('result')!r}")
    for job, entry in needs.items():
        if job not in expected_jobs:
            if not isinstance(entry, dict) or entry.get('result') != 'success':
                raise ValueError(f'{job}: extra dependency did not explicitly succeed')
    return True


def validate_full(plan, needs, group_jobs, *, event_name, ref, workflow_sha,
                  source_sha, run_mode='', retired_jobs=()):
    """Full means every configured group plus the exact full-tier producer."""
    if not isinstance(plan, dict) or plan.get('scope') != 'explicit-full':
        raise ValueError('full gate requires an explicit full plan')
    expected_profile = {'schedule': 'periodic-full', 'workflow_dispatch': 'candidate-full'}.get(event_name)
    if (expected_profile is None or plan.get('execution_profile') != expected_profile
            or (event_name == 'workflow_dispatch' and run_mode != 'full')):
        raise ValueError('full profile does not match its event')
    if type(plan.get('check_inventory_version')) is not int or plan['check_inventory_version'] != 1:
        raise ValueError('unsupported full check inventory')
    for key in ('source_sha', 'tested_sha', 'policy_sha'):
        if not isinstance(plan.get(key), str) or not re.fullmatch(r'[0-9a-f]{40}', plan[key]):
            raise ValueError(f'invalid full identity: {key}')
    if plan['source_sha'] != source_sha or plan['tested_sha'] != source_sha:
        raise ValueError('full plan must test the exact source commit')
    if (event_name == 'schedule'
            and (ref != 'refs/heads/main' or source_sha != workflow_sha
                  or plan['policy_sha'] != workflow_sha)):
        raise ValueError('scheduled full identity is not the trusted main workflow')
    chosen = plan.get('groups')
    if (not isinstance(chosen, list) or any(not isinstance(group, str) for group in chosen)
            or set(chosen) != set(group_jobs)):
        raise ValueError('full plan omitted a configured group')
    full_jobs = {'oasis7-test-build', 'oasis7-test-build-candidate', 'oasis7-test-build-trusted',
                 'full-regression', 'candidate-full-regression'}
    if not isinstance(needs, dict) or not all(isinstance(value, dict) for value in needs.values()):
        raise ValueError('invalid full needs results')
    validate(plan, {name: value for name, value in needs.items() if name not in full_jobs},
             group_jobs, retired_jobs=retired_jobs)
    trusted_source = (ref == 'refs/heads/main' and source_sha == workflow_sha
                      and plan['policy_sha'] == workflow_sha)
    expected_ordinary = ('skipped', 'success') if trusted_source else ('success', 'skipped')
    if 'oasis7-test-build' in needs:
        if needs.get('oasis7-test-build', {}).get('result') != 'success':
            raise ValueError('full ordinary artifact producer did not succeed')
        if ({'oasis7-test-build-candidate', 'oasis7-test-build-trusted'} & set(needs)
                and (needs.get('oasis7-test-build-candidate', {}).get('result'),
                     needs.get('oasis7-test-build-trusted', {}).get('result')) != expected_ordinary):
            raise ValueError('full ordinary artifact producers require the exact selected producer')
    else:
        if (needs.get('oasis7-test-build-candidate', {}).get('result'),
                needs.get('oasis7-test-build-trusted', {}).get('result')) != expected_ordinary:
            raise ValueError('full ordinary artifact producer did not succeed')
    selected = 'full-regression' if trusted_source else 'candidate-full-regression'
    unselected = 'candidate-full-regression' if trusted_source else 'full-regression'
    if (needs.get(selected, {}).get('result') != 'success'
            or needs.get(unselected, {}).get('result') != 'skipped'):
        raise ValueError('full tier requires exactly its selected successful producer')
    return True


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--plan', required=True)
    parser.add_argument('--needs', required=True)
    parser.add_argument('--config', default=str(Path(__file__).with_name('ci-required-scope.json')))
    parser.add_argument('--full', action='store_true')
    parser.add_argument('--event-name')
    parser.add_argument('--ref')
    parser.add_argument('--workflow-sha')
    parser.add_argument('--source-sha')
    parser.add_argument('--run-mode', default='')
    args = parser.parse_args()
    config = json.loads(Path(args.config).read_text())
    plan = json.loads(args.plan)
    jobs = configured_group_jobs(config, plan)
    retired = retired_group_jobs(config, plan)
    if args.full:
        validate_full(plan, json.loads(args.needs), jobs,
                      event_name=args.event_name, ref=args.ref, workflow_sha=args.workflow_sha,
                      source_sha=args.source_sha, run_mode=args.run_mode, retired_jobs=retired)
    else:
        validate(plan, json.loads(args.needs), jobs, retired_jobs=retired)
    print('All selected CI groups succeeded.')


if __name__ == '__main__':
    main()
