#!/usr/bin/env python3
"""Standalone boundary and fleet-lock protection test suites extracted from clean-room adapter tests."""

from __future__ import annotations

from contextlib import ExitStack
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
PLANNER_PATH = ROOT / "scripts" / "p2p-public-testnet-full-network-clean-room.py"
PLANNER_TEST_PATH = ROOT / "scripts" / "p2p-public-testnet-full-network-clean-room.test.py"
ADAPTER_PATH = ROOT / "scripts" / "p2p-public-testnet-full-network-clean-room-adapter.py"
ADAPTER_TEST_PATH = ROOT / "scripts" / "p2p-public-testnet-full-network-clean-room-adapter.test.py"


def load_module(name: str, path: Path) -> types.ModuleType:
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise AssertionError(f"cannot load module: {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


_ADAPTER_TEST_MODULE: types.ModuleType | None = None


def _adapter_test_module() -> types.ModuleType:
    global _ADAPTER_TEST_MODULE
    if _ADAPTER_TEST_MODULE is None:
        _ADAPTER_TEST_MODULE = load_module(
            "p2p_public_testnet_clean_room_adapter_base_tests", ADAPTER_TEST_PATH
        )
    return _ADAPTER_TEST_MODULE


class JournalLedgerAliasTests(unittest.TestCase):
    def test_retained_evidence_alias_matrix_is_rejected_before_lock(self):
        adapter = load_module("retained_alias_adapter", ADAPTER_PATH)
        fields = (
            "raw_v1", "prepare_manifest", "payload", "provider_attestation",
            "unsigned_envelope", "signed_envelope", "verification",
        )
        locations = [("context", None), ("plan_intent", None)] + [
            (field, node) for node in range(5) for field in fields
        ] + [("consumer_impact_record", None)]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for index, (field, node) in enumerate(locations):
                for output_suffix in ("", ".lock", ".emergency.json"):
                    for alias_kind in ("direct", "hardlink", "symlink"):
                        case = root / f"{index}-{len(output_suffix)}-{alias_kind}"
                        case.mkdir()
                        journal = case / "journal.json"
                        output = Path(f"{journal}{output_suffix}")
                        retained = output if alias_kind == "direct" else case / "retained.json"
                        retained.write_bytes(b"retained evidence bytes\n")
                        if alias_kind == "hardlink":
                            os.link(retained, output)
                        elif alias_kind == "symlink":
                            output.symlink_to(retained)
                        descriptor = {"path": str(retained)}
                        evidence = {"entries": [{} for _ in range(5)]}
                        plan = {"identity_v2_evidence": evidence}
                        if field == "consumer_impact_record":
                            plan[field] = descriptor
                        elif node is None:
                            evidence[field] = descriptor
                        else:
                            evidence["entries"][node][field] = descriptor
                        for dry_run in (True, False):
                            for resume in (False, True):
                                with self.subTest(field=field, node=node, output=output_suffix, alias=alias_kind, dry_run=dry_run, resume=resume):
                                    with mock.patch.object(adapter, "_acquire_transaction_lock") as lock, mock.patch.object(adapter, "_release_transaction_lock"), mock.patch.object(adapter, "_execute_unlocked"), mock.patch.object(adapter, "_resume_transaction_unlocked"):
                                        with self.assertRaisesRegex(adapter.AdapterError, "alias"):
                                            if resume:
                                                adapter.resume_transaction(plan, {}, journal, ledger_path=case / "ledger", dry_run=dry_run)
                                            else:
                                                adapter.execute(plan, {}, journal_path=journal, ledger_path=case / "ledger", dry_run=dry_run)
                                        lock.assert_not_called()
                                    self.assertEqual(retained.read_bytes(), b"retained evidence bytes\n")

    def test_cli_input_aliases_are_rejected_before_execute(self):
        adapter = load_module("cli_input_alias_adapter", ADAPTER_PATH)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for input_name in ("plan", "authority", "evidence-map"):
                for suffix in ("", ".lock", ".emergency.json"):
                    case = root / f"{input_name}-{len(suffix)}"
                    case.mkdir()
                    journal = case / "journal.json"
                    paths = {name: case / f"{name}.json" for name in ("plan", "authority", "evidence-map")}
                    paths[input_name] = Path(f"{journal}{suffix}")
                    for path in paths.values():
                        path.write_bytes(b"{}")
                    argv = ["--plan", str(paths["plan"]), "--authority", str(paths["authority"]), "--journal", str(journal), "--ledger", str(case / "ledger")]
                    if input_name == "evidence-map":
                        argv += ["--identity-v2-evidence-map", str(paths[input_name]), "--identity-v2-mode", "current_admission"]
                    with self.subTest(input=input_name, suffix=suffix):
                        with mock.patch.object(adapter, "execute", return_value={}) as execute, mock.patch.object(adapter, "validate_authority"), mock.patch.object(adapter, "_current_identity_v2_admission", return_value={}):
                            with self.assertRaisesRegex(adapter.AdapterError, "alias"):
                                adapter.main(argv)
                            execute.assert_not_called()
                        for path in paths.values():
                            self.assertEqual(path.read_bytes(), b"{}")

    def test_lock_and_emergency_outputs_cannot_alias_ledger(self):
        adapter = load_module("alias_aux_adapter", ADAPTER_PATH)
        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / "journal.json"
            for suffix in (".lock", ".emergency.json"):
                ledger = Path(f"{journal}{suffix}")
                ledger.write_bytes(b"nonce history\n")
                with mock.patch.object(adapter, "_acquire_transaction_lock") as lock:
                    with self.assertRaises(adapter.AdapterError):
                        adapter.execute({}, {}, journal_path=journal, ledger_path=ledger)
                    lock.assert_not_called()
                self.assertEqual(ledger.read_bytes(), b"nonce history\n")

    def test_execute_and_resume_reject_journal_ledger_alias_before_lock(self):
        adapter = load_module("alias_adapter", ADAPTER_PATH)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            ledger = root / "nonce.jsonl"
            ledger.write_bytes(b"retained nonce history\n")
            aliases = [ledger, root / "hardlink", root / "symlink"]
            os.link(ledger, aliases[1])
            aliases[2].symlink_to(ledger)
            for journal in aliases:
                for dry_run in (True, False):
                    for resume in (False, True):
                        with self.subTest(journal=journal.name, dry_run=dry_run, resume=resume):
                            with mock.patch.object(adapter, "_acquire_transaction_lock") as lock, mock.patch.object(adapter, "_release_transaction_lock"), mock.patch.object(adapter, "_execute_unlocked"), mock.patch.object(adapter, "_resume_transaction_unlocked"):
                                with self.assertRaises(adapter.AdapterError):
                                    if resume:
                                        adapter.resume_transaction({}, {}, journal, ledger_path=ledger, dry_run=dry_run)
                                    else:
                                        adapter.execute({}, {}, journal_path=journal, ledger_path=ledger, dry_run=dry_run)
                                lock.assert_not_called()
                            self.assertEqual(ledger.read_bytes(), b"retained nonce history\n")


class ReviewSixBoundaryTests(unittest.TestCase):
    def _anchors(self, adapter, root):
        planner = load_module("review_six_planner", PLANNER_PATH)
        adapter._PLANNER_MODULE = planner
        planner.IDENTITY_V2_PROVIDER_REGISTRY_PATH = root / "registry.json"
        planner.IDENTITY_V2_TRUST_CONFIG_PATH = root / "trust.json"
        artifacts = [root / name for name in ("key-a", "adapter-a", "key-b", "adapter-b", "verifier", "retired-key")]
        for path in artifacts:
            path.write_bytes(b"retained authority")
            path.chmod(0o600)
        registry = {"trust_config_path": str(planner.IDENTITY_V2_TRUST_CONFIG_PATH), "providers": [
            {"public_key_ref": str(artifacts[0]), "adapter_path": str(artifacts[1])},
            {"public_key_ref": str(artifacts[2]), "adapter_path": str(artifacts[3])},
        ], "verifier": {"executable_path": str(artifacts[4])}}
        trust = {"allowlist": [{"public_key_ref": str(path)} for path in (artifacts[0], artifacts[2], artifacts[5])]}
        planner.IDENTITY_V2_PROVIDER_REGISTRY_PATH.write_text(json.dumps(registry))
        planner.IDENTITY_V2_TRUST_CONFIG_PATH.write_text(json.dumps(trust))
        return planner, artifacts

    def test_registry_and_trust_reference_alias_closure(self):
        adapter = load_module("review_six_alias", ADAPTER_PATH)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            _, artifacts = self._anchors(adapter, root)
            for index, retained in enumerate(artifacts):
                for suffix in ("", ".lock", ".emergency.json"):
                    for kind in ("direct", "hardlink", "symlink"):
                        with self.subTest(reference=index, suffix=suffix, alias=kind):
                            journal = root / f"output-{index}-{len(suffix)}-{kind}"
                            output = Path(f"{journal}{suffix}")
                            if kind == "direct":
                                # Change only the declared reference, not its meaning.
                                retained.rename(output)
                                for anchor in (adapter._PLANNER_MODULE.IDENTITY_V2_PROVIDER_REGISTRY_PATH, adapter._PLANNER_MODULE.IDENTITY_V2_TRUST_CONFIG_PATH):
                                    anchor.write_text(anchor.read_text().replace(str(retained), str(output)))
                            elif kind == "hardlink":
                                os.link(retained, output)
                            else:
                                output.symlink_to(retained)
                            snapshot = (output.read_bytes(), output.stat().st_mode)
                            try:
                                with self.assertRaisesRegex(adapter.AdapterError, "alias"):
                                    adapter._reject_journal_input_aliases(journal, root / "ledger", {})
                            finally:
                                self.assertEqual((output.read_bytes(), output.stat().st_mode), snapshot)
                                if kind == "direct":
                                    output.rename(retained)
                                    for anchor in (adapter._PLANNER_MODULE.IDENTITY_V2_PROVIDER_REGISTRY_PATH, adapter._PLANNER_MODULE.IDENTITY_V2_TRUST_CONFIG_PATH):
                                        anchor.write_text(anchor.read_text().replace(str(output), str(retained)))

    def test_authority_anchor_read_failure_blocks_before_lock(self):
        adapter = load_module("review_six_unreadable", ADAPTER_PATH)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            planner, _ = self._anchors(adapter, root)
            for field in ("IDENTITY_V2_PROVIDER_REGISTRY_PATH", "IDENTITY_V2_TRUST_CONFIG_PATH"):
                path = getattr(planner, field)
                original = path.read_bytes()
                for failure in ("malformed", "missing"):
                    with self.subTest(anchor=field, failure=failure):
                        if failure == "malformed":
                            path.write_bytes(b"not json")
                        else:
                            path.unlink()
                        try:
                            with mock.patch.object(adapter, "_acquire_transaction_lock") as acquire, mock.patch.object(adapter, "_release_transaction_lock"), mock.patch.object(adapter, "_execute_unlocked"):
                                with self.assertRaises(adapter.AdapterError):
                                    adapter.execute({}, {}, journal_path=root / "journal", ledger_path=root / "ledger")
                                acquire.assert_not_called()
                        finally:
                            path.write_bytes(original)

    def test_registry_key_api_alias_fails_before_lock_and_preserves_mode(self):
        adapter = load_module("review_six_api_alias", ADAPTER_PATH)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            _, artifacts = self._anchors(adapter, root)
            for resume in (False, True):
                for dry_run in (False, True):
                    with self.subTest(resume=resume, dry_run=dry_run):
                        key = artifacts[2]
                        before = (key.read_bytes(), key.stat().st_mode)
                        try:
                            with mock.patch.object(adapter, "_acquire_transaction_lock") as acquire, mock.patch.object(adapter, "_release_transaction_lock"), mock.patch.object(adapter, "_execute_unlocked"), mock.patch.object(adapter, "_resume_transaction_unlocked"):
                                with self.assertRaisesRegex(adapter.AdapterError, "alias"):
                                    if resume:
                                        adapter.resume_transaction({}, {}, key, ledger_path=root / "ledger", dry_run=dry_run)
                                    else:
                                        adapter.execute({}, {}, journal_path=key, ledger_path=root / "ledger", dry_run=dry_run)
                                acquire.assert_not_called()
                        finally:
                            self.assertEqual((key.read_bytes(), key.stat().st_mode), before)

    def test_transaction_lock_open_and_flock_identity_races(self):
        import fcntl
        adapter = load_module("review_six_lock", ADAPTER_PATH)
        real_open, real_flock = os.open, fcntl.flock
        for boundary in ("open", "flock"):
            for target in ("lock", "parent"):
                with self.subTest(boundary=boundary, target=target), tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    parent = root / "parent"
                    parent.mkdir(mode=0o700)
                    journal = parent / "journal"
                    lock = Path(f"{journal}.lock")
                    swapped = False
                    def replace():
                        nonlocal swapped
                        if swapped:
                            return
                        swapped = True
                        if target == "parent":
                            parent.rename(root / "old-parent")
                            parent.mkdir(mode=0o700)
                        else:
                            lock.unlink()
                        lock.write_bytes(b"")
                        lock.chmod(0o600)
                    def opening(path, flags, *args, **kwargs):
                        fd = real_open(path, flags, *args, **kwargs)
                        if boundary == "open" and Path(path) == lock:
                            replace()
                        return fd
                    def flocking(fd, operation):
                        result = real_flock(fd, operation)
                        if boundary == "flock" and operation & fcntl.LOCK_EX:
                            replace()
                        return result
                    handle = None
                    try:
                        with mock.patch.object(os, "open", side_effect=opening), mock.patch.object(fcntl, "flock", side_effect=flocking):
                            with self.assertRaises(adapter.AdapterError):
                                handle = adapter._acquire_transaction_lock(journal)
                        self.assertTrue(swapped)
                    finally:
                        if handle is not None:
                            adapter._release_transaction_lock(handle)

    def test_transaction_lock_rejects_writable_parent(self):
        adapter = load_module("review_six_parent", ADAPTER_PATH)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            root.chmod(0o777)
            handle = None
            try:
                with self.assertRaises(adapter.AdapterError):
                    handle = adapter._acquire_transaction_lock(root / "journal")
            finally:
                if handle is not None:
                    adapter._release_transaction_lock(handle)

    def test_held_lock_drift_blocks_next_journal_effect(self):
        adapter = load_module("review_six_lifetime", ADAPTER_PATH)
        for resume in (False, True):
            with self.subTest(resume=resume), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                journal = root / "journal"
                journal.write_bytes(b"retained journal")
                journal.chmod(0o600)
                def unlocked(*args, **kwargs):
                    lock = Path(f"{journal}.lock")
                    # An unchanged persistent inode must continue to exclude a
                    # second holder. Replacement must not make the first safe.
                    with self.assertRaisesRegex(adapter.AdapterError, "locked"):
                        adapter._acquire_transaction_lock(journal)
                    lock.unlink()
                    lock.write_bytes(b"")
                    lock.chmod(0o600)
                    adapter._write_journal(journal, {"status": "must-not-persist"})
                with mock.patch.object(adapter, "_reject_journal_input_aliases"), mock.patch.object(adapter, "_execute_unlocked", side_effect=unlocked), mock.patch.object(adapter, "_resume_transaction_unlocked", side_effect=unlocked):
                    with self.assertRaises(adapter.AdapterError):
                        if resume:
                            adapter.resume_transaction({}, {}, journal, ledger_path=root / "ledger")
                        else:
                            adapter.execute({}, {}, journal_path=journal, ledger_path=root / "ledger")
                self.assertEqual(journal.read_bytes(), b"retained journal")


class ReviewFiveBoundaryTests(unittest.TestCase):
    def test_nonce_replacement_after_lock_cannot_report_reservation(self):
        import fcntl
        adapter = load_module("nonce_race_adapter", ADAPTER_PATH)
        real_flock = fcntl.flock
        with tempfile.TemporaryDirectory() as directory:
            ledger = Path(directory) / "ledger"
            replacement = Path(directory) / "replacement"
            for path in (ledger, replacement):
                path.write_text("")
                path.chmod(0o600)
            def replace_after_lock(fd, operation):
                real_flock(fd, operation)
                if operation == fcntl.LOCK_EX:
                    os.replace(replacement, ledger)
            with mock.patch.object(fcntl, "flock", side_effect=replace_after_lock):
                with self.assertRaises(adapter.AdapterError):
                    adapter.reserve_nonce(ledger, "transaction-review-five", "nonce-review-five")
            self.assertEqual(ledger.read_bytes(), b"")

    def test_identity_v2_anchors_are_protected_from_all_journal_outputs(self):
        adapter = load_module("anchor_alias_adapter", ADAPTER_PATH)
        planner = load_module("anchor_alias_planner", PLANNER_PATH)
        adapter._PLANNER_MODULE = planner
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for field in ("IDENTITY_V2_TRUST_CONFIG_PATH", "IDENTITY_V2_PROVIDER_REGISTRY_PATH"):
                for suffix in ("", ".lock", ".emergency.json"):
                    with self.subTest(field=field, suffix=suffix):
                        journal = root / "journal"
                        anchor = Path(f"{journal}{suffix}")
                        anchor.write_bytes(b"authority anchor")
                        anchor.chmod(0o600)
                        with mock.patch.object(planner, field, anchor):
                            with self.assertRaises(adapter.AdapterError):
                                adapter._reject_journal_input_aliases(journal, root / "ledger", {})
                        self.assertEqual(anchor.read_bytes(), b"authority anchor")

    def test_adapter_fixture_module_is_initialized_once_across_setups(self):
        # Stop before crypto setup: count the real module lifecycle boundary,
        # not subprocess timing. Fresh fixture instances remain per-test.
        class StopSetup(Exception):
            pass
        adapter_tests = _adapter_test_module()
        modules = []
        instances = []
        real_load = adapter_tests.load_module
        def tracked_load(name, path):
            module = real_load(name, path)
            if path == adapter_tests.PLANNER_TEST_PATH:
                modules.append(module)
                def setup(instance):
                    instances.append(instance)
                    instance.mutable_probe = []
                    raise StopSetup()
                module.FullNetworkCleanRoomPlanTests.setUp = setup
            return module
        with mock.patch.object(adapter_tests, "load_module", tracked_load):
            adapter_tests.FullNetworkCleanRoomAdapterTests.setUpClass()
            try:
                for _ in range(2):
                    case = adapter_tests.FullNetworkCleanRoomAdapterTests("runTest")
                    with self.assertRaises(StopSetup):
                        case.setUp()
                instances[0].mutable_probe.append("first-test-only")
                self.assertEqual(instances[1].mutable_probe, [])
                self.assertIsNot(instances[0], instances[1])
                self.assertEqual(len(modules), 1, "planner crypto baseline module reloaded per test")
            finally:
                adapter_tests.FullNetworkCleanRoomAdapterTests.tearDownClass()


class FleetLockPublisherProtectionTests(unittest.TestCase):
    """Repository publishers must not invalidate another callback's fleet lock."""

    def _exercise_publisher(self, publisher):
        for alias in ("exact", "normalized", "hardlink"):
            with self.subTest(publisher=publisher, alias=alias), tempfile.TemporaryDirectory() as directory:
                root = Path(directory).resolve()
                adapter = load_module("publisher_lock_adapter", ADAPTER_PATH)
                planner = load_module("publisher_lock_planner", PLANNER_PATH)
                signing = load_module("publisher_lock_signing", ROOT / "scripts/p2p-public-testnet-identity-v2-signing-tool.py")
                sidecar = load_module("publisher_lock_sidecar", ROOT / "scripts/p2p-public-testnet-identity-receipt-v2.py")
                aggregate = load_module("publisher_lock_aggregate", ROOT / "scripts/p2p-public-testnet-identity-v2-evidence-aggregate.py")
                lock_path = root / "full-network-clean-room.lock"
                modules = (adapter, adapter._load_planner()._peer_registry_authority(),
                           planner._peer_registry_authority(), signing._peer_registry_authority(),
                           aggregate.PLANNER._peer_registry_authority())
                with ExitStack() as patches:
                    # Relocate only code-owned fixture authority, never expose a CLI override.
                    for module in modules:
                        patches.enter_context(mock.patch.object(module, "CANONICAL_FLEET_LOCK_PATH", str(lock_path)))
                    patches.enter_context(mock.patch.object(sidecar, "_load_planner", return_value=planner))

                    def publish(output):
                        if publisher == "planner":
                            protected = planner._plan_output_inputs(root / "input", root / "map", {})
                            planner._write_plan_atomic(output, {"fixture": True}, protected)
                        elif publisher == "signing":
                            signing._reject_output_aliases([(output, "verified output")], [])
                            signing._atomic_write(output, b'{"fixture": true}', "verified output")
                        elif publisher == "sidecar":
                            sidecar._reject_output_aliases([(output, "evidence-map output")], [])
                            sidecar._write_atomically(output, {"fixture": True})
                        else:
                            aggregate._write_atomic({"fixture": True}, output, retained_paths=[])

                    first = adapter._acquire_fleet_transaction_guard(root / "first.json")
                    token = adapter._ACTIVE_TRANSACTION_GUARD.set(first)
                    try:
                        # Positive control must succeed while a fleet callback is active.
                        ordinary = root / "ordinary.json"
                        publish(ordinary)
                        self.assertEqual(json.loads(ordinary.read_text()), {"fixture": True})
                        output = lock_path
                        if alias == "normalized":
                            (root / "nested").mkdir()
                            output = root / "nested" / ".." / lock_path.name
                        elif alias == "hardlink":
                            output = root / "lock-hardlink"
                            os.link(lock_path, output)
                        original = (lock_path.read_bytes(), lock_path.stat().st_ino)
                        output_original = (output.read_bytes(), output.stat().st_ino)
                        observed = {}

                        def callback():
                            try:
                                publish(output)
                            except (SystemExit, signing.ToolError, adapter.AdapterError):
                                observed["publisher_rejected"] = True
                            else:
                                observed["publisher_rejected"] = False
                            observed["lock_preserved"] = (lock_path.read_bytes(), lock_path.stat().st_ino) == original
                            observed["output_preserved"] = (output.read_bytes(), output.stat().st_ino) == output_original
                            try:
                                second = adapter._acquire_fleet_transaction_guard(root / "second.json")
                            except adapter.AdapterError:
                                observed["second_lock_blocked"] = True
                            else:
                                observed["second_lock_blocked"] = False
                                second.close()

                        try:
                            adapter._guarded_callback(callback)
                        except adapter.AdapterError:
                            observed["post_callback_guard_failed"] = True
                        else:
                            observed["post_callback_guard_failed"] = False
                        self.assertEqual(observed, {
                            "publisher_rejected": True, "lock_preserved": True,
                            "output_preserved": True, "second_lock_blocked": True,
                            "post_callback_guard_failed": False,
                        }, "publisher must reject before replacement, not detect drift after the callback")
                    finally:
                        adapter._ACTIVE_TRANSACTION_GUARD.reset(token)
                        first.close()

    def test_planner_cannot_replace_held_fleet_lock(self):
        self._exercise_publisher("planner")

    def test_signing_cannot_replace_held_fleet_lock(self):
        self._exercise_publisher("signing")

    def test_sidecar_cannot_replace_held_fleet_lock(self):
        self._exercise_publisher("sidecar")

    def test_aggregate_cannot_replace_held_fleet_lock(self):
        self._exercise_publisher("aggregate")


if __name__ == "__main__":
    unittest.main()
