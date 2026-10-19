#!/usr/bin/env python3
"""Report cache storage and retire old trusted Formal snapshots after a save.

Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7.
No cache payload is restored here. Remote maintenance is advisory, defaults to
read-only, and can only delete individually identified main Formal entries.
"""
import argparse
from collections import Counter, defaultdict
from datetime import datetime
import html
import json
import os
from pathlib import Path
import re
import time
from urllib.error import HTTPError, URLError
from urllib.request import HTTPRedirectHandler, Request, build_opener

FORMAL_MAX_BYTES = 2 * 1024 ** 3
FORMAL_KEEP_GENERATIONS = 2
MAX_REQUESTS = 60
MAX_SECONDS = 120
PAGE_SIZE = 100
MAIN_REF = 'refs/heads/main'
WORKFLOW_PATH = '.github/workflows/rust.yml'
FORMAL_KEY = re.compile(
    r'formal-workspace-v2-ubuntu24-(?:X64|ARM64)-\d+\.\d+\.\d+-'
    r'nightly-\d{4}-\d{2}-\d{2}-[0-9a-f]{64}-[0-9a-f]{40}\Z', re.ASCII)


class MaintenanceError(ValueError):
    """No further mutations are safe with the available evidence."""


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
    return entry['ref'] == MAIN_REF and FORMAL_KEY.fullmatch(entry['key']) is not None


def compatibility(entry):
    # The archive version encodes paths/compression and is part of restore identity.
    return entry['key'].rsplit('-', 1)[0], entry['version']


def immutable_identity(entry):
    return tuple(entry[field] for field in ('id', 'key', 'ref', 'version', 'created_at', 'size_in_bytes'))


def plan_retention(entries, current_key, run_started_at, *, max_bytes=FORMAL_MAX_BYTES,
                   keep_generations=FORMAL_KEEP_GENERATIONS):
    validate_inventory(entries)
    if not FORMAL_KEY.fullmatch(current_key or ''):
        raise MaintenanceError('current key is outside the managed Formal schema')
    cutoff = timestamp(run_started_at)
    owned = [entry for entry in entries if managed(entry)]
    current_matches = [entry for entry in owned if entry['key'] == current_key]
    if len(current_matches) != 1 or current_matches[0]['size_in_bytes'] == 0:
        raise MaintenanceError('saved current key must identify one complete main cache, including its version')
    current = current_matches[0]
    protected = {entry['id'] for entry in owned
                 if entry['id'] == current['id'] or entry['size_in_bytes'] == 0
                 or timestamp(entry['created_at']) >= cutoff
                 or timestamp(entry['last_accessed_at']) >= cutoff}
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
        if counts[group] >= keep_generations:
            reason = 'generation limit'
        elif retained_bytes + entry['size_in_bytes'] > max_bytes:
            reason = 'Formal family compressed-byte budget'
        else:
            retained_bytes += entry['size_in_bytes']
            counts[group] += 1
            continue
        deletions.append(dict(entry, reason=reason))
    # Oldest first; stop on the first unexpected deletion error.
    deletions.sort(key=lambda entry: (timestamp(entry['created_at']), entry['id']))
    return {'managed_bytes': sum(entry['size_in_bytes'] for entry in owned),
            'managed_count': len(owned), 'max_bytes': max_bytes, 'keep_generations': keep_generations,
            'current_key': current_key, 'run_created_at': run_started_at,
            'protected_ids': sorted(protected), 'protected_bytes': protected_bytes,
            'protected_over_budget_bytes': max(0, protected_bytes - max_bytes),
            'projected_bytes': retained_bytes, 'delete': deletions}


def family(key):
    if FORMAL_KEY.fullmatch(key):
        return 'formal-workspace-v2'
    if key.startswith('v0-rust-'):
        match = re.match(r'v0-rust-(.+?-trusted-v2)(?:-|$)', key)
        if match:
            return 'rust/' + match[1]
    for prefix in ('trunk-archive-', 'wasm-bindgen-', 'node-cache-', 'setup-node-', 'cache-trust-sentinel-'):
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
        self.deadline = time.monotonic() + MAX_SECONDS
        self.opener = build_opener(NoRedirect())

    def request(self, method, path):
        if method not in ('GET', 'DELETE') or not path.startswith('/actions/') or '..' in path:
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
            raise MaintenanceError(f'cache API {method} returned HTTP {error.code}') from None
        except (URLError, OSError, ValueError) as error:
            if isinstance(error, MaintenanceError):
                raise
            # Do not print response bodies, bearer tokens, URLs or arbitrary exception messages.
            raise MaintenanceError(f'cache API {method} failed ({type(error).__name__})') from None

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

    def delete_cache(self, cache_id):
        if type(cache_id) is not int or cache_id <= 0:
            raise MaintenanceError('invalid cache ID')
        return self.request('DELETE', f'/actions/caches/{cache_id}')


