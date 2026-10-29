#!/usr/bin/env python3
"""Bounded official default-suite profiling; no result decoding or upload."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import time
import codeql_rust_diagnostic_runtime as runtime

CAP = 256 * 1024**2
SUMMARY_RESERVE = 64 * 1024**2


def log_completeness(path):
    headers = footers = records = 0
    if not path.is_file():
        return {'status': 'NOT_REACHED'}
    try:
        with path.open() as stream:
            for line in stream:
                if len(line.encode()) > 4 * 1024**2:
                    raise ValueError('oversized event')
                if not line.strip():
                    continue
                value = json.loads(line)
                if not isinstance(value, dict):
                    raise ValueError('event is not an object')
                records += 1
                headers += value.get('type') == 'LOG_HEADER'
                footers += value.get('type') == 'LOG_FOOTER'
        status = 'COMPLETE' if headers == footers == 1 else 'PARTIAL'
    except (ValueError, OSError):
        status = 'PARTIAL'
    return dict(status=status, headers=headers, footers=footers, records=records,
                sha256=runtime.digest(path), bytes=path.stat().st_size)


def suite_path(frozen, cli):
    root = Path(frozen['query_root'])
    if not root.resolve().is_relative_to((cli.parent / 'qlpacks/codeql/rust-queries').resolve()):
        raise ValueError('suite outside official bundle')
    suite = root / 'codeql-suites/rust-code-scanning.qls'
    if runtime.digest(suite) != frozen['suite_sha256']:
        raise ValueError('official suite changed')
    if not frozen['queries']:
        raise ValueError('empty official suite')
    for item in frozen['queries']:
        query = root / item['relative']
        if not query.resolve().is_relative_to(root.resolve()) or runtime.digest(query) != item['sha256']:
            raise ValueError('official query changed')
    return suite


def execute(args, capacity):
    out, source, cli = args.output, args.source, args.codeql.resolve()
    now, wall = time.monotonic(), time.time()
    end = now + args.deadline - wall - 180
    analysis_end = now + args.analysis_end - wall
    terminal = dict(status='NOT_REACHED', full_rust_coverage='HOLD', production_capacity='UNKNOWN',
                    security_upload=False, result_verification='NOT_REQUESTED',
                    summaries='NOT_REACHED', evaluator_log={'status':'NOT_REACHED'},
                    query_status='NOT_REACHED', tuple_counting=False,
                    fixed_job_deadline=args.deadline, fixed_analysis_end=args.analysis_end,
                    profile_is_instrumented=True, database_output_cap='NOT_ENFORCED_BY_EVIDENCE_CAP')
    failure = None
    evaluator = out / 'official-evaluator.jsonl'
    runtime.CAP = CAP
    runtime.WRITE_CAP = CAP - 8 * 1024**2
    try:
        if not 0 < analysis_end - time.monotonic() <= 2400 or args.analysis_end > args.deadline - 600:
            raise ValueError('invalid shared analysis deadline')
        frozen = json.loads((out / 'default-suite.json').read_text())
        suite = suite_path(frozen, cli)
        env = dict(os.environ)
        runtime.run([str(cli), 'resolve', 'queries', str(suite), '--format=json'], source,
                    out/'profile-resolved-suite.log', analysis_end, 30, out, env, file_limit=None)
        actual = capacity.suite_entries(json.loads((out/'profile-resolved-suite.log').read_text()), Path(frozen['query_root']))
        if actual != frozen['queries']:
            raise ValueError('resolved suite differs from frozen official default')
        actual_suite = args.database / 'temp/config-queries.qls'
        if actual_suite.stat().st_size > 4 * 1024**2:
            raise ValueError('oversized Action suite')
        runtime.evidence_write(out/'profile-action-config-queries.qls', actual_suite.read_bytes(), out)
        runtime.run([str(cli), 'resolve', 'queries', str(actual_suite), '--format=json'], source,
                    out/'profile-action-suite.log', analysis_end, 30, out, env, file_limit=None)
        configured = capacity.suite_entries(json.loads((out/'profile-action-suite.log').read_text()), Path(frozen['query_root']))
        if configured != frozen['queries']:
            raise ValueError('Action configured suite differs from frozen official default')
        terminal['query_count'] = len(actual)
        terminal['status'] = 'RUNNING'
        terminal['query_status'] = 'RUNNING'
        runtime.save(out/'profile-terminal.json', terminal, out, terminal=True)
        runtime.WRITE_CAP -= SUMMARY_RESERVE
        try:
            runtime.run([str(cli), 'database', 'run-queries', str(args.database), str(actual_suite),
                         '--threads='+str(args.threads), '--ram='+str(args.ram),
                         '--evaluator-log', '{EVALUATOR_FIFO}', '--evaluator-log-minify',
                         '--no-tuple-counting'], source, out/'profile-query.log', analysis_end,
                        max(0, analysis_end-time.monotonic()), out, env, file_limit=None, evaluator=evaluator)
            terminal['query_status'] = 'COMPLETED_NOT_VERIFIED'
        except runtime.PhaseError as exc:
            failure = exc
            terminal['status'] = 'INCOMPLETE'
            terminal['query_status'] = 'INCOMPLETE'
            terminal['failure'] = exc.result
        finally:
            runtime.WRITE_CAP = CAP - 8 * 1024**2
        terminal['evaluator_log'] = log_completeness(evaluator)
        terminal['summaries'] = (runtime.official_summaries(str(cli), evaluator, source, out, min(end-300,time.monotonic()+60), env, failure)
                                 if evaluator.is_file() else 'NOT_REACHED_NO_EVALUATOR_LOG')
        complete = (terminal['query_status'] == 'COMPLETED_NOT_VERIFIED' and
                    terminal['evaluator_log']['status'] == 'COMPLETE' and
                    isinstance(terminal['summaries'],dict) and
                    all(terminal['summaries'].get(fmt) == 'GENERATED_NOT_EVALUATION_COMPLETION' for fmt in ('overall','predicates')))
        terminal['status'] = 'PROFILE_COMPLETED_QUERIES_NOT_VERIFIED' if complete else 'INCOMPLETE'
    except Exception as exc:
        terminal['status'] = 'INCOMPLETE'
        terminal['error'] = repr(exc)[:2048]
        terminal['evaluator_log'] = log_completeness(evaluator)
    finally:
        runtime.WRITE_CAP = CAP - 8 * 1024**2
        runtime.save(out/'profile-terminal.json', terminal, out, terminal=True)
    return 0 if terminal['status'] == 'PROFILE_COMPLETED_QUERIES_NOT_VERIFIED' else 1


def main():
    ap = argparse.ArgumentParser()
    for name in ('codeql','database','source','output'):
        ap.add_argument('--'+name, type=Path, required=True)
    ap.add_argument('--deadline', type=float, required=True)
    ap.add_argument('--analysis-end', type=float, required=True)
    ap.add_argument('--threads', type=int, required=True)
    ap.add_argument('--ram', type=int, required=True)
    args = ap.parse_args()
    if args.threads != 4 or args.ram <= 0:
        raise SystemExit('invalid production resources')
    spec = importlib.util.spec_from_file_location('capacity', Path(__file__).with_name('codeql-rust-capacity.py'))
    capacity = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(capacity)
    return execute(args, capacity)


if __name__ == '__main__':
    raise SystemExit(main())
