#!/usr/bin/env python3
"""Run the pinned, repaired Rust default source queries on a production checkout."""
import argparse
import importlib.util
import json
import math
import os
from pathlib import Path
import re
import shutil
import time


HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('production_candidate', HERE / 'codeql-rust-candidate-workload.py')
candidate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(candidate)
comparison, capacity, runtime = candidate.comparison, candidate.capacity, candidate.runtime
evidence, cargo_guard = capacity.evidence, capacity.cargo_guard
action_deadline = capacity.action_deadline

VERSION = '2.27.4'
PACK = 'oasis7/rust-default-production'
CATEGORY = 'oasis7/rust-repo/default'
SCHEMA = 'oasis7-codeql-rust-production/v1'
CAP = 256 * 1024**2
CLOSEOUT_SECONDS = 90
MODE = 'rust-default-production'


def read_json(path, limit=16 * 1024**2):
    return candidate.read_json(path, limit)


def budget(deadline, *, now=None, monotonic=None):
    now = time.time() if now is None else now
    monotonic = time.monotonic() if monotonic is None else monotonic
    if (not isinstance(deadline, (int, float)) or isinstance(deadline, bool)
            or not math.isfinite(deadline) or deadline - now > 4500):
        raise RuntimeError('invalid fixed production analysis deadline')
    return monotonic + deadline - now


def validate_paths(args):
    runner = Path(os.environ['RUNNER_TEMP']).resolve()
    for name in ('source', 'codeql', 'database', 'work', 'output', 'sarif'):
        path = getattr(args, name).absolute()
        if path.resolve() != path or any(c in str(path) for c in ('\n', '\r')):
            raise RuntimeError('unsafe production path: ' + name)
        setattr(args, name, path)
    if not args.source.is_dir() or not args.codeql.is_file():
        raise RuntimeError('missing production checkout or CodeQL executable')
    roots = [args.work, args.output, args.database, args.codeql.parent, args.sarif.parent]
    for root in roots[:3] + [args.sarif.parent]:
        if root == runner or not root.is_relative_to(runner):
            raise RuntimeError('production work and evidence must be inside RUNNER_TEMP')
    for index, root in enumerate(roots):
        for other in [args.source, *roots[:index]]:
            if root.is_relative_to(other) or other.is_relative_to(root):
                raise RuntimeError('production paths overlap protected inputs')
    if not re.fullmatch(r'[0-9a-f]{40}', args.source_sha):
        raise RuntimeError('invalid production source SHA')


def identity(args):
    return dict(schema=SCHEMA, source_sha=args.source_sha, source=str(args.source),
                codeql=str(args.codeql), database=str(args.database), work=str(args.work),
                output=str(args.output), sarif=str(args.sarif), analysis_deadline=args.analysis_deadline,
                bundle_version=VERSION, bundle_sha256=capacity.bundle.BUNDLES[VERSION]['sha256'],
                variant=candidate.VARIANT, pack=PACK, category=CATEGORY)


def production_budget(args):
    path = (Path(os.environ['RUNNER_TEMP']).resolve() / 'codeql-rust-production-budget.json'
            if args.phase == 'prepare' else args.output / 'production-budget.json')
    value = read_json(path)
    if value.get('source_sha') != args.source_sha:
        raise RuntimeError('production budget checkout identity differs')
    analysis_end, _ = action_deadline.shared_budget(
        value, value.get('fixed_job_deadline'), args.analysis_deadline,
        time.time(), time.monotonic(), mode=MODE)
    if args.phase == 'prepare':
        save(args, 'production-budget.json', value)
    return analysis_end


class Runner:
    def __init__(self, args, terminal, end):
        self.args, self.terminal, self.end = args, terminal, end
        self.env = {key: value for key, value in os.environ.items()
                    if key not in ('GH_TOKEN', 'GITHUB_TOKEN', 'RUSTC_WRAPPER')}

    def __call__(self, argv, name, limit=None):
        self.terminal['stage'] = name
        remaining = self.end - time.monotonic()
        if remaining <= 0:
            raise TimeoutError('fixed production analysis deadline exhausted')
        return runtime.run(list(map(str, argv)), self.args.source,
                           self.args.output / (name + '.log'), self.end,
                           min(remaining, limit or remaining), self.args.output, self.env,
                           file_limit=None, restricted_output=True)