def trusted_run_start(api, environment, current_key):
    sha = environment.get('GITHUB_SHA', '')
    event = environment.get('GITHUB_EVENT_NAME', '')
    run_id = environment.get('GITHUB_RUN_ID', '')
    if (environment.get('GITHUB_ACTIONS') != 'true' or environment.get('GITHUB_REF') != MAIN_REF
            or environment.get('GITHUB_REPOSITORY') != api.repository
            or event not in ('push', 'schedule', 'workflow_dispatch')
            or not re.fullmatch(r'[1-9][0-9]*', run_id)
            or not re.fullmatch(r'[0-9a-f]{40}', sha)
            or not FORMAL_KEY.fullmatch(current_key or '') or not current_key.endswith('-' + sha)
            or environment.get('GITHUB_WORKFLOW_REF') != f'{api.repository}/{WORKFLOW_PATH}@{MAIN_REF}'):
        raise MaintenanceError('retention requires the current trusted main Rust workflow and exact source key')
    if event == 'workflow_dispatch':
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
            or run.get('path') != WORKFLOW_PATH
            or not isinstance(run.get('repository'), dict)
            or run['repository'].get('full_name') != api.repository):
        raise MaintenanceError('REST workflow run identity does not match the trusted main execution')
    timestamp(run.get('created_at'))
    return run['created_at']


def empty_report(apply):
    return {'mode': 'apply' if apply else 'dry-run', 'status': 'skipped',
            'policy': {'formal_max_bytes': FORMAL_MAX_BYTES, 'keep_generations': FORMAL_KEEP_GENERATIONS},
            'before': None, 'after': None, 'plan': None, 'delete_candidates': [],
            'deleted_ids': [], 'already_absent_ids': [], 'warnings': []}


def maintain(api, current_key=None, run_started_at=None, *, apply=False):
    report = empty_report(apply)
    try:
        if apply and (not current_key or not run_started_at):
            raise MaintenanceError('apply requires a verified current key and workflow run boundary')
        first = api.list_caches()
        report['before'] = inventory_summary(first)
        if current_key:
            report['plan'] = plan_retention(first, current_key, run_started_at)
        if not apply:
            report['status'] = 'dry-run'
            return report
        initial = {entry['id']: entry for entry in report['plan']['delete']}
        second = api.list_caches()
        latest_plan = plan_retention(second, current_key, run_started_at)
        first_current = next(entry for entry in first if managed(entry) and entry['key'] == current_key)
        second_current = next(entry for entry in second if managed(entry) and entry['key'] == current_key)
        if immutable_identity(first_current) != immutable_identity(second_current):
            raise MaintenanceError('saved current cache identity changed between inventories')
        # Revalidation may only shrink the original mutation set. A recent read,
        # new cache or archive identity change cannot expand this invocation.
        candidates = [entry for entry in latest_plan['delete'] if entry['id'] in initial
                      and immutable_identity(entry) == immutable_identity(initial[entry['id']])]
        report['plan'] = latest_plan
        report['delete_candidates'] = candidates
        report['status'] = 'completed'
        for entry in candidates:
            try:
                deleted = api.delete_cache(entry['id'])
                report['deleted_ids' if deleted else 'already_absent_ids'].append(entry['id'])
            except MaintenanceError as error:
                report['status'] = 'partial'
                report['warnings'].append(str(error))
                break
        try:
            report['after'] = inventory_summary(api.list_caches())
        except MaintenanceError as error:
            report['warnings'].append('remaining storage unknown: ' + str(error))
        if latest_plan['protected_over_budget_bytes']:
            report['warnings'].append('protected concurrent/current/recovery caches exceed the Formal family budget')
    except MaintenanceError as error:
        report['warnings'].append(str(error))
    return report


def render_summary(report):
    def cell(value):
        return html.escape(str(value)).replace('|', '&#124;').replace('\n', ' ')

    lines = ['## CI cache storage', '', f"Mode: **{report['mode']}**; status: **{report['status']}**.", '',
             f'Formal main target: {FORMAL_MAX_BYTES:,} compressed bytes; {FORMAL_KEEP_GENERATIONS} generations per compatibility.',
             'Current, previous compatible, concurrent and recently accessed caches take priority over these limits.', '']
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
        lines += [f"Formal managed bytes at planning: {plan['managed_bytes']:,}; protected: {plan['protected_bytes']:,}; "
                  f"protected excess: {plan['protected_over_budget_bytes']:,}.", '',
                  f"Verified DELETE candidates: {len(report['delete_candidates'])}; "
                  f"confirmed deleted IDs: {cell(report['deleted_ids'])}; already absent IDs: {cell(report['already_absent_ids'])}.", '',
                  'The complete plan and per-ID results are in the step log. Planned bytes are not measured reclamation.', '']
    for warning in report['warnings']:
        lines += ['Warning: ' + cell(warning), '']
    return '\n'.join(lines)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--apply', action='store_true', help='delete only after trusted main run and saved-key verification')
    parser.add_argument('--current-key', help='exact saved Formal key; omit for inventory-only dry-run')
    parser.add_argument('--repository', default=os.environ.get('GITHUB_REPOSITORY'))
    parser.add_argument('--summary', default=os.environ.get('GITHUB_STEP_SUMMARY'))
    args = parser.parse_args(argv)
    report = empty_report(args.apply)
    try:
        token = os.environ.get('CACHE_MAINTENANCE_TOKEN', '')
        if args.apply and (not token or not args.current_key):
            raise MaintenanceError('apply requires the scoped job token and exact saved key')
        api = GitHubApi(args.repository, token)
        cutoff = trusted_run_start(api, os.environ, args.current_key) if args.current_key else None
        report = maintain(api, args.current_key, cutoff, apply=args.apply)
    except MaintenanceError as error:
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
