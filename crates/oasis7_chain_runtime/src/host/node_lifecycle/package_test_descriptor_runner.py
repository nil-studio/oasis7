#!/usr/bin/env python3
"""Run the real runtime-compatibility decoder through its libtest child entry."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path


INPUT_ENV = "OASIS7_PACKAGE_COMPATIBILITY_CHILD_INPUT"
TEST_ENTRY = "node_lifecycle::package::compatibility::test_support::descriptor_child_entry"


def fail(message: str) -> None:
    print(message, file=sys.stderr)
    raise SystemExit(2)


def main() -> None:
    arguments = sys.argv[1:]
    if len(arguments) < 3 or arguments[1:3] != ["node", "runtime-compatibility"]:
        fail("test descriptor runner received an unexpected command")

    binary = Path(arguments[0])
    parser = argparse.ArgumentParser()
    parser.add_argument("--config", required=True)
    parser.add_argument("--data-dir", required=True)
    parser.add_argument("--minimum-full-sync-cut")
    parser.add_argument("--minimum-full-sync-cut-sha256")
    options = parser.parse_args(arguments[3:])
    if (options.minimum_full_sync_cut is None) != (
        options.minimum_full_sync_cut_sha256 is None
    ):
        fail("minimum full-sync cut path and digest must be supplied together")

    with tempfile.TemporaryDirectory(prefix="oasis7-package-descriptor-child-") as root:
        root_path = Path(root).resolve()
        input_path = root_path / "input.json"
        output_path = root_path / "output.json"
        child_input = {
            "phase": "candidate",
            "config_path": options.config,
            "data_dir": options.data_dir,
            "minimum_cut_path": options.minimum_full_sync_cut,
            "minimum_cut_sha256": options.minimum_full_sync_cut_sha256,
            "output_path": str(output_path),
        }
        input_path.write_text(json.dumps(child_input), encoding="utf-8")
        os.chmod(input_path, 0o600)
        environment = os.environ.copy()
        environment[INPUT_ENV] = str(input_path)
        try:
            result = subprocess.run(
                [str(binary), "--exact", TEST_ENTRY, "--nocapture"],
                check=False,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=30,
                env=environment,
            )
        except Exception as error:
            fail(f"cannot run the real libtest compatibility child: {error}")
        if result.returncode != 0:
            detail = result.stderr.decode("utf-8", errors="replace").strip()[:1024]
            fail(f"real libtest compatibility child failed: {detail}")
        if not output_path.is_file():
            fail("real libtest compatibility child produced no private result")
        try:
            child_output = json.loads(output_path.read_text(encoding="utf-8"))
        except Exception as error:
            fail(f"real libtest compatibility child returned invalid result data: {error}")
        if not isinstance(child_output, dict):
            fail("real libtest compatibility child returned an invalid result shape")
        descriptor = child_output.get("descriptor")
        child_error = child_output.get("error")
        if not isinstance(descriptor, dict) or child_error is not None:
            fail(str(child_error or "real libtest compatibility child omitted its descriptor"))
        try:
            expected_path = str(binary.resolve(strict=True))
        except Exception as error:
            fail(f"selected runtime path is unavailable: {error}")
        if descriptor.get("executable_path") != expected_path:
            fail("compatibility descriptor did not execute the exact selected runtime path")
        sys.stdout.write(json.dumps(descriptor, separators=(",", ":")))


if __name__ == "__main__":
    main()
