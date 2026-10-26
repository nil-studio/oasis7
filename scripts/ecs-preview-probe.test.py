#!/usr/bin/env python3
"""Contract tests for the signed ECS DevLocal read probe."""

from __future__ import annotations

import importlib.util
import json
import os
import re
import sys
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from unittest.mock import patch


ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location("ecs_preview_probe", ROOT / "scripts/ecs_preview_probe.py")
probe = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(probe)

import blake3
import cbor2
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey, Ed25519PublicKey


WORLD = {"world_id": "devlocal-test-world", "genesis_digest": "blake3:" + "1" * 64}
PATH = "/v1/world/participant-policy"
DOMAIN = "oasis7.world-service.v1"


def raw_public_key(private_key):
    return private_key.public_key().public_bytes(serialization.Encoding.Raw, serialization.PublicFormat.Raw)


def trusted_test_module_paths():
    paths = set()
    for module in (cbor2, blake3):
        paths.add(str(Path(module.__file__).resolve().parent))
    cryptography_file = Path(sys.modules["cryptography"].__file__).resolve()
    for parent in cryptography_file.parents:
        if parent.name in ("site-packages", "dist-packages"):
            paths.add(str(parent))
            break
    else:
        paths.add(str(cryptography_file.parent.parent))
    return sorted(paths)


class SignedPolicyServer:
    def __init__(self, service_key, behavior="good", payload_change=None):
        self.service_key = service_key
        self.behavior = behavior
        self.payload_change = payload_change
        self.requests = []
        outer = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_args):
                return

            def do_POST(self):
                length = int(self.headers.get("Content-Length", "0"))
                body = self.rfile.read(length)
                try:
                    signed = json.loads(body)
                    outer.requests.append((self.path, signed))
                    if self.path != PATH:
                        self.send_error(404)
                        return
                    request = signed["request"]
                    reader_key = Ed25519PublicKey.from_public_bytes(bytes.fromhex(signed["subject_public_key"]))
                    reader_key.verify(bytes.fromhex(signed["signature_hex"]), cbor2.dumps((DOMAIN, PATH, request), canonical=True))
                    if request["contract_version"] != 1 or request["expected_world"] != WORLD:
                        self.send_error(400)
                        return
                    if request["trust_config_ref"] != raw_public_key(outer.service_key).hex():
                        self.send_error(400)
                        return
                    if not re.fullmatch(r"[0-9a-f]{64}", request["observation_nonce"]):
                        self.send_error(400)
                        return
                    signed_bytes = cbor2.dumps((DOMAIN, PATH, signed), canonical=True)
                    request_digest = "blake3:" + blake3.blake3(signed_bytes).hexdigest()
                    payload = {
                        "available": True,
                        "world_id": WORLD["world_id"],
                        "genesis_digest": WORLD["genesis_digest"],
                        "logical_tick": "42",
                        "authority_source": "dev_local_operator",
                    }
                    if outer.payload_change:
                        payload.update(outer.payload_change)
                    response_signature = outer.service_key.sign(cbor2.dumps((DOMAIN, PATH, (request_digest, payload)), canonical=True)).hex()
                    if outer.behavior == "bad_signature":
                        response_signature = "00" * 64
                    response = json.dumps({"request_digest": request_digest, "payload": payload, "signature_hex": response_signature}, separators=(",", ":")).encode()
                    if outer.behavior == "large_body":
                        response += b" " * (probe.MAX_RESPONSE_BYTES + 1 - len(response))
                    if outer.behavior == "redirect":
                        self.send_response(302)
                        self.send_header("Location", "http://example.invalid/steal")
                        self.send_header("Content-Length", "0")
                        self.end_headers()
                        return
                    self.send_response(200)
                    self.send_header("Content-Type", "application/json")
                    self.send_header("Content-Length", str(len(response)))
                    self.end_headers()
                    if outer.behavior == "slow":
                        threading.Event().wait(0.2)
                    self.wfile.write(response)
                except Exception:
                    self.send_error(400)

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.server.daemon_threads = True
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    @property
    def url(self):
        return f"http://127.0.0.1:{self.server.server_address[1]}"

    def close(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=2)


class SignedLoopbackProbeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="ecs-preview-probe-")
        self.directory = Path(self.temp.name)
        self.reader_key = Ed25519PrivateKey.from_private_bytes(bytes([7]) * 32)
        self.service_key = Ed25519PrivateKey.from_private_bytes(bytes([9]) * 32)
        self.reader_key_file = self.directory / "reader.seed"
        self.reader_key_file.write_bytes(bytes([7]) * 32)
        self.reader_key_file.chmod(0o600)
        self.server = SignedPolicyServer(self.service_key)
        self.config = {
            "expected_world": dict(WORLD),
            "service_public_key": raw_public_key(self.service_key).hex(),
            "reader_public_key": raw_public_key(self.reader_key).hex(),
            "reader_key_file": str(self.reader_key_file),
            "world_service_url": self.server.url,
            "reader_uid": os.geteuid(),
        }
        self.paths_patch = patch.object(probe, "_root_trusted_default_paths", return_value=trusted_test_module_paths())
        self.paths_patch.start()

    def tearDown(self):
        self.paths_patch.stop()
        self.server.close()
        self.temp.cleanup()

    def test_preflight_and_real_signed_loopback_read(self):
        self.assertEqual(probe.preflight(self.config), raw_public_key(self.reader_key).hex())
        self.assertEqual(self.server.requests, [])
        result = probe.read_world(self.config)
        self.assertEqual(result, {
            "world_id": WORLD["world_id"],
            "genesis_digest": WORLD["genesis_digest"],
            "logical_tick": 42,
            "service_public_key": raw_public_key(self.service_key).hex(),
            "signature_verified": True,
        })
        self.assertEqual(len(self.server.requests), 1)
        path, signed_request = self.server.requests[0]
        self.assertEqual(path, PATH)
        self.assertEqual(signed_request["subject_public_key"], raw_public_key(self.reader_key).hex())
        self.assertEqual(signed_request["request"]["trust_config_ref"], self.config["service_public_key"])
        self.assertRegex(signed_request["request"]["observation_nonce"], r"^[0-9a-f]{64}$")

    def test_rejects_signed_world_or_genesis_mismatch(self):
        for field, value in (("world_id", "foreign-world"), ("genesis_digest", "blake3:" + "2" * 64)):
            with self.subTest(field=field):
                server = SignedPolicyServer(self.service_key, payload_change={field: value})
                self.server.close()
                self.server = server
                self.config["world_service_url"] = server.url
                with self.assertRaisesRegex(probe.ProbeError, "probe_world_identity_mismatch"):
                    probe.read_world(self.config)

    def test_rejects_a_bad_service_signature(self):
        self.server.close()
        self.server = SignedPolicyServer(self.service_key, behavior="bad_signature")
        self.config["world_service_url"] = self.server.url
        with self.assertRaisesRegex(probe.ProbeError, "probe_service_signature_invalid"):
            probe.read_world(self.config)

    def test_rejects_unavailable_or_non_devlocal_policy(self):
        for changed in ({"available": False}, {"authority_source": "governed"}):
            with self.subTest(changed=changed):
                self.server.close()
                self.server = SignedPolicyServer(self.service_key, payload_change=changed)
                self.config["world_service_url"] = self.server.url
                with self.assertRaisesRegex(probe.ProbeError, "probe_devlocal_policy_unavailable"):
                    probe.read_world(self.config)

    def test_rejects_malformed_or_oversized_response(self):
        self.server.close()
        self.server = SignedPolicyServer(self.service_key, behavior="large_body")
        self.config["world_service_url"] = self.server.url
        with self.assertRaisesRegex(probe.ProbeError, "probe_response_too_large"):
            probe.read_world(self.config)

    def test_does_not_follow_redirect(self):
        self.server.close()
        self.server = SignedPolicyServer(self.service_key, behavior="redirect")
        self.config["world_service_url"] = self.server.url
        with self.assertRaisesRegex(probe.ProbeError, "probe_http_failed"):
            probe.read_world(self.config)
        self.assertEqual(len(self.server.requests), 1)

    def test_rejects_bad_reader_key_pins_permissions_links_and_symlinks_before_http(self):
        bad_pin = dict(self.config, reader_public_key=(bytes([8]) * 32).hex())
        with self.assertRaisesRegex(probe.ProbeError, "probe_reader_key_pin_mismatch"):
            probe.preflight(bad_pin)
        self.assertEqual(self.server.requests, [])

        self.reader_key_file.chmod(0o640)
        with self.assertRaisesRegex(probe.ProbeError, "probe_reader_key_permissions"):
            probe.preflight(self.config)
        self.reader_key_file.chmod(0o600)

        linked = self.directory / "hardlink.seed"
        os.link(self.reader_key_file, linked)
        with self.assertRaisesRegex(probe.ProbeError, "probe_reader_key_file_invalid"):
            probe.preflight(dict(self.config, reader_key_file=str(linked)))
        linked.unlink()

        symlink = self.directory / "symlink.seed"
        symlink.symlink_to(self.reader_key_file)
        with self.assertRaisesRegex(probe.ProbeError, "probe_reader_key_file_invalid"):
            probe.preflight(dict(self.config, reader_key_file=str(symlink)))
        self.assertEqual(self.server.requests, [])

    def test_rejects_untrusted_module_path_and_non_loopback_endpoint_before_http(self):
        untrusted = self.directory / "modules"
        untrusted.mkdir()
        with self.assertRaisesRegex(probe.ProbeError, "probe_python_path_untrusted"):
            probe.preflight(dict(self.config, python_path=[str(untrusted.resolve())]))
        with self.assertRaisesRegex(probe.ProbeError, "probe_loopback_url_invalid"):
            probe.preflight(dict(self.config, world_service_url="http://example.invalid"))
        self.assertEqual(self.server.requests, [])

    def test_parses_only_a_canonical_unsigned_logical_tick(self):
        for value in ("-1", "01", str(1 << 64), 42):
            with self.subTest(value=value):
                self.server.close()
                self.server = SignedPolicyServer(self.service_key, payload_change={"logical_tick": value})
                self.config["world_service_url"] = self.server.url
                with self.assertRaisesRegex(probe.ProbeError, "probe_logical_tick_invalid"):
                    probe.read_world(self.config)


