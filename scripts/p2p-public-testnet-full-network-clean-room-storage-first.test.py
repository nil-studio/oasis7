#!/usr/bin/env python3
"""Standalone storage-first and admission anchor tests extracted from clean-room tests."""

from __future__ import annotations

import copy
import hashlib
import importlib.machinery
import importlib.util
import json
import os
import stat
import subprocess
import tempfile
import types
import unittest
from pathlib import Path
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "scripts" / "p2p-public-testnet-full-network-clean-room.py"
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


def _fixture_digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _write_fixture_json(path: Path, value: object) -> None:
    path.write_bytes(json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode("utf-8"))


def _fixture_descriptor(path: Path) -> dict[str, object]:
    payload = path.read_bytes()
    return {"path": str(path), "sha256": hashlib.sha256(payload).hexdigest(), "size_bytes": len(payload)}


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
