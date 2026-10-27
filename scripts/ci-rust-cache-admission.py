#!/usr/bin/env python3
"""Admit trusted Rust dependency caches at their actual upload boundary.

Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7.
Only the pinned Swatinem action receives the Linux tar adapter or Release
Node preload in its step-local environment. Its GitHub cache provider creates
the complete archive before reserving or uploading it; a denied checkpoint
makes that optional save stop. Neither
the upstream action nor its cache identity is modified. This is an observed
capacity check, not a cross-job reservation. Upstream pin changes require a
review of the createTar -> reserve -> upload ordering and argv contract.
"""
import argparse
from datetime import datetime, timezone
import importlib.util
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile


sys.dont_write_bytecode = True
_spec = importlib.util.spec_from_file_location('dependency_maintenance',
    Path(__file__).resolve().with_name('ci-cache-maintenance.py'))
maintenance = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(maintenance)
policy = maintenance.policy
ACTION_REVISION = 'd1527c8f67ef9af357290475d4af4b0fab8f12e0'
DENIED = 78
FAMILIES = {
    'shared-native': ('ci-shared-native-trusted-v3', '.github/workflows/rust.yml', 'full-regression'),
    'ecs': ('oasis7-ecs-native-linux-ecs-v2', '.github/workflows/ecs-packages.yml', 'native-trusted'),
    'release-web': ('release-packages-web-wasm-trusted-v3', '.github/workflows/release-packages.yml', 'build-web-dist'),
    'release-linux': ('oasis7-package-native-Linux-native-packaging-trusted-v2', '.github/workflows/release-packages.yml', 'package-native'),
    'release-windows': ('release-packages-package-native-Windows-native-trusted-v2', '.github/workflows/release-packages.yml', 'package-native'),
    'release-macos': ('release-packages-package-native-macOS-x86_64-apple-darwin-trusted-v2', '.github/workflows/release-packages.yml', 'package-native'),
    'testnet-linux': ('oasis7-package-native-Linux-native-packaging-trusted-v2', '.github/workflows/testnet-packages.yml', 'package-native-trusted'),
}
PLATFORMS = {name: (('Linux', 'X64'),) for name in FAMILIES}
PLATFORMS['release-windows'] = (('Windows', 'X64'),)
PLATFORMS['release-macos'] = (('macOS', 'ARM64'), ('macOS', 'X64'))


class AdmissionError(ValueError):
    """The archive cannot be authorized with the available evidence."""


def regular_file(path):
    value = path.lstat()
    if not stat.S_ISREG(value.st_mode) or value.st_nlink != 1 or value.st_size <= 0:
        raise AdmissionError('cache archive or manifest is not one complete regular file')
    return value


def file_identity(value):
    return value.st_dev, value.st_ino, value.st_size, value.st_mtime_ns, value.st_ctime_ns, value.st_mode


def completed_archive(path, environment):
    """Validate the completed provider archive without assuming a tar executable."""
    path = Path(path)
    runner_temp = Path(environment.get('RUNNER_TEMP', ''))
    workspace = Path(environment.get('GITHUB_WORKSPACE', ''))
    if (not path.is_absolute() or not runner_temp.is_absolute() or not workspace.is_absolute()
            or path.name not in ('cache.tzst', 'cache.tgz')
            or path.parent != path.parent.resolve()
            or path.parent.parent != runner_temp.resolve()
            or path.parent == workspace.resolve()):
        raise AdmissionError('completed cache archive is outside its private runner temp directory')
    regular_file(path)
    if regular_file(path.parent / 'manifest.txt').st_size > 1024 * 1024:
        raise AdmissionError('cache manifest exceeds the bounded pinned-action contract')
    return path


