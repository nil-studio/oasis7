import tempfile
from pathlib import Path
import unittest
from types import SimpleNamespace
from unittest.mock import patch
import time
import json

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

    def experiment(self, clean, broken_summary=False, spawned=True):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d); db=p/'db';db.mkdir();self.database(db)
            out=p/'out';out.mkdir();work=p/'work';work.mkdir()
            args=SimpleNamespace(output=out,work=work,codeql=Path('/fixed/codeql'),
                business_source=p,deadline=time.monotonic()+1000)
            calls=[]
            def run(argv,cwd,output,*rest):
                if argv[1:3]==['resolve','library-path']:
                    output.write_text('{}');return
                self.assertEqual(argv[1:3],['database','run-queries'])
                calls.append(argv[3])
                if broken_summary:
                    (out/('original-Files-evaluator.jsonl')).write_text('{}\n')
                raise m.runtime.PhaseError({'argv':argv,'failure_kind':'timeout',
                    'cleanup_errors':[] if clean else ['child still alive'],'evidence_errors':[],
                    'pid':123 if spawned else None,'pgid':123 if spawned else None,
                    'exit':-15 if spawned else None,'started_wall':time.time(),'elapsed':180.0})
            with patch.object(m.plan,'inventory',return_value={}),patch.object(m.plan,'prepare_copy',return_value=p/'private'),patch.object(m.runtime,'run',side_effect=run),patch.object(m.runtime,'official_summaries',return_value={'overall':{'status':'UNKNOWN'}}):
                if broken_summary:
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

    def test_failed_summary_prevents_candidate_even_after_clean_query_timeout(self):
        self.experiment(True, broken_summary=True)

    def test_pre_spawn_deadline_is_not_an_actual_query_timeout(self):
        self.experiment(True, spawned=False)

    def test_unclean_timeout_prevents_candidate(self):
        self.experiment(False)


if __name__=='__main__': unittest.main()
