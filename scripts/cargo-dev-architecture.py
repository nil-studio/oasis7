#!/usr/bin/env python3
"""Inspect declared boundaries and root-specific Cargo graphs; never edit locks.

Metadata supplies identities and declarations only. Cargo tree selects each
root's graph; cargo check cross-checks compilation units including host tools.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tomllib

VERSION = 1
KINDS = {None: 'normal', 'build': 'build', 'dev': 'dev'}


class InspectionError(Exception):
    pass


def run(root, command):
    env = dict(os.environ, CARGO_TERM_COLOR='never')
    env.pop('RUSTC_WRAPPER', None)
    result = subprocess.run(command, cwd=root, env=env, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if result.returncode:
        raise InspectionError(f'{command!r}: {result.stderr[-12000:]}')
    return result.stdout


def matches(condition, package):
    return all(package.get(k) == v for k, v in condition.items())


def validate(config):
    if set(config) != {'revision', 'rules'} or config['revision'] != VERSION or not config['rules']:
        raise InspectionError('invalid rule document')
    seen = set()
    for rule in config['rules']:
        required = {'key', 'upstream', 'roots', 'forbidden', 'kinds',
                    'declaration_kinds', 'profiles', 'required_profiles', 'allows', 'exceptions'}
        if set(rule) - (required | {'empty_default', 'forbidden_feature_tokens', 'test_edges'}) or not required <= rule.keys():
            raise InspectionError('invalid rule fields')
        if rule['key'] in seen or not rule['roots'] or not rule['profiles']:
            raise InspectionError('duplicate key or missing roots/profiles')
        seen.add(rule['key'])
        if not rule['upstream'].count('#') == 1:
            raise InspectionError('upstream must include path#anchor')
        for identity in rule['roots'] + rule['forbidden']:
            if not isinstance(identity, dict) or 'name' not in identity or set(identity) - {'name', 'version', 'source'}:
                raise InspectionError('invalid package identity')
        for kinds in (rule['kinds'], rule['declaration_kinds']):
            if not kinds or not set(kinds) <= {'normal', 'build', 'dev'}:
                raise InspectionError('invalid edge kinds')
        if set(rule['kinds']) != {'normal', 'build'}:
            raise InspectionError('production graph must contain exactly normal/build kinds')
        if (not isinstance(rule['required_profiles'], list) or not rule['required_profiles'] or
                len(set(rule['required_profiles'])) != len(rule['required_profiles'])):
            raise InspectionError('invalid required profile registry')
        # No blanket or implicit exceptions. Stable allowed relationships are
        # documentation, not bypasses of forbidden closure traversal.
        if rule['exceptions']:
            raise InspectionError('migration exceptions are not supported in revision 1')
        for allow in rule.get('test_edges', []):
            if set(allow) != {'root', 'dependency', 'kind', 'target', 'profiles', 'cfg', 'default_features', 'features', 'purpose', 'upstream'}:
                raise InspectionError('invalid test edge allowance')
            if (not allow['purpose'] or '#' not in allow['upstream'] or
                    type(allow['default_features']) is not bool or not isinstance(allow['features'], list)):
                raise InspectionError('imprecise test edge allowance')
            if (allow['kind'] != 'dev' or not allow['target'] or allow['target'] == 'all' or
                    not allow['profiles'] or not set(allow['profiles']) <= {p['id'] for p in rule['profiles']}):
                raise InspectionError('test edge allowance requires exact dev kind/target/profiles')
            for identity in (allow['root'], allow['dependency']):
                if not {'name', 'source'} <= identity.keys() or set(identity) - {'name', 'source', 'version'}:
                    raise InspectionError('test allowance requires precise package source identity')
        ids = set()
        for profile in rule['profiles']:
            if set(profile) != {'id', 'target', 'default_features', 'features', 'mode'}:
                raise InspectionError('invalid profile fields')
            if profile['id'] in ids or profile['target'] == 'all' or profile['mode'] not in ('production', 'test'):
                raise InspectionError('invalid or duplicate profile')
            if type(profile['default_features']) is not bool or not isinstance(profile['features'], list):
                raise InspectionError('invalid feature selection')
            ids.add(profile['id'])


def location(package, dependency):
    """Cargo has already normalized workspace inheritance and package= aliases."""
    kind = KINDS[dependency['kind']]
    table = {'normal': 'dependencies', 'build': 'build-dependencies', 'dev': 'dev-dependencies'}[kind]
    if dependency['target']:
        table = f'target.{dependency["target"]}.{table}'
    alias = dependency.get('rename') or dependency['name']
    return {'manifest': package['manifest_path'], 'table': table, 'alias': alias,
            'kind': kind, 'cfg': dependency['target'], 'optional': dependency['optional'],
            'default_features': dependency['uses_default_features'], 'features': dependency['features']}


def declared_identity(dep, packages):
    # Resolve version requirements through Cargo's actual resolve nodes when
    # available. Inactive optional/platform edges still have normalized source,
    # package name and requirement, and must not disappear from this view.
    candidates = [p for p in packages if p['name'] == dep['name']]
    if dep.get('path'):
        candidates = [p for p in candidates if Path(p['manifest_path']).parent == Path(dep['path'])]
    elif dep['source']:
        candidates = [p for p in candidates if p['source'] and p['source'].split('#')[0] == dep['source'].split('#')[0]]
    return candidates


def resolved_declaration(parent, dep, packages):
    alias = (dep.get('rename') or dep['name']).replace('-', '_')
    ids = {edge['pkg'] for edge in parent['_resolved_deps']
           if edge['name'].replace('-', '_') == alias
           and any(k['kind'] == dep['kind'] and k['target'] == dep['target'] for k in edge['dep_kinds'])}
    return [p for p in declared_identity(dep, packages) if p['id'] in ids]


def tree_nodes(output, packages):
    """Depth output preserves a witness path even when Cargo deduplicates leaves."""
    stack = []
    nodes = []
    for line in output.splitlines():
        match = re.fullmatch(r'(\d+)([^ ]+) v([^ |]+)(.*?)\|(.*)', line)
        if not match:
            raise InspectionError(f'unrecognized cargo tree output: {line!r}')
        depth, name, version, suffix, features = match.groups()
        candidates = [p for p in packages if p['name'] == name and p['version'] == version]
        if ' (' in suffix:
            path = suffix.split(' (', 1)[1].split(')', 1)[0]
            if path.startswith('/') or re.match(r'^[A-Za-z]:', path):
                candidates = [p for p in candidates if str(Path(p['manifest_path']).parent) == path]
            elif path.startswith(('http', 'git')):
                candidates = [p for p in candidates if p['source'] and path.split('#')[0] in p['source']]
        if len(candidates) != 1:
            raise InspectionError(f'ambiguous or absent Cargo identity: {line}')
        depth = int(depth)
        if depth > len(stack):
            raise InspectionError('invalid Cargo depth')
        package = candidates[0]
        stack = stack[:depth] + [package]
        nodes.append((package, list(stack), features.replace(' (*)', '').split(',')))
    if not nodes:
        raise InspectionError('empty Cargo graph')
    return nodes


def expanded_paths(nodes, skip_first):
    """Revisit Cargo's (*) nodes via their expanded adjacency, not by display path.

    Features are part of the node key so separate host/target feature units are
    not collapsed just because they share a package identity.
    """
    stack, graph, identities = [], {}, {}
    for package, path, features in nodes:
        depth = len(path) - 1
        key = (package['id'], tuple(sorted(features)))
        identities[key] = package
        if depth:
            graph.setdefault(stack[depth - 1], set()).add(key)
        stack = stack[:depth] + [key]
    start = (nodes[0][0]['id'], tuple(sorted(nodes[0][2])))
    queue = [(start, [identities[start]])]
    seen = {start}
    while queue:
        node, path = queue.pop(0)
        yield identities[node], path, list(node[1])
        for child in sorted(graph.get(node, ())):
            if node == start and skip_first(identities[child]):
                continue
            if child not in seen:
                seen.add(child)
                queue.append((child, path + [identities[child]]))


def witness(path, host, target):
    result = []
    platform = target
    for parent, child in zip(path, path[1:]):
        edges = [location(parent, d) for d in parent['dependencies']
                 if d['name'] == child['name'] and child in declared_identity(d, [child])]
        if not edges:
            raise InspectionError(f'cannot locate edge {parent["id"]} -> {child["id"]}')
        proc_macro = any('proc-macro' in t['kind'] for t in child['targets'])
        for edge in edges:
            edge['compile_platform'] = host if edge['kind'] == 'build' or proc_macro else platform
        platforms = {e['compile_platform'] for e in edges}
        if len(platforms) != 1:
            # A display node can represent both units; keep both declarations
            # rather than silently assigning a target runtime meaning to host code.
            platform = f'host={host};target={target}'
        else:
            platform = platforms.pop()
        result.append({'from': parent['id'], 'to': child['id'], 'declarations': edges,
                       'proc_macro': proc_macro})
    return result


def allowed_test_path(rule, profile, path):
    if profile['mode'] != 'test' or len(path) < 2:
        return False
    for allow in rule.get('test_edges', []):
        if profile['id'] not in allow['profiles'] or profile['target'] != allow['target']:
            continue
        if not matches(allow['root'], path[0]) or not matches(allow['dependency'], path[1]):
            continue
        for dep in path[0]['dependencies']:
            if (dep['name'] == path[1]['name'] and dep['kind'] == 'dev'
                    and dep['target'] == allow['cfg']
                    and dep['uses_default_features'] == allow['default_features']
                    and sorted(dep['features']) == sorted(allow['features'])):
                # If the same package is also a production dependency, the
                # separate production profiles still reject its closure.
                return True
    return False


def inspect(root, config, builds=True, selected=None):
    validate(config)
    lock = root / 'Cargo.lock'
    if not lock.is_file():
        raise InspectionError('missing Cargo.lock')
    toolchain = run(root, ['rustc', '-vV'])
    host = next(line.split(': ', 1)[1] for line in toolchain.splitlines() if line.startswith('host: '))
    commands = []
    metadata_cmd = ['cargo', 'metadata', '--offline', '--locked', '--all-features', '--format-version', '1']
    commands.append(metadata_cmd)
    metadata = json.loads(run(root, metadata_cmd))
    packages = metadata['packages']
    resolve = {n['id']: n['deps'] for n in metadata['resolve']['nodes']}
    for package in packages:
        package['_resolved_deps'] = resolve.get(package['id'], [])
    members = [p for p in packages if p['id'] in metadata['workspace_members']]
    evidence = {'source_sha': run(root, ['git', 'rev-parse', 'HEAD']).strip(),
                'lock_sha256': hashlib.sha256(lock.read_bytes()).hexdigest(),
                'tool_version': VERSION, 'rule_revision': config['revision'],
                'rules_sha256': hashlib.sha256(json.dumps(config, sort_keys=True).encode()).hexdigest(),
                'toolchain': toolchain, 'host': host}
    results = []
    for rule in config['rules']:
        upstream_path, anchor = rule['upstream'].split('#')
        upstream = (root / upstream_path).read_text()
        anchors = [re.sub(r'[^\w\- ]', '', line.lstrip('# ').strip()).lower().replace(' ', '-')
                   for line in upstream.splitlines() if line.startswith('#')]
        if anchor not in upstream and anchor not in anchors:
            raise InspectionError(f'missing upstream: {rule["upstream"]}')
        for identity in rule['roots']:
            roots = [p for p in members if matches(identity, p)]
            if len(roots) != 1:
                raise InspectionError(f'root identity missing/ambiguous: {identity}')
            package = roots[0]
            context = {'rule': rule['key'], 'root': package['id'], 'upstream': rule['upstream'],
                       'allows': rule['allows'], 'exceptions': rule['exceptions']}
            missing_profiles = set(rule['required_profiles']) - {p['id'] for p in rule['profiles']}
            for profile_id in sorted(missing_profiles):
                results.append(dict(context, status='coverage_gap', reason='required profile not registered',
                                    profile_id=profile_id))
            for dep in package['dependencies']:
                if KINDS[dep['kind']] not in rule['declaration_kinds']:
                    continue
                for forbidden in rule['forbidden']:
                    if dep['name'] != forbidden['name']:
                        continue
                    candidates = resolved_declaration(package, dep, packages)
                    if not candidates and len(forbidden) > 1:
                        raise InspectionError(f'cannot resolve constrained declaration: {dep}')
                    if (len(forbidden) == 1 or any(matches(forbidden, p) for p in candidates)):
                        results.append(dict(context, status='violation', view='declaration',
                                            forbidden=forbidden, edge=location(package, dep)))
            manifest = tomllib.loads(Path(package['manifest_path']).read_text())
            if rule.get('empty_default') and manifest.get('features', {}).get('default', []):
                results.append(dict(context, status='violation', view='features', edge='features.default'))
            for key, values in manifest.get('features', {}).items():
                for value in values:
                    if any(token in value for token in rule.get('forbidden_feature_tokens', [])):
                        results.append(dict(context, status='violation', view='features', edge=f'features.{key}', value=value))
            for profile in rule['profiles']:
                scope = dict(context, profile=profile, target=host if profile['target'] == 'host' else profile['target'])
                if selected and profile['id'] not in selected:
                    results.append(dict(scope, status='coverage_gap', reason='required profile not selected'))
                    continue
                features = profile['features']
                missing = set(features) - package['features'].keys()
                if missing:
                    results.append(dict(scope, status='coverage_gap', reason=f'unregistered features: {sorted(missing)}'))
                    continue
                flags = ['--offline', '--locked', '--package', package['name'], '--target', scope['target']]
                if not profile['default_features']:
                    flags += ['--no-default-features']
                if features:
                    flags += ['--features', ','.join(features)]
                kinds = list(rule['kinds'])
                if profile['mode'] == 'test' and 'dev' not in kinds:
                    kinds.append('dev')
                command = ['cargo', 'tree', *flags, '--edges', ','.join(kinds), '--prefix', 'depth',
                           '--format', '{p}|{f}', '--charset', 'ascii']
                commands.append(command)
                print(f'{rule["key"]}/{package["name"]}/{profile["id"]}: graph {scope["target"]}', file=sys.stderr)
                try:
                    nodes = tree_nodes(run(root, command), packages)
                    production_children = set()
                    if profile['mode'] == 'test':
                        production_command = list(command)
                        production_command[production_command.index('--edges') + 1] = 'normal,build'
                        commands.append(production_command)
                        production_nodes = tree_nodes(run(root, production_command), packages)
                        production_children = {p['id'] for p, path, _ in production_nodes if len(path) == 2}
                    def skip_first(child):
                        return (child['id'] not in production_children and
                                allowed_test_path(rule, profile, [package, child]))
                    for dep, path, enabled in expanded_paths(nodes, skip_first):
                        if any(matches(f, dep) for f in rule['forbidden']):
                            results.append(dict(scope, status='violation', view='closure',
                                                forbidden=dep['id'], enabled_features=enabled,
                                                path=[p['id'] for p in path], edges=witness(path, host, scope['target']), command=command))
                    if builds:
                        command = ['cargo', 'check', *flags, '--tests' if profile['mode'] == 'test' else '--lib']
                        # The launcher is binary-only.
                        if profile['mode'] == 'production' and not any('lib' in t['kind'] for t in package['targets']):
                            command[-1] = '--bins'
                        commands.append(command)
                        print(f'{rule["key"]}/{package["name"]}/{profile["id"]}: target build', file=sys.stderr)
                        run(root, command)
                        results.append(dict(scope, status='pass', view='closure_and_build', command=command))
                    else:
                        results.append(dict(scope, status='coverage_gap', reason='graph inspected; target build not executed'))
                except InspectionError as error:
                    results.append(dict(scope, status='inspection_error', reason=str(error), command=command))
    return {'evidence': evidence, 'commands': commands, 'results': results}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo-root', type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument('--rules', type=Path, default=Path(__file__).with_suffix('.json'))
    parser.add_argument('--graphs-only', action='store_true', help='diagnostic mode; leaves build coverage gaps')
    parser.add_argument('--profile', action='append', help='diagnostic subset; omitted profiles are coverage gaps')
    args = parser.parse_args()
    try:
        config = json.loads(args.rules.read_text())
        report = inspect(args.repo_root.resolve(), config, not args.graphs_only, args.profile)
    except (InspectionError, OSError, ValueError, KeyError, TypeError, AttributeError, StopIteration) as error:
        report = {'results': [{'status': 'inspection_error', 'reason': str(error)}]}
    print(json.dumps(report, indent=2))
    return int(any(r['status'] != 'pass' for r in report['results']))


if __name__ == '__main__':
    sys.exit(main())
