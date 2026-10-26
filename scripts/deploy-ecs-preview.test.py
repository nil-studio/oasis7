#!/usr/bin/env python3
"""Local provenance, stream and one-SSH regression tests; no live host is contacted."""
import hashlib
import io
import json
import os
from pathlib import Path
import struct
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import ecs_preview_deploy as deploy

SOURCE = "a" * 40


def archive_fixture(directory, *, extra=None):
    root = directory / "tree"
    root.mkdir()
    for binary in deploy.BINS:
        path = root / "bin" / binary
        path.parent.mkdir(exist_ok=True)
        path.write_text("#!/bin/sh\nexit 0\n")
        path.chmod(0o755)
    (root / "web-dist").mkdir()
    (root / "web-dist/index.html").write_text("viewer fixture")
    (root / "deployment").mkdir()
    (root / "deployment/ecs-skill-release.py").write_text("# skill fixture")
    (root / "BUILDINFO").write_text(f"source={SOURCE}\nprofile=ecs\nplatform=linux-x64\nkind=ecs-server\n")
    (root / "manifest.json").write_text(json.dumps({"source": SOURCE, "profile": "ecs", "platform": "linux-x64", "binaries": list(deploy.BINS)}))
    files = sorted(path for path in root.rglob("*") if path.is_file())
    (root / "SHA256SUMS").write_text("".join(f"{deploy.digest(path)}  {path.relative_to(root).as_posix()}\n" for path in files))
    archive = directory / "package.tar.gz"
    with tarfile.open(archive, "w:gz") as output:
        output.add(root, arcname=".")
        if extra:
            member = tarfile.TarInfo(extra)
            member.size = 4
            output.addfile(member, io.BytesIO(b"evil"))
    archive.with_name(archive.name + ".sha256").write_text(f"{deploy.digest(archive)}  {archive.name}\n")
    return archive


