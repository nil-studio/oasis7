#!/usr/bin/env python3
"""Bounded public-callee observations after sealed capacity or extraction evidence."""
import argparse
import copy
import csv
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import shutil
import time

import codeql_rust_diagnostic_runtime as runtime

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('qualifier_capacity', HERE / 'codeql-rust-capacity.py')
capacity = importlib.util.module_from_spec(spec)
spec.loader.exec_module(capacity)

PATCH_SHA = '985c0bdd11650fef89ce56556cb78c5ee9135f17dc030fe309b02eb43354c989'
OFFICIAL_SHA = 'e60563835e6d1dedd77787e0a341da5875c4d7d3c8ae73043105abccdfef6dbc'
CANDIDATE_SHA = '5cec72044c199b520f543bcbc2d204a263ae255efa4c8eeeca3112389c5cfb53'
TYPE_FILE = 'codeql/rust/internal/typeinference/TypeInference.qll'
NAMES = ('CallsCandidate', 'CallsLegacy')
PACK = 'oasis7/rust-qualifier-comparison'
MAX_SECONDS, JOB_RESERVE, AUDIT_RESERVE = 900, 450, 60
RESEARCH_SECONDS, VARIANT_SECONDS = 7200, 3600


def allocation(deadline, now, monotonic, mode='capacity'):
    if mode not in ('capacity', 'qualifier-research'):
        raise RuntimeError('unknown comparison mode')
    research = mode == 'qualifier-research'
    remaining = deadline - now
    if not math.isfinite(remaining) or remaining > (9000 if research else 5400):
        raise RuntimeError('comparison cannot extend the fixed job deadline')
    seconds = min(RESEARCH_SECONDS if research else MAX_SECONDS, max(0, remaining - JOB_RESERVE))
    return seconds, monotonic + seconds


def verify_seal(root, end):
    root = Path(root)
    if root.is_symlink() or not root.is_dir():
        raise RuntimeError('missing or unsafe official evidence')
    paths = list(root.rglob('*'))
    if len(paths) > 8192 or any(p.is_symlink() or not (p.is_file() or p.is_dir()) for p in paths):
        raise RuntimeError('unsafe official evidence inventory')
    files = {str(p.relative_to(root)): p for p in paths if p.is_file()}
    if 'hashes.json' not in files or sum(p.stat().st_size for p in files.values()) > capacity.CAP:
        raise RuntimeError('official evidence seal missing or oversized')
    expected = json.loads(files['hashes.json'].read_text())
    if not isinstance(expected, dict) or set(expected) != set(files) - {'hashes.json'}:
        raise RuntimeError('official evidence inventory differs from seal')
    actual = {name: capacity.digest_until(path, end) for name, path in files.items()}
    if any(actual[name] != digest for name, digest in expected.items()):
        raise RuntimeError('official evidence content differs from seal')
    return actual


def admit(captured, finished, source, deadline, mode='capacity'):
    if mode not in ('capacity', 'qualifier-research'):
        raise RuntimeError('unknown comparison mode')
    for phase, value in [('capture', captured), ('finish', finished)]:
        if (value.get('phase') != phase or value.get('mode') != mode
                or value.get('source') != capacity.CAPACITY_SOURCE
                or value.get('source_root') != str(source)
                or value.get('fixed_job_deadline') != deadline
                or value.get('analyze_outcome') != 'success'):
            raise RuntimeError('official terminal identity differs')
    if (captured.get('status') != 'CAPTURED' or captured.get('evidence_status') != 'COMPLETE'
            or captured.get('cargo_guard', {}).get('status') != 'PASS'
            or captured.get('coverage_capture', {}).get('status') != 'PASS'
            or any(captured.get(k) != 'PASS' for k in
                   ('checkout_gate', 'lock_observation_gate', 'resource_gate'))):
        raise RuntimeError('official capture admission failed')
    if mode == 'qualifier-research':
        if (any(v.get('expected_bundle_version') != '2.27.4' for v in (captured, finished))
                or finished.get('status') != 'EXTRACTION_READY'
                or finished.get('assessment_status') != 'RESEARCH_READY'
                or finished.get('verification_admitted') is not True
                or finished.get('extraction_checkpoint') != 'PASS' or finished.get('archived_source') != 'PASS'
                or finished.get('default_result_verification') != 'NOT_REQUESTED'
                or finished.get('public_semantics') != 'NOT_REQUESTED'
                or any(finished.get('after_identity', {}).get(k) != 'UNCHANGED' for k in ('source', 'official'))):
            raise RuntimeError('official extraction checkpoint did not finish')
        return
    if (finished.get('verification_admitted') is not True
            or finished.get('default_result_verification') != 'PASS'
            or finished.get('public_semantics') not in ('PASS', 'FAIL')):
        raise RuntimeError('official results or public witnesses did not finish')
    tables = set(finished.get('public_semantic_tables', {})) | set(finished.get('public_semantic_failures', {}))
    if tables != set(capacity.semantic_queries(source)):
        raise RuntimeError('official public witness observations are incomplete')