def archive_for_create(args, environment, directory):
    """Accept only the pinned Linux tar contract in its private temp directory."""
    workspace = environment.get('GITHUB_WORKSPACE', '')
    runner_temp = environment.get('RUNNER_TEMP', '')
    if not Path(workspace).is_absolute() or not Path(runner_temp).is_absolute():
        raise AdmissionError('cache archive requires absolute runner workspace and temp paths')
    directory = Path(directory)
    if (directory != directory.resolve() or directory.parent != Path(runner_temp).resolve()
            or directory == Path(workspace).resolve() or not directory.is_dir()):
        raise AdmissionError('cache archive is outside its private runner temp directory')
    for filename, compression in (('cache.tzst', ['--use-compress-program', 'zstdmt']),
                                   ('cache.tgz', ['-z'])):
        expected = ['--posix', '-cf', filename, '--exclude', filename, '-P', '-C', workspace,
                    '--files-from', 'manifest.txt', *compression]
        if args == expected:
            manifest = directory / 'manifest.txt'
            if regular_file(manifest).st_size > 1024 * 1024:
                raise AdmissionError('cache manifest exceeds the bounded pinned-action contract')
            archive = directory / filename
            if archive.exists() or archive.is_symlink():
                raise AdmissionError('cache archive unexpectedly exists before creation')
            return archive
    raise AdmissionError('unknown pinned Rust cache tar creation arguments; upload denied')


def trusted_writer(environment, api):
    family = environment.get('OASIS7_DEPENDENCY_CACHE_FAMILY')
    if family not in FAMILIES:
        raise AdmissionError('unknown dependency cache writer family')
    namespace, workflow, job = FAMILIES[family]
    sha, event, run_id = (environment.get(name, '') for name in
                         ('GITHUB_SHA', 'GITHUB_EVENT_NAME', 'GITHUB_RUN_ID'))
    events = ('schedule', 'workflow_dispatch') if family == 'shared-native' else ('workflow_dispatch',)
    if (environment.get('GITHUB_ACTIONS') != 'true'
            or (environment.get('RUNNER_OS'), environment.get('RUNNER_ARCH')) not in PLATFORMS[family]
            or environment.get('GITHUB_REF') != maintenance.MAIN_REF
            or environment.get('GITHUB_REPOSITORY') != api.repository
            or environment.get('GITHUB_WORKFLOW_SHA') != sha
            or environment.get('GITHUB_JOB') != job or event not in events
            or environment.get('INPUT_SHARED-KEY') != namespace
            or environment.get('GITHUB_WORKFLOW_REF') != f'{api.repository}/{workflow}@refs/heads/main'
            or not re.fullmatch(r'[0-9a-f]{40}', sha) or not re.fullmatch(r'[1-9][0-9]*', run_id)):
        raise AdmissionError('dependency upload requires the current trusted main writer identity')
    if family == 'shared-native' and event == 'workflow_dispatch':
        try:
            payload = json.loads(Path(environment.get('GITHUB_EVENT_PATH', '')).read_text())
            full = payload.get('inputs', {}).get('run_mode') == 'full'
        except (OSError, ValueError, AttributeError):
            full = False
        if not full:
            raise AdmissionError('manual dependency upload requires run_mode=full')
    if family.startswith('release-') or family == 'testnet-linux':
        if (environment.get('OASIS7_DEPENDENCY_CACHE_SOURCE_SHA') != sha
                or environment.get('INPUT_CACHE-PROVIDER') != 'github'):
            raise AdmissionError('package dependency upload requires the matching planned main source and GitHub provider')
        try:
            # The plan must still describe the actual checkout after the build.
            git_environment = {key: value for key, value in environment.items()
                               if key not in ('GH_TOKEN', 'GITHUB_TOKEN', 'ACTIONS_RUNTIME_TOKEN', 'NODE_OPTIONS')}
            head = subprocess.check_output(['git', 'rev-parse', 'HEAD'],
                cwd=environment.get('GITHUB_WORKSPACE'), env=git_environment,
                text=True, stderr=subprocess.DEVNULL, timeout=10).strip()
        except (OSError, subprocess.SubprocessError):
            raise AdmissionError('package source HEAD could not be verified') from None
        if head != sha:
            raise AdmissionError('package source HEAD does not match the planned main source')
    if family == 'testnet-linux':
        try:
            inputs = json.loads(Path(environment.get('GITHUB_EVENT_PATH', '')).read_text()).get('inputs', {})
            selected = inputs.get('package_scope') == 'linux_only' and inputs.get('build_profile') == 'packaging'
        except (OSError, ValueError, AttributeError):
            selected = False
        if not selected:
            raise AdmissionError('Testnet dependency upload requires the trusted Linux packaging lane')
    run = api.request('GET', f'/actions/runs/{run_id}')
    if (not isinstance(run, dict) or run.get('id') != int(run_id) or run.get('head_sha') != sha
            or run.get('head_branch') != 'main' or run.get('event') != event or run.get('path') != workflow
            or not isinstance(run.get('repository'), dict)
            or run['repository'].get('full_name') != api.repository):
        raise AdmissionError('REST run identity does not match the trusted dependency writer')
    return family


