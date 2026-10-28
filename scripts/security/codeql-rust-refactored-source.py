"""Fresh, crate-scoped public-model witnesses for immutable refactored real source."""
import csv
import hashlib
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import time
import zipfile
import codeql_rust_diagnostic_runtime as runtime

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
    return dict(PeerCandidate=peer.replace('["candidate","legacy"]', '"candidate"'),
                CallsCandidate=calls.replace('["candidate","legacy"]', '"candidate"'),
                Driver=driver, Files=files)


EXPECTED = {
    "Peer": {"derive-ast":1,"derive-df":1,"closure-flow":1},
    "Driver": {k:(0 if k in ("wrong-branch","wrong-state") else 1) for k in
        ("cli-flow","request-binding","response-binding","message-binding","request-assert",
         "response-assert","message-server-assert","message-client-assert","wrong-branch",
         "rpc-completion","gossip-completion","published-completion","wrong-state",
         "typed-request-call","typed-response-call","typed-publish-call")},

}


def protected(source):
    names = subprocess.check_output(["git","ls-files","-z","--",BASE,IMAGE,"Cargo.toml","Cargo.lock","vendor-libp2p-swarm-0.48.0"],cwd=source).decode().split("\0")
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


def verify_archive(database, selected, before, report=None):
    found={}
    with zipfile.ZipFile(database/'src.zip') as archive:
        entries=archive.infolist()
        if len(entries)>65536 or sum(i.file_size for i in entries)>8*1024**3:
            if report: report({'entry_count':len(entries),'expanded_bytes':sum(i.file_size for i in entries),'error':'archive cap'})
            raise RuntimeError('source archive cap')
        observations={}
        for name in selected:
            matches=[i for i in entries if i.filename.endswith('/'+name)]
            reasons=[]
            if not matches: reasons.append('zero matches')
            if len(matches)>1: reasons.append('multiple matches')
            if any(i.file_size>8*1024**2 for i in matches): reasons.append('selected member oversize')
            observations[name]={'matches':[{'filename':i.filename,'bytes':i.file_size} for i in matches],
                                'match_count':len(matches),'reasons':reasons}
        # Save every selected observation before rejecting any individual item.
        if report: report({'archive_bytes':(database/'src.zip').stat().st_size,
            'entry_count':len(entries),'expanded_bytes':sum(i.file_size for i in entries),
            'entries':[{'filename':i.filename,'bytes':i.file_size,'compressed_bytes':i.compress_size} for i in entries],
            'selected':observations})
        for name in selected:
            observation=observations[name]
            if observation['reasons']:
                raise RuntimeError('source archive identity: '+name+' '+repr(observation))
            actual=hashlib.sha256(archive.read(observation['matches'][0]['filename'])).hexdigest()
            if actual!=before[name]: raise RuntimeError('archive source differs: '+name)
            found[name]=actual
    return found


def collection(source, context):
    relative=BASE+'/'+context if context!='image' else IMAGE
    directories=[relative,BASE+'/common'] if context!='image' else [relative]
    names=subprocess.check_output(['git','ls-files','-z','--',*directories],cwd=source).decode().split('\0')
    files=sorted(name for name in names if name.endswith('.rs'))
    if not files or len(files)!=len(set(files)):
        raise RuntimeError('empty/duplicate initial source collection')
    if context!='image' and len(files)!=6:
        raise RuntimeError('frozen peer initial collection changed')
    filters='\n'.join('include:'+directory+'/**' for directory in directories)
    return filters,files


def partition_query(text, selected):
    """Keep original top-level witness clauses; partition only their finite label domain."""
    label_match = re.search(r'string labels\(\) \{ result = \[(.*?)\] \}', text, re.S)
    labels = re.findall(r'"([^"]+)"', label_match.group(1))
    if not selected or len(selected) != len(set(selected)) or not set(selected) <= set(labels):
        raise RuntimeError('invalid witness partition')
    start = text.index('predicate witness(string label) {\n') + len('predicate witness(string label) {\n')
    end = text.index('\n}\n', start)
    clauses = re.split(r'\n  or (?=label = ")', text[start:end])
    keyed = {}
    for clause in clauses:
        match = re.match(r'\s*label = "([^"]+)"', clause)
        if match is None or match.group(1) in keyed:
            raise RuntimeError('unrecognized witness clause')
        keyed[match.group(1)] = clause
    if set(keyed) != set(labels):
        raise RuntimeError('witness labels do not cover clauses')
    body = '  ' + '\n  or '.join(keyed[name].lstrip() for name in labels if name in selected)
    result = text[:start] + body + text[end:]
    return result[:label_match.start(1)] + ','.join(qstr(x) for x in labels if x in selected) + result[label_match.end(1):]


