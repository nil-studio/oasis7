#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

python3 - "$ROOT_DIR" <<'PY'
from pathlib import Path
import os
import re
import shlex
import subprocess
import sys
import tempfile
import textwrap

root = Path(sys.argv[1])
workflow_paths = [
    root / ".github/workflows/testnet-packages.yml",
    root / ".github/workflows/mainnet-packages.yml",
    root / ".github/workflows/release-packages.yml",
]


def package_native_body(workflow: str, path: Path) -> str:
    match = re.search(
        r"^  package-native:\n(?P<body>.*?)(?=^  [A-Za-z0-9_-]+:\n|\Z)",
        workflow,
        re.MULTILINE | re.DOTALL,
    )
    assert match, f"{path.name} must define package-native"
    return match.group("body")


def cache_steps(workflow: str, path: Path) -> list[str]:
    body = package_native_body(workflow, path)
    steps = re.findall(
        r"^      - uses: Swatinem/rust-cache@6323deb102c322ba6fcbdcafc7e3dddab59af2b6 # v2\n(?P<body>.*?)(?=^      - |\Z)",
        body,
        re.MULTILINE | re.DOTALL,
    )
    assert len(steps) == 2, (
        f"{path.name} package-native must keep separate Linux and non-Linux cache steps"
    )
    return steps


def linux_cache_guard(workflow: str, path: Path) -> str:
    body = package_native_body(workflow, path)
    cache_marker = "      - uses: Swatinem/rust-cache@6323deb102c322ba6fcbdcafc7e3dddab59af2b6 # v2\n        if: runner.os == 'Linux'"
    cache_offset = body.find(cache_marker)
    assert cache_offset >= 0, f"{path.name} must define a Linux package cache step"
    pre_cache = body[:cache_offset]
    guards = re.findall(
        r"^      - name: Compute Linux package cache save eligibility\n"
        r"(?P<body>.*?)(?=^      - |\Z)",
        pre_cache,
        re.MULTILINE | re.DOTALL,
    )
    assert len(guards) == 1, (
        f"{path.name} must compute Linux cache save eligibility after checkout"
    )
    return guards[0]


testnet = workflow_paths[0].read_text(encoding="utf-8")
assert "  package-native-trusted:\n    cache-mode: write\n" in testnet, "missing trusted native cache writer"
assert "github.workflow_sha == github.sha" in testnet, "writer must bind trusted workflow revision"
assert "needs.plan.outputs.checkoutSha == github.sha" in testnet, "writer must bind source to main event"
assert "steps: &testnet-native-steps" in testnet and "steps: *testnet-native-steps" in testnet

# Compilation has no Web dependency; assembly joins both successful producers.
for lane in ('package-native', 'package-native-trusted'):
    body = re.search(rf'^  {lane}:\n(.*?)(?=^  [\w-]+:\n|\Z)', testnet, re.M | re.S)[1]
    assert '      - build-web-dist' not in body
assembly = testnet.split('  package-installers:\n', 1)[1]
for dependency in ('plan', 'build-web-dist', 'package-native', 'package-native-trusted'):
    assert '      - ' + dependency + '\n' in assembly
assert '!cancelled()' in assembly and "needs.build-web-dist.result == 'success'" in assembly
assert '--native-binaries' in assembly and 'Swatinem/rust-cache@' not in assembly
installer = assembly.split('      - name: Package native installer\n', 1)[1].split('      - name:', 1)[0]
assert '--out-dir output/testnet-packages/assets' in installer
assert '--ops-out-dir' not in installer

assert '--native-only' in testnet and 'testnet-native-stage-${{ matrix.platform }}' in testnet

# Execute the actual lane predicates, including untrusted branch/source revisions.
def condition(name):
    body = re.search(rf"^  {name}:\n(.*?)(?=^  [\w-]+:\n|\Z)", testnet, re.M | re.S)[1]
    predicate = re.search(r"^    if: >-\n(.*?)(?=^    \S)", body, re.M | re.S)[1]
    return " ".join(predicate.split())

