#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
python3 - "$ROOT_DIR" <<'PY'
from pathlib import Path
import os
import re
import shutil
import subprocess
import sys
import tempfile
import textwrap

root = Path(sys.argv[1])
workflow = (root / ".github/workflows/release-packages.yml").read_text()
mainnet = (root / ".github/workflows/mainnet-packages.yml").read_text()
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
testnet_image = re.search(
    r"^    container:\n      image: ghcr\.io/nil-studio/oasis7/ci-rust-build@sha256:([0-9a-f]{64})$",
    testnet_web, re.MULTILINE,
)
if testnet_image:
    assert field(testnet_web, "shared-key", 10) == "testnet-web-ci-rust-build-" + testnet_image[1]
    assert field(testnet_web, "add-rust-environment-hash-key", 10) == "true"
else:
    assert "\n    container:" not in testnet_web
    assert field(testnet_web, "shared-key", 10) == "release-packages-web-wasm-trusted-v3"
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

mainnet_web = job(mainnet, "build-web-dist")
assert field(mainnet_web, "TRUNK_VERSION", 6) == version
assert "ref: ${{ github.workflow_sha }}" in mainnet_web
assert "sparse-checkout: .github/actions/setup-linux-mold" in mainnet_web
assert "sparse-checkout-cone-mode: false" not in mainnet_web
stage_name = "Stage the executing workflow's pinned Trunk installer"
assert mainnet_web.index("ref: ${{ github.workflow_sha }}") < mainnet_web.index(stage_name)
assert mainnet_web.index(stage_name) < mainnet_web.index("ref: ${{ needs.plan.outputs.checkoutSha }}")
stage = step(mainnet_web, stage_name)
assert 'WORKFLOW_TOOLS_SHA: ${{ github.workflow_sha }}' in stage
assert 'test "$(git rev-parse HEAD)" = "$WORKFLOW_TOOLS_SHA"' in run(stage)
assert 'git show HEAD:scripts/install-ci-trunk.sh > "$installer"' in run(stage)
assert run(step(mainnet_web, "Install verified pinned Trunk")) == expected_install.replace(
    "bash scripts/install-ci-trunk.sh", 'bash "$RUNNER_TEMP/oasis7-mainnet-install-trunk.sh"')
assert run(step(mainnet_web, "Verify pinned trunk")) == run(verify)
assert not re.search(r"\bcargo\s+install\s+trunk\b", mainnet_web)
testnet_build = run(step(testnet_web, "Build web dists"))
for package_job, outdir in ((web, "release"), (mainnet_web, "mainnet-packages")):
    assert run(step(package_job, "Build web dists")) == testnet_build.replace(
        "output/testnet-packages/", f"output/{outdir}/")

legacy_name = "Keep the planned source's Rust toolchain"
legacy_toolchain = run(step(mainnet_web, legacy_name))
assert run(step(web, legacy_name)) == legacy_toolchain
for package_job in (web, mainnet_web):
    assert package_job.index("Verify planned source HEAD") < package_job.index("Verify complete planned Web checkout")
    assert package_job.index("Verify complete planned Web checkout") < package_job.index("Install viewer web dependencies")
    assert 'git ls-files -t' in run(step(package_job, "Verify complete planned Web checkout"))
    assert 'git diff --exit-code HEAD --' in run(step(package_job, "Verify complete planned Web checkout"))
    assert package_job.index("Verify planned source HEAD") < package_job.index(legacy_name)
    assert package_job.index(legacy_name) < package_job.index("Normalize Rust cache toolchains")

