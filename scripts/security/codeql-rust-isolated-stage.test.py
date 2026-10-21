import csv
import hashlib
import importlib.util
from pathlib import Path
import tempfile
import json
import time
from types import SimpleNamespace
from unittest.mock import patch
import unittest

spec = importlib.util.spec_from_file_location("isolated", Path(__file__).with_name("codeql-rust-isolated-stage.py"))
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


class Isolated(unittest.TestCase):
    def source(self, root):
        path = root / m.STAGE
        path.parent.mkdir(parents=True)
        path.write_text("module Stages {\n" + m.OLD + "\n cached predicate ref() {1=1}\n }\n}\n")
        return hashlib.sha256(path.read_bytes()).hexdigest()

    def test_annotation_only_copy_preserves_official_and_all_formulas(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); original = root / "original"; candidate = root / "candidate"
            digest = self.source(original)
            with patch.object(m, "BEFORE_SHA", digest):
                before, after = m.copy_and_patch(original, candidate)
            self.assertEqual(before, m.inventory(original))
            self.assertNotEqual(before[m.STAGE], after[m.STAGE])
            self.assertEqual((candidate / m.STAGE).read_text().replace(m.NEW, m.OLD, 1), (original / m.STAGE).read_text())

    def test_interaction_copy_preserves_fallback_and_synth_override(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); original = root / "official"
            stage_sha = self.source(original)
            location = original / m.LOCATABLE; location.parent.mkdir(parents=True, exist_ok=True)
            text = "final override Location getLocation() { synth() }\n" + m.LOCATION_OLD + "\n defaultFallback(this)\n}\n"
            location.write_text(text)
            with patch.object(m, "BEFORE_SHA", stage_sha), patch.object(m, "LOCATION_SHA", hashlib.sha256(text.encode()).hexdigest()):
                before, after = m.copy_and_patch(original, root / "candidate", True)
            self.assertEqual({p for p in before if before[p] != after[p]}, {m.STAGE, m.LOCATABLE})
            self.assertEqual((root / "candidate" / m.LOCATABLE).read_text().replace(m.LOCATION_NEW, m.LOCATION_OLD, 1), text)
            self.assertEqual(m.inventory(original), before)

    def test_complete_group_cut_keeps_individual_macro_formulas_cached(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); official = root / "official"
            stage_sha = self.source(official)
            location = official / m.LOCATABLE; location.parent.mkdir(parents=True, exist_ok=True)
            location.write_text(m.LOCATION_OLD + "\n getLocationDefault(this)\n}\n")
            macro = official / m.MACRO_MODULE
            text = m.MACRO_OLD + "\n cached predicate isRelevantElement() { any() }\n cached predicate isFromMacroExpansion() { originalFormula() }\n}\n"
            macro.write_text(text)
            with patch.object(m,"BEFORE_SHA",stage_sha), patch.object(m,"LOCATION_SHA",hashlib.sha256(location.read_bytes()).hexdigest()), patch.object(m,"MACRO_SHA",hashlib.sha256(text.encode()).hexdigest()):
                before, after = m.copy_and_patch(official, root / "candidate", True, True)
            self.assertEqual({p for p in before if before[p] != after[p]}, {m.STAGE, m.LOCATABLE, m.MACRO_MODULE})
            self.assertEqual((root/"candidate"/m.MACRO_MODULE).read_text().replace(m.MACRO_NEW,m.MACRO_OLD,1),text)
            self.assertEqual(m.inventory(official),before)

    def test_actual_plan_wiring_only_compiles_and_keeps_copies_separate(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); official = root / "bundle/qlpacks/codeql/rust-all/0.2.23"
            sha = self.source(official)
            location = official / m.LOCATABLE; location.parent.mkdir(parents=True, exist_ok=True)
            location.write_text(m.LOCATION_OLD + "\n fallback(this)\n}\n")
            locsha = hashlib.sha256(location.read_bytes()).hexdigest()
            work = root / "work"; output = root / "output"; work.mkdir(); output.mkdir()
            args = SimpleNamespace(codeql=root / "bundle/codeql", work=work, output=output, deadline=time.monotonic()+1000)
            calls = []
            def run(argv, cwd, log, *rest, **kwargs):
                calls.append(argv)
                if "--help" in argv: log.write_text("--[no-]dump-ra")
                elif "resolve" in argv:
                    query = Path(next(a.split("=",1)[1] for a in argv if a.startswith("--query=")))
                    library = Path(next(a.split("=",1)[1] for a in argv if a.startswith("--additional-packs=")))
                    log.write_text(json.dumps({"libraryPath": [str(library), str(query.parent)], "dbscheme": str(library/"rust.dbscheme"),
                        "compilationCache": [], "qlPackName": "oasis7/rust-model-diagnostic", "relativeName": "oasis7/rust-model-diagnostic/SourceAst.ql"}))
                elif "compile" in argv:
                    self.assertIn("--dump-ra", argv); self.assertIn("--no-default-compilation-cache", argv); log.write_text("dummy RA")
                else: self.fail("unexpected non-compile command")
            def write(path, text): path.parent.mkdir(parents=True, exist_ok=True); path.write_text(text)
            def evidence(path, data, artifact): path.write_bytes(data)
            driver = SimpleNamespace(run=run, write=write, LIBRARY_VERSIONS={"rust-all": "0.2.23"})
            production = SimpleNamespace(evidence_write=evidence, QUERIES={"SourceAst": "import rust\nselect 1\n"})
            with patch.object(m, "BEFORE_SHA", sha), patch.object(m, "LOCATION_SHA", locsha), patch.object(m.shutil,"disk_usage",return_value=SimpleNamespace(free=8*1024**3)):
                m.inspect_location_plans(driver, production, args, {})
            self.assertEqual(len(calls), 5)
            self.assertFalse(any("database" in call or "run" in call for call in calls))
            result = json.loads((output / "location-plan-result.json").read_text())
            self.assertEqual(result["query_evaluations"], 0)
            self.assertEqual(len({p["compilation_cache"] for p in result["plans"].values()}), 2)
            self.assertTrue(json.loads((output/"location-plan-official-after.json").read_text())["unchanged"])

    def test_wrong_source_zero_copy_and_precompiled_or_link_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); source = root / "source"; self.source(source)
            with self.assertRaisesRegex(RuntimeError, "frozen"):
                m.copy_and_patch(source, root / "candidate")
            self.assertFalse((root / "candidate").exists())
            artifact = source / "stale.qlx"; artifact.write_bytes(b"old")
            with self.assertRaisesRegex(RuntimeError, "non-source"):
                m.inventory(source)
            artifact.unlink(); artifact.symlink_to(source / m.STAGE)
            with self.assertRaisesRegex(RuntimeError, "linked"):
                m.inventory(source)

    def test_resolver_cannot_select_official_instead_of_copy(self):
        roots = [Path("/owned/rust-all"), Path("/bundle/util")]; query = Path("/owned/query")
        value = {"libraryPath": list(map(str, roots)) + [str(query)], "dbscheme": "/owned/rust-all/rust.dbscheme",
                 "compilationCache": [], "qlPackName": "oasis7/rust-model-diagnostic", "relativeName": "oasis7/rust-model-diagnostic/Macros.ql"}
        m.verify_resolver(value, roots, query, "Macros", roots[0])
        value["libraryPath"][0] = "/bundle/rust-all"
        with self.assertRaisesRegex(RuntimeError, "roots differ"):
            m.verify_resolver(value, roots, query, "Macros", roots[0])

    def test_reference_zero_positive_generated_and_nested_controls(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            rows = {"Macros": [["src/lib.rs", "1", "route", "2", "1"], ["orphan.rs", "1", "route", "0", "0"],
                ["src/panic_shape.rs", "4", "format", "2", "1"], ["src/panic_shape.rs", "5", "assert", "2", "1"], ["src/panic_shape.rs", "5", "format", "2", "1"], ["src/panic_shape.rs", "5", "...::panic_2021", "2", "1"]],
                "Calls": [["src/lib.rs", "1", "version_one_sink", "deps/v1/src/lib.rs", "1"]], "Closures": [["panic-closure", "1", "1"]]}
            def write():
                for name, table in rows.items():
                    with (root / ("isolated-original-" + name + ".csv")).open("w") as stream:
                        csv.writer(stream).writerows([["header"], *table])
            write(); self.assertTrue(m.tables(root, "original"))
            for column in (3, 4):
                rows["Macros"][2][column] = "0"; write()
                with self.assertRaisesRegex(RuntimeError, "nested expansion/flow"):
                    m.tables(root, "original")
                rows["Macros"][2][column] = "2"
            rows["Closures"][0][2] = "0"; write()
            with self.assertRaisesRegex(RuntimeError, "closure AST/flow"):
                m.tables(root, "original")
            rows["Closures"][0][2] = "1"
            rows["Macros"][1][3] = "1"; write()
            with self.assertRaisesRegex(RuntimeError, "zero control"):
                m.tables(root, "original")


if __name__ == "__main__": unittest.main()
