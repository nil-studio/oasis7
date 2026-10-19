#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
python3 - "$ROOT_DIR" <<'PY'
from pathlib import Path
import os
import re
import subprocess
import sys
import tempfile
import textwrap

root = Path(sys.argv[1])
workflow = (root / ".github/workflows/release-packages.yml").read_text()
testnet = (root / ".github/workflows/testnet-packages.yml").read_text()
rust = (root / ".github/workflows/rust.yml").read_text()
installer = (root / "scripts/install-ci-trunk.sh").read_text()


def job(source: str, name: str) -> str:
    match = re.search(
        rf"^  {re.escape(name)}:\n(?P<body>.*?)(?=^  [A-Za-z0-9_-]+:\n|\Z)",
        source.split("\njobs:\n", 1)[1],
        re.MULTILINE | re.DOTALL,
    )
    assert match, f"missing workflow job: {name}"
    return match.group("body")


def step(body: str, name: str) -> str:
    matches = re.findall(
        rf"^      - name: {re.escape(name)}\n(.*?)(?=^      - |\Z)",
        body,
        re.MULTILINE | re.DOTALL,
    )
    assert len(matches) == 1, f"expected exactly one workflow step: {name}"
    return matches[0]


def field(body: str, name: str, indent: int = 8) -> str:
    matches = re.findall(rf"^{' ' * indent}{re.escape(name)}: (.+)$", body, re.MULTILINE)
    assert len(matches) == 1, f"expected exactly one {name} field"
    return matches[0]


def run(body: str) -> str:
    value = field(body, "run")
    if value == "|":
        return textwrap.dedent(body.split("        run: |\n", 1)[1])
    return value


versions = re.findall(r"^  TRUNK_VERSION: ([0-9]+\.[0-9]+\.[0-9]+)$", workflow, re.MULTILINE)
assert len(versions) == 1, "release workflow must pin exactly one TRUNK_VERSION"
version = versions[0]
assert re.findall(r"^  TRUNK_VERSION: (.+)$", rust, re.MULTILINE) == versions
assert re.findall(r"^version=(.+)$", installer, re.MULTILINE) == versions
assert re.findall(r"^  TRUNK_VERSION: (.+)$", testnet, re.MULTILINE) == versions

web = job(workflow, "build-web-dist")
writer = job(rust, "full-regression")
candidate = job(rust, "candidate-full-regression")
preflight = step(web, "Verify release Trunk installer")
cache = step(web, "Restore pinned Trunk release archive")
install = step(web, "Install verified pinned Trunk")
verify = step(web, "Verify pinned trunk")
writer_cache = step(writer, "Cache pinned Trunk release archive")
candidate_cache = step(candidate, "Cache pinned Trunk release archive")

expected_action = "actions/cache/restore@55cc8345863c7cc4c66a329aec7e433d2d1c52a9 # v6.1.0"
assert field(cache, "uses") == expected_action == field(candidate_cache, "uses")
expected_path = "${{ runner.temp }}/ci-tools/trunk-x86_64-unknown-linux-gnu.tar.gz"
expected_key = (
    "trunk-archive-${{ runner.os }}-${{ runner.arch }}-${{ env.TRUNK_VERSION }}-"
    "${{ hashFiles('scripts/install-ci-trunk.sh') }}"
)
for body in (cache, writer_cache, candidate_cache):
    assert field(body, "path", 10) == expected_path
    assert field(body, "key", 10) == expected_key
    assert "restore-keys:" not in body, "a different installer identity must stay cold"
assert "cache-mode: write" in writer
assert "github.ref == 'refs/heads/main'" in writer
assert "actions/cache@" not in web and "actions/cache/save@" not in web
assert "release-packages-trunk-" not in workflow and "~/.cargo/bin/trunk" not in workflow
assert not re.search(r"\bcargo\s+install\s+trunk\b", workflow)

# A cache hit must still call the verified installer; failures must stop the build.
for body in (preflight, cache, install, verify):
    assert not re.search(r"^        (?:if|continue-on-error):", body, re.MULTILINE)
for body in (preflight, install, verify):
    assert field(body, "shell") == "bash"
