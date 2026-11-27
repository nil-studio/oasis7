#!/usr/bin/env python3
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
spec = importlib.util.spec_from_file_location("source_plan", Path(__file__).with_name("package-source-plan.py"))
plan = importlib.util.module_from_spec(spec)
spec.loader.exec_module(plan)
with tempfile.TemporaryDirectory() as temporary:
    root=Path(temporary)
    def git(*args): return subprocess.check_output(["git", *args],text=True).strip()
    old=os.getcwd()
    os.chdir(root)
    try:
        git("init", "-q")
        (root / "a").write_text("fixture")
        git("add", ".")
        git("-c", "user.name=Test", "-c", "user.email=test@example.test", "commit", "-qm", "fixture")
        sha=git("rev-parse", "HEAD")
        git("update-ref", "refs/remotes/origin/main", sha)
        git("tag", "v1.2.3")
        git("-c", "user.name=Test", "-c", "user.email=test@example.test", "tag", "-am", "annotated", "v1.2.4")
        for ref in ("v1.2.3", "refs/tags/v1.2.4"):
            result=plan.resolve(ref, True)
            assert result["checkoutSha"]==sha
        git("update-ref", "refs/remotes/origin/v1.2.3", sha)
        try: plan.resolve("v1.2.3")
        except ValueError: pass
        else: raise AssertionError("ambiguous ref accepted")
        assert plan.resolve("refs/heads/main")["checkoutSha"]==sha
        assert plan.resolve(sha)["checkoutSha"]==sha
        strange="literal$(touch${IFS}pwned)"
        git("update-ref", "refs/remotes/origin/" + strange, sha)
        assert plan.resolve(strange)["checkoutSha"] == sha
        assert not (root / "pwned").exists()
        for bad in ("v1.2.3\nextra=x", "v1.2.3;touch pwn", "--help", "v1abc", "refs/tags/v1.2.3\x00"):
            try: plan.resolve(bad, True)
            except ValueError: pass
            else: raise AssertionError(bad)
        git("checkout", "-q", "--orphan", "outside-protected-history")
        git("-c", "user.name=Test", "-c", "user.email=test@example.test", "commit", "-qm", "outside")
        git("tag", "v9.9.9")
        try: plan.resolve("v9.9.9", True)
        except ValueError: pass
        else: raise AssertionError("release outside protected history accepted")
    finally: os.chdir(old)
print("ok: immutable source plan tags, commits, branches, ambiguity, input injection")

import re
repo = Path(__file__).resolve().parent.parent
def reject_duplicate_workflow_keys(text):
    # Repository workflow mappings use plain scalar keys and block scalars.
    # Unlike safe_load, retain every mapping entry and reject repeated keys.
    scopes = {}
    scalar_indent = None
    for number, line in enumerate(text.splitlines(), 1):
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        indent = len(line) - len(line.lstrip())
        if scalar_indent is not None:
            if indent > scalar_indent:
                continue
            scalar_indent = None
        content = line.lstrip()
        for depth in list(scopes):
            if depth > indent:
                del scopes[depth]
        if content.startswith("- "):
            scopes.pop(indent + 2, None)
            content = content[2:]
            indent += 2
        match = re.match(r"([A-Za-z_][A-Za-z0-9_-]*):(?:\s|$)", content)
        if not match:
            continue
        key = match.group(1).lower()
        keys = scopes.setdefault(indent, {})
        assert key not in keys, f"duplicate YAML key {key} at {number}; first at {keys.get(key)}"
        keys[key] = number
        if re.match(r"[|>][+-]?[0-9]?$", content[match.end():].strip()):
            scalar_indent = indent

try:
    reject_duplicate_workflow_keys("with:\n  save-if: false\n  save-if: false\n")
except AssertionError:
    pass
else:
    raise AssertionError("duplicate mapping entry accepted")

for path in (repo / ".github/workflows").glob("*.yml"):
    workflow = path.read_text()
    reject_duplicate_workflow_keys(workflow)
    assert "\npermissions:\n" in workflow, path
    for action in re.findall(r"uses: ([^\s]+)", workflow):
        if not action.startswith("./"):
            assert re.fullmatch(r"[\w/-]+@[0-9a-f]{40}", action), (path, action)
