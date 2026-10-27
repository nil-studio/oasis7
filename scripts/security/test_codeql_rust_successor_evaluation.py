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


if __name__=='__main__': unittest.main()
