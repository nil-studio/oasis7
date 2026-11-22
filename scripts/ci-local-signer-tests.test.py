#!/usr/bin/env python3
import importlib.util
import io
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("signer_runner", HERE / "ci-local-signer-tests.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class HelperCoverage(unittest.TestCase):
    def native(self, outcome="success"):
        class Native(unittest.TestCase):
            def runTest(self):
                if outcome == "skip":
                    self.skipTest("actual Darwin ABI requires macOS")
                if outcome == "failure":
                    self.fail("native failure")
            def id(self):
                return runner.NATIVE_TEST
        return Native()

    def run_fixture(self, tests, **options):
        output = io.StringIO()
        status = runner.run_suite(unittest.TestSuite(tests), stream=output, **options)
        return status, output.getvalue()

    def test_zero_tests_and_missing_native_fail(self):
        self.assertEqual(self.run_fixture([], platform="linux")[0], 1)
        self.assertEqual(self.run_fixture([unittest.FunctionTestCase(lambda: None)], platform="linux")[0], 1)

    def test_linux_known_native_skip_and_darwin_success(self):
        status, output = self.run_fixture([self.native("skip")], platform="linux")
        self.assertEqual(status, 0)
        self.assertIn(runner.NATIVE_TEST, output)
        self.assertIn("Ran 1 test", output)
        self.assertIn("skipped=1", output)
        self.assertEqual(self.run_fixture([self.native()], platform="darwin", require_darwin=True)[0], 0)

    def test_failures_and_illegal_skips(self):
        self.assertEqual(self.run_fixture([self.native("failure")], platform="darwin")[0], 1)
        self.assertEqual(self.run_fixture([self.native("skip")], platform="darwin")[0], 1)
        def skipped():
            raise unittest.SkipTest("unexpected")
        self.assertEqual(self.run_fixture([self.native("skip"), unittest.FunctionTestCase(skipped)], platform="linux")[0], 1)

    def test_require_darwin_on_other_host_fails(self):
        self.assertEqual(self.run_fixture([self.native()], platform="linux", require_darwin=True)[0], 1)

    def test_boundary_runs_exact_consumer_with_private_generated_fixture(self):
        seen = []

        def cargo(command, **kwargs):
            self.assertEqual(command, ["cargo", "test", "--locked", "-p", "oasis7_local_signer",
                                       "--lib", "--", "--ignored", "--exact", runner.BOUNDARY_TEST])
            self.assertNotIn("RUSTC_WRAPPER", kwargs["env"])
            fixture = Path(kwargs["env"]["OASIS7_INSTALLER_BOUNDARY_FIXTURE"])
            self.assertEqual(fixture.read_text(), "{}")
            self.assertEqual(fixture.parent.stat().st_mode & 0o777, 0o700)
            seen.append(fixture)
            return SimpleNamespace(returncode=0, stdout=f"running 1 test\ntest {runner.BOUNDARY_TEST} ... ok\n")

        with patch.object(runner.subprocess, "run", side_effect=cargo):
            with patch.dict(runner.os.environ, RUSTC_WRAPPER="unexpected"):
                self.assertEqual(runner.run_installer_boundary(
                    lambda path: path.write_text("{}"), stream=io.StringIO()), 0)
        self.assertEqual(len(seen), 1)
        self.assertFalse(seen[0].parent.exists())

    def test_boundary_rejects_success_exit_without_exact_pass(self):
        for status, output in ((0, "running 0 tests\n"), (0, "test other ... ok\n"),
                               (0, f"test {runner.BOUNDARY_TEST} ... ok\n"),
                               (1, f"running 1 test\ntest {runner.BOUNDARY_TEST} ... ok\n")):
            with self.subTest(status=status, log=output):
                with patch.object(runner.subprocess, "run",
                                  return_value=SimpleNamespace(returncode=status, stdout=output)):
                    stream = io.StringIO()
                    self.assertEqual(runner.run_installer_boundary(
                        lambda path: path.write_text("{}"), stream=stream), 1)
                    self.assertIn("expected exact installer boundary importer success", stream.getvalue())

    def test_discovery_import_errors_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            Path(directory, "test_broken.py").write_text("raise ImportError('fixture import failure')")
            suite = unittest.TestLoader().discover(directory)
            suite.addTest(self.native("skip"))
            status = runner.run_suite(suite, platform="linux", stream=io.StringIO())
            self.assertEqual(status, 1)

    def test_discovery_collects_classes_after_main_guard(self):
        with tempfile.TemporaryDirectory() as directory:
            Path(directory, "test_late.py").write_text("import unittest\nif __name__ == '__main__': unittest.main()\nclass Late(unittest.TestCase):\n def test_after_main(self): pass\n")
            suite = unittest.TestLoader().discover(directory)
            self.assertEqual(suite.countTestCases(), 1)
            suite.addTest(self.native("skip"))
            self.assertEqual(runner.run_suite(suite, platform="linux", stream=io.StringIO()), 0)


if __name__ == "__main__":
    unittest.main()
