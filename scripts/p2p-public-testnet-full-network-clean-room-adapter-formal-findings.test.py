#!/usr/bin/env python3
"""Standalone formal-findings and guard-effect test suites extracted from clean-room adapter tests."""

from __future__ import annotations

import copy
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import threading
import types
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
PLANNER_PATH = ROOT / "scripts" / "p2p-public-testnet-full-network-clean-room.py"
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


class TransactionGuardEffectTests(unittest.TestCase):
    def test_detected_drift_blocks_provider_rollback_and_emergency_effects(self):
        adapter = load_module("guard_effect_adapter", ADAPTER_PATH)
        for effect in ("provider", "rollback", "emergency"):
            with self.subTest(effect=effect), tempfile.TemporaryDirectory() as directory:
                journal = Path(directory) / "journal"
                guard = adapter._acquire_transaction_lock(journal)
                token = adapter._ACTIVE_TRANSACTION_GUARD.set(guard)
                callback = mock.Mock()
                try:
                    lock = Path(f"{journal}.lock")
                    lock.unlink()
                    lock.write_bytes(b"")
                    lock.chmod(0o600)
                    with self.assertRaises(adapter.AdapterError):
                        if effect == "emergency":
                            adapter._persist_terminal(journal, {"status": "terminal-failure"})
                        else:
                            adapter._guarded_callback(callback, {"operation": effect})
                    callback.assert_not_called()
                    self.assertFalse(journal.exists())
                    self.assertFalse(Path(f"{journal}.emergency.json").exists())
                finally:
                    adapter._ACTIVE_TRANSACTION_GUARD.reset(token)
                    adapter._release_transaction_lock(guard)

    def test_callback_replacement_is_detected_on_return(self):
        adapter = load_module("guard_return_adapter", ADAPTER_PATH)
        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / "journal"
            guard = adapter._acquire_transaction_lock(journal)
            token = adapter._ACTIVE_TRANSACTION_GUARD.set(guard)
            def callback():
                lock = Path(f"{journal}.lock")
                lock.unlink()
                lock.write_bytes(b"")
                lock.chmod(0o600)
                return {"verified": True}
            try:
                with self.assertRaises(adapter.AdapterError):
                    adapter._guarded_callback(callback)
            finally:
                adapter._ACTIVE_TRANSACTION_GUARD.reset(token)
                adapter._release_transaction_lock(guard)


