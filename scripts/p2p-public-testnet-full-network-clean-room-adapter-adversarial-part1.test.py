#!/usr/bin/env python3
"""Standalone storage-first adversarial part-1 RED test suite extracted from clean-room adapter tests."""

from __future__ import annotations

import copy
from contextlib import ExitStack
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


class StorageFirstAdversarialPart1RedTests(unittest.TestCase):
    """Second-wave RED coverage (part 1) for formal storage-first blockers."""

    @classmethod
    def setUpClass(cls) -> None:
        cls.adapter = load_module(
            "storage_first_adversarial_part1_adapter_under_test", ADAPTER_PATH
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


if __name__ == "__main__":
    unittest.main()
