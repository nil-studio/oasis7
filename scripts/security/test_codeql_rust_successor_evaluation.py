import tempfile
from pathlib import Path
import unittest
from types import SimpleNamespace
from unittest.mock import patch
import time
import json
import shutil

import codeql_rust_successor_evaluation as m


class ComparisonTests(unittest.TestCase):
    def database(self, root):
        (root/'db-rust/default').mkdir(parents=True)
        (root/'db-rust/default/facts').write_bytes(b'facts')
        (root/'codeql-database.yml').write_text('primaryLanguage: rust\n')

    def test_query_cache_does_not_change_fact_identity_but_fact_mutation_does(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d); self.database(p)
            original=m.facts(p)[0]
            (p/'db-rust/default/cache').mkdir()
            (p/'db-rust/default/cache/derived').write_bytes(b'evaluated')
            self.assertEqual(m.facts(p)[0],original)
            (p/'db-rust/default/facts').write_bytes(b'changed')
            self.assertNotEqual(m.facts(p)[0],original)

    def test_symlinks_cannot_escape_database(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d); self.database(p)
            (p/'escape').symlink_to('/etc/passwd')
            with self.assertRaisesRegex(RuntimeError,'symlink'): m.facts(p)

    def test_equal_results_are_order_independent_but_preserve_duplicates(self):
        a={'Flow':[['label','present'],['positive','1'],['negative','0']]}
        b={'Flow':list(reversed(a['Flow']))}
        self.assertEqual(m.same_results(a,b)['Flow'],{'original_only':0,'candidate_only':0})
        b['Flow'].append(['positive','1'])
        with self.assertRaisesRegex(RuntimeError,'differ'):m.same_results(a,b)

    def test_incomplete_domains_cannot_pass_difference(self):
        with self.assertRaisesRegex(RuntimeError,'incomplete'):m.same_results({'Flow':[]},{})

    def test_source_inventory_failure_does_not_skip_arm_difference(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d);source=p/'source';source.mkdir();self.database(source)
            baseline=m.facts(source)[0];arm=p/'arm';shutil.copytree(source,arm)
            (source/'bad-link').symlink_to('/etc/passwd')
            (arm/'db-rust/default/facts').write_text('changed')
            intact,details=m.after_facts(source,arm,baseline)
            self.assertFalse(intact)
            self.assertEqual(details['source']['status'],'UNKNOWN')
            self.assertEqual(details['arm']['changed'][0]['path'],'db-rust/default/facts')

    def test_difference_details_are_bounded_without_losing_total_count(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d);source=p/'source';source.mkdir();self.database(source)
            baseline=m.facts(source)[0];arm=p/'arm';shutil.copytree(source,arm)
            for i in range(20):(arm/('new-'+str(i))).write_text('derived')
            intact,details=m.after_facts(source,arm,baseline)
            self.assertFalse(intact)
            self.assertEqual(details['arm']['added_count'],20)
            self.assertEqual(len(details['arm']['added']),4)
            self.assertFalse(details['arm']['details_complete'])

    def experiment(self, clean, broken_summary=False, spawned=True, mutate=False):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d);work=p/'work';work.mkdir()
            db=work/'candidate-db';db.mkdir();self.database(db)
            out=p/'out';out.mkdir()
            args=SimpleNamespace(output=out,work=work,codeql=Path('/fixed/codeql'),
                business_source=p,deadline=time.monotonic()+1000)
            calls=[]
            def run(argv,cwd,output,*rest):
                if argv[1:3]==['resolve','library-path']:
                    output.write_text('{}');return
                self.assertEqual(argv[1:3],['database','run-queries'])
                calls.append(argv[3])
                if mutate:
                    (Path(argv[3])/"new-derived-marker").write_text("new")
                if broken_summary:
                    (out/('original-Files-evaluator.jsonl')).write_text('{}\n')
                raise m.runtime.PhaseError({'argv':argv,'failure_kind':'timeout',
                    'cleanup_errors':[] if clean else ['child still alive'],'evidence_errors':[],
                    'pid':123 if spawned else None,'pgid':123 if spawned else None,
                    'exit':-15 if spawned else None,'started_wall':time.time(),'elapsed':180.0})
            with patch.object(m.plan,'inventory',return_value={}),patch.object(m.plan,'prepare_copy',return_value=p/'private'),patch.object(m.runtime,'run',side_effect=run),patch.object(m.runtime,'official_summaries',return_value={'overall':{'status':'UNKNOWN'}}):
                if mutate:
                    with self.assertRaisesRegex(RuntimeError,'facts changed'):
                        m.execute(args,{}, {'codeql/rust-all':{'root':'official'}},lambda *a:None,
                            db,{'Files':'query'},p,set(),SimpleNamespace(SOURCE='fixed'))
                    terminal=json.loads((out/'comparison-terminal.json').read_text())
                    self.assertEqual(terminal['original']['status'],'FAILED_FACTS_AFTER')
                    self.assertEqual(terminal['original']['pre_fact_status'],'CLEAN_QUERY_TIMEOUT')
                    details=terminal['original']['facts_after']
                    self.assertEqual(details['source']['status'],'UNCHANGED')
                    self.assertEqual(details['arm']['status'],'CHANGED')
                    self.assertEqual(details['arm']['added'][0]['path'],'new-derived-marker')
                    self.assertEqual(terminal['candidate']['status'],'NOT_REACHED')
                    self.assertEqual(len(calls),1)
                elif broken_summary:
                    with self.assertRaisesRegex(RuntimeError,'summary incomplete'):
                        m.execute(args,{}, {'codeql/rust-all':{'root':'official'}},lambda *a:None,
                            db,{'Files':'query'},p,set(),SimpleNamespace(SOURCE='fixed'))
                    self.assertEqual(len(calls),1)
                elif clean and spawned:
                    m.execute(args,{}, {'codeql/rust-all':{'root':'official'}},lambda *a:None,
                        db,{'Files':'query'},p,set(),SimpleNamespace(SOURCE='fixed'))
                    self.assertEqual(len(calls),2)
                    result=json.loads((out/'comparison-result.json').read_text())
                    self.assertEqual(result['status'],'INCOMPLETE_EQUALITY_UNKNOWN')
                else:
                    with self.assertRaises(m.runtime.PhaseError):
                        m.execute(args,{}, {'codeql/rust-all':{'root':'official'}},lambda *a:None,
                            db,{'Files':'query'},p,set(),SimpleNamespace(SOURCE='fixed'))
                    self.assertEqual(len(calls),1)
                    terminal=json.loads((out/'comparison-terminal.json').read_text())
                    self.assertEqual(terminal['candidate']['status'],'NOT_REACHED')

    def test_clean_query_timeout_still_measures_candidate_without_claiming_equality(self):
        self.experiment(True)

    def test_changed_arm_records_exact_difference_and_checks_source_too(self):
        self.experiment(True, mutate=True)

    def test_failed_summary_prevents_candidate_even_after_clean_query_timeout(self):
        self.experiment(True, broken_summary=True)

    def test_pre_spawn_deadline_is_not_an_actual_query_timeout(self):
        self.experiment(True, spawned=False)

    def test_unclean_timeout_prevents_candidate(self):
        self.experiment(False)

    def resolution_experiment(self, copy_delay=0):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d);work=p/'work';work.mkdir()
            db=work/'candidate-db';db.mkdir();self.database(db)
            out=p/'out';out.mkdir()
            clock=[100.0]; windows=[]
            args=SimpleNamespace(output=out,work=work,codeql=Path('/fixed/codeql'),
                business_source=p,deadline=1100.0)
            def run(argv,cwd,output,end,*rest):
                if argv[1:3]==['resolve','library-path']:
                    clock[0]+=10;output.write_text('{}');return
                windows.append(end-clock[0]);clock[0]=end
                raise m.runtime.PhaseError({'argv':argv,'failure_kind':'timeout',
                    'cleanup_errors':[],'evidence_errors':[],'pid':123,'pgid':123,
                    'exit':-15,'started_wall':time.time(),'elapsed':180.0})
            def prepare(*a):
                clock[0]+=copy_delay;return p/'private'
            with patch.object(m.plan,'inventory',return_value={}),patch.object(m.plan,'prepare_copy',side_effect=prepare),patch.object(m.runtime,'run',side_effect=run),patch.object(m.time,'monotonic',side_effect=lambda:clock[0]):
                if copy_delay:
                    with self.assertRaisesRegex(RuntimeError,'insufficient full arm'):
                        m.execute(args,{}, {'codeql/rust-all':{'root':'official'}},lambda *a:None,
                            db,{str(i):'query' for i in range(6)},p,set(),SimpleNamespace(SOURCE='fixed'))
                    self.assertEqual(windows,[])
                else:
                    m.execute(args,{}, {'codeql/rust-all':{'root':'official'}},lambda *a:None,
                        db,{str(i):'query' for i in range(6)},p,set(),SimpleNamespace(SOURCE='fixed'))
                    self.assertEqual(windows,[180.0,180.0])

    def test_resolution_does_not_consume_either_model_clock(self):
        self.resolution_experiment()

    def test_copy_cost_cannot_consume_candidate_reserve_before_original_starts(self):
        self.resolution_experiment(copy_delay=300)

    def files_inventory(self, mutate):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d);work=p/'work';work.mkdir();db=work/'candidate-db';db.mkdir();self.database(db)
            out=p/'out';out.mkdir();clock=time.monotonic()
            args=SimpleNamespace(output=out,work=work,codeql=Path('/fixed/codeql'),business_source=p,deadline=clock+800)
            calls=[];seen_rows=[];initial_write_cap=m.runtime.WRITE_CAP
            def check(rows,wanted):
                self.assertEqual(rows,[[str(p/'source.rs'),'source.rs']])
                self.assertEqual(wanted,{str(p/'source.rs')});seen_rows.extend(rows)
            model=SimpleNamespace(SOURCE='fixed',check_files=check,complete_union=lambda *a:self.fail('Files is not full semantic coverage'))
            def run(argv,cwd,output,*rest):
                if argv[1:3]==['resolve','library-path']:
                    output.write_text('{}');return
                if argv[1:3]==['database','run-queries']:
                    self.assertEqual(m.runtime.WRITE_CAP,initial_write_cap-m.AFTER_MAP_RESERVE)
                    calls.append(argv)
                    self.assertTrue(argv[4].endswith('/Files.ql'))
                    queried=Path(argv[3]);results=queried/'results';results.mkdir()
                    (results/'Files.bqrs').write_bytes(b'complete bounded fixture')
                    (out/'original-Files-evaluator.jsonl').write_text('{}\n')
                    if mutate:(queried/'derived-index').write_text('derived')
                    return
                self.assertEqual(argv[1:3],['bqrs','decode'])
                output.write_text('"col0","col1"\n"'+str(p/'source.rs')+'","source.rs"\n')
            summary={k:'GENERATED_NOT_EVALUATION_COMPLETION' for k in ('overall','predicates')}
            with patch.object(m.plan,'prepare_copy',side_effect=AssertionError('no candidate library')),patch.object(m.runtime,'run',side_effect=run),patch.object(m.runtime,'official_summaries',return_value=summary):
                if mutate:
                    with self.assertRaisesRegex(RuntimeError,'facts changed after arm'):
                        m.execute(args,{}, {'codeql/rust-all':{'root':'official'}},lambda *a:None,db,{'Files':'fixed query'},p,{str(p/'source.rs')},model,files_only=True)
                else:
                    m.execute(args,{}, {'codeql/rust-all':{'root':'official'}},lambda *a:None,db,{'Files':'fixed query'},p,{str(p/'source.rs')},model,files_only=True)
                    result=json.loads((out/'files-inventory-result.json').read_text())
                    self.assertEqual(result['status'],'FILES_VALIDATED')
                    self.assertEqual(result['candidate_queries'],0)
            self.assertEqual(len(calls),1)
            self.assertEqual(m.runtime.WRITE_CAP,initial_write_cap)
            self.assertEqual(len(seen_rows),1)
            before=json.loads((out/'before-source-inventory.json').read_text())
            self.assertEqual(before,json.loads((out/'before-arm-inventory.json').read_text()))
            self.assertEqual(before,json.loads((out/'after-source-inventory.json').read_text()))
            after=json.loads((out/'after-arm-inventory.json').read_text())
            self.assertEqual('derived-index' in after,mutate)
            self.assertFalse((work/'comparison-databases/candidate').exists())

    def test_files_mode_uses_one_official_query_and_complete_dual_snapshots(self):
        self.files_inventory(False)

    def test_files_mode_records_real_change_and_stops_without_candidate(self):
        self.files_inventory(True)

    def test_files_mode_rejects_extra_model_query_before_any_cli(self):
        args=SimpleNamespace(output=Path('/unused'),work=Path('/unused'),codeql=Path('/unused'))
        with self.assertRaisesRegex(RuntimeError,'exactly one Files'):
            m.execute(args,{}, {},None,None,{'Files':'query','PeerDf':'query'},None,None,None,files_only=True)

    def test_insufficient_snapshot_reserve_rejects_query_without_changing_cap(self):
        with tempfile.TemporaryDirectory() as d:
            old=m.runtime.WRITE_CAP
            with patch.object(m.runtime,'output_bytes',return_value=old-m.AFTER_MAP_RESERVE-100):
                with self.assertRaisesRegex(RuntimeError,'NOT_REACHED'):
                    m.reserve_after_maps(Path(d),{})
            self.assertEqual(m.runtime.WRITE_CAP,old)

    def test_full_query_allowance_and_terminal_receipts_leave_both_after_maps_writable(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);old=m.runtime.WRITE_CAP
            try:
                m.reserve_after_maps(root,{})
                remaining=m.runtime.WRITE_CAP-m.runtime.output_bytes(root)
                with (root/'query.log').open('wb') as stream:stream.truncate(remaining)
                for i in range(4):
                    m.runtime.evidence_write(root/('receipt-'+str(i)),b'x'*m.runtime.MAX_PROCESS_RECEIPT,root,terminal=True)
                m.runtime.WRITE_CAP=old
                for i in range(2):
                    m.runtime.evidence_write(root/('after-map-'+str(i)),b'x'*m.MAX_INVENTORY_MAP_BYTES,root)
                self.assertEqual(m.runtime.output_bytes(root),old)
                m.runtime.save(root/"terminal-result.json",{"status":"captured"},root,terminal=True)
                self.assertLess(m.runtime.output_bytes(root),m.runtime.CAP)
            finally:m.runtime.WRITE_CAP=old

    def test_oversized_full_inventory_is_explicitly_rejected(self):
        with patch.object(m,'MAX_INVENTORY_MAP_BYTES',8):
            with self.assertRaisesRegex(RuntimeError,'inventory exceeds'):
                m.inventory_bytes({'file':'a'*64})

    def test_prequery_failure_remains_not_reached_and_does_not_write_after_maps(self):
        for failure_phase in ('resolve','reserve'):
            with self.subTest(phase=failure_phase),tempfile.TemporaryDirectory() as d:
                p=Path(d);work=p/'work';work.mkdir();db=work/'candidate-db';db.mkdir();self.database(db)
                out=p/'out';out.mkdir()
                args=SimpleNamespace(output=out,work=work,codeql=Path('/fixed/codeql'),business_source=p,deadline=time.monotonic()+800)
                def run(argv,cwd,output,*a):
                    self.assertEqual(argv[1:3],['resolve','library-path'])
                    if failure_phase=='resolve':raise RuntimeError('prequery resolve failure')
                    output.write_text('{}')
                with patch.object(m.runtime,'run',side_effect=run),patch.object(m,'reserve_after_maps',side_effect=RuntimeError('prequery reserve failure')):
                    with self.assertRaisesRegex(RuntimeError,'prequery'):
                        m.execute(args,{}, {'codeql/rust-all':{'root':'official'}},lambda *a:None,
                            db,{'Files':'query'},p,set(),SimpleNamespace(SOURCE='fixed'),files_only=True)
                state=json.loads((out/'comparison-terminal.json').read_text())['original']
                self.assertEqual(state['status'],'NOT_REACHED')
                self.assertEqual(state['queries']['Files'],'NOT_REACHED')
                self.assertFalse(list(out.glob('after-*-inventory.json')))


if __name__=='__main__': unittest.main()
