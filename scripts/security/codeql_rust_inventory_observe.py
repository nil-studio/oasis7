"""One private instrumented extraction; no omitted sources or model queries."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import stat
import time
import codeql_rust_diagnostic_runtime as runtime

BUSINESS = '49b25e1ab631ffb433bce88709fa5601720cfc17'
BUNDLE_SHA = '218a19c49a4e44dee236d2cc55e20cda03ac4f37ddc581b1b0e5fff2e7912237'
EXTRACTOR = 'rust/tools/linux64/extractor'
TOOLCHAIN_SHA = '201d7d298734cc7dd4f16a41c6e25dcc126a61b3349f8700b11a676eb88a45fa'


def tree_manifest(root):
    root = root.resolve()
    result = {}
    for path in sorted(root.rglob('*')):
        key = str(path.relative_to(root)); mode = path.lstat().st_mode
        item = {'execmode': stat.S_IMODE(mode)&0o111}
        if stat.S_ISLNK(mode):
            target = os.readlink(path)
            resolved = path.resolve(strict=True)
            if os.path.isabs(target) or not resolved.is_relative_to(root):
                raise RuntimeError('escaping/absolute bundle symlink')
            item.update(type='symlink',target=target,normalized_target=str(resolved.relative_to(root)))
        elif stat.S_ISDIR(mode): item.update(type='directory')
        elif stat.S_ISREG(mode): item.update(type='regular',bytes=path.stat().st_size,sha256=runtime.digest(path))
        else: raise RuntimeError('nonregular bundle member')
        result[key] = item
    return result


def tree_digest(value):
    return hashlib.sha256(json.dumps(value,sort_keys=True,separators=(',',':')).encode()).hexdigest()


def verify_replacement(original, candidate, binary_sha):
    if set(original)!=set(candidate) or EXTRACTOR not in original:
        raise RuntimeError('private bundle member set differs')
    changed = [name for name in original if original[name]!=candidate[name]]
    if changed != [EXTRACTOR] or original[EXTRACTOR]['type']!='regular':
        raise RuntimeError('private bundle differs beyond extractor executable')
    expected = dict(original[EXTRACTOR],sha256=binary_sha,bytes=candidate[EXTRACTOR]['bytes'])
    if candidate[EXTRACTOR]!=expected:
        raise RuntimeError('replacement type/mode/hash differs')


def copy_regular(source,target,end,guard):
    if not stat.S_ISREG(source.lstat().st_mode):
        raise RuntimeError('copy source is not a regular owned file')
    if source.stat().st_size>runtime.DATA_LIMIT:
        raise RuntimeError('owned binary/data file exceeds1GiB')
    target.parent.mkdir(parents=True,exist_ok=True)
    with source.open('rb') as incoming,target.open('xb') as outgoing:
        while True:
            if time.monotonic()>=end: raise TimeoutError('copy fixed deadline exhausted')
            chunk=incoming.read(1024**2)
            if not chunk:break
            outgoing.write(chunk)
    shutil.copymode(source,target)
    guard()


def copy_bundle(original,candidate,manifest,end,guard):
    candidate.mkdir(exist_ok=False)
    for name,item in manifest.items():
        if time.monotonic()>=end:raise TimeoutError('bundle copy fixed deadline exhausted')
        target=candidate/name
        if item['type']=='directory':
            target.mkdir(parents=True,exist_ok=True);shutil.copymode(original/name,target)
        elif item['type']=='symlink':
            target.parent.mkdir(parents=True,exist_ok=True);target.symlink_to(item['target'])
        else:copy_regular(original/name,target,end,guard)
    guard()


def parse_inventory(logs,initial,wanted_manifests):
    rows=[];unknown=[];startups=[];ansi=re.compile(r'\x1b\[[0-9;]*m')
    for label,text in logs:
        for number,line in enumerate(text.splitlines(),1):
            clean=ansi.sub('',line)
            if 'OASIS7_INVENTORY_STARTUP' in clean:
                match=re.search(r'OASIS7_INVENTORY_STARTUP (\{.*\})$',clean)
                try:
                    marker=json.loads(match[1]) if match else None
                    if not isinstance(marker,dict) or set(marker)!= {'process_id','phase'} or type(marker['process_id']) is not int or marker['process_id']<=0 or marker['phase']!='main_entry':raise ValueError('bad startup')
                    startups.append(marker)
                except (ValueError,TypeError):unknown.append({'stream':label,'line':number,'reason':'unknown startup marker'})
                continue
            if 'OASIS7_INVENTORY_PHASE' in clean:continue
            if 'OASIS7_INVENTORY' not in clean:continue
            match=re.search(r'\bINFO\s+OASIS7_INVENTORY (\{.*\})$',clean)
            if not match:
                unknown.append({'stream':label,'line':number,'reason':'unknown framing'});continue
            try:
                row=json.loads(match[1])
                if not isinstance(row,dict) or row.get('kind') not in ('file','summary','without_semantics','without_semantics_summary'):
                    raise ValueError('unknown record kind')
                if type(row.get('process_id')) is not int or row['process_id']<=0 or row.get('extraction_action')!='UNCHANGED':
                    raise ValueError('missing process/action identity')
                if row['kind']=='file' and (row.get('phase') not in ('initial','vfs') or
                    not isinstance(row.get('path'),str) or not isinstance(row.get('manifest'),str) or
                    not isinstance(row.get('selected_library'),bool)):
                    raise ValueError('unknown file schema')
                count_fields={'summary':('rows','dropped_rows','initial_observations','vfs_observations','selected_library_observations'),
                    'without_semantics_summary':('observations','dropped_rows')}.get(row['kind'],())
                if any(type(row.get(k)) is not int or row[k]<0 for k in count_fields):
                    raise ValueError('unknown counter schema')
                if row['kind']=='summary' and (not isinstance(row.get('manifest'),str) or type(row.get('map_incomplete')) is not bool):
                    raise ValueError('unknown summary identity')
                if row['kind'] in ('file','without_semantics') and (type(row.get('observation_incomplete')) is not bool or row.get('category') not in
                    ('KEEP_EXPLICIT_INPUT','KEEP_POSITIVE_INCLUDE','KEEP_PHYSICAL_MODULE','KEEP_UNKNOWN_NO_PHYSICAL_MODULE','KEEP_BUILTIN','KEEP_UNKNOWN_NO_SEMANTICS')):
                    raise ValueError('unknown classification schema')
                if row['kind']=='without_semantics' and (not isinstance(row.get('path'),str) or type(row.get('library')) is not bool):
                    raise ValueError('unknown fallback schema')
                rows.append(row)
            except (ValueError,TypeError):unknown.append({'stream':label,'line':number,'reason':'unknown JSON/schema'})
    files=[row for row in rows if row['kind']=='file']
    summaries=[row for row in rows if row['kind']=='summary']
    keys=[(row.get('process_id'),row.get('manifest'),row.get('phase'),row.get('path')) for row in files]
    duplicates=len(keys)!=len(set(keys))
    observed_initial={row.get('path') for row in files if row.get('phase')=='initial'}
    fallback=[row for row in rows if row['kind']=='without_semantics']
    covered=observed_initial|{row.get('path') for row in fallback if not row.get('library')}
    manifests={row.get('manifest') for row in summaries}
    reconciliations=[]
    summary_keys=[(r.get('process_id'),r.get('manifest')) for r in summaries]
    for summary in summaries:
        matching=[r for r in files if (r.get('process_id'),r.get('manifest'))==(summary.get('process_id'),summary.get('manifest'))]
        actual={'rows':len(matching),'initial_observations':sum(r.get('phase')=='initial' for r in matching),
            'vfs_observations':sum(r.get('phase')=='vfs' for r in matching),
            'selected_library_observations':sum(r.get('selected_library') is True for r in matching)}
        reconciliations.append({'process_id':summary.get('process_id'),'manifest':summary.get('manifest'),
            'decoded':actual,'matches_reported':all(summary.get(k)==value for k,value in actual.items())})
    fallbacks=[r for r in rows if r['kind']=='without_semantics_summary']
    fallback_keys=[r.get('process_id') for r in fallbacks]
    file_contexts={(r.get('process_id'),r.get('manifest')) for r in files}
    summary_contexts=set(summary_keys)
    all_observed_pids={r.get('process_id') for r in files+fallback}
    context_coverage=file_contexts==summary_contexts
    fallback_coverage=set(fallback_keys)==all_observed_pids
    fallback_ok=bool(fallbacks) and fallback_coverage and len(fallback_keys)==len(set(fallback_keys)) and all(
        r.get('observations')==sum(x.get('process_id')==r.get('process_id') for x in fallback)+r.get('dropped_rows',0) for r in fallbacks)
    incomplete=bool(not context_coverage or len(summary_keys)!=len(set(summary_keys)) or not fallback_ok or any(not r['matches_reported'] for r in reconciliations) or unknown or duplicates or covered!=set(initial) or manifests!=set(wanted_manifests) or
        any(row.get('dropped_rows',0) or row.get('map_incomplete',False) or row.get('observation_incomplete',False) for row in rows))
    if not rows:incomplete=True
    return {'status':'OBSERVED_INCOMPLETE' if incomplete else 'OBSERVED_ZERO_OMISSION_CLASSIFICATION',
        'startup_markers':startups,'unknown_framing':unknown,'duplicate_context_rows':duplicates,'initial_covered':sorted(covered),
        'initial_expected':sorted(initial),'manifest_contexts':sorted(manifests),
        'reconciliations':reconciliations,'context_summaries_cover_all_rows':context_coverage,
        'counter_evidence_scope':'observer row/counter self-consistency; not independently measured extraction-call counts',
        'fallback_reconciled':fallback_ok,
        'summaries':summaries,'fallback_summary':[row for row in rows if row['kind']=='without_semantics_summary'],
        'category_counts':{category:sum(r.get('category')==category for r in rows)
            for category in sorted({r['category'] for r in rows if 'category'in r})},
        'rows':rows,'extraction_action':'UNCHANGED','safe_exclusion':False,'closure_proven':False}


def verify_logging_test(text):
    name='inventory::tests::logging_target_contract'
    if (not re.search(r'^running 1 test$',text,re.M) or
        not re.search(r'^test '+re.escape(name)+r' \.\.\. ok$',text,re.M) or
        not re.search(r'^test result: ok[.] 1 passed; 0 failed; 0 ignored; 0 measured; [0-9]+ filtered out;',text,re.M)):
        raise RuntimeError('logging contract exact one-test execution unverified')


def safe_log_collection(result):
    if not isinstance(result,dict) or not all(k in result for k in ('exit','error','failure_kind','cleanup_errors','evidence_errors')):return False
    if result['cleanup_errors']!=[] or result['evidence_errors']!=[]:return False
    return (result['exit']==0 and result['error'] is None and result['failure_kind'] is None) or (result['failure_kind']=='timeout' and isinstance(result['error'],str) and result['error'].startswith('TimeoutError('))


def collect_database_logs(database,out):
    """After safe process cleanup only; no subprocess, no DB/cache upload."""
    root=database/'log'; receipt={'status':'INCOMPLETE','files':[]}
    if not root.exists():
        receipt['reason']='owned database log directory absent'
        runtime.save(out/'database-log-collection.json',receipt,out);return
    identity=root.lstat()
    if not stat.S_ISDIR(identity.st_mode) or root.is_symlink():raise RuntimeError('unsafe database log root')
    destination=out/'database-logs';destination.mkdir(exist_ok=False)
    count=0
    def walk_error(exc):raise exc
    for folder,dirs,files in os.walk(root,followlinks=False,onerror=walk_error):
        for name in dirs:
            if (Path(folder)/name).is_symlink():raise RuntimeError('database log directory symlink')
        for name in sorted(files):
            count+=1
            path=Path(folder)/name;relative=path.relative_to(root);before=path.lstat()
            if count>64 or len('database-logs/'+str(relative))>160:raise RuntimeError('database log count/path limit')
            if not stat.S_ISREG(before.st_mode):raise RuntimeError('nonregular database log')
            target=destination/relative;target.parent.mkdir(parents=True,exist_ok=True)
            with path.open('rb') as source,target.open('xb') as sink:
                opened=os.fstat(source.fileno())
                if (opened.st_dev,opened.st_ino)!=(before.st_dev,before.st_ino):raise RuntimeError('database log identity changed')
                for chunk in iter(lambda:source.read(65536),b''):
                    if runtime.output_bytes(out)+len(chunk)>runtime.WRITE_CAP:raise RuntimeError('combined evidence budget exceeded before log write')
                    sink.write(chunk);sink.flush()
                after=os.fstat(source.fileno())
            fresh=path.lstat()
            if (after.st_dev,after.st_ino,after.st_size,after.st_mtime_ns)!=(before.st_dev,before.st_ino,before.st_size,before.st_mtime_ns) or (fresh.st_dev,fresh.st_ino)!=(before.st_dev,before.st_ino):raise RuntimeError('database log changed during collection')
            receipt['files'].append({'path':str(relative),'bytes':after.st_size,'sha256':runtime.digest(target)})
    fresh=root.lstat()
    if (fresh.st_dev,fresh.st_ino)!=(identity.st_dev,identity.st_ino):raise RuntimeError('database log root changed')
    receipt['status']='COLLECTED_COMPLETE_DIRECTORY_SNAPSHOT'
    runtime.save(out/'database-log-collection.json',receipt,out)


def prepare_business(run,work,source_end):
    business=work/'business';business.mkdir()
    for argv,name in [(['git','init','--quiet'],'init'),
        (['git','remote','add','origin','https://github.com/nil-studio/oasis7.git'],'remote'),
        (['git','config','gc.auto','0'],'gc'),(['git','config','maintenance.auto','false'],'maintenance'),
        (['git','fetch','--filter=blob:none','--depth=1','origin',BUSINESS],'fetch'),
        (['git','checkout','--detach',BUSINESS],'checkout')]:
        run(argv,'business-'+name,max(0,source_end-time.monotonic()),cwd=business)
    return business.resolve()


def execute(args,env,end,out,work,source,business,run,overlay,source_snapshot,guard,terminal,toolchain_end):
    original_manifest=None;candidate_manifest=None;official=None;copied=None;protected=None;before_source=None;primary=None
    last_guard = 0
    def copy_guard():
        nonlocal last_guard
        if time.monotonic()-last_guard >= 2:
            guard(); last_guard=time.monotonic()
    try:
        run(['rustup','toolchain','install','1.96.0','1.99.0','--profile','minimal','--component','rust-src'],
            'stable-toolchains',max(0,toolchain_end-time.monotonic()))
        for version in ('1.96.0','1.99.0'):
            run(['rustup','run',version,'rustc','-vV'],'rustc-'+version,10)
        if runtime.digest(source/'rust/extractor/src/toolchain.rs')!=TOOLCHAIN_SHA:
            raise RuntimeError('fixed source analysis toolchain contract differs')
        bundle_end=min(end-90,time.monotonic()+120)
        archive=work/'bundle.tar.zst'
        run(['curl','--fail','--location','--max-time','120',
            'https://github.com/github/codeql-action/releases/download/codeql-bundle-v2.27.3/codeql-bundle-rust-linux64.tar.zst',
            '--output',str(archive)],'bundle-download',max(0,bundle_end-time.monotonic()))
        if runtime.digest(archive)!=BUNDLE_SHA:raise RuntimeError('bundle digest differs')
        unpack=work/'official-bundle';unpack.mkdir()
        run(['tar','--zstd','--no-same-owner','--no-same-permissions','-xf',str(archive),'-C',str(unpack)],
            'bundle-unpack',max(0,bundle_end-time.monotonic()))
        official=unpack/'codeql';original_manifest=tree_manifest(official)
        executable=original_manifest.get(EXTRACTOR,{})
        if executable.get('type')!='regular' or not executable.get('execmode'):
            raise RuntimeError('official Linux extractor identity/path is not regular executable')
        schema_sha=runtime.digest(out/'source-schema.log')
        for name in ('rust/rust.dbscheme','qlpacks/codeql/rust-all/0.2.24/rust.dbscheme'):
            if original_manifest.get(name,{}).get('sha256')!=schema_sha:
                raise RuntimeError('source/released extractor/library database scheme differs')
        runtime.save(out/'schema-identity.json',{'source':schema_sha,'release':schema_sha,'library':schema_sha},out)
        runtime.save(out/'official-bundle-before.json',{'tree_sha256':tree_digest(original_manifest),
            'members':len(original_manifest),'entries':original_manifest},out)
        build_end=min(end-90,time.monotonic()+360)
        before_source=source_snapshot(source)
        runtime.save(out/'source-built-original.json',before_source,out)
        run(['rustup','run','nightly-2026-09-03','cargo','build','--locked','-p','codeql-rust','--bin','codeql-rust','--jobs','2'],
            'original-binary-build',max(0,build_end-time.monotonic()))
        if source_snapshot(source)!=before_source:raise RuntimeError('original build changed source')
        built=Path(env['CARGO_TARGET_DIR'])/'debug/codeql-rust'
        original_binary=work/'original-bin/codeql-rust';copy_regular(built,original_binary,end-90,copy_guard)
        runtime.save(out/'overlay.json',overlay.apply(source),out)
        before_source=source_snapshot(source)
        runtime.save(out/'instrumented-source-before.json',before_source,out)
        run(['rustup','run','nightly-2026-09-03','cargo','build','--locked','-p','codeql-rust','--bin','codeql-rust','--jobs','2'],
            'instrumented-binary-build',max(0,build_end-time.monotonic()))
        terminal['test_binary_attempts']=1
        terminal['test_binary_executions']='UNKNOWN'
        terminal['test_binary_scope']='inventory::tests::logging_target_contract ONLY'
        run(['rustup','run','nightly-2026-09-03','cargo','test','--locked','--jobs','2','-p','codeql-rust','--bin','codeql-rust',
            'inventory::tests::logging_target_contract','--','--exact'],
            'inventory-logging-contract-test',max(0,build_end-time.monotonic()))
        verify_logging_test((out/'inventory-logging-contract-test.log').read_text())
        terminal['test_binary_executions']=1
        instrumented=work/'instrumented-bin/codeql-rust';copy_regular(built,instrumented,end-90,copy_guard)
        (work/'instrumented-bundle').mkdir(exist_ok=False)
        copied=work/'instrumented-bundle/codeql';copy_bundle(official,copied,original_manifest,end-90,copy_guard)
        replacement=copied/EXTRACTOR
        if replacement.is_symlink() or not replacement.is_file():raise RuntimeError('unexpected extractor path')
        old_mode=replacement.stat().st_mode;replacement.unlink();copy_regular(instrumented,replacement,end-90,copy_guard)
        replacement.chmod(stat.S_IMODE(old_mode))
        candidate_manifest=tree_manifest(copied);verify_replacement(original_manifest,candidate_manifest,runtime.digest(instrumented))
        runtime.save(out/'private-bundle.json',{'tree_sha256':tree_digest(candidate_manifest),
            'released_extractor':original_manifest[EXTRACTOR],'source_built_original_sha256':runtime.digest(original_binary),
            'instrumented_sha256':runtime.digest(instrumented),'candidate_extractor':candidate_manifest[EXTRACTOR],
            'changed_members':[EXTRACTOR],'original_binary_executions':0},out)
        cli=str(copied/'codeql')
        run([cli,'version','--format=json'],'private-cli-version',20)
        if json.loads((out/'private-cli-version.log').read_text())['version']!='2.27.3':raise RuntimeError('private CLI identity differs')
        spec=importlib.util.spec_from_file_location('real_context',Path(__file__).with_name('codeql-rust-refactored-source.py'))
        model=importlib.util.module_from_spec(spec);spec.loader.exec_module(model)
        run(['git','rev-parse','HEAD'],'business-head',10,cwd=business)
        run(['git','diff','--exit-code'],'business-clean',10,cwd=business)
        if (out/'business-head.log').read_text().strip()!=BUSINESS:raise RuntimeError('business source differs')
        protected=model.protected(business);runtime.save(out/'protected-before.json',protected,out)
        metadata_env=dict(env,RUSTUP_TOOLCHAIN='1.96.0',CARGO_NET_OFFLINE='false',OASIS7_RUST_INVENTORY='0')
        folder=business/model.BASE/'candidate'
        run(['rustup','run','1.96.0','cargo','metadata','--quiet','--locked','--all-features','--format-version=1',
            '--manifest-path',str(folder/'Cargo.toml')],'metadata',90,cwd=business,env_override=metadata_env)
        libp2p=model.verify_metadata(json.loads((out/'metadata.log').read_text()),business,'candidate')
        filters,initial=model.collection(business,'candidate')
        runtime.save(out/'terminal-size-admission.json',runtime.admit_terminal_snapshots(before_source,protected),out)
        runtime.save(out/'context.json',{'business':BUSINESS,'initial_files':{name:protected[name] for name in initial},
            'filters':filters,'libp2p_root':str(libp2p),'project_toolchain':'1.96.0','analysis_toolchain':'1.99.0',
            'build_toolchain':'nightly-2026-09-03','logging_verbosity':'info','rust_log':'single_arch_extractor=info,codeql_extractor=info','inventory':'1'},out)
        common=work/'codeql-common-cache';common.mkdir()
        run([cli,'database','create','--help','-v'],'create-help',20)
        if '--common-caches' not in (out/'create-help.log').read_text():raise RuntimeError('owned common cache option unsupported')
        if end-time.monotonic()<270:raise RuntimeError('NOT_REACHED no extraction180+receipt90 window')
        extract_env=dict(metadata_env,RUST_LOG='single_arch_extractor=info,codeql_extractor=info',CARGO_NET_OFFLINE='true',OASIS7_RUST_INVENTORY='1',
            LGTM_INDEX_FILTERS=filters,CODEQL_EXTRACTOR_RUST_OPTION_CARGO_ALL_TARGETS='true',
            CODEQL_EXTRACTOR_RUST_OPTION_EXTRA_INCLUDES=json.dumps([str(business/model.BASE/'common')]),
            CODEQL_EXTRACTOR_RUST_OPTION_LOGGING_VERBOSITY='info')
        database=work/'candidate-inventory-db'
        terminal.update(database_create_attempts=1,extractor_executions='INTERNAL_CHILDREN_NOT_INDEPENDENTLY_COUNTED',extractions=1)
        extract_primary=None
        try:
            run([cli,'database','create',str(database),'--language=rust','--build-mode=none','--source-root='+str(business),
                '--threads=2','--ram=3072','--common-caches='+str(common)],'inventory-extract',180,cwd=business,env_override=extract_env)
        except BaseException as exc:
            extract_primary=exc;raise
        finally:
            try:
                result_path=out/'inventory-extract.log.result.json'
                result=json.loads(result_path.read_text()) if result_path.is_file() else None
                safe=safe_log_collection(result)
                if safe:collect_database_logs(database,out)
                else:runtime.save(out/'database-log-collection.json',{'status':'NOT_COLLECTED_UNSAFE_OR_UNKNOWN'},out)
            except BaseException as exc:
                terminal['database_log_collection_error']=repr(exc)[:512]
                if extract_primary is None:raise
        stdout=(out/'inventory-extract.log').read_text();stderr=(out/'inventory-extract.log.stderr').read_text()
        internal=[(str(p.relative_to(out)),p.read_text(errors='replace')) for p in (out/'database-logs').rglob('*') if p.is_file()]
        text=stdout+stderr+''.join(value for _,value in internal)
        if 'number of inputs: '+str(len(initial))+',' not in text or 'cargo_all_targets: true' not in text or 'cargo_features: []' not in text or 'extract_dependencies_as_source: false' not in text or 'force_library_mode: false' not in text:
            raise RuntimeError('actual complete real context configuration unverified')
        if 'project Rust toolchain: 1.96.0' not in text or '1.99.0-' not in text:
            raise RuntimeError('actual project/analysis toolchain not observed')
        selected=[model.DRIVER,model.BASE+'/common/src/protocol.rs',model.BASE+'/candidate/src/adapter.rs']
        archived=model.verify_archive(database,selected,protected,lambda value:runtime.save(out/'archive-index.json',value,out))
        runtime.save(out/'archived-source.json',archived,out)
        import zipfile
        with zipfile.ZipFile(database/'src.zip') as z:
            toolchains={match[1] for name in z.namelist() for match in [re.search(r'(?:^|/)toolchains/([^/]+)/lib/rustlib/src/rust/library/',name)] if match}
        if not any(part.startswith('1.99.0-') for part in toolchains) or any(part.startswith(('1.98.','nightly')) for part in toolchains):
            raise RuntimeError('analysis sysroot archive identity differs')
        runtime.save(out/'analysis-archive-toolchains.json',sorted(toolchains),out)
        observed=parse_inventory([('stdout',stdout),('stderr',stderr)]+internal,
            [str(business/name) for name in initial],[str(folder/'Cargo.toml'),str(business/model.BASE/'common/Cargo.toml')])
        runtime.save(out/'inventory-classification.json',observed,out)
        terminal.update(status=observed['status'],extraction_action='UNCHANGED',safe_exclusion=False)
    except BaseException as exc:
        primary=exc;raise
    finally:
        errors=[]
        for label,fn in [('official-bundle',lambda:tree_digest(tree_manifest(official))==tree_digest(original_manifest) if original_manifest else True),
            ('private-bundle',lambda:tree_digest(tree_manifest(copied))==tree_digest(candidate_manifest) if candidate_manifest else True),
            ('instrumented-source',lambda:source_snapshot(source)==before_source if before_source else True),
            ('business-source',lambda:model.protected(business)==protected if protected else True)]:
            try:
                equal=fn();runtime.save(out/(label+'-after.json'),{'unchanged':equal},out,terminal=True)
                if not equal:errors.append(label+' changed')
            except BaseException as exc:errors.append(label+':'+repr(exc)[:512])
        if errors:
            terminal.update(status='FAILED',verification_errors=errors)
            if primary is None:raise RuntimeError('post-observation identity verification failed')
