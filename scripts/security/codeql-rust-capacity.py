#!/usr/bin/env python3
"""One manual production-sized Rust assessment; never uploads security results."""
import argparse
import csv
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import time
import zipfile
import codeql_rust_diagnostic_runtime as runtime
import codeql_rust_capacity_evidence as evidence
import codeql_rust_cargo_guard as cargo_guard

BASE_SOURCE='d2bf383d8c377b36de83aae4a06d9ec17612e205'
EXCLUSION_SOURCE='c036927d33b917a036a731f071209e16f6deb033'
SOURCE=EXCLUSION_SOURCE
BUNDLE='2.27.3'
CAP=256*1024**2
EXCLUDES=('third_party/','target/','node_modules/','dist/','.pm/')

def load(name,file):
    spec=importlib.util.spec_from_file_location(name,Path(__file__).with_name(file))
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module

plan=load('capacity_plan','codeql-plan.py')
model=load('capacity_model','codeql-rust-refactored-source.py')
bundle=load('capacity_bundle','codeql-rust-model-diagnostic.py')
native=load('capacity_native','codeql-rust-cargo-check.py')

def check_time(end):
    if end is not None and time.monotonic()>=end:raise TimeoutError('NOT_REACHED fixed evidence deadline reserve')


def digest_until(path,end=None):
    if end is None:return runtime.digest(path)
    digest=hashlib.sha256()
    with path.open('rb') as stream:
        while True:
            check_time(end)
            data=stream.read(1024**2)
            if not data:break
            digest.update(data)
    return digest.hexdigest()


def snapshot(source, selected=None, *, end=None):
    check_time(end)
    if selected is None:
        entries=subprocess.check_output(['git','ls-files','--stage','-z'],cwd=source).decode().split('\0')
        selected={}
        for entry in filter(None,entries):
            metadata,name=entry.split('\t',1);mode,oid,stage=metadata.split()
            if stage!='0' or name in selected:raise RuntimeError('unmerged/duplicate source index')
            selected[name]={'mode':mode,'gitlink':oid} if mode=='160000' else {}
        strict=True
    else:strict=False
    result={}
    for name,identity in selected.items():
        check_time(end)
        if 'gitlink' in identity:
            # Mode/OID are source-index facts; excluded submodule contents are never inspected.
            result[name]={'mode':'160000','gitlink':identity['gitlink']};continue
        path=source/name
        if path.is_symlink():result[name]={'symlink':os.readlink(path)}
        elif path.is_file():result[name]={'sha256':digest_until(path,end),'bytes':path.stat().st_size}
        elif strict:raise RuntimeError('missing ordinary tracked source: '+name)
        else:result[name]={'missing':True}
    return result


def official_snapshot(root, selected=None, *, end=None):
    check_time(end)
    result={}
    paths=sorted(root.rglob('*')) if selected is None else [root/name for name in sorted(selected)]
    for path in paths:
        check_time(end)
        relative=str(path.relative_to(root))
        if path.is_symlink():result[relative]={'symlink':os.readlink(path)}
        elif path.is_file():result[relative]={'sha256':digest_until(path,end),'mode':path.stat().st_mode&0o111}
    return result


def after_identity(source,official,out,*,end=None):
    check_time(end)
    result={}
    before=out/'source-before.json'
    frozen_source=json.loads(before.read_text()) if before.is_file() else None
    blobs={name:value for name,value in (frozen_source or {}).items() if 'gitlink' not in value}
    result['source']='NOT_FROZEN' if frozen_source is None else ('UNCHANGED' if blobs==snapshot(source,blobs,end=end) else 'CHANGED')
    result['source_scope']='frozen non-gitlink regular/symlink inputs only'
    result['gitlinks']={name:dict(value,payload='NOT_INSPECTED',finally_oid_check='NOT_RECHECKED_WITHOUT_SUBPROCESS') for name,value in (frozen_source or {}).items() if 'gitlink' in value}
    before=out/'official-before.json'
    if not before.is_file():result['official']='NOT_FROZEN'
    else:
        frozen=json.loads(before.read_text())
        result['official']='UNCHANGED' if official_snapshot(official,frozen,end=end)==frozen else 'CHANGED'
        # New compiler/cache artifacts are distinguished from every frozen official input.
        additional=[]
        for path in official.rglob('*'):
            check_time(end)
            if (path.is_file() or path.is_symlink()) and str(path.relative_to(official)) not in frozen:additional.append(str(path.relative_to(official)))
        result['additional_official_paths']=sorted(additional)
    return result


def resources(env):
    result={}
    for key in ('CODEQL_RAM','CODEQL_THREADS'):
        if not re.fullmatch(r'[1-9][0-9]*',env.get(key,'')):raise RuntimeError('missing Action-selected '+key)
        result[key]=int(env[key])
    result['selection']='pinned production init Action exported values; no helper resource calculation'
    result['safe_environment']={k:env.get(k) for k in ('CODEQL_RAM','CODEQL_THREADS','CODEQL_OVERLAY_DATABASE_MODE','CODEQL_ACTION_SCALING_RESERVED_RAM_PERCENTAGE')}
    result['runner_memory_bytes']=os.sysconf('SC_PHYS_PAGES')*os.sysconf('SC_PAGE_SIZE')
    result['runner_cpu_count']=os.cpu_count()
    result['cgroups']={str(p):p.read_text() for p in map(Path,('/sys/fs/cgroup/memory.max','/sys/fs/cgroup/cpu.max','/sys/fs/cgroup/cpuset.cpus.effective')) if p.is_file()}
    return result


