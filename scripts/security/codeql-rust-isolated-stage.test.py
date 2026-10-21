import csv
import hashlib
import importlib.util
from pathlib import Path
import tempfile
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
