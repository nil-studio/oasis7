#!/usr/bin/env python3
"""Bounded official default-suite profiling; no result decoding or upload."""
import argparse
import importlib.util
import json
import math
import heapq
import subprocess
import sys
import os
from pathlib import Path
import time
import codeql_rust_diagnostic_runtime as runtime

CAP = 256 * 1024**2
SUMMARY_RESERVE = 64 * 1024**2
SUMMARY_OUTPUT_BUDGET = 8 * 1024**2


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
        if (out/'profile-query-terminal.json').exists():
            raise ValueError('query terminal already exists; no repeated evaluation')
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
        terminal['summaries'] = 'NOT_REACHED_PRIORITY_CLOSEOUT'
        complete = terminal['query_status'] == 'COMPLETED_NOT_VERIFIED' and terminal['evaluator_log']['status'] == 'COMPLETE'
        terminal['status'] = 'EVALUATION_COMPLETED_AWAITING_SUMMARY' if complete else 'INCOMPLETE'
    except Exception as exc:
        terminal['status'] = 'INCOMPLETE'
        terminal['error'] = repr(exc)[:2048]
        terminal['evaluator_log'] = log_completeness(evaluator)
    finally:
        runtime.WRITE_CAP = CAP - 8 * 1024**2
        if not (out/'profile-query-terminal.json').exists():
            runtime.save(out/'profile-query-terminal.json', terminal, out, terminal=True)
        runtime.save(out/'profile-terminal.json', terminal, out, terminal=True)
    return 0 if terminal['status'] == 'EVALUATION_COMPLETED_AWAITING_SUMMARY' else 1


def observed_summary_shape(value, signatures):
    types={k:type(v).__name__ for k,v in value.items()}
    for signature in signatures:
        if types!=signature['types']:
            continue
        if all(json.dumps(value.get(field,'ABSENT')) in signature[field+'_counts']
               for field in ('isCached','completionType','evaluationStrategy')):
            return True
    return False


