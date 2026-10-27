#!/usr/bin/env python3
"""Report cache storage and safely retire trusted cache generations.

Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7.
No cache payload is restored here. Remote maintenance is advisory, defaults to
read-only, and only deletes explicitly anchored main workspaces, enumerated
retired schema generations, compatible dependency redundancy with recovery,
or migrated Rust namespaces.
"""
import argparse
from collections import Counter, defaultdict
from datetime import datetime
import html
import importlib.util
import json
import os
from pathlib import Path
import re
import sys
import time
from types import SimpleNamespace
from urllib.error import HTTPError, URLError
from urllib.request import HTTPRedirectHandler, Request, build_opener

sys.dont_write_bytecode = True
_policy_spec = importlib.util.spec_from_file_location('maintenance_policy', Path(__file__).with_name('ci-cache-policy.py'))
policy = importlib.util.module_from_spec(_policy_spec)
_policy_spec.loader.exec_module(policy)

FORMAL_MAX_BYTES = policy.FAMILIES['formal']['max_bytes']
FORMAL_KEEP_GENERATIONS = policy.FAMILIES['formal']['keep_generations']
MAX_REQUESTS = 60
MAX_SECONDS = 120
PAGE_SIZE = 100
# GitHub limits workflow-run queries using status to 1,000 results.
MAX_FILTERED_RUNS = 1000
ACTIVE_RUN_INVENTORY_ATTEMPTS = 3
ACTIVE_RUN_STATUSES = ('requested', 'pending', 'waiting', 'queued', 'in_progress')
MAIN_REF = 'refs/heads/main'
WORKFLOW_PATH = '.github/workflows/rust.yml'
ECS_WORKFLOW_PATH = '.github/workflows/ecs-packages.yml'
RELEASE_WORKFLOW_PATH = '.github/workflows/release-packages.yml'
# The pinned Swatinem action's src/config.ts uses HASH_LENGTH=8 for both hashes.
RUST_KEY = re.compile(r'v0-rust-(?P<family>[A-Za-z0-9_]+(?:-[A-Za-z0-9_]+)*)-'
                      r'(?P<platform>Linux|Windows_NT|Darwin)-(?P<arch>x64|arm64)-'
                      r'[0-9a-f]{8}-[0-9a-f]{8}\Z', re.ASCII)
LEGACY_DEPENDENCY_NAMESPACES = frozenset(('ci-shared-native-trusted-v3', 'oasis7-ecs-native-linux-ecs-v2'))
DEPENDENCY_PLATFORMS = {
    **{name: frozenset({('Linux', 'x64')}) for name in LEGACY_DEPENDENCY_NAMESPACES},
    'release-packages-web-wasm-trusted-v3': frozenset({('Linux', 'x64')}),
    'oasis7-package-native-Linux-native-packaging-trusted-v2': frozenset({('Linux', 'x64')}),
    'release-packages-package-native-Windows-native-trusted-v2': frozenset({('Windows_NT', 'x64')}),
    'release-packages-package-native-macOS-x86_64-apple-darwin-trusted-v2':
        frozenset({('Darwin', 'arm64'), ('Darwin', 'x64')}),
}
DEPENDENCY_NAMESPACES = frozenset(DEPENDENCY_PLATFORMS)
RETIRED_NAMESPACES = {
    'ci-full-regression-trusted-v2': 'ci-shared-native-trusted-v3',
    **{f'release-packages-release-gate-{shard}-trusted-v2': 'ci-shared-native-trusted-v3'
       for shard in ('runtime-core', 'runtime-support', 'runtime-sync', 'soak')},
    'release-packages-web-wasm-trusted-v2': 'release-packages-web-wasm-trusted-v3',
    'oasis7-ecs-native-linux-packaging-v1': 'oasis7-ecs-native-linux-ecs-v2',
    'oasis7-ecs-native-linux-ecs-v1': 'oasis7-ecs-native-linux-ecs-v2',
}
# This list is intentionally explicit. A current format with a different
# compiler/environment hash is never a bootstrap retirement candidate.
RETIRED_WORKSPACE_SCHEMAS = {
    'formal-v2': re.compile(policy.FORMAL_V2 + r'\Z', re.ASCII),
    'ecs-v1': re.compile(r'ecs-workspace-v1-' + policy.SOURCE_SUFFIX + r'\Z', re.ASCII),
}
# These known schemas have fixed Linux X64 writers. Cache version alone is not
# platform proof; a future writer platform must introduce a distinct schema.
LINEAGE_WORKSPACE_SCHEMAS = frozenset(('ordinary-workspace-v1', 'formal-workspace-v3', 'ecs-workspace-v2'))
# Leave room in the shared request budget for inventories, active runs and DELETEs.
MAX_ANCESTRY_REQUESTS = 20


class MaintenanceError(ValueError):
    """No further mutations are safe with the available evidence."""


class ActiveRunInventoryChanged(MaintenanceError):
    """A complete fresh inventory may resolve non-atomic status pagination."""


class MaintenanceApiError(MaintenanceError):
    """An HTTP failure with bounded, non-secret diagnostics for cache callers."""

    def __init__(self, method, error):
        self.status = error.code
        self.diagnostics = {'http_status': error.code}
        headers = error.headers or {}
        allowed = {
            'X-Accepted-GitHub-Permissions': ('accepted_permissions',
                r'[a-z][a-z0-9_-]*=(?:read|write)(?:[,;] ?[a-z][a-z0-9_-]*=(?:read|write))*'),
            'X-GitHub-Request-Id': ('request_id', r'[A-Fa-f0-9:]{1,128}'),
        }
        for header, (name, pattern) in allowed.items():
            value = headers.get(header, '')
            if isinstance(value, str) and len(value) <= 512 and re.fullmatch(pattern, value, re.ASCII):
                self.diagnostics[name] = value
        for header, name in (('X-RateLimit-Remaining', 'rate_limit_remaining'),
                             ('X-RateLimit-Reset', 'rate_limit_reset'),
                             ('Retry-After', 'retry_after_seconds')):
            value = headers.get(header, '')
            if isinstance(value, str) and re.fullmatch(r'[0-9]{1,16}', value, re.ASCII):
                self.diagnostics[name] = int(value)
        # Classify known errors without publishing arbitrary response text, URLs,
        # credentials, header values or exception messages.
        category = 'unknown'
        try:
            body = error.read(16 * 1024 + 1)
            value = json.loads(body) if len(body) <= 16 * 1024 else None
            message = value.get('message') if isinstance(value, dict) else None
            if message == 'Resource not accessible by integration':
                category = 'integration_access'
            elif message == 'Resource not accessible by personal access token':
                category = 'personal_token_access'
            elif isinstance(message, str) and 'rate limit' in message.lower():
                category = 'rate_limit'
        except (OSError, ValueError):
            pass
        self.diagnostics['category'] = category
        self.rate_limited = (error.code == 429 or category == 'rate_limit'
                             or self.diagnostics.get('rate_limit_remaining') == 0
                             or headers.get('Retry-After') is not None)
        super().__init__(f'cache API {method} returned HTTP {error.code}')


def timestamp(value):
    try:
        parsed = datetime.fromisoformat(value.replace('Z', '+00:00'))
        if parsed.tzinfo is None:
            raise ValueError('timezone missing')
        return parsed
    except (AttributeError, TypeError, ValueError) as error:
        raise MaintenanceError('cache/run timestamp is invalid') from error


def validate_inventory(entries):
    if not isinstance(entries, list):
        raise MaintenanceError('cache inventory is not a list')
    ids = set()
    for entry in entries:
        if not isinstance(entry, dict):
            raise MaintenanceError('cache entry is not an object')
        if type(entry.get('id')) is not int or entry['id'] <= 0 or entry['id'] in ids:
            raise MaintenanceError('cache inventory has invalid or duplicate IDs')
        ids.add(entry['id'])
        if type(entry.get('size_in_bytes')) is not int or entry['size_in_bytes'] < 0:
            raise MaintenanceError('cache size is invalid')
        for field in ('key', 'ref', 'version'):
            value = entry.get(field)
            if not isinstance(value, str) or not value or len(value) > 1024 or any(ord(c) < 32 for c in value):
                raise MaintenanceError(f'cache {field} is invalid')
        timestamp(entry.get('created_at'))
        timestamp(entry.get('last_accessed_at'))
    return entries


