#!/usr/bin/env python3
"""Fail-closed witness oracles; these do not execute or simulate CodeQL."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import Mock, patch
import types
import time
import zipfile

P=Path(__file__).with_name('codeql-rust-refactored-source.py')
spec=importlib.util.spec_from_file_location('refactored',P)
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)

class Guards(unittest.TestCase):
    def test_complete_fixed_metric_tables(self):
        for name,values in m.EXPECTED.items():
            self.assertEqual(m.check_metrics([[k,str(v)] for k,v in values.items()],name),values)
    def test_missing_positive_never_passes(self):
        rows=[[k,str(v)] for k,v in m.EXPECTED['Driver'].items() if k!='request-assert']
        with self.assertRaises(RuntimeError):m.check_metrics(rows,'Driver')
    def test_all_zero_positive_never_passes(self):
        with self.assertRaises(RuntimeError):m.check_metrics([[k,'0'] for k in m.EXPECTED['Peer']],'Peer')
    def test_wrong_branch_false_flow_rejected(self):
        values=dict(m.EXPECTED['Driver'],**{'wrong-branch':1})
        with self.assertRaises(RuntimeError):m.check_metrics([[k,str(v)] for k,v in values.items()],'Driver')
    def test_duplicate_zero_not_hidden(self):
        rows=[[k,str(v)] for k,v in m.EXPECTED['Image'].items()]
        rows.append(['wrong-digest-flow','0'])
        with self.assertRaises(RuntimeError):m.check_metrics(rows,'Image')
    def test_source_archive_selected_bytes_exact(self):
        with tempfile.TemporaryDirectory() as temporary:
            db=Path(temporary);content=b'original source'
            with zipfile.ZipFile(db/'src.zip','w') as z:z.writestr('home/source/'+m.EXPECT,content)
            expected={m.EXPECT:hashlib.sha256(content).hexdigest()}
            self.assertEqual(m.verify_archive(db,[m.EXPECT],expected),expected)
            with self.assertRaises(RuntimeError):m.verify_archive(db,[m.EXPECT],{m.EXPECT:'0'*64})
    def test_source_archive_ambiguous_same_full_suffix_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            db=Path(temporary)
            with zipfile.ZipFile(db/'src.zip','w') as z:
                z.writestr('one/'+m.EXPECT,b'source');z.writestr('two/'+m.EXPECT,b'source')
            with self.assertRaises(RuntimeError):m.verify_archive(db,[m.EXPECT],{m.EXPECT:'0'*64})
    def test_archive_report_precedes_all_selected_failures(self):
        with tempfile.TemporaryDirectory() as temporary:
            db=Path(temporary);reports=[]
            with zipfile.ZipFile(db/'src.zip','w') as z:
                z.writestr('one/duplicate.rs',b'source');z.writestr('two/duplicate.rs',b'source')
                z.writestr('one/large.rs',b'x'*(8*1024**2+1))
            with self.assertRaisesRegex(RuntimeError,'missing.rs'):
                m.verify_archive(db,['missing.rs','duplicate.rs','large.rs'],{},reports.append)
            self.assertEqual(len(reports),1)
            self.assertEqual(reports[0]['selected']['missing.rs']['reasons'],['zero matches'])
            self.assertEqual(reports[0]['selected']['duplicate.rs']['reasons'],['multiple matches'])
            self.assertEqual(reports[0]['selected']['large.rs']['reasons'],['selected member oversize'])
            self.assertEqual(len(reports[0]['entries']),3)

    def test_collection_only_current_adapter_and_common(self):
        for context in ('candidate','legacy'):
            files=[m.BASE+'/'+context+'/src/'+name for name in ('main.rs','adapter.rs')]
            files += [m.BASE+'/common/src/'+name for name in ('lib.rs','driver.rs','protocol.rs','driver/tests.rs')]
            with patch.object(m.subprocess,'check_output',return_value=('\0'.join(files)+'\0').encode()):
                filters,actual=m.collection(Path('/source'),context)
            self.assertEqual(set(actual),set(files))
            self.assertEqual(filters.splitlines(),['include:'+m.BASE+'/'+context+'/**','include:'+m.BASE+'/common/**'])
            self.assertNotIn('legacy' if context=='candidate' else 'candidate',filters)

    def test_image_collection_uses_real_independent_inventory(self):
        files=[m.IMAGE+'/build.rs',m.EXPECT]
        with patch.object(m.subprocess,'check_output',return_value=('\0'.join(files)+'\0').encode()):
            filters,actual=m.collection(Path('/source'),'image')
        self.assertEqual(filters,'include:'+m.IMAGE+'/**')
        self.assertEqual(set(actual),set(files))

    def test_full_metadata_missing_resolve_rejected(self):
        with self.assertRaises(RuntimeError):m.verify_metadata({'resolve':None},Path('/source'),'candidate')
    def test_query_target_identity_and_generated_nodes_preserved(self):
        queries=m.queries(Path('/owned/source'))
        self.assertTrue(all('/owned/source' in text for text in queries.values()))
        self.assertIn('n.asExpr().getParentNode*()',queries['PeerCandidate'])
        self.assertNotIn('fileAt(n',queries['PeerCandidate'].split('predicate witness')[1])
        self.assertIn('inExpansion(b.asExpr()',queries['Driver'])
        self.assertNotIn('\"legacy\"',queries['PeerCandidate'])
        self.assertNotIn('\"candidate\"',queries['PeerLegacy'])
    def test_failure_finally_never_spawns_more_processes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);source=root/'source';source.mkdir()
            (source/'witness.rs').write_text('original')
            output=root/'evidence';output.mkdir();work=root/'work';work.mkdir()
            cli=root/'tools/codeql';official=cli.parent/'qlpacks/codeql/rust-all/0.2.23'
            official.mkdir(parents=True);(official/'f.qll').write_text('official')
            args=types.SimpleNamespace(business_source=source,output=output,work=work,
                codeql=cli,deadline=time.monotonic()+1080)
            driver=types.SimpleNamespace(LIBRARY_VERSIONS={'rust-all':'0.2.23'},
                run=Mock(side_effect=RuntimeError('cleanup uncertain')))
            def write(path,data,artifact_root):path.write_bytes(data)
            production=types.SimpleNamespace(evidence_write=write)
            with patch.object(m,'protected',return_value={'witness.rs':m.digest(source/'witness.rs')}), \
                 patch.object(m.subprocess,'check_output',side_effect=[(m.SOURCE+'\n').encode(),b'']) as command:
                # The first HEAD call requests text=True, so return the corresponding type.
                command.side_effect=[m.SOURCE+'\n',b'']
                with self.assertRaisesRegex(RuntimeError,'cleanup uncertain'):
                    m.execute(driver,production,args,{})
                self.assertEqual(command.call_count,2)
                self.assertEqual(driver.run.call_count,1)
                self.assertTrue((output/'refactored-protected-after.json').is_file())
                self.assertTrue((output/'refactored-official-after.json').is_file())

    def test_partition_formula_union_and_zero_rows_exact(self):
        original=m.queries(Path('/owned/source'));parts=m.partition_queries(Path('/owned/source'))
        def clauses(text):
            body=text.split('predicate witness(string label) {\n',1)[1].split('\n}\n',1)[0]
            return {m.re.match(r'\s*label = "([^"]+)"',x).group(1):x.strip()
                    for x in m.re.split(r'\n  or (?=label = ")',body)}
        for name,children in [('PeerCandidate',['PeerAst','PeerDf']),('Driver',['DriverNonTarget','DriverTarget'])]:
            merged={}
            for child in children:
                for key,value in clauses(parts[child]).items():
                    self.assertNotIn(key,merged);merged[key]=value
                self.assertIn('present = 0 and not witness(label)',parts[child])
            self.assertEqual(merged,clauses(original[name]))
        self.assertEqual(parts['CallsCandidate'],original['CallsCandidate'])
        self.assertEqual(parts['Files'],original['Files'])
        self.assertEqual(len(clauses(parts['DriverNonTarget'])),13)
        self.assertEqual(len(clauses(parts['DriverTarget'])),3)
        self.assertIn('wrong-branch',clauses(parts['DriverNonTarget']))
        self.assertIn('wrong-state',clauses(parts['DriverNonTarget']))

    def test_invalid_partition_rejected(self):
        text=m.queries(Path('/owned/source'))['PeerCandidate']
        for labels in [[],['missing'],['derive-ast','derive-ast']]:
            with self.assertRaises(RuntimeError):m.partition_query(text,labels)

    def test_partition_compile_early_return_never_materializes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);source=root/'source';source.mkdir()
            output=root/'evidence';output.mkdir();work=root/'work';work.mkdir()
            cli=root/'tools/codeql';official=cli.parent/'qlpacks/codeql/rust-all/0.2.23'
            official.mkdir(parents=True);(official/'f.qll').write_text('official')
            args=types.SimpleNamespace(business_source=source,output=output,work=work,
                codeql=cli,deadline=time.monotonic()+1080,refactored_partition_plans=True)
            calls=[]
            def run(argv,cwd,log,*rest):
                calls.append(argv)
                if '--help' in argv:log.write_text('--[no-]dump-ra')
                elif argv[1:3]==['resolve','library-path']:log.write_text('{}')
                else:log.write_text('mock compiler stdout; not actual QL evidence')
            driver=types.SimpleNamespace(LIBRARY_VERSIONS={'rust-all':'0.2.23'},run=run,verify_library_path=Mock())
            production=types.SimpleNamespace(evidence_write=lambda path,data,artifact_root:path.write_bytes(data))
            with patch.object(m,'protected',return_value={}),patch.object(m.subprocess,'check_output',side_effect=[m.SOURCE+'\n',b'',b'']):
                m.execute(driver,production,args,{})
            self.assertEqual(len(calls),8)
            self.assertTrue(all(argv[1] in ('resolve','query') for argv in calls))
            self.assertEqual(calls[-1][1:3],['query','compile'])
            self.assertIn('--dump-ra',calls[-1])
            self.assertEqual(len([x for x in calls[-1] if x.endswith('.ql')]),6)
            self.assertTrue((output/'partition-compile-only.json').is_file())

    def test_exact_new_source_anchors(self):
        root=P.resolve().parents[2]
        anchors={m.DRIVER:{23:'async fn run',40:'Event::Request',44:'Event::Response',52:'Event::Gossip',64:'if !server'},
                 m.EXPECT:{12:'fn assert_platform_report',13:'format!',14:'assert!',23:'format!',24:'assert!'},
                 m.BASE+'/candidate/src/adapter.rs':{10:'struct Behaviour',42:'with_existing_identity',51:'.with_behaviour',60:'Signed(key.clone())'},
                 m.BASE+'/legacy/src/adapter.rs':{10:'struct Behaviour',42:'with_existing_identity'}}
        for path,lines in anchors.items():
            source=subprocess.check_output(['git','show',m.SOURCE+':'+path],cwd=root,text=True).splitlines()
            for line,expected in lines.items():self.assertIn(expected,source[line-1])

if __name__=='__main__':unittest.main()