# A matching HEAD is insufficient if checkout left files sparse or missing.
with tempfile.TemporaryDirectory(prefix="package-web-checkout-") as directory:
    temp = Path(directory)
    repo = temp / "repo"
    repo.mkdir()
    install_blob = b"#!/bin/bash\n# immutable workflow installer\n"
    lockfile = repo / "crates/oasis7_viewer/package-lock.json"
    lockfile.parent.mkdir(parents=True)
    linker = repo / ".github/actions/setup-linux-mold/action.yml"
    linker.parent.mkdir(parents=True)
    linker.write_text("workflow tool\n")
    installer_path = repo / "scripts/install-ci-trunk.sh"
    installer_path.parent.mkdir()
    installer_path.write_bytes(install_blob)
    env = dict(os.environ, GIT_CONFIG_GLOBAL=str(temp / "git-config"), RUNNER_TEMP=str(temp),
               GITHUB_WORKSPACE=str(repo))

    def git(*args):
        return subprocess.check_output(["git", *args], cwd=repo, env=env, text=True).strip()

    def shell(script):
        return subprocess.run(["bash", "-e", "-o", "pipefail", "-c", script], cwd=repo,
                              env=env, capture_output=True, text=True)

    git("init", "-q")
    git("config", "user.name", "CI fixture")
    git("config", "user.email", "fixture@example.invalid")
    lockfile.write_text("business lock\n")
    git("add", ".")
    git("commit", "-qm", "business source")
    business = git("rev-parse", "HEAD")
    lockfile.write_text("workflow revision lock\n")
    git("commit", "-qam", "workflow revision")
    tooling = git("rev-parse", "HEAD")
    git("sparse-checkout", "set", "--cone", ".github/actions/setup-linux-mold")
    assert not installer_path.exists()
    env["WORKFLOW_TOOLS_SHA"] = business
    assert shell(run(stage)).returncode != 0
    env["WORKFLOW_TOOLS_SHA"] = tooling
    assert shell(run(stage)).returncode == 0
    assert (temp / "oasis7-mainnet-install-trunk.sh").read_bytes() == install_blob

    verify_checkout = run(step(mainnet_web, "Verify complete planned Web checkout"))
    assert shell(verify_checkout).returncode != 0
    git("sparse-checkout", "disable")
    git("checkout", "--detach", business)
    assert shell(verify_checkout).returncode == 0
    assert lockfile.read_text() == "business lock\n"
    lockfile.unlink()
    assert shell(verify_checkout).returncode != 0

# Both jobs must keep the active source pin and skip image cache on the compatibility path.
with tempfile.TemporaryDirectory(prefix="package-web-rustup-") as directory:
    temp = Path(directory)
    image_home = temp / "image-rustup"
    (image_home / "toolchains/image-default").mkdir(parents=True)
    (image_home / "settings.toml").write_text('default_toolchain = "image-default"\n')
    binary_dir = temp / "bin"
    binary_dir.mkdir()
    rustup = binary_dir / "rustup"
    rustup.write_text(textwrap.dedent("""\
        #!/bin/bash
        set -euo pipefail
        if [[ "$*" == "show active-toolchain" ]]; then
          if [[ -z "${RUSTUP_AUTO_INSTALL:-}" && "$RUSTUP_HOME" == "$IMAGE_HOME" ]]; then exit 88; fi
          if [[ "$HISTORICAL" == no || -f "$RUSTUP_HOME/legacy-installed" ]]; then exit 0; fi
          exit 1
        fi
        [[ "$HISTORICAL" == yes && "$RUSTUP_HOME" != "$IMAGE_HOME" ]]
        [[ "$*" == "toolchain install --no-self-update --no-update --profile default --target wasm32-unknown-unknown" ]]
        test -L "$RUSTUP_HOME/toolchains/image-default"
        cmp "$IMAGE_HOME/settings.toml" "$RUSTUP_HOME/settings.toml"
        [[ "$INSTALL_FAIL" == no ]] || exit 19
        touch "$RUSTUP_HOME/legacy-installed"
        """))
    rustup.chmod(0o755)
    for historical, install_fail in (("no", "no"), ("yes", "no"), ("yes", "yes")):
        github_env, github_output = temp / "env", temp / "out"
        github_env.write_text("")
        github_output.write_text("")
        env = dict(os.environ, PATH=str(binary_dir) + os.pathsep + os.environ["PATH"],
                   RUSTUP_HOME=str(image_home), IMAGE_HOME=str(image_home), RUNNER_TEMP=str(temp),
                   HISTORICAL=historical, INSTALL_FAIL=install_fail, GITHUB_ENV=str(github_env),
                   GITHUB_OUTPUT=str(github_output))
        result = subprocess.run(["bash", "-e", "-o", "pipefail", "-c", legacy_toolchain], cwd=temp,
                                env=env, capture_output=True, text=True)
        assert (result.returncode == 0) == (install_fail == "no"), (historical, install_fail, result)
        assert github_output.read_text() == ("image_toolchain=true\n" if historical == "no" else "")
        receipt = github_env.read_text()
        if historical == "yes" and install_fail == "no":
            task_home = Path(receipt.strip().removeprefix("RUSTUP_HOME="))
            assert task_home.parent == temp and (task_home / "legacy-installed").is_file()
        else:
            assert not receipt
        assert list((image_home / "toolchains").iterdir()) == [image_home / "toolchains/image-default"]

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

