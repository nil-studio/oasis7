#!/usr/bin/env python3
"""Actual HTTP child/outer/merger boundary; host/traffic remain isolated doubles."""
import hashlib
import importlib.util
import json
import os
import signal
from pathlib import Path
import shutil
import subprocess
import sys
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
    def test_bft_diagnostics_flow_through_actual_collector_without_qualification(self):
        self.status = fixtures.bft_fixture()
        self.status["observation"]["private_action"] = "DO_NOT_COPY"
        report = self.run_outer()
        profile = report["nodes"]["local_node"]["profile_readiness"]
        self.assertEqual(profile["height"], 10)
        self.assertEqual(profile["runtime_profile"], "bft/version=1")
        self.assertEqual(profile["status"], "unknown")
        self.assertFalse(profile["can_propose"])
        self.assertEqual(profile["observed_at_unix_ms"], self.status["observation"]["observed_at_unix_ms"])
        self.assertNotIn("DO_NOT_COPY", json.dumps(report))
        child = load("bft_http_reader", "p2p-triad-http-profile.py")
        observed = self.status["observation"]["observed_at_unix_ms"]
        expired = child.refresh_report(report, observed+60_001)["nodes"]["local_node"]["profile_readiness"]
        self.assertEqual(expired["status"], "stale")
        self.assertEqual(expired["source_freshness"], "stale")
        self.assertNotIn("running", expired)
        future = child.refresh_report(report, observed-1)["nodes"]["local_node"]["profile_readiness"]
        self.assertEqual(future["source_freshness"], "unknown")

    def test_pinned_bft_identity_rejects_wrong_slot_and_recovers_in_outer_report(self):
        self.status = fixtures.bft_fixture()
        observation = self.status["observation"]
        pins = {key: observation[key] for key in ("world_id", "chain_id", "operator_instance_id")}
        wrong = dict(pins, operator_instance_id="wrong-instance")
        options = ["--local-profile-status-url", self.url,
                   "--local-bft-identity", json.dumps(pins),
                   "--sequencer-profile-status-url", self.url,
                   "--sequencer-bft-identity", json.dumps(wrong),
                   "--storage-profile-status-url", self.url,
                   "--storage-bft-identity", json.dumps(pins)]
        process = self.fixture.start(self.env, options)
        out, err = process.communicate(timeout=20)
        self.assertEqual(process.returncode, 1, (out.decode(), err.decode()))
        report = json.loads((self.fixture.out / "latest_summary.json").read_text())
        nodes = report["nodes"]
        self.assertEqual(self.hits, 3)
        for label in ("local_node", "storage_ecs"):
            node = nodes[label]
            self.assertEqual(node["profile_collection"]["mode"], "bft_pinned_http")
            self.assertTrue(node["profile_collection"]["complete"])
            self.assertTrue(node["profile_readiness"]["configured_identity_match"])
            self.assertEqual(node["profile_readiness"]["observed_at_unix_ms"],
                             observation["observed_at_unix_ms"])
            self.assertFalse(node["profile_readiness"]["can_propose"])
        rejected = nodes["sequencer_ecs"]
        self.assertFalse(rejected["profile_collection"]["complete"])
        self.assertEqual(rejected["profile_readiness"]["reason"], "bft_identity_mismatch")
        self.assertNotIn("height", rejected["profile_readiness"])
        self.assertNotIn("wrong-instance", json.dumps(report))
        self.assertNotEqual(rejected["profile_collection"]["endpoint_scope_digest"],
                            nodes["local_node"]["profile_collection"]["endpoint_scope_digest"])

        # The same pinned scope recovers after its producer reports the
        # configured instance; the original producer timestamp is not reset.
        observation["operator_instance_id"] = wrong["operator_instance_id"]
        process = self.fixture.start(self.env, options)
        out, err = process.communicate(timeout=20)
        self.assertEqual(process.returncode, 1, (out.decode(), err.decode()))
        recovered = json.loads((self.fixture.out / "latest_summary.json").read_text())["nodes"]["sequencer_ecs"]
        self.assertTrue(recovered["profile_collection"]["complete"])
        self.assertTrue(recovered["profile_readiness"]["configured_identity_match"])
        self.assertEqual(recovered["profile_readiness"]["observed_at_unix_ms"],
                         observation["observed_at_unix_ms"])
        self.assertEqual(recovered["profile_readiness"]["status"], "unknown")
        self.assertFalse(recovered["profile_readiness"]["can_propose"])

    def test_pinned_bft_slot_rejects_observer_schema_and_ambiguous_config(self):
        self.status = fixtures.fixture()
        pins = json.dumps({"world_id": "w", "chain_id": "c", "operator_instance_id": "i"})
        options = ["--local-profile-status-url", self.url, "--local-bft-identity", pins]
        process = self.fixture.start(self.env, options)
        out, err = process.communicate(timeout=20)
        self.assertEqual(process.returncode, 1, (out.decode(), err.decode()))
        node = json.loads((self.fixture.out / "latest_summary.json").read_text())["nodes"]["local_node"]
        self.assertEqual(node["profile_readiness"]["reason"], "bft_schema_mismatch")
        self.assertFalse(node["profile_collection"]["complete"])
        self.assertFalse(node["profile_readiness"]["can_propose"])
        hits = self.hits
        for invalid in (["--local-bft-identity", pins],
                        options + ["--local-bft-identity", pins],
                        options + ["--local-profile-status-url", self.url],
                        ["--local-profile-status-url", self.url, "--local-bft-identity", "{}"]):
            process = self.fixture.start(self.env, invalid)
            process.communicate(timeout=20)
            self.assertNotEqual(process.returncode, 0)
        self.assertEqual(self.hits, hits)

    def test_pinned_bft_malformed_checkpoint_rejected_then_recovers(self):
        self.status = fixtures.bft_fixture()
        observation = self.status["observation"]
        pins = json.dumps({key: observation[key] for key in
                           ("world_id", "chain_id", "operator_instance_id")})
        options = ["--local-profile-status-url", self.url,
                   "--local-bft-identity", pins]
        original_digest = observation["binding_digest"]
        original_time = observation["observed_at_unix_ms"]
        observation["binding_digest"] = "malformed-checkpoint"
        process = self.fixture.start(self.env, options)
        out, err = process.communicate(timeout=20)
        self.assertEqual(process.returncode, 1, (out.decode(), err.decode()))
        report = json.loads((self.fixture.out / "latest_summary.json").read_text())
        node = report["nodes"]["local_node"]
        self.assertFalse(node["profile_collection"]["complete"])
        self.assertEqual(node["profile_collection"]["result"], "invalid_bft_digest")
        self.assertEqual(node["profile_readiness"]["reason"], "invalid_bft_digest")
        self.assertNotIn("configured_identity_match", node["profile_readiness"])
        self.assertNotIn("height", node["profile_readiness"])
        markdown = (self.fixture.out / "latest_summary.md").read_text()
        self.assertIn("invalid_bft_digest", markdown)
        self.assertNotIn("malformed-checkpoint", json.dumps(report) + markdown)
        rejected_scope = node["profile_collection"]["endpoint_scope_digest"]

        observation["binding_digest"] = original_digest
        process = self.fixture.start(self.env, options)
        out, err = process.communicate(timeout=20)
        self.assertEqual(process.returncode, 0, (out.decode(), err.decode()))
        report = json.loads((self.fixture.out / "latest_summary.json").read_text())
        node = report["nodes"]["local_node"]
        profile = node["profile_readiness"]
        self.assertTrue(node["profile_collection"]["complete"])
        self.assertEqual(node["profile_collection"]["endpoint_scope_digest"], rejected_scope)
        self.assertTrue(profile["configured_identity_match"])
        self.assertEqual(profile["observed_at_unix_ms"], original_time)
        self.assertEqual(profile["reason"], "bft_qualification_not_observed")
        self.assertFalse(profile["can_propose"])
        markdown = (self.fixture.out / "latest_summary.md").read_text()
        self.assertIn("bft_qualification_not_observed", markdown)

        # A new collector timestamp cannot refresh historical producer data.
        observation["observed_at_unix_ms"] = max(0, original_time - 120_000)
        process = self.fixture.start(self.env, options)
        out, err = process.communicate(timeout=20)
        self.assertEqual(process.returncode, 0, (out.decode(), err.decode()))
        report = json.loads((self.fixture.out / "latest_summary.json").read_text())
        profile = report["nodes"]["local_node"]["profile_readiness"]
        self.assertEqual(profile["status"], "stale")
        self.assertEqual(profile["observed_at_unix_ms"], observation["observed_at_unix_ms"])
        self.assertFalse(profile["can_propose"])
        self.assertIn("bft_observation_expired", (self.fixture.out / "latest_summary.md").read_text())

    def test_truncated_content_length_is_not_a_complete_observation(self):
        self.status = fixtures.bft_fixture()
        self.content_length_extra = 10
        report = self.run_outer(expected=1)
        node = report["nodes"]["local_node"]
        self.assertEqual(node["profile_collection"]["result"], "invalid_status")
        self.assertFalse(node["profile_collection"]["complete"])
        self.assertEqual(node["profile_readiness"]["status"], "unknown")
        self.assertNotIn("height", node["profile_readiness"])
        # A subsequent complete response may recover collection; neither
        # response grants formal BFT write qualification.
        self.content_length_extra = 0
        recovered = self.run_outer()["nodes"]["local_node"]
        self.assertTrue(recovered["profile_collection"]["complete"])
        self.assertEqual(recovered["profile_readiness"]["height"], 10)
        self.assertFalse(recovered["profile_readiness"]["can_propose"])

    def test_duplicate_status_fields_fail_closed_before_projection_and_recover(self):
        # Both a top-level readiness claim and a nested BFT checkpoint fact
        # must be rejected before an otherwise valid profile is projected.
        cases = (
            (fixtures.fixture(), '"formal_readiness": true',
             '"formal_readiness": false, "formal_readiness": true'),
            (fixtures.bft_fixture(), '"height": 10',
             '"height": 10, "height": 11'),
        )
        for status, original, conflicting in cases:
            with self.subTest(schema=status["api_version"]):
                self.status = status
                encoded = json.dumps(status)
                self.assertIn(original, encoded)
                self.body = encoded.replace(original, conflicting, 1).encode("utf-8")
                rejected = self.run_outer(expected=1)["nodes"]["local_node"]
                self.assertEqual(rejected["profile_collection"]["result"], "invalid_status")
                self.assertFalse(rejected["profile_collection"]["complete"])
                self.assertEqual(rejected["profile_readiness"]["status"], "unknown")
                self.assertFalse(rejected["profile_readiness"]["can_propose"])
                self.assertNotIn("height", rejected["profile_readiness"])

                self.body = None
                recovered = self.run_outer()["nodes"]["local_node"]
                self.assertTrue(recovered["profile_collection"]["complete"])
                if status["api_version"] == "oasis7.bft-node-observation/v1":
                    self.assertEqual(recovered["profile_readiness"]["height"], 10)
                    self.assertFalse(recovered["profile_readiness"]["can_propose"])
                else:
                    self.assertEqual(recovered["profile_readiness"]["status"], "ready")

    def test_nonfinite_http_status_numbers_fail_closed_before_projection_and_recover(self):
        # Python's default JSON decoder accepts these non-JSON numeric tokens.
        # An otherwise valid controlled or BFT producer must not be collected.
        for status in (fixtures.fixture(), fixtures.bft_fixture()):
            for literal in ("NaN", "Infinity", "-Infinity", "1e9999"):
                with self.subTest(schema=status["api_version"], token=literal):
                    self.status = status
                    payload = json.dumps(status)
                    self.body = ('{"untrusted_nonfinite": ' + literal + ',' + payload[1:]).encode("utf-8")
                    rejected = self.run_outer(expected=1)["nodes"]["local_node"]
                    self.assertEqual(rejected["profile_collection"]["result"], "invalid_status")
                    self.assertFalse(rejected["profile_collection"]["complete"])
                    self.assertEqual(rejected["profile_readiness"]["status"], "unknown")
                    self.assertFalse(rejected["profile_readiness"]["can_propose"])
                    self.assertNotIn("untrusted_nonfinite", json.dumps(rejected))

                    self.body = None
                    recovered = self.run_outer()["nodes"]["local_node"]
                    self.assertTrue(recovered["profile_collection"]["complete"])
                    if status["api_version"] == "oasis7.bft-node-observation/v1":
                        self.assertEqual(recovered["profile_readiness"]["height"], 10)
                        self.assertFalse(recovered["profile_readiness"]["can_propose"])
                    else:
                        self.assertEqual(recovered["profile_readiness"]["status"], "ready")

    def test_bft_future_applied_height_is_rejected_through_actual_http_report(self):
        self.status = fixtures.bft_fixture()
        observed = self.status["observation"]
        observed["last_applied_height"] = observed["height"] + 1
        report = self.run_outer()
        profile = report["nodes"]["local_node"]["profile_readiness"]
        self.assertEqual(profile["status"], "unknown")
        self.assertEqual(profile["reason"], "invalid_bft_window_or_applied")
        self.assertFalse(profile["can_propose"])
        self.assertNotIn("last_applied_height", profile)
        self.assertTrue(report["nodes"]["local_node"]["profile_collection"]["complete"])

        observed["last_applied_height"] = observed["height"]
        recovered = self.run_outer()["nodes"]["local_node"]["profile_readiness"]
        self.assertEqual(recovered["reason"], "bft_qualification_not_observed")
        self.assertEqual(recovered["last_applied_height"], observed["height"])
        self.assertFalse(recovered["can_propose"])

    def test_bft_window_history_exceeding_observed_span_is_unknown_and_recovers(self):
        self.status = fixtures.bft_fixture()
        self.status["observation"]["committed_window_heights"] = 2
        invalid = self.run_outer()["nodes"]["local_node"]
        self.assertTrue(invalid["profile_collection"]["complete"])
        self.assertEqual(invalid["profile_readiness"]["status"], "unknown")
        self.assertEqual(invalid["profile_readiness"]["reason"], "invalid_bft_window_or_applied")
        self.assertFalse(invalid["profile_readiness"]["can_propose"])

        self.status["observation"]["committed_window_heights"] = 1
        recovered = self.run_outer()["nodes"]["local_node"]["profile_readiness"]
        self.assertEqual(recovered["reason"], "bft_qualification_not_observed")
        self.assertEqual(recovered["committed_window_heights"], 1)
        self.assertFalse(recovered["can_propose"])

    def test_formal_http_uses_same_identity_and_producer_age_as_file(self):
        status = fixtures.fixture()
        now = time.time_ns() // 1_000_000
        self.assertEqual(adapter.project(status, "file", now)["status"], "ready")
        self.assertEqual(adapter.project(status, "http", now)["status"], "ready")

    def test_missing_bft_blocker_does_not_abort_later_node_collection(self):
        self.status = fixtures.bft_fixture()
        del self.status["blocker"]
        process = self.fixture.start(self.env, ["--local-profile-status-url", self.url,
            "--storage-profile-status-url", self.url])
        out, err = process.communicate(timeout=20)
        self.assertEqual(process.returncode, 0, (out.decode(), err.decode()))
        report = json.loads((self.fixture.out / "latest_summary.json").read_text())
        self.assertEqual(self.hits, 2, "malformed first node must not abort later probes")
        for label in ("local_node", "storage_ecs"):
            node = report["nodes"][label]
            self.assertEqual(node["profile_collection"]["result"], "success")
            self.assertEqual(node["profile_readiness"]["status"], "unknown")
            self.assertEqual(node["profile_readiness"]["reason"], "invalid_bft_observation")
            self.assertFalse(node["profile_readiness"]["can_propose"])

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
        self.content_length_extra = None
        owner = self
        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):
                owner.hits += 1
                time.sleep(owner.header_delay)
                body = owner.body if owner.body is not None else json.dumps(owner.status).encode()
                self.send_response(owner.http_code)
                if owner.content_length_extra is not None:
                    self.send_header("Content-Length", str(len(body) + owner.content_length_extra))
                if owner.http_code == 302:
                    self.send_header("Location", owner.url + "/redirect-target")
                self.end_headers()
                try:
                    self.wfile.write(body)
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

    def test_bft_missing_observation_keeps_producer_reason_in_canonical_report(self):
        self.status = fixtures.bft_fixture()
        self.status["observation"] = None
        report = self.run_outer()
        profile = report["nodes"]["local_node"]["profile_readiness"]
        self.assertEqual(profile["status"], "unknown")
        self.assertEqual(profile["reason"], "bft_observation_unavailable")
        self.assertEqual(profile["source_freshness"], "unknown")
        self.assertIs(profile["can_propose"], False)
        reader = load("bft_attempt_reader", "p2p-real-env-observability-attempt.py")
        report, markdown, code = reader.read(self.fixture.out)
        self.assertEqual(code, 0)
        self.assertEqual(report["nodes"]["local_node"]["profile_readiness"]["reason"],
                         "bft_observation_unavailable")
        self.assertIn('"bft_observation_unavailable"', markdown)

    def controlled_inventory_fixture(self):
        runtime = self.fixture.root / "fixture-status-verifier"
        runtime.write_text('''#!/usr/bin/env python3
import json, sys
assert sys.argv[1:5] == ["op", "formal-status-observe", "--request", "-"]
assert len(sys.argv) in (5, 7)
pins = json.load(sys.stdin)
assert pins["service_public_key"] == "e" * 64
assert "owner_config" not in pins
result = dict(api_version="oasis7.formal-live-status-observation/v1",
    observation_scope="authenticated_host_diagnostics_only", authority_qualification="unknown",
    node_id=pins["expected_node_id"], world=pins["expected_world"], host_state="ready",
    host_reports_ready=True, node_running=True, runtime_error_present=False,
    committed_position=8, committed_record_id="f" * 64, raw_error="DO_NOT_COPY")
if len(sys.argv) == 7:
    assert sys.argv[5] == "--owner-config"
    with open(sys.argv[6]) as config:
        result["protected_pair"] = json.load(config)
print(json.dumps(result))
''')
        runtime.chmod(0o700)
        pins = dict(status_url=self.url.replace("/status", "/v1/chain/status"),
                    service_public_key="e" * 64, expected_node_id="primary-host",
                    expected_world=dict(world_id="controlled-world", genesis_digest="a" * 64))
        inventory = self.fixture.root / "controlled-inventory.json"
        inventory.write_text(json.dumps(dict(api_version="oasis7.controlled-status-inventory/v1",
                                              runtime_bin=str(runtime), slots=dict(local_node=pins))))
        return inventory

    def test_controlled_inventory_projects_only_verified_host_scope(self):
        inventory = self.controlled_inventory_fixture()
        process = self.fixture.start(self.env, [
            "--controlled-status-inventory", str(inventory), "--storage-profile-status-url", self.url
        ])
        out, err = process.communicate(timeout=20)
        self.assertEqual(process.returncode, 0, (out.decode(), err.decode()))
        report = json.loads((self.fixture.out / "latest_summary.json").read_text())
        host = report["nodes"]["local_node"]
        profile = host["profile_readiness"]
        self.assertEqual(profile["source"], "verified_controlled_status")
        self.assertEqual(profile["committed_position"], 8)
        self.assertEqual(profile["status"], "unknown")
        self.assertIs(profile["host_reports_ready"], True)
        self.assertIs(profile["can_propose"], False)
        self.assertEqual(profile["authority_qualification"], "unknown")
        pins = json.loads(inventory.read_text())["slots"]["local_node"]
        self.assertEqual(host["profile_collection"]["endpoint_scope_digest"],
                         hashlib.sha256(json.dumps(pins, sort_keys=True, separators=(",", ":")).encode()).hexdigest())
        self.assertEqual(report["nodes"]["storage_ecs"]["profile_readiness"]["status"], "ready")
        markdown = (self.fixture.out / "latest_summary.md").read_text()
        self.assertIn("local_authenticated_receipt", markdown)
        self.assertNotIn("DO_NOT_COPY", json.dumps(report) + markdown)
        self.assertNotIn(self.url, json.dumps(report) + markdown)
        self.assertNotIn("e" * 64, json.dumps(report) + markdown)
        child = load("controlled_http_reader", "p2p-triad-http-profile.py")
        as_of = profile["observed_at_unix_ms"] + child.adapter.MAX_AGE_MS + 1
        stale = child.refresh_report(report, as_of)["nodes"]["local_node"]["profile_readiness"]
        self.assertEqual(stale["reason"], "controlled_host_observation_expired")
        self.assertEqual(stale["source_freshness"], "stale")
        self.assertEqual(stale["observed_at_unix_ms"], profile["observed_at_unix_ms"])

    def test_controlled_receipt_projection_preserves_verifier_time_and_rejects_bad_receipts(self):
        # Exercise only the collector's DTO contract; the shipping Rust verifier
        # separately verifies the real signed TCP response. These fixture
        # commitments do not establish P/R authority or business durability.
        inventory = self.controlled_inventory_fixture()
        runtime = self.fixture.root / "fixture-status-verifier"
        original = runtime.read_text()
        child = load("controlled_receipt_reader", "p2p-triad-http-profile.py")
        digest_key = "authenticated_status_receipt_sha256"
        time_key = "authenticated_status_verified_at_unix_ms"
        options = ["--controlled-status-inventory", str(inventory)]

        def collect(extra, expected):
            runtime.write_text(original.replace("print(json.dumps(result))",
                "import time\n" + extra + "\nprint(json.dumps(result))"))
            process = self.fixture.start(self.env, options)
            out, err = process.communicate(timeout=20)
            self.assertEqual(process.returncode, expected, (out.decode(), err.decode()))
            report = json.loads((self.fixture.out / "latest_summary.json").read_text())
            markdown = (self.fixture.out / "latest_summary.md").read_text()
            return report, markdown

        valid = (
            'result["authenticated_status_receipt_sha256"] = "1" * 64\n'
            'result["authenticated_status_verified_at_unix_ms"] = time.time_ns() // 1_000_000\n'
            'time.sleep(0.05)'
        )
        report, markdown = collect(valid, 0)
        node = report["nodes"]["local_node"]
        profile = node["profile_readiness"]
        verified_at = profile[time_key]
        self.assertEqual(profile[digest_key], "1" * 64)
        self.assertEqual(profile["observed_at_unix_ms"], verified_at)
        self.assertLess(verified_at, node["profile_collection"]["finished_at_unix_ms"])
        self.assertEqual(profile["sample_time_basis"], "local_host_verification")
        self.assertEqual(profile["authority_qualification"], "unknown")
        self.assertFalse(profile["can_propose"])
        self.assertIn("1" * 64, markdown)
        self.assertIn(str(verified_at), markdown)
        expired = child.refresh_report(report, verified_at + child.adapter.MAX_AGE_MS + 1)
        stale = expired["nodes"]["local_node"]["profile_readiness"]
        self.assertEqual(stale["source_freshness"], "stale")
        self.assertEqual(stale[digest_key], "1" * 64)
        self.assertEqual(stale[time_key], verified_at)
        self.assertFalse(stale["can_propose"])

        for extra, reason in (
            (valid + '\ndel result["authenticated_status_verified_at_unix_ms"]',
             "controlled_status_receipt_invalid"),
            (valid + '\nresult["authenticated_status_receipt_sha256"] = "bad"',
             "controlled_status_receipt_invalid"),
            (valid + '\nresult["authenticated_status_verified_at_unix_ms"] = 0',
             "controlled_status_receipt_clock_uncertain"),
            (valid + '\nresult["authenticated_status_verified_at_unix_ms"] += 60_000',
             "controlled_status_receipt_clock_uncertain"),
        ):
            with self.subTest(reason=reason, extra=extra):
                rejected, markdown = collect(extra, 1)
                node = rejected["nodes"]["local_node"]
                self.assertFalse(node["profile_collection"]["complete"])
                self.assertEqual(node["profile_readiness"]["reason"], reason)
                self.assertNotIn(digest_key, node["profile_readiness"])
                self.assertIn(reason, markdown)
                self.assertFalse(node["profile_readiness"]["can_propose"])

        missing_clock, _ = collect(valid + '\nresult["authenticated_status_verified_at_unix_ms"] = None', 0)
        profile = missing_clock["nodes"]["local_node"]["profile_readiness"]
        self.assertEqual(profile[digest_key], "1" * 64)
        self.assertIsNone(profile["observed_at_unix_ms"])
        self.assertIsNone(profile["age_ms"])
        self.assertEqual(profile["source_freshness"], "unknown")
        self.assertEqual(profile["reason"], "controlled_status_receipt_clock_unavailable")
        self.assertFalse(profile["can_propose"])

        recovered, markdown = collect(valid.replace('"1" * 64', '"2" * 64'), 0)
        node = recovered["nodes"]["local_node"]
        self.assertTrue(node["profile_collection"]["complete"])
        self.assertEqual(node["profile_readiness"][digest_key], "2" * 64)
        self.assertEqual(node["profile_readiness"]["authority_qualification"], "unknown")
        self.assertFalse(node["profile_readiness"]["can_propose"])
        self.assertIn("2" * 64, markdown)

    def test_controlled_signed_receipt_scope_refusal_and_cold_report_recovery(self):
        # The selected shipping verifier owns authentication. The report must
        # never promote its diagnostic receipt into protected P/R authority.
        inventory = self.controlled_inventory_fixture()
        runtime = self.fixture.root / "fixture-status-verifier"
        original = runtime.read_text()
        options = ["--controlled-status-inventory", str(inventory)]
        child = load("controlled_receipt_recovery_reader", "p2p-triad-http-profile.py")

        def collect(extra):
            runtime.write_text(original.replace("print(json.dumps(result))",
                "import time\n" + extra + "\nprint(json.dumps(result))"))
            process = self.fixture.start(self.env, options)
            out, err = process.communicate(timeout=20)
            report = json.loads((self.fixture.out / "latest_summary.json").read_text())
            markdown = (self.fixture.out / "latest_summary.md").read_text()
            return process.returncode, report["nodes"]["local_node"], markdown, (out, err)

        receipt = (
            'result["authenticated_status_receipt_sha256"] = "4" * 64\n'
            'result["authenticated_status_verified_at_unix_ms"] = time.time_ns() // 1_000_000'
        )
        code, node, markdown, output = collect(receipt)
        self.assertEqual(code, 0, output)
        profile = node["profile_readiness"]
        verified = profile["authenticated_status_verified_at_unix_ms"]
        self.assertEqual(profile["authenticated_status_receipt_sha256"], "4" * 64)
        self.assertEqual(profile["observed_at_unix_ms"], verified)
        self.assertEqual(profile["authority_qualification"], "unknown")
        self.assertFalse(profile["can_propose"])
        self.assertIn("4" * 64, markdown)
        self.assertNotIn("DO_NOT_COPY", markdown)
        stale = child.refresh_node(node, verified + child.adapter.MAX_AGE_MS + 1)["profile_readiness"]
        self.assertEqual(stale["source_freshness"], "stale")
        self.assertEqual(stale["observed_at_unix_ms"], verified)
        self.assertFalse(stale["can_propose"])

        for mutation in (
            'result["node_id"] = "wrong-host"',
            'result["world"]["world_id"] = "wrong-world"',
            'result["authority_qualification"] = "qualified"',
            'result["host_reports_ready"] = False',
            'result["authenticated_status_verified_at_unix_ms"] += 60_000',
        ):
            with self.subTest(mutation=mutation):
                rc, rejected, markdown, output = collect(receipt + "\n" + mutation)
                self.assertEqual(rc, 1, output)
                self.assertFalse(rejected["profile_collection"]["complete"])
                self.assertEqual(rejected["profile_readiness"]["status"], "unknown")
                self.assertFalse(rejected["profile_readiness"]["can_propose"])
                self.assertNotIn("4" * 64, json.dumps(rejected) + markdown)
                self.assertNotIn("DO_NOT_COPY", json.dumps(rejected) + markdown)

        recovered_receipt = receipt.replace('"4" * 64', '"5" * 64')
        rc, recovered, markdown, output = collect(recovered_receipt)
        self.assertEqual(rc, 0, output)
        self.assertTrue(recovered["profile_collection"]["complete"])
        profile = recovered["profile_readiness"]
        self.assertEqual(profile["authenticated_status_receipt_sha256"], "5" * 64)
        self.assertEqual(profile["authority_qualification"], "unknown")
        self.assertFalse(profile["can_propose"])
        self.assertNotIn("4" * 64, json.dumps(recovered) + markdown)
        self.assertIn("5" * 64, markdown)

    def test_controlled_verifier_stdout_is_bounded_before_collection_and_recovers(self):
        inventory = self.controlled_inventory_fixture()
        runtime = self.fixture.root / "fixture-status-verifier"
        original = runtime.read_bytes()
        for output_bytes in (64 * 1024 + 1, 1024 * 1024):
            with self.subTest(output_bytes=output_bytes):
                runtime.write_text('''#!/usr/bin/env python3
import sys
sys.stdout.buffer.write(b"X" * %d)
''' % output_bytes)
                process = self.fixture.start(self.env, [
                    "--controlled-status-inventory", str(inventory),
                ])
                out, err = process.communicate(timeout=20)
                self.assertEqual(process.returncode, 1, (out.decode(), err.decode()))
                report = json.loads((self.fixture.out / "latest_summary.json").read_text())
                node = report["nodes"]["local_node"]
                self.assertEqual(node["profile_collection"]["result"], "output_limit")
                self.assertFalse(node["profile_collection"]["complete"])
                self.assertEqual(node["profile_readiness"]["status"], "unknown")
                self.assertFalse(node["profile_readiness"]["can_propose"])
                self.assertNotIn("XXXX", json.dumps(report))

        # A later valid observation uses the same configured consumer without
        # borrowing failed stdout or turning host diagnostics into authority.
        runtime.write_bytes(original)
        process = self.fixture.start(self.env, [
            "--controlled-status-inventory", str(inventory),
        ])
        out, err = process.communicate(timeout=20)
        self.assertEqual(process.returncode, 0, (out.decode(), err.decode()))
        recovered = json.loads((self.fixture.out / "latest_summary.json").read_text())
        node = recovered["nodes"]["local_node"]
        self.assertTrue(node["profile_collection"]["complete"])
        self.assertEqual(node["profile_readiness"]["source"], "verified_controlled_status")
        self.assertEqual(node["profile_readiness"]["committed_position"], 8)
        self.assertFalse(node["profile_readiness"]["can_propose"])


    def test_controlled_verifier_nonfinite_json_fails_closed_and_recovers(self):
        inventory = self.controlled_inventory_fixture()
        runtime = self.fixture.root / "fixture-status-verifier"
        original = runtime.read_text()
        self.assertIn("print(json.dumps(result))", original)
        runtime.write_text(original.replace(
            "print(json.dumps(result))",
            'print(json.dumps(dict(result, untrusted_nonfinite=float("nan"))))',
        ))
        process = self.fixture.start(self.env, ["--controlled-status-inventory", str(inventory)])
        out, err = process.communicate(timeout=20)
        self.assertEqual(process.returncode, 1, (out.decode(), err.decode()))
        rejected = json.loads((self.fixture.out / "latest_summary.json").read_text())["nodes"]["local_node"]
        self.assertEqual(rejected["profile_collection"]["result"], "invalid_status")
        self.assertFalse(rejected["profile_collection"]["complete"])
        self.assertEqual(rejected["profile_readiness"]["status"], "unknown")
        self.assertFalse(rejected["profile_readiness"]["can_propose"])

        runtime.write_text(original)
        process = self.fixture.start(self.env, ["--controlled-status-inventory", str(inventory)])
        out, err = process.communicate(timeout=20)
        self.assertEqual(process.returncode, 0, (out.decode(), err.decode()))
        recovered = json.loads((self.fixture.out / "latest_summary.json").read_text())["nodes"]["local_node"]
        self.assertTrue(recovered["profile_collection"]["complete"])
        self.assertEqual(recovered["profile_readiness"]["source"], "verified_controlled_status")
        self.assertEqual(recovered["profile_readiness"]["committed_position"], 8)
        self.assertFalse(recovered["profile_readiness"]["can_propose"])

    @unittest.skipUnless(os.name == "posix", "process-group cleanup uses POSIX sessions")
    def test_controlled_verifier_timeout_reaps_descendant_holding_stdout_and_recovers(self):
        # The verifier exits but its ordinary child inherits the stdout pipe.
        # Without group termination, the collector's timeout is extended
        # until that child exits even though the direct verifier has gone.
        child = load("controlled_descendant_reader", "p2p-triad-http-profile.py")
        marker = self.fixture.root / "verifier-descendant-started"
        verifier = (
            "import subprocess, sys\n"
            "from pathlib import Path\n"
            "subprocess.Popen([sys.executable, '-c', "
            "'import time;time.sleep(12)'])\n"
            "Path(sys.argv[1]).write_text('spawned')\n"
            "print('parent exited', flush=True)\n"
        )
        started = time.monotonic()
        output, status = child.run_controlled_bounded(
            [sys.executable, "-c", verifier, str(marker)], b"{}", 3.0
        )
        elapsed = time.monotonic() - started
        self.assertTrue(marker.is_file(), "fixture must spawn the inheriting child before timeout")
        self.assertEqual((output, status), (None, "timeout"))
        self.assertLess(elapsed, 7.0, "timeout cleanup must not wait for the child to exit")

        # A later healthy child must still return an unmodified bounded result.
        recovered, status = child.run_controlled_bounded(
            [sys.executable, "-c", "import sys; sys.stdin.read(); print('ok')"],
            b"{}", 4.0,
        )
        self.assertEqual((recovered, status), (b"ok\n", "success"))

    @unittest.skipUnless(os.name == "posix", "detached process fixture needs POSIX sessions")
    def test_controlled_verifier_detached_stdout_does_not_extend_timeout(self):
        # The grandchild starts a DIFFERENT session. Killing the verifier's
        # process group cannot close the inherited stdout pipe.
        child = load("controlled_detached_reader", "p2p-triad-http-profile.py")
        marker = self.fixture.root / "detached-verifier-child-pid"
        detached = (
            "import os,sys,time\n"
            "from pathlib import Path\n"
            "Path(sys.argv[1]).write_text(str(os.getpid()))\n"
            "time.sleep(12)\n"
        )
        verifier = (
            "import os,subprocess,sys,time\n"
            f"subprocess.Popen([sys.executable, '-c', {detached!r}, sys.argv[1]], "
            "start_new_session=True)\n"
            "for _ in range(200):\n"
            "    if os.path.exists(sys.argv[1]): break\n"
            "    time.sleep(.01)\n"
            "print('parent exited', flush=True)\n"
        )
        started = time.monotonic()
        try:
            output, status = child.run_controlled_bounded(
                [sys.executable, "-c", verifier, str(marker)], b"{}", 2.0
            )
            elapsed = time.monotonic() - started
            self.assertTrue(marker.is_file(), "fixture must detach before timeout")
            self.assertEqual((output, status), (None, "timeout"))
            self.assertLess(elapsed, 5.0, "cleanup cannot wait for detached child EOF")
        finally:
            # Unlike the verified same-session case, a detached child is
            # intentionally outside our process group; test must reap it.
            if marker.is_file():
                try:
                    os.kill(int(marker.read_text()), signal.SIGKILL)
                except ProcessLookupError:
                    pass

        recovered, status = child.run_controlled_bounded(
            [sys.executable, "-c", "import sys; sys.stdin.read(); print('ok')"],
            b"{}", 4.0,
        )
        self.assertEqual((recovered, status), (b"ok\n", "success"))


    def test_controlled_owner_observation_keeps_its_original_sample_time(self):
        inventory = self.controlled_inventory_fixture()
        config = self.fixture.root / "owner-private-config"
        pins = json.loads(inventory.read_text())
        pins["slots"]["local_node"]["owner_config"] = str(config)
        inventory.write_text(json.dumps(pins))
        endpoint = dict(status="authenticated", head=dict(position=8, record_id="f" * 64),
                        instance_digest="b" * 64, checkpoint_revision=3, epoch_floor=2,
                        sealed=False, native_request_pending=True, private_path="DO_NOT_COPY")
        started_at = time.time_ns() // 1_000_000
        pair = dict(status="observed", sample_started_at_unix_ms=started_at,
                    finalizing_authority_id="a" * 64, both_heads_match_unchanged_cut=True,
                    primary=endpoint, replica=endpoint)
        for expected in ("observed", "invalid_observation"):
            config.write_text(json.dumps(pair))
            process = self.fixture.start(self.env, ["--controlled-status-inventory", str(inventory)])
            out, err = process.communicate(timeout=20)
            self.assertEqual(process.returncode, 0, (out.decode(), err.decode()))
            report = json.loads((self.fixture.out / "latest_summary.json").read_text())
            markdown = (self.fixture.out / "latest_summary.md").read_text()
            profile = report["nodes"]["local_node"]["profile_readiness"]
            self.assertEqual(profile["authority_qualification"], "unknown")
            self.assertFalse(profile["can_propose"])
            self.assertEqual(profile["protected_pair"]["status"], expected)
            self.assertNotIn(str(config), json.dumps(report) + markdown)
            self.assertNotIn("DO_NOT_COPY", json.dumps(report) + markdown)
            if expected == "observed":
                self.assertEqual(profile["protected_pair"]["primary"]["checkpoint_revision"], 3)
                child = load("controlled_owner_reader", "p2p-triad-http-profile.py")
                expired = child.refresh_report(report, started_at + 60_001)
                protected = expired["nodes"]["local_node"]["profile_readiness"]["protected_pair"]
                self.assertEqual(protected["freshness"], "stale")
                self.assertEqual(protected["sample_started_at_unix_ms"], started_at)
                pair["replica"] = dict(endpoint, head=dict(position=9, record_id="d" * 64))
            else:
                self.assertEqual(profile["protected_pair"], {"status": "invalid_observation"})

    def test_receipt_aware_owner_pair_rejects_cross_invocation_clock_and_recovers(self):
        inventory = self.controlled_inventory_fixture()
        runtime = self.fixture.root / "fixture-status-verifier"
        original = runtime.read_text()
        config = self.fixture.root / "owner-private-config"
        pins = json.loads(inventory.read_text())
        pins["slots"]["local_node"]["owner_config"] = str(config)
        inventory.write_text(json.dumps(pins))
        endpoint = dict(status="authenticated", head=dict(position=8, record_id="f" * 64),
                        instance_digest="b" * 64, checkpoint_revision=3, epoch_floor=2,
                        sealed=False, native_request_pending=False)
        config.write_text(json.dumps(dict(status="observed",
            finalizing_authority_id="a" * 64, both_heads_match_unchanged_cut=True,
            primary=endpoint, replica=endpoint)))
        options = ["--controlled-status-inventory", str(inventory)]
        for offset, missing_clock, expected_reason in (
                (0, False, None),
                (-120_000, False, "protected_pair_receipt_clock_uncertain"),
                (1, False, "protected_pair_receipt_clock_uncertain"),
                (0, True, "protected_pair_host_clock_unavailable"),
                (0, False, None)):
            with self.subTest(offset=offset, missing_clock=missing_clock):
                runtime.write_text(original.replace("print(json.dumps(result))",
                    "import time\n"
                    "sample = time.time_ns() // 1_000_000\n"
                    'result["authenticated_status_receipt_sha256"] = "1" * 64\n'
                    + f'result["authenticated_status_verified_at_unix_ms"] = {"None" if missing_clock else "sample"}\n'
                    + f'result["protected_pair"]["sample_started_at_unix_ms"] = sample + ({offset})\n'
                    + "time.sleep(0.05)\nprint(json.dumps(result))"))
                process = self.fixture.start(self.env, options)
                out, err = process.communicate(timeout=20)
                self.assertEqual(process.returncode, 0, (out.decode(), err.decode()))
                report = json.loads((self.fixture.out / "latest_summary.json").read_text())
                markdown = (self.fixture.out / "latest_summary.md").read_text()
                profile = report["nodes"]["local_node"]["profile_readiness"]
                pair = profile["protected_pair"]
                self.assertEqual(profile["authority_qualification"], "unknown")
                self.assertFalse(profile["can_propose"])
                self.assertEqual(profile["authenticated_status_receipt_sha256"], "1" * 64)
                if expected_reason is None:
                    self.assertEqual(pair["status"], "observed")
                    self.assertEqual(pair["sample_started_at_unix_ms"],
                                     profile["authenticated_status_verified_at_unix_ms"])
                    self.assertTrue(pair["both_heads_match_unchanged_cut"])
                else:
                    self.assertEqual(pair, {"status": "invalid_observation", "reason": expected_reason})
                    self.assertIn(expected_reason, markdown)
                    self.assertNotIn("both_heads_match_unchanged_cut", markdown)
                    self.assertNotIn("finalizing_authority_id", markdown)

    def test_controlled_inventory_rejects_conflict_and_mutation(self):
        inventory = self.controlled_inventory_fixture()
        options = ["--controlled-status-inventory", str(inventory)]
        process = self.fixture.start(self.env, options + ["--local-profile-status-url", self.url])
        process.communicate(timeout=10)
        self.assertEqual(process.returncode, 2)
        self.assertFalse(self.fixture.out.exists())

        actual = hashlib.sha256(inventory.read_bytes()).hexdigest()
        inventory.write_text(inventory.read_text() + "\n")
        summary = self.fixture.root / "mutated-inventory-summary.json"
        process = subprocess.run([
            sys.executable, str(SCRIPTS / "p2p-triad-http-profile.py"),
            "--controlled-inventory", str(inventory), "--expected-controlled-inventory-digest", actual,
            "--summary-json", str(summary),
        ], capture_output=True, timeout=10, check=False)
        self.assertEqual(process.returncode, 1)
        report = json.loads(summary.read_text())
        host = report["nodes"]["local_node"]["profile_readiness"]
        self.assertEqual(host["reason"], "controlled_inventory_unavailable")
        self.assertNotIn("host_reports_ready", host)

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
