#!/usr/bin/env bash
set -euo pipefail

# Exercise the real wrapper with an isolated, fail-closed Docker model. No daemon,
# Cargo, toolchain, or shared cache is used. Barriers model image publication,
# not timing sleeps or a command that disappears when the defect is repaired.
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
python3 - "$ROOT_DIR" <<'PY'
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import unittest

EXPECTED = "sha256:f3538a8039074c0b074946ae9d1803f6c74b823aee9152d52584111844ee1ed8"
REBUILT = "sha256:" + "a" * 64
OTHER = "sha256:" + "b" * 64
TAG = "oasis7/wasm-builder:nightly-2025-12-11"

FAKE_DOCKER = r'''#!/usr/bin/env python3
import json, os, pathlib, sys, time, uuid
root = pathlib.Path(os.environ["FIXTURE_STATE"])
args = sys.argv[1:]
role = os.environ.get("FIXTURE_ROLE", "single")
(root / ("call-" + uuid.uuid4().hex + ".json")).write_text(json.dumps({"role":role,"args":args}))
tag = "oasis7/wasm-builder:nightly-2025-12-11"
rebuilt = "sha256:" + "a" * 64
def fail(message, code=97):
    print("fixture: " + message, file=sys.stderr)
    sys.exit(code)
def wait(name):
    deadline = time.monotonic() + 10
    while not (root / name).exists():
        if time.monotonic() >= deadline:
            fail("barrier timeout " + name, 98)
        time.sleep(.01)
def publish(image):
    (root / ("image-" + image.replace(":", "-"))).touch()
    replacement = root / ("tag-" + uuid.uuid4().hex)
    replacement.write_text(image)
    replacement.replace(root / "tag")
if args == ["info"]:
    sys.exit(0)
if args[:2] == ["image", "inspect"]:
    if len(args) == 3:
        ref = args[2]
        if not (root / ("image-" + ref.replace(":", "-"))).exists():
            fail("candidate must inspect immutable build IID: " + ref)
        actual = "sha256:" + "b" * 64 if os.environ.get("FIXTURE_WRONG_INSPECT") == "1" else ref
        print(json.dumps([{"Id":actual,"Os":"linux","Architecture":"amd64","Config":{"Labels":{"io.oasis7.candidate.purpose":"candidate-build","io.oasis7.candidate.producer-recipe":"c"*64}}}]))
        sys.exit(0)
    if len(args) != 5 or args[3:] != ["--format", "{{.Id}}"]:
        fail("unexpected inspect argv " + repr(args))
    ref = args[2]
    if ref == tag or ref == "fixture/custom":
        if not (root / "tag").exists():
            print("No such image: " + ref, file=sys.stderr)
            sys.exit(1)
        print((root / "tag").read_text())
    elif (root / ("image-" + ref.replace(":", "-"))).exists():
        print(ref)
    else:
        fail("unexpected image ref " + ref)
    sys.exit(0)
if args == ["image", "rm", "-f", tag]:
    if (root / "tag").exists():
        old = (root / "tag").read_text()
        (root / "tag").unlink()
        (root / ("image-" + old.replace(":", "-"))).unlink(missing_ok=True)
    sys.exit(0)
if args == ["buildx", "version"]:
    sys.exit(1 if os.environ.get("FIXTURE_LEGACY") == "1" else 0)
if args[:2] == ["buildx", "build"] or args[:1] == ["build"]:
    if "--iidfile" in args:
        if args[args.index("--tag")+1] != "oasis7/wasm-builder:candidate-" + "c"*64:
            fail("wrong candidate tag")
        publish(rebuilt)
        pathlib.Path(args[args.index("--iidfile")+1]).write_text(rebuilt)
        # Retarget immediately on build return, before inspection can occur.
        publish("sha256:" + "b"*64)
        sys.exit(0)
    if "--tag" not in args or args[args.index("--tag") + 1] != tag or "--file" not in args:
        fail("unexpected build argv " + repr(args))
    if role == "B":
        (root / "B-publication-ready").touch()
        wait("B-publish")
    publish(rebuilt)
    sys.exit(0)
if args[:1] == ["run"]:
    # Every option in the production wrapper has one value except --rm.
    i = 1
    allowed = {"--platform", "--user", "--workdir", "--mount", "--env"}
    env = {}
    while i < len(args) and args[i].startswith("--"):
        if args[i] == "--rm":
            i += 1
        elif args[i] in allowed and i + 1 < len(args):
            if args[i] == "--env":
                key, _, value = args[i+1].partition("=")
                env[key] = value
            i += 2
        else:
            fail("unexpected run option " + repr(args[i:]))
    if i >= len(args):
        fail("missing run image")
    ref = args[i]
    (root / (role + "-run.json")).write_text(json.dumps({"image":ref,"env":env,"payload":args[i+1:]}))
    if role == "A":
        (root / "A-selected").touch()
        wait("A-run")
    if os.environ.get("FIXTURE_RETAG") == "1":
        publish(rebuilt)
    if ref == tag or ref == "fixture/custom":
        if not (root / "tag").exists():
            print("docker: Unable to find image locally; pull access denied", file=sys.stderr)
            sys.exit(125)
    elif not (root / ("image-" + ref.replace(":", "-"))).exists():
        print("docker: No such image: " + ref, file=sys.stderr)
        sys.exit(125)
    sys.exit(0)
fail("unexpected Docker command " + repr(args))
'''


