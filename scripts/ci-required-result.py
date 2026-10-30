#!/usr/bin/env python3
"""Validate selection and actual Actions results; every expected result is explicit."""
import argparse
import json
import re
from pathlib import Path


def validate(plan, needs, group_jobs):
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
    expected_jobs = {'select', *group_jobs.values()}
    for job, entry in needs.items():
        if job not in expected_jobs:
            if not isinstance(entry, dict) or entry.get('result') != 'success':
                raise ValueError(f'{job}: extra dependency did not explicitly succeed')
    return True


def validate_full(plan, needs, group_jobs, *, event_name, ref, workflow_sha,
                  source_sha, run_mode=''):
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
    full_jobs = {'oasis7-test-build', 'full-regression', 'candidate-full-regression'}
    if not isinstance(needs, dict) or not all(isinstance(value, dict) for value in needs.values()):
        raise ValueError('invalid full needs results')
    validate(plan, {name: value for name, value in needs.items() if name not in full_jobs}, group_jobs)
    if needs.get('oasis7-test-build', {}).get('result') != 'success':
        raise ValueError('full ordinary artifact producer did not succeed')
    trusted_source = (ref == 'refs/heads/main' and source_sha == workflow_sha
                      and plan['policy_sha'] == workflow_sha)
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
    jobs = {group: group.replace('_', '-') for group in config['groups']}
    if args.full:
        validate_full(json.loads(args.plan), json.loads(args.needs), jobs,
                      event_name=args.event_name, ref=args.ref, workflow_sha=args.workflow_sha,
                      source_sha=args.source_sha, run_mode=args.run_mode)
    else:
        validate(json.loads(args.plan), json.loads(args.needs), jobs)
    print('All selected CI groups succeeded.')


if __name__ == '__main__':
    main()
