#!/usr/bin/env python3
"""Standalone storage-first adversarial part-3 RED test suite extracted from clean-room adapter tests."""

from __future__ import annotations

import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
import types
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
ADAPTER_PATH = ROOT / "scripts" / "p2p-public-testnet-full-network-clean-room-adapter.py"
ADAPTER_TEST_PATH = ROOT / "scripts" / "p2p-public-testnet-full-network-clean-room-adapter.test.py"
FIXTURES_PATH = ROOT / "scripts" / "p2p-public-testnet-clean-room-adapter-test-fixtures.py"


def load_module(name: str, path: Path) -> types.ModuleType:
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise AssertionError(f"cannot load module: {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


_ADAPTER_TEST_FIXTURES = load_module(
    "p2p_public_testnet_clean_room_adapter_test_fixtures", FIXTURES_PATH
)
STORAGE_FIRST_CHILD_OPERATIONS = _ADAPTER_TEST_FIXTURES.STORAGE_FIRST_CHILD_OPERATIONS
StorageFirstCanonicalTransport = _ADAPTER_TEST_FIXTURES.StorageFirstCanonicalTransport
_STORAGE_FIRST_ADAPTER_RED_TESTS: type[unittest.TestCase] | None = None


def _storage_first_adapter_red_tests() -> type[unittest.TestCase]:
    global _STORAGE_FIRST_ADAPTER_RED_TESTS
    if _STORAGE_FIRST_ADAPTER_RED_TESTS is None:
        module = load_module(
            "p2p_public_testnet_clean_room_adapter_base_tests", ADAPTER_TEST_PATH
        )
        _STORAGE_FIRST_ADAPTER_RED_TESTS = module.StorageFirstAdapterRedTests
    return _STORAGE_FIRST_ADAPTER_RED_TESTS


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
        self.fixture = _storage_first_adapter_red_tests()("runTest")
        self.fixture.adapter = self.adapter
        self.fixture.setUp()
        bindings = getattr(self.adapter, "_STORAGE_FIRST_ADMISSION_BINDINGS", None)
        if isinstance(bindings, dict):
            bindings.clear()
        self.plan = self.fixture.plan

    def tearDown(self) -> None:
        self.fixture.tearDown()

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


if __name__ == "__main__":
    unittest.main()
