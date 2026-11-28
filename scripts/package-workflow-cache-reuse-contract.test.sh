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
        r"^      - uses: Swatinem/rust-cache@d1527c8f67ef9af357290475d4af4b0fab8f12e0 # v2\n(?P<body>.*?)(?=^      - |\Z)",
        body,
        re.MULTILINE | re.DOTALL,
    )
    assert len(steps) == 2, (
        f"{path.name} package-native must keep separate Linux and non-Linux cache steps"
    )
    return steps


def linux_cache_guard(workflow: str, path: Path) -> str:
    body = package_native_body(workflow, path)
    cache_marker = "      - uses: Swatinem/rust-cache@d1527c8f67ef9af357290475d4af4b0fab8f12e0 # v2\n        if: runner.os == 'Linux'"
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
assert 'full-regression-qualification' not in testnet
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

# The real join must run after plan (which includes full-regression qualification)
# and one selected native lane, never on failure, cancellation, unknown results,
# or both producers selected.
import itertools
join = condition('package-installers')
for plan_res, web, candidate, trusted in itertools.product(('success', 'skipped', 'failure', 'cancelled', 'unknown'), repeat=4):
    expression = join.replace('!cancelled()', 'True')
    values = {'needs.plan.result': plan_res, 'needs.build-web-dist.result': web,
              'needs.package-native.result': candidate, 'needs.package-native-trusted.result': trusted}
    for key in sorted(values, key=len, reverse=True):
        expression = expression.replace(key, repr(values[key]))
    actual = eval(expression.replace('&&', ' and ').replace('||', ' or '), {'__builtins__': {}})
    expected = (plan_res == 'success' and web == 'success'
                and (candidate, trusted) in [('success', 'skipped'), ('skipped', 'success')])
    assert actual == expected, (plan_res, web, candidate, trusted)

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
linux_images: list[str] = []
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
    native = re.search(r"(?ms)^  package-native:\n.*?(?=^  [a-z][a-z0-9_-]+:|\Z)", workflow)[0]
    image = re.search(r"image: \$\{\{ matrix\.os == 'ubuntu-24\.04' && '(ghcr\.io/nil-studio/oasis7/ci-rust-build@sha256:([0-9a-f]{64}))' \|\| '' \}\}", native)
    if image:
        linux_images.append(image[1])
        profile = "${{ inputs.build_profile }}" if path.name == "testnet-packages.yml" else "packaging"
        assert linux_key["key"] == f"oasis7-package-native-linux-x64-{profile}-image-{image[2]}"
        assert "RUST_PACKAGE_NATIVE_IMAGE: " + image[1] in linux[0]
        assert "steps.source-toolchain.outputs.image_toolchain == 'true'" in linux[0]
        assert "cache-provider: github" in linux[0]
        assert "options: --user 1001:1001 --init" in native
    else:
        assert "${{ runner.os }}" in linux_key["key"]
        assert "${{ matrix.target_triple }}" in linux_key["key"]
        assert "${{ inputs.build_profile }}" in linux_key["key"] or "packaging" in linux_key["key"]
    assert "github.ref_name" not in linux_key.group("key")
    assert "GITHUB_REF_NAME" not in linux_key.group("key")
    assert "cache-on-failure: false" in linux[0], f"{path.name} Linux cache must save successful jobs only"
    expected_save = {
        "release-packages.yml": "save-if: ${{ job.status == 'success' && steps.dependency-cache-tools.outputs.guard_ready == 'true' && steps.linux-package-cache-write.outputs.save == 'true' && needs.plan.outputs.checkoutSha == github.sha }}",
        "testnet-packages.yml": "save-if: ${{ job.status == 'success' && github.job == 'package-native-trusted' && steps.linux-package-cache-write.outputs.save == 'true' }}",
        "mainnet-packages.yml": "save-if: false",
    }
    if image and path.name == "testnet-packages.yml":
        expected_save[path.name] = expected_save[path.name].replace(
            "job.status == 'success' &&", "job.status == 'success' && steps.dependency-cache-tools.outputs.guard_ready == 'true' &&")
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
assert not linux_images or len(linux_images) == len(workflow_paths) and len(set(linux_images)) == 1
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