def verify_action_config(config,source,database):
    if config.get('dbLocation')!=str(database.parent) or config.get('repositoryRoot')!=str(source) or config.get('languages')!=['rust'] or config.get('buildMode')!='none' or config.get('overlayDatabaseMode')!='none' or config.get('trapCaches') or config.get('useOverlayDatabaseCaching'):
        raise RuntimeError('actual Action source/DB/cache configuration differs')


def suite_entries(value,root):
    if not isinstance(value,list) or not value or not all(isinstance(x,str) for x in value) or len(value)!=len(set(value)):
        raise RuntimeError('unknown resolved default suite schema')
    result=[];ids=set()
    for item in sorted(value):
        path=Path(item).resolve()
        if not path.is_relative_to(root.resolve()) or path.suffix!='.ql' or not path.is_file():raise RuntimeError('foreign default query')
        qids=re.findall(r'@id\s+([^\s*]+)',path.read_text())
        if len(qids)!=1 or qids[0] in ids:raise RuntimeError('default query identity ambiguous')
        ids.add(qids[0]);result.append({'relative':str(path.relative_to(root.resolve())),'id':qids[0],'sha256':runtime.digest(path)})
    return result


def semantic_queries(root):
    result=model.queries(root)
    result['PeerLegacy']=result['PeerCandidate'].replace('"candidate"','"legacy"')
    result['CallsLegacy']=result['CallsCandidate'].replace('"candidate"','"legacy"')
    image=model.metrics_query(root,'''string image() { result = "'''+model.EXPECT+'''" }
string labels() { result = ["format-original","format-replacement","original-flow","replacement-flow","contains-line-callee","replacement-line-callee","whole-line-flow","output-line-flow","wrong-function-flow"] }
predicate imageFlow(int sourceLine, int macroLine) {
 exists(DataFlow::Node a,DataFlow::Node b | lineAt(a.asParameter(),image(),sourceLine) and
  (sourceLine = 12 and a.asParameter().toString() = "original_digest" or sourceLine = 22 and a.asParameter().toString() = "replacement_digest") and
  inExpansion(b.asExpr(),image(),macroLine) and DataFlow::localFlow(a,b))
}
predicate witness(string label) {
 label = "format-original" and exists(MacroCall m,AstNode n | macroAt(m,image(),13) and m.getMacroCallExpansion()=n.getParentNode*())
 or label = "format-replacement" and exists(MacroCall m,AstNode n | macroAt(m,image(),23) and m.getMacroCallExpansion()=n.getParentNode*())
 or label = "original-flow" and imageFlow(12,13)
 or label = "replacement-flow" and imageFlow(22,23)
 or label = "contains-line-callee" and exists(Call c | lineAt(c,image(),14) and c.getTargetName()="contains_line" and lineAt(c.getStaticTarget(),image(),2))
 or label = "replacement-line-callee" and exists(Call c | lineAt(c,image(),24) and c.getTargetName()="contains_line" and lineAt(c.getStaticTarget(),image(),2))
 or label = "output-line-flow" and exists(DataFlow::Node a,DataFlow::Node b | lineAt(a.asParameter(),image(),2) and a.asParameter().toString()="output" and lineAt(b.asExpr(),image(),4) and b.asExpr().toString()="line == expected" and DataFlow::localFlow(a,b))
 or label = "whole-line-flow" and exists(DataFlow::Node a,DataFlow::Node b | lineAt(a.asParameter(),image(),2) and a.asParameter().toString()="expected" and lineAt(b.asExpr(),image(),4) and b.asExpr().toString()="line == expected" and DataFlow::localFlow(a,b))
 or label = "wrong-function-flow" and imageFlow(12,23)
}
''')
    result['Image']=image
    return result


def expected(name):
    if name in ('PeerCandidate','PeerLegacy'):return model.EXPECTED['Peer']
    if name=='Driver':return model.EXPECTED['Driver']
    if name=='Image':return {k:(0 if k=='wrong-function-flow' else 1) for k in ('format-original','format-replacement','original-flow','replacement-flow','contains-line-callee','replacement-line-callee','whole-line-flow','output-line-flow','wrong-function-flow')}
    return None


def validate(name,rows,source,libp2p):
    wanted=expected(name)
    if wanted is not None:
        if any(len(row)!=2 or row[1] not in ('0','1') for row in rows):raise RuntimeError('invalid semantic rows')
        values={row[0]:int(row[1]) for row in rows}
        if len(values)!=len(rows) or values!=wanted:raise RuntimeError(name+' semantic positive/negative mismatch')
        return values
    if name=='Files':
        files={str(source/p) for p in (model.DRIVER,model.BASE+'/common/src/protocol.rs',model.EXPECT,model.BASE+'/candidate/src/adapter.rs',model.BASE+'/legacy/src/adapter.rs')}
        model.check_files(rows,files);return {'files':sorted(files)}
    context='candidate' if name=='CallsCandidate' else 'legacy'
    if len(rows)!=1 or len(rows[0])!=4:raise RuntimeError('callee target row missing/duplicate')
    row=rows[0]
    if row[:3]!=[str(source/model.BASE/context/'src/adapter.rs'),'42','with_existing_identity'] or not Path(row[3]).is_relative_to(Path(libp2p[context])):
        raise RuntimeError('wrong version-specific callee provenance')
    return {'source':row[0],'target':row[3]}


def save(path,value,out):runtime.save(path,value,out)

