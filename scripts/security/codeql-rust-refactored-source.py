"""Fresh, crate-scoped public-model witnesses for immutable refactored real source."""
import csv
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time
import zipfile

SOURCE = "49b25e1ab631ffb433bce88709fa5601720cfc17"
BASE = "tests/libp2p-compat"
DRIVER = BASE + "/common/src/driver.rs"
IMAGE = "crates/oasis7_chain_runtime"
EXPECT = IMAGE + "/src/host/executable_image/replacement_tests/expectations.rs"
DATA_LIMIT = 1024**3


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def qstr(value):
    return json.dumps(str(value))


def header(root):
    # Exact absolute paths preserve original source identity even outside source-root.
    return '''import rust
import codeql.rust.dataflow.DataFlow
predicate fileAt(Locatable n, string relative) {
  n.getLocation().getFile().getAbsolutePath() = ''' + qstr(root) + ''' + "/" + relative
}
predicate lineAt(Locatable n, string relative, int line) {
  fileAt(n, relative) and n.getLocation().getStartLine() = line
}
predicate macroAt(MacroCall m, string file, int line) {
  lineAt(m, file, line) and (
    file = "tests/libp2p-compat/common/src/driver.rs" and m.getPath().toString() = "assert_eq"
    or file = "crates/oasis7_chain_runtime/src/host/executable_image/replacement_tests/expectations.rs" and
      (line = [13,23] and m.getPath().toString() = "format" or
       line = [14,19,24] and m.getPath().toString() = "assert")
  )
}
predicate inExpansion(Expr e, string file, int line) {
  exists(MacroCall m | macroAt(m,file,line) and m.getMacroCallExpansion() = e.getParentNode*())
}
'''


def metrics_query(root, body):
    return header(root) + body + '''
from string label, int present
where label = labels() and
 (present = 1 and witness(label) or present = 0 and not witness(label))
select label, present
'''