for name in ("mainnet-packages", "testnet-packages", "release-packages"):
    workflow = (repo / ".github/workflows" / (name + ".yml")).read_text()
    blocks = re.split(r"(?=^  [\w-]+:\n)", workflow, flags=re.M)
    for block in blocks:
        if "ref: ${{ needs.plan.outputs.checkoutSha }}" in block:
            assert "Verify planned source HEAD" in block
            assert "persist-credentials: false" in block
            assert "plan" in re.search(r"    needs:[^\n]*(?:\n      - [^\n]+)*", block)[0]
    assert "SOURCE_PLAN: ${{ needs.plan.outputs.sourcePlan }}" in workflow
    assert "trusted-v2" in workflow
    if name != "release-packages":
        assert "cache-mode: read" in workflow
        for cache in re.findall(r"uses: Swatinem/rust-cache@[^\n]+\n(.*?)(?=      - |\Z)", workflow, re.S):
            image_cache = re.search(r"shared-key: oasis7-package-native-linux-x64-\$\{\{ inputs\.build_profile \}\}-image-([0-9a-f]{64})", cache)
            if name == "testnet-packages" and image_cache:
                image = "ghcr.io/nil-studio/oasis7/ci-rust-build@sha256:" + image_cache[1]
                assert f"RUST_PACKAGE_NATIVE_IMAGE: {image}" in cache
                assert "OASIS7_DEPENDENCY_CACHE_FAMILY: testnet-linux-image" in cache
                assert "NODE_OPTIONS: ${{ steps.dependency-cache-tools.outputs.node_options }}" in cache
                assert "save-if: ${{ job.status == 'success' && steps.dependency-cache-tools.outputs.guard_ready == 'true' && github.job == 'package-native-trusted' && steps.linux-package-cache-write.outputs.save == 'true' }}" in cache
            elif name == "testnet-packages" and "shared-key: oasis7-package-native-" in cache:
                assert "save-if: ${{ job.status == 'success' && github.job == 'package-native-trusted' && steps.linux-package-cache-write.outputs.save == 'true' }}" in cache
            else:
                assert "save-if: false" in cache
for workflow in ("release", "testnet", "mainnet"):
    native_yaml = (repo / ".github/workflows" / (workflow + "-packages.yml")).read_text()
    native = re.search(r"(?ms)^  package-native:\n.*?(?=^  [a-z][a-z0-9_-]+:|\Z)", native_yaml)[0]
    image = re.search(r"ghcr\.io/nil-studio/oasis7/ci-rust-build@sha256:([0-9a-f]{64})", native)
    if image:
        assert "image: ${{ matrix.os == 'ubuntu-24.04' && '" + image[0] + "' || '' }}" in native
        assert native.count("options: --user 1001:1001 --init") == 1
        assert "packages: read" in native and "timeout-minutes: 180" in native
        assert "sudo apt-get " not in native and "        volumes:" not in native
        assert "name: Install pinned Rust toolchain\n        if: runner.os != 'Linux'" in native
        assert "RUST_PACKAGE_NATIVE_IMAGE: " + image[0] in native
        assert "INPUT_SHARED-KEY" not in native
        if workflow != "mainnet":
            assert "steps.dependency-cache-tools.outputs.guard_ready" in native
        if workflow == "release":
            assert f"shared-key: oasis7-package-native-linux-x64-packaging-image-{image[1]}" in native
            assert "OASIS7_DEPENDENCY_CACHE_FAMILY: release-linux-image" in native
            assert "dpkg-deb --version" in native
        elif workflow == "testnet":
            assert "container: &testnet-native-container" in native
            trusted = re.search(r"(?ms)^  package-native-trusted:\n.*?(?=^  [a-z][a-z0-9_-]+:|\Z)", native_yaml)[0]
            assert "container: *testnet-native-container" in trusted
            assert "steps: *testnet-native-steps" in trusted
            assert "cache-mode: write" in trusted and "timeout-minutes: 180" in trusted

release = (repo / ".github/workflows/release-packages.yml").read_text()
assert 'test "$object" = "$PLANNED_SHA"' in release
assert "cache-mode: none" in release
assert 'while [[ "$kind" == tag ]]' in release
print("ok: every current workflow action pinned, package SHA consumers and cache policies")

