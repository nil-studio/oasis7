#!/usr/bin/env python3
"""Standalone storage-first blockchain P1 part-1 RED test suite extracted from clean-room adapter tests."""

from __future__ import annotations

from collections.abc import Mapping
import copy
import importlib.util
import json
from pathlib import Path
import sys
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
StorageFirstCanonicalPlan = _ADAPTER_TEST_FIXTURES.StorageFirstCanonicalPlan
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


class StorageFirstBlockchainP1Part1RedTests(unittest.TestCase):
    """RED regressions (part 1) for the five blockchain-ops P1 bypass classes."""

    @classmethod
    def setUpClass(cls) -> None:
        cls.adapter = load_module(
            "storage_first_blockchain_p1_part1_adapter_under_test", ADAPTER_PATH
        )

    def setUp(self) -> None:
        self.fixture = _storage_first_adapter_red_tests()("runTest")
        self.fixture.adapter = self.adapter
        self.fixture.setUp()

    def tearDown(self) -> None:
        self.fixture.tearDown()

    def test_p1_side_effect_operation_cannot_bypass_canonical_admission(self) -> None:
        """A caller-controlled fault marker must not authorize shape-only apply."""
        transport = self.fixture._Transport(
            side_effect_operation="never-called-sentinel"
        )
        with self.assertRaises(Exception):
            self.fixture._runner(transport, live_revalidator=lambda: True)
        self.assertEqual(transport.mutations, [])

    def test_p1_identity_less_child_provenance_is_rejected_before_mutation(self) -> None:
        canonical = self.fixture._canonical_fixture()
        try:
            transport = StorageFirstCanonicalTransport(
                canonical.adapter, canonical.plan
            )

            def identity_less_child(verifier_plan, receipt):
                # Keep the parent authority receipt fully authenticated, while
                # making the phase callback result deliberately omit its
                # identity-bound signer/verifier/trust-root fields.
                if isinstance(receipt, dict) and "bindings" in receipt:
                    return canonical._recovery_verifier(verifier_plan, receipt)
                return {"verified": True, "bindings": receipt}

            with self.assertRaises(Exception):
                self.fixture._canonical_runner(
                    canonical,
                    transport,
                    provenance_verifier=identity_less_child,
                )
            self.assertEqual(transport.mutations, [])
        finally:
            canonical.tearDown()

    def test_p1_custom_mapping_cannot_forge_child_projection_for_identityless_provenance(self) -> None:
        canonical = self.fixture._canonical_fixture()
        try:
            class ForgedChildProjection(Mapping):
                """Caller mapping with divergent get/dict views."""

                def __init__(self, source):
                    self.source = source

                def __getitem__(self, key):
                    return self.source[key]

                def __iter__(self):
                    return iter(self.source.keys())

                def __len__(self):
                    return len(self.source)

                def keys(self):
                    return self.source.keys()

                def get(self, key, default=None):
                    if key == "global_order":
                        return list(STORAGE_FIRST_CHILD_OPERATIONS)
                    return self.source.get(key, default)

            forged_plan = ForgedChildProjection(canonical.plan)
            transport = StorageFirstCanonicalTransport(canonical.adapter, forged_plan)

            def identity_less_child(verifier_plan, receipt):
                if isinstance(receipt, dict) and "bindings" in receipt:
                    return canonical._recovery_verifier(verifier_plan, receipt)
                return {"verified": True, "bindings": receipt}

            with self.assertRaises(Exception):
                self.fixture._canonical_runner(
                    canonical,
                    transport,
                    plan=forged_plan,
                    provenance_verifier=identity_less_child,
                )
            self.assertEqual(transport.mutations, [])
        finally:
            canonical.tearDown()

    def test_runtime_p1_dict_subclass_nodes_redirect_rejected_before_effects(self) -> None:
        """A dict subclass must not split canonical bytes from callback targets."""
        canonical = self.fixture._canonical_fixture()
        try:
            class RedirectedNodesPlan(StorageFirstCanonicalPlan):
                """Return attacker targets from get('nodes') only."""

                def get(self, key, default=None):
                    if key != "nodes":
                        return super().get(key, default)
                    nodes = copy.deepcopy(super().get(key, default))
                    for node in nodes:
                        if node.get("name") == "storage-205":
                            node["endpoints"] = {
                                **node["endpoints"],
                                "healthz": "https://attacker.invalid/healthz",
                                "evidence": "https://attacker.invalid/evidence",
                            }
                    return nodes

            forged_plan = RedirectedNodesPlan(canonical.plan)
            self.assertEqual(dict(forged_plan)["nodes"], canonical.plan["nodes"])
            self.assertNotEqual(forged_plan.get("nodes"), dict(forged_plan)["nodes"])
            redirected_storage = next(
                node for node in forged_plan.get("nodes")
                if node["name"] == "storage-205"
            )
            self.assertEqual(
                redirected_storage["endpoints"]["evidence"],
                "https://attacker.invalid/evidence",
            )

            def assert_rejected_without_effects(label, invoke, transport, journal, before=None):
                effects = {"child_journal": 0, "parent_journal": 0, "fleet_lock": 0}

                def trap(effect):
                    def record(*args, **kwargs):
                        effects[effect] += 1
                        raise AssertionError(f"unexpected {effect} effect")

                    return record

                caught = None
                with mock.patch.object(
                    canonical.adapter,
                    "_storage_first_journal_write",
                    side_effect=trap("child_journal"),
                ), mock.patch.object(
                    canonical.adapter,
                    "_write_journal",
                    side_effect=trap("parent_journal"),
                ), mock.patch.object(
                    canonical.adapter,
                    "_acquire_fleet_transaction_guard",
                    side_effect=trap("fleet_lock"),
                ):
                    try:
                        invoke()
                    except Exception as error:
                        caught = error

                self.assertEqual(
                    effects,
                    {"child_journal": 0, "parent_journal": 0, "fleet_lock": 0},
                    f"{label} crossed a protected side-effect boundary",
                )
                self.assertEqual(transport.mutations, [], f"{label} reached provider mutation")
                if before is not None:
                    self.assertEqual(journal.read_bytes(), before, f"{label} changed its journal")
                self.assertIsNotNone(caught, f"{label} accepted redirected provider targets")

            execute_journal = canonical.root / "dict-subclass-execute.journal.json"
            execute_transport = StorageFirstCanonicalTransport(
                canonical.adapter, forged_plan
            )
            assert_rejected_without_effects(
                "execute",
                lambda: self.fixture._canonical_runner(
                    canonical,
                    execute_transport,
                    plan=forged_plan,
                    journal_path=execute_journal,
                ),
                execute_transport,
                execute_journal,
            )

            resume_journal = self.fixture._canonical_prefix_journal(
                canonical,
                ["stop:storage-205"],
                name="dict-subclass-resume.journal.json",
            )
            resume_before = resume_journal.read_bytes()
            canonical.plan = forged_plan
            resume_transport = StorageFirstCanonicalTransport(
                canonical.adapter, forged_plan
            )
            assert_rejected_without_effects(
                "resume",
                lambda: self.fixture._canonical_resume(
                    canonical, resume_journal, resume_transport
                ),
                resume_transport,
                resume_journal,
                resume_before,
            )
        finally:
            canonical.tearDown()

    def test_runtime_p1_nested_node_dict_subclass_cannot_redirect_child_projection(self) -> None:
        """Nested dict subclasses must not alter the private child view."""
        canonical = self.fixture._canonical_fixture()
        try:
            class RedirectOnDeepcopyNode(dict):
                """Return attacker endpoints only when the child copies nodes."""

                def __deepcopy__(self, memo):
                    copied = {
                        key: copy.deepcopy(value, memo) for key, value in self.items()
                    }
                    if copied.get("name") == "storage-205":
                        copied["endpoints"] = {
                            "healthz": "https://attacker.invalid/healthz",
                            "evidence": "https://attacker.invalid/evidence",
                        }
                    return copied

            forged_plan = copy.deepcopy(dict(canonical.plan))
            forged_plan["nodes"][0] = RedirectOnDeepcopyNode(
                canonical.plan["nodes"][0]
            )
            self.assertIs(type(forged_plan), dict)
            self.assertEqual(
                dict(forged_plan["nodes"][0]), canonical.plan["nodes"][0]
            )

            def assert_rejected_without_effects(label, invoke, transport, journal, before=None):
                effects = {"child_journal": 0, "parent_journal": 0, "fleet_lock": 0}

                def trap(effect):
                    def record(*args, **kwargs):
                        effects[effect] += 1
                        raise AssertionError(f"unexpected {effect} effect")

                    return record

                caught = None
                with mock.patch.object(
                    canonical.adapter,
                    "_storage_first_journal_write",
                    side_effect=trap("child_journal"),
                ), mock.patch.object(
                    canonical.adapter,
                    "_write_journal",
                    side_effect=trap("parent_journal"),
                ), mock.patch.object(
                    canonical.adapter,
                    "_acquire_fleet_transaction_guard",
                    side_effect=trap("fleet_lock"),
                ):
                    try:
                        invoke()
                    except Exception as error:
                        caught = error

                self.assertEqual(
                    effects,
                    {"child_journal": 0, "parent_journal": 0, "fleet_lock": 0},
                    f"{label} crossed a protected side-effect boundary",
                )
                self.assertEqual(transport.mutations, [], f"{label} reached provider mutation")
                if before is not None:
                    self.assertEqual(journal.read_bytes(), before, f"{label} changed its journal")
                else:
                    self.assertFalse(journal.exists(), f"{label} created a journal")
                self.assertIsNotNone(caught, f"{label} accepted a nested node projection")

            execute_journal = canonical.root / "nested-dict-subclass-execute.journal.json"
            execute_transport = StorageFirstCanonicalTransport(canonical.adapter, forged_plan)
            assert_rejected_without_effects(
                "execute",
                lambda: self.fixture._canonical_runner(
                    canonical,
                    execute_transport,
                    plan=forged_plan,
                    journal_path=execute_journal,
                ),
                execute_transport,
                execute_journal,
            )

            resume_journal = self.fixture._canonical_prefix_journal(
                canonical,
                ["stop:storage-205"],
                name="nested-dict-subclass-resume.journal.json",
            )
            resume_before = resume_journal.read_bytes()
            canonical.plan = forged_plan
            resume_transport = StorageFirstCanonicalTransport(canonical.adapter, forged_plan)
            assert_rejected_without_effects(
                "resume",
                lambda: self.fixture._canonical_resume(
                    canonical, resume_journal, resume_transport
                ),
                resume_transport,
                resume_journal,
                resume_before,
            )
        finally:
            canonical.tearDown()

    def test_runtime_p1_authority_dict_subclass_cannot_forge_child_authorization(self) -> None:
        """Child authority reads must use the canonical parent authority view."""
        canonical = self.fixture._canonical_fixture()
        try:
            class ForgedChildAuthority(dict):
                """Expose apply=false to parent gates but true to child reads."""

                def __init__(self, value, plan):
                    super().__init__(value)
                    self._child = {
                        "action": "storage-205-first",
                        "targets": ["storage-205"],
                        "task_uid": plan["task_uid"],
                        "frozen_head_oid": plan["head_oid"],
                        "plan_digest": plan["plan_digest"],
                        "transaction_id": plan["transaction_id"],
                        "capture_window_id": plan["capture_window_id"],
                        "current_authorization": True,
                        "signed": True,
                        "expires_at": "2099-01-01T00:00:00Z",
                    }

                def get(self, key, default=None):
                    return self._child.get(key, super().get(key, default))

            forged_authority = ForgedChildAuthority(
                canonical._authority(False, canonical.plan), canonical.plan
            )
            self.assertIs(type(dict(forged_authority)), dict)
            self.assertFalse(dict(forged_authority)["apply_authorized"])
            self.assertTrue(forged_authority.get("current_authorization"))

            transport = StorageFirstCanonicalTransport(
                canonical.adapter, canonical.plan
            )
            journal = canonical.root / "authority-dict-subclass.journal.json"
            effects = {"journal": 0, "lock": 0}

            def trap(effect):
                def record(*args, **kwargs):
                    effects[effect] += 1
                    raise AssertionError(f"unexpected {effect} effect")

                return record

            def canonical_provenance_verifier(verifier_plan, receipt):
                if "bindings" in receipt:
                    return canonical._recovery_verifier(verifier_plan, receipt)
                return {
                    "verified": True,
                    "bindings": receipt,
                    "verifier_id": canonical.adapter.CANONICAL_VERIFIER_ID,
                    "trust_root_id": canonical.adapter.CANONICAL_TRUST_ROOT_ID,
                    "signer_id": "governance-signer",
                }

            caught = None
            with mock.patch.object(
                canonical.adapter,
                "_storage_first_journal_write",
                side_effect=trap("journal"),
            ), mock.patch.object(
                canonical.adapter,
                "_write_journal",
                side_effect=trap("journal"),
            ), mock.patch.object(
                canonical.adapter,
                "_acquire_fleet_transaction_guard",
                side_effect=trap("lock"),
            ):
                try:
                    canonical.adapter.execute_storage_first(
                        canonical.plan,
                        forged_authority,
                        phase="storage-205-first",
                        identity_v2_evidence=canonical.identity_v2_evidence,
                        journal_path=journal,
                        ledger_path=canonical.ledger_path,
                        transport=transport,
                        dry_run=False,
                        provenance_verifier=canonical_provenance_verifier,
                        live_revalidator=lambda: True,
                    )
                except Exception as error:
                    caught = error

            self.assertIsNotNone(caught, "forged child authority was accepted")
            self.assertEqual(effects, {"journal": 0, "lock": 0})
            self.assertEqual(transport.mutations, [])
            self.assertFalse(journal.exists())
        finally:
            canonical.tearDown()

    def test_ops_p1_storage_journal_cannot_alias_governance_root(self) -> None:
        """Storage output paths must protect the code-owned governance root."""
        canonical = self.fixture._canonical_fixture()
        try:
            trust_root = Path(canonical.adapter.CANONICAL_TRUST_ROOT_PATH)
            trust_root_before = (
                trust_root.read_bytes() if trust_root.is_file() else None
            )
            fixture_before = canonical.adapter.CANONICAL_TRUST_ROOT_FIXTURE_PATH.read_bytes()
            prefix = self.fixture._canonical_prefix_journal(
                canonical,
                ["stop:storage-205"],
                name="trust-root-resume-prefix.journal.json",
            )
            resume_record = json.loads(prefix.read_text(encoding="utf-8"))

            def run_case(label, invoke, transport, reader=None):
                effects = {"journal": 0, "lock": 0}

                def trap(effect):
                    def record(*args, **kwargs):
                        effects[effect] += 1
                        raise AssertionError(f"unexpected {effect} effect")

                    return record

                caught = None
                with mock.patch.object(
                    canonical.adapter,
                    "_storage_first_journal_write",
                    side_effect=trap("journal"),
                ), mock.patch.object(
                    canonical.adapter,
                    "_write_journal",
                    side_effect=trap("journal"),
                ), mock.patch.object(
                    canonical.adapter,
                    "_acquire_fleet_transaction_guard",
                    side_effect=trap("lock"),
                ):
                    reader_context = (
                        mock.patch.object(
                            canonical.adapter,
                            "_storage_first_read_journal",
                            side_effect=reader,
                        )
                        if reader is not None
                        else mock.patch.object(
                            canonical.adapter,
                            "_storage_first_read_journal",
                        )
                    )
                    with reader_context as read_journal:
                        try:
                            invoke()
                        except Exception as error:
                            caught = error

                if trust_root_before is None:
                    self.assertFalse(trust_root.exists())
                else:
                    self.assertEqual(trust_root.read_bytes(), trust_root_before)
                self.assertFalse(Path(f"{trust_root}.lock").exists())
                self.assertFalse(Path(f"{trust_root}.emergency.json").exists())
                return {
                    "caught": caught,
                    "effects": effects,
                    "operations": list(transport.operations),
                    "mutations": list(transport.mutations),
                    "read_calls": read_journal.call_count,
                }

            execute_transport = StorageFirstCanonicalTransport(
                canonical.adapter, canonical.plan
            )
            execute_result = run_case(
                "execute",
                lambda: self.fixture._canonical_runner(
                    canonical,
                    execute_transport,
                    journal_path=trust_root,
                ),
                execute_transport,
            )

            resume_transport = StorageFirstCanonicalTransport(
                canonical.adapter, canonical.plan
            )
            resume_result = run_case(
                "resume",
                lambda: self.fixture._canonical_resume(
                    canonical,
                    trust_root,
                    resume_transport,
                ),
                resume_transport,
                reader=lambda path: resume_record,
            )
            for label, result, reader in (
                ("execute", execute_result, None),
                ("resume", resume_result, resume_record),
            ):
                with self.subTest(case=label):
                    self.assertIsNotNone(
                        result["caught"],
                        f"{label} accepted a protected output alias",
                    )
                    self.assertEqual(result["effects"], {"journal": 0, "lock": 0})
                    self.assertEqual(
                        result["operations"], [],
                        f"{label} reached provider callbacks",
                    )
                    self.assertEqual(
                        result["mutations"], [],
                        f"{label} reached provider mutation",
                    )
                    if reader is not None:
                        self.assertEqual(result["read_calls"], 0)
            self.assertEqual(
                canonical.adapter.CANONICAL_TRUST_ROOT_FIXTURE_PATH.read_bytes(),
                fixture_before,
            )
        finally:
            canonical.tearDown()


if __name__ == "__main__":
    unittest.main()
