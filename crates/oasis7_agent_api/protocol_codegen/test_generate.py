"""RG-P01/P02 fault injection against the real compiler and publication path."""
import copy
import importlib.util
import json
from pathlib import Path
import shutil
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("generate", HERE / "generate.py")
g = importlib.util.module_from_spec(spec)
spec.loader.exec_module(g)
BASE = HERE.relative_to(g.ROOT)

class GenerationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        shutil.copytree(HERE, self.root / BASE, ignore=shutil.ignore_patterns("__pycache__", "generated"))
        source = g.generate()[0]["source"]
        (self.root / source).parent.mkdir(parents=True)
        shutil.copyfile(g.ROOT / source, self.root / source)
        self.source = self.root / source
        self.output = self.root / g.generate()[0]["output"]

    def tearDown(self):
        self.temp.cleanup()

    def mutate(self, change):
        source = json.loads(self.source.read_text())
        change(source)
        self.source.write_bytes(g.encoded(source))

    def test_logical_paths_are_portable_posix(self):
        from pathlib import PureWindowsPath, PurePosixPath
        for kind in (PureWindowsPath, PurePosixPath):
            base = kind("crates") / "oasis7_agent_api" / "protocol_codegen"
            self.assertEqual(g.logical(base / "generated"), "crates/oasis7_agent_api/protocol_codegen/generated")
            self.assertEqual(g.logical(base / "profile.json"), "crates/oasis7_agent_api/protocol_codegen/profile.json")
            self.assertEqual(g.logical(base / "toolchain.lock.json"), "crates/oasis7_agent_api/protocol_codegen/toolchain.lock.json")
            self.assertEqual(g.logical(base / "generate.py"), "crates/oasis7_agent_api/protocol_codegen/generate.py")
        profile, _, outputs = g.generate(self.root)
        self.assertNotIn(chr(92), profile["output"])
        manifest = json.loads(outputs["manifest.json"])
        self.assertTrue(all(chr(92) not in path for path in manifest["inputs"]))

    def test_deterministic_across_paths_and_unrelated_files(self):
        expected = g.generate()[2]
        self.assertEqual(g.generate(self.root)[2], expected)
        (self.root / "unrelated.md").write_text("unrelated documentation")
        self.assertEqual(g.generate(self.root)[2], expected)
        g.run("write", self.root)
        g.run("check", self.root)

    def test_check_is_read_only_and_reports_missing_stale_extra(self):
        with self.assertRaisesRegex(g.Invalid, "missing"):
            g.run("check", self.root)
        self.assertFalse(self.output.exists())
        g.run("write", self.root)
        (self.output / "schema.json").write_text("{}")
        (self.output / "extra.txt").write_text("keep")
        before = g.snapshot(self.output)
        with self.assertRaisesRegex(g.Invalid, "extra: extra.txt.*|extra: extra.txt"):
            g.run("check", self.root)
        self.assertEqual(before, g.snapshot(self.output))
        with self.assertRaisesRegex(g.Invalid, "extra output"):
            g.run("write", self.root)

    def test_invalid_refs_and_unsupported_semantics(self):
        self.mutate(lambda d: d["components"]["schemas"]["ReadError"].update({"$ref": "https://example.invalid/external"}))
        with self.assertRaisesRegex(g.Invalid, "invalid local"):
            g.generate(self.root)
        shutil.copyfile(g.ROOT / g.generate()[0]["source"], self.source)
        self.mutate(lambda d: d["components"]["schemas"]["ReadError"].update({"unevaluatedProperties": False}))
        with self.assertRaisesRegex(g.Invalid, "unsupported schema"):
            g.generate(self.root)

    def test_large_integer_literals_preserved(self):
        output = g.generate(self.root)[2]["schema.json"]
        self.assertIn(b"18446744073709551615", output)
        self.assertNotIn(b"18446744073709552000", output)

    def test_input_and_output_preimage_conflicts(self):
        g.run("write", self.root)
        original = g.snapshot(self.output)
        with self.assertRaisesRegex(g.Invalid, "input preimage"):
            g.run("write", self.root, lambda: self.source.write_bytes(self.source.read_bytes() + b"\n"))
        self.assertEqual(original, g.snapshot(self.output))
        with self.assertRaisesRegex(g.Invalid, "output preimage"):
            g.run("write", self.root, lambda: (self.output / "types.ts").write_text("conflict"))
        self.assertFalse((self.output.parent / ".generate.lock").exists())

    def test_interrupted_mixture_rejected_then_regenerated(self):
        g.run("write", self.root)
        self.mutate(lambda d: d["components"]["schemas"]["ReadError"].update({"description": "changed"}))
        replacement = g.generate(self.root)[2]
        (self.output / "schema.json").write_bytes(replacement["schema.json"])
        with self.assertRaisesRegex(g.Invalid, "stale"):
            g.run("check", self.root)
        g.run("write", self.root)
        g.run("check", self.root)

    def test_actual_interrupted_publication_is_detected(self):
        from unittest.mock import patch
        g.run("write", self.root)
        self.mutate(lambda d: d["components"]["schemas"]["ReadError"].update({"description": "new contract description"}))
        original_replace = g.os.replace
        calls = []
        def interrupt(source, target):
            calls.append(Path(target).name)
            if len(calls) == 2:
                raise OSError("simulated interruption after first rename")
            original_replace(source, target)
        with patch.object(g.os, "replace", side_effect=interrupt):
            with self.assertRaisesRegex(OSError, "simulated interruption"):
                g.run("write", self.root)
        self.assertEqual(calls, ["schema.json", "types.ts"])
        with self.assertRaisesRegex(g.Invalid, "stale: manifest.json"):
            g.run("check", self.root)
        g.run("write", self.root)
        g.run("check", self.root)

    def test_exact_input_and_output_capacity_boundaries(self):
        config = self.root / BASE / "profile.json"
        profile, inputs, outputs = g.generate(self.root)
        profile["limits"]["input_bytes"] = max(len(data) for data in inputs.values())
        profile["limits"]["output_bytes"] = sum(len(data) for data in outputs.values())
        config.write_bytes(g.encoded(profile))
        _, actual_inputs, actual_outputs = g.generate(self.root)
        self.assertEqual(max(map(len, actual_inputs.values())), profile["limits"]["input_bytes"])
        self.assertEqual(sum(map(len, actual_outputs.values())), profile["limits"]["output_bytes"])
        g.run("write", self.root)
        g.run("check", self.root)

    def test_lock_symlink_and_traversal(self):
        g.run("write", self.root)
        lock = self.output.parent / ".generate.lock"
        lock.write_text("interrupted writer")
        with self.assertRaisesRegex(g.Invalid, "lock exists"):
            g.run("write", self.root)
        lock.unlink()
        (self.output / "types.ts").unlink()
        (self.output / "types.ts").symlink_to(self.source)
        with self.assertRaisesRegex(g.Invalid, "invalid managed output"):
            g.run("check", self.root)
        with self.assertRaisesRegex(g.Invalid, "outside"):
            g.safe(self.root, "../escape")
        with self.assertRaisesRegex(g.Invalid, "symlink"):
            g.safe(self.root, (self.output / "types.ts").relative_to(self.root))

    def test_schema_keyword_values_fail_closed(self):
        for value in ({"minimum": "0"}, {"anyOf": []}, {"type": "bogus"}, {"required": ["x", "x"]}, {"additionalProperties": 3}, {"pattern": 1}):
            with self.subTest(value=value), self.assertRaises((g.Invalid, TypeError)):
                g.validate_schema(value, {})

    def test_regex_dialect_and_reserved_types_rejected(self):
        for pattern in ("(?P<name>a)", "a++", "a*+", "a?+", "a{1,2}+", "a{", "a]", r"\-"):
            with self.subTest(pattern=pattern), self.assertRaisesRegex(g.Invalid, "non-ECMAScript"):
                g.validate_schema({"type": "string", "pattern": pattern}, {})
        for name in ("as", "string", "class", "Operations", "Array"):
            with self.subTest(name=name):
                shutil.copyfile(g.ROOT / g.generate()[0]["source"], self.source)
                self.mutate(lambda d: d["components"]["schemas"].update({name: {"type": "string"}}))
                with self.assertRaisesRegex(g.Invalid, "invalid exported name"):
                    g.generate(self.root)

    def test_profile_cannot_redirect_managed_output(self):
        path = self.root / BASE / "profile.json"
        data = json.loads(path.read_text())
        data["output"] = "crates/oasis7_agent_api/src"
        path.write_bytes(g.encoded(data))
        with self.assertRaisesRegex(g.Invalid, "explicit managed paths"):
            g.generate(self.root)

    def test_recursive_component_aliases_fail_closed(self):
        for schemas in ({"Self": {"$ref": "#/components/schemas/Self"}}, {"A": {"$ref": "#/components/schemas/B"}, "B": {"$ref": "#/components/schemas/A"}}):
            with self.subTest(schemas=schemas), self.assertRaisesRegex(g.Invalid, "recursive component"):
                g.validate_reference_graph(schemas, 64)
        g.validate_reference_graph({"A": {"$ref": "#/components/schemas/B"}, "B": {"type": "string"}}, 64)

    def test_nullable_objects_and_type_expansion_capacity(self):
        schema = {"type": ["object", "null"], "properties": {"value": {"type": "string"}}, "additionalProperties": False}
        self.assertEqual(g.ts(schema), '({ "value"?: string; } | null)')
        shared = {"type": ["object", "null"], "properties": {"v": {"type": "string"}}, "allOf": [{"type": "string"}]}
        self.assertEqual(g.ts(shared).count("string"), 3)  # property, index signature, shared clause
        with self.assertRaisesRegex(g.Invalid, "expansion capacity"):
            g.ts({"type": "object", "properties": {"long_property_name": {"type": "string"}}}, [10])

    def test_cli_timeout_is_bounded_and_read_only(self):
        import subprocess
        import sys
        script = self.root / BASE / "generate.py"
        data = script.read_text().replace("class Invalid(ValueError):", "if '--worker' in sys.argv:\n    import time\n    time.sleep(3)\n\nclass Invalid(ValueError):")
        script.write_text(data)
        config = self.root / BASE / "profile.json"
        profile = json.loads(config.read_text())
        profile["limits"]["timeout_seconds"] = 1
        config.write_bytes(g.encoded(profile))
        result = subprocess.run([sys.executable, str(script), "check"], capture_output=True, text=True, timeout=5)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("timeout exceeded", result.stderr)
        self.assertFalse(self.output.exists())

    def test_capacity_depth_and_duplicate_keys(self):
        with self.assertRaisesRegex(g.Invalid, "capacity"):
            g.bounded([[[0]]], {"depth": 1, "nodes": 100})
        with self.assertRaisesRegex(g.Invalid, "capacity"):
            g.bounded([1, 2], {"depth": 10, "nodes": 2})
        with self.assertRaisesRegex(g.Invalid, "duplicate"):
            g.load(b"{\"a\":1,\"a\":2}")
        config = self.root / BASE / "profile.json"
        data = json.loads(config.read_text())
        data["limits"]["output_bytes"] = 1
        config.write_bytes(g.encoded(data))
        with self.assertRaisesRegex(g.Invalid, "capacity"):
            g.generate(self.root)

if __name__ == "__main__":
    unittest.main()