def managed(entry):
    return entry['ref'] == MAIN_REF and policy.family_for_key(entry['key']) is not None


def compatibility(entry):
    # The archive version encodes paths/compression and is part of restore identity.
    return entry['key'].rsplit('-', 1)[0], entry['version']


def immutable_identity(entry):
    return tuple(entry[field] for field in ('id', 'key', 'ref', 'version', 'created_at', 'size_in_bytes'))


def validate_active_runs(entries, repository=None, *, allow_completed=False):
    if not isinstance(entries, list):
        raise MaintenanceError('active run inventory is not a list')
    ids = set()
    statuses = (*ACTIVE_RUN_STATUSES, 'completed') if allow_completed else ACTIVE_RUN_STATUSES
    for entry in entries:
        if not isinstance(entry, dict):
            raise MaintenanceError('active run entry is not an object')
        if type(entry.get('id')) is not int or entry['id'] <= 0 or entry['id'] in ids:
            raise MaintenanceError('active run inventory has invalid or duplicate IDs')
        ids.add(entry['id'])
        if entry.get('status') not in statuses:
            raise MaintenanceError('active run status is unknown')
        timestamp(entry.get('created_at'))
        if not isinstance(entry.get('head_sha'), str) or not re.fullmatch('[0-9a-f]{40}', entry['head_sha']):
            raise MaintenanceError('active run source identity is invalid')
        path = entry.get('path')
        if not isinstance(path, str) or not path or len(path) > 1024 or any(ord(c) < 32 for c in path):
            raise MaintenanceError('active run workflow identity is invalid')
        owner = entry.get('repository')
        if (not isinstance(owner, dict) or not isinstance(owner.get('full_name'), str)
                or not re.fullmatch('[A-Za-z0-9][A-Za-z0-9-]*/[A-Za-z0-9_.-]+', owner['full_name'])
                or (repository is not None and owner['full_name'] != repository)):
            raise MaintenanceError('active run repository identity is invalid')
    return entries


def run_identity(entry):
    return (entry['id'], entry['head_sha'], entry['path'], entry['created_at'], entry['repository']['full_name'])


def protected_by_activity(entry, cutoff):
    return (entry['size_in_bytes'] == 0 or timestamp(entry['created_at']) >= cutoff
            or timestamp(entry['last_accessed_at']) >= cutoff)


def rust_namespace(entry):
    match = RUST_KEY.fullmatch(entry['key']) if entry['ref'] == MAIN_REF else None
    if not match:
        return None
    platforms = DEPENDENCY_PLATFORMS.get(match['family'])
    if platforms is None and match['family'] in RETIRED_NAMESPACES:
        platforms = {('Linux', 'x64')}
    return match['family'] if (match['platform'], match['arch']) in (platforms or ()) else None


def plan_namespace_retirement(entries, cutoff, *, protected_ids=()):
    replacements = defaultdict(list)
    owned, protected, deletions = [], [], []
    for entry in entries:
        namespace = rust_namespace(entry)
        if namespace in RETIRED_NAMESPACES:
            owned.append(entry)
        elif namespace in RETIRED_NAMESPACES.values() and entry['size_in_bytes'] > 0:
            # Record every complete replacement identity. The independent
            # dependency rule may prune within a restore-compatible group;
            # neither rule chooses an entire environment hash to retire.
            replacements[namespace].append(entry)
    missing = set()
    for entry in owned:
        replacement = RETIRED_NAMESPACES[rust_namespace(entry)]
        if entry['id'] in protected_ids or protected_by_activity(entry, cutoff):
            protected.append(entry['id'])
        elif not replacements[replacement]:
            missing.add(replacement)
        else:
            deletions.append(dict(entry, reason='retired Rust cache namespace', replacement_family=replacement))
    managed_bytes = sum(entry['size_in_bytes'] for entry in owned)
    return {'managed_bytes': managed_bytes, 'managed_count': len(owned),
            'protected_ids': sorted(protected), 'missing_replacements': sorted(missing),
            'replacements': dict(replacements),
            'projected_bytes': managed_bytes - sum(entry['size_in_bytes'] for entry in deletions),
            'delete': deletions}


def plan_redundant_generations(owned, cutoff, reason):
    """Preserve recovery in each already-allowlisted full prefix/version group."""
    groups = defaultdict(list)
    for entry in owned:
        groups[compatibility(entry)].append(entry)
    key_counts = Counter(entry['key'] for entry in owned)
    protected, recoveries, blocked, deletions = set(), [], [], []
    for (prefix, version), group in sorted(groups.items()):
        complete = [entry for entry in group if entry['size_in_bytes'] > 0]
        created = [timestamp(entry['created_at']) for entry in complete]
        blocked_reason = None
        if any(key_counts[entry['key']] != 1 for entry in group):
            blocked_reason = 'ambiguous exact cache key'
        elif len(created) != len(set(created)):
            blocked_reason = 'ambiguous generation timestamps'
        elif not complete:
            blocked_reason = 'no complete recovery snapshot'
        if blocked_reason:
            protected.update(entry['id'] for entry in group)
            blocked.append({'prefix': prefix, 'version': version, 'reason': blocked_reason})
            continue
        # Select within a fully identified restore compatibility/version only.
        # Creation order never chooses one environment group for elimination.
        recovery = max(complete, key=lambda entry: timestamp(entry['created_at']))
        recoveries.append(recovery)
        for entry in group:
            if entry['id'] == recovery['id'] or protected_by_activity(entry, cutoff):
                protected.add(entry['id'])
            else:
                deletions.append(dict(entry, reason=reason, recovery_id=recovery['id']))
    managed_bytes = sum(entry['size_in_bytes'] for entry in owned)
    return {'managed_bytes': managed_bytes, 'managed_count': len(owned),
            'keep_per_compatibility': 1, 'protected_ids': sorted(protected),
            'protected_bytes': sum(entry['size_in_bytes'] for entry in owned if entry['id'] in protected),
            'recoveries': recoveries, 'recovery_ids': sorted(entry['id'] for entry in recoveries),
            'blocked_groups': blocked,
            'projected_bytes': managed_bytes - sum(entry['size_in_bytes'] for entry in deletions),
            'delete': deletions}


def plan_retired_schemas(entries, cutoff):
    owned = [entry for entry in entries if entry['ref'] == MAIN_REF
             and any(pattern.fullmatch(entry['key']) for pattern in RETIRED_WORKSPACE_SCHEMAS.values())]
    return plan_redundant_generations(owned, cutoff, 'older retired workspace schema generation')


def plan_dependency_retention(entries, cutoff, *, namespaces=DEPENDENCY_NAMESPACES):
    # RUST_KEY and the main ref identify the exact action restore boundary:
    # remove only the final dependency hash, preserving namespace, platform,
    # architecture, environment hash and cache version in every group.
    owned = [entry for entry in entries if rust_namespace(entry) in namespaces]
    return plan_redundant_generations(owned, cutoff, 'older compatible dependency snapshot')


def plan_bootstrap(entries, run_started_at, *, active_runs=None):
    """Prune enumerated retired formats and compatible dependency redundancy."""
    validate_inventory(entries)
    active_runs = validate_active_runs([] if active_runs is None else active_runs)
    protection_started_at = min([run_started_at, *(run['created_at'] for run in active_runs)], key=timestamp)
    cutoff = timestamp(protection_started_at)
    schemas = plan_retired_schemas(entries, cutoff)
    dependencies = plan_dependency_retention(entries, cutoff, namespaces=LEGACY_DEPENDENCY_NAMESPACES)
    # Bootstrap has a narrower allowlist than confirmed-save maintenance.
    # Native Rust namespace retirement remains tied to the post-save path.
    retired = plan_namespace_retirement([], cutoff)
    deletions = sorted([*schemas['delete'], *dependencies['delete']],
                       key=lambda entry: (timestamp(entry['created_at']), entry['id']))
    totals = {field: schemas[field] + dependencies[field] for field in
              ('managed_bytes', 'managed_count', 'protected_bytes', 'projected_bytes')}
    return totals | {
        'protected_ids': sorted(set(schemas['protected_ids']) | set(dependencies['protected_ids'])),
        'max_bytes': 0, 'protected_over_budget_bytes': 0,
        'current_key': None, 'current_keys': [], 'families': {},
        'unanchored_families': sorted(policy.FAMILIES),
        'run_created_at': run_started_at, 'protection_started_at': protection_started_at,
        'active_run_ids': sorted(run['id'] for run in active_runs),
        'retired_schemas': schemas, 'dependencies': dependencies,
        'retired_namespaces': retired, 'delete': deletions}