def candidate_closure(official, private):
    candidate = copy.deepcopy(official)
    candidate['codeql/rust-all']['root'] = str(private)
    return candidate


def prepare_candidate(closure, work, out, end, run, *, copy_end):
    """Apply the same fixed behavior patch for the two bounded research consumers."""
    original = Path(closure['codeql/rust-all']['root'])
    patch_file = HERE / 'codeql-rust-omitted-qualifier.patch'
    if runtime.digest(patch_file) != PATCH_SHA or runtime.digest(original / TYPE_FILE) != OFFICIAL_SHA:
        raise RuntimeError('behavior patch or official type-inference identity differs')
    original_files = capacity.official_snapshot(original, end=end)
    if any('symlink' in value for value in original_files.values()):
        raise RuntimeError('private source copy requires regular official files')
    private = work / 'candidate-rust-all'
    def copy_file(src, dest):
        capacity.check_time(copy_end)
        return shutil.copy2(src, dest)
    shutil.copytree(original, private, copy_function=copy_file)
    run(['patch', '--batch', '--fuzz=0', private / TYPE_FILE, patch_file], 'apply-behavior-patch', 20)
    private_files = capacity.official_snapshot(private, end=end)
    changed = [p for p in set(original_files) | set(private_files) if original_files.get(p) != private_files.get(p)]
    if changed != [TYPE_FILE] or runtime.digest(private / TYPE_FILE) != CANDIDATE_SHA:
        raise RuntimeError('candidate contains changes beyond the fixed behavior patch')
    runtime.save(out / 'patch-identity.json', dict(patch_sha256=PATCH_SHA, official_sha256=OFFICIAL_SHA,
                 candidate_sha256=CANDIDATE_SHA, changed_files=changed, official_files=original_files,
                 candidate_files=private_files), out)
    return private


def observation(name, rows, source, provenance):
    # Zero rows is the original defect; it must not prevent the candidate run.
    if not rows:
        return {'contract': 'MISSING', 'rows': []}
    checked = capacity.validate(name, rows, source, provenance)
    return {'contract': 'PASS', 'rows': rows, 'validated': checked}


def compare(observed):
    restored, preserved, unresolved = [], [], []
    for name in NAMES:
        before, after = observed['official'][name], observed['candidate'][name]
        if before['rows'] and before['rows'] != after['rows']:
            raise RuntimeError('candidate lost or changed an existing public target')
        if after['contract'] != 'PASS':
            unresolved.append(name)
        elif before['contract'] == 'MISSING':
            restored.append(name)
        else:
            preserved.append(name)
    return dict(restored=restored, preserved=preserved, unresolved=unresolved)


def preserve_results(results, variant, out, end):
    """Save bounded raw results even when a batch fails; do not decode them."""
    saved = {}
    for name, path in results.items():
        capacity.check_time(end)
        if path.is_symlink():
            raise RuntimeError('unsafe comparison result')
        if not path.is_file():
            saved[name] = {'status': 'NOT_PRODUCED'}
            continue
        size = path.stat().st_size
        if size > 8 * 1024**2:
            raise RuntimeError('comparison BQRS exceeds bounded evidence capacity')
        data = path.read_bytes()
        target = out / (variant + '-' + name + '.bqrs')
        runtime.evidence_write(target, data, out)
        saved[name] = dict(status='SAVED_UNVERIFIED', bytes=size, sha256=hashlib.sha256(data).hexdigest())
    runtime.save(out / (variant + '-bqrs.json'), saved, out)


