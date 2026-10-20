#!/usr/bin/env python3
"""Fail closed unless one exact libtest unit test actually passed."""
import json
from pathlib import Path
import re
import sys


def verify(log_path, identity, exit_code):
    path = Path(log_path)
    text = path.read_text(encoding="utf-8", errors="replace")
    tests = re.findall(r"^test (\S+) \.\.\. (\S+) *$", text, re.MULTILINE)
    runs = re.findall(r"^running (\d+) tests? *$", text, re.MULTILINE)
    summaries = re.findall(
        r"^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; "
        r"(\d+) ignored; (\d+) measured; (\d+) filtered out; finished in [^\n]+$",
        text, re.MULTILINE,
    )
    valid = (exit_code == 0 and runs == ["1"] and tests == [(identity, "ok")]
             and len(summaries) == 1 and summaries[0][:5] == ("ok", "1", "0", "0", "0"))
    result = {
        "test_identity": identity,
        "process_exit_code": exit_code,
        "verified_test_count": int(valid),
        "status": "passed" if valid else "failed",
        "observed_tests": tests,
        "observed_run_counts": runs,
        "observed_summaries": summaries,
    }
    path.with_suffix(".result.json").write_text(json.dumps(result, indent=2) + "\n")
    with path.open("a") as log:
        log.write(f"\nverified_test_count={int(valid)}\n")
        if not valid:
            log.write("execution_validation=failed (expected one exact passing libtest result)\n")
    return 0 if valid else 1


if __name__ == "__main__":
    sys.exit(verify(sys.argv[1], sys.argv[2], int(sys.argv[3])))