class ProbePreflightFailureTests(unittest.TestCase):
    def test_dependency_failure_is_preflight_only_and_sanitized(self):
        config = {
            "expected_world": dict(WORLD),
            "service_public_key": "11" * 32,
            "reader_public_key": "22" * 32,
            "reader_key_file": "/not/read/by/dependency-test",
            "world_service_url": "http://127.0.0.1:8080",
        }
        with patch.object(probe, "_load_dependencies", side_effect=probe.ProbeError("probe_dependency_missing")):
            with self.assertRaisesRegex(probe.ProbeError, "probe_dependency_missing") as error:
                probe.preflight(config)
        self.assertEqual(str(error.exception), "probe_dependency_missing")

    def test_rejects_bad_pins_and_non_loopback_config_without_importing_dependencies(self):
        config = {
            "expected_world": dict(WORLD),
            "service_public_key": "not-a-key",
            "reader_public_key": "22" * 32,
            "reader_key_file": "/not/read/by-config-test",
            "world_service_url": "https://127.0.0.1:8080",
        }
        with patch.object(probe, "_load_dependencies", side_effect=AssertionError("dependencies were loaded")):
            with self.assertRaisesRegex(probe.ProbeError, "probe_service_key_invalid"):
                probe.preflight(config)


if __name__ == "__main__":
    unittest.main()
