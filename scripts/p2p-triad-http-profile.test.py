#!/usr/bin/env python3
"""Actual HTTP child/outer/merger boundary; host/traffic remain isolated doubles."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import threading
import time
import unittest
from unittest.mock import patch
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

SCRIPTS = Path(__file__).resolve().parent

def load(name, file):
    spec = importlib.util.spec_from_file_location(name, SCRIPTS / file)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

fixtures = load("profile_fixtures_http", "p2p-profile-readiness.test.py")
attempts = load("attempt_fixtures_http", "p2p-real-env-observability-attempt.test.py")
adapter = load("profile_adapter_http", "p2p-profile-readiness.py")

class ProfileHttpTests(unittest.TestCase):
    def test_formal_http_uses_same_identity_and_producer_age_as_file(self):
        status = fixtures.fixture()
        now = time.time_ns() // 1_000_000
        self.assertEqual(adapter.project(status, "file", now)["status"], "ready")
        self.assertEqual(adapter.project(status, "http", now)["status"], "ready")

    def setUp(self):
        self.fixture = attempts.AttemptWrapperTests("runTest")
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        self.env = dict(self.fixture.env, PYTHONDONTWRITEBYTECODE="1")
        for name in ("p2p-real-env-observability-summary.py", "p2p-profile-readiness.py",
                     "p2p-triad-http-profile.py"):
            if (SCRIPTS / name).exists():
                shutil.copy2(SCRIPTS / name, self.fixture.root / "scripts" / name)
        self.status = fixtures.fixture()
        self.http_code = 200
        self.body = None
        self.hits = 0
        self.header_delay = 0
        owner = self
        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):
                owner.hits += 1
                time.sleep(owner.header_delay)
                self.send_response(owner.http_code)
                if owner.http_code == 302:
                    self.send_header("Location", owner.url + "/redirect-target")
                self.end_headers()
                try:
                    self.wfile.write(owner.body if owner.body is not None else json.dumps(owner.status).encode())
                except (BrokenPipeError, ConnectionResetError):
                    pass
            def log_message(self, *args):
                pass
        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        thread.start()
        self.addCleanup(self.server.server_close)
        self.addCleanup(self.server.shutdown)
        self.url = "http://127.0.0.1:%s/status" % self.server.server_port

    def run_outer(self, expected=0):
        child = self.fixture.start(self.env, ["--local-profile-status-url", self.url])
        out, err = child.communicate(timeout=20)
        self.assertEqual(child.returncode, expected, (out.decode(), err.decode()))
        return json.loads((self.fixture.out / "latest_summary.json").read_text())

    def test_real_http_through_child_outer_and_merger(self):
        report = self.run_outer()
        profile = report["nodes"]["local_node"]["profile_readiness"]
        self.assertEqual(profile["status"], "ready")
        self.assertEqual(profile["source"], "http")
        self.assertEqual(profile["observed_at_unix_ms"], self.status["formal_public_observer"]["last_read_observed_unix_ms"])
        self.assertEqual(report["nodes"]["storage_ecs"]["profile_readiness"]["status"], "unsupported")
        serialized = json.dumps(report)
        self.assertNotIn("DO_NOT_COPY", serialized)
        self.assertNotIn(self.url, serialized)

    def test_http_profile_preserves_full_sync_and_fail_closed_boundaries(self):
        for profile in (fixtures.OBSERVER, fixtures.FULL):
            self.status = fixtures.fixture(profile)
            report = self.run_outer()
            self.assertEqual(report["nodes"]["local_node"]["profile_readiness"]["status"], "ready")
            child = self.status["formal_public_observer" if profile == fixtures.OBSERVER else "formal_full_state_sync"]
            child["generation"] += 1
            report = self.run_outer()
            self.assertEqual(report["nodes"]["local_node"]["profile_readiness"]["reason"], "instance_identity_mismatch")
        self.status = fixtures.fixture()
        observed = time.time_ns() // 1_000_000 - 61_000
        self.status["formal_public_observer"]["last_read_observed_unix_ms"] = observed
        report = self.run_outer()
        self.assertEqual(report["nodes"]["local_node"]["profile_readiness"]["status"], "stale")
        self.assertEqual(report["nodes"]["local_node"]["profile_readiness"]["observed_at_unix_ms"], observed)
        self.status = {"node_id": "old", "world_id": "world", "running": True}
        report = self.run_outer()
        self.assertEqual(report["nodes"]["local_node"]["profile_readiness"]["status"], "unsupported")

    def test_latest_http_failure_does_not_borrow_old_ready_and_recovers(self):
        self.run_outer()
        self.http_code = 503
        report = self.run_outer(expected=1)
        self.assertEqual(report["nodes"]["local_node"]["profile_readiness"]["status"], "unknown")
        self.assertEqual(report["nodes"]["local_node"]["profile_collection"]["result"], "http_failed")
        self.assertEqual(report["attempt"]["result"], "failed")
        self.assertEqual(report["last_success"]["sequence"], 1)
        self.http_code = 200
        self.assertEqual(self.run_outer()["nodes"]["local_node"]["profile_readiness"]["status"], "ready")

    def test_canonical_read_rechecks_age_without_mutating_frozen_artifacts(self):
        source = self.run_outer()
        original_json = (self.fixture.out / "latest_summary.json").read_bytes()
        original_md = (self.fixture.out / "latest_summary.md").read_bytes()
        publisher = load("profile_attempt_reader", "p2p-real-env-observability-attempt.py")
        observed = self.status["formal_public_observer"]["last_read_observed_unix_ms"]
        with patch.object(publisher.time, "time_ns", return_value=(observed + 60_001) * 1_000_000):
            report, markdown, code = publisher.read(self.fixture.out)
        self.assertEqual(code, 0)
        self.assertEqual(report["nodes"]["local_node"]["profile_readiness"]["status"], "stale")
        self.assertIn('"stale"', markdown)
        section = markdown.split("<!-- triad-profile-start -->", 1)[1]
        self.assertNotIn('"ready"', section)
        self.assertEqual((self.fixture.out / "latest_summary.json").read_bytes(), original_json)
        self.assertEqual((self.fixture.out / "latest_summary.md").read_bytes(), original_md)
        self.assertEqual(source["nodes"]["local_node"]["profile_readiness"]["observed_at_unix_ms"], observed)

    def test_http_child_redirect_size_json_and_preflight(self):
        child = load("http_profile_child", "p2p-triad-http-profile.py")
        self.http_code = 302
        self.assertEqual(child.fetch(self.url)[1], "http_failed")
        self.assertEqual(self.hits, 1, "redirect must not be followed")
        self.http_code = 200
        self.body = b"x" * (child.CAP + 1)
        self.assertEqual(child.fetch(self.url)[1], "output_limit")
        self.body = b"not JSON DO_NOT_COPY"
        self.assertEqual(child.fetch(self.url)[1], "invalid_status")
        self.header_delay = .3
        started = time.monotonic()
        with patch.object(child, "TIMEOUT", .1):
            self.assertEqual(child.fetch(self.url)[1], "timeout")
        self.assertLess(time.monotonic() - started, 1)

        with self.assertRaises(ValueError):
            child.endpoint("http://user:secret@localhost/status")
        with patch.object(child.subprocess, "Popen", side_effect=OSError("private spawn detail")):
            self.assertEqual(child.fetch(self.url), (None, "spawn_failed"))

        for options in (["--local-profile-status-url", "file:///private/status"],
                        ["--local-profile-status-url", "file:///private/status", "--local-profile-status-url", self.url]):
            process = self.fixture.start(self.env, options)
            process.communicate(timeout=10)
            self.assertEqual(process.returncode, 2)
            self.assertFalse(self.fixture.out.exists(), "every repeated endpoint must validate before attempt reservation")

if __name__ == "__main__":
    unittest.main()
