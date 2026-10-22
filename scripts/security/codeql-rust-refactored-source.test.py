#!/usr/bin/env python3
"""Projection, source identity, and partial-result contracts; no CodeQL process."""
import importlib.util
import json
from pathlib import Path
import tempfile
import time
import types
import unittest
from unittest.mock import patch, Mock
import zipfile

P=Path(__file__).with_name('codeql-rust-refactored-source.py')
spec=importlib.util.spec_from_file_location('model',P)
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
ROOT=Path('/home/runner/work/oasis7/oasis7/model-source')


class Projection(unittest.TestCase):
    def setUp(self):self.texts=m.diagnostic_queries(ROOT)

    def rows(self,name):
        return [[label,str(m.EXPECTED[m.QUERY_CATEGORY[name]][label])] for label in m.query_labels(self.texts[name])]

    def test_peer_df_and_driver_parts_project_full_expected_once(self):
        values={}
        for name in ('PeerAst','PeerDf','DriverNonTarget','DriverTarget'):
            values[name]=m.validate_partition(name,self.texts[name],self.rows(name))
        self.assertEqual(m.complete_union(values),{key:m.EXPECTED[key] for key in ('Peer','Driver')})
        self.assertNotIn('derive-ast',values['PeerDf'])
        self.assertEqual(len(values['DriverNonTarget']),13)
        self.assertEqual(len(values['DriverTarget']),3)

    def test_missing_extra_duplicate_wrong_negative_not_skipped(self):
        name='DriverNonTarget';rows=self.rows(name)
        wrong=[[key,'1' if key=='wrong-branch' else value] for key,value in rows]
        for bad in (rows[:-1],rows+[['foreign','1']],rows+rows[:1],wrong):
            with self.assertRaises(RuntimeError):m.validate_partition(name,self.texts[name],bad)
        for name in ('PeerDf','DriverTarget','NodeOnly'):
            with self.assertRaises(RuntimeError):m.validate_partition(name,self.texts[name],[])

    def test_endpoints_cannot_waive_negative_or_complete_union(self):
        labels=m.query_labels(self.texts['Endpoints'])
        values=m.validate_partition('Endpoints',self.texts['Endpoints'],[[x,'0'] for x in labels])
        self.assertEqual(set(values.values()),{0})
        with self.assertRaises(RuntimeError):m.complete_union({'Endpoints':values})
        self.assertIn('line = 45',self.texts['Endpoints'])
        self.assertIn('inExpansion(n.asExpr()',self.texts['Endpoints'])

    def test_unknown_partition_or_nonboolean_value_rejected(self):
        with self.assertRaises(RuntimeError):m.validate_partition('Unknown',self.texts['PeerDf'],self.rows('PeerDf'))
        with self.assertRaises(RuntimeError):m.validate_partition('PeerDf',self.texts['PeerDf'],[['derive-df','01'],['closure-flow','1']])

    def test_formula_partitions_preserve_disjoint_union_and_zero_rows(self):
        def clauses(text):
            body=text.split('predicate witness(string label) {\n',1)[1].split('\n}\n',1)[0]
            return {m.re.match(r'\s*label = "([^"]+)"',x).group(1):x.strip()
                    for x in m.re.split(r'\n  or (?=label = ")',body)}
        original=m.queries(ROOT)
        for name,children in [('PeerCandidate',('PeerAst','PeerDf')),('Driver',('DriverNonTarget','DriverTarget'))]:
            union={}
            for child in children:
                values=clauses(self.texts[child]);self.assertFalse(set(union)&set(values));union.update(values)
                self.assertIn('present = 0 and not witness(label)',self.texts[child])
            self.assertEqual(union,clauses(original[name]))
        self.assertEqual(self.texts['Files'],original['Files'])
        self.assertEqual(self.texts['CallsCandidate'],original['CallsCandidate'])

    def test_reviewed_original_query_bytes_unchanged(self):
        import hashlib
        expected={'PeerCandidate':'33c0fa3e623b91d306f14d3919ca581f4270f5062afad18a60dc000dd2da23a2',
                  'Driver':'fcdea4ae7b6a095fae784aad582d2c2faae6163a94efd9a5b02254569cc66121',
                  'CallsCandidate':'d82b9d54032a771ab056dfe9288c16596f6d932cffbabb8d562ebc894c1c01f7'}
        for name,value in expected.items():self.assertEqual(hashlib.sha256(m.queries(ROOT)[name].encode()).hexdigest(),value)