def current_keys(value):
    """An explicit anchor per family; creation order cannot choose an anchor."""
    values = [value] if isinstance(value, str) else value
    if not isinstance(values, (list, tuple)) or not values:
        raise MaintenanceError('retention requires explicit current workspace keys')
    anchors = {}
    for key in values:
        name = policy.family_for_key(key)
        if name is None:
            raise MaintenanceError('current key is outside the managed workspace schemas')
        if name in anchors and anchors[name] != key:
            raise MaintenanceError('each workspace family requires one unambiguous current key')
        anchors[name] = key
    return [anchors[name] for name in sorted(anchors)]


def plan_family_retention(entries, current_key, cutoff, max_bytes, keep_generations):
    name = policy.family_for_key(current_key)
    owned = [entry for entry in entries if managed(entry) and policy.family_for_key(entry['key']) == name]
    current_matches = [entry for entry in owned if entry['key'] == current_key]
    if len(current_matches) != 1 or current_matches[0]['size_in_bytes'] == 0:
        raise MaintenanceError('confirmed current key must identify one complete main cache, including its version')
    current = current_matches[0]
    protected = {entry['id'] for entry in owned
                 if entry['id'] == current['id'] or protected_by_activity(entry, cutoff)}
    previous = [entry for entry in owned if compatibility(entry) == compatibility(current)
                and timestamp(entry['created_at']) < timestamp(current['created_at'])
                and entry['size_in_bytes'] > 0]
    if previous:
        protected.add(max(previous, key=lambda entry: (timestamp(entry['created_at']), entry['id']))['id'])
    retained = [entry for entry in owned if entry['id'] in protected]
    protected_bytes = sum(entry['size_in_bytes'] for entry in retained)
    retained_bytes = protected_bytes
    counts = Counter(compatibility(entry) for entry in retained)
    deletions = []
    candidates = sorted((entry for entry in owned if entry['id'] not in protected),
                        key=lambda entry: (timestamp(entry['last_accessed_at']),
                                           timestamp(entry['created_at']), entry['id']), reverse=True)
    for entry in candidates:
        group = compatibility(entry)
        if group != compatibility(current):
            reason = 'superseded workspace compatibility'
        elif counts[group] >= keep_generations:
            reason = 'generation limit'
        elif retained_bytes + entry['size_in_bytes'] > max_bytes:
            reason = 'workspace family compressed-byte budget'
        else:
            retained_bytes += entry['size_in_bytes']
            counts[group] += 1
            continue
        deletions.append(dict(entry, reason=reason, workspace_family=name))
    return {'managed_bytes': sum(entry['size_in_bytes'] for entry in owned),
            'managed_count': len(owned), 'max_bytes': max_bytes, 'keep_generations': keep_generations,
            'current_key': current_key, 'current_id': current['id'],
            'protected_ids': sorted(protected), 'protected_bytes': protected_bytes,
            'protected_over_budget_bytes': max(0, protected_bytes - max_bytes),
            'projected_bytes': retained_bytes, 'delete': deletions}


def plan_retention(entries, current_key, run_started_at, *, max_bytes=None,
                   keep_generations=FORMAL_KEEP_GENERATIONS, active_runs=None):
    validate_inventory(entries)
    keys = current_keys(current_key)
    if type(keep_generations) is not int or keep_generations < 2:
        raise MaintenanceError('retention must preserve the current cache and a compatible recovery point')
    if max_bytes is not None and (type(max_bytes) is not int or max_bytes <= 0):
        raise MaintenanceError('workspace family byte budget is invalid')
    active_runs = validate_active_runs([] if active_runs is None else active_runs)
    protection_started_at = min([run_started_at, *(run['created_at'] for run in active_runs)], key=timestamp)
    cutoff = timestamp(protection_started_at)
    families = {}
    for key in keys:
        name = policy.family_for_key(key)
        families[name] = plan_family_retention(entries, key, cutoff,
            policy.FAMILIES[name]['max_bytes'] if max_bytes is None else max_bytes, keep_generations)
    dependencies = plan_dependency_retention(entries, cutoff)
    retired = plan_namespace_retirement(entries, cutoff, protected_ids=dependencies['protected_ids'])
    deletions = [entry for plan in families.values() for entry in plan['delete']]
    deletions.extend(dependencies['delete'])
    deletions.extend(retired['delete'])
    # Oldest first; stop on the first unexpected deletion error.
    deletions.sort(key=lambda entry: (timestamp(entry['created_at']), entry['id']))
    totals = {field: sum(plan[field] for plan in families.values()) for field in
              ('managed_bytes', 'managed_count', 'max_bytes', 'protected_bytes',
               'protected_over_budget_bytes', 'projected_bytes')}
    return {**totals, 'keep_generations': keep_generations,
            'current_key': keys[0] if len(keys) == 1 else None, 'current_keys': keys,
            'families': families, 'unanchored_families': sorted(set(policy.FAMILIES) - set(families)),
            'run_created_at': run_started_at, 'protection_started_at': protection_started_at,
            'active_run_ids': sorted(run['id'] for run in active_runs),
            'protected_ids': sorted(entry_id for plan in families.values() for entry_id in plan['protected_ids']),
            'dependencies': dependencies, 'retired_namespaces': retired, 'delete': deletions}


def plan_workspace_recovery(entries, keys, cutoff):
    """Recover within exact compatibility groups unless a confirmed key supersedes one."""
    owned = [entry for entry in entries if managed(entry)]
    current = {}
    for key in keys:
        matches = [entry for entry in owned if entry['key'] == key]
        if len(matches) != 1 or matches[0]['size_in_bytes'] == 0:
            raise MaintenanceError('confirmed current key must identify one complete main cache, including its version')
        current[policy.family_for_key(key)] = matches[0]
    groups = defaultdict(list)
    for entry in owned:
        groups[compatibility(entry)].append(entry)
    key_counts = Counter(entry['key'] for entry in owned)
    protected, recoveries, blocked, deletions, extra = set(), dict(), [], [], []
    for (prefix, version), group in sorted(groups.items()):
        complete = [entry for entry in group if entry['size_in_bytes'] > 0]
        created = [timestamp(entry['created_at']) for entry in complete]
        reason = None
        if any(key_counts[entry['key']] != 1 for entry in group):
            reason = 'ambiguous exact cache key'
        elif len(created) != len(set(created)):
            reason = 'ambiguous generation timestamps'
        elif not complete:
            reason = 'no complete recovery snapshot'
        if reason:
            protected.update(entry['id'] for entry in group)
            blocked.append({'prefix': prefix, 'version': version, 'reason': reason})
            continue
        name = policy.family_for_key(group[0]['key'])
        explicit = current.get(name)
        superseded = explicit is not None and compatibility(explicit) != (prefix, version)
        recovery = explicit if superseded else max(complete, key=lambda entry: timestamp(entry['created_at']))
        recoveries[recovery['id']] = recovery
        anchors = {recovery['id']}
        if explicit is not None:
            # A successfully rebuilt exact hit can be older than another entry;
            # keep both it and the newest compatible recovery, never pick a SHA by time.
            anchors.add(explicit['id'])
            recoveries[explicit['id']] = explicit
        protected.update(entry['id'] for entry in group
                         if entry['id'] in anchors or protected_by_activity(entry, cutoff))
        retained = sum(entry['size_in_bytes'] > 0 and entry['id'] in protected for entry in group)
        keep = policy.FAMILIES[name]['keep_generations'] if policy.is_current_key(group[0]['key']) else 1
        for entry in sorted(group, key=lambda item: (timestamp(item['created_at']), item['id']), reverse=True):
            if entry['id'] in protected:
                continue
            candidate = dict(entry, workspace_family=name, recovery_id=recovery['id'])
            if superseded:
                deletions.append(candidate | {'reason': 'superseded workspace compatibility'})
            elif retained >= keep:
                deletions.append(candidate | {'reason': 'workspace recovery generation limit'})
            else:
                retained += 1
                extra.append(candidate | {'reason': 'dependency rotation headroom'})
    return {'managed_bytes': sum(entry['size_in_bytes'] for entry in owned),
            'managed_count': len(owned), 'protected_ids': sorted(protected),
            'protected_bytes': sum(entry['size_in_bytes'] for entry in owned if entry['id'] in protected),
            'recoveries': list(recoveries.values()), 'recovery_ids': sorted(recoveries),
            'blocked_groups': blocked, 'delete': deletions, 'pressure_candidates': extra}