def cache_identity(workflow: str, name: str, non_linux: bool = False, image_web: bool = False) -> dict:
    """Compare inputs used by the pinned action, including its implicit defaults.

    This establishes configuration compatibility for the same source and runner
    image. Real ref reachability, installed runner image tools and cache contents
    still require hosted observations.
    """
    body = job_body(workflow, name)
    matches = list(re.finditer(r"^      - uses: (Swatinem/rust-cache@[^\n]+)\n(.*?)(?=^      - |\Z)",
                               body, re.M | re.S))
    if image_web:
        assert name == "release_gate_web"
        matches = list(re.finditer(
            r"^      - name: Restore the matching Release Web image dependency cache\n"
            r"        if: env[.]RELEASE_WEB_IMAGE_COMPATIBLE == 'true'\n"
            r"        uses: (Swatinem/rust-cache@[^\n]+)\n(.*?)(?=^      - |\Z)", body, re.M | re.S))
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
    image = re.search(r"^    container:\n      image: ghcr\.io/nil-studio/oasis7/ci-rust-build@sha256:([0-9a-f]{64})$",
                      body, re.M)
    if image:
        before_cache = body[:cache.start()]
        if name in ("web", "build-web-dist"):
            prefix = {
                ecs: "ecs-web", testnet: "testnet-web",
                release: "release-web", mainnet: "mainnet-web",
            }[workflow]
            assert inputs.get("shared-key") == prefix + "-ci-rust-build-" + image[1]
            assert inputs.get("save-if") == "false" and inputs.get("cache-on-failure") == "false"
            assert inputs.get("add-rust-environment-hash-key") == "true"
            assert 'cargo +"$RUST_TOOLCHAIN" --version' in before_cache
            assert 'rustup target list --installed --toolchain "$RUST_TOOLCHAIN" | grep -Fx wasm32-unknown-unknown' in before_cache
            assert 'python3 scripts/ci-rust-cache-toolchains.py "$RUST_TOOLCHAIN" "$OASIS7_WASM_TOOLCHAIN"' in before_cache
            nightly = env.get("OASIS7_WASM_TOOLCHAIN", "")
            assert re.fullmatch(r"nightly-\d{4}-\d{2}-\d{2}", nightly)
            toolchains.add(nightly)
        else:
            assert name in ("newapi-bridge-package", "package-linux-x86-64"), name
            assert inputs.get("shared-key") == "newapi-bridge-service-linux-x86-64-ci-rust-build-" + image[1]
            assert inputs.get("save-if") == "${{ github.ref == 'refs/heads/main' && github.workflow_sha == github.sha }}"
            assert inputs.get("cache-on-failure") == "false"
            assert 'rustc +"${RUST_TOOLCHAIN}" -vV' in before_cache
            assert "pkg-config --exists wayland-client x11 alsa libudev openssl" in before_cache
        default = env["RUST_TOOLCHAIN"]
        toolchains.add(default)
    web_image = 'ghcr.io/nil-studio/oasis7/ci-rust-release-web@sha256:f47cf4a0ce572de9b871475aed4e38c35fc7e6e128dda93a7d96e50480e03c59'
    image_lane = (name == 'web-wasm-cache' and '      image: ' + web_image + '\n' in body) or image_web
    if image_lane:
        before_cache = body[:cache.start()]
        if image_web:
            assert ("image: ${{ needs.plan.outputs.webImageCompatible == 'true' && '" + web_image + "' || '' }}") in body
            source = '$EXPECTED_WORKFLOW_SHA'
            assert 'EXPECTED_WORKFLOW_SHA: ${{ github.workflow_sha }}' in before_cache
            assert 'RUST_RELEASE_WEB_IMAGE' not in body.split('    steps:', 1)[0]
            assert inputs.get('save-if') == 'false'
        else:
            source = '$GITHUB_SHA'
        assert ('python3 scripts/ci-release-web-image.py --preflight --image-source '
                'd5319297d754a9c7d4de347a2ce8e1a20ba43591 --source "' + source + '"') in before_cache
        helper = (root / 'scripts/ci-release-web-image.py').read_text()
        assert 'run(["rustc", "+" + version, "--version"], rust_env)' in helper
        assert 'if "wasm32-unknown-unknown" not in targets.splitlines():' in helper
        assert 'RUSTUP_AUTO_INSTALL="0"' in helper
        assert env['RUST_RELEASE_WEB_IMAGE'] == web_image
        assert inputs['shared-key'] == 'ci-rust-release-web-' + web_image.split('@sha256:')[1]
        assert inputs['add-rust-environment-hash-key'] == 'true'
        assert inputs['env-vars'] == 'CARGO CC CFLAGS CXX CMAKE RUST'
        compiler = declared_env((root / '.github/workflows/rust.yml').read_text().split('\njobs:\n', 1)[0], 0)
        assert env['RUST_TOOLCHAIN'] == compiler['RUST_TOOLCHAIN']
        toolchains = {env['RUST_TOOLCHAIN'], compiler['OASIS7_WASM_TOOLCHAIN']}
        default = env['RUST_TOOLCHAIN']
        image = re.search(r'@sha256:([0-9a-f]{64})$', web_image)
    assert toolchains and default, f"{name}: verify a pinned toolchain before restoring cache"
    runner = values(body, 4)["runs-on"]
    if runner == (
        "${{ github.event_name == 'pull_request' && "
        "github.event.pull_request.head.repo.full_name == github.repository && "
        "contains(github.event.pull_request.labels.*.name, 'ci-priority') && "
        "'oasis7-priority-linux' || 'ubuntu-24.04' }}"
    ):
        runner = "ubuntu-24.04"
    return {
        "action": cache[1].split(" #", 1)[0], "runner": runner,
        "image": image[1] if image else None,
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
native_cache = (root / ".github/workflows/rust-cache.yml").read_text()
ecs = (root / ".github/workflows/ecs-packages.yml").read_text()

# Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7.
# One repository pool, not a separate allowance for each producer. This is a
# contract for the checked-in block-style env declarations, not a YAML parser.
repository_budget = '20000000000'
budget_entries = {
    'rust.yml': ('CI_CACHE_REPOSITORY_BUDGET_BYTES', 3),
    'rust-cache.yml': ('CI_CACHE_REPOSITORY_BUDGET_BYTES', 4),
    'ecs-packages.yml': ('ECS_CACHE_REPOSITORY_BUDGET_BYTES',
                         1 if 'container: &ecs-native-container' in ecs else 2),
    'release-packages.yml': ('CI_CACHE_REPOSITORY_BUDGET_BYTES', 2),
    'testnet-packages.yml': ('CI_CACHE_REPOSITORY_BUDGET_BYTES', 1),
    'hosted-login-linux-build.yml': ('CI_CACHE_REPOSITORY_BUDGET_BYTES', 1),
}
budget_assignment = re.compile(
    r'^( +)((?:CI|ECS)_CACHE_REPOSITORY_BUDGET_BYTES|'
    r'OASIS7_DEPENDENCY_CACHE_BUDGET_BYTES):[ \t]*([^\n]+)$', re.M)


def check_repository_cache_budgets(workflows):
    declared = {name for name, text in workflows.items() if budget_assignment.search(text)}
    assert declared == set(budget_entries), ('unregistered or missing budget entrypoints', declared)
    for name, (variable, dependency_steps) in budget_entries.items():
        text = workflows[name]
        header = text.split('\njobs:\n', 1)[0]
        assert declared_env(header, 0).get(variable) == repository_budget, (name, 'repository budget')
        reference = '${{ env.' + variable + ' }}'
        expected = [(2, variable, repository_budget)]
        expected += [(10, 'OASIS7_DEPENDENCY_CACHE_BUDGET_BYTES', reference)] * dependency_steps
        if name == 'ecs-packages.yml':
            expected.append((10, 'CI_CACHE_REPOSITORY_BUDGET_BYTES', reference))
            maintenance = job_body(text, 'cache-maintenance')
            assert 'CI_CACHE_REPOSITORY_BUDGET_BYTES: ' + reference in maintenance
        actual = [(len(indent), key, value.strip().strip("\"'"))
                  for indent, key, value in budget_assignment.findall(text)]
        assert sorted(actual) == sorted(expected), (name, 'budget override or broken forwarding', actual)
        assert '--repository-budget-bytes' not in text, (name, 'CLI must not override the workflow budget')
        assert not re.search(r'\b(?:CI|ECS)_CACHE_REPOSITORY_BUDGET_BYTES=', text), \
            (name, 'shell must not override the workflow budget')


# Scan all entrypoints, so a new budget-bearing workflow cannot silently escape
# the inventory. Read-only consumers without budgets need no artificial writer.
budget_workflows = {path.name: path.read_text(encoding='utf-8')
                    for suffix in ('*.yml', '*.yaml')
                    for path in (root / '.github/workflows').glob(suffix)}
check_repository_cache_budgets(budget_workflows)
budget_negative_cases = 0


def reject_budget_mutation(workflows, label):
    global budget_negative_cases
    try:
        check_repository_cache_budgets(workflows)
    except AssertionError:
        budget_negative_cases += 1
    else:
        raise AssertionError(('budget regression was not detected', label))


for name, (variable, dependency_steps) in budget_entries.items():
    text = budget_workflows[name]
    declaration = re.search(rf'^  {variable}: [^\n]+\n', text, re.M)[0]
    for label, replacement in (
        ('old 10 GB', declaration.replace(repository_budget, '10000000000')),
        ('20 GiB instead of decimal GB', declaration.replace(repository_budget, '21474836480')),
        ('missing declaration', ''),
        ('duplicate declaration', declaration + declaration),
    ):
        reject_budget_mutation(budget_workflows | {name: text.replace(declaration, replacement, 1)},
                               (name, label))
    shadow = ('\njobs:\n  budget-shadow-fixture:\n    runs-on: ubuntu-24.04\n'
              f'    env:\n      {variable}: "10000000000"\n'
              '    steps:\n      - run: "true"\n')
    reject_budget_mutation(budget_workflows | {name: text.replace('\njobs:\n', shadow, 1)},
                           (name, 'job override'))
    if not dependency_steps:
        continue
    for index, forwarded in enumerate(re.finditer(
            r'^          OASIS7_DEPENDENCY_CACHE_BUDGET_BYTES: [^\n]+\n', text, re.M)):
        for label, replacement in (
            ('stale upload budget', '          OASIS7_DEPENDENCY_CACHE_BUDGET_BYTES: "10000000000"\n'),
            ('missing upload budget', ''),
            ('duplicate upload budget', forwarded[0] + forwarded[0]),
        ):
            mutated = text[:forwarded.start()] + replacement + text[forwarded.end():]
            reject_budget_mutation(budget_workflows | {name: mutated}, (name, index, label))
reject_budget_mutation({name: text.replace(repository_budget, '10000000000')
                        for name, text in budget_workflows.items()}, 'all entrypoints consistently wrong')
reject_budget_mutation(budget_workflows | {
    'ecs-packages.yml': budget_workflows['ecs-packages.yml'].replace(
        'CI_CACHE_REPOSITORY_BUDGET_BYTES: ${{ env.ECS_CACHE_REPOSITORY_BUDGET_BYTES }}',
        'CI_CACHE_REPOSITORY_BUDGET_BYTES: "10000000000"')}, 'stale ECS maintenance bridge')
reject_budget_mutation(budget_workflows | {
    'unregistered-budget.yml': 'env:\n  CI_CACHE_REPOSITORY_BUDGET_BYTES: "20000000000"\n'},
    'new budget-bearing workflow')
print(f'ok: {len(budget_entries)} repository-wide 20 GB budgets and forwarding; {budget_negative_cases} drift mutations rejected')

assert "\ncache-mode: read\n" in mainnet
native_gates = ("release_gate_runtime_core", "release_gate_runtime_support",
                "release_gate_runtime_sync", "release_gate_soak")
for name in native_gates:
    assert cache_identity(release, name)["shared-key"] == "ci-shared-native-trusted-v4", name
for wf_label, wf_text, allow_candidate in (
    ("testnet-packages.yml", testnet, True),
    ("mainnet-packages.yml", mainnet, False),
    ("release-packages.yml", release, False),
):
    assert "full-regression-qualification" not in wf_text, wf_label
    plan_body = job_body(wf_text, "plan")
    assert "    cache-mode: none\n" in plan_body, wf_label
    assert "    permissions:\n      contents: read\n      actions: read\n" in plan_body, wf_label
    assert "      - uses: actions/checkout@" in plan_body and "ref: ${{ github.workflow_sha }}\n" in plan_body, wf_label
    assert "          fetch-depth: 0\n" in plan_body, wf_label
    assert "          persist-credentials: false\n" in plan_body, wf_label
    assert plan_body.count("GH_TOKEN:") == 1, wf_label
    assert "      - name: Require complete CI for the exact package source\n" in plan_body, wf_label
    assert "SOURCE_SHA: ${{ steps.source.outputs.checkoutSha }}" in plan_body, wf_label
    assert "GITHUB_WORKFLOW_SHA: ${{ github.workflow_sha }}" in plan_body, wf_label
    expected_cmd = (
        'run: python3 scripts/ci-full-result.py verify --source "$SOURCE_SHA" --allow-candidate-source\n'
        if allow_candidate
        else 'run: python3 scripts/ci-full-result.py verify --source "$SOURCE_SHA"\n'
    )
    assert expected_cmd in plan_body, wf_label
    if not allow_candidate:
        assert "--allow-candidate-source" not in plan_body, wf_label
    assert plan_body.index("id: source\n") < plan_body.index("name: Require complete CI for the exact package source\n"), wf_label

for name in ("release_gate_runtime_core", "release_gate_runtime_support", "release_gate_runtime_sync",
             "release_gate_soak", "release_gate_web", "build-web-dist"):
    if name != "build-web-dist":
        assert "    needs: plan\n" in job_body(release, name), (
            f"release-packages.yml {name} must wait for plan (with inlined full-regression qualification) to fail fast"
        )
        assert "    needs: plan\n" in job_body(mainnet, name), (
            f"mainnet-packages.yml {name} must wait for plan (with inlined full-regression qualification) to fail fast"
        )
    expected = cache_identity(release, name)
    actual = cache_identity(mainnet, name)
    if name != "build-web-dist":
        assert actual == expected, (name, "Mainnet cache identity differs from Release", actual, expected)
    assert "save-if: false" in job_body(mainnet, name)
    assert "cache-mode: write" not in job_body(mainnet, name)
    if name in native_gates:
        assert expected == cache_identity(native_cache, "shared-native-cache"), (name, "native writer identity differs")
        assert "save-if: false" in job_body(release, name)
        assert "    cache-mode: read\n" in job_body(release, name)
assert cache_identity(mainnet, "package-native", True) == cache_identity(release, "package-native", True)

web_identity = cache_identity(native_cache, "web-wasm-cache")
legacy_web_identity = cache_identity(release, "release_gate_web")
assert legacy_web_identity["shared-key"] == "release-packages-web-wasm-trusted-v3"
new_web_image = web_identity["image"] is not None
assert web_identity["shared-key"] == ('ci-rust-release-web-' + web_identity['image'] if new_web_image
                                      else 'release-packages-web-wasm-trusted-v3')
if new_web_image:
    for workflow in (release, mainnet):
        assert cache_identity(workflow, 'release_gate_web', image_web=True) == web_identity
    for workflow, name, selected in ((native_cache, 'web-wasm-cache', False),
                                      (release, 'release_gate_web', True),
                                      (mainnet, 'release_gate_web', True)):
        for broken in (
                workflow.replace('shared-key: ci-rust-release-web-', 'shared-key: unrelated-'),
                workflow.replace('env-vars: CARGO CC CFLAGS CXX CMAKE RUST', 'env-vars: CARGO'),
                workflow.replace('ci-rust-release-web@sha256:', 'ci-rust-build@sha256:'),
                workflow.replace(' --preflight --image-source ', ' --lookup-only --image-source ')):
            try:
                cache_identity(broken, name, image_web=selected)
            except AssertionError:
                pass
            else:
                raise AssertionError((name, 'image cache drift must be rejected'))
for workflow, names in ((release, ("release_gate_web",)), (mainnet, ("release_gate_web",)),
                        (rust, ("viewer-js-required", "viewer-performance-report"))):
    for name in names:
        identity = cache_identity(workflow, name)
        body = job_body(workflow, name)
        header = body.split("    steps:", 1)[0]
        if name in ("viewer-js-required", "viewer-performance-report") and "    container:\n" in header:
            image = re.search(
                r"(?m)^      image: \$\{\{ needs\.select\.outputs\.viewer_image_compatible == 'true' && "
                r"'ghcr\.io/nil-studio/oasis7/ci-rust-viewer@sha256:([0-9a-f]{64})' \|\| '' \}\}$", header)
            assert image, (name, "Viewer image and historical host fallback must share the compatibility decision")
            assert "      VIEWER_IMAGE_COMPATIBLE: ${{ needs.select.outputs.viewer_image_compatible }}\n" in header
            assert identity["shared-key"] == (
                "${{ env.VIEWER_IMAGE_COMPATIBLE == 'true' && 'ci-rust-viewer-" + image[1]
                + "' || 'release-packages-web-wasm-trusted-v3' }}"
            ), (name, "Viewer image cache must use only its digest namespace")
            assert "cache-mode: write" not in header and "\ncache-mode: read\n" in workflow
            assert "actions/cache/save@" not in body and "OASIS7_DEPENDENCY_CACHE_" not in body
            identity["shared-key"] = legacy_web_identity["shared-key"]
        assert identity == legacy_web_identity, (name, "historical host Web cache inputs differ")
        assert "save-if: false" in body
package_readers = [cache_identity(workflow, "build-web-dist") for workflow in (release, mainnet)]
assert all(reader["image"] for reader in package_readers)
assert len({reader["image"] for reader in package_readers}) == 1
assert len({reader["shared-key"] for reader in [*package_readers, web_identity]}) == 3
for workflow in (release, mainnet):
    body = job_body(workflow, "build-web-dist")
    job_mode = values(body.split("    steps:", 1)[0], 4).get("cache-mode")
    workflow_mode = values(workflow.split("\njobs:\n", 1)[0], 0).get("cache-mode")
    assert (job_mode or workflow_mode) == "read"
    assert "dependency-cache-tools" not in body and "OASIS7_DEPENDENCY_CACHE_" not in body
    assert "actions/cache/save@" not in body and "actions/cache@" not in body
testnet_web_identity = cache_identity(testnet, "build-web-dist")
testnet_web_build = job_body(testnet, "build-web-dist")
assert "save-if: false" in testnet_web_build
if testnet_web_identity["image"]:
    assert "\ncache-mode: read\n" in testnet
    assert "    cache-mode: write\n" not in testnet_web_build
    assert testnet_web_identity["shared-key"] != web_identity["shared-key"]
else:
    assert testnet_web_identity == legacy_web_identity, "Testnet host Web cache inputs differ"
ecs_web_identity = cache_identity(ecs, "web")
assert ecs_web_identity["image"], "ECS web requires the digest-scoped image reader"
assert ecs_web_identity["shared-key"] != web_identity["shared-key"]
assert "    cache-mode: read\n" in job_body(release, "release_gate_web")
release_web_build = job_body(release, "build-web-dist")
assert "    cache-mode: read\n" in release_web_build
assert "dependency-cache-tools" not in release_web_build
assert "OASIS7_DEPENDENCY_CACHE_" not in release_web_build
web_writer = job_body(native_cache, "web-wasm-cache")
assert "    cache-mode: write\n" in web_writer
assert "OASIS7_DEPENDENCY_CACHE_SOURCE_SHA: ${{ github.sha }}" in web_writer
assert "cache-on-failure: false" in web_writer
save_expression = re.search(r"^          save-if: \$\{\{ (.*) \}\}$", web_writer, re.M)[1]
for ref, workflow_sha, event, mode, status, guard in itertools.product(
    ("refs/heads/main", "refs/heads/candidate", "refs/tags/v1.0.0"),
    ("event", "other-workflow"), ("schedule", "workflow_dispatch", "push", "pull_request"),
    ("full", "", "pr-fast"), ("success", "failure", "cancelled"), ("true", "false", "")):
    expression = save_expression
    context = {"github.ref": ref,
               "github.workflow_sha": workflow_sha, "github.sha": "event",
               "github.event_name": event, "inputs.run_mode": mode, "job.status": status,
               "steps.dependency-cache-tools.outputs.guard_ready": guard}
    for key in sorted(context, key=len, reverse=True):
        expression = expression.replace(key, repr(context[key]))
    expected = (ref == "refs/heads/main" and workflow_sha == "event" and status == "success"
                and guard == "true" and (event == "schedule" or event == "workflow_dispatch" and mode == "full"))
    assert eval(expression.replace("&&", " and ").replace("||", " or "), {"__builtins__": {}}) == expected

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
assert "    needs: [plan, release_gate, cache-bootstrap-maintenance]\n" in release_web_build
assert "    needs: [plan, package-native]\n" in job_body(release, "publish-release")
assert "    needs:\n      - plan\n      - package-native\n      - build-web-dist\n" in job_body(mainnet, "package-index")
for name, required in (("build-web-dist", "release_gate"),
                       ("package-native", "build-web-dist"), ("publish-release", "package-native")):
    body = job_body(release, name)
    assert "cache-maintenance" not in body
    for plan, producer, maintenance in itertools.product(
            ("success", "failure", "skipped", "cancelled", "unknown"), repeat=3):
        context = base_context | {"needs.plan.result": plan, "needs." + required + ".result": producer,
                                  "needs.cache-bootstrap-maintenance.result": maintenance}
        expected = plan == producer == "success"
        assert evaluate(release_condition(name), context) == expected, (name, context)
        assert not evaluate(release_condition(name), context, cancelled=True)

# Native Release and the independent Web writer use the final-archive hook. Admission
# failure can only skip optional caching, while original compile gates survive.
assert "NODE_OPTIONS:" not in release.split("\njobs:\n", 1)[0]
assert "NODE_OPTIONS:" not in native_cache.split("\njobs:\n", 1)[0]
for workflow, name, source in ((release, "package-native", "needs.plan.outputs.checkoutSha"),
                               (native_cache, "web-wasm-cache", "github.sha")):
    body = job_body(workflow, name)
    assert "      contents: read\n      actions: read\n" in body
    assert "actions: write" not in body
    assert "python3 scripts/ci-rust-cache-admission.py install-release" in body
    for step in re.findall(r"^      - (.*?)(?=^      - |\Z)", body, re.M | re.S):
        if "Swatinem/rust-cache@" not in step:
            if "continue-on-error: true" in step:
                expected_install = "Web" if name == "web-wasm-cache" else "Release"
                assert step.startswith("name: Install " + expected_install + " cache admission")
            continue
        for required in ("continue-on-error: true", "cache-provider: github", "cache-on-failure: false",
                         "NODE_OPTIONS: ${{ steps.dependency-cache-tools.outputs.node_options }}",
                         "OASIS7_DEPENDENCY_CACHE_PYTHON: ${{ steps.dependency-cache-tools.outputs.python }}",
                         "OASIS7_DEPENDENCY_CACHE_SOURCE_SHA: ${{ " + source + " }}",
                         "OASIS7_DEPENDENCY_CACHE_BUDGET_BYTES: ${{ env.CI_CACHE_REPOSITORY_BUDGET_BYTES }}"):
            assert required in step, (name, required)
        save = re.search(r"^          save-if: \$\{\{ (.*) \}\}$", step, re.M)[1]
        context = base_context | {"job.status": "success",
                  "steps.dependency-cache-tools.outputs.guard_ready": "true",
                  "steps.linux-package-cache-write.outputs.save": "true", "inputs.run_mode": "full"}
        assert evaluate(save, context)
        for change in ({"job.status": "failure"}, {"job.status": "cancelled"},
                       {"steps.dependency-cache-tools.outputs.guard_ready": ""},
                       {"steps.dependency-cache-tools.outputs.guard_ready": "false"},
                       {source: "old-source"}):
            assert not evaluate(save, context | change), (name, change)
testnet_linux = next(step for step in cache_steps(testnet, workflow_paths[0]) if "if: runner.os == 'Linux'" in step)
if linux_images:
    assert "NODE_OPTIONS: ${{ steps.dependency-cache-tools.outputs.node_options }}" in testnet_linux
    assert "OASIS7_DEPENDENCY_CACHE_PYTHON: ${{ steps.dependency-cache-tools.outputs.python }}" in testnet_linux
    assert "OASIS7_DEPENDENCY_CACHE_FAMILY: testnet-linux-image" in testnet_linux
    mainnet_linux = next(step for step in cache_steps(mainnet, workflow_paths[1]) if "if: runner.os == 'Linux'" in step)
    assert "NODE_OPTIONS:" not in mainnet_linux and "GH_TOKEN:" not in mainnet_linux
else:
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
        if workflow == ecs and name == "native":
            assert len(guarded) == 3, "ECS shared native steps require one normalizer and two exclusive cache paths"
            normal, candidate, image = guarded
            assert f"if: hashFiles('{normalizer}') != ''" in normal
            assert f"if: github.job == 'native' && hashFiles('{normalizer}') != ''" in candidate
            assert "save-if: false" in candidate
            if 'container: &ecs-native-container' in body:
                assert '"$RUST_TOOLCHAIN" "$OASIS7_WASM_TOOLCHAIN"' in normal
                assert "shared-key: ${{ env.ECS_NATIVE_DEPENDENCY_NAMESPACE }}" in candidate
                assert "cache-provider: github" in candidate
                assert "GH_TOKEN" not in candidate
            else:
                assert "if [[ \"$GITHUB_JOB\" == 'native-trusted' ]]; then" in normal
                assert 'toolchains+=("$OASIS7_WASM_TOOLCHAIN")' in normal
                assert "shared-key: oasis7-ecs-native-linux-ecs-v2" in candidate
            assert f"if: github.job == 'native-trusted' && hashFiles('{normalizer}') != '' && steps.image-dependency-cache-tools.outputs.guard_ready == 'true'" in image
            assert "NODE_OPTIONS: ${{ steps.image-dependency-cache-tools.outputs.node_options }}" in image
            continue
        web_image_reader = (name == 'release_gate_web' and '    container:' in body)
        if web_image_reader:
            assert len(guarded) == 3, (name, 'one host normalizer and exclusive host/image cache steps')
            assert "if: env.RELEASE_WEB_IMAGE_COMPATIBLE == 'true'" in guarded[2]
            assert 'ci-rust-cache-toolchains.py' not in guarded[2]
            assert 'save-if: false' in guarded[2]
            cache_identity(workflow, name, image_web=True)
            guarded = guarded[:2]
        assert len(guarded) == 2, (name, "one normalizer and one cache expected")
        for step in guarded:
            guard = f"if: hashFiles('{normalizer}') != ''"
            if web_image_reader:
                guard = f"if: env.RELEASE_WEB_IMAGE_COMPATIBLE != 'true' && (hashFiles('{normalizer}') != '')"
                assert re.search(r"^        " + re.escape(guard) + "$", step, re.M), name
                continue
            if name == "build-web-dist" and workflow in (release, mainnet):
                guard += " && steps.source-toolchain.outputs.image_toolchain == 'true'"
                assert re.search(r"^        " + re.escape(guard) + "$", step, re.M), name
            else:
                assert guard in step, name
        assert body.index(f"python3 {normalizer}") < body.index("Swatinem/rust-cache@"), name
        keeps_wasm_toolchain = (name in native_gates
                                or (workflow == ecs and name == "web")
                                or (name == "build-web-dist" and (
                                    workflow in (release, mainnet)
                                    or workflow == testnet and testnet_web_identity["image"] is not None
                                )))
        for command in re.findall(r"^\s+(?:run: )?python3 scripts/ci-rust-cache-toolchains.py (.*)$", body, re.M):
            assert ('"$OASIS7_WASM_TOOLCHAIN"' in command) == keeps_wasm_toolchain

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
            lanes = ("false", "true") if (
                name == "release_gate_runtime_sync"
                or name == "release_gate_runtime_core" and "USE_CORE_IMAGE:" in gate
            ) else ("false",)
            (fixture / "oasis7-runtime-core-image.sh").write_text(
                'test "$1" = --core || exit 24\nexec ./scripts/ci-tests.sh full-core\n')
            launcher = fixture / "oasis7-runtime-sync-image.sh"
            launcher.write_text('exec bash --noprofile --norc -e -o pipefail -s\n')
            for lane in lanes:
                for filename in filenames:
                    result = subprocess.run(
                        ["bash", "--noprofile", "--norc", "-e", "-o", "pipefail", "-c", script],
                        cwd=fixture, env=dict(os.environ, TEST_FAIL_SCRIPT=filename,
                                              USE_IMAGE=lane, USE_CORE_IMAGE=lane,
                                              EXPECTED_SHA="0" * 40, RUNNER_TEMP=str(fixture)),
                        capture_output=True, text=True)
                    assert result.returncode == 23, (name, lane, filename, result.stdout, result.stderr)
                    report = fixture / ".tmp/release_gate_runtime_sync/runtime-sync-checks.log"
                    if name == "release_gate_runtime_sync":
                        assert filename in report.read_text()
        if name != "release_gate_runtime_sync":
            assert "fetch-depth: 0" in body, "frozen fixture history must be available"
            assert 'scripts/doc-governance-requirements.txt' in body
            assert 'OASIS7_PRODUCT_DOC_BASE=' in body and 'OASIS7_PRODUCT_DOC_HEAD=' in body
            assert "git rev-parse --verify 'HEAD^1'" in body
            assert "git rev-parse --verify 'HEAD^{commit}'" in body

launcher = root / "scripts/package-release-runtime-sync-image.sh"
if any("Select runtime sync image" in workflow for workflow in (release, mainnet)):
    with tempfile.TemporaryDirectory() as directory:
        checkout = Path(directory)
        subprocess.run(["git", "init", "-q", str(checkout)], check=True)
        toolchain = checkout / "rust-toolchain.toml"
        toolchain.write_bytes((root / toolchain.name).read_bytes())
        subprocess.run(["git", "add", toolchain.name], cwd=checkout, check=True)
        subprocess.run(["git", "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
                        "-c", "commit.gpgsign=false", "commit", "-qm", "toolchain"],
                       cwd=checkout, check=True)
        source = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=checkout, text=True).strip()
        output = checkout / "github-output"
        env = dict(os.environ, GITHUB_REPOSITORY="nil-studio/oasis7", GITHUB_JOB="release_gate_runtime_sync",
                   GITHUB_EVENT_NAME="workflow_dispatch", GITHUB_REF="refs/heads/main",
                   GITHUB_WORKFLOW_REF="nil-studio/oasis7/.github/workflows/mainnet-packages.yml@refs/heads/main",
                   GITHUB_SHA=source, GITHUB_WORKFLOW_SHA=source, EXPECTED_SHA=source,
                   RUNNER_ENVIRONMENT="github-hosted", RUNNER_OS="Linux", RUNNER_ARCH="X64",
                   GITHUB_OUTPUT=str(output))

        def select(expected, mode="--select", **overrides):
            output.write_text("")
            subprocess.run(["bash", str(launcher), mode], cwd=checkout,
                           env=env | overrides, check=True)
            assert output.read_text() == f"enabled={expected}\n", overrides

        select("true")
        tag = "refs/tags/2026-11-26_v1"
        push = dict(GITHUB_EVENT_NAME="push", GITHUB_REF=tag,
                    GITHUB_WORKFLOW_REF=f"nil-studio/oasis7/.github/workflows/release-packages.yml@{tag}")
        select("true", **push)
        select("false", **(push | {"EXPECTED_SHA": "0" * 40}))
        select("false", GITHUB_REPOSITORY="example/oasis7")
        select("false", RUNNER_ENVIRONMENT="self-hosted")
        select("false", GITHUB_REF="refs/heads/candidate")
        select("false", GITHUB_EVENT_NAME="pull_request")
        select("false", GITHUB_WORKFLOW_SHA="0" * 40)
        if any("Select runtime core image" in workflow for workflow in (release, mainnet)):
            core_job = {"GITHUB_JOB": "release_gate_runtime_core"}
            select("false", "--select-core", **core_job)
            select("false", "--select-core")
            driver = checkout / "scripts/ci-tests.sh"
            driver.parent.mkdir()
            driver.write_bytes((root / "scripts/ci-tests.sh").read_bytes())
            subprocess.run(["git", "add", "scripts/ci-tests.sh"], cwd=checkout, check=True)
            subprocess.run(["git", "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
                            "-c", "commit.gpgsign=false", "commit", "-qm", "core driver"],
                           cwd=checkout, check=True)
            new = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=checkout, text=True).strip()
            current = dict(EXPECTED_SHA=new, GITHUB_SHA=new, GITHUB_WORKFLOW_SHA=new)
            select("true", "--select-core", **core_job, **current)
            select("false", "--select-core", **core_job, **(current | {"EXPECTED_SHA": source}))
            select("false", "--select-core", **core_job, **(current | {"GITHUB_EVENT_NAME": "pull_request"}))
            select("false", "--select", **core_job, **current)
        toolchain.write_text(toolchain.read_text().replace("1.96.0", "1.92.0"))
        select("false")

print("ok: compatible cache readers share exclusive writers, old sources stay cold, and gate failures propagate")

PY