def queries(root):
    # One query is instantiated separately in each real peer context; no merged entities.
    peer = metrics_query(root, '''
string adapter() { result = "''' + BASE + '''/" + ["candidate","legacy"] + "/src/adapter.rs" }
string labels() { result = ["derive-ast","derive-df","closure-flow"] }
predicate witness(string label) {
  label = "derive-ast" and exists(TypeItem item, AstNode n |
    item instanceof Struct and item.getName().toString() = "Behaviour" and
    lineAt(item.getName(),adapter(),10) and item.getADeriveMacroExpansion() = n.getParentNode*())
  or label = "derive-df" and exists(TypeItem item, DataFlow::Node n |
    item instanceof Struct and item.getName().toString() = "Behaviour" and
    lineAt(item.getName(),adapter(),10) and item.getADeriveMacroExpansion() = n.asExpr().getParentNode*())
  or label = "closure-flow" and exists(DataFlow::Node a, DataFlow::Node b |
    lineAt(a.asParameter(),adapter(),51) and lineAt(b.asExpr(),adapter(),60) and
    b.asExpr().toString() = "key" and DataFlow::localFlow(a,b))
}
''')
    driver = metrics_query(root, '''
string driver() { result = "''' + DRIVER + '''" }
string labels() { result = ["cli-flow","request-binding","response-binding","message-binding",
 "request-assert","response-assert","message-server-assert","message-client-assert",
 "wrong-branch","rpc-completion","gossip-completion","published-completion","wrong-state",
 "typed-request-call","typed-response-call","typed-publish-call"] }
predicate binding(DataFlow::Node n, int line, string name) {
  lineAt(n.asPat(),driver(),line) and n.asPat().toString() = name
}
predicate payload(string name,int sourceLine,int sinkLine) {
  exists(DataFlow::Node a, DataFlow::Node b |
    binding(a,sourceLine,name) and inExpansion(b.asExpr(),driver(),sinkLine) and DataFlow::localFlow(a,b))
}
predicate state(int sourceLine,string sinkName) {
  exists(DataFlow::Node a, DataFlow::Node b |
    lineAt(a.asExpr(),driver(),sourceLine) and
    (sourceLine = 50 and a.asExpr() instanceof Call or sourceLine != 50 and a.asExpr().toString() = "true") and
    lineAt(b.asExpr(),driver(),64) and b.asExpr() instanceof PathExpr and
    b.asExpr().toString() = sinkName and DataFlow::localFlow(a,b))
}
predicate witness(string label) {
  label = "cli-flow" and exists(DataFlow::Node a,DataFlow::Node b |
    lineAt(a.asParameter(),driver(),23) and lineAt(b.asExpr(),driver(),24) and
    b.asExpr() instanceof PathExpr and DataFlow::localFlow(a,b))
  or label = "request-binding" and exists(DataFlow::Node a | binding(a,40,"request"))
  or label = "response-binding" and exists(DataFlow::Node a | binding(a,44,"response"))
  or label = "message-binding" and exists(DataFlow::Node a | binding(a,52,"message"))
  or label = "request-assert" and payload("request",40,41)
  or label = "response-assert" and payload("response",44,45)
  or label = "message-server-assert" and payload("message",52,54)
  or label = "message-client-assert" and payload("message",52,57)
  or label = "wrong-branch" and payload("request",40,45)
  or label = "rpc-completion" and state(46,"rpc_done")
  or label = "gossip-completion" and state(58,"gossip_done")
  or label = "published-completion" and state(50,"published")
  or label = "wrong-state" and state(46,"gossip_done")
  or label = "typed-request-call" and exists(Call c | lineAt(c,driver(),38) and
    c.getTargetName() = "request" and lineAt(c.getStaticTarget(),"tests/libp2p-compat/common/src/protocol.rs",27))
  or label = "typed-response-call" and exists(Call c | lineAt(c,driver(),42) and
    c.getTargetName() = "respond" and lineAt(c.getStaticTarget(),"tests/libp2p-compat/common/src/protocol.rs",28))
  or label = "typed-publish-call" and exists(Call c | lineAt(c,driver(),50) and
    c.getTargetName() = "publish" and lineAt(c.getStaticTarget(),"tests/libp2p-compat/common/src/protocol.rs",29))
}
''')
    image = metrics_query(root, '''
string target() { result = "''' + EXPECT + '''" }
string labels() { result = ["format13-ast","format13-df","assert14-ast","assert14-df",
 "format23-ast","format23-df","assert24-ast","assert24-df","digest-flow","wrong-digest-flow",
 "whole-line-equality","positive-oracle-call","negative-oracle-call"] }
predicate layer(string label,int line) {
  label = "format13" and line = 13 or label = "assert14" and line = 14 or
  label = "format23" and line = 23 or label = "assert24" and line = 24
}
predicate digestFlow(int line) {
  exists(DataFlow::Node a,DataFlow::Node b |
    lineAt(a.asParameter(),target(),line) and inExpansion(b.asExpr(),target(),13) and DataFlow::localFlow(a,b))
}
predicate witness(string label) {
  exists(string prefix,int line,MacroCall m,AstNode n |
    layer(prefix,line) and label = prefix + "-ast" and macroAt(m,target(),line) and
    m.getMacroCallExpansion() = n.getParentNode*())
  or exists(string prefix,int line,MacroCall m,DataFlow::Node n |
    layer(prefix,line) and label = prefix + "-df" and macroAt(m,target(),line) and
    m.getMacroCallExpansion() = n.asExpr().getParentNode*())
  or label = "digest-flow" and digestFlow(12)
  or label = "wrong-digest-flow" and digestFlow(22)
  or label = "whole-line-equality" and exists(Expr e |
    lineAt(e,target(),4) and e.toString() = "line == expected")
  or label = "positive-oracle-call" and exists(Call c |
    lineAt(c,target(),14) and c.getTargetName() = "contains_line" and
    lineAt(c.getStaticTarget(),target(),2))
  or label = "negative-oracle-call" and exists(Call c |
    lineAt(c,target(),24) and c.getTargetName() = "contains_line" and
    lineAt(c.getStaticTarget(),target(),2))
}
''')
    calls = header(root) + '''
from Call c
where lineAt(c,"''' + BASE + '''/" + ["candidate","legacy"] + "/src/adapter.rs",42) and
 c.getTargetName() = "with_existing_identity"
select c.getLocation().getFile().getAbsolutePath(), c.getLocation().getStartLine(),
 c.getTargetName(), c.getStaticTarget().getLocation().getFile().getAbsolutePath()
'''
    files = '''import rust
from File f
where f.getAbsolutePath() = [''' + ','.join(qstr(root / p) for p in [DRIVER,BASE+'/common/src/protocol.rs',EXPECT,BASE+'/candidate/src/adapter.rs',BASE+'/legacy/src/adapter.rs']) + ''']
select f.getAbsolutePath(), f.getRelativePath()
'''
    mac = metrics_query(root, """
string labels() { result = ["inactive-mac-assert-entity","inactive-mac-assert-expansion","inactive-mac-assert-df"] }
predicate witness(string label) {
  label = "inactive-mac-assert-entity" and exists(MacroCall m | macroAt(m,""" + qstr(EXPECT) + """,19))
  or label = "inactive-mac-assert-expansion" and exists(MacroCall m,AstNode n |
    macroAt(m,""" + qstr(EXPECT) + """,19) and m.getMacroCallExpansion() = n.getParentNode*())
  or label = "inactive-mac-assert-df" and exists(MacroCall m,DataFlow::Node n |
    macroAt(m,""" + qstr(EXPECT) + """,19) and m.getMacroCallExpansion() = n.asExpr().getParentNode*())
}
""")
    return dict(PeerCandidate=peer.replace('["candidate","legacy"]', '"candidate"'),
                PeerLegacy=peer.replace('["candidate","legacy"]', '"legacy"'),
                CallsCandidate=calls.replace('["candidate","legacy"]', '"candidate"'),
                CallsLegacy=calls.replace('["candidate","legacy"]', '"legacy"'),
                Driver=driver, Image=image, Files=files, MacObservation=mac)


