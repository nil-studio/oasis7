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
FIXTURES_PATH = ROOT / "scripts" / "p2p-public-testnet-clean-room-adapter-test-fixtures.py"
TEST_BASE_PATH = ROOT / "scripts" / "p2p-public-testnet-clean-room-adapter-test-base.py"
STORAGE_RED_TEST_PATH = (
    ROOT / "scripts" / "p2p-public-testnet-full-network-clean-room-adapter-storage-red.test.py"
)
FORMAL_FINDINGS_TEST_PATH = (
    ROOT / "scripts" / "p2p-public-testnet-full-network-clean-room-adapter-formal-findings.test.py"
)
BOUNDARIES_TEST_PATH = (
    ROOT / "scripts" / "p2p-public-testnet-full-network-clean-room-adapter-boundaries.test.py"
)
STORAGE_ADAPTER_RED_TEST_PATH = (
    ROOT / "scripts" / "p2p-public-testnet-full-network-clean-room-adapter-storage-adapter-red.test.py"
)
ADVERSARIAL_PART1_TEST_PATH = (
    ROOT / "scripts" / "p2p-public-testnet-full-network-clean-room-adapter-adversarial-part1.test.py"
)
ADVERSARIAL_PART2_TEST_PATH = (
    ROOT / "scripts" / "p2p-public-testnet-full-network-clean-room-adapter-adversarial-part2.test.py"
)
ADVERSARIAL_PART3_TEST_PATH = (
    ROOT / "scripts" / "p2p-public-testnet-full-network-clean-room-adapter-adversarial-part3.test.py"
)
BLOCKCHAIN_P1_PART1_TEST_PATH = (
    ROOT / "scripts" / "p2p-public-testnet-full-network-clean-room-adapter-blockchain-p1.test.py"
)
CORE_PART1_TEST_PATH = (
    ROOT / "scripts" / "p2p-public-testnet-full-network-clean-room-adapter-core-part1.test.py"
)


