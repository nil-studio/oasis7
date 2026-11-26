#!/usr/bin/env python3
"""Shared fixture and transport doubles for clean-room adapter test suites."""

from __future__ import annotations

from collections.abc import Mapping
import copy
import hashlib
import json
import os


STORAGE_FIRST_CHILD_OPERATIONS = [
    "stop:storage-205",
    "delete:storage-205",
    "rebuild:storage-205",
    "start:storage-205",
    "verify:storage-205",
]


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


__all__ = [
    "STORAGE_FIRST_CHILD_OPERATIONS",
    "FakeTransport",
    "ApplyTransport",
    "StorageFirstCanonicalTransport",
    "StorageFirstInspectEvidence",
    "StorageFirstConsumerImpact",
    "StorageFirstCurrentAdmissionEvidence",
    "_plain_storage_first_admission_evidence",
    "StorageFirstCanonicalPlan",
    "StorageFirstCanonicalChildPlan",
    "StorageFirstCanonicalAuthority",
    "ReceivedPlanOnlyTransport",
]
