#!/usr/bin/env python3
"""Contract tests for the governed full-network clean-room plan."""

from __future__ import annotations

import atexit
import contextlib
import importlib.machinery
import importlib.util
import copy
import hashlib
import io
import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile
import types
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "scripts" / "p2p-public-testnet-full-network-clean-room.py"
SIDECAR_PATH = ROOT / "scripts" / "p2p-public-testnet-identity-receipt-v2.py"
SIGNING_TEST_PATH = ROOT / "scripts" / "p2p-public-testnet-identity-v2-signing-tool.test.py"
FIXTURE_PEERS = {
    "storage-205": "12D3KooWtriadtestnetstorage",
    "sequencer-204": "12D3KooWtriadtestnetsequencer",
    "linux-lan-observer": "12D3KooWtriadtestnetlocal",
    "windows-observer": "12D3KooWtriadtestnetwindowsobserver",
    "macos-observer": "12D3KooWtriadtestnetfourthlocal",
}
_COMPILED_MODULE_CODE: dict[str, types.CodeType] = {}


def _load_compiled_module(name: str, path: Path) -> types.ModuleType:
    key = str(path.resolve())
    code = _COMPILED_MODULE_CODE.get(key)
    if code is None:
        if not path.is_file():
            raise AssertionError(f"cannot load module: {path}")
        code = compile(path.read_bytes(), str(path), "exec")
        _COMPILED_MODULE_CODE[key] = code
    spec = importlib.machinery.ModuleSpec(name, None, origin=str(path))
    module = importlib.util.module_from_spec(spec)
    module.__file__ = str(path)
    exec(code, module.__dict__)
    return module


def load_module():
    module = _load_compiled_module("full_network_clean_room", MODULE_PATH)
    # Fixture-generator compatibility only; production never reads this field.
    module.CANONICAL_PEER_REGISTRY = dict(FIXTURE_PEERS)
    return module


def load_sidecar_module():
    return _load_compiled_module("identity_receipt_v2_sidecar", SIDECAR_PATH)


def load_signing_test_module():
    return _load_compiled_module("identity_v2_signing_tool_fixture", SIGNING_TEST_PATH)



def _fixture_digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _write_fixture_json(path: Path, value: object) -> None:
    path.write_bytes(json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode("utf-8"))


def _fixture_descriptor(path: Path) -> dict[str, object]:
    payload = path.read_bytes()
    return {"path": str(path), "sha256": hashlib.sha256(payload).hexdigest(), "size_bytes": len(payload)}


