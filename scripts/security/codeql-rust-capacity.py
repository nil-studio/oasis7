#!/usr/bin/env python3
"""One manual production-sized Rust assessment; never uploads security results."""
import argparse
import csv
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time
import zipfile
import codeql_rust_diagnostic_runtime as runtime

SOURCE='9ab1d76982e8ab58994728d881f3d60398834ade'
BUNDLE='2.27.3'
CAP=256*1024**2
EXCLUDES=('third_party/','target/','node_modules/','dist/','.pm/')

def load(name,file):
    spec=importlib.util.spec_from_file_location(name,Path(__file__).with_name(file))
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module

plan=load('capacity_plan','codeql-plan.py')
model=load('capacity_model','codeql-rust-refactored-source.py')
bundle=load('capacity_bundle','codeql-rust-model-diagnostic.py')

def snapshot(source, selected=None):
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
        if 'gitlink' in identity:
            # Mode/OID are source-index facts; excluded submodule contents are never inspected.
            result[name]={'mode':'160000','gitlink':identity['gitlink']};continue
        path=source/name
        if path.is_symlink():result[name]={'symlink':os.readlink(path)}
        elif path.is_file():result[name]={'sha256':runtime.digest(path),'bytes':path.stat().st_size}
        elif strict:raise RuntimeError('missing ordinary tracked source: '+name)
        else:result[name]={'missing':True}
    return result


def official_snapshot(root, selected=None):
    result={}
    paths=sorted(root.rglob('*')) if selected is None else [root/name for name in sorted(selected)]
    for path in paths:
        relative=str(path.relative_to(root))
        if path.is_symlink():result[relative]={'symlink':os.readlink(path)}
        elif path.is_file():result[relative]={'sha256':runtime.digest(path),'mode':path.stat().st_mode&0o111}
    return result


def after_identity(source,official,out):
    result={}
    before=out/'source-before.json'
    frozen_source=json.loads(before.read_text()) if before.is_file() else None
    blobs={name:value for name,value in (frozen_source or {}).items() if 'gitlink' not in value}
    result['source']='NOT_FROZEN' if frozen_source is None else ('UNCHANGED' if blobs==snapshot(source,blobs) else 'CHANGED')
    result['source_scope']='frozen non-gitlink regular/symlink inputs only'
    result['gitlinks']={name:dict(value,payload='NOT_INSPECTED',finally_oid_check='NOT_RECHECKED_WITHOUT_SUBPROCESS') for name,value in (frozen_source or {}).items() if 'gitlink' in value}
    before=out/'official-before.json'
    if not before.is_file():result['official']='NOT_FROZEN'
    else:
        frozen=json.loads(before.read_text())
        result['official']='UNCHANGED' if official_snapshot(official,frozen)==frozen else 'CHANGED'
        # New compiler/cache artifacts are distinguished from every frozen official input.
        result['additional_official_paths']=sorted(str(p.relative_to(official)) for p in official.rglob('*') if (p.is_file() or p.is_symlink()) and str(p.relative_to(official)) not in frozen)
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
    if config.get('dbLocation')!=str(database.parent) or config.get('repositoryRoot')!=str(source) or config.get('languages')!=['rust'] or config.get('overlayDatabaseMode')!='none' or config.get('trapCaches') or config.get('useOverlayDatabaseCaching'):
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


def collect_logs(database,out):
    root=database/'log';texts=[]
    if not root.is_dir() or root.is_symlink():return texts
    destination=out/'database-logs';destination.mkdir(exist_ok=True)
    for index,path in enumerate(sorted(root.rglob('*.log'))):
        if index>=128 or path.is_symlink() or not path.is_file() or any(p.is_symlink() for p in path.parents if p!=root.parent):raise RuntimeError('EVIDENCE_INCOMPLETE unsafe/excess database logs')
        target=destination/path.relative_to(root);target.parent.mkdir(parents=True,exist_ok=True)
        before=path.stat()
        with path.open('rb') as src,target.open('xb') as dst:
            for data in iter(lambda:src.read(65536),b''):
                if runtime.output_bytes(out)+len(data)>runtime.WRITE_CAP:raise RuntimeError('EVIDENCE_INCOMPLETE log cap')
                dst.write(data);dst.flush()
        after=path.stat()
        if (before.st_ino,before.st_size,before.st_mtime_ns)!=(after.st_ino,after.st_size,after.st_mtime_ns):raise RuntimeError('EVIDENCE_INCOMPLETE live log changed')
        texts.append(target.read_text(errors='replace'))
    return texts


