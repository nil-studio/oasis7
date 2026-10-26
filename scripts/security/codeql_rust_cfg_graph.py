#!/usr/bin/env python3
"""Bounded raw CFG-input diagnosis; never exports the evaluator closure."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import time
import codeql_rust_diagnostic_runtime as runtime

CAP = 256 * 1024**2
EDGE_CAP = 100000
TOP = 20
PATCHES = {
    'codeql/controlflow': ('codeql/controlflow/Cfg.qll', '203e8d576f3f0c384ecfe883cbea6e3b34cc5a60901ba104b7c8162a46063815'),
    'codeql/rust-all': ('codeql/rust/controlflow/internal/ControlFlowGraphImpl.qll', '814efdfe4961f0d489b3c1768e4ee2c5cda8a94f9bd781b94c2f41e47730907e'),
}
WRAPPER = r"""
// Diagnostic exports in a private pack copy only. No closure or final CFG node.
predicate oasisEdge(int a, int b) {
  exists(CfgInput::AstNode p, CfgInput::AstNode n |
    CfgImpl::Reachability::intraSplitsSucc(p, n) and
    a = CfgInput::idOfAstNode(p) and b = CfgInput::idOfAstNode(n))
}
predicate oasisNode(int a, int scope, string loc) {
  exists(CfgInput::AstNode n, CfgInput::CfgScope s |
    a = CfgInput::idOfAstNode(n) and s = n.getEnclosingCfgScope() and
    scope = CfgInput::idOfCfgScope(s) and loc = s.getLocation().toString())
}
predicate oasisStart(int a) {
  exists(CfgInput::AstNode n | CfgImpl::Reachability::startsSplits(n) and
    a = CfgInput::idOfAstNode(n))
}
"""
IMPORT = 'import codeql.rust.controlflow.internal.ControlFlowGraphImpl\n'
SUMMARY = IMPORT + r"""
from int s, string loc, int edges, int starts
where
  (exists(int a | oasisNode(a, s, loc)) and
   edges = count(int a, int b | oasisEdge(a, b) and oasisNode(a, s, loc)) and
   starts = count(int a | oasisStart(a) and oasisNode(a, s, loc)))
  or
  (s = -1 and loc = "UNKNOWN_SCOPE" and
   edges = count(int a, int b | oasisEdge(a, b) and not exists(int t, string l | oasisNode(a,t,l))) and
   starts = count(int a | oasisStart(a) and not exists(int t, string l | oasisNode(a,t,l))))
  or
  (s = -2 and loc = "CROSS_SCOPE_EDGES" and starts=0 and
   edges=count(int a,int b | oasisEdge(a,b) and exists(int x,int y,string l,string m | oasisNode(a,x,l) and oasisNode(b,y,m) and x!=y)))
  or
  (s = -3 and loc = "UNKNOWN_TARGET_SCOPE_EDGES" and starts=0 and
   edges=count(int a,int b | oasisEdge(a,b) and not exists(int t,string l | oasisNode(b,t,l))))
