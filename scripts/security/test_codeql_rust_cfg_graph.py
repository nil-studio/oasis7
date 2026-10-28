#!/usr/bin/env python3
"""Contract tests for bounded CFG diagnosis; no CodeQL process launched."""
import unittest
import tempfile
from unittest.mock import patch
import json
import os
import sys
import time
from pathlib import Path
import codeql_rust_cfg_graph as cfg

class GraphTests(unittest.TestCase):
    def test_evaluator_pagefile_is_not_an_evidence_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp); evidence=root/"evidence"; evidence.mkdir()
            code="import os,sys; f=open(sys.argv[3], \"wb\"); f.truncate(65*1024**2); f.close()"
            # Real child file growth crosses the old 64 MiB process limit.
            argv=[sys.executable,"-c",code,"query","run",str(root/"page")]
            limit=cfg.command_file_limit(["codeql","query","run","summary.ql"])
            cfg.runtime.run(argv,root,evidence/"child",time.monotonic()+10,10,evidence,os.environ.copy(),file_limit=limit)
            self.assertEqual((root/"page").stat().st_size,65*1024**2)
            decode_limit=cfg.command_file_limit(["codeql","bqrs","decode"])
            argv[-1]=str(root/"decoded")
            with self.assertRaises(cfg.runtime.PhaseError):
                cfg.runtime.run(argv,root,evidence/"decode",time.monotonic()+10,10,evidence,os.environ.copy(),file_limit=decode_limit)
            self.assertLessEqual((root/"decoded").stat().st_size,64*1024**2)

    def test_context_exclusion_requires_negative_and_positive_database_witnesses(self):
        paths=cfg.CONTEXT_PATHS
        good=[[p,0,0] if p in cfg.EXCLUDED_CONTEXT_PATHS else [p,50,10] for p in paths]
        self.assertEqual(cfg.validate_context_exclusion(good)["status"],"EXCLUSION_AST_VERIFIED")
        for index in range(len(paths)):
            bad=[row[:] for row in good]
            bad[index][1:]=[1,1] if paths[index] in cfg.EXCLUDED_CONTEXT_PATHS else [0,0]
            with self.assertRaises(RuntimeError):cfg.validate_context_exclusion(bad)
        for bad in (good[:2],good+[good[0]],[[paths[0],1,1],*good[1:]],[[paths[0],0,0],[paths[1],0,0],good[2]],[["other.rs",0,0],*good[1:]],[[paths[0],False,0],*good[1:]]):
            with self.assertRaises(RuntimeError):cfg.validate_context_exclusion(bad)

    def test_digest_exception_preserves_primary_error(self):
        terminal={'status':'DIAGNOSTIC_COMPLETED','error':'original query failure'}
        with tempfile.TemporaryDirectory() as tmp:
            p=Path(tmp)/'original';p.write_text('x')
            with patch.object(cfg.runtime,'digest',side_effect=OSError('injected read failure')):
                cfg.verify_originals(terminal,{str(p):'expected'})
        self.assertEqual(terminal['status'],'INCOMPLETE')
        self.assertFalse(terminal['originals_unchanged'])
        self.assertEqual(terminal['error'],'original query failure')
        self.assertEqual(terminal['integrity_error'],'injected read failure')
    def test_deadline_and_shared_operation_budget(self):
        with self.assertRaises(TimeoutError):cfg.directed_metrics([(1,2)],{1},0)
        budget=[0];r=cfg.directed_metrics([(1,2)],{1},budget=budget)
        self.assertEqual(r['positive_length_reachable_pairs'],'NOT_COMPUTED_OPERATION_CAP')
        with self.assertRaises(TimeoutError):cfg.graph_report([[1,2,1,'x',1,1,0]],[1],0)
    def test_stage_free_scope_and_external_cache(self):
        self.assertIn('s = n.getEnclosingCfgScope()',cfg.WRAPPER)
        self.assertNotIn('CfgInput::getCfgScope',cfg.WRAPPER)
    def test_cross_scope_cannot_claim_scope_complete(self):
        r=cfg.graph_report([[1,2,1,'x',2,1,0]],[1])['scopes'][0]
        self.assertFalse(r['scope_complete'])
        self.assertEqual(r['domain'],'SAME_SCOPE_INDUCED_EDGE_ENDPOINTS')
        self.assertEqual(r['positive_length_reachable_pairs'],0)
    def test_decoded_rowcount_and_set_reconciliation(self):
        with tempfile.TemporaryDirectory() as tmp:
            p=Path(tmp)/'data.json';p.write_text(json.dumps({'#select':{'tuples':[[1]]}}))
            self.assertEqual(cfg.decode_rows(p,{'result-sets':[{'name':'#select','rows':1}]}),[[1]])
            for info in ({'result-sets':[{'name':'#select','rows':2}]},{'result-sets':[{'name':'other','rows':1}]}):
                with self.assertRaises(RuntimeError):cfg.decode_rows(p,info)
    def test_chain_cycle_diamond_reachability(self):
        chain=cfg.directed_metrics([(1,2),(2,3)],{1})
        self.assertEqual(chain['scc_sizes'],[1,1,1]);self.assertEqual(chain['positive_length_reachable_pairs'],3)
        self.assertEqual(chain['starts_positive_length_pairs'],2)
        cycle=cfg.directed_metrics([(1,2),(2,3),(3,1)],{1})
        self.assertEqual(cycle['scc_sizes'],[3]);self.assertEqual(cycle['positive_length_reachable_pairs'],9)
        diamond=cfg.directed_metrics([(1,2),(1,3),(2,4),(3,4),(10,11)],{1,10})
        self.assertEqual(diamond['positive_length_reachable_pairs'],6)
        self.assertEqual(diamond['starts_positive_length_pairs'],4)
    def test_reachability_node_cap(self):
        result=cfg.directed_metrics([(i,i+1) for i in range(5001)],{0})
        self.assertEqual(result['positive_length_reachable_pairs'],'NOT_COMPUTED_NODE_CAP')
        self.assertEqual(len(result['scc_sizes']),5002)
    def test_patch_only_visibility(self):
        original = "private module Reachability {\nprivate predicate startsSplits(AstNode n) {}\nprivate predicate intraSplitsSucc(AstNode pred, AstNode succ) {}\nprivate predicate splitsBlockContains(AstNode start, AstNode n) = fastTC(intraSplitsSucc/2)(start,n)\n}"
        patched=cfg.patch_text('codeql/controlflow',original)
        self.assertEqual(patched, original.replace('private module Reachability','module Reachability').replace('private predicate startsSplits','predicate startsSplits').replace('private predicate intraSplitsSucc','predicate intraSplitsSucc'))
        self.assertIn('private predicate splitsBlockContains',patched)
    def test_patch_anchor_change_rejected(self):
        for text in ('', 'private module Reachability {'*2):
            with self.assertRaises(RuntimeError):cfg.patch_text('codeql/controlflow',text)
    def test_no_closure_in_diagnostic(self):
        self.assertNotIn('fastTC',cfg.SUMMARY+cfg.WRAPPER+cfg.edge_query([1]))
        self.assertNotIn('getAnElement',cfg.SUMMARY+cfg.WRAPPER)
        self.assertIn('intraSplitsSucc',cfg.WRAPPER)
    def test_largest_scope_not_silently_partial(self):
        selected,skipped=cfg.choose_scopes([[1,'giant',100001,1],[2,'small',2,1],[-1,'UNKNOWN_SCOPE',30,0]])
        self.assertEqual(selected,[2]);self.assertEqual(skipped,[{'scope':1,'reason':'EDGE_CAP','edges':100001}])
    def test_total_cap_and_top_bound(self):
        selected,skipped=cfg.choose_scopes([[i,'x',6000,1] for i in range(30)])
        self.assertEqual(len(selected),16);self.assertEqual(len(skipped),4)
    def test_summary_rejects_ambiguous_or_negative(self):
        for rows in ([[1,'x',-1,0]],[[1,'x',1,0],[1,'y',1,0]],[[True,'x',1,0]]):
            with self.assertRaises(RuntimeError):cfg.choose_scopes(rows)
    def test_graph_cycles_chains_and_cross_scope(self):
        rows=[[1,2,9,'loc',9,1,0],[2,3,9,'loc',9,0,0],[3,1,9,'loc',9,0,1],[10,11,9,'loc',8,0,0],[20,21,9,'loc',-1,0,0]]
        r=cfg.graph_report(rows,[9]);self.assertEqual(r['scopes'][0]['weak_components'],[3,2,2])
        self.assertEqual(r['cross_scope_edges'],1);self.assertEqual(r['unknown_target_scope_edges'],1)
        self.assertEqual(r['closure_cardinality'],'NOT_MEASURED');self.assertEqual(r['evaluator_internal_rows'],'UNKNOWN')
    def test_empty_edge_scope_is_explicit(self):
        r=cfg.graph_report([],[1]);self.assertEqual(r['scopes'][0]['nodes'],0)
        self.assertIn('isolated nodes excluded',r['graph_domain'])
    def test_duplicate_and_foreign_edges_rejected(self):
        row=[1,2,1,'x',1,1,0]
        for rows in ([row,row],[[1,2,2,'x',2,0,0]],[[1,2,1,'x',1,False,0]]):
            with self.assertRaises(RuntimeError):cfg.graph_report(rows,[1])
    def test_query_injection_and_no_scope_rejected(self):
        for scopes in ([],['1)'],[-1],[True]):
            with self.assertRaises(RuntimeError):cfg.edge_query(scopes)
    def test_unknown_source_retained_in_summary(self):
        self.assertIn('UNKNOWN_SCOPE',cfg.SUMMARY)
        self.assertIn('not exists(int t, string l | oasisNode(a,t,l))',cfg.SUMMARY)
        self.assertIn('count(int a | oasisStart(a)',cfg.SUMMARY)

if __name__=='__main__':unittest.main()