class StorageFirstFormalFindingsRedTests(unittest.TestCase):
    """Canonical/adversarial RED coverage for the formal storage findings.

    The fixture transport is deliberately in-process.  Tests in this class
    state the missing release gates as executable contracts; they must fail at
    the frozen implementation until the matching domain owner closes them.
    """

    @classmethod
    def setUpClass(cls) -> None:
        cls.adapter = load_module("storage_first_formal_findings_under_test", ADAPTER_PATH)

    def setUp(self) -> None:
        self.fixture = _storage_first_adapter_red_tests()("runTest")
        self.fixture.adapter = self.adapter
        self.fixture.setUp()
        # Keep process-local admission state from making tests order-dependent.
        bindings = getattr(self.adapter, "_STORAGE_FIRST_ADMISSION_BINDINGS", None)
        if isinstance(bindings, dict):
            bindings.clear()
        self.plan = self.fixture.plan
        self.identity_map = self.fixture.identity_map

    def tearDown(self) -> None:
        self.fixture.tearDown()

    def _authority(self) -> dict[str, object]:
        return self.fixture._authority()

    def _runner(self, transport, **overrides):
        return self.fixture._runner(transport, **overrides)

    def test_formal_qa_sf_001_canonical_parent_projection_accepts_per_node_bindings(self) -> None:
        """The child must consume code-owned per-node planner bindings."""
        planner = load_module("storage_first_formal_planner_under_test", PLANNER_PATH)
        parent = copy.deepcopy(self.plan)
        for node in parent["nodes"]:
            name = node["name"]
            node["host_binding"] = copy.deepcopy(planner.CANONICAL_HOST_INVENTORY[name])
            node["endpoints"] = copy.deepcopy(planner.CANONICAL_ENDPOINT_INVENTORY[name])
        builder = getattr(planner, "build_storage_first_contract", None)
        self.assertTrue(callable(builder), "RED: missing storage-first planner API")
        try:
            contract = builder(parent)
        except Exception as error:
            self.fail(f"canonical planner projection was rejected: {error}")
        self.assertEqual(contract["target_nodes"], ["storage-205"])
        self.assertNotIn("/v1/chain/status", json.dumps(contract["sequencer_proof"], sort_keys=True))
        admission = getattr(self.adapter, "_storage_first_validate_admission", None)
        self.assertTrue(callable(admission), "RED: missing storage-first admission validator")
        try:
            admission(parent, self._authority(), phase="storage-205-first",
                      identity_v2_evidence=copy.deepcopy(parent["identity_v2_evidence"]))
        except Exception as error:
            self.fail(f"canonical adapter admission rejected planner projection: {error}")

    def test_formal_qa_sf_002_cryptographically_unbound_parent_and_authority_reject(self) -> None:
        validator = getattr(self.adapter, "_storage_first_validate_admission", None)
        self.assertTrue(callable(validator), "RED: missing storage-first admission validator")
        cases = {
            "identity-map": lambda plan, authority: plan["identity_v2_evidence"].update({"digest": "u" * 64}),
            "known-hosts": lambda plan, authority: plan.update({"known_hosts_digest": "v" * 64}),
            "no-backup-receipt": lambda plan, authority: plan["forensic_backup"].update(
                {"receipt": {"authenticated": False, "signed": False}}
            ),
            "authority-signature": lambda plan, authority: authority.update(
                {"signed_payload_sha256": "w" * 64, "signature_hex": "z" * 128}
            ),
        }
        for label, mutate in cases.items():
            with self.subTest(mutation=label):
                bindings = getattr(self.adapter, "_STORAGE_FIRST_ADMISSION_BINDINGS", None)
                if isinstance(bindings, dict):
                    bindings.clear()
                plan = copy.deepcopy(self.plan)
                authority = self._authority()
                mutate(plan, authority)
                with self.assertRaises(Exception):
                    validator(
                        plan,
                        authority,
                        phase="storage-205-first",
                        identity_v2_evidence=copy.deepcopy(plan["identity_v2_evidence"]),
                    )

    def test_formal_ops_sf_001_distinct_journals_cannot_bypass_fleet_guard(self) -> None:
        canonical = self.fixture._canonical_fixture()
        try:
            first_plan = canonical.plan
            replacements = {first_plan["transaction_id"]: "txn-independent-second"}
            replacements.update({
                value: value + "-second"
                for value in first_plan["credential_nonce_ledger"]["reserved_nonces"]
            })

            def rebind(value):
                if isinstance(value, dict):
                    return {key: rebind(item) for key, item in value.items()}
                if isinstance(value, list):
                    return [rebind(item) for item in value]
                return replacements.get(value, value) if isinstance(value, str) else value

            second_plan = rebind(copy.deepcopy(first_plan))
            canonical.fixture._sign_semantic_fixture(second_plan)
            second_plan["plan_digest"] = canonical.adapter.canonical_plan_digest(second_plan)
            second_plan = copy.deepcopy(dict(second_plan))
            first_authority = canonical._authority(True, first_plan)
            second_authority = canonical._authority(True, second_plan)
            canonical.adapter.validate_authority(dict(first_plan), first_authority)
            canonical.adapter.validate_authority(dict(second_plan), second_authority)
            self.assertNotEqual(first_plan["transaction_id"], second_plan["transaction_id"])
            self.assertTrue(set(first_plan["credential_nonce_ledger"]["reserved_nonces"]).isdisjoint(
                second_plan["credential_nonce_ledger"]["reserved_nonces"]
            ))

            control = StorageFirstCanonicalTransport(canonical.adapter, second_plan)
            self.fixture._canonical_runner(
                canonical, control, plan=second_plan,
                journal_path=canonical.root / "independent-control.json",
            )
            canonical._write_ledger(
                canonical.ledger_path,
                [{
                    "schema_version": canonical.adapter.NONCE_ROW_SCHEMA,
                    "transaction_id": "fixture-existing-transaction",
                    "nonce": "fixture-existing-nonce",
                    "one_shot": True,
                }],
            )

            first = StorageFirstCanonicalTransport(canonical.adapter, first_plan)
            second = StorageFirstCanonicalTransport(canonical.adapter, second_plan)
            entered = threading.Event()
            release = threading.Event()
            original_mutate = first.mutate

            def hold_first(operation, node):
                if operation == "stop:storage-205":
                    entered.set()
                    if not release.wait(5):
                        raise RuntimeError("test release timeout")
                return original_mutate(operation, node)

            first.mutate = hold_first
            intrusions: list[str] = []

            def forbidden_inspect(node):
                intrusions.append(node["name"])
                raise AssertionError("second storage transaction reached provider")

            second.inspect_node = forbidden_inspect
            first_journal = canonical.root / "fleet-first.json"
            second_journal = canonical.root / "fleet-second.json"
            first_results: list[object] = []

            def run_first() -> None:
                try:
                    first_results.append(self.fixture._canonical_runner(
                        canonical, first, plan=first_plan, journal_path=first_journal,
                    ))
                except BaseException as error:
                    first_results.append(error)

            worker = threading.Thread(target=run_first)
            worker.start()
            try:
                self.assertTrue(entered.wait(5))
                with self.assertRaises(Exception):
                    self.fixture._canonical_runner(
                        canonical, second, plan=second_plan, journal_path=second_journal,
                    )
            finally:
                release.set()
                worker.join(10)
            self.assertFalse(worker.is_alive())
            self.assertEqual(intrusions, [])
            self.assertEqual(len(first_results), 1)
            self.assertIsInstance(first_results[0], dict)

            result = self.fixture._canonical_runner(
                canonical,
                StorageFirstCanonicalTransport(canonical.adapter, second_plan),
                plan=second_plan,
                journal_path=canonical.root / "fleet-second-after-release.json",
            )
            self.assertEqual(result["status"], "storage-205-verified")
        finally:
            canonical.tearDown()

    def test_formal_ops_sf_002_journal_and_ledger_alias_is_rejected_before_mutation(self) -> None:
        alias = self.fixture.root / "retained-ledger.jsonl"
        alias.write_text("retained-context\n", encoding="utf-8")
        before = alias.read_bytes()
        transport = self.fixture._Transport()
        with self.assertRaises(Exception):
            self._runner(
                transport, journal_path=alias, ledger_path=alias,
                live_revalidator=lambda: True,
            )
        self.assertEqual(alias.read_bytes(), before)
        self.assertEqual(transport.mutations, [])

    def test_formal_ops_sf_003_forged_receipt_cannot_authorize_completion(self) -> None:
        validator = getattr(self.adapter, "validate_storage_first_receipt", None)
        self.assertTrue(callable(validator), "RED: missing storage-first receipt validator")
        forged = {
            "schema_version": "oasis7.storage_first_receipt.v1",
            "phase_id": "storage-205-first",
            "operation": "verify:storage-205",
            "target": "storage-205",
            "observer_mutation": False,
            "completion_boundary": "storage-205-verified-pending-sequencer-probe",
            "authenticated": True,
            "verified": True,
            "transaction_id": self.plan["transaction_id"],
            "capture_window_id": self.plan["capture_window_id"],
        }
        with self.assertRaises(Exception):
            validator(forged)

    def test_formal_ops_sf_003_forged_verified_journal_cannot_skip_cursor(self) -> None:
        receipts = [
            {
                "schema_version": "oasis7.storage_first_receipt.v1",
                "phase_id": "storage-205-first",
                "operation": operation,
                "target": "storage-205",
                "observer_mutation": False,
                "completion_boundary": "storage-205-verified-pending-sequencer-probe",
                "verified": True,
            }
            for operation in (
                "stop:storage-205", "delete:storage-205", "rebuild:storage-205",
                "start:storage-205", "verify:storage-205",
            )
        ]
        journal = {
            "schema_version": "oasis7.storage_first_mutation_journal.v1",
            "phase_id": "storage-205-first",
            "status": "storage-205-verified",
            "next_operation": "reconciliation-required",
            "completed_operations": [
                "stop:storage-205", "delete:storage-205", "rebuild:storage-205",
                "start:storage-205", "verify:storage-205",
            ],
            "transaction_id": self.plan["transaction_id"],
            "capture_window_id": self.plan["capture_window_id"],
            "task_uid": self.plan["task_uid"],
            "head_oid": self.plan["head_oid"],
            "plan_digest": self.plan["plan_digest"],
            "phase_contract_digest": self.adapter._storage_first_phase_digest(
                self.plan, self.identity_map
            ),
            "ledger_path": str(self.fixture.root / "parent-nonce-ledger.jsonl"),
            "storage_receipts": receipts,
        }
        journal_path = self.fixture.root / "forged-verified.json"
        journal_path.write_text(json.dumps(journal), encoding="utf-8")
        with self.assertRaises(Exception):
            self.fixture._resume(journal_path)

    def test_formal_ops_sf_004_provider_evidence_is_required_before_next_operation(self) -> None:
        base = self.fixture._Transport

        class BareEvidenceTransport(base):
            def inspect_node(self, node):
                self.calls.append(f"inspect:{node['name']}")
                return {}

            def preflight(self, operation, node):
                self.calls.append(operation)
                return {"verified": True}

        transport = BareEvidenceTransport()
        with self.assertRaises(Exception):
            self._runner(transport, live_revalidator=lambda: True)
        self.assertEqual(transport.mutations, [])

    def test_formal_ops_sf_005_verify_uses_read_only_callback_not_mutate(self) -> None:
        canonical = self.fixture._canonical_fixture()
        base = StorageFirstCanonicalTransport

        class RecordingTransport(base):
            def __init__(self, adapter, plan):
                super().__init__(adapter, plan)
                self.verify_operations: list[str] = []

            def verify(self, operation, node):
                return super().verify(operation, node)

            def mutate(self, operation, node):
                if operation == "verify:storage-205":
                    self.mutations.append(operation)
                    return self._receipt(operation, self._storage_node())
                return super().mutate(operation, node)

        try:
            transport = RecordingTransport(canonical.adapter, canonical.plan)
            self.fixture._canonical_runner(canonical, transport)
            self.assertEqual(transport.verify_operations, ["verify:storage-205"])
            self.assertNotIn("verify:storage-205", transport.mutations)
        finally:
            canonical.tearDown()

    def test_formal_ops_sf_006_provenance_verifier_gets_sanitized_plan_and_bound_result(self) -> None:
        canonical = self.fixture._canonical_fixture()
        seen: list[dict[str, object]] = []

        def verifier(plan, receipt):
            seen.append(copy.deepcopy(plan))
            return {"verified": True}

        try:
            with self.assertRaises(Exception):
                self.fixture._canonical_runner(
                    canonical,
                    StorageFirstCanonicalTransport(canonical.adapter, canonical.plan),
                    provenance_verifier=verifier,
                )
            self.assertTrue(seen)
            self.assertNotIn("credential_seam", json.dumps(seen[0], sort_keys=True))
        finally:
            canonical.tearDown()

    def test_formal_ops_sf_007_non_boolean_live_revalidation_fails_closed(self) -> None:
        transport = self.fixture._Transport()
        with self.assertRaises(Exception):
            self._runner(transport, live_revalidator=lambda: "verified")
        self.assertEqual(transport.mutations, [])

    def test_formal_runtime_sf_005_journal_write_failure_persists_reconciliation(self) -> None:
        canonical = self.fixture._canonical_fixture()
        try:
            journal = canonical.root / "durability-failure.json"
            original = canonical.adapter._storage_first_journal_write
            failed = False

            def fail_after_stop(path, record):
                nonlocal failed
                if not failed and record.get("completed_operations") == ["stop:storage-205"]:
                    failed = True
                    raise OSError("durability injection after provider mutation")
                return original(path, record)

            with mock.patch.object(
                canonical.adapter, "_storage_first_journal_write", side_effect=fail_after_stop
            ):
                with self.assertRaises(Exception):
                    self.fixture._canonical_runner(
                        canonical,
                        StorageFirstCanonicalTransport(canonical.adapter, canonical.plan),
                        journal_path=journal,
                    )
            self.assertTrue(failed)
            self.assertTrue(journal.exists())
            record = json.loads(journal.read_text())
            self.assertEqual(record["next_operation"], "reconciliation-required")
            self.assertIn(record["status"], {"terminal-failure", "reconciliation-blocked"})
        finally:
            canonical.tearDown()

    def test_formal_runtime_sf_006_nonce_ledger_path_requires_bound_readback(self) -> None:
        unbound = self.fixture.root / "unbound-ledger.jsonl"
        unbound.write_text("", encoding="utf-8")
        transport = self.fixture._Transport()
        with self.assertRaises(Exception):
            self._runner(transport, ledger_path=unbound, live_revalidator=lambda: True)
        self.assertEqual(transport.mutations, [])

    def test_formal_qa_sf_007_cli_exposes_explicit_storage_phase(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plan = root / "plan.json"
            authority = root / "authority.json"
            journal = root / "journal.json"
            ledger = root / "ledger.jsonl"
            for path in (plan, authority, journal, ledger):
                path.write_text("{}", encoding="utf-8")
            authority_root = root / "authority-anchors"
            authority_root.mkdir(mode=0o700)
            public_key = authority_root / "provider-public-key"
            provider = authority_root / "provider-adapter"
            verifier_tool = authority_root / "identity-verifier"
            for path in (public_key, provider, verifier_tool):
                path.write_bytes(b"synthetic canonical authority fixture")
                path.chmod(0o600)
            trust_config = authority_root / "trust-config.json"
            registry = authority_root / "provider-registry.json"
            trust_config.write_text(
                json.dumps({"allowlist": [{"public_key_ref": str(public_key)}]}),
                encoding="utf-8",
            )
            registry.write_text(
                json.dumps({
                    "trust_config_path": str(trust_config),
                    "providers": [{
                        "public_key_ref": str(public_key),
                        "adapter_path": str(provider),
                    }],
                    "verifier": {"executable_path": str(verifier_tool)},
                }),
                encoding="utf-8",
            )
            planner = load_module("storage_first_cli_alias_planner", PLANNER_PATH)
            with mock.patch.object(self.adapter, "_PLANNER_MODULE", planner), \
                 mock.patch.object(planner, "IDENTITY_V2_TRUST_CONFIG_PATH", trust_config), \
                 mock.patch.object(planner, "IDENTITY_V2_PROVIDER_REGISTRY_PATH", registry), \
                 mock.patch.object(self.adapter, "execute_storage_first", return_value={"status": "dry-run"}) as child, \
                 mock.patch.object(self.adapter, "execute", return_value={"status": "legacy"}) as legacy:
                try:
                    result = self.adapter.main([
                        "--plan", str(plan), "--authority", str(authority),
                        "--journal", str(journal), "--ledger", str(ledger),
                        "--phase", "storage-205-first",
                    ])
                except SystemExit as error:
                    self.fail(f"storage-first phase is not reachable from the adapter CLI: {error.code}")
            self.assertEqual(result["status"], "dry-run")
            child.assert_called_once()
            legacy.assert_not_called()


class StorageFirstSecurityFollowupRedTests(unittest.TestCase):
    """Follow-up RED coverage for provider/authentication compatibility seams."""

    @classmethod
    def setUpClass(cls) -> None:
        cls.adapter = load_module("storage_first_security_followup_under_test", ADAPTER_PATH)

    def setUp(self) -> None:
        self.fixture = _storage_first_adapter_red_tests()("runTest")
        self.fixture.adapter = self.adapter
        self.fixture.setUp()
        bindings = getattr(self.adapter, "_STORAGE_FIRST_ADMISSION_BINDINGS", None)
        if isinstance(bindings, dict):
            bindings.clear()

    def tearDown(self) -> None:
        self.fixture.tearDown()

    def test_provider_receipt_cannot_synthesize_authentication_via_setdefault(self) -> None:
        binder = getattr(self.adapter, "_storage_first_bind_receipt", None)
        self.assertTrue(callable(binder), "RED: missing provider receipt binding boundary")
        operation = "stop:storage-205"
        raw_provider_receipt = {
            "schema_version": "oasis7.storage_first_receipt.v1",
            "phase_id": "storage-205-first",
            "operation": operation,
            "target": "storage-205",
            "observer_mutation": False,
            "completion_boundary": "storage-205-verified-pending-sequencer-probe",
        }
        with self.assertRaises(Exception):
            binder(self.fixture.plan, operation, raw_provider_receipt)

    def test_none_live_revalidation_fails_even_for_lambda(self) -> None:
        transport = self.fixture._Transport()
        with self.assertRaises(Exception):
            self.fixture._runner(transport, live_revalidator=lambda: None)
        self.assertEqual(transport.mutations, [])

    def test_unbound_provenance_result_fails_even_for_lambda(self) -> None:
        transport = self.fixture._Transport()
        with self.assertRaises(Exception):
            self.fixture._runner(
                transport,
                provenance_verifier=lambda plan, receipt: {"verified": True},
                live_revalidator=lambda: True,
            )
        self.assertEqual(transport.mutations, [])


if __name__ == "__main__":
    unittest.main()
