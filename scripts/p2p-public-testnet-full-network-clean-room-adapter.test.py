#!/usr/bin/env python3
"""TDD contract tests for the external full-network clean-room adapter.

These tests deliberately use the planner's synthetic, authenticated fixture.  No
provider transport is configured and no credential is read.  The adapter is
expected to remain a dry-run boundary until a separately governed transport is
supplied.
"""

from __future__ import annotations

import copy
import hashlib
import importlib.machinery
import importlib.util
import json
import os
from collections.abc import Mapping
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import types
import unittest
from contextlib import ExitStack
from datetime import datetime, timezone
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
PLANNER_PATH = ROOT / "scripts" / "p2p-public-testnet-full-network-clean-room.py"
PLANNER_TEST_PATH = ROOT / "scripts" / "p2p-public-testnet-full-network-clean-room.test.py"
ADAPTER_PATH = ROOT / "scripts" / "p2p-public-testnet-full-network-clean-room-adapter.py"
PROVENANCE_PATH = ROOT / "scripts" / "p2p-public-testnet-validator-pair-provenance.py"
STORAGE_FIRST_CHILD_OPERATIONS = [
    "stop:storage-205",
    "delete:storage-205",
    "rebuild:storage-205",
    "start:storage-205",
    "verify:storage-205",
]
_COMPILED_MODULE_CODE: dict[str, types.CodeType] = {}
_SHARED_PLANNER_FIXTURE_MODULE: types.ModuleType | None = None


def load_module(name: str, path: Path):
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


_DEFAULT_LOAD_MODULE = load_module


def _shared_planner_fixture_module() -> types.ModuleType:
    global _SHARED_PLANNER_FIXTURE_MODULE
    if _SHARED_PLANNER_FIXTURE_MODULE is None:
        fixture_module = _DEFAULT_LOAD_MODULE("shared_full_network_clean_room_fixture", PLANNER_TEST_PATH)
        fixture_module.FullNetworkCleanRoomPlanTests.setUpClass()
        _SHARED_PLANNER_FIXTURE_MODULE = fixture_module
    else:
        _SHARED_PLANNER_FIXTURE_MODULE.FullNetworkCleanRoomPlanTests.setUpClass()
    return _SHARED_PLANNER_FIXTURE_MODULE


class FakeTransport:
    def __init__(self) -> None:
        self.mutations: list[str] = []

    def mutate(self, operation: str, node: dict[str, object] | None) -> None:
        self.mutations.append(operation)
        raise AssertionError("dry-run must not call provider mutation")


class ApplyTransport:
    def __init__(
        self,
        adapter,
        plan: dict[str, object],
        *,
        invalid_operation: str | None = None,
        invalid_signature: bool = False,
        peer_mismatch: bool = False,
        rollback_failure: bool = False,
        side_effect_operation: str | None = None,
    ) -> None:
        self.adapter = adapter
        self.plan = plan
        self.invalid_operation = invalid_operation
        self.invalid_signature = invalid_signature
        self.peer_mismatch = peer_mismatch
        self.rollback_failure = rollback_failure
        self.side_effect_operation = side_effect_operation
        self.operations: list[str] = []
        self.rollback_operations: list[str] = []
        self.rollback_reobservations: list[str] = []
        self.rollback_started: list[str] = []
        self.failed_operation: str | None = None

    def inspect_node(self, node: dict[str, object]) -> dict[str, object]:
        name = node["name"]
        original = next(item for item in self.plan["nodes"] if item["name"] == name)
        required_bytes, required_inodes = self.adapter.capacity_requirement(self.plan, original)
        binding = original["host_binding"]
        evidence = {
            "node": name,
            "node_id": original["node_id"],
            "provider_uid": self.adapter.CANONICAL_PROVIDER_UID[name],
            "node_root": original["node_root"],
            "persistent_state_paths": list(original["persistent_state_paths"]),
            "symlink_free": True,
            "free_bytes": required_bytes,
            "required_bytes": required_bytes,
            "free_inodes": required_inodes,
            "required_inodes": required_inodes,
            "host_target": binding["target"],
            "known_hosts_path": binding["known_hosts_path"],
            "known_host_fingerprint": binding["known_host_fingerprint"],
            "known_hosts_regular": True,
            "known_hosts_owner_uid": os.getuid(),
            "known_hosts_mode": "0600",
        }
        evidence["receipt"] = self._receipt(f"preflight:{name}", original, evidence=evidence)
        return evidence

    def _receipt(
        self,
        operation: str,
        node: dict[str, object] | None,
        *,
        evidence: dict[str, object] | None = None,
    ) -> dict[str, object]:
        node_name = node["name"] if node is not None else None
        peer_id = (
            self.adapter.CANONICAL_PEER_REGISTRY[node_name]
            if node_name is not None
            else ("validator-pair" if operation == "fresh-root-probe" else "fleet")
        )
        if self.peer_mismatch and node_name is not None:
            peer_id = "12D3KooWattacker"
        bindings = {
            "task_uid": self.plan["task_uid"],
            "frozen_head_oid": self.plan["head_oid"],
            "plan_digest": self.plan["plan_digest"],
            "transaction_id": self.plan["transaction_id"],
            "capture_window_id": self.plan["capture_window_id"],
            "operation": operation,
            "node": node_name,
            "peer_id": peer_id,
            "ledger_path": self.plan["credential_nonce_ledger"]["path"],
            "consumer_impact_record": {
                "path": self.plan["consumer_impact_record"]["path"],
                "sha256": self.plan["consumer_impact_record"]["sha256"],
            },
        }
        if evidence is not None:
            bindings["evidence_sha256"] = self.adapter._remote_evidence_digest(evidence)
        receipt: dict[str, object] = {
            "schema_version": self.adapter.PHASE_RECEIPT_SCHEMAS[
                self.adapter._receipt_phase(operation)
            ],
            "authenticated": True,
            "verified": True,
            "signer_id": "governance-signer",
            "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
            "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
            "signed_payload_sha256": "a" * 64,
            "signature_hex": "b" * 128,
            "canonical_digest": "c" * 64,
            "transaction_id": self.plan["transaction_id"],
            "capture_window_id": self.plan["capture_window_id"],
            "operation": operation,
            "node": node_name,
            "peer_id": peer_id,
            "bindings": bindings,
            "phase": self.adapter._receipt_phase(operation),
            "captured_at": "2026-09-01T00:00:00Z",
            "replayed": False,
            "observer_mutation": False,
            "status": (
                "completed"
                if self.adapter._receipt_phase(operation)
                in {"backup", "apply", "rollback", "reobserve"}
                else "verified"
            ),
        }
        if self.adapter._receipt_phase(operation) in {"backup", "apply"}:
            receipt["seed_eligible"] = False
            receipt["backup_manifest"] = {
                "node": node_name,
                "sha256": "d" * 64,
                "size_bytes": 256,
                "verified": True,
                "seed_eligible": False,
            }
        if self.invalid_signature and operation == "preflight:storage-205":
            receipt["signature_hex"] = "0" * 128
        if operation == "fresh-root-probe":
            checkpoint = self.plan["truth"]["checkpoint"]
            receipt["replayed"] = False
            receipt["checkpoint_manifest_hash"] = checkpoint["manifest_hash"]
            receipt["checkpoint_id"] = checkpoint["checkpoint_id"]
            receipt["height"] = checkpoint["height"]
            receipt["package_commit"] = self.plan["truth"]["package"]["commit"]
            receipt["execution_block_hash"] = checkpoint["execution_block_hash"]
            receipt["execution_state_root"] = checkpoint["execution_state_root"]
            receipt["blob_closure"] = copy.deepcopy(self.plan["truth"]["execution"])
            receipt["runtime"] = {
                "sha256": self.plan["truth"]["package"]["runtime_sha256"],
                "size_bytes": self.plan["truth"]["package"]["runtime_size_bytes"],
            }
            receipt["connected_provider"] = {
                "verified": True,
                "providers": [
                    {
                        "node": name,
                        "node_id": next(item for item in self.plan["nodes"] if item["name"] == name)["node_id"],
                        "peer_id": self.adapter.CANONICAL_PEER_REGISTRY[name],
                        "provider_uid": self.adapter.CANONICAL_PROVIDER_UID[name],
                    }
                    for name in ("storage-205", "sequencer-204")
                ],
            }
            receipt["recovery_receipt"] = {
                "schema_version": "oasis7.recovery_receipt.v1",
                "authenticated": True,
                "verified": True,
                "signer_id": "governance-signer",
                "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
                "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
                "signed_payload_sha256": "a" * 64,
                "signature_hex": "b" * 128,
                "canonical_digest": "c" * 64,
                "bindings": {
                    "task_uid": self.plan["task_uid"],
                    "transaction_id": self.plan["transaction_id"],
                    "capture_window_id": self.plan["capture_window_id"],
                    "checkpoint_id": checkpoint["checkpoint_id"],
                    "checkpoint_manifest_hash": checkpoint["manifest_hash"],
                },
            }
        if operation in {"reobserve-failed-state", "rollback-clean-redeploy"}:
            receipt["failed_operation"] = self.failed_operation or "stop:storage-205"
            receipt["failed_state_digest"] = "d" * 64
            receipt["rollback_steps"] = list(self.plan["rollback"]["steps"])
            receipt["reobserved"] = True
        if operation == "fleet-health":
            validator_peers = [
                next(
                    item["identity_receipt"]["peer_id"]
                    for item in self.plan["nodes"]
                    if item["name"] == name
                )
                for name in ("storage-205", "sequencer-204")
            ]
            receipt["fleet_health_closure"] = {
                "verified": True,
                "nodes": list(self.plan["node_order"]),
                "healthy": True,
                "snapshot": {
                    name: {
                        "running": True,
                        "last_error": None,
                        "committed_height": 100,
                        "network_committed_height": 100,
                        "last_execution_height": 100,
                        "connected_peers": validator_peers,
                        "readiness": {"ready": True, "failed_gates": []},
                        "consensus": {"network_head": {"decision": "ready"}},
                    }
                    for name in self.plan["node_order"]
                },
            }
        return receipt

    def verify_fresh_root_probe(self, plan: dict[str, object]) -> dict[str, object]:
        self.operations.append("fresh-root-probe")
        return self._receipt("fresh-root-probe", None)

    def fetch_sequencer_proof(self, operation: str, node: dict[str, object]) -> dict[str, object]:
        """Return a fresh, bounded, signed proof for storage-first tests."""
        self.operations.append(operation)
        return self._receipt("preflight:sequencer-204", node)

    def preflight(self, operation: str, node: dict[str, object] | None) -> dict[str, object]:
        self.operations.append(operation)
        if operation == self.invalid_operation:
            self.failed_operation = operation
            return {"schema_version": "caller-owned.invalid"}
        return self._receipt(operation, node)

    def verify(self, operation: str, node: dict[str, object] | None) -> dict[str, object]:
        self.operations.append(operation)
        if operation == self.invalid_operation:
            self.failed_operation = operation
            return {"schema_version": "caller-owned.invalid"}
        return self._receipt(operation, node)

    def health(self, operation: str) -> dict[str, object]:
        self.operations.append(operation)
        if operation == self.invalid_operation:
            self.failed_operation = operation
            return {"schema_version": "caller-owned.invalid"}
        return self._receipt(operation, None)

    def mutate(self, operation: str, node: dict[str, object] | None) -> dict[str, object]:
        self.operations.append(operation)
        if operation == self.side_effect_operation:
            self.failed_operation = operation
            raise RuntimeError("provider side effect then throw")
        if operation == self.invalid_operation:
            self.failed_operation = operation
            return {"schema_version": "caller-owned.invalid"}
        return self._receipt(operation, node)

    def reobserve_failed_state(
        self, plan: dict[str, object], started: list[str], failed_operation: str
    ) -> dict[str, object]:
        self.rollback_reobservations.append(failed_operation)
        self.failed_operation = failed_operation
        receipt = self._receipt("reobserve-failed-state", None)
        receipt["bindings"]["rollback_candidates"] = list(started)
        return receipt

    def rollback_clean_redeploy(
        self,
        plan: dict[str, object],
        started: list[str],
        failed_state: dict[str, object] | None = None,
    ) -> dict[str, object]:
        self.rollback_operations.append("rollback")
        self.rollback_started = list(started)
        if self.rollback_failure:
            raise RuntimeError("rollback transport unavailable")
        receipt = self._receipt("rollback-clean-redeploy", None)
        receipt["bindings"]["rollback_candidates"] = list(started)
        return receipt


class StorageFirstCanonicalTransport(ApplyTransport):
    """Canonical parent-backed transport double for the child phase tests."""

    def _storage_node(self) -> dict[str, object]:
        assert self.plan is not None
        return next(node for node in self.plan["nodes"] if node["name"] == "storage-205")

    def __init__(self, adapter, plan, **kwargs):
        super().__init__(adapter, plan, **kwargs)
        self.mutations: list[str] = []
        self.verify_operations: list[str] = []

    def inspect_node(self, node: dict[str, object]) -> dict[str, object]:
        evidence = super().inspect_node(node)
        evidence["known_hosts_verified"] = True
        return evidence

    def mutate(self, operation: str, node: dict[str, object] | None) -> dict[str, object]:
        self.mutations.append(operation)
        try:
            return super().mutate(operation, node)
        except Exception:
            self.mutations.pop()
            raise

    def verify(self, operation: str, node: dict[str, object] | None) -> dict[str, object]:
        self.verify_operations.append(operation)
        return super().verify(operation, node)

    def reobserve_failed_state(
        self, plan: dict[str, object], started: list[str], failed_operation: str
    ) -> dict[str, object]:
        self.rollback_reobservations.append(failed_operation)
        self.failed_operation = failed_operation
        receipt = self._receipt("reobserve-failed-state", self._storage_node())
        receipt["bindings"]["rollback_candidates"] = list(started)
        receipt["rollback_steps"] = list(plan["rollback"]["steps"])
        return receipt

    def rollback_clean_redeploy(
        self,
        plan: dict[str, object],
        started: list[str],
        failed_state: dict[str, object] | None = None,
    ) -> dict[str, object]:
        self.rollback_operations.append("rollback")
        receipt = self._receipt("rollback-clean-redeploy", self._storage_node())
        receipt["bindings"]["rollback_candidates"] = list(started)
        receipt["rollback_steps"] = list(plan["rollback"]["steps"])
        return receipt


class StorageFirstInspectEvidence(dict):
    """Expose the child callback marker without widening canonical evidence."""

    def get(self, key, default=None):
        if key == "known_hosts_verified" and key not in self:
            return True
        return super().get(key, default)


class StorageFirstConsumerImpact(dict):
    """Expose child proceed status while preserving the signed locator shape."""

    def get(self, key, default=None):
        if key == "decision" and key not in self:
            return "proceed"
        return super().get(key, default)


class StorageFirstCurrentAdmissionEvidence(dict):
    """Project the child admission mode without changing signed parent bytes."""

    def __init__(self, value):
        super().__init__(value)
        self._digest = hashlib.sha256(
            json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest()

    def get(self, key, default=None):
        if key == "mode" and key not in self:
            return "current_admission"
        if key == "digest" and key not in self:
            return self._digest
        return super().get(key, default)


def _plain_storage_first_admission_evidence(
    evidence: Mapping[str, object],
) -> dict[str, object]:
    """Materialize the current-admission envelope as an exact plain dict."""
    projected = StorageFirstCurrentAdmissionEvidence(evidence)
    materialized = copy.deepcopy(dict(evidence))
    for key in ("mode", "digest"):
        materialized[key] = copy.deepcopy(projected.get(key))
    return materialized


class StorageFirstCanonicalPlan(dict):
    """Expose the child backup scope as a derived, non-mutating projection."""

    def get(self, key, default=None):
        value = super().get(key, default)
        if key in {
            "known_hosts_digest",
            "package_provenance_digest",
            "deployment_inventory_digest",
        } and value is None:
            return self._derived_digest(key)
        if key == "independent_verifier" and value is None:
            return {
                "verifier_id": "governed-receipt-verifier",
                "trust_root_id": "oasis7-public-testnet-governance-root-v1",
            }
        if key == "consumer_impact_record" and isinstance(value, dict):
            return StorageFirstConsumerImpact(value)
        if key == "credential_nonce_ledger" and isinstance(value, dict):
            projected = copy.deepcopy(value)
            projected["count"] = len(self["node_order"])
            projected["reservations"] = [
                {"node": node["name"]} for node in self["nodes"]
            ]
            return projected
        if key != "forensic_backup" or not isinstance(value, dict):
            return value
        projected = copy.deepcopy(value)
        if projected.get("repository") is None:
            projected["repository"] = "eng-cc/oasis7"
        if projected.get("action") is None:
            projected["action"] = "full-network-clean-room"
        if projected.get("targets") is None:
            projected["targets"] = list(self["node_order"])
        return projected

    def _derived_digest(self, key):
        value = super().get(key)
        if value is not None:
            return value
        source = {
            "known_hosts_digest": self.get("canonical_host_inventory"),
            "package_provenance_digest": self.get("truth", {}).get("package"),
            "deployment_inventory_digest": self.get("deployment_inventory"),
        }[key]
        return hashlib.sha256(
            json.dumps(source, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest()


class StorageFirstCanonicalChildPlan(StorageFirstCanonicalPlan):
    """Expose the signed parent plan through the storage-child rollback scope."""

    def __getitem__(self, key):
        if key == "global_order":
            return list(STORAGE_FIRST_CHILD_OPERATIONS)
        return super().__getitem__(key)

    def get(self, key, default=None):
        if key == "global_order":
            return list(STORAGE_FIRST_CHILD_OPERATIONS)
        return super().get(key, default)


class StorageFirstCanonicalAuthority(dict):
    """Project the signed parent authority into the child phase envelope."""

    def __init__(self, value, plan):
        super().__init__(value)
        self._phase = {
            "action": "storage-205-first",
            "targets": ["storage-205"],
            "task_uid": plan["task_uid"],
            "frozen_head_oid": plan["head_oid"],
            "plan_digest": plan["plan_digest"],
            "transaction_id": plan["transaction_id"],
            "capture_window_id": plan["capture_window_id"],
            "current_authorization": True,
            "signed": True,
            "expires_at": "2099-01-01T00:00:00Z",
        }

    def get(self, key, default=None):
        return self._phase.get(key, super().get(key, default))


class ReceivedPlanOnlyTransport:
    """Exercise rollback callbacks without retaining the planner's full plan."""

    def __init__(self) -> None:
        self.received_policy: dict[str, object] | None = None

    def _rollback_receipt(self, plan: dict[str, object]) -> dict[str, object]:
        self.received_policy = {
            "forensic_backup": copy.deepcopy(plan["forensic_backup"]),
            "rollback": copy.deepcopy(plan["rollback"]),
        }
        return {"rollback_steps": list(plan["rollback"]["steps"])}

    def reobserve_failed_state(
        self, plan: dict[str, object], started: list[str], failed_operation: str
    ) -> dict[str, object]:
        return self._rollback_receipt(plan)

    def rollback_clean_redeploy(
        self,
        plan: dict[str, object],
        started: list[str],
        failed_state: dict[str, object] | None = None,
    ) -> dict[str, object]:
        return self._rollback_receipt(plan)


class FullNetworkCleanRoomAdapterTests(unittest.TestCase):
    def test_checkpoint_and_nested_payload_mutations_reject_before_effects(self):
        adapter = self.adapter
        root = Path(self._test_directory.name)
        control_transport = ApplyTransport(adapter, self.plan)
        result = adapter.execute(self.plan, self._authority(True), journal_path=root / "valid-control.json",
                                 ledger_path=self.ledger_path, transport=control_transport, dry_run=False,
                                 provenance_verifier=self._recovery_verifier)
        self.assertEqual(result["status"], "complete")
        self.assertTrue(control_transport.operations)
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
                self._write_ledger(self.ledger_path)
                ledger_before = self.ledger_path.read_bytes()
                changed = copy.deepcopy(self.plan)
                target = (changed["truth"]["checkpoint"] if section == "checkpoint" else
                          changed["fresh_root_probe"]["validator_verify_outputs"]["storage-205"])
                target[field] = value
                self.assertEqual(changed["truth"]["checkpoint"]["receipt"], self.plan["truth"]["checkpoint"]["receipt"])
                self.assertEqual(changed["fresh_root_probe"]["receipt"], self.plan["fresh_root_probe"]["receipt"])
                for key in ("signed_payload_sha256", "signature_hex", "canonical_digest"):
                    self.assertEqual(changed["fresh_root_probe"]["validator_verify_outputs"]["storage-205"][key],
                                     self.plan["fresh_root_probe"]["validator_verify_outputs"]["storage-205"][key])
                # A caller can rehash a plan; its digest is not receipt authority.
                changed["plan_digest"] = adapter.canonical_plan_digest(changed)
                try:
                    adapter.validate_plan(changed)
                except adapter.AdapterError:
                    admission_rejected = True
                else:
                    admission_rejected = False
                transport = ApplyTransport(adapter, changed)
                journal = root / f"{label}.json"
                with mock.patch.object(adapter, "_write_journal", wraps=adapter._write_journal) as write, \
                     mock.patch.object(adapter, "reserve_nonce", wraps=adapter.reserve_nonce) as reserve, \
                     mock.patch.object(transport, "inspect_node", wraps=transport.inspect_node) as inspect:
                    try:
                        adapter.execute(changed, self._authority(True, changed), journal_path=journal,
                                        ledger_path=self.ledger_path, transport=transport, dry_run=False,
                                        provenance_verifier=self._recovery_verifier)
                    except adapter.AdapterError:
                        execute_rejected = True
                    else:
                        execute_rejected = False
                self.assertEqual((admission_rejected, execute_rejected, write.call_count, reserve.call_count,
                                  inspect.call_count, len(transport.operations), journal.exists(),
                                  self.ledger_path.read_bytes() == ledger_before),
                                 (True, True, 0, 0, 0, 0, False, True),
                                 "receipt-integrity rejection must precede nonce, callback and journal effects")

    def test_plan_verifier_covers_truth_probe_and_nested_receipts(self):
        seen = []
        def verifier(plan, receipt):
            seen.append(receipt["schema_version"])
            return self._recovery_verifier(plan, receipt)
        self.adapter._verify_plan_receipts_with_verifier(self.plan, verifier)
        required = {self.plan["truth"][section]["receipt"]["schema_version"]
                    for section in ("package", "genesis", "world", "checkpoint")}
        required.add(self.plan["fresh_root_probe"]["receipt"]["schema_version"])
        required.add(self.plan["fresh_root_probe"]["validator_verify_outputs"]["storage-205"]["schema_version"])
        self.assertTrue(required.issubset(set(seen)), f"unverified truth/probe receipt schemas: {sorted(required - set(seen))}")
        self.assertEqual(seen.count(self.plan["fresh_root_probe"]["validator_verify_outputs"]["storage-205"]["schema_version"]), 2)

    def _assert_live_root_callback_boundary(self, boundary):
        adapter = self.adapter
        original_validator = self.live_trust_root_patcher.temp_original
        root = Path(self._test_directory.name) / "live-governance-root.json"
        for drift in (False, True):
            with self.subTest(boundary=boundary, drift=drift):
                self._write_ledger(self.ledger_path)
                root.write_bytes(adapter.CANONICAL_TRUST_ROOT_FIXTURE_PATH.read_bytes())
                root.chmod(0o600)
                triggered = []
                checks = []
                def check_root():
                    checks.append(True)
                    with mock.patch.object(adapter, "CANONICAL_TRUST_ROOT_PATH", str(root)):
                        return original_validator()
                def change_root():
                    if not triggered:
                        triggered.append(boundary)
                        if drift:
                            root.write_bytes(b"replaced authority")
                recovery = boundary in {"recovery-verifier", "reobserve"}
                failed = "stop:storage-205"
                transport = ApplyTransport(adapter, self.plan,
                    side_effect_operation=failed if recovery else None)
                inspect = transport.inspect_node
                mutate = transport.mutate
                reobserve = transport.reobserve_failed_state
                def inspected(node):
                    receipt = inspect(node)
                    if boundary == "preflight": change_root()
                    return receipt
                def mutated(operation, node):
                    receipt = mutate(operation, node)
                    if boundary == "prior-mutation" and operation == failed: change_root()
                    return receipt
                def reobserved(*args):
                    receipt = reobserve(*args)
                    if boundary == "reobserve": change_root()
                    return receipt
                def verifier(plan, receipt):
                    result = self._recovery_verifier(plan, receipt)
                    if boundary == "recovery-verifier" and transport.failed_operation == failed:
                        change_root()
                    return result
                transport.inspect_node = inspected
                transport.mutate = mutated
                transport.reobserve_failed_state = reobserved
                journal = Path(self._test_directory.name) / f"root-{boundary}-{drift}.json"
                error = None
                with mock.patch.object(adapter, "validate_live_trust_root_file", side_effect=check_root):
                    try:
                        result = adapter.execute(self.plan, self._authority(True), journal_path=journal,
                            ledger_path=self.ledger_path, transport=transport, dry_run=False,
                            provenance_verifier=verifier)
                    except adapter.AdapterError as caught:
                        error = caught
                self.assertEqual(triggered, [boundary])
                self.assertTrue(checks)
                if not drift:
                    if recovery:
                        self.assertIsNotNone(error)
                        self.assertEqual(transport.rollback_operations, ["rollback"])
                    else:
                        self.assertIsNone(error)
                        self.assertEqual(result["status"], "complete")
                    continue
                destructive = [op for op in transport.operations if adapter._rollback_candidate(op)]
                self.assertEqual(destructive, [] if boundary == "preflight" else [failed],
                                 "live root drift must stop further fleet mutation")
                self.assertEqual(transport.rollback_reobservations,
                                 [failed] if boundary == "reobserve" else [])
                self.assertEqual(transport.rollback_operations, [])
                self.assertIsNotNone(error)
                if recovery or boundary == "prior-mutation":
                    record = json.loads(journal.read_text())
                    self.assertEqual(record["rollback_status"], "reconciliation-blocked")
                    self.assertEqual(record["rollback_candidates"], [failed])
                    self.assertIsNone(record["rollback_receipt"])

    def test_live_root_drift_during_preflight_blocks_mutation(self):
        self._assert_live_root_callback_boundary("preflight")

    def test_live_root_drift_after_mutation_blocks_next_and_recovery(self):
        self._assert_live_root_callback_boundary("prior-mutation")

    def test_live_root_drift_during_recovery_verifier_blocks_reobserve(self):
        self._assert_live_root_callback_boundary("recovery-verifier")

    def test_live_root_drift_during_reobserve_blocks_redeploy(self):
        self._assert_live_root_callback_boundary("reobserve")

    def _assert_same_fleet_transactions_serialized(self, resume):
        adapter = self.adapter
        first = self.plan
        replacements = {first["transaction_id"]: "txn-independent-second"}
        replacements.update({value: value + "-second" for value in first["credential_nonce_ledger"]["reserved_nonces"]})
        def rebind(value):
            if isinstance(value, dict): return {key: rebind(item) for key, item in value.items()}
            if isinstance(value, list): return [rebind(item) for item in value]
            return replacements.get(value, value) if isinstance(value, str) else value
        second = rebind(first)
        self.fixture._sign_semantic_fixture(second)
        second["plan_digest"] = adapter.canonical_plan_digest(second)
        authorities = [self._authority(True, plan) for plan in (first, second)]
        # Both independently bound admissions must be valid before contention.
        for plan, authority in zip((first, second), authorities):
            adapter.validate_authority(plan, authority)
        self.assertNotEqual(first["transaction_id"], second["transaction_id"])
        self.assertTrue(set(first["credential_nonce_ledger"]["reserved_nonces"]).isdisjoint(
            second["credential_nonce_ledger"]["reserved_nonces"]))
        root = Path(self._test_directory.name)
        journals = [root / "fleet-first.json", root / "fleet-second.json"]
        control = adapter.execute(second, authorities[1], journal_path=root / "independent-control.json",
            ledger_path=self.ledger_path, transport=ApplyTransport(adapter, second), dry_run=False,
            provenance_verifier=self._recovery_verifier)
        self.assertEqual(control["status"], "complete")
        # This is an isolated synthetic ledger, reset between independent cases.
        self._write_ledger(self.ledger_path)
        if resume:
            original_write = adapter._write_journal
            def prepared(path, record):
                original_write(path, record)
                if record["status"] == "prepared": raise KeyboardInterrupt
            with mock.patch.object(adapter, "_write_journal", side_effect=prepared):
                with self.assertRaises(KeyboardInterrupt):
                    adapter.execute(second, authorities[1], journal_path=journals[1],
                        ledger_path=self.ledger_path, transport=ApplyTransport(adapter, second),
                        dry_run=False, provenance_verifier=self._recovery_verifier)
        entered, release = threading.Event(), threading.Event()
        transport = ApplyTransport(adapter, first)
        original_mutate = transport.mutate
        def hold(operation, node):
            if operation == "stop:storage-205":
                entered.set()
                if not release.wait(30): raise RuntimeError("test callback barrier timed out")
            return original_mutate(operation, node)
        transport.mutate = hold
        first_results = []
        def run_first():
            try:
                first_results.append(adapter.execute(first, authorities[0], journal_path=journals[0],
                    ledger_path=self.ledger_path, transport=transport, dry_run=False,
                    provenance_verifier=self._recovery_verifier))
            except BaseException as error: first_results.append(error)
        worker = threading.Thread(target=run_first)
        worker.start()
        intrusions = []
        second_transport = ApplyTransport(adapter, second)
        def forbidden_inspect(node):
            intrusions.append(node["name"])
            raise KeyboardInterrupt("second transaction reached provider while fleet busy")
        second_transport.inspect_node = forbidden_inspect
        def invoke(transport):
            if resume:
                return adapter.resume_transaction(second, authorities[1], journals[1],
                    ledger_path=self.ledger_path, transport=transport, dry_run=False,
                    provenance_verifier=self._recovery_verifier)
            return adapter.execute(second, authorities[1], journal_path=journals[1],
                ledger_path=self.ledger_path, transport=transport, dry_run=False,
                provenance_verifier=self._recovery_verifier)
        contention_error = None
        try:
            self.assertTrue(entered.wait(30), first_results)
            ledger_before = self.ledger_path.read_bytes()
            try: invoke(second_transport)
            except BaseException as error: contention_error = error
            ledger_after = self.ledger_path.read_bytes()
        finally:
            release.set()
            worker.join(30)
        self.assertFalse(worker.is_alive())
        self.assertEqual(len(first_results), 1)
        self.assertIsInstance(first_results[0], dict, first_results)
        self.assertEqual(first_results[0]["status"], "complete")
        self.assertEqual(intrusions, [], "different journals cannot bypass fleet serialization")
        self.assertIsInstance(contention_error, adapter.AdapterError)
        self.assertRegex(str(contention_error), "(?i)lock|busy|fleet")
        self.assertEqual(ledger_before, ledger_after, "contender must not consume nonces")
        # The rejected independent plan must remain usable after release.
        result = invoke(ApplyTransport(adapter, second))
        self.assertEqual(result["status"], "complete")

    def test_same_fleet_distinct_execute_transactions_are_serialized(self):
        self._assert_same_fleet_transactions_serialized(False)

    def test_same_fleet_distinct_resume_transactions_are_serialized(self):
        self._assert_same_fleet_transactions_serialized(True)

    _owns_isolated_fixture_module = False

    @classmethod
    def setUpClass(cls):
        if load_module is _DEFAULT_LOAD_MODULE:
            cls.fixture_module = _shared_planner_fixture_module()
            cls._owns_isolated_fixture_module = False
        else:
            cls.fixture_module = load_module("full_network_clean_room_fixture", PLANNER_TEST_PATH)
            cls.fixture_module.FullNetworkCleanRoomPlanTests.setUpClass()
            cls._owns_isolated_fixture_module = True

    @classmethod
    def tearDownClass(cls):
        if cls._owns_isolated_fixture_module:
            cls.fixture_module.FullNetworkCleanRoomPlanTests.tearDownClass()
            cls._owns_isolated_fixture_module = False

    def _recovery_verifier(self, plan, receipt):
        return {"verified": True, "bindings": receipt["bindings"],
                "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
                "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
                "signer_id": "governance-signer"}

    def _assert_recovery_expiry_boundary(self, boundary):
        failed = "stop:storage-205"
        original_plan = self.plan
        real_datetime = self.adapter.dt.datetime
        # Building the no-backup fixture refreshes its on-disk impact record;
        # consume the original forensic plan before that fixture transition.
        for mode in ("valid-window", "capture-expired", "no-backup-expired"):
            with self.subTest(boundary=boundary, mode=mode):
                self.plan = self._no_backup_plan() if mode == "no-backup-expired" else original_plan
                self._write_ledger(self.ledger_path)
                plan = self.plan
                end = real_datetime.fromisoformat(
                    (plan["forensic_backup"]["expires_at"] if mode == "no-backup-expired"
                     else plan["capture_window"]["ends_at"]).replace("Z", "+00:00")
                )
                clock = [real_datetime(2026, 9, 11, tzinfo=timezone.utc)]
                expired = mode != "valid-window"
                triggered = []

                class RecoveryClock(real_datetime):
                    @classmethod
                    def now(cls, tz=None):
                        return clock[0] if tz is not None else clock[0].replace(tzinfo=None)

                def advance():
                    triggered.append(boundary)
                    if expired:
                        clock[0] = end

                transport = ApplyTransport(self.adapter, plan, side_effect_operation=failed)
                original_reobserve = transport.reobserve_failed_state
                observed_receipts = []

                def reobserve(*args):
                    receipt = original_reobserve(*args)
                    observed_receipts.append(copy.deepcopy(receipt))
                    if boundary == "reobserve":
                        advance()
                    return receipt

                def verifier(transport_plan, receipt):
                    result = self._recovery_verifier(transport_plan, receipt)
                    if boundary == "provenance" and transport.failed_operation == failed and not triggered:
                        advance()
                    elif boundary == "receipt-verifier" and receipt.get("operation") == "reobserve-failed-state":
                        advance()
                    return result

                transport.reobserve_failed_state = reobserve
                journal = Path(self._test_directory.name) / f"expiry-{boundary}-{mode}.json"
                with mock.patch.object(self.adapter.dt, "datetime", RecoveryClock):
                    with self.assertRaises(self.adapter.AdapterError) as failure:
                        self.adapter.execute(plan, self._authority(True), journal_path=journal,
                            ledger_path=self.ledger_path, transport=transport, dry_run=False,
                            provenance_verifier=verifier)
                self.assertTrue(journal.exists(), str(failure.exception))
                record = json.loads(journal.read_text())
                self.assertEqual(triggered, [boundary])
                self.assertEqual(record["rollback_candidates"], [failed])
                self.assertEqual(record["failed_operation"], failed)
                self.assertTrue(record["provider_receipts"], "completed evidence must survive expiry")
                self.assertTrue(record["preflight_evidence_receipts"])
                if observed_receipts:
                    self.assertEqual(record["rollback_reobservation_receipt"], observed_receipts[0])
                self.assertEqual(transport.rollback_reobservations,
                                 [] if expired and boundary == "provenance" else [failed])
                self.assertEqual(transport.rollback_operations, [] if expired else ["rollback"])
                self.assertEqual(record["rollback_status"], "reconciliation-blocked" if expired else "completed")
                if expired:
                    self.assertIsNone(record["rollback_receipt"])
        self.plan = original_plan

    def test_capture_expiry_during_recovery_provenance_blocks_reobserve(self):
        self._assert_recovery_expiry_boundary("provenance")

    def test_capture_expiry_during_reobserve_blocks_clean_redeploy(self):
        self._assert_recovery_expiry_boundary("reobserve")

    def test_capture_expiry_during_reobservation_verifier_blocks_clean_redeploy(self):
        self._assert_recovery_expiry_boundary("receipt-verifier")

    def test_recovery_receipts_bind_exact_attempted_candidates(self):
        failed = "rebuild:storage-205"
        expected = [op for op in self.plan["global_order"][:self.plan["global_order"].index(failed) + 1]
                    if self.adapter._rollback_candidate(op)]
        self.assertGreater(len(expected), 1)
        for phase in ("reobserve", "rollback"):
            for mutation in ("exact", "omitted", "truncated", "reordered", "duplicated", "substituted"):
                with self.subTest(phase=phase, mutation=mutation):
                    self._write_ledger(self.ledger_path)
                    transport = ApplyTransport(self.adapter, self.plan, side_effect_operation=failed)
                    method = "reobserve_failed_state" if phase == "reobserve" else "rollback_clean_redeploy"
                    original = getattr(transport, method)
                    def response(*args):
                        receipt = original(*args)
                        candidates = receipt["bindings"]["rollback_candidates"]
                        if mutation == "omitted":
                            del receipt["bindings"]["rollback_candidates"]
                        elif mutation == "truncated":
                            receipt["bindings"]["rollback_candidates"] = candidates[:-1]
                        elif mutation == "reordered":
                            receipt["bindings"]["rollback_candidates"] = list(reversed(candidates))
                        elif mutation == "duplicated":
                            receipt["bindings"]["rollback_candidates"] = candidates + candidates[:1]
                        elif mutation == "substituted":
                            receipt["bindings"]["rollback_candidates"] = candidates[:-1] + ["start:macos-observer"]
                        return receipt
                    setattr(transport, method, response)
                    journal = Path(self._test_directory.name) / f"scope-{phase}-{mutation}.json"
                    with self.assertRaises(self.adapter.AdapterError):
                        self.adapter.execute(self.plan, self._authority(True), journal_path=journal,
                            ledger_path=self.ledger_path, transport=transport, dry_run=False,
                            provenance_verifier=self._recovery_verifier)
                    record = json.loads(journal.read_text())
                    self.assertEqual(record["rollback_status"], "completed" if mutation == "exact" else "reconciliation-blocked")
                    self.assertEqual(record["rollback_candidates"], expected)
                    if mutation == "exact":
                        for key in ("rollback_receipt", "rollback_reobservation_receipt"):
                            self.assertEqual(record[key]["bindings"]["rollback_candidates"], expected)
                        # Even re-digested local journals cannot omit/alter the
                        # scope while retaining the signed recovery receipts.
                        for candidate_scope in (None, expected[:-1], list(reversed(expected)), expected + expected[:1], expected[:-1] + ["start:macos-observer"]):
                            tampered = copy.deepcopy(record)
                            if candidate_scope is None:
                                tampered.pop("rollback_candidates")
                            else:
                                tampered["rollback_candidates"] = candidate_scope
                            self.adapter._write_journal(journal, tampered)
                            with self.assertRaisesRegex(self.adapter.AdapterError, "rollback candidate"):
                                self.adapter.resume_transaction(self.plan, self._authority(True), journal,
                                    ledger_path=self.ledger_path, dry_run=False,
                                    provenance_verifier=self._recovery_verifier)

    def test_no_backup_expiry_after_side_effect_retains_reconciliation_scope(self):
        plan = self._no_backup_plan()
        failed = "stop:storage-205"
        transport = ApplyTransport(self.adapter, plan)
        original_datetime = self.adapter.dt.datetime
        expired = False
        class Clock(original_datetime):
            @classmethod
            def now(cls, tz=None):
                if expired:
                    return original_datetime.fromisoformat(plan["forensic_backup"]["expires_at"].replace("Z", "+00:00"))
                return original_datetime.now(tz)
        original_mutate = transport.mutate
        def mutate(operation, node):
            nonlocal expired
            receipt = original_mutate(operation, node)
            if operation == failed:
                expired = True
                raise RuntimeError("fixture side effect then expiry")
            return receipt
        transport.mutate = mutate
        journal = Path(self._test_directory.name) / "expired-recovery.json"
        with mock.patch.object(self.adapter.dt, "datetime", Clock):
            with self.assertRaises(self.adapter.AdapterError):
                self.adapter.execute(plan, self._authority(True, plan), journal_path=journal,
                    ledger_path=self.ledger_path, transport=transport, dry_run=False,
                    provenance_verifier=self._recovery_verifier)
        self.assertTrue(expired)
        self.assertEqual(transport.rollback_operations, [])
        self.assertEqual(transport.rollback_reobservations, [])
        record = json.loads(journal.read_text())
        self.assertEqual(record["rollback_status"], "reconciliation-blocked")
        self.assertEqual(record["rollback_candidates"], [failed])
        self.assertIsNone(record["rollback_receipt"])

    def test_capture_lease_expiry_before_stop_blocks_callback(self) -> None:
        authority = self._authority(apply_authorized=True)
        transport = ApplyTransport(self.adapter, self.plan)
        original_write = self.adapter._write_journal
        original_datetime = self.adapter.dt.datetime
        stop_index = self.plan["global_order"].index("stop:storage-205")
        expired = False

        class Clock(original_datetime):
            @classmethod
            def now(cls, tz=None):
                if expired:
                    return original_datetime.fromisoformat(
                        self.plan["capture_window"]["ends_at"].replace("Z", "+00:00")
                    ) + self.adapter.dt.timedelta(seconds=1)
                return original_datetime.now(tz)

        def write(path, record):
            nonlocal expired
            original_write(path, record)
            if record["status"] == "in-flight" and record["next_operation_index"] == stop_index:
                expired = True

        def verifier(plan, receipt):
            return {"verified": True, "bindings": receipt["bindings"],
                    "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
                    "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
                    "signer_id": "governance-signer"}

        with mock.patch.object(self.adapter.dt, "datetime", Clock), mock.patch.object(
            self.adapter, "_write_journal", side_effect=write
        ):
            try:
                self.adapter.execute(self.plan, authority,
                    journal_path=Path(self._test_directory.name) / "expiry.json",
                    ledger_path=self.ledger_path, transport=transport,
                    dry_run=False, provenance_verifier=verifier)
            except self.adapter.AdapterError:
                pass
        self.assertTrue(expired)
        self.assertEqual(transport.operations, self.plan["global_order"][:stop_index])

    def setUp(self) -> None:
        self.planner = load_module("full_network_clean_room", PLANNER_PATH)
        self.adapter = load_module("full_network_clean_room_adapter", ADAPTER_PATH)
        self.fixture = self.fixture_module.FullNetworkCleanRoomPlanTests()
        self.fixture.setUp()
        self.planner._PEER_REGISTRY_MODULE = self.fixture.module._peer_registry_authority()
        self.planner.CANONICAL_PEER_REGISTRY = dict(self.fixture_module.FIXTURE_PEERS)
        self.adapter.CANONICAL_PEER_REGISTRY = dict(self.fixture_module.FIXTURE_PEERS)
        self._planner_anchor_patch = mock.patch.multiple(
            self.planner,
            _independently_verify_identity_v2_entries=mock.Mock(return_value={}),
        )
        self._planner_anchor_patch.start()
        # Adapter admission loads the canonical planner lazily; point that
        # loader at this fixture module. The independent verifier itself is
        # covered by the process-level bridge suite; unit tests stay focused
        # on adapter ordering and side-effect boundaries.
        self.adapter._PLANNER_MODULE = self.planner
        self.planner._SEMANTIC_SIGNING_MODULE = self.fixture.module._semantic_signing_authority()
        self._test_directory = tempfile.TemporaryDirectory()
        self._fleet_lock_patch = mock.patch.object(
            self.adapter, "CANONICAL_FLEET_LOCK_PATH",
            Path(self._test_directory.name) / "governed-fleet.lock", create=True,
        )
        self._fleet_lock_patch.start()
        # The alias fence reads the authority reference closure independently
        # of the mocked cryptographic verifier. Provision real synthetic files.
        authority_root = Path(self._test_directory.name) / "authority"
        authority_root.mkdir(mode=0o700)
        public_key = authority_root / "public-key"
        provider = authority_root / "provider"
        verifier_tool = authority_root / "verifier"
        for path in (public_key, provider, verifier_tool):
            path.write_bytes(b"synthetic authority fixture")
            path.chmod(0o600)
        trust_config = authority_root / "trust.json"
        registry = authority_root / "registry.json"
        trust_config.write_text(json.dumps({"allowlist": [{"public_key_ref": str(public_key)}]}))
        registry.write_text(json.dumps({
            "trust_config_path": str(trust_config),
            "providers": [{"public_key_ref": str(public_key), "adapter_path": str(provider)}],
            "verifier": {"executable_path": str(verifier_tool)},
        }))
        self.planner.IDENTITY_V2_TRUST_CONFIG_PATH = trust_config
        self.planner.IDENTITY_V2_PROVIDER_REGISTRY_PATH = registry
        self.ledger_path = Path(self._test_directory.name) / "nonce.jsonl"
        self._write_ledger(self.ledger_path)
        evidence_root = Path(self._test_directory.name) / "identity-v2-evidence"
        evidence_root.mkdir(mode=0o700)
        evidence, _ = self.fixture._network_binding_evidence_fixture(
            evidence_root,
            context_network_id=self.planner.CANONICAL_NETWORK_ID,
        )
        self.identity_v2_evidence = evidence
        self.plan = self._bind_test_ledger(
            self.planner.build_plan(self.fixture._input(), identity_v2_evidence=evidence)
        )
        self.live_trust_root_patcher = mock.patch.object(
            self.adapter,
            "validate_live_trust_root_file",
            return_value={
                "path": self.adapter.CANONICAL_TRUST_ROOT_PATH,
                "sha256": self.adapter.CANONICAL_TRUST_ROOT_FILE_SHA256,
                "root_digest": self.adapter.CANONICAL_TRUST_ROOT_DIGEST,
                "owner_scope": self.adapter.CANONICAL_TRUST_ROOT_OWNER_SCOPE,
                "owner_uid": os.getuid(),
                "mode": "0600",
                "regular_file": True,
            },
        )
        self.live_trust_root_patcher.start()

    def tearDown(self) -> None:
        self._fleet_lock_patch.stop()
        self.live_trust_root_patcher.stop()
        self._planner_anchor_patch.stop()
        self.adapter._PLANNER_MODULE = None
        self._test_directory.cleanup()
        self.fixture.tearDown()

    def test_journal_cannot_overwrite_retained_context_in_valid_dry_run(self) -> None:
        journal = Path(self.identity_v2_evidence["context"]["path"])
        original = journal.read_bytes()
        with self.assertRaisesRegex(self.adapter.AdapterError, "alias"):
            self.adapter.execute(
                self.plan, self._authority(), journal_path=journal,
                ledger_path=self.ledger_path, dry_run=True,
            )
        self.assertEqual(journal.read_bytes(), original)
        self.assertFalse(Path(f"{journal}.lock").exists())

    def test_identity_v2_evidence_schema_matches_planner_v2(self) -> None:
        self.assertEqual(
            self.adapter.IDENTITY_V2_EVIDENCE_SCHEMA,
            self.planner.IDENTITY_V2_EVIDENCE_SCHEMA,
        )
        self.assertEqual(
            self.adapter.IDENTITY_V2_EVIDENCE_SCHEMA,
            "oasis7.identity_v2_evidence_map.v2",
        )

    def test_transport_plan_passes_forensic_backup_and_rollback_to_received_dto(self) -> None:
        """Rollback transport must consume policy from its received DTO."""
        transport_plan = self.adapter._transport_plan(self.plan)
        transport = ReceivedPlanOnlyTransport()
        receipt = transport.reobserve_failed_state(
            transport_plan, ["stop:storage-205"], "stop:storage-205"
        )
        self.assertIsNotNone(transport.received_policy)
        assert transport.received_policy is not None
        self.assertEqual(transport.received_policy["forensic_backup"], self.plan["forensic_backup"])
        self.assertEqual(transport.received_policy["rollback"], self.plan["rollback"])
        self.assertEqual(receipt["rollback_steps"], self.plan["rollback"]["steps"])
        receipt = transport.rollback_clean_redeploy(
            transport_plan, ["stop:storage-205"], receipt
        )
        self.assertEqual(receipt["rollback_steps"], self.plan["rollback"]["steps"])

    def test_redigested_policy_drift_is_rejected_before_transport(self) -> None:
        """A self-consistent plan digest cannot authorize policy changes."""
        mutations = (
            (
                "rollback steps",
                lambda plan: plan["rollback"].__setitem__(
                    "steps", ["unsafe-provider-operation"]
                ),
            ),
            (
                "rollback unknown field",
                lambda plan: plan["rollback"].__setitem__(
                    "unexpected_policy", {"operator_action": "unsafe"}
                ),
            ),
            (
                "forensic unknown field",
                lambda plan: plan["forensic_backup"].__setitem__(
                    "unexpected_policy", {"operator_action": "unsafe"}
                ),
            ),
        )
        for label, mutate in mutations:
            with self.subTest(policy=label):
                plan = copy.deepcopy(self.plan)
                mutate(plan)
                plan["plan_digest"] = self.adapter.canonical_plan_digest(plan)
                with self.assertRaises(self.adapter.AdapterError) as raised:
                    self.adapter.validate_plan(plan)
                self.assertRegex(
                    str(raised.exception), r"(?i)rollback|forensic|policy|canonical|field"
                )

    def test_authority_accepts_exact_rollback_policy_binding(self) -> None:
        """External apply authority must cover the exact clean-redeploy policy."""
        authority = self._authority()
        self.adapter.validate_authority(self.plan, authority)

    def test_validate_plan_rejects_missing_identity_v2_evidence_map(self) -> None:
        """The adapter must not validate a legacy receipt-only plan."""
        legacy_plan = copy.deepcopy(self.plan)
        legacy_plan.pop("identity_v2_evidence", None)
        legacy_plan["plan_digest"] = self.adapter.canonical_plan_digest(legacy_plan)
        with self.assertRaises(self.adapter.AdapterError) as raised:
            self.adapter.validate_plan(legacy_plan)
        self.assertRegex(str(raised.exception), r"(?i)identity.?v2|evidence|map|admission")

    def test_validate_authority_rejects_missing_identity_v2_evidence_map(self) -> None:
        """Authority validation must fail before any provider boundary on a legacy plan."""
        legacy_plan = copy.deepcopy(self.plan)
        legacy_plan.pop("identity_v2_evidence", None)
        legacy_plan["plan_digest"] = self.adapter.canonical_plan_digest(legacy_plan)
        with self.assertRaises(self.adapter.AdapterError) as raised:
            self.adapter.validate_authority(legacy_plan, self._authority(plan=legacy_plan))
        self.assertRegex(str(raised.exception), r"(?i)identity.?v2|evidence|map|admission")

    def test_execute_and_resume_reject_noncurrent_maps_before_persistence_or_transport(self) -> None:
        """Every mutating entry point must fence absent, stale, or tampered maps."""
        cases = ("missing", "stale", "tampered")
        for case in cases:
            with self.subTest(map_state=case), tempfile.TemporaryDirectory() as directory:
                plan = copy.deepcopy(self.plan)
                evidence = plan.get("identity_v2_evidence")
                self.assertIsInstance(evidence, dict)
                if case == "missing":
                    plan.pop("identity_v2_evidence", None)
                elif case == "stale":
                    assert isinstance(evidence, dict)
                    evidence["task_uid"] = "task-stale-map"
                else:
                    assert isinstance(evidence, dict)
                    evidence["entries"][0]["provider_attestation"]["sha256"] = "0" * 64
                plan["plan_digest"] = self.adapter.canonical_plan_digest(plan)
                authority = self._authority(plan=plan)
                journal = Path(directory) / "journal.jsonl"
                before = self.ledger_path.read_bytes()
                transport = mock.Mock()
                with self.assertRaises(self.adapter.AdapterError):
                    self.adapter.execute(
                        plan,
                        authority,
                        journal_path=journal,
                        ledger_path=self.ledger_path,
                        transport=transport,
                        dry_run=False,
                    )
                self.assertEqual(self.ledger_path.read_bytes(), before)
                self.assertFalse(journal.exists())
                self.assertEqual(transport.mock_calls, [])

                resume_journal = Path(directory) / "resume-journal.jsonl"
                with self.assertRaises(self.adapter.AdapterError):
                    self.adapter.resume_transaction(
                        plan,
                        authority,
                        resume_journal,
                        ledger_path=self.ledger_path,
                        transport=transport,
                        dry_run=False,
                    )
                self.assertEqual(self.ledger_path.read_bytes(), before)
                self.assertFalse(resume_journal.exists())
                self.assertEqual(transport.mock_calls, [])

    def test_cli_apply_requires_current_identity_map_before_execute(self) -> None:
        """CLI apply must not enter execute/provenance without its exact map and mode."""
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plan_path = root / "plan.json"
            authority_path = root / "authority.json"
            journal_path = root / "journal.jsonl"
            ledger_path = root / "ledger.jsonl"
            plan_path.write_text(json.dumps(self.plan, sort_keys=True), encoding="utf-8")
            authority_path.write_text(
                json.dumps(self._authority(plan=self.plan), sort_keys=True), encoding="utf-8"
            )
            before_ledger = self.ledger_path.read_bytes()
            with mock.patch.object(
                self.adapter,
                "execute",
                side_effect=self.adapter.AdapterError("identity-v2 CLI gate missing"),
            ) as execute:
                with self.assertRaises(self.adapter.AdapterError) as raised:
                    self.adapter.main(
                        [
                            "--plan",
                            str(plan_path),
                            "--authority",
                            str(authority_path),
                            "--journal",
                            str(journal_path),
                            "--ledger",
                            str(ledger_path),
                            "--apply",
                        ]
                    )
            self.assertRegex(str(raised.exception), r"(?i)identity.?v2|evidence|map|current|apply")
            execute.assert_not_called()
            self.assertFalse(journal_path.exists())
            self.assertFalse(ledger_path.exists())
            self.assertEqual(self.ledger_path.read_bytes(), before_ledger)

    def test_cli_apply_rejects_mismatched_or_historical_map_before_execute(self) -> None:
        """CLI apply must fence a non-retained or audit-only map before execution."""
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plan_path = root / "plan.json"
            authority_path = root / "authority.json"
            plan_path.write_text(json.dumps(self.plan, sort_keys=True), encoding="utf-8")
            authority_path.write_text(
                json.dumps(self._authority(plan=self.plan), sort_keys=True), encoding="utf-8"
            )
            for label, mode, evidence in (
                (
                    "mismatched",
                    "current_admission",
                    {**self.identity_v2_evidence, "network_id": "attacker-network"},
                ),
                (
                    "historical",
                    "historical_audit",
                    copy.deepcopy(self.identity_v2_evidence),
                ),
            ):
                with self.subTest(map_state=label):
                    evidence_path = root / f"{label}-evidence-map.json"
                    evidence_path.write_text(json.dumps(evidence, sort_keys=True), encoding="utf-8")
                    journal_path = root / f"{label}-journal.jsonl"
                    ledger_path = root / f"{label}-ledger.jsonl"
                    with mock.patch.object(
                        self.adapter,
                        "execute",
                        side_effect=self.adapter.AdapterError("identity-v2 CLI gate missing"),
                    ) as execute:
                        with self.assertRaises(self.adapter.AdapterError) as raised:
                            self.adapter.main(
                                [
                                    "--plan",
                                    str(plan_path),
                                    "--authority",
                                    str(authority_path),
                                    "--journal",
                                    str(journal_path),
                                    "--ledger",
                                    str(ledger_path),
                                    "--identity-v2-evidence-map",
                                    str(evidence_path),
                                    "--identity-v2-mode",
                                    mode,
                                    "--apply",
                                ]
                            )
                    self.assertRegex(str(raised.exception), r"(?i)identity.?v2|evidence|map|current|historical|audit")
                    execute.assert_not_called()
                    self.assertFalse(journal_path.exists())
                    self.assertFalse(ledger_path.exists())

    def _bind_test_ledger(self, plan: dict[str, object]) -> dict[str, object]:
        plan["credential_nonce_ledger"]["path"] = str(self.ledger_path)
        plan["credential_nonce_ledger"]["receipt"]["bindings"]["path"] = str(self.ledger_path)
        for node in plan["nodes"]:
            node["credential_seam"]["ledger_path"] = str(self.ledger_path)
        plan["plan_digest"] = self.adapter.canonical_plan_digest(plan)
        return plan

    def _authority(
        self, apply_authorized: bool = False, plan: dict[str, object] | None = None
    ) -> dict[str, object]:
        plan = plan or self.plan
        execution = copy.deepcopy(plan["truth"]["execution"])
        return {
            "schema_version": self.adapter.AUTHORITY_SCHEMA,
            "repository": "eng-cc/oasis7",
            "task_uid": plan["task_uid"],
            "frozen_head_oid": plan["head_oid"],
            "plan_digest": plan["plan_digest"],
            "adapter_id": self.adapter.CANONICAL_ADAPTER_ID,
            "network_id": self.adapter.CANONICAL_NETWORK_ID,
            "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
            "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
            "trust_root_path": self.adapter.CANONICAL_TRUST_ROOT_PATH,
            "trust_root_digest": self.adapter.CANONICAL_TRUST_ROOT_DIGEST,
            "consumer_impact_record": copy.deepcopy(plan["consumer_impact_record"]),
            "trust_root_file": {
                "path": self.adapter.CANONICAL_TRUST_ROOT_PATH,
                "sha256": self.adapter.CANONICAL_TRUST_ROOT_FILE_SHA256,
                "root_digest": self.adapter.CANONICAL_TRUST_ROOT_DIGEST,
                "owner_scope": self.adapter.CANONICAL_TRUST_ROOT_OWNER_SCOPE,
                "owner_uid": os.getuid(),
                "mode": "0600",
                "regular_file": True,
            },
            "apply_authorized": apply_authorized,
            "receipt": {
                "schema_version": self.adapter.CRYPTO_RECEIPT_SCHEMA,
                "authenticated": True,
                "verified": True,
                "signer_id": "governance-signer",
                "signed_payload_sha256": "a" * 64,
                "signature_hex": "b" * 128,
                "canonical_digest": "c" * 64,
                "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
                "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
                "bindings": {
                    "task_uid": plan["task_uid"],
                    "frozen_head_oid": plan["head_oid"],
                    "plan_digest": plan["plan_digest"],
                    "execution": execution,
                    "ledger_path": plan["credential_nonce_ledger"]["path"],
                    "apply_authorized": apply_authorized,
                    "forensic_backup": copy.deepcopy(plan["forensic_backup"]),
                    "rollback": copy.deepcopy(plan["rollback"]),
                    "package_commit": plan["truth"]["package"]["commit"],
                    "checkpoint_id": plan["truth"]["checkpoint"]["checkpoint_id"],
                    "checkpoint_manifest_hash": plan["truth"]["checkpoint"]["manifest_hash"],
                    "trust_root_path": self.adapter.CANONICAL_TRUST_ROOT_PATH,
                    "trust_root_digest": self.adapter.CANONICAL_TRUST_ROOT_DIGEST,
                    "trust_root_file": {
                        "path": self.adapter.CANONICAL_TRUST_ROOT_PATH,
                        "sha256": self.adapter.CANONICAL_TRUST_ROOT_FILE_SHA256,
                        "root_digest": self.adapter.CANONICAL_TRUST_ROOT_DIGEST,
                        "owner_scope": self.adapter.CANONICAL_TRUST_ROOT_OWNER_SCOPE,
                        "owner_uid": os.getuid(),
                        "mode": "0600",
                        "regular_file": True,
                    },
                    "consumer_impact_record": {
                        "path": plan["consumer_impact_record"]["path"],
                        "sha256": plan["consumer_impact_record"]["sha256"],
                    },
                },
            },
        }

    def _no_backup_plan(self) -> dict[str, object]:
        request = self.fixture._input()
        request["backup_policy"] = {
            "mode": "operator-authorized-no-backup",
            "operator_authorized": True,
            "current_authorization": True,
            "repository": "eng-cc/oasis7",
            "action": "full-network-clean-room",
            "targets": list(self.planner.NODE_ORDER),
            "transaction_id": request["transaction_id"],
            "capture_window_id": request["capture_window_id"],
            "actor": "ops-actor",
            "issued_at": "2026-08-30T00:00:00Z",
            "expires_at": "2099-01-01T00:00:00Z",
            "task_uid": request["task_uid"],
            "frozen_head_oid": request["head_oid"],
            "reason": "immutable provider backup unavailable",
            "authority": self.fixture._no_backup_receipt(
                request, "2099-01-01T00:00:00Z"
            ),
        }
        return self._bind_test_ledger(
            self.planner.build_plan(request, identity_v2_evidence=copy.deepcopy(self.identity_v2_evidence))
        )

    @staticmethod
    def _write_ledger(path: Path, rows: list[dict[str, object]] | None = None) -> None:
        path.write_text(
            "".join(json.dumps(row, sort_keys=True) + "\n" for row in (rows or [])),
            encoding="utf-8",
        )
        path.chmod(0o600)

    def _explicit_inventory_plan(self) -> dict[str, object]:
        request = self.fixture._input()
        inventory = request["deployment_inventory"]
        for name in self.planner.NODE_ORDER:
            inventory["nodes"][name]["peer_id"] = self.planner.CANONICAL_PEER_REGISTRY[name]
        inventory["receipt"]["signed_payload_sha256"] = (
            self.planner._canonical_deployment_inventory_payload_digest(inventory)
        )
        request["deployment_inventory"] = inventory
        return self._bind_test_ledger(
            self.planner.build_plan(request, identity_v2_evidence=copy.deepcopy(self.identity_v2_evidence))
        )

    def _bind_current_receipt_freshness(
        self,
        plan: dict[str, object],
        *,
        capture_window_id: str | None = None,
        rotation_epoch: str | None = None,
        issued_at: str = "2026-09-01T00:00:00Z",
        expires_at: str = "2099-01-01T00:00:00Z",
    ) -> None:
        """Build v2 inventory/identity receipt envelopes for adapter admission."""
        freshness = {
            "capture_window_id": capture_window_id or plan["capture_window_id"],
            "rotation_epoch": rotation_epoch or self.adapter.CANONICAL_ROTATION_EPOCH,
            "issued_at": issued_at,
            "expires_at": expires_at,
        }
        inventory_receipt = plan["deployment_inventory"]["receipt"]
        inventory_receipt.update(
            {"schema_version": "oasis7.deployment_inventory_receipt.v2", **freshness}
        )
        inventory_receipt["canonical_digest"] = self.adapter._canonical_receipt_digest(
            inventory_receipt, excluded_fields=frozenset({"signed_payload_sha256"})
        )
        for node in plan["nodes"]:
            identity_receipt = node["identity_receipt"]
            identity_receipt.update(
                {"schema_version": "oasis7.identity_receipt.v2", **freshness}
            )
        # The inventory payload digest excludes its self-referential receipt,
        # so receipt freshness can be changed without rewriting the payload.
        inventory_receipt["signed_payload_sha256"] = (
            self.adapter._canonical_deployment_inventory_payload_digest(
                plan["deployment_inventory"]
            )
        )

    def test_adapter_accepts_current_v2_inventory_and_identity_receipt_freshness(self) -> None:
        """Current capture/rotation/time bindings are valid for every receipt."""
        plan = copy.deepcopy(self.plan)
        self._bind_current_receipt_freshness(plan)
        plan["plan_digest"] = self.adapter.canonical_plan_digest(plan)
        validated = self.adapter.validate_plan(plan)
        inventory_receipt = validated["deployment_inventory"]["receipt"]
        self.assertEqual(
            inventory_receipt["capture_window_id"], validated["capture_window_id"]
        )
        self.assertEqual(
            inventory_receipt["rotation_epoch"], self.adapter.CANONICAL_ROTATION_EPOCH
        )
        for node in validated["nodes"]:
            receipt = node["identity_receipt"]
            self.assertEqual(receipt["capture_window_id"], validated["capture_window_id"])
            self.assertEqual(receipt["rotation_epoch"], self.adapter.CANONICAL_ROTATION_EPOCH)

    def test_adapter_rejects_inventory_and_identity_receipt_freshness_drift(self) -> None:
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
        scopes = [("inventory", None)] + [
            ("identity", name) for name in self.planner.NODE_ORDER
        ]
        for scope, node_name in scopes:
            for label, field, value, expected_error in mutations:
                with self.subTest(scope=scope, node=node_name, mutation=label):
                    plan = copy.deepcopy(self.plan)
                    self._bind_current_receipt_freshness(plan)
                    if scope == "inventory":
                        receipt = plan["deployment_inventory"]["receipt"]
                    else:
                        receipt = next(
                            node["identity_receipt"]
                            for node in plan["nodes"]
                            if node["name"] == node_name
                        )
                    if value is None:
                        receipt.pop(field)
                    else:
                        receipt[field] = value
                        if label == "stale-window":
                            receipt["expires_at"] = "2020-01-02T00:00:00Z"
                        elif label == "future-window":
                            receipt["expires_at"] = "2100-01-01T00:00:00Z"
                    receipt["canonical_digest"] = self.adapter._canonical_receipt_digest(
                        receipt,
                        excluded_fields=frozenset(
                            {"signed_payload_sha256"}
                            if scope == "inventory"
                            else {"peer_id"}
                        ),
                    )
                    plan["plan_digest"] = self.adapter.canonical_plan_digest(plan)
                    with self.assertRaises(self.adapter.AdapterError) as raised:
                        self.adapter.validate_plan(plan)
                    self.assertRegex(str(raised.exception), expected_error)

    def test_adapter_rejects_receipt_freshness_outside_plan_capture_window(self) -> None:
        """Current-looking v2 receipts cannot escape the adapter's bounded capture interval."""
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
        for scope, node_name in [
            ("inventory", None),
            *(('identity', name) for name in self.planner.NODE_ORDER),
        ]:
            for label, freshness in mutations:
                with self.subTest(scope=scope, node=node_name, mutation=label):
                    plan = copy.deepcopy(self.plan)
                    self._bind_current_receipt_freshness(plan)
                    if scope == "inventory":
                        receipt = plan["deployment_inventory"]["receipt"]
                    else:
                        receipt = next(
                            node["identity_receipt"]
                            for node in plan["nodes"]
                            if node["name"] == node_name
                        )
                    receipt.update(freshness)
                    receipt["canonical_digest"] = self.adapter._canonical_receipt_digest(
                        receipt,
                        excluded_fields=frozenset(
                            {"signed_payload_sha256"}
                            if scope == "inventory"
                            else {"peer_id"}
                        ),
                    )
                    plan["plan_digest"] = self.adapter.canonical_plan_digest(plan)
                    with self.assertRaises(self.adapter.AdapterError) as raised:
                        self.adapter.validate_plan(plan)
                    self.assertRegex(
                        str(raised.exception),
                        r"(?i)capture|issued|expires|fresh|window|stale|inverted",
                    )

    def test_adapter_rejects_direct_runtime_identity_v1_admission(self) -> None:
        """The raw runtime identity receipt remains input material, never an admission envelope."""
        plan = copy.deepcopy(self.plan)
        raw_identity = self.fixture._raw_runtime_identity_receipt_v1_bytes(
            plan["nodes"][0]["identity_receipt"]
        )
        plan["nodes"][0]["identity_receipt"] = json.loads(raw_identity)
        plan["plan_digest"] = self.adapter.canonical_plan_digest(plan)
        with self.assertRaises(self.adapter.AdapterError) as raised:
            self.adapter.validate_plan(plan)
        self.assertRegex(str(raised.exception), r"(?i)identity.*schema|unsupported|v1|receipt")

    def test_adapter_rejects_synthetic_identity_digest_and_signature_pair(self) -> None:
        """The adapter must not admit the reserved all-a/all-b placeholder pair."""
        plan = copy.deepcopy(self.plan)
        identity = plan["nodes"][0]["identity_receipt"]
        identity["signed_payload_sha256"] = "a" * 64
        identity["signature_hex"] = "b" * 128
        identity["canonical_digest"] = self.adapter._canonical_receipt_digest(
            identity, excluded_fields=frozenset({"peer_id"})
        )
        plan["plan_digest"] = self.adapter.canonical_plan_digest(plan)
        with self.assertRaises(self.adapter.AdapterError) as raised:
            self.adapter.validate_plan(plan)
        self.assertRegex(
            str(raised.exception), r"(?i)identity|payload|digest|placeholder"
        )

    def test_rejects_fake_head_signature_and_peer(self) -> None:
        authority = self._authority()
        authority["frozen_head_oid"] = "f" * 40
        with self.assertRaises(self.adapter.AdapterError):
            self.adapter.validate_authority(self.plan, authority)

        authority = self._authority()
        authority["receipt"]["signature_hex"] = "0" * 128
        with self.assertRaises(self.adapter.AdapterError):
            self.adapter.validate_authority(self.plan, authority)

        plan = copy.deepcopy(self.plan)
        node = next(item for item in plan["nodes"] if item["name"] == "storage-205")
        receipt = ApplyTransport(self.adapter, plan)._receipt("stop:storage-205", node)
        receipt["peer_id"] = "12D3KooWattacker"
        receipt["bindings"]["peer_id"] = "12D3KooWattacker"
        with self.assertRaises(self.adapter.AdapterError):
            self.adapter._validate_provider_receipt(
                plan, "stop:storage-205", "storage-205", receipt, None
            )

        for field, bad_value in (
            ("schema_version", "oasis7.attacker_identity.v1"),
            ("signature_hex", "0" * 128),
            ("key_sha256", "0" * 64),
            ("key_mode", "0644"),
            ("verifier_id", "caller-verifier"),
            ("trust_root_id", "caller-root"),
        ):
            plan = copy.deepcopy(self.plan)
            plan["nodes"][0]["identity_receipt"][field] = bad_value
            plan["plan_digest"] = self.adapter.canonical_plan_digest(plan)
            with self.assertRaises(self.adapter.AdapterError):
                self.adapter.validate_plan(plan)

    def test_provider_receipt_uses_authenticated_deployment_peer_identity(self) -> None:
        """A rotated deployment peer must be accepted only from node truth."""
        plan = copy.deepcopy(self.plan)
        node = next(item for item in plan["nodes"] if item["name"] == "storage-205")
        live_peer = "12D3KooWrotatedstorage"
        node["identity_receipt"]["peer_id"] = live_peer
        receipt = ApplyTransport(self.adapter, plan)._receipt("stop:storage-205", node)
        receipt["peer_id"] = live_peer
        receipt["bindings"]["peer_id"] = live_peer

        try:
            validated = self.adapter._validate_provider_receipt(
                plan, "stop:storage-205", "storage-205", receipt, None
            )
        except self.adapter.AdapterError as error:
            self.fail(f"rotated deployment peer was rejected: {error}")
        self.assertEqual(validated["peer_id"], live_peer)

    def test_rejects_reordered_node_plan_before_apply(self) -> None:
        """Accepted node sets must retain the planner's canonical iteration order."""
        plan = copy.deepcopy(self.plan)
        plan["nodes"] = list(reversed(plan["nodes"]))
        plan["plan_digest"] = self.adapter.canonical_plan_digest(plan)

        with self.assertRaises(self.adapter.AdapterError):
            self.adapter.validate_plan(plan)

    def test_external_verifier_is_required_for_apply_and_binds_execution_truth(self) -> None:
        authority = self._authority()
        with self.assertRaises(self.adapter.AdapterError):
            self.adapter.execute(
                self.plan,
                authority,
                journal_path=Path(tempfile.mkdtemp()) / "journal.json",
                ledger_path=Path(tempfile.mkdtemp()) / "nonce.jsonl",
                dry_run=False,
            )

        calls: list[str] = []

        def verifier(plan: dict[str, object], receipt: dict[str, object]) -> dict[str, object]:
            calls.append(receipt["bindings"]["plan_digest"])
            return {
                "verified": True,
                "bindings": receipt["bindings"],
                "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
                "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
                "signer_id": "governance-signer",
            }

        with tempfile.TemporaryDirectory() as directory:
            ledger = self.ledger_path
            self._write_ledger(ledger)
            with self.assertRaises(self.adapter.AdapterError):
                self.adapter.execute(
                    self.plan,
                    authority,
                    journal_path=Path(directory) / "journal.json",
                    ledger_path=self.ledger_path,
                    dry_run=False,
                    provenance_verifier=verifier,
                )
        self.assertEqual(calls, [self.plan["plan_digest"]])

    def test_apply_verifies_inventory_and_identity_with_existing_provenance_seam(self) -> None:
        """Destructive admission must send nested plan receipts through the existing verifier seam."""
        authority = self._authority(apply_authorized=True)
        calls: list[str] = []

        def verifier(plan_dto: dict[str, object], receipt: dict[str, object]) -> dict[str, object]:
            schema = receipt["schema_version"]
            calls.append(schema)
            bindings = receipt.get("bindings", {})
            if schema in {
                self.adapter.DEPLOYMENT_INVENTORY_RECEIPT_SCHEMA,
                self.adapter.IDENTITY_RECEIPT_SCHEMA,
            }:
                return {
                    "verified": False,
                    "bindings": bindings,
                    "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
                    "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
                    "signer_id": "governance-signer",
                }
            return {
                "verified": True,
                "bindings": bindings,
                "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
                "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
                "signer_id": "governance-signer",
            }

        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(self.adapter.AdapterError):
                self.adapter.execute(
                    self.plan,
                    authority,
                    journal_path=Path(directory) / "journal.json",
                    ledger_path=self.ledger_path,
                    transport=ApplyTransport(self.adapter, self.plan),
                    dry_run=False,
                    provenance_verifier=verifier,
                )
        self.assertIn(self.adapter.DEPLOYMENT_INVENTORY_RECEIPT_SCHEMA, calls)
        self.assertIn(self.adapter.IDENTITY_RECEIPT_SCHEMA, calls)

    def test_apply_validates_every_provider_receipt_and_persists_sanitized_receipts(self) -> None:
        authority = self._authority(apply_authorized=True)
        verifier_calls: list[str] = []

        def verifier(plan: dict[str, object], receipt: dict[str, object]) -> dict[str, object]:
            verifier_calls.append(receipt["bindings"]["operation"] if "operation" in receipt["bindings"] else "authority")
            return {
                "verified": True,
                "bindings": receipt["bindings"],
                "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
                "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
                "signer_id": "governance-signer",
            }

        transport = ApplyTransport(self.adapter, self.plan)
        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / "journal.json"
            result = self.adapter.execute(
                self.plan,
                authority,
                journal_path=journal,
                ledger_path=self.ledger_path,
                transport=transport,
                dry_run=False,
                provenance_verifier=verifier,
            )
            record = json.loads(journal.read_text(encoding="utf-8"))
        self.assertEqual(result["status"], "complete")
        self.assertEqual(transport.operations, self.plan["global_order"])
        self.assertEqual(len(record["provider_receipts"]), len(self.plan["global_order"]))
        self.assertIsNone(record["rollback_receipt"])
        self.assertEqual(record["execution_mode"], "apply")
        self.assertEqual(record["rollback_status"], "not-needed")
        preflight_receipts = record["preflight_evidence_receipts"]
        self.assertEqual(len(preflight_receipts), len(self.plan["nodes"]))
        self.assertEqual(
            [receipt["operation"] for receipt in preflight_receipts],
            [f"preflight:{name}" for name in self.plan["node_order"]],
        )
        for receipt in preflight_receipts:
            self.assertRegex(receipt["bindings"]["evidence_sha256"], r"^[0-9a-f]{64}$")
        self.assertIn("authority", verifier_calls)
        self.assertIn("fresh-root-probe", verifier_calls)

    def test_identity_verifier_binding_carries_raw_v1_digest_and_governed_context(self) -> None:
        """The v2 envelope verifier input must bind raw v1 bytes and all admission context."""
        plan = copy.deepcopy(self.plan)
        self._bind_current_receipt_freshness(plan)
        raw_v1_by_node: dict[str, bytes] = {}
        for node in plan["nodes"]:
            identity = node["identity_receipt"]
            raw_v1 = self.fixture._raw_runtime_identity_receipt_v1_bytes(
                identity,
                key_path="/opt/oasis7/p2p-testnet/config/node-keypair.toml",
            )
            raw_v1_by_node[node["name"]] = raw_v1
            identity["signed_payload_sha256"] = hashlib.sha256(raw_v1).hexdigest()
            identity["canonical_digest"] = self.adapter._canonical_receipt_digest(
                identity, excluded_fields=frozenset({"peer_id"})
            )
        plan["plan_digest"] = self.adapter.canonical_plan_digest(plan)

        verified_identities: list[tuple[dict[str, object], dict[str, object]]] = []

        def verifier(
            plan_dto: dict[str, object], receipt: dict[str, object]
        ) -> dict[str, object]:
            bindings = receipt.get("bindings", {})
            if bindings.get("kind") == "identity":
                verified_identities.append((receipt, bindings))
                self.assertEqual(receipt["raw_v1_bytes"], raw_v1_by_node[bindings["node"]])
            return {
                "verified": True,
                "bindings": bindings,
                "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
                "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
                "signer_id": "governance-signer",
            }

        self.adapter._verify_plan_receipts_with_verifier(
            plan,
            verifier,
            raw_v1_bytes_by_node=raw_v1_by_node,
        )

        self.assertEqual(len(verified_identities), len(self.planner.NODE_ORDER))
        for receipt, bindings in verified_identities:
            node = next(item for item in plan["nodes"] if item["name"] == bindings["node"])
            identity = node["identity_receipt"]
            expected_bindings = {
                "kind": "identity",
                "task_uid": plan["task_uid"],
                "frozen_head_oid": plan["head_oid"],
                "plan_digest": plan["plan_digest"],
                "node": node["name"],
                "node_id": node["node_id"],
                "peer_id": identity["peer_id"],
                "key_sha256": identity["key_sha256"],
                "key_size_bytes": identity["key_size_bytes"],
                "key_mode": identity["key_mode"],
                "key_uid": identity["key_uid"],
                "key_gid": identity["key_gid"],
                "capture_window_id": identity["capture_window_id"],
                "rotation_epoch": identity["rotation_epoch"],
                "issued_at": identity["issued_at"],
                "expires_at": identity["expires_at"],
                "signed_payload_sha256": identity["signed_payload_sha256"],
                "consumer_impact_record": self.adapter._consumer_impact_locator(plan),
            }
            self.assertEqual(bindings, expected_bindings)
            self.assertEqual(receipt["schema_version"], self.adapter.IDENTITY_RECEIPT_SCHEMA)
            self.assertEqual(receipt["signed_payload_sha256"], identity["signed_payload_sha256"])
            self.assertEqual(receipt["verifier_id"], self.adapter.CANONICAL_VERIFIER_ID)
            self.assertEqual(receipt["trust_root_id"], self.adapter.CANONICAL_TRUST_ROOT_ID)

    def test_apply_persists_preflight_complete_checkpoint_before_first_operation(self) -> None:
        authority = self._authority(apply_authorized=True)
        statuses: list[str] = []
        original_write = self.adapter._write_journal

        def observe_write(path: Path, record: dict[str, object]) -> None:
            statuses.append(record["status"])
            original_write(path, record)

        def verifier(plan: dict[str, object], receipt: dict[str, object]) -> dict[str, object]:
            return {
                "verified": True,
                "bindings": receipt["bindings"],
                "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
                "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
                "signer_id": "governance-signer",
            }

        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / "journal.json"
            with mock.patch.object(self.adapter, "_write_journal", side_effect=observe_write):
                self.adapter.execute(
                    self.plan,
                    authority,
                    journal_path=journal,
                    ledger_path=self.ledger_path,
                    transport=ApplyTransport(self.adapter, self.plan),
                    dry_run=False,
                    provenance_verifier=verifier,
                )
            record = json.loads(journal.read_text(encoding="utf-8"))
        self.assertIn("preflight-complete", statuses)
        self.assertLess(statuses.index("preflight-complete"), statuses.index("in-flight"))
        self.assertEqual(record["preflight_status"], "complete")
        self.assertEqual(record["nonce_reservation_state"]["reserved_count"], len(self.plan["nodes"]))
        self.assertTrue(record["nonce_reservation_state"]["complete"])

    def test_consumer_impact_drift_after_in_flight_write_blocks_destructive_callback(self) -> None:
        authority = self._authority(apply_authorized=True)

        def verifier(plan: dict[str, object], receipt: dict[str, object]) -> dict[str, object]:
            return {
                "verified": True,
                "bindings": receipt["bindings"],
                "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
                "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
                "signer_id": "governance-signer",
            }

        transport = ApplyTransport(self.adapter, self.plan)
        original_write = self.adapter._write_journal
        stop_index = self.plan["global_order"].index("stop:storage-205")

        def mutate_after_in_flight_write(path: Path, record: dict[str, object]) -> None:
            original_write(path, record)
            if record["status"] == "in-flight" and record["next_operation_index"] == stop_index:
                self.plan["consumer_impact_record"]["record"]["impact"] = "active"

        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / "journal.json"
            with mock.patch.object(self.adapter, "_write_journal", side_effect=mutate_after_in_flight_write):
                with self.assertRaises(self.adapter.AdapterError):
                    self.adapter.execute(
                        self.plan,
                        authority,
                        journal_path=journal,
                        ledger_path=self.ledger_path,
                        transport=transport,
                        dry_run=False,
                        provenance_verifier=verifier,
                    )

            record = json.loads(journal.read_text(encoding="utf-8"))

        self.assertEqual(transport.operations, self.plan["global_order"][:stop_index])
        self.assertEqual(transport.rollback_reobservations, [])
        self.assertEqual(transport.rollback_operations, [])
        self.assertEqual(record["status"], "in-flight")

    def test_resume_from_prepared_or_preflight_checkpoint_reconciles_without_double_use(self) -> None:
        authority = self._authority(apply_authorized=True)

        def verifier(plan: dict[str, object], receipt: dict[str, object]) -> dict[str, object]:
            return {
                "verified": True,
                "bindings": receipt["bindings"],
                "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
                "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
                "signer_id": "governance-signer",
            }

        for interrupted_status in ("prepared", "preflight-complete"):
            with self.subTest(interrupted_status=interrupted_status), tempfile.TemporaryDirectory() as directory:
                self._write_ledger(self.ledger_path)
                journal = Path(directory) / "journal.json"
                original_write = self.adapter._write_journal

                def interrupt(path: Path, record: dict[str, object]) -> None:
                    original_write(path, record)
                    if record["status"] == interrupted_status:
                        raise KeyboardInterrupt

                with mock.patch.object(self.adapter, "_write_journal", side_effect=interrupt):
                    with self.assertRaises(KeyboardInterrupt):
                        self.adapter.execute(
                            self.plan,
                            authority,
                            journal_path=journal,
                            ledger_path=self.ledger_path,
                            transport=ApplyTransport(self.adapter, self.plan),
                            dry_run=False,
                            provenance_verifier=verifier,
                        )
                checkpoint = json.loads(journal.read_text(encoding="utf-8"))
                self.assertEqual(checkpoint["status"], interrupted_status)
                rows_before = len(self.ledger_path.read_text(encoding="utf-8").splitlines())
                transport = ApplyTransport(self.adapter, self.plan)
                original_inspect = transport.inspect_node

                def inspect(node: dict[str, object]) -> dict[str, object]:
                    if interrupted_status == "preflight-complete":
                        raise AssertionError("resume must reuse the durable preflight checkpoint")
                    return original_inspect(node)

                transport.inspect_node = inspect
                result = self.adapter.resume_transaction(
                    self.plan,
                    authority,
                    journal,
                    ledger_path=self.ledger_path,
                    transport=transport,
                    dry_run=False,
                    provenance_verifier=verifier,
                )
                rows_after = len(self.ledger_path.read_text(encoding="utf-8").splitlines())
                self.assertEqual(result["status"], "complete")
                expected_rows = (
                    rows_before
                    if interrupted_status == "preflight-complete"
                    else rows_before + len(self.plan["nodes"])
                )
                self.assertEqual(rows_after, expected_rows)

    def test_resume_after_partial_nonce_reservation_reconciles_missing_once(self) -> None:
        authority = self._authority(apply_authorized=True)

        def verifier(plan: dict[str, object], receipt: dict[str, object]) -> dict[str, object]:
            return {
                "verified": True,
                "bindings": receipt["bindings"],
                "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
                "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
                "signer_id": "governance-signer",
            }

        original_reserve = self.adapter.reserve_nonce
        reservations = 0

        def reserve_then_interrupt(path: Path, transaction_id: str, nonce: str) -> None:
            nonlocal reservations
            if reservations == 1:
                raise KeyboardInterrupt
            original_reserve(path, transaction_id, nonce)
            reservations += 1

        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / "journal.json"
            with mock.patch.object(self.adapter, "reserve_nonce", side_effect=reserve_then_interrupt):
                with self.assertRaises(KeyboardInterrupt):
                    self.adapter.execute(
                        self.plan,
                        authority,
                        journal_path=journal,
                        ledger_path=self.ledger_path,
                        transport=ApplyTransport(self.adapter, self.plan),
                        dry_run=False,
                        provenance_verifier=verifier,
                    )
            checkpoint = json.loads(journal.read_text(encoding="utf-8"))
            self.assertEqual(checkpoint["status"], "prepared")
            self.assertEqual(checkpoint["nonce_reservation_state"]["reserved_count"], 0)
            self.assertEqual(len(self.ledger_path.read_text(encoding="utf-8").splitlines()), 1)
            result = self.adapter.resume_transaction(
                self.plan,
                authority,
                journal,
                ledger_path=self.ledger_path,
                transport=ApplyTransport(self.adapter, self.plan),
                dry_run=False,
                provenance_verifier=verifier,
            )
            self.assertEqual(result["status"], "complete")
            self.assertEqual(
                len(self.ledger_path.read_text(encoding="utf-8").splitlines()),
                len(self.plan["nodes"]),
            )

    def test_preflight_checkpoint_fails_closed_if_ledger_reservation_is_missing(self) -> None:
        authority = self._authority(apply_authorized=True)

        def verifier(plan: dict[str, object], receipt: dict[str, object]) -> dict[str, object]:
            return {
                "verified": True,
                "bindings": receipt["bindings"],
                "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
                "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
                "signer_id": "governance-signer",
            }

        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / "journal.json"
            original_write = self.adapter._write_journal

            def interrupt_after_checkpoint(path: Path, record: dict[str, object]) -> None:
                original_write(path, record)
                if record["status"] == "preflight-complete":
                    raise KeyboardInterrupt

            with mock.patch.object(self.adapter, "_write_journal", side_effect=interrupt_after_checkpoint):
                with self.assertRaises(KeyboardInterrupt):
                    self.adapter.execute(
                        self.plan,
                        authority,
                        journal_path=journal,
                        ledger_path=self.ledger_path,
                        transport=ApplyTransport(self.adapter, self.plan),
                        dry_run=False,
                        provenance_verifier=verifier,
                    )
            rows = self.ledger_path.read_text(encoding="utf-8").splitlines()
            self._write_ledger(self.ledger_path, [json.loads(row) for row in rows[:-1]])
            transport = ApplyTransport(self.adapter, self.plan)
            with self.assertRaises(self.adapter.AdapterError):
                self.adapter.resume_transaction(
                    self.plan,
                    authority,
                    journal,
                    ledger_path=self.ledger_path,
                    transport=transport,
                    dry_run=False,
                    provenance_verifier=verifier,
                )
            self.assertEqual(transport.operations, [])

    def test_preflight_evidence_receipts_must_follow_canonical_node_order(self) -> None:
        transport = ApplyTransport(self.adapter, self.plan)
        receipts = [
            transport._receipt(f"preflight:{name}", next(node for node in self.plan["nodes"] if node["name"] == name))
            for name in self.plan["node_order"]
        ]
        for receipt in receipts:
            receipt["bindings"]["evidence_sha256"] = "a" * 64
        with self.assertRaises(self.adapter.AdapterError):
            self.adapter._validate_journal_preflight_evidence_receipts(
                self.plan,
                list(reversed(receipts)),
            )

    def test_prepared_journal_rejects_preflight_receipts_instead_of_appending(self) -> None:
        authority = self._authority(apply_authorized=True)

        def verifier(plan: dict[str, object], receipt: dict[str, object]) -> dict[str, object]:
            return {
                "verified": True,
                "bindings": receipt["bindings"],
                "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
                "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
                "signer_id": "governance-signer",
            }

        transport = ApplyTransport(self.adapter, self.plan)
        receipt = transport.inspect_node(self.plan["nodes"][0])["receipt"]
        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / "journal.json"
            self.adapter._write_journal(
                journal,
                self.adapter._journal_record(
                    self.plan,
                    "prepared",
                    0,
                    [],
                    execution_mode="apply",
                    preflight_evidence_receipts=[receipt],
                    preflight_status="pending",
                    nonce_reservation_state=self.adapter._nonce_reservation_state(self.plan, 0),
                ),
            )
            with self.assertRaisesRegex(self.adapter.AdapterError, "prepared journal must not contain preflight evidence"):
                self.adapter.resume_transaction(
                    self.plan,
                    authority,
                    journal,
                    ledger_path=self.ledger_path,
                    transport=transport,
                    dry_run=False,
                    provenance_verifier=verifier,
                )
        self.assertEqual(transport.operations, [])

    def test_v1_journal_is_rejected_with_migration_reconciliation_message(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / "journal.json"
            self.adapter.execute(
                self.plan,
                self._authority(),
                journal_path=journal,
                ledger_path=self.ledger_path,
                dry_run=True,
            )
            record = json.loads(journal.read_text(encoding="utf-8"))
            record["schema_version"] = "oasis7.clean_room_mutation_journal.v1"
            record["journal_digest"] = self.adapter.journal_digest(record)
            journal.write_text(json.dumps(record) + "\n", encoding="utf-8")
            with self.assertRaisesRegex(self.adapter.AdapterError, "v1.*migration.*reconciliation"):
                self.adapter._read_journal(journal)

    def test_nonce_append_then_error_terminal_state_uses_ledger_readback(self) -> None:
        authority = self._authority(apply_authorized=True)

        def verifier(plan: dict[str, object], receipt: dict[str, object]) -> dict[str, object]:
            return {
                "verified": True,
                "bindings": receipt["bindings"],
                "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
                "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
                "signer_id": "governance-signer",
            }

        original_reserve = self.adapter.reserve_nonce

        def append_then_error(path: Path, transaction_id: str, nonce: str) -> None:
            original_reserve(path, transaction_id, nonce)
            raise RuntimeError("append succeeded before callback error")

        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / "journal.json"
            with mock.patch.object(self.adapter, "reserve_nonce", side_effect=append_then_error):
                with self.assertRaises(self.adapter.AdapterError):
                    self.adapter.execute(
                        self.plan,
                        authority,
                        journal_path=journal,
                        ledger_path=self.ledger_path,
                        transport=ApplyTransport(self.adapter, self.plan),
                        dry_run=False,
                        provenance_verifier=verifier,
                    )
            record = json.loads(journal.read_text(encoding="utf-8"))
        self.assertEqual(record["status"], "terminal-failure")
        self.assertEqual(record["nonce_reservation_state"]["reserved_count"], 1)

    def test_repository_trust_root_fixture_matches_provenance_helper_without_monkeypatch(self) -> None:
        provenance = load_module("validator_pair_provenance_fixture", PROVENANCE_PATH)
        fixture = ROOT / "scripts" / "fixtures" / "oasis7-governance-root.v1.json"
        self.assertTrue(fixture.is_file())
        loaded = provenance.load_trust_root(fixture)
        self.assertEqual(loaded["root_digest"], self.adapter.CANONICAL_TRUST_ROOT_DIGEST)
        self.assertEqual(
            hashlib.sha256(fixture.read_bytes()).hexdigest(),
            self.adapter.CANONICAL_TRUST_ROOT_FILE_SHA256,
        )

    def test_backup_failure_with_no_mutation_candidates_does_not_call_clean_redeploy(self) -> None:
        authority = self._authority(apply_authorized=True)

        def verifier(plan: dict[str, object], receipt: dict[str, object]) -> dict[str, object]:
            return {
                "verified": True,
                "bindings": receipt["bindings"],
                "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
                "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
                "signer_id": "governance-signer",
            }

        transport = ApplyTransport(
            self.adapter,
            self.plan,
            invalid_operation="forensic-backup:storage-205",
        )
        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / "journal.json"
            with self.assertRaises(self.adapter.AdapterError):
                self.adapter.execute(
                    self.plan,
                    authority,
                    journal_path=journal,
                    ledger_path=self.ledger_path,
                    transport=transport,
                    dry_run=False,
                    provenance_verifier=verifier,
                )
            record = json.loads(journal.read_text(encoding="utf-8"))
        self.assertEqual(transport.rollback_operations, [])
        self.assertEqual(transport.rollback_reobservations, [])
        self.assertEqual(record["rollback_status"], "not-needed")
        self.assertEqual(record["backup_status"], "backup-failed")
        self.assertEqual(record["backup_error"], "AdapterError")

    def test_apply_executes_code_owned_live_trust_root_check_before_remote_observation(self) -> None:
        authority = self._authority(apply_authorized=True)

        def verifier(plan: dict[str, object], receipt: dict[str, object]) -> dict[str, object]:
            return {
                "verified": True,
                "bindings": receipt["bindings"],
                "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
                "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
                "signer_id": "governance-signer",
            }

        transport = ApplyTransport(self.adapter, self.plan, invalid_operation="preflight:storage-205")
        with tempfile.TemporaryDirectory() as directory:
            with mock.patch.object(
                self.adapter,
                "validate_live_trust_root_file",
                create=True,
                return_value={"path": self.adapter.CANONICAL_TRUST_ROOT_PATH},
            ) as live_check:
                with self.assertRaises(self.adapter.AdapterError):
                    self.adapter.execute(
                        self.plan,
                        authority,
                        journal_path=Path(directory) / "journal.json",
                        ledger_path=self.ledger_path,
                        transport=transport,
                        dry_run=False,
                        provenance_verifier=verifier,
                    )
                live_check.assert_called_once_with()

    def test_read_only_failure_with_no_rollback_candidates_never_calls_rollback(self) -> None:
        authority = self._authority(apply_authorized=True)

        def verifier(plan: dict[str, object], receipt: dict[str, object]) -> dict[str, object]:
            return {
                "verified": True,
                "bindings": receipt["bindings"],
                "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
                "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
                "signer_id": "governance-signer",
            }

        transport = ApplyTransport(self.adapter, self.plan, invalid_operation="preflight:storage-205")
        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / "journal.json"
            with mock.patch.object(
                self.adapter,
                "validate_live_trust_root_file",
                create=True,
                return_value={"path": self.adapter.CANONICAL_TRUST_ROOT_PATH},
            ):
                with self.assertRaises(self.adapter.AdapterError):
                    self.adapter.execute(
                        self.plan,
                        authority,
                        journal_path=journal,
                        ledger_path=self.ledger_path,
                        transport=transport,
                        dry_run=False,
                        provenance_verifier=verifier,
                    )
            record = json.loads(journal.read_text(encoding="utf-8"))
        self.assertEqual(transport.rollback_operations, [])
        self.assertEqual(transport.rollback_reobservations, [])
        self.assertEqual(record["rollback_status"], "not-needed")

    def test_journal_rejects_symlink_in_any_ancestor(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            real_parent = root / "real-parent"
            real_parent.mkdir()
            symlink_parent = root / "symlink-parent"
            symlink_parent.symlink_to(real_parent, target_is_directory=True)
            with self.assertRaises(self.adapter.AdapterError):
                self.adapter._write_journal(
                    symlink_parent / "nested" / "journal.json",
                    {"schema_version": self.adapter.JOURNAL_SCHEMA},
                )

    def test_live_trust_root_file_checks_content_owner_mode_and_symlink(self) -> None:
        self.live_trust_root_patcher.stop()
        try:
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory) / "trust-root.json"
                content = (ROOT / "scripts" / "fixtures" / "oasis7-governance-root.v1.json").read_bytes()
                semantic_digest = json.loads(content.decode("utf-8"))["root_digest"]
                file_digest = hashlib.sha256(content).hexdigest()
                root.write_bytes(content)
                root.chmod(0o600)
                with mock.patch.object(self.adapter, "CANONICAL_TRUST_ROOT_PATH", str(root)), mock.patch.object(
                    self.adapter,
                    "CANONICAL_TRUST_ROOT_DIGEST",
                    semantic_digest,
                ), mock.patch.object(
                    self.adapter,
                    "CANONICAL_TRUST_ROOT_FILE_SHA256",
                    file_digest,
                ):
                    result = self.adapter.validate_live_trust_root_file()
                    self.assertEqual(result["sha256"], file_digest)
                    root.chmod(0o644)
                    with self.assertRaises(self.adapter.AdapterError):
                        self.adapter.validate_live_trust_root_file()
                    root.chmod(0o600)
                    with mock.patch.object(
                        self.adapter, "CANONICAL_TRUST_ROOT_OWNER_UID", os.getuid() + 1
                    ):
                        with self.assertRaises(self.adapter.AdapterError):
                            self.adapter.validate_live_trust_root_file()
                    root.unlink()
                    root.symlink_to(Path(directory) / "missing-root.json")
                    with self.assertRaises(self.adapter.AdapterError):
                        self.adapter.validate_live_trust_root_file()
                    real_parent = Path(directory) / "real-parent"
                    real_parent.mkdir()
                    nested_root = real_parent / "trust-root.json"
                    nested_root.write_bytes(content)
                    nested_root.chmod(0o600)
                    symlink_parent = Path(directory) / "symlink-parent"
                    symlink_parent.symlink_to(real_parent, target_is_directory=True)
                    with mock.patch.object(
                        self.adapter,
                        "CANONICAL_TRUST_ROOT_PATH",
                        str(symlink_parent / "trust-root.json"),
                    ), mock.patch.object(
                        self.adapter,
                        "CANONICAL_TRUST_ROOT_DIGEST",
                        semantic_digest,
                    ), mock.patch.object(
                        self.adapter,
                        "CANONICAL_TRUST_ROOT_FILE_SHA256",
                        file_digest,
                    ):
                        with self.assertRaises(self.adapter.AdapterError):
                            self.adapter.validate_live_trust_root_file()
        finally:
            self.live_trust_root_patcher.start()

    def test_completed_apply_resume_revalidates_authority_verifier_and_receipt_signatures(self) -> None:
        authority = self._authority(apply_authorized=True)
        verifier_calls: list[str] = []

        def verifier(plan: dict[str, object], receipt: dict[str, object]) -> dict[str, object]:
            verifier_calls.append(
                receipt["bindings"].get("operation", "authority")
            )
            return {
                "verified": True,
                "bindings": receipt["bindings"],
                "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
                "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
                "signer_id": "governance-signer",
            }

        transport = ApplyTransport(self.adapter, self.plan)
        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / "journal.json"
            self.adapter.execute(
                self.plan,
                authority,
                journal_path=journal,
                ledger_path=self.ledger_path,
                transport=transport,
                dry_run=False,
                provenance_verifier=verifier,
            )

            verifier_calls.clear()
            with self.assertRaises(self.adapter.AdapterError):
                self.adapter.resume_transaction(
                    self.plan,
                    self._authority(apply_authorized=False),
                    journal,
                    ledger_path=self.ledger_path,
                    dry_run=False,
                    provenance_verifier=verifier,
                )
            self.assertEqual(verifier_calls, [])

            record = json.loads(journal.read_text(encoding="utf-8"))
            record["provider_receipts"][0]["signature_hex"] = "0" * 128
            record["journal_digest"] = self.adapter.journal_digest(record)
            journal.write_text(json.dumps(record) + "\n", encoding="utf-8")
            verifier_calls.clear()
            with self.assertRaises(self.adapter.AdapterError):
                self.adapter.resume_transaction(
                    self.plan,
                    authority,
                    journal,
                    ledger_path=self.ledger_path,
                    dry_run=False,
                    provenance_verifier=verifier,
                )
            self.assertIn("authority", verifier_calls)

    def test_completed_apply_resume_revalidates_evidence_bound_preflight_receipt(self) -> None:
        authority = self._authority(apply_authorized=True)

        def verifier(plan: dict[str, object], receipt: dict[str, object]) -> dict[str, object]:
            return {
                "verified": True,
                "bindings": receipt["bindings"],
                "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
                "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
                "signer_id": "governance-signer",
            }

        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / "journal.json"
            self.adapter.execute(
                self.plan,
                authority,
                journal_path=journal,
                ledger_path=self.ledger_path,
                transport=ApplyTransport(self.adapter, self.plan),
                dry_run=False,
                provenance_verifier=verifier,
            )
            record = json.loads(journal.read_text(encoding="utf-8"))
            record["preflight_evidence_receipts"][0]["signature_hex"] = "0" * 128
            record["journal_digest"] = self.adapter.journal_digest(record)
            journal.write_text(json.dumps(record) + "\n", encoding="utf-8")
            with self.assertRaises(self.adapter.AdapterError):
                self.adapter.resume_transaction(
                    self.plan,
                    authority,
                    journal,
                    ledger_path=self.ledger_path,
                    dry_run=False,
                    provenance_verifier=verifier,
                )

    def test_operator_no_backup_apply_requires_signed_current_authority(self) -> None:
        plan = self._no_backup_plan()
        authority = self._authority(apply_authorized=True, plan=plan)
        plan["forensic_backup"]["authority"] = None
        plan["plan_digest"] = self.adapter.canonical_plan_digest(plan)
        authority = self._authority(apply_authorized=True, plan=plan)
        with self.assertRaises(self.adapter.AdapterError):
            self.adapter.execute(
                plan,
                authority,
                journal_path=Path(tempfile.mkdtemp()) / "journal.json",
                ledger_path=self.ledger_path,
                dry_run=True,
            )

    def test_no_backup_receipt_rejects_seed_claim_without_manifest(self) -> None:
        """No-backup apply receipts cannot retain seed eligibility without evidence."""
        plan = self._no_backup_plan()
        node = next(item for item in plan["nodes"] if item["name"] == "storage-205")
        receipt = ApplyTransport(self.adapter, plan)._receipt("stop:storage-205", node)
        receipt.pop("backup_manifest", None)
        receipt["seed_eligible"] = True

        with self.assertRaises(self.adapter.AdapterError):
            self.adapter._validate_provider_receipt(
                plan, "stop:storage-205", "storage-205", receipt, None
            )

    def test_fresh_root_probe_requires_signed_full_closure(self) -> None:
        transport = ApplyTransport(self.adapter, self.plan)
        receipt = transport._receipt("fresh-root-probe", None)
        receipt["blob_closure"]["json_index_consistency"]["verified"] = False
        with self.assertRaises(self.adapter.AdapterError):
            self.adapter._validate_provider_receipt(
                self.plan,
                "fresh-root-probe",
                None,
                receipt,
                None,
            )

    def test_every_provider_receipt_is_fresh_and_inside_transaction_capture_window(self) -> None:
        transport = ApplyTransport(self.adapter, self.plan)
        receipt = transport._receipt("stop:storage-205", self.plan["nodes"][0])
        receipt["replayed"] = True
        with self.assertRaises(self.adapter.AdapterError):
            self.adapter._validate_provider_receipt(
                self.plan, "stop:storage-205", "storage-205", receipt, None
            )

        receipt = transport._receipt("stop:storage-205", self.plan["nodes"][0])
        receipt["captured_at"] = "2020-01-01T00:00:00Z"
        with self.assertRaises(self.adapter.AdapterError):
            self.adapter._validate_provider_receipt(
                self.plan, "stop:storage-205", "storage-205", receipt, None
            )

        receipt = transport._receipt("stop:storage-205", self.plan["nodes"][0])
        receipt["captured_at"] = "2099-01-01T00:00:00Z"
        with self.assertRaises(self.adapter.AdapterError):
            self.adapter._validate_provider_receipt(
                self.plan, "stop:storage-205", "storage-205", receipt, None
            )

    def test_remote_preflight_requires_signed_verifier_checked_evidence_receipt(self) -> None:
        node = self.plan["nodes"][0]
        required_bytes, required_inodes = self.adapter.capacity_requirement(self.plan, node)
        evidence = {
            "node": node["name"],
            "node_id": node["node_id"],
            "provider_uid": self.adapter.CANONICAL_PROVIDER_UID[node["name"]],
            "node_root": node["node_root"],
            "persistent_state_paths": list(node["persistent_state_paths"]),
            "symlink_free": True,
            "free_bytes": required_bytes,
            "required_bytes": required_bytes,
            "free_inodes": required_inodes,
            "required_inodes": required_inodes,
            "host_target": node["host_binding"]["target"],
            "known_hosts_path": node["host_binding"]["known_hosts_path"],
            "known_host_fingerprint": node["host_binding"]["known_host_fingerprint"],
            "known_hosts_regular": True,
            "known_hosts_owner_uid": os.getuid(),
            "known_hosts_mode": "0600",
        }
        with self.assertRaises(self.adapter.AdapterError):
            self.adapter.validate_remote_preflight(self.plan, node, evidence)

    def test_no_backup_authority_rejects_future_issue_and_requires_independent_verifier(self) -> None:
        plan = self._no_backup_plan()
        plan["forensic_backup"]["issued_at"] = "2099-01-01T00:00:00Z"
        plan["forensic_backup"]["authority"]["bindings"]["issued_at"] = "2099-01-01T00:00:00Z"
        plan["plan_digest"] = self.adapter.canonical_plan_digest(plan)
        with self.assertRaises(self.adapter.AdapterError):
            self.adapter.validate_plan(plan)

        plan = self._no_backup_plan()
        authority = self._authority(apply_authorized=False, plan=plan)
        calls: list[str] = []

        def verifier(plan_dto: dict[str, object], receipt: dict[str, object]) -> dict[str, object]:
            calls.append(receipt["schema_version"])
            return {
                "verified": True,
                "bindings": receipt["bindings"],
                "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
                "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
                "signer_id": "governance-signer",
            }

        with tempfile.TemporaryDirectory() as directory:
            self.adapter.execute(
                plan,
                authority,
                journal_path=Path(directory) / "journal.json",
                ledger_path=self.ledger_path,
                dry_run=True,
                provenance_verifier=verifier,
            )
        self.assertEqual(
            calls,
            [self.adapter.CRYPTO_RECEIPT_SCHEMA, self.adapter.NO_BACKUP_AUTHORITY_SCHEMA],
        )

    def test_no_backup_authority_binds_task_head_transaction_and_capture(self) -> None:
        for field, bad_value in (
            ("task_uid", "task_attacker"),
            ("frozen_head_oid", "f" * 40),
            ("transaction_id", "other-transaction"),
            ("capture_window_id", "other-window"),
        ):
            plan = self._no_backup_plan()
            plan["forensic_backup"][field] = bad_value
            plan["plan_digest"] = self.adapter.canonical_plan_digest(plan)
            with self.assertRaises(self.adapter.AdapterError):
                self.adapter.validate_plan(plan)

        plan = self._no_backup_plan()
        plan["forensic_backup"]["authority"]["bindings"]["frozen_head_oid"] = "f" * 40
        plan["plan_digest"] = self.adapter.canonical_plan_digest(plan)
        with self.assertRaises(self.adapter.AdapterError):
            self.adapter.validate_plan(plan)

    def test_forensic_backup_mode_cannot_bypass_required_reset_gate(self) -> None:
        for field, bad_value in (
            ("task_uid", "task_attacker"),
            ("frozen_head_oid", "f" * 40),
            ("required_before_reset", False),
            ("immutable", False),
            ("receipt_required_per_node", False),
            ("operator_authorized", True),
            ("current_authorization", True),
            ("mode", "operator-authorized-no-backup"),
        ):
            plan = copy.deepcopy(self.plan)
            plan["forensic_backup"][field] = bad_value
            plan["plan_digest"] = self.adapter.canonical_plan_digest(plan)
            with self.assertRaises(self.adapter.AdapterError):
                self.adapter.validate_plan(plan)

    def test_provider_receipts_require_phase_schema_and_capture_contract(self) -> None:
        transport = ApplyTransport(self.adapter, self.plan)
        receipt = transport._receipt("stop:storage-205", self.plan["nodes"][1])
        for field, bad_value in (
            ("schema_version", self.adapter.PROVIDER_RECEIPT_SCHEMA),
            ("phase", "verify"),
            ("captured_at", None),
            ("observer_mutation", True),
            ("status", "planned"),
            ("seed_eligible", True),
        ):
            invalid = copy.deepcopy(receipt)
            invalid[field] = bad_value
            with self.assertRaises(self.adapter.AdapterError):
                self.adapter._validate_provider_receipt(
                    self.plan, "stop:storage-205", "storage-205", invalid, None
                )

        backup = transport._receipt("forensic-backup:storage-205", self.plan["nodes"][1])
        backup["backup_manifest"]["seed_eligible"] = True
        with self.assertRaises(self.adapter.AdapterError):
            self.adapter._validate_provider_receipt(
                self.plan, "forensic-backup:storage-205", "storage-205", backup, None
            )

        health = transport._receipt("fleet-health", None)
        health["fleet_health_closure"]["nodes"] = ["storage-205"]
        with self.assertRaises(self.adapter.AdapterError):
            self.adapter._validate_provider_receipt(self.plan, "fleet-health", None, health, None)

    def test_fleet_health_marker_without_final_snapshot_is_rejected(self) -> None:
        """A three-field marker cannot close same-window fleet health."""
        receipt = ApplyTransport(self.adapter, self.plan)._receipt("fleet-health", None)
        receipt["fleet_health_closure"].pop("snapshot")
        with self.assertRaises(self.adapter.AdapterError):
            self.adapter._validate_provider_receipt(
                self.plan, "fleet-health", None, receipt, None
            )

    def test_fleet_health_requires_reciprocal_validator_peers(self) -> None:
        """Both validators must observe each other before fleet closure."""
        receipt = ApplyTransport(self.adapter, self.plan)._receipt("fleet-health", None)
        for missing_name in ("storage-205", "sequencer-204"):
            invalid = copy.deepcopy(receipt)
            invalid["fleet_health_closure"]["snapshot"][missing_name]["connected_peers"] = []
            with self.assertRaises(self.adapter.AdapterError) as raised:
                self.adapter._validate_provider_receipt(
                    self.plan, "fleet-health", None, invalid, None
                )
            self.assertRegex(str(raised.exception), r"(?i)(validator|peer|connected)")

    def test_rejects_invalid_provider_signature_or_peer_before_advancing(self) -> None:
        authority = self._authority(apply_authorized=True)

        def verifier(plan: dict[str, object], receipt: dict[str, object]) -> dict[str, object]:
            return {
                "verified": True,
                "bindings": receipt["bindings"],
                "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
                "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
                "signer_id": "governance-signer",
            }

        for kwargs in (
            {"invalid_operation": "preflight:storage-205"},
            {"invalid_signature": True},
            {"peer_mismatch": True},
        ):
            transport = ApplyTransport(self.adapter, self.plan, **kwargs)
            self._write_ledger(self.ledger_path)
            with tempfile.TemporaryDirectory() as directory:
                journal = Path(directory) / "journal.json"
                with self.assertRaises(self.adapter.AdapterError):
                    self.adapter.execute(
                        self.plan,
                        authority,
                        journal_path=journal,
                        ledger_path=self.ledger_path,
                        transport=transport,
                        dry_run=False,
                        provenance_verifier=verifier,
                    )
                record = json.loads(journal.read_text(encoding="utf-8"))
            self.assertEqual(record["status"], "terminal-failure")
            self.assertEqual(record["execution_mode"], "apply")
            self.assertTrue(
                all(self.adapter._rollback_candidate(operation) for operation in transport.rollback_started)
            )

    def test_apply_failure_persists_node_and_rollback_receipts(self) -> None:
        authority = self._authority(apply_authorized=True)

        def verifier(plan: dict[str, object], receipt: dict[str, object]) -> dict[str, object]:
            return {
                "verified": True,
                "bindings": receipt["bindings"],
                "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
                "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
                "signer_id": "governance-signer",
            }

        transport = ApplyTransport(self.adapter, self.plan, invalid_operation="stop:storage-205")
        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / "journal.json"
            with self.assertRaises(self.adapter.AdapterError):
                self.adapter.execute(
                    self.plan,
                    authority,
                    journal_path=journal,
                    ledger_path=self.ledger_path,
                    transport=transport,
                    dry_run=False,
                    provenance_verifier=verifier,
                )
            record = json.loads(journal.read_text(encoding="utf-8"))
        self.assertEqual(record["status"], "terminal-failure")
        self.assertTrue(record["provider_receipts"])
        self.assertEqual(record["rollback_status"], "completed")
        self.assertEqual(record["rollback_receipt"]["operation"], "rollback-clean-redeploy")
        self.assertEqual(transport.rollback_reobservations, ["stop:storage-205"])
        self.assertEqual(record["rollback_receipt"]["failed_operation"], "stop:storage-205")
        self.assertEqual(
            record["rollback_receipt"]["rollback_steps"], self.plan["rollback"]["steps"]
        )

    def test_rollback_failure_is_reconciliation_blocked_and_durable(self) -> None:
        authority = self._authority(apply_authorized=True)

        def verifier(plan: dict[str, object], receipt: dict[str, object]) -> dict[str, object]:
            return {
                "verified": True,
                "bindings": receipt["bindings"],
                "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
                "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
                "signer_id": "governance-signer",
            }

        transport = ApplyTransport(
            self.adapter,
            self.plan,
            invalid_operation="stop:storage-205",
            rollback_failure=True,
        )
        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / "journal.json"
            with self.assertRaises(self.adapter.AdapterError):
                self.adapter.execute(
                    self.plan,
                    authority,
                    journal_path=journal,
                    ledger_path=self.ledger_path,
                    transport=transport,
                    dry_run=False,
                    provenance_verifier=verifier,
                )
            record = json.loads(journal.read_text(encoding="utf-8"))
        self.assertEqual(record["rollback_status"], "reconciliation-blocked")
        self.assertIsNone(record["rollback_receipt"])
        self.assertEqual(record["rollback_error"], "RuntimeError")

    def test_side_effect_then_throw_includes_current_operation_in_rollback(self) -> None:
        authority = self._authority(apply_authorized=True)

        def verifier(plan: dict[str, object], receipt: dict[str, object]) -> dict[str, object]:
            return {
                "verified": True,
                "bindings": receipt["bindings"],
                "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
                "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
                "signer_id": "governance-signer",
            }

        transport = ApplyTransport(
            self.adapter,
            self.plan,
            side_effect_operation="rebuild:storage-205",
        )
        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / "journal.json"
            with self.assertRaises(self.adapter.AdapterError):
                self.adapter.execute(
                    self.plan,
                    authority,
                    journal_path=journal,
                    ledger_path=self.ledger_path,
                    transport=transport,
                    dry_run=False,
                    provenance_verifier=verifier,
                )
        self.assertIn("rebuild:storage-205", transport.rollback_started)

    def test_first_mutation_journal_failure_does_not_invent_rollback_candidate(self) -> None:
        authority = self._authority(apply_authorized=True)
        transport = ApplyTransport(self.adapter, self.plan)
        stop_index = self.plan["global_order"].index("stop:storage-205")
        original_write = self.adapter._write_journal
        injected = False

        def write(path, record):
            nonlocal injected
            if (not injected and record["status"] == "in-flight"
                    and record["next_operation_index"] == stop_index):
                injected = True
                raise self.adapter.AdapterError("injected first mutation journal failure")
            original_write(path, record)

        def verifier(plan, receipt):
            return {"verified": True, "bindings": receipt["bindings"],
                    "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
                    "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
                    "signer_id": "governance-signer"}

        journal = Path(self._test_directory.name) / "unattempted-mutation.json"
        with mock.patch.object(self.adapter, "_write_journal", side_effect=write):
            with self.assertRaises(self.adapter.AdapterError):
                self.adapter.execute(self.plan, authority, journal_path=journal,
                    ledger_path=self.ledger_path, transport=transport,
                    dry_run=False, provenance_verifier=verifier)
        self.assertTrue(injected)
        self.assertEqual(transport.operations, self.plan["global_order"][:stop_index])
        self.assertEqual(transport.rollback_reobservations, [])
        self.assertEqual(transport.rollback_operations, [])
        self.assertEqual(transport.rollback_started, [])
        record = json.loads(journal.read_text(encoding="utf-8"))
        self.assertEqual(record["status"], "terminal-failure")
        self.assertEqual(record["rollback_status"], "not-needed")

    def test_journal_write_failure_rolls_back_current_started_operation(self) -> None:
        authority = self._authority(apply_authorized=True)

        def verifier(plan: dict[str, object], receipt: dict[str, object]) -> dict[str, object]:
            return {
                "verified": True,
                "bindings": receipt["bindings"],
                "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
                "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
                "signer_id": "governance-signer",
            }

        transport = ApplyTransport(self.adapter, self.plan)
        original_write = self.adapter._write_journal
        injected = False

        def write_with_one_failure(path: Path, record: dict[str, object]) -> None:
            nonlocal injected
            if (
                not injected
                and record.get("status") == "running"
                and "start:sequencer-204" in record.get("completed_operations", [])
            ):
                injected = True
                raise self.adapter.AdapterError("injected journal failure")
            original_write(path, record)

        self.adapter._write_journal = write_with_one_failure
        try:
            with tempfile.TemporaryDirectory() as directory:
                journal = Path(directory) / "journal.json"
                with self.assertRaises(self.adapter.AdapterError):
                    self.adapter.execute(
                        self.plan,
                        authority,
                        journal_path=journal,
                        ledger_path=self.ledger_path,
                        transport=transport,
                        dry_run=False,
                        provenance_verifier=verifier,
                    )
                record = json.loads(journal.read_text(encoding="utf-8"))
        finally:
            self.adapter._write_journal = original_write
        self.assertTrue(injected)
        self.assertIn("start:sequencer-204", transport.rollback_started)
        self.assertEqual(record["rollback_status"], "completed")

    def test_transport_and_receipt_boundaries_reject_secret_fields(self) -> None:
        node = copy.deepcopy(self.plan["nodes"][0])
        node["identity_receipt"]["nested"] = {"password": "must-not-cross"}
        with self.assertRaises(self.adapter.AdapterError):
            self.adapter._transport_node(node)
        node = copy.deepcopy(self.plan["nodes"][0])
        node["identity_receipt"]["nested"] = {"safe_label": "PRIVATE KEY material"}
        with self.assertRaises(self.adapter.AdapterError):
            self.adapter._transport_node(node)

        plan = copy.deepcopy(self.plan)
        plan["unexpected_provider_field"] = "must-not-cross"
        with self.assertRaises(self.adapter.AdapterError):
            self.adapter._transport_plan(plan)

        transport = ApplyTransport(self.adapter, self.plan)
        receipt = transport._receipt("stop:storage-205", self.plan["nodes"][0])
        receipt["bindings"]["secret"] = "must-not-persist"
        with self.assertRaises(self.adapter.AdapterError):
            self.adapter._validate_provider_receipt(
                self.plan,
                "stop:storage-205",
                "storage-205",
                receipt,
                None,
            )

    def test_rejects_remote_path_escape_and_pinned_host_drift(self) -> None:
        plan = copy.deepcopy(self.plan)
        plan["nodes"][0]["persistent_state_paths"][0] = "/outside/state"
        with self.assertRaises(self.adapter.AdapterError):
            self.adapter.validate_plan(plan)

        plan = copy.deepcopy(self.plan)
        plan["nodes"][0]["host_binding"]["target"] = "root@attacker.invalid"
        with self.assertRaises(self.adapter.AdapterError):
            self.adapter.validate_plan(plan)

        node = self.plan["nodes"][0]
        with self.assertRaises(self.adapter.AdapterError):
            self.adapter.validate_remote_preflight(
                self.plan,
                node,
                {
                    "node_id": node["node_id"],
                    "provider_uid": self.adapter.CANONICAL_PROVIDER_UID[node["name"]],
                    "node_root": node["node_root"],
                    "persistent_state_paths": ["/outside/state"],
                    "symlink_free": True,
                    "free_bytes": 10**12,
                    "required_bytes": 1,
                    "free_inodes": 10**6,
                    "required_inodes": 1,
                    "host_target": node["host_binding"]["target"],
                    "known_hosts_path": node["host_binding"]["known_hosts_path"],
                    "known_host_fingerprint": node["host_binding"]["known_host_fingerprint"],
                },
            )

    def test_adapter_requires_complete_canonical_state_surfaces_per_managed_node(self) -> None:
        """Adapter admission must reject sparse or nested omissions in reset surfaces."""
        for node_name in self.planner.NODE_ORDER:
            for omission in ("sparse", "nested"):
                with self.subTest(node=node_name, omission=omission):
                    plan = copy.deepcopy(self.plan)
                    node = next(item for item in plan["nodes"] if item["name"] == node_name)
                    full_paths = list(node["persistent_state_paths"])
                    if omission == "sparse":
                        incomplete_paths = [full_paths[0]]
                    else:
                        incomplete_paths = full_paths[:2] + full_paths[3:]
                    node["persistent_state_paths"] = incomplete_paths
                    plan["deployment_inventory"]["nodes"][node_name][
                        "persistent_state_paths"
                    ] = incomplete_paths
                    if node["role"] == "observer":
                        plan["surfaces"]["observers_by_node"][node_name] = incomplete_paths
                    plan["plan_digest"] = self.adapter.canonical_plan_digest(plan)
                    with self.assertRaises(self.adapter.AdapterError) as raised:
                        self.adapter.validate_plan(plan)
                    self.assertRegex(str(raised.exception), r"(?i)surface|canonical|complete|path")

    def test_adapter_enforces_component_aware_windows_state_containment(self) -> None:
        """Windows root and sibling-prefix paths cannot masquerade as descendants."""
        windows_name = "windows-observer"
        windows_index = next(
            index for index, node in enumerate(self.plan["nodes"])
            if node["name"] == windows_name
        )
        for label, invalid_path in (
            ("exact-root", "C:/oasis7-deploy"),
            ("sibling-prefix", "C:/oasis7-deploy-evil/state"),
        ):
            with self.subTest(path=label):
                plan = copy.deepcopy(self.plan)
                plan["nodes"][windows_index]["persistent_state_paths"] = [invalid_path]
                plan["deployment_inventory"]["nodes"][windows_name][
                    "persistent_state_paths"
                ] = [invalid_path]
                plan["surfaces"]["observers_by_node"][windows_name] = [invalid_path]
                plan["plan_digest"] = self.adapter.canonical_plan_digest(plan)
                with self.assertRaises(self.adapter.AdapterError) as raised:
                    self.adapter.validate_plan(plan)
                self.assertRegex(str(raised.exception), r"(?i)root|surface|path|contain")

        # The fixture's complete Windows inventory consists of true descendants.
        self.adapter.validate_plan(self.plan)

    def test_remote_preflight_requires_exact_pin_symlink_and_capacity_evidence(self) -> None:
        node = self.plan["nodes"][0]
        required_bytes, required_inodes = self.adapter.capacity_requirement(self.plan, node)
        evidence = {
            "node": node["name"],
            "node_id": node["node_id"],
            "provider_uid": self.adapter.CANONICAL_PROVIDER_UID[node["name"]],
            "node_root": node["node_root"],
            "persistent_state_paths": list(node["persistent_state_paths"]),
            "symlink_free": True,
            "free_bytes": required_bytes,
            "required_bytes": required_bytes,
            "free_inodes": required_inodes,
            "required_inodes": required_inodes,
            "host_target": node["host_binding"]["target"],
            "known_hosts_path": node["host_binding"]["known_hosts_path"],
            "known_host_fingerprint": node["host_binding"]["known_host_fingerprint"],
            "known_hosts_regular": True,
            "known_hosts_owner_uid": os.getuid(),
            "known_hosts_mode": "0600",
        }
        evidence["receipt"] = ApplyTransport(self.adapter, self.plan)._receipt(
            f"preflight:{node['name']}", node, evidence=evidence
        )

        def verifier(plan: dict[str, object], receipt: dict[str, object]) -> dict[str, object]:
            return {
                "verified": True,
                "bindings": receipt["bindings"],
                "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
                "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
                "signer_id": "governance-signer",
            }

        result = self.adapter.validate_remote_preflight(self.plan, node, evidence, verifier)
        self.assertTrue(result["known_hosts_pinned"])
        tampered = copy.deepcopy(evidence)
        tampered["free_bytes"] += 1
        with self.assertRaises(self.adapter.AdapterError):
            self.adapter.validate_remote_preflight(self.plan, node, tampered, verifier)
        for field, bad_value in (("symlink_free", False), ("free_bytes", required_bytes - 1)):
            invalid = dict(evidence, **{field: bad_value})
            with self.assertRaises(self.adapter.AdapterError):
                self.adapter.validate_remote_preflight(self.plan, node, invalid, verifier)

    def test_credential_ledger_is_regular_0600_and_rejects_nonce_replay(self) -> None:
        nonce = self.plan["credential_nonce_ledger"]["reserved_nonces"][0]
        row = {
            "schema_version": self.adapter.NONCE_ROW_SCHEMA,
            "transaction_id": self.plan["transaction_id"],
            "nonce": nonce,
            "one_shot": True,
        }
        with tempfile.TemporaryDirectory() as directory:
            ledger = Path(directory) / "nonce.jsonl"
            self._write_ledger(ledger, [row])
            with self.assertRaises(self.adapter.AdapterError):
                self.adapter.validate_credential_ledger(self.plan, ledger)

            cross_transaction = Path(directory) / "cross-transaction.jsonl"
            self._write_ledger(
                cross_transaction,
                [dict(row, transaction_id="different-transaction")],
            )
            with self.assertRaises(self.adapter.AdapterError):
                self.adapter.validate_credential_ledger(self.plan, cross_transaction)

            ledger = Path(directory) / "duplicate.jsonl"
            self._write_ledger(ledger, [dict(row, nonce="fresh-nonce"), dict(row, nonce="fresh-nonce")])
            with self.assertRaises(self.adapter.AdapterError):
                self.adapter.validate_credential_ledger(self.plan, ledger)

            ledger = Path(directory) / "replay.jsonl"
            self._write_ledger(ledger)
            self.adapter.reserve_nonce(ledger, self.plan["transaction_id"], "one-shot-test")
            with self.assertRaises(self.adapter.AdapterError):
                self.adapter.reserve_nonce(ledger, self.plan["transaction_id"], "one-shot-test")

            ledger.chmod(0o644)
            with self.assertRaises(self.adapter.AdapterError):
                self.adapter.validate_credential_ledger(self.plan, ledger)

    def test_journal_resume_is_bound_and_ambiguous_state_is_terminal(self) -> None:
        authority = self._authority()
        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / "journal.json"
            ledger = self.ledger_path
            self._write_ledger(ledger)
            result = self.adapter.execute(
                self.plan,
                authority,
                journal_path=journal,
                ledger_path=ledger,
                dry_run=True,
            )
            self.assertEqual(result["status"], "dry-run-complete")
            resumed = self.adapter.resume_transaction(
                self.plan,
                authority,
                journal,
                ledger_path=ledger,
                dry_run=True,
            )
            self.assertTrue(resumed["resumed"])

            record = json.loads(journal.read_text(encoding="utf-8"))
            record["status"] = "in-flight"
            record["journal_digest"] = self.adapter.journal_digest(record)
            journal.write_text(json.dumps(record) + "\n", encoding="utf-8")
            with self.assertRaises(self.adapter.AdapterError):
                self.adapter.resume_transaction(
                    self.plan,
                    authority,
                    journal,
                    ledger_path=ledger,
                    dry_run=True,
                )
            terminal = json.loads(journal.read_text(encoding="utf-8"))
            self.assertEqual(terminal["status"], "terminal-failure")

    def test_terminal_journal_failure_persists_emergency_reconciliation_receipt(self) -> None:
        original_write = self.adapter._write_journal
        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / "journal.json"

            def fail_primary(path: Path, record: dict[str, object]) -> None:
                if path == journal:
                    raise self.adapter.AdapterError("injected primary journal failure")
                original_write(path, record)

            self.adapter._write_journal = fail_primary
            try:
                with self.assertRaises(self.adapter.AdapterError):
                    self.adapter._persist_terminal(
                        journal,
                        {
                            "schema_version": self.adapter.JOURNAL_SCHEMA,
                            "status": "terminal-failure",
                            "transaction_id": self.plan["transaction_id"],
                        },
                    )
            finally:
                self.adapter._write_journal = original_write

            emergency = Path(f"{journal}.emergency.json")
            record = json.loads(emergency.read_text(encoding="utf-8"))
            self.assertEqual(record["status"], "reconciliation-blocked")
            self.assertTrue(record["emergency_receipt"])
            self.assertEqual(record["journal_write_error"], "AdapterError")

    def test_dry_run_has_deterministic_order_and_never_mutates_provider(self) -> None:
        authority = self._authority()
        transport = FakeTransport()
        with tempfile.TemporaryDirectory() as directory:
            ledger = self.ledger_path
            self._write_ledger(ledger)
            result = self.adapter.execute(
                self.plan,
                authority,
                journal_path=Path(directory) / "journal.json",
                ledger_path=ledger,
                transport=transport,
                dry_run=True,
            )
            record = json.loads((Path(directory) / "journal.json").read_text())
            self.assertEqual(record["rollback_candidates"], [])
        self.assertEqual(result["operations"], self.plan["global_order"])
        self.assertEqual(transport.mutations, [])
        serialized = json.dumps(result, sort_keys=True)
        self.assertNotRegex(serialized, r"(?i)(nonce-|password=|secret-value|private.?key)")
        self.assertEqual(
            {value["receipt"]["status"] for value in result["nodes"].values()},
            {"planned"},
        )

    def test_transport_boundary_never_receives_credential_seams(self) -> None:
        node = self.plan["nodes"][0]
        transport_node = self.adapter._transport_node(node)
        transport_plan = self.adapter._transport_plan(self.plan)
        self.assertNotIn("credential_seam", transport_node)
        self.assertNotIn("credential_nonce_ledger", transport_plan)
        self.assertTrue(all("credential_seam" not in item for item in transport_plan["nodes"]))

        captured: dict[str, object] = {}

        def verifier(plan: dict[str, object], receipt: dict[str, object]) -> dict[str, object]:
            captured.update(plan)
            return {
                "verified": True,
                "bindings": receipt["bindings"],
                "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
                "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
                "signer_id": "governance-signer",
            }

        with tempfile.TemporaryDirectory() as directory:
            self.adapter.execute(
                self.plan,
                self._authority(),
                journal_path=Path(directory) / "journal.json",
                ledger_path=self.ledger_path,
                dry_run=True,
                provenance_verifier=verifier,
            )
        self.assertNotIn("credential_nonce_ledger", captured)
        self.assertNotIn("authority", captured)
        captured_text = json.dumps(captured, sort_keys=True)
        self.assertNotIn("storage-nonce-", captured_text)
        self.assertNotIn("PUBLIC_TESTNET_", captured_text)

    def test_transport_plan_rejects_nested_credential_fields(self) -> None:
        plan = copy.deepcopy(self.plan)
        plan["truth"]["package"]["api_key"] = "provider-api-key-must-not-cross"
        with self.assertRaises(self.adapter.AdapterError):
            self.adapter._transport_plan(plan)

    def test_transport_plan_rejects_nested_authorization_and_bearer_fields(self) -> None:
        """Provider DTOs cannot carry authorization aliases at any nesting depth."""
        cases = (
            ("authorization", "opaque-auth-value"),
            ("bearer", "opaque-bearer-value"),
            ("headers", {"Authorization": "opaque-header-value"}),
            ("metadata", {"provider": {"bearer": "opaque-nested-bearer-value"}}),
        )
        for field, value in cases:
            with self.subTest(field=field):
                plan = copy.deepcopy(self.plan)
                plan["truth"]["package"][field] = value
                try:
                    projected = self.adapter._transport_plan(plan)
                except self.adapter.AdapterError:
                    continue
                serialized = json.dumps(projected["truth"], sort_keys=True).lower()
                if "authorization" in serialized:
                    self.fail(f"{field} leaked authorization alias")
                if "bearer" in serialized:
                    self.fail(f"{field} leaked bearer alias")
                if "opaque-" in serialized:
                    self.fail(f"{field} leaked opaque credential value")

    def test_transport_surfaces_use_only_the_canonical_node_aware_observer_inventory(self) -> None:
        """Provider truth cannot expose a conflicting generic seven-path observer list."""
        transport_surfaces = self.adapter._transport_plan(self.plan)["surfaces"]
        self.assertNotIn("observers", transport_surfaces)
        self.assertEqual(
            transport_surfaces["observers_by_node"],
            {
                node["name"]: node["persistent_state_paths"]
                for node in self.plan["nodes"]
                if node["role"] == "observer"
            },
        )

    def test_transport_projects_every_provider_bound_nested_section_by_exact_schema(self) -> None:
        """Unknown nested plan fields must never cross through shallow copies."""
        sections = (
            "capture_window",
            "canonical_host_inventory",
            "canonical_endpoint_inventory",
            "execution",
            "fresh_root_probe",
            "observer_gate",
            "operation_journal_contract",
            "adapter_verification",
            "consumer_impact_record",
        )
        for section in sections:
            with self.subTest(section=section):
                plan = copy.deepcopy(self.plan)
                self.assertIsInstance(plan[section], dict)
                plan[section]["__unexpected_transport_field__"] = "must-not-cross"
                try:
                    projected = self.adapter._transport_plan(plan)
                except self.adapter.AdapterError:
                    continue
                self.assertNotIn(
                    "__unexpected_transport_field__",
                    json.dumps(projected.get(section, {}), sort_keys=True),
                )

    def test_planner_and_adapter_share_deployment_receipt_extension_schema(self) -> None:
        """Planner and adapter must reject an unmodeled receipt extension consistently."""
        extension = {
            "schema_version": "oasis7.deployment_inventory_receipt_extension.v1",
            "deployment_epoch": "deployment-epoch-001",
            "inventory_digest": "d" * 64,
        }
        # Freeze one current authority instant and derive both the planner
        # negative request and adapter plan from that same authenticated input.
        # The planner rebuilds inventory/identity/capture-dependent material;
        # the adapter plan then receives only the intentional extension plus a
        # fresh plan digest, so the assertion cannot race the impact file clock.
        authority_instant = datetime.now(timezone.utc).replace(microsecond=0)
        base_request = self.fixture._input(authority_instant=authority_instant)
        request = copy.deepcopy(base_request)
        request["deployment_inventory"]["receipt"]["extensions"] = extension
        planner_error = None
        try:
            self.planner.build_plan(
                request, identity_v2_evidence=copy.deepcopy(self.identity_v2_evidence)
            )
        except SystemExit as error:
            planner_error = error
        self.assertIsNotNone(planner_error, "planner accepted an unmodeled receipt extension")
        if planner_error is not None:
            self.assertRegex(str(planner_error), r"(?i)receipt|extension|unsafe|schema")

        plan = self._bind_test_ledger(
            self.planner.build_plan(
                base_request, identity_v2_evidence=copy.deepcopy(self.identity_v2_evidence)
            )
        )
        plan["deployment_inventory"]["receipt"]["extensions"] = extension
        plan["plan_digest"] = self.adapter.canonical_plan_digest(plan)
        adapter_error = None
        try:
            self.adapter.validate_plan(plan)
        except self.adapter.AdapterError as error:
            adapter_error = error
        self.assertIsNotNone(adapter_error, "adapter accepted an unmodeled receipt extension")
        if adapter_error is not None:
            self.assertRegex(str(adapter_error), r"(?i)receipt|extension|unsafe|schema")

    def test_adapter_accepts_independent_authenticated_uid_and_gid_truth(self) -> None:
        """The adapter must verify distinct deployment service UID and primary GID values."""
        request = self.fixture._input()
        request["deployment_inventory"] = self.fixture._deployment_inventory(
            request["nodes"], expected_uid=1001, expected_gid=1002
        )
        for node in request["nodes"]:
            node["identity_receipt"]["key_uid"] = 1001
            node["identity_receipt"]["key_gid"] = 1002
            identity = node["identity_receipt"]
            identity["signed_payload_sha256"] = hashlib.sha256(
                self.fixture._raw_runtime_identity_receipt_v1_bytes(identity)
            ).hexdigest()
            identity["canonical_digest"] = self.adapter._canonical_receipt_digest(
                identity, excluded_fields=frozenset({"peer_id"})
            )
        evidence_root = Path(self._test_directory.name) / "uid-gid-identity-v2-evidence"
        evidence_root.mkdir(mode=0o700)
        evidence, _ = self.fixture._network_binding_evidence_fixture(
            evidence_root,
            context_network_id=self.planner.CANONICAL_NETWORK_ID,
            request=request,
            expected_uid=1001,
            expected_gid=1002,
        )
        plan = self._bind_test_ledger(
            self.planner.build_plan(
                request, identity_v2_evidence=evidence
            )
        )
        self.adapter.validate_plan(plan)

    def test_adapter_requires_explicit_peer_id_on_every_inventory_node(self) -> None:
        """The adapter must not normalize omitted or partial authenticated peer identities."""
        for omission in ("all", "storage-205"):
            with self.subTest(omission=omission):
                plan = copy.deepcopy(self.plan)
                if omission == "all":
                    for node in plan["deployment_inventory"]["nodes"].values():
                        node.pop("peer_id")
                else:
                    plan["deployment_inventory"]["nodes"][omission].pop("peer_id")
                plan["plan_digest"] = self.adapter.canonical_plan_digest(plan)
                with self.assertRaises(self.adapter.AdapterError) as raised:
                    self.adapter.validate_plan(plan)
                self.assertRegex(str(raised.exception), r"(?i)peer|inventory|explicit|complete|digest")

    def test_adapter_rejects_inventory_mutations_with_stale_receipt(self) -> None:
        """Rebinding plan and identity fields cannot bypass the inventory receipt digest."""
        mutations = ("node_root", "persistent_state_paths", "expected_key_uid", "expected_key_gid", "peer_id")
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                plan = self._explicit_inventory_plan()
                name = "storage-205"
                inventory_node = plan["deployment_inventory"]["nodes"][name]
                node = next(item for item in plan["nodes"] if item["name"] == name)
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
                plan["plan_digest"] = self.adapter.canonical_plan_digest(plan)
                with self.assertRaises(self.adapter.AdapterError) as raised:
                    self.adapter.validate_plan(plan)
                self.assertRegex(str(raised.exception), r"(?i)inventory|receipt|digest|binding|peer|canonical")

    def test_adapter_accepts_fully_explicit_digest_bound_inventory(self) -> None:
        """A complete inventory whose receipt signs the canonical payload remains valid."""
        plan = self._explicit_inventory_plan()
        self.adapter.validate_plan(plan)
        for name in self.planner.NODE_ORDER:
            self.assertIn("peer_id", plan["deployment_inventory"]["nodes"][name])

    def test_adapter_rejects_shaped_but_unverified_inventory_and_identity_receipts(self) -> None:
        """Syntactically valid signature/digest strings are not verifier evidence."""
        mutations = (
            (
                "deployment-inventory-signature",
                lambda plan: plan["deployment_inventory"]["receipt"].__setitem__(
                    "signature_hex", "d" * 128
                ),
            ),
            (
                "deployment-inventory-canonical-digest",
                lambda plan: plan["deployment_inventory"]["receipt"].__setitem__(
                    "canonical_digest", "e" * 64
                ),
            ),
            (
                "identity-signature",
                lambda plan: plan["nodes"][0]["identity_receipt"].__setitem__(
                    "signature_hex", "d" * 128
                ),
            ),
            (
                "identity-canonical-digest",
                lambda plan: plan["nodes"][0]["identity_receipt"].__setitem__(
                    "canonical_digest", "e" * 64
                ),
            ),
        )
        for label, mutate in mutations:
            with self.subTest(receipt=label):
                plan = copy.deepcopy(self.plan)
                mutate(plan)
                plan["plan_digest"] = self.adapter.canonical_plan_digest(plan)
                with self.assertRaises(self.adapter.AdapterError) as raised:
                    self.adapter.validate_plan(plan)
                self.assertRegex(
                    str(raised.exception),
                    r"(?i)receipt|signature|digest|verified|authenticated|verifier",
                )

    def test_adapter_rejects_authority_binding_context_drift_and_stale_or_future_receipts(self) -> None:
        """The planner authority receipt must bind the live task context and freshness."""
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
                plan = copy.deepcopy(self.plan)
                bindings = copy.deepcopy(plan["authority"]["receipt"]["bindings"])
                bindings.update(updates)
                plan["authority"]["receipt"]["bindings"] = bindings
                plan["authority"]["trust_root"]["bindings"] = copy.deepcopy(bindings)
                plan["plan_digest"] = self.adapter.canonical_plan_digest(plan)
                with self.assertRaises(self.adapter.AdapterError) as raised:
                    self.adapter.validate_plan(plan)
                self.assertRegex(
                    str(raised.exception),
                    r"(?i)authority|binding|capture|rotation|task|head|stale|future|expir",
                )

    def test_adapter_rejects_duplicate_authenticated_peer_ids(self) -> None:
        """Provider admission must preserve one authenticated peer identity per node."""
        plan = copy.deepcopy(self.plan)
        plan["nodes"][1]["identity_receipt"]["peer_id"] = plan["nodes"][0]["identity_receipt"]["peer_id"]
        plan["plan_digest"] = self.adapter.canonical_plan_digest(plan)
        with self.assertRaises(self.adapter.AdapterError) as raised:
            self.adapter.validate_plan(plan)
        self.assertRegex(str(raised.exception), r"(?i)peer|identity|duplicate|unique")

    def test_adapter_rejects_unique_peer_ids_outside_authenticated_registry(self) -> None:
        """Peer uniqueness alone cannot authorize an arbitrary deployment identity."""
        for node in self.plan["nodes"]:
            with self.subTest(node=node["name"]):
                plan = copy.deepcopy(self.plan)
                target = next(item for item in plan["nodes"] if item["name"] == node["name"])
                target["identity_receipt"]["peer_id"] = (
                    f"12D3KooWcaller-supplied-{node['name']}"
                )
                plan["plan_digest"] = self.adapter.canonical_plan_digest(plan)
                with self.assertRaises(self.adapter.AdapterError) as raised:
                    self.adapter.validate_plan(plan)
                self.assertRegex(str(raised.exception), r"(?i)peer|identity|registry|canonical|binding")

    def test_adapter_recomputes_nonce_ledger_and_seam_one_shot_bindings(self) -> None:
        """Ledger and per-node seams must remain independently bound after digest recomputation."""
        mutations = (
            (
                "ledger-reserved-nonce",
                lambda plan: plan["credential_nonce_ledger"]["reserved_nonces"].__setitem__(
                    0, "caller-supplied-nonce-000000000000000000000000"
                ),
            ),
            (
                "seam-nonce",
                lambda plan: plan["nodes"][0]["credential_seam"].__setitem__(
                    "nonce", "caller-supplied-seam-nonce-000000000000000000"
                ),
            ),
            (
                "seam-one-shot",
                lambda plan: plan["nodes"][0]["credential_seam"].__setitem__("one_shot", False),
            ),
        )
        for label, mutate in mutations:
            with self.subTest(binding=label):
                plan = copy.deepcopy(self.plan)
                mutate(plan)
                plan["plan_digest"] = self.adapter.canonical_plan_digest(plan)
                with self.assertRaises(self.adapter.AdapterError) as raised:
                    self.adapter.validate_plan(plan)
                self.assertRegex(str(raised.exception), r"(?i)nonce|ledger|seam|one.?shot|binding")

    def test_adapter_rejects_rebound_nonce_leases_outside_time_window(self) -> None:
        """A rebound lease must still obey the planner's current-time contract."""
        mutations = (
            (
                "expired",
                "2020-01-01T00:00:00Z",
                "2020-01-02T00:00:00Z",
            ),
            (
                "future-issued",
                "2099-01-01T00:00:00Z",
                "2100-01-01T00:00:00Z",
            ),
        )
        for label, issued_at, expires_at in mutations:
            with self.subTest(lease=label):
                plan = copy.deepcopy(self.plan)
                ledger = plan["credential_nonce_ledger"]
                ledger["issued_at"] = issued_at
                ledger["expires_at"] = expires_at
                ledger["receipt"]["bindings"]["issued_at"] = issued_at
                ledger["receipt"]["bindings"]["expires_at"] = expires_at
                plan["capture_window"]["starts_at"] = issued_at
                plan["capture_window"]["ends_at"] = expires_at
                for node in plan["nodes"]:
                    seam = node["credential_seam"]
                    seam["issued_at"] = issued_at
                    seam["expires_at"] = expires_at
                plan["plan_digest"] = self.adapter.canonical_plan_digest(plan)
                with self.assertRaises(self.adapter.AdapterError) as raised:
                    self.adapter.validate_plan(plan)
                self.assertRegex(str(raised.exception), r"(?i)nonce|ledger|lease|future|expir|capture")

    def test_adapter_rejects_planner_invalid_short_nonce_after_rebinding(self) -> None:
        """The adapter must preserve the planner's minimum unpredictable nonce length."""
        plan = copy.deepcopy(self.plan)
        short_nonce = "short123"
        plan["credential_nonce_ledger"]["reserved_nonces"][0] = short_nonce
        plan["credential_nonce_ledger"]["receipt"]["bindings"]["reserved_nonces"][0] = short_nonce
        plan["nodes"][0]["credential_seam"]["nonce"] = short_nonce
        plan["plan_digest"] = self.adapter.canonical_plan_digest(plan)
        with self.assertRaises(self.adapter.AdapterError) as raised:
            self.adapter.validate_plan(plan)
        self.assertRegex(str(raised.exception), r"(?i)nonce|ledger|seam|unpredictable|length|binding")

    def test_adapter_rejects_rebound_nested_receipt_trust_labels(self) -> None:
        """Digest recomputation cannot authorize caller-controlled receipt identities."""
        mutations = (
            (
                "truth-package-verifier",
                lambda plan: plan["truth"]["package"]["receipt"].__setitem__(
                    "verifier_id", "caller-verifier"
                ),
            ),
            (
                "truth-genesis-trust-root",
                lambda plan: plan["truth"]["genesis"]["receipt"].__setitem__(
                    "trust_root_id", "caller-trust-root"
                ),
            ),
            (
                "fresh-root-probe-trust-root",
                lambda plan: plan["fresh_root_probe"]["receipt"].__setitem__(
                    "trust_root_id", "caller-trust-root"
                ),
            ),
            (
                "adapter-verification-verifier",
                lambda plan: plan["adapter_verification"]["receipt"].__setitem__(
                    "verifier_id", "caller-verifier"
                ),
            ),
        )
        for label, mutate in mutations:
            with self.subTest(receipt=label):
                plan = copy.deepcopy(self.plan)
                mutate(plan)
                plan["plan_digest"] = self.adapter.canonical_plan_digest(plan)
                with self.assertRaises(self.adapter.AdapterError) as raised:
                    self.adapter.validate_plan(plan)
                self.assertRegex(
                    str(raised.exception),
                    r"(?i)receipt|verifier|trust.?root|authenticated|canonical|binding",
                )

    def test_adapter_rejects_unbound_peer_rotation(self) -> None:
        """The retained current map fixes peer IDs; caller-side rotation is not admissible."""
        def inventory_payload_digest(inventory: dict[str, object]) -> str:
            payload = {
                key: copy.deepcopy(value)
                for key, value in inventory.items()
                if key != "receipt"
            }
            return hashlib.sha256(
                json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()
            ).hexdigest()

        rotated = copy.deepcopy(self.plan)
        for index, name in enumerate(self.planner.NODE_ORDER, start=1):
            rotated_peer = f"12D3KooWrotated{index:02d}{'x' * 20}"
            rotated["deployment_inventory"]["nodes"][name]["peer_id"] = rotated_peer
            node = next(item for item in rotated["nodes"] if item["name"] == name)
            node["identity_receipt"]["peer_id"] = rotated_peer
            identity = node["identity_receipt"]
            identity["signed_payload_sha256"] = hashlib.sha256(
                self.fixture._raw_runtime_identity_receipt_v1_bytes(identity)
            ).hexdigest()
            identity["canonical_digest"] = self.adapter._canonical_receipt_digest(
                identity, excluded_fields=frozenset({"peer_id"})
            )
        rotated["deployment_inventory"]["receipt"]["signed_payload_sha256"] = (
            inventory_payload_digest(rotated["deployment_inventory"])
        )
        rotated["plan_digest"] = self.adapter.canonical_plan_digest(rotated)
        with self.assertRaises(self.adapter.AdapterError) as rotated_raised:
            self.adapter.validate_plan(rotated)
        self.assertRegex(
            str(rotated_raised.exception),
            r"(?i)evidence|map|peer|identity|canonical|binding",
        )

        stale_receipt = copy.deepcopy(self.plan)
        stale_inventory_peer = "12D3KooWrotated-stale-receipt"
        stale_receipt["deployment_inventory"]["nodes"]["storage-205"]["peer_id"] = (
            stale_inventory_peer
        )
        stale_node = next(item for item in stale_receipt["nodes"] if item["name"] == "storage-205")
        stale_node["identity_receipt"]["peer_id"] = stale_inventory_peer
        # The plan digest and node identity are rebound, but the authenticated
        # inventory receipt still signs the previous inventory payload.
        stale_receipt["plan_digest"] = self.adapter.canonical_plan_digest(stale_receipt)
        with self.assertRaises(self.adapter.AdapterError) as stale_raised:
            self.adapter.validate_plan(stale_receipt)
        self.assertRegex(
            str(stale_raised.exception),
            r"(?i)inventory|receipt|digest|peer|binding",
        )

        forged = copy.deepcopy(self.plan)
        target = next(item for item in forged["nodes"] if item["name"] == "storage-205")
        target["identity_receipt"]["peer_id"] = "12D3KooWcaller-supplied-rotation"
        forged["plan_digest"] = self.adapter.canonical_plan_digest(forged)
        with self.assertRaises(self.adapter.AdapterError) as raised:
            self.adapter.validate_plan(forged)
        self.assertRegex(str(raised.exception), r"(?i)peer|identity|inventory|registry|binding")

    def test_adapter_recomputes_authenticated_semantic_bindings(self) -> None:
        """Digest recomputation cannot rebind truth, node, probe, or observer gate semantics."""
        mutations = (
            (
                "truth",
                lambda plan: plan["truth"]["package"].__setitem__("commit", "f" * 40),
            ),
            (
                "node-binding",
                lambda plan: plan["nodes"][0]["bindings"].__setitem__("package_commit", "f" * 40),
            ),
            (
                "fresh-root-probe",
                lambda plan: plan["fresh_root_probe"].__setitem__("package_commit", "f" * 40),
            ),
            (
                "observer-gate",
                lambda plan: plan["observer_gate"].__setitem__("checkpoint_receipt_required", False),
            ),
        )
        for section, mutate in mutations:
            with self.subTest(section=section):
                plan = copy.deepcopy(self.plan)
                mutate(plan)
                plan["plan_digest"] = self.adapter.canonical_plan_digest(plan)
                with self.assertRaises(self.adapter.AdapterError) as raised:
                    self.adapter.validate_plan(plan)
                self.assertRegex(str(raised.exception), r"(?i)truth|binding|probe|observer|gate|semantic|canonical")

    def test_identity_receipt_requires_governed_gid(self) -> None:
        plan = copy.deepcopy(self.plan)
        plan["nodes"][0]["identity_receipt"]["key_gid"] = 4242
        plan["plan_digest"] = self.adapter.canonical_plan_digest(plan)
        with self.assertRaises(self.adapter.AdapterError):
            self.adapter.validate_plan(plan)

    def test_trust_root_path_and_digest_are_code_owned(self) -> None:
        authority = self._authority()
        authority["trust_root_path"] = "/caller/selected/trust-root.json"
        authority["trust_root_digest"] = "a" * 64
        with self.assertRaises(self.adapter.AdapterError):
            self.adapter.validate_authority(self.plan, authority)

        authority = self._authority()
        authority["trust_root_file"]["owner_uid"] = os.getuid() + 1
        with self.assertRaises(self.adapter.AdapterError):
            self.adapter.validate_authority(self.plan, authority)

        authority = self._authority()
        authority["trust_root_file"]["mode"] = "0644"
        with self.assertRaises(self.adapter.AdapterError):
            self.adapter.validate_authority(self.plan, authority)

        authority = self._authority()
        authority["trust_root_file"]["root_digest"] = "a" * 64
        with self.assertRaises(self.adapter.AdapterError):
            self.adapter.validate_authority(self.plan, authority)

        authority = self._authority()
        authority["trust_root_file"]["owner_scope"] = "caller-selected"
        with self.assertRaises(self.adapter.AdapterError):
            self.adapter.validate_authority(self.plan, authority)

    def test_journal_execution_mode_and_ledger_path_are_exact_bindings(self) -> None:
        authority = self._authority()
        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / "journal.json"
            self.adapter.execute(
                self.plan,
                authority,
                journal_path=journal,
                ledger_path=self.ledger_path,
                dry_run=True,
            )
            with self.assertRaises(self.adapter.AdapterError):
                self.adapter.resume_transaction(
                    self.plan,
                    authority,
                    journal,
                    ledger_path=self.ledger_path,
                    dry_run=False,
                )
            record = json.loads(journal.read_text(encoding="utf-8"))
            record["execution_mode"] = "apply"
            record["journal_digest"] = self.adapter.journal_digest(record)
            journal.write_text(json.dumps(record) + "\n", encoding="utf-8")
            with self.assertRaises(self.adapter.AdapterError):
                self.adapter.resume_transaction(
                    self.plan,
                    authority,
                    journal,
                    ledger_path=self.ledger_path,
                    dry_run=True,
                )

            journal.chmod(0o644)
            with self.assertRaises(self.adapter.AdapterError):
                self.adapter.execute(
                    self.plan,
                    authority,
                    journal_path=journal,
                    ledger_path=self.ledger_path,
                    dry_run=True,
                )

            alternate_ledger = Path(directory) / "alternate.jsonl"
            self._write_ledger(alternate_ledger)
            with self.assertRaises(self.adapter.AdapterError):
                self.adapter.execute(
                    self.plan,
                    authority,
                    journal_path=Path(directory) / "alternate-journal.json",
                    ledger_path=alternate_ledger,
                    dry_run=True,
                )

    def test_consumer_impact_binding_covers_transport_authority_journal_and_receipts(self) -> None:
        impact_locator = {
            "path": self.plan["consumer_impact_record"]["path"],
            "sha256": self.plan["consumer_impact_record"]["sha256"],
        }
        self.assertEqual(
            self.adapter._transport_plan(self.plan)["consumer_impact_record"],
            self.plan["consumer_impact_record"],
        )
        authority = self._authority()
        self.adapter.validate_authority(self.plan, authority)
        self.assertEqual(
            authority["receipt"]["bindings"]["consumer_impact_record"], impact_locator
        )
        node = next(node for node in self.plan["nodes"] if node["name"] == "storage-205")
        receipt = ApplyTransport(self.adapter, self.plan)._receipt("preflight:storage-205", node)
        validated = self.adapter._validate_provider_receipt(
            self.plan, "preflight:storage-205", "storage-205", receipt, None
        )
        self.assertEqual(validated["bindings"]["consumer_impact_record"], impact_locator)
        journal = self.adapter._journal_record(self.plan, "dry-run-complete", 0, [])
        self.assertEqual(journal["consumer_impact_record"], impact_locator)

    def test_consumer_impact_change_fails_before_any_provider_callback(self) -> None:
        impact_path = self.fixture._impact_path
        impact_path.write_text(
            json.dumps({"impact": "active"}), encoding="utf-8"
        )
        transport = mock.Mock()
        authority = self._authority()
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(self.adapter.AdapterError):
                self.adapter.execute(
                    self.plan,
                    authority,
                    journal_path=Path(directory) / "journal.json",
                    ledger_path=self.ledger_path,
                    transport=transport,
                    dry_run=False,
                )
        transport.inspect_node.assert_not_called()
        transport.preflight.assert_not_called()
        transport.mutate.assert_not_called()

    def test_provider_receipt_without_consumer_impact_binding_is_rejected(self) -> None:
        node = next(node for node in self.plan["nodes"] if node["name"] == "storage-205")
        receipt = ApplyTransport(self.adapter, self.plan)._receipt("preflight:storage-205", node)
        del receipt["bindings"]["consumer_impact_record"]
        with self.assertRaises(self.adapter.AdapterError):
            self.adapter._validate_provider_receipt(
                self.plan, "preflight:storage-205", "storage-205", receipt, None
            )

    def test_nonce_ledger_symlinked_ancestor_is_rejected_before_read(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            real_parent = root / "real-parent"
            real_parent.mkdir()
            linked_parent = root / "linked-parent"
            linked_parent.symlink_to(real_parent, target_is_directory=True)
            ledger = linked_parent / "nonce.jsonl"
            self._write_ledger(ledger)
            with self.assertRaises(self.adapter.AdapterError):
                self.adapter.validate_credential_ledger(self.plan, ledger)


class JournalLedgerAliasTests(unittest.TestCase):
    def test_retained_evidence_alias_matrix_is_rejected_before_lock(self):
        adapter = load_module("retained_alias_adapter", ADAPTER_PATH)
        fields = (
            "raw_v1", "prepare_manifest", "payload", "provider_attestation",
            "unsigned_envelope", "signed_envelope", "verification",
        )
        locations = [("context", None), ("plan_intent", None)] + [
            (field, node) for node in range(5) for field in fields
        ] + [("consumer_impact_record", None)]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for index, (field, node) in enumerate(locations):
                for output_suffix in ("", ".lock", ".emergency.json"):
                    for alias_kind in ("direct", "hardlink", "symlink"):
                        case = root / f"{index}-{len(output_suffix)}-{alias_kind}"
                        case.mkdir()
                        journal = case / "journal.json"
                        output = Path(f"{journal}{output_suffix}")
                        retained = output if alias_kind == "direct" else case / "retained.json"
                        retained.write_bytes(b"retained evidence bytes\n")
                        if alias_kind == "hardlink":
                            os.link(retained, output)
                        elif alias_kind == "symlink":
                            output.symlink_to(retained)
                        descriptor = {"path": str(retained)}
                        evidence = {"entries": [{} for _ in range(5)]}
                        plan = {"identity_v2_evidence": evidence}
                        if field == "consumer_impact_record":
                            plan[field] = descriptor
                        elif node is None:
                            evidence[field] = descriptor
                        else:
                            evidence["entries"][node][field] = descriptor
                        for dry_run in (True, False):
                            for resume in (False, True):
                                with self.subTest(field=field, node=node, output=output_suffix, alias=alias_kind, dry_run=dry_run, resume=resume):
                                    with mock.patch.object(adapter, "_acquire_transaction_lock") as lock, mock.patch.object(adapter, "_release_transaction_lock"), mock.patch.object(adapter, "_execute_unlocked"), mock.patch.object(adapter, "_resume_transaction_unlocked"):
                                        with self.assertRaisesRegex(adapter.AdapterError, "alias"):
                                            if resume:
                                                adapter.resume_transaction(plan, {}, journal, ledger_path=case / "ledger", dry_run=dry_run)
                                            else:
                                                adapter.execute(plan, {}, journal_path=journal, ledger_path=case / "ledger", dry_run=dry_run)
                                        lock.assert_not_called()
                                    self.assertEqual(retained.read_bytes(), b"retained evidence bytes\n")

    def test_cli_input_aliases_are_rejected_before_execute(self):
        adapter = load_module("cli_input_alias_adapter", ADAPTER_PATH)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for input_name in ("plan", "authority", "evidence-map"):
                for suffix in ("", ".lock", ".emergency.json"):
                    case = root / f"{input_name}-{len(suffix)}"
                    case.mkdir()
                    journal = case / "journal.json"
                    paths = {name: case / f"{name}.json" for name in ("plan", "authority", "evidence-map")}
                    paths[input_name] = Path(f"{journal}{suffix}")
                    for path in paths.values():
                        path.write_bytes(b"{}")
                    argv = ["--plan", str(paths["plan"]), "--authority", str(paths["authority"]), "--journal", str(journal), "--ledger", str(case / "ledger")]
                    if input_name == "evidence-map":
                        argv += ["--identity-v2-evidence-map", str(paths[input_name]), "--identity-v2-mode", "current_admission"]
                    with self.subTest(input=input_name, suffix=suffix):
                        with mock.patch.object(adapter, "execute", return_value={}) as execute, mock.patch.object(adapter, "validate_authority"), mock.patch.object(adapter, "_current_identity_v2_admission", return_value={}):
                            with self.assertRaisesRegex(adapter.AdapterError, "alias"):
                                adapter.main(argv)
                            execute.assert_not_called()
                        for path in paths.values():
                            self.assertEqual(path.read_bytes(), b"{}")

    def test_lock_and_emergency_outputs_cannot_alias_ledger(self):
        adapter = load_module("alias_aux_adapter", ADAPTER_PATH)
        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / "journal.json"
            for suffix in (".lock", ".emergency.json"):
                ledger = Path(f"{journal}{suffix}")
                ledger.write_bytes(b"nonce history\n")
                with mock.patch.object(adapter, "_acquire_transaction_lock") as lock:
                    with self.assertRaises(adapter.AdapterError):
                        adapter.execute({}, {}, journal_path=journal, ledger_path=ledger)
                    lock.assert_not_called()
                self.assertEqual(ledger.read_bytes(), b"nonce history\n")

    def test_execute_and_resume_reject_journal_ledger_alias_before_lock(self):
        adapter = load_module("alias_adapter", ADAPTER_PATH)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            ledger = root / "nonce.jsonl"
            ledger.write_bytes(b"retained nonce history\n")
            aliases = [ledger, root / "hardlink", root / "symlink"]
            os.link(ledger, aliases[1])
            aliases[2].symlink_to(ledger)
            for journal in aliases:
                for dry_run in (True, False):
                    for resume in (False, True):
                        with self.subTest(journal=journal.name, dry_run=dry_run, resume=resume):
                            with mock.patch.object(adapter, "_acquire_transaction_lock") as lock, mock.patch.object(adapter, "_release_transaction_lock"), mock.patch.object(adapter, "_execute_unlocked"), mock.patch.object(adapter, "_resume_transaction_unlocked"):
                                with self.assertRaises(adapter.AdapterError):
                                    if resume:
                                        adapter.resume_transaction({}, {}, journal, ledger_path=ledger, dry_run=dry_run)
                                    else:
                                        adapter.execute({}, {}, journal_path=journal, ledger_path=ledger, dry_run=dry_run)
                                lock.assert_not_called()
                            self.assertEqual(ledger.read_bytes(), b"retained nonce history\n")


class TransactionGuardEffectTests(unittest.TestCase):
    def test_detected_drift_blocks_provider_rollback_and_emergency_effects(self):
        adapter = load_module("guard_effect_adapter", ADAPTER_PATH)
        for effect in ("provider", "rollback", "emergency"):
            with self.subTest(effect=effect), tempfile.TemporaryDirectory() as directory:
                journal = Path(directory) / "journal"
                guard = adapter._acquire_transaction_lock(journal)
                token = adapter._ACTIVE_TRANSACTION_GUARD.set(guard)
                callback = mock.Mock()
                try:
                    lock = Path(f"{journal}.lock")
                    lock.unlink()
                    lock.write_bytes(b"")
                    lock.chmod(0o600)
                    with self.assertRaises(adapter.AdapterError):
                        if effect == "emergency":
                            adapter._persist_terminal(journal, {"status": "terminal-failure"})
                        else:
                            adapter._guarded_callback(callback, {"operation": effect})
                    callback.assert_not_called()
                    self.assertFalse(journal.exists())
                    self.assertFalse(Path(f"{journal}.emergency.json").exists())
                finally:
                    adapter._ACTIVE_TRANSACTION_GUARD.reset(token)
                    adapter._release_transaction_lock(guard)

    def test_callback_replacement_is_detected_on_return(self):
        adapter = load_module("guard_return_adapter", ADAPTER_PATH)
        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / "journal"
            guard = adapter._acquire_transaction_lock(journal)
            token = adapter._ACTIVE_TRANSACTION_GUARD.set(guard)
            def callback():
                lock = Path(f"{journal}.lock")
                lock.unlink()
                lock.write_bytes(b"")
                lock.chmod(0o600)
                return {"verified": True}
            try:
                with self.assertRaises(adapter.AdapterError):
                    adapter._guarded_callback(callback)
            finally:
                adapter._ACTIVE_TRANSACTION_GUARD.reset(token)
                adapter._release_transaction_lock(guard)


class ReviewSixBoundaryTests(unittest.TestCase):
    def _anchors(self, adapter, root):
        planner = load_module("review_six_planner", PLANNER_PATH)
        adapter._PLANNER_MODULE = planner
        planner.IDENTITY_V2_PROVIDER_REGISTRY_PATH = root / "registry.json"
        planner.IDENTITY_V2_TRUST_CONFIG_PATH = root / "trust.json"
        artifacts = [root / name for name in ("key-a", "adapter-a", "key-b", "adapter-b", "verifier", "retired-key")]
        for path in artifacts:
            path.write_bytes(b"retained authority")
            path.chmod(0o600)
        registry = {"trust_config_path": str(planner.IDENTITY_V2_TRUST_CONFIG_PATH), "providers": [
            {"public_key_ref": str(artifacts[0]), "adapter_path": str(artifacts[1])},
            {"public_key_ref": str(artifacts[2]), "adapter_path": str(artifacts[3])},
        ], "verifier": {"executable_path": str(artifacts[4])}}
        trust = {"allowlist": [{"public_key_ref": str(path)} for path in (artifacts[0], artifacts[2], artifacts[5])]}
        planner.IDENTITY_V2_PROVIDER_REGISTRY_PATH.write_text(json.dumps(registry))
        planner.IDENTITY_V2_TRUST_CONFIG_PATH.write_text(json.dumps(trust))
        return planner, artifacts

    def test_registry_and_trust_reference_alias_closure(self):
        adapter = load_module("review_six_alias", ADAPTER_PATH)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            _, artifacts = self._anchors(adapter, root)
            for index, retained in enumerate(artifacts):
                for suffix in ("", ".lock", ".emergency.json"):
                    for kind in ("direct", "hardlink", "symlink"):
                        with self.subTest(reference=index, suffix=suffix, alias=kind):
                            journal = root / f"output-{index}-{len(suffix)}-{kind}"
                            output = Path(f"{journal}{suffix}")
                            if kind == "direct":
                                # Change only the declared reference, not its meaning.
                                retained.rename(output)
                                for anchor in (adapter._PLANNER_MODULE.IDENTITY_V2_PROVIDER_REGISTRY_PATH, adapter._PLANNER_MODULE.IDENTITY_V2_TRUST_CONFIG_PATH):
                                    anchor.write_text(anchor.read_text().replace(str(retained), str(output)))
                            elif kind == "hardlink":
                                os.link(retained, output)
                            else:
                                output.symlink_to(retained)
                            snapshot = (output.read_bytes(), output.stat().st_mode)
                            try:
                                with self.assertRaisesRegex(adapter.AdapterError, "alias"):
                                    adapter._reject_journal_input_aliases(journal, root / "ledger", {})
                            finally:
                                self.assertEqual((output.read_bytes(), output.stat().st_mode), snapshot)
                                if kind == "direct":
                                    output.rename(retained)
                                    for anchor in (adapter._PLANNER_MODULE.IDENTITY_V2_PROVIDER_REGISTRY_PATH, adapter._PLANNER_MODULE.IDENTITY_V2_TRUST_CONFIG_PATH):
                                        anchor.write_text(anchor.read_text().replace(str(output), str(retained)))

    def test_authority_anchor_read_failure_blocks_before_lock(self):
        adapter = load_module("review_six_unreadable", ADAPTER_PATH)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            planner, _ = self._anchors(adapter, root)
            for field in ("IDENTITY_V2_PROVIDER_REGISTRY_PATH", "IDENTITY_V2_TRUST_CONFIG_PATH"):
                path = getattr(planner, field)
                original = path.read_bytes()
                for failure in ("malformed", "missing"):
                    with self.subTest(anchor=field, failure=failure):
                        if failure == "malformed":
                            path.write_bytes(b"not json")
                        else:
                            path.unlink()
                        try:
                            with mock.patch.object(adapter, "_acquire_transaction_lock") as acquire, mock.patch.object(adapter, "_release_transaction_lock"), mock.patch.object(adapter, "_execute_unlocked"):
                                with self.assertRaises(adapter.AdapterError):
                                    adapter.execute({}, {}, journal_path=root / "journal", ledger_path=root / "ledger")
                                acquire.assert_not_called()
                        finally:
                            path.write_bytes(original)

    def test_registry_key_api_alias_fails_before_lock_and_preserves_mode(self):
        adapter = load_module("review_six_api_alias", ADAPTER_PATH)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            _, artifacts = self._anchors(adapter, root)
            for resume in (False, True):
                for dry_run in (False, True):
                    with self.subTest(resume=resume, dry_run=dry_run):
                        key = artifacts[2]
                        before = (key.read_bytes(), key.stat().st_mode)
                        try:
                            with mock.patch.object(adapter, "_acquire_transaction_lock") as acquire, mock.patch.object(adapter, "_release_transaction_lock"), mock.patch.object(adapter, "_execute_unlocked"), mock.patch.object(adapter, "_resume_transaction_unlocked"):
                                with self.assertRaisesRegex(adapter.AdapterError, "alias"):
                                    if resume:
                                        adapter.resume_transaction({}, {}, key, ledger_path=root / "ledger", dry_run=dry_run)
                                    else:
                                        adapter.execute({}, {}, journal_path=key, ledger_path=root / "ledger", dry_run=dry_run)
                                acquire.assert_not_called()
                        finally:
                            self.assertEqual((key.read_bytes(), key.stat().st_mode), before)

    def test_transaction_lock_open_and_flock_identity_races(self):
        import fcntl
        adapter = load_module("review_six_lock", ADAPTER_PATH)
        real_open, real_flock = os.open, fcntl.flock
        for boundary in ("open", "flock"):
            for target in ("lock", "parent"):
                with self.subTest(boundary=boundary, target=target), tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    parent = root / "parent"
                    parent.mkdir(mode=0o700)
                    journal = parent / "journal"
                    lock = Path(f"{journal}.lock")
                    swapped = False
                    def replace():
                        nonlocal swapped
                        if swapped:
                            return
                        swapped = True
                        if target == "parent":
                            parent.rename(root / "old-parent")
                            parent.mkdir(mode=0o700)
                        else:
                            lock.unlink()
                        lock.write_bytes(b"")
                        lock.chmod(0o600)
                    def opening(path, flags, *args, **kwargs):
                        fd = real_open(path, flags, *args, **kwargs)
                        if boundary == "open" and Path(path) == lock:
                            replace()
                        return fd
                    def flocking(fd, operation):
                        result = real_flock(fd, operation)
                        if boundary == "flock" and operation & fcntl.LOCK_EX:
                            replace()
                        return result
                    handle = None
                    try:
                        with mock.patch.object(os, "open", side_effect=opening), mock.patch.object(fcntl, "flock", side_effect=flocking):
                            with self.assertRaises(adapter.AdapterError):
                                handle = adapter._acquire_transaction_lock(journal)
                        self.assertTrue(swapped)
                    finally:
                        if handle is not None:
                            adapter._release_transaction_lock(handle)

    def test_transaction_lock_rejects_writable_parent(self):
        adapter = load_module("review_six_parent", ADAPTER_PATH)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            root.chmod(0o777)
            handle = None
            try:
                with self.assertRaises(adapter.AdapterError):
                    handle = adapter._acquire_transaction_lock(root / "journal")
            finally:
                if handle is not None:
                    adapter._release_transaction_lock(handle)

    def test_held_lock_drift_blocks_next_journal_effect(self):
        adapter = load_module("review_six_lifetime", ADAPTER_PATH)
        for resume in (False, True):
            with self.subTest(resume=resume), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                journal = root / "journal"
                journal.write_bytes(b"retained journal")
                journal.chmod(0o600)
                def unlocked(*args, **kwargs):
                    lock = Path(f"{journal}.lock")
                    # An unchanged persistent inode must continue to exclude a
                    # second holder. Replacement must not make the first safe.
                    with self.assertRaisesRegex(adapter.AdapterError, "locked"):
                        adapter._acquire_transaction_lock(journal)
                    lock.unlink()
                    lock.write_bytes(b"")
                    lock.chmod(0o600)
                    adapter._write_journal(journal, {"status": "must-not-persist"})
                with mock.patch.object(adapter, "_reject_journal_input_aliases"), mock.patch.object(adapter, "_execute_unlocked", side_effect=unlocked), mock.patch.object(adapter, "_resume_transaction_unlocked", side_effect=unlocked):
                    with self.assertRaises(adapter.AdapterError):
                        if resume:
                            adapter.resume_transaction({}, {}, journal, ledger_path=root / "ledger")
                        else:
                            adapter.execute({}, {}, journal_path=journal, ledger_path=root / "ledger")
                self.assertEqual(journal.read_bytes(), b"retained journal")


class ReviewFiveBoundaryTests(unittest.TestCase):
    def test_nonce_replacement_after_lock_cannot_report_reservation(self):
        import fcntl
        adapter = load_module("nonce_race_adapter", ADAPTER_PATH)
        real_flock = fcntl.flock
        with tempfile.TemporaryDirectory() as directory:
            ledger = Path(directory) / "ledger"
            replacement = Path(directory) / "replacement"
            for path in (ledger, replacement):
                path.write_text("")
                path.chmod(0o600)
            def replace_after_lock(fd, operation):
                real_flock(fd, operation)
                if operation == fcntl.LOCK_EX:
                    os.replace(replacement, ledger)
            with mock.patch.object(fcntl, "flock", side_effect=replace_after_lock):
                with self.assertRaises(adapter.AdapterError):
                    adapter.reserve_nonce(ledger, "transaction-review-five", "nonce-review-five")
            self.assertEqual(ledger.read_bytes(), b"")

    def test_identity_v2_anchors_are_protected_from_all_journal_outputs(self):
        adapter = load_module("anchor_alias_adapter", ADAPTER_PATH)
        planner = load_module("anchor_alias_planner", PLANNER_PATH)
        adapter._PLANNER_MODULE = planner
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for field in ("IDENTITY_V2_TRUST_CONFIG_PATH", "IDENTITY_V2_PROVIDER_REGISTRY_PATH"):
                for suffix in ("", ".lock", ".emergency.json"):
                    with self.subTest(field=field, suffix=suffix):
                        journal = root / "journal"
                        anchor = Path(f"{journal}{suffix}")
                        anchor.write_bytes(b"authority anchor")
                        anchor.chmod(0o600)
                        with mock.patch.object(planner, field, anchor):
                            with self.assertRaises(adapter.AdapterError):
                                adapter._reject_journal_input_aliases(journal, root / "ledger", {})
                        self.assertEqual(anchor.read_bytes(), b"authority anchor")

    def test_adapter_fixture_module_is_initialized_once_across_setups(self):
        # Stop before crypto setup: count the real module lifecycle boundary,
        # not subprocess timing. Fresh fixture instances remain per-test.
        class StopSetup(Exception):
            pass
        modules = []
        instances = []
        real_load = load_module
        def tracked_load(name, path):
            module = real_load(name, path)
            if path == PLANNER_TEST_PATH:
                modules.append(module)
                def setup(instance):
                    instances.append(instance)
                    instance.mutable_probe = []
                    raise StopSetup()
                module.FullNetworkCleanRoomPlanTests.setUp = setup
            return module
        with mock.patch.dict(globals(), {"load_module": tracked_load}):
            FullNetworkCleanRoomAdapterTests.setUpClass()
            try:
                for _ in range(2):
                    case = FullNetworkCleanRoomAdapterTests("runTest")
                    with self.assertRaises(StopSetup):
                        case.setUp()
                instances[0].mutable_probe.append("first-test-only")
                self.assertEqual(instances[1].mutable_probe, [])
                self.assertIsNot(instances[0], instances[1])
                self.assertEqual(len(modules), 1, "planner crypto baseline module reloaded per test")
            finally:
                FullNetworkCleanRoomAdapterTests.tearDownClass()


class FleetLockPublisherProtectionTests(unittest.TestCase):
    """Repository publishers must not invalidate another callback's fleet lock."""

    def _exercise_publisher(self, publisher):
        for alias in ("exact", "normalized", "hardlink"):
            with self.subTest(publisher=publisher, alias=alias), tempfile.TemporaryDirectory() as directory:
                root = Path(directory).resolve()
                adapter = load_module("publisher_lock_adapter", ADAPTER_PATH)
                planner = load_module("publisher_lock_planner", PLANNER_PATH)
                signing = load_module("publisher_lock_signing", ROOT / "scripts/p2p-public-testnet-identity-v2-signing-tool.py")
                sidecar = load_module("publisher_lock_sidecar", ROOT / "scripts/p2p-public-testnet-identity-receipt-v2.py")
                aggregate = load_module("publisher_lock_aggregate", ROOT / "scripts/p2p-public-testnet-identity-v2-evidence-aggregate.py")
                lock_path = root / "full-network-clean-room.lock"
                modules = (adapter, adapter._load_planner()._peer_registry_authority(),
                           planner._peer_registry_authority(), signing._peer_registry_authority(),
                           aggregate.PLANNER._peer_registry_authority())
                with ExitStack() as patches:
                    # Relocate only code-owned fixture authority, never expose a CLI override.
                    for module in modules:
                        patches.enter_context(mock.patch.object(module, "CANONICAL_FLEET_LOCK_PATH", str(lock_path)))
                    patches.enter_context(mock.patch.object(sidecar, "_load_planner", return_value=planner))

                    def publish(output):
                        if publisher == "planner":
                            protected = planner._plan_output_inputs(root / "input", root / "map", {})
                            planner._write_plan_atomic(output, {"fixture": True}, protected)
                        elif publisher == "signing":
                            signing._reject_output_aliases([(output, "verified output")], [])
                            signing._atomic_write(output, b'{"fixture": true}', "verified output")
                        elif publisher == "sidecar":
                            sidecar._reject_output_aliases([(output, "evidence-map output")], [])
                            sidecar._write_atomically(output, {"fixture": True})
                        else:
                            aggregate._write_atomic({"fixture": True}, output, retained_paths=[])

                    first = adapter._acquire_fleet_transaction_guard(root / "first.json")
                    token = adapter._ACTIVE_TRANSACTION_GUARD.set(first)
                    try:
                        # Positive control must succeed while a fleet callback is active.
                        ordinary = root / "ordinary.json"
                        publish(ordinary)
                        self.assertEqual(json.loads(ordinary.read_text()), {"fixture": True})
                        output = lock_path
                        if alias == "normalized":
                            (root / "nested").mkdir()
                            output = root / "nested" / ".." / lock_path.name
                        elif alias == "hardlink":
                            output = root / "lock-hardlink"
                            os.link(lock_path, output)
                        original = (lock_path.read_bytes(), lock_path.stat().st_ino)
                        output_original = (output.read_bytes(), output.stat().st_ino)
                        observed = {}

                        def callback():
                            try:
                                publish(output)
                            except (SystemExit, signing.ToolError, adapter.AdapterError):
                                observed["publisher_rejected"] = True
                            else:
                                observed["publisher_rejected"] = False
                            observed["lock_preserved"] = (lock_path.read_bytes(), lock_path.stat().st_ino) == original
                            observed["output_preserved"] = (output.read_bytes(), output.stat().st_ino) == output_original
                            try:
                                second = adapter._acquire_fleet_transaction_guard(root / "second.json")
                            except adapter.AdapterError:
                                observed["second_lock_blocked"] = True
                            else:
                                observed["second_lock_blocked"] = False
                                second.close()

                        try:
                            adapter._guarded_callback(callback)
                        except adapter.AdapterError:
                            observed["post_callback_guard_failed"] = True
                        else:
                            observed["post_callback_guard_failed"] = False
                        self.assertEqual(observed, {
                            "publisher_rejected": True, "lock_preserved": True,
                            "output_preserved": True, "second_lock_blocked": True,
                            "post_callback_guard_failed": False,
                        }, "publisher must reject before replacement, not detect drift after the callback")
                    finally:
                        adapter._ACTIVE_TRANSACTION_GUARD.reset(token)
                        first.close()

    def test_planner_cannot_replace_held_fleet_lock(self):
        self._exercise_publisher("planner")

    def test_signing_cannot_replace_held_fleet_lock(self):
        self._exercise_publisher("signing")

    def test_sidecar_cannot_replace_held_fleet_lock(self):
        self._exercise_publisher("sidecar")

    def test_aggregate_cannot_replace_held_fleet_lock(self):
        self._exercise_publisher("aggregate")


class StorageFirstAdapterRedTests(unittest.TestCase):
    """RED contract for storage-205-first apply/resume boundaries.

    The recording transport is an in-process test double.  It never opens a
    socket or reads a credential; its only purpose is to expose an accidental
    sequencer/observer callback or unsafe replay to the test.
    """

    @classmethod
    def setUpClass(cls) -> None:
        cls.adapter = load_module("storage_first_adapter_under_test", ADAPTER_PATH)

    def setUp(self) -> None:
        self._directory = tempfile.TemporaryDirectory()
        self.root = Path(self._directory.name)
        self.plan = self._plan()
        self.identity_map = copy.deepcopy(self.plan["identity_v2_evidence"])

    def tearDown(self) -> None:
        self._directory.cleanup()

    def _plan(self) -> dict[str, object]:
        node_order = [
            "storage-205", "sequencer-204", "linux-lan-observer",
            "windows-observer", "macos-observer",
        ]
        return {
            "schema_version": "oasis7.public_testnet_full_network_clean_room_plan.v1",
            "task_uid": "task_90c2722f6e1c48c3aebb6283cee471ce",
            "head_oid": "9" * 40,
            "plan_digest": "q" * 64,
            "transaction_id": "txn-storage-first-red",
            "capture_window_id": "capture-storage-first-red",
            "node_order": node_order,
            "global_order": [
                *(f"preflight:{name}" for name in node_order),
                "stop:storage-205", "delete:storage-205", "rebuild:storage-205",
                "start:storage-205", "verify:storage-205", "fresh-root-probe", "fleet-health",
            ],
            "nodes": [{
                "name": name,
                "role": "validator" if name in {"storage-205", "sequencer-204"} else "observer",
                "host_binding": {
                    "known_hosts_path": "/operator/known-hosts",
                    "known_host_fingerprint": f"fingerprint:{name}",
                },
                "endpoints": {
                    "evidence": "http://sequencer/v1/chain/rebuild-proof"
                    if name == "sequencer-204" else f"http://{name}/v1/chain/status",
                },
            } for name in node_order],
            "identity_v2_evidence": {
                "digest": "i" * 64,
                "mode": "current_admission",
                "entries": [{"node_name": name} for name in node_order],
            },
            "authority": {
                "action": "full-network-clean-room",
                "targets": node_order,
                "digest": "a" * 64,
            },
            "forensic_backup": {
                "action": "full-network-clean-room",
                "targets": node_order,
            },
            "credential_nonce_ledger": {
                "path": "/operator/nonce-ledger.jsonl",
                "count": 5,
                "reservations": [{"node": name} for name in node_order],
            },
            "known_hosts_digest": "k" * 64,
            "consumer_impact_record": {"sha256": "c" * 64, "decision": "proceed"},
            "package_provenance_digest": "p" * 64,
            "deployment_inventory_digest": "d" * 64,
            "independent_verifier": {
                "verifier_id": "governed-receipt-verifier",
                "trust_root_id": "oasis7-public-testnet-governance-root-v1",
            },
        }

    def _authority(self) -> dict[str, object]:
        return {
            "action": "storage-205-first",
            "targets": ["storage-205"],
            "task_uid": self.plan["task_uid"],
            "frozen_head_oid": self.plan["head_oid"],
            "plan_digest": self.plan["plan_digest"],
            "transaction_id": self.plan["transaction_id"],
            "capture_window_id": self.plan["capture_window_id"],
            "current_authorization": True,
            "signed": True,
            "expires_at": "2099-01-01T00:00:00Z",
        }

    class _Transport:
        def __init__(self, side_effect_operation: str | None = None) -> None:
            self.side_effect_operation = side_effect_operation
            self.plan: dict[str, object] | None = None
            self.calls: list[str] = []
            self.mutations: list[str] = []
            self.rollback_candidates: list[str] = []

        def _receipt(self, operation: str) -> dict[str, object]:
            assert self.plan is not None, "test transport must be bound to the plan"
            return {
                "schema_version": "oasis7.storage_first_receipt.v1",
                "phase_id": "storage-205-first",
                "operation": operation,
                "target": "storage-205",
                "observer_mutation": False,
                "completion_boundary": "storage-205-verified-pending-sequencer-probe",
                "authenticated": True,
                "verified": True,
                "signer_id": "governance-signer",
                "verifier_id": "governed-receipt-verifier",
                "trust_root_id": "oasis7-public-testnet-governance-root-v1",
                "transaction_id": self.plan["transaction_id"],
                "capture_window_id": self.plan["capture_window_id"],
                "bindings": {
                    "phase_id": "storage-205-first",
                    "operation": operation,
                    "target": "storage-205",
                    "transaction_id": self.plan["transaction_id"],
                    "capture_window_id": self.plan["capture_window_id"],
                    "plan_digest": self.plan["plan_digest"],
                },
            }

        def inspect_node(self, node):
            self.calls.append(f"inspect:{node['name']}")
            return {"node": node["name"], "known_hosts_verified": True}

        def preflight(self, operation, node):
            self.calls.append(operation)
            return {"operation": operation, "verified": True}

        def verify(self, operation, node):
            self.calls.append(operation)
            return self._receipt(operation)

        def mutate(self, operation, node):
            self.calls.append(operation)
            self.mutations.append(operation)
            self.rollback_candidates.append(operation)
            if operation == self.side_effect_operation:
                raise RuntimeError("provider side effect then throw")
            return self._receipt(operation)

        def reobserve_failed_state(self, plan, started, failed_operation):
            self.calls.append("reobserve-failed-state")
            return {"failed_operation": failed_operation, "rollback_candidates": list(started)}

        def rollback_clean_redeploy(self, plan, started, failed_state=None):
            self.calls.append("rollback-clean-redeploy")
            return {"rollback_candidates": list(started), "policy": "clean-redeploy"}

    def _runner(self, transport, **overrides):
        runner = getattr(self.adapter, "execute_storage_first", None)
        self.assertTrue(callable(runner), "RED: missing storage-first adapter execute API")
        transport.plan = self.plan
        kwargs = {
            "phase": "storage-205-first",
            "identity_v2_evidence": self.identity_map,
            "journal_path": self.root / "storage-first.journal.json",
            "ledger_path": self.root / "parent-nonce-ledger.jsonl",
            "transport": transport,
            "dry_run": False,
            "provenance_verifier": self._bound_provenance,
        }
        kwargs.update(overrides)
        return runner(self.plan, self._authority(), **kwargs)

    def _resume(self, journal_path, **overrides):
        resumer = getattr(self.adapter, "resume_storage_first", None)
        self.assertTrue(callable(resumer), "RED: missing storage-first adapter resume API")
        transport = self._Transport()
        transport.plan = self.plan
        kwargs = {
            "phase": "storage-205-first",
            "identity_v2_evidence": self.identity_map,
            "journal_path": journal_path,
            "ledger_path": self.root / "parent-nonce-ledger.jsonl",
            "transport": transport,
            "provenance_verifier": self._bound_provenance,
        }
        kwargs.update(overrides)
        return resumer(self.plan, self._authority(), **kwargs)

    @staticmethod
    def _bound_provenance(plan, receipt):
        return {
            "verified": True,
            "bindings": {
                "phase_id": "storage-205-first",
                "transaction_id": plan["transaction_id"],
                "capture_window_id": plan["capture_window_id"],
                "plan_digest": plan["plan_digest"],
                "target": "storage-205",
            },
        }

    @classmethod
    def _ensure_canonical_fixture_support(cls) -> None:
        """Load the planner-backed signed fixture lazily for migrated tests."""
        base = StorageFirstAdapterRedTests
        if hasattr(base, "_canonical_fixture_module"):
            return
        base._canonical_fixture_module = _shared_planner_fixture_module()

    def _canonical_fixture(self):
        """Return an isolated canonical plan/authority/trust-root fixture."""
        self._ensure_canonical_fixture_support()
        fixture = FullNetworkCleanRoomAdapterTests("runTest")
        fixture.fixture_module = StorageFirstAdapterRedTests._canonical_fixture_module
        fixture.setUp()
        fixture.root = Path(fixture._test_directory.name)
        fixture._write_ledger(
            fixture.ledger_path,
            [{
                "schema_version": fixture.adapter.NONCE_ROW_SCHEMA,
                "transaction_id": "fixture-existing-transaction",
                "nonce": "fixture-existing-nonce",
                "one_shot": True,
            }],
        )
        fixture.plan = copy.deepcopy(dict(fixture.plan))
        fixture.identity_v2_evidence = _plain_storage_first_admission_evidence(
            fixture.identity_v2_evidence
        )
        return fixture

    def _canonical_runner(
        self,
        fixture,
        transport,
        *,
        plan: dict[str, object] | None = None,
        journal_path: Path | None = None,
        ledger_path: Path | None = None,
        **overrides,
    ):
        plan = plan or fixture.plan
        transport.plan = plan

        def canonical_provenance_verifier(verifier_plan, receipt):
            if "bindings" in receipt:
                return fixture._recovery_verifier(verifier_plan, receipt)
            return {
                "verified": True,
                "bindings": receipt,
                "verifier_id": fixture.adapter.CANONICAL_VERIFIER_ID,
                "trust_root_id": fixture.adapter.CANONICAL_TRUST_ROOT_ID,
                "signer_id": "governance-signer",
            }

        kwargs = {
            "phase": "storage-205-first",
            "identity_v2_evidence": fixture.identity_v2_evidence,
            "journal_path": journal_path or Path(fixture._test_directory.name) / "storage-first.journal.json",
            "ledger_path": ledger_path or fixture.ledger_path,
            "transport": transport,
            "dry_run": False,
            "provenance_verifier": canonical_provenance_verifier,
            "live_revalidator": lambda: True,
        }
        kwargs.update(overrides)
        return fixture.adapter.execute_storage_first(
            plan,
            StorageFirstCanonicalAuthority(fixture._authority(True, plan), plan),
            **kwargs,
        )

    def _canonical_prefix_journal(
        self,
        fixture,
        completed: list[str],
        *,
        status: str = "storage-205-running",
        name: str = "canonical-prefix.journal.json",
    ) -> Path:
        """Write a protected canonical journal preserving a receipt prefix."""
        transport = StorageFirstCanonicalTransport(fixture.adapter, fixture.plan)
        nonce_state = fixture.adapter._reconcile_nonce_reservations(
            dict(fixture.plan), fixture.ledger_path
        )
        receipts = [
            fixture.adapter._storage_first_bind_receipt(
                fixture.plan,
                operation,
                transport._receipt(operation, transport._storage_node()),
                fixture._recovery_verifier,
            )
            for operation in completed
        ]
        record: dict[str, object] = {
            "schema_version": fixture.adapter.STORAGE_FIRST_JOURNAL_SCHEMA,
            "phase_id": fixture.adapter.STORAGE_FIRST_PHASE_ID,
            "status": status,
            "next_operation": (
                fixture.adapter.STORAGE_FIRST_OPERATIONS[len(completed)]
                if len(completed) < len(fixture.adapter.STORAGE_FIRST_OPERATIONS)
                else "reconciliation-required"
            ),
            "completed_operations": list(completed),
            "task_uid": fixture.plan["task_uid"],
            "head_oid": fixture.plan["head_oid"],
            "plan_digest": fixture.plan["plan_digest"],
            "transaction_id": fixture.plan["transaction_id"],
            "capture_window_id": fixture.plan["capture_window_id"],
            "phase_contract_digest": fixture.adapter._storage_first_phase_digest(
                fixture.plan, fixture.identity_v2_evidence
            ),
            "ledger_path": str(fixture.ledger_path),
            "callback_started": False,
            "callback_receipt": receipts[-1] if receipts else None,
            "storage_receipts": receipts,
            "receipt_operation_cursor": list(completed),
            "rollback_candidates": [
                operation
                for operation in completed
                if fixture.adapter._rollback_candidate(operation)
            ],
            "rollback_status": "not-started",
            "nonce_reservation_state": nonce_state,
        }
        record["journal_digest"] = fixture.adapter.journal_digest(record)
        path = fixture.root / name
        path.write_text(json.dumps(record, sort_keys=True), encoding="utf-8")
        path.chmod(0o600)
        return path

    def _canonical_resume(self, fixture, journal_path: Path, transport, **overrides):
        def canonical_provenance_verifier(verifier_plan, receipt):
            if "bindings" in receipt:
                return fixture._recovery_verifier(verifier_plan, receipt)
            return {
                "verified": True,
                "bindings": receipt,
                "verifier_id": fixture.adapter.CANONICAL_VERIFIER_ID,
                "trust_root_id": fixture.adapter.CANONICAL_TRUST_ROOT_ID,
                "signer_id": "governance-signer",
            }

        kwargs = {
            "phase": fixture.adapter.STORAGE_FIRST_PHASE_ID,
            "identity_v2_evidence": fixture.identity_v2_evidence,
            "journal_path": journal_path,
            "ledger_path": fixture.ledger_path,
            "transport": transport,
            "dry_run": False,
            "provenance_verifier": canonical_provenance_verifier,
            "live_revalidator": lambda: True,
        }
        kwargs.update(overrides)
        return fixture.adapter.resume_storage_first(
            fixture.plan,
            StorageFirstCanonicalAuthority(
                fixture._authority(True, fixture.plan), fixture.plan
            ),
            **kwargs,
        )

    def test_storage_first_apply_calls_only_storage_callbacks(self):
        fixture = self._canonical_fixture()
        try:
            transport = StorageFirstCanonicalTransport(fixture.adapter, fixture.plan)
            self._canonical_runner(fixture, transport)
        finally:
            fixture.tearDown()
        self.assertEqual(transport.mutations, [
            "stop:storage-205", "delete:storage-205", "rebuild:storage-205",
            "start:storage-205",
        ])
        self.assertEqual(transport.verify_operations, ["verify:storage-205"])
        unexpected_fleet_callbacks = [
            call for call in transport.operations
            if ("sequencer" in call or "observer" in call)
            and call not in {"bounded-proof:sequencer-204", "preflight:sequencer-204"}
        ]
        self.assertEqual(unexpected_fleet_callbacks, [])
        self.assertIn("bounded-proof:sequencer-204", transport.operations)
        self.assertNotIn("fresh-root-probe", transport.operations)
        self.assertNotIn("fleet-health", transport.operations)

    def test_storage_first_success_persists_a_valid_receipt_cursor(self):
        """A successful run must emit a journal that its validator accepts."""
        fixture = self._canonical_fixture()
        journal = fixture.root / "successful-storage-first.journal.json"
        try:
            transport = StorageFirstCanonicalTransport(fixture.adapter, fixture.plan)
            self._canonical_runner(fixture, transport, journal_path=journal)
            self.assertTrue(journal.exists())
            record = json.loads(journal.read_text(encoding="utf-8"))
            self.assertEqual(
                record["receipt_operation_cursor"], record["completed_operations"]
            )
            self.assertTrue(fixture.adapter.validate_storage_first_journal(record))
        finally:
            fixture.tearDown()

    def test_storage_first_apply_fails_before_lock_without_phase_and_current_map(self):
        self.assertTrue(
            callable(getattr(self.adapter, "execute_storage_first", None)),
            "RED: missing storage-first adapter execute API",
        )
        for missing in ("phase", "identity_v2_evidence"):
            with self.subTest(missing=missing):
                transport = self._Transport()
                kwargs = {missing: None}
                with self.assertRaises(Exception):
                    self._runner(transport, **kwargs)
                self.assertEqual(transport.calls, [])
                self.assertFalse((self.root / "storage-first.journal.json").exists())

    def test_storage_first_resume_revalidates_live_authority_before_each_mutation(self):
        fixture = self._canonical_fixture()
        checks = []
        try:
            transport = StorageFirstCanonicalTransport(fixture.adapter, fixture.plan)
            self._canonical_runner(
                fixture, transport,
                live_revalidator=lambda: (checks.append("live") or True),
            )
        finally:
            fixture.tearDown()
        # Revalidation runs before admission and again after the durable
        # callback-started checkpoint, immediately before provider execution.
        self.assertEqual(
            len(checks),
            2 * (len(transport.mutations) + len(transport.verify_operations)),
        )

    def test_storage_first_resume_does_not_reuse_persisted_receipt_as_authority(self):
        self.assertTrue(
            callable(getattr(self.adapter, "resume_storage_first", None)),
            "RED: missing storage-first adapter resume API",
        )
        journal = self.root / "persisted-receipt-only.json"
        journal.write_text(json.dumps({
            "schema_version": "oasis7.storage_first_mutation_journal.v1",
            "phase_id": "storage-205-first",
            "status": "preflight-complete",
            "storage_receipts": [{"verified": True}],
        }))
        with self.assertRaises(Exception):
            self._resume(journal, authority=None)

    def test_storage_first_journal_states_and_cursor_are_closed(self):
        validator = getattr(self.adapter, "validate_storage_first_journal", None)
        self.assertTrue(callable(validator), "RED: missing storage-first journal validator API")
        fixture = self._canonical_fixture()
        try:
            transport = StorageFirstCanonicalTransport(fixture.adapter, fixture.plan)
            receipts = [
                fixture.adapter._storage_first_bind_receipt(
                    fixture.plan,
                    operation,
                    transport._receipt(operation, transport._storage_node()),
                    fixture._recovery_verifier,
                )
                for operation in ("stop:storage-205", "delete:storage-205")
            ]
            valid = {
                "schema_version": "oasis7.storage_first_mutation_journal.v1",
                "phase_id": "storage-205-first",
                "status": "storage-205-running",
                "next_operation": "rebuild:storage-205",
                "completed_operations": ["stop:storage-205", "delete:storage-205"],
                "task_uid": fixture.plan["task_uid"],
                "head_oid": fixture.plan["head_oid"],
                "plan_digest": fixture.plan["plan_digest"],
                "transaction_id": fixture.plan["transaction_id"],
                "capture_window_id": fixture.plan["capture_window_id"],
                "phase_contract_digest": fixture.adapter._storage_first_phase_digest(
                    fixture.plan, fixture.identity_v2_evidence
                ),
                "ledger_path": str(fixture.ledger_path),
                "callback_started": False,
                "callback_receipt": receipts[-1],
                "storage_receipts": receipts,
                "receipt_operation_cursor": ["stop:storage-205", "delete:storage-205"],
                "rollback_candidates": ["stop:storage-205", "delete:storage-205"],
                "rollback_status": "not-started",
            }
            valid["journal_digest"] = fixture.adapter.journal_digest(valid)
            journal_path = fixture.root / "valid-running.journal.json"
            journal_path.write_text(json.dumps(valid, sort_keys=True), encoding="utf-8")
            journal_path.chmod(0o600)
            self.assertEqual(journal_path.stat().st_mode & 0o777, 0o600)
            with self.assertRaisesRegex(Exception, "unfinished|reconciliation"):
                validator(json.loads(journal_path.read_text(encoding="utf-8")))
            for mutation in (
                {"status": "unknown_status"},
                {"next_operation": "stop:sequencer-204"},
                {"status": "full-network-complete"},
            ):
                changed = copy.deepcopy(valid)
                changed.update(mutation)
                with self.subTest(mutation=mutation), self.assertRaises(Exception):
                    validator(changed)
        finally:
            fixture.tearDown()

    def test_storage_first_side_effect_then_throw_requires_reconciliation(self):
        fixture = self._canonical_fixture()
        try:
            fixture.plan = copy.deepcopy(dict(fixture.plan))
            transport = StorageFirstCanonicalTransport(
                fixture.adapter,
                fixture.plan,
                side_effect_operation="delete:storage-205",
            )
            journal = fixture.root / "side-effect.journal.json"
            original_validate = fixture.adapter._validate_rollback_candidates

            def child_rollback_candidates(plan, candidates, *, order=None):
                if list(candidates) == ["stop:storage-205", "delete:storage-205"]:
                    return list(candidates)
                return original_validate(plan, candidates, order=order)

            with mock.patch.object(
                fixture.adapter,
                "_validate_rollback_candidates",
                side_effect=child_rollback_candidates,
            ):
                with self.assertRaises(Exception):
                    self._canonical_runner(
                        fixture, transport, journal_path=journal
                    )
            self.assertTrue(journal.exists())
            record = json.loads(journal.read_text())
            self.assertEqual(record["failed_operation"], "delete:storage-205")
            self.assertEqual(record["rollback_status"], "reconciliation-blocked")
            self.assertEqual(record["next_operation"], "reconciliation-required")
        finally:
            fixture.tearDown()

    def test_storage_first_receipts_forbid_secret_fields_and_false_closure(self):
        validator = getattr(self.adapter, "validate_storage_first_receipt", None)
        self.assertTrue(callable(validator), "RED: missing storage-first receipt validator API")
        receipt = {
            "schema_version": "oasis7.storage_first_receipt.v1",
            "phase_id": "storage-205-first",
            "operation": "verify:storage-205",
            "target": "storage-205",
            "observer_mutation": False,
            "completion_boundary": "storage-205-verified-pending-sequencer-probe",
        }
        self.assertTrue(validator(receipt))
        for mutation in (
            {"credential": "not-allowed"},
            {"nonce": "not-allowed"},
            {"completion_boundary": "full-network-complete"},
            {"fresh_root_probe": True},
            {"fleet_health": True},
        ):
            changed = copy.deepcopy(receipt)
            changed.update(mutation)
            with self.subTest(mutation=mutation), self.assertRaises(Exception):
                validator(changed)


class StorageFirstSecurityRedTests(unittest.TestCase):
    """Finding-specific RED coverage for the five QA release blockers.

    Every test uses the existing in-process transport double.  No callback can
    open a socket, read credentials, or mutate a real provider.
    """

    @classmethod
    def setUpClass(cls) -> None:
        cls.adapter = load_module("storage_first_security_adapter_under_test", ADAPTER_PATH)

    def setUp(self) -> None:
        self.fixture = StorageFirstAdapterRedTests("runTest")
        self.fixture.adapter = self.adapter
        self.fixture.setUp()
        self.plan = self.fixture.plan
        self.identity_map = self.fixture.identity_map

    def tearDown(self) -> None:
        self.fixture.tearDown()

    def _authority(self) -> dict[str, object]:
        return self.fixture._authority()

    def _runner(self, transport, **overrides):
        return self.fixture._runner(transport, **overrides)

    def test_qa_sf_001_authority_and_parent_bindings_are_fresh_and_exact(self) -> None:
        validator = getattr(self.adapter, "_storage_first_validate_admission", None)
        self.assertTrue(callable(validator), "RED: missing storage-first admission validator")

        def attempt(mutate_plan=None, mutate_authority=None):
            plan = copy.deepcopy(self.plan)
            authority = self._authority()
            if mutate_plan is not None:
                mutate_plan(plan)
            if mutate_authority is not None:
                mutate_authority(authority)
            evidence = copy.deepcopy(plan["identity_v2_evidence"])
            return validator(
                plan,
                authority,
                phase="storage-205-first",
                identity_v2_evidence=evidence,
            )

        cases = {
            "expired-authority": (
                None,
                lambda authority: authority.update({"expires_at": "2000-01-01T00:00:00Z"}),
            ),
            "identity-digest-rebound": (
                lambda plan: plan["identity_v2_evidence"].update({"digest": "x" * 64}),
                None,
            ),
            "known-host-digest-rebound": (
                lambda plan: plan.update({"known_hosts_digest": "x" * 64}),
                None,
            ),
            "known-host-path-rebound": (
                lambda plan: plan["nodes"][0]["host_binding"].update(
                    {"known_hosts_path": "/operator/rebound-known-hosts"}
                ),
                None,
            ),
            "missing-bounded-proof": (lambda plan: plan.pop("sequencer_proof"), None),
            "missing-ledger-path": (
                lambda plan: plan["credential_nonce_ledger"].pop("path"),
                None,
            ),
            "impact-digest-rebound": (
                lambda plan: plan["consumer_impact_record"].update({"sha256": "x" * 64}),
                None,
            ),
            "plan-digest-rebound": (
                lambda plan: plan.update({"plan_digest": "x" * 64}),
                lambda authority: authority.update({"plan_digest": "x" * 64}),
            ),
        }
        for mutation, (mutate_plan, mutate_authority) in cases.items():
            with self.subTest(mutation=mutation), self.assertRaises(Exception):
                attempt(mutate_plan, mutate_authority)

    def test_qa_sf_002_provider_callbacks_receive_secret_free_node_projection(self) -> None:
        canonical = self.fixture._canonical_fixture()
        base_transport = StorageFirstCanonicalTransport

        class RecordingTransport(base_transport):
            def __init__(self, adapter, plan):
                super().__init__(adapter, plan)
                self.node_keys: list[set[str]] = []

            def _record(self, node):
                self.node_keys.append(set(node))

            def inspect_node(self, node):
                self._record(node)
                return super().inspect_node(node)

            def preflight(self, operation, node):
                self._record(node)
                return super().preflight(operation, node)

            def verify(self, operation, node):
                self._record(node)
                return super().verify(operation, node)

            def mutate(self, operation, node):
                self._record(node)
                return super().mutate(operation, node)

        try:
            transport = RecordingTransport(canonical.adapter, canonical.plan)
            self.fixture._canonical_runner(canonical, transport)
            self.assertTrue(transport.node_keys)
            self.assertTrue(
                all("credential_seam" not in keys for keys in transport.node_keys),
                "provider callbacks must receive the credential-free transport projection",
            )
        finally:
            canonical.tearDown()

    def test_qa_sf_003_live_trust_revalidation_is_mandatory_before_mutation(self) -> None:
        transport = self.fixture._Transport()
        with self.assertRaises(Exception):
            self._runner(transport)
        self.assertEqual(transport.mutations, [])

    def test_qa_sf_003_fresh_process_requires_live_revalidation_before_mutation(self) -> None:
        """A clean interpreter must not inherit a test-order bypass."""
        probe = r'''
import importlib.util
import json
from pathlib import Path

test_path = Path.cwd() / "scripts" / "p2p-public-testnet-full-network-clean-room-adapter.test.py"
spec = importlib.util.spec_from_file_location("fresh_fixture", test_path)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
adapter = module.load_module("fresh_probe_adapter", module.ADAPTER_PATH)
fixture = module.StorageFirstAdapterRedTests("runTest")
fixture.adapter = adapter
fixture.setUp()
transport = fixture._Transport()
try:
    result = fixture._runner(transport)
except Exception as error:
    record = {"outcome": "rejected", "error": error.__class__.__name__, "mutations": transport.mutations}
else:
    record = {"outcome": "completed", "status": result.get("status"), "mutations": transport.mutations}
finally:
    fixture.tearDown()
print(json.dumps(record, sort_keys=True))
'''
        completed = subprocess.run(
            [sys.executable, "-c", probe],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)
        record = json.loads(completed.stdout.strip().splitlines()[-1])
        self.assertEqual(record["mutations"], [])
        self.assertEqual(record["outcome"], "rejected")

    def test_qa_sf_004_receipts_require_complete_bindings(self) -> None:
        receipt_validator = getattr(self.adapter, "validate_storage_first_receipt", None)
        self.assertTrue(callable(receipt_validator), "RED: missing receipt validator")
        minimal_receipt = {
            "schema_version": "oasis7.storage_first_receipt.v1",
            "phase_id": "storage-205-first",
            "operation": "stop:storage-205",
            "target": "storage-205",
            "observer_mutation": False,
        }
        with self.assertRaises(Exception):
            receipt_validator(minimal_receipt)

    def test_qa_sf_004_journals_require_complete_bindings(self) -> None:
        journal_validator = getattr(self.adapter, "validate_storage_first_journal", None)
        self.assertTrue(callable(journal_validator), "RED: missing journal validator")
        minimal_journal = {
            "schema_version": "oasis7.storage_first_mutation_journal.v1",
            "phase_id": "storage-205-first",
            "status": "prepared",
            "next_operation": "stop:storage-205",
        }
        with self.assertRaises(Exception):
            journal_validator(minimal_journal)

    def test_qa_sf_005_side_effect_uncertainty_persists_reconciliation_handoff(self) -> None:
        canonical = self.fixture._canonical_fixture()
        try:
            canonical.plan = copy.deepcopy(dict(canonical.plan))
            transport = StorageFirstCanonicalTransport(
                canonical.adapter,
                canonical.plan,
                side_effect_operation="delete:storage-205",
            )
            journal = canonical.root / "side-effect-security.journal.json"
            original_validate = canonical.adapter._validate_rollback_candidates

            def child_rollback_candidates(plan, candidates, *, order=None):
                if list(candidates) == ["stop:storage-205", "delete:storage-205"]:
                    return list(candidates)
                return original_validate(plan, candidates, order=order)

            with mock.patch.object(
                canonical.adapter,
                "_validate_rollback_candidates",
                side_effect=child_rollback_candidates,
            ):
                with self.assertRaises(Exception):
                    self.fixture._canonical_runner(
                        canonical, transport, journal_path=journal
                    )
            record = json.loads(journal.read_text())
            self.assertEqual(record["rollback_status"], "reconciliation-blocked")
            self.assertEqual(record["next_operation"], "reconciliation-required")
            self.assertEqual(record.get("reconciliation_requirements"), {
                "reobserve_failed_state": True,
                "clean_redeploy": True,
                "automatic_replay": False,
            })
        finally:
            canonical.tearDown()


class StorageFirstFormalFindingsRedTests(unittest.TestCase):
    """Canonical/adversarial RED coverage for the formal storage findings.

    The fixture transport is deliberately in-process.  Tests in this class
    state the missing release gates as executable contracts; they must fail at
    the frozen implementation until the matching domain owner closes them.
    """

    @classmethod
    def setUpClass(cls) -> None:
        cls.adapter = load_module("storage_first_formal_findings_under_test", ADAPTER_PATH)

    def setUp(self) -> None:
        self.fixture = StorageFirstAdapterRedTests("runTest")
        self.fixture.adapter = self.adapter
        self.fixture.setUp()
        # Keep process-local admission state from making tests order-dependent.
        bindings = getattr(self.adapter, "_STORAGE_FIRST_ADMISSION_BINDINGS", None)
        if isinstance(bindings, dict):
            bindings.clear()
        self.plan = self.fixture.plan
        self.identity_map = self.fixture.identity_map

    def tearDown(self) -> None:
        self.fixture.tearDown()

    def _authority(self) -> dict[str, object]:
        return self.fixture._authority()

    def _runner(self, transport, **overrides):
        return self.fixture._runner(transport, **overrides)

    def test_formal_qa_sf_001_canonical_parent_projection_accepts_per_node_bindings(self) -> None:
        """The child must consume code-owned per-node planner bindings."""
        planner = load_module("storage_first_formal_planner_under_test", PLANNER_PATH)
        parent = copy.deepcopy(self.plan)
        for node in parent["nodes"]:
            name = node["name"]
            node["host_binding"] = copy.deepcopy(planner.CANONICAL_HOST_INVENTORY[name])
            node["endpoints"] = copy.deepcopy(planner.CANONICAL_ENDPOINT_INVENTORY[name])
        builder = getattr(planner, "build_storage_first_contract", None)
        self.assertTrue(callable(builder), "RED: missing storage-first planner API")
        try:
            contract = builder(parent)
        except Exception as error:
            self.fail(f"canonical planner projection was rejected: {error}")
        self.assertEqual(contract["target_nodes"], ["storage-205"])
        self.assertNotIn("/v1/chain/status", json.dumps(contract["sequencer_proof"], sort_keys=True))
        admission = getattr(self.adapter, "_storage_first_validate_admission", None)
        self.assertTrue(callable(admission), "RED: missing storage-first admission validator")
        try:
            admission(parent, self._authority(), phase="storage-205-first",
                      identity_v2_evidence=copy.deepcopy(parent["identity_v2_evidence"]))
        except Exception as error:
            self.fail(f"canonical adapter admission rejected planner projection: {error}")

    def test_formal_qa_sf_002_cryptographically_unbound_parent_and_authority_reject(self) -> None:
        validator = getattr(self.adapter, "_storage_first_validate_admission", None)
        self.assertTrue(callable(validator), "RED: missing storage-first admission validator")
        cases = {
            "identity-map": lambda plan, authority: plan["identity_v2_evidence"].update({"digest": "u" * 64}),
            "known-hosts": lambda plan, authority: plan.update({"known_hosts_digest": "v" * 64}),
            "no-backup-receipt": lambda plan, authority: plan["forensic_backup"].update(
                {"receipt": {"authenticated": False, "signed": False}}
            ),
            "authority-signature": lambda plan, authority: authority.update(
                {"signed_payload_sha256": "w" * 64, "signature_hex": "z" * 128}
            ),
        }
        for label, mutate in cases.items():
            with self.subTest(mutation=label):
                bindings = getattr(self.adapter, "_STORAGE_FIRST_ADMISSION_BINDINGS", None)
                if isinstance(bindings, dict):
                    bindings.clear()
                plan = copy.deepcopy(self.plan)
                authority = self._authority()
                mutate(plan, authority)
                with self.assertRaises(Exception):
                    validator(
                        plan,
                        authority,
                        phase="storage-205-first",
                        identity_v2_evidence=copy.deepcopy(plan["identity_v2_evidence"]),
                    )

    def test_formal_ops_sf_001_distinct_journals_cannot_bypass_fleet_guard(self) -> None:
        canonical = self.fixture._canonical_fixture()
        try:
            first_plan = canonical.plan
            replacements = {first_plan["transaction_id"]: "txn-independent-second"}
            replacements.update({
                value: value + "-second"
                for value in first_plan["credential_nonce_ledger"]["reserved_nonces"]
            })

            def rebind(value):
                if isinstance(value, dict):
                    return {key: rebind(item) for key, item in value.items()}
                if isinstance(value, list):
                    return [rebind(item) for item in value]
                return replacements.get(value, value) if isinstance(value, str) else value

            second_plan = rebind(copy.deepcopy(first_plan))
            canonical.fixture._sign_semantic_fixture(second_plan)
            second_plan["plan_digest"] = canonical.adapter.canonical_plan_digest(second_plan)
            second_plan = copy.deepcopy(dict(second_plan))
            first_authority = canonical._authority(True, first_plan)
            second_authority = canonical._authority(True, second_plan)
            canonical.adapter.validate_authority(dict(first_plan), first_authority)
            canonical.adapter.validate_authority(dict(second_plan), second_authority)
            self.assertNotEqual(first_plan["transaction_id"], second_plan["transaction_id"])
            self.assertTrue(set(first_plan["credential_nonce_ledger"]["reserved_nonces"]).isdisjoint(
                second_plan["credential_nonce_ledger"]["reserved_nonces"]
            ))

            control = StorageFirstCanonicalTransport(canonical.adapter, second_plan)
            self.fixture._canonical_runner(
                canonical, control, plan=second_plan,
                journal_path=canonical.root / "independent-control.json",
            )
            canonical._write_ledger(
                canonical.ledger_path,
                [{
                    "schema_version": canonical.adapter.NONCE_ROW_SCHEMA,
                    "transaction_id": "fixture-existing-transaction",
                    "nonce": "fixture-existing-nonce",
                    "one_shot": True,
                }],
            )

            first = StorageFirstCanonicalTransport(canonical.adapter, first_plan)
            second = StorageFirstCanonicalTransport(canonical.adapter, second_plan)
            entered = threading.Event()
            release = threading.Event()
            original_mutate = first.mutate

            def hold_first(operation, node):
                if operation == "stop:storage-205":
                    entered.set()
                    if not release.wait(5):
                        raise RuntimeError("test release timeout")
                return original_mutate(operation, node)

            first.mutate = hold_first
            intrusions: list[str] = []

            def forbidden_inspect(node):
                intrusions.append(node["name"])
                raise AssertionError("second storage transaction reached provider")

            second.inspect_node = forbidden_inspect
            first_journal = canonical.root / "fleet-first.json"
            second_journal = canonical.root / "fleet-second.json"
            first_results: list[object] = []

            def run_first() -> None:
                try:
                    first_results.append(self.fixture._canonical_runner(
                        canonical, first, plan=first_plan, journal_path=first_journal,
                    ))
                except BaseException as error:
                    first_results.append(error)

            worker = threading.Thread(target=run_first)
            worker.start()
            try:
                self.assertTrue(entered.wait(5))
                with self.assertRaises(Exception):
                    self.fixture._canonical_runner(
                        canonical, second, plan=second_plan, journal_path=second_journal,
                    )
            finally:
                release.set()
                worker.join(10)
            self.assertFalse(worker.is_alive())
            self.assertEqual(intrusions, [])
            self.assertEqual(len(first_results), 1)
            self.assertIsInstance(first_results[0], dict)

            result = self.fixture._canonical_runner(
                canonical,
                StorageFirstCanonicalTransport(canonical.adapter, second_plan),
                plan=second_plan,
                journal_path=canonical.root / "fleet-second-after-release.json",
            )
            self.assertEqual(result["status"], "storage-205-verified")
        finally:
            canonical.tearDown()

    def test_formal_ops_sf_002_journal_and_ledger_alias_is_rejected_before_mutation(self) -> None:
        alias = self.fixture.root / "retained-ledger.jsonl"
        alias.write_text("retained-context\n", encoding="utf-8")
        before = alias.read_bytes()
        transport = self.fixture._Transport()
        with self.assertRaises(Exception):
            self._runner(
                transport, journal_path=alias, ledger_path=alias,
                live_revalidator=lambda: True,
            )
        self.assertEqual(alias.read_bytes(), before)
        self.assertEqual(transport.mutations, [])

    def test_formal_ops_sf_003_forged_receipt_cannot_authorize_completion(self) -> None:
        validator = getattr(self.adapter, "validate_storage_first_receipt", None)
        self.assertTrue(callable(validator), "RED: missing storage-first receipt validator")
        forged = {
            "schema_version": "oasis7.storage_first_receipt.v1",
            "phase_id": "storage-205-first",
            "operation": "verify:storage-205",
            "target": "storage-205",
            "observer_mutation": False,
            "completion_boundary": "storage-205-verified-pending-sequencer-probe",
            "authenticated": True,
            "verified": True,
            "transaction_id": self.plan["transaction_id"],
            "capture_window_id": self.plan["capture_window_id"],
        }
        with self.assertRaises(Exception):
            validator(forged)

    def test_formal_ops_sf_003_forged_verified_journal_cannot_skip_cursor(self) -> None:
        receipts = [
            {
                "schema_version": "oasis7.storage_first_receipt.v1",
                "phase_id": "storage-205-first",
                "operation": operation,
                "target": "storage-205",
                "observer_mutation": False,
                "completion_boundary": "storage-205-verified-pending-sequencer-probe",
                "verified": True,
            }
            for operation in (
                "stop:storage-205", "delete:storage-205", "rebuild:storage-205",
                "start:storage-205", "verify:storage-205",
            )
        ]
        journal = {
            "schema_version": "oasis7.storage_first_mutation_journal.v1",
            "phase_id": "storage-205-first",
            "status": "storage-205-verified",
            "next_operation": "reconciliation-required",
            "completed_operations": [
                "stop:storage-205", "delete:storage-205", "rebuild:storage-205",
                "start:storage-205", "verify:storage-205",
            ],
            "transaction_id": self.plan["transaction_id"],
            "capture_window_id": self.plan["capture_window_id"],
            "task_uid": self.plan["task_uid"],
            "head_oid": self.plan["head_oid"],
            "plan_digest": self.plan["plan_digest"],
            "phase_contract_digest": self.adapter._storage_first_phase_digest(
                self.plan, self.identity_map
            ),
            "ledger_path": str(self.fixture.root / "parent-nonce-ledger.jsonl"),
            "storage_receipts": receipts,
        }
        journal_path = self.fixture.root / "forged-verified.json"
        journal_path.write_text(json.dumps(journal), encoding="utf-8")
        with self.assertRaises(Exception):
            self.fixture._resume(journal_path)

    def test_formal_ops_sf_004_provider_evidence_is_required_before_next_operation(self) -> None:
        base = self.fixture._Transport

        class BareEvidenceTransport(base):
            def inspect_node(self, node):
                self.calls.append(f"inspect:{node['name']}")
                return {}

            def preflight(self, operation, node):
                self.calls.append(operation)
                return {"verified": True}

        transport = BareEvidenceTransport()
        with self.assertRaises(Exception):
            self._runner(transport, live_revalidator=lambda: True)
        self.assertEqual(transport.mutations, [])

    def test_formal_ops_sf_005_verify_uses_read_only_callback_not_mutate(self) -> None:
        canonical = self.fixture._canonical_fixture()
        base = StorageFirstCanonicalTransport

        class RecordingTransport(base):
            def __init__(self, adapter, plan):
                super().__init__(adapter, plan)
                self.verify_operations: list[str] = []

            def verify(self, operation, node):
                return super().verify(operation, node)

            def mutate(self, operation, node):
                if operation == "verify:storage-205":
                    self.mutations.append(operation)
                    return self._receipt(operation, self._storage_node())
                return super().mutate(operation, node)

        try:
            transport = RecordingTransport(canonical.adapter, canonical.plan)
            self.fixture._canonical_runner(canonical, transport)
            self.assertEqual(transport.verify_operations, ["verify:storage-205"])
            self.assertNotIn("verify:storage-205", transport.mutations)
        finally:
            canonical.tearDown()

    def test_formal_ops_sf_006_provenance_verifier_gets_sanitized_plan_and_bound_result(self) -> None:
        canonical = self.fixture._canonical_fixture()
        seen: list[dict[str, object]] = []

        def verifier(plan, receipt):
            seen.append(copy.deepcopy(plan))
            return {"verified": True}

        try:
            with self.assertRaises(Exception):
                self.fixture._canonical_runner(
                    canonical,
                    StorageFirstCanonicalTransport(canonical.adapter, canonical.plan),
                    provenance_verifier=verifier,
                )
            self.assertTrue(seen)
            self.assertNotIn("credential_seam", json.dumps(seen[0], sort_keys=True))
        finally:
            canonical.tearDown()

    def test_formal_ops_sf_007_non_boolean_live_revalidation_fails_closed(self) -> None:
        transport = self.fixture._Transport()
        with self.assertRaises(Exception):
            self._runner(transport, live_revalidator=lambda: "verified")
        self.assertEqual(transport.mutations, [])

    def test_formal_runtime_sf_005_journal_write_failure_persists_reconciliation(self) -> None:
        canonical = self.fixture._canonical_fixture()
        try:
            journal = canonical.root / "durability-failure.json"
            original = canonical.adapter._storage_first_journal_write
            failed = False

            def fail_after_stop(path, record):
                nonlocal failed
                if not failed and record.get("completed_operations") == ["stop:storage-205"]:
                    failed = True
                    raise OSError("durability injection after provider mutation")
                return original(path, record)

            with mock.patch.object(
                canonical.adapter, "_storage_first_journal_write", side_effect=fail_after_stop
            ):
                with self.assertRaises(Exception):
                    self.fixture._canonical_runner(
                        canonical,
                        StorageFirstCanonicalTransport(canonical.adapter, canonical.plan),
                        journal_path=journal,
                    )
            self.assertTrue(failed)
            self.assertTrue(journal.exists())
            record = json.loads(journal.read_text())
            self.assertEqual(record["next_operation"], "reconciliation-required")
            self.assertIn(record["status"], {"terminal-failure", "reconciliation-blocked"})
        finally:
            canonical.tearDown()

    def test_formal_runtime_sf_006_nonce_ledger_path_requires_bound_readback(self) -> None:
        unbound = self.fixture.root / "unbound-ledger.jsonl"
        unbound.write_text("", encoding="utf-8")
        transport = self.fixture._Transport()
        with self.assertRaises(Exception):
            self._runner(transport, ledger_path=unbound, live_revalidator=lambda: True)
        self.assertEqual(transport.mutations, [])

    def test_formal_qa_sf_007_cli_exposes_explicit_storage_phase(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plan = root / "plan.json"
            authority = root / "authority.json"
            journal = root / "journal.json"
            ledger = root / "ledger.jsonl"
            for path in (plan, authority, journal, ledger):
                path.write_text("{}", encoding="utf-8")
            authority_root = root / "authority-anchors"
            authority_root.mkdir(mode=0o700)
            public_key = authority_root / "provider-public-key"
            provider = authority_root / "provider-adapter"
            verifier_tool = authority_root / "identity-verifier"
            for path in (public_key, provider, verifier_tool):
                path.write_bytes(b"synthetic canonical authority fixture")
                path.chmod(0o600)
            trust_config = authority_root / "trust-config.json"
            registry = authority_root / "provider-registry.json"
            trust_config.write_text(
                json.dumps({"allowlist": [{"public_key_ref": str(public_key)}]}),
                encoding="utf-8",
            )
            registry.write_text(
                json.dumps({
                    "trust_config_path": str(trust_config),
                    "providers": [{
                        "public_key_ref": str(public_key),
                        "adapter_path": str(provider),
                    }],
                    "verifier": {"executable_path": str(verifier_tool)},
                }),
                encoding="utf-8",
            )
            planner = load_module("storage_first_cli_alias_planner", PLANNER_PATH)
            with mock.patch.object(self.adapter, "_PLANNER_MODULE", planner), \
                 mock.patch.object(planner, "IDENTITY_V2_TRUST_CONFIG_PATH", trust_config), \
                 mock.patch.object(planner, "IDENTITY_V2_PROVIDER_REGISTRY_PATH", registry), \
                 mock.patch.object(self.adapter, "execute_storage_first", return_value={"status": "dry-run"}) as child, \
                 mock.patch.object(self.adapter, "execute", return_value={"status": "legacy"}) as legacy:
                try:
                    result = self.adapter.main([
                        "--plan", str(plan), "--authority", str(authority),
                        "--journal", str(journal), "--ledger", str(ledger),
                        "--phase", "storage-205-first",
                    ])
                except SystemExit as error:
                    self.fail(f"storage-first phase is not reachable from the adapter CLI: {error.code}")
            self.assertEqual(result["status"], "dry-run")
            child.assert_called_once()
            legacy.assert_not_called()


class StorageFirstSecurityFollowupRedTests(unittest.TestCase):
    """Follow-up RED coverage for provider/authentication compatibility seams."""

    @classmethod
    def setUpClass(cls) -> None:
        cls.adapter = load_module("storage_first_security_followup_under_test", ADAPTER_PATH)

    def setUp(self) -> None:
        self.fixture = StorageFirstAdapterRedTests("runTest")
        self.fixture.adapter = self.adapter
        self.fixture.setUp()
        bindings = getattr(self.adapter, "_STORAGE_FIRST_ADMISSION_BINDINGS", None)
        if isinstance(bindings, dict):
            bindings.clear()

    def tearDown(self) -> None:
        self.fixture.tearDown()

    def test_provider_receipt_cannot_synthesize_authentication_via_setdefault(self) -> None:
        binder = getattr(self.adapter, "_storage_first_bind_receipt", None)
        self.assertTrue(callable(binder), "RED: missing provider receipt binding boundary")
        operation = "stop:storage-205"
        raw_provider_receipt = {
            "schema_version": "oasis7.storage_first_receipt.v1",
            "phase_id": "storage-205-first",
            "operation": operation,
            "target": "storage-205",
            "observer_mutation": False,
            "completion_boundary": "storage-205-verified-pending-sequencer-probe",
        }
        with self.assertRaises(Exception):
            binder(self.fixture.plan, operation, raw_provider_receipt)

    def test_none_live_revalidation_fails_even_for_lambda(self) -> None:
        transport = self.fixture._Transport()
        with self.assertRaises(Exception):
            self.fixture._runner(transport, live_revalidator=lambda: None)
        self.assertEqual(transport.mutations, [])

    def test_unbound_provenance_result_fails_even_for_lambda(self) -> None:
        transport = self.fixture._Transport()
        with self.assertRaises(Exception):
            self.fixture._runner(
                transport,
                provenance_verifier=lambda plan, receipt: {"verified": True},
                live_revalidator=lambda: True,
            )
        self.assertEqual(transport.mutations, [])


class StorageFirstAdversarialRedTests(unittest.TestCase):
    """Second-wave RED coverage for the exact formal storage-first blockers.

    These tests deliberately keep all provider interactions in the existing
    in-process transport double.  They encode the missing canonical byte,
    freshness, cursor, recovery, lock, provenance, durability, and journal
    contracts without opening a socket or reading an operator credential.
    """

    @classmethod
    def setUpClass(cls) -> None:
        cls.adapter = load_module("storage_first_adversarial_adapter_under_test", ADAPTER_PATH)

    def setUp(self) -> None:
        self.fixture = StorageFirstAdapterRedTests("runTest")
        self.fixture.adapter = self.adapter
        self.fixture.setUp()
        bindings = getattr(self.adapter, "_STORAGE_FIRST_ADMISSION_BINDINGS", None)
        if isinstance(bindings, dict):
            bindings.clear()
        self.plan = self.fixture.plan

    def tearDown(self) -> None:
        self.fixture.tearDown()

    def test_planner_identity_v2_map_projects_to_current_storage_admission(self) -> None:
        """The planner's retained map must yield the child-only admission view."""
        canonical = self.fixture._canonical_fixture()
        try:
            retained = canonical.plan["identity_v2_evidence"]
            self.assertNotIn("mode", retained)
            self.assertNotIn("digest", retained)
            projected = self.adapter._storage_first_child_projection(
                canonical.plan, retained
            )
            admission = projected["identity_v2_evidence"]
            self.assertEqual(admission["mode"], "current_admission")
            self.assertEqual(
                admission["digest"],
                hashlib.sha256(
                    json.dumps(
                        retained,
                        ensure_ascii=True,
                        sort_keys=True,
                        separators=(",", ":"),
                    ).encode()
                ).hexdigest(),
            )
        finally:
            canonical.tearDown()

    def test_storage_callbacks_receive_isolated_transport_node_projections(self) -> None:
        """A callback mutation must not poison a later host binding."""
        transport = self.fixture._Transport()
        original_inspect = transport.inspect_node
        original_preflight = transport.preflight
        original_mutate = transport.mutate
        original_verify = transport.verify

        def inspect(node):
            self.assertEqual(node["name"], "storage-205")
            result = original_inspect(node)
            node["name"] = "poisoned-by-inspect"
            return result

        def preflight(operation, node):
            self.assertEqual(node["name"], "storage-205")
            result = original_preflight(operation, node)
            node["name"] = "poisoned-by-preflight"
            return result

        def mutate(operation, node):
            self.assertEqual(node["name"], "storage-205")
            result = original_mutate(operation, node)
            node["name"] = "poisoned-by-mutate"
            return result

        def verify(operation, node):
            self.assertEqual(node["name"], "storage-205")
            result = original_verify(operation, node)
            node["name"] = "poisoned-by-verify"
            return result

        transport.inspect_node = inspect
        transport.preflight = preflight
        transport.mutate = mutate
        transport.verify = verify
        self.plan["capture_window"] = {"ends_at": "2099-01-01T00:00:00Z"}

        def bound_provenance(plan, receipt):
            result = self.fixture._bound_provenance(plan, receipt)
            result.update({
                "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
                "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
                "signer_id": "governance-signer",
            })
            return result

        with mock.patch.object(self.adapter, "_storage_first_reject_aliases"), \
                mock.patch.object(
                    self.adapter,
                    "_storage_first_canonical_gates",
                    return_value={"apply_authorized": True},
                ), \
                mock.patch.object(self.adapter, "_storage_first_validate_ledger_binding"):
            result = self.fixture._runner(
                transport,
                live_revalidator=lambda: True,
                provenance_verifier=bound_provenance,
            )
        self.assertEqual(result["status"], "storage-205-verified")

    def _prefix_journal(
        self,
        completed: list[str],
        *,
        receipts: list[dict[str, object]] | None = None,
        status: str = "preflight-complete",
    ) -> Path:
        transport = self.fixture._Transport()
        transport.plan = self.plan
        if receipts is None:
            receipts = [transport._receipt(operation) for operation in completed]
        ledger = self.fixture.root / "parent-nonce-ledger.jsonl"
        journal = self.fixture.root / "resume-prefix.journal.json"
        record: dict[str, object] = {
            "schema_version": self.adapter.STORAGE_FIRST_JOURNAL_SCHEMA,
            "phase_id": self.adapter.STORAGE_FIRST_PHASE_ID,
            "phase_contract_digest": self.adapter._storage_first_phase_digest(
                self.plan, self.fixture.identity_map
            ),
            "task_uid": self.plan["task_uid"],
            "head_oid": self.plan["head_oid"],
            "plan_digest": self.plan["plan_digest"],
            "transaction_id": self.plan["transaction_id"],
            "capture_window_id": self.plan["capture_window_id"],
            "status": status,
            "next_operation": self.adapter.STORAGE_FIRST_OPERATIONS[len(completed)],
            "completed_operations": list(completed),
            "callback_started": False,
            "callback_receipt": None,
            "storage_receipts": copy.deepcopy(receipts),
            "rollback_candidates": list(completed),
            "rollback_status": "not-started",
            "ledger_path": str(ledger),
        }
        record["journal_digest"] = hashlib.sha256(
            json.dumps(record, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest()
        journal.write_text(json.dumps(record, sort_keys=True) + "\n", encoding="utf-8")
        return journal

    def _resume(self, journal: Path, transport: object, **overrides: object) -> object:
        if hasattr(transport, "plan"):
            setattr(transport, "plan", self.plan)
        kwargs: dict[str, object] = {
            "phase": self.adapter.STORAGE_FIRST_PHASE_ID,
            "identity_v2_evidence": self.fixture.identity_map,
            "journal_path": journal,
            "ledger_path": self.fixture.root / "parent-nonce-ledger.jsonl",
            "transport": transport,
            "dry_run": False,
            "provenance_verifier": self.fixture._bound_provenance,
            "live_revalidator": lambda: True,
        }
        kwargs.update(overrides)
        return self.adapter.resume_storage_first(self.plan, self.fixture._authority(), **kwargs)

    def test_ops_sf_009_storage_apply_invokes_canonical_admission_authority_trust_and_ledger_gates(self) -> None:
        calls: list[str] = []

        canonical = self.fixture._canonical_fixture()
        try:
            with ExitStack() as stack:
                for name in (
                    "validate_plan",
                    "validate_authority",
                    "_validate_planner_authority",
                    "validate_live_trust_root_file",
                    "validate_credential_ledger",
                ):
                    original = getattr(canonical.adapter, name)

                    def record_and_validate(
                        *args, _name=name, _original=original, **kwargs
                    ):
                        calls.append(_name)
                        return _original(*args, **kwargs)

                    stack.enter_context(
                        mock.patch.object(
                            canonical.adapter, name, side_effect=record_and_validate
                        )
                    )
                self.fixture._canonical_runner(
                    canonical,
                    StorageFirstCanonicalTransport(canonical.adapter, canonical.plan),
                )
            self.assertTrue(
                set(calls) & {"validate_plan", "validate_authority", "_validate_planner_authority"},
                "storage apply must enter the canonical parent/authority validation boundary",
            )
            self.assertIn("validate_live_trust_root_file", calls)
            self.assertIn("validate_credential_ledger", calls)
        finally:
            canonical.tearDown()

    def test_ops_sf_010_and_runtime_sf_008_storage_apply_requires_declared_canonical_ledger(self) -> None:
        cases = {
            "missing-ledger": (self.fixture.root / "missing-ledger.jsonl", None),
            "nonempty-alternate-ledger": (
                self.fixture.root / "alternate-ledger.jsonl",
                "foreign-ledger-bytes\n",
            ),
        }
        for label, (ledger, contents) in cases.items():
            with self.subTest(case=label):
                if contents is not None:
                    ledger.write_text(contents, encoding="utf-8")
                transport = self.fixture._Transport()
                with self.assertRaises(Exception):
                    self.fixture._runner(
                        transport, ledger_path=ledger, live_revalidator=lambda: True
                    )
                self.assertEqual(transport.mutations, [])

    def test_ops_sf_011_fresh_sequencer_proof_is_required_before_initial_mutation(self) -> None:
        canonical = self.fixture._canonical_fixture()
        try:
            class MissingProofTransport(StorageFirstCanonicalTransport):
                def __init__(self) -> None:
                    super().__init__(canonical.adapter, canonical.plan)
                    self.proof_calls: list[str] = []

                def fetch_sequencer_proof(self, *args: object) -> None:
                    self.proof_calls.append("fetch-sequencer-proof")
                    return None

            transport = MissingProofTransport()
            with self.assertRaises(Exception):
                self.fixture._canonical_runner(canonical, transport)
            self.assertEqual(transport.mutations, [])
            self.assertEqual(transport.proof_calls, ["fetch-sequencer-proof"])
        finally:
            canonical.tearDown()

    def test_ops_sf_011_and_ops_sf_014_nonempty_resume_blocks_before_proof(self) -> None:
        canonical = self.fixture._canonical_fixture()
        try:
            journal = self.fixture._canonical_prefix_journal(
                canonical, ["stop:storage-205"], name="resume-missing-proof.journal.json"
            )

            class MissingProofTransport(StorageFirstCanonicalTransport):
                def __init__(self) -> None:
                    super().__init__(canonical.adapter, canonical.plan)
                    self.proof_calls: list[str] = []

                def fetch_sequencer_proof(self, *args: object) -> None:
                    self.proof_calls.append("fetch-sequencer-proof")
                    return None

            transport = MissingProofTransport()
            with self.assertRaises(Exception):
                self.fixture._canonical_resume(canonical, journal, transport)
            self.assertEqual(transport.mutations, [])
            self.assertEqual(transport.proof_calls, [])
            record = json.loads(journal.read_text(encoding="utf-8"))
            self.assertEqual(record["status"], "reconciliation-blocked")
        finally:
            canonical.tearDown()

    def test_ops_sf_012_pre_mutation_failure_persists_terminal_nonresumable_journal(self) -> None:
        canonical = self.fixture._canonical_fixture()
        try:
            class FailingInspectTransport(StorageFirstCanonicalTransport):
                def inspect_node(self, node: dict[str, object]) -> dict[str, object]:
                    self.calls.append("inspect:storage-205")
                    return {"node": "storage-205", "known_hosts_verified": False}

            journal = canonical.root / "pre-mutation-failure.journal.json"
            transport = FailingInspectTransport(canonical.adapter, canonical.plan)
            with self.assertRaises(Exception):
                self.fixture._canonical_runner(
                    canonical, transport, journal_path=journal
                )
            self.assertEqual(transport.mutations, [])
            record = json.loads(journal.read_text(encoding="utf-8"))
            self.assertIn(record["status"], {"terminal-failure", "reconciliation-blocked"})
            self.assertEqual(record["next_operation"], "reconciliation-required")
            self.assertTrue(record.get("terminal_error"))
        finally:
            canonical.tearDown()

    def test_ops_sf_013_recovery_requires_authenticated_receipts_and_fresh_revalidation(self) -> None:
        canonical = self.fixture._canonical_fixture()
        try:
            # The parent plan remains signed and intact; this phase-facing
            # projection narrows only rollback candidates to this child.
            canonical.plan = copy.deepcopy(dict(canonical.plan))
            checks: list[str] = []
            transport = StorageFirstCanonicalTransport(
                canonical.adapter, canonical.plan, side_effect_operation="delete:storage-205"
            )
            journal = canonical.root / "recovery-auth.journal.json"
            original_validate = canonical.adapter._validate_rollback_candidates

            def child_rollback_candidates(plan, candidates, *, order=None):
                if list(candidates) == ["stop:storage-205", "delete:storage-205"]:
                    return list(candidates)
                return original_validate(plan, candidates, order=order)

            with mock.patch.object(
                canonical.adapter,
                "_validate_rollback_candidates",
                side_effect=child_rollback_candidates,
            ):
                with self.assertRaises(Exception):
                    self.fixture._canonical_runner(
                        canonical,
                        transport,
                        journal_path=journal,
                        live_revalidator=lambda: (checks.append("live") or True),
                    )
            record = json.loads(journal.read_text(encoding="utf-8"))
            self.assertEqual(record["failed_operation"], "delete:storage-205")
            for field in ("reconciliation_reobserve", "reconciliation_handoff"):
                self.assertIn(field, record, record)
                receipt = record[field]
                self.assertIs(receipt.get("authenticated"), True)
                self.assertIs(receipt.get("verified"), True)
                self.assertIn(receipt.get("phase"), {"reobserve", "rollback"})
            self.assertGreaterEqual(len(checks), len(transport.mutations) + 1)
        finally:
            canonical.tearDown()

    def test_ops_sf_014_nonempty_resume_blocks_before_storage_preflight(self) -> None:
        canonical = self.fixture._canonical_fixture()
        try:
            journal = self.fixture._canonical_prefix_journal(
                canonical, ["stop:storage-205"], name="resume-preflight.journal.json"
            )

            class RecordingTransport(StorageFirstCanonicalTransport):
                def __init__(self) -> None:
                    super().__init__(canonical.adapter, canonical.plan)
                    self.calls: list[str] = []

                def inspect_node(self, node: dict[str, object]) -> dict[str, object]:
                    self.calls.append(f"inspect:{node['name']}")
                    return super().inspect_node(node)

                def preflight(self, operation: str, node: dict[str, object]) -> dict[str, object]:
                    self.calls.append(operation)
                    return super().preflight(operation, node)

            transport = RecordingTransport()
            with self.assertRaises(Exception):
                self.fixture._canonical_resume(canonical, journal, transport)
            self.assertEqual(transport.calls, [])
            record = json.loads(journal.read_text(encoding="utf-8"))
            self.assertEqual(record["status"], "reconciliation-blocked")
        finally:
            canonical.tearDown()

    def test_runtime_sf_015_resume_reads_journal_under_guard_and_never_replays_stale_cursor(self) -> None:
        """Resume must bind the journal read to the fleet transaction guard."""
        canonical = self.fixture._canonical_fixture()
        try:
            stale_path = self.fixture._canonical_prefix_journal(
                canonical, ["stop:storage-205"], name="stale-prefix.json"
            )
            stale_record = json.loads(stale_path.read_text(encoding="utf-8"))
            journal = self.fixture._canonical_prefix_journal(
                canonical,
                ["stop:storage-205", "delete:storage-205"],
                name="resume-race.json",
            )
            journal.chmod(0o600)
            transport = StorageFirstCanonicalTransport(
                canonical.adapter, canonical.plan
            )
            events: list[str] = []
            read_journal = canonical.adapter._storage_first_read_journal
            acquire_guard = canonical.adapter._acquire_fleet_transaction_guard

            def read(path: Path):
                events.append("read")
                # If resume reads before the guard, expose the stale prefix. A
                # guarded read must observe the fresh on-disk two-operation prefix.
                return stale_record if "lock" not in events else read_journal(path)

            def acquire(path: Path):
                events.append("lock")
                return acquire_guard(path)

            with mock.patch.object(
                canonical.adapter, "_storage_first_read_journal", side_effect=read
            ), mock.patch.object(
                canonical.adapter, "_acquire_fleet_transaction_guard", side_effect=acquire
            ):
                with self.assertRaises(Exception):
                    self.fixture._canonical_resume(canonical, journal, transport)

            self.assertLess(events.index("lock"), events.index("read"))
            self.assertEqual(transport.mutations, [])
            self.assertEqual(transport.verify_operations, [])
            record = json.loads(journal.read_text(encoding="utf-8"))
            self.assertEqual(record["status"], "reconciliation-blocked")
        finally:
            canonical.tearDown()

    def test_runtime_sf_016_storage_apply_requires_callable_verify_before_provider_operations(self) -> None:
        """The final read-only verify callback must be admitted up front."""
        canonical = self.fixture._canonical_fixture()
        try:
            transport = StorageFirstCanonicalTransport(
                canonical.adapter, canonical.plan
            )
            transport.verify = None
            with self.assertRaises(Exception):
                self.fixture._canonical_runner(canonical, transport)
            self.assertEqual(transport.mutations, [])
        finally:
            canonical.tearDown()

    def test_runtime_sf_017_recovery_receipts_use_storage_child_rollback_prefix(self) -> None:
        """Recovery receipts bind only the storage mutation prefix, never verify."""
        canonical = self.fixture._canonical_fixture()
        try:
            started = ["stop:storage-205", "delete:storage-205"]
            transport = StorageFirstCanonicalTransport(canonical.adapter, canonical.plan)
            transport.failed_operation = "delete:storage-205"
            receipt = transport._receipt(
                "reobserve-failed-state", transport._storage_node()
            )
            receipt["bindings"]["rollback_candidates"] = list(started)
            receipt["rollback_steps"] = canonical.adapter._storage_first_rollback_policy(
                canonical.plan
            )["steps"]
            bound = canonical.adapter._storage_first_recovery_receipt(
                canonical.plan,
                receipt,
                "reobserve-failed-state",
                "delete:storage-205",
                started,
                canonical._recovery_verifier,
            )
            self.assertEqual(bound["bindings"]["rollback_candidates"], started)
            self.assertNotIn("verify:storage-205", bound["bindings"]["rollback_candidates"])
        finally:
            canonical.tearDown()

    def test_runtime_sf_018_recovery_callbacks_are_required_before_first_mutation(self) -> None:
        """Both governed recovery callbacks must be present before provider mutation."""
        canonical = self.fixture._canonical_fixture()
        try:
            class MissingRecoveryCallbacks(StorageFirstCanonicalTransport):
                reobserve_failed_state = None
                rollback_clean_redeploy = None

            transport = MissingRecoveryCallbacks(canonical.adapter, canonical.plan)
            with self.assertRaises(Exception):
                self.fixture._canonical_runner(canonical, transport)
            self.assertEqual(transport.mutations, [])
        finally:
            canonical.tearDown()

    def test_runtime_sf_019_recovery_callbacks_receive_fresh_plan_and_cursor_snapshots(self) -> None:
        """A re-observer cannot poison the subsequent rollback callback inputs."""
        canonical = self.fixture._canonical_fixture()
        try:
            class PoisoningRecoveryTransport(StorageFirstCanonicalTransport):
                def __init__(self, adapter, plan):
                    super().__init__(adapter, plan, side_effect_operation="delete:storage-205")
                    self.reobserve_seen = None
                    self.rollback_seen = None

                def reobserve_failed_state(self, callback_plan, started, failed_operation):
                    receipt = super().reobserve_failed_state(
                        callback_plan, started, failed_operation
                    )
                    self.reobserve_seen = (callback_plan, started)
                    callback_plan["node"]["name"] = "poisoned-by-reobserve"
                    started.append("poisoned-by-reobserve")
                    return receipt

                def rollback_clean_redeploy(self, callback_plan, started, failed_state=None):
                    self.rollback_seen = (
                        copy.deepcopy(callback_plan),
                        list(started),
                        copy.deepcopy(failed_state),
                    )
                    return super().rollback_clean_redeploy(
                        callback_plan, started, failed_state
                    )

            transport = PoisoningRecoveryTransport(canonical.adapter, canonical.plan)
            with self.assertRaises(Exception):
                self.fixture._canonical_runner(canonical, transport)
            self.assertIsNotNone(transport.reobserve_seen)
            self.assertIsNotNone(transport.rollback_seen)
            rollback_plan, rollback_started, _ = transport.rollback_seen
            self.assertEqual(rollback_plan["node"]["name"], "storage-205")
            self.assertEqual(
                rollback_started,
                ["stop:storage-205", "delete:storage-205"],
            )
            self.assertIsNot(
                transport.reobserve_seen[0], transport.rollback_seen[0]
            )
        finally:
            canonical.tearDown()

    def test_runtime_sf_045_recovery_callbacks_receive_derived_child_bindings(self) -> None:
        """Canonical recovery DTOs must carry the admitted child artifact bindings."""
        canonical = self.fixture._canonical_fixture()
        try:
            class BindingCaptureTransport(StorageFirstCanonicalTransport):
                def __init__(self, adapter, plan):
                    super().__init__(adapter, plan, side_effect_operation="delete:storage-205")
                    self.reobserve_plan = None
                    self.rollback_plan = None

                def reobserve_failed_state(self, callback_plan, started, failed_operation):
                    self.reobserve_plan = copy.deepcopy(callback_plan)
                    return super().reobserve_failed_state(
                        callback_plan, started, failed_operation
                    )

                def rollback_clean_redeploy(self, callback_plan, started, failed_state=None):
                    self.rollback_plan = copy.deepcopy(callback_plan)
                    return super().rollback_clean_redeploy(
                        callback_plan, started, failed_state
                    )

            transport = BindingCaptureTransport(canonical.adapter, canonical.plan)
            with self.assertRaises(Exception):
                self.fixture._canonical_runner(canonical, transport)
            for callback_plan in (transport.reobserve_plan, transport.rollback_plan):
                self.assertIsNotNone(callback_plan)
                for field in (
                    "package_provenance_digest",
                    "deployment_inventory_digest",
                    "independent_verifier",
                ):
                    self.assertTrue(callback_plan[field], field)
        finally:
            canonical.tearDown()

    def test_runtime_sf_020_reobserve_receipt_must_bind_actual_failed_operation(self) -> None:
        """A mismatched re-observation must stop before rollback is invoked."""
        canonical = self.fixture._canonical_fixture()
        try:
            class MismatchedReobserveTransport(StorageFirstCanonicalTransport):
                def __init__(self, adapter, plan):
                    super().__init__(adapter, plan, side_effect_operation="delete:storage-205")
                    self.rollback_called = False

                def reobserve_failed_state(self, callback_plan, started, failed_operation):
                    receipt = super().reobserve_failed_state(
                        callback_plan, started, failed_operation
                    )
                    receipt["failed_operation"] = "stop:storage-205"
                    return receipt

                def rollback_clean_redeploy(self, callback_plan, started, failed_state=None):
                    self.rollback_called = True
                    return super().rollback_clean_redeploy(
                        callback_plan, started, failed_state
                    )

            transport = MismatchedReobserveTransport(canonical.adapter, canonical.plan)
            journal = canonical.root / "mismatched-reobserve-operation.journal.json"
            with self.assertRaises(Exception):
                self.fixture._canonical_runner(canonical, transport, journal_path=journal)
            self.assertFalse(transport.rollback_called)
            record = json.loads(journal.read_text(encoding="utf-8"))
            self.assertNotIn("reconciliation_handoff", record)
        finally:
            canonical.tearDown()

    def test_runtime_sf_021_rollback_receipt_must_bind_reobserved_state_digest(self) -> None:
        """Rollback acceptance must be bound to the accepted re-observation digest."""
        canonical = self.fixture._canonical_fixture()
        try:
            class MismatchedRollbackTransport(StorageFirstCanonicalTransport):
                def __init__(self, adapter, plan):
                    super().__init__(adapter, plan, side_effect_operation="delete:storage-205")

                def rollback_clean_redeploy(self, callback_plan, started, failed_state=None):
                    receipt = super().rollback_clean_redeploy(
                        callback_plan, started, failed_state
                    )
                    receipt["failed_state_digest"] = "e" * 64
                    return receipt

            transport = MismatchedRollbackTransport(canonical.adapter, canonical.plan)
            journal = canonical.root / "mismatched-rollback-state.journal.json"
            with self.assertRaises(Exception):
                self.fixture._canonical_runner(canonical, transport, journal_path=journal)
            record = json.loads(journal.read_text(encoding="utf-8"))
            self.assertIn("reconciliation_reobserve", record)
            self.assertNotIn("reconciliation_handoff", record)
        finally:
            canonical.tearDown()

    def test_runtime_sf_022_storage_callback_plan_excludes_fleet_capabilities(self) -> None:
        """Recovery callbacks receive only storage-child data, never fleet controls."""
        canonical = self.fixture._canonical_fixture()
        try:
            storage_node = next(
                node for node in canonical.plan["nodes"] if node["name"] == "storage-205"
            )
            callback_plan = self.adapter._storage_first_callback_plan(
                canonical.plan, storage_node
            )
            self.assertEqual(callback_plan["target_nodes"], ["storage-205"])
            self.assertEqual(callback_plan["node"]["name"], "storage-205")
            for field in (
                "nodes",
                "node_order",
                "global_order",
                "operations",
                "operation_journal_contract",
                "forensic_backup",
                "reset",
            ):
                self.assertNotIn(field, callback_plan)
            self.assertFalse(callback_plan["rollback"]["rerun_fresh_root_probe"])
            self.assertNotIn(
                "rerun-fresh-root-probe", callback_plan["rollback"]["steps"]
            )
        finally:
            canonical.tearDown()

    def test_runtime_sf_023_persisted_rollback_candidates_exclude_verify_after_failure(self) -> None:
        """A verify or later journal failure must never persist verify as rollback work."""
        canonical = self.fixture._canonical_fixture()
        try:
            transport = StorageFirstCanonicalTransport(
                canonical.adapter,
                canonical.plan,
                invalid_operation="verify:storage-205",
            )
            journal = canonical.root / "verify-failure-rollback-candidates.journal.json"
            with self.assertRaises(Exception):
                self.fixture._canonical_runner(canonical, transport, journal_path=journal)
            record = json.loads(journal.read_text(encoding="utf-8"))
            self.assertEqual(
                record["rollback_candidates"],
                [
                    "stop:storage-205",
                    "delete:storage-205",
                    "rebuild:storage-205",
                    "start:storage-205",
                ],
            )
            self.assertNotIn("verify:storage-205", record["rollback_candidates"])
            forged = copy.deepcopy(record)
            forged["rollback_candidates"].append("verify:storage-205")
            forged["journal_digest"] = canonical.adapter.journal_digest(forged)
            with self.assertRaises(Exception):
                canonical.adapter.validate_storage_first_journal(forged)
        finally:
            canonical.tearDown()

    def test_runtime_sf_024_fresh_same_plan_execution_is_locked_before_admission(self) -> None:
        """A concurrent same-plan caller must fail before ledger/provenance admission."""
        canonical = self.fixture._canonical_fixture()
        try:
            first = StorageFirstCanonicalTransport(canonical.adapter, canonical.plan)
            second = StorageFirstCanonicalTransport(canonical.adapter, canonical.plan)
            entered = threading.Event()
            release = threading.Event()
            original_mutate = first.mutate

            def hold_first(operation, node):
                if operation == "stop:storage-205":
                    entered.set()
                    if not release.wait(30):
                        raise RuntimeError("test release timeout")
                return original_mutate(operation, node)

            first.mutate = hold_first
            first_results: list[object] = []

            def run_first() -> None:
                try:
                    first_results.append(
                        self.fixture._canonical_runner(
                            canonical,
                            first,
                            journal_path=canonical.root / "same-plan-first.json",
                        )
                    )
                except BaseException as error:
                    first_results.append(error)

            worker = threading.Thread(target=run_first)
            worker.start()
            self.assertTrue(entered.wait(30), first_results)
            admission_after_contention: list[str] = []
            original_gates = canonical.adapter._storage_first_canonical_gates

            def observe_gates(*args, **kwargs):
                admission_after_contention.append("gate")
                return original_gates(*args, **kwargs)

            try:
                with mock.patch.object(
                    canonical.adapter,
                    "_storage_first_canonical_gates",
                    side_effect=observe_gates,
                ):
                    with self.assertRaises(Exception):
                        self.fixture._canonical_runner(
                            canonical,
                            second,
                            journal_path=canonical.root / "same-plan-second.json",
                        )
            finally:
                release.set()
                worker.join(30)
            self.assertFalse(worker.is_alive())
            self.assertEqual(admission_after_contention, [])
            self.assertEqual(len(first_results), 1)
            self.assertIsInstance(first_results[0], dict, first_results)
        finally:
            canonical.tearDown()

    def test_runtime_sf_025_completed_resume_revalidates_each_persisted_receipt(self) -> None:
        """A completed child journal must cryptographically revalidate every receipt on resume."""
        canonical = self.fixture._canonical_fixture()
        try:
            journal = self.fixture._canonical_prefix_journal(
                canonical,
                list(STORAGE_FIRST_CHILD_OPERATIONS),
                status="storage-205-verified",
                name="completed-revalidation.journal.json",
            )
            transport = StorageFirstCanonicalTransport(canonical.adapter, canonical.plan)
            operations: list[str] = []

            def verifier(verifier_plan, receipt):
                operations.append(receipt.get("operation"))
                return canonical._recovery_verifier(verifier_plan, receipt)

            result = self.fixture._canonical_resume(
                canonical,
                journal,
                transport,
                provenance_verifier=verifier,
            )
            self.assertEqual(result["status"], "storage-205-verified")
            self.assertEqual(operations, list(STORAGE_FIRST_CHILD_OPERATIONS))
            self.assertEqual(transport.mutations, [])
        finally:
            canonical.tearDown()

    def test_runtime_sf_026_completed_resume_rejects_unkeyed_persisted_receipt(self) -> None:
        """An owner-written child receipt without signed material cannot claim completion."""
        canonical = self.fixture._canonical_fixture()
        try:
            journal = self.fixture._canonical_prefix_journal(
                canonical,
                list(STORAGE_FIRST_CHILD_OPERATIONS),
                status="storage-205-verified",
                name="unkeyed-completed.journal.json",
            )
            record = json.loads(journal.read_text(encoding="utf-8"))
            for field in ("signed_payload_sha256", "signature_hex", "canonical_digest"):
                record["storage_receipts"][0].pop(field, None)
            record["journal_digest"] = canonical.adapter.journal_digest(record)
            journal.write_text(json.dumps(record, sort_keys=True), encoding="utf-8")
            journal.chmod(0o600)
            transport = StorageFirstCanonicalTransport(canonical.adapter, canonical.plan)
            with self.assertRaises(Exception):
                self.fixture._canonical_resume(canonical, journal, transport)
            self.assertEqual(transport.mutations, [])
        finally:
            canonical.tearDown()

    def test_runtime_sf_028_completed_resume_requires_independent_verifier(self) -> None:
        """Signed-looking persisted bytes cannot authorize completion without a verifier."""
        canonical = self.fixture._canonical_fixture()
        try:
            journal = self.fixture._canonical_prefix_journal(
                canonical,
                list(STORAGE_FIRST_CHILD_OPERATIONS),
                status="storage-205-verified",
                name="completed-without-verifier.journal.json",
            )
            transport = StorageFirstCanonicalTransport(canonical.adapter, canonical.plan)
            with self.assertRaises(Exception):
                self.fixture._canonical_resume(
                    canonical,
                    journal,
                    transport,
                    provenance_verifier=None,
                )
            self.assertEqual(transport.mutations, [])
        finally:
            canonical.tearDown()

    def test_runtime_sf_029_prepared_journal_rejects_completed_cursor(self) -> None:
        """A prepared checkpoint cannot erase nonce proof for completed mutation work."""
        canonical = self.fixture._canonical_fixture()
        try:
            journal = self.fixture._canonical_prefix_journal(
                canonical,
                ["stop:storage-205"],
                status="prepared",
                name="prepared-completed-cursor.journal.json",
            )
            transport = StorageFirstCanonicalTransport(canonical.adapter, canonical.plan)
            with self.assertRaises(Exception):
                self.fixture._canonical_resume(canonical, journal, transport)
            self.assertEqual(transport.mutations, [])
        finally:
            canonical.tearDown()

    def test_runtime_sf_030_recovery_revalidates_canonical_mutation_gates(self) -> None:
        """Authority or trust-root drift after reobserve must block rollback mutation."""
        for gate_name in ("validate_authority", "validate_live_trust_root_file"):
            with self.subTest(gate=gate_name):
                canonical = self.fixture._canonical_fixture()
                try:
                    transport = StorageFirstCanonicalTransport(
                        canonical.adapter,
                        canonical.plan,
                        side_effect_operation="stop:storage-205",
                    )
                    original_gate = getattr(canonical.adapter, gate_name)

                    def drift_after_reobserve(*args, **kwargs):
                        if transport.rollback_reobservations:
                            raise canonical.adapter.AdapterError("current recovery gate drifted")
                        return original_gate(*args, **kwargs)

                    with mock.patch.object(
                        canonical.adapter, gate_name, side_effect=drift_after_reobserve
                    ):
                        with self.assertRaises(Exception):
                            self.fixture._canonical_runner(canonical, transport)
                    self.assertEqual(transport.rollback_reobservations, ["stop:storage-205"])
                    self.assertEqual(transport.rollback_operations, [])
                finally:
                    canonical.tearDown()

    def test_runtime_sf_031_recovery_rejects_capture_window_expiry_after_reobserve(self) -> None:
        """A re-observation that crosses lease end must not authorize rollback mutation."""
        canonical = self.fixture._canonical_fixture()
        try:
            transport = StorageFirstCanonicalTransport(
                canonical.adapter,
                canonical.plan,
                side_effect_operation="stop:storage-205",
            )
            original_datetime = canonical.adapter.dt.datetime
            capture_end = original_datetime.fromisoformat(
                canonical.plan["capture_window"]["ends_at"].replace("Z", "+00:00")
            )

            class RecoveryClock(original_datetime):
                @classmethod
                def now(cls, tz=None):
                    if transport.rollback_reobservations:
                        return capture_end if tz is not None else capture_end.replace(tzinfo=None)
                    return original_datetime.now(tz)

            with mock.patch.object(canonical.adapter.dt, "datetime", RecoveryClock):
                with self.assertRaises(Exception):
                    self.fixture._canonical_runner(canonical, transport)
            self.assertEqual(transport.rollback_reobservations, ["stop:storage-205"])
            self.assertEqual(transport.rollback_operations, [])
        finally:
            canonical.tearDown()

    def test_runtime_sf_032_mutation_rejects_capture_window_expiry_during_live_check(self) -> None:
        """A live check cannot authorize a provider callback after consuming the lease."""
        canonical = self.fixture._canonical_fixture()
        try:
            transport = StorageFirstCanonicalTransport(canonical.adapter, canonical.plan)
            original_datetime = canonical.adapter.dt.datetime
            capture_end = original_datetime.fromisoformat(
                canonical.plan["capture_window"]["ends_at"].replace("Z", "+00:00")
            )
            expired = False

            class MutationClock(original_datetime):
                @classmethod
                def now(cls, tz=None):
                    if expired:
                        return capture_end if tz is not None else capture_end.replace(tzinfo=None)
                    return original_datetime.now(tz)

            def live_revalidator():
                nonlocal expired
                expired = True
                return True

            with mock.patch.object(canonical.adapter.dt, "datetime", MutationClock):
                with self.assertRaises(Exception):
                    self.fixture._canonical_runner(
                        canonical,
                        transport,
                        live_revalidator=live_revalidator,
                    )
            self.assertEqual(transport.mutations, [])
        finally:
            canonical.tearDown()

    def test_runtime_sf_033_rollback_rejects_capture_window_expiry_during_impact_check(self) -> None:
        """Recovery impact validation cannot outlive the rollback capture lease."""
        canonical = self.fixture._canonical_fixture()
        try:
            transport = StorageFirstCanonicalTransport(
                canonical.adapter,
                canonical.plan,
                side_effect_operation="stop:storage-205",
            )
            original_datetime = canonical.adapter.dt.datetime
            capture_end = original_datetime.fromisoformat(
                canonical.plan["capture_window"]["ends_at"].replace("Z", "+00:00")
            )
            expired = False
            original_impact = canonical.adapter._storage_first_check_impact

            class RecoveryImpactClock(original_datetime):
                @classmethod
                def now(cls, tz=None):
                    if expired:
                        return capture_end if tz is not None else capture_end.replace(tzinfo=None)
                    return original_datetime.now(tz)

            def impact(child_plan):
                nonlocal expired
                result = original_impact(child_plan)
                if transport.rollback_reobservations:
                    expired = True
                return result

            with mock.patch.object(canonical.adapter.dt, "datetime", RecoveryImpactClock), \
                    mock.patch.object(canonical.adapter, "_storage_first_check_impact", side_effect=impact):
                with self.assertRaises(Exception):
                    self.fixture._canonical_runner(canonical, transport)
            self.assertEqual(transport.rollback_reobservations, ["stop:storage-205"])
            self.assertEqual(transport.rollback_operations, [])
        finally:
            canonical.tearDown()

    def test_runtime_sf_034_phase_verifier_failure_releases_fleet_guard(self) -> None:
        """A verifier rejection before execution must not wedge later transactions."""
        canonical = self.fixture._canonical_fixture()
        try:
            transport = StorageFirstCanonicalTransport(canonical.adapter, canonical.plan)

            def rejected_verifier(*_args):
                raise RuntimeError("rejected")

            with mock.patch.object(canonical.adapter, "_verify_provenance", return_value=True):
                with self.assertRaises(Exception):
                    self.fixture._canonical_runner(
                        canonical,
                        transport,
                        provenance_verifier=rejected_verifier,
                    )
            self.assertIsNone(canonical.adapter._ACTIVE_TRANSACTION_GUARD.get())
        finally:
            canonical.tearDown()

    def test_runtime_sf_035_persisted_receipt_retains_original_provider_envelope(self) -> None:
        """Resume verification must receive the provider envelope that was signed."""
        canonical = self.fixture._canonical_fixture()
        try:
            journal = self.fixture._canonical_prefix_journal(
                canonical,
                ["stop:storage-205"],
                name="signed-provider-envelope.journal.json",
            )
            record = json.loads(journal.read_text(encoding="utf-8"))
            receipt = record["storage_receipts"][0]
            envelope = receipt.get("provider_envelope")
            self.assertIsInstance(envelope, dict)
            self.assertEqual(envelope["schema_version"], canonical.adapter.PHASE_RECEIPT_SCHEMAS["apply"])
            self.assertEqual(envelope["operation"], "stop:storage-205")
            self.assertNotEqual(envelope["bindings"], receipt["bindings"])
        finally:
            canonical.tearDown()

    def test_runtime_sf_036_empty_cursor_downgrade_cannot_reuse_committed_nonces(self) -> None:
        """Clearing a completed journal cannot replay the destructive sequence."""
        canonical = self.fixture._canonical_fixture()
        try:
            journal = self.fixture._canonical_prefix_journal(
                canonical,
                list(STORAGE_FIRST_CHILD_OPERATIONS),
                status="storage-205-verified",
                name="downgraded-empty-cursor.journal.json",
            )
            record = json.loads(journal.read_text(encoding="utf-8"))
            record.update({
                "status": "prepared",
                "next_operation": "stop:storage-205",
                "completed_operations": [],
                "storage_receipts": [],
                "receipt_operation_cursor": [],
                "callback_receipt": None,
                "rollback_candidates": [],
            })
            record.pop("nonce_reservation_state", None)
            record["journal_digest"] = canonical.adapter.journal_digest(record)
            journal.write_text(json.dumps(record, sort_keys=True), encoding="utf-8")
            journal.chmod(0o600)
            transport = StorageFirstCanonicalTransport(canonical.adapter, canonical.plan)
            with self.assertRaises(Exception):
                self.fixture._canonical_resume(canonical, journal, transport)
            self.assertEqual(transport.mutations, [])
        finally:
            canonical.tearDown()

    def test_runtime_sf_037_fresh_preflight_checkpoint_is_not_prepared(self) -> None:
        """After nonce commit, a preflight crash must retain committed-reservation semantics."""
        canonical = self.fixture._canonical_fixture()
        try:
            transport = StorageFirstCanonicalTransport(canonical.adapter, canonical.plan)
            transport.preflight = mock.Mock(side_effect=RuntimeError("preflight failed"))
            journal = canonical.root / "fresh-preflight-failure.journal.json"
            with self.assertRaises(Exception):
                self.fixture._canonical_runner(canonical, transport, journal_path=journal)
            record = json.loads(journal.read_text(encoding="utf-8"))
            self.assertEqual(record["status"], "terminal-failure")
            self.assertIn("nonce_reservation_state", record)
        finally:
            canonical.tearDown()

    def test_runtime_sf_038_resumed_preflight_failure_preserves_reconciliation_cursor(self) -> None:
        """Completed mutations remain recoverable when resume preflight drifts."""
        canonical = self.fixture._canonical_fixture()
        try:
            journal = self.fixture._canonical_prefix_journal(
                canonical,
                ["stop:storage-205"],
                name="resume-preflight-failure.journal.json",
            )
            transport = StorageFirstCanonicalTransport(canonical.adapter, canonical.plan)
            transport.preflight = mock.Mock(side_effect=RuntimeError("preflight drift"))
            with self.assertRaises(Exception):
                self.fixture._canonical_resume(canonical, journal, transport)
            record = json.loads(journal.read_text(encoding="utf-8"))
            self.assertEqual(record["status"], "reconciliation-blocked")
            self.assertEqual(record["completed_operations"], ["stop:storage-205"])
            self.assertEqual(record["rollback_candidates"], ["stop:storage-205"])
            self.assertEqual(record["reconciliation_requirements"]["automatic_replay"], False)
            self.assertEqual(transport.mutations, [])
        finally:
            canonical.tearDown()

    def test_runtime_sf_039_verify_does_not_mutate_transport_bookkeeping(self) -> None:
        """Production code must not append read-only verification to caller mutation state."""
        canonical = self.fixture._canonical_fixture()
        try:
            transport = StorageFirstCanonicalTransport(canonical.adapter, canonical.plan)
            self.fixture._canonical_runner(canonical, transport)
            self.assertNotIn("verify:storage-205", transport.mutations)
        finally:
            canonical.tearDown()

    def test_runtime_sf_040_resume_rejects_provider_envelope_operation_rebinding(self) -> None:
        """A signed envelope cannot be wrapped as a different completed operation."""
        canonical = self.fixture._canonical_fixture()
        try:
            journal = self.fixture._canonical_prefix_journal(
                canonical, ["stop:storage-205"], name="provider-envelope-rebind.journal.json"
            )
            record = json.loads(journal.read_text(encoding="utf-8"))
            transport = StorageFirstCanonicalTransport(canonical.adapter, canonical.plan)
            record["storage_receipts"][0]["provider_envelope"] = transport._receipt(
                "delete:storage-205", transport._storage_node()
            )
            record["callback_receipt"] = copy.deepcopy(record["storage_receipts"][0])
            record["journal_digest"] = canonical.adapter.journal_digest(record)
            journal.write_text(json.dumps(record, sort_keys=True), encoding="utf-8")
            journal.chmod(0o600)
            with self.assertRaises(Exception):
                self.fixture._canonical_resume(canonical, journal, transport)
            self.assertEqual(transport.mutations, [])
        finally:
            canonical.tearDown()

    def test_runtime_sf_041_resume_preserves_running_status_after_preflight(self) -> None:
        """A nonempty completed cursor cannot be downgraded to preflight-complete."""
        canonical = self.fixture._canonical_fixture()
        try:
            journal = self.fixture._canonical_prefix_journal(
                canonical, ["stop:storage-205"], name="resume-running-status.journal.json"
            )
            transport = StorageFirstCanonicalTransport(
                canonical.adapter, canonical.plan, side_effect_operation="delete:storage-205"
            )
            with self.assertRaises(Exception):
                self.fixture._canonical_resume(canonical, journal, transport)
            record = json.loads(journal.read_text(encoding="utf-8"))
            self.assertNotEqual(record["status"], "preflight-complete")
            self.assertEqual(record["completed_operations"], ["stop:storage-205"])
        finally:
            canonical.tearDown()

    def test_runtime_sf_042_resume_rejects_running_complete_cursor(self) -> None:
        """A complete cursor cannot retain an impossible running status."""
        canonical = self.fixture._canonical_fixture()
        try:
            journal = self.fixture._canonical_prefix_journal(
                canonical,
                list(STORAGE_FIRST_CHILD_OPERATIONS),
                status="storage-205-running",
                name="running-complete-cursor.journal.json",
            )
            transport = StorageFirstCanonicalTransport(canonical.adapter, canonical.plan)
            with self.assertRaises(Exception):
                self.fixture._canonical_resume(canonical, journal, transport)
            self.assertEqual(transport.mutations, [])
            self.assertEqual(transport.verify_operations, [])
        finally:
            canonical.tearDown()

    def test_runtime_sf_043_resume_checkpoint_write_failure_preserves_reconciliation(self) -> None:
        """A durability failure after completed work must retain governed recovery."""
        canonical = self.fixture._canonical_fixture()
        try:
            journal = self.fixture._canonical_prefix_journal(
                canonical, ["stop:storage-205"], name="resume-checkpoint-write-failure.json"
            )
            transport = StorageFirstCanonicalTransport(canonical.adapter, canonical.plan)
            original_write = canonical.adapter._storage_first_journal_write
            writes = 0

            def fail_second_write(path, record):
                nonlocal writes
                writes += 1
                if writes == 2:
                    raise OSError("injected checkpoint durability failure")
                return original_write(path, record)

            with mock.patch.object(
                canonical.adapter,
                "_storage_first_journal_write",
                side_effect=fail_second_write,
            ):
                with self.assertRaises(Exception):
                    self.fixture._canonical_resume(canonical, journal, transport)
            record = json.loads(journal.read_text(encoding="utf-8"))
            self.assertEqual(record["status"], "reconciliation-blocked")
            self.assertEqual(record["completed_operations"], ["stop:storage-205"])
            self.assertEqual(record["rollback_candidates"], ["stop:storage-205"])
            self.assertEqual(record["reconciliation_requirements"]["automatic_replay"], False)
            self.assertEqual(transport.mutations, [])
        finally:
            canonical.tearDown()

    def test_runtime_sf_044_pre_callback_checkpoint_failure_blocks_replay(self) -> None:
        """A post-replace running-checkpoint failure must enter governed recovery."""
        canonical = self.fixture._canonical_fixture()
        try:
            transport = StorageFirstCanonicalTransport(canonical.adapter, canonical.plan)
            original_write = canonical.adapter._storage_first_journal_write
            injected = False

            def fail_running_checkpoint(path, record):
                nonlocal injected
                original_write(path, record)
                if record.get("status") == "storage-205-running" and not injected:
                    injected = True
                    raise OSError("injected post-replace durability failure")

            with mock.patch.object(
                canonical.adapter,
                "_storage_first_journal_write",
                side_effect=fail_running_checkpoint,
            ):
                with self.assertRaises(Exception):
                    self.fixture._canonical_runner(canonical, transport)
            journal = Path(canonical._test_directory.name) / "storage-first.journal.json"
            record = json.loads(journal.read_text(encoding="utf-8"))
            self.assertEqual(record["status"], "reconciliation-blocked")
            self.assertEqual(record["completed_operations"], [])
            self.assertEqual(record["rollback_candidates"], ["stop:storage-205"])
            self.assertEqual(record["reconciliation_requirements"]["automatic_replay"], False)
            self.assertEqual(transport.mutations, [])
            self.assertEqual(transport.verify_operations, [])
        finally:
            canonical.tearDown()

    def test_runtime_sf_054_nonempty_running_resume_requires_reconciliation(self) -> None:
        """A replaceable nonempty journal prefix must never authorize replay."""
        canonical = self.fixture._canonical_fixture()
        try:
            journal = self.fixture._canonical_prefix_journal(
                canonical,
                ["stop:storage-205"],
                name="truncated-nonempty-prefix.journal.json",
            )
            transport = StorageFirstCanonicalTransport(canonical.adapter, canonical.plan)
            with self.assertRaises(Exception):
                self.fixture._canonical_resume(canonical, journal, transport)
            record = json.loads(journal.read_text(encoding="utf-8"))
            self.assertEqual(record["status"], "reconciliation-blocked")
            self.assertEqual(record["completed_operations"], ["stop:storage-205"])
            self.assertEqual(record["next_operation"], "reconciliation-required")
            self.assertEqual(
                record["reconciliation_requirements"]["automatic_replay"], False
            )
            self.assertEqual(transport.mutations, [])
            self.assertEqual(transport.verify_operations, [])
        finally:
            canonical.tearDown()

    def test_runtime_sf_055_rejected_receipt_preserves_mutation_audit(self) -> None:
        """A performed callback remains visible when its receipt is rejected."""
        canonical = self.fixture._canonical_fixture()
        try:
            transport = StorageFirstCanonicalTransport(canonical.adapter, canonical.plan)
            original_mutate = transport.mutate

            def mutate_with_rejected_receipt(operation, node):
                receipt = original_mutate(operation, node)
                receipt["signature_hex"] = "0" * 128
                return receipt

            transport.mutate = mutate_with_rejected_receipt
            with self.assertRaises(Exception):
                self.fixture._canonical_runner(canonical, transport)
            self.assertEqual(transport.mutations, ["stop:storage-205"])
            record = json.loads(
                (canonical.root / "storage-first.journal.json").read_text(encoding="utf-8")
            )
            self.assertEqual(record["failed_operation"], "stop:storage-205")
            self.assertEqual(record["rollback_candidates"], ["stop:storage-205"])
        finally:
            canonical.tearDown()

    def test_runtime_sf_056_verifier_cannot_rewrite_persisted_provider_envelope(self) -> None:
        """Verifier input mutation must not poison the admitted provider receipt."""
        canonical = self.fixture._canonical_fixture()
        try:
            transport = StorageFirstCanonicalTransport(canonical.adapter, canonical.plan)

            def mutating_verifier(verifier_plan, receipt):
                if receipt.get("operation") == "stop:storage-205":
                    receipt["operation"] = "delete:storage-205"
                return {
                    "verified": True,
                    "bindings": copy.deepcopy(receipt.get("bindings", receipt)),
                    "verifier_id": canonical.adapter.CANONICAL_VERIFIER_ID,
                    "trust_root_id": canonical.adapter.CANONICAL_TRUST_ROOT_ID,
                    "signer_id": "governance-signer",
                }

            self.fixture._canonical_runner(
                canonical, transport, provenance_verifier=mutating_verifier
            )
            journal = Path(canonical._test_directory.name) / "storage-first.journal.json"
            record = json.loads(journal.read_text(encoding="utf-8"))
            self.assertEqual(record["status"], "storage-205-verified")
            for operation, receipt in zip(
                STORAGE_FIRST_CHILD_OPERATIONS, record["storage_receipts"]
            ):
                self.assertEqual(receipt["provider_envelope"]["operation"], operation)
        finally:
            canonical.tearDown()

    def test_runtime_sf_027_resume_validates_nonce_checkpoint_before_prepared_write(self) -> None:
        """Missing committed nonce state must not replace a resumable journal checkpoint."""
        canonical = self.fixture._canonical_fixture()
        try:
            journal = self.fixture._canonical_prefix_journal(
                canonical,
                ["stop:storage-205"],
                status="preflight-complete",
                name="nonce-checkpoint.journal.json",
            )
            before = journal.read_bytes()
            original_validate = canonical.adapter._validate_committed_nonce_reservations
            calls = 0

            def remove_on_second_validation(plan, path, state):
                nonlocal calls
                calls += 1
                if calls == 2:
                    Path(path).unlink()
                return original_validate(plan, path, state)

            transport = StorageFirstCanonicalTransport(canonical.adapter, canonical.plan)
            with mock.patch.object(
                canonical.adapter,
                "_validate_committed_nonce_reservations",
                side_effect=remove_on_second_validation,
            ):
                with self.assertRaises(Exception):
                    self.fixture._canonical_resume(canonical, journal, transport)
            # The impossible preflight-complete/nonempty cursor is now rejected
            # before nonce reconciliation can touch its durable checkpoint.
            self.assertEqual(calls, 0)
            self.assertEqual(journal.read_bytes(), before)
            self.assertEqual(transport.mutations, [])
        finally:
            canonical.tearDown()

    def test_runtime_sf_009_resume_rejects_receipt_operation_cursor_drift(self) -> None:
        receipt_transport = self.fixture._Transport()
        receipt_transport.plan = self.plan
        forged_receipt = receipt_transport._receipt("rebuild:storage-205")
        journal = self._prefix_journal(
            ["stop:storage-205"], receipts=[forged_receipt]
        )
        transport = self.fixture._Transport()
        with self.assertRaises(Exception):
            self._resume(journal, transport)
        self.assertEqual(transport.mutations, [])

    def test_runtime_sf_011_fleet_lock_alias_is_rejected_without_replacement(self) -> None:
        fleet_lock = self.fixture.root / "operator" / "truth" / "fleet.lock"
        fleet_lock.parent.mkdir(parents=True)
        transport = self.fixture._Transport()
        with mock.patch.object(self.adapter, "CANONICAL_FLEET_LOCK_PATH", str(fleet_lock)):
            with self.assertRaises(Exception):
                self.fixture._runner(
                    transport, journal_path=fleet_lock, live_revalidator=lambda: True
                )
        self.assertFalse(fleet_lock.exists())
        self.assertEqual(transport.mutations, [])

    def test_runtime_sf_011_missing_operator_lock_fails_closed_without_tmp_fallback(self) -> None:
        canonical = self.fixture.root / "operator" / "missing" / "truth" / "fleet.lock"
        journal = self.fixture.root / "fallback-journal.json"
        fallback_root = self.fixture.root / "tmp-fallback"
        guard = None
        with mock.patch.object(self.adapter, "CANONICAL_FLEET_LOCK_PATH", str(canonical)), \
             mock.patch.object(self.adapter.tempfile, "gettempdir", return_value=str(fallback_root)):
            try:
                with self.assertRaises(Exception):
                    guard = self.adapter._acquire_fleet_transaction_guard(journal)
            finally:
                if guard is not None:
                    guard.close()
        self.assertFalse(fallback_root.exists())

    def test_runtime_sf_012_provider_receipt_signer_must_be_code_owned(self) -> None:
        base = self.fixture._Transport

        class AttackerReceiptTransport(base):
            def _receipt(self, operation: str) -> dict[str, object]:
                receipt = super()._receipt(operation)
                receipt["signer_id"] = "attacker"
                return receipt

        transport = AttackerReceiptTransport()
        with self.assertRaises(Exception):
            self.fixture._runner(transport, live_revalidator=lambda: True)
        self.assertEqual(transport.mutations, [])

    def test_runtime_sf_013_reconciliation_write_failure_keeps_durable_handoff(self) -> None:
        canonical = self.fixture._canonical_fixture()
        try:
            canonical.plan = copy.deepcopy(dict(canonical.plan))
            journal = canonical.root / "reconciliation-write-failure.journal.json"
            original = canonical.adapter._storage_first_journal_write

            def fail_reconciliation_write(path: Path, record: object) -> None:
                if isinstance(record, dict) and record.get("status") == "reconciliation-blocked":
                    raise OSError("injected reconciliation journal failure")
                original(path, record)

            transport = StorageFirstCanonicalTransport(
                canonical.adapter,
                canonical.plan,
                side_effect_operation="delete:storage-205",
            )
            original_validate = canonical.adapter._validate_rollback_candidates

            def child_rollback_candidates(plan, candidates, *, order=None):
                if list(candidates) == ["stop:storage-205", "delete:storage-205"]:
                    return list(candidates)
                return original_validate(plan, candidates, order=order)

            with mock.patch.object(
                canonical.adapter,
                "_validate_rollback_candidates",
                side_effect=child_rollback_candidates,
            ), mock.patch.object(
                canonical.adapter,
                "_storage_first_journal_write",
                side_effect=fail_reconciliation_write,
            ):
                with self.assertRaises(Exception):
                    self.fixture._canonical_runner(
                        canonical, transport, journal_path=journal
                    )
            durable_candidates = [journal, Path(f"{journal}.emergency.json")]
            durable_records = [
                json.loads(path.read_text(encoding="utf-8"))
                for path in durable_candidates
                if path.exists()
            ]
            self.assertTrue(
                any(
                    record.get("status")
                    in {"terminal-failure", "reconciliation-blocked"}
                    and record.get("failed_operation") == "delete:storage-205"
                    and record.get("next_operation") == "reconciliation-required"
                    for record in durable_records
                )
            )
        finally:
            canonical.tearDown()

    def test_runtime_sf_014_journal_requires_complete_closure_projection(self) -> None:
        validator = self.adapter.validate_storage_first_journal
        incomplete = {
            "schema_version": self.adapter.STORAGE_FIRST_JOURNAL_SCHEMA,
            "phase_id": self.adapter.STORAGE_FIRST_PHASE_ID,
            "status": "storage-205-running",
            "next_operation": "stop:storage-205",
            "completed_operations": [],
            "callback_started": False,
            "callback_receipt": None,
            "storage_receipts": [],
            "rollback_candidates": [],
            "rollback_status": "not-started",
        }
        with self.assertRaises(Exception):
            validator(incomplete)

    def test_runtime_sf_014_journal_reader_requires_protected_file(self) -> None:
        journal = self._prefix_journal([])
        journal.chmod(0o644)
        with self.assertRaises(Exception):
            self.adapter._storage_first_read_journal(journal)


class StorageFirstBlockchainP1RedTests(unittest.TestCase):
    """RED regressions for the five blockchain-ops P1 bypass classes."""

    @classmethod
    def setUpClass(cls) -> None:
        cls.adapter = load_module(
            "storage_first_blockchain_p1_adapter_under_test", ADAPTER_PATH
        )

    def setUp(self) -> None:
        self.fixture = StorageFirstAdapterRedTests("runTest")
        self.fixture.adapter = self.adapter
        self.fixture.setUp()

    def tearDown(self) -> None:
        self.fixture.tearDown()

    def test_p1_side_effect_operation_cannot_bypass_canonical_admission(self) -> None:
        """A caller-controlled fault marker must not authorize shape-only apply."""
        transport = self.fixture._Transport(
            side_effect_operation="never-called-sentinel"
        )
        with self.assertRaises(Exception):
            self.fixture._runner(transport, live_revalidator=lambda: True)
        self.assertEqual(transport.mutations, [])

    def test_p1_identity_less_child_provenance_is_rejected_before_mutation(self) -> None:
        canonical = self.fixture._canonical_fixture()
        try:
            transport = StorageFirstCanonicalTransport(
                canonical.adapter, canonical.plan
            )

            def identity_less_child(verifier_plan, receipt):
                # Keep the parent authority receipt fully authenticated, while
                # making the phase callback result deliberately omit its
                # identity-bound signer/verifier/trust-root fields.
                if isinstance(receipt, dict) and "bindings" in receipt:
                    return canonical._recovery_verifier(verifier_plan, receipt)
                return {"verified": True, "bindings": receipt}

            with self.assertRaises(Exception):
                self.fixture._canonical_runner(
                    canonical,
                    transport,
                    provenance_verifier=identity_less_child,
                )
            self.assertEqual(transport.mutations, [])
        finally:
            canonical.tearDown()

    def test_p1_custom_mapping_cannot_forge_child_projection_for_identityless_provenance(self) -> None:
        canonical = self.fixture._canonical_fixture()
        try:
            class ForgedChildProjection(Mapping):
                """Caller mapping with divergent get/dict views."""

                def __init__(self, source):
                    self.source = source

                def __getitem__(self, key):
                    return self.source[key]

                def __iter__(self):
                    return iter(self.source.keys())

                def __len__(self):
                    return len(self.source)

                def keys(self):
                    return self.source.keys()

                def get(self, key, default=None):
                    if key == "global_order":
                        return list(STORAGE_FIRST_CHILD_OPERATIONS)
                    return self.source.get(key, default)

            forged_plan = ForgedChildProjection(canonical.plan)
            transport = StorageFirstCanonicalTransport(canonical.adapter, forged_plan)

            def identity_less_child(verifier_plan, receipt):
                if isinstance(receipt, dict) and "bindings" in receipt:
                    return canonical._recovery_verifier(verifier_plan, receipt)
                return {"verified": True, "bindings": receipt}

            with self.assertRaises(Exception):
                self.fixture._canonical_runner(
                    canonical,
                    transport,
                    plan=forged_plan,
                    provenance_verifier=identity_less_child,
                )
            self.assertEqual(transport.mutations, [])
        finally:
            canonical.tearDown()

    def test_runtime_p1_dict_subclass_nodes_redirect_rejected_before_effects(self) -> None:
        """A dict subclass must not split canonical bytes from callback targets."""
        canonical = self.fixture._canonical_fixture()
        try:
            class RedirectedNodesPlan(StorageFirstCanonicalPlan):
                """Return attacker targets from get('nodes') only."""

                def get(self, key, default=None):
                    if key != "nodes":
                        return super().get(key, default)
                    nodes = copy.deepcopy(super().get(key, default))
                    for node in nodes:
                        if node.get("name") == "storage-205":
                            node["endpoints"] = {
                                **node["endpoints"],
                                "healthz": "https://attacker.invalid/healthz",
                                "evidence": "https://attacker.invalid/evidence",
                            }
                    return nodes

            forged_plan = RedirectedNodesPlan(canonical.plan)
            self.assertEqual(dict(forged_plan)["nodes"], canonical.plan["nodes"])
            self.assertNotEqual(forged_plan.get("nodes"), dict(forged_plan)["nodes"])
            redirected_storage = next(
                node for node in forged_plan.get("nodes")
                if node["name"] == "storage-205"
            )
            self.assertEqual(
                redirected_storage["endpoints"]["evidence"],
                "https://attacker.invalid/evidence",
            )

            def assert_rejected_without_effects(label, invoke, transport, journal, before=None):
                effects = {"child_journal": 0, "parent_journal": 0, "fleet_lock": 0}

                def trap(effect):
                    def record(*args, **kwargs):
                        effects[effect] += 1
                        raise AssertionError(f"unexpected {effect} effect")

                    return record

                caught = None
                with mock.patch.object(
                    canonical.adapter,
                    "_storage_first_journal_write",
                    side_effect=trap("child_journal"),
                ), mock.patch.object(
                    canonical.adapter,
                    "_write_journal",
                    side_effect=trap("parent_journal"),
                ), mock.patch.object(
                    canonical.adapter,
                    "_acquire_fleet_transaction_guard",
                    side_effect=trap("fleet_lock"),
                ):
                    try:
                        invoke()
                    except Exception as error:
                        caught = error

                self.assertEqual(
                    effects,
                    {"child_journal": 0, "parent_journal": 0, "fleet_lock": 0},
                    f"{label} crossed a protected side-effect boundary",
                )
                self.assertEqual(transport.mutations, [], f"{label} reached provider mutation")
                if before is not None:
                    self.assertEqual(journal.read_bytes(), before, f"{label} changed its journal")
                self.assertIsNotNone(caught, f"{label} accepted redirected provider targets")

            execute_journal = canonical.root / "dict-subclass-execute.journal.json"
            execute_transport = StorageFirstCanonicalTransport(
                canonical.adapter, forged_plan
            )
            assert_rejected_without_effects(
                "execute",
                lambda: self.fixture._canonical_runner(
                    canonical,
                    execute_transport,
                    plan=forged_plan,
                    journal_path=execute_journal,
                ),
                execute_transport,
                execute_journal,
            )

            resume_journal = self.fixture._canonical_prefix_journal(
                canonical,
                ["stop:storage-205"],
                name="dict-subclass-resume.journal.json",
            )
            resume_before = resume_journal.read_bytes()
            canonical.plan = forged_plan
            resume_transport = StorageFirstCanonicalTransport(
                canonical.adapter, forged_plan
            )
            assert_rejected_without_effects(
                "resume",
                lambda: self.fixture._canonical_resume(
                    canonical, resume_journal, resume_transport
                ),
                resume_transport,
                resume_journal,
                resume_before,
            )
        finally:
            canonical.tearDown()

    def test_runtime_p1_nested_node_dict_subclass_cannot_redirect_child_projection(self) -> None:
        """Nested dict subclasses must not alter the private child view."""
        canonical = self.fixture._canonical_fixture()
        try:
            class RedirectOnDeepcopyNode(dict):
                """Return attacker endpoints only when the child copies nodes."""

                def __deepcopy__(self, memo):
                    copied = {
                        key: copy.deepcopy(value, memo) for key, value in self.items()
                    }
                    if copied.get("name") == "storage-205":
                        copied["endpoints"] = {
                            "healthz": "https://attacker.invalid/healthz",
                            "evidence": "https://attacker.invalid/evidence",
                        }
                    return copied

            forged_plan = copy.deepcopy(dict(canonical.plan))
            forged_plan["nodes"][0] = RedirectOnDeepcopyNode(
                canonical.plan["nodes"][0]
            )
            self.assertIs(type(forged_plan), dict)
            self.assertEqual(
                dict(forged_plan["nodes"][0]), canonical.plan["nodes"][0]
            )

            def assert_rejected_without_effects(label, invoke, transport, journal, before=None):
                effects = {"child_journal": 0, "parent_journal": 0, "fleet_lock": 0}

                def trap(effect):
                    def record(*args, **kwargs):
                        effects[effect] += 1
                        raise AssertionError(f"unexpected {effect} effect")

                    return record

                caught = None
                with mock.patch.object(
                    canonical.adapter,
                    "_storage_first_journal_write",
                    side_effect=trap("child_journal"),
                ), mock.patch.object(
                    canonical.adapter,
                    "_write_journal",
                    side_effect=trap("parent_journal"),
                ), mock.patch.object(
                    canonical.adapter,
                    "_acquire_fleet_transaction_guard",
                    side_effect=trap("fleet_lock"),
                ):
                    try:
                        invoke()
                    except Exception as error:
                        caught = error

                self.assertEqual(
                    effects,
                    {"child_journal": 0, "parent_journal": 0, "fleet_lock": 0},
                    f"{label} crossed a protected side-effect boundary",
                )
                self.assertEqual(transport.mutations, [], f"{label} reached provider mutation")
                if before is not None:
                    self.assertEqual(journal.read_bytes(), before, f"{label} changed its journal")
                else:
                    self.assertFalse(journal.exists(), f"{label} created a journal")
                self.assertIsNotNone(caught, f"{label} accepted a nested node projection")

            execute_journal = canonical.root / "nested-dict-subclass-execute.journal.json"
            execute_transport = StorageFirstCanonicalTransport(canonical.adapter, forged_plan)
            assert_rejected_without_effects(
                "execute",
                lambda: self.fixture._canonical_runner(
                    canonical,
                    execute_transport,
                    plan=forged_plan,
                    journal_path=execute_journal,
                ),
                execute_transport,
                execute_journal,
            )

            resume_journal = self.fixture._canonical_prefix_journal(
                canonical,
                ["stop:storage-205"],
                name="nested-dict-subclass-resume.journal.json",
            )
            resume_before = resume_journal.read_bytes()
            canonical.plan = forged_plan
            resume_transport = StorageFirstCanonicalTransport(canonical.adapter, forged_plan)
            assert_rejected_without_effects(
                "resume",
                lambda: self.fixture._canonical_resume(
                    canonical, resume_journal, resume_transport
                ),
                resume_transport,
                resume_journal,
                resume_before,
            )
        finally:
            canonical.tearDown()

    def test_runtime_p1_authority_dict_subclass_cannot_forge_child_authorization(self) -> None:
        """Child authority reads must use the canonical parent authority view."""
        canonical = self.fixture._canonical_fixture()
        try:
            class ForgedChildAuthority(dict):
                """Expose apply=false to parent gates but true to child reads."""

                def __init__(self, value, plan):
                    super().__init__(value)
                    self._child = {
                        "action": "storage-205-first",
                        "targets": ["storage-205"],
                        "task_uid": plan["task_uid"],
                        "frozen_head_oid": plan["head_oid"],
                        "plan_digest": plan["plan_digest"],
                        "transaction_id": plan["transaction_id"],
                        "capture_window_id": plan["capture_window_id"],
                        "current_authorization": True,
                        "signed": True,
                        "expires_at": "2099-01-01T00:00:00Z",
                    }

                def get(self, key, default=None):
                    return self._child.get(key, super().get(key, default))

            forged_authority = ForgedChildAuthority(
                canonical._authority(False, canonical.plan), canonical.plan
            )
            self.assertIs(type(dict(forged_authority)), dict)
            self.assertFalse(dict(forged_authority)["apply_authorized"])
            self.assertTrue(forged_authority.get("current_authorization"))

            transport = StorageFirstCanonicalTransport(
                canonical.adapter, canonical.plan
            )
            journal = canonical.root / "authority-dict-subclass.journal.json"
            effects = {"journal": 0, "lock": 0}

            def trap(effect):
                def record(*args, **kwargs):
                    effects[effect] += 1
                    raise AssertionError(f"unexpected {effect} effect")

                return record

            def canonical_provenance_verifier(verifier_plan, receipt):
                if "bindings" in receipt:
                    return canonical._recovery_verifier(verifier_plan, receipt)
                return {
                    "verified": True,
                    "bindings": receipt,
                    "verifier_id": canonical.adapter.CANONICAL_VERIFIER_ID,
                    "trust_root_id": canonical.adapter.CANONICAL_TRUST_ROOT_ID,
                    "signer_id": "governance-signer",
                }

            caught = None
            with mock.patch.object(
                canonical.adapter,
                "_storage_first_journal_write",
                side_effect=trap("journal"),
            ), mock.patch.object(
                canonical.adapter,
                "_write_journal",
                side_effect=trap("journal"),
            ), mock.patch.object(
                canonical.adapter,
                "_acquire_fleet_transaction_guard",
                side_effect=trap("lock"),
            ):
                try:
                    canonical.adapter.execute_storage_first(
                        canonical.plan,
                        forged_authority,
                        phase="storage-205-first",
                        identity_v2_evidence=canonical.identity_v2_evidence,
                        journal_path=journal,
                        ledger_path=canonical.ledger_path,
                        transport=transport,
                        dry_run=False,
                        provenance_verifier=canonical_provenance_verifier,
                        live_revalidator=lambda: True,
                    )
                except Exception as error:
                    caught = error

            self.assertIsNotNone(caught, "forged child authority was accepted")
            self.assertEqual(effects, {"journal": 0, "lock": 0})
            self.assertEqual(transport.mutations, [])
            self.assertFalse(journal.exists())
        finally:
            canonical.tearDown()

    def test_ops_p1_storage_journal_cannot_alias_governance_root(self) -> None:
        """Storage output paths must protect the code-owned governance root."""
        canonical = self.fixture._canonical_fixture()
        try:
            trust_root = Path(canonical.adapter.CANONICAL_TRUST_ROOT_PATH)
            trust_root_before = (
                trust_root.read_bytes() if trust_root.is_file() else None
            )
            fixture_before = canonical.adapter.CANONICAL_TRUST_ROOT_FIXTURE_PATH.read_bytes()
            prefix = self.fixture._canonical_prefix_journal(
                canonical,
                ["stop:storage-205"],
                name="trust-root-resume-prefix.journal.json",
            )
            resume_record = json.loads(prefix.read_text(encoding="utf-8"))

            def run_case(label, invoke, transport, reader=None):
                effects = {"journal": 0, "lock": 0}

                def trap(effect):
                    def record(*args, **kwargs):
                        effects[effect] += 1
                        raise AssertionError(f"unexpected {effect} effect")

                    return record

                caught = None
                with mock.patch.object(
                    canonical.adapter,
                    "_storage_first_journal_write",
                    side_effect=trap("journal"),
                ), mock.patch.object(
                    canonical.adapter,
                    "_write_journal",
                    side_effect=trap("journal"),
                ), mock.patch.object(
                    canonical.adapter,
                    "_acquire_fleet_transaction_guard",
                    side_effect=trap("lock"),
                ):
                    reader_context = (
                        mock.patch.object(
                            canonical.adapter,
                            "_storage_first_read_journal",
                            side_effect=reader,
                        )
                        if reader is not None
                        else mock.patch.object(
                            canonical.adapter,
                            "_storage_first_read_journal",
                        )
                    )
                    with reader_context as read_journal:
                        try:
                            invoke()
                        except Exception as error:
                            caught = error

                if trust_root_before is None:
                    self.assertFalse(trust_root.exists())
                else:
                    self.assertEqual(trust_root.read_bytes(), trust_root_before)
                self.assertFalse(Path(f"{trust_root}.lock").exists())
                self.assertFalse(Path(f"{trust_root}.emergency.json").exists())
                return {
                    "caught": caught,
                    "effects": effects,
                    "operations": list(transport.operations),
                    "mutations": list(transport.mutations),
                    "read_calls": read_journal.call_count,
                }

            execute_transport = StorageFirstCanonicalTransport(
                canonical.adapter, canonical.plan
            )
            execute_result = run_case(
                "execute",
                lambda: self.fixture._canonical_runner(
                    canonical,
                    execute_transport,
                    journal_path=trust_root,
                ),
                execute_transport,
            )

            resume_transport = StorageFirstCanonicalTransport(
                canonical.adapter, canonical.plan
            )
            resume_result = run_case(
                "resume",
                lambda: self.fixture._canonical_resume(
                    canonical,
                    trust_root,
                    resume_transport,
                ),
                resume_transport,
                reader=lambda path: resume_record,
            )
            for label, result, reader in (
                ("execute", execute_result, None),
                ("resume", resume_result, resume_record),
            ):
                with self.subTest(case=label):
                    self.assertIsNotNone(
                        result["caught"],
                        f"{label} accepted a protected output alias",
                    )
                    self.assertEqual(result["effects"], {"journal": 0, "lock": 0})
                    self.assertEqual(
                        result["operations"], [],
                        f"{label} reached provider callbacks",
                    )
                    self.assertEqual(
                        result["mutations"], [],
                        f"{label} reached provider mutation",
                    )
                    if reader is not None:
                        self.assertEqual(result["read_calls"], 0)
            self.assertEqual(
                canonical.adapter.CANONICAL_TRUST_ROOT_FIXTURE_PATH.read_bytes(),
                fixture_before,
            )
        finally:
            canonical.tearDown()

    def test_p1_forged_persisted_receipt_bindings_cannot_survive_resume(self) -> None:
        canonical = self.fixture._canonical_fixture()
        try:
            journal = self.fixture._canonical_prefix_journal(
                canonical,
                ["stop:storage-205"],
                name="forged-receipt-resume.journal.json",
            )
            record = json.loads(journal.read_text(encoding="utf-8"))
            forged = copy.deepcopy(record["storage_receipts"][0])
            forged["transaction_id"] = "attacker-transaction"
            forged["bindings"]["transaction_id"] = "attacker-transaction"
            record["storage_receipts"][0] = forged
            record["callback_receipt"] = copy.deepcopy(forged)
            record.pop("journal_digest", None)
            record["journal_digest"] = canonical.adapter.journal_digest(record)
            journal.write_text(
                json.dumps(record, sort_keys=True) + "\n", encoding="utf-8"
            )
            journal.chmod(0o600)

            transport = StorageFirstCanonicalTransport(
                canonical.adapter, canonical.plan
            )
            with self.assertRaises(Exception):
                self.fixture._canonical_resume(canonical, journal, transport)
            self.assertEqual(transport.mutations, [])
        finally:
            canonical.tearDown()

    def test_p1_missing_nonce_reservation_cannot_be_repaired_after_checkpoint(self) -> None:
        canonical = self.fixture._canonical_fixture()
        try:
            before_resume = canonical.ledger_path.read_bytes()
            # The checkpoint claims all five reservations were committed, but
            # the authoritative ledger still contains only its unrelated
            # pre-existing row.  Resume must reject rather than repair it.
            nonce_state = canonical.adapter._nonce_reservation_state(
                dict(canonical.plan), len(canonical.plan["nodes"]), complete=True
            )

            record: dict[str, object] = {
                "schema_version": canonical.adapter.STORAGE_FIRST_JOURNAL_SCHEMA,
                "phase_id": canonical.adapter.STORAGE_FIRST_PHASE_ID,
                "status": "preflight-complete",
                "next_operation": "stop:storage-205",
                "completed_operations": [],
                "task_uid": canonical.plan["task_uid"],
                "head_oid": canonical.plan["head_oid"],
                "plan_digest": canonical.plan["plan_digest"],
                "transaction_id": canonical.plan["transaction_id"],
                "capture_window_id": canonical.plan["capture_window_id"],
                "phase_contract_digest": canonical.adapter._storage_first_phase_digest(
                    canonical.plan, canonical.identity_v2_evidence
                ),
                "ledger_path": str(canonical.ledger_path),
                "callback_started": False,
                "callback_receipt": None,
                "storage_receipts": [],
                "receipt_operation_cursor": [],
                "rollback_candidates": [],
                "rollback_status": "not-started",
                "nonce_reservation_state": nonce_state,
            }
            record["journal_digest"] = canonical.adapter.journal_digest(record)
            journal = canonical.root / "missing-nonce-resume.journal.json"
            journal.write_text(
                json.dumps(record, sort_keys=True) + "\n", encoding="utf-8"
            )
            journal.chmod(0o600)

            transport = StorageFirstCanonicalTransport(
                canonical.adapter, canonical.plan
            )
            with self.assertRaises(Exception):
                self.fixture._canonical_resume(canonical, journal, transport)
            self.assertEqual(canonical.ledger_path.read_bytes(), before_resume)
            self.assertEqual(transport.mutations, [])
        finally:
            canonical.tearDown()

    def test_p1_double_journal_write_failure_persists_emergency_handoff(self) -> None:
        canonical = self.fixture._canonical_fixture()
        try:
            canonical.plan = copy.deepcopy(dict(canonical.plan))
            journal = canonical.root / "double-journal-failure.journal.json"
            emergency = Path(f"{journal}.emergency.json")
            original_write = canonical.adapter._storage_first_journal_write

            def fail_primary(path: Path, record: object) -> None:
                if isinstance(record, dict) and record.get("status") == "reconciliation-blocked":
                    raise OSError("injected primary reconciliation journal failure")
                original_write(path, record)

            def fail_reconciliation(path: Path, record: object) -> None:
                raise OSError("injected reconciliation journal failure")

            transport = StorageFirstCanonicalTransport(
                canonical.adapter,
                canonical.plan,
                side_effect_operation="delete:storage-205",
            )
            original_validate = canonical.adapter._validate_rollback_candidates

            def child_rollback_candidates(plan, candidates, *, order=None):
                if list(candidates) == ["stop:storage-205", "delete:storage-205"]:
                    return list(candidates)
                return original_validate(plan, candidates, order=order)

            with mock.patch.object(
                canonical.adapter,
                "_validate_rollback_candidates",
                side_effect=child_rollback_candidates,
            ), mock.patch.object(
                canonical.adapter,
                "_storage_first_journal_write",
                side_effect=fail_primary,
            ), mock.patch.object(
                canonical.adapter,
                "_storage_first_reconciliation_write",
                side_effect=fail_reconciliation,
            ):
                with self.assertRaises(Exception):
                    self.fixture._canonical_runner(
                        canonical, transport, journal_path=journal
                    )
            self.assertTrue(emergency.exists())
            emergency_record = json.loads(emergency.read_text(encoding="utf-8"))
            self.assertEqual(
                emergency_record.get("failed_operation"), "delete:storage-205"
            )
            self.assertEqual(
                emergency_record.get("next_operation"), "reconciliation-required"
            )
        finally:
            canonical.tearDown()


class StorageFirstResidualBypassRedTests(unittest.TestCase):
    """RED regressions for the remaining exact-head compatibility bypasses."""

    @classmethod
    def setUpClass(cls) -> None:
        cls.adapter = load_module("storage_first_residual_bypass_adapter_under_test", ADAPTER_PATH)

    def setUp(self) -> None:
        self.fixture = StorageFirstAdapterRedTests("runTest")
        self.fixture.adapter = self.adapter
        self.fixture.setUp()
        bindings = getattr(self.adapter, "_STORAGE_FIRST_ADMISSION_BINDINGS", None)
        if isinstance(bindings, dict):
            bindings.clear()
        self.plan = self.fixture.plan

    def tearDown(self) -> None:
        self.fixture.tearDown()

    def test_ops_sf_009_shape_only_library_caller_cannot_bypass_canonical_bytes(self) -> None:
        """A caller-shaped sentinel plan must not enter destructive apply."""
        transport = self.fixture._Transport()
        with self.assertRaises(Exception):
            self.fixture._runner(transport, live_revalidator=lambda: True)
        self.assertEqual(transport.mutations, [])

    def test_ops_sf_010_runtime_sf_008_arbitrary_missing_ledger_is_rejected(self) -> None:
        ledger = self.fixture.root / "arbitrary-missing-ledger.jsonl"
        transport = self.fixture._Transport()
        with self.assertRaises(Exception):
            self.fixture._runner(
                transport, ledger_path=ledger, live_revalidator=lambda: True
            )
        self.assertFalse(ledger.exists())
        self.assertEqual(transport.mutations, [])

    def test_ops_sf_011_runtime_sf_012_forged_shape_sequencer_proof_is_rejected(self) -> None:
        base = self.fixture._Transport

        class ForgedProofTransport(base):
            def fetch_sequencer_proof(self, *args: object) -> dict[str, object]:
                return {
                    "operation": "bounded-proof:sequencer-204",
                    "verified": True,
                    "mutation": False,
                }

        transport = ForgedProofTransport()
        with self.assertRaises(Exception):
            self.fixture._runner(transport, live_revalidator=lambda: True)
        self.assertEqual(transport.mutations, [])

    def test_ops_sf_013_shape_recovery_receipts_cannot_be_synthesized(self) -> None:
        base = self.fixture._Transport

        class BareRecoveryTransport(base):
            def __init__(self) -> None:
                super().__init__(side_effect_operation="delete:storage-205")
                self.raw_reobserve: dict[str, object] | None = None

            def reobserve_failed_state(
                self, plan: object, started: object, failed_operation: object
            ) -> dict[str, object]:
                result = super().reobserve_failed_state(plan, started, failed_operation)
                self.raw_reobserve = result
                return result

        transport = BareRecoveryTransport()
        journal = self.fixture.root / "bare-recovery.journal.json"
        with self.assertRaises(Exception):
            self.fixture._runner(
                transport, journal_path=journal, live_revalidator=lambda: True
            )
        # Shape-only callers are rejected at canonical admission, so an
        # unauthenticated recovery callback must never be entered or journaled.
        self.assertIsNone(transport.raw_reobserve)
        self.assertEqual(transport.mutations, [])
        self.assertFalse(journal.exists())

    def test_runtime_sf_014_noninitial_running_journal_requires_complete_closure(self) -> None:
        transport = self.fixture._Transport()
        transport.plan = self.plan
        receipt = transport._receipt("stop:storage-205")
        incomplete = {
            "schema_version": self.adapter.STORAGE_FIRST_JOURNAL_SCHEMA,
            "phase_id": self.adapter.STORAGE_FIRST_PHASE_ID,
            "status": "storage-205-running",
            "next_operation": "delete:storage-205",
            "completed_operations": ["stop:storage-205"],
            "callback_started": False,
            "callback_receipt": receipt,
            "storage_receipts": [receipt],
            "rollback_candidates": ["stop:storage-205"],
            "rollback_status": "not-started",
        }
        with self.assertRaises(Exception):
            self.adapter.validate_storage_first_journal(incomplete)

    def test_runtime_sf_011_cli_rejects_journal_aliasing_plan_input(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plan = root / "plan.json"
            authority = root / "authority.json"
            ledger = root / "ledger.jsonl"
            plan.write_text(json.dumps(self.plan), encoding="utf-8")
            plan.chmod(0o600)
            authority.write_text(json.dumps(self.fixture._authority()), encoding="utf-8")
            ledger.write_text("", encoding="utf-8")
            with mock.patch.object(
                self.adapter, "execute_storage_first", return_value={"status": "dry-run"}
            ) as child:
                with self.assertRaises(Exception):
                    self.adapter.main([
                        "--plan", str(plan),
                        "--authority", str(authority),
                        "--journal", str(plan),
                        "--ledger", str(ledger),
                        "--phase", "storage-205-first",
                    ])
            child.assert_not_called()


class StorageFirstExactHeadFindingTests(unittest.TestCase):
    """Regressions for the exact-head review findings on PR 3677."""

    @classmethod
    def setUpClass(cls) -> None:
        cls.adapter = load_module("storage_first_exact_head_adapter_under_test", ADAPTER_PATH)

    def test_loaded_journal_rewrite_discards_the_prior_digest(self) -> None:
        for writer_name in (
            "_storage_first_journal_write",
            "_storage_first_reconciliation_write",
        ):
            with self.subTest(writer=writer_name), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "journal.json"
                writer = getattr(self.adapter, writer_name)
                record = {
                    "schema_version": self.adapter.STORAGE_FIRST_JOURNAL_SCHEMA,
                    "phase_id": self.adapter.STORAGE_FIRST_PHASE_ID,
                    "status": "reconciliation-blocked",
                    "next_operation": "reconciliation-required",
                    "completed_operations": [],
                    "rollback_candidates": [],
                    "reconciliation_requirements": {
                        "reobserve_failed_state": True,
                        "clean_redeploy": True,
                        "automatic_replay": False,
                    },
                    "journal_digest": "0" * 64,
                }
                writer(path, record)
                self.assertTrue(
                    self.adapter.validate_storage_first_journal(
                        json.loads(path.read_text(encoding="utf-8"))
                    )
                )

    def test_every_started_incomplete_cursor_is_ambiguous(self) -> None:
        journal = {
            "schema_version": self.adapter.STORAGE_FIRST_JOURNAL_SCHEMA,
            "phase_id": self.adapter.STORAGE_FIRST_PHASE_ID,
            "status": "storage-205-running",
            "next_operation": "stop:storage-205",
            "completed_operations": [],
            "callback_started": True,
            "callback_receipt": {"verified": True},
            "rollback_candidates": [],
        }
        with self.assertRaisesRegex(Exception, "ambiguous|reconciliation"):
            self.adapter.validate_storage_first_journal(journal)

    def test_phase_verifier_projection_uses_derived_child_fields(self) -> None:
        fixture = StorageFirstAdapterRedTests("runTest")
        fixture.adapter = self.adapter
        fixture.setUp()
        try:
            child = self.adapter._storage_first_child_projection(
                fixture.plan, fixture.identity_map
            )
            node = next(node for node in child["nodes"] if node["name"] == "storage-205")
            dto = self.adapter._storage_first_callback_plan(child, node)
            self.assertIsNotNone(dto["package_provenance_digest"])
            self.assertIsNotNone(dto["deployment_inventory_digest"])
            self.assertIsNotNone(dto["independent_verifier"])
            self.assertFalse(dto["rollback"]["rerun_fresh_root_probe"])
            self.assertNotIn("rerun-fresh-root-probe", dto["rollback"]["steps"])
        finally:
            fixture.tearDown()

    def test_provider_receipt_rejects_dual_view_mapping(self) -> None:
        class DualView(dict):
            def get(self, key, default=None):
                if key == "operation":
                    return "stop:storage-205"
                return super().get(key, default)

            def items(self):
                changed = dict(super().items())
                changed["operation"] = "delete:storage-205"
                return changed.items()

        with self.assertRaisesRegex(Exception, "concrete|mapping|receipt"):
            self.adapter._storage_first_bind_receipt(
                {"schema_version": "not-a-fixture"},
                "stop:storage-205",
                DualView(operation="delete:storage-205"),
            )

    def test_empty_preflight_resume_requires_reconciliation(self) -> None:
        helper = StorageFirstAdapterRedTests("runTest")
        helper.adapter = self.adapter
        canonical = helper._canonical_fixture()
        try:
            journal = helper._canonical_prefix_journal(
                canonical, [], status="preflight-complete", name="empty-preflight.json"
            )
            transport = StorageFirstCanonicalTransport(self.adapter, canonical.plan)
            with self.assertRaisesRegex(Exception, "unfinished|reconciliation"):
                helper._canonical_resume(canonical, journal, transport)
            self.assertEqual(transport.mutations, [])
            record = json.loads(journal.read_text(encoding="utf-8"))
            self.assertEqual(record["status"], "reconciliation-blocked")
            self.assertTrue(self.adapter.validate_storage_first_journal(record))
        finally:
            canonical.tearDown()

    def test_fresh_execution_refuses_an_existing_journal(self) -> None:
        helper = StorageFirstAdapterRedTests("runTest")
        helper.adapter = self.adapter
        canonical = helper._canonical_fixture()
        try:
            journal = canonical.root / "existing-storage-first.json"
            journal.write_text('{"retained":true}\n', encoding="utf-8")
            journal.chmod(0o600)
            transport = StorageFirstCanonicalTransport(self.adapter, canonical.plan)
            with self.assertRaisesRegex(Exception, "refuses|existing journal"):
                helper._canonical_runner(
                    canonical, transport, journal_path=journal
                )
            self.assertEqual(transport.mutations, [])
            self.assertEqual(json.loads(journal.read_text()), {"retained": True})
        finally:
            canonical.tearDown()

    def test_public_validator_rejects_every_unfinished_resume_state(self) -> None:
        base = {
            "schema_version": self.adapter.STORAGE_FIRST_JOURNAL_SCHEMA,
            "phase_id": self.adapter.STORAGE_FIRST_PHASE_ID,
            "next_operation": "stop:storage-205",
            "completed_operations": [],
            "task_uid": "task-fixture",
            "head_oid": "a" * 40,
            "plan_digest": "b" * 64,
            "transaction_id": "transaction-fixture",
            "capture_window_id": "capture-fixture",
            "phase_contract_digest": "c" * 64,
            "ledger_path": "/tmp/fixture-ledger",
            "callback_started": False,
            "callback_receipt": None,
            "storage_receipts": [],
            "receipt_operation_cursor": [],
            "rollback_candidates": [],
            "rollback_status": "not-started",
        }
        for status in ("prepared", "preflight-complete", "storage-205-running"):
            journal = {**base, "status": status}
            journal["journal_digest"] = self.adapter.journal_digest(journal)
            with self.subTest(status=status), self.assertRaisesRegex(
                Exception, "unfinished|reconciliation"
            ):
                self.adapter.validate_storage_first_journal(journal)

    def test_recovery_receipt_requires_a_concrete_dictionary_tree(self) -> None:
        class DualView(dict):
            pass

        fixture = StorageFirstAdapterRedTests("runTest")
        fixture.adapter = self.adapter
        fixture.setUp()
        try:
            with self.assertRaisesRegex(Exception, "concrete|mapping"):
                self.adapter._storage_first_recovery_receipt(
                    fixture.plan,
                    DualView(),
                    "reobserve-failed-state",
                    "stop:storage-205",
                    ["stop:storage-205"],
                )
        finally:
            fixture.tearDown()

    def test_second_live_check_is_followed_by_final_local_admission(self) -> None:
        helper = StorageFirstAdapterRedTests("runTest")
        helper.adapter = self.adapter
        canonical = helper._canonical_fixture()
        events: list[str] = []
        try:
            transport = StorageFirstCanonicalTransport(canonical.adapter, canonical.plan)
            original_mutate = transport.mutate

            def mutate(operation, node):
                events.append("mutate")
                return original_mutate(operation, node)

            transport.mutate = mutate
            original_validate = canonical.adapter.validate_authority

            def validate(*args, **kwargs):
                events.append("authority")
                return original_validate(*args, **kwargs)

            def live():
                events.append("live")
                return True

            with mock.patch.object(canonical.adapter, "validate_authority", side_effect=validate):
                helper._canonical_runner(
                    canonical, transport, live_revalidator=live
                )
            first_mutation = events.index("mutate")
            self.assertEqual(events[first_mutation - 2:first_mutation], ["live", "authority"])
        finally:
            canonical.tearDown()

    def test_fresh_empty_nonce_ledger_is_valid_initial_state(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            ledger = Path(directory) / "validator-pair-nonces.jsonl"
            ledger.write_bytes(b"")
            ledger.chmod(0o600)
            self.adapter._storage_first_validate_ledger_readback(ledger)


def _flatten_suite(suite: unittest.TestSuite) -> list[unittest.TestCase]:
    cases: list[unittest.TestCase] = []
    for item in suite:
        if isinstance(item, unittest.TestSuite):
            cases.extend(_flatten_suite(item))
        else:
            cases.append(item)
    return cases


def _run_cli() -> None:
    if len(sys.argv) == 3 and sys.argv[1] == "--shard":
        raw = sys.argv[2]
        parts = raw.split("/")
        if len(parts) != 2 or not all(part.isdigit() for part in parts):
            raise SystemExit(f"invalid --shard specification: {raw!r}")
        index, total = int(parts[0]), int(parts[1])
        if total < 1 or total > 64 or not (0 <= index < total):
            raise SystemExit(f"out-of-range --shard specification: {raw!r}")
        loaded = unittest.defaultTestLoader.loadTestsFromModule(sys.modules[__name__])
        if unittest.defaultTestLoader.errors:
            for error in unittest.defaultTestLoader.errors:
                sys.stderr.write(f"{error}\n")
            raise SystemExit(1)
        all_cases = sorted(_flatten_suite(loaded), key=lambda case: case.id())
        if any(case.__class__.__name__ == "_FailedTest" for case in all_cases):
            raise SystemExit("failed to discover clean-room adapter tests")
        if len(all_cases) < total:
            raise SystemExit(f"shard count {total} exceeds discovered tests {len(all_cases)}")
        selected = all_cases[index::total]
        runner = unittest.TextTestRunner(verbosity=1)
        result = runner.run(unittest.TestSuite(selected))
        if not result.wasSuccessful() or result.testsRun != len(selected):
            raise SystemExit(1)
        return
    unittest.main()


if __name__ == "__main__":
    _run_cli()
