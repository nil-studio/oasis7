"""Disposable same-run Testnet package consumer, isolated install and loopback startup."""
import hashlib
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import sys
import tarfile
import textwrap
import time
import urllib.error
import urllib.parse
import urllib.request
import zipfile

ROOT = Path.cwd()
SOURCE = ROOT / "frozen-business-source"
RESULT = Path(os.environ["RUNNER_TEMP"]).resolve() / "testnet-linux-installer-probe"
REPOSITORY = "https://api.github.com/repos/nil-studio/oasis7"
VERSION = "0.0.0+testnet.352.cc84d8a8552b"


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
    headers = {"Authorization": "Bearer " + os.environ["ARTIFACT_READ_TOKEN"],
               "Accept": "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28"}
    artifacts = (
        ("testnet-final", 12186057513, "testnet-package-linux-x64-0.0.0+testnet.352.cc84d8a8552b", 39238960713,
         "TESTNET_SOURCE", "a9e9497d1b296116d8db4baaa0866adfe8547e6353076e3a358868074cccac0a", 150039878),
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


def elf(path):
    with path.open("rb") as stream:
        header = stream.read(20)
    assert header[:7] == b"\x7fELF\x02\x01\x01" and header[18:20] == b"\x3e\x00", path.name


def cold_start(prefix):
    bundle = prefix / "opt/oasis7"
    with socket.socket() as temporary:
        temporary.bind(("127.0.0.1", 0))
        port = temporary.getsockname()[1]
    home = RESULT / "cold-home"
    home.mkdir()
    clean = {"PATH": os.environ["PATH"], "HOME": str(home), "XDG_CONFIG_HOME": str(home / "config"),
             "XDG_DATA_HOME": str(home / "data"), "XDG_CACHE_HOME": str(home / "cache")}
    with (RESULT / "cold.log").open("w") as logs:
        process = subprocess.Popen([str(bundle / "run-web-launcher.sh"), "--listen-bind",
                                    f"127.0.0.1:{port}", "--no-open-browser"],
                                   cwd=home, env=clean, stdout=logs, stderr=logs, start_new_session=True)
        try:
            request = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                assert process.poll() is None, (RESULT / "cold.log").read_text()
                try:
                    with request.open(f"http://127.0.0.1:{port}/", timeout=1) as response:
                        assert response.status == 200
                        assert response.read() == (bundle / "web-launcher/index.html").read_bytes()
                        with request.open(f"http://127.0.0.1:{port}/healthz", timeout=1) as health:
                            assert health.status == 200 and health.read() == b"ok"
                        print("installed entrypoint cold-started and served the packaged loopback index byte-for-byte")
                        return
                except urllib.error.URLError:
                    time.sleep(0.1)
            raise AssertionError("installed loopback launcher did not become ready")
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGTERM)
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()


