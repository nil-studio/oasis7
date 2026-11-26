#!/usr/bin/env python3
"""Standalone core part-1 and remaining blockchain P1 tests extracted from clean-room adapter tests."""

from __future__ import annotations

import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import types
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
ADAPTER_PATH = ROOT / "scripts" / "p2p-public-testnet-full-network-clean-room-adapter.py"
ADAPTER_TEST_PATH = ROOT / "scripts" / "p2p-public-testnet-full-network-clean-room-adapter.test.py"
TEST_BASE_PATH = ROOT / "scripts" / "p2p-public-testnet-clean-room-adapter-test-base.py"


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
load_module = _ADAPTER_TEST_BASE.load_module
FullNetworkCleanRoomAdapterTestBase = _ADAPTER_TEST_BASE.FullNetworkCleanRoomAdapterTestBase
_ADAPTER_TEST_FIXTURES = _ADAPTER_TEST_BASE._ADAPTER_TEST_FIXTURES
ApplyTransport = _ADAPTER_TEST_FIXTURES.ApplyTransport
StorageFirstCanonicalTransport = _ADAPTER_TEST_FIXTURES.StorageFirstCanonicalTransport
_STORAGE_FIRST_ADAPTER_RED_TESTS: type[unittest.TestCase] | None = None


def _storage_first_adapter_red_tests() -> type[unittest.TestCase]:
    global _STORAGE_FIRST_ADAPTER_RED_TESTS
    if _STORAGE_FIRST_ADAPTER_RED_TESTS is None:
        module_name = "p2p_public_testnet_clean_room_adapter_base_tests"
        spec = importlib.util.spec_from_file_location(module_name, ADAPTER_TEST_PATH)
        if spec is None or spec.loader is None:
            raise AssertionError(f"cannot load module: {ADAPTER_TEST_PATH}")
        module = importlib.util.module_from_spec(spec)
        sys.modules[module_name] = module
        spec.loader.exec_module(module)
        _STORAGE_FIRST_ADAPTER_RED_TESTS = module.StorageFirstAdapterRedTests
    return _STORAGE_FIRST_ADAPTER_RED_TESTS


class FullNetworkCleanRoomAdapterCorePart1Tests(FullNetworkCleanRoomAdapterTestBase):
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
        self.fixture = _storage_first_adapter_red_tests()("runTest")
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


if __name__ == "__main__":
    unittest.main()