def workspace_lineage_successors(entries, workspaces, keys, head_sha, is_ancestor):
    """Prove optional storage succession; never extend payload compatibility."""
    if not head_sha or is_ancestor is None:
        return {}
    if not isinstance(head_sha, str) or not re.fullmatch(r'[0-9a-f]{40}', head_sha):
        raise MaintenanceError('workspace lineage requires the verified immutable main source')
    explicit_families = {policy.family_for_key(key) for key in keys}
    groups = defaultdict(list)
    for entry in entries:
        if managed(entry):
            groups[compatibility(entry)].append(entry)
    eligible = {}
    for recovery in workspaces['recoveries']:
        schema = recovery['key'].rsplit('-', 2)[0]
        source = recovery['key'].rsplit('-', 1)[1]
        if (schema in LINEAGE_WORKSPACE_SCHEMAS
                and policy.family_for_key(recovery['key']) not in explicit_families
                and (source == head_sha or is_ancestor(source, head_sha) is True)):
            eligible[recovery['id']] = recovery
    edges = defaultdict(set)
    for old in eligible.values():
        old_group = compatibility(old)
        old_sources = {entry['key'].rsplit('-', 1)[1] for entry in groups[old_group]
                       if entry['size_in_bytes'] > 0}
        for new in eligible.values():
            if (old_group == compatibility(new) or old['version'] != new['version']
                    or old['key'].rsplit('-', 2)[0] != new['key'].rsplit('-', 2)[0]):
                continue
            source = new['key'].rsplit('-', 1)[1]
            # A late old build may be the newest-created entry in its group.
            # Verify every complete old source, not just that group's anchor.
            if all(prior != source and is_ancestor(prior, source) is True for prior in old_sources):
                edges[old['id']].add(new['id'])
    terminal = set(eligible) - {identifier for identifier, successors in edges.items() if successors}
    result = {}
    for identifier, successors in edges.items():
        choices = successors & terminal
        if choices:
            result[identifier] = eligible[min(choices)]
    return result


def plan_recovery(entries, run_started_at, *, repository_limit_bytes, current_key=None, active_runs=None,
                  verified_head_sha=None, is_ancestor=None):
    validate_inventory(entries)
    if type(repository_limit_bytes) is not int or repository_limit_bytes <= 0:
        raise MaintenanceError('recovery requires a verified positive repository budget')
    keys = current_keys(current_key) if current_key else []
    active_runs = validate_active_runs([] if active_runs is None else active_runs)
    protection_started_at = min([run_started_at, *(run['created_at'] for run in active_runs)], key=timestamp)
    cutoff = timestamp(protection_started_at)
    workspaces = plan_workspace_recovery(entries, keys, cutoff)
    dependencies = plan_dependency_retention(entries, cutoff)
    retired = plan_namespace_retirement(entries, cutoff, protected_ids=dependencies['protected_ids'])
    deletions = [*workspaces['delete'], *dependencies['delete'], *retired['delete']]
    observed = sum(entry['size_in_bytes'] for entry in entries)
    projected = observed - sum(entry['size_in_bytes'] for entry in deletions)
    headroom = policy.DEPENDENCY_ROTATION_HEADROOM_BYTES
    resident_target = max(0, repository_limit_bytes - headroom)
    normal_deletions = {entry['id'] for entry in deletions}
    family_bytes = Counter()
    for entry in entries:
        if managed(entry) and entry['id'] not in normal_deletions:
            family_bytes[policy.family_for_key(entry['key'])] += entry['size_in_bytes']
    for entry in sorted(workspaces['pressure_candidates'], key=lambda item: (timestamp(item['created_at']), item['id'])):
        name = entry['workspace_family']
        family_pressure = family_bytes[name] > policy.FAMILIES[name]['max_bytes']
        if projected <= resident_target and not family_pressure:
            continue
        reason = 'workspace family compressed-byte budget' if family_pressure else 'dependency rotation headroom'
        deletions.append(entry | {'reason': reason})
        projected -= entry['size_in_bytes']
        family_bytes[name] -= entry['size_in_bytes']
    workspaces['source_lineage'] = []
    if projected > resident_target or any(family_bytes[name] > spec['max_bytes']
                                         for name, spec in policy.FAMILIES.items()):
        successors = workspace_lineage_successors(entries, workspaces, keys, verified_head_sha, is_ancestor)
        anchors = {entry['id']: entry for entry in workspaces['recoveries']}
        for identifier in sorted(successors, key=lambda value: (timestamp(anchors[value]['created_at']), value)):
            old, successor = anchors[identifier], successors[identifier]
            name = policy.family_for_key(old['key'])
            if (protected_by_activity(old, cutoff)
                    or (projected <= resident_target and family_bytes[name] <= policy.FAMILIES[name]['max_bytes'])):
                continue
            candidate = dict(old, workspace_family=name, recovery_id=successor['id'],
                             reason='verified ancestor workspace compatibility')
            deletions.append(candidate)
            # Generation pruning within this group must use the permanent
            # successor too, because its former local anchor is now retired.
            deletions = [entry | {'recovery_id': successor['id']}
                         if managed(entry) and compatibility(entry) == compatibility(old) else entry
                         for entry in deletions]
            projected -= old['size_in_bytes']
            family_bytes[name] -= old['size_in_bytes']
            workspaces['source_lineage'].append(dict(retired_id=identifier, recovery_id=successor['id'],
                source=old['key'].rsplit('-', 1)[1], successor_source=successor['key'].rsplit('-', 1)[1],
                verified_head_sha=verified_head_sha))
    deletions.sort(key=lambda entry: (timestamp(entry['created_at']), entry['id']))
    deleted_ids = {entry['id'] for entry in deletions}
    workspaces['protected_ids'] = sorted(set(workspaces['protected_ids']) - deleted_ids)
    workspaces['protected_bytes'] = sum(entry['size_in_bytes'] for entry in entries
                                       if entry['id'] in workspaces['protected_ids'])
    workspaces['recoveries'] = [entry for entry in workspaces['recoveries'] if entry['id'] not in deleted_ids]
    workspaces['recovery_ids'] = sorted(entry['id'] for entry in workspaces['recoveries'])
    workspaces['delete'] = [entry for entry in deletions if managed(entry)]
    workspaces['projected_bytes'] = sum(entry['size_in_bytes'] for entry in entries
                                       if managed(entry) and entry['id'] not in deleted_ids)
    families = {}
    for name, specification in policy.FAMILIES.items():
        owned = [entry for entry in entries if managed(entry) and policy.family_for_key(entry['key']) == name]
        protected_bytes = sum(entry['size_in_bytes'] for entry in owned if entry['id'] in workspaces['protected_ids'])
        families[name] = dict(managed_bytes=sum(entry['size_in_bytes'] for entry in owned),
            managed_count=len(owned), max_bytes=specification['max_bytes'],
            protected_bytes=protected_bytes,
            protected_over_budget_bytes=max(0, protected_bytes - specification['max_bytes']),
            projected_bytes=sum(entry['size_in_bytes'] for entry in owned if entry['id'] not in deleted_ids))
    return dict(managed_bytes=workspaces['managed_bytes'], managed_count=workspaces['managed_count'],
        max_bytes=sum(plan['max_bytes'] for plan in families.values()),
        protected_ids=workspaces['protected_ids'], protected_bytes=workspaces['protected_bytes'],
        protected_over_budget_bytes=sum(plan['protected_over_budget_bytes'] for plan in families.values()),
        projected_bytes=workspaces['projected_bytes'], current_keys=keys,
        current_key=keys[0] if len(keys) == 1 else None, families=families,
        unanchored_families=sorted(set(policy.FAMILIES) - {policy.family_for_key(key) for key in keys}),
        run_created_at=run_started_at, protection_started_at=protection_started_at,
        active_run_ids=sorted(run['id'] for run in active_runs),
        workspace_recovery=workspaces, dependencies=dependencies, retired_namespaces=retired, delete=deletions,
        rotation=dict(effective_repository_budget_bytes=repository_limit_bytes,
            dependency_headroom_target_bytes=headroom, resident_target_bytes=resident_target,
            observed_repository_bytes=observed, projected_repository_bytes=projected,
            projected_available_repository_bytes=max(0, repository_limit_bytes - projected),
            projected_headroom_deficit_bytes=max(0, projected + headroom - repository_limit_bytes)))



