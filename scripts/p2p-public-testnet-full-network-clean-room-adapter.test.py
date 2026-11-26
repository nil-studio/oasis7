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


def _load_adapter_test_fixtures() -> types.ModuleType:
    module_name = "p2p_public_testnet_clean_room_adapter_test_fixtures"
    spec = importlib.util.spec_from_file_location(module_name, FIXTURES_PATH)
    if spec is None or spec.loader is None:
        raise AssertionError(f"cannot load module: {FIXTURES_PATH}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    return module


_ADAPTER_TEST_FIXTURES = _load_adapter_test_fixtures()
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