# The real join must run after one selected native lane, but never on failure,
# cancellation, unknown results, or both producers selected.
import itertools
join = condition('package-installers')
for web, candidate, trusted in itertools.product(('success', 'skipped', 'failure', 'cancelled', 'unknown'), repeat=3):
    expression = join.replace('!cancelled()', 'True')
    values = {'needs.plan.result': 'success', 'needs.build-web-dist.result': web,
              'needs.package-native.result': candidate, 'needs.package-native-trusted.result': trusted}
    for key in sorted(values, key=len, reverse=True):
        expression = expression.replace(key, repr(values[key]))
    actual = eval(expression.replace('&&', ' and ').replace('||', ' or '), {'__builtins__': {}})
    expected = web == 'success' and (candidate, trusted) in [('success', 'skipped'), ('skipped', 'success')]
    assert actual == expected, (web, candidate, trusted)

trusted_condition = condition("package-native-trusted")
candidate_condition = condition("package-native")
for ref, event, source, workflow, scope, profile, expected in (
    ("refs/heads/main", "workflow_dispatch", "event", "event", "linux_only", "packaging", True),
    ("refs/heads/candidate", "workflow_dispatch", "event", "event", "linux_only", "packaging", False),
    ("refs/tags/v1", "workflow_dispatch", "event", "event", "linux_only", "packaging", False),
    ("refs/heads/main", "pull_request", "event", "event", "linux_only", "packaging", False),
    ("refs/heads/main", "workflow_dispatch", "older-source", "event", "linux_only", "packaging", False),
    ("refs/heads/main", "workflow_dispatch", "event", "other-workflow", "linux_only", "packaging", False),
    ("refs/heads/main", "workflow_dispatch", "event", "event", "all_existing", "packaging", False),
    ("refs/heads/main", "workflow_dispatch", "event", "event", "linux_only", "dev", False),
    ("refs/heads/main", "workflow_dispatch", "event", "event", "linux_only", "packaging;false", False),
):
    def selected(predicate):
        values = {"github.ref": ref, "github.event_name": event,
                  "needs.plan.outputs.checkoutSha": source,
                  "github.workflow_sha": workflow, "github.sha": "event",
                  "inputs.package_scope": scope, "inputs.build_profile": profile}
        for key in sorted(values, key=len, reverse=True):
            predicate = predicate.replace(key, repr(values[key]))
        return eval(predicate.replace("&&", " and ").replace("||", " or "), {"__builtins__": {}})
    assert selected(trusted_condition) == expected, (ref, event, source, workflow)
    assert selected(candidate_condition) != expected, "native lanes must be mutually exclusive and exhaustive"

linux_keys: list[str] = []
expected_non_linux_keys = {
    "testnet-packages-${{ runner.os }}-${{ matrix.target_triple }}-trusted-v2",
    "release-packages-package-native-${{ runner.os }}-${{ matrix.target_triple }}-trusted-v2",
}
observed_non_linux_keys: set[str] = set()

