import json
from contextlib import nullcontext
from pathlib import Path
import tempfile
import time
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import codeql_rust_peer_ast_inventory as m


class PrefixTests(unittest.TestCase):
    def scenario(self, mutation=None, failure=None, reserve_failure=False, slow_peer=False, df=False, df_timeout=False):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);work=root/'work';work.mkdir();db=work/'candidate-db';db.mkdir()
            (db/'db-rust/default').mkdir(parents=True)
            (db/'db-rust/default/input.rel').write_bytes(b'input')
            (db/'codeql-database.yml').write_text('rust')
            out=root/'out';out.mkdir();(out/'queries').mkdir()
            texts={'Files':'files query','PeerAst':'ast query'}
            if df:texts['PeerDf']='df query'
            for name,text in texts.items():(out/'queries'/(name+'.ql')).write_text(text)
            timer=[100.0]
            args=SimpleNamespace(output=out,work=work,codeql=Path('/fixed/codeql'),business_source=root,deadline=1100.0 if slow_peer or df_timeout else time.monotonic()+1000,probe='peer-df-inventory' if df else 'peer-ast-inventory')
            queries=[];summaries=[];query_ends=[];initial_cap=m.runtime.WRITE_CAP
            def receipt(argv):return {'argv':argv,'pid':123,'pgid':123,'exit':0,
                'started_wall':time.time(),'elapsed':1.0,'cleanup_errors':[],'evidence_errors':[]}
            def run(argv,cwd,output,end,*rest):
                if argv[1:3]==['resolve','library-path']:
                    output.write_text('{}');return receipt(argv)
                if argv[1:3]==['database','run-queries']:
                    name=Path(argv[4]).stem;queries.append(argv);query_ends.append(end)
                    if slow_peer or df_timeout:timer[0]+={'Files':18,'PeerAst':77 if df else 146,'PeerDf':20}[name]
                    r=receipt(argv)
                    if failure in ('pre-spawn','unsafe'):
                        r.update(failure_kind='timeout',exit=-15)
                        if failure=='pre-spawn':r.update(pid=None,pgid=None,exit=None)
                        else:r['cleanup_errors']=['live writer']
                        raise m.runtime.PhaseError(r)
                    target=Path(argv[3]);results=target/'results';results.mkdir(exist_ok=True)
                    (results/(name+'.bqrs')).write_bytes(b'complete')
                    if failure!='missing-log':(out/('prefix-'+name+'-evaluator.jsonl')).write_text('{}\n')
                    if mutation==name:(target/('derived-'+name)).write_text('generated')
                    if df_timeout and name=='PeerDf':
                        timer[0]=end;r.update(failure_kind='timeout',exit=143)
                        raise m.runtime.PhaseError(r)
                    return r
                if argv[1:3]==['bqrs','decode']:
                    if slow_peer or df_timeout:timer[0]+=1
                    if 'Files.bqrs' in argv[3]:output.write_text('"col0","col1"\n"file","relative"\n')
                    elif 'PeerAst.bqrs' in argv[3]:output.write_text('"label","present"\n"derive-ast",1\n')
                    else:output.write_text('"label","present"\n"derive-df",1\n"closure-flow",1\n')
                    return receipt(argv)
                self.assertEqual(argv[1:3],['generate','log-summary'])
                summaries.append((argv,end))
                if slow_peer or df_timeout:timer[0]+=5 if "Files" in argv[-1] else 1
                if failure=='summary':
                    r=receipt(argv);r.update(failure_kind='timeout',pid=None,pgid=None,exit=None)
                    raise m.runtime.PhaseError(r)
                output.write_text('{}\n');return receipt(argv)
            def validate(name,text,rows):
                if name=='PeerAst':
                    self.assertEqual((text,rows),('ast query',[['derive-ast','1']]))
                    return {'derive-ast':1}
                self.assertEqual((name,text,rows),('PeerDf','df query',[['derive-df','1'],['closure-flow','1']]))
                return {'derive-df':1,'closure-flow':1}
            model=SimpleNamespace(SOURCE='fixed',validate_partition=validate,
                check_files=lambda rows,wanted:self.assertEqual(rows,[['file','relative']]))
            reservation=m.inventory.reserve_after_maps
            def reserve(output,baseline):
                if reserve_failure:raise RuntimeError('NOT_REACHED reserve')
                return reservation(output,baseline)
            with patch.object(m.runtime,'run',side_effect=run),patch.object(m.inventory,'reserve_after_maps',side_effect=reserve), (patch.object(m.time,'monotonic',side_effect=lambda:timer[0]) if slow_peer or df_timeout else nullcontext()):
                if mutation or failure or reserve_failure or df_timeout:
                    with self.assertRaises((RuntimeError,m.runtime.PhaseError)):
                        m.execute(args,{}, {'codeql/rust-all':{'root':'official'}},lambda *a:None,
                            db,texts,root,{'file'},model)
                else:m.execute(args,{}, {'codeql/rust-all':{'root':'official'}},lambda *a:None,db,texts,root,{'file'},model)
            state=json.loads((out/'prefix-terminal.json').read_text())
            names=[Path(a[4]).stem for a in queries]
            self.assertEqual(m.runtime.WRITE_CAP,initial_cap)
            if mutation=='Files':
                self.assertEqual(names,['Files'])
                self.assertEqual(state['queries']['PeerAst']['status'],'NOT_REACHED')
            elif mutation in ('PeerAst','PeerDf') or df_timeout or not(failure or reserve_failure):
                expected=['Files','PeerAst','PeerDf'] if df and mutation!='PeerAst' else ['Files','PeerAst']
                self.assertEqual(names,expected)
                self.assertEqual(len({a[3] for a in queries}),1)
                self.assertEqual([a for a in queries[0] if 'common-caches=' in a or 'compilation-cache=' in a],
                    [a for a in queries[1] if 'common-caches=' in a or 'compilation-cache=' in a])
                self.assertEqual(len(set(query_ends)),1)
                self.assertTrue((out/'after-Files-arm-inventory.json').exists())
                self.assertTrue((out/'after-PeerAst-arm-inventory.json').exists())
                if df and mutation!='PeerAst':self.assertTrue((out/'after-PeerDf-arm-inventory.json').exists())
                if df:self.assertEqual(state['queries']['PeerAst']['snapshot_budget'],'shared-query-window')
            elif failure=='unsafe':
                self.assertEqual(state['queries']['Files']['facts_after']['arm']['status'],'UNKNOWN')
                self.assertFalse(list(out.glob('after-*-inventory.json')))
                self.assertFalse(summaries)
            elif failure=='pre-spawn' or reserve_failure:
                self.assertEqual(state['queries']['Files']['status'],'NOT_REACHED')
                self.assertFalse(list(out.glob('after-*-inventory.json')))
                self.assertFalse(summaries)
            elif failure in ('summary','missing-log'):
                self.assertEqual(names,['Files'])
                self.assertEqual(state['queries']['Files']['status'],'FAILED_SUMMARIES')
                self.assertEqual(state['queries']['Files']['pre_summary_status'],'DECODED_VALIDATED')
                self.assertTrue((out/'after-Files-arm-inventory.json').exists())
                self.assertEqual(state['queries']['PeerAst']['status'],'NOT_REACHED')
            if mutation:
                phase=state['queries'][mutation]
                self.assertEqual(phase['status'],'FAILED_FACTS_AFTER')
                self.assertEqual(phase['facts_after']['source']['status'],'UNCHANGED')
                self.assertEqual(phase['facts_after']['arm']['added'][0]['path'],'derived-'+mutation)
            if not(mutation or failure or reserve_failure or df_timeout):
                self.assertTrue(all(deadline<=query_ends[0] for _,deadline in summaries))
                self.assertEqual(len(summaries),6 if df else 4)
                if slow_peer:self.assertEqual(summaries[-1][1],query_ends[0])
                self.assertEqual(json.loads((out/'prefix-result.json').read_text())['status'],'PREFIX_TRACKED_FACTS_UNCHANGED')
            if df_timeout:
                peer=state['queries']['PeerDf']
                self.assertEqual(peer['summary_budget'],'terminal-postprocessing')
                self.assertGreater(summaries[-1][1],query_ends[0])
                self.assertGreater(peer['remaining_query_seconds_before_call'],0)
                self.assertLess(peer['remaining_query_seconds_before_call'],180)
                self.assertEqual(peer.get('pre_fact_status',peer['status']),'CLEAN_QUERY_TIMEOUT')
                self.assertFalse((out/'prefix-PeerDf.bqrs').exists())

    def test_ordered_prefix_same_db_and_caches_shared_clock_and_unique_snapshots(self):self.scenario()
    def test_first_files_difference_stops_before_peer_ast(self):self.scenario(mutation='Files')
    def test_peer_ast_difference_is_captured_without_overwriting_files_snapshot(self):self.scenario(mutation='PeerAst')
    def test_unsafe_cleanup_never_hashes_live_writer(self):self.scenario(failure='unsafe')
    def test_pre_spawn_refusal_has_no_after_snapshot(self):self.scenario(failure='pre-spawn')
    def test_summary_pre_spawn_failure_still_preserves_clean_query_after_maps(self):self.scenario(failure='summary')
    def test_missing_evaluator_log_cannot_pass_or_start_peer_ast(self):self.scenario(failure='missing-log')
    def test_late_peer_summary_is_clipped_to_shared_query_clock(self):self.scenario(slow_peer=True)
    def test_three_stage_fixed_order_and_same_caches(self):self.scenario(df=True)
    def test_peer_ast_difference_prevents_peer_df(self):self.scenario(df=True,mutation='PeerAst')
    def test_peer_df_clean_timeout_keeps_complete_after_maps_and_terminal_summaries(self):self.scenario(df=True,df_timeout=True)
    def test_peer_df_timeout_mutation_keeps_earlier_unchanged_snapshots(self):self.scenario(df=True,df_timeout=True,mutation='PeerDf')
    def test_fixed_mode_rejects_extra_or_reordered_queries(self):
        for sequence in [('Files','PeerDf','PeerAst'),('Files','PeerAst','PeerDf','CallsCandidate')]:
            with self.subTest(sequence=sequence),self.assertRaisesRegex(RuntimeError,'fixed probe sequence'):
                m.execute(SimpleNamespace(probe='peer-df-inventory'),{}, {},None,None,dict.fromkeys(sequence,''),None,None,None)

    def test_reserve_refusal_is_not_reached(self):self.scenario(reserve_failure=True)


if __name__=='__main__':unittest.main()
