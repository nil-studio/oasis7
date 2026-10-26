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
        '"${live_head}" == "${GITHUB_SHA}"'
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
        "release-packages.yml": "save-if: ${{ job.status == 'success' && steps.dependency-cache-tools.outputs.guard_ready == 'true' && steps.linux-package-cache-write.outputs.save == 'true' && needs.plan.outputs.checkoutSha == github.sha }}",
        "testnet-packages.yml": "save-if: ${{ job.status == 'success' && github.job == 'package-native-trusted' && steps.linux-package-cache-write.outputs.save == 'true' }}",
        "mainnet-packages.yml": "save-if: false",
    }
    assert expected_save[path.name] in linux[0], (
        f"{path.name} Linux cache must only save when checkout matches the trusted branch"
    )

    non_linux_key = re.search(r"^          shared-key: (?P<key>.+)$", non_linux[0], re.MULTILINE)
    assert non_linux_key, f"{path.name} non-Linux cache must declare shared-key"
    observed_non_linux_keys.add(non_linux_key.group("key"))
    expected_failure = "false" if path.name == "release-packages.yml" else "true"
    assert "cache-on-failure: " + expected_failure in non_linux[0]
    if path.name != "release-packages.yml":
        assert "save-if: false" in non_linux[0]

    # The job predicate is not sufficient: exercise the post-checkout shell guard.
    shell = textwrap.dedent(guard.split("        run: |\n", 1)[1])
    with tempfile.TemporaryDirectory() as directory:
        fixture = Path(directory)
        git = fixture / "git"
        git.write_text('#!/bin/sh\nprintf "%s\\n" "$TEST_HEAD"\n')
        git.chmod(0o755)
        contexts = [("refs/heads/main", "event", "event", "workflow_dispatch", "true"),
                    ("refs/heads/main", "candidate", "event", "workflow_dispatch", "false"),
                    ("refs/heads/other", "event", "event", "workflow_dispatch", "false")]
        if path.name == "release-packages.yml":
            contexts += [("refs/heads/main", "event", "stale-workflow", "workflow_dispatch", "false"),
                         ("refs/heads/main", "event", "event", "push", "false")]
        for ref, head, workflow_sha, event, expected in contexts:
            output = fixture / "output"
            env = dict(os.environ, PATH=directory + os.pathsep + os.environ["PATH"],
                       GITHUB_REF=ref, GITHUB_SHA="event", TEST_HEAD=head, GITHUB_OUTPUT=str(output),
                       GITHUB_WORKFLOW_SHA=workflow_sha, GITHUB_EVENT_NAME=event)
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
ecs = (root / ".github/workflows/ecs-packages.yml").read_text()
assert "\ncache-mode: read\n" in mainnet
native_gates = ("release_gate_runtime_core", "release_gate_runtime_support",
                "release_gate_runtime_sync", "release_gate_soak")
for name in native_gates:
    assert cache_identity(release, name)["shared-key"] == "ci-shared-native-trusted-v3", name
for name in ("release_gate_runtime_core", "release_gate_runtime_support", "release_gate_runtime_sync",
             "release_gate_soak", "release_gate_web", "build-web-dist"):
    expected = cache_identity(release, name)
    actual = cache_identity(mainnet, name)
    assert actual == expected, (name, "Mainnet cache identity differs from Release", actual, expected)
    assert "save-if: false" in job_body(mainnet, name)
    assert "cache-mode: write" not in job_body(mainnet, name)
    if name in native_gates:
        assert expected == cache_identity(rust, "full-regression"), (name, "native writer identity differs")
        assert "save-if: false" in job_body(release, name)
        assert "    cache-mode: read\n" in job_body(release, name)
assert cache_identity(mainnet, "package-native", True) == cache_identity(release, "package-native", True)
assert cache_identity(rust, "newapi-bridge-package") == cache_identity(bridge, "package-linux-x86-64"), \
    "NewAPI entrypoints must agree on Cargo environment and toolchain/path defaults, not just shared-key"

web_identity = cache_identity(release, "build-web-dist")
assert web_identity["shared-key"] == "release-packages-web-wasm-trusted-v3"
for workflow, names in ((release, ("release_gate_web",)), (mainnet, ("release_gate_web", "build-web-dist")),
                        (testnet, ("build-web-dist",)), (ecs, ("web",))):
    for name in names:
        assert cache_identity(workflow, name) == web_identity, (name, "Web cache inputs differ")
        assert "save-if: false" in job_body(workflow, name)
assert "    cache-mode: read\n" in job_body(release, "release_gate_web")
web_writer = job_body(release, "build-web-dist")
assert "cache-on-failure: false" in web_writer
save_expression = re.search(r"^          save-if: \$\{\{ (.*) \}\}$", web_writer, re.M)[1]
for ref, source, workflow_sha, expected in (
    ("refs/heads/main", "event", "event", True),
    ("refs/heads/main", "older-source", "event", False),
    ("refs/heads/main", "event", "other-workflow", False),
    ("refs/heads/candidate", "event", "event", False),
    ("refs/tags/v1.0.0", "event", "event", False),
):
    expression = save_expression
    context = {"github.ref": ref, "needs.plan.outputs.checkoutSha": source,
               "github.workflow_sha": workflow_sha, "github.sha": "event",
               "github.event_name": "workflow_dispatch", "job.status": "success",
               "steps.dependency-cache-tools.outputs.guard_ready": "true"}
    for key in sorted(context, key=len, reverse=True):
        expression = expression.replace(key, repr(context[key]))
    assert eval(expression.replace("&&", " and "), {"__builtins__": {}}) == expected