def family(key):
    workspace = policy.family_for_key(key)
    if workspace:
        return workspace + '-workspace'
    match = RUST_KEY.fullmatch(key)
    if match:
        return 'rust/' + match['family']
    if key.startswith('v0-rust-'):
        match = re.match(r'v0-rust-(.+?-trusted-v[23])(?:-|$)', key)
        if match:
            return 'rust/' + match[1]
    for prefix in ('trunk-archive-', 'wasm-bindgen-', 'node-cache-', 'setup-node-', 'cache-trust-sentinel-',
                   'oasis7-cache-permission-sentinel-'):
        if key.startswith(prefix):
            return prefix.rstrip('-')
    return 'unknown'


def inventory_summary(entries):
    groups = defaultdict(lambda: {'count': 0, 'bytes': 0})
    for entry in entries:
        group = groups[(family(entry['key']), entry['ref'])]
        group['count'] += 1
        group['bytes'] += entry['size_in_bytes']
    return {'total_count': len(entries), 'total_bytes': sum(entry['size_in_bytes'] for entry in entries),
            'groups': [{'family': name, 'ref': ref, **values}
                       for (name, ref), values in sorted(groups.items(), key=lambda item: (-item[1]['bytes'], item[0]))]}


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        # Never forward the job token to a redirected or arbitrary pagination URL.
        return None


class GitHubApi:
    def __init__(self, repository, token):
        if (not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9-]*/[A-Za-z0-9_.-]+', repository or '')
                or repository.split('/')[-1] in ('.', '..')):
            raise MaintenanceError('invalid GitHub repository')
        self.repository = repository
        self.token = token
        self.requests = 0
        self.active_run_inventory_retries = 0
        self.ancestry_requests = 0
        self.ancestry_deferred = False
        self.ancestry = {}
        self.deadline = time.monotonic() + MAX_SECONDS
        self.opener = build_opener(NoRedirect())

    def request(self, method, path):
        comparison = (method == 'GET' and re.fullmatch(
            r'/compare/[0-9a-f]{40}\.\.\.[0-9a-f]{40}\?per_page=1', path or '', re.ASCII))
        action = (method in ('GET', 'DELETE') and isinstance(path, str)
                  and path.startswith('/actions/') and '..' not in path)
        if not (action or comparison):
            raise MaintenanceError('API path is outside the repository Actions boundary')
        remaining = self.deadline - time.monotonic()
        if self.requests >= MAX_REQUESTS or remaining <= 0:
            raise MaintenanceError('cache API request/time budget exhausted')
        self.requests += 1
        headers = {'Accept': 'application/vnd.github+json', 'X-GitHub-Api-Version': '2022-11-28',
                   'User-Agent': 'oasis7-ci-cache-maintenance'}
        if self.token:
            headers['Authorization'] = 'Bearer ' + self.token
        request = Request(f'https://api.github.com/repos/{self.repository}{path}', headers=headers, method=method)
        try:
            with self.opener.open(request, timeout=min(15, remaining)) as response:
                if method == 'DELETE':
                    if response.status != 204:
                        raise MaintenanceError(f'cache DELETE returned HTTP {response.status}')
                    return True
                body = response.read(5 * 1024 * 1024 + 1)
                if len(body) > 5 * 1024 * 1024:
                    raise MaintenanceError('cache API response exceeds size budget')
                return json.loads(body)
        except HTTPError as error:
            if method == 'DELETE' and error.code == 404:
                return False
            raise MaintenanceApiError(method, error) from None
        except (URLError, OSError, ValueError) as error:
            if isinstance(error, MaintenanceError):
                raise
            # Do not print response bodies, bearer tokens, URLs or arbitrary exception messages.
            raise MaintenanceError(f'cache API {method} failed ({type(error).__name__})') from None

    def is_ancestor(self, older, newer):
        if any(not isinstance(sha, str) or not re.fullmatch(r'[0-9a-f]{40}', sha) for sha in (older, newer)):
            raise MaintenanceError('source ancestry requires immutable complete SHA identities')
        if older == newer:
            return True
        pair = older, newer
        if pair in self.ancestry:
            return self.ancestry[pair]
        if self.ancestry_requests >= MAX_ANCESTRY_REQUESTS:
            self.ancestry_deferred = True
            return None
        self.ancestry_requests += 1
        try:
            result = self.request('GET', f'/compare/{newer}...{older}?per_page=1')
        except MaintenanceApiError as error:
            if error.status != 404:
                raise
            # Missing historical objects cannot prove a successor; ordinary
            # within-group retention does not require this optional proof.
            self.ancestry[pair] = None
            self.ancestry_deferred = True
            return None
        if (not isinstance(result, dict) or not isinstance(result.get('base_commit'), dict)
                or result['base_commit'].get('sha') != newer
                or not isinstance(result.get('merge_base_commit'), dict)
                or not isinstance(result['merge_base_commit'].get('sha'), str)
                or not re.fullmatch(r'[0-9a-f]{40}', result['merge_base_commit']['sha'])
                or any(type(result.get(field)) is not int or result[field] < 0
                       for field in ('ahead_by', 'behind_by', 'total_commits'))
                or not isinstance(result.get('commits'), list) or len(result['commits']) > 1):
            raise MaintenanceError('source ancestry response has invalid immutable identity or counts')
        status = result.get('status')
        if status == 'behind':
            if (result['merge_base_commit']['sha'] != older or result['ahead_by'] != 0
                    or result['behind_by'] <= 0 or result['total_commits'] != 0 or result['commits']):
                raise MaintenanceError('source ancestry response does not prove the exact ancestor')
            value = True
        elif status in ('ahead', 'diverged') and result['ahead_by'] > 0:
            if (result['total_commits'] != result['ahead_by'] or len(result['commits']) != 1
                    or (status == 'ahead' and (result['behind_by'] != 0
                                              or result['merge_base_commit']['sha'] != newer))
                    or (status == 'diverged' and result['behind_by'] <= 0)):
                raise MaintenanceError('source ancestry response has inconsistent divergence')
            value = False
        else:
            raise MaintenanceError('source ancestry response has no valid relationship')
        self.ancestry[pair] = value
        return value

    def list_caches(self):
        entries, total, page = [], None, 1
        while True:
            result = self.request('GET', f'/actions/caches?per_page={PAGE_SIZE}&page={page}&sort=created_at&direction=asc')
            if not isinstance(result, dict) or type(result.get('total_count')) is not int or result['total_count'] < 0:
                raise MaintenanceError('cache pagination has invalid total_count')
            if total is None:
                total = result['total_count']
            if result['total_count'] != total:
                raise MaintenanceError('cache inventory changed during pagination')
            batch = result.get('actions_caches')
            validate_inventory(batch)
            if len(batch) > PAGE_SIZE or len(entries) + len(batch) > total:
                raise MaintenanceError('cache pagination exceeds expected count')
            entries.extend(batch)
            validate_inventory(entries)
            if len(entries) == total:
                return entries
            if len(batch) != PAGE_SIZE:
                raise MaintenanceError('cache pagination ended before total_count')
            page += 1

    def list_active_runs(self):
        for attempt in range(ACTIVE_RUN_INVENTORY_ATTEMPTS):
            try:
                return self._list_active_runs_once()
            except ActiveRunInventoryChanged:
                if attempt + 1 == ACTIVE_RUN_INVENTORY_ATTEMPTS:
                    raise
                self.active_run_inventory_retries += 1
                # Restart every status and page on this same bounded client.
                # Partial runs from the failed attempt cannot authorize deletion.

    def _list_active_runs_once(self):
        # The API has no "all unfinished" filter. Query every nonterminal
        # status, conservatively covering the entire repository. A run moving
        # between statuses may appear twice; its immutable identity must agree.
        active = {}
        for status in ACTIVE_RUN_STATUSES:
            entries, total, page = [], None, 1
            while True:
                result = self.request('GET', f'/actions/runs?per_page={PAGE_SIZE}&page={page}&status={status}')
                if (not isinstance(result, dict) or type(result.get('total_count')) is not int
                        or not 0 <= result['total_count'] <= MAX_FILTERED_RUNS):
                    raise MaintenanceError('active run pagination has invalid or truncated total_count')
                if total is None:
                    total = result['total_count']
                batch = validate_active_runs(result.get('workflow_runs'), self.repository, allow_completed=True)
                if len(batch) > PAGE_SIZE:
                    raise MaintenanceError('active run pagination exceeds expected count')
                if result['total_count'] != total:
                    raise ActiveRunInventoryChanged('active run inventory changed during pagination')
                if len(entries) + len(batch) > total:
                    raise ActiveRunInventoryChanged('active run pagination exceeds expected count')
                entries.extend(batch)
                validate_active_runs(entries, self.repository, allow_completed=True)
                if len(entries) == total:
                    break
                if len(batch) != PAGE_SIZE:
                    raise ActiveRunInventoryChanged('active run pagination ended before total_count')
                page += 1
            for entry in entries:
                prior = active.get(entry['id'])
                if prior and run_identity(prior) != run_identity(entry):
                    raise MaintenanceError('active run identity changed between status queries')
                if entry['status'] != 'completed':
                    active.setdefault(entry['id'], entry)
        return sorted(active.values(), key=lambda run: run['id'])

    def delete_cache(self, cache_id):
        if type(cache_id) is not int or cache_id <= 0:
            raise MaintenanceError('invalid cache ID')
        return self.request('DELETE', f'/actions/caches/{cache_id}')