def save(args, name, value, *, terminal=False):
    runtime.save(args.output / name, value, args.output, terminal=terminal)


def action_identity(args):
    path = Path(os.environ['CODEQL_ACTION_TEMP']).resolve() / 'config'
    config = read_json(path)
    capacity.verify_action_config(config, args.source, args.database)
    if os.environ.get('CODEQL_EXTRACTOR_RUST_OPTION_CARGO_ALL_TARGETS') != 'true':
        raise RuntimeError('Rust all-targets extraction was not enabled')
    wanted = [str(args.source / 'crates/oasis7_builtin_wasm_modules/_templates')]
    if json.loads(os.environ.get('CODEQL_EXTRACTOR_RUST_OPTION_EXTRA_INCLUDES', 'null')) != wanted:
        raise RuntimeError('Rust template inclusion differs')
    return dict(sha256=runtime.digest(path), settings={key: config.get(key) for key in
                ('languages', 'buildMode', 'dbLocation', 'repositoryRoot', 'overlayDatabaseMode',
                 'trapCaches', 'dependencyCachingEnabled', 'computedConfig', 'useOverlayDatabaseCaching')})


def maintained_manifests(frozen):
    return sorted(name for name in frozen if Path(name).name == 'Cargo.toml'
                  and not name.startswith(capacity.EXCLUDES))


def source_queries(official, work, suite, end):
    """Retain every official QL/support byte, replacing only published bindings."""
    entries = candidate.freeze_suite(suite)
    published = official / 'qlpacks/codeql/rust-queries/0.1.46'
    if suite.get('query_root') != str(published):
        raise RuntimeError('official Rust query pack location differs')
    root = work / 'default-queries'
    root.mkdir()
    excluded = []
    for relative in candidate.regular_snapshot(published, end):
        capacity.check_time(end)
        path = Path(relative)
        if (path.parts[0] == '.codeql' or path.suffix == '.qlx'
                or relative in ('.packinfo', 'codeql-pack.lock.yml', 'qlpack.yml')):
            excluded.append(relative)
            continue
        destination = root / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(published / relative, destination)
        if capacity.digest_until(destination, end) != capacity.digest_until(published / relative, end):
            raise RuntimeError('official query source copy differs')
    manifest = ('name: ' + PACK + '\nversion: 0.0.0\nlibrary: false\nwarnOnImplicitThis: true\n'
                'dependencies:\n  codeql/rust-all: 0.2.25\n  codeql/suite-helpers: 1.0.61\n'
                '  codeql/util: 2.0.48\nextractor: rust\n'
                'defaultSuiteFile: codeql-suites/rust-code-scanning.qls\n')
    (root / 'qlpack.yml').write_text(manifest)
    if runtime.digest(root / 'codeql-suites/rust-code-scanning.qls') != candidate.SUITE_SHA:
        raise RuntimeError('production default suite bytes differ')
    records = [dict(entry, path=str(root / entry['relative']), root=str(root), pack=PACK) for entry in entries]
    validate_records(records, work)
    return records, dict(excluded_published_files=excluded, replacement_manifest=manifest)


def validate_records(records, work):
    root = work / 'default-queries'
    if not isinstance(records, list):
        raise RuntimeError('invalid production query records')
    entries = [{key: record[key] for key in ('relative', 'id', 'sha256')} for record in records]
    candidate.freeze_suite(dict(queries=entries, suite_sha256=runtime.digest(root / 'codeql-suites/rust-code-scanning.qls')))
    for record in records:
        path = Path(record['path'])
        if (record['root'] != str(root) or record['pack'] != PACK or path.resolve() != path
                or path != root / record['relative'] or not path.is_relative_to(root)
                or runtime.digest(path) != record['sha256']):
            raise RuntimeError('production default query identity differs')


def query_options(closure, work, prefix):
    return candidate.query_options(closure, work, prefix)