EXPECTED = {
    "Peer": {"derive-ast":1,"derive-df":1,"closure-flow":1},
    "Driver": {k:(0 if k in ("wrong-branch","wrong-state") else 1) for k in
        ("cli-flow","request-binding","response-binding","message-binding","request-assert",
         "response-assert","message-server-assert","message-client-assert","wrong-branch",
         "rpc-completion","gossip-completion","published-completion","wrong-state",
         "typed-request-call","typed-response-call","typed-publish-call")},
    "Image": {k:(0 if k == "wrong-digest-flow" else 1) for k in
        ("format13-ast","format13-df","assert14-ast","assert14-df","format23-ast","format23-df",
         "assert24-ast","assert24-df","digest-flow","wrong-digest-flow","whole-line-equality",
         "positive-oracle-call","negative-oracle-call")},
}


def check_metrics(rows, name):
    values = {r[0]:int(r[1]) for r in rows}
    if len(rows) != len(values) or values != EXPECTED[name]:
        raise RuntimeError("public semantic witness mismatch: " + name + " " + repr(values))
    return values


def protected(source):
    names = subprocess.check_output(["git","ls-files","-z","--",BASE,IMAGE+"/src",IMAGE+"/Cargo.toml","Cargo.toml","Cargo.lock","vendor-libp2p-swarm-0.48.0"],cwd=source).decode().split("\0")
    result = {}
    for name in filter(None,names):
        p=source/name
        if p.is_symlink() or not p.is_file(): raise RuntimeError("nonregular protected source")
        result[name]=digest(p)
    if any(name not in result for name in (DRIVER,EXPECT,BASE+'/common/Cargo.lock')):
        raise RuntimeError("incomplete immutable source")
    return result


