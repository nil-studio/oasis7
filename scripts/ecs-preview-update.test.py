#!/usr/bin/env python3
"""Isolated coordinator regressions; no service or production storage access."""
import contextlib
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import ecs_preview_update as u


class UpdateTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.old = self.release("old", "a" * 40)
        self.new = self.release("new", "b" * 40)
        self.config = {"state_dir": str(self.root / "state"), "releases_dir": str(self.root / "releases"), "lock_path": str(self.root / "lock"), "compatibility_file": str(self.root / "policy.json"), "components": {}, "world_identity": {"world_id": "original"}, "verifier_command": ["/trusted/verifier"], "skill": {}, "prefix": "preview", "endpoint": "https://oss.example", "bucket": "bucket", "channel_key": "preview/channels/preview.json"}
        for name in ("chain", "player", "web", "skill"):
            path = self.root / name
            path.symlink_to(self.old / name)
            self.config["components"][name] = {"path": str(path), "relative_path": name, **({"service": name + ".service"} if name in ("chain", "player") else {})}
        self.updater = u.Updater(self.config)
        self.channel = {"intent": "run-2", "sequence": 2, "manifest_key": "manifest", "release_id": "new", "source_sha": "b" * 40}
        self.manifest = {"release_id": "new", "source_sha": "b" * 40, "platform": "linux-x64", "components": ["player"], "compatibility": {}}
        class Store:
            def download_to(_, key, path, max_bytes):
                path.write_bytes(b"bad archive")
            def get(_, key):
                return json.dumps(self.channel if key == self.config["channel_key"] else self.manifest).encode()
        self.updater.store = Store()
        self.api = type("API", (), {"validate_channel": staticmethod(lambda *args: None), "validate_manifest": staticmethod(lambda *args: None)})

    def release(self, name, source):
        root = self.root / "releases" / name
        root.mkdir(parents=True)
        (root / "manifest.json").write_text(json.dumps({"source": source}))
        for component in ("chain", "player", "web", "skill"):
            (root / component).write_text(name + component)
        return root

    @contextlib.contextmanager
    def execution(self, fail=False):
        with patch.object(u, "module", return_value=self.api), patch.object(self.updater, "stage", return_value=self.new), patch.object(self.updater, "permit", return_value={"rollback_allowed": True}), patch.object(self.updater, "skill"), patch.object(self.updater, "service") as service, patch.object(self.updater, "verify", side_effect=[{}, ValueError("unhealthy"), {}] if fail else [{}, {}]):
            yield service

    def test_player_success_preserves_chain_and_duplicate_is_noop(self):
        with self.execution() as service:
            self.updater.tick()
            self.updater.tick()
        self.assertEqual((self.root / "chain").resolve(), self.old / "chain")
        self.assertEqual((self.root / "player").resolve(), self.new / "player")
        self.assertEqual((self.root / "web").resolve(), self.new / "web")
        self.assertEqual(self.updater.state["successful_intent"], "run-2")
        self.assertEqual(service.call_count, 2)

    def test_failed_player_restores_web_skill_and_suppresses_intent(self):
        with self.execution(fail=True) as service:
            with self.assertRaisesRegex(ValueError, "unhealthy"):
                self.updater.tick()
            count = service.call_count
            self.updater.tick()
            self.assertEqual(service.call_count, count)
        self.assertEqual(self.updater.state["rollback_result"], "success")
        for name in self.config["components"]:
            self.assertEqual((self.root / name).resolve(), self.old / name)

    def test_interrupted_switch_blocks_new_intent(self):
        self.updater.save(phase="switching", target=self.channel)
        self.updater.tick()
        self.assertEqual(self.updater.state["phase"], "blocked")
        with patch.object(self.updater, "stage") as stage:
            self.updater.tick()
        stage.assert_not_called()

    def test_pause_does_not_fetch_or_change_world(self):
        self.updater.save(paused=True)
        self.updater.store = None
        self.updater.tick()
        self.assertEqual(self.updater.actual(), self.updater.state["components"])

    def test_compatibility_requires_independent_exact_pairs(self):
        before = self.updater.actual()
        after = dict(before, player=u.binding(self.new / "player"))
        permission = {"from": {k:v["source_sha"] for k,v in before.items()}, "to": {k:v["source_sha"] for k,v in after.items()}, "contracts": {}, "evidence": "review-123", "rollback_allowed": True}
        path = Path(self.config["compatibility_file"])
        path.write_text(json.dumps({"schema_version":1,"permissions":[permission]}))
        self.assertTrue(self.updater.permit(before, after, self.manifest)["rollback_allowed"])
        after["chain"] = u.binding(self.new / "chain")
        with self.assertRaisesRegex(ValueError, "independent"):
            self.updater.permit(before, after, self.manifest)

    def test_lock_excludes_other_maintenance_and_reloads_state(self):
        import fcntl
        with self.updater.locked():
            other = open(self.config["lock_path"], "r+")
            self.addCleanup(other.close)
            with self.assertRaises(BlockingIOError):
                fcntl.flock(other, fcntl.LOCK_EX | fcntl.LOCK_NB)
            self.updater.save(paused=True)
        stale = u.Updater(self.config)
        self.updater.save(paused=False)
        with stale.locked():
            self.assertFalse(stale.state["paused"])

    def test_tar_traversal_rejected_before_switch(self):
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode="w:gz") as tar:
            member = tarfile.TarInfo("../../world")
            member.size = 1
            tar.addfile(member, io.BytesIO(b"x"))
        payload = buffer.getvalue()
        self.updater.store = type("Store", (), {"download_to": lambda _, key, path, limit: path.write_bytes(payload)})()
        with self.assertRaisesRegex(ValueError, "unsafe archive"):
            self.updater.stage({"release_id":"unsafe", "archive_key":"archive", "archive_sha256": hashlib.sha256(payload).hexdigest()})
        self.assertFalse((self.root / "world").exists())

    def test_archive_hash_failure_does_not_change_bindings(self):
        before = self.updater.actual()
        with self.assertRaisesRegex(ValueError, "SHA256"):
            self.updater.stage({"release_id":"bad", "archive_key":"archive", "archive_sha256":"0"*64})
        self.assertEqual(self.updater.actual(), before)

    def test_disk_reserve_fails_before_download(self):
        with patch.object(u.shutil, "disk_usage", return_value=type("Disk", (), {"free":1})()):
            with self.assertRaisesRegex(ValueError, "disk reserve"):
                self.updater.stage({"release_id":"space"})
        self.assertEqual((self.root / "player").resolve(), self.old / "player")

    def test_failed_chain_unknown_writes_never_restore_old_code(self):
        self.manifest["components"] = ["chain", "player"]
        def runner(args, **kwargs):
            request = json.loads(kwargs["input"])
            return subprocess.CompletedProcess(args, 0, stdout=json.dumps({"nonce":request["nonce"], "identity":self.config["world_identity"], "chain_write_status":"unknown"}))
        self.updater.runner = runner
        with self.execution(fail=True):
            with self.assertRaisesRegex(ValueError, "unhealthy"):
                self.updater.tick()
        self.assertEqual(self.updater.state["phase"], "blocked")
        self.assertEqual((self.root / "chain").resolve(), self.new / "chain")
        self.assertIn("unknown", self.updater.state["rollback_error"])

    def test_transient_download_failure_retries_bounded_without_switch(self):
        with patch.object(u, "module", return_value=self.api), patch.object(self.updater, "stage", side_effect=TimeoutError("network")), patch.object(self.updater, "switch") as switch:
            for i in range(3):
                self.updater.state["retry_after"] = 0
                with self.assertRaises(TimeoutError):
                    self.updater.tick()
                self.assertEqual(self.updater.state["attempts"], i + 1)
            self.updater.state["retry_after"] = 0
            self.updater.tick()
            self.assertEqual(self.updater.state["failed_intent"], self.channel["intent"])
        switch.assert_not_called()

    def test_rollback_failure_retains_actual_bindings_and_error(self):
        with self.execution(fail=True), patch.object(self.updater, "switch", side_effect=[None, ValueError("restore failed")]):
            with self.assertRaisesRegex(ValueError, "unhealthy"):
                self.updater.tick()
        self.assertEqual(self.updater.state["rollback_result"], "failed")
        self.assertEqual(self.updater.state["rollback_error"], "restore failed")
        self.assertEqual(self.updater.state["components"], self.updater.actual())

    def test_corrupt_state_status_keeps_diagnostics(self):
        self.updater.path.parent.mkdir()
        self.updater.path.write_text("{broken")
        other = u.Updater(self.config)
        self.assertEqual(other.state["result"], "state_error")
        with other.locked():
            self.assertEqual(other.state["phase"], "blocked")
        (self.root / "chain").unlink()
        broken = u.Updater(self.config)
        self.assertIn("error", broken.state["components"]["chain"])

    def test_config_rejects_shared_component_and_unsafe_service(self):
        config = dict(self.config, schema_version=1)
        path = self.root / "config.json"
        path.write_text(json.dumps(config))
        self.assertEqual(u.load_config(path, trusted=False)["schema_version"], 1)
        config["components"]["web"]["path"] = config["components"]["player"]["path"]
        path.write_text(json.dumps(config))
        with self.assertRaisesRegex(ValueError, "independent"):
            u.load_config(path, trusted=False)
        config["components"]["web"]["path"] = str(self.root / "web")
        config["components"]["player"]["service"] = "--help"
        path.write_text(json.dumps(config))
        with self.assertRaisesRegex(ValueError, "service name"):
            u.load_config(path, trusted=False)

    def test_preflight_rejects_unit_not_bound_to_component(self):
        def runner(args, **kwargs):
            return subprocess.CompletedProcess(args, 0, stdout="ExecStart={ path=/other/bin ; argv[]=/other/bin ; }\nMainPID=42\n")
        with patch.object(u.platform, "system", return_value="Linux"), patch.object(u.platform, "machine", return_value="x86_64"):
            with self.assertRaisesRegex(ValueError, "ExecStart"):
                u.preflight(self.config, runner=runner)
        self.assertEqual(self.updater.actual(), self.updater.state["components"])

    def test_duplicate_normalized_tar_path_rejected(self):
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode="w:gz") as tar:
            for name in ("payload", "./payload"):
                member = tarfile.TarInfo(name)
                member.size = 1
                tar.addfile(member, io.BytesIO(b"x"))
        payload = buffer.getvalue()
        self.updater.store = type("Store", (), {"download_to": lambda _, key, path, limit: path.write_bytes(payload)})()
        with self.assertRaisesRegex(ValueError, "duplicate"):
            self.updater.stage({"release_id":"duplicate", "archive_key":"archive", "archive_sha256":hashlib.sha256(payload).hexdigest()})

    def bundle(self, source="b" * 40):
        root = self.root / "bundle"
        root.mkdir()
        files = {"manifest.json":json.dumps({"source":source,"platform":"linux-x64","profile":"ecs"}), "BUILDINFO": "source=" + source + "\nprofile=ecs\nplatform=linux-x64\n", "player":"launcher", "bin/oasis7_game_launcher":"launcher", "deployment/ecs-skill-release.py":Path(u.__file__).with_name("package-ecs-skills.py").read_text(), "web-dist/skills/index.html":"index", "web-dist/skills/oasis7-play/SKILL.md":"skill"}
        for binary in ("oasis7_chain_runtime", "oasis7_viewer_live", "oasis7_game_client"):
            files["bin/" + binary] = "runtime program"
        files["web-dist/skills/oasis7-play/manifest.json"] = json.dumps({"schema":1,"source":source,"canonical_path":"/skills/oasis7-play/SKILL.md","files":{"SKILL.md":{"sha256":hashlib.sha256(b"skill").hexdigest(),"bytes":5}}})
        for name, content in files.items():
            path = root / name
            path.parent.mkdir(parents=True,exist_ok=True)
            path.write_text(content)
            if name.startswith("bin/"):
                path.chmod(0o755)
        (root / "SHA256SUMS").write_text("".join(u.digest(root / name) + "  " + name + "\n" for name in sorted(files)))
        output = io.BytesIO()
        with tarfile.open(fileobj=output,mode="w:gz") as tar:
            tar.add(root,arcname=".")
        payload = output.getvalue()
        self.updater.store = type("Store", (), {"download_to":lambda _,key,path,limit:path.write_bytes(payload)})()
        manifest = dict(self.manifest,release_id="staged",archive_key="archive",archive_sha256=hashlib.sha256(payload).hexdigest())
        self.updater.runner = lambda args,**kwargs:subprocess.CompletedProcess(args,0,stdout="libc.so => /lib/libc.so")
        return manifest

    def test_complete_bundle_stages_and_reuses_verified_release(self):
        manifest = self.bundle()
        staged = self.updater.stage(manifest)
        self.assertEqual((staged / "player").read_text(), "launcher")
        self.assertEqual(self.updater.stage(manifest), staged)
        self.assertEqual((self.root / "player").resolve(),self.old / "player")
        (staged / "player").write_text("corrupt")
        with self.assertRaisesRegex(ValueError,"checksums"):
            self.updater.stage(manifest)

    def test_staging_under_service_private_umask_remains_publicly_traversable(self):
        manifest = self.bundle()
        previous = os.umask(0o077)
        try:
            with patch.object(self.updater, "run", return_value=subprocess.CompletedProcess([], 0, stdout="libraries found")):
                release = self.updater.stage(manifest)
        finally:
            os.umask(previous)
        for directory in (release, release / "web-dist", release / "web-dist/skills"):
            self.assertEqual(directory.stat().st_mode & 0o005, 0o005)
        self.assertEqual((release / "web-dist/skills/index.html").stat().st_mode & 0o004, 0o004)

    def test_missing_viewer_dependency_is_rejected_before_any_service_change(self):
        manifest = self.bundle()
        calls = []
        def ldd(args, **kwargs):
            calls.append(args)
            output = "libviewer.so => not found" if args[-1].endswith("oasis7_viewer_live") else "all libraries found"
            return subprocess.CompletedProcess(args, 0, stdout=output)
        with patch.object(self.updater, "run", side_effect=ldd), self.assertRaisesRegex(ValueError, "dependency"):
            self.updater.stage(manifest)
        self.assertEqual((self.root / "player").resolve(), self.old / "player")
        self.assertFalse(any(command[0] == "systemctl" for command in calls))

    def test_reused_bundle_rejects_a_symbolic_viewer_even_when_bytes_match(self):
        manifest = self.bundle()
        with patch.object(self.updater, "run", return_value=subprocess.CompletedProcess([], 0, stdout="libraries found")):
            release = self.updater.stage(manifest)
            viewer = release / "bin/oasis7_viewer_live"
            outside = self.root / "outside-viewer"
            outside.write_bytes(viewer.read_bytes())
            outside.chmod(0o755)
            viewer.unlink()
            viewer.symlink_to(outside)
            with self.assertRaisesRegex(ValueError, "symbolic"):
                self.updater.stage(manifest)

    def test_wrong_source_fails_before_switch(self):
        manifest = self.bundle(source="c" * 40)
        with self.assertRaisesRegex(ValueError,"BUILDINFO"):
            self.updater.stage(manifest)
        self.assertEqual((self.root / "player").resolve(),self.old / "player")

    def test_missing_dependency_fails_before_switch(self):
        manifest = self.bundle()
        self.updater.runner = lambda args,**kwargs: subprocess.CompletedProcess(args,0,stdout="libmissing.so => not found")
        with self.assertRaisesRegex(ValueError,"runtime dependency"):
            self.updater.stage(manifest)
        self.assertEqual((self.root / "player").resolve(),self.old / "player")

    def viewer_policy(self):
        policy = {"schema_version":1, "allowed_inputs":["crates/oasis7_viewer/src/render.rs"], "evidence":"administrator audited external chain read-only mode", "read_only_viewer":True}
        Path(self.config["compatibility_file"]).write_text(json.dumps({"schema_version":1,"permissions":[],"viewer_policy":policy}))
        return policy

    def test_viewer_policy_authorizes_proven_player_bundle_without_exact_source_permission(self):
        policy = self.viewer_policy()
        before = self.updater.actual()
        after = dict(before)
        for name in ("player","web","skill"):
            after[name] = u.binding(self.new / name)
        with patch.object(u,"module") as load:
            load.return_value.evaluate_viewer.return_value = True
            result = self.updater.permit(before, after, self.manifest)
            load.assert_called_once_with("ecs_preview_compatibility")
            load.return_value.evaluate_viewer.assert_called_once_with(self.old,self.new,policy)
        self.assertTrue(result["rollback_allowed"])
        self.assertIn(policy["evidence"],result["evidence"])

    def test_viewer_policy_never_authorizes_changed_chain_binding_or_manifest(self):
        self.viewer_policy()
        before = self.updater.actual()
        after = dict(before,player=u.binding(self.new / "player"),chain=u.binding(self.new / "chain"))
        with patch.object(u,"module") as load:
            with self.assertRaisesRegex(ValueError,"chain update"):
                self.updater.permit(before,after,self.manifest)
            after["chain"] = before["chain"]
            with self.assertRaisesRegex(ValueError,"chain update"):
                self.updater.permit(before,after,dict(self.manifest,components=["chain","player"]))
        load.assert_not_called()

    def test_viewer_policy_rejects_missing_or_invalid_input_proof(self):
        self.viewer_policy()
        before = self.updater.actual()
        after = dict(before,player=u.binding(self.new / "player"))
        with patch.object(u,"module") as load:
            load.return_value.evaluate_viewer.side_effect = ValueError("missing complete Git input map")
            with self.assertRaisesRegex(ValueError,"input map"):
                self.updater.permit(before,after,self.manifest)
            load.return_value.evaluate_viewer.side_effect = None
            load.return_value.evaluate_viewer.return_value = False
            with self.assertRaisesRegex(ValueError,"not accepted"):
                self.updater.permit(before,after,self.manifest)

    def test_real_health_schema_and_nonce_required(self):
        components = self.updater.actual()
        identity = self.config["world_identity"]
        def runner(args, **kwargs):
            request = json.loads(kwargs["input"])
            evidence = {"nonce":request["nonce"], "identity":identity, "components":components, "checks":dict.fromkeys(("api","web","skill","signed_world"), True), "chain_write_status":"compatible", "samples":[{"timestamp":i*20,"world_tick":i,"identity":identity,"signed_world":True} for i in range(4)]}
            return subprocess.CompletedProcess(args, 0, stdout=json.dumps(evidence))
        self.updater.runner = runner
        with patch.object(u.time, "monotonic", side_effect=[0,60]):
            self.updater.verify(components)
        with patch.object(u.time, "monotonic", side_effect=[0,0]):
            with self.assertRaisesRegex(ValueError, "60-second"):
                self.updater.verify(components)


if __name__ == "__main__":
    unittest.main()