def empty_report(environment):
    return dict(mode='dependency-cache-upload-admission', allowed=False,
                family=environment.get('OASIS7_DEPENDENCY_CACHE_FAMILY'),
                action_revision=ACTION_REVISION, checked_at=datetime.now(timezone.utc).isoformat(),
                archive_bytes=None, projected_repository_bytes=None,
                saved_confirmed=None, reason='upload not authorized')


def check_archive(archive, environment, api):
    report = empty_report(environment)
    try:
        before = file_identity(regular_file(archive))
        report['archive_bytes'] = before[2]
        report['family'] = trusted_writer(environment, api)
        budget = policy.configured_budget(environment.get('OASIS7_DEPENDENCY_CACHE_BUDGET_BYTES'))
        if budget is None:
            raise AdmissionError('verified dependency cache repository budget is missing')
        policy.capacity_context(maintenance, api, budget, report)
        if file_identity(regular_file(archive)) != before:
            raise AdmissionError('cache archive changed during capacity verification')
        projected = report['observed_repository_bytes'] + report['archive_bytes']
        report['projected_repository_bytes'] = projected
        report['allowed'] = projected <= report['effective_repository_budget_bytes']
        report['reason'] = 'measured archive fits repository budget' if report['allowed'] else 'repository cache capacity gap'
    except (ValueError, OSError) as error:
        report['reason'] = str(error) if isinstance(error, ValueError) else 'cache archive or event could not be read'
    return report


def emit_report(report, environment):
    print(json.dumps(report, sort_keys=True), flush=True)
    if not report['allowed']:
        reason = str(report['reason']).replace('%', '%25').replace('\r', '%0D').replace('\n', '%0A')
        print('::warning::Rust dependency cache upload skipped: ' + reason, flush=True)
    summary = environment.get('GITHUB_STEP_SUMMARY')
    if summary:
        # Only fixed labels, controlled enums and numeric fields enter Markdown.
        lines = ['\n## Rust dependency cache upload admission\n',
                 f"Allowed: **{report['allowed']}**. This does not confirm a saved cache.\n",
                 '| Measurement | Bytes |', '| --- | ---: |']
        for label, key in (('Final compressed archive', 'archive_bytes'),
                           ('Observed all-ref inventory', 'observed_repository_bytes'),
                           ('Projected total', 'projected_repository_bytes'),
                           ('Configured budget', 'configured_repository_budget_bytes'),
                           ('Live quota', 'live_repository_limit_bytes'),
                           ('Effective budget', 'effective_repository_budget_bytes')):
            value = report.get(key)
            lines.append(f'| {label} | {value if type(value) is int else "unknown"} |')
        lines.append('\nThis upload check does not reserve capacity against concurrent writers.\n')
        try:
            with Path(summary).open('a') as output:
                output.write('\n'.join(lines))
        except OSError:
            print('::warning::Could not append dependency cache admission summary', flush=True)