def verify_metadata(value, source, context):
    resolve=value.get('resolve')
    if not isinstance(resolve,dict) or not resolve.get('nodes') or not resolve.get('root'):
        raise RuntimeError('complete locked metadata missing')
    packages=value['packages'];package=next(p for p in packages if p['id']==resolve['root'])
    relative=BASE+'/'+context if context in ('candidate','legacy') else IMAGE
    if Path(package['manifest_path']).resolve() != (source/relative/'Cargo.toml').resolve():
        raise RuntimeError('wrong Cargo context')
    if context in ('candidate','legacy'):
        peer=[p for p in package['targets'] if p['kind']==['bin'] and p['name']=='peer']
        if len(peer)!=1 or Path(peer[0]['src_path']).resolve()!=source/relative/'src/main.rs':
            raise RuntimeError('nonlocal peer entry')
        common=[p for p in packages if p['name']=='oasis7-libp2p-compat-protocol']
        if len(common)!=1 or Path(common[0]['manifest_path']).resolve()!=source/BASE/'common/Cargo.toml':
            raise RuntimeError('wrong common context')
        lp=[p for p in packages if p['name']=='libp2p']
        if len(lp)!=1: raise RuntimeError('ambiguous libp2p')
        if context=='candidate':
            if lp[0]['version']!='0.57.0' or lp[0]['source']!='git+https://github.com/libp2p/rust-libp2p?rev=2d8497b2615086bd88018ef3a1fb32bc613a07ca#2d8497b2615086bd88018ef3a1fb32bc613a07ca': raise RuntimeError('candidate provenance drift')
        elif lp[0]['version']!='0.56.0' or not lp[0]['source'].startswith('registry+') or any(p.get('source','').startswith('git+https://github.com/libp2p/') for p in packages if p.get('source')):
            raise RuntimeError('legacy provenance drift')
        if context=='candidate':
            swarm=[p for p in packages if p['name']=='libp2p-swarm']
            if len(swarm)!=1 or Path(swarm[0]['manifest_path']).resolve()!=source/'vendor-libp2p-swarm-0.48.0/Cargo.toml': raise RuntimeError('candidate vendor patch not resolved')
        return Path(lp[0]['manifest_path']).parent
    if not any(t['kind']==['lib'] for t in package['targets']): raise RuntimeError('production lib context missing')
    return None


def verify_archive(database, selected, before):
    found={}
    with zipfile.ZipFile(database/'src.zip') as archive:
        entries=archive.infolist()
        if len(entries)>65536 or sum(i.file_size for i in entries)>8*1024**3: raise RuntimeError('source archive cap')
        for name in selected:
            matches=[i for i in entries if i.filename.endswith('/'+name)]
            if len(matches)!=1 or matches[0].file_size>8*1024**2: raise RuntimeError('source archive identity ambiguous')
            actual=hashlib.sha256(archive.read(matches[0])).hexdigest()
            if actual!=before[name]: raise RuntimeError('archive source differs')
            found[name]=actual
    return found