def trusted_run_start(api, environment, current_key=None, *, bootstrap=False, recover=False):
    if bootstrap and recover:
        raise MaintenanceError('bootstrap and recovery modes cannot be combined')
    if bootstrap and current_key:
        raise MaintenanceError('bootstrap cannot be combined with current workspace keys')
    keys = [] if bootstrap or (recover and not current_key) else current_keys(current_key)
    sha = environment.get('GITHUB_SHA', '')
    event = environment.get('GITHUB_EVENT_NAME', '')
    run_id = environment.get('GITHUB_RUN_ID', '')
    workflow_ref = environment.get('GITHUB_WORKFLOW_REF')
    if workflow_ref == f'{api.repository}/{WORKFLOW_PATH}@{MAIN_REF}':
        workflow, allowed_families, events = WORKFLOW_PATH, {'formal', 'ordinary'}, ('push', 'schedule', 'workflow_dispatch')
    elif workflow_ref == f'{api.repository}/{ECS_WORKFLOW_PATH}@{MAIN_REF}':
        workflow, allowed_families, events = ECS_WORKFLOW_PATH, {'ecs'}, ('workflow_dispatch',)
    elif workflow_ref == f'{api.repository}/{RELEASE_WORKFLOW_PATH}@{MAIN_REF}':
        workflow, allowed_families, events = RELEASE_WORKFLOW_PATH, set(), ('workflow_dispatch',)
        if not recover or keys or environment.get('CACHE_MAINTENANCE_SOURCE_SHA') != sha:
            raise MaintenanceError('Release recovery requires the matching current source plan and no workspace keys')
    else:
        raise MaintenanceError('retention requires a trusted main Rust, ECS or Release workflow')
    if bootstrap and workflow != WORKFLOW_PATH:
        raise MaintenanceError('bootstrap requires the trusted main Rust workflow')
    if (environment.get('GITHUB_ACTIONS') != 'true' or environment.get('GITHUB_REF') != MAIN_REF
            or environment.get('GITHUB_REPOSITORY') != api.repository
            or event not in events
            or not re.fullmatch(r'[1-9][0-9]*', run_id)
            or not re.fullmatch(r'[0-9a-f]{40}', sha)
            or environment.get('GITHUB_WORKFLOW_SHA') != sha
            or any(policy.family_for_key(key) not in allowed_families or not key.endswith('-' + sha) for key in keys)):
        raise MaintenanceError('retention requires the current trusted main workflow and exact source keys')
    if event == 'workflow_dispatch' and workflow == WORKFLOW_PATH:
        try:
            payload = json.loads(Path(environment.get('GITHUB_EVENT_PATH', '')).read_text())
            full = payload.get('inputs', {}).get('run_mode') == 'full'
        except (OSError, ValueError, AttributeError):
            full = False
        if not full:
            raise MaintenanceError('manual retention requires run_mode=full in the workflow event')
    run = api.request('GET', f'/actions/runs/{run_id}')
    if (not isinstance(run, dict) or run.get('id') != int(run_id) or run.get('head_sha') != sha
            or run.get('head_branch') != 'main' or run.get('event') != event
            or run.get('path') != workflow
            or not isinstance(run.get('repository'), dict)
            or run['repository'].get('full_name') != api.repository):
        raise MaintenanceError('REST workflow run identity does not match the trusted main execution')
    timestamp(run.get('created_at'))
    return run['created_at']


def empty_report(apply, *, bootstrap=False, recover=False):
    return {'mode': 'apply' if apply else 'dry-run', 'status': 'skipped', 'bootstrap': bootstrap, 'recover': recover,
            'policy': {'formal_max_bytes': FORMAL_MAX_BYTES, 'keep_generations': FORMAL_KEEP_GENERATIONS,
                       'workspace_families': {name: {field: values.get(field) for field in
                           ('max_upload_bytes', 'max_bytes', 'rotation_bytes', 'migration_bytes',
                            'staging_bytes', 'keep_generations')}
                           for name, values in policy.FAMILIES.items()},
                       'retired_namespaces': RETIRED_NAMESPACES,
                       'dependency_namespaces': sorted(LEGACY_DEPENDENCY_NAMESPACES if bootstrap else DEPENDENCY_NAMESPACES),
                       'lineage_workspace_schemas': sorted(LINEAGE_WORKSPACE_SCHEMAS),
                       'max_ancestry_requests': MAX_ANCESTRY_REQUESTS,
                       'bootstrap_retired_schemas': sorted(RETIRED_WORKSPACE_SCHEMAS),
                       'dependency_rotation_headroom_bytes': policy.DEPENDENCY_ROTATION_HEADROOM_BYTES},
            'before': None, 'after': None, 'plan': None, 'delete_candidates': [],
            'stable_replacement_ids': {}, 'stable_recovery_ids': [], 'stable_dependency_recovery_ids': [],
            'stable_workspace_recovery_ids': [], 'capacity_before': None, 'capacity_revalidation': None,
            'rotation_after': None, 'active_run_inventory_retries': 0,
            'ancestry_requests': 0, 'ancestry_deferred': False,
            'deleted_ids': [], 'deleted_dependency_ids': [], 'deleted_workspace_ids': [],
            'deleted_namespace_ids': [], 'already_absent_ids': [], 'warnings': []}