class PackageTests(unittest.TestCase):
    def test_final_archive_source_and_complete_checksum_coverage(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            archive = archive_fixture(root)
            path, info = deploy.package_info(root, SOURCE)
            self.assertEqual(path, archive)
            self.assertEqual(info["sha256"], deploy.digest(archive))
            self.assertGreater(info["expanded_bytes"], 0)
            with self.assertRaisesRegex(ValueError, "identity"):
                deploy.package_info(root, "b" * 40)

    def test_rejects_traversal_duplicate_and_unlisted_members(self):
        for member in ("../escaped", "bin/oasis7_chain_runtime", "web-dist/SHA256SUMS"):
            with self.subTest(member=member), tempfile.TemporaryDirectory() as temp:
                root = Path(temp)
                archive_fixture(root, extra=member)
                with self.assertRaises(ValueError):
                    deploy.package_info(root, SOURCE)
                self.assertFalse((root.parent / "escaped").exists())

    def test_corrupted_archive_fails_before_extraction(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            archive = archive_fixture(root)
            archive.write_bytes(archive.read_bytes() + b"tampered")
            with self.assertRaisesRegex(ValueError, "checksum"):
                deploy.package_info(root, SOURCE)


class ProvenanceTests(unittest.TestCase):
    def responses(self):
        return [
            {"head_sha": SOURCE, "status": "completed", "conclusion": "success", "head_branch": "main", "path": ".github/workflows/ecs-packages.yml"},
            [{"workflow_runs": [{"id": 12, "head_sha": SOURCE, "status": "completed", "conclusion": "success", "head_branch": "main", "path": ".github/workflows/rust.yml", "event": "push"}]}],
            [{"jobs": [{"name": "required-gate", "status": "completed", "conclusion": "success"}]}],
        ]

    def test_binds_required_gate_to_same_source_main_rust_run(self):
        with patch.object(deploy, "gh_json", side_effect=self.responses()) as gh:
            self.assertEqual(deploy.selected_run("nil-studio/oasis7", 123), SOURCE)
            self.assertIn("actions/workflows/rust.yml/runs", gh.call_args_list[1].args[1])
            self.assertIn("actions/runs/12/jobs", gh.call_args_list[2].args[1])

    def test_ignores_unrelated_same_named_gate_and_blocks_latest_failure(self):
        values = self.responses()
        values[1][0]["workflow_runs"][0]["path"] = ".github/workflows/unrelated.yml"
        with patch.object(deploy, "gh_json", side_effect=values), self.assertRaisesRegex(ValueError, "missing"):
            deploy.selected_run("nil-studio/oasis7", 123)
        values = self.responses()
        values[1][0]["workflow_runs"].append({**values[1][0]["workflow_runs"][0], "id": 13, "conclusion": "failure"})
        with patch.object(deploy, "gh_json", side_effect=values), self.assertRaisesRegex(ValueError, "not passed"):
            deploy.selected_run("nil-studio/oasis7", 123)


class FakeSession:
    def __init__(self, cfg, *, wrong_digest=False, oversized=False, fail_verify=False):
        self.calls = []
        self.closed = None
        self.blob = b"consistent stopped backup"
        self.data = bytearray(struct.pack(">I", len(self.blob)) + self.blob + struct.pack(">I", 0))
        if oversized:
            self.data[:4] = struct.pack(">I", deploy.CHUNK + 1)
        self.responses = [
            {"ok": True}, {"ok": True, "bytes": len(self.blob), "sha256": "wrong" if wrong_digest else hashlib.sha256(self.blob).hexdigest()}, {"ok": True}]
        self.fail_verify = fail_verify

    def call(self, op, **fields):
        self.calls.append(op)
        if op == "backup":
            return {"ok": True, "stream": True}
        if op == "verify" and self.fail_verify:
            raise RuntimeError("verification failed")
        return {"ok": True}

    def send(self, frame):
        self.calls.append(frame["op"])

    def upload(self, path):
        self.calls.append("upload")

    def response(self):
        return self.responses.pop(0)

    def read(self, size):
        value = bytes(self.data[:size])
        del self.data[:size]
        return value

    def close(self, accepted=False):
        self.closed = accepted


class ControllerTests(unittest.TestCase):
    def run_fixture(self, root, session):
        archive = root / "archive"
        archive.write_bytes(b"verified package")
        cfg = {"local_backup_root": str(root / "backups"), "max_backup_bytes": 1024**3}
        return deploy.execute(cfg, 123, SOURCE, archive, {"bytes": archive.stat().st_size, "sha256": deploy.digest(archive), "expanded_bytes": 42}, session_factory=lambda _: session)

    def test_stream_backup_compare_before_switch_and_accept_after_verification(self):
        with tempfile.TemporaryDirectory() as temp:
            session = FakeSession({})
            result = self.run_fixture(Path(temp), session)
            self.assertEqual(session.calls, ["init", "install", "upload", "backup", "compare", "upload", "switch", "verify", "accept"])
            self.assertTrue(session.closed)
            backup = Path(result["backup"]["path"])
            self.assertEqual(backup.read_bytes(), session.blob)
            self.assertEqual(backup.stat().st_mode & 0o777, 0o600)

    def test_unverified_or_oversized_backup_never_switches(self):
        for flags in ({"wrong_digest": True}, {"oversized": True}):
            with self.subTest(flags=flags), tempfile.TemporaryDirectory() as temp:
                session = FakeSession({}, **flags)
                with self.assertRaises(ValueError):
                    self.run_fixture(Path(temp), session)
                self.assertNotIn("switch", session.calls)
                self.assertFalse(session.closed)

    def test_failure_after_switch_closes_without_accepting(self):
        with tempfile.TemporaryDirectory() as temp:
            session = FakeSession({}, fail_verify=True)
            with self.assertRaises(RuntimeError):
                self.run_fixture(Path(temp), session)
            self.assertNotIn("accept", session.calls)
            self.assertFalse(session.closed)

    def test_ssh_is_one_process_and_strict_without_retry(self):
        # Inspect the actual Session constructor; fake pipes do not invoke ssh.
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            inp = root / "input"
            out = root / "output"
            inp.write_bytes(b"")
            out.write_bytes(b"")
            class Process:
                stdin = inp.open("wb", buffering=0)
                stdout = out.open("rb", buffering=0)
            with patch.dict(os.environ, {}, clear=True), patch.object(deploy.subprocess, "Popen", return_value=Process()) as create:
                session = deploy.Session({"ssh_host": "root@preview.example.test", "phase_timeout_seconds": 120})
                self.assertEqual(create.call_count, 1)
                command = create.call_args.args[0]
                self.assertIn("StrictHostKeyChecking=yes", command)
                self.assertIn("NumberOfPasswordPrompts=1", command)
                self.assertEqual(command[0], "ssh")
                session.process.stdin.close()
                session.process.stdout.close()

    def test_full_transport_uses_one_ssh_authentication_for_all_phases(self):
        # A real child with stdin/stdout exercises framing and partial pipe IO.
        # It is a protocol fixture, not a claim of Linux/systemd acceptance.
        fake_source = r'''#!/usr/bin/env python3
import sys,json,struct,hashlib,os
s=sys.stdin.buffer;o=sys.stdout.buffer
def emit(value):o.write(json.dumps(value).encode()+b'\n');o.flush()
def take(n):
 b=bytearray()
 while len(b)<n:
  p=s.read(n-len(b))
  if not p:raise SystemExit(3)
  b.extend(p)
 return bytes(b)
with open(os.environ['DEPLOY_TEST_LOG'],'a') as log:log.write('ssh-start\n')
assert 'probe_source' in json.loads(s.readline())
backup=b'stopped-world-fixture'
while True:
 line=s.readline()
 if not line:break
 frame=json.loads(line);op=frame['op']
 with open(os.environ['DEPLOY_TEST_LOG'],'a') as log:log.write(op+'\n')
 if op in ['install','compare']:
  data=take(frame['bytes']);assert hashlib.sha256(data).hexdigest()==frame['sha256']
 if op=='backup':
  emit({'ok':True,'stream':True})
  o.write(struct.pack('>I',len(backup))+backup+struct.pack('>I',0));o.flush()
  emit({'ok':True,'bytes':len(backup),'sha256':hashlib.sha256(backup).hexdigest()})
 else:emit({'ok':True})
 if op=='accept':break
'''
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            fake = root / "ssh"
            fake.write_text(fake_source)
            fake.chmod(0o755)
            archive = root / "package"
            archive.write_bytes(b"verified ECS fixture")
            cfg = {"ssh_host": "root@fixture.invalid", "phase_timeout_seconds": 120,
                   "local_backup_root": str(root / "backups"), "max_backup_bytes": 1024**3}
            env = {"PATH": str(root) + os.pathsep + os.environ["PATH"], "DEPLOY_TEST_LOG": str(root / "calls")}
            with patch.dict(os.environ, env):
                os.environ.pop("SSHPASS", None)
                result = deploy.execute(cfg, 123, SOURCE, archive, {"bytes": archive.stat().st_size,
                                        "sha256": deploy.digest(archive), "expanded_bytes": 42})
            self.assertEqual((root / "calls").read_text().splitlines(),
                             ["ssh-start", "init", "install", "backup", "compare", "switch", "verify", "accept"])
            self.assertEqual(Path(result["backup"]["path"]).read_bytes(), b"stopped-world-fixture")


if __name__ == "__main__":
    unittest.main()