select s, loc, edges, starts
"""


def patch_text(name, text):
    if name == 'codeql/controlflow':
        for old in ('private module Reachability {', 'private predicate startsSplits(AstNode n)',
                    'private predicate intraSplitsSucc(AstNode pred, AstNode succ)'):
            if text.count(old) != 1:
                raise RuntimeError('patch anchor not unique: ' + old)
            text = text.replace(old, old.removeprefix('private '))
        return text
    if name != 'codeql/rust-all' or 'oasisEdge' in text:
        raise RuntimeError('unsupported or already patched input')
    return text + WRAPPER


def choose_scopes(rows):
    seen = set()
    for row in rows:
        if len(row) != 4 or type(row[0]) is not int or not isinstance(row[1],str) or any(type(x) is not int or x < 0 for x in row[2:]):
            raise RuntimeError('invalid summary row')
        if row[0] in seen:
            raise RuntimeError('ambiguous scope identity')
        seen.add(row[0])
    ranked = sorted((r for r in rows if r[0] >= 0), key=lambda r:(-r[2],r[0]))[:TOP]
    selected, skipped, used = [], [], 0
    for row in ranked:
        if used + row[2] > EDGE_CAP:
            skipped.append({'scope':row[0], 'reason':'EDGE_CAP', 'edges':row[2]})
        else:
            selected.append(row[0]); used += row[2]
    return selected, skipped


def edge_query(scopes):
    if not scopes or any(type(x) is not int or x < 0 for x in scopes):
        raise RuntimeError('no admitted scopes')
    return IMPORT + 'from int a, int b, int s, string loc, int t\nwhere oasisEdge(a,b) and oasisNode(a,s,loc) and s in [' + ','.join(map(str,scopes)) + '] and (exists(string l | oasisNode(b,t,l)) or (t=-1 and not exists(int u, string l | oasisNode(b,u,l))))\nselect a,b,s,loc,t,count(int x | x=a and oasisStart(x)),count(int x | x=b and oasisStart(x))\n'


def directed_metrics(edges, starts, deadline=float("inf"), budget=None):
    """Iterative Kosaraju; optional bounded positive-length reachability, no pair export."""
    budget = budget if budget is not None else [20000000]
    def check():
        if time.monotonic() >= deadline:raise TimeoutError("graph analysis deadline")
    check()
    graph={};reverse={}
    for a,b in edges:
        graph.setdefault(a,set()).add(b);graph.setdefault(b,set())
        reverse.setdefault(b,set()).add(a);reverse.setdefault(a,set())
    seen=set();order=[]
    for root in graph:
        if root in seen:continue
        seen.add(root);stack=[(root,iter(graph[root]))]
        while stack:
            check()
            n,it=stack[-1]
            m=next(it,None)
            if m is None:order.append(n);stack.pop()
            elif m not in seen:seen.add(m);stack.append((m,iter(graph[m])))
    seen=set();sizes=[]
    for root in reversed(order):
        if root in seen:continue
        seen.add(root);stack=[root];size=0
        while stack:
            check()
            n=stack.pop();size+=1
            for m in reverse[n]:
                if m not in seen:seen.add(m);stack.append(m)
        sizes.append(size)
    result={'scc_sizes':sorted(sizes,reverse=True),'positive_length_reachable_pairs':'NOT_COMPUTED_NODE_CAP',
            'starts_positive_length_pairs':'NOT_COMPUTED_NODE_CAP','reflexive_zero_length_pairs':len(graph)}
    if len(graph)>5000:return result
    total=source_total=operations=0
    for root in graph:
        reached=set();stack=list(graph[root])
        while stack:
            check()
            n=stack.pop();operations+=1;budget[0]-=1
            if budget[0]<0:
                result.update(positive_length_reachable_pairs='NOT_COMPUTED_OPERATION_CAP',starts_positive_length_pairs='NOT_COMPUTED_OPERATION_CAP')
                return result
            if n not in reached:reached.add(n);stack.extend(graph[n]-reached)
        total+=len(reached)
        if root in starts:source_total+=len(reached)
    result.update(positive_length_reachable_pairs=total,starts_positive_length_pairs=source_total)
    return result


def graph_report(rows, selected, deadline=float("inf")):
    def check():
        if time.monotonic() >= deadline:raise TimeoutError('graph analysis deadline')
    if len(rows) > EDGE_CAP:
        raise RuntimeError('decoded edge cap exceeded')
    graphs = {s:{} for s in selected}; cross=unknown=0; seen=set()
    for row in rows:
        if len(row)!=7 or any(type(row[i]) is not int or row[i]<0 for i in (0,1,2)) or row[2] not in graphs or not isinstance(row[3],str) or type(row[4]) is not int or row[4]<-1 or any(type(row[i]) is not int or row[i] not in (0,1) for i in (5,6)):
            raise RuntimeError('invalid edge row')
        a,b,s,_,target,_,_=row
        if (a,b,s) in seen: raise RuntimeError('duplicate edge row')
        seen.add((a,b,s))
        if target == -1: unknown+=1
        elif target != s: cross+=1
        g=graphs[s];g.setdefault(a,set()).add(b);g.setdefault(b,set()).add(a)
    reports=[];budget=[20000000]
    for s,g in graphs.items():
        remaining=set(g);sizes=[]
        while remaining:
            stack=[remaining.pop()];size=0
            while stack:
                check()
                n=stack.pop();size+=1
                for m in g[n]:
                    if m in remaining: remaining.remove(m);stack.append(m)
            sizes.append(size)
        metrics=directed_metrics([(r[0],r[1]) for r in rows if r[2]==s and r[4]==s], {r[0] for r in rows if r[2]==s and r[5]==1} | {r[1] for r in rows if r[2]==s and r[6]==1},deadline,budget)
        metrics['domain']='SAME_SCOPE_INDUCED_EDGE_ENDPOINTS'
        metrics['scope_complete']=not any(r[2]==s and r[4]!=s for r in rows)
        reports.append({'scope':s,'nodes':len(g),'weak_components':sorted(sizes,reverse=True),
                        'direct_edges':sum(1 for a,b,t in seen if t==s),**metrics})
    return {'scopes':reports,'cross_scope_edges':cross,'unknown_target_scope_edges':unknown,
            'graph_domain':'edge endpoints only; isolated nodes excluded from component sizes',
            'closure_cardinality':'NOT_MEASURED','evaluator_internal_rows':'UNKNOWN',
            'coverage':'SELECTED_SCOPES_ONLY','run_local_ids_only':True}


def decode_rows(path, info):
    value=json.loads(path.read_text())
    resultsets=info.get('result-sets') if isinstance(info,dict) else None
    if not isinstance(resultsets,list) or len(resultsets)!=1 or resultsets[0].get('name')!='#select' or set(value)!={'#select'}:raise RuntimeError('result-set mismatch')
    sets=value.get('#select')
    if not isinstance(sets,dict) or not isinstance(sets.get('tuples'),list):raise RuntimeError('unknown decoded schema')
    if type(resultsets[0].get('rows')) is not int or resultsets[0]['rows']!=len(sets['tuples']):raise RuntimeError('decoded rows incomplete')
    return sets['tuples']


def verify_originals(terminal, originals):
    """Integrity failure must not mask an earlier error or suppress terminal evidence."""
    try:
        terminal['originals_unchanged']=all(Path(p).is_file() and runtime.digest(Path(p))==sha for p,sha in originals.items())
        if not terminal['originals_unchanged']:terminal['status']='INCOMPLETE'
    except Exception as exc:
        terminal['originals_unchanged']=False
        terminal['integrity_error']=str(exc)
        terminal['status']='INCOMPLETE'


def command_file_limit(argv):
    # Evaluator page files belong to the private database, not the evidence stream.
    # Keep the existing direct-file bound for decode and other small outputs.
    return None if list(map(str, argv[1:3])) == ["query", "run"] else 64*1024**2


def main():
    ap=argparse.ArgumentParser(description=__doc__)
    for key in ('codeql','database','source','output','work'):ap.add_argument('--'+key,type=Path,required=True)
    ap.add_argument('--check-only',action='store_true');ap.add_argument('--deadline',type=float,required=True);ap.add_argument('--threads',type=int,default=4);ap.add_argument('--ram',type=int,required=True)
    args=ap.parse_args();out=args.output;out.mkdir(exist_ok=True);args.work.mkdir(exist_ok=True)
    runtime.CAP=CAP;runtime.WRITE_CAP=CAP-8*1024**2
    end=time.monotonic()+max(0,args.deadline-time.time())-180
    terminal={'status':'INCOMPLETE','full_rust_coverage':'HOLD','production_capacity':'UNKNOWN','security_upload':False,'closure_cardinality':'NOT_MEASURED'}
    originals={}
    def run(argv,name):
        runtime.run(list(map(str,argv)),args.source,out/(name+'.log'),end,min(900,max(0,end-time.monotonic())),out,
                    {k:v for k,v in os.environ.items() if k not in ('GH_TOKEN','GITHUB_TOKEN')},file_limit=command_file_limit(argv))
    try:
        if end<=time.monotonic() or args.threads!=4 or args.ram<=0:raise RuntimeError('resource/deadline admission')
        spec=importlib.util.spec_from_file_location('model',Path(__file__).with_name('codeql-rust-model-diagnostic.py'))
        model=importlib.util.module_from_spec(spec);spec.loader.exec_module(model)
        closure=model.bundle_closure(args.codeql.parent,'2.27.3')
        packs=args.work/'private-packs';packs.mkdir(exist_ok=True);patchlog={}
        for name,(rel,sha) in PATCHES.items():
            root=Path(closure[name]['root']);p=root/rel
            if runtime.digest(p)!=sha:raise RuntimeError('official source hash differs')
            originals[str(p)]=sha
            target=packs/name/closure[name]['version']
            if not target.exists():
                shutil.copytree(root,target,symlinks=False,ignore=shutil.ignore_patterns("*.qlx","*.bqrs",".cache",".codeql",".pack-cache",".compilation-cache"))
                q=target/rel;q.write_text(patch_text(name,q.read_text()))
            q=target/rel
            if q.read_text()!=patch_text(name,p.read_text()):raise RuntimeError('private patch changed')
            closure[name]=dict(closure[name],root=str(target))
            patchlog[name]={'original':str(p),'original_sha256':sha,'copy':str(q),'patched_sha256':runtime.digest(q)}
        runtime.save(out/'private-patches.json',patchlog,out)
        tree={str(p.relative_to(packs)):runtime.digest(p) for p in packs.rglob('*') if p.is_file()}
        treefile=out/'private-tree.json'
        if treefile.exists():
            baseline=json.loads(treefile.read_text())
            if any(tree.get(k)!=v for k,v in baseline.items()):raise RuntimeError('private pack tree changed')
            if any(Path(k).suffix in ('.ql','.qll','.yml','.dbscheme') for k in set(tree)-set(baseline)):raise RuntimeError('new private source file')
        else:runtime.save(treefile,tree,out)
        additional=os.pathsep.join(item['root'] for item in closure.values())
        cache=args.work/'compilation-cache';cache.mkdir(exist_ok=True)
        options=['--additional-packs='+additional,'--compilation-cache='+str(cache),'--no-default-compilation-cache']
        query=args.work/'queries';query.mkdir(exist_ok=True);(query/'qlpack.yml').write_text('name: oasis7/rust-cfg-diagnostic\nversion: 0.0.0\ndependencies:\n  codeql/rust-all: 0.2.24\n')
        def evaluate(name,text):
            q=query/(name+'.ql');q.write_text(text)
            run([args.codeql,'resolve','library-path','--query='+str(q),'--format=json','--additional-packs='+additional],'library-'+name)
            model.verify_library_path(json.loads((out/('library-'+name+'.log')).read_text()),query,name,closure,'oasis7/rust-cfg-diagnostic')
            b=out/(name+'.bqrs')
            run([args.codeql,'query','run',q,'--database='+str(args.database),'--output='+str(b),'--threads=4','--ram='+str(args.ram),*options],name)
            run([args.codeql,'bqrs','info',b,'--format=json'],'info-'+name)
            info=json.loads((out/('info-'+name+'.log')).read_text())
            sets=info.get('result-sets') if isinstance(info,dict) else None
            if not isinstance(sets,list) or len(sets)!=1 or sets[0].get('name')!='#select' or type(sets[0].get('rows')) is not int or sets[0]['rows']<0:raise RuntimeError('invalid BQRS result-set identity')
            if sets[0]['rows']>(EDGE_CAP if name=='edges' else 1000000):raise RuntimeError('BQRS row cap before decode')
            dest=out/(name+'.json')
            run([args.codeql,'bqrs','decode',b,'--format=json','--output='+str(dest)],'decode-'+name)
            return decode_rows(dest,info)
        if args.check_only:
            for name,text in (('summary',SUMMARY),('edges',edge_query([0]))):
                q=query/(name+'.ql');q.write_text(text)
                run([args.codeql,'resolve','library-path','--query='+str(q),'--format=json','--additional-packs='+additional],'check-library-'+name)
                model.verify_library_path(json.loads((out/('check-library-'+name+'.log')).read_text()),query,name,closure,'oasis7/rust-cfg-diagnostic')
                run([args.codeql,'query','compile','--check-only',q,'--threads='+str(args.threads),'--ram='+str(args.ram),*options],'check-'+name)
            terminal['status']='QL_CHECKED'
        if not args.check_only:
            rows=evaluate('summary',SUMMARY);selected,skipped=choose_scopes(rows)
            runtime.save(out/'selection.json',{'selected':selected,'skipped':skipped,'summary_rows':len(rows),'global_checks':[r for r in rows if r[0]<0],'selected_start_counts':{r[0]:r[3] for r in rows if r[0] in selected},'edge_cap':EDGE_CAP,'scope_cap':TOP},out)
            if not selected:raise RuntimeError('no bounded scope admitted; retained aggregate evidence')
            edges=evaluate('edges',edge_query(selected))
            expected={r[0]:r[2] for r in rows if r[0] in selected}
            actual={s:sum(r[2]==s for r in edges) for s in selected}
            if actual!=expected:raise RuntimeError('scope edge result incomplete or inconsistent')
            runtime.save(out/'graph-report.json',graph_report(edges,selected,end),out)
            terminal['status']='DIAGNOSTIC_COMPLETED'
    except Exception as exc:
        terminal['error']=str(exc)
    finally:
        verify_originals(terminal, originals)
        runtime.save(out/'cfg-terminal.json',terminal,out,terminal=True)
    return 0 if terminal['status'] in ('DIAGNOSTIC_COMPLETED','QL_CHECKED') else 1

if __name__=='__main__':raise SystemExit(main())
