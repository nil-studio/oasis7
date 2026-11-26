#!/usr/bin/env python3
"""Shared fixture and helper base class for full-network clean-room plan tests."""

from __future__ import annotations

import atexit
import contextlib
import copy
import hashlib
import importlib.machinery
import importlib.util
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


class FullNetworkCleanRoomPlanTestBase(unittest.TestCase):
    _baseline_fixture_ready = False
    _baseline_evidence_cache: dict[str, object] | None = None
    _baseline_distinct_evidence_cache: dict[str, object] | None = None
    _baseline_artifacts_map: dict[str, dict[str, Path]] | None = None
    _cached_baseline_verified: dict[str, dict[str, object]] | None = None
    _baseline_evidence_file_digests: tuple[tuple[Path, str], ...] = ()
    _atexit_registered = False
    _in_process_signing_modules: dict[int, tuple[types.ModuleType, types.ModuleType]] = {}

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
