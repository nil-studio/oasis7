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

if __name__=='__main__':unittest.main()