def partition_queries(root):
    original = queries(root)
    targets = ['typed-request-call', 'typed-response-call', 'typed-publish-call']
    driver_labels = re.findall(r'"([^"]+)"', re.search(r'string labels\(\) \{ result = \[(.*?)\] \}', original['Driver'], re.S).group(1))
    return {
        'PeerAst': partition_query(original['PeerCandidate'], ['derive-ast']),
        'PeerDf': partition_query(original['PeerCandidate'], ['derive-df', 'closure-flow']),
        'DriverNonTarget': partition_query(original['Driver'], [x for x in driver_labels if x not in targets]),
        'DriverTarget': partition_query(original['Driver'], targets),
        'CallsCandidate': original['CallsCandidate'],
        'Files': original['Files'],
    }


def check_files(rows, wanted):
    if any(len(row) != 2 for row in rows):
        raise RuntimeError('invalid public File row')
    actual = [row[0] for row in rows]
    if set(actual) != wanted or len(actual) != len(set(actual)):
        raise RuntimeError('public File witness missing/duplicate')


QUERY_CATEGORY = {'PeerAst': 'Peer', 'PeerDf': 'Peer', 'DriverNonTarget': 'Driver',
                  'DriverTarget': 'Driver', 'NodeOnly': 'Driver'}
PROBES = {'peer-df-inventory': ('Files', 'PeerAst', 'PeerDf'), 'peer-ast-inventory': ('Files', 'PeerAst'), 'files-inventory': ('Files',), 'node-only': ('Files', 'NodeOnly'), 'endpoints': ('Files', 'Endpoints'),
    'peer-df': ('Files', 'PeerDf'), 'driver-non-target': ('Files', 'DriverNonTarget'),
    'driver-target': ('Files', 'DriverTarget'), 'calls': ('Files', 'CallsCandidate'),
    'full': ('Files', 'PeerAst', 'PeerDf', 'DriverNonTarget', 'DriverTarget', 'CallsCandidate')}


def query_labels(text):
    match = re.search(r'string labels\(\) \{ result = \[(.*?)\] \}', text, re.S)
    if not match:
        raise RuntimeError('missing finite query labels')
    labels = re.findall(r'"([^"]+)"', match.group(1))
    if not labels or len(labels) != len(set(labels)):
        raise RuntimeError('duplicate/empty query label domain')
    return labels


def validate_partition(name, text, rows):
    labels = query_labels(text)
    values = {}
    for row in rows:
        if len(row) != 2 or row[0] in values or row[1] not in ('0', '1'):
            raise RuntimeError('invalid/duplicate partition row')
        values[row[0]] = int(row[1])
    if set(values) != set(labels):
        raise RuntimeError('partition label omission/extra')
    if name == 'Endpoints':
        return values  # diagnostic presence only; never replaces a semantic oracle
    category = QUERY_CATEGORY.get(name)
    if category is None or not set(labels) <= set(EXPECTED[category]):
        raise RuntimeError('unknown partition expected projection')
    projected = {key: EXPECTED[category][key] for key in labels}
    if values != projected:
        raise RuntimeError('public semantic partition mismatch: ' + name + repr(values))
    return values


def diagnostic_queries(root):
    result = partition_queries(root)
    result['NodeOnly'] = partition_query(queries(root)['Driver'],
        ['request-binding', 'response-binding', 'message-binding'])
    # These endpoint presences explain negative flow witnesses, never replace them.
    result['Endpoints'] = metrics_query(root, '''
string driver() { result = "''' + DRIVER + '''" }
string labels() { result = ["request-source","response-source","message-source",
 "request-sink","response-sink","server-message-sink","client-message-sink"] }
predicate witness(string label) {
  exists(DataFlow::Node n,int line,string name |
    (label = "request-source" and line = 40 and name = "request" or
     label = "response-source" and line = 44 and name = "response" or
     label = "message-source" and line = 52 and name = "message") and
    lineAt(n.asPat(),driver(),line) and n.asPat().toString() = name)
  or exists(DataFlow::Node n,int line |
    (label = "request-sink" and line = 41 or label = "response-sink" and line = 45 or
     label = "server-message-sink" and line = 54 or label = "client-message-sink" and line = 57) and
    inExpansion(n.asExpr(),driver(),line))
}
''')
    return result