def main():
    started = time.monotonic()
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('codeql', 'source', 'database', 'official-evidence', 'output', 'work'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--deadline', type=float, required=True)
    parser.add_argument('--mode', choices=('capacity', 'qualifier-research'), default='capacity')
    args = parser.parse_args()
    seconds, end = allocation(args.deadline, time.time(), started, args.mode)
    source, database, official = args.source.resolve(), args.database.resolve(), args.codeql.parent.resolve()
    out, work = args.output.absolute(), args.work.absolute()
    runner = Path(os.environ['RUNNER_TEMP']).resolve()
    for path in (out, work):
        if (path.resolve() != path or not path.is_relative_to(runner) or path == runner
                or any(path.is_relative_to(p) or p.is_relative_to(path)
                       for p in (source, database, official, args.official_evidence.resolve()))):
            raise RuntimeError('comparison paths overlap protected inputs')
    if out.is_relative_to(work) or work.is_relative_to(out):
        raise RuntimeError('comparison output and working data overlap')
    out.mkdir(exist_ok=False)
    runtime.CAP, runtime.WRITE_CAP = 64 * 1024**2, 63 * 1024**2
    terminal = dict(status='INCOMPLETE', mode=args.mode, source=capacity.CAPACITY_SOURCE,
                    fixed_job_deadline=args.deadline, allocated_seconds=seconds,
                    post_research_reserve_seconds=JOB_RESERVE, stages={},
                    evaluator_cache='shared finalized dataset; sequential, not a cold-cache comparison',
                    production_acceptance='UNCHANGED', production_capacity='UNKNOWN', full_rust_coverage='HOLD',
                    database_upload=False, security_upload=False)
    env = {k: v for k, v in os.environ.items() if k not in ('GH_TOKEN', 'GITHUB_TOKEN', 'RUSTC_WRAPPER')}
    seal = before = None

    def save(name, value):
        runtime.save(out / name, value, out)

    def run(argv, name, limit=None):
        remaining = end - AUDIT_RESERVE - time.monotonic()
        if remaining <= 0:
            raise TimeoutError('comparison query deadline exhausted')
        return runtime.run(list(map(str, argv)), source, out / (name + '.log'),
                           end - AUDIT_RESERVE, min(remaining, limit or remaining), out, env, file_limit=None)

    try:
        if seconds < 60:
            terminal['status'] = 'NOT_REACHED'
            terminal['reason'] = 'insufficient time after fixed job reserve'
            return 1
        work.mkdir(exist_ok=False)
        seal = verify_seal(args.official_evidence, end)
        save('official-evidence-before.json', seal)
        captured = json.loads((args.official_evidence / 'capacity-capture-terminal.json').read_text())
        finished = json.loads((args.official_evidence / 'capacity-finish-terminal.json').read_text())
        admit(captured, finished, source, args.deadline, args.mode)
        terminal['official_terminal_status'] = finished['status']
        terminal['official_public_semantics'] = finished['public_semantics']
        terminal['official_default_result_verification'] = finished['default_result_verification']
        terminal['official_evidence_seal_sha256'] = seal['hashes.json']
        before = capacity.after_identity(source, official, args.official_evidence, end=end)
        save('protected-inputs-before.json', before)
        if before.get('source') != 'UNCHANGED' or before.get('official') != 'UNCHANGED':
            raise RuntimeError('frozen source or official library changed')
        run([args.codeql, 'resolve', 'database', database, '--format=json'], 'database-identity', 20)
        db = json.loads((out / 'database-identity.log').read_text())
        terminal['database_metadata_sha256'] = capacity.finalized_database_identity(db, source, database)
        if args.mode == 'qualifier-research' and finished.get('database_metadata_sha256') != terminal['database_metadata_sha256']:
            raise RuntimeError('database metadata differs from extraction checkpoint')
        resources = capacity.resources(env)
        recorded = json.loads((args.official_evidence / 'resources.json').read_text())
        if any(resources[k] != recorded[k] for k in ('CODEQL_RAM', 'CODEQL_THREADS')):
            raise RuntimeError('Action resource selection differs')
        save('resources.json', resources)
        closure = capacity.bundle.bundle_closure(official, '2.27.4')
        if closure != json.loads((args.official_evidence / 'library-closure.json').read_text()):
            raise RuntimeError('official dependency closure differs')
        private = prepare_candidate(closure, work, out, end, run, copy_end=end - AUDIT_RESERVE)
        texts = {n: capacity.semantic_queries(source)[n] for n in NAMES}
        frozen_queries = json.loads((args.official_evidence / 'semantic-query-inputs.json').read_text())
        if any(hashlib.sha256(text.encode()).hexdigest() != frozen_queries[n] for n, text in texts.items()):
            raise RuntimeError('public comparison query differs from official witness')
        save('public-query-inputs.json', texts)
        provenance = json.loads((args.official_evidence / 'libp2p-provenance.json').read_text())
        observed = {}
        for variant, selected in [('official', closure), ('candidate', candidate_closure(closure, private))]:
            observed[variant] = {}
            save('observations.json', observed)
            root = work / variant
            query = root / 'queries'
            query.mkdir(parents=True)
            research = args.mode == 'qualifier-research'
            pack = PACK + '-' + variant if research else PACK
            (query / 'qlpack.yml').write_text('name: ' + pack + '\nversion: 0.0.0\ndependencies:\n  codeql/rust-all: 0.2.25\n')
            options = ['--additional-packs=' + os.pathsep.join(p['root'] for p in selected.values()),
                       '--no-default-compilation-cache', '--compilation-cache=' + str(root / 'compilation-cache'),
                       '--common-caches=' + str(root / 'common-cache')]
            save(variant + '-closure.json', selected)
            results = {}
            for name, text in texts.items():
                terminal['stages'][variant + '-' + name] = 'INCOMPLETE'
                (query / (name + '.ql')).write_text(text)
                results[name] = (database / 'results' / pack if research else root) / (name + '.bqrs')

            def resolve(name):
                stage, q = variant + '-' + name, query / (name + '.ql')
                run([args.codeql, 'resolve', 'library-path', '--query=' + str(q), '--format=json', *options], stage + '-library', 20)
                resolved = json.loads((out / (stage + '-library.log')).read_text())
                capacity.bundle.verify_library_path(resolved, query, name, selected, pack)

            def decode(name):
                stage = variant + '-' + name
                run([args.codeql, 'bqrs', 'decode', results[name], '--format=csv'], stage + '-decode', 20)
                with (out / (stage + '-decode.log')).open() as stream:
                    rows = list(csv.reader(stream))
                if not rows or rows[0] != ['col0', 'col1', 'col2', 'col3'] or len(rows) > 3:
                    raise RuntimeError('public callee result schema or row count differs')
                observed[variant][name] = observation(name, rows[1:], source, provenance)
                terminal['stages'][stage] = 'COMPLETE'
                save('observations.json', observed)

            if research:
                for name in texts: resolve(name)
                batch = dict(status='INCOMPLETE', pack=pack, queries=list(texts))
                terminal.setdefault('batches', {})[variant] = batch
                batch_start = time.monotonic()
                failed = False
                try:
                    run([args.codeql, 'database', 'run-queries', database, *[query / (n + '.ql') for n in texts],
                         '--threads=' + str(resources['CODEQL_THREADS']), '--ram=' + str(resources['CODEQL_RAM']),
                         *options], variant + '-batch', VARIANT_SECONDS)
                    batch['status'] = 'COMPUTED'
                except Exception:
                    failed = True
                    raise
                finally:
                    batch['elapsed_seconds'] = time.monotonic() - batch_start
                    try:
                        preserve_results(results, variant, out, end)
                    except Exception as error:
                        if not failed: raise
                        terminal.setdefault('result_capture_errors', []).append(str(error))
                for name in texts: decode(name)
                batch['status'] = 'COMPLETE'
            else:
                # Preserve the existing capacity add-on's four sequential commands and budget.
                for name in texts:
                    resolve(name)
                    run([args.codeql, 'query', 'run', query / (name + '.ql'), '--database=' + str(database),
                         '--output=' + str(results[name]), '--threads=' + str(resources['CODEQL_THREADS']),
                         '--ram=' + str(resources['CODEQL_RAM']), *options], variant + '-' + name + '-run')
                    decode(name)
        terminal['comparison'] = compare(observed)
        terminal['status'] = 'COMPARISON_COMPLETED'
    except Exception as error:
        terminal.update(status='INCOMPLETE', error_type=type(error).__name__, error=str(error))
    finally:
        try:
            if before is not None:
                after = capacity.after_identity(source, official, args.official_evidence, end=end)
                save('protected-inputs-after.json', after)
                if after != before:
                    raise RuntimeError('research changed source or official library inputs')
                if runtime.digest(database / 'codeql-database.yml') != terminal.get('database_metadata_sha256'):
                    raise RuntimeError('research database metadata changed or was not admitted')
            if seal is not None and verify_seal(args.official_evidence, end) != seal:
                raise RuntimeError('research changed sealed official evidence')
        except Exception as error:
            terminal.update(status='INCOMPLETE', integrity_error=str(error))
        terminal['elapsed_seconds'] = time.monotonic() - started
        terminal['remaining_job_seconds'] = args.deadline - time.time()
        runtime.save(out / 'comparison-terminal.json', terminal, out, terminal=True)
        hashes = {str(p.relative_to(out)): runtime.digest(p) for p in sorted(out.rglob('*')) if p.is_file()}
        runtime.save(out / 'hashes.json', hashes, out, terminal=True)
    return 0 if terminal['status'] == 'COMPARISON_COMPLETED' and not terminal['comparison']['unresolved'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
