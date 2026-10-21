#!/usr/bin/env python3
import csv
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("diagnostic", Path(__file__).with_name("codeql-rust-model-diagnostic.py"))
D = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(D)


class FixtureContract(unittest.TestCase):
    def test_original_sources_unchanged_and_real_targets_remain_distinct(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            for name, wrappers in [("baseline", False), ("wrappers", True)]:
                source = root / name
                D.fixture(source, wrappers)
                for child, version in [("candidate", "1.0.0"), ("legacy", "2.0.0")]:
                    manifest = source / child / "Cargo.toml"
                    result = subprocess.run(["cargo", "metadata", "--offline", "--format-version=1",
                                             "--manifest-path", str(manifest)],
                                            capture_output=True, text=True, timeout=20,
                                            env=dict(os.environ, CARGO_NET_OFFLINE="true"))
                    self.assertEqual(result.returncode, 0, result.stderr)
                    metadata = json.loads(result.stdout)
                    dependency = [p for p in metadata["packages"] if p["name"] == "model-dep"]
                    self.assertEqual([p["version"] for p in dependency], [version])
                    package = next(p for p in metadata["packages"] if p["name"] == "model-" + child)
                    targets = {t["name"]: t["src_path"] for t in package["targets"]}
                    self.assertEqual(Path(targets["peer"]).resolve(), (source / "peer.rs").resolve())
                    self.assertEqual("codeql_coverage" in targets, wrappers)
            for path in ["peer.rs", "candidate/Cargo.toml", "legacy/Cargo.toml"]:
                self.assertEqual((root / "baseline" / path).read_bytes(), (root / "wrappers" / path).read_bytes())


def tables(root):
    for layout in ("baseline", "wrappers"):
        macros = [["src/lib.rs", "1", "route", "2", "1"], ["orphan.rs", "1", "route", "0", "0"]]
        calls = [["src/lib.rs", "1", "version_one_sink", "deps/v1/src/lib.rs", "1"]]
        if layout == "wrappers":
            macros += [["peer.rs", "1", "route", "2", "1"]]
            calls += [["peer.rs", "1", "version_one_sink", "deps/v1/src/lib.rs", "1"],
                      ["peer.rs", "1", "version_two_sink", "deps/v2/src/lib.rs", "1"]]
        macros += [["src/panic_shape.rs", "4", "format", "2", "1"],
                   ["src/panic_shape.rs", "5", "assert", "0", "0"],
                   ["src/panic_shape.rs", "5", "format", "0", "0"]]
        for query, rows in [("Macros", macros), ("Calls", calls),
                            ("Closures", [["panic-closure", "1", "1"]])]:
            with (root / f"{layout}-{query}.csv").open("w") as stream:
                writer = csv.writer(stream)
                writer.writerow(["a", "b", "c", "d", "e"])
                writer.writerows(rows)


class ModelControls(unittest.TestCase):
    def test_expected_table(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            tables(root)
            self.assertEqual(D.check_tables(root)["production_coverage"], "NOT_ESTABLISHED")

    def test_cannot_relabel_same_dependency_as_second_context(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            tables(root)
            p = root / "wrappers-Calls.csv"
            p.write_text(p.read_text().replace("deps/v2/src/lib.rs", "deps/v1/src/lib.rs"))
            with self.assertRaisesRegex(RuntimeError, "both actual dependency"):
                D.check_tables(root)

    def test_no_flow_is_not_success(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            tables(root)
            p = root / "wrappers-Calls.csv"
            p.write_text(p.read_text().replace("deps/v2/src/lib.rs,1", "deps/v2/src/lib.rs,0"))
            with self.assertRaisesRegex(RuntimeError, "both actual dependency"):
                D.check_tables(root)

    def test_negative_control_must_have_no_expansion(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            tables(root)
            p = root / "baseline-Macros.csv"
            p.write_text(p.read_text().replace("orphan.rs,1,route,0,0", "orphan.rs,1,route,1,0"))
            with self.assertRaisesRegex(RuntimeError, "negative model control"):
                D.check_tables(root)

    def test_missing_panic_row_is_not_zero_expansion(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            tables(root)
            p = root / "baseline-Macros.csv"
            p.write_text(p.read_text().replace("src/panic_shape.rs,5,assert,0,0\n", ""))
            with self.assertRaisesRegex(RuntimeError, "panic outer macro row missing"):
                D.check_tables(root)

    def test_direct_macro_requires_expansion_and_flow(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            tables(root)
            p = root / "baseline-Macros.csv"
            p.write_text(p.read_text().replace("src/lib.rs,1,route,2,1", "src/lib.rs,1,route,2,0"))
            with self.assertRaisesRegex(RuntimeError, "direct-positive"):
                D.check_tables(root)


class ProcessBoundaries(unittest.TestCase):
    def test_existing_output_is_never_claimed_or_written(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            before = list(root.iterdir())
            with patch.object(sys, "argv", ["diagnostic", "--codeql", "/unavailable",
                                            "--output", str(root), "--work", str(root / "work"),
                                            "--deadline", str(time.time() + 5)]):
                with self.assertRaises(FileExistsError):
                    D.main()
            self.assertEqual(list(root.iterdir()), before)

    def test_expired_deadline_spawns_nothing(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            with patch.object(D.subprocess, "Popen") as spawn:
                with self.assertRaisesRegex(RuntimeError, "before spawn"):
                    D.run([sys.executable, "-c", "pass"], root, root / "out.log", time.monotonic() - 1, 1, root, os.environ)
                spawn.assert_not_called()

    def test_log_cap_is_checked_before_write(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            with patch.object(D, "CAP", 1024 * 1024 + 1024):
                with self.assertRaisesRegex(RuntimeError, "before write"):
                    D.run([sys.executable, "-c", "print('x'*4096)"], root, root / "out.log", time.monotonic() + 5, 2, root, os.environ)
            self.assertEqual((root / "out.log").stat().st_size, 0)
            self.assertEqual(json.loads((root / "out.log.result.json").read_text())["cleanup_errors"], [])


    def test_selector_close_failure_still_reaps_and_saves(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            selector = D.selectors.DefaultSelector()
            close = selector.close
            def broken_close():
                close()
                raise OSError("controlled selector close failure")
            with patch.object(selector, "close", side_effect=broken_close), patch.object(D.selectors, "DefaultSelector", return_value=selector):
                with self.assertRaisesRegex(RuntimeError, "controlled selector close failure"):
                    D.run([sys.executable, "-c", "import time; time.sleep(10)"], root, root / "out.log", time.monotonic()+5, .1, root, os.environ)
            result = json.loads((root / "out.log.result.json").read_text())
            self.assertIsNotNone(result["exit"])
            with self.assertRaises(ProcessLookupError):
                os.kill(result["pid"], 0)

    def test_term_and_first_wait_failure_still_kills_reaps_and_saves(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            real_killpg = D.os.killpg
            real_wait = D.subprocess.Popen.wait
            waits = []
            def killpg(pid, sig):
                if sig == D.signal.SIGTERM:
                    raise PermissionError("controlled TERM failure")
                return real_killpg(pid, sig)
            def wait(process, *args, **kwargs):
                waits.append(process.pid)
                if len(waits) == 1:
                    raise OSError("controlled wait failure")
                return real_wait(process, *args, **kwargs)
            with patch.object(D.os, "killpg", side_effect=killpg), patch.object(D.subprocess.Popen, "wait", new=wait):
                with self.assertRaisesRegex(RuntimeError, "controlled TERM failure"):
                    D.run([sys.executable, "-c", "import time; time.sleep(10)"], root, root / "out.log", time.monotonic()+5, .1, root, os.environ)
            result = json.loads((root / "out.log.result.json").read_text())
            self.assertGreaterEqual(len(waits), 2)
            self.assertIsNotNone(result["exit"])
            with self.assertRaises(ProcessLookupError):
                os.kill(result["pid"], 0)

    def test_timeout_reaps_owned_process(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            with self.assertRaisesRegex(RuntimeError, "phase deadline"):
                D.run([sys.executable, "-c", "import time; time.sleep(10)"], root, root / "out.log", time.monotonic() + 5, .1, root, os.environ)
            result = json.loads((root / "out.log.result.json").read_text())
            self.assertEqual(result["cleanup_errors"], [])
            with self.assertRaises(ProcessLookupError):
                os.kill(result["pid"], 0)


if __name__ == "__main__":
    unittest.main()