# Maintenance is optional to packaging and never an artifact/publish gate.
# Its bounded recovery runs under the same mutex before the first writer and
# after both writers' post actions. Tags and older planned sources stay read-only.
def release_condition(name):
    body = job_body(release, name)
    match = re.search(r"^    if: >-\n(.*?)(?=^    \S)", body, re.M | re.S)
    assert match, (name, "explicit status propagation is required")
    return " ".join(match[1].split())

def evaluate(predicate, context, cancelled=False):
    predicate = predicate.replace("always()", "True").replace("!cancelled()", str(not cancelled))
    for key in sorted(context, key=len, reverse=True):
        predicate = predicate.replace(key, repr(context[key]))
    return eval(predicate.replace("&&", " and ").replace("||", " or "), {"__builtins__": {}})

base_context = {"github.ref": "refs/heads/main", "github.event_name": "workflow_dispatch",
                "github.workflow_sha": "event", "github.sha": "event",
                "needs.plan.outputs.checkoutSha": "event", "needs.plan.result": "success",
                "needs.build-web-dist.result": "success", "needs.package-native.result": "success",
                "needs.release_gate.result": "success"}
for name in ("cache-bootstrap-maintenance", "cache-maintenance"):
    body = job_body(release, name)
    assert "    cache-mode: none\n" in body
    assert "      contents: read\n      actions: write\n" in body
    assert "      group: ci-cache-maintenance-main\n      cancel-in-progress: false\n" in body
    assert "run: python3 scripts/ci-cache-maintenance.py --recover --apply" in body
    assert "CACHE_MAINTENANCE_SOURCE_SHA: ${{ needs.plan.outputs.checkoutSha }}" in body
    assert "ref: ${{ github.sha }}" in body and "persist-credentials: false" in body
    assert "--current" not in body and "--bootstrap" not in body
    predicate = release_condition(name)
    assert evaluate(predicate, base_context)
    assert not evaluate(predicate, base_context, cancelled=True)
    for change in ({"github.ref": "refs/tags/v1"}, {"github.ref": "refs/heads/candidate"},
                   {"github.event_name": "push"}, {"github.workflow_sha": "stale"},
                   {"needs.plan.outputs.checkoutSha": "old-tag"}, {"needs.plan.result": "failure"}):
        assert not evaluate(predicate, base_context | change), (name, change)
assert "    needs: plan\n" in job_body(release, "cache-bootstrap-maintenance")
assert "    needs: [plan, build-web-dist, package-native]\n" in job_body(release, "cache-maintenance")
for web, native in itertools.product(("success", "failure", "skipped", "cancelled"), repeat=2):
    actual = evaluate(release_condition("cache-maintenance"), base_context |
        {"needs.build-web-dist.result": web, "needs.package-native.result": native})
    assert actual == ("success" in (web, native)), (web, native)
assert "    needs: [plan, release_gate, cache-bootstrap-maintenance]\n" in web_writer
for name, required in (("build-web-dist", "release_gate"),
                       ("package-native", "build-web-dist"), ("publish-release", "package-native")):
    body = job_body(release, name)
    assert "cache-maintenance" not in body
    for plan, producer, maintenance in itertools.product(("success", "failure", "skipped"), repeat=3):
        context = base_context | {"needs.plan.result": plan, "needs." + required + ".result": producer,
                                  "needs.cache-bootstrap-maintenance.result": maintenance}
        expected = plan == producer == "success"
        assert evaluate(release_condition(name), context) == expected, (name, context)
        assert not evaluate(release_condition(name), context, cancelled=True)

