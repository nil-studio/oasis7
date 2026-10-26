#!/usr/bin/env python3
"""Installer unit wiring and independent health evidence negative cases."""
import copy
import subprocess
from unittest.mock import patch
import importlib.util
from pathlib import Path
import unittest
from ecs_preview_health import validate_evidence

spec = importlib.util.spec_from_file_location("install", Path(__file__).with_name("ecs-preview-install.py"))
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


class InstallTests(unittest.TestCase):
    def test_timer_is_independent_60_second_oneshot_without_world_initialization(self):
        units = installer.units(Path("/opt/updater/abc"), Path("/etc/oasis7/updater.json"), Path("/opt/venv/bin/python"))
        service = units["oasis7-preview-update.service"]
        timer = units["oasis7-preview-update.timer"]
        self.assertIn("Type=oneshot", service)
        self.assertIn("--config /etc/oasis7/updater.json tick", service)
        self.assertIn("OnUnitInactiveSec=60s", timer)
        self.assertNotIn("PartOf=", service)
        self.assertNotIn("initialize", service)

    def test_systemd_command_injection_paths_rejected(self):
        for path in ("relative", "/tmp/a b", "/tmp/a%h", "/tmp/a\nExecStart=/bad"):
            with self.subTest(path=path), self.assertRaises(ValueError):
                installer.units(Path(path), Path("/etc/updater.json"), Path("/usr/bin/python3"))


class EvidenceTests(unittest.TestCase):
    def setUp(self):
        self.identity = {"world_id": "original", "genesis_hash": "genesis", "service_public_key": "key", "identity": "reader"}
        self.components = {"player": {"source_sha": "a" * 40, "sha256": "b" * 64, "path": "/release/bin/player"}}
        self.evidence = {"identity": self.identity, "components": self.components,
            "checks": dict.fromkeys(("api", "web", "skill", "signed_world"), True),
            "samples": [{"timestamp": i * 20, "world_tick": 10 + i, "identity": self.identity, "signed_world": True} for i in range(4)],
            "chain_write_status": "compatible"}

    def validate(self, evidence):
        return validate_evidence(evidence, self.identity, self.components)

    def test_complete_signed_world_window(self):
        self.assertIs(self.validate(self.evidence), self.evidence)

    def test_snapshot_http_active_or_stale_evidence_cannot_pass(self):
        mutations = [lambda e: e.update(samples=e["samples"][:3]),
            lambda e: e["samples"][-1].update(timestamp=59),
            lambda e: e["samples"][-1].update(world_tick=12),
            lambda e: e["samples"][1].update(identity={"world_id": "new"}),
            lambda e: e["samples"][2].update(signed_world=False),
            lambda e: e["checks"].update(api=False),
            lambda e: e.update(components={}),
            lambda e: e.update(identity={}),
            lambda e: e.update(chain_write_status="safe"),
            lambda e: e["samples"][-1].update(timestamp=float("inf"))]
        for mutation in mutations:
            evidence = copy.deepcopy(self.evidence)
            mutation(evidence)
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                self.validate(evidence)


class SignedVerifierTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        spec = importlib.util.spec_from_file_location("verify", Path(__file__).with_name("ecs-preview-verify.py"))
        cls.verifier = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cls.verifier)

    def test_uses_real_signed_capabilities_entry_and_requires_tick(self):
        result = subprocess.CompletedProcess([], 0, '{"api_revision":1,"logical_tick":19}')
        with patch.object(self.verifier.subprocess, "run", return_value=result) as run:
            sample = self.verifier.signed_sample(Path("/release/bin/oasis7_game_client"), Path("/etc/device.json"), {"world_id": "original"})
        self.assertEqual(run.call_args.args[0], ["/release/bin/oasis7_game_client", "--config", "/etc/device.json", "capabilities"])
        self.assertEqual(sample["world_tick"], 19)
        self.assertTrue(run.call_args.kwargs["check"])
        for body in ('{}', '{"api_revision":1,"logical_tick":true}', '{"api_revision":2,"logical_tick":1}'):
            with patch.object(self.verifier.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, body)), self.assertRaises(ValueError):
                self.verifier.signed_sample(Path("/client"), Path("/device"), {})

    def test_public_html_allows_only_the_real_launcher_runtime_injection(self):
        source = b"<html><head></head><body>version A</body></html>"
        node = b'<script id="oasis7-viewer-runtime-config" type="application/json">{"deploymentMode":"hosted_public_join","viewerWsEndpoint":"wss://preview.invalid/runtime"}</script>'
        public = source.replace(b"</head>", node + b"</head>")
        self.verifier.verify_web_bytes(Path("index.html"), source, public, {})
        for bad in (public.replace(b"version A", b"version B"), public + node, public.replace(b"hosted_public_join", b"trusted_local_only")):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                self.verifier.verify_web_bytes(Path("index.html"), source, bad, {})
        with self.assertRaises(ValueError):
            self.verifier.verify_web_bytes(Path("client.js"), b"old", b"new", {})

    def test_signature_client_failure_cannot_be_turned_into_health(self):
        with patch.object(self.verifier.subprocess, "run", side_effect=subprocess.CalledProcessError(1, ["client"])), self.assertRaises(subprocess.CalledProcessError):
            self.verifier.signed_sample(Path("/client"), Path("/device"), {})


if __name__ == "__main__":
    unittest.main()