def replay():
    assert "ARTIFACT_READ_TOKEN" not in os.environ and os.getuid() == os.getgid() == 1001
    historical = RESULT / "historical"
    extract_zip("testnet-final", historical)
    run("sha256sum", "-c", "linux-x64-SHA256SUMS", cwd=historical)
    plan = (historical / "linux-x64-SOURCEPLAN.json").read_text().strip()
    source = os.environ["TESTNET_SOURCE"]
    assert json.loads(plan)["checkoutSha"] == source
    workflow = run("git", "show", f"{os.environ['CANDIDATE']}:.github/workflows/testnet-packages.yml")
    job = workflow.split("  package-installers:\n", 1)[1]
    environment = os.environ | {
        "GITHUB_WORKSPACE": str(SOURCE), "GITHUB_RUN_ID": "39238960713", "GITHUB_RUN_NUMBER": "352",
        "GITHUB_REPOSITORY": "nil-studio/oasis7", "RUNNER_OS": "Linux", "SOURCE_PLAN": plan,
        "REQUESTED_REF": source, "GITHUB_OUTPUT": str(RESULT / "version"),
    }
    values = {
        "needs.plan.outputs.checkoutSha": source, "matrix.platform": "linux-x64", "matrix.target_triple": "native",
        "matrix.asset_name": "oasis7-linux-x64.deb", "inputs.build_profile": "packaging",
        "inputs.package_scope": "linux_only", "steps.package_version.outputs.value": VERSION,
    }

    def step(title, updates=None):
        body = job.split(f"      - name: {title}\n", 1)[1].split("\n      - ", 1)[0]
        _, separator, script = body.partition("        run: |\n")
        if not separator:
            script = next(line.removeprefix("        run: ") for line in body.splitlines()
                          if line.startswith("        run: "))
        for name, value in values.items():
            script = script.replace("${{ " + name + " }}", value)
        assert "${{" not in script, title
        print(run("bash", "-e", "-o", "pipefail", "-c", textwrap.dedent(script), cwd=SOURCE,
                  env=environment | (updates or {})), end="")

    step("Verify the Linux packaging runtime")
    step("Verify planned source HEAD", {"EXPECTED_SHA": source})
    output = SOURCE / "output"
    extract_zip("testnet-native", output / "native-artifact")
    extract_zip("testnet-web", output / "testnet-packages")
    with tarfile.open(output / "native-artifact/native-stage.tar.gz", "r:gz") as native:
        for member in native:
            p = Path(member.name)
            assert not p.is_absolute() and ".." not in p.parts and (member.isfile() or member.isdir())
    for name in ("Extract native stage", "Resolve package version", "Assemble launcher bundle without compiling",
                 "Archive operator tools", "Validate bundle entrypoints", "Stage bundle manifest",
                 "Write BUILDINFO", "Package native installer", "Generate checksums", "Generate ops package checksum"):
        step(name)
    assert (RESULT / "version").read_text() == f"value={VERSION}\n"
    assets = output / "testnet-packages/assets"
    for name in ("linux-x64-SOURCEPLAN.json", "linux-x64-BUILDINFO"):
        assert (assets / name).read_bytes() == (historical / name).read_bytes(), name
    manifest_name = "linux-x64-bundle-manifest.json"
    original_manifest = json.loads((historical / manifest_name).read_text())
    replayed_manifest = json.loads((assets / manifest_name).read_text())
    for manifest in (original_manifest, replayed_manifest):
        assert type(manifest.pop("generatedAtUnixMs")) is int
        assert type(manifest.pop("sourceLatestMtimeNs")) is int
        latest = manifest.pop("sourceLatestPath")
        assert any(latest == path or latest.startswith(path + "/") for path in manifest["sourceScope"])
    assert original_manifest == replayed_manifest
    run("sha256sum", "-c", "linux-x64-SHA256SUMS", cwd=assets)
    run("sha256sum", "-c", "linux-x64-ops-tools-SHA256SUMS", cwd=assets)

    archive = assets / values["matrix.asset_name"]
    assert run("dpkg-deb", "-f", str(archive)) == run("dpkg-deb", "-f", str(historical / archive.name))
    control = RESULT / "control"
    run("dpkg-deb", "-e", str(archive), str(control))
    assert {p.name for p in control.iterdir()} == {"control"}, "unexpected maintainer scripts"
    prefix = RESULT / "installed"
    database = prefix / "var/lib/dpkg"
    database.mkdir(parents=True)
    (database / "status").touch()
    run("dpkg", "--force-not-root", f"--root={prefix}", "--install", str(archive))
    assert run("dpkg-query", f"--admindir={database}", "-W", "-f=${Status}|${Architecture}", "oasis7") == "install ok installed|amd64"
    installed = prefix / "opt/oasis7"
    assert (installed / "BUILDINFO").read_bytes() == (historical / "linux-x64-BUILDINFO").read_bytes()
    legacy_root = RESULT / "historical-payload"
    run("dpkg-deb", "-x", str(historical / archive.name), str(legacy_root))
    original_checksums = (legacy_root / "opt/oasis7/SHA256SUMS").read_text().splitlines()
    installed_checksums = (installed / "SHA256SUMS").read_text().splitlines()
    def payload_files(lines):
        files = dict(line.split("  ", 1)[::-1] for line in lines)
        assert len(files) == len(lines) and ".oasis7-bundle-manifest.json" in files
        del files[".oasis7-bundle-manifest.json"]
        return files
    assert payload_files(original_checksums) == payload_files(installed_checksums)
    run("sha256sum", "-c", "SHA256SUMS", cwd=installed)
    stage = output / "native-stage"
    receipt = json.loads((stage / "native-build.json").read_text())
    assert (receipt["source"], receipt["profile"], receipt["target"], receipt["platform"]) == (source, "packaging", "native", "linux-x64")
    assert len(receipt["files"]) == 9
    for name, expected in receipt["files"].items():
        item = assets / "oasis7-linux-x64-ops-tools" / name.removeprefix("ops/") if name.startswith("ops/") else installed / name
        assert sha256(item) == expected["sha256"] and item.stat().st_mode & 0o777 == expected["mode"]
        if not name.endswith("service-readback"):
            elf(item)
            assert "not found" not in run("ldd", str(item)), name
    print("isolated dpkg database confirms installed amd64; original BUILDINFO and bundle hashes match; 8 ELF/9 stage files verified")
    cold_start(prefix)

    verify = ("python3", "scripts/package-native-stage.py", "verify", str(stage))
    bad_source = subprocess.run((*verify, str(ROOT), "packaging", "native", "linux-x64", "1"), cwd=SOURCE, capture_output=True, text=True)
    assert bad_source.returncode and "native build identity or bytes mismatch" in bad_source.stderr
    wrong_binary = stage / "bin/oasis7_web_launcher"
    with wrong_binary.open("r+b") as foreign:
        foreign.seek(18)
        foreign.write(b"\xb7\x00")
    try:
        elf(wrong_binary)
    except AssertionError:
        pass
    else:
        raise AssertionError("foreign ABI was accepted")
    bad_hash = subprocess.run((*verify, str(SOURCE), "packaging", "native", "linux-x64", "1"), cwd=SOURCE, capture_output=True, text=True)
    assert bad_hash.returncode and "native build identity or bytes mismatch" in bad_hash.stderr
    print("foreign ELF/header and altered native hash rejected; wrong checkout identity rejected")


if __name__ == "__main__":
    {"capture": capture, "replay": replay}[sys.argv[1]]()