def run_tar(args, environment=None, directory=None, *, api=None):
    environment = os.environ if environment is None else environment
    directory = Path.cwd() if directory is None else Path(directory)
    # These are the pinned provider's only read-only tar operations. Unknown
    # operations cannot silently become an unguarded cache creation on upgrade.
    passthrough = args == ['--version'] or (args and args[0] in ('-xf', '-tf'))
    if passthrough:
        return subprocess.run(['/usr/bin/tar', *args], cwd=directory, env=environment).returncode
    report = empty_report(environment)
    try:
        archive = archive_for_create(args, environment, directory)
        result = subprocess.run(['/usr/bin/tar', *args], cwd=directory, env=environment)
        if result.returncode:
            return result.returncode
        if api is None:
            if not environment.get('GH_TOKEN'):
                raise AdmissionError('read-only Actions token missing for dependency cache admission')
            # Start the API's request/time budget after potentially slow packing.
            api = maintenance.GitHubApi(environment.get('GITHUB_REPOSITORY'), environment['GH_TOKEN'])
        report = check_archive(archive, environment, api)
    except (ValueError, OSError) as error:
        report['reason'] = str(error) if isinstance(error, ValueError) else 'cache archiver could not complete'
    emit_report(report, environment)
    return 0 if report['allowed'] else DENIED


def install_adapter(environment):
    runner_temp = Path(environment.get('RUNNER_TEMP', ''))
    source = Path(__file__).resolve()
    original_path = environment.get('PATH', '')
    if (not runner_temp.is_absolute() or not runner_temp.is_dir()
            or not original_path or any(char in original_path + str(runner_temp) for char in '\r\n')
            or not os.access('/usr/bin/tar', os.X_OK) or not os.access(source, os.X_OK)):
        raise AdmissionError('cannot install executable cache adapter in the runner temp directory')
    directory = Path(tempfile.mkdtemp(prefix='rust-cache-tar-', dir=runner_temp))
    (directory / 'tar').symlink_to(source)
    return str(directory) + os.pathsep + original_path


def install_release_guard(environment):
    """Keep the preload confined to one action and its post, including Windows."""
    if environment.get('NODE_OPTIONS'):
        raise AdmissionError('dependency cache guard cannot inherit unknown NODE_OPTIONS')
    source = Path(__file__).resolve()
    preload = source.with_suffix('.cjs')
    hook = source.with_name('ci-rust-cache-admission-hook.cjs')
    for path in (preload, hook):
        regular_file(path)
    python = str(Path(sys.executable).resolve())
    preload_option = str(preload).replace('\\', '/')
    if any(char in preload_option + python for char in '\r\n"'):
        raise AdmissionError('dependency cache guard paths cannot contain quotes or newlines')
    return {'node_options': '--require "' + preload_option + '"', 'python': python, 'guard_ready': 'true'}


def main(argv=None):
    if Path(sys.argv[0]).name == 'tar':
        return run_tar(sys.argv[1:])
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=('install', 'install-release', 'check'))
    parser.add_argument('--archive')
    args = parser.parse_args(argv)
    if args.command == 'check':
        report = empty_report(os.environ)
        try:
            path = completed_archive(args.archive or '', os.environ)
            if not os.environ.get('GH_TOKEN'):
                raise AdmissionError('read-only Actions token missing for dependency cache admission')
            api = maintenance.GitHubApi(os.environ.get('GITHUB_REPOSITORY'), os.environ['GH_TOKEN'])
            report = check_archive(path, os.environ, api)
        except (ValueError, OSError) as error:
            report['reason'] = str(error) if isinstance(error, ValueError) else 'completed archive could not be read'
        emit_report(report, os.environ)
        return 0 if report['allowed'] else DENIED
    outputs = (install_release_guard(os.environ) if args.command == 'install-release'
               else {'path': install_adapter(os.environ)})
    with Path(os.environ['GITHUB_OUTPUT']).open('a') as output:
        for name, value in outputs.items():
            output.write(name + '=' + value + '\n')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