def coverage_report(source,inventory,texts):
    found=set();diagnostics=[]
    for text in texts:
        section=False
        for raw in text.splitlines():
            line=re.sub(r'\x1b\[[0-9;]*m','',raw)
            if 'found manifests:' in line:section=True;continue
            if section:
                match=re.search(r'  ('+re.escape(str(source))+r'/[^\r\n]*Cargo[.]toml)\s*$',line)
                if match:found.add(str(Path(match[1]).relative_to(source)))
                else:section=False
            if any(marker in line for marker in ('failed to load workspace','semantic analyzer unavailable','failed to expand macro')):diagnostics.append(line[-2048:])
    return {'scope':'Linux host, all targets, default all features; not all OS/target triples',
        'maintained_manifests':inventory,'found_manifests':sorted(found),'missing_found_manifests':sorted(set(inventory)-found),
        'diagnostics':sorted(set(diagnostics)),
        'status':'COVERAGE_REQUIRES_REVIEW' if set(inventory)-found or diagnostics else 'MANIFEST_COLLECTION_OBSERVED_NOT_ALL_SEMANTICS_PROVEN',
        'boundaries':['found manifest does not prove every cfg/target/macro model','nightly and WASM/foreign-platform contexts require independent diagnostics; not waived']}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('phase',choices=('prepare','finish'))
    for name in ('codeql','source','database','output','work','sarif'):parser.add_argument('--'+name,type=Path,required=True)
    parser.add_argument('--deadline',type=float,required=True)
    parser.add_argument('--analyze-outcome',default='unknown')
    args=parser.parse_args();out=args.output;work=args.work;source=args.source.resolve()
    # Capacity artifacts have their own bound. Database files inherit runner limits, not probe RLIMIT_FSIZE.
    runtime.CAP=CAP;runtime.WRITE_CAP=CAP-1024**2
    end=time.monotonic()+max(0,args.deadline-time.time());terminal={'status':'FAILED','source':SOURCE,'phase':args.phase,'security_upload':False,'database_upload':False,'full_rust_coverage':'HOLD'}
    out.mkdir(exist_ok=True);work.mkdir(exist_ok=True)
    env={k:v for k,v in os.environ.items() if k not in ('GH_TOKEN','GITHUB_TOKEN','RUSTC_WRAPPER')}
    resource_values=None
    def run(argv,name,seconds=None):
        remaining=end-120-time.monotonic()
        if remaining<=0:raise RuntimeError('NOT_REACHED fixed job deadline reserve')
        return runtime.run([str(x) for x in argv],source,out/(name+'.log'),end-120,min(remaining,seconds or remaining),out,env,file_limit=None)
    primary=None
    try:
        resource_values=resources(env)
        if subprocess.check_output(['git','rev-parse','HEAD'],cwd=source,text=True).strip()!=SOURCE or subprocess.check_output(['git','status','--porcelain'],cwd=source):raise RuntimeError('frozen business checkout differs')
        if args.phase=='prepare':
            frozen=snapshot(source)
            save(out/'source-before.json',frozen,out)
            files,_=plan.inventory(source,SOURCE)
            manifests=sorted(name for name in files if Path(name).name=='Cargo.toml' and not name.startswith(EXCLUDES))
            save(out/'manifest-inventory.json',manifests,out)
            save(out/'target-toolchain-inputs.json',{name:value for name,value in frozen.items() if Path(name).name in ('rust-toolchain.toml','rust-toolchain') or name in manifests},out)
            save(out/'resources.json',dict(resource_values,space=shutil.disk_usage(work)._asdict()),out)
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
                run(['cargo','metadata','--locked','--format-version=1','--manifest-path',manifest],'metadata-'+context,120)
                data=json.loads((out/('metadata-'+context+'.log')).read_text())
                if context in ('candidate','legacy'):libp2p[context]=str(model.verify_metadata(data,source,context))
                if not isinstance(data.get('resolve'),dict) or not data['resolve'].get('nodes'):raise RuntimeError('incomplete Cargo graph')
            save(out/'libp2p-provenance.json',libp2p,out)
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
            save(out/'action-config.json',{'sha256':runtime.digest(action_config),'settings':{key:config.get(key) for key in ('languages','dbLocation','repositoryRoot','overlayDatabaseMode','trapCaches','dependencyCachingEnabled','computedConfig','useOverlayDatabaseCaching')}},out)
            verify_action_config(config,source,args.database)
            remaining=args.deadline-time.time()
            minutes=min(35,int((remaining-600)//60))
            if minutes<1:raise RuntimeError('NOT_REACHED no analysis+semantic+receipt capacity')
            with open(os.environ['GITHUB_OUTPUT'],'a') as output:output.write('analysis_minutes='+str(minutes)+'\n')
            terminal.update(status='PREPARED',analysis_minutes=minutes)
        else:
            logs=collect_logs(args.database,out)
            coverage=coverage_report(source,json.loads((out/'manifest-inventory.json').read_text()),logs)
            save(out/'coverage.json',coverage,out)
            if args.analyze_outcome!='success':raise RuntimeError('ACTION_ANALYSIS_FAILED_OR_INCOMPLETE; no semantic child started')
            frozen=json.loads((out/'source-before.json').read_text())
            if snapshot(source)!=frozen:raise RuntimeError('business source changed')
            suite=json.loads((out/'default-suite.json').read_text());manifest=[]
            actual_suite=args.database/'temp/config-queries.qls'
            runtime.evidence_write(out/'action-config-queries.qls',actual_suite.read_bytes(),out)
            run([args.codeql,'resolve','queries',actual_suite,'--format=json'],'action-default-suite',30)
            actual_entries=suite_entries(json.loads((out/'action-default-suite.log').read_text()),Path(suite['query_root']))
            if actual_entries!=suite['queries']:raise RuntimeError('actual Action query set differs from frozen official default suite')
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
            selected=[model.DRIVER,model.BASE+'/common/src/protocol.rs',model.EXPECT,model.BASE+'/candidate/src/adapter.rs',model.BASE+'/legacy/src/adapter.rs']
            before={name:item['sha256'] for name,item in frozen.items() if 'sha256' in item}
            save(out/'archived-source.json',archive_identity(args.database,selected,before),out)
            query_dir=work/'queries';texts=semantic_queries(source)
            if json.loads((out/'semantic-query-inputs.json').read_text())!={name:runtime.digest(query_dir/(name+'.ql')) for name in texts}:raise RuntimeError('public query bytes changed')
            common=work/'semantic-common';common.mkdir(exist_ok=False)
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
            remaining=args.deadline-time.time()
            terminal['margin_scope']='evidence readiness; final hosted job duration including upload must independently confirm >=270s'
            terminal.update(status='NEAR_LIMIT' if remaining<270 else 'CAPACITY_COMPLETED',findings=findings,default_queries=len(manifest),public_semantic_tables=validated,remaining_seconds=remaining,full_rust_coverage=coverage['status'],assessment_scope='execution plus fixed semantic witnesses; no operational enablement')
    except BaseException as exc:
        primary=exc
        terminal['error']=repr(exc)[:2048]
        terminal['failure_scope']='ASSESSMENT_OR_INSTRUMENTATION; not a production45min-unusable conclusion'
        raise
    finally:
        terminal['remaining_seconds']=args.deadline-time.time()
        try:
            integrity=after_identity(source,args.codeql.parent,out)
            terminal['after_identity']=integrity
            if 'CHANGED' in integrity.values():
                terminal['status']='FAILED'
                if primary is None:raise RuntimeError('post-assessment identity changed')
        except BaseException as exc:
            terminal['status']='FAILED'
            terminal['identity_error']=repr(exc)[:1024]
            if primary is None:primary=exc
        try:runtime.save(out/('capacity-'+args.phase+'-terminal.json'),terminal,out,terminal=True)
        except BaseException:
            if primary is None:raise
        if primary is not None and terminal.get('error') is None:raise primary

if __name__=='__main__':main()
