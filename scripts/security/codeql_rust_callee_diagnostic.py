#!/usr/bin/env python3
"""Observe one frozen candidate callee failure without changing its oracle."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import time

import codeql_rust_diagnostic_runtime as runtime

SOURCE = 'f9ab0ca14620f118c84130b3d558e48dc1e6314d'
ADAPTER = 'tests/libp2p-compat/candidate/src/adapter.rs'
NAME = 'with_existing_identity'
CAP = 256 * 1024**2
ROW_CAP = 100


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(file))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def queries(source):
    path = json.dumps(str(source / ADAPTER))
    header = f'''import rust
predicate atCall(Call c) {{
  c.getLocation().getFile().getAbsolutePath() = {path} and
  c.getLocation().getStartLine() = 42
}}
predicate syntactic(Call c) {{
  atCall(c) and c instanceof CallExpr and
  c.(CallExpr).getFunction().toString().matches("%with_existing_identity%")
}}
'''
    syntax = header + '''from CallExpr c
where atCall(c)
select c.getLocation().getStartLine(), c.getLocation().getEndLine(),
  c.getFunction().toString()
'''
    counts = header + '''from string stage, int n
where
  stage = "call-at-line" and n = count(Call c | atCall(c))
  or stage = "syntactic-name" and n = count(Call c | syntactic(c))
  or stage = "resolved-any" and n = count(Call c, Function f |
    syntactic(c) and f = c.getStaticTarget())
  or stage = "resolved-name" and n = count(Call c, Function f |
    syntactic(c) and f = c.getStaticTarget() and
    f.getName().getText() = "with_existing_identity")
  or stage = "target-file" and n = count(Call c, Function f, File p |
    syntactic(c) and f = c.getStaticTarget() and
    f.getName().getText() = "with_existing_identity" and p = f.getLocation().getFile())
  or stage = "function-declaration" and n = count(Function f |
    f.getName().getText() = "with_existing_identity")
select stage, n
'''
    functions = '''import rust
from Function f, string path, string loc
where f.getName().getText() = "with_existing_identity" and
  (path = f.getLocation().getFile().getAbsolutePath() or
   not exists(f.getLocation().getFile().getAbsolutePath()) and path = "NO_FILE") and
  (loc = f.getLocation().toString() or
   not exists(f.getLocation().toString()) and loc = "NO_LOCATION")
select f.getName().getText(), path, loc
'''
    original = load('callee_capacity', 'codeql-rust-capacity.py').semantic_queries(source)['CallsCandidate']
    return {'Syntax': syntax, 'ResolutionCounts': counts,
            'TargetFunctions': functions, 'OriginalCalls': original}


STAGES = {'call-at-line', 'syntactic-name', 'resolved-any', 'resolved-name',
          'target-file', 'function-declaration'}


def classify(rows):
    counts = {}
    for row in rows:
        if (len(row) != 2 or row[0] not in STAGES or row[0] in counts or
                type(row[1]) is not int or row[1] < 0):
            raise RuntimeError('invalid callee stage count')
        counts[row[0]] = row[1]
    if set(counts) != STAGES:
        raise RuntimeError('callee stage counts incomplete')
    if counts['call-at-line'] == 0:
        boundary = 'CALL_AST_NOT_FOUND_AT_LINE'
    elif counts['syntactic-name'] == 0:
        boundary = 'SYNTACTIC_NAME_NOT_MATCHED'
    elif counts['resolved-any'] == 0:
        boundary = 'STATIC_TARGET_NOT_RESOLVED'
    elif counts['resolved-name'] == 0:
        boundary = 'STATIC_TARGET_NAME_DIFFERS'
    elif counts['target-file'] == 0:
        boundary = 'TARGET_LOCATION_MISSING'
    else:
        boundary = 'RESOLUTION_PRESENT_CHECK_ORIGINAL_ROWS'
    return {'counts': counts, 'first_missing_boundary': boundary,
            'root_cause': 'UNKNOWN', 'capacity': 'UNKNOWN', 'full_rust': 'HOLD'}


def run_diagnostic(args):
    out = args.output
    out.mkdir(exist_ok=True)
    args.work.mkdir(exist_ok=True)
    runtime.CAP = CAP
    runtime.WRITE_CAP = CAP - 8 * 1024**2
    end = time.monotonic() + args.deadline - time.time() - 180
    terminal = {'status': 'INCOMPLETE', 'source': SOURCE, 'root_cause': 'UNKNOWN',
                'capacity': 'UNKNOWN', 'full_rust': 'HOLD', 'security_upload': False,
                'database_upload': False,
                'stages': {n: 'NOT_REACHED' for n in ('Syntax', 'ResolutionCounts', 'TargetFunctions', 'OriginalCalls')},
                'action_cleanup': 'UNKNOWN_RUNNER_OWNED'}
    checked = args.check_only
    try:
        if args.threads != 4 or args.ram <= 0 or end <= time.monotonic():
            raise RuntimeError('resource/deadline admission')
        capacity = load('callee_capacity_run', 'codeql-rust-capacity.py')
        identity = capacity.evidence.capture_identity(args.source, out, end, SOURCE, prefix='callee')
        if identity.get('status') != 'PASS':
            raise RuntimeError('frozen source identity failed')
        model = capacity.bundle
        closure = model.bundle_closure(args.codeql.parent, '2.27.3')
        additional = os.pathsep.join(item['root'] for item in closure.values())
        query = args.work / 'callee-queries'
        query.mkdir(exist_ok=True)
        (query / 'qlpack.yml').write_text('name: oasis7/rust-callee-diagnostic\nversion: 0.0.0\ndependencies:\n  codeql/rust-all: 0.2.24\n')
        texts = queries(args.source)
        terminal['stages'] = {name: 'NOT_REACHED' for name in texts}
        runtime.save(out / 'callee-query-inputs.json', texts, out)
        runtime.save(out / 'callee-query-hashes.json',
                     {n: hashlib.sha256(t.encode()).hexdigest() for n, t in texts.items()}, out)
        options = ['--additional-packs=' + additional]
        env = {k: v for k, v in os.environ.items() if k not in ('GH_TOKEN', 'GITHUB_TOKEN')}

        def run(argv, name):
            # BQRS and evaluator spill remain private, not evidence artifacts.
            limit = None if list(map(str, argv[1:3])) == ['query', 'run'] else 8 * 1024**2
            runtime.run(list(map(str, argv)), args.source, out / (name + '.log'), end,
                        min(900, max(0, end - time.monotonic())), out, env, file_limit=limit)

        decoded = {}
        for name, text in texts.items():
            terminal['stages'][name] = 'INCOMPLETE'
            q = query / (name + '.ql')
            q.write_text(text)
            run([args.codeql, 'resolve', 'library-path', '--query=' + str(q),
                 '--format=json', *options], 'callee-library-' + name)
            model.verify_library_path(json.loads((out / ('callee-library-' + name + '.log')).read_text()),
                                      query, name, closure, 'oasis7/rust-callee-diagnostic')
            if checked:
                run([args.codeql, 'query', 'compile', '--check-only', q,
                     '--threads=4', '--ram=' + str(args.ram), *options], 'callee-check-' + name)
            else:
                b = args.work / (name + '.bqrs')
                run([args.codeql, 'query', 'run', q, '--database=' + str(args.database),
                     '--output=' + str(b), '--threads=4', '--ram=' + str(args.ram), *options], 'callee-run-' + name)
                run([args.codeql, 'bqrs', 'info', b, '--format=json'], 'callee-info-' + name)
                info = json.loads((out / ('callee-info-' + name + '.log')).read_text())
                sets = info.get('result-sets')
                if (not isinstance(sets, list) or len(sets) != 1 or
                        sets[0].get('name') != '#select' or type(sets[0].get('rows')) is not int or
                        not 0 <= sets[0]['rows'] <= ROW_CAP):
                    raise RuntimeError('callee BQRS schema or row cap')
                dest = out / ('callee-' + name + '.json')
                run([args.codeql, 'bqrs', 'decode', b, '--format=json', '--output=' + str(dest)], 'callee-decode-' + name)
                from codeql_rust_cfg_graph import decode_rows
                rows = decode_rows(dest, info)
                if any(len(str(row)) > 8192 for row in rows):
                    raise RuntimeError('callee decoded row byte bound')
                decoded[name] = rows
            terminal['stages'][name] = 'PASS'
        if not checked:
            terminal['observation'] = classify(decoded['ResolutionCounts'])
            terminal['original_result_rows'] = len(decoded['OriginalCalls'])
        terminal['status'] = 'QL_CHECKED' if checked else 'DIAGNOSTIC_COMPLETED_NOT_ACCEPTANCE'
    except Exception as exc:
        terminal['error'] = repr(exc)[:2048]
    finally:
        runtime.save(out / ('callee-check-terminal.json' if checked else 'callee-terminal.json'),
                     terminal, out, terminal=True)
    return 0 if terminal['status'] in ('QL_CHECKED', 'DIAGNOSTIC_COMPLETED_NOT_ACCEPTANCE') else 1
