#!/usr/bin/env python3
"""Capacity assessment source, completeness and resource-boundary regressions."""
import importlib.util
import json
import hashlib
import os
from pathlib import Path
import resource
import re
import subprocess
import sys
import tempfile
import textwrap
import time
import unittest
from unittest.mock import patch

P=Path(__file__).with_name('codeql-rust-capacity.py')
spec=importlib.util.spec_from_file_location('capacity',P);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
ROOT=Path('/frozen')

def workflow_steps():
    # Select this workflow's literal step blocks without adding a test dependency.
    text=(P.parents[2]/'.github/workflows/codeql-rust-capacity.yml').read_text()
    steps=re.split(r'^      - ',text,flags=re.MULTILINE)[1:]
    by_id={s.splitlines()[0][4:]:s for s in steps if s.startswith('id: ')}
    return steps,by_id

class Capacity(unittest.TestCase):
    def test_source_pin_cli_defaults_and_allowed_exclusion_candidate(self):
        for selected in (None,m.BASE_SOURCE,m.EXCLUSION_SOURCE,m.JSON_FIXTURE_SOURCE,m.DECOMPOSITION_SOURCE):
            with tempfile.TemporaryDirectory() as tmp:
                root=Path(tmp); source=root/"source";source.mkdir()
                argv=["capacity","capture","--source",str(source),"--database",str(root/"db"),"--output",str(root/"out"),"--work",str(root/"work"),"--sarif",str(root/"sarif"),"--deadline",str(time.time()+300)]
                if selected:argv += ["--source-sha",selected]
                observed=[]
                def capture(args,source,out,end,env,terminal):observed.append((m.SOURCE,terminal["source"]))
                with patch.object(m,"SOURCE",m.BASE_SOURCE),patch.object(sys,"argv",argv),patch.object(m,"capture_failure_evidence",side_effect=capture):m.main()
                self.assertEqual(observed,[(selected or m.EXCLUSION_SOURCE,selected or m.EXCLUSION_SOURCE)])
                with patch.object(sys,"argv",argv+["--source-sha","f"*40]),patch.object(m.argparse.ArgumentParser,"error",side_effect=ValueError("rejected")):
                    with self.assertRaises(ValueError):m.main()

    def test_failed_analysis_dirty_checkout_and_missing_resources_still_capture(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);source=root/'source';source.mkdir();out=root/'out';out.mkdir()
            subprocess.run(['git','init','--quiet',str(source)],check=True)
            (source/'file.rs').write_text('before')
            subprocess.run(['git','-C',str(source),'add','file.rs'],check=True)
            subprocess.run(['git','-C',str(source),'-c','user.name=Capacity Test','-c','user.email=capacity@example.invalid',
                            '-c','core.hooksPath=/dev/null','commit','--quiet','-m','Fixture'],check=True)
            (source/'file.rs').write_text('changed')
            log=root/'db/log/action.log';log.parent.mkdir(parents=True);log.write_text('original analysis failure\n')
            deadline=time.time()+400
            argv=['capacity','capture','--source',str(source),'--database',str(root/'db'),
                '--output',str(out),'--work',str(root/'work'),'--sarif',str(root/'sarif'),
                '--deadline',str(deadline),'--init-outcome','success','--prepare-outcome','success','--analyze-outcome','failure']
            commands=[];run=m.runtime.run
            def only_git(args,*pos,**kwargs):
                commands.append(args)
                self.assertEqual(args[0],'git','failure capture must not start CodeQL')
                return run(args,*pos,**kwargs)
            with patch.object(sys,'argv',argv),patch.object(m,'resources',side_effect=RuntimeError('missing resource exports')),patch.object(m.runtime,'run',side_effect=only_git):
                m.main()
            self.assertEqual((out/'database-logs/action.log').read_bytes(),log.read_bytes())
            identity=json.loads((out/'capture-identity.json').read_text())
            self.assertNotEqual(identity['identity_gate'],'PASS')
            self.assertNotEqual(identity['clean_gate'],'PASS')
            self.assertTrue(any('status' in args for args in commands))
            terminal=json.loads((out/'capacity-capture-terminal.json').read_text())
            self.assertEqual(terminal['primary_failure'],{'stage':'analyze','outcome':'failure'})
            self.assertEqual(terminal['analyze_outcome'],'failure')
            self.assertEqual(terminal['default_result_verification'],'NOT_REACHED')
            self.assertEqual(terminal['public_semantics'],'NOT_REACHED')
            self.assertEqual(terminal['action_cleanup'],'UNKNOWN_RUNNER_OWNED')
            self.assertEqual(terminal['fixed_job_deadline'],deadline)
            self.assertEqual(terminal['resource_gate'],'FAIL')
            argv[1]='finish';argv+=['--codeql',str(root/'codeql')]
            with patch.object(sys,'argv',argv),patch.object(m.runtime,'run',side_effect=AssertionError('no CodeQL after failure')):
                with self.assertRaisesRegex(RuntimeError,'ACTION_ANALYSIS_FAILED_OR_INCOMPLETE'):m.main()
            final=json.loads((out/'capacity-finish-terminal.json').read_text())
            self.assertEqual(final['primary_failure'],terminal['primary_failure'])
            self.assertEqual(final['public_semantics'],'NOT_REACHED')

    def test_capture_without_prepare_or_remaining_budget_writes_terminal(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);out=root/'out';deadline=time.time()-1
            argv=['capacity','capture','--source',str(root/'missing-source'),'--database',str(root/'missing-db'),
                '--output',str(out),'--work',str(root/'work'),'--sarif',str(root/'sarif'),
                '--deadline',str(deadline),'--init-outcome','failure','--prepare-outcome','skipped','--analyze-outcome','skipped']
            with patch.object(sys,'argv',argv),patch.object(m.runtime,'run',side_effect=AssertionError('no child after deadline')):
                m.main()
            terminal=json.loads((out/'capacity-capture-terminal.json').read_text())
            self.assertEqual(terminal['primary_failure'],{'stage':'init','outcome':'failure'})
            self.assertEqual(terminal['checkout_gate'],'NOT_REACHED')
            self.assertEqual(terminal['public_semantics'],'NOT_REACHED')
            self.assertEqual(terminal['fixed_job_deadline'],deadline)

    def test_action_resources_must_be_real_exports(self):
        with self.assertRaises(RuntimeError):m.resources({})
        with self.assertRaises(RuntimeError):m.resources({'CODEQL_RAM':'3072','CODEQL_THREADS':'auto'})
        selected=m.resources({'CODEQL_RAM':'14848','CODEQL_THREADS':'4'})
        self.assertEqual((selected['CODEQL_RAM'],selected['CODEQL_THREADS']),(14848,4))

    def test_action_source_and_cache_must_bind_frozen_context(self):
        config={'dbLocation':'/data','repositoryRoot':'/frozen','languages':['rust'],'buildMode':'none','overlayDatabaseMode':'none','trapCaches':{},'useOverlayDatabaseCaching':False}
        m.verify_action_config(config,ROOT,Path('/data/rust'))
        for field,value in [('repositoryRoot','/tools'),('buildMode','manual'),('overlayDatabaseMode','overlay'),('trapCaches',{'rust':'/cache'}),('useOverlayDatabaseCaching',True)]:
            with self.assertRaises(RuntimeError):m.verify_action_config(dict(config,**{field:value}),ROOT,Path('/data/rust'))

    def test_official_input_mutation_distinct_from_generated_cache(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);official=root/'tools';official.mkdir();out=root/'out';out.mkdir()
            (official/'input.qll').write_text('original')
            (out/'official-before.json').write_text(json.dumps(m.official_snapshot(official)))
            (official/'new.qlx').write_text('generated')
            value=m.after_identity(ROOT,official,out)
            self.assertEqual(value['official'],'UNCHANGED');self.assertEqual(value['additional_official_paths'],['new.qlx'])
            (official/'input.qll').write_text('mutated')
            self.assertEqual(m.after_identity(ROOT,official,out)['official'],'CHANGED')

    def test_final_identity_never_spawns_after_primary_failure(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);source=root/'source';source.mkdir();official=root/'tools';official.mkdir();out=root/'out';out.mkdir()
            (source/'a.rs').write_text('original')
            frozen={'a.rs':{'sha256':m.runtime.digest(source/'a.rs'),'bytes':8}}
            (out/'source-before.json').write_text(json.dumps(frozen))
            with patch.object(m.subprocess,'check_output',side_effect=AssertionError('no new child after failed primary')):
                self.assertEqual(m.after_identity(source,official,out)['source'],'UNCHANGED')
                (source/'a.rs').unlink()
                self.assertEqual(m.after_identity(source,official,out)['source'],'CHANGED')

    def test_real_gitlinks_are_explicit_without_reading_payload(self):
        import subprocess
        with tempfile.TemporaryDirectory() as temp:
            source=Path(temp)
            subprocess.run(['git','init','--quiet',str(source)],check=True)
            (source/'regular.rs').write_text('tracked')
            subprocess.run(['git','-C',str(source),'add','regular.rs'],check=True)
            oid='a'*40
            for name in ('third_party/absent','third_party/present'):
                subprocess.run(['git','-C',str(source),'update-index','--add','--cacheinfo','160000,'+oid+','+name],check=True)
            payload=source/'third_party/present';payload.mkdir(parents=True)
            (payload/'foreign').write_text('must not inspect')
            value=m.snapshot(source)
            self.assertEqual(value['third_party/absent'],{'mode':'160000','gitlink':oid})
            self.assertEqual(value['third_party/present'],{'mode':'160000','gitlink':oid})
            self.assertNotIn('third_party/present/foreign',value)
            (source/'regular.rs').unlink()
            with self.assertRaises(RuntimeError):m.snapshot(source)

    def test_primary_failure_terminal_uses_reserved_evidence_tail(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);out=root/'evidence'
            argv=['capacity','prepare','--codeql',str(root/'tools/codeql'),'--source',str(root/'source'),
                '--database',str(root/'db'),'--output',str(out),'--work',str(root/'work'),'--sarif',str(root/'sarif'),
                '--deadline',str(time.time()+120)]
            old=(m.runtime.CAP,m.runtime.WRITE_CAP)
            def fail_resources(_):
                m.runtime.CAP=4096;m.runtime.WRITE_CAP=256
                (out/'prior-evidence').write_bytes(b'x'*256)
                raise RuntimeError('preserve original failure')
            try:
                with patch.object(sys,'argv',argv),patch.object(m,'resources',side_effect=fail_resources):
                    with self.assertRaisesRegex(RuntimeError,'preserve original failure'):m.main()
                terminal=json.loads((out/'capacity-prepare-terminal.json').read_text())
                self.assertIn('preserve original failure',terminal['error'])
                self.assertEqual(terminal['status'],'FAILED')
                self.assertLess(sum(p.stat().st_size for p in out.iterdir()),4096)
            finally:m.runtime.CAP,m.runtime.WRITE_CAP=old

    def test_real_finally_identity_failure_is_failed_and_preserves_primary(self):
        import ast
        from types import SimpleNamespace
        tree=ast.parse(P.read_text());main=next(n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name=='main')
        guarded=next(n for n in main.body if isinstance(n,ast.Try) and n.finalbody)
        code=compile(ast.fix_missing_locations(ast.Module(body=guarded.finalbody,type_ignores=[])),str(P),'exec')
        with tempfile.TemporaryDirectory() as temp:
            out=Path(temp)
            for outcome in (RuntimeError('identity read failed'),{'source':'CHANGED'}):
                terminal={'status':'CAPACITY_COMPLETED'}
                namespace={'args':SimpleNamespace(deadline=time.time()+60,phase='finish',codeql=out/'codeql'),'out':out,'source':out,
                    'terminal':terminal,'primary':None,'time':time,'end':time.monotonic()+300,'after_identity':lambda *_,**__:None,'runtime':m.runtime}
                def identity(*_,**__):
                    if isinstance(outcome,Exception):raise outcome
                    return outcome
                namespace['after_identity']=identity
                with self.assertRaises(RuntimeError):exec(code,namespace)
                self.assertEqual(json.loads((out/'capacity-finish-terminal.json').read_text())['status'],'FAILED')
            original=RuntimeError('original query error')
            namespace.update(primary=original,terminal={'status':'FAILED','error':repr(original)},after_identity=lambda *_,**__:(_ for _ in ()).throw(AssertionError('failed work must not rehash')))
            def active_primary():
                try:raise original
                finally:exec(code,namespace)
            with self.assertRaises(RuntimeError) as caught:active_primary()
            self.assertIs(caught.exception,original)
            self.assertIn('original query error',json.loads((out/'capacity-finish-terminal.json').read_text())['error'])

    def test_capacity_pack_binding_rejects_foreign_library(self):
        query=Path('/owned/query');closure={'codeql/rust-all':{'root':'/bundle/rust'}}
        value={'libraryPath':['/owned/query','/bundle/rust'],'dbscheme':'/bundle/rust/rust.dbscheme','compilationCache':[],'relativeName':'oasis7/rust-capacity/Image.ql','qlPackName':'oasis7/rust-capacity'}
        m.bundle.verify_library_path(value,query,'Image',closure,pack_name='oasis7/rust-capacity')
        with self.assertRaises(RuntimeError):m.bundle.verify_library_path(dict(value,libraryPath=['/owned/query','/other/rust']),query,'Image',closure,pack_name='oasis7/rust-capacity')

    def test_suite_freezes_unique_local_query_ids(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);query=root/'x.ql';query.write_text('/** @id rust/test */\nselect 1')
            self.assertEqual(m.suite_entries([str(query)],root)[0]['id'],'rust/test')
            for invalid in ([],[str(query),str(query)],{'query':str(query)},['/foreign.ql']):
                with self.assertRaises(RuntimeError):m.suite_entries(invalid,root)
            query.write_text('/** no id */')
            with self.assertRaises(RuntimeError):m.suite_entries([str(query)],root)

    def test_every_default_resultset_must_be_completely_decoded(self):
        info={'result-sets':[{'name':'#select','rows':1}]};decoded={'#select':{'tuples':[[1]]}}
        m.verify_decoded(info,decoded)
        for bad in ({},{'#select':{'tuples':[]}},{'#select':{'tuples':[[1],[2]]}},dict(decoded,foreign={'tuples':[]})):
            with self.assertRaises(RuntimeError):m.verify_decoded(info,bad)
        with self.assertRaises(RuntimeError):m.verify_decoded({},decoded)

    def test_sarif_missing_invocation_or_execution_error_not_success(self):
        value={'version':'2.1.0','runs':[{'invocations':[{'executionSuccessful':True}],'results':[]} ]}
        self.assertEqual(m.check_sarif(value),0)
        for run in ({},{'invocations':[{}]},{'invocations':[{'executionSuccessful':False}]},{'invocations':[{'executionSuccessful':True,'toolExecutionNotifications':[{'level':'error'}]}]}):
            with self.assertRaises(RuntimeError):m.check_sarif({'version':'2.1.0','runs':[run]})

    def test_complete_semantic_union_and_negative_are_not_weakened(self):
        texts=m.semantic_queries(ROOT)
        self.assertEqual(set(texts),{'Files','PeerCandidate','PeerLegacy','Driver','CallsCandidate','CallsLegacy','Image'})
        self.assertIn('"legacy"',texts['PeerLegacy']);self.assertNotIn('"candidate"',texts['PeerLegacy'])
        for name in ('PeerCandidate','PeerLegacy','Driver','Image'):
            wanted=m.expected(name);rows=[[key,str(value)] for key,value in wanted.items()]
            self.assertEqual(m.validate(name,rows,ROOT,{}),wanted)
            with self.assertRaises(RuntimeError):m.validate(name,rows[:-1],ROOT,{})
            with self.assertRaises(RuntimeError):m.validate(name,rows+rows[:1],ROOT,{})
        rows=[[key,'1'] for key in m.expected('Image')]
        with self.assertRaises(RuntimeError):m.validate('Image',rows,ROOT,{})
        self.assertIn('a.asParameter().toString() = "original_digest"',texts['Image'])
        self.assertIn('b.asExpr().toString()="line == expected"',texts['Image'])

    def test_each_version_callee_requires_own_source_and_dependency(self):
        roots={'candidate':'/git/libp2p','legacy':'/registry/libp2p'}
        row=[str(ROOT/m.model.BASE/'candidate/src/adapter.rs'),'42','with_existing_identity','/git/libp2p/src/builder.rs']
        m.validate('CallsCandidate',[row],ROOT,roots)
        with self.assertRaises(RuntimeError):m.validate('CallsLegacy',[row],ROOT,roots)
        with self.assertRaises(RuntimeError):m.validate('CallsCandidate',[row[:3]+['/registry/libp2p/src/builder.rs']],ROOT,roots)

    def test_research_prepare_never_generates_or_compiles_custom_queries(self):
        self.research_prepare_case()

    def test_metadata_created_lock_stops_before_analysis_even_if_git_gate_is_clean(self):
        self.research_prepare_case(create_lock=True)

    def test_incomplete_identity_observation_stops_metadata_preparation(self):
        self.research_prepare_case(identity_failure=True)

    def test_metadata_failure_remains_primary_after_input_observation(self):
        self.research_prepare_case(metadata_failure=True)

    def test_missing_independent_lock_stops_before_tool_or_graph_commands(self):
        self.research_prepare_case(missing_vendor=True)

    def test_native_prerequisite_failure_stops_before_metadata_or_analysis(self):
        self.research_prepare_case(native_failure=True)

    def research_prepare_case(self,create_lock=False,identity_failure=False,metadata_failure=False,missing_vendor=False,native_failure=False):
        import contextlib
        with tempfile.TemporaryDirectory() as temp, contextlib.ExitStack() as stack:
            root=Path(temp);source=root/'source';source.mkdir();out=root/'out';work=root/'work'
            official=root/'official';suite=official/'qlpacks/codeql/rust-queries/1/codeql-suites/rust-code-scanning.qls'
            suite.parent.mkdir(parents=True);suite.write_text('official')
            query=suite.parent.parent/'official.ql';query.write_text('/** @id rust/official */')
            action=root/'action';action.mkdir();(action/'config').write_text('{}')
            (source/'Cargo.toml').write_text('[workspace]\n')
            (source/'Cargo.lock').write_text('initial lock')
            manifests=['Cargo.toml']
            if missing_vendor:
                (source/'vendor').mkdir();(source/'vendor/Cargo.toml').write_text('[workspace]\n')
                manifests.append('vendor/Cargo.toml')
            frozen={'Cargo.lock':{'sha256':hashlib.sha256(b'initial lock').hexdigest()}}
            commands=[]
            def run(argv,cwd,output,*args,**kwargs):
                commands.append(argv)
                if native_failure and argv[0]=="pkg-config":raise RuntimeError("native prerequisite failed")
                if create_lock and argv[0]=='cargo':(source/'Cargo.lock').write_text('created during metadata')
                self.assertNotIn('compile',argv)
                value={'version':m.BUNDLE} if 'version' in argv else ({'resolve':{'nodes':[1]}} if argv[0]=='cargo' else [str(query)])
                output.write_text(json.dumps(value))
                if metadata_failure and argv[0]=='cargo':raise RuntimeError('original metadata failure')
                return {'exit':0}
            patches=[patch.object(sys,'argv',['capacity','prepare','--mode','official-default-research',
                '--codeql',str(official/'codeql'),'--source',str(source),'--database',str(root/'db'),
                '--output',str(out),'--work',str(work),'--sarif',str(root/'sarif'),'--deadline',str(time.time()+21600)]),
                patch.dict(os.environ,{'CODEQL_ACTION_TEMP':str(action),'GITHUB_OUTPUT':str(root/'outputs'),'GITHUB_ENV':str(root/'github-env')}),
                patch.object(m,'resources',return_value={'CODEQL_THREADS':4,'CODEQL_RAM':14575}),
                patch.object(m.evidence,'capture_identity',side_effect=[{'status':'PASS'},{'status':'PARTIAL'}] if identity_failure else None,return_value={'status':'PASS'}),
                patch.object(m,'snapshot',return_value=frozen),patch.object(m.plan,'inventory',return_value=(manifests,None)),
                patch.object(m,'official_snapshot',return_value={}),patch.object(m.bundle,'bundle_closure',return_value={}),
                patch.object(m.model,'verify_metadata',return_value=Path('/libp2p')),
                patch.object(m,'verify_action_config'),patch.object(m.runtime,'run',side_effect=run),
                patch.object(m,'after_identity',return_value={'source':'UNCHANGED','official':'UNCHANGED'}),
                patch.object(m,'semantic_queries',side_effect=AssertionError('custom queries forbidden'))]
            for item in patches:stack.enter_context(item)
            # Layout admission still precedes expensive work.
            out.mkdir();(out/'collection-layout.json').write_text('{"status":"PASS"}')
            if missing_vendor:
                with self.assertRaisesRegex(RuntimeError,'independent workspace locks are not frozen'):m.main()
                self.assertEqual(commands,[])
                self.assertFalse((root/'outputs').exists())
                rejected=json.loads((out/'independent-workspace-locks.json').read_text())
                self.assertEqual(rejected['rejected_locks'],['vendor/Cargo.lock'])
                return
            if native_failure:
                with self.assertRaisesRegex(RuntimeError,"native prerequisite failed"):m.main()
                self.assertFalse(any(argv[0] in ("cargo",str(official/"codeql")) for argv in commands))
                self.assertFalse((root/"outputs").exists())
                terminal=json.loads((out/"capacity-prepare-terminal.json").read_text())
                self.assertEqual(terminal["native_preparation"],"INCOMPLETE")
                return
            if metadata_failure:
                with self.assertRaisesRegex(RuntimeError,'original metadata failure'):m.main()
                self.assertEqual(len([argv for argv in commands if argv[0]=='cargo']),1)
                self.assertTrue((out/'input-observation-after-metadata-workspace.json').is_file())
                self.assertFalse((root/'outputs').exists())
                return
            if create_lock or identity_failure:
                with self.assertRaisesRegex(RuntimeError,'metadata changed or obscured frozen inputs'):m.main()
                self.assertEqual(len([argv for argv in commands if argv[0]=='cargo']),1)
                self.assertFalse((root/'outputs').exists())
                observed=json.loads((out/'input-observation-after-metadata-workspace.json').read_text())
                self.assertEqual(observed['changed_locks'],['Cargo.lock'] if create_lock else [])
                self.assertEqual(observed['dependency_resolution_impact'],'UNKNOWN')
                return
            m.main()
            self.assertFalse((work/'queries').exists())
            self.assertEqual([call.kwargs['prefix'] for call in m.evidence.capture_identity.call_args_list],
                ['prepare','after-metadata-workspace','after-metadata-candidate',
                 'after-metadata-legacy','after-metadata-common','pre-analyze'])
            terminal=json.loads((out/'capacity-prepare-terminal.json').read_text())
            self.assertEqual(terminal['public_semantics'],'NOT_REQUESTED')
            self.assertEqual(terminal['production_capacity'],'UNKNOWN')
            self.assertGreater(terminal['analysis_minutes'],300)
            self.assertEqual(terminal['native_preparation'],'PASS')
            native_index=next(i for i,argv in enumerate(commands) if argv[0]=='pkg-config')
            metadata_index=next(i for i,argv in enumerate(commands) if argv[0]=='cargo')
            self.assertLess(native_index,metadata_index)
            self.assertFalse(any('check' in argv for argv in commands))

    def test_research_finish_decodes_official_only_and_retains_gates(self):
        import contextlib
        for gate in ('PASS','FAIL','LOCK_FAIL','LOCK_MISSING','CARGO_FAIL','CARGO_MISSING'):
            with self.subTest(gate=gate), tempfile.TemporaryDirectory() as temp, contextlib.ExitStack() as stack:
                root=Path(temp);source=root/'source';source.mkdir();out=root/'out';out.mkdir()
                db=root/'db';(db/'temp').mkdir(parents=True);(db/'temp/config-queries.qls').write_text('official')
                query_root=root/'official';query_root.mkdir();query=query_root/'one.ql';query.write_text('/** @id rust/one */')
                entries=m.suite_entries([str(query)],query_root)
                bqrs=db/'results/codeql/rust-queries/one.bqrs';bqrs.parent.mkdir(parents=True);bqrs.write_bytes(b'fixture')
                sarif=root/'sarif';sarif.mkdir();(sarif/'result.sarif').write_text(json.dumps({'version':'2.1.0','runs':[{'invocations':[{'executionSuccessful':True}]}]}))
                deadline=time.time()+21600
                captured={'mode':'official-default-research','source_root':str(source),'fixed_job_deadline':deadline,
                    'analyze_outcome':'success','checkout_gate':'FAIL' if gate=='FAIL' else 'PASS','lock_observation_gate':'FAIL' if gate=='LOCK_FAIL' else 'PASS','resource_gate':'PASS','evidence_status':'COMPLETE',
                    'coverage_capture':{'status':'PASS'},'cargo_guard':{'status':'FAIL' if gate=='CARGO_FAIL' else 'PASS'},'resources':{'CODEQL_RAM':14575,'CODEQL_THREADS':4}}
                if gate=='LOCK_MISSING':captured.pop('lock_observation_gate')
                if gate=='CARGO_MISSING':captured.pop('cargo_guard')
                for name,value in [('capacity-capture-terminal.json',captured),('resources.json',captured['resources']),
                    ('coverage.json',{'status':'COVERAGE_REQUIRES_REVIEW'}),('source-before.json',{}),
                    ('default-suite.json',{'query_root':str(query_root),'queries':entries})]:
                    (out/name).write_text(json.dumps(value))
                commands=[]
                def run(argv,cwd,output,*args,**kwargs):
                    commands.append(argv);self.assertNotIn('run-queries',argv)
                    value=[str(query)] if 'resolve' in argv else ({'result-sets':[{'name':'#select','rows':0}]} if 'info' in argv else {'#select':{'tuples':[]}})
                    output.write_text(json.dumps(value));return {'exit':0}
                argv=['capacity','finish','--mode','official-default-research','--codeql',str(query_root/'codeql'),
                    '--source',str(source),'--database',str(db),'--output',str(out),'--work',str(root/'work'),
                    '--sarif',str(sarif),'--deadline',str(deadline),'--analyze-outcome','success']
                for item in [patch.object(sys,'argv',argv),patch.object(m,'snapshot',return_value={}),
                    patch.object(m.runtime,'run',side_effect=run),patch.object(m,'semantic_queries',side_effect=AssertionError('custom forbidden')),
                    patch.object(m,'after_identity',return_value={'source':'UNCHANGED','official':'UNCHANGED'})]:stack.enter_context(item)
                if gate!='PASS':
                    with self.assertRaisesRegex(RuntimeError,'admission failed'):m.main()
                    self.assertEqual(commands,[])
                else:m.main()
                terminal=json.loads((out/'capacity-finish-terminal.json').read_text())
                self.assertEqual(terminal['status'],'RESEARCH_COMPLETED' if gate=='PASS' else 'RESEARCH_INCOMPLETE')
                self.assertEqual(terminal['public_semantics'],'NOT_REQUESTED')
                self.assertEqual(terminal['production_capacity'],'UNKNOWN')
                self.assertEqual(terminal['full_rust_coverage'],'HOLD')

    def test_gzip_progress_and_manifest_sections_survive_chunk_boundaries_and_reiteration(self):
        import gzip
        with tempfile.TemporaryDirectory() as temp:
            path=Path(temp)/'logs.gz'
            raw=(f'found manifests:\n  {ROOT}/Cargo.toml\nend list\n'
                 'Starting evaluation of /official/one.ql.\n'
                 'Evaluation done; writing results to /official/one.bqrs.\n').encode()
            stored=gzip.compress(raw,mtime=0);path.write_bytes(stored)
            record={'encoding':'gzip','status':'COMPLETE','raw_bytes':len(raw),
                'raw_sha256':hashlib.sha256(raw).hexdigest(),'copied_bytes':len(stored),
                'sha256':hashlib.sha256(stored).hexdigest()}
            with patch.object(m.evidence,'CHUNK_BYTES',7):
                texts=m.captured_segments(path,record,end=time.monotonic()+30)
                self.assertEqual(m.query_progress(texts)['completed_count'],1)
                coverage=m.coverage_report(ROOT,['Cargo.toml'],texts)
                self.assertEqual(coverage['manifest_enumeration'],'COMPLETE')
                self.assertEqual(coverage['missing_found_manifests'],[])

    def test_post_analysis_ignored_lock_change_rejects_even_with_clean_git_identity(self):
        from types import SimpleNamespace
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);source=root/'source';source.mkdir();out=root/'out';out.mkdir()
            db=root/'db';(db/'log').mkdir(parents=True)
            deadline=time.time()+1000;end=time.monotonic()+1000
            (out/'manifest-inventory.json').write_text('["Cargo.toml"]')
            initial=m.evidence.lock_inventory(source,['Cargo.toml'],out,end,m.SOURCE,stage='initial',fixed_deadline=deadline)
            m.observe_inputs(source,out,end,deadline,['Cargo.toml'],initial,'pre-analyze',identity={'status':'PASS'})
            (source/'Cargo.lock').write_text('new ignored bytes')
            terminal={'secondary_errors':[]}
            with patch.object(m.evidence,'capture_identity',return_value={'status':'PASS'}), \
                 patch.object(m,'resources',return_value={'CODEQL_THREADS':4,'CODEQL_RAM':14575}):
                m.capture_failure_evidence(SimpleNamespace(database=db,deadline=deadline),source,out,end,{},terminal)
            self.assertEqual(terminal['checkout_gate'],'PASS')
            self.assertEqual(terminal['lock_observation_gate'],'FAIL')
            self.assertEqual(json.loads((out/'input-observation-post-analyze.json').read_text())['changed_locks'],['Cargo.lock'])

    def test_research_allocation_is_distinct_and_bounded(self):
        self.assertEqual(m.analysis_budget('capacity',2547)['analysis_allocated_minutes'],32)
        research=m.analysis_budget('official-default-research',21400)
        self.assertEqual(research['analysis_allocated_minutes'],346)
        self.assertEqual(research['job_budget_seconds'],21600)
        self.assertEqual(research['post_analysis_reserve_seconds'],600)
        with self.assertRaises(ValueError):m.analysis_budget('unknown',21400)

    def test_research_workflow_does_not_change_default_contract(self):
        text=(P.parents[2]/'.github/workflows/codeql-rust-capacity.yml').read_text()
        self.assertIn('default: capacity',text)
        self.assertIn('official-default-research',text)
        self.assertIn("inputs.mode == 'official-default-research' && 360 || 45",text)
        self.assertEqual(text.count('--mode "$CAPACITY_MODE"'),3)

    def test_manifest_discovery_is_not_semantic_coverage(self):
        text='INFO found manifests:\n[build-stdout]   /frozen/Cargo.toml\nINFO next\n'
        value=m.coverage_report(ROOT,['Cargo.toml','nested/Cargo.toml'],[text])
        self.assertEqual(value['found_manifests'],['Cargo.toml'])
        self.assertEqual(value['missing_found_manifests'],['nested/Cargo.toml'])
        self.assertEqual(value['status'],'COVERAGE_REQUIRES_REVIEW')

    def test_abbreviated_manifest_list_cannot_prove_missing(self):
        text='found manifests:\n  /frozen/Cargo.toml\n  and 36 more\n'
        value=m.coverage_report(ROOT,['Cargo.toml','nested/Cargo.toml'],[text])
        self.assertEqual(value['found_manifests'],['Cargo.toml'])
        self.assertIsNone(value['missing_found_manifests'])
        self.assertEqual(value['manifest_enumeration'], 'UNKNOWN')

    def test_unclosed_manifest_list_cannot_prove_missing(self):
        value=m.coverage_report(ROOT,['Cargo.toml'],['found manifests:\n  /frozen/Cargo.toml'])
        self.assertIsNone(value['missing_found_manifests'])

    def test_partial_logs_keep_positive_manifest_but_no_missing_claim(self):
        text='found manifests:\n  /frozen/Cargo.toml\nINFO next\n'
        value=m.coverage_report(ROOT,['Cargo.toml','nested/Cargo.toml'],[text],logs_complete=False)
        self.assertEqual(value['found_manifests'],['Cargo.toml'])
        self.assertIsNone(value['missing_found_manifests'])

    def test_progress_does_not_join_a_cut_line_across_ranges(self):
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'log'
            head=b'Starting evaluation of x.ql.\nEvaluation done; '
            tail=b'writing results to x.bqrs.\nEvaluation done; writing results to y.bqrs.\n'
            path.write_bytes(head+tail)
            parts=m.captured_segments(path,{'source_ranges':[
                {'source_offset':0,'destination_offset':0,'length':len(head)},
                {'source_offset':500,'destination_offset':len(head),'length':len(tail)}]})
            value=m.query_progress(parts)
            self.assertEqual(value['started'],['x.ql'])
            self.assertEqual(value['completed'],['y.bqrs'])

    def test_malformed_manifest_line_cannot_stall_failure_capture(self):
        script='''import importlib.util, pathlib, sys
spec=importlib.util.spec_from_file_location('capacity',sys.argv[1])
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
text='found manifests:\\n'+'  /frozen/'*20000
value=m.coverage_report(pathlib.Path('/frozen'),['Cargo.toml'],[text])
assert value['found_manifests']==[]
'''
        subprocess.run([sys.executable,'-c',script,str(P)],cwd=P.parent,
                       check=True,timeout=2,capture_output=True)

    def test_derived_coverage_and_final_identity_respect_the_same_deadline(self):
        end=time.monotonic()-1
        with self.assertRaisesRegex(TimeoutError,'fixed evidence deadline'):
            m.coverage_report(ROOT,['Cargo.toml'],['found manifests:\n  /frozen/Cargo.toml'],end=end)
        with patch.object(m.subprocess,'check_output',side_effect=AssertionError('no new child')):
            with self.assertRaisesRegex(TimeoutError,'fixed evidence deadline'):
                m.after_identity(ROOT,Path('/official'),Path('/evidence'),end=end)

    def test_capacity_child_inherits_runner_file_limit_not_old_probe_limit(self):
        import codeql_rust_diagnostic_runtime as r
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)
            actual=r.run([sys.executable,'-c','import resource;print(resource.getrlimit(resource.RLIMIT_FSIZE)[0])'],root,root/'run.log',time.monotonic()+5,3,root,dict(os.environ),file_limit=None)
            self.assertEqual(int((root/'run.log').read_text()),resource.getrlimit(resource.RLIMIT_FSIZE)[0])
            self.assertIsNone(actual['file_size_limit_bytes'])

    def test_job_env_does_not_use_runner_context_unavailable_to_schema(self):
        workflow=(P.parents[2]/'.github/workflows/codeql-rust-capacity.yml').read_text()
        job_env=workflow.split('    env:\n',1)[1].split('    steps:',1)[0]
        self.assertNotIn('${{ runner.',job_env)
        self.assertIn("CODEQL_ACTION_TEMP={os.environ['RUNNER_TEMP']}/rust-capacity-action",workflow)

    def test_workflow_places_the_source_at_the_action_cwd_before_init(self):
        steps,by_id=workflow_steps()
        checkouts=[(i,s) for i,s in enumerate(steps) if 'uses: actions/checkout@' in s]
        self.assertEqual(len(checkouts),2)
        self.assertTrue(all('          path:' not in s for _,s in checkouts))
        self.assertIn('          ref: ${{ github.sha }}',checkouts[0][1])
        self.assertIn('          ref: '+m.SOURCE,checkouts[1][1])
        self.assertLess(checkouts[0][0],steps.index(by_id['snapshot']))
        self.assertLess(steps.index(by_id['snapshot']),checkouts[1][0])
        self.assertLess(checkouts[1][0],steps.index(by_id['layout']))
        self.assertLess(steps.index(by_id['layout']),steps.index(by_id['init']))
        self.assertIn('          source-root: ${{ github.workspace }}',by_id['init'])
        self.assertIn('          checkout_path: ${{ github.workspace }}',by_id['analyze'])
        for phase in ('prepare','capture','finish'):
            self.assertIn('"$RUNNER_TEMP/rust-capacity-helper/codeql-rust-capacity.py" '+phase,by_id[phase])
            self.assertIn('--source "$GITHUB_WORKSPACE"',by_id[phase])
            self.assertIn("--source-sha "+m.EXCLUSION_SOURCE,by_id[phase])
        condition=next(line.strip() for line in by_id['capture'].splitlines() if line.strip().startswith('if:'))
        self.assertEqual(condition,"if: always() && steps.snapshot.outcome == 'success'")
        self.assertIn("steps.capture.outcome == 'success'",by_id['finish'])

    def test_only_analysis_enables_existing_load_cargo_failure_diagnostics(self):
        steps,by_id=workflow_steps()
        analyze=by_id["analyze"]
        self.assertIn("        env:\n          RUST_LOG: single_arch_extractor=info,codeql_extractor=info,ra_ap_load_cargo=debug\n",analyze)
        self.assertEqual(sum("RUST_LOG:" in step for step in steps),1)
        job_env=(P.parents[2]/".github/workflows/codeql-rust-capacity.yml").read_text().split("    env:\n",1)[1].split("    steps:",1)[0]
        self.assertNotIn("RUST_LOG",job_env)
        self.assertNotIn("ra_ap_project_model=debug",analyze)

    def test_upload_preserves_hidden_guard_controls_in_sealed_evidence(self):
        steps,by_id=workflow_steps()
        upload=[s for s in steps if s.startswith("uses: actions/upload-artifact@")]
        self.assertEqual(len(upload),1)
        self.assertIn("          include-hidden-files: true",upload[0])
        self.assertIn("          path: ${{ runner.temp }}/rust-capacity-evidence\n",upload[0])
        script=textwrap.dedent(by_id["evidence"].split("python3 - <<'PY'\n",1)[1].split("          PY",1)[0])
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)/"rust-capacity-evidence";audit=root/"cargo-guard-calls";audit.mkdir(parents=True)
            controls={".counter":b"0000000000000001",".failed":b"1",".lock":b""}
            for name,data in controls.items():(audit/name).write_bytes(data)
            (audit/"call.json").write_text('{"status":"FAIL","exit":101}')
            with patch.dict(os.environ,{"RUNNER_TEMP":temp,"CAPACITY_MODE":"capacity"},clear=True):
                exec(compile(script,"workflow-evidence","exec"),{})
            hashes=json.loads((root/"hashes.json").read_text())
            for name,data in controls.items():
                self.assertEqual(hashes["cargo-guard-calls/"+name],hashlib.sha256(data).hexdigest())
            self.assertIn("cargo-guard-calls/call.json",hashes)

    def test_workflow_seals_partial_and_early_failure_receipts_with_hashes(self):
        _,by_id=workflow_steps();step=by_id['evidence']
        self.assertIn('        if: always()',step)
        script=textwrap.dedent(step.split("python3 - <<'PY'\n",1)[1].split('          PY',1)[0])
        for helper_present in (False,True):
            with tempfile.TemporaryDirectory() as temp:
                root=Path(temp)/'rust-capacity-evidence'
                if helper_present:
                    root.mkdir();(root/'partial.log').write_text('original partial bytes')
                env={'CAPACITY_MODE':'capacity','RUNNER_TEMP':temp,'INIT_OUTCOME':'failure','ANALYZE_OUTCOME':'skipped'}
                with patch.dict(os.environ,env,clear=True):exec(compile(script,'workflow-evidence','exec'),{})
                value=json.loads((root/'workflow-outcomes.json').read_text())
                self.assertEqual(value['outcomes']['init'],'failure')
                self.assertEqual(value['action_cleanup'],'UNKNOWN_RUNNER_OWNED')
                self.assertIsNone(json.loads((root/'job-clock.json').read_text())['fixed_job_deadline'])
                hashes=json.loads((root/'hashes.json').read_text())
                self.assertEqual(set(hashes),{'workflow-outcomes.json','job-clock.json'}|({'partial.log'} if helper_present else set()))
                for name,sha in hashes.items():self.assertEqual(m.runtime.digest(root/name),sha)
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)/'rust-capacity-evidence';root.mkdir()
            with (root/'at-cap').open('wb') as stream:stream.truncate(m.CAP)
            with patch.dict(os.environ,{'RUNNER_TEMP':temp},clear=True):
                with self.assertRaisesRegex(SystemExit,'artifact cap including receipts'):exec(compile(script,'workflow-evidence','exec'),{})
            self.assertEqual((root/'at-cap').stat().st_size,m.CAP)

    def test_manual_disabled_scope_and_production_security_boundary(self):
        workflow=P.parents[2]/'.github/workflows/codeql-rust-capacity.yml'
        text=workflow.read_text()
        self.assertIn('default: false',text);self.assertIn('inputs.execute == true',text)
        self.assertNotIn('pull_request:',text);self.assertNotIn('schedule:',text)
        self.assertIn("timeout-minutes: ${{ inputs.mode == 'official-default-research' && 360 || 45 }}",text)
        self.assertIn('source-root: ${{ github.workspace }}',text)
        self.assertIn('checkout_path: ${{ github.workspace }}',text)
        self.assertNotIn('/capacity-source',text)
        self.assertIn('upload: never',text);self.assertIn('upload-database: false',text)
        self.assertIn('trap-caching: false',text);self.assertIn('CODEQL_OVERLAY_DATABASE_MODE: none',text)
        self.assertNotIn('security-events: write',text);self.assertNotIn('upload-sarif@',text)
        self.assertIn("budget=21600 if mode=='official-default-research' else 2700",text);self.assertIn('start+budget',text);self.assertIn('oasis7-codeql-scan-slot-1',text)
        self.assertIn('codeql-rust-capacity.test.py',(P.parents[1]/'ci-tests.sh').read_text())

def load_tests(loader, tests, pattern):
    # Keep diagnostic regressions separate, using the existing selected entry.
    for name in ('codeql-rust-cargo-guard.test.py', 'codeql-rust-cargo-check.test.py'):
        spec=importlib.util.spec_from_file_location(name,P.with_name(name))
        module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
        tests.addTests(loader.loadTestsFromModule(module))
    return tests


if __name__=='__main__':unittest.main()
