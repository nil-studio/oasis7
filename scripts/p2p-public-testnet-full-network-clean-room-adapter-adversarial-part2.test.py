#!/usr/bin/env python3
"""Standalone storage-first adversarial part-2 RED test suite extracted from clean-room adapter tests."""

from __future__ import annotations

import copy
import importlib.util
import json
from pathlib import Path
import sys
import threading
import types
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
ADAPTER_PATH = ROOT / "scripts" / "p2p-public-testnet-full-network-clean-room-adapter.py"
STORAGE_ADAPTER_RED_TEST_PATH = (
    ROOT / "scripts" / "p2p-public-testnet-full-network-clean-room-adapter-storage-adapter-red.test.py"
)
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
            "p2p_public_testnet_clean_room_adapter_storage_adapter_red_tests",
            STORAGE_ADAPTER_RED_TEST_PATH,
        )
        _STORAGE_FIRST_ADAPTER_RED_TESTS = module.StorageFirstAdapterRedTests
    return _STORAGE_FIRST_ADAPTER_RED_TESTS


class StorageFirstAdversarialPart2RedTests(unittest.TestCase):
    """Second-wave RED coverage (part 2) for formal storage-first blockers."""

    @classmethod
    def setUpClass(cls) -> None:
        cls.adapter = load_module(
            "storage_first_adversarial_part2_adapter_under_test", ADAPTER_PATH
        )

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


if __name__ == "__main__":
    unittest.main()