for path in workflow_paths:
    workflow = path.read_text(encoding="utf-8")
    guard = linux_cache_guard(workflow, path)
    assert "id: linux-package-cache-write" in guard
    assert 'live_head="$(git rev-parse HEAD)"' in guard
    assert (
        '[[ "${GITHUB_REF}" == "refs/heads/main" && '
        '"${live_head}" == "${GITHUB_SHA}" ]]'
    ) in guard, (
        f"{path.name} must reject main-dispatch builds checked out at a tag/SHA"
    )
    assert 'echo "save=${cache_write}" >> "${GITHUB_OUTPUT}"' in guard
    steps = cache_steps(workflow, path)
    linux = [step for step in steps if "if: runner.os == 'Linux'" in step]
    non_linux = [step for step in steps if "if: runner.os != 'Linux'" in step]
    assert len(linux) == 1, f"{path.name} must gate its shared cache to Linux"
    assert len(non_linux) == 1, f"{path.name} must preserve a non-Linux cache step"

    linux_key = re.search(r"^          shared-key: (?P<key>.+)$", linux[0], re.MULTILINE)
    assert linux_key, f"{path.name} Linux cache must declare shared-key"
    linux_keys.append(linux_key.group("key"))
    assert "${{ runner.os }}" in linux_key.group("key")
    assert "${{ matrix.target_triple }}" in linux_key.group("key")
    assert "${{ inputs.build_profile }}" in linux_key.group("key") or "packaging" in linux_key.group("key")
    assert "github.ref_name" not in linux_key.group("key")
    assert "GITHUB_REF_NAME" not in linux_key.group("key")
    assert "cache-on-failure: false" in linux[0], f"{path.name} Linux cache must save successful jobs only"
    expected_save = {
        "release-packages.yml": "save-if: ${{ steps.linux-package-cache-write.outputs.save == 'true' }}",
        "testnet-packages.yml": "save-if: ${{ github.job == 'package-native-trusted' && steps.linux-package-cache-write.outputs.save == 'true' }}",
        "mainnet-packages.yml": "save-if: false",
    }
    assert expected_save[path.name] in linux[0], (
        f"{path.name} Linux cache must only save when checkout matches the trusted branch"
    )

    non_linux_key = re.search(r"^          shared-key: (?P<key>.+)$", non_linux[0], re.MULTILINE)
    assert non_linux_key, f"{path.name} non-Linux cache must declare shared-key"
    observed_non_linux_keys.add(non_linux_key.group("key"))
    assert "cache-on-failure: true" in non_linux[0], (
        f"{path.name} non-Linux cache behavior must remain unchanged"
    )
    if path.name != "release-packages.yml":
        assert "save-if: false" in non_linux[0]

    # The job predicate is not sufficient: exercise the post-checkout shell guard.
    shell = textwrap.dedent(guard.split("        run: |\n", 1)[1])
    with tempfile.TemporaryDirectory() as directory:
        fixture = Path(directory)
        git = fixture / "git"
        git.write_text('#!/bin/sh\nprintf "%s\\n" "$TEST_HEAD"\n')
        git.chmod(0o755)
        for ref, head, expected in (("refs/heads/main", "event", "true"),
                                    ("refs/heads/main", "candidate", "false"),
                                    ("refs/heads/other", "event", "false")):
            output = fixture / "output"
            env = dict(os.environ, PATH=directory + os.pathsep + os.environ["PATH"],
                       GITHUB_REF=ref, GITHUB_SHA="event", TEST_HEAD=head, GITHUB_OUTPUT=str(output))
            subprocess.run(["bash", "-e", "-o", "pipefail", "-c", shell], env=env, check=True)
            assert output.read_text() == "save=" + expected + "\n"
            output.unlink()

normalized_linux_keys = {
    key.replace("${{ inputs.build_profile }}", "packaging") for key in linux_keys
}
assert len(normalized_linux_keys) == 1, (
    "Testnet packaging profile=packaging, Mainnet, and Release Linux caches must share one key"
)
assert observed_non_linux_keys == expected_non_linux_keys, (
    "Mainnet packaging must consume Release's OS/target cache; Testnet stays profile-isolated"
)


def job_body(workflow: str, name: str) -> str:
    match = re.search(rf"^  {re.escape(name)}:\n(.*?)(?=^  [\w-]+:\n|\Z)",
                      workflow.split("\njobs:\n", 1)[1], re.M | re.S)
    assert match, f"missing workflow job: {name}"
    return match[1]


def values(body: str, indent: int) -> dict[str, str]:
    return {name: value.strip().strip("\"'") for name, value in
            re.findall(rf"^{' ' * indent}([\w-]+): ([^\n]+)$", body, re.M)}


def declared_env(body: str, indent: int) -> dict[str, str]:
    match = re.search(rf"^{' ' * indent}env:\n(.*?)(?=^{' ' * indent}\S|\Z)",
                      body, re.M | re.S)
    return values(match[1], indent + 2) if match else {}


