"""Disposable fixed-image replay for the Release and Testnet Linux ABI/consumer boundaries."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tarfile
import textwrap
import urllib.error
import urllib.parse
import urllib.request
import zipfile

ROOT = Path.cwd()
RESULT = Path(os.environ["RUNNER_TEMP"]).resolve() / "linux-native-package-probe"
RELEASE = ROOT / "evidence-release"
TESTNET = ROOT / "evidence-testnet"
LEGACY = ROOT / "evidence-legacy"
REPOSITORY = "https://api.github.com/repos/nil-studio/oasis7"


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, fp, code, msg, headers, newurl):
        return None


def run(*args, cwd=ROOT, env=None):
    return subprocess.check_output(args, cwd=cwd, env=env, text=True)


def sha256(file):
    with file.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def capture():
    RESULT.mkdir(mode=0o700)
    opener = urllib.request.build_opener(NoRedirect())
    headers = {"Authorization": "Bearer " + os.environ["CACHELESS_ARTIFACT_READ_TOKEN"],
               "Accept": "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28"}
    artifacts = (
        ("release", 11942975095, "release-asset-linux-x64", 38659287921, "RELEASE_SOURCE",
         "3bfb4b2f94256922c9ff6f2e0d1ae32dd390f667e5991b23bc2fd5c53348e91a", 72808379),
        ("testnet-native", 12185013975, "testnet-native-stage-linux-x64", 39238960713, "TESTNET_SOURCE",
         "67979a1860382af641047d36411a5375ca5b3547b0b6ca4ac0ddaf64bd923656", 176947877),
        ("testnet-web", 12180692299, "testnet-web-dists", 39238960713, "TESTNET_SOURCE",
         "c7c162250d5cba979b4cb666b7d29edee924b3a62c3dde5c7af911e24eb02372", 11437867),
    )
    for label, artifact_id, artifact_name, run_id, source_key, digest, size in artifacts:
        url = f"{REPOSITORY}/actions/artifacts/{artifact_id}"
        with opener.open(urllib.request.Request(url, headers=headers), timeout=30) as response:
            artifact = json.load(response)
        assert (artifact["id"], artifact["name"], artifact["workflow_run"]["id"]) == (artifact_id, artifact_name, run_id)
        assert artifact["workflow_run"]["head_sha"] == os.environ[source_key] and not artifact["expired"]
        assert artifact["digest"] == "sha256:" + digest and artifact["size_in_bytes"] == size
        try:
            opener.open(urllib.request.Request(url + "/zip", headers=headers), timeout=30)
            raise AssertionError("GitHub artifact download must redirect")
        except urllib.error.HTTPError as error:
            assert error.code == 302
            location = error.headers["Location"]
        destination = urllib.parse.urlsplit(location)
        assert destination.scheme == "https" and destination.hostname and not destination.username and not destination.password
        archive = RESULT / f"{label}.zip"
        with opener.open(urllib.request.Request(location), timeout=90) as response, archive.open("xb") as output:
            remaining = size
            while remaining:
                chunk = response.read(min(1024 * 1024, remaining))
                assert chunk, "artifact shorter than authoritative size"
                output.write(chunk)
                remaining -= len(chunk)
            assert not response.read(1), "artifact larger than authoritative size"
        assert sha256(archive) == digest
        print(f"{label} {artifact_id} sha256:{digest}")


def extract_zip(label, target):
    with zipfile.ZipFile(RESULT / f"{label}.zip") as archive:
        names = archive.namelist()
        assert len(names) == len(set(names))
        for name in names:
            path = Path(name)
            assert not path.is_absolute() and ".." not in path.parts and "\\" not in name
        archive.extractall(target)


def prep(kind, source, *, historical=False):
    workflow = f".github/workflows/{kind}-packages.yml"
    body = run("git", "show", f"{os.environ['CANDIDATE']}:{workflow}")
    step = body.split("      - name: Verify the fixed Linux package toolchain\n", 1)[1].split("      - name:", 1)[0]
    script = textwrap.dedent(step.split("        run: |\n", 1)[1])
    assert "${{" not in script
    directory = RESULT / f"prepare-{kind}-{source.name}"
    directory.mkdir()
    output, command_env = directory / "output", directory / "env"
    output.touch()
    command_env.touch()
    image_home = Path(os.environ["RUSTUP_HOME"])
    image_tools = sorted(p.name for p in (image_home / "toolchains").iterdir())
    result = run("bash", "-c", script, cwd=source, env=os.environ | {
        "GITHUB_OUTPUT": str(output), "GITHUB_ENV": str(command_env), "RUNNER_TEMP": str(directory)})
    if historical:
        prefix = "RUSTUP_HOME="
        setting = command_env.read_text().strip()
        assert setting.startswith(prefix) and not output.read_text()
        custom_home = Path(setting.removeprefix(prefix))
        assert custom_home.parent == directory and custom_home.resolve(strict=True) == custom_home
        cargo = run("cargo", "--version", cwd=source, env=os.environ | {"RUSTUP_HOME": str(custom_home), "RUSTUP_AUTO_INSTALL": "0"})
        assert re.match(r"cargo 1\.92\.0 ", cargo), cargo
    else:
        assert output.read_text() == "image_toolchain=true\n" and not command_env.read_text()
        assert re.search(r"cargo 1\.96\.0 ", result)
    assert image_tools == sorted(p.name for p in (image_home / "toolchains").iterdir())
    run("git", "diff", "--exit-code", "--", "Cargo.lock", cwd=source)
    print(kind, source.name, result.splitlines()[0], "no image toolchain write")


def elf(path):
    identity = run("readelf", "-h", str(path))
    assert "ELF64" in identity and "Advanced Micro Devices X86-64" in identity


def native_abi():
    fixture = RESULT / "abi.rs"
    fixture.write_text("fn main() { println!(\"native-image-abi-ok\"); }\n")
    binary = RESULT / "abi-smoke"
    linker = os.environ["CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER"]
    run("rustc", "-C", f"linker={linker}", str(fixture), "-o", str(binary))
    elf(binary)
    assert "mold 3.0.0" in run("readelf", "-p", ".comment", str(binary))
    assert run(str(binary)).strip() == "native-image-abi-ok"
    assert "not found" not in run("ldd", str(binary))
    print("actual cargo image ABI: ELF64/x86-64/mold 3.0.0/runs")


def release_replay():
    bundle_root = RESULT / "release-asset"
    extract_zip("release", bundle_root)
    source_plan = json.loads((bundle_root / "linux-x64-SOURCEPLAN.json").read_text())
    assert source_plan["checkoutSha"] == source_plan["workflowSha"] == os.environ["RELEASE_SOURCE"]
    assert source_plan["resolvedRef"] == "refs/tags/2026-10-15_v1" and source_plan["repository"] == "eng-cc/oasis7"
    assert source_plan["tagObjectSha"] == "0fa9d444bbb651b4fb48dbf4b41a84dbcfce597f"
    package = bundle_root / "oasis7-linux-x64.deb"
    data = RESULT / "release-payload"
    run("dpkg-deb", "--extract", str(package), str(data))
    original = data / "opt/oasis7"
    assert (original / "BUILDINFO").read_bytes() == (bundle_root / "linux-x64-BUILDINFO").read_bytes()
    assert f"commit={os.environ['RELEASE_SOURCE']}\n" in (original / "BUILDINFO").read_text()
    assert "build_profile=packaging\n" in (original / "BUILDINFO").read_text()
    run("sha256sum", "-c", "SHA256SUMS", cwd=original)
    for binary in (original / "bin").iterdir():
        if binary.name.startswith("oasis7_"):
            elf(binary)
    repackage = RESULT / "release-repackaged"
    run("./scripts/package-native-installer.sh", "--platform", "linux-x64", "--bundle-dir", str(original),
        "--out-dir", str(repackage), "--asset-name", package.name, "--version", source_plan["version"], cwd=RELEASE)
    rebuilt = repackage / package.name
    assert run("dpkg-deb", "-f", str(package)) == run("dpkg-deb", "-f", str(rebuilt))
    next_data = RESULT / "release-repackaged-data"
    run("dpkg-deb", "--extract", str(rebuilt), str(next_data))
    expected = next_data / "opt/oasis7"
    assert (expected / "SHA256SUMS").read_bytes() == (original / "SHA256SUMS").read_bytes()
    run("sha256sum", "-c", "SHA256SUMS", cwd=expected)
    print("historical Release SOURCEPLAN/BUILDINFO/ELF and actual dpkg repackaged contents passed")


def testnet_replay():
    probe = RESULT / "testnet"
    extract_zip("testnet-native", probe)
    extract_zip("testnet-web", probe)
    stage = probe / "native"
    stage.mkdir()
    with tarfile.open(probe / "native-stage.tar.gz", "r:gz") as archive:
        assert all(m.isdir() or m.isfile() for m in archive.getmembers())
        archive.extractall(stage, filter="data")
    receipt = json.loads((stage / "native-build.json").read_text())
    assert receipt["source"] == os.environ["TESTNET_SOURCE"]
    assert receipt["profile"] == "packaging" and receipt["platform"] == "linux-x64" and len(receipt["files"]) == 9
    for name in receipt["files"]:
        if name.rsplit("/", 1)[1].startswith("oasis7_"):
            elf(stage / name)
    verify = ("python3", "./scripts/package-native-stage.py", "verify", str(stage))
    run(*verify, str(TESTNET), "packaging", "native", "linux-x64", "1", cwd=TESTNET)
    output = probe / "assembled"
    ops = probe / "ops"
    run("./scripts/build-game-launcher-bundle.sh", "--platform", "linux-x64", "--target-triple", "native",
        "--native-binaries", str(stage), "--web-dist", str(probe / "web-dist"), "--web-launcher-dist",
        str(probe / "web-launcher-dist"), "--out-dir", str(output), "--ops-out-dir", str(ops),
        "--profile", "packaging", cwd=TESTNET)
    run("./scripts/validate-release-platform-entrypoints.sh", "--platform", "linux-x64", "--bundle-dir", str(output), cwd=TESTNET)
    for name, value in receipt["files"].items():
        path = probe / name if name.startswith("ops/") else output / name
        assert sha256(path) == value["sha256"] and path.stat().st_mode & 0o777 == value["mode"]

    def reject(source, profile):
        result = subprocess.run((*verify, str(source), profile, "native", "linux-x64", "1"), cwd=TESTNET,
                                capture_output=True, text=True)
        assert result.returncode and "native build identity or bytes mismatch" in result.stderr

    reject(ROOT, "packaging")
    reject(TESTNET, "dev")
    with (stage / "ops/bin/service-readback").open("ab") as output_file:
        output_file.write(b"\n")
    reject(TESTNET, "packaging")
    print("historical Testnet exact native stage and web consumed without compiling; all 9 modes/hashes preserved; wrong source, profile and bytes rejected")


def replay():
    assert "CACHELESS_ARTIFACT_READ_TOKEN" not in os.environ and os.getuid() == os.getgid() == 1001
    prep("release", RELEASE)
    prep("testnet", TESTNET)
    prep("release", LEGACY, historical=True)
    native_abi()
    release_replay()
    testnet_replay()


if __name__ == "__main__":
    {"capture": capture, "replay": replay}[sys.argv[1]]()