class Identity(unittest.TestCase):
    def test_file_exact_context_duplicate_and_missing_rejected(self):
        expected={'/source/candidate/adapter.rs','/source/common/driver.rs','/source/common/protocol.rs'}
        rows=[[x,x] for x in expected];m.check_files(rows,expected)
        for bad in (rows[:-1],rows+rows[:1],rows+[['/source/legacy/adapter.rs','foreign']],[["short"]]):
            with self.assertRaises(RuntimeError):m.check_files(bad,expected)

    def test_archive_records_all_failures_before_reject(self):
        with tempfile.TemporaryDirectory() as directory:
            db=Path(directory);(db/'src.zip').touch()
            with zipfile.ZipFile(db/'src.zip','w') as archive:
                archive.writestr('root/a/file.rs','one');archive.writestr('b/a/file.rs','two')
            reports=[]
            with self.assertRaises(RuntimeError):m.verify_archive(db,['a/file.rs','missing.rs'],{},reports.append)
            self.assertEqual(len(reports),1)
            observations=reports[0]['selected']
            self.assertEqual(observations['a/file.rs']['match_count'],2)
            self.assertEqual(observations['missing.rs']['match_count'],0)

    def test_complete_metadata_requires_resolve_graph(self):
        with self.assertRaises(RuntimeError):m.verify_metadata({'resolve':None},Path('/source'),'candidate')

    def test_full_source_collection_is_current_context_only(self):
        text=P.read_text()
        self.assertIn("filters, initial = collection(source, 'candidate')",text)
        self.assertIn('model_end = min(args.deadline-90, time.monotonic() + 180)',text)
        self.assertNotIn('time.monotonic() + 180',text.split('for index, name in enumerate(names):')[1])

    def test_success_decode_terminal_survives_next_partition_failure(self):
        self.partial_failure_case('child_exit',False)

    def test_summary_failure_does_not_mask_original_clean_timeout(self):
        self.partial_failure_case('timeout',True)

    def partial_failure_case(self,kind,summary_failure):
        with tempfile.TemporaryDirectory() as directory:
            base=Path(directory).resolve();source=base/'source';source.mkdir();out=base/'out';out.mkdir();work=base/'work';work.mkdir()
            selected=[m.DRIVER,m.BASE+'/common/src/protocol.rs',m.BASE+'/candidate/src/adapter.rs']
            for name in selected:
                path=source/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_text(name)
            before={name:m.digest(source/name) for name in selected}
            cli=base/'codeql/codeql';pack=cli.parent/'qlpacks/codeql/rust-all/0.2.23';pack.mkdir(parents=True);(pack/'qlpack.yml').write_text('official')
            closure={'codeql/rust-all':{'root':str(pack),'version':'0.2.23'}}
            args=types.SimpleNamespace(business_source=source,output=out,work=work,codeql=cli,
                deadline=time.monotonic()+1080,probe='node-only',bundle_version='2.27.2')
            calls=[]
            failure=m.runtime.PhaseError({'failure_kind':kind,'cleanup_errors':[], 'argv':['cli','database','run-queries']})
            def run(argv,cwd,output,end,timeout,artifact,env,file_limit=m.runtime.CAP,evaluator=None):
                calls.append((argv,end))
                if '--help' in argv:output.write_text('--evaluator-log --[no-]tuple-counting predicates overall --minify-output')
                elif argv[1:3]==['resolve','library-path']:output.write_text('{}')
                elif argv[0]=='cargo':output.write_text('{}')
                elif argv[1:3]==['database','create']:
                    database=Path(argv[3]);results=database/'results';results.mkdir(parents=True);(results/'Files.bqrs').write_text('file results')
                    output.write_text('number of inputs: 3,\ncargo_all_targets: true\ncargo_features: []\n'+str(source/m.BASE/'candidate/Cargo.toml')+'\n'+str(source/m.BASE/'common/Cargo.toml'))
                    output.with_name(output.name+'.stderr').write_text('')
                elif argv[1:3]==['database','run-queries']:
                    evaluator.write_text('{"type":"LOG_HEADER"}\n{"type":"LOG_FOOTER"}\n')
                    if 'NodeOnly.ql' in ' '.join(argv):raise failure
                elif argv[1:3]==['bqrs','decode']:
                    import csv
                    with output.open('w') as stream:
                        writer=csv.writer(stream);writer.writerow(['col0','col1']);writer.writerows([[str(source/name),name] for name in selected])
                else:output.write_text('mock successful compile')
            with patch.object(m,'protected',return_value=before),patch.object(m,'verify_metadata',return_value=source/'libp2p'), \
                 patch.object(m,'collection',return_value=('current context',set(selected))),patch.object(m,'verify_archive',return_value=before), \
                 patch.object(m.shutil,'disk_usage',return_value=types.SimpleNamespace(free=8*1024**3)), \
                 patch.object(m.subprocess,'check_output',side_effect=[m.SOURCE+'\n',b'']), \
                 patch.object(m.runtime,'run',side_effect=run),patch.object(m.runtime,'official_summaries',side_effect=
                    [{'overall':'GENERATED','predicates':'GENERATED'},RuntimeError('summary cap failed')] if summary_failure else
                    lambda *args,**kwargs:{'overall':'GENERATED','predicates':'GENERATED'}) as summary:
                with self.assertRaises(m.runtime.PhaseError) as caught:m.execute(args,{},closure,Mock())
            self.assertIs(caught.exception,failure)
            self.assertEqual(json.loads((out/'Files-terminal.json').read_text())['status'],'DECODED_VALIDATED')
            self.assertEqual(json.loads((out/'NodeOnly-terminal.json').read_text())['status'],'FAILED_OR_NOT_MEASURED')
            self.assertEqual(summary.call_count,2 if summary_failure else 1)
            if summary_failure:self.assertIn('postprocessing_error',json.loads((out/'NodeOnly-terminal.json').read_text()))
            ends=[end for argv,end in calls if '--help' not in argv and argv[1:3] in (['database','run-queries'],['bqrs','decode'])]
            self.assertEqual(len(set(ends)),1)


if __name__=='__main__':unittest.main()
