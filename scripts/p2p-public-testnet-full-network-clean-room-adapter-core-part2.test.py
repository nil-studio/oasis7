#!/usr/bin/env python3
"""Standalone core part-2 tests extracted from clean-room adapter tests."""

from __future__ import annotations

import copy
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import types
import unittest

ROOT = Path(__file__).resolve().parents[1]
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
FullNetworkCleanRoomAdapterTestBase = _ADAPTER_TEST_BASE.FullNetworkCleanRoomAdapterTestBase
_ADAPTER_TEST_FIXTURES = _ADAPTER_TEST_BASE._ADAPTER_TEST_FIXTURES
ApplyTransport = _ADAPTER_TEST_FIXTURES.ApplyTransport
FakeTransport = _ADAPTER_TEST_FIXTURES.FakeTransport


class FullNetworkCleanRoomAdapterCorePart2Tests(FullNetworkCleanRoomAdapterTestBase):
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


if __name__ == "__main__":
    unittest.main()
