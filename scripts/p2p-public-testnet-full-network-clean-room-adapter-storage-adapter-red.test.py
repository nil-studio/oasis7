#!/usr/bin/env python3
"""Standalone storage-first adapter RED test suite extracted from clean-room adapter tests."""

from __future__ import annotations

import copy
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
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


_FIXTURES = load_module("p2p_public_testnet_clean_room_adapter_test_fixtures", FIXTURES_PATH)
StorageFirstCanonicalTransport = _FIXTURES.StorageFirstCanonicalTransport
StorageFirstCanonicalAuthority = _FIXTURES.StorageFirstCanonicalAuthority
_plain_storage_first_admission_evidence = _FIXTURES._plain_storage_first_admission_evidence
_ADAPTER_TEST_MODULE: types.ModuleType | None = None


def _adapter_test_module() -> types.ModuleType:
    global _ADAPTER_TEST_MODULE
    if _ADAPTER_TEST_MODULE is None:
        _ADAPTER_TEST_MODULE = load_module(
            "p2p_public_testnet_clean_room_adapter_base_tests", ADAPTER_TEST_PATH
        )
    return _ADAPTER_TEST_MODULE


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
        base._canonical_fixture_module = _adapter_test_module()._shared_planner_fixture_module()

    def _canonical_fixture(self):
        """Return an isolated canonical plan/authority/trust-root fixture."""
        self._ensure_canonical_fixture_support()
        fixture = _adapter_test_module().FullNetworkCleanRoomAdapterTests("runTest")
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


if __name__ == "__main__":
    unittest.main()