def resolve_queries(args, records, closure, run):
    options = query_options(closure, args.work, 'resolve')
    root = args.work / 'default-queries'
    run([args.codeql, 'resolve', 'queries', root / 'codeql-suites/rust-code-scanning.qls',
         '--format=json', options[0], options[-1]], 'production-default-suite', 30)
    actual = capacity.suite_entries(read_json(args.output / 'production-default-suite.log'), root)
    expected = [{key: record[key] for key in ('relative', 'id', 'sha256')} for record in records]
    if actual != expected:
        raise RuntimeError('production wrapper changed official default query selection')
    receipts = []
    for index, record in enumerate(records):
        name = 'resolve-' + str(index).zfill(2)
        run([args.codeql, 'resolve', 'library-path', '--query=' + record['path'], '--format=json',
             *options], name, 20)
        value = read_json(args.output / (name + '.log'))
        capacity.bundle.verify_library_path(value, root, str(Path(record['relative']).with_suffix('')), closure, PACK)
        if value['compilationCache'] != [str(args.work / 'resolve-compilation-cache')]:
            raise RuntimeError('production resolver selected an unbound compilation cache')
        receipts.append(dict(query=record['path'], actual=value, status='PASS_REPAIRED_CLOSURE'))
        save(args, 'library-resolution.json', receipts)


def input_snapshot(args, official_closure, end):
    return dict(official={name: candidate.regular_snapshot(Path(pack['root']), end)
                          for name, pack in sorted(official_closure.items())},
                repaired_library=candidate.regular_snapshot(args.work / 'candidate-rust-all', end),
                query_pack=candidate.regular_snapshot(args.work / 'default-queries', end))


def source_observation(args, stage, end):
    initial = read_json(args.output / 'lock-inventory-initial.json')
    manifests = read_json(args.output / 'manifest-inventory.json')
    current = evidence.lock_inventory(args.source, manifests, args.output, end, args.source_sha,
                                      stage=stage, fixed_deadline=args.analysis_deadline)
    checkout = evidence.capture_identity(args.source, args.output, end, args.source_sha,
                                         prefix=stage, initial_locks=initial)
    frozen = read_json(args.output / 'source-before.json')
    blobs = {name: value for name, value in frozen.items() if 'gitlink' not in value}
    unchanged = capacity.snapshot(args.source, blobs, end=end) == blobs
    def binding(value):
        return {row['path']: (row['status'], row.get('bytes'), row.get('sha256')) for row in value['locks']}
    locks_unchanged = (initial.get('status') == current.get('status') == 'COMPLETE'
                       and binding(initial) == binding(current))
    report = dict(status='PASS' if unchanged and locks_unchanged and checkout.get('status') == 'PASS' else 'FAIL',
                  source_sha=args.source_sha, source_bytes='UNCHANGED' if unchanged else 'CHANGED',
                  checkout=checkout.get('status'), locks='UNCHANGED' if locks_unchanged else 'UNKNOWN_OR_CHANGED')
    save(args, stage + '-inputs.json', report)
    if report['status'] != 'PASS':
        raise RuntimeError('production checkout or lock inputs changed: ' + stage)
    return report