def maintain(api, current_key=None, run_started_at=None, *, apply=False, bootstrap=False, recover=False,
             repository_budget_bytes=None, verified_head_sha=None):
    report = empty_report(apply, bootstrap=bootstrap, recover=recover)
    try:
        if bootstrap and recover:
            raise MaintenanceError('bootstrap and recovery modes cannot be combined')
        if bootstrap and current_key:
            raise MaintenanceError('bootstrap cannot be combined with current workspace keys')
        if (apply or recover) and (not (current_key or bootstrap or recover) or not run_started_at):
            raise MaintenanceError('apply requires an anchor or recovery mode and a verified workflow run boundary')
        def read_inventory(field):
            if not recover:
                return validate_inventory(api.list_caches())
            report[field] = {}
            boundary = SimpleNamespace(validate_inventory=validate_inventory, MaintenanceApiError=MaintenanceApiError)
            return policy.capacity_context(boundary, api, repository_budget_bytes, report[field])

        def build_plan(entries, capacity):
            active_runs = api.list_active_runs()
            if recover:
                return plan_recovery(entries, run_started_at, current_key=current_key, active_runs=active_runs,
                                     repository_limit_bytes=capacity['effective_repository_budget_bytes'],
                                     verified_head_sha=verified_head_sha,
                                     is_ancestor=api.is_ancestor if verified_head_sha else None)
            if bootstrap:
                return plan_bootstrap(entries, run_started_at, active_runs=active_runs)
            return plan_retention(entries, current_key, run_started_at, active_runs=active_runs)
        first = read_inventory('capacity_before')
        report['before'] = inventory_summary(first)
        if current_key or bootstrap or recover:
            report['plan'] = build_plan(first, report['capacity_before'])
        if not apply:
            report['status'] = 'dry-run'
            return report
        initial = {entry['id']: entry for entry in report['plan']['delete']}
        second = read_inventory('capacity_revalidation')
        latest_plan = build_plan(second, report['capacity_revalidation'])
        for key in report['plan']['current_keys']:
            first_current = next(entry for entry in first if managed(entry) and entry['key'] == key)
            second_current = next(entry for entry in second if managed(entry) and entry['key'] == key)
            if immutable_identity(first_current) != immutable_identity(second_current):
                raise MaintenanceError('confirmed current cache identity changed between inventories')
        first_replacements = report['plan']['retired_namespaces']['replacements']
        second_replacements = latest_plan['retired_namespaces']['replacements']
        stable = {}
        for namespace, replacements in first_replacements.items():
            identities = {immutable_identity(entry) for entry in replacements}
            stable[namespace] = sorted(entry['id'] for entry in second_replacements.get(namespace, [])
                                       if immutable_identity(entry) in identities)
        report['stable_replacement_ids'] = stable
        for section, result_field, label in (
                ('retired_schemas', 'stable_recovery_ids', 'retired workspace'),
                ('dependencies', 'stable_dependency_recovery_ids', 'dependency'),
                ('workspace_recovery', 'stable_workspace_recovery_ids', 'workspace')):
            if section not in report['plan']:
                continue
            first_group, second_group = report['plan'][section], latest_plan[section]
            identities = {immutable_identity(entry) for entry in first_group['recoveries']}
            report[result_field] = sorted(entry['id'] for entry in second_group['recoveries']
                                          if immutable_identity(entry) in identities)
            if set(first_group['recovery_ids']) - set(report[result_field]):
                report['warnings'].append(label + ' recovery changed between inventories; affected groups deferred')
        stable_recoveries = (set(report['stable_recovery_ids']) | set(report['stable_dependency_recovery_ids'])
                             | set(report['stable_workspace_recovery_ids']))
        # Revalidation may only shrink the original mutation set. A recent read,
        # new cache or archive identity change cannot expand this invocation.
        candidates = [entry for entry in latest_plan['delete'] if entry['id'] in initial
                      and immutable_identity(entry) == immutable_identity(initial[entry['id']])
                      and entry.get('recovery_id') == initial[entry['id']].get('recovery_id')
                      and (not entry.get('replacement_family') or stable.get(entry['replacement_family']))
                      and (not entry.get('recovery_id') or entry['recovery_id'] in stable_recoveries)]
        report['plan'] = latest_plan
        report['delete_candidates'] = candidates
        report['status'] = 'completed'
        for entry in candidates:
            try:
                deleted = api.delete_cache(entry['id'])
                report['deleted_ids' if deleted else 'already_absent_ids'].append(entry['id'])
                if deleted:
                    category = ('workspace' if managed(entry) else
                                'dependency' if rust_namespace(entry) in DEPENDENCY_NAMESPACES else 'namespace')
                    report['deleted_' + category + '_ids'].append(entry['id'])
            except MaintenanceError as error:
                report['status'] = 'partial'
                report['warnings'].append(str(error))
                break
        try:
            report['after'] = inventory_summary(api.list_caches())
            if recover:
                limit = report['capacity_revalidation']['effective_repository_budget_bytes']
                observed = report['after']['total_bytes']
                report['rotation_after'] = dict(effective_repository_budget_bytes=limit,
                    observed_repository_bytes=observed, available_repository_bytes=max(0, limit - observed),
                    headroom_deficit_bytes=max(0, observed + policy.DEPENDENCY_ROTATION_HEADROOM_BYTES - limit))
                if report['rotation_after']['headroom_deficit_bytes']:
                    report['warnings'].append('dependency rotation headroom remains below target; protected or other caches retained')
        except MaintenanceError as error:
            report['warnings'].append('remaining storage unknown: ' + str(error))
        if latest_plan['protected_over_budget_bytes']:
            report['warnings'].append('protected concurrent/current/recovery caches exceed a workspace family budget')
    except ValueError as error:
        report['warnings'].append(str(error))
    finally:
        retries = getattr(api, 'active_run_inventory_retries', 0)
        report['active_run_inventory_retries'] = retries if type(retries) is int else 0
        ancestry_requests = getattr(api, 'ancestry_requests', 0)
        report['ancestry_requests'] = ancestry_requests if type(ancestry_requests) is int else 0
        report['ancestry_deferred'] = getattr(api, 'ancestry_deferred', False) is True
        if report['ancestry_deferred']:
            report['warnings'].append('some workspace source comparisons deferred; unproven compatibility groups retained')
    return report


