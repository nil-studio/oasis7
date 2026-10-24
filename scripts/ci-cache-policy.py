#!/usr/bin/env python3
"""Capacity policy shared by trusted workspace-cache producers.

Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7.
This module neither uploads nor deletes caches. A complete inventory is a
point-in-time observation, not a reservation against concurrent writers.
"""
import re


MAIN_REF = 'refs/heads/main'
FORMAL_V2 = (r'formal-workspace-v2-ubuntu24-(?:X64|ARM64)-\d+\.\d+\.\d+-'
             r'nightly-\d{4}-\d{2}-\d{2}-[0-9a-f]{64}-[0-9a-f]{40}')
SOURCE_SUFFIX = r'[0-9a-f]{64}-[0-9a-f]{40}'
FAMILIES = {
    'formal': {
        'current_pattern': re.compile(r'formal-workspace-v3-' + SOURCE_SUFFIX + r'\Z', re.ASCII),
        'managed_pattern': re.compile(r'(?:' + FORMAL_V2 + r'|formal-workspace-v3-'
                                      + SOURCE_SUFFIX + r')\Z', re.ASCII),
        'max_upload_bytes': 768 * 1024 ** 2, 'staging_bytes': 8 * 1024 ** 3,
        'keep_generations': 2,
    },
    'ordinary': {
        'current_pattern': re.compile(r'ordinary-workspace-v1-' + SOURCE_SUFFIX + r'\Z', re.ASCII),
        'managed_pattern': re.compile(r'ordinary-workspace-v1-' + SOURCE_SUFFIX + r'\Z', re.ASCII),
        'max_upload_bytes': 768 * 1024 ** 2, 'staging_bytes': 6 * 1024 ** 3,
        'keep_generations': 2,
    },
    'ecs': {
        'current_pattern': re.compile(r'ecs-workspace-v2-' + SOURCE_SUFFIX + r'\Z', re.ASCII),
        'managed_pattern': re.compile(r'ecs-workspace-v[12]-' + SOURCE_SUFFIX + r'\Z', re.ASCII),
        'max_upload_bytes': 1024 ** 3, 'staging_bytes': 6 * 1024 ** 3,
        'keep_generations': 2,
    },
}
for _specification in FAMILIES.values():
    # A raw staging cap is not a cache-storage entitlement. Two retained
    # uploads and a temporary third have distinct compressed-byte budgets;
    # the full repository observation still decides every admission.
    _specification['max_bytes'] = _specification['keep_generations'] * _specification['max_upload_bytes']
    _specification['rotation_bytes'] = (_specification['keep_generations'] + 1) * _specification['max_upload_bytes']
# Current ECS v2 can still have three old snapshots across compatibility groups.
# The first adoption of a genuinely absent compatibility may bridge that state;
# it never bypasses the per-upload or full repository budget and cannot delete.
FAMILIES['ecs']['migration_bytes'] = 4 * 1024 ** 3


def family_for_key(key):
    if isinstance(key, str):
        for name, specification in FAMILIES.items():
            if specification['managed_pattern'].fullmatch(key):
                return name
    return None


def is_current_key(key):
    name = family_for_key(key)
    return bool(name and FAMILIES[name]['current_pattern'].fullmatch(key))


def quota_bytes(value):
    limit = value.get('max_cache_size_gb') if isinstance(value, dict) else None
    if type(limit) is not int or limit <= 0:
        raise ValueError('repository cache quota unavailable or invalid')
    return limit * 1000 ** 3


def configured_budget(value):
    if value in (None, ''):
        return None
    if not isinstance(value, str) or not re.fullmatch(r'[1-9][0-9]{0,18}', value, re.ASCII):
        raise ValueError('configured repository budget must be positive decimal bytes')
    return int(value)