# Execute the trusted workflow publication check against a fake GitHub API.
import json
import shutil
check = re.search(r"      - name: Verify release tag still matches plan\n(.*?)(?=      - name: Publish GitHub release)", release, re.S)[1]
script = check.split("        run: |\n", 1)[1]
script = "\n".join(line[10:] for line in script.splitlines())
with tempfile.TemporaryDirectory() as temporary:
    root=Path(temporary)
    tool=root / "gh"
    tool.write_text("#!/usr/bin/env python3\nimport json,os,sys\nkind=os.environ['MOCK_KIND']\nsha=os.environ['MOCK_SHA']\nprint(json.dumps({'object': {'type': 'commit' if '/git/tags/' in sys.argv[2] else kind, 'sha': sha if kind == 'commit' or '/git/tags/' in sys.argv[2] else 'b'*40}}))\n")
    tool.chmod(0o755)
    # The doc-checker runner does not necessarily ship jq. Exercise the
    # workflow's exact jq argument contract against a strict local fixture
    # rather than allowing a missing system package to mask release checks.
    jq = root / "jq"
    jq.write_text("""#!/usr/bin/env python3
import json,sys
args=sys.argv[1:]
if len(args)==2 and args[0]=='-r' and args[1] in ('.object.sha','.object.type'):
    value=json.load(sys.stdin)['object'][args[1].split('.')[-1]]
    if not isinstance(value,str): sys.exit(2)
    print(value)
elif len(args)==6 and args[:3]==['-e','--arg','sha'] and args[4]=='.checkoutSha == $sha':
    with open(args[5],encoding='utf-8') as source:
        actual=json.load(source).get('checkoutSha')
    print('true' if actual==args[3] else 'false')
    sys.exit(0 if actual==args[3] else 1)
else:
    sys.exit(2)
""")
    jq.chmod(0o755)
    (root / "output/release/assets").mkdir(parents=True)
    for platform in ("linux-x64", "macos-x64", "windows-x64"):
        (root / "output/release/assets" / (platform+"-SOURCEPLAN.json")).write_text(json.dumps({"checkoutSha": "a"*40}))
        (root / "output/release/assets" / (platform+"-BUILDINFO")).write_text("commit=" + "a"*40 + "\n")
    for kind in ("commit", "tag"):
        for moved in (False, True):
            environment=dict(os.environ, PATH=str(root)+os.pathsep+os.environ['PATH'],
                PLANNED_REF="refs/tags/v1.2.3", PLANNED_SHA="a"*40,
                GITHUB_REPOSITORY="example/repo", GITHUB_OUTPUT=str(root / "github-output"),
                MOCK_KIND=kind, MOCK_SHA=("c" if moved else "a")*40)
            result=subprocess.run(["bash", "-c", script], env=environment, cwd=root, capture_output=True, text=True)
            assert (result.returncode == 0) != moved, (kind, moved, result.stderr)
    (root / "output/release/assets/linux-x64-SOURCEPLAN.json").write_text(json.dumps({"checkoutSha": "c"*40}))
    environment["MOCK_SHA"]="a"*40
    assert subprocess.run(["bash", "-c", script], env=environment, cwd=root, capture_output=True).returncode != 0
print("ok: publication rejects lightweight/annotated tag drift and mixed artifact SHA")


# Exercise the actual Testnet staging/checksum blocks and artifact path selection.
import glob
# Windows hosted runners use Bash >= 4 (mapfile); macOS system Bash is 3.2.
workflow_bash = next((candidate for candidate in (shutil.which("bash"), "/opt/homebrew/bin/bash", "/usr/local/bin/bash")
                      if candidate and Path(candidate).is_file() and
                      subprocess.run([candidate, "-c", '(( BASH_VERSINFO[0] >= 4 ))'], capture_output=True).returncode == 0), None)
assert workflow_bash, "Testnet checksum fixture requires Bash >= 4, as used by the Windows runner"
testnet = (repo / ".github/workflows/testnet-packages.yml").read_text()
def workflow_run(name):
    block = re.search(r"      - name: " + re.escape(name) + r"\n(.*?)(?=      - name: |\Z)", testnet, re.S)[1]
    return "\n".join(line[10:] for line in block.split("        run: |\n", 1)[1].splitlines())

def render(text, platform, asset):
    return text.replace("${{ matrix.platform }}", platform).replace("${{ matrix.asset_name }}", asset)

