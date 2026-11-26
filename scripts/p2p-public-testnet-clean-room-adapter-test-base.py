#!/usr/bin/env python3
"""Shared unittest base class and fixture helpers for clean-room adapter tests."""

from __future__ import annotations

import copy
from datetime import timezone
import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import threading
import types
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
PLANNER_PATH = ROOT / "scripts" / "p2p-public-testnet-full-network-clean-room.py"
PLANNER_TEST_PATH = ROOT / "scripts" / "p2p-public-testnet-full-network-clean-room.test.py"
ADAPTER_PATH = ROOT / "scripts" / "p2p-public-testnet-full-network-clean-room-adapter.py"
FIXTURES_PATH = ROOT / "scripts" / "p2p-public-testnet-clean-room-adapter-test-fixtures.py"

_COMPILED_MODULE_CODE: dict[str, types.CodeType] = {}
_SHARED_PLANNER_FIXTURE_MODULE: types.ModuleType | None = None


def load_module(name: str, path: Path) -> types.ModuleType:
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
    existing = sys.modules.get(module_name)
    if existing is not None:
        return existing
    spec = importlib.util.spec_from_file_location(module_name, FIXTURES_PATH)
    if spec is None or spec.loader is None:
        raise AssertionError(f"cannot load module: {FIXTURES_PATH}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    return module


_ADAPTER_TEST_FIXTURES = _load_adapter_test_fixtures()
ApplyTransport = _ADAPTER_TEST_FIXTURES.ApplyTransport


class FullNetworkCleanRoomAdapterTestBase(unittest.TestCase):
    _owns_isolated_fixture_module = False

    @classmethod
    def _current_load_module(cls):
        owner_module = sys.modules.get(cls.__module__)
        if owner_module is not None:
            owner_loader = getattr(owner_module, "load_module", None)
            if callable(owner_loader):
                return owner_loader
        return load_module

    @classmethod
    def setUpClass(cls):
        loader = cls._current_load_module()
        if loader is _DEFAULT_LOAD_MODULE:
            cls.fixture_module = _shared_planner_fixture_module()
            cls._owns_isolated_fixture_module = False
        else:
            cls.fixture_module = loader("full_network_clean_room_fixture", PLANNER_TEST_PATH)
            cls.fixture_module.FullNetworkCleanRoomPlanTests.setUpClass()
            cls._owns_isolated_fixture_module = True

    @classmethod
    def tearDownClass(cls):
        if cls._owns_isolated_fixture_module:
            cls.fixture_module.FullNetworkCleanRoomPlanTests.tearDownClass()
            cls._owns_isolated_fixture_module = False

    def setUp(self) -> None:
        loader = self._current_load_module()
        self.planner = loader("full_network_clean_room", PLANNER_PATH)
        self.adapter = loader("full_network_clean_room_adapter", ADAPTER_PATH)
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

    def _recovery_verifier(self, plan, receipt):
        return {"verified": True, "bindings": receipt["bindings"],
                "verifier_id": self.adapter.CANONICAL_VERIFIER_ID,
                "trust_root_id": self.adapter.CANONICAL_TRUST_ROOT_ID,
                "signer_id": "governance-signer"}

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