def prepare(args, terminal, run):
    end = run.end
    checkout = evidence.capture_identity(args.source, args.output, end, args.source_sha, prefix='prepare')
    if checkout.get('status') != 'PASS':
        raise RuntimeError('production checkout is not the planned clean source')
    frozen = capacity.snapshot(args.source, end=end)
    save(args, 'source-before.json', frozen)
    manifests = maintained_manifests(frozen)
    if not manifests:
        raise RuntimeError('no maintained Rust manifests')
    save(args, 'manifest-inventory.json', manifests)
    initial = evidence.lock_inventory(args.source, manifests, args.output, end, args.source_sha,
                                      stage='initial', fixed_deadline=args.analysis_deadline)
    if initial.get('status') != 'COMPLETE':
        raise RuntimeError('initial production lock observation incomplete')
    independent = cargo_guard.standalone_locks(args.source, manifests, initial, frozen)
    save(args, 'independent-workspace-locks.json', dict(independent, source_sha=args.source_sha))
    if independent['status'] != 'PASS':
        raise RuntimeError('production independent workspace locks are not frozen')
    save(args, 'action-config.json', action_identity(args))
    resources = capacity.resources(os.environ)
    save(args, 'resources.json', resources)
    run([args.codeql, 'version', '--format=json'], 'version', 20)
    if read_json(args.output / 'version.log').get('version') != VERSION:
        raise RuntimeError('production CodeQL version differs')
    official = args.codeql.parent
    original_closure = capacity.bundle.bundle_closure(official, VERSION)
    original_closure['codeql/suite-helpers'] = candidate.suite_helper(official)
    if len(original_closure) != 14:
        raise RuntimeError('unexpected production library closure')
    published = official / 'qlpacks/codeql/rust-queries/0.1.46'
    suite_file = published / 'codeql-suites/rust-code-scanning.qls'
    run([args.codeql, 'resolve', 'queries', suite_file, '--format=json'], 'official-default-suite', 30)
    suite = dict(query_root=str(published), suite_sha256=runtime.digest(suite_file),
                 queries=capacity.suite_entries(read_json(args.output / 'official-default-suite.log'), published))
    candidate.freeze_suite(suite)
    save(args, 'official-default-suite.json', suite)
    private = comparison.prepare_candidate(original_closure, args.work, args.output, end, run, copy_end=end)
    candidate.prepare_lines_model(Path(original_closure['codeql/rust-all']['root']), private, args.output, end)
    closure = comparison.candidate_closure(original_closure, private)
    records, packaging = source_queries(official, args.work, suite, end)
    save(args, 'queries.json', records)
    save(args, 'packaging.json', packaging)
    save(args, 'selected-closure.json', closure)
    save(args, 'official-closure.json', original_closure)
    resolve_queries(args, records, closure, run)
    save(args, 'input-identities.json', input_snapshot(args, original_closure, end))
    source_observation(args, 'pre-extract', end)
    home = cargo_guard.install(args.work, args.output, args.analysis_deadline, args.source_sha,
                               [args.source / name for name in independent['manifests']], monotonic_deadline=end)
    minutes = int((args.analysis_deadline - time.time()) // 60)
    if minutes < 1:
        raise RuntimeError('no extraction/query budget remains after production preparation')
    terminal.update(status='PREPARED', execution_status='not_run', default_queries=len(records),
                    maintained_manifests=len(manifests), independent_workspaces=len(independent['manifests']),
                    analysis_minutes=minutes, cargo_home=str(home))
    # The wrapper restores the original Cargo home for real commands; only the
    # extractor's Cargo entry point is replaced, preserving its toolchain choice.
    with open(os.environ['GITHUB_ENV'], 'a') as stream:
        stream.write('CARGO_HOME=' + str(home) + '\n')
    with open(os.environ['GITHUB_OUTPUT'], 'a') as stream:
        stream.write('cargo_home=' + str(home) + '\nanalysis_minutes=' + str(minutes) + '\n'
                     + 'node_options=--require=' + str(HERE / 'codeql-rust-action-deadline.cjs') + '\n')


def check_prepared(args, end):
    prepared = read_json(args.output / 'production-prepare.json')
    if prepared.get('status') != 'PREPARED' or any(prepared.get(key) != value for key, value in identity(args).items()):
        raise RuntimeError('production preparation identity or fixed deadline differs')
    original_closure = read_json(args.output / 'official-closure.json')
    actual = capacity.bundle.bundle_closure(args.codeql.parent, VERSION)
    actual['codeql/suite-helpers'] = candidate.suite_helper(args.codeql.parent)
    if actual != original_closure:
        raise RuntimeError('production official dependency identity differs')
    closure = read_json(args.output / 'selected-closure.json')
    if closure != comparison.candidate_closure(original_closure, args.work / 'candidate-rust-all'):
        raise RuntimeError('production repaired dependency selection differs')
    if input_snapshot(args, original_closure, end) != read_json(args.output / 'input-identities.json'):
        raise RuntimeError('production query or library bytes changed')
    if candidate.lines_identity(Path(original_closure['codeql/rust-all']['root']),
                                args.work / 'candidate-rust-all', end) != read_json(args.output / 'lines-model-identity.json'):
        raise RuntimeError('production two-file repair identity differs')
    records = read_json(args.output / 'queries.json')
    validate_records(records, args.work)
    recorded = read_json(args.output / 'resources.json')
    resources = capacity.resources(os.environ)
    if any(resources[key] != recorded[key] for key in ('CODEQL_RAM', 'CODEQL_THREADS')):
        raise RuntimeError('production resource selection changed')
    if action_identity(args) != read_json(args.output / 'action-config.json'):
        raise RuntimeError('production Action configuration changed')
    return records, closure, original_closure, resources


def guard_observation(args, stage, end):
    report = cargo_guard.inspect(args.output, args.analysis_deadline, args.source_sha, end=end)
    save(args, stage + '-cargo-guard.json', report)
    if report.get('status') != 'PASS':
        raise RuntimeError('production extraction Cargo audit failed: ' + stage)
    return report


def run_queries(args, terminal, run):
    if args.extraction_outcome != 'success':
        raise RuntimeError('official extraction did not succeed; queries not started')
    capacity.check_time(run.end)
    records, closure, original_closure, resources = check_prepared(args, run.end)
    supervision = action_deadline.admit_production(args.output, args.source, args.source_sha,
                                                   args.analysis_deadline)
    if supervision.get('status') != 'PASS':
        raise RuntimeError('absolute production extraction deadline was not admitted')
    save(args, 'production-action-deadline.json', supervision)
    terminal['extraction_deadline'] = supervision
    source_observation(args, 'pre-query', run.end)
    terminal['cargo_guard'] = guard_observation(args, 'pre-query', run.end)
    run([args.codeql, 'resolve', 'database', args.database, '--format=json'], 'database-identity', 20)
    terminal['database_metadata_sha256'] = capacity.finalized_database_identity(
        read_json(args.output / 'database-identity.log'), args.source, args.database)
    result_root = args.database / 'results'
    if result_root.is_symlink() or any(path.is_symlink() or path.suffix == '.bqrs' for path in result_root.rglob('*')):
        raise RuntimeError('production extraction checkpoint has existing query results')
    for name in ('batch-compilation-cache', 'batch-common-cache'):
        (args.work / name).mkdir()
    terminal['queries_status'] = 'RUNNING'
    save(args, 'production-run.json', terminal, terminal=True)
    run([args.codeql, 'database', 'run-queries', *query_options(closure, args.work, 'batch'),
         '--no-use-precompiled', '--threads=' + str(resources['CODEQL_THREADS']),
         '--ram=' + str(resources['CODEQL_RAM']), '--', args.database,
         *[record['path'] for record in records]], 'default-query-batch')
    terminal['queries_status'] = 'COMPUTED_UNVERIFIED'
    verified = []
    for index, record in enumerate(records):
        label = str(index).zfill(2)
        result = result_root / PACK / Path(record['relative']).with_suffix('.bqrs')
        if result.resolve() != result or not result.is_file():
            raise RuntimeError('missing or unsafe production default BQRS: ' + record['id'])
        run([args.codeql, 'bqrs', 'info', result, '--format=json'], 'info-' + label, 20)
        run([args.codeql, 'bqrs', 'decode', result, '--format=json'], 'decoded-' + label, 30)
        info = read_json(args.output / ('info-' + label + '.log'))
        capacity.verify_decoded(info, read_json(args.output / ('decoded-' + label + '.log'), 64 * 1024**2))
        verified.append(dict(id=record['id'], relative=record['relative'], result_info=info,
                             bqrs_bytes=result.stat().st_size, bqrs_sha256=capacity.digest_until(result, run.end)))
        save(args, 'default-results.json', verified)
    staged_sarif = args.work / 'default-results.sarif'
    run([args.codeql, 'database', 'interpret-results', '--format=sarifv2.1.0',
         '--output=' + str(staged_sarif), '--sarif-category=' + CATEGORY,
         '--threads=' + str(resources['CODEQL_THREADS']), query_options(closure, args.work, 'batch')[0],
         '--common-caches=' + str(args.work / 'batch-common-cache'), '--', args.database,
         *[record['path'] for record in records]], 'interpret-default-results')
    findings = capacity.check_sarif(read_json(staged_sarif, 256 * 1024**2))
    source_observation(args, 'post-query', run.end)
    terminal['cargo_guard'] = guard_observation(args, 'post-query', run.end)
    if input_snapshot(args, original_closure, run.end) != read_json(args.output / 'input-identities.json'):
        raise RuntimeError('production query or library input changed during analysis')
    capacity.check_time(run.end)
    args.sarif.parent.mkdir(exist_ok=True)
    if args.sarif.exists() or any(args.sarif.parent.glob('*.sarif')):
        raise RuntimeError('production SARIF output already exists')
    shutil.move(staged_sarif, args.sarif)
    terminal.update(status='COMPLETE', execution_status='success', queries_status='VERIFIED',
                    default_queries=len(records), findings=findings,
                    sarif_sha256=capacity.digest_until(args.sarif, run.end), sarif_bytes=args.sarif.stat().st_size,
                    upload_status='NOT_PERFORMED_BY_HELPER', coverage_acceptance='PENDING_HOSTED_REVIEW',
                    semantic_controls='PENDING_PRODUCTION_ACCEPTANCE')


def capture_coverage(args, terminal, end):
    """Failure closeout never starts a query or decoding subprocess."""
    coverage = dict(status='COVERAGE_REQUIRES_REVIEW', manifest_enumeration='UNKNOWN',
                    source_sha=args.source_sha, diagnostics=[], capture_status='NOT_REACHED')
    try:
        capacity.check_time(end)
        capture = evidence.collect_logs(args.database, args.output, end)
        terminal['log_capture_status'] = capture.get('status', 'PARTIAL')
        texts = []
        for item in capture.get('files', []):
            name = item.get('destination')
            if not name:
                continue
            path = args.output / name
            if path.resolve() != path or not path.is_relative_to(args.output / 'database-logs'):
                raise RuntimeError('unsafe production captured log path')
            if path.is_file():
                texts.extend(capacity.captured_segments(path, item, end=end))
        coverage = capacity.coverage_report(args.source, read_json(args.output / 'manifest-inventory.json'), texts,
                                            end=end, logs_complete=capture.get('status') == 'COMPLETE')
        coverage.update(source_sha=args.source_sha, capture_status=capture.get('status', 'PARTIAL'))
    except Exception as error:
        coverage.update(capture_status='PARTIAL', capture_error=type(error).__name__ + ': ' + str(error)[:2048])
    save(args, 'coverage.json', coverage, terminal=True)
    terminal['coverage_status'] = coverage['status']


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('phase', choices=('prepare', 'run'))
    for name in ('codeql', 'source', 'database', 'work', 'output', 'sarif'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--source-sha', required=True)
    parser.add_argument('--analysis-deadline', type=float, required=True)
    parser.add_argument('--extraction-outcome', choices=('success', 'failure', 'cancelled', 'skipped'), default='skipped')
    args = parser.parse_args(argv)
    validate_paths(args)
    end = budget(args.analysis_deadline)
    if args.phase == 'prepare':
        args.output.mkdir(exist_ok=False)
        args.work.mkdir(exist_ok=False)
    elif not args.output.is_dir() or not args.work.is_dir():
        raise RuntimeError('production preparation directories missing')
    runtime.CAP, runtime.WRITE_CAP = CAP, CAP - 1024**2
    terminal = dict(identity(args), phase=args.phase, status='FAILED', execution_status='failure',
                    queries_status='NOT_STARTED', stage='admission', extraction_outcome=args.extraction_outcome,
                    coverage_acceptance='PENDING_HOSTED_REVIEW', primary_failure=None)
    with runtime.cancellation_guard():
        try:
            if (os.environ.get('GITHUB_EVENT_NAME') not in ('workflow_dispatch', 'schedule')
                    or os.environ.get('GITHUB_REF') != 'refs/heads/main'
                    or os.environ.get('GITHUB_SHA') != args.source_sha):
                raise RuntimeError('production repaired queries require the trusted main maintenance checkout')
            end = production_budget(args)
            run = Runner(args, terminal, end)
            if args.phase == 'prepare':
                prepare(args, terminal, run)
            else:
                run_queries(args, terminal, run)
        except Exception as error:
            terminal.update(status='FAILED', execution_status='failure', primary_failure=dict(
                stage=terminal['stage'], error=type(error).__name__ + ': ' + str(error)[:4096]))
            if terminal['queries_status'] == 'RUNNING':
                terminal['queries_status'] = 'INCOMPLETE'
        finally:
            # Preserve the primary outcome before optional log traversal.
            save(args, 'production-' + args.phase + '.json', terminal, terminal=True)
            if args.phase == 'run':
                capture_coverage(args, terminal, min(end + CLOSEOUT_SECONDS, time.monotonic() + CLOSEOUT_SECONDS))
                save(args, 'production-run.json', terminal, terminal=True)
    print(json.dumps({key: terminal[key] for key in ('phase', 'status', 'execution_status', 'primary_failure')}))
    return 0 if terminal['status'] in ('PREPARED', 'COMPLETE') else 1


if __name__ == '__main__':
    raise SystemExit(main())
