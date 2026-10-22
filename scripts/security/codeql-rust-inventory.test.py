#!/usr/bin/env python3
"""Observation overlay guards; these do not claim Rust API compilation."""
import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest
import sys
from unittest.mock import patch

sys.path.insert(0,str(Path(__file__).parent))
ROOT = Path(__file__).with_name('codeql-rust-inventory')
spec = importlib.util.spec_from_file_location('inventory_overlay', ROOT/'apply.py')
module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
SELECTION = '''                if let Some(file) = file.as_path().map(<_ as AsRef<Path>>::as_ref)
                    && file.extension().is_some_and(|ext| ext == "rs")
                    && processed_files.insert(file.to_owned())
                    && db.source_root(db.file_source_root(file_id).source_root_id(db)).source_root(db).is_library
                {
                    extractor.extract_with_semantics(file, &semantics, vfs, library_mode);
                    extractor.archiver.archive(file);
                }
'''
FIXTURE = ('''mod archive;
        self.extract(&RustAnalyzer::from(err), file, source_kind);
    extractor.emit_extraction_diagnostics(start, &cfg)
            let semantics = Semantics::new(db);
            for file in files {
                match extractor.load_source(file) {}
            }
            for (file_id, file) in vfs.iter() {
'''+SELECTION+'''            }
        } else {
''').encode()

class OverlayTests(unittest.TestCase):
    def test_original_selection_and_insert_remain_exactly_once(self):
        with patch.object(module,'MAIN_SHA',hashlib.sha256(FIXTURE).hexdigest()):
            candidate=module.patch_main(FIXTURE).decode()
        self.assertIn(SELECTION,candidate)
        self.assertEqual(candidate.count('processed_files.insert('),1)
        self.assertIn('processed_files.contains(path)',candidate)
        self.assertIn('observer.finish()',candidate)
        self.assertIn('inventory::without_semantics',candidate)
        self.assertIn('inventory::finish_fallback()',candidate)
        self.assertIn('OASIS7_RUST_INVENTORY',candidate)

    def test_changed_source_or_ambiguous_anchor_is_rejected(self):
        with self.assertRaises(RuntimeError):module.patch_main(FIXTURE)
        data=FIXTURE+b'mod archive;\n'
        with patch.object(module,'MAIN_SHA',hashlib.sha256(data).hexdigest()):
            with self.assertRaises(RuntimeError):module.patch_main(data)

    def test_owned_copy_required_and_existing_module_preserved(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);src=root/'rust/extractor/src';src.mkdir(parents=True)
            (src/'main.rs').write_bytes(FIXTURE)
            with self.assertRaises(RuntimeError):module.apply(root)
            (root/'.oasis7-owned-inventory-source').touch();(src/'inventory.rs').write_text('foreign')
            with patch.object(module,'MAIN_SHA',hashlib.sha256(FIXTURE).hexdigest()):
                with self.assertRaises(RuntimeError):module.apply(root)
            self.assertEqual((src/'inventory.rs').read_text(),'foreign')
            self.assertEqual((src/'main.rs').read_bytes(),FIXTURE)

    def test_missing_vfs_and_overflow_never_omit_source(self):
        text=(ROOT/'inventory.rs').read_text()
        self.assertIn('file_id: Option<FileId>',text)
        self.assertIn('UNKNOWN_NO_VFS_ID',text)
        self.assertIn('KEEP_UNKNOWN_NO_PHYSICAL_MODULE',text)
        self.assertIn('"extraction_action":"UNCHANGED"',text)
        self.assertIn('"provenance_complete":false',text)
        self.assertNotIn('extract_with_semantics',text)
        self.assertNotIn('processed_files.insert',text)
        self.assertIn('INSTRUMENTED_LAZY_QUERIES',text)

