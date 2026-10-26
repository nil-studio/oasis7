#!/usr/bin/env python3
"""Target-side remote-engine tests; no systemd, SSH, or network is used."""

import hashlib
import io
import json
import os
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import ecs_preview_remote as remote


SOURCE = "a" * 40
WORLD = {
    "world_id": "world-test",
    "genesis_digest": "genesis-test",
    "service_public_key": "b" * 64,
    "logical_tick": 7,
    "signature_verified": True,
}


def package_files(extra=None):
    files = {
        f"bin/{name}": b"#!/bin/sh\nexit 0\n"
        for name in remote.PACKAGE_BINS
    }
    files.update({
        "web-dist/index.html": b"<!doctype html>fixture",
        "deployment/ecs-skill-release.py": b"# skill fixture\n",
        "BUILDINFO": f"source={SOURCE}\nprofile=ecs\nplatform=linux-x64\nkind=ecs-server\n".encode(),
    })
    files.update(extra or {})
    files["manifest.json"] = json.dumps({
        "schema": 1,
        "source": SOURCE,
        "profile": "ecs",
        "platform": "linux-x64",
        "binaries": list(remote.PACKAGE_BINS),
    }, separators=(",", ":")).encode()
    files["SHA256SUMS"] = "".join(
        f"{hashlib.sha256(content).hexdigest()}  {name}\n"
        for name, content in sorted(files.items())
    ).encode()
    return files


def make_archive(files=None, *, extra_members=()):
    files = package_files() if files is None else files
    stream = io.BytesIO()
    with tarfile.open(fileobj=stream, mode="w:gz", format=tarfile.PAX_FORMAT) as archive:
        for name, content in sorted(files.items()):
            member = tarfile.TarInfo(name)
            member.size = len(content)
            member.mode = 0o755 if name.startswith("bin/") else 0o644
            archive.addfile(member, io.BytesIO(content))
        for member in extra_members:
            archive.addfile(member)
    blob = stream.getvalue()
    expanded = sum(map(len, files.values())) + sum(
        member.size for member in extra_members if member.isreg()
    )
    return blob, expanded