upload = testnet.split("      - name: Upload testnet package artifacts\n", 1)[1]
paths = upload.split("          path: |\n", 1)[1].split("          if-no-files-found:", 1)[0]
for platform, asset in (("linux-x64", "oasis7-linux-x64.deb"),
                        ("macos-x64", "oasis7-macos-x64.dmg"),
                        ("windows-x64", "oasis7-windows-x64.exe")):
    for missing in (False, True):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            assets = root / "output/testnet-packages/assets"
            assets.mkdir(parents=True)
            sourceplan = platform + "-SOURCEPLAN.json"
            files = [asset, platform + "-BUILDINFO", platform + "-bundle-manifest.json",
                     platform + "-entrypoint-validation.log", "oasis7-" + platform + "-ops-tools.tar.gz"]
            for name in files:
                (assets / name).write_text("fixture\n")
            if not missing:
                (assets / sourceplan).write_text(json.dumps({"checkoutSha": "a" * 40}))
            if platform == "windows-x64":
                # Stub only the unrelated governed-world producer; execute the real move loop.
                tools = root / "tools"
                tools.mkdir()
                stub = tools / "python3"
                stub.write_text("#!/bin/bash\nset -eu\nwhile [[ $# -gt 0 ]]; do if [[ $1 == --out-dir ]]; then closure=$2; break; fi; shift; done\nmkdir -p \"$closure/generated-world\" \"$closure/doc/testing/evidence\"\nfor kind in bundle genesis manifest; do touch \"$closure/public-testnet-governed-bootstrap-$kind-2026-06-06.windows.json\"; done\ntouch \"$closure/public-testnet-governed-bootstrap-bootstrap-peers-2026-06-06.windows.txt\"\n")
                stub.chmod(0o755)
                environment = dict(os.environ, PATH=str(tools) + os.pathsep + os.environ["PATH"])
                staged = subprocess.run([workflow_bash, "-c", workflow_run("Stage Windows governed deployment closure")], cwd=root, env=environment, capture_output=True, text=True)
                if missing:
                    assert staged.returncode != 0, "missing Windows SOURCEPLAN accepted"
                    continue
                assert staged.returncode == 0, staged.stderr
            result = subprocess.run([workflow_bash, "-c", render(workflow_run("Generate checksums"), platform, asset)], cwd=root, capture_output=True, text=True)
            if missing:
                assert result.returncode != 0, "missing " + platform + " SOURCEPLAN accepted"
                continue
            assert result.returncode == 0, result.stderr
            checksum_root = assets / "windows-governed-closure" if platform == "windows-x64" else assets
            sums = (checksum_root / (platform + "-SHA256SUMS")).read_text()
            assert sourceplan in sums, platform + " SOURCEPLAN not checksummed"
            checked = subprocess.run(["shasum", "-a", "256", "-c", platform + "-SHA256SUMS"], cwd=checksum_root, capture_output=True, text=True)
            assert checked.returncode == 0, checked.stderr
            selected = {file.resolve() for pattern in render(paths, platform, asset).splitlines()
                        for file in map(Path, glob.glob(str(root / pattern.strip()), recursive=True)) if file.is_file()}
            assert (checksum_root / sourceplan).resolve() in selected, platform + " SOURCEPLAN not uploaded"
            (checksum_root / sourceplan).write_text("tampered")
            assert subprocess.run(["shasum", "-a", "256", "-c", platform + "-SHA256SUMS"], cwd=checksum_root, capture_output=True).returncode != 0
print("ok: Testnet SOURCEPLAN final upload selection, missing-file rejection and checksum integrity on all native platforms")

# Exercise the exact checksum shell used by each migrated Linux package
# consumer, and compare the manifest bytes with the legacy Perl verifier.
mainnet = (repo / ".github/workflows/mainnet-packages.yml").read_text()
installer = testnet.split("  package-installers:\n", 1)[1]
fixed_installer = "name: Verify the Linux packaging runtime" in installer
if fixed_installer:
    image = "ghcr.io/nil-studio/oasis7/ci-runtime@sha256:3984caa42d82963f93af752bf79e3a07838f2c94279cfbb2deeb08f715cd62c9"
    assert "image: ${{ matrix.os == 'ubuntu-24.04' && '" + image + "' || '' }}" in installer
    assert "options: --user 1001:1001 --init" in installer
    assert "timeout-minutes: 90" in installer
    assert "name: Install pinned Rust toolchain\n        if: runner.os != 'Linux'" in installer
    assert "sudo apt-get" not in installer
    assert "--native-binaries" in installer and "--web-dist" in installer and "--web-launcher-dist" in installer
    assert "volumes:" not in installer and "actions: write" not in installer

