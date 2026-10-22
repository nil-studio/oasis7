"""Compile-only scheduling experiment on a private fixed-version library copy."""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import time

import codeql_rust_diagnostic_runtime as runtime

NODE = 'codeql/rust/dataflow/internal/Node.qll'
STAGES = 'codeql/rust/internal/CachedStages.qll'
NODE_SHA = '266a8c647689c26ce3bd2f52f6dddcd7404f7dbb26ca56601cc51ef094a66a64'
STAGES_SHA = 'e9ec32dedd08185b23787efb12879032dc7a7c0cc59db2b48dd49202d54b5754'
OLD = b'cached\nnewtype TNode =\n  TExprNode(Expr e) { e.hasEnclosingCfgScope() and Stages::DataFlowStage::ref() } or'
NEW = b'newtype TNode =\n  TExprNode(Expr e) { e.hasEnclosingCfgScope() } or'


def patched_node(data):
    if hashlib.sha256(data).hexdigest() != NODE_SHA or data.count(OLD) != 1:
        raise RuntimeError('fixed official TNode bytes differ')
    candidate = data.replace(OLD, NEW, 1)
    if candidate.count(NEW) != 1 or candidate.replace(NEW, OLD, 1) != data:
        raise RuntimeError('TNode patch is not exactly reversible')
    return candidate


def precompiled_or_cache(relative):
    return relative.suffix in ('.qlx', '.bqrs') or any(
        part in ('.cache', '.codeql', 'compilation-cache', 'query-cache', 'common-cache')
        for part in relative.parts[:-1])


def supports_dump_ra(text):
    return '--dump-ra' in text or '--[no-]dump-ra' in text


def inventory(closure):
    result = {}
    for name, pack in closure.items():
        root = Path(pack['root'])
        for path in sorted(root.rglob('*')):
            if path.is_symlink() or not (path.is_dir() or path.is_file()):
                raise RuntimeError('nonregular official library input')
            if path.is_file():
                result[name + '/' + str(path.relative_to(root))] = runtime.digest(path)
    return result


def verify_after(closure, before, out, primary):
    try:
        after = inventory(closure)
        runtime.save(out/'official-after.json', after, out, terminal=True)
        if after != before:
            raise RuntimeError('official library contents changed')
    except BaseException as verification_error:
        try:
            runtime.save(out/'official-after-error.json', {'status':'UNKNOWN_OR_CHANGED',
                'error':repr(verification_error)[:2048]}, out, terminal=True)
        except BaseException:
            if primary is None:
                raise verification_error
        if primary is None:
            raise verification_error


def execute(args, env, closure, verify_library, model):
    if args.bundle_version != '2.27.3':
        raise RuntimeError('TNode experiment requires complete official2.27.3')
    out, work = args.output, args.work
    official = Path(closure['codeql/rust-all']['root'])
    before = inventory(closure)
    runtime.save(out / 'terminal-size-admission.json', runtime.admit_terminal_snapshots({}, before), out)
    runtime.save(out / 'official-before.json', before, out)
    primary = None
    try:
        node = (official / NODE).read_bytes()
        stages = (official / STAGES).read_bytes()
        if hashlib.sha256(stages).hexdigest() != STAGES_SHA:
            raise RuntimeError('fixed official CachedStages bytes differ')
        if not re.search(rb'module DataFlowStage\s*\{.*?predicate ref\(\) \{ 1 = 1 \}', stages, re.S):
            raise RuntimeError('DataFlowStage ref is not the fixed constant true')
        patched = patched_node(node)
        copy = work / 'isolated-rust-all'; copy.mkdir(exist_ok=False)
        omitted = []
        for path in sorted(official.rglob('*')):
            relative = path.relative_to(official)
            if path.is_dir():
                (copy / relative).mkdir(exist_ok=True)
            elif precompiled_or_cache(relative):
                omitted.append(str(relative))
            else:
                target = copy / relative; target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(path, target)
        (copy / NODE).write_bytes(patched)
        actual = {str(p.relative_to(copy)): runtime.digest(p) for p in copy.rglob('*') if p.is_file()}
        expected = {str(p.relative_to(official)): runtime.digest(p) for p in official.rglob('*')
                    if p.is_file() and str(p.relative_to(official)) not in omitted}
        if NODE not in actual or STAGES not in actual:
            raise RuntimeError('required source missing from isolated copy')
        expected[NODE] = hashlib.sha256(patched).hexdigest()
        if actual != expected:
            raise RuntimeError('isolated copy contains an unexpected difference')
        runtime.save(out / 'isolated-source.json', {'original_node_sha256': NODE_SHA,
            'candidate_node_sha256': expected[NODE], 'cached_stages_sha256': STAGES_SHA,
            'copied_regular_files': actual, 'omitted_precompiled_or_cache': omitted,
            'changes': ['TNode cached annotation', 'TExprNode constant-true stage conjunct']}, out)
        text = model.diagnostic_queries(args.business_source.resolve())['NodeOnly']
        runtime.save(out / 'query-inputs.json', {'NodeOnly': hashlib.sha256(text.encode()).hexdigest()}, out)
        runtime.run([str(args.codeql), 'query', 'compile', '--help', '-v'], work,
                    out / 'compile-help.log', args.deadline-90, 20, out, env)
        if not supports_dump_ra((out / 'compile-help.log').read_text()):
            raise RuntimeError('actual CLI does not advertise stdout dump-ra')
        if args.deadline-time.monotonic() < 450:
            raise RuntimeError('NOT_REACHED insufficient fixed compile budget')
        compile_end = min(args.deadline-90, time.monotonic()+360)
        for label, root in [('original', official), ('candidate', copy)]:
            selected = {name: dict(pack) for name, pack in closure.items()}
            selected['codeql/rust-all']['root'] = str(root)
            query = out / (label+'-query'); query.mkdir(exist_ok=False)
            runtime.evidence_write(query/'qlpack.yml', b'name: oasis7/rust-model-diagnostic\nversion: 0.0.0\ndependencies:\n  codeql/rust-all: 0.2.24\n', out)
            runtime.evidence_write(query/'NodeOnly.ql', text.encode(), out)
            packs = os.pathsep.join(pack['root'] for pack in selected.values())
            resolved = out/(label+'-library.json')
            runtime.run([str(args.codeql), 'resolve', 'library-path', '--query='+str(query/'NodeOnly.ql'),
                '--format=json', '--additional-packs='+packs], work, resolved, compile_end, 20, out, env)
            verify_library(json.loads(resolved.read_text()), query, 'NodeOnly', selected)
            cache = work/(label+'-compile-cache'); cache.mkdir(exist_ok=False)
            common = work/(label+'-common-cache'); common.mkdir(exist_ok=False)
            runtime.run([str(args.codeql), 'query', 'compile', '--dump-ra', '--additional-packs='+packs,
                '--compilation-cache='+str(cache), '--no-default-compilation-cache', '--common-caches='+str(common),
                '--threads=2', '--ram=3072', str(query/'NodeOnly.ql')], work,
                out/(label+'-ra.log'), compile_end, 360, out, env)
        runtime.save(out/'compile-plan-terminal.json', {'status':'COMPILE_PLAN_REQUIRES_INDEPENDENT_REVIEW',
            'database_downloads':0, 'extractions':0, 'query_evaluations':0, 'runtime_go':False,
            'full_rust_coverage':'HOLD'}, out)
    except BaseException as exc:
        primary = exc
        raise
    finally:
        verify_after(closure, before, out, primary)