def cache_identity(workflow: str, name: str, non_linux: bool = False) -> dict:
    """Compare inputs used by the pinned action, including its implicit defaults.

    This establishes configuration compatibility for the same source and runner
    image. Real ref reachability, installed runner image tools and cache contents
    still require hosted observations.
    """
    body = job_body(workflow, name)
    matches = list(re.finditer(r"^      - uses: (Swatinem/rust-cache@[^\n]+)\n(.*?)(?=^      - |\Z)",
                               body, re.M | re.S))
    if non_linux:
        matches = [match for match in matches if "if: runner.os != 'Linux'" in match[2]]
    assert len(matches) == 1, (name, "expected one selected Rust cache")
    cache = matches[0]
    inputs = values(cache[2], 10)
    env = declared_env(workflow.split("\njobs:\n", 1)[0], 0)
    env.update(declared_env(body.split("    steps:", 1)[0], 4))
    env.update(declared_env(cache[2], 8))
    prefixes = {"CARGO", "CC", "CFLAGS", "CXX", "CMAKE", "RUST"}
    prefixes.update(inputs.get("env-vars", "").split())
    # rust-cache considers all installed Rust versions, not just the default.
    # Only setup before this exact cache step can contribute to its identity.
    toolchains, default = set(), None
    for command in re.findall(r"^\s+rustup (?:toolchain install|default) [^\n]+$", body[:cache.start()], re.M):
        expanded = re.sub(r"\$\{(\w+)\}|\$(\w+)",
                          lambda match: env.get(match[1] or match[2], match[0]), command)
        args = shlex.split(expanded)
        if args[1:3] == ["toolchain", "install"]:
            toolchains.add(args[3])
        elif args[1] == "default":
            default = args[2]
    assert toolchains and default, f"{name}: install a pinned toolchain before restoring cache"
    return {
        "action": cache[1], "runner": values(body, 4)["runs-on"],
        "prefix": inputs.get("prefix-key", "v0-rust"),
        "shared-key": inputs.get("shared-key"),
        "environment-hash": inputs.get("add-rust-environment-hash-key", "true"),
        "environment": {key: value for key, value in env.items()
                        if value and any(key.startswith(prefix) for prefix in prefixes)},
        "toolchains": sorted(toolchains), "default-toolchain": default,
        "workspaces": inputs.get("workspaces", ". -> target"),
        "cache-targets": inputs.get("cache-targets", "true"),
        "cache-bin": inputs.get("cache-bin", "true"),
        "cache-directories": inputs.get("cache-directories", ""),
        "cache-provider": inputs.get("cache-provider", "github"),
    }


release = workflow_paths[2].read_text()
mainnet = workflow_paths[1].read_text()
rust = (root / ".github/workflows/rust.yml").read_text()
bridge = (root / ".github/workflows/newapi-bridge-service-package.yml").read_text()
assert "\ncache-mode: read\n" in mainnet
for name in ("release_gate_runtime_core", "release_gate_runtime_support", "release_gate_runtime_sync",
             "release_gate_soak", "release_gate_web", "build-web-dist"):
    expected = cache_identity(release, name)
    actual = cache_identity(mainnet, name)
    assert actual == expected, (name, "Mainnet cache identity differs from Release", actual, expected)
    assert "save-if: false" in job_body(mainnet, name)
    assert "cache-mode: write" not in job_body(mainnet, name)
    # Existing shard producers remain separate: merging their keys would let the
    # first finisher freeze an incomplete set of compiled dependencies.
    if name.startswith("release_gate_runtime_") or name == "release_gate_soak":
        assert expected["shared-key"].startswith("release-packages-release-gate-")
        assert "save-if: false" not in job_body(release, name)
assert cache_identity(mainnet, "package-native", True) == cache_identity(release, "package-native", True)
assert cache_identity(rust, "newapi-bridge-package") == cache_identity(bridge, "package-linux-x86-64"), \
    "NewAPI entrypoints must agree on Cargo environment and toolchain/path defaults, not just shared-key"

print("ok: package readers reuse reachable compatible writers without merging shard targets or profile boundaries")

PY