# All Release writers use the cross-platform final-archive hook. Admission
# failure can only skip optional caching, while original compile gates survive.
assert "NODE_OPTIONS:" not in release.split("\njobs:\n", 1)[0]
for name in ("build-web-dist", "package-native"):
    body = job_body(release, name)
    assert "      contents: read\n      actions: read\n" in body
    assert "actions: write" not in body
    assert "python3 scripts/ci-rust-cache-admission.py install-release" in body
    for step in re.findall(r"^      - (.*?)(?=^      - |\Z)", body, re.M | re.S):
        if "Swatinem/rust-cache@" not in step:
            if "continue-on-error: true" in step:
                assert step.startswith("name: Install Release cache admission")
            continue
        for required in ("continue-on-error: true", "cache-provider: github", "cache-on-failure: false",
                         "NODE_OPTIONS: ${{ steps.dependency-cache-tools.outputs.node_options }}",
                         "OASIS7_DEPENDENCY_CACHE_PYTHON: ${{ steps.dependency-cache-tools.outputs.python }}",
                         "OASIS7_DEPENDENCY_CACHE_SOURCE_SHA: ${{ needs.plan.outputs.checkoutSha }}",
                         "OASIS7_DEPENDENCY_CACHE_BUDGET_BYTES: ${{ env.CI_CACHE_REPOSITORY_BUDGET_BYTES }}"):
            assert required in step, (name, required)
        save = re.search(r"^          save-if: \$\{\{ (.*) \}\}$", step, re.M)[1]
        context = base_context | {"job.status": "success",
                  "steps.dependency-cache-tools.outputs.guard_ready": "true",
                  "steps.linux-package-cache-write.outputs.save": "true"}
        assert evaluate(save, context)
        for change in ({"job.status": "failure"}, {"job.status": "cancelled"},
                       {"steps.dependency-cache-tools.outputs.guard_ready": ""},
                       {"steps.dependency-cache-tools.outputs.guard_ready": "false"},
                       {"needs.plan.outputs.checkoutSha": "old-tag"}):
            assert not evaluate(save, context | change), (name, change)
testnet_linux = next(step for step in cache_steps(testnet, workflow_paths[0]) if "if: runner.os == 'Linux'" in step)
assert "steps.dependency-cache-tools.outcome == 'success'" in testnet_linux
assert "PATH: ${{ steps.dependency-cache-tools.outputs.path }}" in testnet_linux
assert "OASIS7_DEPENDENCY_CACHE_FAMILY: testnet-linux" in testnet_linux
assert "cache-provider: github" in testnet_linux

# An older planned tag may not contain the normalizer. Both the normalizer and
# its new cache step must then be skipped while the original cold build remains.
normalizer = "scripts/ci-rust-cache-toolchains.py"
for workflow, names in ((release, (*native_gates, "release_gate_web", "build-web-dist")),
                        (mainnet, (*native_gates, "release_gate_web", "build-web-dist")),
                        (testnet, ("build-web-dist",)), (ecs, ("web", "native"))):
    for name in names:
        body = job_body(workflow, name)
        steps = re.findall(r"^      - (.*?)(?=^      - |\Z)", body, re.M | re.S)
        guarded = [step for step in steps if normalizer in step or "Swatinem/rust-cache@" in step]
        assert len(guarded) == 2, (name, "one normalizer and one cache expected")
        for step in guarded:
            assert f"if: hashFiles('{normalizer}') != ''" in step, name
        assert body.index(f"python3 {normalizer}") < body.index("Swatinem/rust-cache@"), name
        for command in re.findall(r"^\s+(?:run: )?python3 scripts/ci-rust-cache-toolchains.py (.*)$", body, re.M):
            assert ('"$OASIS7_WASM_TOOLCHAIN"' in command) == (name in native_gates)

# Run the real logging pipelines with failing upstream commands. Merely finding
# the word pipefail would not establish that tee cannot hide a gate failure.
for workflow in (release, mainnet):
    for name in ("release_gate_runtime_core", "release_gate_runtime_support", "release_gate_runtime_sync"):
        body = job_body(workflow, name)
        blocks = re.findall(r"^      - name: ([^\n]+)\n(.*?)(?=^      - |\Z)", body, re.M | re.S)
        gate = next(block for title, block in blocks if title.startswith("Run runtime"))
        assert "shell: bash" in gate
        script = textwrap.dedent(gate.split("        run: |\n", 1)[1])
        with tempfile.TemporaryDirectory() as directory:
            fixture = Path(directory)
            (fixture / "scripts").mkdir()
            filenames = (("sync-m1-builtin-wasm-artifacts.sh", "sync-m4-builtin-wasm-artifacts.sh",
                          "sync-m5-builtin-wasm-artifacts.sh") if name == "release_gate_runtime_sync"
                         else ("ci-tests.sh",))
            for filename in filenames:
                path = fixture / "scripts" / filename
                path.write_text('#!/bin/sh\n[ "${0##*/}" != "$TEST_FAIL_SCRIPT" ] || exit 23\n')
                path.chmod(0o755)
            for filename in filenames:
                result = subprocess.run(["bash", "--noprofile", "--norc", "-e", "-o", "pipefail", "-c", script],
                                        cwd=fixture, env=dict(os.environ, TEST_FAIL_SCRIPT=filename),
                                        capture_output=True, text=True)
                assert result.returncode == 23, (name, filename, result.returncode, result.stdout, result.stderr)
        if name != "release_gate_runtime_sync":
            assert "fetch-depth: 0" in body, "frozen fixture history must be available"
            assert 'scripts/doc-governance-requirements.txt' in body
            assert 'OASIS7_PRODUCT_DOC_BASE=' in body and 'OASIS7_PRODUCT_DOC_HEAD=' in body
            assert "git rev-parse --verify 'HEAD^1'" in body
            assert "git rev-parse --verify 'HEAD^{commit}'" in body

print("ok: compatible cache readers share exclusive writers, old sources stay cold, and gate failures propagate")

PY