def check_sarif(value):
    if value.get('version')!='2.1.0' or not value.get('runs'):raise RuntimeError('missing complete local SARIF')
    for run in value['runs']:
        if not run.get('invocations'):raise RuntimeError('missing SARIF execution record')
        for invocation in run['invocations']:
            if invocation.get('executionSuccessful') is not True:raise RuntimeError('default query invocation failed')
            for notice in invocation.get('toolExecutionNotifications',[]):
                if notice.get('level')=='error':raise RuntimeError('default query execution error')
    return sum(len(run.get('results',[])) for run in value['runs'])


def verify_decoded(info,decoded):
    sets=info.get('result-sets') if isinstance(info,dict) else None
    if not isinstance(sets,list) or not sets or not isinstance(decoded,dict):raise RuntimeError('unknown BQRS schemas')
    names=[item.get('name') for item in sets]
    if len(names)!=len(set(names)) or set(names)!=set(decoded):raise RuntimeError('result-set collection differs')
    for item in sets:
        value=decoded[item['name']]
        if type(item.get('rows')) is not int or not isinstance(value,dict) or not isinstance(value.get('tuples'),list) or len(value['tuples'])!=item['rows']:
            raise RuntimeError('decoded result incomplete')


def archive_identity(database,selected,before):
    # Read selected small source bytes, without importing the old probe's aggregate data limit.
    with zipfile.ZipFile(database/'src.zip') as archive:
        entries=archive.infolist();observations={}
        for name in selected:
            matches=[item for item in entries if item.filename.endswith('/'+name)]
            if len(matches)!=1 or matches[0].file_size>8*1024**2:raise RuntimeError('archive source ambiguous/missing/oversize '+name)
            import hashlib
            actual=hashlib.sha256(archive.read(matches[0])).hexdigest()
            if actual!=before[name]:raise RuntimeError('archive source SHA differs '+name)
            observations[name]={'sha256':actual,'archive_path':matches[0].filename}
        sysroots=sorted({match[1] for item in entries for match in [re.search(r'(?:^|/)toolchains/([^/]+)/lib/rustlib/src/rust/library/',item.filename)] if match})
        if not sysroots:raise RuntimeError('analysis sysroot archive identity absent')
        return {'selected':observations,'entry_count':len(entries),'expanded_bytes':sum(item.file_size for item in entries),'analysis_sysroots':sysroots}


def captured_segments(path,item,*,end=None):
    """Never join across an omitted source interval or parse a cut line."""
    if item.get("encoding") == "gzip":
        return [evidence.CompressedLogText(path,item,end if end is not None else float("inf"))]
    if item.get('encoding') not in (None,'raw'):raise RuntimeError('unknown log encoding')
    if path.stat().st_size>evidence.MAX_FILE_BYTES:raise RuntimeError('raw retained log size bound')
    data=path.read_bytes(); texts=[]
    ranges=item.get('source_ranges',[{'source_offset':0,'destination_offset':0,'length':len(data)}])
    for segment in ranges:
        start=segment['destination_offset']; length=segment['length']
        value=data[start:start+length]
        if segment['source_offset']:
            value=value.partition(b'\n')[2]
        # An unterminated last line cannot prove a complete progress marker.
        if not value.endswith(b'\n'):value=value.rpartition(b'\n')[0]
        texts.append(value.decode(errors='replace'))
    return texts


def query_progress(texts,*,end=None):
    started=set();completed=set()
    for text in texts:
        check_time(end)
        for line in (text.splitlines() if isinstance(text,str) else text):
            check_time(end)
            match=re.search(r'Starting evaluation of (\S+\.ql)\.',line)
            if match:
                if len(match[1].encode())>4096:raise RuntimeError('query progress path bound')
                started.add(match[1])
                if len(started)>512:raise RuntimeError('query progress count bound')
            match=re.search(r'Evaluation done; writing results to (\S+\.bqrs)\.',line)
            if match:
                if len(match[1].encode())>4096:raise RuntimeError('query progress path bound')
                completed.add(match[1])
                if len(completed)>512:raise RuntimeError('query progress count bound')
    return {'scope':'unique markers in complete lines of retained ranges only; omitted ranges UNKNOWN; not decoded results',
            'started':sorted(started),'completed':sorted(completed),
            'started_count':len(started),'completed_count':len(completed),
            'full_query_results':'NOT_VERIFIED'}


def coverage_report(source,inventory,texts,*,end=None,logs_complete=True):
    found=set();diagnostics=set();enumeration_complete=logs_complete;sections=0
    for text in texts:
        check_time(end)
        section=False
        for raw in (text.splitlines() if isinstance(text,str) else text):
            check_time(end)
            line=re.sub(r'\x1b\[[0-9;]*m','',raw)
            if 'found manifests:' in line:
                if section:enumeration_complete=False
                section=True;sections+=1;continue
            if section:
                # One linear search: repeated source prefixes in malformed long
                # lines must not cause the old unanchored regex to backtrack.
                offset=line.find('  '+str(source)+'/')
                candidate=line[offset+2:].rstrip() if offset>=0 else ''
                if candidate.endswith('/Cargo.toml'):
                    if len(candidate.encode())>4096:raise RuntimeError('manifest observation path bound')
                    found.add(str(Path(candidate).relative_to(source)))
                    if len(found)>4096:raise RuntimeError('manifest observation count bound')
                else:
                    if re.search(r'\band\s+[0-9]+\s+more\b',line) or not line.strip():enumeration_complete=False
                    section=False
            if any(marker in line for marker in ('failed to load workspace','semantic analyzer unavailable','failed to expand macro')):
                diagnostics.add(line[-2048:])
                if len(diagnostics)>1024:raise RuntimeError('coverage diagnostic count bound')
        if section:enumeration_complete=False
    enumeration_complete=enumeration_complete and sections>0
    return {'scope':'Linux host, all targets, default all features; not all OS/target triples',
        'maintained_manifests':inventory,'found_manifests':sorted(found),'missing_found_manifests':sorted(set(inventory)-found) if enumeration_complete else None,
        'manifest_enumeration':'COMPLETE' if enumeration_complete else 'UNKNOWN',
        'diagnostics':sorted(set(diagnostics)),
        'status':'COVERAGE_REQUIRES_REVIEW' if not enumeration_complete or set(inventory)-found or diagnostics else 'MANIFEST_COLLECTION_OBSERVED_NOT_ALL_SEMANTICS_PROVEN',
        'boundaries':['found manifest does not prove every cfg/target/macro model','nightly and WASM/foreign-platform contexts require independent diagnostics; not waived']}