def project_summary(stream, record_limit=32*1024**2, total_limit=256*1024**2, state=None):
    """Project observed CodeQL 2.27.3 summary shapes, never the raw event schema."""
    used = count = markers = untimed = incomplete = timed = 0
    seen={}
    shared_hashes=0
    tops={'official_millis':[],'known_iteration_subtotal':[]}
    schema_path=Path(__file__).with_name('codeql_summary_schema_2_27_3.json')
    if schema_path.stat().st_size>256*1024:raise ValueError('schema fixture bound')
    signatures=json.loads(schema_path.read_text())['signatures']
    if not 0<len(signatures)<=64:raise ValueError('schema signature bound')
    header = None
    allowed_types = {'appearsAs':dict,'completionTime':str,'completionType':str,'dependencies':dict,
        'evaluationStrategy':str,'isCached':bool,'millis':int,'position':str,'predicateName':str,
        'queryCausingWork':str,'raHash':str,'resultSize':int,'ra':dict,'pipelineRuns':list,
        'predicateIterationMillis':list,'deltaSizes':list,'mainHash':str,'layerSize':int,'sentinelRaHash':str}
    strategies = {'SENTINEL_EMPTY','CACHACA','EXTENSIONAL','EXTENSIBLE_PREDICATE','COMPUTE_SIMPLE',
                  'COMPUTED_EXTENSIONAL','COMPUTE_RECURSIVE','IN_LAYER','NAMED_LOCAL'}
    while True:
        line=stream.readline(record_limit+1)
        if not line:break
        used+=len(line)
        if state is not None:state['used']=used
        if len(line)>record_limit or used>total_limit:raise ValueError('official summary input bound exceeded')
        if not line.strip():continue
        v=json.loads(line)
        if not isinstance(v,dict):raise ValueError('official summary record is not an object')
        if set(v)=={'summaryLogVersion','codeqlVersion','startTime'}:
            if header is not None or count or v['codeqlVersion']!='2.27.3' or any(not isinstance(v[k],str) or not v[k] or len(v[k].encode())>128 for k in v):
                raise ValueError('unexpected official summary header')
            header=v;continue
        if header is None:raise ValueError('missing official summary header')
        if not observed_summary_shape(v,signatures):raise ValueError('unobserved official summary shape')
        if any(k not in allowed_types or type(value) is not allowed_types[k] for k,value in v.items()):
            raise ValueError('unknown official summary field/type')
        for key in ('raHash','predicateName','evaluationStrategy'):
            if not isinstance(v.get(key),str) or not v[key] or len(v[key].encode())>8192:
                raise ValueError('unknown official summary identity schema '+key)
        if v['evaluationStrategy'] not in strategies or 'isCached' in v and v['isCached'] is not True:
            raise ValueError('unobserved strategy/cache flag')
        key=v['raHash']
        strategy=v['evaluationStrategy']
        role='metadata' if strategy in ('SENTINEL_EMPTY','CACHACA') else 'timing'
        if count>=100000:raise ValueError('excessive physical summary records')
        if key in seen:
            name,roles,strategies_seen=seen[key]
            if name!=v['predicateName'] or role in roles or strategies_seen|{strategy}!={'SENTINEL_EMPTY','IN_LAYER'}:
                raise ValueError('duplicate or conflicting summary role identity')
            roles.add(role);shared_hashes+=1
        else:seen[key]=(v['predicateName'],{role},{strategy})
        count+=1
        if strategy in ('SENTINEL_EMPTY','CACHACA'):
            expected={'appearsAs','completionTime','evaluationStrategy','predicateName','raHash'}
            expected.add('sentinelRaHash' if strategy=='SENTINEL_EMPTY' else 'resultSize')
            if 'isCached' in v:expected.add('isCached')
            if set(v)!=expected:raise ValueError('unobserved untimed metadata shape')
            untimed+=1;continue
        if ('completionTime' in v)==('completionType' in v) or 'completionType' in v and v['completionType']!='INCOMPLETE':
            raise ValueError('unobserved completion marker')
        is_incomplete=v.get('completionType')=='INCOMPLETE'
        incomplete+=is_incomplete;timed+=1
        millis=v.get('millis')
        times=v.get('predicateIterationMillis')
        if millis is not None and not 0<=millis<=2**63-1:raise ValueError('invalid official millis')
        if times is not None and any(type(n) is not int or n < -1 or n > 2**63-1 for n in times):
            raise ValueError('unknown official iteration timing schema')
        if millis is None and times is None:raise ValueError('missing reported timing')
        known=sum(n for n in (times or []) if n>=0);missing=sum(n==-1 for n in (times or []));markers+=missing
        if known>2**63-1:raise ValueError('summary timing overflow')
        score=millis if millis is not None else known
        position=v.get('position')
        if position is not None and len(position.encode())>8192:raise ValueError('summary source position bound')
        category='recursive_layer' if strategy=='COMPUTE_RECURSIVE' else 'layer_member' if strategy in ('IN_LAYER','NAMED_LOCAL') else 'nonrecursive'
        row=dict(raHash=key,predicateName=v['predicateName'],evaluationStrategy=strategy,
            category=category,position=position,completionType=v.get('completionType'),has_completion_time='completionTime' in v,
            reported_millis=millis,known_iteration_millis=known if millis is None and times is not None else None,
            negative_marker_entries=missing,metric_kind='official_millis' if millis is not None else 'known_iteration_subtotal')
        if 'mainHash' in v:row['mainHash']=v['mainHash']
        top=tops[row['metric_kind']]
        heapq.heappush(top,(score,key,row))
        if len(top)>50:heapq.heappop(top)
    if not any(tops.values()):raise ValueError('no reported timing records in official summary')
    return dict(status='PROJECTED_OFFICIAL_REPORTED_COSTS',header=header,records=count,
        metadata_timing_shared_hashes=shared_hashes,untimed_metadata_records=untimed,timing_records=timed,incomplete_timing_records=incomplete,
        input_bytes=used,negative_marker_entries=markers,rankings={kind:[v[2] for v in sorted(values,reverse=True)] for kind,values in tops.items()},
        limits='Literal official millis and nonnegative predicateIterationMillis subtotals are ranked separately, never mixed. Untimed metadata is not assigned zero. -1 markers are preserved without interpretation. Layers include members and categories must not be added together or read as CPU. Raw input completeness remains separate; no cross-run hash joins.')