class PackageTests(unittest.TestCase):
    def extract(self, blob, expanded, root):
        return remote._extract_streaming_archive(
            io.BytesIO(blob), root,
            archive_bytes=len(blob),
            expected_digest=hashlib.sha256(blob).hexdigest(),
            expanded_bytes=expanded,
        )

    def test_streamed_package_is_verified_and_modes_are_normalized(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / "release"
            root.mkdir()
            blob, expanded = make_archive()
            digests = self.extract(blob, expanded, root)
            remote._validate_package(root, digests, SOURCE)
            self.assertEqual(set(digests), set(package_files()))
            self.assertEqual((root / "bin/oasis7_chain_runtime").stat().st_mode & 0o777, 0o755)
            self.assertEqual((root / "deployment/ecs-skill-release.py").stat().st_mode & 0o777, 0o644)
            self.assertEqual((root / "web-dist").stat().st_mode & 0o777, 0o755)

    def test_checksum_paths_allow_canonical_unicode_and_spaces(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / "release"
            root.mkdir()
            files = package_files({"web-dist/asset café.css": b".fixture { color: red }"})
            blob, expanded = make_archive(files)
            digests = self.extract(blob, expanded, root)
            remote._validate_package(root, digests, SOURCE)
            self.assertIn("web-dist/asset café.css", digests)

    def test_rejects_traversal_noncanonical_duplicate_and_link_members(self):
        cases = []
        traversal = tarfile.TarInfo("../escaped")
        traversal.size = 4
        cases.append((traversal,))
        noncanonical = tarfile.TarInfo("web-dist//bad")
        noncanonical.size = 1
        cases.append((noncanonical,))
        duplicate = tarfile.TarInfo("bin/oasis7_chain_runtime")
        duplicate.size = 1
        cases.append((duplicate,))
        link = tarfile.TarInfo("web-dist/link")
        link.type = tarfile.SYMTYPE
        link.linkname = "../outside"
        cases.append((link,))
        for extra in cases:
            with self.subTest(member=extra[0].name), tempfile.TemporaryDirectory() as temp:
                root = Path(temp) / "release"
                root.mkdir()
                blob, expanded = make_archive(extra_members=extra)
                with self.assertRaises(remote.RemoteError):
                    self.extract(blob, expanded, root)
                self.assertFalse(Path(temp).parent.joinpath("escaped").exists())
                self.assertFalse((Path(temp).parent / "outside").exists())

    def test_archive_digest_and_expanded_length_are_bound(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / "release"
            root.mkdir()
            blob, expanded = make_archive()
            with self.assertRaisesRegex(remote.RemoteError, "archive_digest_mismatch"):
                remote._extract_streaming_archive(
                    io.BytesIO(blob), root, archive_bytes=len(blob),
                    expected_digest="0" * 64, expanded_bytes=expanded,
                )
            with self.assertRaisesRegex(remote.RemoteError, "archive_expanded_size_mismatch"):
                second_root = Path(temp) / "second-release"
                second_root.mkdir()
                remote._extract_streaming_archive(
                    io.BytesIO(blob), second_root, archive_bytes=len(blob),
                    expected_digest=hashlib.sha256(blob).hexdigest(), expanded_bytes=expanded + 1,
                )

    def test_install_publishes_traversable_immutable_release_root(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            releases = base / "releases"
            releases.mkdir(mode=0o755)
            os.chmod(releases, 0o755)
            blob, expanded = make_archive()
            digest = hashlib.sha256(blob).hexdigest()
            engine = remote.Engine(stdin=io.BytesIO(blob), enforce_root=False)
            engine.phase = "initialized"
            engine.source = SOURCE
            engine.config = {"release_root": releases}
            engine.release = releases / f"ecs-42-{SOURCE[:12]}"
            engine.archive_meta = {"bytes": len(blob), "sha256": digest, "expanded_bytes": expanded}
            engine._require_reserve = lambda _extra=0: None
            result = engine._install({"bytes": len(blob), "sha256": digest})
            self.assertEqual(result["source"], SOURCE)
            self.assertEqual(engine.release.stat().st_mode & 0o777, 0o755)
            self.assertTrue(os.access(engine.release / "bin/oasis7_chain_runtime", os.X_OK))


class EngineTests(unittest.TestCase):
    def test_reserve_is_enforced_before_expansion(self):
        class Usage:
            free = remote.MIN_FREE_BYTES + 10

        engine = remote.Engine(disk_usage=lambda _path: Usage(), enforce_root=False)
        engine.config = {"release_root": Path("/tmp")}
        engine._require_reserve(10)
        with self.assertRaisesRegex(remote.RemoteError, "disk_reserve"):
            engine._require_reserve(11)

    def test_world_identity_rejects_non_string_service_key(self):
        engine = remote.Engine(enforce_root=False)
        engine.config = {"probe": {"expected_world": {"world_id": "world-test", "genesis_digest": "genesis-test"}}}
        engine.service_public_key = "b" * 64
        with self.assertRaisesRegex(remote.RemoteError, "world_identity_mismatch"):
            engine._validate_world({**WORLD, "service_public_key": None})

    def test_profile_accepts_ancestor_backup_and_derives_controller_lock(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp).resolve()
            release_root = root / "releases"
            release_root.mkdir(mode=0o755)
            os.chmod(release_root, 0o755)
            units = root / "units"
            units.mkdir()
            chain, player = "chain.service", "player.service"
            (units / chain).write_text("chain")
            (units / player).write_text("player")
            world = root / "world"
            key = world / "private/reader.key"
            key.parent.mkdir(parents=True)
            key.write_bytes(b"k" * 32)
            snippets = root / "snippets"
            snippets.mkdir()
            include = snippets / "skills.conf"
            include.write_text("# managed")
            config = remote._validate_config({
                "release_root": str(release_root), "ops_root": str(root / "ops"),
                "deploy_lock": str(root / "locks/deploy.lock"),
                "chain_service": chain, "player_service": player,
                "unit_dir": str(units), "backup_paths": [str(world), str(units / chain),
                                                            str(units / player), str(snippets)],
                "public_origin": "https://preview.example.test", "launcher_http_port": 4173,
                "nginx_include": str(include),
                "probe": {"reader_key_file": str(key)},
            })
            self.assertEqual(config["controller_lock"], root / "locks/deploy.lock.controller")

    def test_all_relevant_cgroup_executables_are_hash_checked(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / "ecs-release"
            binaries = root / "bin"
            binaries.mkdir(parents=True)
            for name in ("oasis7_game_launcher", "oasis7_viewer_live"):
                (binaries / name).write_bytes(name.encode())
            chain_digest = hashlib.sha256(b"oasis7_game_launcher").hexdigest()
            viewer_digest = hashlib.sha256(b"oasis7_viewer_live").hexdigest()
            identities = {
                101: {"pid": 101, "start_ticks": 1, "name": "oasis7_game_launcher",
                      "executable": str(binaries / "oasis7_game_launcher"), "sha256": chain_digest},
                102: {"pid": 102, "start_ticks": 2, "name": "oasis7_viewer_live",
                      "executable": str(binaries / "oasis7_viewer_live"), "sha256": viewer_digest},
                103: {"pid": 103, "start_ticks": 3, "name": "oasis7_viewer_live",
                      "executable": str(binaries / "oasis7_viewer_live"), "sha256": "f" * 64},
            }

            class FakeEngine(remote.Engine):
                def _parse_show(self, service):
                    return {"MainPID": "101", "ActiveState": "active", "ControlGroup": "/fake"}

                def _process_identity(self, pid):
                    return identities[pid]

                def _cgroup_pids(self, _service):
                    return {102}

            engine = FakeEngine(enforce_root=False)
            engine.config = {"chain_service": "chain.service", "player_service": "player.service",
                             "release_root": Path(temp)}
            engine._service_processes("player.service", expected_root=root)
            identities[102] = {**identities[102], "sha256": "0" * 64}
            with self.assertRaisesRegex(remote.RemoteError, "process_hash_mismatch"):
                engine._service_processes("player.service", expected_root=root)

    def test_partial_unit_replacement_restores_old_units_and_services(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            old_root = base / "old-release"
            release = base / "new-release"
            old_root.mkdir()
            release.mkdir()
            chain = "chain.service"
            player = "player.service"
            paths = {chain: base / chain, player: base / player}
            before = {
                chain: f"ExecStart={old_root}/bin/oasis7_chain_runtime\n".encode(),
                player: f"ExecStart={old_root}/bin/oasis7_game_launcher\n".encode(),
            }
            for service, path in paths.items():
                path.write_bytes(before[service])
            engine = remote.Engine(enforce_root=False)
            engine.config = {"chain_service": chain, "player_service": player,
                             "release_root": base, "probe": {"expected_world": {
                                 "world_id": WORLD["world_id"], "genesis_digest": WORLD["genesis_digest"]}}}
            engine.phase = "backup_verified"
            engine.backup_verified = True
            engine.initialized = True
            engine.deploy_lock_held = True
            engine.release = release
            engine.units = paths
            engine.before_units = before
            engine.unit_modes = {service: (os.getuid(), os.getgid(), 0o644) for service in paths}
            engine.old_roots = {chain: old_root, player: old_root}
            engine.world_identity = dict(WORLD)
            engine.service_public_key = WORLD["service_public_key"]
            engine.probe_ns = {"read_world": lambda _probe: dict(WORLD)}
            engine._assert_stopped = lambda: None
            engine._require_reserve = lambda _extra=0: None
            engine._record = lambda _name, _data: None
            engine._verify_service = lambda _service, _root: {}
            calls = []
            state = {service: "inactive" for service in paths}

            def systemctl(*args, **_kwargs):
                calls.append(args)
                if args[0] == "start":
                    state[args[1]] = "active"
                return b""

            engine._systemctl = systemctl
            engine._parse_show = lambda service: {
                "MainPID": "0" if state[service] == "inactive" else "333",
                "ActiveState": state[service], "ControlGroup": "/fake/" + service,
            }
            original_atomic = engine._atomic_unit
            failed = False

            def fail_second_new_write(service, content):
                nonlocal failed
                if not failed and service == player and content != before[player]:
                    failed = True
                    raise remote.RemoteError("unit_replace_failed")
                original_atomic(service, content)

            engine._atomic_unit = fail_second_new_write
            with self.assertRaisesRegex(remote.RemoteError, "unit_replace_failed"):
                engine._switch()
            result = engine._abort("unit_replace_failed")
            self.assertTrue(result["restored"])
            self.assertEqual({service: path.read_bytes() for service, path in paths.items()}, before)
            self.assertEqual([call[:2] for call in calls if call[0] == "start"],
                             [("start", chain), ("start", player)])

    def test_post_writer_abort_fences_candidate_without_old_restart(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            release = base / "candidate"
            release.mkdir()
            chain, player = "chain.service", "player.service"
            paths = {chain: base / chain, player: base / player}
            new = {chain: b"new chain unit", player: b"new player unit"}
            for service, path in paths.items():
                path.write_bytes(new[service])
            chain_exe = release / "bin/oasis7_chain_runtime"
            current = {201: {"pid": 201, "start_ticks": 22, "name": "oasis7_chain_runtime",
                             "executable": str(chain_exe), "sha256": "a" * 64}}

            class FakeEngine(remote.Engine):
                def _parse_show(self, service):
                    active = state[service]
                    return {"MainPID": "201" if active else "0", "ActiveState": "active" if active else "inactive",
                            "ControlGroup": "/fake/" + service}

                def _process_identity(self, pid):
                    return current[pid]

                def _cgroup_pids(self, service):
                    return {201} if service == chain and state[service] else set()

                def _assert_stopped(self):
                    return None

            state = {chain: True, player: False}
            engine = FakeEngine(enforce_root=False)
            engine.config = {"chain_service": chain, "player_service": player}
            engine.initialized = True
            engine.writer_boundary = True
            engine.deploy_lock_held = True
            engine.release = release
            engine.units = paths
            engine.new_units = new
            engine.old_processes = {f"{chain}:201": {"pid": 201, "start_ticks": 1}}
            engine.candidate_processes = {f"{chain}:201": current[201]}
            engine._safe_record = lambda _name, _data: None
            calls = []

            def systemctl(*args, **_kwargs):
                calls.append(args)
                if args[0] == "stop":
                    state[args[1]] = False
                return b""

            engine._systemctl = systemctl
            result = engine._abort("verify_failed")
            self.assertTrue(result["fenced"])
            self.assertEqual([call[:2] for call in calls], [("stop", chain)])
            self.assertFalse(any(call[0] == "start" for call in calls))

    def test_post_writer_identity_change_does_not_stop_another_service(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            release = base / "candidate"
            release.mkdir()
            chain = "chain.service"
            path = base / chain
            path.write_bytes(b"candidate unit")

            class FakeEngine(remote.Engine):
                def _parse_show(self, _service):
                    return {"MainPID": "202", "ActiveState": "active", "ControlGroup": "/fake"}

                def _process_identity(self, _pid):
                    return {"pid": 202, "start_ticks": 44, "name": "oasis7_chain_runtime",
                            "executable": str(base / "unrelated/bin/oasis7_chain_runtime"), "sha256": "c" * 64}

                def _cgroup_pids(self, _service):
                    return {202}

            engine = FakeEngine(enforce_root=False)
            engine.config = {"chain_service": chain, "player_service": "player.service"}
            engine.initialized = True
            engine.writer_boundary = True
            engine.deploy_lock_held = True
            engine.release = release
            engine.units = {chain: path}
            engine.new_units = {chain: b"candidate unit"}
            engine._safe_record = lambda _name, _data: None
            calls = []
            engine._systemctl = lambda *args, **_kwargs: calls.append(args) or b""
            with self.assertRaisesRegex(remote.RemoteError, "fence_failed"):
                engine._abort("transport_eof")
            self.assertEqual(calls, [])

    def test_skill_verify_failure_keeps_already_reacquired_deploy_lock(self):
        with tempfile.TemporaryDirectory() as temp:
            release = Path(temp)
            script = release / "deployment/ecs-skill-release.py"
            script.parent.mkdir()
            script.write_text("# fixture")
            engine = remote.Engine(enforce_root=False)
            engine.release = release
            engine.source = SOURCE
            engine.config = {"deploy_lock": Path(temp) / "deploy.lock",
                             "public_origin": "https://preview.example.test",
                             "player_service": "player.service", "nginx_include": Path(temp) / "skills.conf"}
            engine.deploy_lock_held = True
            reacquired = []
            engine._release_deploy_lock = lambda: setattr(engine, "deploy_lock_held", False)

            def reacquire():
                reacquired.append(True)
                engine.deploy_lock_held = True

            engine._reacquire_deploy_lock = reacquire

            def run(argv, **_kwargs):
                if argv[2] == "verify":
                    raise remote.RemoteError("skill_verify_failed")
                return b""

            engine._run = run
            with self.assertRaisesRegex(remote.RemoteError, "skill_verify_failed"):
                engine._activate_skill()
            self.assertEqual(len(reacquired), 1)
            self.assertTrue(engine.deploy_lock_held)

    def test_bootstrap_error_is_a_bounded_protocol_response(self):
        output = io.BytesIO()
        engine = remote.Engine(stdin=io.BytesIO(b"{}\n"), stdout=output, stderr=io.BytesIO(), enforce_root=False)
        self.assertEqual(engine.serve(), 1)
        self.assertEqual(json.loads(output.getvalue()), {"ok": False, "error": "bootstrap_invalid"})

    def test_init_accepts_controller_json_integer_run_id(self):
        engine = remote.Engine(enforce_root=False)
        engine.config = {"deploy_lock": Path("/unused"), "controller_lock": None}
        engine._prepare_paths = lambda: None
        engine._acquire_lock = lambda _path: None
        engine._require_reserve = lambda _extra=0: None
        engine._preflight_probe = lambda: None
        engine._create_operation_dir = lambda: None
        engine._snapshot_services = lambda: None
        engine._record = lambda _name, _data: None
        request = {"op": "init", "source": SOURCE, "run_id": 42,
                   "archive": {"bytes": 1, "sha256": "c" * 64, "expanded_bytes": 1},
                   "config": {}}
        with patch.object(remote, "_validate_config", lambda _raw: dict(engine.config)):
            engine._init(request)
        self.assertEqual(engine.run_id, "42")

    def test_init_does_not_delete_preexisting_operation_directory(self):
        with tempfile.TemporaryDirectory() as temp:
            operation = Path(temp) / "deploy-42-aaaaaaaaaaaa"
            operation.mkdir()
            sentinel = operation / "operator-state.json"
            sentinel.write_text("keep")
            engine = remote.Engine(enforce_root=False)
            engine.config = {"deploy_lock": Path(temp) / "deploy.lock", "controller_lock": None}
            engine._prepare_paths = lambda: setattr(engine, "operation_dir", operation)
            engine._acquire_lock = lambda _path: None
            engine._require_reserve = lambda _extra=0: None
            engine._preflight_probe = lambda: None
            engine._create_operation_dir = lambda: (_ for _ in ()).throw(remote.RemoteError("operation_exists"))
            request = {"op": "init", "source": SOURCE, "run_id": 42,
                       "archive": {"bytes": 1, "sha256": "c" * 64, "expanded_bytes": 1},
                       "config": {}}
            with patch.object(remote, "_validate_config", lambda _raw: dict(engine.config)):
                with self.assertRaisesRegex(remote.RemoteError, "operation_exists"):
                    engine._init(request)
            self.assertEqual(sentinel.read_text(), "keep")


if __name__ == "__main__":
    unittest.main()