def execute(driver, production, args, env):
    source=args.business_source.resolve();out=args.output;work=args.work;cli=str(args.codeql)
    save=lambda name,value:production.evidence_write(out/name,(json.dumps(value,indent=2)+'\n').encode(),out)
    actual=subprocess.check_output(['git','rev-parse','HEAD'],cwd=source,text=True).strip()
    if actual!=SOURCE or subprocess.check_output(['git','status','--porcelain'],cwd=source):
        raise RuntimeError('business source is not exact clean immutable checkout')
    before=protected(source);save('refactored-protected-before.json',before)
    official=args.codeql.parent/'qlpacks/codeql/rust-all/0.2.23'
    inventory=lambda:{str(p.relative_to(official)):digest(p) for p in official.rglob('*') if p.is_file()}
    official_before=inventory();save('refactored-official-before.json',official_before)
    cargo_home=work/'cargo-home';cargo_home.mkdir(exist_ok=False)
    env=dict(env,CARGO_HOME=str(cargo_home));env.pop('RUSTC_WRAPPER',None)
    primary=None
    try:
        # A source pack is new and unlocked, never a copied compiled-pack lock.
        query=out/'refactored-queries';query.mkdir(exist_ok=False)
        production.evidence_write(query/'qlpack.yml',b'name: oasis7/rust-model-diagnostic\nversion: 0.0.0\ndependencies:\n  codeql/rust-all: 0.2.23\n',out)
        texts=queries(source)
        for name,text in texts.items(): production.evidence_write(query/(name+'.ql'),text.encode(),out)
        packs=os.pathsep.join(str(args.codeql.parent/'qlpacks/codeql'/name/version) for name,version in driver.LIBRARY_VERSIONS.items())
        run=driver.run
        for name in texts:
            log=out/f'refactored-library-{name}.json'
            run([cli,'resolve','library-path','--query='+str(query/(name+'.ql')),'--format=json','--additional-packs='+packs],work,log,args.deadline-90,20,out,env)
            driver.verify_library_path(json.loads(log.read_text()),args.codeql.parent,query,name)
        cache=work/'refactored-query-cache';cache.mkdir(exist_ok=False)
        common=work/'refactored-common-cache';common.mkdir(exist_ok=False)
        run([cli,'query','compile','--keep-going','--compilation-cache='+str(cache),'--no-default-compilation-cache','--common-caches='+str(common),'--additional-packs='+packs,'--threads=2','--ram=3072',*[str(query/(name+'.ql')) for name in texts]],work,out/'refactored-compile.log',args.deadline-90,240,out,env)
        save('refactored-query-inputs.json',{name:hashlib.sha256(text.encode()).hexdigest() for name,text in texts.items()})
        for context in ('candidate','legacy','image'):
            if args.deadline-time.monotonic()<450: raise RuntimeError('NOT_REACHED insufficient total context budget: '+context)
            if shutil.disk_usage(work).free<4*1024**3: raise RuntimeError('insufficient owned data space')
            relative=BASE+'/'+context if context!='image' else IMAGE
            folder=source/relative
            material=dict(env,CARGO_NET_OFFLINE='false');material.pop('RUSTC_WRAPPER',None)
            meta=out/f'{context}-metadata.log'
            run(['cargo','metadata','--quiet','--locked','--all-features','--format-version=1','--manifest-path',str(folder/'Cargo.toml')],source,meta,args.deadline-90,90,out,material)
            objects=[json.loads(line) for line in meta.read_text().splitlines() if line.startswith('{')]
            if len(objects)!=1: raise RuntimeError('ambiguous Cargo metadata JSON')
            libp2p_root=verify_metadata(objects[0],source,context)
            extract_env=dict(env,CARGO_NET_OFFLINE='true',CODEQL_EXTRACTOR_RUST_OPTION_CARGO_ALL_TARGETS='true',CODEQL_EXTRACTOR_RUST_OPTION_EXTRA_INCLUDES=json.dumps([str(source/BASE/'common')] if context!='image' else []))
            save(context+'-context.json',{'business_source':SOURCE,'source_root':str(folder),'libp2p_root':str(libp2p_root) if libp2p_root else None,'extractor_env':{k:v for k,v in extract_env.items() if k.startswith('CODEQL_EXTRACTOR_RUST_')},'scope':'real crate within complete immutable checkout'})
            database=work/(context+'-db')
            run([cli,'database','create',str(database),'--language=rust','--build-mode=none','--source-root='+str(folder),'--threads=2','--ram=3072'],source,out/(context+'-extract.log'),args.deadline-90,180,out,extract_env,DATA_LIMIT)
            extract_text=(out/(context+'-extract.log')).read_text()
            if 'cargo_all_targets: true' not in extract_text or 'cargo_features: []' not in extract_text:
                raise RuntimeError('actual extractor feature/target configuration unverified')
            if context!='image' and str(source/BASE/'common') not in extract_text:
                raise RuntimeError('actual extra include configuration unverified')
            selected=[DRIVER,BASE+'/common/src/protocol.rs',BASE+'/'+context+'/src/adapter.rs'] if context!='image' else [IMAGE+'/src/lib.rs',IMAGE+'/src/host.rs',EXPECT,IMAGE+'/src/host/executable_image/mod.rs',IMAGE+'/src/host/executable_image/replacement_tests.rs',IMAGE+'/src/host/executable_image/replacement_tests/fixture.rs']
            save(context+'-archived-source.json',verify_archive(database,selected,before))
            suffix=context.capitalize()
            names=('Peer'+suffix,'Driver','Calls'+suffix,'Files') if context!='image' else ('Image','MacObservation','Files')
            logs=out/(context+'-model.log')
            # Keep partial evaluator events in the existing bounded stdout pipe.
            run([cli,'database','run-queries',str(database),*[str(query/(n+'.ql')) for n in names],'--additional-packs='+packs,'--compilation-cache='+str(cache),'--no-default-compilation-cache','--common-caches='+str(common),'--max-disk-cache=1024','--threads=2','--ram=3072','--evaluator-log=/dev/stderr','--evaluator-log-minify','--verbosity=progress'],source,logs,args.deadline-90,180,out,extract_env,DATA_LIMIT)
            tables={}
            for name in names:
                candidates=list((database/'results').rglob(name+'.bqrs'))
                if len(candidates)!=1: raise RuntimeError('ambiguous or missing complete BQRS '+name)
                result=out/(context+'-'+name+'.csv')
                run([cli,'bqrs','decode',str(candidates[0]),'--format=csv','--output='+str(result)],source,out/(context+'-'+name+'-decode.log'),args.deadline-90,20,out,env)
                if driver.output_bytes(out)>63*1024**2: raise RuntimeError('result evidence cap')
                with result.open() as stream: tables[name]=list(csv.reader(stream))[1:]
                category='Peer' if name.startswith('Peer') else name
                if category in EXPECTED: check_metrics(tables[name],category)
            wanted={str(source/DRIVER),str(source/BASE/'common/src/protocol.rs'),str(source/BASE/context/'src/adapter.rs')} if context!='image' else {str(source/EXPECT)}
            actual_files=[r[0] for r in tables['Files']]
            if set(actual_files)!=wanted or len(actual_files)!=len(set(actual_files)): raise RuntimeError('public File witness missing/duplicate')
            if context!='image':
                rows=tables['Calls'+suffix]
                if len(rows)!=1 or rows[0][0]!=str(source/BASE/context/'src/adapter.rs') or not Path(rows[0][3]).is_relative_to(libp2p_root): raise RuntimeError('static call context mismatch')
            save(context+'-model-result.json',{'public_witnesses':'PASS','full_repository_coverage':'NOT_CLAIMED','source':SOURCE,'database':str(database),'metrics':{n:dict(tables[n]) for n in names if n in EXPECTED or n.startswith('Peer')},'inactive_mac_observation':tables.get('MacObservation'),'cross_trait_interprocedural_flow':'NOT_CLAIMED'})
    except BaseException as exc:
        primary=exc
        raise
    finally:
        try:
            # No further subprocess after any failure/uncertain owned cleanup.
            after={name:(digest(source/name) if (source/name).is_file() and not (source/name).is_symlink() else None) for name in before}
            official_after=inventory()
            if primary is None and subprocess.check_output(['git','diff','--name-only','HEAD'],cwd=source):
                raise RuntimeError('tracked business dependency source changed')
            save('refactored-protected-after.json',after)
            save('refactored-official-after.json',{'unchanged':official_after==official_before,'hashes':official_after})
            if before!=after or official_before!=official_after: raise RuntimeError('immutable source/official pack changed')
        except BaseException:
            if primary is None: raise
