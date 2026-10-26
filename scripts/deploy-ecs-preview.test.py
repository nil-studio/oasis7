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


def profile_fixture(root, **changes):
    cfg = {
        "ssh_host": "root@fixture.invalid",
        "local_backup_root": str(root / "backups"),
        "release_root": "/opt/oasis7-preview/releases",
        "ops_root": "/root/oasis7-preview-ops",
        "deploy_lock": "/run/lock/oasis7-preview-deploy.lock",
        "chain_service": "oasis7-preview-chain.service",
        "player_service": "oasis7-preview-player.service",
        "backup_paths": ["/var/lib/oasis7-preview"],
        "public_origin": "https://preview.fixture.invalid",
        "launcher_http_port": 4173,
        "nginx_include": "/etc/nginx/snippets/oasis7-skills.conf",
        "probe": {},
    }
    cfg.update(changes)
    path = root / "profile.json"
    path.write_text(json.dumps(cfg))
    path.chmod(0o600)
    return path


class ProfileTests(unittest.TestCase):
    def test_backup_mode_defaults_to_remote_and_accepts_explicit_local(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            self.assertEqual(deploy.profile(profile_fixture(root))["backup_mode"], "remote")
            self.assertEqual(deploy.profile(profile_fixture(root, backup_mode="local"))["backup_mode"], "local")

    def test_profile_rejects_unknown_or_non_string_backup_mode(self):
        for mode in ("hybrid", ["remote"]):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as temp:
                path = profile_fixture(Path(temp), backup_mode=mode)
                with self.assertRaisesRegex(ValueError, "backup_mode"):
                    deploy.profile(path)


class FakeSession:
    def __init__(self, cfg, *, wrong_digest=False, oversized=False, fail_verify=False, backup_stream=True):
        self.calls = []
        self.closed = None
        self.blob = b"consistent stopped backup"
        self.data = bytearray(struct.pack(">I", len(self.blob)) + self.blob + struct.pack(">I", 0))
        if oversized:
            self.data[:4] = struct.pack(">I", deploy.CHUNK + 1)
        self.responses = [
            {"ok": True}, {"ok": True, "bytes": len(self.blob), "sha256": "wrong" if wrong_digest else hashlib.sha256(self.blob).hexdigest()}, {"ok": True}]
        self.fail_verify = fail_verify
        self.backup_stream = backup_stream

    def call(self, op, **fields):
        self.calls.append(op)
        if op == "backup":
            return {"ok": True, "stream": self.backup_stream}
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


class FakeRemoteSession:
    def __init__(self, *, header=None, refuse_compare=False):
        self.calls = []
        self.uploads = []
        self.closed = None
        self.refuse_compare = refuse_compare
        self.header = header or {
            "ok": True,
            "stream": False,
            "bytes": 6_392_429_837,
            "sha256": "c" * 64,
            "remote_archive": "/var/lib/oasis7-preview-ops/run-123/pre-upgrade.tar.gz",
        }
        self.init_config = None
        self.receipt_root = None
        self.receipt_at_switch = None
        self.receipt_at_accept = None

    def call(self, op, **fields):
        self.calls.append(op)
        if op == "init":
            self.init_config = fields["config"]
        if op == "backup":
            return self.header
        if op == "compare" and self.refuse_compare:
            raise RuntimeError("remote backup compare refused")
        if op == "switch" and self.receipt_root is not None:
            receipt = next(self.receipt_root.glob("ecs-*/deployment.json"))
            self.receipt_at_switch = json.loads(receipt.read_text())
        if op == "accept" and self.receipt_root is not None:
            receipt = next(self.receipt_root.glob("ecs-*/deployment.json"))
            self.receipt_at_accept = json.loads(receipt.read_text())
        return {"ok": True}

    def send(self, frame):
        self.calls.append(frame["op"])

    def upload(self, path):
        self.calls.append("upload")
        self.uploads.append(Path(path).read_bytes())

    def response(self):
        return {"ok": True}

    def read(self, _size):
        raise AssertionError("remote backup mode must not read a stream")

    def close(self, accepted=False):
        self.closed = accepted


class ControllerTests(unittest.TestCase):
    def run_fixture(self, root, session):
        archive = root / "archive"
        archive.write_bytes(b"verified package")
        cfg = {"local_backup_root": str(root / "backups"), "max_backup_bytes": 8 * 1024**3,
               "backup_mode": "local"}
        return deploy.execute(cfg, 123, SOURCE, archive, {"bytes": archive.stat().st_size, "sha256": deploy.digest(archive), "expanded_bytes": 42}, session_factory=lambda _: session)

    def run_remote_fixture(self, root, session, *, explicit_mode=None):
        archive = root / "archive"
        archive.write_bytes(b"verified package")
        cfg = {"local_backup_root": str(root / "backups"), "max_backup_bytes": 8 * 1024**3}
        if explicit_mode is not None:
            cfg["backup_mode"] = explicit_mode
        return deploy.execute(cfg, 123, SOURCE, archive,
                              {"bytes": archive.stat().st_size, "sha256": deploy.digest(archive), "expanded_bytes": 42},
                              session_factory=lambda _: session)

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

    def test_remote_backup_compares_by_remote_identity_without_uploading_world(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            session = FakeRemoteSession()
            session.receipt_root = root / "backups"
            result = self.run_remote_fixture(root, session)
            self.assertEqual(session.calls, ["init", "install", "upload", "backup", "compare", "switch", "verify", "accept"])
            self.assertEqual(session.init_config["backup_mode"], "remote")
            self.assertEqual(len(session.uploads), 1)
            self.assertEqual(session.uploads[0], b"verified package")
            self.assertTrue(session.closed)
            self.assertEqual(result["backup"], {
                "mode": "remote",
                "remote_archive": session.header["remote_archive"],
                "bytes": session.header["bytes"],
                "sha256": session.header["sha256"],
            })
            local_receipt = next((root / "backups").glob("ecs-*/deployment.json"))
            saved = json.loads(local_receipt.read_text())
            self.assertEqual(saved["backup"]["remote_archive"], session.header["remote_archive"])
            self.assertEqual(session.receipt_at_switch["backup"]["remote_archive"], session.header["remote_archive"])
            self.assertNotIn("verification", session.receipt_at_switch)
            self.assertEqual(session.receipt_at_accept["backup"]["remote_archive"], session.header["remote_archive"])
            self.assertEqual(session.receipt_at_accept["verification"], {"ok": True})
            self.assertEqual(local_receipt.stat().st_mode & 0o777, 0o600)

    def test_remote_compare_refusal_never_switches_or_accepts(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            session = FakeRemoteSession(refuse_compare=True)
            with self.assertRaisesRegex(RuntimeError, "compare refused"):
                self.run_remote_fixture(root, session)
            self.assertEqual(session.calls, ["init", "install", "upload", "backup", "compare"])
            self.assertEqual(len(session.uploads), 1)
            self.assertNotIn("switch", session.calls)
            self.assertNotIn("accept", session.calls)
            self.assertFalse(session.closed)

    def test_backup_mode_mismatch_and_invalid_remote_identity_fail_closed(self):
        cases = [
            (FakeRemoteSession(header={"ok": True, "stream": True}), None, "mode mismatch"),
            (FakeRemoteSession(header={"ok": True, "stream": False, "bytes": 0, "sha256": "c" * 64,
                                       "remote_archive": "/var/backup.tar.gz"}), None, "size invalid"),
            (FakeRemoteSession(header={"ok": True, "stream": False, "bytes": 1, "sha256": "bad",
                                       "remote_archive": "/var/backup.tar.gz"}), None, "digest invalid"),
            (FakeRemoteSession(header={"ok": True, "stream": False, "bytes": 8 * 1024**3 + 1,
                                       "sha256": "c" * 64, "remote_archive": "/var/backup.tar.gz"}),
             None, "size invalid"),
            (FakeRemoteSession(header={"ok": True, "stream": False, "bytes": 1, "sha256": "c" * 64,
                                       "remote_archive": "relative.tar.gz"}), None, "path invalid"),
        ]
        for session, mode, error in cases:
            with self.subTest(error=error), tempfile.TemporaryDirectory() as temp:
                with self.assertRaisesRegex(ValueError, error):
                    self.run_remote_fixture(Path(temp), session, explicit_mode=mode)
                self.assertNotIn("switch", session.calls)
                self.assertNotIn("accept", session.calls)

        with tempfile.TemporaryDirectory() as temp:
            session = FakeSession({}, backup_stream=False)
            with self.assertRaisesRegex(ValueError, "local backup mode mismatch"):
                self.run_fixture(Path(temp), session)
            self.assertNotIn("switch", session.calls)
            self.assertNotIn("accept", session.calls)

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
                   "local_backup_root": str(root / "backups"), "max_backup_bytes": 1024**3,
                   "backup_mode": "local"}
            env = {"PATH": str(root) + os.pathsep + os.environ["PATH"], "DEPLOY_TEST_LOG": str(root / "calls")}
            with patch.dict(os.environ, env):
                os.environ.pop("SSHPASS", None)
                result = deploy.execute(cfg, 123, SOURCE, archive, {"bytes": archive.stat().st_size,
                                        "sha256": deploy.digest(archive), "expanded_bytes": 42})
            self.assertEqual((root / "calls").read_text().splitlines(),
                             ["ssh-start", "init", "install", "backup", "compare", "switch", "verify", "accept"])
            self.assertEqual(Path(result["backup"]["path"]).read_bytes(), b"stopped-world-fixture")

    def test_full_transport_remote_backup_uses_one_ssh_and_sends_no_world_archive(self):
        fake_source = r'''#!/usr/bin/env python3
import sys,json,hashlib,os
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
payload_bytes=0
refuse=os.environ.get('DEPLOY_TEST_REFUSE_COMPARE')=='1'
while True:
 line=s.readline()
 if not line:break
 frame=json.loads(line);op=frame['op']
 with open(os.environ['DEPLOY_TEST_LOG'],'a') as log:log.write(op+'\n')
 if op=='install':
  data=take(frame['bytes']);payload_bytes+=len(data)
  assert hashlib.sha256(data).hexdigest()==frame['sha256']
  emit({'ok':True})
 elif op=='backup':
  emit({'ok':True,'stream':False,'bytes':6392429837,'sha256':'c'*64,
        'remote_archive':'/var/lib/oasis7-preview-ops/run-123/pre-upgrade.tar.gz'})
 elif op=='compare':
  assert frame.get('remote') is True and 'archive' not in frame
  emit({'ok':not refuse,'error':'remote_compare_refused' if refuse else None})
  if refuse:break
 elif op=='accept':
  emit({'ok':True});break
 else:emit({'ok':True})
with open(os.environ['DEPLOY_TEST_LOG'],'a') as log:log.write('payload_bytes:'+str(payload_bytes)+'\n')
'''
        for refuse in (False, True):
            with self.subTest(refuse_compare=refuse), tempfile.TemporaryDirectory() as temp:
                root = Path(temp)
                fake = root / "ssh"
                fake.write_text(fake_source)
                fake.chmod(0o755)
                archive = root / "package"
                archive.write_bytes(b"verified ECS fixture")
                cfg = {"ssh_host": "root@fixture.invalid", "phase_timeout_seconds": 120,
                       "local_backup_root": str(root / "backups"), "max_backup_bytes": 8 * 1024**3}
                env = {"PATH": str(root) + os.pathsep + os.environ["PATH"],
                       "DEPLOY_TEST_LOG": str(root / "calls"),
                       "DEPLOY_TEST_REFUSE_COMPARE": "1" if refuse else "0"}
                with patch.dict(os.environ, env):
                    os.environ.pop("SSHPASS", None)
                    if refuse:
                        with self.assertRaisesRegex(RuntimeError, "remote_compare_refused"):
                            deploy.execute(cfg, 123, SOURCE, archive,
                                           {"bytes": archive.stat().st_size, "sha256": deploy.digest(archive),
                                            "expanded_bytes": 42})
                    else:
                        result = deploy.execute(cfg, 123, SOURCE, archive,
                                                {"bytes": archive.stat().st_size, "sha256": deploy.digest(archive),
                                                 "expanded_bytes": 42})
                        self.assertEqual(result["backup"]["remote_archive"],
                                         "/var/lib/oasis7-preview-ops/run-123/pre-upgrade.tar.gz")
                lines = (root / "calls").read_text().splitlines()
                self.assertEqual(lines.count("ssh-start"), 1)
                self.assertEqual(lines[-1], f"payload_bytes:{archive.stat().st_size}")
                self.assertNotIn("compare-upload", lines)
                if refuse:
                    self.assertNotIn("switch", lines)
                    self.assertNotIn("accept", lines)
                    self.assertEqual([line for line in lines if line in {"init", "install", "backup", "compare"}],
                                     ["init", "install", "backup", "compare"])
                else:
                    self.assertEqual([line for line in lines if line in
                                      {"init", "install", "backup", "compare", "switch", "verify", "accept"}],
                                     ["init", "install", "backup", "compare", "switch", "verify", "accept"])


if __name__ == "__main__":
    unittest.main()
