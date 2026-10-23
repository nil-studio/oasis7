#!/usr/bin/env python3
"""Capacity assessment source, completeness and resource-boundary regressions."""
import importlib.util
import json
import os
from pathlib import Path
import resource
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

P=Path(__file__).with_name('codeql-rust-capacity.py')
spec=importlib.util.spec_from_file_location('capacity',P);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
ROOT=Path('/frozen')

class Capacity(unittest.TestCase):
    def test_action_resources_must_be_real_exports(self):
        with self.assertRaises(RuntimeError):m.resources({})
        with self.assertRaises(RuntimeError):m.resources({'CODEQL_RAM':'3072','CODEQL_THREADS':'auto'})
        selected=m.resources({'CODEQL_RAM':'14848','CODEQL_THREADS':'4'})
        self.assertEqual((selected['CODEQL_RAM'],selected['CODEQL_THREADS']),(14848,4))

    def test_action_source_and_cache_must_bind_frozen_context(self):
        config={'dbLocation':'/data','repositoryRoot':'/frozen','languages':['rust'],'overlayDatabaseMode':'none','trapCaches':{},'useOverlayDatabaseCaching':False}
        m.verify_action_config(config,ROOT,Path('/data/rust'))
        for field,value in [('repositoryRoot','/tools'),('overlayDatabaseMode','overlay'),('trapCaches',{'rust':'/cache'}),('useOverlayDatabaseCaching',True)]:
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

    def test_manifest_discovery_is_not_semantic_coverage(self):
        text='INFO found manifests:\n[build-stdout]   /frozen/Cargo.toml\nINFO next\n'
        value=m.coverage_report(ROOT,['Cargo.toml','nested/Cargo.toml'],[text])
        self.assertEqual(value['found_manifests'],['Cargo.toml'])
        self.assertEqual(value['missing_found_manifests'],['nested/Cargo.toml'])
        self.assertEqual(value['status'],'COVERAGE_REQUIRES_REVIEW')

    def test_capacity_child_inherits_runner_file_limit_not_old_probe_limit(self):
        import codeql_rust_diagnostic_runtime as r
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)
            actual=r.run([sys.executable,'-c','import resource;print(resource.getrlimit(resource.RLIMIT_FSIZE)[0])'],root,root/'run.log',time.monotonic()+5,3,root,dict(os.environ),file_limit=None)
            self.assertEqual(int((root/'run.log').read_text()),resource.getrlimit(resource.RLIMIT_FSIZE)[0])
            self.assertIsNone(actual['file_size_limit_bytes'])

    def test_manual_disabled_scope_and_production_security_boundary(self):
        workflow=P.parents[2]/'.github/workflows/codeql-rust-capacity.yml'
        text=workflow.read_text()
        self.assertIn('default: false',text);self.assertIn('inputs.execute == true',text)
        self.assertNotIn('pull_request:',text);self.assertNotIn('schedule:',text)
        self.assertIn('timeout-minutes: 45',text)
        self.assertIn('source-root: capacity-source',text)
        self.assertIn('checkout_path: ${{ github.workspace }}/capacity-source',text)
        self.assertIn('upload: never',text);self.assertIn('upload-database: false',text)
        self.assertIn('trap-caching: false',text);self.assertIn('CODEQL_OVERLAY_DATABASE_MODE: none',text)
        self.assertNotIn('security-events: write',text);self.assertNotIn('upload-sarif@',text)
        self.assertIn('start+2700',text);self.assertIn('oasis7-codeql-scan-slot-1',text)
        self.assertIn('codeql-rust-capacity.test.py',(P.parents[1]/'ci-tests.sh').read_text())

if __name__=='__main__':unittest.main()