# Exercise the real Testnet shell using Trunk's metadata protocol and real offline Cargo.
with tempfile.TemporaryDirectory(prefix="testnet-trunk-lock-") as directory:
    fixture = Path(directory)
    launcher = fixture / "crates/oasis7_client_launcher"
    cargo = shutil.which("cargo")
    assert cargo, "Testnet Launcher contract requires Cargo"
    for name, package in ((launcher, "launcher"), (launcher / "aaa", "aaa"),
                          (launcher / "bbb", "bbb"), (launcher / "ccc", "ccc")):
        (name / "src").mkdir(parents=True)
        (name / "src/lib.rs").touch()
        (name / "Cargo.toml").write_text(
            f'[package]\nname = "{package}"\nversion = "0.1.0"\nedition = "2021"\n'
        )
    manifest = launcher / "Cargo.toml"
    manifest.write_text(manifest.read_text() + '[dependencies]\naaa = {path = "aaa"}\nbbb = {path = "bbb"}\n')
    subprocess.run([cargo, "generate-lockfile", "--offline"], cwd=launcher, check=True, capture_output=True)
    lock = launcher / "Cargo.lock"
    canonical = lock.read_bytes()
    source_lock = canonical.replace(b' "aaa",\n "bbb",', b' "bbb",\n "aaa",')
    assert source_lock != canonical
    lock.write_bytes(source_lock)

    (fixture / "scripts").mkdir()
    for name in ("build-viewer-software-safe", "copy-viewer-web-dist", "package-viewer-web-delivery"):
        script = fixture / "scripts" / (name + ".sh")
        script.write_text("#!/bin/bash\nexit 0\n")
        script.chmod(0o755)
    bin_dir = fixture / "bin"
    bin_dir.mkdir()
    trunk = bin_dir / "trunk"
    trunk.write_text(
        '#!/bin/bash\nset -euo pipefail\n'
        '"${CARGO:-cargo}" metadata --offline --format-version 1 --manifest-path Cargo.toml > /dev/null\n'
        '[[ "$*" == "build --release --locked --dist ../../output/testnet-packages/web-launcher-dist" ]]\n'
        'touch "$TRUNK_BUILT"\n'
    )
    trunk.chmod(0o755)
    temp = fixture / "runner temp"
    temp.mkdir()
    built = fixture / "launcher-built"
    env = dict(os.environ, PATH=str(bin_dir) + os.pathsep + os.environ["PATH"],
               RUNNER_TEMP=str(temp), TRUNK_BUILT=str(built))
    env.pop("CARGO", None)
    command = ["bash", "-e", "-o", "pipefail", "-c", run(step(testnet_web, "Build web dists"))]
    result = subprocess.run(command, cwd=fixture, env=env, capture_output=True, text=True)
    assert lock.read_bytes() == source_lock, "Launcher Trunk metadata modified the planned lockfile"
    assert result.returncode == 0 and built.is_file(), result.stderr
    assert not list(temp.iterdir()), "Launcher Cargo adapter must be temporary"

    built.unlink()
    manifest.write_text(manifest.read_text() + 'ccc = {path = "ccc"}\n')
    result = subprocess.run(command, cwd=fixture, env=env, capture_output=True, text=True)
    assert result.returncode != 0 and "--locked" in result.stderr, result.stderr
    assert not built.exists(), "Trunk must stop before build when metadata needs a different lock"
    assert lock.read_bytes() == source_lock and not list(temp.iterdir())

print(f"ok: Release and Testnet restore the shared archive and always verify Trunk {version}; source preflight and failure paths pass")
PY