class CheckBoundaryTests(unittest.TestCase):
    def test_periodic_data_bound_is_observation_not_quota(self):
        spec=importlib.util.spec_from_file_location('check_driver',Path(__file__).with_name('codeql-rust-inventory-check.py'))
        driver=importlib.util.module_from_spec(spec);spec.loader.exec_module(driver)
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);(root/'data').write_text('bounded')
            with patch.object(driver.shutil,'disk_usage',return_value=type('Space',(),{'free':20*1024**3})()):
                self.assertIn('NOT_OS_HARD_QUOTA',driver.owned_data_guard(root)['kind'])
                with patch.object(driver,'DATA_LIMIT',1):
                    with self.assertRaises(RuntimeError):driver.owned_data_guard(root)
        source=Path(driver.__file__).read_text()
        self.assertIn("'check','--locked','-p','codeql-rust','--tests'",source)
        self.assertNotIn("'cargo','test'",source)
        self.assertNotIn("'cargo','fetch'",source)
        self.assertIn("RUSTUP_HOME=str(rustup.resolve())",source)
        self.assertIn("end-90",source)

    def test_leaf_rename_rescans_and_never_accepts_partial_total(self):
        spec=importlib.util.spec_from_file_location('check_races',Path(__file__).with_name('codeql-rust-inventory-check.py'))
        driver=importlib.util.module_from_spec(spec);spec.loader.exec_module(driver)
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);leaf=root/'moving';leaf.write_text('data')
            original=Path.lstat;calls=[]
            def race(path,*args,**kwargs):
                if path==leaf and not calls:
                    calls.append('renamed');leaf.rename(root/'moved')
                    raise FileNotFoundError('atomic rename')
                return original(path,*args,**kwargs)
            with patch.object(Path,'lstat',race),patch.object(driver.shutil,'disk_usage',return_value=type('Space',(),{'free':20*1024**3})()):
                value=driver.owned_data_guard(root)
            self.assertEqual(value['rescans'],1)
            self.assertEqual(value['entries'],1)
            with patch.object(Path,'lstat',new=lambda path,*a,**kw: (_ for _ in ()).throw(FileNotFoundError('race')) if path!=root else original(path,*a,**kw)):
                with self.assertRaisesRegex(RuntimeError,'INCOMPLETE'):
                    driver.owned_data_guard(root)
            with patch.object(Path,'lstat',new=lambda path,*a,**kw: (_ for _ in ()).throw(PermissionError('denied')) if path!=root else original(path,*a,**kw)):
                with self.assertRaises(PermissionError):driver.owned_data_guard(root)
        text=Path(driver.__file__).read_text()
        self.assertIn("'gc.auto','0'",text)
        self.assertIn("'maintenance.auto','false'",text)

    def test_literal_manual_check_upload_requires_budget_success(self):
        workflow=Path(__file__).resolve().parents[2]/'.github/workflows/rust.yml'
        text=workflow.read_text().split('  codeql-rust-inventory-check:',1)[1].split('  select:',1)[0]
        self.assertIn("github.event_name == 'workflow_dispatch'",text)
        self.assertIn("inputs.run_mode == 'codeql_rust_inventory_check'",text)
        self.assertIn("steps.inventory-budget.outcome == 'success'",text)
        self.assertNotIn('security-events:',text)
        self.assertNotIn('GITHUB_TOKEN:',text)
        self.assertIn('timeout-minutes: 20',text)

    def test_guard_failure_is_before_spawn(self):
        import codeql_rust_diagnostic_runtime as runtime
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)
            with patch.object(runtime.subprocess,'Popen') as spawn:
                with self.assertRaises(runtime.PhaseError):
                    runtime.run(['unused'],root,root/'run.log',__import__('time').monotonic()+5,2,root,{},
                        safety_check=lambda:(_ for _ in ()).throw(RuntimeError('space guard')))
                spawn.assert_not_called()