def encoded_projection(report):
    data=(json.dumps(report,separators=(',',':'))+'\n').encode()
    if len(data)>1024**2:
        raise ValueError('projection output exceeds 1MiB')
    return data


def project_cli():
    ap=argparse.ArgumentParser()
    ap.add_argument('--project-summary',action='store_true',required=True)
    ap.add_argument('--codeql',type=Path,required=True)
    ap.add_argument('--input',type=Path,required=True)
    args=ap.parse_args()
    proc=subprocess.Popen([str(args.codeql),'generate','log-summary','--format=predicates','--minify-output',str(args.input)],stdout=subprocess.PIPE)
    state={}
    try:
        report=project_summary(proc.stdout,state=state)
        rc=proc.wait()
        if rc:
            raise ValueError('official summary exited '+str(rc))
        sys.stdout.buffer.write(encoded_projection(report))
        sys.stdout.buffer.flush()
        return 0
    except Exception as exc:
        # Drain only the remaining bounded output so the launcher can reap Java.
        remaining=max(0,256*1024**2-state.get('used',0))
        while remaining:
            chunk=proc.stdout.read(min(65536,remaining))
            if not chunk:break
            remaining-=len(chunk)
        proc.stdout.close()
        proc.wait()
        print(json.dumps({'status':'PARTIAL_PROJECTION','error':repr(exc)[:1024]}))
        return 1
    finally:
        proc.stdout.close()


def bounded_summary(cli, evaluator, source, out, end, env, failure=None):
    """One projected official predicates summary; never consume core-evidence space."""
    if failure is not None and not failure.clean_timeout:
        return {'status':'NOT_RUN_UNSAFE_FAILURE'}
    previous = runtime.WRITE_CAP
    try:
        if (out/'official-evaluator-summary-projection.json').exists():
            return {'status':'NOT_RUN_EXISTING_OUTPUT'}
        used = runtime.output_bytes(out)
        runtime.WRITE_CAP = min(previous, used + SUMMARY_OUTPUT_BUDGET - runtime.MAX_PROCESS_RECEIPT)
        if runtime.WRITE_CAP - used < runtime.MAX_PROCESS_RECEIPT or end-time.monotonic() < 5:
            return {'status':'NOT_REACHED_CLOSEOUT_BUDGET'}
        runtime.run([sys.executable,str(Path(__file__).resolve()),'--project-summary',
                     '--codeql',str(cli),'--input',str(evaluator)], source,
                    out/'official-evaluator-summary-projection.json',end,20,out,env,file_limit=None)
        return {'status':'GENERATED_PROJECTED_NOT_EVALUATION_COMPLETION'}
    except runtime.PhaseError as exc:
        return {'status':'PARTIAL','failure':exc.result}
    finally:
        runtime.WRITE_CAP = previous