def read_initial_locks(out,source,deadline):
    path=out/'lock-inventory-initial.json'
    try:
        parent=evidence._open_directory(out)
        try:descriptor=os.open(path.name,os.O_RDONLY|os.O_NOFOLLOW|os.O_NONBLOCK,dir_fd=parent)
        finally:os.close(parent)
        with os.fdopen(descriptor,'rb') as stream:
            if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):return None
            raw=stream.read(evidence.MAX_RECEIPT_BYTES+1)
        if len(raw)>evidence.MAX_RECEIPT_BYTES:return None
        value=json.loads(raw)
        if (value.get('source')!=str(source) or value.get('expected_head')!=SOURCE
            or value.get('fixed_job_deadline')!=deadline or value.get('status')!='COMPLETE'):return None
        return value
    except (OSError,ValueError,TypeError,AttributeError):return None


def observe_inputs(source,out,end,deadline,manifests,initial,stage,*,identity=None,metadata=None,metadata_path=None):
    current=evidence.lock_inventory(source,manifests,out,end,SOURCE,stage=stage,fixed_deadline=deadline)
    identity=identity if identity is not None else evidence.capture_identity(
        source,out,end,SOURCE,prefix=stage,initial_locks=initial)
    def binding(value):
        return {item['path']:(item['status'],item.get('bytes'),item.get('sha256')) for item in value['locks']}
    comparable=initial is not None and current['status']=='COMPLETE'
    changed=sorted(name for name,value in binding(current).items()
                   if value!=binding(initial).get(name)) if comparable else None
    report={'stage':stage,'source':str(source),'expected_head':SOURCE,'fixed_job_deadline':deadline,
            'identity_status':identity.get('status'),'lock_status':current['status'],
            'changed_locks':changed,'origin':'UNKNOWN','dependency_resolution_impact':'UNKNOWN',
            'status':'PASS' if comparable and changed==[] and identity.get('status')=='PASS' else 'FAIL'}
    if metadata_path is not None and metadata_path.is_file():
        report['metadata_output_sha256']=digest_until(metadata_path,end)
        if isinstance(metadata,dict):
            report['resolve_document_sha256']=hashlib.sha256(json.dumps(metadata.get('resolve'),sort_keys=True,separators=(',',':')).encode()).hexdigest()
            report['resolve_digest_scope']='existing JSON document identity, not semantic equivalence or a post-analysis graph'
    save(out/('input-observation-'+stage+'.json'),report,out)
    return report