def _load_adapter_test_base() -> types.ModuleType:
    module_name = "p2p_public_testnet_clean_room_adapter_test_base"
    existing = sys.modules.get(module_name)
    if existing is not None:
        return existing
    spec = importlib.util.spec_from_file_location(module_name, TEST_BASE_PATH)
    if spec is None or spec.loader is None:
        raise AssertionError(f"cannot load module: {TEST_BASE_PATH}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    return module


_ADAPTER_TEST_BASE = _load_adapter_test_base()
_COMPILED_MODULE_CODE = _ADAPTER_TEST_BASE._COMPILED_MODULE_CODE
load_module = _ADAPTER_TEST_BASE.load_module
_DEFAULT_LOAD_MODULE = _ADAPTER_TEST_BASE._DEFAULT_LOAD_MODULE
_shared_planner_fixture_module = _ADAPTER_TEST_BASE._shared_planner_fixture_module
FullNetworkCleanRoomAdapterTestBase = _ADAPTER_TEST_BASE.FullNetworkCleanRoomAdapterTestBase
_ADAPTER_TEST_FIXTURES = _ADAPTER_TEST_BASE._ADAPTER_TEST_FIXTURES
STORAGE_FIRST_CHILD_OPERATIONS = _ADAPTER_TEST_FIXTURES.STORAGE_FIRST_CHILD_OPERATIONS
FakeTransport = _ADAPTER_TEST_FIXTURES.FakeTransport
ApplyTransport = _ADAPTER_TEST_FIXTURES.ApplyTransport
StorageFirstCanonicalTransport = _ADAPTER_TEST_FIXTURES.StorageFirstCanonicalTransport
StorageFirstInspectEvidence = _ADAPTER_TEST_FIXTURES.StorageFirstInspectEvidence
StorageFirstConsumerImpact = _ADAPTER_TEST_FIXTURES.StorageFirstConsumerImpact
StorageFirstCurrentAdmissionEvidence = _ADAPTER_TEST_FIXTURES.StorageFirstCurrentAdmissionEvidence
_plain_storage_first_admission_evidence = _ADAPTER_TEST_FIXTURES._plain_storage_first_admission_evidence
StorageFirstCanonicalPlan = _ADAPTER_TEST_FIXTURES.StorageFirstCanonicalPlan
StorageFirstCanonicalChildPlan = _ADAPTER_TEST_FIXTURES.StorageFirstCanonicalChildPlan
StorageFirstCanonicalAuthority = _ADAPTER_TEST_FIXTURES.StorageFirstCanonicalAuthority
ReceivedPlanOnlyTransport = _ADAPTER_TEST_FIXTURES.ReceivedPlanOnlyTransport


class FullNetworkCleanRoomAdapterTests(FullNetworkCleanRoomAdapterTestBase):
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

    def test_live_root_drift_during_preflight_blocks_mutation(self):
        self._assert_live_root_callback_boundary("preflight")

    def test_live_root_drift_after_mutation_blocks_next_and_recovery(self):
        self._assert_live_root_callback_boundary("prior-mutation")

    def test_live_root_drift_during_recovery_verifier_blocks_reobserve(self):
        self._assert_live_root_callback_boundary("recovery-verifier")

    def test_live_root_drift_during_reobserve_blocks_redeploy(self):
        self._assert_live_root_callback_boundary("reobserve")

    def test_same_fleet_distinct_execute_transactions_are_serialized(self):
        self._assert_same_fleet_transactions_serialized(False)

    def test_same_fleet_distinct_resume_transactions_are_serialized(self):
        self._assert_same_fleet_transactions_serialized(True)

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


def _load_storage_adapter_red_test_suite() -> types.ModuleType:
    module_name = "p2p_public_testnet_clean_room_adapter_storage_adapter_red_tests"
    spec = importlib.util.spec_from_file_location(module_name, STORAGE_ADAPTER_RED_TEST_PATH)
    if spec is None or spec.loader is None:
        raise AssertionError(f"cannot load module: {STORAGE_ADAPTER_RED_TEST_PATH}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    module._ADAPTER_TEST_MODULE = sys.modules.get(__name__)
    return module


_STORAGE_ADAPTER_RED_TEST_SUITE = _load_storage_adapter_red_test_suite()
StorageFirstAdapterRedTests = _STORAGE_ADAPTER_RED_TEST_SUITE.StorageFirstAdapterRedTests


def _load_storage_red_test_suite() -> types.ModuleType:
    module_name = "p2p_public_testnet_clean_room_adapter_storage_red_tests"
    spec = importlib.util.spec_from_file_location(module_name, STORAGE_RED_TEST_PATH)
    if spec is None or spec.loader is None:
        raise AssertionError(f"cannot load module: {STORAGE_RED_TEST_PATH}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    module._STORAGE_FIRST_ADAPTER_RED_TESTS = StorageFirstAdapterRedTests
    return module


_STORAGE_RED_TEST_SUITE = _load_storage_red_test_suite()
StorageFirstSecurityRedTests = _STORAGE_RED_TEST_SUITE.StorageFirstSecurityRedTests
StorageFirstResidualBypassRedTests = _STORAGE_RED_TEST_SUITE.StorageFirstResidualBypassRedTests
StorageFirstExactHeadFindingTests = _STORAGE_RED_TEST_SUITE.StorageFirstExactHeadFindingTests


def _load_formal_findings_test_suite() -> types.ModuleType:
    module_name = "p2p_public_testnet_clean_room_adapter_formal_findings_tests"
    spec = importlib.util.spec_from_file_location(module_name, FORMAL_FINDINGS_TEST_PATH)
    if spec is None or spec.loader is None:
        raise AssertionError(f"cannot load module: {FORMAL_FINDINGS_TEST_PATH}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    module._STORAGE_FIRST_ADAPTER_RED_TESTS = StorageFirstAdapterRedTests
    return module


_FORMAL_FINDINGS_TEST_SUITE = _load_formal_findings_test_suite()
TransactionGuardEffectTests = _FORMAL_FINDINGS_TEST_SUITE.TransactionGuardEffectTests
StorageFirstFormalFindingsRedTests = _FORMAL_FINDINGS_TEST_SUITE.StorageFirstFormalFindingsRedTests
StorageFirstSecurityFollowupRedTests = _FORMAL_FINDINGS_TEST_SUITE.StorageFirstSecurityFollowupRedTests


def _load_boundaries_test_suite() -> types.ModuleType:
    module_name = "p2p_public_testnet_clean_room_adapter_boundaries_tests"
    spec = importlib.util.spec_from_file_location(module_name, BOUNDARIES_TEST_PATH)
    if spec is None or spec.loader is None:
        raise AssertionError(f"cannot load module: {BOUNDARIES_TEST_PATH}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    module._ADAPTER_TEST_MODULE = sys.modules.get(__name__)
    return module


_BOUNDARIES_TEST_SUITE = _load_boundaries_test_suite()
JournalLedgerAliasTests = _BOUNDARIES_TEST_SUITE.JournalLedgerAliasTests
ReviewSixBoundaryTests = _BOUNDARIES_TEST_SUITE.ReviewSixBoundaryTests
ReviewFiveBoundaryTests = _BOUNDARIES_TEST_SUITE.ReviewFiveBoundaryTests
FleetLockPublisherProtectionTests = _BOUNDARIES_TEST_SUITE.FleetLockPublisherProtectionTests


def _load_adversarial_part1_test_suite() -> types.ModuleType:
    module_name = "p2p_public_testnet_clean_room_adapter_adversarial_part1_tests"
    spec = importlib.util.spec_from_file_location(module_name, ADVERSARIAL_PART1_TEST_PATH)
    if spec is None or spec.loader is None:
        raise AssertionError(f"cannot load module: {ADVERSARIAL_PART1_TEST_PATH}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    module._STORAGE_FIRST_ADAPTER_RED_TESTS = StorageFirstAdapterRedTests
    return module


_ADVERSARIAL_PART1_TEST_SUITE = _load_adversarial_part1_test_suite()
StorageFirstAdversarialPart1RedTests = (
    _ADVERSARIAL_PART1_TEST_SUITE.StorageFirstAdversarialPart1RedTests
)


def _load_adversarial_part2_test_suite() -> types.ModuleType:
    module_name = "p2p_public_testnet_clean_room_adapter_adversarial_part2_tests"
    spec = importlib.util.spec_from_file_location(module_name, ADVERSARIAL_PART2_TEST_PATH)
    if spec is None or spec.loader is None:
        raise AssertionError(f"cannot load module: {ADVERSARIAL_PART2_TEST_PATH}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    module._STORAGE_FIRST_ADAPTER_RED_TESTS = StorageFirstAdapterRedTests
    return module


_ADVERSARIAL_PART2_TEST_SUITE = _load_adversarial_part2_test_suite()
StorageFirstAdversarialPart2RedTests = (
    _ADVERSARIAL_PART2_TEST_SUITE.StorageFirstAdversarialPart2RedTests
)


def _load_adversarial_part3_test_suite() -> types.ModuleType:
    module_name = "p2p_public_testnet_clean_room_adapter_adversarial_part3_tests"
    spec = importlib.util.spec_from_file_location(module_name, ADVERSARIAL_PART3_TEST_PATH)
    if spec is None or spec.loader is None:
        raise AssertionError(f"cannot load module: {ADVERSARIAL_PART3_TEST_PATH}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    module._STORAGE_FIRST_ADAPTER_RED_TESTS = StorageFirstAdapterRedTests
    return module


_ADVERSARIAL_PART3_TEST_SUITE = _load_adversarial_part3_test_suite()
StorageFirstAdversarialRedTests = (
    _ADVERSARIAL_PART3_TEST_SUITE.StorageFirstAdversarialRedTests
)


def _load_blockchain_p1_part1_test_suite() -> types.ModuleType:
    module_name = "p2p_public_testnet_clean_room_adapter_blockchain_p1_part1_tests"
    spec = importlib.util.spec_from_file_location(module_name, BLOCKCHAIN_P1_PART1_TEST_PATH)
    if spec is None or spec.loader is None:
        raise AssertionError(f"cannot load module: {BLOCKCHAIN_P1_PART1_TEST_PATH}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    module._STORAGE_FIRST_ADAPTER_RED_TESTS = StorageFirstAdapterRedTests
    return module


_BLOCKCHAIN_P1_PART1_TEST_SUITE = _load_blockchain_p1_part1_test_suite()
StorageFirstBlockchainP1Part1RedTests = (
    _BLOCKCHAIN_P1_PART1_TEST_SUITE.StorageFirstBlockchainP1Part1RedTests
)


def _load_core_part1_test_suite() -> types.ModuleType:
    module_name = "p2p_public_testnet_clean_room_adapter_core_part1_tests"
    spec = importlib.util.spec_from_file_location(module_name, CORE_PART1_TEST_PATH)
    if spec is None or spec.loader is None:
        raise AssertionError(f"cannot load module: {CORE_PART1_TEST_PATH}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    module._STORAGE_FIRST_ADAPTER_RED_TESTS = StorageFirstAdapterRedTests
    return module


_CORE_PART1_TEST_SUITE = _load_core_part1_test_suite()
FullNetworkCleanRoomAdapterCorePart1Tests = (
    _CORE_PART1_TEST_SUITE.FullNetworkCleanRoomAdapterCorePart1Tests
)
StorageFirstBlockchainP1RedTests = (
    _CORE_PART1_TEST_SUITE.StorageFirstBlockchainP1RedTests
)


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