expected_install = (
    'bash scripts/install-ci-trunk.sh "$RUNNER_TEMP/ci-tools/trunk-x86_64-unknown-linux-gnu.tar.gz" '
    '"$RUNNER_TEMP/ci-tools/bin"'
)
assert run(install) == expected_install
assert 'trunk --version | grep -Fqx "trunk ${TRUNK_VERSION}"' in run(verify)
source = step(web, "Verify planned source HEAD")
assert 'EXPECTED_SHA: ${{ needs.plan.outputs.checkoutSha }}' in source
assert run(source) == 'test "$(git rev-parse HEAD)" = "$EXPECTED_SHA"'
assert 'ref: ${{ needs.plan.outputs.checkoutSha }}' in web
assert "persist-credentials: false" in web
order = (
    "actions/checkout@", "Verify planned source HEAD", "Verify release Trunk installer",
    "Restore pinned Trunk release archive", "Install verified pinned Trunk",
    "Verify pinned trunk", "Build web dists",
)
assert [web.index(name) for name in order] == sorted(web.index(name) for name in order)
assert "trunk build --release" in step(web, "Build web dists")

testnet_web = job(testnet, "build-web-dist")
testnet_preflight = step(testnet_web, "Verify planned Trunk installer")
testnet_install = step(testnet_web, "Install verified pinned Trunk")
testnet_cache = step(testnet_web, "Restore pinned Trunk release archive")
assert field(testnet_cache, "uses") == expected_action
assert field(testnet_cache, "path", 10) == expected_path
assert field(testnet_cache, "key", 10) == expected_key
assert "restore-keys:" not in testnet_cache
assert run(testnet_install) == expected_install
assert "actions/cache@" not in testnet_web and "actions/cache/save@" not in testnet_web
assert "save-if: false" in testnet_web
assert "shared-key: release-packages-web-wasm-trusted-v2" in testnet_web
assert "shared-key: testnet-packages-web-wasm-trusted-v2" not in testnet_web
assert not re.search(r"\bcargo\s+install\s+trunk\b", testnet)
assert [testnet_web.index(name) for name in (
    "Verify planned source HEAD", "Verify planned Trunk installer",
    "Restore pinned Trunk release archive", "Install verified pinned Trunk", "Build web dists"
)] == sorted(testnet_web.index(name) for name in (
    "Verify planned source HEAD", "Verify planned Trunk installer",
    "Restore pinned Trunk release archive", "Install verified pinned Trunk", "Build web dists"
))
for body in (testnet_preflight, testnet_cache, testnet_install, step(testnet_web, "Verify pinned trunk")):
    assert not re.search(r"^        (?:if|continue-on-error):", body, re.MULTILINE)

# Exercise the actual workflow shell with a local installer spy, without downloads.
with tempfile.TemporaryDirectory(prefix="release-trunk-contract-") as directory:
    fixture = Path(directory)
    scripts = fixture / "scripts"
    scripts.mkdir()
    helper = scripts / "install-ci-trunk.sh"
    calls = fixture / "calls"
    env = dict(os.environ, RUNNER_TEMP=str(fixture / "runner temp"), TEST_INSTALL_CALLS=str(calls))

    def invoke(body: str) -> subprocess.CompletedProcess:
        return subprocess.run(
            ["bash", "--noprofile", "--norc", "-e", "-o", "pipefail", "-c", run(body)],
            cwd=fixture, env=env, text=True, capture_output=True,
        )

    missing = invoke(preflight)
    assert missing.returncode != 0 and "planned release source" in missing.stderr
    assert invoke(testnet_preflight).returncode != 0
    helper.touch()
    assert invoke(preflight).returncode != 0, "an empty helper must fail before cache restore"
    assert invoke(testnet_preflight).returncode != 0
    helper.unlink()
    target = fixture / "outside.sh"
    target.write_text("exit 0\n")
    helper.symlink_to(target)
    assert invoke(preflight).returncode != 0, "source symlinks must fail before cache restore"
    assert invoke(testnet_preflight).returncode != 0
    helper.unlink()
    helper.write_text(
        '#!/usr/bin/env bash\nset -euo pipefail\n'
        'printf "%s\\n" "$@" > "$TEST_INSTALL_CALLS"\n'
        'exit "${TEST_INSTALL_EXIT:-0}"\n'
    )
    assert invoke(preflight).returncode == 0
    assert invoke(testnet_preflight).returncode == 0
    assert invoke(install).returncode == 0
    assert invoke(testnet_install).returncode == 0
    assert calls.read_text().splitlines() == [
        str(fixture / "runner temp/ci-tools/trunk-x86_64-unknown-linux-gnu.tar.gz"),
        str(fixture / "runner temp/ci-tools/bin"),
    ]
    env["TEST_INSTALL_EXIT"] = "42"
    assert invoke(install).returncode == 42, "installer failure must propagate"
    assert invoke(testnet_install).returncode == 42, "Testnet installer failure must propagate"

print(f"ok: Release and Testnet restore the shared archive and always verify Trunk {version}; source preflight and failure paths pass")
PY
