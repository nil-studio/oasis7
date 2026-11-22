#!/usr/bin/env python3
"""Run all local-signer contracts and enforce native platform coverage."""
import argparse
import os
from pathlib import Path
import sys
import subprocess
import tempfile
import unittest

BOUNDARY_TEST = "store::tests::emitted_installer_binding_is_v3_and_policyless_doctor_is_unready"
NATIVE_TEST = "test_path_filesystem_contract.FilesystemContract.test_installed_darwin_descriptor"


class CoverageResult(unittest.TextTestResult):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.successful_ids = set()

    def getDescription(self, test):
        return test.id()

    def addSuccess(self, test):
        self.successful_ids.add(test.id())
        super().addSuccess(test)


def test_ids(suite):
    for test in suite:
        if isinstance(test, unittest.TestSuite):
            yield from test_ids(test)
        else:
            yield test.id()


def run_suite(suite, *, require_darwin=False, platform=None, stream=None):
    platform = sys.platform if platform is None else platform
    stream = sys.stderr if stream is None else stream
    ids = list(test_ids(suite))
    if require_darwin and platform != "darwin":
        print("local-signer coverage: --require-darwin requires Darwin", file=stream)
        return 1
    if not ids or NATIVE_TEST not in ids:
        print("local-signer coverage: empty suite or missing native descriptor test", file=stream)
        return 1
    result = unittest.TextTestRunner(stream=stream, verbosity=2,
                                     resultclass=CoverageResult).run(suite)
    allowed_skips = {NATIVE_TEST} if platform.startswith("linux") and not require_darwin else set()
    unexpected_skips = [test.id() for test, _ in result.skipped if test.id() not in allowed_skips]
    native_missing = platform == "darwin" and NATIVE_TEST not in result.successful_ids
    if unexpected_skips or native_missing:
        print(f"local-signer coverage: unexpected skips={unexpected_skips}; "
              f"native success missing={native_missing}", file=stream)
    return int(not result.wasSuccessful() or bool(unexpected_skips) or native_missing)


def run_installer_boundary(generate_fixture, *, stream=None):
    """Require evidence that the exact ignored Rust importer actually passed."""
    stream = sys.stderr if stream is None else stream
    root = Path(__file__).resolve().parent.parent
    with tempfile.TemporaryDirectory(prefix="oasis7-signer-boundary-") as directory:
        fixture = Path(directory) / "installer-boundary.json"
        generate_fixture(fixture)
        environment = dict(os.environ, OASIS7_INSTALLER_BOUNDARY_FIXTURE=str(fixture))
        environment.pop("RUSTC_WRAPPER", None)
        result = subprocess.run(
            ["cargo", "test", "--locked", "-p", "oasis7_local_signer", "--lib",
             "--", "--ignored", "--exact", BOUNDARY_TEST],
            cwd=root, env=environment, stdout=subprocess.PIPE, text=True, check=False,
        )
        print(result.stdout, end="", file=stream)
        lines = result.stdout.splitlines()
        if result.returncode or "running 1 test" not in lines or f"test {BOUNDARY_TEST} ... ok" not in lines:
            print("local-signer coverage: expected exact installer boundary importer success", file=stream)
            return 1
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--require-darwin", action="store_true")
    args = parser.parse_args()
    directory = Path(__file__).resolve().parent / "local-signer" / "tests"
    suite = unittest.TestLoader().discover(str(directory), pattern="test_*.py")
    status = run_suite(suite, require_darwin=args.require_darwin)
    if status or not args.require_darwin:
        return status
    # Discovery loaded the test module from the repository test directory.
    from test_release_installer import generate_boundary_fixture
    return run_installer_boundary(generate_boundary_fixture)


if __name__ == "__main__":
    sys.exit(main())