def capture_failure_evidence(args,source,out,end,env,terminal):
    """Collect existing bytes before any admission gate; never evaluate CodeQL here."""
    capture_end=min(end-120,time.monotonic()+90)
    terminal['capture_limits']={'maximum_seconds':90,'identity_reserve_seconds':30,
        'scope':'within the unchanged job deadline and two minute capture step'}
    initial_locks=read_initial_locks(out,source,args.deadline)
    for name,operation in (
        ('log_capture',lambda:evidence.collect_logs(args.database,out,capture_end-30,settle=True)),
        ('checkout',lambda:evidence.capture_identity(source,out,capture_end,SOURCE,prefix='capture',initial_locks=initial_locks)),
        ('resources',lambda:resources(env)),
    ):
        try:terminal[name]=operation()
        except Exception as exc:
            terminal[name]={'status':'PARTIAL','error':repr(exc)[:2048]}
            terminal['secondary_errors'].append({'stage':name,'error':repr(exc)[:2048]})
    try:
        terminal['cargo_guard']=cargo_guard.inspect(out,args.deadline,SOURCE,end=capture_end)
    except Exception as exc:
        terminal['cargo_guard']={'status':'FAIL','error':type(exc).__name__}
    terminal['checkout_gate']=terminal['checkout'].get('status','PARTIAL')
    terminal['resource_gate']='PASS' if 'CODEQL_THREADS' in terminal['resources'] and 'CODEQL_RAM' in terminal['resources'] else 'FAIL'
    terminal['evidence_status']=terminal['log_capture'].get('status','PARTIAL')
    terminal['status']='CAPTURED'
    terminal['coverage_capture']={'status':'NOT_REACHED'}
    terminal['lock_observation_gate']='NOT_REACHED'
    try:
        if initial_locks is not None:
            manifests=json.loads((out/'manifest-inventory.json').read_text())
            prior=json.loads((out/'input-observation-pre-analyze.json').read_text())
            if (prior.get('status')!='PASS' or prior.get('source')!=str(source)
                or prior.get('expected_head')!=SOURCE or prior.get('fixed_job_deadline')!=args.deadline):
                raise RuntimeError('pre-analysis input observation is missing or unbound')
            observed=observe_inputs(source,out,capture_end,args.deadline,manifests,initial_locks,
                                    'post-analyze',identity=terminal['checkout'])
            terminal['lock_observation_gate']=observed['status']
    except Exception as exc:
        terminal['lock_observation_gate']='FAIL'
        terminal['secondary_errors'].append({'stage':'lock-observation','error':repr(exc)[:2048]})

    # Preserve the primary failure and raw-capture receipts before any derived
    # parsing. A step timeout during that optional work must not erase them.
    runtime.save(out/'capacity-capture-terminal.json',terminal,out,terminal=True)
    try:
        check_time(capture_end)
        inventory=json.loads((out/'manifest-inventory.json').read_text())
        texts=[];expanded=0
        for item in terminal['log_capture'].get('files',[]):
            check_time(capture_end)
            if not item.get('artifact_created'):continue
            name=item.get('destination')
            if not name:continue
            path=out/name
            if path.is_symlink() or not path.resolve().is_relative_to((out/'database-logs').resolve()):raise RuntimeError('unsafe captured log reference')
            if path.is_file():
                expanded+=item.get('raw_bytes',path.stat().st_size)
                if expanded>evidence.MAX_RAW_CAPTURE_BYTES:raise RuntimeError('expanded log aggregate bound')
                texts.extend(captured_segments(path,item,end=capture_end))
        save(out/'query-progress.json',query_progress(texts,end=capture_end),out)
        coverage=coverage_report(source,inventory,texts,end=capture_end,logs_complete=terminal['evidence_status']=='COMPLETE')
        coverage['log_evidence_status']=terminal['evidence_status']
        if terminal['evidence_status']!='COMPLETE':coverage['status']='COVERAGE_REQUIRES_REVIEW'
        save(out/'coverage.json',coverage,out)
        terminal['coverage_capture']={'status':'PASS'}
    except Exception as exc:
        terminal['coverage_capture']={'status':'NOT_REACHED','error':repr(exc)[:2048]}
    budget=out/'capacity-budget.json'
    if budget.is_file():
        try:terminal['budget']=json.loads(budget.read_text())
        except Exception as exc:terminal['secondary_errors'].append({'stage':'budget','error':repr(exc)[:2048]})
    terminal['status']='CAPTURED'