class FullNetworkCleanRoomPlanTests(unittest.TestCase):
    _baseline_fixture_ready = False

    def test_checkpoint_and_nested_payload_mutations_reject_before_publication(self):
        """Valid signatures/claims must not authorize changed recovery metadata."""
        request = self._input()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, evidence_path = root / "request.json", root / "identity-map.json"
            _write_fixture_json(source, request)
            _write_fixture_json(evidence_path, self._baseline_identity_v2_evidence)
            control = root / "valid-plan.json"
            self.assertEqual(self.module.main([
                "--input", str(source), "--identity-v2-evidence-map", str(evidence_path),
                "--out", str(control),
            ]), 0)
            self.assertEqual(json.loads(control.read_text())["truth"]["checkpoint"], request["truth"]["checkpoint"])
            cases = (
                ("checkpoint-sha", "checkpoint", "sha256", "f" * 64),
                ("checkpoint-path", "checkpoint", "receipt_path", "/operator/truth/substituted-checkpoint.json"),
                ("checkpoint-size", "checkpoint", "size_bytes", 1025),
                ("nested-output", "nested", "output_sha256", "f" * 64),
                ("nested-verifier", "nested", "verifier_id", "untrusted-verifier"),
                ("nested-root", "nested", "trust_root_id", "untrusted-root"),
            )
            for label, section, field, value in cases:
                with self.subTest(mutation=label):
                    changed = copy.deepcopy(request)
                    target = (changed["truth"]["checkpoint"] if section == "checkpoint" else
                              changed["fresh_root_probe"]["validator_verify_outputs"]["storage-205"])
                    target[field] = value
                    # Keep every existing receipt and signature byte unchanged.
                    self.assertEqual(changed["truth"]["checkpoint"]["receipt"], request["truth"]["checkpoint"]["receipt"])
                    self.assertEqual(changed["fresh_root_probe"]["receipt"], request["fresh_root_probe"]["receipt"])
                    for key in ("signed_payload_sha256", "signature_hex", "canonical_digest"):
                        self.assertEqual(changed["fresh_root_probe"]["validator_verify_outputs"]["storage-205"][key],
                                         request["fresh_root_probe"]["validator_verify_outputs"]["storage-205"][key])
                    _write_fixture_json(source, changed)
                    output = root / f"{label}-plan.json"
                    with patch.object(self.module, "_write_plan_atomic", wraps=self.module._write_plan_atomic) as publish:
                        try:
                            self.module.main(["--input", str(source), "--identity-v2-evidence-map", str(evidence_path), "--out", str(output)])
                        except SystemExit:
                            rejected = True
                        else:
                            rejected = False
                    self.assertEqual((rejected, publish.call_count, output.exists()), (True, 0, False),
                                     "unchanged receipts must reject mutated checkpoint/nested payload before publication")

    _baseline_evidence_cache: dict[str, object] | None = None
    _baseline_distinct_evidence_cache: dict[str, object] | None = None
    _baseline_artifacts_map: dict[str, dict[str, Path]] | None = None
    _cached_baseline_verified: dict[str, dict[str, object]] | None = None
    _baseline_evidence_file_digests: tuple[tuple[Path, str], ...] = ()
    _atexit_registered = False

    @classmethod
    def setUpClass(cls) -> None:
        """Create one independent, real-crypto v2 authority before request mutations."""
        if cls._baseline_fixture_ready:
            return
        cls._baseline_module = load_module()
        cls._baseline_signing_module = load_signing_test_module()
        cls._baseline_signing = cls._baseline_signing_module.IdentityV2SigningToolContractTests(
            "runTest"
        )
        cls._baseline_signing.setUp()
        cls._semantic_signature_cache = {}
        cls._semantic_root_info = None
        cls._configure_semantic_authority(cls._baseline_module)
        cls._baseline_signing.peer_registry = cls._baseline_signing.root / "managed-peers.json"
        peer_authority = cls._baseline_module._peer_registry_authority()
        peer_authority.REGISTRY_PATH = cls._baseline_signing.peer_registry
        _write_fixture_json(peer_authority.REGISTRY_PATH, {
            "schema_version": peer_authority.SCHEMA, "network_id": peer_authority.NETWORK_ID,
            "registry_epoch": "fixture-peer-epoch-1",
            "nodes": [{"node_name": name, "node_id": peer_authority.NODE_IDS[name], "peer_id": peer}
                      for name, peer in cls._baseline_module.CANONICAL_PEER_REGISTRY.items()],
        })
        peer_authority.REGISTRY_PATH.chmod(0o600)
        peer_authority.REGISTRY_SHA256 = _fixture_digest(peer_authority.REGISTRY_PATH)
        cls._baseline_peer_pins = (peer_authority.REGISTRY_PATH, peer_authority.REGISTRY_SHA256)
        cls._baseline_impact_directory = tempfile.TemporaryDirectory(
            prefix="oasis7-planner-baseline-"
        )
        builder = cls("runTest")
        builder.module = cls._baseline_module
        builder.signing = cls._baseline_signing
        builder._impact_path = Path(cls._baseline_impact_directory.name) / "consumer-impact.json"
        builder._baseline_request = builder._input()
        builder._align_signing_context()
        cls._baseline_anchor_pins = (
            cls._baseline_signing.verifier,
            _fixture_digest(cls._baseline_signing.verifier),
            cls._baseline_signing.trust,
            _fixture_digest(cls._baseline_signing.trust),
            cls._baseline_signing.registry,
            _fixture_digest(cls._baseline_signing.registry),
        )
        cls._baseline_anchor_stats = tuple(
            (p.stat().st_mode, p.stat().st_uid, p.stat().st_ino)
            for p in (cls._baseline_signing.verifier, cls._baseline_signing.trust, cls._baseline_signing.registry)
        )
        cls._baseline_evidence_cache = None
        cls._baseline_distinct_evidence_cache = None
        cls._baseline_artifacts_map = None
        cls._cached_baseline_verified = None
        cls._baseline_evidence_file_digests = ()
        cls._baseline_request = builder._baseline_request
        cls._baseline_builder = builder
        cls._baseline_fixture_ready = True
        if not cls._atexit_registered:
            atexit.register(cls.tearDownClass)
            cls._atexit_registered = True

    @classmethod
    def _collect_evidence_file_digests(cls, evidence: dict[str, object]) -> tuple[tuple[Path, str], ...]:
        items: list[tuple[Path, str]] = []
        for top_key in ("context", "plan_intent"):
            desc = evidence[top_key]
            items.append((Path(desc["path"]), str(desc["sha256"])))
        for entry in evidence["entries"]:
            for field in (
                "raw_v1", "prepare_manifest", "payload", "provider_attestation",
                "unsigned_envelope", "signed_envelope", "verification",
            ):
                desc = entry[field]
                items.append((Path(desc["path"]), str(desc["sha256"])))
        return tuple(items)

    @classmethod
    def _ensure_baseline_evidence(cls) -> None:
        cls.setUpClass()
        if cls._baseline_evidence_cache is not None:
            return
        builder = cls._baseline_builder
        artifacts = builder._make_baseline_signed_artifacts(label="baseline")
        cls._baseline_artifacts_map = artifacts
        cls._baseline_evidence_cache = builder._write_baseline_evidence_map(
            artifacts, label="baseline"
        )
        cls._baseline_evidence_file_digests = cls._collect_evidence_file_digests(
            cls._baseline_evidence_cache
        )

    @classmethod
    def _get_baseline_evidence(cls) -> dict[str, object]:
        cls._ensure_baseline_evidence()
        assert cls._baseline_evidence_cache is not None
        return cls._baseline_evidence_cache

    @classmethod
    def _get_baseline_distinct_evidence(cls) -> dict[str, object]:
        cls.setUpClass()
        if cls._baseline_distinct_evidence_cache is None:
            builder = cls._baseline_builder
            distinct_artifacts = builder._make_baseline_signed_artifacts(
                expected_uid=1001, expected_gid=1002, label="distinct-uid-gid"
            )
            cls._baseline_distinct_evidence_cache = builder._write_baseline_evidence_map(
                distinct_artifacts, label="distinct-uid-gid"
            )
        return cls._baseline_distinct_evidence_cache

    @property
    def _baseline_artifacts(self) -> dict[str, dict[str, Path]]:
        if hasattr(self, "_instance_baseline_artifacts"):
            return self._instance_baseline_artifacts
        type(self)._ensure_baseline_evidence()
        assert type(self)._baseline_artifacts_map is not None
        return type(self)._baseline_artifacts_map

    @_baseline_artifacts.setter
    def _baseline_artifacts(self, value: dict[str, dict[str, Path]]) -> None:
        self._instance_baseline_artifacts = value

    @property
    def _baseline_identity_v2_evidence(self) -> dict[str, object]:
        if not hasattr(self, "_instance_baseline_identity_v2_evidence"):
            self._instance_baseline_identity_v2_evidence = copy.deepcopy(
                type(self)._get_baseline_evidence()
            )
        return self._instance_baseline_identity_v2_evidence

    @_baseline_identity_v2_evidence.setter
    def _baseline_identity_v2_evidence(self, value: dict[str, object]) -> None:
        self._instance_baseline_identity_v2_evidence = value

    @classmethod
    def tearDownClass(cls) -> None:
        if not cls._baseline_fixture_ready:
            return
        cls._in_process_signing_modules.pop(id(cls._baseline_signing), None)
        cls._baseline_signing.tearDown()
        cls._baseline_impact_directory.cleanup()
        cls._baseline_evidence_cache = None
        cls._baseline_distinct_evidence_cache = None
        cls._baseline_artifacts_map = None
        cls._cached_baseline_verified = None
        cls._baseline_evidence_file_digests = ()
        cls._semantic_root_info = None
        cls._baseline_fixture_ready = False

    def _can_reuse_cached_baseline_verification(
        self, evidence: object, context_path: Path, intent_path: Path
    ) -> bool:
        baseline = type(self)
        if baseline._cached_baseline_verified is None or baseline._baseline_evidence_cache is None:
            return False
        if self.module.subprocess.run is not subprocess.run:
            return False
        if not baseline._baseline_signing.verifier_invocation_marker.exists():
            return False
        current_pins = (
            self.module.IDENTITY_V2_VERIFY_TOOL_PATH,
            self.module.IDENTITY_V2_VERIFY_TOOL_SHA256,
            self.module.IDENTITY_V2_TRUST_CONFIG_PATH,
            self.module.IDENTITY_V2_TRUST_CONFIG_SHA256,
            self.module.IDENTITY_V2_PROVIDER_REGISTRY_PATH,
            self.module.IDENTITY_V2_PROVIDER_REGISTRY_SHA256,
        )
        if current_pins != baseline._baseline_anchor_pins:
            return False
        try:
            anchor_files = (
                baseline._baseline_signing.verifier,
                baseline._baseline_signing.trust,
                baseline._baseline_signing.registry,
            )
            for idx, anchor_path in enumerate(anchor_files):
                st = anchor_path.lstat()
                if (st.st_mode, st.st_uid, st.st_ino) != baseline._baseline_anchor_stats[idx]:
                    return False
                if _fixture_digest(anchor_path) != baseline._baseline_anchor_pins[idx * 2 + 1]:
                    return False
            peer_auth = self.module._peer_registry_authority()
            if (peer_auth.REGISTRY_PATH, peer_auth.REGISTRY_SHA256) != baseline._baseline_peer_pins:
                return False
            if _fixture_digest(peer_auth.REGISTRY_PATH) != baseline._baseline_peer_pins[1]:
                return False
            if evidence != baseline._baseline_evidence_cache:
                return False
            if (
                Path(context_path).resolve() != Path(baseline._baseline_evidence_cache["context"]["path"]).resolve()
                or Path(intent_path).resolve() != Path(baseline._baseline_evidence_cache["plan_intent"]["path"]).resolve()
            ):
                return False
            for file_path, expected_sha in baseline._baseline_evidence_file_digests:
                st = file_path.lstat()
                if not stat.S_ISREG(st.st_mode) or (st.st_mode & 0o777) != 0o600 or st.st_uid != os.getuid():
                    return False
                if _fixture_digest(file_path) != expected_sha:
                    return False
        except OSError:
            return False
        return True

    def setUp(self) -> None:
        type(self).setUpClass()
        self.module = load_module()
        type(self)._configure_semantic_authority(self.module)
        self._impact_directory = tempfile.TemporaryDirectory()
        self._impact_path = Path(self._impact_directory.name) / "consumer-impact.json"
        baseline = type(self)
        self.module._PEER_REGISTRY_MODULE = baseline._baseline_module._peer_registry_authority()
        self._build_plan_without_evidence = self.module.build_plan
        (
            self.module.IDENTITY_V2_VERIFY_TOOL_PATH,
            self.module.IDENTITY_V2_VERIFY_TOOL_SHA256,
            self.module.IDENTITY_V2_TRUST_CONFIG_PATH,
            self.module.IDENTITY_V2_TRUST_CONFIG_SHA256,
            self.module.IDENTITY_V2_PROVIDER_REGISTRY_PATH,
            self.module.IDENTITY_V2_PROVIDER_REGISTRY_SHA256,
        ) = baseline._baseline_anchor_pins

        original_verify = self.module._independently_verify_identity_v2_entries

        def cached_verify(evidence, context_path, intent_path):
            if self._can_reuse_cached_baseline_verification(evidence, context_path, intent_path):
                return copy.deepcopy(baseline._cached_baseline_verified)
            verified = original_verify(evidence, context_path, intent_path)
            if (
                baseline._cached_baseline_verified is None
                and baseline._baseline_evidence_cache is not None
                and evidence == baseline._baseline_evidence_cache
                and self.module.subprocess.run is subprocess.run
            ):
                baseline._cached_baseline_verified = copy.deepcopy(verified)
            return verified

        self.module._independently_verify_identity_v2_entries = cached_verify

        original_build_plan = self.module.build_plan

        def build_plan_with_baseline_evidence(
            request: dict[str, object], **kwargs: object
        ) -> dict[str, object]:
            if "identity_v2_evidence" not in kwargs:
                kwargs["identity_v2_evidence"] = copy.deepcopy(self._baseline_identity_v2_evidence)
            return original_build_plan(request, **kwargs)

        # Keep the legacy test bodies focused on their request mutation while
        # routing every normal admission through the immutable baseline map.
        # The missing-map test calls _build_plan_without_evidence explicitly.
        self.module.build_plan = build_plan_with_baseline_evidence

    def tearDown(self) -> None:
        self._impact_directory.cleanup()

    def _build_plan_with_evidence(
        self, request: dict[str, object], evidence: dict[str, object]
    ) -> dict[str, object]:
        return self._build_plan_without_evidence(
            request, identity_v2_evidence=copy.deepcopy(evidence)
        )

    def test_storage_first_builder_accepts_canonical_build_plan_output(self):
        """The public child builder must consume the canonical producer output."""
        parent = self.module.build_plan(self._input())
        contract = self.module.build_storage_first_contract(parent)
        self.assertEqual(contract["target_nodes"], ["storage-205"])
        self.assertEqual(
            contract["parent_bindings"]["consumer_impact_record"]["decision"],
            "proceed",
        )
        for field in (
            "package_provenance_digest",
            "deployment_inventory_digest",
            "independent_verifier",
        ):
            self.assertTrue(contract["parent_bindings"][field], field)

    def _tampered_evidence_artifact(
        self,
        evidence: dict[str, object],
        *,
        node_name: str = "storage-205",
        artifact: str = "signed_envelope",
        mutate=None,
    ) -> dict[str, object]:
        """Return a map copy whose retained bytes, not request, are tampered."""
        mutated = copy.deepcopy(evidence)
        entry = next(item for item in mutated["entries"] if item["node_name"] == node_name)
        source = Path(entry[artifact]["path"])
        payload = json.loads(source.read_text(encoding="utf-8"))
        if mutate is not None:
            mutate(payload)
        target = Path(self._impact_directory.name) / f"tampered-{node_name}-{artifact}.json"
        _write_fixture_json(target, payload)
        target.chmod(0o600)
        entry[artifact] = _fixture_descriptor(target)
        return mutated

    def _align_signing_context(self) -> None:
        """Bind the one class-level signing fixture to the baseline request."""
        context = json.loads(self.signing.context.read_text(encoding="utf-8"))
        now = datetime.now(timezone.utc).replace(microsecond=0)
        context.update(
            {
                "task_uid": self._baseline_request["task_uid"],
                "head_oid": self._baseline_request["head_oid"],
                "capture_window_id": self._baseline_request["capture_window_id"],
                "network_id": self.module.CANONICAL_NETWORK_ID,
                "rotation_epoch": self.module.CANONICAL_ROTATION_EPOCH,
                "capture_start": (now - timedelta(minutes=1)).isoformat().replace("+00:00", "Z"),
                "capture_end": (now + timedelta(days=1)).isoformat().replace("+00:00", "Z"),
                "issued_at": (now - timedelta(seconds=10)).isoformat().replace("+00:00", "Z"),
                "expires_at": (now + timedelta(days=1)).isoformat().replace("+00:00", "Z"),
            }
        )
        _write_fixture_json(self.signing.context, context)
        self.signing.context_digest = _fixture_digest(self.signing.context)
        trust = json.loads(self.signing.trust.read_text(encoding="utf-8"))
        trust["rotation_epoch"] = context["rotation_epoch"]
        for signer in trust["allowlist"]:
            signer["valid_from"] = context["capture_start"]
            signer["valid_until"] = context["expires_at"]
        _write_fixture_json(self.signing.trust, trust)
        registry = json.loads(self.signing.registry.read_text(encoding="utf-8"))
        registry["trust_config_sha256"] = _fixture_digest(self.signing.trust)
        _write_fixture_json(self.signing.registry, registry)

        intent_nodes = []
        for node in sorted(self._baseline_request["nodes"], key=lambda item: str(item["name"])):
            name = str(node["name"])
            intent_nodes.append(
                {
                    "node_name": name,
                    "node_id": str(node["node_id"]),
                    "peer_id": self.module.CANONICAL_PEER_REGISTRY[name],
                    "role": str(node["role"]),
                    "reset_surface_ids": ["config", "execution", "world"],
                }
            )
        _write_fixture_json(
            self.signing.intent,
            {
                "schema_version": "oasis7.clean_room_plan_intent.v2",
                "peer_registry_sha256": self.module._peer_registry_authority().REGISTRY_SHA256,
                "peer_registry_epoch": self.module._peer_registry_authority().load_snapshot()["registry_epoch"],
                "context_digest": self.signing.context_digest,
                "adapter_action": "public-testnet-governed-rebuild",
                "nodes": intent_nodes,
            },
        )
        self.signing.plan_digest = _fixture_digest(self.signing.intent)

    def _write_baseline_node_raw_and_template(
        self,
        node: dict[str, object],
        *,
        expected_uid: int,
        expected_gid: int,
    ) -> None:
        name = str(node["name"])
        node_id = str(node["node_id"])
        peer_id = self.module.CANONICAL_PEER_REGISTRY[name]
        raw = {
            "schema_version": "oasis7.identity_receipt.v1",
            "node_id": node_id,
            "peer_id": peer_id,
            "key_path": f"config/{node_id}-node-keypair.toml",
            "key_sha256": "7" * 64,
            "key_size_bytes": 128,
            "key_mode": 0o600,
            "key_uid": expected_uid,
            "key_gid": expected_gid,
        }
        self.signing.raw.write_bytes((json.dumps(raw, indent=2) + "\n").encode("utf-8"))
        self.signing.raw_digest = _fixture_digest(self.signing.raw)
        context = json.loads(self.signing.context.read_text(encoding="utf-8"))
        _write_fixture_json(
            self.signing.template,
            {
                "domain_separator": "oasis7.identity_receipt.v2/signature/v1",
                "schema_version": "oasis7.identity_receipt.v2",
                "signer_id": "identity-v2-ephemeral-test-signer",
                "verifier_id": "governed-receipt-verifier",
                "trust_root_id": "oasis7-public-testnet-governance-root-v1",
                "network_id": self.module.CANONICAL_NETWORK_ID,
                "task_uid": context["task_uid"],
                "head_oid": context["head_oid"],
                "frozen_head_oid": context["head_oid"],
                "plan_digest": self.signing.plan_digest,
                "context_digest": self.signing.context_digest,
                "capture_window_id": context["capture_window_id"],
                "rotation_epoch": context["rotation_epoch"],
                "issued_at": context["issued_at"],
                "expires_at": context["expires_at"],
                "node_id": node_id,
                "peer_id": peer_id,
                "key_sha256": raw["key_sha256"],
                "key_size_bytes": raw["key_size_bytes"],
                "key_mode": "0600",
                "key_uid": raw["key_uid"],
                "key_gid": raw["key_gid"],
                "signed_payload_sha256": self.signing.raw_digest,
            },
        )

    _in_process_signing_modules: dict[int, tuple[types.ModuleType, types.ModuleType]] = {}

    def _run_signing_tool_in_process(
        self, *args: str, authority_pins: tuple[str, str] | None = None
    ) -> subprocess.CompletedProcess[bytes]:
        signing = self.signing
        cached_pair = type(self)._in_process_signing_modules.get(id(signing))
        tool_path = ROOT / "scripts" / "p2p-public-testnet-identity-v2-signing-tool.py"
        adapter_path = ROOT / "scripts" / "p2p-public-testnet-full-network-clean-room-adapter.py"
        if cached_pair is None:
            adapter_mod = _load_compiled_module("oasis7_identity_v2_adapter", adapter_path)
            previous_adapter = sys.modules.get("oasis7_identity_v2_adapter")
            sys.modules["oasis7_identity_v2_adapter"] = adapter_mod
            try:
                tool_mod = _load_compiled_module("oasis7_identity_v2_signing_tool", tool_path)
            finally:
                if previous_adapter is None:
                    sys.modules.pop("oasis7_identity_v2_adapter", None)
                else:
                    sys.modules["oasis7_identity_v2_adapter"] = previous_adapter
            type(self)._in_process_signing_modules[id(signing)] = (adapter_mod, tool_mod)
        else:
            adapter_mod, tool_mod = cached_pair
        trusted_config_sha256, trusted_registry_sha256 = authority_pins or (
            _fixture_digest(signing.trust),
            _fixture_digest(signing.registry),
        )
        adapter_mod.CANONICAL_TRUST_ROOT_PATH = str(signing.governance_root)
        adapter_mod.CANONICAL_TRUST_ROOT_FILE_SHA256 = (
            "f278bc8f060cd6777d68f086fc3131edc5d6b5a6080bde09208ba69a69e3ef66"
        )
        adapter_mod.CANONICAL_TRUST_ROOT_OWNER_UID = os.getuid()
        adapter_mod.CANONICAL_TRUST_ROOT_MODE = "0600"
        tool_mod.DEPLOYED_TRUST_CONFIG = signing.trust
        tool_mod.DEPLOYED_PROVIDER_REGISTRY = signing.registry
        tool_mod.PINNED_TRUST_CONFIG_SHA256 = trusted_config_sha256
        tool_mod.PINNED_PROVIDER_REGISTRY_SHA256 = trusted_registry_sha256
        tool_mod.DEPLOYED_GOVERNANCE_ROOT = signing.governance_root
        if hasattr(signing, "peer_registry"):
            peer_auth = tool_mod._peer_registry_authority()
            peer_auth.REGISTRY_PATH = Path(str(signing.peer_registry))
            peer_auth.REGISTRY_SHA256 = _fixture_digest(signing.peer_registry)
        previous_adapter = sys.modules.get("oasis7_identity_v2_adapter")
        sys.modules["oasis7_identity_v2_adapter"] = adapter_mod
        out_buf, err_buf = io.StringIO(), io.StringIO()
        try:
            with contextlib.redirect_stdout(out_buf), contextlib.redirect_stderr(err_buf):
                try:
                    rc = tool_mod.main(list(args))
                except SystemExit as exc:
                    rc = exc.code if isinstance(exc.code, int) else (0 if exc.code is None else 1)
        finally:
            if previous_adapter is None:
                sys.modules.pop("oasis7_identity_v2_adapter", None)
            else:
                sys.modules["oasis7_identity_v2_adapter"] = previous_adapter
        return subprocess.CompletedProcess(
            [sys.executable, str(tool_path), *args],
            rc,
            out_buf.getvalue().encode("utf-8"),
            err_buf.getvalue().encode("utf-8"),
        )

    def _make_baseline_signed_artifacts(
        self, *, expected_uid: int = 0, expected_gid: int = 0, label: str
    ) -> dict[str, dict[str, Path]]:
        artifacts: dict[str, dict[str, Path]] = {}
        original_run = self.signing._run
        self.signing._run = self._run_signing_tool_in_process
        try:
            for node in self._baseline_request["nodes"]:
                name = str(node["name"])
                self._write_baseline_node_raw_and_template(
                    node, expected_uid=expected_uid, expected_gid=expected_gid
                )
                stem = "planner-baseline-" + name
                payload, manifest, _, attestation, unsigned = self.signing._prepare_sign_assemble(stem)
                verified, verification = self.signing._verify(unsigned)
                node_dir = self.signing.root / "planner-baseline-artifacts"
                node_dir.mkdir(mode=0o700, exist_ok=True)
                copied: dict[str, Path] = {}
                source_paths = {
                    "raw_v1": self.signing.raw,
                    "prepare_manifest": manifest,
                    "payload": payload,
                    "provider_attestation": attestation,
                    "unsigned_envelope": unsigned,
                    "signed_envelope": verified,
                    "verification": verification,
                }
                for field, source in source_paths.items():
                    destination = node_dir / f"{label}-{name}.{field}"
                    shutil.copyfile(source, destination)
                    destination.chmod(0o600)
                    copied[field] = destination
                artifacts[name] = copied
        finally:
            self.signing._run = original_run
        return artifacts

    def _write_baseline_evidence_map(
        self, artifacts: dict[str, dict[str, Path]], *, label: str
    ) -> dict[str, object]:
        retention = self.signing.root / f"planner-baseline-retention-{label}"
        retention.mkdir(mode=0o700, exist_ok=True)
        context = retention / "context.json"
        intent = retention / "plan-intent.json"
        shutil.copyfile(self.signing.context, context)
        shutil.copyfile(self.signing.intent, intent)
        context.chmod(0o600)
        intent.chmod(0o600)
        evidence: dict[str, object] = {
            "schema_version": self.module.IDENTITY_V2_EVIDENCE_SCHEMA,
            "network_id": self.module.CANONICAL_NETWORK_ID,
            "task_uid": self._baseline_request["task_uid"],
            "head_oid": self._baseline_request["head_oid"],
            "context": _fixture_descriptor(context),
            "plan_intent": _fixture_descriptor(intent),
            "entries": [],
        }
        artifact_fields = (
            "raw_v1",
            "prepare_manifest",
            "payload",
            "provider_attestation",
            "unsigned_envelope",
            "signed_envelope",
            "verification",
        )
        entries: list[dict[str, object]] = []
        for node_name in self.module.NODE_ORDER:
            node = next(item for item in self._baseline_request["nodes"] if item["name"] == node_name)
            source = artifacts[node_name]
            entry: dict[str, object] = {
                "node_name": node_name,
                "node_id": str(node["node_id"]),
                "peer_id": self.module.CANONICAL_PEER_REGISTRY[node_name],
            }
            for field in artifact_fields:
                retained = retention / f"{node_name}.{field}"
                shutil.copyfile(source[field], retained)
                retained.chmod(0o600)
                entry[field] = _fixture_descriptor(retained)
            entries.append(entry)
        evidence["entries"] = entries
        return evidence

    def _consumer_impact_reference(
        self, authority_instant: datetime | None = None
    ) -> dict[str, str]:
        authority_instant = authority_instant or datetime.now(timezone.utc)
        record = {
            "impact": "none",
            "evidence_source": "test-fixture-direct-observation",
            "timestamp": authority_instant.replace(microsecond=0).isoformat().replace("+00:00", "Z"),
            "validators_already_stopped": False,
            "outage_update_channel": "n/a",
            "recovery_update_checkpoint": "n/a",
            "producer_wording_approval": "n/a",
            "decision": "proceed",
        }
        payload = json.dumps(record, sort_keys=True, separators=(",", ":")).encode("utf-8")
        self._impact_path.write_bytes(payload)
        return {"path": str(self._impact_path), "sha256": hashlib.sha256(payload).hexdigest()}

    def _rewrite_consumer_impact(self, request: dict[str, object], **changes: object) -> None:
        record = json.loads(self._impact_path.read_text(encoding="utf-8"))
        record.update(changes)
        payload = json.dumps(record, sort_keys=True, separators=(",", ":")).encode("utf-8")
        self._impact_path.write_bytes(payload)
        reference = {"path": str(self._impact_path), "sha256": hashlib.sha256(payload).hexdigest()}
        request["consumer_impact_record"] = reference
        request["authority"]["consumer_impact_record"] = dict(reference)

    @staticmethod
    def _receipt(schema: str = "oasis7.authenticated_receipt.v1") -> dict[str, object]:
        return {
            "schema_version": schema,
            "verified": True,
            "authenticated": True,
            "signer_id": "governance-signer",
            "verifier_id": "governed-receipt-verifier",
            "trust_root_id": "oasis7-public-testnet-governance-root-v1",
            "signed_payload_sha256": "a" * 64,
            "signature_hex": "b" * 128,
            "canonical_digest": "c" * 64,
        }

    def _identity_receipt(
        self,
        node_id: str,
        *,
        capture_window_id: str = "capture-window-20260901-001",
        rotation_epoch: str | None = None,
        issued_at: str = "2026-09-01T00:00:00Z",
        expires_at: str = "2099-01-01T00:00:00Z",
    ) -> dict[str, object]:
        receipt = self._receipt("oasis7.identity_receipt.v2")
        receipt.update(
            {
                "node_id": node_id,
                "peer_id": f"12D3KooW{node_id.replace('-', '')}",
                "key_sha256": "7" * 64,
                "key_size_bytes": 128,
                "key_mode": "0600",
                "key_uid": 0,
                "key_gid": 0,
                "capture_window_id": capture_window_id,
                "rotation_epoch": rotation_epoch or self.module.CANONICAL_ROTATION_EPOCH,
                "issued_at": issued_at,
                "expires_at": expires_at,
            }
        )
        raw_v1 = self._raw_runtime_identity_receipt_v1_bytes(receipt)
        receipt["signed_payload_sha256"] = hashlib.sha256(raw_v1).hexdigest()
        receipt["canonical_digest"] = self.module._canonical_receipt_digest(
            receipt, excluded_fields=frozenset({"peer_id"})
        )
        return receipt

    def _no_backup_receipt(self, request: dict[str, object], expires_at: str) -> dict[str, object]:
        receipt = self._receipt("oasis7.no_backup_authority.v1")
        receipt["bindings"] = {
            "repository": "eng-cc/oasis7",
            "action": "full-network-clean-room",
            "targets": list(self.module.NODE_ORDER),
            "task_uid": request["task_uid"],
            "transaction_id": request["transaction_id"],
            "capture_window_id": request["capture_window_id"],
            "frozen_head_oid": request["head_oid"],
            "actor": "ops-actor",
            "issued_at": "2026-08-30T00:00:00Z",
            "expires_at": expires_at,
            "current_authorization": True,
            "consumer_impact_record": {
                "path": request["consumer_impact_record"]["path"],
                "sha256": request["consumer_impact_record"]["sha256"],
            },
        }
        return receipt

    @staticmethod
    def _windows_state_path(surface: str) -> str:
        surface = surface.replace("{node_id}", "triad-testnet-windows-observer")
        return rf"C:\\oasis7-deploy\\{surface.replace('/', chr(92))}"

    def _deployment_inventory(
        self,
        nodes: list[dict[str, object]],
        *,
        expected_uid: int = 0,
        expected_gid: int = 0,
        include_layout: bool = True,
        capture_window_id: str = "capture-window-20260901-001",
        rotation_epoch: str | None = None,
        issued_at: str = "2026-09-01T00:00:00Z",
        expires_at: str = "2099-01-01T00:00:00Z",
    ) -> dict[str, object]:
        inventory_nodes: dict[str, dict[str, object]] = {}
        for node in nodes:
            entry: dict[str, object] = {
                "node_id": node["node_id"],
                "expected_key_uid": expected_uid,
                "expected_key_gid": expected_gid,
                "peer_id": self.module.CANONICAL_PEER_REGISTRY[str(node["name"])],
            }
            if include_layout:
                entry.update(
                    {
                        "node_root": node["node_root"],
                        "persistent_state_paths": list(node["persistent_state_paths"]),
                    }
                )
                node_name = str(node["name"])
                path_style = (
                    "windows"
                    if self.module.EXPECTED_NODES[node_name]["platform"] == "windows-x64"
                    else "posix"
                )
                entry["node_root"] = self.module._normalized_path(
                    entry["node_root"], path_style, f"{node['name']}.node_root"
                )
                entry["persistent_state_paths"] = [
                    self.module._normalized_path(
                        path,
                        path_style,
                        f"{node['name']}.persistent_state_paths[{index}]",
                    )
                    for index, path in enumerate(entry["persistent_state_paths"])
                ]
            inventory_nodes[str(node["name"])] = entry
        inventory = {
            "schema_version": "oasis7.deployment_inventory.v2",
            "authenticated": True,
            "verified": True,
            "signer_id": "governance-signer",
            "trust_root_id": "oasis7-public-testnet-governance-root-v1",
            "nodes": inventory_nodes,
            "receipt": self._receipt("oasis7.deployment_inventory_receipt.v2"),
        }
        inventory["receipt"].update(
            {
                "capture_window_id": capture_window_id,
                "rotation_epoch": rotation_epoch or self.module.CANONICAL_ROTATION_EPOCH,
                "issued_at": issued_at,
                "expires_at": expires_at,
            }
        )
        inventory["receipt"]["canonical_digest"] = self.module._canonical_receipt_digest(
            inventory["receipt"], excluded_fields=frozenset({"signed_payload_sha256"})
        )
        inventory["receipt"]["signed_payload_sha256"] = (
            self.module._canonical_deployment_inventory_payload_digest(inventory)
        )
        return inventory

    def _bind_current_receipt_freshness(
        self,
        request: dict[str, object],
        *,
        capture_window_id: str | None = None,
        rotation_epoch: str | None = None,
        issued_at: str = "2026-09-01T00:00:00Z",
        expires_at: str = "2099-01-01T00:00:00Z",
    ) -> None:
        """Build the v2 receipt envelope used by the freshness contract."""
        freshness = {
            "capture_window_id": capture_window_id or request["capture_window_id"],
            "rotation_epoch": rotation_epoch or self.module.CANONICAL_ROTATION_EPOCH,
            "issued_at": issued_at,
            "expires_at": expires_at,
        }
        inventory_receipt = request["deployment_inventory"]["receipt"]
        inventory_receipt.update(
            {"schema_version": "oasis7.deployment_inventory_receipt.v2", **freshness}
        )
        inventory_receipt["canonical_digest"] = self.module._canonical_receipt_digest(
            inventory_receipt, excluded_fields=frozenset({"signed_payload_sha256"})
        )
        for node in request["nodes"]:
            identity_receipt = node["identity_receipt"]
            identity_receipt.update(
                {"schema_version": "oasis7.identity_receipt.v2", **freshness}
            )
            identity_receipt["canonical_digest"] = self.module._canonical_receipt_digest(
                identity_receipt, excluded_fields=frozenset({"peer_id"})
            )
        # Receipt freshness is independent metadata; the inventory payload
        # digest still covers the complete caller-supplied node inventory.
        inventory_receipt["signed_payload_sha256"] = (
            self.module._canonical_deployment_inventory_payload_digest(
                request["deployment_inventory"]
            )
        )

    @classmethod
    def _configure_semantic_authority(cls, module) -> None:
        """Relocate only the live authority fixture; use the real strict verifier."""
        signing = cls._baseline_signing
        authority = module._semantic_signing_authority()._load_adapter_module()
        if cls._semantic_root_info is None:
            key = signing.public_key.read_bytes()
            body = {"schema_version": "oasis7.validator_pair_provenance_trust_root.v1",
                    "root_id": authority.CANONICAL_TRUST_ROOT_ID,
                    "network_id": module.CANONICAL_NETWORK_ID,
                    "allowlist": [{"signer_id": "governance-signer", "algorithm": "ed25519",
                                   "public_key_hex": key.hex(),
                                   "public_key_sha256": hashlib.sha256(key).hexdigest()}]}
            body["root_digest"] = hashlib.sha256(json.dumps(
                body, ensure_ascii=True, sort_keys=True, separators=(",", ":")
            ).encode()).hexdigest()
            path = signing.root / "semantic-governance-root.json"
            _write_fixture_json(path, body)
            path.chmod(0o600)
            cls._semantic_root_info = (str(path), _fixture_digest(path), body["root_digest"])
        root_path, root_file_sha, root_digest = cls._semantic_root_info
        authority.CANONICAL_TRUST_ROOT_PATH = root_path
        authority.CANONICAL_TRUST_ROOT_FILE_SHA256 = root_file_sha
        authority.CANONICAL_TRUST_ROOT_DIGEST = root_digest
        authority.CANONICAL_TRUST_ROOT_OWNER_UID = os.getuid()

    def _sign_semantic_fixture(self, request):
        """Sign initial valid payloads only, never caller mutations at admission."""
        cls = type(self)
        signing = cls._baseline_signing
        values = [(kind, request["truth"][kind]) for kind in ("package", "genesis", "world", "checkpoint")]
        values += [("validator_verify_output", value) for value in
                   request["fresh_root_probe"]["validator_verify_outputs"].values()]
        values.append(("fresh_root_probe", request["fresh_root_probe"]))
        for kind, value in values:
            receipt = value if kind == "validator_verify_output" else value["receipt"]
            receipt["verifier_id"] = "governed-receipt-verifier"
            receipt["trust_root_id"] = "oasis7-public-testnet-governance-root-v1"
            material = self.module.canonical_semantic_receipt_payload(kind, value, receipt)
            digest = hashlib.sha256(material).hexdigest()
            if digest not in cls._semantic_signature_cache:
                payload = signing.root / "semantic-payload.bin"
                signature = signing.root / "semantic-signature.bin"
                payload.write_bytes(material)
                signing._openssl("pkeyutl", "-sign", "-inkey", str(signing.private_key),
                                 "-rawin", "-in", str(payload), "-out", str(signature))
                cls._semantic_signature_cache[digest] = signature.read_bytes().hex()
            receipt["signed_payload_sha256"] = digest
            receipt["signature_hex"] = cls._semantic_signature_cache[digest]
            receipt["canonical_digest"] = self.module._canonical_receipt_digest(receipt)
        return request

    def _input(self, *, authority_instant: datetime | None = None) -> dict[str, object]:
        transaction_id = "txn-clean-room-001"
        capture_window_id = "capture-window-20260901-001"
        task_uid = "task_174f0a5a87394012b071171cc4a52372"
        head_oid = "e" * 40
        network_id = "oasis7-public-testnet-governed-20260606"
        authority_bindings = {
            "task_uid": task_uid,
            "head_oid": head_oid,
            "network_id": network_id,
            "signer_allowlist": ["governance-signer"],
            "trust_root_id": "oasis7-public-testnet-governance-root-v1",
            "verifier_id": "governed-receipt-verifier",
        }
        consumer_impact_record = self._consumer_impact_reference(authority_instant)
        authority_bindings["consumer_impact_record"] = dict(consumer_impact_record)
        execution_bindings = {
            "execution_records_root": {
                "path": "/operator/truth/execution-records/root",
                "sha256": "a" * 64,
                "size_bytes": 16384,
            },
            "cas": {
                "root": "/operator/truth/execution-cas",
                "blake3": "b" * 64,
                "size_bytes": 32768,
            },
            "world_head": {
                "path": "/operator/truth/world-head.json",
                "sha256": "c" * 64,
                "size_bytes": 1024,
                "height": 100,
                "block_hash": "8" * 64,
                "state_root": "9" * 64,
            },
            "generated_world_sidecar": {
                "path": "/operator/truth/world/.distfs-state/sidecar-generations/index.json",
                "sha256": "d" * 64,
                "size_bytes": 4096,
                "provenance_path": "/operator/truth/world/generated-world.provenance.json",
                "provenance_sha256": "4" * 64,
                "provenance_size_bytes": 256,
            },
            "json_index_consistency": {
                "verified": True,
                "snapshot_sha256": "e" * 64,
                "snapshot_size_bytes": 8192,
                "journal_sha256": "f" * 64,
                "journal_size_bytes": 16384,
                "index_sha256": "0" * 64,
                "index_size_bytes": 4096,
            },
        }
        truth = {
            "package": {
                "package_id": "testnet-package-linux-windows-macos-001",
                "package_dir": "/operator/packages/testnet-package-linux-windows-macos-001",
                "provenance_path": "/operator/packages/testnet-package-linux-windows-macos-001/provenance.json",
                "provenance_sha256": "a" * 64,
                "provenance_size_bytes": 256,
                "commit": "d" * 40,
                "package_version": "0.0.0+testnet.001",
                "runtime_sha256": "1" * 64,
                "runtime_size_bytes": 1024,
                "genesis_sha256": "2" * 64,
                "world_sha256": "3" * 64,
                "platforms": {
                    platform: {
                        "package_sha256": "a" * 64,
                        "package_size_bytes": 4096,
                        "world_sha256": "3" * 64,
                        "world_size_bytes": 8192,
                        "world_provenance_sha256": "4" * 64,
                        "world_provenance_size_bytes": 256,
                        "commit": "d" * 40,
                    }
                    for platform in ("linux-x64", "windows-x64", "macos-arm64")
                },
                "receipt": self._receipt("oasis7.package_provenance.v1"),
            },
            "genesis": {
                "network_id": network_id,
                "chain_id": "oasis7-public-testnet-governed-20260606",
                "world_id": "oasis7-public-testnet-governed-20260606",
                "path": "/operator/truth/genesis.json",
                "size_bytes": 2048,
                "sha256": "2" * 64,
                "receipt": self._receipt("oasis7.genesis_binding.v1"),
            },
            "world": {
                "world_id": "oasis7-public-testnet-governed-20260606",
                "generation": "gen-001",
                "path": "/operator/truth/world",
                "provenance_path": "/operator/truth/world-provenance.json",
                "size_bytes": 8192,
                "sha256": "3" * 64,
                "provenance_sha256": "4" * 64,
                "provenance_size_bytes": 256,
                "receipt": self._receipt("oasis7.world_binding.v1"),
            },
            "execution": execution_bindings,
            "checkpoint": {
                "checkpoint_id": "checkpoint-001",
                "manifest_hash": "5" * 64,
                "height": 100,
                "receipt_path": "/operator/truth/checkpoint-receipt.json",
                "size_bytes": 512,
                "execution_block_hash": "8" * 64,
                "execution_state_root": "9" * 64,
                "sha256": "6" * 64,
                "receipt": self._receipt("oasis7.checkpoint_binding.v1"),
            },
        }
        nodes = [
            {
                "name": "sequencer-204",
                "node_id": "triad-testnet-sequencer",
                "role": "validator",
                "platform": "linux-x64",
                "node_root": "/opt/oasis7/p2p-testnet",
                "service_manager": "systemd",
                "service": "oasis7-triad-sequencer.service",
                "host_binding": {
                    "target": "root@39.104.204.172",
                    "known_host_fingerprint": "SHA256:7NkC2GehDCcN+IWPbaxh+0JuIVGCEtKpdK69S6fHZPU",
                    "known_hosts_path": "/opt/oasis7/p2p-testnet/config/public-testnet-validator-pair-known-hosts",
                },
                "endpoints": {
                    "healthz": "http://127.0.0.1:6631/healthz",
                    "evidence": "http://127.0.0.1:6631/v1/chain/rebuild-proof",
                },
                "credential_seam": {
                    "kind": "temporary-fd-or-environment",
                    "environment_name": "PUBLIC_TESTNET_SEQUENCER_SSHPASS",
                    "nonce": "sequencer-nonce-" + "x" * 32,
                    "issued_at": "2026-08-30T00:00:00Z",
                    "expires_at": "2099-01-01T00:00:00Z",
                    "ledger_path": "/operator/credential-nonce-ledger.jsonl",
                    "one_shot": True,
                },
                "persistent_state_paths": [
                    f"/opt/oasis7/p2p-testnet/{surface}"
                    for surface in self.module.VALIDATOR_RESET_SURFACES
                ],
                "identity_receipt": self._identity_receipt("triad-testnet-sequencer"),
            },
            {
                "name": "storage-205",
                "node_id": "triad-testnet-storage",
                "role": "validator",
                "platform": "linux-x64",
                "node_root": "/opt/oasis7/p2p-testnet",
                "service_manager": "systemd",
                "service": "oasis7-triad-storage.service",
                "host_binding": {
                    "target": "root@39.104.205.67",
                    "known_host_fingerprint": "SHA256:1SVgiaT5JLCw8PsPpVfLE9UyWNf82IJDZsiE7LAa1gI",
                    "known_hosts_path": "/opt/oasis7/p2p-testnet/config/public-testnet-validator-pair-known-hosts",
                },
                "endpoints": {
                    "healthz": "http://127.0.0.1:6632/healthz",
                    "evidence": "http://127.0.0.1:6632/v1/chain/status",
                },
                "credential_seam": {
                    "kind": "temporary-fd-or-environment",
                    "environment_name": "PUBLIC_TESTNET_STORAGE_SSHPASS",
                    "nonce": "storage-nonce-" + "x" * 32,
                    "issued_at": "2026-08-30T00:00:00Z",
                    "expires_at": "2099-01-01T00:00:00Z",
                    "ledger_path": "/operator/credential-nonce-ledger.jsonl",
                    "one_shot": True,
                },
                "persistent_state_paths": [
                    f"/opt/oasis7/p2p-testnet/{surface}"
                    for surface in self.module.VALIDATOR_RESET_SURFACES
                ],
                "identity_receipt": self._identity_receipt("triad-testnet-storage"),
            },
            {
                "name": "linux-lan-observer",
                "node_id": "triad-testnet-local",
                "role": "observer",
                "platform": "linux-x64",
                "node_root": "/opt/oasis7/p2p-testnet-local",
                "service_manager": "systemd",
                "service": "oasis7-testnet-observer.service",
                "host_binding": {
                    "target": "observer@linux-lan",
                    "known_host_fingerprint": "SHA256:2NkC2GehDCcN+IWPbaxh+0JuIVGCEtKpdK69S6fHZPU",
                    "known_hosts_path": "/operator/known-hosts",
                },
                "endpoints": {
                    "healthz": "http://127.0.0.1:6633/healthz",
                    "evidence": "http://127.0.0.1:6633/v1/chain/status",
                },
                "credential_seam": {
                    "kind": "temporary-fd-or-environment",
                    "environment_name": "PUBLIC_TESTNET_LINUX_OBSERVER_SSHPASS",
                    "nonce": "linux-observer-nonce-" + "x" * 32,
                    "issued_at": "2026-08-30T00:00:00Z",
                    "expires_at": "2099-01-01T00:00:00Z",
                    "ledger_path": "/operator/credential-nonce-ledger.jsonl",
                    "one_shot": True,
                },
                "persistent_state_paths": [
                    f"/opt/oasis7/p2p-testnet-local/{surface.replace('{node_id}', 'triad-testnet-local')}"
                    for surface in self.module.LINUX_OBSERVER_PERSISTENT_STATE_SURFACES
                ],
                "identity_receipt": self._identity_receipt("triad-testnet-local"),
            },
            {
                "name": "windows-observer",
                "node_id": "triad-testnet-windows-observer",
                "role": "observer",
                "platform": "windows-x64",
                "node_root": r"C:\\oasis7-deploy",
                "service_manager": "scheduled-task",
                "service": "Oasis7Observer",
                "host_binding": {
                    "target": "observer@windows-lan",
                    "known_host_fingerprint": "SHA256:3NkC2GehDCcN+IWPbaxh+0JuIVGCEtKpdK69S6fHZPU",
                    "known_hosts_path": "/operator/known-hosts",
                },
                "endpoints": {
                    "healthz": "http://127.0.0.1:5121/healthz",
                    "evidence": "http://127.0.0.1:5121/v1/chain/status",
                },
                "credential_seam": {
                    "kind": "temporary-fd-or-environment",
                    "environment_name": "PUBLIC_TESTNET_WINDOWS_OBSERVER_SSHPASS",
                    "nonce": "windows-observer-nonce-" + "x" * 32,
                    "issued_at": "2026-08-30T00:00:00Z",
                    "expires_at": "2099-01-01T00:00:00Z",
                    "ledger_path": "/operator/credential-nonce-ledger.jsonl",
                    "one_shot": True,
                },
                "persistent_state_paths": [
                    self._windows_state_path(surface)
                    for surface in self.module.OBSERVER_RESET_SURFACES
                ],
                "identity_receipt": self._identity_receipt("triad-testnet-windows-observer"),
            },
            {
                "name": "macos-observer",
                "node_id": "triad-testnet-fourth-local",
                "role": "observer",
                "platform": "macos-arm64",
                "node_root": "/Applications/oasis7",
                "service_manager": "launchd",
                "service": "oasis7.testnet.fourth",
                "host_binding": {
                    "target": "observer@macos-lan",
                    "known_host_fingerprint": "SHA256:4NkC2GehDCcN+IWPbaxh+0JuIVGCEtKpdK69S6fHZPU",
                    "known_hosts_path": "/operator/known-hosts",
                },
                "endpoints": {
                    "healthz": "http://127.0.0.1:19083/healthz",
                    "evidence": "http://127.0.0.1:19083/v1/chain/status",
                },
                "credential_seam": {
                    "kind": "temporary-fd-or-environment",
                    "environment_name": "PUBLIC_TESTNET_MACOS_OBSERVER_SSHPASS",
                    "nonce": "macos-observer-nonce-" + "x" * 32,
                    "issued_at": "2026-08-30T00:00:00Z",
                    "expires_at": "2099-01-01T00:00:00Z",
                    "ledger_path": "/operator/credential-nonce-ledger.jsonl",
                    "one_shot": True,
                },
                "persistent_state_paths": [
                    f"/Applications/oasis7/{surface.replace('{node_id}', 'triad-testnet-fourth-local')}"
                    for surface in self.module.OBSERVER_RESET_SURFACES
                ],
                "identity_receipt": self._identity_receipt("triad-testnet-fourth-local"),
            },
        ]
        deployment_inventory = self._deployment_inventory(nodes)
        return self._sign_semantic_fixture({
            "schema_version": "oasis7.public_testnet_full_network_clean_room_input.v1",
            "transaction_id": transaction_id,
            "capture_window_id": capture_window_id,
            "task_uid": task_uid,
            "head_oid": head_oid,
            "authority": {
                "authorized": True,
                "task_uid": task_uid,
                "head_oid": head_oid,
                "frozen_head_oid": head_oid,
                "consumer_impact_record": consumer_impact_record,
                "signer_allowlist": ["governance-signer"],
                "crypto_verifier_receipt": {
                    "schema_version": "oasis7.crypto_verifier_receipt.v1",
                    "authenticated": True,
                    "verified": True,
                    "signer_id": "governance-signer",
                    "signed_payload_sha256": "a" * 64,
                    "signature_hex": "b" * 128,
                    "canonical_digest": "c" * 64,
                    "algorithm": "ed25519",
                    "scope": "all-plan-receipts",
                    "verifier_id": "governed-receipt-verifier",
                    "executable_path": "/operator/bin/verify-receipt",
                    "executable_sha256": "f" * 64,
                    "bindings": json.loads(json.dumps(execution_bindings)),
                },
                "trust_root": {
                    **self._receipt("oasis7.governed_trust_root_receipt.v1"),
                    "trust_root_id": "oasis7-public-testnet-governance-root-v1",
                    "verifier_id": "governed-receipt-verifier",
                    "signer_allowlist": ["governance-signer"],
                    "bindings": authority_bindings,
                },
                "receipt": {
                    **self._receipt("oasis7.clean_room_authority.v1"),
                    "bindings": authority_bindings,
                },
            },
            "consumer_impact_record": consumer_impact_record,
            "truth": truth,
            "fresh_root_probe": {
                "schema_version": "oasis7.fresh_root_probe.v1",
                "verified": True,
                "authenticated": True,
                "package_commit": "d" * 40,
                "checkpoint_id": "checkpoint-001",
                "manifest_hash": "5" * 64,
                "height": 100,
                "transaction_id": transaction_id,
                "capture_window_id": capture_window_id,
                "replayed": False,
                "post_validator_verify": True,
                "validator_verify_outputs": {
                    name: {
                        "schema_version": "oasis7.validator_verify_output.v1",
                        "authenticated": True,
                        "verified": True,
                        "signer_id": "governance-signer",
                        "signed_payload_sha256": "a" * 64,
                        "signature_hex": "b" * 128,
                        "canonical_digest": "c" * 64,
                        "node": name,
                        "transaction_id": transaction_id,
                        "capture_window_id": capture_window_id,
                        "package_commit": "d" * 40,
                        "checkpoint_id": "checkpoint-001",
                        "manifest_hash": "5" * 64,
                        "height": 100,
                        "output_sha256": "6" * 64,
                    }
                    for name in ("sequencer-204", "storage-205")
                },
                "receipt": self._receipt("oasis7.fresh_root_probe_receipt.v1"),
            },
            "credential_nonce_ledger": {
                "schema_version": "oasis7.credential_nonce_ledger.v1",
                "path": "/operator/credential-nonce-ledger.jsonl",
                "transaction_id": transaction_id,
                "capture_window_id": capture_window_id,
                "one_shot": True,
                "replay": False,
                "issued_at": "2026-08-30T00:00:00Z",
                "expires_at": "2099-01-01T00:00:00Z",
                "reserved_nonces": [
                    "storage-nonce-" + "x" * 32,
                    "sequencer-nonce-" + "x" * 32,
                    "linux-observer-nonce-" + "x" * 32,
                    "windows-observer-nonce-" + "x" * 32,
                    "macos-observer-nonce-" + "x" * 32,
                ],
                "receipt": {
                    **self._receipt("oasis7.credential_nonce_ledger_receipt.v1"),
                    "bindings": {
                        "path": "/operator/credential-nonce-ledger.jsonl",
                        "transaction_id": transaction_id,
                        "capture_window_id": capture_window_id,
                        "one_shot": True,
                        "replay": False,
                        "issued_at": "2026-08-30T00:00:00Z",
                        "expires_at": "2099-01-01T00:00:00Z",
                        "reserved_nonces": [
                            "storage-nonce-" + "x" * 32,
                            "sequencer-nonce-" + "x" * 32,
                            "linux-observer-nonce-" + "x" * 32,
                            "windows-observer-nonce-" + "x" * 32,
                            "macos-observer-nonce-" + "x" * 32,
                        ],
                    },
                },
            },
            "adapter_verification": {
                "schema_version": "oasis7.clean_room_adapter_verification.v1",
                "authenticated": True,
                "verified": True,
                "signer_id": "governance-signer",
                "signed_payload_sha256": "a" * 64,
                "signature_hex": "b" * 128,
                "canonical_digest": "c" * 64,
                "adapter_id": "external-clean-room-adapter",
                "transaction_id": transaction_id,
                "capture_window_id": capture_window_id,
                "live_receipts_verified": True,
                "credential_nonce_ledger_verified": True,
                "backup_or_no_backup_authority_verified": True,
                "apply_authority_granted": False,
                "durable_journal_authoritative": False,
                "durable_journal_receipt_required": True,
                "receipt": self._receipt("oasis7.clean_room_adapter_verification_receipt.v1"),
            },
            "deployment_inventory": deployment_inventory,
            "nodes": nodes,
        })

    def test_independently_signed_new_peer_snapshot_is_admitted(self) -> None:
        signing_module = type(self)._baseline_signing_module
        signing = signing_module.IdentityV2SigningToolContractTests("runTest")
        signing.setUp()
        self.addCleanup(signing.tearDown)
        builder = type(self)("runTest")
        builder.module = load_module()
        builder.signing = signing
        builder._impact_path = signing.root / "consumer-impact.json"
        # Only the fixture generator gets these synthetic identities. The
        # independent production module under test keeps its actual code.
        builder.module.CANONICAL_PEER_REGISTRY = {
            name: f"GovernedFixturePeer{index}" for index, name in enumerate(builder.module.NODE_ORDER)
        }
        authority = builder.module._peer_registry_authority()
        signing.peer_registry = signing.root / "managed-peers.json"
        authority.REGISTRY_PATH = signing.peer_registry
        _write_fixture_json(signing.peer_registry, {"schema_version": authority.SCHEMA,
            "network_id": authority.NETWORK_ID, "registry_epoch": "fixture-rotated-epoch",
            "nodes": [{"node_name": name, "node_id": authority.NODE_IDS[name], "peer_id": peer}
                      for name, peer in builder.module.CANONICAL_PEER_REGISTRY.items()]})
        signing.peer_registry.chmod(0o600)
        authority.REGISTRY_SHA256 = _fixture_digest(signing.peer_registry)
        builder._baseline_request = builder._input()
        builder._align_signing_context()
        artifacts = builder._make_baseline_signed_artifacts(label="new-peers")
        evidence = builder._write_baseline_evidence_map(artifacts, label="new-peers")
        target = load_module()
        target._PEER_REGISTRY_MODULE = authority
        for stem, path in (("VERIFY_TOOL", signing.verifier), ("TRUST_CONFIG", signing.trust),
                           ("PROVIDER_REGISTRY", signing.registry)):
            setattr(target, f"IDENTITY_V2_{stem}_PATH", path)
            setattr(target, f"IDENTITY_V2_{stem}_SHA256", _fixture_digest(path))
        target._independently_verify_identity_v2_entries(
            evidence, Path(evidence["context"]["path"]), Path(evidence["plan_intent"]["path"]))
        with self.subTest(admission="current-independently-signed"):
            try:
                admitted, _, _ = target._identity_v2_evidence_map(evidence, builder._baseline_request)
            except SystemExit as error:
                self.fail(f"independently signed current peer snapshot rejected: {error}")
            self.assertEqual(admitted, evidence)
        remapped = copy.deepcopy(evidence)
        remapped["entries"][0]["peer_id"] = "CallerInventedPeer"
        with self.assertRaises(SystemExit):
            target._identity_v2_evidence_map(remapped, builder._baseline_request)

    def test_signed_peer_snapshot_rotation_rejects_old_plan_before_effects(self) -> None:
        # Baseline setup prepared, signed and independently verified all five
        # node envelopes with ephemeral Ed25519 keys and an exact v2 intent.
        plan = self.module.build_plan(self._input())
        adapter_path = Path(__file__).with_name("p2p-public-testnet-full-network-clean-room-adapter.py")
        spec = importlib.util.spec_from_file_location("peer_rotation_adapter", adapter_path)
        adapter = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(adapter)
        adapter._PLANNER_MODULE = self.module
        adapter.validate_plan(plan)
        registry = self.module._peer_registry_authority()
        before_bytes = registry.REGISTRY_PATH.read_bytes()
        before_pin = registry.REGISTRY_SHA256
        try:
            for change in ("digest-only", "epoch-only", "peer-rotation"):
                changed = json.loads(before_bytes)
                if change == "epoch-only": changed["registry_epoch"] = "fixture-peer-epoch-2"
                if change == "peer-rotation": changed["nodes"][0]["peer_id"] = "RotatedFixturePeerOnly"
                raw = before_bytes + b"\n" if change == "digest-only" else json.dumps(changed, sort_keys=True).encode()
                registry.REGISTRY_PATH.write_bytes(raw)
                registry.REGISTRY_SHA256 = hashlib.sha256(raw).hexdigest()
                for route in ("execute-api", "resume-api", "execute-cli"):
                    with self.subTest(change=change, route=route), tempfile.TemporaryDirectory() as directory:
                        root = Path(directory)
                        journal, ledger = root / "journal.json", root / "ledger.jsonl"
                        plan_path, authority_path, evidence_path = root / "plan.json", root / "authority.json", root / "evidence.json"
                        _write_fixture_json(plan_path, plan)
                        _write_fixture_json(authority_path, {})
                        _write_fixture_json(evidence_path, plan["identity_v2_evidence"])
                        before = {path.name: _fixture_digest(path) for path in root.iterdir()}
                        with self.assertRaisesRegex(adapter.AdapterError, r"intent|peer registry"):
                            if route.endswith("api"):
                                method = adapter.execute if route == "execute-api" else adapter.resume_transaction
                                method(plan, {}, journal_path=journal, ledger_path=ledger, dry_run=True)
                            else:
                                adapter.main(["--plan", str(plan_path), "--authority", str(authority_path),
                                    "--journal", str(journal), "--ledger", str(ledger),
                                    "--identity-v2-evidence-map", str(evidence_path),
                                    "--identity-v2-mode", "current_admission"])
                        self.assertEqual(before, {path.name: _fixture_digest(path) for path in root.iterdir()},
                                         "rotated old plan must be rejected before lock/journal/output creation")
        finally:
            registry.REGISTRY_PATH.write_bytes(before_bytes)
            registry.REGISTRY_SHA256 = before_pin

    def test_plan_emits_fixed_five_node_order_and_8_7_surfaces(self) -> None:
        plan = self.module.build_plan(self._input())

        self.assertEqual(
            plan["consumer_impact_record"]["path"], str(self._impact_path)
        )
        self.assertEqual(
            plan["authority"]["consumer_impact_record"], plan["consumer_impact_record"]
        )

        self.assertEqual(
            plan["node_order"],
            [
                "storage-205",
                "sequencer-204",
                "linux-lan-observer",
                "windows-observer",
                "macos-observer",
            ],
        )
        self.assertEqual(len(plan["surfaces"]["validators"]), 8)
        self.assertEqual(len(plan["surfaces"]["observers"]), 7)
        self.assertEqual(plan["rollback"]["policy"], "clean-redeploy")
        self.assertEqual(
            plan["rollback"]["steps"],
            [
                "stop-started-nodes",
                "preserve-failed-state-for-forensics",
                "reinstall-exact-package-and-truth",
                "rerun-fresh-root-probe",
            ],
        )
        self.assertFalse(plan["rollback"]["restore_old_state"])
        self.assertFalse(plan["rollback"]["cross_node_state_copy"])

        self.assertEqual(plan["observer_gate"]["required_before"], ["windows-observer", "macos-observer"])
        self.assertLess(
            plan["global_order"].index("fresh-root-probe"),
            plan["global_order"].index("start:windows-observer"),
        )
        self.assertLess(
            plan["global_order"].index("start:windows-observer"),
            plan["global_order"].index("start:macos-observer"),
        )
        phases = [entry["phase"] for entry in plan["operation_journal"]]
        self.assertLess(phases.index("stop"), phases.index("delete"))
        self.assertLess(phases.index("delete"), phases.index("rebuild"))
        self.assertLess(phases.index("rebuild"), phases.index("start"))
        self.assertEqual(
            set(plan["truth"]["package"]["platforms"]),
            {"linux-x64", "windows-x64", "macos-arm64"},
        )
        self.assertEqual(
            plan["capture_window"],
            {
                "id": plan["capture_window_id"],
                "starts_at": plan["credential_nonce_ledger"]["issued_at"],
                "ends_at": plan["credential_nonce_ledger"]["expires_at"],
            },
        )

    def test_linux_observer_surfaces_match_managed_reset_layout(self) -> None:
        plan = self.module.build_plan(self._input())
        observer = next(
            node for node in plan["nodes"] if node["name"] == "linux-lan-observer"
        )
        root = "/opt/oasis7/p2p-testnet-local"
        required_paths = {
            f"{root}/world",
            f"{root}/world-simulator-mirror",
            f"{root}/execution-records",
            f"{root}/store",
            f"{root}/replication-root",
            f"{root}/runtime-root",
            f"{root}/output/chain-runtime/triad-testnet-local/reward-runtime-execution-bridge-state.json",
            f"{root}/output/node-distfs/triad-testnet-local",
        }
        self.assertTrue(
            required_paths.issubset(set(observer["persistent_state_paths"])),
            observer["persistent_state_paths"],
        )

    def test_service_account_ownership_requires_independent_deployment_truth(self) -> None:
        """An observed equal uid/gid pair cannot define its own expectation."""
        request = self._input()
        inventory_nodes = {
            node["name"]: {
                "expected_key_uid": 1001,
                "expected_key_gid": 1001,
            }
            for node in request["nodes"]
        }
        request["deployment_inventory"] = {
            "schema_version": "oasis7.deployment_inventory.v1",
            "authenticated": True,
            "verified": True,
            "signer_id": "governance-signer",
            "trust_root_id": "oasis7-public-testnet-governance-root-v1",
            "nodes": inventory_nodes,
            "receipt": self._receipt("oasis7.deployment_inventory_receipt.v1"),
        }
        for node in request["nodes"]:
            node["identity_receipt"]["key_uid"] = 4242
            node["identity_receipt"]["key_gid"] = 4242

        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)(expected|deployment|uid|gid|owner)")

    def test_macos_observer_uses_authenticated_inventory_root_and_surfaces(self) -> None:
        """macOS reset targets must come from authenticated deployment inventory."""
        request = self._input()
        root = "/Users/operator/oasis7-fourth"
        declared_paths = [
            f"{root}/world",
            f"{root}/world-simulator-mirror",
            f"{root}/execution-records",
            f"{root}/store",
            f"{root}/replication-root",
            f"{root}/runtime-root",
            f"{root}/output/chain-runtime/triad-testnet-fourth-local/reward-runtime-execution-bridge-state.json",
            f"{root}/output/node-distfs/triad-testnet-fourth-local",
        ]
        request["deployment_inventory"] = self._explicit_inventory(request)
        request["deployment_inventory"]["nodes"]["macos-observer"].update(
            {"node_root": root, "persistent_state_paths": declared_paths}
        )
        request["deployment_inventory"]["receipt"]["signed_payload_sha256"] = (
            self.module._canonical_deployment_inventory_payload_digest(
                request["deployment_inventory"]
            )
        )

        plan = self.module.build_plan(request)
        macos = next(node for node in plan["nodes"] if node["name"] == "macos-observer")
        self.assertEqual(macos["node_root"], root)
        self.assertEqual(macos["persistent_state_paths"], declared_paths)

    def test_observer_surface_summary_matches_governed_node_inventory(self) -> None:
        """The exported observer summary must include the governed eight paths."""
        plan = self.module.build_plan(self._input())
        linux = next(
            node for node in plan["nodes"] if node["name"] == "linux-lan-observer"
        )
        summary = plan["surfaces"]
        self.assertEqual(summary["observer_count"], len(linux["persistent_state_paths"]))
        self.assertEqual(summary["observer_count"], 8)
        self.assertIn("observers_by_node", summary)
        self.assertEqual(
            summary["observers_by_node"]["linux-lan-observer"],
            linux["persistent_state_paths"],
        )

    def test_plan_requires_explicit_authenticated_deployment_inventory(self) -> None:
        """Release admission cannot silently replace absent deployment truth."""
        request = self._input()
        request.pop("deployment_inventory")
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)deployment|inventory|missing|authenticated")

    def test_plan_allows_independent_authenticated_uid_and_gid_truth(self) -> None:
        """Deployment truth may authenticate distinct service UID and primary GID."""
        request = self._input()
        request["deployment_inventory"] = self._deployment_inventory(
            request["nodes"], expected_uid=1001, expected_gid=1002
        )
        for node in request["nodes"]:
            node["identity_receipt"]["key_uid"] = 1001
            node["identity_receipt"]["key_gid"] = 1002
            identity = node["identity_receipt"]
            identity["signed_payload_sha256"] = hashlib.sha256(
                self._raw_runtime_identity_receipt_v1_bytes(identity)
            ).hexdigest()
            identity["canonical_digest"] = self.module._canonical_receipt_digest(
                identity, excluded_fields=frozenset({"peer_id"})
            )

        plan = self._build_plan_with_evidence(
            request, type(self)._get_baseline_distinct_evidence()
        )
        for node in plan["nodes"]:
            inventory_node = plan["deployment_inventory"]["nodes"][node["name"]]
            self.assertEqual(inventory_node["expected_key_uid"], 1001)
            self.assertEqual(inventory_node["expected_key_gid"], 1002)
            self.assertEqual(node["identity_receipt"]["key_uid"], 1001)
            self.assertEqual(node["identity_receipt"]["key_gid"], 1002)

    def test_plan_requires_authenticated_root_and_reset_surfaces_per_managed_node(self) -> None:
        """Inventory layout is required per node; code-owned defaults are not evidence."""
        for node_name in self.module.NODE_ORDER:
            for field in ("node_root", "persistent_state_paths"):
                with self.subTest(node=node_name, field=field):
                    request = self._input()
                    inventory = self._deployment_inventory(request["nodes"])
                    inventory["nodes"][node_name].pop(field)
                    request["deployment_inventory"] = inventory
                    with self.assertRaises(SystemExit) as raised:
                        self.module.build_plan(request)
                    self.assertRegex(str(raised.exception), r"(?i)deployment|inventory|root|surface|path")

    def test_plan_requires_complete_canonical_state_surfaces_per_managed_node(self) -> None:
        """Authenticated state inventory must cover every role/platform surface."""
        for node_name in self.module.NODE_ORDER:
            for omission in ("sparse", "nested"):
                with self.subTest(node=node_name, omission=omission):
                    request = self._input()
                    inventory = self._deployment_inventory(request["nodes"])
                    node = next(item for item in request["nodes"] if item["name"] == node_name)
                    full_paths = list(node["persistent_state_paths"])
                    if omission == "sparse":
                        incomplete_paths = [full_paths[0]]
                    else:
                        incomplete_paths = full_paths[:2] + full_paths[3:]
                    inventory["nodes"][node_name]["persistent_state_paths"] = incomplete_paths
                    node["persistent_state_paths"] = incomplete_paths
                    request["deployment_inventory"] = inventory
                    with self.assertRaises(SystemExit) as raised:
                        self.module.build_plan(request)
                    self.assertRegex(str(raised.exception), r"(?i)surface|canonical|complete|path")

    def test_plan_enforces_component_aware_windows_state_containment(self) -> None:
        """Windows roots are components: root and sibling-prefix paths are not surfaces."""
        windows_name = "windows-observer"
        windows_index = next(
            index for index, node in enumerate(self._input()["nodes"])
            if node["name"] == windows_name
        )
        for label, invalid_path in (
            ("exact-root", "C:/oasis7-deploy"),
            ("sibling-prefix", "C:/oasis7-deploy-evil/state"),
        ):
            with self.subTest(path=label):
                request = self._input()
                inventory = self._deployment_inventory(request["nodes"])
                inventory["nodes"][windows_name]["persistent_state_paths"] = [invalid_path]
                request["nodes"][windows_index]["persistent_state_paths"] = [invalid_path]
                request["deployment_inventory"] = inventory
                with self.assertRaises(SystemExit) as raised:
                    self.module.build_plan(request)
                self.assertRegex(str(raised.exception), r"(?i)root|surface|path|contain")

        # Existing complete inventory paths are genuine descendants and remain accepted.
        self.module.build_plan(self._input())

    def test_plan_rejects_duplicate_authenticated_peer_ids(self) -> None:
        """Distinct retained v2 entries cannot share one authenticated peer identity."""
        request = self._input()
        evidence = copy.deepcopy(self._baseline_identity_v2_evidence)
        evidence["entries"][1]["peer_id"] = evidence["entries"][0]["peer_id"]
        with self.assertRaises(SystemExit) as raised:
            self._build_plan_with_evidence(request, evidence)
        self.assertRegex(str(raised.exception), r"(?i)peer|identity|duplicate|unique")

    def test_plan_rejects_unique_peer_ids_outside_authenticated_registry(self) -> None:
        """Peer uniqueness alone cannot authorize a caller-supplied retained identity."""
        for node in self._input()["nodes"]:
            with self.subTest(node=node["name"]):
                request = self._input()
                evidence = copy.deepcopy(self._baseline_identity_v2_evidence)
                target = next(
                    item for item in evidence["entries"] if item["node_name"] == node["name"]
                )
                target["peer_id"] = f"12D3KooWcaller-supplied-{node['name']}"
                with self.assertRaises(SystemExit) as raised:
                    self._build_plan_with_evidence(request, evidence)
                self.assertRegex(str(raised.exception), r"(?i)peer|identity|registry|canonical|binding")

    def test_plan_requires_fresh_bound_consumer_impact_record(self) -> None:
        request = self._input()
        request.pop("consumer_impact_record")
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)consumer|impact|record|binding")

        request = self._input()
        self._rewrite_consumer_impact(request, timestamp="2020-01-01T00:00:00Z")
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)consumer|impact|stale|fresh|timestamp")

        request = self._input()
        self._rewrite_consumer_impact(request, impact="active", outage_update_channel="n/a")
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)consumer|impact|outage|channel|n/a")

        request = self._input()
        request["consumer_impact_record"]["sha256"] = "0" * 64
        request["authority"]["consumer_impact_record"] = dict(request["consumer_impact_record"])
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)consumer|impact|sha|digest|binding")

    def test_observer_destructive_phases_follow_fresh_root_probe(self) -> None:
        plan = self.module.build_plan(self._input())
        order = plan["global_order"]
        probe_index = order.index("fresh-root-probe")
        for name in ("linux-lan-observer", "windows-observer", "macos-observer"):
            for phase in ("stop", "delete", "rebuild"):
                self.assertGreater(order.index(f"{phase}:{name}"), probe_index)

        invalid = list(order)
        invalid.remove("stop:windows-observer")
        invalid.insert(probe_index, "stop:windows-observer")
        with patch.object(self.module, "_global_order", return_value=invalid):
            with self.assertRaises(SystemExit) as raised:
                self.module.build_plan(self._input())
        self.assertRegex(str(raised.exception), r"(?i)order|probe|observer|destructive")

    def test_plan_rejects_attacker_endpoint_host_port_or_path(self) -> None:
        request = self._input()
        request["nodes"][0]["endpoints"]["healthz"] = "http://attacker.invalid:6631/healthz"
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)canonical|endpoint|host|port|binding")

    def test_plan_requires_code_owned_external_trust_root_and_receipt_bindings(self) -> None:
        request = self._input()
        request["authority"]["trust_root"]["trust_root_id"] = "caller-owned-root"
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)trust|root|authority|identity")

        request = self._input()
        request["authority"]["receipt"]["bindings"]["head_oid"] = "f" * 40
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)authority|head|receipt|binding")

        request = self._input()
        request["truth"]["genesis"]["network_id"] = "caller-owned-network"
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)network|genesis|trust|code-owned")

    def _network_binding_evidence_fixture(
        self,
        root: Path,
        *,
        context_network_id: str,
        request: dict[str, object] | None = None,
        expected_uid: int = 0,
        expected_gid: int = 0,
    ) -> tuple[dict[str, object], dict[str, object]]:
        """Make a shape-valid five-node map for network-binding admission tests."""
        request = request or self._input()
        context = {
            "schema_version": "oasis7.identity_v2_context.v1",
            "network_id": context_network_id,
            "task_uid": request["task_uid"],
            "head_oid": request["head_oid"],
            "capture_window_id": request["capture_window_id"],
            "capture_start": "2026-09-01T00:00:00Z",
            "capture_end": "2099-01-01T00:00:00Z",
            "rotation_epoch": self.module.CANONICAL_ROTATION_EPOCH,
            "issued_at": "2026-09-01T00:00:00Z",
            "expires_at": "2099-01-01T00:00:00Z",
        }
        context_path = root / "context.json"
        context_path.write_bytes(json.dumps(context, sort_keys=True, separators=(",", ":")).encode())
        context_path.chmod(0o600)
        context_digest = hashlib.sha256(context_path.read_bytes()).hexdigest()
        intent = {
            "schema_version": "oasis7.clean_room_plan_intent.v2",
            "peer_registry_sha256": self.module._peer_registry_authority().REGISTRY_SHA256,
            "peer_registry_epoch": self.module._peer_registry_authority().load_snapshot()["registry_epoch"],
            "context_digest": context_digest,
            "adapter_action": self.module.CANONICAL_PLAN_INTENT_ACTION,
            "nodes": [
                {
                    "node_name": node_name,
                    "node_id": self.module.EXPECTED_NODES[node_name]["node_id"],
                    "peer_id": self.module.CANONICAL_PEER_REGISTRY[node_name],
                    "role": self.module.EXPECTED_NODES[node_name]["role"],
                    "reset_surface_ids": list(
                        self.module.CANONICAL_PLAN_INTENT_RESET_SURFACE_IDS
                    ),
                }
                for node_name in sorted(self.module.NODE_ORDER)
            ],
        }
        intent_path = root / "plan-intent.json"
        intent_path.write_bytes(json.dumps(intent, sort_keys=True, separators=(",", ":")).encode())
        intent_path.chmod(0o600)
        plan_digest = hashlib.sha256(intent_path.read_bytes()).hexdigest()
        entries: list[dict[str, object]] = []
        for node_name in self.module.NODE_ORDER:
            expected = self.module.EXPECTED_NODES[node_name]
            # This fixture is code-owned test truth, not a projection of the
            # caller request.  In particular, request mutations must not be
            # able to manufacture an admission map that follows them.
            node_id = expected["node_id"]
            peer_id = self.module.CANONICAL_PEER_REGISTRY[node_name]
            key_sha256 = "1" * 64
            key_size_bytes = 1
            key_mode = "0600"
            raw_key_mode = int(key_mode, 8)
            key_uid = expected_uid
            key_gid = expected_gid
            signature_hex = "a" * 128
            raw_bytes = json.dumps(
                {
                    "schema_version": "oasis7.identity_receipt.v1",
                    "node_id": node_id,
                    "peer_id": peer_id,
                    "key_path": f"config/{node_id}-node-keypair.toml",
                    "key_sha256": key_sha256,
                    "key_size_bytes": key_size_bytes,
                    "key_mode": raw_key_mode,
                    "key_uid": key_uid,
                    "key_gid": key_gid,
                },
                sort_keys=True,
                separators=(",", ":"),
            ).encode()
            raw_path = root / f"{node_name}.raw-v1"
            raw_path.write_bytes(raw_bytes)
            envelope = {
                field: "placeholder"
                for field in self.module.IDENTITY_V2_ENVELOPE_FIELDS
            }
            envelope.update(
                {
                    "domain_separator": "oasis7.identity_receipt.v2/signature/v1",
                    "schema_version": "oasis7.identity_receipt.v2",
                    "signer_id": "governance-signer",
                    "verifier_id": self.module.CANONICAL_VERIFIER_ID,
                    "trust_root_id": self.module.CANONICAL_TRUST_ROOT_ID,
                    "network_id": context_network_id,
                    "task_uid": request["task_uid"],
                    "head_oid": request["head_oid"],
                    "frozen_head_oid": request["head_oid"],
                    "plan_digest": plan_digest,
                    "context_digest": context_digest,
                    "capture_window_id": request["capture_window_id"],
                    "rotation_epoch": self.module.CANONICAL_ROTATION_EPOCH,
                    "issued_at": "2026-09-01T00:00:00Z",
                    "expires_at": "2099-01-01T00:00:00Z",
                    "node_id": node_id,
                    "peer_id": peer_id,
                    "key_sha256": key_sha256,
                    "key_size_bytes": key_size_bytes,
                    "key_mode": key_mode,
                    "key_uid": key_uid,
                    "key_gid": key_gid,
                    "signed_payload_sha256": hashlib.sha256(raw_bytes).hexdigest(),
                    "signature_hex": signature_hex,
                    "authenticated": True,
                    "verified": True,
                    "historical_only": False,
                    "apply_authorized": True,
                }
            )
            envelope["canonical_digest"] = hashlib.sha256(
                json.dumps(
                    {
                        **{field: envelope[field] for field in self.module.IDENTITY_V2_SIGNED_FIELDS},
                        "signature_hex": envelope["signature_hex"],
                    },
                    sort_keys=True,
                    separators=(",", ":"),
                ).encode()
            ).hexdigest()
            envelope_path = root / f"{node_name}.envelope.json"
            envelope_path.write_bytes(json.dumps(envelope, sort_keys=True, separators=(",", ":")).encode())
            unsigned_envelope = dict(envelope)
            unsigned_envelope["authenticated"] = False
            unsigned_envelope["verified"] = False
            unsigned_path = root / f"{node_name}.unsigned-envelope.json"
            unsigned_path.write_bytes(
                json.dumps(unsigned_envelope, sort_keys=True, separators=(",", ":")).encode()
            )
            payload_value = {
                field: envelope[field] for field in self.module.IDENTITY_V2_SIGNED_FIELDS
            }
            payload_bytes = b"OASIS7-IDENTITY-RECEIPT-V2\0" + json.dumps(
                payload_value, sort_keys=True, separators=(",", ":")
            ).encode()
            payload_path = root / f"{node_name}.payload.bin"
            payload_path.write_bytes(payload_bytes)
            manifest = {
                "schema_version": "oasis7.identity_v2_prepare_manifest.v1",
                "network_id": self.module.CANONICAL_NETWORK_ID,
                "raw_v1_sha256": hashlib.sha256(raw_bytes).hexdigest(),
                "canonical_payload_sha256": hashlib.sha256(payload_bytes).hexdigest(),
                "payload_size_bytes": len(payload_bytes),
            }
            manifest_path = root / f"{node_name}.prepare-manifest.json"
            manifest_path.write_bytes(json.dumps(manifest, sort_keys=True, separators=(",", ":")).encode())
            request_id = "req-v2:" + format(self.module.NODE_ORDER.index(node_name) + 1, "064x")
            proof_ref = "proof-v1:" + hashlib.sha256(("provider-proof:" + request_id).encode()).hexdigest()
            attestation = {
                "schema_version": "oasis7.identity_v2_provider_attestation.v2",
                "network_id": context_network_id,
                "provider_id": "ephemeral-test-provider",
                "request_id": request_id,
                "signer_id": "governance-signer",
                "public_key_sha256": "1" * 64,
                "algorithm": "ed25519",
                "canonical_payload_sha256": hashlib.sha256(payload_bytes).hexdigest(),
                "signature_sha256": hashlib.sha256(bytes.fromhex(signature_hex)).hexdigest()
                if isinstance(signature_hex, str) and len(signature_hex) == 128
                else "0" * 64,
                "context_digest": context_digest,
                "rotation_epoch": self.module.CANONICAL_ROTATION_EPOCH,
                "capture_window_id": request["capture_window_id"],
                "issued_at": "2026-09-01T00:00:00Z",
                "expires_at": "2099-01-01T00:00:00Z",
                "task_uid": request["task_uid"],
                "head_oid": request["head_oid"],
                "proof_ref": proof_ref,
            }
            claims = {
                "schema_version": "oasis7.identity_v2_provider_authentication_claims.v1",
                "domain_separator": "oasis7.identity-v2-provider-authentication/v1",
                "network_id": attestation["network_id"],
                "provider_id": attestation["provider_id"],
                "request_id": attestation["request_id"],
                "signer_id": attestation["signer_id"],
                "public_key_sha256": attestation["public_key_sha256"],
                "canonical_payload_sha256": attestation["canonical_payload_sha256"],
                "signature_sha256": attestation["signature_sha256"],
                "context_digest": attestation["context_digest"],
                "task_uid": attestation["task_uid"],
                "head_oid": attestation["head_oid"],
                "rotation_epoch": attestation["rotation_epoch"],
                "capture_window_id": attestation["capture_window_id"],
                "issued_at": attestation["issued_at"],
                "expires_at": attestation["expires_at"],
                "proof_ref": attestation["proof_ref"],
            }
            claims_bytes = json.dumps(claims, sort_keys=True, separators=(",", ":")).encode()
            attestation["proof"] = {
                "schema_version": "oasis7.identity_v2_provider_authentication_proof.v1",
                "algorithm": "ed25519",
                "claims_sha256": hashlib.sha256(claims_bytes).hexdigest(),
                "signature_hex": "a" * 128,
            }
            attestation_path = root / f"{node_name}.provider-attestation.json"
            attestation_path.write_bytes(
                json.dumps(attestation, sort_keys=True, separators=(",", ":")).encode()
            )
            signed_payload = b"OASIS7-IDENTITY-RECEIPT-V2\0" + json.dumps(
                {field: envelope[field] for field in self.module.IDENTITY_V2_SIGNED_FIELDS},
                sort_keys=True,
                separators=(",", ":"),
            ).encode()
            verification = {
                field: "0" * 64
                for field in self.module.IDENTITY_V2_VERIFICATION_FIELDS
                if field.endswith("sha256")
            }
            verification.update(
                {
                    "schema_version": "oasis7.identity_v2_verification_receipt.v1",
                    "mode": "current_admission",
                    "evaluation_time": "2026-09-01T00:00:00Z",
                    "canonical_payload_sha256": hashlib.sha256(signed_payload).hexdigest(),
                    "envelope_sha256": hashlib.sha256(unsigned_path.read_bytes()).hexdigest(),
                    "signer_id": "governance-signer",
                    "task_uid": request["task_uid"],
                    "head_oid": request["head_oid"],
                    "node_id": expected["node_id"],
                    "peer_id": peer_id,
                    "capture_window_id": request["capture_window_id"],
                    "rotation_epoch": self.module.CANONICAL_ROTATION_EPOCH,
                    "network_id": context_network_id,
                    "proof_ref": attestation["proof_ref"],
                    "proof_claims_sha256": attestation["proof"]["claims_sha256"],
                    "historical_only": False,
                    "apply_authorized": True,
                    "authority_scope": "deployed-governance-root",
                    "verified": True,
                }
            )
            verification["raw_v1_sha256"] = hashlib.sha256(raw_bytes).hexdigest()
            verification_path = root / f"{node_name}.verification.json"
            verification_path.write_bytes(json.dumps(verification, sort_keys=True, separators=(",", ":")).encode())
            for artifact_path in (
                raw_path, envelope_path, unsigned_path, payload_path,
                manifest_path, attestation_path, verification_path,
            ):
                artifact_path.chmod(0o600)
            descriptor = lambda path: {
                "path": str(path),
                "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                "size_bytes": path.stat().st_size,
            }
            entries.append(
                {
                    "node_name": node_name,
                    "node_id": expected["node_id"],
                    "peer_id": peer_id,
                    "raw_v1": descriptor(raw_path),
                    "prepare_manifest": descriptor(manifest_path),
                    "payload": descriptor(payload_path),
                    "provider_attestation": descriptor(attestation_path),
                    "unsigned_envelope": descriptor(unsigned_path),
                    "signed_envelope": descriptor(envelope_path),
                    "verification": descriptor(verification_path),
                }
            )
        evidence = {
            "schema_version": self.module.IDENTITY_V2_EVIDENCE_SCHEMA,
            "network_id": self.module.CANONICAL_NETWORK_ID,
            "task_uid": request["task_uid"],
            "head_oid": request["head_oid"],
            "context": {
                "path": str(context_path),
                "sha256": hashlib.sha256(context_path.read_bytes()).hexdigest(),
                "size_bytes": context_path.stat().st_size,
            },
            "plan_intent": {
                "path": str(intent_path),
                "sha256": hashlib.sha256(intent_path.read_bytes()).hexdigest(),
                "size_bytes": intent_path.stat().st_size,
            },
            "entries": entries,
        }
        return evidence, request

    def test_identity_v2_evidence_map_rejects_context_network_mismatch(self) -> None:
        """A five-node map cannot bind an attacker-selected context network."""
        with tempfile.TemporaryDirectory() as directory:
            evidence, request = self._network_binding_evidence_fixture(
                Path(directory), context_network_id="attacker-network"
            )
            with self.assertRaises(SystemExit) as raised:
                self.module._identity_v2_evidence_map(evidence, request)
        self.assertRegex(str(raised.exception), r"(?i)network|context|binding")

    def test_identity_v2_evidence_map_accepts_governed_context_network(self) -> None:
        """The canonical deployment network remains a valid map binding."""
        with tempfile.TemporaryDirectory() as directory:
            evidence, request = self._network_binding_evidence_fixture(
                Path(directory), context_network_id=self.module.CANONICAL_NETWORK_ID
            )
            validated, raw_by_node, envelopes = self.module._identity_v2_evidence_map(
                evidence, request
            )
        self.assertEqual(set(raw_by_node), set(self.module.NODE_ORDER))
        self.assertEqual(set(envelopes), set(self.module.NODE_ORDER))

    def test_identity_v2_evidence_map_rejects_plan_intent_governed_truth_drift(self) -> None:
        """Admission binds action, managed nodes, roles, and reset surfaces to code truth."""
        mutations = (
            ("adapter action", lambda intent: intent.__setitem__("adapter_action", "unsafe-reset")),
            (
                "node role",
                lambda intent: intent["nodes"][0].__setitem__("role", "validator"),
            ),
            (
                "reset surfaces",
                lambda intent: intent["nodes"][0].__setitem__("reset_surface_ids", ["world"]),
            ),
            (
                "managed node set",
                lambda intent: intent["nodes"].pop(),
            ),
        )
        for label, mutate in mutations:
            with self.subTest(mutation=label), tempfile.TemporaryDirectory() as directory:
                evidence = copy.deepcopy(self._baseline_identity_v2_evidence)
                source = Path(evidence["plan_intent"]["path"])
                intent = json.loads(source.read_text(encoding="utf-8"))
                mutate(intent)
                target = Path(directory) / "tampered-plan-intent.json"
                _write_fixture_json(target, intent)
                target.chmod(0o600)
                evidence["plan_intent"] = _fixture_descriptor(target)
                with self.assertRaises(SystemExit) as raised:
                    self.module._identity_v2_evidence_map(evidence, self._baseline_request)
                self.assertRegex(
                    str(raised.exception), r"(?i)plan intent|deployment truth|action|node|surface"
                )

    def test_plan_requires_adapter_live_receipt_and_never_treats_plan_as_apply_proof(self) -> None:
        request = self._input()
        request.pop("adapter_verification")
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)adapter|live|receipt|ledger")

        request = self._input()
        request["adapter_verification"]["apply_authority_granted"] = True
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)apply|proof|adapter|authority")

    def test_build_plan_rejects_missing_identity_v2_evidence_map(self) -> None:
        """Legacy receipt-only input must not bypass v2 map admission."""
        with self.assertRaises(SystemExit) as raised:
            self._build_plan_without_evidence(self._input())
        self.assertRegex(str(raised.exception), r"(?i)identity.?v2|evidence|map|admission")

    def test_plan_marks_operation_journal_non_authoritative_and_adapter_owned(self) -> None:
        plan = self.module.build_plan(self._input())
        contract = plan["operation_journal_contract"]
        self.assertFalse(contract["authoritative"])
        self.assertFalse(contract["apply_usable"])
        self.assertTrue(contract["adapter_owned"])
        self.assertTrue(contract["durable_receipt_required"])

    def test_plan_rejects_missing_fresh_root_probe_before_windows_or_macos(self) -> None:
        request = self._input()
        request.pop("fresh_root_probe")
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)fresh[_-]?root.*probe")

    def test_plan_rejects_shaped_but_unverified_inventory_identity_and_authority_receipts(self) -> None:
        """Receipt-shaped fields are not independent verification evidence."""
        mutations = (
            (
                "deployment-inventory-signature",
                lambda request: request["deployment_inventory"]["receipt"].__setitem__(
                    "signature_hex", "0" * 128
                ),
            ),
            (
                "deployment-inventory-digest",
                lambda request: request["deployment_inventory"]["receipt"].__setitem__(
                    "canonical_digest", "0" * 64
                ),
            ),
            (
                "identity-signature",
                lambda request: request["nodes"][0]["identity_receipt"].__setitem__(
                    "signature_hex", "0" * 128
                ),
            ),
            (
                "identity-digest",
                lambda request: request["nodes"][0]["identity_receipt"].__setitem__(
                    "canonical_digest", "0" * 64
                ),
            ),
            (
                "authority-verifier-signature",
                lambda request: request["authority"]["crypto_verifier_receipt"].__setitem__(
                    "signature_hex", "0" * 128
                ),
            ),
        )
        for label, mutate in mutations:
            with self.subTest(receipt=label):
                request = self._input()
                evidence = self._baseline_identity_v2_evidence
                if label == "identity-signature":
                    evidence = self._tampered_evidence_artifact(
                        evidence,
                        mutate=lambda envelope: envelope.__setitem__("signature_hex", "0" * 128),
                    )
                elif label == "identity-digest":
                    evidence = self._tampered_evidence_artifact(
                        evidence,
                        mutate=lambda envelope: envelope.__setitem__("canonical_digest", "0" * 64),
                    )
                else:
                    mutate(request)
                with self.assertRaises(SystemExit) as raised:
                    self._build_plan_with_evidence(request, evidence)
                self.assertRegex(
                    str(raised.exception),
                    r"(?i)receipt|signature|digest|verified|authenticated|verifier|binding|envelope|identity",
                )

    def test_plan_rejects_authority_binding_context_drift_and_stale_or_future_receipts(self) -> None:
        """Authority receipts bind task/head/window/rotation and a bounded issue window."""
        valid = self._input()
        valid_context = {
            "capture_window_id": valid["capture_window_id"],
            "rotation_epoch": "rotation-epoch-20260901-001",
            "issued_at": "2026-08-30T00:00:00Z",
            "expires_at": "2099-01-01T00:00:00Z",
        }
        for container in (
            valid["authority"]["receipt"]["bindings"],
            valid["authority"]["trust_root"]["bindings"],
        ):
            container.update(valid_context)
        self.module.build_plan(valid)

        mutations = (
            ("task-mismatch", {"task_uid": "task-attacker"}),
            ("head-mismatch", {"head_oid": "f" * 40}),
            ("frozen-head-mismatch", {"frozen_head_oid": "f" * 40}),
            ("capture-window-mismatch", {"capture_window_id": "other-window"}),
            ("rotation-epoch-mismatch", {"rotation_epoch": "rotation-attacker"}),
            (
                "stale-authority",
                {"issued_at": "2020-01-01T00:00:00Z", "expires_at": "2020-01-02T00:00:00Z"},
            ),
            (
                "future-authority",
                {"issued_at": "2099-01-01T00:00:00Z", "expires_at": "2100-01-01T00:00:00Z"},
            ),
        )
        for label, updates in mutations:
            with self.subTest(binding=label):
                request = self._input()
                for container in (
                    request["authority"]["receipt"]["bindings"],
                    request["authority"]["trust_root"]["bindings"],
                ):
                    container.update(valid_context)
                    container.update(updates)
                with self.assertRaises(SystemExit) as raised:
                    self.module.build_plan(request)
                self.assertRegex(
                    str(raised.exception),
                    r"(?i)authority|binding|capture|rotation|task|head|stale|future|expir",
                )

    def test_plan_rejects_old_state_restore_or_cross_node_copy(self) -> None:
        request = self._input()
        request["recovery"] = {"restore_old_state": True}
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)old.?state|seed|copy|forensic")

        request = self._input()
        request["recovery"] = {"source_node": "sequencer-204", "cross_node_state_copy": True}
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)cross.?node|copy|source")

    def test_plan_rejects_binding_drift_and_unverified_receipt(self) -> None:
        request = self._input()
        request["truth"]["checkpoint"]["manifest_hash"] = "f" * 64
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)probe|checkpoint|manifest|binding")

        request = self._input()
        request["truth"]["package"]["receipt"]["verified"] = False
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)receipt|verified|authenticated")

    def test_plan_rejects_service_path_or_identity_inventory_drift(self) -> None:
        request = self._input()
        request["nodes"][0]["service"] = "caller-owned.service"
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)service|identity|contract")

        request = self._input()
        request["nodes"][2]["persistent_state_paths"] = request["nodes"][2]["persistent_state_paths"][:-1]
        request["deployment_inventory"]["nodes"]["linux-lan-observer"]["persistent_state_paths"] = [
            "/operator/not-the-authenticated-observer-root"
        ]
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)surface|persistent|state")

        request = self._input()
        evidence = self._tampered_evidence_artifact(
            self._baseline_identity_v2_evidence,
            mutate=lambda envelope: envelope.__setitem__("key_mode", "0644"),
        )
        with self.assertRaises(SystemExit) as raised:
            self._build_plan_with_evidence(request, evidence)
        self.assertRegex(str(raised.exception), r"(?i)key_mode|0600|identity|evidence|binding")

    def test_plan_rejects_unbound_frozen_head_signer_or_verifier(self) -> None:
        request = self._input()
        request["authority"]["frozen_head_oid"] = "f" * 40
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)frozen|head|binding")

        request = self._input()
        request["authority"]["signer_allowlist"] = ["different-signer"]
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)signer|allow")

        request = self._input()
        request["authority"]["crypto_verifier_receipt"]["verified"] = False
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)verifier|crypt|receipt")

    def test_plan_rejects_endpoint_pin_or_nonce_drift(self) -> None:
        request = self._input()
        request["nodes"][0]["endpoints"]["evidence"] = "http://127.0.0.1:6631/v1/chain/status"
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)endpoint|204|rebuild-proof|status")

        request = self._input()
        request["nodes"][1]["host_binding"]["known_host_fingerprint"] = "unbound-pin"
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)host|fingerprint|pin")

        request = self._input()
        request["nodes"][2]["credential_seam"]["nonce"] = "too-short"
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)nonce|credential|seam")

    def test_operator_authorized_no_backup_mode_is_explicit(self) -> None:
        request = self._input()
        request["backup_policy"] = {
            "mode": "operator-authorized-no-backup",
            "operator_authorized": True,
            "current_authorization": True,
            "repository": "eng-cc/oasis7",
            "action": "full-network-clean-room",
            "targets": list(self.module.NODE_ORDER),
            "transaction_id": request["transaction_id"],
            "capture_window_id": request["capture_window_id"],
            "actor": "ops-actor",
            "issued_at": "2026-08-30T00:00:00Z",
            "expires_at": "2099-01-01T00:00:00Z",
            "task_uid": request["task_uid"],
            "frozen_head_oid": request["head_oid"],
            "reason": "immutable provider backup unavailable",
            "authority": self._no_backup_receipt(request, "2099-01-01T00:00:00Z"),
        }
        plan = self.module.build_plan(request)
        self.assertFalse(plan["forensic_backup"]["required_before_reset"])
        self.assertEqual(plan["forensic_backup"]["mode"], "operator-authorized-no-backup")
        self.assertFalse(plan["forensic_backup"]["immutable"])
        self.assertFalse(plan["forensic_backup"]["receipt_required_per_node"])

    def test_plan_rejects_caller_inventory_override(self) -> None:
        request = self._input()
        request["nodes"][0]["host_binding"]["known_hosts_path"] = "/operator/other-known-hosts"
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)canonical|inventory|known.?hosts|host")

    def test_verifier_receipt_binds_execution_world_and_json_index_evidence(self) -> None:
        request = self._input()
        request["authority"]["crypto_verifier_receipt"]["bindings"]["cas"]["blake3"] = "0" * 64
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)verifier|execution|cas|binding")

        request = self._input()
        request["truth"]["execution"]["json_index_consistency"]["verified"] = False
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)json|index|consistency|execution")

    def test_probe_binds_transaction_capture_and_post_validator_outputs(self) -> None:
        request = self._input()
        request["fresh_root_probe"]["transaction_id"] = "different-transaction"
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)transaction|capture|probe")

        request = self._input()
        request["fresh_root_probe"]["replayed"] = True
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)replay|probe")

        request = self._input()
        request["fresh_root_probe"]["validator_verify_outputs"].pop("storage-205")
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)validator|verify|output|probe")

    def test_no_backup_authority_binds_full_context_and_expiry(self) -> None:
        request = self._input()
        request["backup_policy"] = {
            "mode": "operator-authorized-no-backup",
            "operator_authorized": True,
            "current_authorization": True,
            "repository": "eng-cc/oasis7",
            "action": "full-network-clean-room",
            "targets": list(self.module.NODE_ORDER),
            "transaction_id": request["transaction_id"],
            "capture_window_id": request["capture_window_id"],
            "actor": "ops-actor",
            "issued_at": "2026-08-30T00:00:00Z",
            "expires_at": "2099-01-01T00:00:00Z",
            "task_uid": request["task_uid"],
            "frozen_head_oid": request["head_oid"],
            "reason": "immutable provider backup unavailable",
            "authority": self._no_backup_receipt(request, "2099-01-01T00:00:00Z"),
        }
        request["backup_policy"]["action"] = "other-action"
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)backup|authority|action|binding")

        request["backup_policy"]["action"] = "full-network-clean-room"
        request["backup_policy"]["issued_at"] = "2099-01-01T00:00:00Z"
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)expir|future|backup|authority")

        request = self._input()
        request["backup_policy"] = {
            "mode": "operator-authorized-no-backup",
            "operator_authorized": True,
            "current_authorization": True,
            "repository": "eng-cc/oasis7",
            "action": "full-network-clean-room",
            "targets": list(self.module.NODE_ORDER),
            "transaction_id": request["transaction_id"],
            "capture_window_id": request["capture_window_id"],
            "actor": "ops-actor",
            "issued_at": "2026-08-30T00:00:00Z",
            "expires_at": "2020-01-01T00:00:00Z",
            "task_uid": request["task_uid"],
            "frozen_head_oid": request["head_oid"],
            "reason": "immutable provider backup unavailable",
            "authority": self._no_backup_receipt(request, "2020-01-01T00:00:00Z"),
        }
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)expir|backup|authority")

        request = self._input()
        request["backup_policy"] = {
            "mode": "operator-authorized-no-backup",
            "operator_authorized": True,
            "current_authorization": True,
            "repository": "eng-cc/oasis7",
            "action": "full-network-clean-room",
            "targets": list(self.module.NODE_ORDER),
            "transaction_id": request["transaction_id"],
            "capture_window_id": request["capture_window_id"],
            "actor": "ops-actor",
            "issued_at": "2026-08-30T00:00:00Z",
            "expires_at": "2099-01-01T00:00:00Z",
            "task_uid": request["task_uid"],
            "frozen_head_oid": request["head_oid"],
            "reason": "immutable provider backup unavailable",
            "authority": self._no_backup_receipt(request, "2099-01-01T00:00:00Z"),
        }
        request["backup_policy"]["authority"]["bindings"]["actor"] = "different-actor"
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)authority|receipt|binding")

    def test_credential_lease_rejects_future_issued_at(self) -> None:
        request = self._input()
        issued_at = "2099-01-01T00:00:00Z"
        expires_at = "2100-01-01T00:00:00Z"
        ledger = request["credential_nonce_ledger"]
        ledger["issued_at"] = issued_at
        ledger["expires_at"] = expires_at
        ledger["receipt"]["bindings"]["issued_at"] = issued_at
        ledger["receipt"]["bindings"]["expires_at"] = expires_at
        for node in request["nodes"]:
            node["credential_seam"]["issued_at"] = issued_at
            node["credential_seam"]["expires_at"] = expires_at

        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)future|issued|credential|lease")

    def test_credential_nonce_ledger_is_unique_live_and_one_shot(self) -> None:
        request = self._input()
        request["credential_nonce_ledger"]["reserved_nonces"][1] = request[
            "credential_nonce_ledger"
        ]["reserved_nonces"][0]
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)nonce|ledger|unique|duplicate")

        request = self._input()
        request["nodes"][0]["credential_seam"]["expires_at"] = "2020-01-01T00:00:00Z"
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)nonce|credential|expir")

        request = self._input()
        request["credential_nonce_ledger"]["replay"] = True
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)one.?shot|replay|ledger|nonce")

        request = self._input()
        request["credential_nonce_ledger"]["receipt"]["bindings"]["capture_window_id"] = "other-window"
        with self.assertRaises(SystemExit) as raised:
            self.module.build_plan(request)
        self.assertRegex(str(raised.exception), r"(?i)receipt|ledger|binding")

    def test_operation_journal_binds_transaction_capture_targets_and_truth(self) -> None:
        plan = self.module.build_plan(self._input())
        self.assertTrue(plan["operation_journal"])
        for entry in plan["operation_journal"]:
            self.assertEqual(entry["transaction_id"], "txn-clean-room-001")
            self.assertEqual(entry["capture_window_id"], "capture-window-20260901-001")
            if entry["node"] is not None:
                self.assertIn("target_root", entry)
                self.assertEqual(entry["package_commit"], "d" * 40)

    def test_plan_is_deterministic_and_contains_no_secret_or_mutation_command(self) -> None:
        request = self._input()
        first = self.module.build_plan(request)
        shuffled = json.loads(json.dumps(request))
        shuffled["nodes"] = list(reversed(shuffled["nodes"]))
        second = self.module.build_plan(shuffled)
        self.assertEqual(first, second)
        serialized = json.dumps(first, sort_keys=True)
        self.assertNotRegex(serialized, r"(?i)(password=|secret-value|token-value|private.?key-bytes)")
        self.assertEqual(first["execution"]["mode"], "plan-only")
        self.assertFalse(first["execution"]["provider_mutation_performed"])

    def test_cli_rejects_missing_identity_v2_evidence_map(self) -> None:
        request = self._input()
        with tempfile.TemporaryDirectory() as directory:
            input_path = Path(directory) / "input.json"
            output_path = Path(directory) / "plan.json"
            input_path.write_text(json.dumps(request), encoding="utf-8")
            result = subprocess.run(
                [
                    "python3",
                    str(MODULE_PATH),
                    "--input",
                    str(input_path),
                    "--out",
                    str(output_path),
                    "--json",
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0, "legacy input bypassed identity-v2 admission")
            self.assertRegex(result.stderr, r"(?i)identity.?v2|evidence|admission|verification")
            self.assertFalse(output_path.exists())
            self.assertEqual(sorted(path.name for path in Path(directory).iterdir()), ["input.json"])

    def test_plan_accepts_current_v2_inventory_and_identity_receipt_freshness(self) -> None:
        """Current capture/rotation/time bindings are valid for every receipt."""
        request = self._input()
        self._bind_current_receipt_freshness(request)
        plan = self.module.build_plan(request)
        inventory_receipt = plan["deployment_inventory"]["receipt"]
        self.assertEqual(
            inventory_receipt["capture_window_id"], plan["capture_window_id"]
        )
        self.assertEqual(
            inventory_receipt["rotation_epoch"], self.module.CANONICAL_ROTATION_EPOCH
        )
        for node in plan["nodes"]:
            receipt = node["identity_receipt"]
            self.assertEqual(receipt["capture_window_id"], plan["capture_window_id"])
            self.assertEqual(receipt["rotation_epoch"], self.module.CANONICAL_ROTATION_EPOCH)

    def test_plan_rejects_inventory_and_identity_receipt_freshness_drift(self) -> None:
        """Shape-valid v2 receipts cannot rebind current capture authority."""
        mutations = (
            ("missing-capture-window", "capture_window_id", None, r"capture_window_id"),
            ("capture-window-mismatch", "capture_window_id", "other-window", r"capture_window_id"),
            ("missing-rotation-epoch", "rotation_epoch", None, r"rotation_epoch"),
            ("rotation-epoch-mismatch", "rotation_epoch", "rotation-attacker", r"rotation_epoch"),
            ("missing-issued-at", "issued_at", None, r"issued_at"),
            ("missing-expires-at", "expires_at", None, r"expires_at"),
            (
                "stale-window",
                "issued_at",
                "2020-01-01T00:00:00Z",
                r"issued_at|expires_at|stale|fresh",
            ),
            (
                "future-window",
                "issued_at",
                "2099-01-01T00:00:00Z",
                r"issued_at|future|fresh",
            ),
        )
        for scope, node_name in (("inventory", None), *(('identity', name) for name in self.module.NODE_ORDER)):
            for label, field, value, expected_error in mutations:
                with self.subTest(scope=scope, node=node_name, mutation=label):
                    request = self._input()
                    self._bind_current_receipt_freshness(request)
                    evidence = self._baseline_identity_v2_evidence
                    if scope == "inventory":
                        receipt = request["deployment_inventory"]["receipt"]
                    else:
                        receipt = next(
                            node["identity_receipt"]
                            for node in request["nodes"]
                            if node["name"] == node_name
                        )
                        if value is None:
                            evidence = self._tampered_evidence_artifact(
                                evidence,
                                node_name=node_name,
                                mutate=lambda envelope, field=field: envelope.pop(field, None),
                            )
                        else:
                            evidence = self._tampered_evidence_artifact(
                                evidence,
                                node_name=node_name,
                                mutate=lambda envelope, field=field, value=value: envelope.__setitem__(
                                    field, value
                                ),
                            )
                            if label == "stale-window":
                                evidence = self._tampered_evidence_artifact(
                                    evidence,
                                    node_name=node_name,
                                    mutate=lambda envelope: envelope.__setitem__(
                                        "expires_at", "2020-01-02T00:00:00Z"
                                    ),
                                )
                            elif label == "future-window":
                                evidence = self._tampered_evidence_artifact(
                                    evidence,
                                    node_name=node_name,
                                    mutate=lambda envelope: envelope.__setitem__(
                                        "expires_at", "2100-01-01T00:00:00Z"
                                    ),
                                )
                    if value is None:
                        receipt.pop(field)
                    else:
                        receipt[field] = value
                        if label == "stale-window":
                            receipt["expires_at"] = "2020-01-02T00:00:00Z"
                        elif label == "future-window":
                            receipt["expires_at"] = "2100-01-01T00:00:00Z"
                    receipt["canonical_digest"] = self.module._canonical_receipt_digest(
                        receipt,
                        excluded_fields=frozenset(
                            {"signed_payload_sha256"}
                            if scope == "inventory"
                            else {"peer_id"}
                        ),
                    )
                    with self.assertRaises(SystemExit) as raised:
                        self._build_plan_with_evidence(request, evidence)
                    self.assertRegex(
                        str(raised.exception),
                        expected_error + r"|identity|evidence|binding|envelope|verif",
                    )

    def test_plan_rejects_receipt_freshness_outside_plan_capture_window(self) -> None:
        """Current-looking v2 receipts cannot escape the plan's bounded capture interval."""
        mutations = (
            (
                "issued-before-plan-start",
                {"issued_at": "2026-08-29T23:59:59Z", "expires_at": "2099-01-01T00:00:00Z"},
            ),
            (
                "expires-after-plan-end",
                {"issued_at": "2026-09-01T00:00:00Z", "expires_at": "2100-01-01T00:00:00Z"},
            ),
            (
                "inverted-inside-capture-window",
                {"issued_at": "2026-08-31T00:00:00Z", "expires_at": "2026-08-30T12:00:00Z"},
            ),
        )
        for scope, node_name in (
            ("inventory", None),
            *(('identity', name) for name in self.module.NODE_ORDER),
        ):
            for label, freshness in mutations:
                with self.subTest(scope=scope, node=node_name, mutation=label):
                    request = self._input()
                    self._bind_current_receipt_freshness(request, **freshness)
                    evidence = self._baseline_identity_v2_evidence
                    if scope == "inventory":
                        receipt = request["deployment_inventory"]["receipt"]
                    else:
                        receipt = next(
                            node["identity_receipt"]
                            for node in request["nodes"]
                            if node["name"] == node_name
                        )
                        evidence = self._tampered_evidence_artifact(
                            evidence,
                            node_name=node_name,
                            mutate=lambda envelope, freshness=freshness: envelope.update(freshness),
                        )
                    receipt.update(freshness)
                    receipt["canonical_digest"] = self.module._canonical_receipt_digest(
                        receipt,
                        excluded_fields=frozenset(
                            {"signed_payload_sha256"}
                            if scope == "inventory"
                            else {"peer_id"}
                        ),
                    )
                    with self.assertRaises(SystemExit) as raised:
                        self._build_plan_with_evidence(request, evidence)
                    self.assertRegex(
                        str(raised.exception),
                        r"(?i)capture|issued|expires|fresh|window|stale|inverted|identity|evidence|binding|envelope|verif",
                    )

    def test_plan_ignores_direct_runtime_identity_v1_in_favor_of_retained_v2(self) -> None:
        """Caller-supplied raw v1 identity cannot replace retained v2 evidence."""
        request = self._input()
        raw_identity = self._raw_runtime_identity_receipt_v1_bytes(
            request["nodes"][0]["identity_receipt"]
        )
        request["nodes"][0]["identity_receipt"] = json.loads(raw_identity)
        plan = self.module.build_plan(request)
        self.assertTrue(
            all(node["identity_receipt"]["schema_version"] == self.module.IDENTITY_RECEIPT_SCHEMA
                for node in plan["nodes"])
        )

    @staticmethod
    def _raw_runtime_identity_receipt_v1_bytes(
        identity: dict[str, object], *, key_path: str = "/operator/keys/node-keypair.toml"
    ) -> bytes:
        """Return the runtime's compact, ordered v1 identity metadata bytes."""
        raw = {
            "schema_version": "oasis7.identity_receipt.v1",
            "node_id": identity["node_id"],
            "peer_id": identity["peer_id"],
            "key_path": key_path,
            "key_sha256": identity["key_sha256"],
            "key_size_bytes": identity["key_size_bytes"],
            "key_mode": int("0600", 8),
            "key_uid": identity["key_uid"],
            "key_gid": identity["key_gid"],
        }
        return json.dumps(raw, ensure_ascii=True, separators=(",", ":")).encode("utf-8")

    def test_plan_rejects_identity_metadata_rebound_without_new_raw_v1_digest(self) -> None:
        """Changing retained raw key metadata cannot reuse the prior v2 bindings."""
        request = self._input()
        evidence = self._tampered_evidence_artifact(
            self._baseline_identity_v2_evidence,
            artifact="raw_v1",
            mutate=lambda raw: raw.update(key_sha256="8" * 64, key_size_bytes=256),
        )
        with self.assertRaises(SystemExit) as raised:
            self._build_plan_with_evidence(request, evidence)
        self.assertRegex(
            str(raised.exception),
            r"(?i)identity|payload|digest|signature|binding|key|evidence|raw",
        )

    def test_executable_v2_sidecar_binds_exact_runtime_raw_v1_bytes(self) -> None:
        """The governed sidecar must hash the exact runtime bytes, including its key path."""
        producer = ROOT / "scripts" / "p2p-public-testnet-identity-receipt-v2.py"
        self.assertTrue(producer.is_file(), f"missing executable v2 producer: {producer}")
        self.assertNotEqual(producer.stat().st_mode & 0o111, 0, producer)

        request = self._input()
        identity = request["nodes"][0]["identity_receipt"]
        runtime_key_path = "/opt/oasis7/p2p-testnet/config/node-keypair.toml"
        raw_v1 = self._raw_runtime_identity_receipt_v1_bytes(
            identity, key_path=runtime_key_path
        )
        template = dict(identity)
        template.update(
            {
                "signed_payload_sha256": "a" * 64,
                "signature_hex": "d" * 128,
                "canonical_digest": "c" * 64,
            }
        )
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            raw_path = root / "identity-receipt.v1.raw"
            template_path = root / "identity-receipt.v2.template.json"
            output_path = root / "identity-receipt.v2.json"
            raw_path.write_bytes(raw_v1)
            template_path.write_text(
                json.dumps(template, sort_keys=True), encoding="utf-8"
            )
            result = subprocess.run(
                [
                    "python3",
                    str(producer),
                    "--raw-v1",
                    str(raw_path),
                    "--template",
                    str(template_path),
                    "--out",
                    str(output_path),
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            envelope = json.loads(output_path.read_text(encoding="utf-8"))

        self.assertEqual(envelope["schema_version"], self.module.IDENTITY_RECEIPT_SCHEMA)
        self.assertEqual(set(envelope), self.module.IDENTITY_RECEIPT_FIELDS)
        self.assertEqual(
            envelope["signed_payload_sha256"], hashlib.sha256(raw_v1).hexdigest()
        )
        self.assertNotEqual(
            envelope["signed_payload_sha256"],
            hashlib.sha256(
                self._raw_runtime_identity_receipt_v1_bytes(identity)
            ).hexdigest(),
        )
        self.assertNotIn("key_path", envelope)
        self.assertEqual(
            envelope["canonical_digest"],
            self.module._canonical_receipt_digest(
                envelope, excluded_fields=frozenset({"peer_id"})
            ),
        )

    def test_v2_sidecar_executes_signer_and_verifier_over_final_canonical_payload(self) -> None:
        """The final v2 payload must pass the existing signer and verifier seam."""
        sidecar = load_sidecar_module()
        request = self._input()
        identity = request["nodes"][0]["identity_receipt"]
        context = {
            "task_uid": request["task_uid"],
            "frozen_head_oid": request["head_oid"],
            "plan_digest": "p" * 64,
            "node_id": identity["node_id"],
            "peer_id": identity["peer_id"],
        }
        signer_calls: list[tuple[bytes, dict[str, object]]] = []
        verifier_calls: list[tuple[bytes, dict[str, object], dict[str, object]]] = []

        def signer(payload: bytes, received_context: dict[str, object]) -> str:
            signer_calls.append((payload, dict(received_context)))
            return "d" * 128

        def verifier(
            payload: bytes,
            envelope: dict[str, object],
            received_context: dict[str, object],
        ) -> dict[str, object]:
            verifier_calls.append((payload, dict(envelope), dict(received_context)))
            return {"verified": True}

        template = dict(identity)
        template.update(
            {
                "signed_payload_sha256": "a" * 64,
                "signature_hex": "d" * 128,
                "canonical_digest": "c" * 64,
            }
        )
        raw_v1 = self._raw_runtime_identity_receipt_v1_bytes(identity)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            raw_path = root / "identity-receipt.v1.raw"
            template_path = root / "identity-receipt.v2.template.json"
            output_path = root / "identity-receipt.v2.json"
            raw_path.write_bytes(raw_v1)
            template_path.write_text(json.dumps(template), encoding="utf-8")
            envelope = sidecar.create(
                raw_path,
                template_path,
                output_path,
                context=context,
                signer=signer,
                verifier=verifier,
            )

        self.assertEqual(len(signer_calls), 1)
        self.assertEqual(len(verifier_calls), 1)
        self.assertEqual(signer_calls[0][1], context)
        self.assertEqual(verifier_calls[0][2], context)
        self.assertEqual(verifier_calls[0][0], signer_calls[0][0])
        self.assertEqual(verifier_calls[0][1]["signature_hex"], "d" * 128)
        self.assertEqual(envelope["signature_hex"], "d" * 128)
        self.assertIn(request["task_uid"].encode(), signer_calls[0][0])
        self.assertIn(request["head_oid"].encode(), signer_calls[0][0])
        self.assertIn(context["plan_digest"].encode(), signer_calls[0][0])

    def test_executable_v2_sidecar_rejects_raw_template_identity_cross_pair(self) -> None:
        """A raw identity from one node cannot be wrapped with another node's template."""
        request = self._input()
        raw_identity = request["nodes"][0]["identity_receipt"]
        template_identity = request["nodes"][1]["identity_receipt"]
        raw_v1 = self._raw_runtime_identity_receipt_v1_bytes(
            raw_identity, key_path="/opt/oasis7/p2p-testnet/config/node-keypair.toml"
        )
        template = dict(template_identity)
        template.update(
            {
                "signed_payload_sha256": "a" * 64,
                "signature_hex": "d" * 128,
                "canonical_digest": "c" * 64,
            }
        )
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            raw_path = root / "identity-receipt.v1.raw"
            template_path = root / "identity-receipt.v2.template.json"
            output_path = root / "identity-receipt.v2.json"
            raw_path.write_bytes(raw_v1)
            template_path.write_text(json.dumps(template), encoding="utf-8")
            result = subprocess.run(
                [
                    "python3",
                    str(SIDECAR_PATH),
                    "--raw-v1",
                    str(raw_path),
                    "--template",
                    str(template_path),
                    "--out",
                    str(output_path),
                ],
                check=False,
                capture_output=True,
                text=True,
            )
        self.assertNotEqual(result.returncode, 0, result.stderr)
        self.assertRegex(result.stderr, r"(?i)node|peer|key|identity|match")
        self.assertFalse(output_path.exists())

    def test_executable_v2_sidecar_rejects_malformed_template_freshness(self) -> None:
        """Malformed freshness metadata must never be emitted as a v2 envelope."""
        request = self._input()
        identity = request["nodes"][0]["identity_receipt"]
        template = dict(identity)
        template.update(
            {
                "issued_at": "not-an-rfc3339-timestamp",
                "signed_payload_sha256": "a" * 64,
                "signature_hex": "d" * 128,
                "canonical_digest": "c" * 64,
            }
        )
        raw_v1 = self._raw_runtime_identity_receipt_v1_bytes(identity)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            raw_path = root / "identity-receipt.v1.raw"
            template_path = root / "identity-receipt.v2.template.json"
            output_path = root / "identity-receipt.v2.json"
            raw_path.write_bytes(raw_v1)
            template_path.write_text(json.dumps(template), encoding="utf-8")
            result = subprocess.run(
                [
                    "python3",
                    str(SIDECAR_PATH),
                    "--raw-v1",
                    str(raw_path),
                    "--template",
                    str(template_path),
                    "--out",
                    str(output_path),
                ],
                check=False,
                capture_output=True,
                text=True,
            )
        self.assertNotEqual(result.returncode, 0, result.stderr)
        self.assertRegex(result.stderr, r"(?i)issued|expires|timestamp|fresh|time")
        self.assertFalse(output_path.exists())

    def test_sidecar_output_cannot_replace_retained_v2_evidence(self) -> None:
        """An unverified sidecar output cannot replace retained v2 evidence."""
        request = self._input()
        identity = request["nodes"][0]["identity_receipt"]
        raw_v1 = self._raw_runtime_identity_receipt_v1_bytes(identity)
        template = dict(identity)
        template.update(
            {
                "signed_payload_sha256": "a" * 64,
                "signature_hex": "d" * 128,
                "canonical_digest": "c" * 64,
            }
        )
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            raw_path = root / "identity-receipt.v1.raw"
            template_path = root / "identity-receipt.v2.template.json"
            output_path = root / "identity-receipt.v2.json"
            raw_path.write_bytes(raw_v1)
            template_path.write_text(json.dumps(template), encoding="utf-8")
            result = subprocess.run(
                [
                    "python3",
                    str(SIDECAR_PATH),
                    "--raw-v1",
                    str(raw_path),
                    "--template",
                    str(template_path),
                    "--out",
                    str(output_path),
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            request["nodes"][0]["identity_receipt"] = json.loads(
                output_path.read_text(encoding="utf-8")
            )

        plan = self.module.build_plan(request)
        self.assertEqual(
            plan["identity_v2_evidence"], self._baseline_identity_v2_evidence
        )
        self.assertTrue(
            all(node["identity_receipt"]["schema_version"] == self.module.IDENTITY_RECEIPT_SCHEMA
                for node in plan["nodes"])
        )

    def test_plan_projects_identity_commands_through_governed_v2_sidecar(self) -> None:
        """Every authoritative identity command must use the executable v2 sidecar."""
        runbook = (
            (ROOT / "doc/p2p/blockchain/public-testnet-governed-bootstrap.runbook.md").read_text(encoding="utf-8")
            + "\n"
            + (ROOT / "doc/p2p/blockchain/public-testnet-governed-bootstrap-phase-c-rebuild.runbook.md").read_text(encoding="utf-8")
        )
        self.assertIn("p2p-public-testnet-identity-receipt-v2.py", runbook)
        self.assertIn("--raw-v1", runbook)
        self.assertIn("--template", runbook)
        self.assertIn("--out", runbook)
        self.assertNotIn("oasis7_chain_runtime identity-receipt --config-dir", runbook)

    def test_plan_rejects_synthetic_identity_digest_and_signature_pair(self) -> None:
        """A synthetic retained envelope cannot become an apply-ready identity."""
        request = self._input()
        evidence = self._tampered_evidence_artifact(
            self._baseline_identity_v2_evidence,
            mutate=lambda envelope: envelope.update(
                signed_payload_sha256="a" * 64,
                signature_hex="b" * 128,
                canonical_digest="c" * 64,
            ),
        )
        with self.assertRaises(SystemExit) as raised:
            self._build_plan_with_evidence(request, evidence)
        self.assertRegex(str(raised.exception), r"(?i)identity|payload|digest|placeholder|evidence|binding")

    def test_governed_bootstrap_runbook_describes_v2_identity_envelope_boundary(self) -> None:
        """The authoritative runbook must retire raw v1 direct admission and specify v2."""
        runbook = (
            (ROOT / "doc/p2p/blockchain/public-testnet-governed-bootstrap.runbook.md").read_text(encoding="utf-8")
            + "\n"
            + (ROOT / "doc/p2p/blockchain/public-testnet-governed-bootstrap-phase-c-rebuild.runbook.md").read_text(encoding="utf-8")
        )
        self.assertRegex(
            runbook,
            r"(?is)oasis7\.identity_receipt\.v1.{0,800}(?:raw|原始).{0,800}(?:retir|退役).{0,800}(?:direct admission|直接.*(?:admission|准入))",
        )
        self.assertIn("oasis7.identity_receipt.v2", runbook)
        for field in (
            "signed_payload_sha256",
            "signature_hex",
            "canonical_digest",
            "node_id",
            "peer_id",
            "key_sha256",
            "key_size_bytes",
            "key_mode",
            "key_uid",
            "key_gid",
            "task_uid",
            "head_oid",
            "plan_digest",
            "capture_window_id",
            "rotation_epoch",
            "issued_at",
            "expires_at",
        ):
            with self.subTest(field=field):
                self.assertIn(field, runbook)
        self.assertRegex(
            runbook,
            r"(?is)(?:independent|governed).{0,500}verifier.{0,500}(?:before|prior to).{0,500}(?:provider|mutation|admission)",
        )

    def _explicit_inventory(self, request: dict[str, object]) -> dict[str, object]:
        inventory = self._deployment_inventory(request["nodes"])
        for node in request["nodes"]:
            name = str(node["name"])
            inventory["nodes"][name]["peer_id"] = self.module.CANONICAL_PEER_REGISTRY[name]
        inventory["receipt"]["signed_payload_sha256"] = (
            self.module._canonical_deployment_inventory_payload_digest(inventory)
        )
        return inventory

    def test_plan_requires_explicit_peer_id_on_every_inventory_node(self) -> None:
        """Legacy omitted/partial peer identities must not enter authenticated plans."""
        for omission in ("all", "storage-205"):
            with self.subTest(omission=omission):
                request = self._input()
                inventory = self._explicit_inventory(request)
                if omission == "all":
                    for node in inventory["nodes"].values():
                        node.pop("peer_id")
                else:
                    inventory["nodes"][omission].pop("peer_id")
                request["deployment_inventory"] = inventory
                with self.assertRaises(SystemExit) as raised:
                    self.module.build_plan(request)
                self.assertRegex(str(raised.exception), r"(?i)peer|inventory|explicit|complete")

    def test_plan_validates_inventory_digest_before_normalization(self) -> None:
        """Inventory field mutations must fail against the incoming stale receipt."""
        mutations = ("node_root", "persistent_state_paths", "expected_key_uid", "expected_key_gid", "peer_id")
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                request = self._input()
                inventory = self._explicit_inventory(request)
                request["deployment_inventory"] = inventory
                name = "storage-205"
                inventory_node = inventory["nodes"][name]
                node = next(item for item in request["nodes"] if item["name"] == name)
                if mutation in {"node_root", "persistent_state_paths"}:
                    old_root = node["node_root"]
                    new_root = "/opt/oasis7/attacker-root"
                    node["node_root"] = new_root
                    node["persistent_state_paths"] = [
                        path.replace(old_root, new_root, 1)
                        for path in node["persistent_state_paths"]
                    ]
                    inventory_node["node_root"] = new_root
                    inventory_node["persistent_state_paths"] = list(node["persistent_state_paths"])
                elif mutation == "expected_key_uid":
                    inventory_node["expected_key_uid"] = 1001
                    node["identity_receipt"]["key_uid"] = 1001
                elif mutation == "expected_key_gid":
                    inventory_node["expected_key_gid"] = 1002
                    node["identity_receipt"]["key_gid"] = 1002
                else:
                    rotated_peer = "12D3KooWstale-inventory-peer"
                    inventory_node["peer_id"] = rotated_peer
                    node["identity_receipt"]["peer_id"] = rotated_peer
                with self.assertRaises(SystemExit) as raised:
                    self.module.build_plan(request)
                self.assertRegex(str(raised.exception), r"(?i)inventory|receipt|digest|binding|peer|canonical")

    def test_plan_accepts_fully_explicit_digest_bound_inventory(self) -> None:
        """The governed explicit inventory schema remains a valid admission path."""
        request = self._input()
        request["deployment_inventory"] = self._explicit_inventory(request)
        plan = self.module.build_plan(request)
        for name in self.module.NODE_ORDER:
            self.assertIn("peer_id", plan["deployment_inventory"]["nodes"][name])
        self.assertEqual(
            plan["deployment_inventory"]["receipt"]["signed_payload_sha256"],
            self.module._canonical_deployment_inventory_payload_digest(
                plan["deployment_inventory"]
            ),
        )


class PlannerOutputAliasTests(unittest.TestCase):
    def test_cli_cannot_replace_held_fleet_lock(self):
        for alias in ("exact", "normalized", "hardlink"):
            with self.subTest(alias=alias), tempfile.TemporaryDirectory() as directory:
                module = load_module()
                root = Path(directory).resolve()
                paths, source, evidence = self._authority_fixture(module, root)
                spec = importlib.util.spec_from_file_location(
                    "planner_cli_lock_adapter", ROOT / "scripts/p2p-public-testnet-full-network-clean-room-adapter.py")
                adapter = importlib.util.module_from_spec(spec)
                spec.loader.exec_module(adapter)
                lock_path = root / "full-network-clean-room.lock"
                adapter.CANONICAL_FLEET_LOCK_PATH = str(lock_path)
                with patch.object(module._peer_registry_authority(), "CANONICAL_FLEET_LOCK_PATH", str(lock_path)):
                    first = adapter._acquire_fleet_transaction_guard(root / "first.json")
                    try:
                        plan = {"plan_digest": "test", "execution": {"mode": "plan-only"}}
                        with patch.object(module, "build_plan", return_value=plan):
                            ordinary = root / "ordinary-plan.json"
                            self.assertEqual(module.main(["--input", str(source), "--identity-v2-evidence-map", str(evidence), "--out", str(ordinary)]), 0)
                        output = lock_path
                        if alias == "normalized":
                            (root / "nested").mkdir()
                            output = root / "nested" / ".." / lock_path.name
                        elif alias == "hardlink":
                            output = root / "hardlink"
                            os.link(lock_path, output)
                        before = (lock_path.read_bytes(), lock_path.stat().st_ino,
                                  output.read_bytes(), output.stat().st_ino)
                        with patch.object(module, "build_plan", return_value=plan) as build:
                            try:
                                module.main(["--input", str(source), "--identity-v2-evidence-map", str(evidence), "--out", str(output)])
                            except SystemExit:
                                rejected = True
                            else:
                                rejected = False
                        try:
                            second = adapter._acquire_fleet_transaction_guard(root / "second.json")
                        except adapter.AdapterError:
                            blocked = True
                        else:
                            blocked = False
                            second.close()
                        after = (lock_path.read_bytes(), lock_path.stat().st_ino,
                                 output.read_bytes(), output.stat().st_ino)
                        self.assertEqual((rejected, build.call_count, before == after, blocked),
                                         (True, 0, True, True),
                                         "CLI must reject lock aliases before plan building or publication")
                    finally:
                        first.close()

    def test_governance_root_is_protected_without_opening_it(self):
        module = load_module()
        protected = module._plan_output_inputs(Path("/fixture/input"), Path("/fixture/map"), {})
        for path in (Path("/operator/truth/governance-root.json"),
                     Path(module.__file__).with_name("p2p-public-testnet-full-network-clean-room-adapter.py"),
                     Path(module.__file__).with_name("fixtures") / "oasis7-governance-root.v1.json"):
            with self.subTest(path=path), self.assertRaises(SystemExit):
                module._reject_plan_output_aliases(path, protected)

    def _authority_fixture(self, module, root):
        paths = {name: root / name for name in
                 ("tool", "trust", "registry", "registry-verifier", "trust-key", "provider-key", "provider-adapter")}
        for path in paths.values():
            path.write_bytes(b"fixture authority bytes")
            path.chmod(0o700)
        paths["trust"].write_text(json.dumps({"allowlist": [{"public_key_ref": str(paths["trust-key"])}]}))
        paths["registry"].write_text(json.dumps({
            "trust_config_path": str(paths["trust"]),
            "providers": [{"public_key_ref": str(paths["provider-key"]), "adapter_path": str(paths["provider-adapter"])}],
            "verifier": {"executable_path": str(paths["registry-verifier"])},
        }))
        for prefix, name in (("VERIFY_TOOL", "tool"), ("TRUST_CONFIG", "trust"), ("PROVIDER_REGISTRY", "registry")):
            setattr(module, f"IDENTITY_V2_{prefix}_PATH", paths[name])
            setattr(module, f"IDENTITY_V2_{prefix}_SHA256", _fixture_digest(paths[name]))
        source, evidence = root / "input.json", root / "map.json"
        source.write_text("{}")
        evidence.write_text("{}")
        return paths, source, evidence

    def test_cli_protects_complete_pinned_authority_closure(self):
        for name in ("tool", "trust", "registry", "registry-verifier", "trust-key", "provider-key", "provider-adapter"):
            for alias in ("direct", "resolved", "hardlink"):
                with self.subTest(name=name, alias=alias), tempfile.TemporaryDirectory() as directory:
                    module = load_module()
                    root = Path(directory).resolve()
                    paths, source, evidence = self._authority_fixture(module, root)
                    output = paths[name]
                    if alias == "resolved":
                        parent = root / "parent-alias"
                        parent.symlink_to(root, target_is_directory=True)
                        output = parent / name
                    elif alias == "hardlink":
                        output = root / "hardlink"
                        os.link(paths[name], output)
                    before = {path: (path.read_bytes(), path.stat()) for path in paths.values()}
                    entries = set(root.iterdir())
                    with patch.object(module, "build_plan", return_value={"plan_digest": "test", "execution": {"mode": "plan-only"}}) as build:
                        with self.assertRaises(SystemExit):
                            module.main(["--input", str(source), "--identity-v2-evidence-map", str(evidence), "--out", str(output)])
                        build.assert_not_called()
                    self.assertEqual(set(root.iterdir()), entries)
                    for path, (raw, metadata) in before.items():
                        self.assertEqual(path.read_bytes(), raw)
                        self.assertEqual(module._identity_v2_anchor_identity(path.stat()), module._identity_v2_anchor_identity(metadata))
        with tempfile.TemporaryDirectory() as directory:
            module = load_module()
            root = Path(directory).resolve()
            paths, source, evidence = self._authority_fixture(module, root)
            output = root / "ordinary-plan.json"
            with patch.object(module, "build_plan", return_value={"plan_digest": "test", "execution": {"mode": "plan-only"}}) as build:
                self.assertEqual(module.main(["--input", str(source), "--identity-v2-evidence-map", str(evidence), "--out", str(output)]), 0)
                build.assert_called_once()
            self.assertEqual(json.loads(output.read_text())["plan_digest"], "test")

    def test_output_closure_rejects_unpinned_reference_bytes_before_build(self):
        with tempfile.TemporaryDirectory() as directory:
            module = load_module()
            root = Path(directory).resolve()
            paths, source, evidence = self._authority_fixture(module, root)
            paths["registry"].write_text(json.dumps({"providers": []}))
            output = root / "new-parent" / "plan.json"
            with patch.object(module, "build_plan", return_value={"plan_digest": "test", "execution": {"mode": "plan-only"}}) as build:
                with self.assertRaisesRegex(SystemExit, "pin|digest"):
                    module.main(["--input", str(source), "--identity-v2-evidence-map", str(evidence), "--out", str(output)])
                build.assert_not_called()
            self.assertFalse(output.parent.exists())

    def test_atomic_output_preserves_prior_plan_on_replace_failure(self):
        module = load_module()
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "plan.json"
            output.write_bytes(b"prior plan\n")
            with patch.object(module.os, "replace", side_effect=OSError("injected failure")):
                with self.assertRaises(OSError):
                    module._write_plan_atomic(output, {"plan": "new"}, [])
            self.assertEqual(output.read_bytes(), b"prior plan\n")
            self.assertEqual(list(output.parent.iterdir()), [output])
            module._write_plan_atomic(output, {"plan": "new"}, [])
            self.assertEqual(json.loads(output.read_text()), {"plan": "new"})

    def test_cli_rejects_output_aliases_without_modifying_evidence(self):
        module = load_module()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, evidence_path, artifact = (root / name for name in ("input.json", "map.json", "raw.bin"))
            source.write_text("{}")
            artifact.write_bytes(b"exact runtime bytes")
            descriptor = {"path": str(artifact), "sha256": hashlib.sha256(artifact.read_bytes()).hexdigest(), "size_bytes": artifact.stat().st_size}
            evidence_path.write_text(json.dumps({"context": descriptor, "plan_intent": descriptor, "entries": [{"raw_v1": descriptor}]}))
            hardlink = root / "hardlink"
            os.link(artifact, hardlink)
            parent_alias = root / "parent-alias"
            parent_alias.symlink_to(root, target_is_directory=True)
            before = {path: path.read_bytes() for path in (source, evidence_path, artifact)}
            for output in (source, evidence_path, artifact, hardlink, parent_alias / artifact.name):
                with self.subTest(output=str(output)), patch.object(module, "build_plan", return_value={"plan_digest": "test", "execution": {"mode": "plan-only"}}):
                    with self.assertRaises(SystemExit):
                        module.main(["--input", str(source), "--identity-v2-evidence-map", str(evidence_path), "--out", str(output)])
                    self.assertEqual({path: path.read_bytes() for path in before}, before)


class IdentityV2AdmissionAnchorTests(unittest.TestCase):
    """Exercise the planner's filesystem boundary without signing fixtures."""

    def setUp(self) -> None:
        self.module = load_module()
        self.directory = tempfile.TemporaryDirectory(prefix="oasis7-anchor-test-")
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name).resolve()
        self.root.chmod(0o700)
        self.anchors = {}
        for prefix in ("VERIFY_TOOL", "TRUST_CONFIG", "PROVIDER_REGISTRY"):
            path = self.root / prefix
            path.write_bytes(b"# fixture authority\n")
            path.chmod(0o700 if prefix == "VERIFY_TOOL" else 0o644)
            self.anchors[prefix] = path
            setattr(self.module, f"IDENTITY_V2_{prefix}_PATH", path)
            setattr(self.module, f"IDENTITY_V2_{prefix}_SHA256", _fixture_digest(path))
        self.receipt = {
            "verified": True, "mode": "current_admission",
            "historical_only": False, "apply_authorized": True,
        }
        self.envelope = {"authenticated": True, "verified": True}
        self.entry = {"node_name": "fixture-node"}
        for field, value in (("raw_v1", {}), ("signed_envelope", self.envelope),
                             ("provider_attestation", {}), ("verification", self.receipt)):
            path = self.root / f"{field}.json"
            _write_fixture_json(path, value)
            path.chmod(0o600)
            self.entry[field] = _fixture_descriptor(path)
        # Control: public 0644 metadata and the platform's protected temp
        # ancestry must admit before each targeted unsafe mutation is applied.
        with patch.object(self.module.subprocess, "run", side_effect=self._successful_child):
            self.assertEqual(set(self._verify()), {"fixture-0"})

    def _verify(self, count: int = 1) -> object:
        entries = [dict(self.entry, node_name=f"fixture-{index}") for index in range(count)]
        return self.module._independently_verify_identity_v2_entries(
            {"entries": entries}, self.root / "context", self.root / "intent"
        )

    def _successful_child(self, command, **kwargs):
        # This child deliberately mirrors retained claims; the parent must
        # establish executable identity before trusting such matching output.
        _write_fixture_json(Path(command[command.index("--out") + 1]), self.envelope)
        _write_fixture_json(Path(command[command.index("--verification-out") + 1]), self.receipt)
        return subprocess.CompletedProcess(command, 0, "", "")

    def test_anchor_rejects_non_owner_write_permissions(self) -> None:
        for name, path in self.anchors.items():
            for mode in (0o720, 0o702):
                with self.subTest(anchor=name, mode=oct(mode)):
                    path.chmod(mode)
                    try:
                        with self.assertRaises(SystemExit):
                            self.module._identity_v2_admission_anchors()
                    finally:
                        path.chmod(0o700 if name == "VERIFY_TOOL" else 0o644)

    def test_anchor_rejects_wrong_operator_owner(self) -> None:
        with patch.object(os, "getuid", return_value=os.getuid() + 1):
            with self.assertRaises(SystemExit):
                self.module._identity_v2_admission_anchors()

    def test_anchor_rejects_non_executable_verifier(self) -> None:
        self.anchors["VERIFY_TOOL"].chmod(0o644)
        with self.assertRaises(SystemExit):
            self.module._identity_v2_admission_anchors()

    def test_anchor_rejects_writable_ancestor(self) -> None:
        self.root.chmod(0o777)  # Not sticky: not the supported root-owned /tmp exception.
        with self.assertRaises(SystemExit):
            self.module._identity_v2_admission_anchors()

    def test_anchor_rejects_foreign_owned_0755_ancestor(self) -> None:
        real_stat = Path.stat
        def parent_stat(path, *args, **kwargs):
            metadata = real_stat(path, *args, **kwargs)
            if path == self.root:
                values = list(metadata)
                values[0] = stat.S_IFDIR | 0o755
                values[4] = os.getuid() + 1
                return os.stat_result(values)
            return metadata
        with patch.object(Path, "stat", parent_stat):
            with self.assertRaises(SystemExit):
                self.module._identity_v2_admission_anchors()

    def test_anchor_rejects_symlink_ancestor(self) -> None:
        alias = self.root / "alias"
        alias.symlink_to(self.root, target_is_directory=True)
        self.module.IDENTITY_V2_VERIFY_TOOL_PATH = alias / "VERIFY_TOOL"
        with self.assertRaises(SystemExit):
            self.module._identity_v2_admission_anchors()

    def test_anchor_rejects_replacement_during_read(self) -> None:
        tool = self.anchors["VERIFY_TOOL"]
        real_read, real_fstat = Path.read_bytes, os.fstat
        original_inode = tool.stat().st_ino
        replaced = False

        def replace():
            nonlocal replaced
            if not replaced:
                replacement = self.root / "replacement"
                replacement.write_bytes(b"# replaced verifier\n")
                replacement.chmod(0o700)
                replacement.replace(tool)
                replaced = True

        def read(path):
            raw = real_read(path)
            if path == tool:
                replace()
            return raw

        def fstat(descriptor):
            metadata = real_fstat(descriptor)
            if metadata.st_ino == original_inode:
                replace()
            return metadata

        with patch.object(Path, "read_bytes", side_effect=read, autospec=True), patch.object(os, "fstat", side_effect=fstat):
            with self.assertRaises(SystemExit):
                self.module._identity_v2_admission_anchors()
        self.assertTrue(replaced)

    def test_anchor_mutation_after_initial_pin_before_child_is_rejected(self) -> None:
        original_descriptor = self.module._evidence_descriptor
        for name, path in self.anchors.items():
            with self.subTest(anchor=name):
                original = path.read_bytes()
                changed = False

                def descriptor(*args):
                    nonlocal changed
                    result = original_descriptor(*args)
                    if not changed:
                        path.write_bytes(b"# changed before child\n")
                        changed = True
                    return result

                try:
                    with patch.object(self.module, "_evidence_descriptor", side_effect=descriptor), patch.object(self.module.subprocess, "run", side_effect=self._successful_child) as child:
                        with self.assertRaises(SystemExit):
                            self._verify()
                        child.assert_not_called()
                finally:
                    path.write_bytes(original)

    def test_anchor_mutation_during_child_is_rejected_before_next_entry(self) -> None:
        for name, path in self.anchors.items():
            with self.subTest(anchor=name):
                original = path.read_bytes()
                calls = []

                def child(command, **kwargs):
                    calls.append(command)
                    result = self._successful_child(command, **kwargs)
                    path.write_bytes(b"# changed during child\n")
                    return result

                try:
                    with patch.object(self.module.subprocess, "run", side_effect=child):
                        with self.assertRaises(SystemExit):
                            self._verify(count=2)
                    self.assertEqual(len(calls), 1, "reject before dispatching another node")
                finally:
                    path.write_bytes(original)


class StorageFirstContractRedTests(unittest.TestCase):
    """RED contract for the code-owned storage-205-first child plan.

    This fixture is intentionally shape-only: it exercises the planner API
    without creating signed material or touching a node.  The implementation
    must still perform the full parent-plan admission checks before returning
    a storage-only child contract.
    """

    @classmethod
    def setUpClass(cls) -> None:
        cls.module = load_module()

    def _parent_plan(self) -> dict[str, object]:
        node_order = list(self.module.NODE_ORDER)
        nodes = []
        for name in node_order:
            nodes.append({
                "name": name,
                "role": "validator" if name in {"storage-205", "sequencer-204"} else "observer",
                "host_binding": {
                    "known_hosts_path": " /operator/known-hosts ".strip(),
                    "known_host_fingerprint": f"fingerprint:{name}",
                },
                "endpoints": {
                    "evidence": "http://sequencer/v1/chain/rebuild-proof"
                    if name == "sequencer-204" else f"http://{name}/v1/chain/status",
                },
                "identity_receipt": {
                    "schema_version": "oasis7.identity_receipt.v2",
                    "node_name": name,
                },
            })
        return {
            "schema_version": "oasis7.public_testnet_full_network_clean_room_plan.v1",
            "task_uid": "task_90c2722f6e1c48c3aebb6283cee471ce",
            "head_oid": "9" * 40,
            "transaction_id": "txn-storage-first-red",
            "capture_window_id": "capture-storage-first-red",
            "node_order": node_order,
            "global_order": [
                *(f"preflight:{name}" for name in node_order),
                "stop:storage-205", "delete:storage-205", "rebuild:storage-205",
                "start:storage-205", "verify:storage-205", "fresh-root-probe", "fleet-health",
            ],
            "nodes": nodes,
            "identity_v2_evidence": {
                "digest": "i" * 64,
                "mode": "current_admission",
                "entries": [{"node_name": name} for name in node_order],
            },
            "forensic_backup": {
                "action": "full-network-clean-room",
                "targets": node_order,
                "receipt": {"authenticated": True, "signed": True},
            },
            "credential_nonce_ledger": {
                "path": "/operator/nonce-ledger.jsonl",
                "count": 5,
                "reservations": [{"node": name} for name in node_order],
            },
            "known_hosts_digest": "k" * 64,
            "sequencer_proof": {
                "endpoint": "http://sequencer/v1/chain/rebuild-proof",
                "bounded": True,
            },
            "consumer_impact_record": {"sha256": "c" * 64, "decision": "proceed"},
            "package_provenance_digest": "p" * 64,
            "deployment_inventory_digest": "d" * 64,
            "independent_verifier": {
                "verifier_id": "governed-receipt-verifier",
                "trust_root_id": "oasis7-public-testnet-governance-root-v1",
            },
            "plan_digest": "q" * 64,
        }

    def _build(self, parent: dict[str, object] | None = None) -> dict[str, object]:
        builder = getattr(self.module, "build_storage_first_contract", None)
        self.assertTrue(callable(builder), "RED: missing code-owned storage-first planner API")
        return builder(copy.deepcopy(parent or self._parent_plan()))

    def _resume_validator(self):
        validator = getattr(self.module, "validate_storage_first_resume", None)
        self.assertTrue(callable(validator), "RED: missing storage-first resume validator API")
        return validator

    def test_storage_first_contract_is_explicit_and_targets_storage_only(self):
        contract = self._build()
        self.assertEqual(contract["phase_id"], "storage-205-first")
        self.assertEqual(contract["target_nodes"], ["storage-205"])
        self.assertEqual(contract["target_set_is_exact"], True)

    def test_storage_first_retains_canonical_parent_node_order(self):
        parent = self._parent_plan()
        contract = self._build(parent)
        self.assertEqual(contract["parent_node_order"], list(self.module.NODE_ORDER))
        for changed_order in (list(reversed(self.module.NODE_ORDER)), ["storage-205"]):
            changed = copy.deepcopy(parent)
            changed["node_order"] = changed_order
            with self.assertRaises(Exception):
                self._build(changed)

    def test_storage_first_requires_full_parent_identity_v2_closure(self):
        parent = self._parent_plan()
        self.assertTrue(
            callable(getattr(self.module, "build_storage_first_contract", None)),
            "RED: missing code-owned storage-first planner API",
        )
        for mutation in ("missing-node", "historical-audit", "digest-drift"):
            changed = copy.deepcopy(parent)
            if mutation == "missing-node":
                changed["identity_v2_evidence"]["entries"].pop()
            elif mutation == "historical-audit":
                changed["identity_v2_evidence"]["mode"] = "historical_audit"
            else:
                changed["identity_v2_evidence"]["digest"] = "x" * 64
            with self.subTest(mutation=mutation), self.assertRaises(Exception):
                self._build(changed)

    def test_storage_first_accepts_retained_identity_map_and_derives_child_fields(self):
        """The child derives mode/digest without widening signed parent evidence."""
        parent = self._parent_plan()
        retained = parent["identity_v2_evidence"]
        retained.pop("mode")
        retained.pop("digest")
        contract = self._build(parent)
        expected_digest = hashlib.sha256(
            json.dumps(
                retained,
                ensure_ascii=True,
                sort_keys=True,
                separators=(",", ":"),
            ).encode()
        ).hexdigest()
        self.assertEqual(contract["identity_v2_digest"], expected_digest)

    def test_storage_first_requires_distinct_signed_no_backup_scope(self):
        contract = self._build()
        scope = contract["no_backup_authority_scope"]
        self.assertEqual(scope["action"], "storage-205-first")
        self.assertEqual(scope["targets"], ["storage-205"])
        self.assertTrue(scope["signed"])
        self.assertNotEqual(scope["targets"], self._parent_plan()["forensic_backup"]["targets"])

    def test_storage_first_requires_parent_nonce_ledger_without_nonce_values_in_journal(self):
        contract = self._build()
        self.assertEqual(contract["parent_nonce_ledger"]["required_reservations"], 5)
        journal = contract["journal_template"]
        forbidden = {"nonce", "nonce_value", "credential", "credential_value", "secret"}
        seen = []
        def walk(value):
            if isinstance(value, dict):
                for key, item in value.items():
                    if str(key).lower() in forbidden:
                        seen.append(key)
                    walk(item)
            elif isinstance(value, list):
                for item in value:
                    walk(item)
        walk(journal)
        self.assertEqual(seen, [])

    def test_storage_first_requires_strict_known_hosts_and_bounded_sequencer_proof(self):
        contract = self._build()
        self.assertEqual(contract["known_hosts"]["source"], "canonical-operator-pinned")
        self.assertEqual(contract["known_hosts"]["target"], "storage-205")
        self.assertEqual(contract["sequencer_proof"]["operation"], "bounded-proof:sequencer-204")
        self.assertNotIn("/v1/chain/status", json.dumps(contract["sequencer_proof"], sort_keys=True))

    def test_storage_first_operation_order_is_stop_delete_rebuild_start_verify(self):
        contract = self._build()
        self.assertEqual(contract["mutating_operations"], [
            "stop:storage-205", "delete:storage-205", "rebuild:storage-205",
            "start:storage-205", "verify:storage-205",
        ])

    def test_storage_first_verified_boundary_cannot_claim_probe_or_fleet_health(self):
        contract = self._build()
        self.assertEqual(contract["completion_boundary"], "storage-205-verified-pending-sequencer-probe")
        self.assertEqual(set(contract["never_claim"]), {
            "full-network-complete", "fresh-root-proven", "fleet-health-proven",
        })

    def test_storage_first_resume_rejects_binding_drift_and_ambiguous_states(self):
        contract = self._build()
        validator = self._resume_validator()
        journal = copy.deepcopy(contract["journal_template"])
        self.assertTrue(validator(contract, journal))
        for mutation in (
            "plan_digest", "transaction_id", "phase_contract_digest",
            "unknown_status", "ambiguous", "ambiguous-with-receipt",
        ):
            changed = copy.deepcopy(journal)
            if mutation == "unknown_status":
                changed["status"] = "unknown_status"
            elif mutation in {"ambiguous", "ambiguous-with-receipt"}:
                changed["status"] = "storage-205-running"
                changed["next_operation"] = "stop:storage-205"
                changed["callback_started"] = True
                changed["callback_receipt"] = (
                    None if mutation == "ambiguous" else {"forged": True}
                )
            else:
                changed[mutation] = "drifted"
            with self.subTest(mutation=mutation), self.assertRaises(Exception):
                validator(contract, changed)

    def test_storage_first_resume_requires_exact_completed_prefix_and_next_operation(self):
        contract = self._build()
        validator = self._resume_validator()
        operations = list(self.module.STORAGE_FIRST_MUTATING_OPERATIONS)
        cases = {
            "reordered-prefix": ([operations[1], operations[0]], operations[2]),
            "skipped-prefix": ([operations[0], operations[2]], operations[3]),
            "next-operation-drift": ([operations[0]], operations[4]),
            "complete-prefix-with-operation-next": (operations, operations[-1]),
        }
        for label, (completed, next_operation) in cases.items():
            changed = copy.deepcopy(contract["journal_template"])
            changed["completed_operations"] = completed
            changed["next_operation"] = next_operation
            with self.subTest(case=label), self.assertRaises(Exception):
                validator(contract, changed)

    def test_storage_first_resume_status_must_match_cursor(self):
        contract = self._build()
        validator = self._resume_validator()
        operations = list(self.module.STORAGE_FIRST_MUTATING_OPERATIONS)
        cases = {
            "verified-empty": ("storage-205-verified", [], operations[0]),
            "prepared-complete": ("prepared", operations, "reconciliation-required"),
            "preflight-after-mutation": ("preflight-complete", operations[:1], operations[1]),
            "running-complete": ("storage-205-running", operations, "reconciliation-required"),
        }
        for label, (status, completed, next_operation) in cases.items():
            changed = copy.deepcopy(contract["journal_template"])
            changed["status"] = status
            changed["completed_operations"] = completed
            changed["next_operation"] = next_operation
            with self.subTest(case=label), self.assertRaises(Exception):
                validator(contract, changed)


class StorageFirstParentSecurityRedTests(unittest.TestCase):
    """Finding-specific RED coverage for QA-SF-001 parent admission.

    These mutations represent parent evidence rebinding that must be rejected
    before a storage-only child contract is projected.  The fixture is entirely
    in-process and contains no signed material or provider access.
    """

    @classmethod
    def setUpClass(cls) -> None:
        cls.module = load_module()

    def _parent_plan(self) -> dict[str, object]:
        fixture = StorageFirstContractRedTests("runTest")
        fixture.module = self.module
        return fixture._parent_plan()

    def test_qa_sf_001_parent_binding_rebinds_fail_closed(self) -> None:
        builder = getattr(self.module, "build_storage_first_contract", None)
        self.assertTrue(callable(builder), "RED: missing storage-first planner API")
        base = self._parent_plan()
        mutations = {
            "known-host-path": lambda plan: plan["nodes"][0]["host_binding"].update(
                {"known_hosts_path": "/operator/rebound-known-hosts"}
            ),
            "known-host-digest": lambda plan: plan.update({"known_hosts_digest": "x" * 64}),
            "missing-ledger-path": lambda plan: plan["credential_nonce_ledger"].pop("path"),
            "impact-digest": lambda plan: plan["consumer_impact_record"].update({"sha256": "x" * 64}),
            "plan-digest": lambda plan: plan.update({"plan_digest": "x" * 64}),
        }
        for mutation, apply in mutations.items():
            changed = copy.deepcopy(base)
            apply(changed)
            with self.subTest(mutation=mutation), self.assertRaises(Exception):
                builder(changed)


if __name__ == "__main__":
    unittest.main()