FAKE_CARGO = r'''#!/usr/bin/env python3
import json, pathlib, sys
args = sys.argv[1:]
args = args[args.index("--")+1:]
if args[0] == "producer-recipe":
    pathlib.Path(args[2]).write_text(json.dumps({"producer_recipe_hash":"c"*64}))
elif args[0] == "candidate-builder-evidence":
    image = json.loads(pathlib.Path(args[2]).read_text())[0]
    pathlib.Path(args[3]).write_text(json.dumps({"builder_execution":{"image_id":image["Id"]}}))
else:
    raise SystemExit("unexpected recipe utility command")
'''


class BuilderLifecycleTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="wasm-builder-lifecycle-")
        # Match the wrapper's pwd -P spelling (macOS /var is /private/var).
        self.root = Path(self.temp.name).resolve()
        (self.root / "scripts").mkdir()
        self.script = self.root / "scripts/build-wasm-module.sh"
        shutil.copyfile(Path(sys.argv[1]) / "scripts/build-wasm-module.sh", self.script)
        (self.root / "target").mkdir()
        (self.root / "docker/wasm-builder").mkdir(parents=True)
        (self.root / "docker/wasm-builder/Dockerfile").write_text("FROM fixture-only\n")
        self.manifest = self.root / "module/Cargo.toml"
        self.manifest.parent.mkdir()
        self.manifest.write_text("# fake Docker never compiles this manifest\n")
        self.state = self.root / "state"
        self.state.mkdir()
        self.bin = self.root / "bin"
        self.bin.mkdir()
        docker = self.bin / "docker"
        docker.write_text(FAKE_DOCKER)
        docker.chmod(0o755)
        self.workers = []
        self.publish(EXPECTED)

    def test_candidate_uses_its_build_iid_even_when_task_tag_is_retargeted(self):
        cargo = self.bin / "cargo"
        cargo.write_text(FAKE_CARGO)
        cargo.chmod(0o755)
        for legacy in ("0", "1"):
            for wrong_inspect in ("0", "1"):
                out = self.root / ("candidate-" + legacy + wrong_inspect)
                result = subprocess.run(["bash", str(self.script), "--candidate-build", "--manifest-path", str(self.manifest), "--out-dir", str(out), "--module-id", "fixture.module"], cwd=self.root, env=self.env(FIXTURE_LEGACY=legacy, FIXTURE_WRONG_INSPECT=wrong_inspect, CARGO_BUILD_JOBS="7"), capture_output=True, text=True, timeout=15)
                if wrong_inspect == "1":
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn("inspection changed", result.stderr)
                else:
                    self.assertEqual(result.returncode, 0, result.stderr)
                    run = json.loads((self.state / "single-run.json").read_text())
                    self.assertEqual(run["image"], REBUILT)
                    self.assertNotIn("OASIS7_WASM_BUILDER_IMAGE_DIGEST", run["env"])
                    self.assertIn("OASIS7_WASM_CANDIDATE_BUILDER_EVIDENCE", run["env"])
                    self.assertEqual(run["env"].get("CARGO_BUILD_JOBS"), "2")
                self.assertEqual(list(out.glob(".candidate-builder.*")), [])

    def test_candidate_output_symlink_is_rejected_before_any_external_write(self):
        cargo = self.bin / "cargo"
        cargo.write_text(FAKE_CARGO)
        cargo.chmod(0o755)
        external = tempfile.TemporaryDirectory(prefix="wasm-candidate-outside-")
        self.addCleanup(external.cleanup)
        outside = Path(external.name).resolve()
        (outside / "marker").write_text("unchanged")
        (self.root / "linked-output").symlink_to(outside, target_is_directory=True)
        result = subprocess.run(["bash", str(self.script), "--candidate-build", "--manifest-path", str(self.manifest), "--out-dir", str(self.root / "linked-output/new-output"), "--module-id", "fixture.module"], cwd=self.root, env=self.env(), capture_output=True, text=True, timeout=15)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("candidate output ancestor", result.stderr)
        self.assertEqual((outside / "marker").read_text(), "unchanged")
        self.assertEqual(sorted(p.name for p in outside.iterdir()), ["marker"])
        calls = [json.loads(p.read_text())["args"] for p in self.state.glob("call-*.json")]
        self.assertFalse(any(args[:1] == ["build"] or args[:2] == ["buildx", "build"] for args in calls))

    def tearDown(self):
        # Release fixture-owned barriers first; never kill unrelated processes.
        for name in ("A-run", "B-publish"):
            (self.state / name).touch()
        for worker in self.workers:
            if worker.poll() is None:
                try:
                    worker.communicate(timeout=12)
                except subprocess.TimeoutExpired:
                    worker.kill()
                    worker.communicate()
        self.temp.cleanup()

    def publish(self, image):
        (self.state / "tag").write_text(image)
        (self.state / ("image-" + image.replace(":", "-"))).touch()

    def env(self, **overrides):
        env = {key: value for key, value in os.environ.items()
               if not key.startswith(("OASIS7_WASM_", "FIXTURE_"))}
        env.update(PATH=str(self.bin) + os.pathsep + env.get("PATH", ""),
                   FIXTURE_STATE=str(self.state))
        env.update(overrides)
        return env

    def start(self, role="single", **overrides):
        worker = subprocess.Popen(["bash", str(self.script), "--manifest-path",
                                   str(self.manifest), "--out-dir", str(self.root / ("out-" + role)),
                                   "--module-id", "fixture.module", "--profile", "release"],
                                  cwd=self.root, env=self.env(FIXTURE_ROLE=role, **overrides),
                                  stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.workers.append(worker)
        return worker

    def finish(self, worker):
        stdout, stderr = worker.communicate(timeout=15)
        self.assertNotIn("fixture:", stderr, "fixture failure is not a defect witness: " + stderr)
        return worker.returncode, stdout, stderr

    def wait(self, name, worker):
        deadline = time.monotonic() + 10
        while not (self.state / name).exists():
            if worker.poll() is not None:
                self.fail("fixture barrier not reached: " + repr(self.finish(worker)))
            if time.monotonic() >= deadline:
                self.fail("fixture barrier timeout: " + name)
            time.sleep(.01)

    def calls(self):
        return [json.loads(path.read_text()) for path in self.state.glob("call-*.json")]

    def run_record(self, role="single"):
        return json.loads((self.state / (role + "-run.json")).read_text())

    def assert_receipt(self, record, digest, ref=TAG):
        self.assertEqual(record["env"]["OASIS7_WASM_BUILDER_IMAGE_REF"], ref)
        self.assertEqual(record["env"]["OASIS7_WASM_BUILDER_IMAGE_DIGEST"], digest)
        self.assertEqual(record["env"]["OASIS7_WASM_CANONICAL_CONTAINER_PLATFORM"], "linux-x86_64")

    def test_container_selects_canonical_toolchain_before_first_rustup_query(self):
        (self.root / "rust-toolchain.toml").write_text('[toolchain]\nchannel = "1.96.0"\n')
        rustup = self.bin / "rustup"
        rustup.write_text('''#!/usr/bin/env python3
import json, os, pathlib, sys
with (pathlib.Path(os.environ["FIXTURE_STATE"]) / "rustup-calls").open("a") as f:
    f.write(json.dumps({"args": sys.argv[1:], "toolchain": os.environ.get("RUSTUP_TOOLCHAIN")}) + "\\n")
if os.environ.get("RUSTUP_TOOLCHAIN") != "nightly-2025-12-11":
    print("workspace override selected before canonical toolchain", file=sys.stderr)
    raise SystemExit(1)
if sys.argv[1:3] == ["toolchain", "list"]:
    print("nightly-2025-12-11-x86_64-unknown-linux-gnu (active, default)")
elif sys.argv[1:3] == ["target", "list"]:
    print("wasm32-unknown-unknown")
else:
    raise SystemExit(1)
''')
        rustup.chmod(0o755)
        rustc = self.bin / "rustc"
        rustc.write_text('#!/bin/sh\n[ "$1" = --print ] && [ "$2" = sysroot ] || exit 1\nprintf "%s\\n" /fixture/sysroot\n')
        rustc.chmod(0o755)
        suite = self.bin / "suite"
        suite.write_text('#!/bin/sh\n[ "$1" = build ] || exit 1\nprintf "%s\\n" public-suite-complete\n')
        suite.chmod(0o755)
        env = self.env(OASIS7_WASM_BUILD_IN_CONTAINER="1",
                       OASIS7_WASM_BUILD_SUITE_BIN=str(suite),
                       RUSTUP_HOME=str(self.root / "rustup"))
        for key in ("RUSTUP_TOOLCHAIN", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS",
                    "CARGO_BUILD_RUSTFLAGS", "CARGO_TARGET_DIR", "RUSTC_BOOTSTRAP"):
            env.pop(key, None)
        completed = subprocess.run(["bash", str(self.script)], cwd=self.root, env=env,
                                   capture_output=True, text=True, timeout=15)
        self.assertEqual(completed.returncode, 0, completed.stderr)
        calls = [json.loads(line) for line in (self.state / "rustup-calls").read_text().splitlines()]
        self.assertEqual(calls[0]["args"], ["toolchain", "list"])
        self.assertTrue(all(call["toolchain"] == "nightly-2025-12-11" for call in calls))
        self.assertIn("public-suite-complete", completed.stdout)
        # A fixture copy without the early selection reproduces the bad query;
        # no real rustup, Docker, network, or compilation is involved.
        source = self.script.read_text()
        self.script.write_text(source.replace('  export RUSTUP_TOOLCHAIN="$WASM_TOOLCHAIN"\n', '', 1))
        rejected = subprocess.run(["bash", str(self.script)], cwd=self.root, env=env,
                                  capture_output=True, text=True, timeout=15)
        self.assertNotEqual(rejected.returncode, 0)
        self.assertIn("workspace override selected before canonical toolchain", rejected.stderr)
        self.assertNotIn("public-suite-complete", rejected.stdout)

    def test_concurrent_rebuild_keeps_selected_image_executable(self):
        a = self.start("A")
        self.wait("A-selected", a)
        b = self.start("B", OASIS7_WASM_BUILDER_IMAGE_DIGEST=OTHER)
        self.wait("B-publication-ready", b)
        (self.state / "A-run").touch()
        a_result = self.finish(a)
        (self.state / "B-publish").touch()
        b_result = self.finish(b)
        print("concurrent witness A=%r B=%r calls=%s" % (a_result, b_result, json.dumps(self.calls())), flush=True)
        self.assertEqual(a_result[0], 0, "selected image disappeared during sibling rebuild: " + a_result[2])
        self.assertEqual(b_result[0], 0, b_result[2])
        self.assertFalse(any(c["args"][:2] == ["image", "rm"] for c in self.calls()),
                         "rebuild must not delete another invocation's selected image")
        self.assertEqual(self.run_record("A")["image"], EXPECTED)
        self.assert_receipt(self.run_record("A"), EXPECTED)
        self.assert_receipt(self.run_record("B"), OTHER)

    def test_retag_after_selection_uses_immutable_id(self):
        result = self.finish(self.start(FIXTURE_RETAG="1"))
        record = self.run_record()
        print("retag witness result=%r record=%s" % (result, json.dumps(record)), flush=True)
        self.assertEqual(result[0], 0, result[2])
        self.assertEqual(record["image"], EXPECTED, "execution must use selected ID, not mutable tag")
        self.assert_receipt(record, EXPECTED)

    def test_exact_id_provenance(self):
        result = self.finish(self.start())
        self.assertEqual(result[0], 0, result[2])
        self.assert_receipt(self.run_record(), EXPECTED)
        run = next(c["args"] for c in self.calls() if c["args"][:1] == ["run"])
        self.assertEqual(sum(arg.startswith("type=bind,") for arg in run), 1)

        self.assertFalse(any(c["args"][:1] == ["build"] or c["args"][:2] == ["buildx", "build"] for c in self.calls()))

    def test_empty_recipe_digest_follows_selected_id(self):
        result = self.finish(self.start(OASIS7_WASM_BUILDER_IMAGE_DIGEST=""))
        self.assertEqual(result[0], 0, result[2])
        self.assert_receipt(self.run_record(), EXPECTED)

    def test_allowed_canonical_id_variance_preserves_recipe_digest(self):
        self.publish(OTHER)
        result = self.finish(self.start(OASIS7_WASM_BUILDER_ALLOW_IMAGE_ID_MISMATCH="1"))
        self.assertEqual(result[0], 0, result[2])
        self.assert_receipt(self.run_record(), EXPECTED)
        self.assertFalse(any(c["args"][:2] == ["image", "rm"] for c in self.calls()))

    def test_rebuild_buildx_preserves_recipe_digest(self):
        self.publish(OTHER)
        result = self.finish(self.start())
        self.assertEqual(result[0], 0, result[2])
        self.assert_receipt(self.run_record(), EXPECTED)
        self.assertTrue(any(c["args"][:2] == ["buildx", "build"] for c in self.calls()))

    def test_rebuild_legacy_preserves_recipe_digest(self):
        self.publish(OTHER)
        result = self.finish(self.start(FIXTURE_LEGACY="1"))
        self.assertEqual(result[0], 0, result[2])
        self.assert_receipt(self.run_record(), EXPECTED)
        self.assertTrue(any(c["args"][:1] == ["build"] for c in self.calls()))

    def test_custom_missing_or_mismatch_rejects_before_run(self):
        for missing in (False, True):
            with self.subTest(missing=missing):
                if missing:
                    (self.state / "tag").unlink()
                else:
                    self.publish(OTHER)
                result = self.finish(self.start(OASIS7_WASM_BUILDER_IMAGE="fixture/custom"))
                self.assertNotEqual(result[0], 0)
                self.assertIn("configured builder image", result[2])
                self.assertFalse((self.state / "single-run.json").exists())

    def test_autobuild_disabled_missing_or_mismatch_rejects_before_run(self):
        for missing in (False, True):
            with self.subTest(missing=missing):
                if missing:
                    (self.state / "tag").unlink()
                else:
                    self.publish(OTHER)
                result = self.finish(self.start(OASIS7_WASM_BUILDER_AUTO_BUILD="0"))
                self.assertNotEqual(result[0], 0)
                self.assertIn("canonical wasm builder image", result[2])
                self.assertFalse((self.state / "single-run.json").exists())


unittest.main(argv=[sys.argv[0]], verbosity=2)
PY