def complete_union(partitions):
    """A partial probe can never become complete coverage by omission."""
    merged = {category: {} for category in ('Peer', 'Driver')}
    for name, values in partitions.items():
        if name not in ('PeerAst', 'PeerDf', 'DriverNonTarget', 'DriverTarget'):
            continue
        target = merged[QUERY_CATEGORY[name]]
        if set(target) & set(values):
            raise RuntimeError('overlapping semantic partitions')
        target.update(values)
    if merged != {category: EXPECTED[category] for category in merged}:
        raise RuntimeError('complete public witness union not measured')
    return merged


def execute(args, env, closure, verify_library, comparison=None):
    source, out, work, cli = args.business_source.resolve(), args.output, args.work, str(args.codeql)
    save = lambda name, value: runtime.save(out / name, value, out)
    if subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=source, text=True).strip() != SOURCE or subprocess.check_output(['git', 'status', '--porcelain'], cwd=source):
        raise RuntimeError('business source is not the exact clean checkout')
    before = protected(source)
    save('protected-before.json', before)
    def inventory():
        result = {}
        for pack in closure.values():
            root = Path(pack['root'])
            for path in root.rglob('*'):
                if path.is_symlink():
                    raise RuntimeError('symlink in official library closure')
                if path.is_file():
                    result[str(path.relative_to(args.codeql.parent))] = runtime.digest(path)
        return result
    official_before = inventory()
    save('terminal-size-admission.json', runtime.admit_terminal_snapshots(before, official_before))
    save('official-before.json', official_before)
    cargo_home = work / 'cargo-home'; cargo_home.mkdir(exist_ok=False)
    env = dict(env, CARGO_HOME=str(cargo_home))
    primary = None
    try:
        query = out / 'queries'; query.mkdir(exist_ok=False)
        rust_version = closure['codeql/rust-all']['version']
        runtime.evidence_write(query / 'qlpack.yml',
            f'name: oasis7/rust-model-diagnostic\nversion: 0.0.0\ndependencies:\n  codeql/rust-all: {rust_version}\n'.encode(), out)
        all_texts = diagnostic_queries(source)
        names = PROBES[args.probe]
        texts = {name: all_texts[name] for name in names}
        packs = os.pathsep.join(pack['root'] for pack in closure.values())
        for name, text in texts.items():
            runtime.evidence_write(query / (name + '.ql'), text.encode(), out)
            log = out / ('library-' + name + '.json')
            runtime.run([cli, 'resolve', 'library-path', '--query=' + str(query / (name + '.ql')),
                '--format=json', '--additional-packs=' + packs], work, log, args.deadline-90, 20, out, env)
            verify_library(json.loads(log.read_text()), query, name, closure)
        cache = work / 'query-cache'; cache.mkdir(exist_ok=False)
        common = work / 'common-cache'; common.mkdir(exist_ok=False)
        options = ['--additional-packs=' + packs, '--compilation-cache=' + str(cache),
            '--no-default-compilation-cache', '--common-caches=' + str(common), '--threads=2', '--ram=3072']
        save('query-inputs.json', {name: hashlib.sha256(text.encode()).hexdigest() for name, text in texts.items()})
        if comparison is None:
            runtime.run([cli, 'query', 'compile', '--keep-going', *options,
                *[str(query / (name + '.ql')) for name in names]], work, out / 'compile.log', args.deadline-90, 240, out, env)
        # Version-specific supported options are checked against the actual bundle.
        for command, label, required in [(['database', 'run-queries'], 'query-help',
            ('--evaluator-log', '--tuple-counting')), (['generate', 'log-summary'], 'summary-help',
            ('predicates', 'overall', '--minify-output'))]:
            runtime.run([cli, *command, '--help', '-v'], work, out / (label + '.log'), args.deadline-90, 20, out, env)
            help_text = (out / (label + '.log')).read_text()
            for flag in required:
                if flag not in help_text and flag.replace('--', '--[no-]', 1) not in help_text:
                    raise RuntimeError('actual CLI does not advertise ' + flag)
        if args.deadline - time.monotonic() < 450 or shutil.disk_usage(work).free < 4 * 1024**3:
            raise RuntimeError('NOT_REACHED insufficient fixed context budget/space')
        folder = source / BASE / 'candidate'
        runtime.run(['cargo', 'metadata', '--quiet', '--locked', '--all-features', '--format-version=1',
            '--manifest-path', str(folder / 'Cargo.toml')], source, out / 'metadata.json', args.deadline-90,
            90, out, dict(env, CARGO_NET_OFFLINE='false'))
        libp2p_root = verify_metadata(json.loads((out / 'metadata.json').read_text()), source, 'candidate')
        filters, initial = collection(source, 'candidate')
        extract_env = dict(env, LGTM_INDEX_FILTERS=filters,
            CODEQL_EXTRACTOR_RUST_OPTION_CARGO_ALL_TARGETS='true',
            CODEQL_EXTRACTOR_RUST_OPTION_EXTRA_INCLUDES=json.dumps([str(source / BASE / 'common')]))
        save('context.json', {'source': SOURCE, 'initial_files': {name: before[name] for name in initial},
            'index_filters': filters, 'libp2p_root': str(libp2p_root),
            'cargo_home': str(cargo_home), 'source_root': str(source)})
        database = work / 'candidate-db'
        runtime.run([cli, 'database', 'create', str(database), '--language=rust', '--build-mode=none',
            '--source-root=' + str(source), '--threads=2', '--ram=3072'], source, out / 'extract.log',
            args.deadline-90, 180, out, extract_env, DATA_LIMIT)
        # Extraction diagnostics may be stdout or stderr; neither stream is discarded.
        extraction = (out / 'extract.log').read_text() + (out / 'extract.log.stderr').read_text()
        if 'number of inputs: ' + str(len(initial)) + ',' not in extraction or any(
            str(path) not in extraction for path in (folder / 'Cargo.toml', source / BASE / 'common/Cargo.toml')):
            raise RuntimeError('actual candidate/common collection not verified')
        if 'cargo_all_targets: true' not in extraction or 'cargo_features: []' not in extraction:
            raise RuntimeError('actual extractor target/feature configuration not verified')
        selected = [DRIVER, BASE + '/common/src/protocol.rs', BASE + '/candidate/src/adapter.rs']
        archived = verify_archive(database, selected, before, lambda value: save('archive-index.json', value))
        save('archived-source.json', archived)
        wanted = {str(source / name) for name in selected}
        if comparison is not None:
            return comparison(args, env, closure, verify_library, database, texts, libp2p_root, wanted)
        model_end = min(args.deadline-90, time.monotonic() + 180)
        completed = {}
        for index, name in enumerate(names):
            receipt = {'query': name, 'query_sha256': runtime.digest(query / (name + '.ql')),
                'business_source': SOURCE, 'database': str(database), 'archived_source': archived,
                'bundle_version': args.bundle_version,
                'pack_closure_canonical_sha256': hashlib.sha256(json.dumps(closure,sort_keys=True).encode()).hexdigest(),
                'model_fixed_monotonic_end': model_end, 'shared_model_budget_seconds': 180,
                'cache_temperature': 'cold-first-partition' if index == 0 else 'reuse-attempt-not-proven-warm',
                'status': 'NOT_COMPLETED'}
            failure = None
            evaluator = out / (name + '-evaluator.jsonl')
            try:
                runtime.run([cli, 'database', 'run-queries', str(database), str(query / (name + '.ql')),
                    *options, '--max-disk-cache=1024', '--evaluator-log', '{EVALUATOR_FIFO}',
                    '--evaluator-log-minify', '--tuple-counting', '--verbosity=progress'], source,
                    out / (name + '-eval.log'), model_end, 180, out, extract_env, DATA_LIMIT, evaluator)
                bqrs = list((database / 'results').rglob(name + '.bqrs'))
                if len(bqrs) != 1:
                    raise RuntimeError('ambiguous or missing complete BQRS: ' + name)
                receipt['bqrs_sha256'] = runtime.digest(bqrs[0])
                result = out / (name + '.csv')
                runtime.run([cli, 'bqrs', 'decode', str(bqrs[0]), '--format=csv'], source, result,
                    model_end, 20, out, env)  # stdout CSV is capped before each write
                with result.open() as stream:
                    table = list(csv.reader(stream))
                expected_header = ['col0', 'col1'] if name == 'Files' else ['col0', 'col1', 'col2', 'col3'] if name == 'CallsCandidate' else ['label', 'present']
                if not table or table[0] != expected_header:
                    raise RuntimeError('unexpected complete BQRS CSV schema')
                rows = table[1:]
                if name == 'Files':
                    check_files(rows, wanted)
                elif name == 'CallsCandidate':
                    if len(rows) != 1 or len(rows[0]) != 4 or rows[0][0] != str(source / BASE / 'candidate/src/adapter.rs') or rows[0][1] != '42' or rows[0][2] != 'with_existing_identity' or not Path(rows[0][3]).is_relative_to(libp2p_root):
                        raise RuntimeError('static call context mismatch')
                else:
                    completed[name] = validate_partition(name, texts[name], rows)
                receipt.update(status='DECODED_VALIDATED', csv_sha256=runtime.digest(result),
                    metrics=completed.get(name), endpoint_only=(name == 'Endpoints'))
            except BaseException as exc:
                failure = exc
                receipt.update(status='FAILED_OR_NOT_MEASURED', error=repr(exc)[:8192])
            finally:
                # Persist successful partial results immediately, before any summary/next query.
                if len((json.dumps(receipt,indent=2)+'\n').encode()) > runtime.MAX_CONTROL_RECEIPT:
                    raise RuntimeError('partition terminal size admission exceeded')
                runtime.save(out / (name + '-terminal.json'), receipt, out, terminal=True)
            if evaluator.exists():
                # Only success or an exclusive clean query timeout permits postprocessing.
                safe_error = failure if isinstance(failure, runtime.PhaseError) else None
                if failure is None or (safe_error is not None and safe_error.clean_timeout and
                                       safe_error.result['argv'][1:3] == ['database', 'run-queries']):
                    try:
                        summary = runtime.official_summaries(cli, evaluator, work, out, args.deadline-20, env, safe_error)
                        save(name + '-summary-status.json', summary)
                        if any(isinstance(value, dict) or value == 'NOT_REACHED_FIXED_DEADLINE' for value in summary.values()):
                            raise RuntimeError('official summary incomplete; no further partition')
                        observation = runtime.stage_observation(evaluator)
                        runtime.save(out / (name + '-stage-observation.json'), observation, out)
                    except BaseException as exc:
                        receipt['postprocessing_error'] = repr(exc)[:4096]
                        runtime.save(out / (name + '-terminal.json'), receipt, out, terminal=True)
                        if failure is None:
                            failure = exc
            if failure is not None:
                raise failure
        if args.probe == 'full':
            complete_union(completed)
        save('model-result.json', {'status': 'SELECTED_PARTITIONS_DECODED', 'probe': args.probe,
            'completed': completed, 'source': SOURCE, 'full_candidate_witness_union': args.probe == 'full',
            'original_batch_performance': 'NOT_CLAIMED', 'full_rust_coverage': 'HOLD',
            'other_contexts': 'legacy/image NOT_REACHED', 'endpoint_values_never_waive_flow_oracles': True})
    except BaseException as exc:
        primary = exc
        raise
    finally:
        # Preserve the first failure; do not start subprocesses after unsafe cleanup.
        try:
            after = {name: (digest(source / name) if (source / name).is_file() and not (source / name).is_symlink() else None) for name in before}
            official_after = inventory()
            runtime.save(out / 'protected-after.json', after, out, terminal=True)
            official_receipt = {'unchanged': official_after == official_before, 'hashes': official_after}
            if set(official_after) != set(official_before):
                official_receipt = {'unchanged': False, 'error': 'official member set changed; frozen size bound invalid',
                    'added_count': len(set(official_after)-set(official_before)),
                    'removed_count': len(set(official_before)-set(official_after))}
            runtime.save(out / 'official-after.json', official_receipt, out, terminal=True)
            if before != after or official_before != official_after:
                raise RuntimeError('immutable source/official pack changed')
        except BaseException:
            if primary is None:
                raise