def analysis_budget(mode,remaining):
    if mode not in ('capacity','official-default-research'):
        raise ValueError('unknown assessment mode')
    research=mode=='official-default-research'
    budget=21600 if research else 2700
    minutes=int((remaining-600)//60)
    if not research:minutes=min(35,minutes)
    return {'job_budget_seconds':budget,'elapsed_since_job_start':budget-remaining,
            'remaining_at_allocation':remaining,'analysis_allocated_minutes':minutes,
            'post_analysis_reserve_seconds':600,'helper_terminal_and_upload_reserve_seconds':120,
            'scope':'official-default research; not production capacity' if research else
                'internal analysis allocation; not a measurement of the complete 45 minute workload'}


def main():
    global SOURCE
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('phase',choices=('prepare','capture','finish'))
    parser.add_argument('--codeql',type=Path)
    parser.add_argument('--source-sha',choices=(BASE_SOURCE,EXCLUSION_SOURCE),default=EXCLUSION_SOURCE)
    parser.add_argument('--mode',choices=('capacity','official-default-research'),default='capacity')
    for name in ('source','database','output','work','sarif'):parser.add_argument('--'+name,type=Path,required=True)
    parser.add_argument('--deadline',type=float,required=True)
    parser.add_argument('--checkout-outcome',default='unknown')
    parser.add_argument('--layout-outcome',default='unknown')
    parser.add_argument('--tools-outcome',default='unknown')
    parser.add_argument('--init-outcome',default='unknown')
    parser.add_argument('--prepare-outcome',default='unknown')
    parser.add_argument('--analyze-outcome',default='unknown')
    args=parser.parse_args()
    SOURCE=args.source_sha;out=args.output;work=args.work;source=args.source.resolve()
    research=args.mode=='official-default-research'
    if args.phase!='capture' and args.codeql is None:parser.error('--codeql is required for prepare/finish')
    # Capacity artifacts have their own bound. Database files inherit runner limits, not probe RLIMIT_FSIZE.
    runtime.CAP=CAP;runtime.WRITE_CAP=CAP-1024**2
    end=time.monotonic()+max(0,args.deadline-time.time())
    terminal={'status':'FAILED','assessment_status':'ASSESSMENT_INCOMPLETE','source':SOURCE,'source_root':str(source),'phase':args.phase,
        'security_upload':False,'database_upload':False,'full_rust_coverage':'HOLD','fixed_job_deadline':args.deadline,
        'checkout_outcome':args.checkout_outcome,'layout_outcome':args.layout_outcome,'tools_outcome':args.tools_outcome,
        'init_outcome':args.init_outcome,'prepare_outcome':args.prepare_outcome,'analyze_outcome':args.analyze_outcome,
        'default_result_verification':'NOT_REACHED','public_semantics':'NOT_REACHED',
        'action_cleanup':'UNKNOWN_RUNNER_OWNED','verification_admitted':False,'primary_failure':None,'secondary_errors':[],
        'mode':args.mode,'official_analysis_outcome':args.analyze_outcome,'production_capacity':'UNKNOWN'}
    if research:terminal.update(assessment_status='RESEARCH_INCOMPLETE',public_semantics='NOT_REQUESTED')
    for stage in ('checkout','layout','tools','init','prepare','analyze'):
        outcome=getattr(args,stage+'_outcome')
        if outcome in ('failure','cancelled'):
            terminal['primary_failure']={'stage':stage,'outcome':outcome};break
    out.mkdir(exist_ok=True);work.mkdir(exist_ok=True)
    env={k:v for k,v in os.environ.items() if k not in ('GH_TOKEN','GITHUB_TOKEN','RUSTC_WRAPPER')}
    resource_values=None
    def run(argv,name,seconds=None):
        remaining=end-120-time.monotonic()
        if remaining<=0:raise RuntimeError('NOT_REACHED fixed job deadline reserve')
        return runtime.run([str(x) for x in argv],source,out/(name+'.log'),end-120,min(remaining,seconds or remaining),out,env,file_limit=None)
    primary=None
    try:
        if args.phase=='capture':
            capture_failure_evidence(args,source,out,end,env,terminal)
        elif args.phase=='prepare':
            resource_values=resources(env)
            layout=json.loads((out/'collection-layout.json').read_text())
            if layout.get('status')!='PASS':raise RuntimeError('collection layout was not admitted before init')
            identity=evidence.capture_identity(source,out,end-120,SOURCE,prefix='prepare')
            if identity.get('status')!='PASS':raise RuntimeError('frozen business checkout differs; see prepare-identity.json')
            frozen=snapshot(source)
            save(out/'source-before.json',frozen,out)
            files,_=plan.inventory(source,SOURCE)
            manifests=sorted(name for name in files if Path(name).name=='Cargo.toml' and not name.startswith(EXCLUDES))
            save(out/'manifest-inventory.json',manifests,out)
            initial_locks=evidence.lock_inventory(source,manifests,out,end-120,SOURCE,stage='initial',fixed_deadline=args.deadline)
            if initial_locks['status']!='COMPLETE':raise RuntimeError('initial lock inventory incomplete')
            independent=cargo_guard.standalone_locks(source,manifests,initial_locks,frozen)
            save(out/'independent-workspace-locks.json',dict(independent,source=SOURCE),out)
            if independent['status']!='PASS':raise RuntimeError('independent workspace locks are not frozen; see independent-workspace-locks.json')
            save(out/'target-toolchain-inputs.json',{name:value for name,value in frozen.items() if Path(name).name in ('rust-toolchain.toml','rust-toolchain') or name in manifests},out)
            save(out/'resources.json',dict(resource_values,space=shutil.disk_usage(work)._asdict()),out)
            terminal['native_preparation']='INCOMPLETE'
            # Reuse only native preparation, never the standalone Cargo check.
            # Its end-90 reserve becomes the capacity end-120 reserve.
            native.prepare_native(source,out,end-30,env)
            terminal['native_preparation']='PASS'
            run(['rustc','-vV'],'project-toolchain',20)
            run([args.codeql,'version','--format=json'],'version',20)
            if json.loads((out/'version.log').read_text())['version']!=BUNDLE:raise RuntimeError('wrong official CLI')
            save(out/'official-before.json',official_snapshot(args.codeql.parent),out)
            closure=bundle.bundle_closure(args.codeql.parent,BUNDLE);save(out/'library-closure.json',closure,out)
            roots=list((args.codeql.parent/'qlpacks/codeql/rust-queries').glob('*/codeql-suites/rust-code-scanning.qls'))
            if len(roots)!=1:raise RuntimeError('default suite identity ambiguous')
            suite=roots[0];query_root=suite.parent.parent
            run([args.codeql,'resolve','queries',suite,'--format=json'],'default-suite',30)
            entries=suite_entries(json.loads((out/'default-suite.log').read_text()),query_root)
            save(out/'default-suite.json',{'suite_sha256':runtime.digest(suite),'query_root':str(query_root),'queries':entries},out)
            libp2p={}
            for context,manifest in [('workspace',source/'Cargo.toml'),('candidate',source/model.BASE/'candidate/Cargo.toml'),('legacy',source/model.BASE/'legacy/Cargo.toml'),('common',source/model.BASE/'common/Cargo.toml')]:
                metadata_path=out/('metadata-'+context+'.log')
                stage='after-metadata-'+context
                try:
                    run(['cargo','metadata','--locked','--format-version=1','--manifest-path',manifest],'metadata-'+context,120)
                    data=json.loads(metadata_path.read_text())
                except Exception:
                    try:observe_inputs(source,out,end-120,args.deadline,manifests,initial_locks,stage,metadata_path=metadata_path)
                    except Exception as observation_error:terminal.setdefault('input_observation_errors',[]).append(repr(observation_error)[:2048])
                    raise
                observed=observe_inputs(source,out,end-120,args.deadline,manifests,initial_locks,stage,metadata=data,metadata_path=metadata_path)
                if observed['status']!='PASS':raise RuntimeError('metadata changed or obscured frozen inputs: '+stage)
                if context in ('candidate','legacy'):libp2p[context]=str(model.verify_metadata(data,source,context))
                if not isinstance(data.get('resolve'),dict) or not data['resolve'].get('nodes'):raise RuntimeError('incomplete Cargo graph')
            save(out/'libp2p-provenance.json',libp2p,out)
            if not research:
                query_dir=work/'queries';query_dir.mkdir(exist_ok=False)
                (query_dir/'qlpack.yml').write_text('name: oasis7/rust-capacity\nversion: 0.0.0\ndependencies:\n  codeql/rust-all: 0.2.24\n')
                texts=semantic_queries(source)
                for name,text in texts.items():(query_dir/(name+'.ql')).write_text(text)
                save(out/'semantic-query-inputs.json',{name:runtime.digest(query_dir/(name+'.ql')) for name in texts},out)
                packs=os.pathsep.join(item['root'] for item in closure.values())
                for name in texts:
                    run([args.codeql,'resolve','library-path','--query='+str(query_dir/(name+'.ql')),'--format=json','--additional-packs='+packs],'library-'+name,20)
                    bundle.verify_library_path(json.loads((out/('library-'+name+'.log')).read_text()),query_dir,name,closure,pack_name='oasis7/rust-capacity')
                # Exact CLI type/import validation before the expensive production analysis.
                run([args.codeql,'query','compile','--check-only','--keep-going','--additional-packs='+packs,
                     '--threads='+str(resource_values['CODEQL_THREADS']),'--ram='+str(resource_values['CODEQL_RAM']),
                     *[query_dir/(name+'.ql') for name in texts]],'semantic-typecheck',120)
            action_config=Path(os.environ['CODEQL_ACTION_TEMP'])/'config'
            config=json.loads(action_config.read_text())
            save(out/'action-config.json',{'sha256':runtime.digest(action_config),'settings':{key:config.get(key) for key in ('languages','buildMode','dbLocation','repositoryRoot','overlayDatabaseMode','trapCaches','dependencyCachingEnabled','computedConfig','useOverlayDatabaseCaching')}},out)
            verify_action_config(config,source,args.database)
            observed=observe_inputs(source,out,end-120,args.deadline,manifests,initial_locks,'pre-analyze')
            if observed['status']!='PASS':raise RuntimeError('pre-analysis inputs were not frozen')
            guard_home=cargo_guard.install(work,out,args.deadline,SOURCE,[source/n for n in independent['manifests']],monotonic_deadline=end)
            with open(os.environ['GITHUB_ENV'],'a') as output:output.write('CARGO_HOME='+str(guard_home)+'\n')
            remaining=args.deadline-time.time()
            budget=analysis_budget(args.mode,remaining);minutes=budget['analysis_allocated_minutes']
            save(out/'capacity-budget.json',dict(budget,fixed_job_deadline=args.deadline,mode=args.mode),out)
            if minutes<1:raise RuntimeError('NOT_REACHED no analysis+semantic+receipt capacity')
            with open(os.environ['GITHUB_OUTPUT'],'a') as output:output.write('analysis_minutes='+str(minutes)+'\n')
            terminal.update(status='PREPARED',analysis_minutes=minutes)
        else:
            captured=json.loads((out/'capacity-capture-terminal.json').read_text())
            terminal['primary_failure']=terminal['primary_failure'] or captured.get('primary_failure')
            terminal['capture_status']=captured.get('evidence_status')
            if args.analyze_outcome!='success':raise RuntimeError('ACTION_ANALYSIS_FAILED_OR_INCOMPLETE; no semantic child started')
            if captured.get('mode','capacity')!=args.mode:raise RuntimeError('capture assessment mode differs')
            if captured.get('source_root')!=str(source) or captured.get('fixed_job_deadline')!=args.deadline or captured.get('analyze_outcome')!='success':raise RuntimeError('failure capture identity differs')
            if captured.get('cargo_guard',{}).get('status')!='PASS' or captured.get('lock_observation_gate')!='PASS' or captured.get('checkout_gate')!='PASS' or captured.get('resource_gate')!='PASS' or captured.get('evidence_status')!='COMPLETE' or captured.get('coverage_capture',{}).get('status')!='PASS':raise RuntimeError('assessment admission failed; see independent capture receipts')
            resource_values=captured['resources']
            before_resources=json.loads((out/'resources.json').read_text())
            if any(resource_values[k]!=before_resources[k] for k in ('CODEQL_RAM','CODEQL_THREADS')):raise RuntimeError('Action resource exports changed')
            coverage=json.loads((out/'coverage.json').read_text())
            frozen=json.loads((out/'source-before.json').read_text())
            if snapshot(source)!=frozen:raise RuntimeError('business source changed')
            terminal['verification_admitted']=True
            suite=json.loads((out/'default-suite.json').read_text());manifest=[]
            actual_suite=args.database/'temp/config-queries.qls'
            runtime.evidence_write(out/'action-config-queries.qls',actual_suite.read_bytes(),out)
            run([args.codeql,'resolve','queries',actual_suite,'--format=json'],'action-default-suite',30)
            actual_entries=suite_entries(json.loads((out/'action-default-suite.log').read_text()),Path(suite['query_root']))
            if actual_entries!=suite['queries']:raise RuntimeError('actual Action query set differs from frozen official default suite')
            terminal['default_result_verification']='RUNNING'
            for query in suite['queries']:
                result=args.database/'results/codeql/rust-queries'/Path(query['relative']).with_suffix('.bqrs')
                if not result.is_file():raise RuntimeError('missing default query BQRS '+query['id'])
                name='default-'+str(len(manifest))
                run([args.codeql,'bqrs','info',result,'--format=json'],name,20)
                info=json.loads((out/(name+'.log')).read_text())
                run([args.codeql,'bqrs','decode',result,'--format=json'],name+'-decoded',30)
                decoded=json.loads((out/(name+'-decoded.log')).read_text())
                verify_decoded(info,decoded)
                manifest.append(dict(query,bqrs_sha256=runtime.digest(result),bqrs_bytes=result.stat().st_size,result_info=info))
                save(out/'default-query-results.json',manifest,out)
            sarifs=list(args.sarif.glob('*.sarif'))
            if len(sarifs)!=1:raise RuntimeError('local Rust SARIF missing/ambiguous')
            if sarifs[0].stat().st_size>runtime.WRITE_CAP-runtime.output_bytes(out):raise RuntimeError('EVIDENCE_INCOMPLETE complete SARIF exceeds artifact capacity')
            sarif=json.loads(sarifs[0].read_text());findings=check_sarif(sarif)
            runtime.evidence_write(out/'default-results.sarif',sarifs[0].read_bytes(),out)
            terminal['default_result_verification']='PASS'
            if research:
                terminal.update(status='RESEARCH_COMPLETED',assessment_status='RESEARCH_COMPLETED',findings=findings,
                    default_queries=len(manifest),assessment_scope='official default results only; no production capacity or public semantics acceptance')
            else:
                selected=[model.DRIVER,model.BASE+'/common/src/protocol.rs',model.EXPECT,model.BASE+'/candidate/src/adapter.rs',model.BASE+'/legacy/src/adapter.rs']
                before={name:item['sha256'] for name,item in frozen.items() if 'sha256' in item}
                save(out/'archived-source.json',archive_identity(args.database,selected,before),out)
                query_dir=work/'queries';texts=semantic_queries(source)
                if json.loads((out/'semantic-query-inputs.json').read_text())!={name:runtime.digest(query_dir/(name+'.ql')) for name in texts}:raise RuntimeError('public query bytes changed')
                common=work/'semantic-common';common.mkdir(exist_ok=False)
                terminal['public_semantics']='RUNNING'
                run([args.codeql,'database','run-queries',args.database,*[query_dir/(name+'.ql') for name in texts],
                     '--additional-packs='+os.pathsep.join(item['root'] for item in json.loads((out/'library-closure.json').read_text()).values()),'--threads='+str(resource_values['CODEQL_THREADS']),
                     '--ram='+str(resource_values['CODEQL_RAM']),'--common-caches='+str(common),'--no-default-compilation-cache'], 'semantic-batch')
                libp2p=json.loads((out/'libp2p-provenance.json').read_text());validated={}
                for name in texts:
                    matches=list((args.database/'results').rglob(name+'.bqrs'))
                    if len(matches)!=1:raise RuntimeError('semantic BQRS missing/ambiguous '+name)
                    run([args.codeql,'bqrs','decode',matches[0],'--format=csv'],'decode-'+name,30)
                    with (out/('decode-'+name+'.log')).open() as stream:rows=list(csv.reader(stream))[1:]
                    validated[name]=validate(name,rows,source,libp2p)
                    save(out/('semantic-'+name+'-terminal.json'),{'status':'PASS','validated':validated[name]},out)
                if snapshot(source)!=frozen:raise RuntimeError('post-evaluation business changed')
                terminal['public_semantics']='PASS'
                remaining=args.deadline-time.time()
                terminal['margin_scope']='evidence readiness; final hosted job duration including upload must independently confirm >=270s'
                terminal.update(status='NEAR_LIMIT' if remaining<270 else 'CAPACITY_COMPLETED',assessment_status='CAPACITY_COMPLETED',findings=findings,default_queries=len(manifest),public_semantic_tables=validated,remaining_seconds=remaining,full_rust_coverage=coverage['status'],assessment_scope='execution plus fixed semantic witnesses; no operational enablement')
    except BaseException as exc:
        primary=exc
        terminal['error']=repr(exc)[:2048]
        failure={'stage':args.phase,'error':terminal['error']}
        if terminal['primary_failure'] is None:terminal['primary_failure']=failure
        else:terminal['secondary_errors'].append(failure)
        for key in ('default_result_verification','public_semantics'):
            if terminal[key]=='RUNNING':terminal[key]='INCOMPLETE'
        if research:terminal.update(status='RESEARCH_INCOMPLETE',assessment_status='RESEARCH_INCOMPLETE')
        terminal['failure_scope']='ASSESSMENT_OR_INSTRUMENTATION; not a production45min-unusable conclusion'
        raise
    finally:
        terminal['remaining_seconds']=args.deadline-time.time()
        try:
            if primary is not None or args.phase=='capture' or args.codeql is None or (args.phase=='finish' and (terminal.get('analyze_outcome','success')!='success' or not terminal.get('verification_admitted',True))):
                integrity={'status':'NOT_REACHED','reason':'failed or unadmitted work uses bounded capture, without a whole-bundle rehash'}
            else:integrity=after_identity(source,args.codeql.parent,out,end=end-120)
            terminal['after_identity']=integrity
            if 'CHANGED' in integrity.values():
                terminal['status']='RESEARCH_INCOMPLETE' if terminal.get('mode')=='official-default-research' else 'FAILED'
                terminal['assessment_status']='RESEARCH_INCOMPLETE' if terminal.get('mode')=='official-default-research' else 'ASSESSMENT_INCOMPLETE'
                if primary is None:raise RuntimeError('post-assessment identity changed')
        except BaseException as exc:
            terminal['status']='RESEARCH_INCOMPLETE' if terminal.get('mode')=='official-default-research' else 'FAILED'
            terminal['assessment_status']='RESEARCH_INCOMPLETE' if terminal.get('mode')=='official-default-research' else 'ASSESSMENT_INCOMPLETE'
            terminal['identity_error']=repr(exc)[:1024]
            if primary is None:primary=exc
        try:runtime.save(out/('capacity-'+args.phase+'-terminal.json'),terminal,out,terminal=True)
        except BaseException:
            if primary is None:raise
        if primary is not None and terminal.get('error') is None:raise primary

if __name__=='__main__':main()
