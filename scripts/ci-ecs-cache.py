#!/usr/bin/env python3
"""Optional fixed-profile ECS workspace cache; cache failures never skip Cargo."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import time

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent.parent
STAGING_BYTES = 6 * 1024 ** 3
FAMILY_BYTES = 12 * 1024 ** 3
GENERATIONS = 2


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'scripts' / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


units = load('ecs_units', 'ci-test-artifacts.py')


def temporary(path):
    path = path.resolve()
    target = (ROOT / 'target').resolve()
    if any(path.is_relative_to(base) or base.is_relative_to(path) for base in (ROOT, target)):
        raise ValueError('ECS cache state and reports must be outside checkout and target')
    return path


def build(command, env, state_path):
    state_path = temporary(state_path)
    started = time.monotonic()
    state = None
    try:
        state = units.cache_begin(ROOT)
    except (OSError, ValueError) as error:
        print('ECS cache recording unavailable: ' + str(error), flush=True)
    local = []
    with subprocess.Popen([*command, '--message-format=json-render-diagnostics'], cwd=ROOT,
                          env=env, stdout=subprocess.PIPE, text=True) as process:
        for line in process.stdout:
            try:
                message = json.loads(line)
            except ValueError:
                print(line, end='', flush=True)
                continue
            if message.get('reason') == 'compiler-message':
                print(message['message'].get('rendered', ''), end='', flush=True)
            if (message.get('reason') == 'compiler-artifact'
                    and Path(message['manifest_path']).is_relative_to(ROOT)):
                local.append({key: message.get(key) for key in
                              ('package_id', 'target', 'profile', 'features', 'fresh')})
            if state is not None:
                try:
                    units.cache_record(state, message)
                except (OSError, ValueError, KeyError, TypeError) as error:
                    state['error'] = str(error)
        status = process.wait()
    if status:
        raise subprocess.CalledProcessError(status, command)
    if state is not None and not state.get('error'):
        try:
            units.cache_finish(state)
        except (OSError, ValueError) as error:
            state['error'] = str(error)
    try:
        state_path.parent.mkdir(parents=True, exist_ok=True)
        state_path.write_text(json.dumps(state or {}, sort_keys=True) + '\n')
        state_path.with_name('ecs-units.json').write_text(json.dumps(dict(
            elapsed_seconds=time.monotonic() - started, units=local,
            fresh=sum(bool(unit['fresh']) for unit in local), rebuilt=sum(not unit['fresh'] for unit in local)), indent=2) + '\n')
    except OSError as error:
        print('ECS cache report unavailable: ' + str(error), flush=True)


def identity():
    # Cargo source contents are validated by the unit manifest; source SHA is
    # deliberately separate from the compatible build-configuration identity.
    metadata = json.loads(subprocess.check_output(
        ['cargo', 'metadata', '--locked', '--no-deps', '--format-version=1'], cwd=ROOT, text=True))
    files = {'Cargo.lock', '.github/workflows/ecs-packages.yml', 'scripts/ci-ecs-cache.py',
             'scripts/package-ecs.py', 'scripts/ci-test-artifacts.py'}
    files.update(str(Path(package['manifest_path']).relative_to(ROOT)) for package in metadata['packages'])
    files.update(str(path.relative_to(ROOT)) for path in (ROOT / '.cargo').rglob('*') if path.is_file())
    files.update(name for name in ('rust-toolchain', 'rust-toolchain.toml') if (ROOT / name).is_file())
    configuration = dict(rustc=subprocess.check_output(['rustc', '-vV'], text=True),
                         cargo=subprocess.check_output(['cargo', '-V'], text=True),
                         profile='ecs', features='default', target='native',
                         environment={key: value for key, value in os.environ.items()
                                      if (key.startswith(('CARGO_', 'RUST', 'CC_', 'CXX_', 'AR_', 'PKG_CONFIG'))
                                          or key in ('CC', 'CXX', 'AR', 'CFLAGS', 'CXXFLAGS', 'CPPFLAGS',
                                                     'LDFLAGS', 'LIBRARY_PATH', 'CPATH'))
                                      and key not in ('CARGO_BUILD_JOBS', 'CARGO_TERM_COLOR', 'CARGO_NET_RETRY')},
                         files={name: units.digest(ROOT / name) for name in sorted(files)})
    digest = hashlib.sha256(json.dumps(configuration, sort_keys=True).encode()).hexdigest()
    prefix = 'ecs-workspace-v1-' + digest + '-'
    source = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    return dict(prefix=prefix, key=prefix + source)


def save_allowed(entries, key, raw_bytes):
    family = [entry for entry in entries if entry['ref'] == 'refs/heads/main'
              and entry['key'].startswith('ecs-workspace-v1-')]
    compatible = [entry for entry in family if entry['key'].rsplit('-', 1)[0] == key.rsplit('-', 1)[0]]
    return (not any(entry['key'] == key for entry in family)
            and len(compatible) < GENERATIONS
            and sum(entry['size_in_bytes'] for entry in family) + raw_bytes <= FAMILY_BYTES)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=('identity', 'restore', 'stage'))
    parser.add_argument('--cache', type=Path)
    parser.add_argument('--state', type=Path)
    parser.add_argument('--report', type=Path)
    args = parser.parse_args()
    try:
        if args.mode == 'identity':
            report = identity()
        elif args.mode == 'restore':
            report = units.cache_restore(ROOT, args.cache, STAGING_BYTES)
        else:
            state = json.loads(temporary(args.state).read_text())
            report = units.cache_stage(state, args.cache, STAGING_BYTES, measure_only=True)
            report['save'] = False
            if report.get('eligible'):
                maintenance = load('ecs_maintenance', 'ci-cache-maintenance.py')
                api = maintenance.GitHubApi(os.environ.get('GITHUB_REPOSITORY'), os.environ.get('GH_TOKEN'))
                entries = maintenance.validate_inventory(api.list_caches())
                if save_allowed(entries, identity()['key'], report['raw_bytes']):
                    report = units.cache_stage(state, args.cache, STAGING_BYTES)
                else:
                    report['reason'] = 'remote family budget, generation limit or existing immutable key'
        if args.report:
            temporary(args.report).write_text(json.dumps(report, indent=2) + '\n')
        if os.environ.get('GITHUB_OUTPUT'):
            with open(os.environ['GITHUB_OUTPUT'], 'a') as stream:
                for key, value in report.items():
                    if key in ('save', 'prefix', 'key'):
                        stream.write(f'{key}={str(value).lower() if isinstance(value, bool) else value}\n')
        print(json.dumps(report, sort_keys=True))
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
        print('ECS cache unavailable; full build remains enabled: ' + str(error))
        if args.report:
            try:
                temporary(args.report).write_text(json.dumps(dict(save=False, reason=str(error))) + '\n')
            except (OSError, ValueError):
                pass
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