def capacity_context(maintenance, api, budget, report):
    """Keep the configured budget distinct from an unreadable live quota."""
    if budget is not None and (type(budget) is not int or budget <= 0):
        raise ValueError('configured repository budget must be positive bytes')
    report.pop('quota_api_diagnostics', None)
    report.update(configured_repository_budget_bytes=budget, live_repository_limit_bytes=None,
                  effective_repository_budget_bytes=None, capacity_basis=None,
                  inventory_complete=False, observed_cache_count=None, observed_repository_bytes=None,
                  available_repository_bytes=None)
    entries = maintenance.validate_inventory(api.list_caches())
    report.update(inventory_complete=True, observed_cache_count=len(entries),
                  observed_repository_bytes=sum(entry['size_in_bytes'] for entry in entries))
    try:
        live = quota_bytes(api.request('GET', '/actions/cache/storage-limit'))
    except maintenance.MaintenanceApiError as error:
        report['quota_api_diagnostics'] = error.diagnostics
        if (budget is None or error.status != 403 or error.rate_limited
                or error.diagnostics['category'] not in ('integration_access', 'personal_token_access')):
            raise
        effective, basis = budget, 'verified_configured_budget'
    else:
        report['live_repository_limit_bytes'] = live
        effective = min(live, budget) if budget is not None else live
        basis = 'live_quota_and_configured_budget' if budget is not None else 'live_quota'
    report.update(effective_repository_budget_bytes=effective, capacity_basis=basis,
                  available_repository_bytes=max(0, effective - report['observed_repository_bytes']))
    return entries


def save_policy(entries, key, upload_bytes, repository_limit_bytes):
    name = family_for_key(key)
    if not name or not is_current_key(key):
        raise ValueError('workspace cache key is outside the current managed schema')
    if type(upload_bytes) is not int or upload_bytes <= 0:
        raise ValueError('cache upload bound must be positive bytes')
    specification = FAMILIES[name]
    family = [entry for entry in entries if entry['ref'] == MAIN_REF
              and family_for_key(entry['key']) == name]
    compatible = [entry for entry in family if entry['key'].rsplit('-', 1)[0] == key.rsplit('-', 1)[0]]
    migration = name == 'ecs' and not compatible
    family_limit = specification['migration_bytes'] if migration else specification['rotation_bytes']
    observed = sum(entry['size_in_bytes'] for entry in entries)
    projected = observed + upload_bytes
    report = dict(capacity_allowed=False, family=name,
                  effective_repository_budget_bytes=repository_limit_bytes,
                  observed_repository_bytes=observed, projected_repository_bytes=projected,
                  new_payload_upload_bound_bytes=upload_bytes,
                  projected_family_bytes=sum(entry['size_in_bytes'] for entry in family) + upload_bytes,
                  single_upload_limit_bytes=specification['max_upload_bytes'],
                  family_resident_limit_bytes=specification['max_bytes'],
                  family_rotation_limit_bytes=specification['rotation_bytes'],
                  family_migration_limit_bytes=specification.get('migration_bytes'),
                  family_limit_bytes=family_limit, migration_admission=migration,
                  admission_basis='new ECS compatibility migration' if migration else 'normal family rotation',
                  compatible_generations=len(compatible),
                  rotation_required=len(compatible) >= specification['keep_generations'],
                  compatible_cache_ids=sorted(entry['id'] for entry in compatible))
    if type(repository_limit_bytes) is not int or repository_limit_bytes <= 0:
        report['reason'] = 'repository cache quota unavailable or invalid'
    elif projected > repository_limit_bytes:
        report['reason'] = 'repository cache capacity gap'
    elif any(entry['key'] == key for entry in family):
        report['reason'] = 'existing immutable key'
    elif upload_bytes > specification['max_upload_bytes']:
        report['reason'] = name + ' single upload budget'
    elif report['projected_family_bytes'] > family_limit:
        report['reason'] = name + ' main family rotation budget'
    else:
        # Two generations is the post-save retention target. Admit a temporary
        # third only when its complete upload fits both budgets; confirmed-save
        # maintenance may then retire unprotected old generations.
        report['capacity_allowed'] = True
    return report