def render_summary(report):
    def cell(value):
        return html.escape(str(value)).replace('|', '&#124;').replace('\n', ' ')

    lines = ['## CI cache storage', '', f"Mode: **{report['mode']}**; status: **{report['status']}**.", '']
    if report.get('recover'):
        lines += ['Recovery uses existing complete main workspace anchors even when this run saved no new cache.',
                  'Current-format groups target two generations; pressure may retire extra inactive copies or verified source ancestors while retaining a complete permanent successor.',
                  'Current exact keys, ambiguous groups, incomplete uploads and observed activity remain protected.', '']
    elif report.get('bootstrap'):
        lines += ['Bootstrap prunes enumerated retired workspace formats and redundant compatible dependency snapshots.',
                  'Every full compatibility prefix and cache version retains its newest complete recovery snapshot.',
                  'Current workspace schemas and other dependency environments are preserved; ambiguous, concurrent and actively accessed entries are protected.', '']
    else:
        lines += ['Each anchored main workspace family targets two current-compatible generations.',
                  'Current, previous compatible, concurrent and actively accessed caches take priority over these limits.',
                  'Retired Rust namespaces require a complete main replacement with the same identity in both inventories.', '']
    lines += ['Allowlisted dependency namespaces retain one complete recovery per existing restore prefix and cache version.',
              'A later dependency snapshot is a fallback candidate, not a promise to contain all older dependencies.', '']
    for label in ('before', 'after'):
        inventory = report[label]
        if inventory is None:
            lines += [f'{label.title()} inventory: unknown / not queried.', '']
            continue
        lines += [f"### {label.title()} inventory", '',
                  f"{inventory['total_count']:,} caches; **{inventory['total_bytes']:,} compressed bytes**.", '',
                  '| Family | Ref | Entries | Compressed bytes |', '| --- | --- | ---: | ---: |']
        for group in inventory['groups']:
            lines.append(f"| {cell(group['family'])} | {cell(group['ref'])} | {group['count']} | {group['bytes']:,} |")
        lines.append('')
    if report['plan']:
        plan = report['plan']
        retired = plan['retired_namespaces']
        if report.get('recover'):
            rotation = plan['rotation']
            workspaces = plan['workspace_recovery']
            lines += [f"Dependency rotation headroom target: {rotation['dependency_headroom_target_bytes']:,} bytes; "
                      f"repository resident target: {rotation['resident_target_bytes']:,} bytes.", '',
                      f"Planned remaining repository bytes: {rotation['projected_repository_bytes']:,}; "
                      f"planned headroom deficit: {rotation['projected_headroom_deficit_bytes']:,} bytes.", '',
                      f"Explicit current keys: {cell(plan['current_keys'])}; "
                      f"workspace recovery IDs: {cell(workspaces['recovery_ids'])}; "
                      f"stable in both inventories: {cell(report['stable_workspace_recovery_ids'])}.", '',
                      f"Ambiguous or incomplete workspace groups deferred: {len(workspaces['blocked_groups'])}.", '',
                      f"Verified source-lineage retirements: {len(workspaces['source_lineage'])}; "
                      f"immutable-source requests: {report['ancestry_requests']}; "
                      f"unproven comparisons deferred: {report['ancestry_deferred']}.", '',
                      '| Workspace family | Resident target | Planned remaining bytes | Protected excess |',
                      '| --- | ---: | ---: | ---: |']
            for name, values in sorted(plan['families'].items()):
                lines.append(f"| {cell(name)} | {values['max_bytes']:,} | {values['projected_bytes']:,} | "
                             f"{values['protected_over_budget_bytes']:,} |")
            lines.append('')
            if report['rotation_after']:
                observed = report['rotation_after']
                lines += [f"Observed remaining headroom: {observed['available_repository_bytes']:,} bytes; "
                          f"remaining target deficit: {observed['headroom_deficit_bytes']:,} bytes.", '']
        elif report.get('bootstrap'):
            schemas = plan['retired_schemas']
            lines += [f"Retired workspace entries: {schemas['managed_count']:,}; "
                      f"compressed bytes: {schemas['managed_bytes']:,}; protected bytes: {schemas['protected_bytes']:,}.", '',
                      f"Recovery snapshot IDs: {cell(schemas['recovery_ids'])}; "
                      f"stable in both inventories: {cell(report['stable_recovery_ids'])}.", '',
                      f"Ambiguous or incomplete retired groups deferred: {len(schemas['blocked_groups'])}.", '']
        else:
            lines += [f"Explicit current keys: {cell(plan['current_keys'])}.", '',
                      f"Unanchored families (no workspace deletions): {cell(plan['unanchored_families'])}.", '',
                      '| Anchored family | Resident byte target | Managed bytes | Protected bytes | Protected excess |',
                      '| --- | ---: | ---: | ---: | ---: |']
            for name, values in sorted(plan['families'].items()):
                lines.append(f"| {cell(name)} | {values['max_bytes']:,} | {values['managed_bytes']:,} | "
                             f"{values['protected_bytes']:,} | {values['protected_over_budget_bytes']:,} |")
            lines += ['', f"Anchored workspace bytes at planning: {plan['managed_bytes']:,}; protected: {plan['protected_bytes']:,}; "
                      f"protected excess: {plan['protected_over_budget_bytes']:,}.", '']
        lines += [f"Activity protection starts at {cell(plan['protection_started_at'])}; "
                  f"unfinished repository runs: {cell(plan['active_run_ids'])}.", '',
                  f"Dependency snapshots: {plan['dependencies']['managed_count']:,} entries, "
                  f"{plan['dependencies']['managed_bytes']:,} compressed bytes; "
                  f"protected bytes: {plan['dependencies']['protected_bytes']:,}; "
                  f"planned remaining bytes: {plan['dependencies']['projected_bytes']:,}.", '',
                  f"Dependency recovery IDs: {cell(plan['dependencies']['recovery_ids'])}; "
                  f"stable in both inventories: {cell(report['stable_dependency_recovery_ids'])}; "
                  f"ambiguous or incomplete groups deferred: {len(plan['dependencies']['blocked_groups'])}.", '',
                  f"Retired namespaces: {retired['managed_count']:,} entries, {retired['managed_bytes']:,} compressed bytes; "
                  f"protected IDs: {cell(retired['protected_ids'])}; "
                  f"missing complete replacement families: {cell(retired['missing_replacements'])}.", '',
                  f"Stable replacement IDs: {cell(report['stable_replacement_ids'])}.", '',
                  f"Verified DELETE candidates: {len(report['delete_candidates'])}; "
                  f"confirmed deleted IDs: {cell(report['deleted_ids'])}; already absent IDs: {cell(report['already_absent_ids'])}.", '',
                  f"Confirmed deleted IDs by category — dependency: {cell(report['deleted_dependency_ids'])}; "
                  f"workspace: {cell(report['deleted_workspace_ids'])}; retired namespaces: {cell(report['deleted_namespace_ids'])}.", '',
                  'The complete plan and per-ID results are in the step log. Planned bytes are not measured reclamation.', '']
    lines += [f"Active-run complete-inventory retries: {report['active_run_inventory_retries']}.", '']
    for warning in report['warnings']:
        lines += ['Warning: ' + cell(warning), '']
    return '\n'.join(lines)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--apply', action='store_true', help='delete only after trusted main run and recovery/anchor verification')
    parser.add_argument('--bootstrap', action='store_true',
                        help='prune known retired workspace and compatible dependency redundancy without a new saved key')
    parser.add_argument('--recover', action='store_true',
                        help='recover workspace rotation from stable existing anchors even without a new saved key')
    parser.add_argument('--repository-budget-bytes', default=os.environ.get('CI_CACHE_REPOSITORY_BUDGET_BYTES'))
    parser.add_argument('--current-key', action='append',
                        help='exact confirmed workspace key; repeat for distinct families; otherwise only recovery/bootstrap can plan deletion')
    parser.add_argument('--repository', default=os.environ.get('GITHUB_REPOSITORY'))
    parser.add_argument('--summary', default=os.environ.get('GITHUB_STEP_SUMMARY'))
    args = parser.parse_args(argv)
    report = empty_report(args.apply, bootstrap=args.bootstrap, recover=args.recover)
    try:
        if args.bootstrap and args.recover:
            raise MaintenanceError('bootstrap and recovery modes cannot be combined')
        if args.bootstrap and args.current_key:
            raise MaintenanceError('bootstrap cannot be combined with current workspace keys')
        token = os.environ.get('CACHE_MAINTENANCE_TOKEN', '')
        if args.apply and (not token or not (args.current_key or args.bootstrap or args.recover)):
            raise MaintenanceError('apply requires the scoped job token and an exact saved key or recovery mode')
        api = GitHubApi(args.repository, token)
        cutoff = (trusted_run_start(api, os.environ, args.current_key, bootstrap=args.bootstrap, recover=args.recover)
                  if args.current_key or args.bootstrap or args.recover else None)
        report = maintain(api, args.current_key, cutoff, apply=args.apply, bootstrap=args.bootstrap, recover=args.recover,
                          repository_budget_bytes=policy.configured_budget(args.repository_budget_bytes) if args.recover else None,
                          verified_head_sha=os.environ.get('GITHUB_SHA') if args.recover else None)
    except ValueError as error:
        report['warnings'].append(str(error))
    if args.summary:
        try:
            with Path(args.summary).open('a') as output:
                output.write(render_summary(report) + '\n')
        except OSError:
            report['warnings'].append('could not write cache step summary')
    print(json.dumps(report, indent=2, sort_keys=True))
    for warning in report['warnings']:
        print('::warning::' + warning.replace('%', '%25').replace('\r', '%0D').replace('\n', '%0A'))
    return 0  # Cache maintenance cannot change the required build/test outcome.


if __name__ == '__main__':
    raise SystemExit(main())