class ObservationTests(unittest.TestCase):
    def setUp(self):
        import codeql_rust_inventory_observe as observe
        self.observe=observe

    def test_bundle_copy_has_only_expected_binary_content_difference(self):
        import shutil
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);original=root/'original';original.mkdir()
            target=original/self.observe.EXTRACTOR;target.parent.mkdir(parents=True);target.write_bytes(b'official');target.chmod(0o755)
            (original/'lib.qll').write_text('unchanged')
            (original/'alias').symlink_to('lib.qll')
            before=self.observe.tree_manifest(original)
            copied=root/'copy'
            self.observe.copy_bundle(original,copied,before,__import__('time').monotonic()+10,lambda:None)
            self.assertEqual(before,self.observe.tree_manifest(copied))
            (copied/self.observe.EXTRACTOR).write_bytes(b'candidate')
            self.observe.verify_replacement(before,self.observe.tree_manifest(copied),hashlib.sha256(b'candidate').hexdigest())
            (copied/'lib.qll').write_text('drift')
            with self.assertRaises(RuntimeError):self.observe.verify_replacement(before,self.observe.tree_manifest(copied),hashlib.sha256(b'candidate').hexdigest())
            (original/'escape').symlink_to(root/'foreign')
            (root/'foreign').write_text('foreign')
            with self.assertRaises(RuntimeError):self.observe.tree_manifest(original)

    def logs(self):
        import json
        rows=[]
        for manifest,path in [('candidate/Cargo.toml','candidate/src/adapter.rs'),('common/Cargo.toml','common/src/driver.rs')]:
            rows.append({'kind':'file','process_id':123,'manifest':manifest,'path':path,'phase':'initial',
                'selected_library':False,'extraction_action':'UNCHANGED','category':'KEEP_EXPLICIT_INPUT','observation_incomplete':False})
            rows.append({'kind':'summary','process_id':123,'manifest':manifest,'rows':1,'initial_observations':1,
                'vfs_observations':0,'selected_library_observations':0,'dropped_rows':0,'map_incomplete':False,'extraction_action':'UNCHANGED'})
        rows.append({'kind':'without_semantics_summary','process_id':123,'observations':0,'dropped_rows':0,'extraction_action':'UNCHANGED'})
        return rows,[('stdout','\n'.join('[build-stdout] \x1b[32m INFO\x1b[0m OASIS7_INVENTORY '+json.dumps(r) for r in rows))]

    def test_context_reconciliation_and_unknown_never_exclude(self):
        rows,logs=self.logs();initial=['candidate/src/adapter.rs','common/src/driver.rs'];manifests=['candidate/Cargo.toml','common/Cargo.toml']
        result=self.observe.parse_inventory(logs,initial,manifests)
        self.assertEqual(result['status'],'OBSERVED_ZERO_OMISSION_CLASSIFICATION')
        self.assertFalse(result['safe_exclusion']);self.assertFalse(result['closure_proven'])
        import json
        duplicate=logs+[('stderr','INFO OASIS7_INVENTORY '+json.dumps(rows[0]))]
        self.assertEqual(self.observe.parse_inventory(duplicate,initial,manifests)['status'],'OBSERVED_INCOMPLETE')
        bad=logs+[('stderr','UNKNOWN OASIS7_INVENTORY not-json')]
        self.assertEqual(self.observe.parse_inventory(bad,initial,manifests)['status'],'OBSERVED_INCOMPLETE')
        self.assertEqual(self.observe.parse_inventory(logs,initial+['missing'],manifests)['status'],'OBSERVED_INCOMPLETE')

    def test_orphan_context_and_fallback_process_are_incomplete(self):
        import json
        rows,_=self.logs()
        extra=dict(rows[0],manifest='orphan/Cargo.toml')
        initial=['candidate/src/adapter.rs','common/src/driver.rs']
        manifests=['candidate/Cargo.toml','common/Cargo.toml']
        for additional in [extra,dict(rows[0],process_id=456),
            dict(rows[1],process_id=456,rows=0,initial_observations=0,vfs_observations=0,selected_library_observations=0),
            {'kind':'without_semantics','process_id':456,'path':'unknown.rs','library':True,
             'category':'KEEP_UNKNOWN_NO_SEMANTICS','observation_incomplete':False,'extraction_action':'UNCHANGED'}]:
            logs=[('stdout','\n'.join('INFO OASIS7_INVENTORY '+json.dumps(r) for r in rows+[additional]))]
            self.assertEqual(self.observe.parse_inventory(logs,initial,manifests)['status'],'OBSERVED_INCOMPLETE')
        no_fallback_summary=[r for r in rows if r['kind']!='without_semantics_summary']
        logs=[('stdout','\n'.join('INFO OASIS7_INVENTORY '+json.dumps(r) for r in no_fallback_summary))]
        self.assertEqual(self.observe.parse_inventory(logs,initial,manifests)['status'],'OBSERVED_INCOMPLETE')

    def test_truncated_or_unreconciled_summary_is_incomplete(self):
        import json
        rows,_=self.logs();rows[1]['rows']=99
        logs=[('stdout','\n'.join('INFO OASIS7_INVENTORY '+json.dumps(r) for r in rows))]
        self.assertEqual(self.observe.parse_inventory(logs,['candidate/src/adapter.rs','common/src/driver.rs'],['candidate/Cargo.toml','common/Cargo.toml'])['status'],'OBSERVED_INCOMPLETE')
        rows[1]['rows']=1;rows[1]['dropped_rows']=4
        logs=[('stdout','\n'.join('INFO OASIS7_INVENTORY '+json.dumps(r) for r in rows))]
        self.assertEqual(self.observe.parse_inventory(logs,['candidate/src/adapter.rs','common/src/driver.rs'],['candidate/Cargo.toml','common/Cargo.toml'])['status'],'OBSERVED_INCOMPLETE')

    def test_manifest_and_stable_analysis_are_separate_from_build(self):
        source=(ROOT/'inventory.rs').read_text()
        self.assertIn('"manifest":self.manifest',source)
        self.assertIn('"process_id":std::process::id()',source)
        text=Path(self.observe.__file__).read_text()
        self.assertIn("RUSTUP_TOOLCHAIN='1.96.0'",text)
        self.assertIn("'1.99.0'",text)
        self.assertIn("runtime.digest(source/'rust/extractor/src/toolchain.rs')",text)
        self.assertIn('build_end-time.monotonic()',text)
        self.assertNotIn("'run-queries'",text)
        self.assertNotIn("'cargo','test'",text)

if __name__=='__main__':unittest.main()