def summarize(args):
    out = args.output
    runtime.CAP, runtime.WRITE_CAP = CAP, CAP-8*1024**2
    if not (out/'profile-query-terminal.json').is_file():
        runtime.save(out/'profile-summary-terminal.json',{'status':'NOT_REACHED_QUERY_TERMINAL_MISSING'},out,terminal=True)
        return 1
    try:
        terminal = json.loads((out/'profile-query-terminal.json').read_text())
        if not isinstance(terminal,dict):
            raise ValueError('invalid query terminal shape')
    except (ValueError,OSError) as exc:
        runtime.save(out/'profile-summary-terminal.json',{'status':'NOT_REACHED_INVALID_QUERY_TERMINAL','error':repr(exc)[:1024]},out,terminal=True)
        return 1
    terminal['query_terminal_sha256'] = runtime.digest(out/'profile-query-terminal.json')
    terminal['summary_incremental_budget_bytes'] = SUMMARY_OUTPUT_BUDGET
    try:
        receipt = json.loads((out/'profile-query.log.result.json').read_text())
        argv = receipt.get('argv')
        expected = [str(args.codeql.resolve()),'database','run-queries',str(args.database),str(args.database/'temp/config-queries.qls'),
                    '--threads='+str(args.threads),'--ram='+str(args.ram),'--evaluator-log',str(out/'official-evaluator.fifo'),
                    '--evaluator-log-minify','--no-tuple-counting']
        times_valid = all(type(receipt.get(k)) in (int,float) and math.isfinite(receipt[k]) for k in ('started_wall','elapsed'))
        if not times_valid or not 0 <= receipt['started_wall'] <= args.analysis_end or not 0 <= receipt['elapsed'] <= args.deadline-receipt['started_wall']:
            raise ValueError('invalid query timing receipt')
        if (terminal.get('fixed_job_deadline') != args.deadline or terminal.get('fixed_analysis_end') != args.analysis_end or
            argv != expected or
            receipt.get('cwd') != str(args.source) or receipt.get('cleanup_errors') != [] or receipt.get('evidence_errors') != [] or
            type(receipt.get('pid')) is not int or receipt['pid']<=0 or receipt.get('pgid')!=receipt['pid']):
            raise ValueError('query receipt identity or cleanup is not admitted')
        success = terminal.get('query_status')=='COMPLETED_NOT_VERIFIED' and receipt.get('exit')==0 and not receipt.get('error') and 'failure' not in terminal
        clean_timeout = (terminal.get('query_status')=='INCOMPLETE' and terminal.get('failure')==receipt and
                         runtime.PhaseError(receipt).clean_timeout and isinstance(receipt.get('pid'),int) and receipt['pid']>0)
        if not success and not clean_timeout:
            raise ValueError('query outcome does not admit summary')
        core = json.loads((out/'capacity-capture-terminal.json').read_text())
        identity = json.loads((out/'profile-after-identity.json').read_text())
        if any(core.get(key) != 'PASS' for key in ('checkout_gate','resource_gate','lock_observation_gate','core_receipt_gate')) or any(identity.get(key) != 'UNCHANGED' for key in ('source','official')):
            terminal['summaries'] = {'status':'NOT_RUN_CORE_EVIDENCE_INCOMPLETE'}
        else:
            evaluator = out/'official-evaluator.jsonl'
            if evaluator.is_symlink() or runtime.digest(evaluator) != terminal['evaluator_log']['sha256']:
                raise ValueError('raw profile differs from frozen query receipt')
            failure = runtime.PhaseError(terminal['failure']) if 'failure' in terminal else None
            end = min(time.monotonic()+30,time.monotonic()+args.deadline-time.time()-300)
            terminal['summaries'] = bounded_summary(args.codeql.resolve(),evaluator,args.source,out,end,dict(os.environ),failure)
        complete = (terminal['query_status']=='COMPLETED_NOT_VERIFIED' and terminal['evaluator_log']['status']=='COMPLETE' and terminal['summaries'].get('status')=='GENERATED_PROJECTED_NOT_EVALUATION_COMPLETION')
        terminal['status'] = 'PROFILE_COMPLETED_QUERIES_NOT_VERIFIED' if complete else 'INCOMPLETE'
    except Exception as exc:
        terminal['status']='INCOMPLETE'
        terminal['summary_error']=repr(exc)[:2048]
    runtime.save(out/'profile-summary-terminal.json',{'status':terminal['status'],'summary':terminal.get('summaries'),'error':terminal.get('summary_error'),'query_terminal_sha256':terminal['query_terminal_sha256']},out,terminal=True)
    runtime.save(out/'profile-terminal.json',terminal,out,terminal=True)
    return 0 if terminal['status']=='PROFILE_COMPLETED_QUERIES_NOT_VERIFIED' else 1


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--phase', choices=('evaluate','summarize'),required=True)
    for name in ('codeql','database','source','output'):
        ap.add_argument('--'+name, type=Path, required=True)
    ap.add_argument('--deadline', type=float, required=True)
    ap.add_argument('--analysis-end', type=float, required=True)
    ap.add_argument('--threads', type=int, required=True)
    ap.add_argument('--ram', type=int, required=True)
    args = ap.parse_args()
    if args.threads != 4 or args.ram <= 0:
        raise SystemExit('invalid production resources')
    if args.phase == 'summarize':
        return summarize(args)
    spec = importlib.util.spec_from_file_location('capacity', Path(__file__).with_name('codeql-rust-capacity.py'))
    capacity = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(capacity)
    return execute(args, capacity)


if __name__ == '__main__':
    raise SystemExit(project_cli() if '--project-summary' in sys.argv else main())