for name, body, migrated in (
    ("Mainnet", mainnet, "name: Verify the fixed Linux package toolchain" in mainnet),
    ("Testnet", installer, fixed_installer),
):
    if not migrated:
        continue
    block = body.split("      - name: Write BUILDINFO\n", 1)[1].split("      - name:", 1)[0]
    checksum_run = block.split('            cd "${bundle}"\n', 1)[1].rsplit("          fi", 1)[0]
    with tempfile.TemporaryDirectory() as temporary:
        bundle = Path(temporary)
        (bundle / "bin").mkdir()
        payloads = ("BUILDINFO", "bin/launcher with spaces", "bin/runtime")
        for file in payloads:
            (bundle / file).write_text(f"fixture for {file}\n")
        expected = subprocess.check_output(["shasum", "-a", "256", *payloads], cwd=bundle)
        subprocess.run([workflow_bash, "-e", "-o", "pipefail", "-c", checksum_run], cwd=bundle, check=True)
        assert (bundle / "SHA256SUMS").read_bytes() == expected
        subprocess.run(["shasum", "-a", "256", "-c", "SHA256SUMS"], cwd=bundle, check=True,
                       capture_output=True)
        (bundle / "bin/runtime").write_text("tampered\n")
        assert subprocess.run(["sha256sum", "-c", "SHA256SUMS"], cwd=bundle,
                              capture_output=True).returncode != 0
    print(f"ok: {name} Linux checksum shell preserves bytes and rejects tampered payload")


soak_selector = repo / "scripts/package-release-soak-image.py"
if soak_selector.is_file():
    with tempfile.TemporaryDirectory() as directory:
        checkout = Path(directory)
        def soak_git(*args):
            return subprocess.check_output(["git", *args], cwd=checkout, text=True).strip()

        def commit_fixture():
            soak_git("add", "rust-toolchain.toml", "scripts")
            soak_git("-c", "user.name=Fixture", "-c", "user.email=test@example.test",
                     "-c", "commit.gpgsign=false", "commit", "-qm", "source")
            return soak_git("rev-parse", "HEAD")

        soak_git("init", "-q")
        (checkout / "scripts").mkdir()
        toolchain = checkout / "rust-toolchain.toml"
        toolchain.write_bytes((repo / toolchain.name).read_bytes())
        for name in ("release-gate.sh", "p2p-longrun-soak.sh", "s10-five-node-game-soak.sh"):
            path = checkout / "scripts" / name
            path.write_text("#!/usr/bin/env bash\n")
            path.chmod(0o755)
        modern = commit_fixture()
        toolchain.write_text(toolchain.read_text().replace("1.96.0", "1.92.0"))
        historical = commit_fixture()
        output = checkout / "github-output"
        env = dict(os.environ, GITHUB_REPOSITORY="nil-studio/oasis7", GITHUB_JOB="plan",
                   GITHUB_EVENT_NAME="workflow_dispatch", GITHUB_REF="refs/heads/main",
                   GITHUB_WORKFLOW_REF="nil-studio/oasis7/.github/workflows/mainnet-packages.yml@refs/heads/main",
                   GITHUB_SHA=historical, GITHUB_WORKFLOW_SHA=historical, GITHUB_OUTPUT=str(output))

        def soak_result(source, expected, **overrides):
            output.write_text("")
            subprocess.run([sys.executable, str(soak_selector), source], cwd=checkout,
                           env=env | overrides, check=True)
            assert output.read_text() == f"compatible={expected}\n", (source, overrides)

        soak_result(modern, "true")
        soak_result(historical, "false")
        soak_result(modern, "false", GITHUB_REF="refs/heads/codex/untrusted")
        soak_result(modern, "false", GITHUB_REPOSITORY="fork/oasis7")
        soak_result(modern, "false", GITHUB_WORKFLOW_SHA=modern)
        soak_result("not-a-sha", "false")
        ref = "refs/tags/v2.0.0"
        tagged = dict(
            GITHUB_REF=ref, GITHUB_EVENT_NAME="push", GITHUB_SHA=modern, GITHUB_WORKFLOW_SHA=modern,
            GITHUB_WORKFLOW_REF=f"nil-studio/oasis7/.github/workflows/release-packages.yml@{ref}")
        soak_result(modern, "true", **tagged)
        soak_result(historical, "false", **tagged)
        (checkout / "scripts/s10-five-node-game-soak.sh").chmod(0o644)
        toolchain.write_bytes((repo / toolchain.name).read_bytes())
        missing_executable = commit_fixture()
        soak_result(missing_executable, "false")
    print("ok: soak image selects the verified Git object and rejects unsafe or incompatible sources")
