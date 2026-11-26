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
LEGACY_V2_CONSOLIDATED_GROUPS = LEGACY_V1_CONSOLIDATED_GROUPS | frozenset({
    'consensus',
    'pixel_world_bridge',
    'viewer_performance_report',
})
LEGACY_V3_CONSOLIDATED_GROUPS = LEGACY_V2_CONSOLIDATED_GROUPS | frozenset({
    'packaging_contracts',
    'cargo_tooling_contracts',
    'windows_rollout',
})
LEGACY_V4_CONSOLIDATED_GROUPS = LEGACY_V3_CONSOLIDATED_GROUPS | frozenset({
    'operational_contracts',
})
DIRECT_CUSTODY_JOBS = (
    'custody-native-contracts',
    'custody-native',
    'custody-wasm-production',
    'custody-node',
)


def _consolidation_overrides(config, plan):
    if not isinstance(plan, dict):
        return None
    offline = plan.get('command_center_offline_version')
    if 'command_center_offline_version' in plan:
        config_offline = config.get('command_center_offline_version')
        if type(offline) is not int or offline != 1:
            raise ValueError('unsupported command center offline version')
        if (type(config_offline) is not int or config_offline != 1
                or config.get('group_jobs', {}).get('command_center') != 'baseline'
                or plan.get('contract_job_consolidation_version') != 5):
            raise ValueError('untrusted command center offline capability')
    if 'contract_job_consolidation_version' not in plan:
        return None
    version = plan['contract_job_consolidation_version']
    config_version = config.get('contract_job_consolidation_version')
    if type(version) is not int or version not in (1, 2, 3, 4, 5):
        raise ValueError('unsupported contract job consolidation version')
    if type(config_version) is not int or config_version not in (1, 2, 3, 4, 5) or version > config_version:
        raise ValueError('untrusted contract job consolidation capability')
    overrides = config.get('group_jobs')
    if not isinstance(overrides, dict) or not overrides:
        raise ValueError('missing consolidated group job mapping')
    default_jobs = {group.replace('_', '-') for group in config['groups']}
    for group, job in overrides.items():
        if (group not in config['groups'] or not isinstance(job, str)
                or job not in default_jobs or job == group.replace('_', '-')):
            raise ValueError('invalid consolidated group job mapping')
    if version < config_version:
        allowed = {
            1: LEGACY_V1_CONSOLIDATED_GROUPS,
            2: LEGACY_V2_CONSOLIDATED_GROUPS,
            3: LEGACY_V3_CONSOLIDATED_GROUPS,
            4: LEGACY_V4_CONSOLIDATED_GROUPS,
        }[version]
        result = {group: job for group, job in overrides.items() if group in allowed}
        if config_version == 5:
            # v1..v4 routed compile_metrics (v1..v4) and packaging_contracts (v3..v4)
            # to standalone doc-checker-contracts before v5 folded them into baseline.
            if 'compile_metrics' in result:
                result['compile_metrics'] = 'doc-checker-contracts'
            if version in (3, 4) and 'packaging_contracts' in result:
                result['packaging_contracts'] = 'doc-checker-contracts'
        return result
    result = dict(overrides)
    if offline != 1:
        result.pop('command_center', None)
    return result


def _direct_workspace_support_gate_version(config, plan):
    if not isinstance(plan, dict) or 'workspace_support_direct_gate_version' not in plan:
        return 0
    version = plan['workspace_support_direct_gate_version']
    config_version = config.get('workspace_support_direct_gate_version')
    if type(version) is not int or version not in (1, 2):
        raise ValueError('unsupported direct workspace support gate version')
    if type(config_version) is not int or config_version not in (1, 2) or version > config_version:
        raise ValueError('untrusted direct workspace support gate capability')
    if ('workspace_support' not in plan.get('groups', ())
            or plan.get('scope') == 'explicit-full'
            or plan.get('workspace_support_partition_version') != 1):
        raise ValueError('invalid direct workspace support gate plan')
    if version == 1:
        if (plan.get('custody_execution_version') == 1
                or plan.get('workspace_support_partitions') != ['other']):
            raise ValueError('invalid direct workspace support gate plan')
        return 1
    if plan.get('custody_execution_version') == 1:
        partitions = plan.get('workspace_support_partitions')
        if not isinstance(partitions, list) or 'custody' not in partitions:
            raise ValueError('invalid direct workspace support gate plan')
        return 2
    if plan.get('workspace_support_partitions') != ['other']:
        raise ValueError('invalid direct workspace support gate plan')
    return 2


def configured_group_jobs(config, plan=None):
    jobs = {group: group.replace('_', '-') for group in config['groups']}
    overrides = _consolidation_overrides(config, plan)
    if overrides:
        jobs.update(overrides)
    direct = _direct_workspace_support_gate_version(config, plan)
    if direct:
        jobs['workspace_support'] = (
            'custody-node' if (direct == 2 and plan.get('custody_execution_version') == 1)
            else 'workspace-support-other'
        )
    return jobs


def retired_group_jobs(config, plan=None):
    retired = set()
    overrides = _consolidation_overrides(config, plan)
    if overrides:
        active = set(configured_group_jobs(config, plan).values())
        retired.update({group.replace('_', '-') for group in overrides} - active)
    if _direct_workspace_support_gate_version(config, plan):
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
    if 'workspace_support' in group_jobs:
        direct_v2_custody = (
            isinstance(plan, dict)
            and plan.get('workspace_support_direct_gate_version') == 2
            and plan.get('custody_execution_version') == 1
        )
        if 'workspace-support-other' in needs and 'workspace-support-other' not in expected_jobs:
            expected_jobs.add('workspace-support-other')
            if direct_v2_custody:
                need_other = 'other' in plan.get('workspace_support_partitions', ())
                allowed_other = {'success'} if need_other else {'skipped'}
            else:
                allowed_other = {'skipped', 'success'}
            if needs['workspace-support-other'].get('result') not in allowed_other:
                raise ValueError('workspace-support-other: unexpected result '
                                 f"{needs['workspace-support-other'].get('result')!r}")
        for custody_job in DIRECT_CUSTODY_JOBS:
            if custody_job in needs and custody_job not in expected_jobs:
                expected_jobs.add(custody_job)
                allowed_custody = {'success'} if direct_v2_custody else {'skipped', 'success'}
                if needs[custody_job].get('result') not in allowed_custody:
                    raise ValueError(f'{custody_job}: unexpected result '
                                     f"{needs[custody_job].get('result')!r}")
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
