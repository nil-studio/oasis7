#!/usr/bin/env bash
set -euo pipefail
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"
python_bin=$(./scripts/find-python-with-module.sh tomllib)

"$python_bin" - <<'PYTHON'
from pathlib import Path
import os
import subprocess
import tempfile

checker = "./scripts/check-rustsec-ignore-baseline.sh"
valid = """[advisories]
ignore = []
[bans]
deny = [{name = 'serde_cbor'}, {name = 'paste'}, {name = 'ttf-parser'}]
"""

def check(content, passes, diagnostic=""):
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "deny.toml"
        path.write_text(content, encoding="utf-8")
        result = subprocess.run(
            [checker], env={**os.environ, "OASIS7_RUSTSEC_DENY_TOML": str(path)},
            text=True, capture_output=True,
        )
    assert (result.returncode == 0) == passes, (content, result.stdout, result.stderr)
    assert diagnostic in result.stderr, result.stderr

check(Path("deny.toml").read_text(), True)
check(valid, True)
approved = '{id = "RUSTSEC-2026-0543", reason = "maintenance risk accepted"}'
check(valid.replace("ignore = []", "ignore = [" + approved + "]"), True)
# Config regression only: the synthetic ID is not a real vulnerability finding.
# cargo-deny still evaluates real advisories in CI, without our own filter.
for denied in (
    approved.replace("2026-0543", "2099-9999"),
    approved + ', "RUSTSEC-2099-9999"',
    approved + ', ' + approved,
    approved.replace('"maintenance risk accepted"', '" "'),
    approved.replace('"maintenance risk accepted"', 'false'),
    approved.replace(', reason = "maintenance risk accepted"', ''),
    approved[:-1] + ', allow = ["aliyun-tablestore-rs@0.1.2"]}',
    approved[:-1] + ', expiry = "2026-12-31"}',
):
    check(valid.replace("ignore = []", "ignore = [" + denied + "]"), False,
          "single approved maintenance exception")

check(valid.replace("[advisories]", "['advisories']") + "# RUSTSEC-2024-0436\n", True)
for ignored in (
    "['RUSTSEC-2021-0127']",
    '["RUSTSEC-2024-0436", "RUSTSEC-2026-0192"]',
    '["RUSTSEC-2024-0436", "RUSTSEC-2024-0436"]',
    '""', "{}", "false", "[12]",
):
    check(valid.replace("ignore = []", f"ignore = {ignored}"), False, "single approved maintenance exception")
check(valid.replace("ignore = []", ""), False, "single approved maintenance exception")
for package in ("serde_cbor", "paste", "ttf-parser"):
    entry = "{name = '" + package + "'}"
    for replacement in (
        "{}", "{name = 'unrelated'}",
        "{name = '" + package + "', version = '<1'}",
        "{name = '" + package + "', wrappers = ['example']}",
    ):
        check(valid.replace(entry, replacement), False, "unconditionally prohibit " + package)
for invalid in ("", "[advisories", "advisories = []", "bans = []", valid + "[bans]\n"):
    check(invalid, False)
# An aliased or transitive manifest key cannot hide the locked package identity.
for package in ("serde_cbor", "paste", "ttf-parser"):
    with tempfile.TemporaryDirectory(prefix="rustsec-policy-test-", dir=Path.cwd()) as directory:
        (Path(directory) / "Cargo.lock").write_text(
            f'version = 4\n[[package]]\nname = "{package}"\nversion = "1.0.0"\n'
        )
        check(valid, False, "retired package(s) still locked: " + package)
print("check-rustsec-ignore-baseline.test: OK")
PYTHON
