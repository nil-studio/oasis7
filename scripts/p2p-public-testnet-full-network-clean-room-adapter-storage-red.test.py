#!/usr/bin/env python3
"""Standalone storage-first RED test suites extracted from clean-room adapter tests."""

from __future__ import annotations

import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
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


class StorageFirstSecurityRedTests(unittest.TestCase):
    """Finding-specific RED coverage for the five QA release blockers.

    Every test uses the existing in-process transport double.  No callback can
    open a socket, read credentials, or mutate a real provider.
    """

    @classmethod
    def setUpClass(cls) -> None:
        cls.adapter = load_module("storage_first_security_adapter_under_test", ADAPTER_PATH)

    def setUp(self) -> None:
        self.fixture = _storage_first_adapter_red_tests()("runTest")
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

test_path = Path.cwd() / "scripts" / "p2p-public-testnet-full-network-clean-room-adapter-storage-adapter-red.test.py"
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


class StorageFirstResidualBypassRedTests(unittest.TestCase):
    """RED regressions for the remaining exact-head compatibility bypasses."""

    @classmethod
    def setUpClass(cls) -> None:
        cls.adapter = load_module("storage_first_residual_bypass_adapter_under_test", ADAPTER_PATH)

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
        fixture = _storage_first_adapter_red_tests()("runTest")
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
        helper = _storage_first_adapter_red_tests()("runTest")
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
        helper = _storage_first_adapter_red_tests()("runTest")
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

        fixture = _storage_first_adapter_red_tests()("runTest")
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
        helper = _storage_first_adapter_red_tests()("runTest")
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


if __name__ == "__main__":
    unittest.main()
