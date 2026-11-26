#!/usr/bin/env python3
"""Apply, host-receipt, backup, fsync, and credential contract tests for validator pair rebuild."""

from __future__ import annotations

import copy
import hashlib
import importlib.util
import json
import os
import subprocess
import sys
import unittest
from pathlib import Path


def _load_test_helpers():
    helpers_path = Path(__file__).resolve().parent / "p2p-public-testnet-validator-pair-rebuild-test-helpers.py"
    spec = importlib.util.spec_from_file_location("pair_rebuild_test_helpers", helpers_path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"unable to load test helpers module from {helpers_path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_HELPERS = _load_test_helpers()
EXECUTOR = _HELPERS.EXECUTOR
PROVENANCE = _HELPERS.PROVENANCE
sha256 = _HELPERS.sha256
CANONICAL_RESET_SURFACES = _HELPERS.CANONICAL_RESET_SURFACES
ValidatorPairRebuildContractBase = _HELPERS.ValidatorPairRebuildContractBase


class ValidatorPairRebuildApplyContractTests(ValidatorPairRebuildContractBase):
    def test_apply_requires_phase_callbacks_and_signed_identity_proof(self) -> None:
        plan = subprocess.run(self._base_args(), text=True, capture_output=True, check=True)
        plan_path = self.out / "transaction.json"
        plan_path.write_text(plan.stdout, encoding="utf-8")
        adapter = self._write_host_adapter()
        result = subprocess.run(
            self._apply_args(plan_path, adapter),
            text=True,
            capture_output=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        receipt = json.loads(result.stdout)
        self.assertEqual(receipt["phase"], "applied")
        self.assertIn("direct_quiescence_observation", receipt)
        self.assertIn("backup_receipt", receipt)
        self.assertIn("sequencer_rebuild_proof", receipt["host_receipt"])

    def test_host_receipt_rejects_trusted_but_unplanned_proof(self) -> None:
        spec = importlib.util.spec_from_file_location("pair_rebuild_host_binding_test", EXECUTOR)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        plan = json.loads(subprocess.run(self._base_args(), text=True, capture_output=True, check=True).stdout)
        plan["schema_version"] = "oasis7.validator_pair_rebuild_transaction.v1"
        plan["transaction_id"] = "pair-rebuild-test-binding"
        plan["phase"] = "staged"
        plan_path = self.out / "binding-transaction.json"
        plan_path.write_text(json.dumps(plan), encoding="utf-8")
        adapter = self._write_host_adapter()
        receipt = module.run_host_adapter(adapter, plan_path, plan, "apply")
        alternate_proof = self.root / "alternate-host-proof.json"
        alternate_signature = self.root / "alternate-host-proof.sig"
        self._write_attestation(
            alternate_proof,
            alternate_signature,
            "oasis7.validator_pair_rebuild_proof.v1",
            "sequencer-204",
            "peer-unplanned",
            "unplanned-sequencer",
        )
        receipt["sequencer_rebuild_proof"]["path"] = str(alternate_proof)
        with self.assertRaises(SystemExit):
            module.validate_host_receipt(receipt, plan, "apply")

    def _bound_phase_receipt(self, phase: str):
        spec = importlib.util.spec_from_file_location("pair_rebuild_host_binding_fixture", EXECUTOR)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        plan = json.loads(subprocess.run(self._base_args(), text=True, capture_output=True, check=True).stdout)
        plan["schema_version"] = "oasis7.validator_pair_rebuild_transaction.v1"
        plan["transaction_id"] = f"pair-rebuild-test-{phase}"
        plan["phase"] = "staged" if phase == "apply" else "prepared"
        plan_path = self.out / f"{phase}-transaction.json"
        plan_path.write_text(json.dumps(plan), encoding="utf-8")
        receipt = module.run_host_adapter(self._write_host_adapter(), plan_path, plan, phase)
        if phase == "apply":
            empty_surface = self.root / "empty-post-delete-surface"
            empty_surface.mkdir(exist_ok=True)
            empty_inventory = module.inventory_tree(empty_surface)
            post_delete_absence = {
                "schema_version": "oasis7.validator_pair_rebuild_post_delete_absence.v1",
                "absent": True,
                "target_set": list(module.RESET_SURFACES),
                "target_set_sha256": module.reset_surface_digest(),
                "target_inventory": {
                    surface: empty_inventory
                    for surface in module.RESET_SURFACES
                },
            }
            for role in ("storage-205", "sequencer-204"):
                receipt["nodes"][role]["post_delete_absence"] = copy.deepcopy(post_delete_absence)
        return module, plan, receipt

    def _bound_host_receipt(self):
        return self._bound_phase_receipt("apply")

    def test_apply_requires_explicit_observer_mutation_false(self) -> None:
        module, plan, receipt = self._bound_host_receipt()
        receipt.pop("observer_mutation", None)
        with self.assertRaisesRegex(SystemExit, "observer"):
            module.validate_host_receipt(receipt, plan, "apply")

    def test_apply_requires_per_node_independent_observation(self) -> None:
        module, plan, receipt = self._bound_host_receipt()
        for role in ("storage-205", "sequencer-204"):
            receipt["nodes"][role].pop("independently_observed", None)
        with self.assertRaisesRegex(SystemExit, r"independent|observ"):
            module.validate_host_receipt(receipt, plan, "apply")

    def test_apply_requires_per_node_service_state(self) -> None:
        module, plan, receipt = self._bound_host_receipt()
        for role in ("storage-205", "sequencer-204"):
            receipt["nodes"][role].pop("service_state", None)
        with self.assertRaisesRegex(SystemExit, r"service.?state|service"):
            module.validate_host_receipt(receipt, plan, "apply")

    def test_apply_rejects_non_exact_per_node_role_and_root_bindings(self) -> None:
        module, plan, receipt = self._bound_host_receipt()
        cases = {
            "role substitution": ("role", "sequencer-204"),
            "root substitution": ("root", str(self.nodes["sequencer-204"])),
            "missing role": ("role", None),
            "missing root": ("root", None),
        }
        for label, (field, value) in cases.items():
            with self.subTest(binding=label):
                candidate = copy.deepcopy(receipt)
                node = candidate["nodes"]["storage-205"]
                if value is None:
                    node.pop(field, None)
                else:
                    node[field] = value
                with self.assertRaisesRegex(SystemExit, r"role|root|binding"):
                    module.validate_host_receipt(candidate, plan, "apply")

    def test_apply_rejects_missing_per_node_post_delete_absence_receipts(self) -> None:
        module, plan, receipt = self._bound_host_receipt()
        for role in ("storage-205", "sequencer-204"):
            receipt["nodes"][role].pop("post_delete_absence", None)
        with self.assertRaises(SystemExit):
            module.validate_host_receipt(receipt, plan, "apply")

    def test_apply_rejects_target_surface_mismatch_including_advertised_bridge_root(self) -> None:
        module, plan, receipt = self._bound_host_receipt()
        advertised_targets = [*CANONICAL_RESET_SURFACES, "data/bridge-root"]
        target_digest = hashlib.sha256(
            json.dumps(advertised_targets, ensure_ascii=True, separators=(",", ":")).encode()
        ).hexdigest()
        for role in ("storage-205", "sequencer-204"):
            receipt["nodes"][role]["post_delete_absence"] = {
                "absent": True,
                "target_set": advertised_targets,
                "target_set_sha256": target_digest,
            }
        with self.assertRaises(SystemExit):
            module.validate_host_receipt(receipt, plan, "apply")

    def test_backup_rejects_each_incomplete_hashed_manifest_binding(self) -> None:
        module, plan, receipt = self._bound_phase_receipt("backup")
        required_fields = (
            "backup_root",
            "backup_manifest_sha256",
            "backup_inventory",
            "backup_capacity",
            "backup_non_seed",
        )
        for missing_field in required_fields:
            incomplete = copy.deepcopy(receipt)
            for role in ("storage-205", "sequencer-204"):
                node = incomplete["nodes"][role]
                node.update(
                    {
                        "backup_root": str(Path(plan["nodes"][role]["root"]) / "backups" / "pair-rebuild-test-backup"),
                        "backup_manifest_sha256": "a" * 64,
                        "backup_inventory": plan["capacity"][role]["inventory"],
                        "backup_capacity": plan["capacity"][role],
                        "backup_non_seed": {
                            "forensic_only": True,
                            "seed_eligible": False,
                            "restore_deleted_chain_state": False,
                        },
                    }
                )
                node.pop(missing_field)
            with self.subTest(missing_field=missing_field):
                with self.assertRaises(SystemExit):
                    module.validate_host_receipt(incomplete, plan, "backup")

    def test_host_receipt_rejects_incomplete_swapped_or_stale_evidence(self) -> None:
        module, plan, receipt = self._bound_host_receipt()
        omitted = copy.deepcopy(receipt)
        omitted["identity_receipts"] = omitted["identity_receipts"][:1]
        with self.assertRaises(SystemExit):
            module.validate_host_receipt(omitted, plan, "apply")
        swapped = copy.deepcopy(receipt)
        swapped["identity_receipts"][0]["role"], swapped["identity_receipts"][1]["role"] = (
            swapped["identity_receipts"][1]["role"],
            swapped["identity_receipts"][0]["role"],
        )
        with self.assertRaises(SystemExit):
            module.validate_host_receipt(swapped, plan, "apply")
        stale = copy.deepcopy(receipt)
        stale["captured_at"] = "2000-01-01T00:00:00Z"
        with self.assertRaises(SystemExit):
            module.validate_host_receipt(stale, plan, "apply")
        stale_window = copy.deepcopy(receipt)
        plan["adapter_binding"]["phase_window_started_at"] = "2000-01-01T00:00:00Z"
        with self.assertRaises(SystemExit):
            module.validate_host_receipt(stale_window, plan, "apply")
        substituted = copy.deepcopy(receipt)
        substituted["evidence_bindings"]["identity_receipts"][0]["sha256"] = "0" * 64
        with self.assertRaises(SystemExit):
            module.validate_host_receipt(substituted, plan, "apply")

    def test_adapter_binding_separates_raw_and_verification_proof_files(self) -> None:
        spec = importlib.util.spec_from_file_location("pair_rebuild_binding_shape_test", EXECUTOR)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        identities = []
        for role in ("storage-205", "sequencer-204"):
            path = self.root / f"binding-{role}.json"
            path.write_text(role, encoding="utf-8")
            identities.append({
                "path": str(path),
                "sha256": sha256(path),
                "role": role,
                "node_id": f"node-{role}",
                "peer_id": f"peer-{role}",
                "key_path": f"/remote/{role}/config/node-keypair.toml",
                "key_sha256": "c" * 64,
                "key_size_bytes": 321,
                "key_mode": 0o600,
                "key_uid": 1001,
                "key_gid": 1001,
            })
        raw_proof = self.root / "binding-raw-proof.json"
        verification = self.root / "binding-verification.json"
        raw_proof.write_text("raw-proof", encoding="utf-8")
        verification.write_text("verification-receipt", encoding="utf-8")
        plan = {
            "plan_digest": "a" * 64,
            "transaction_id": "binding-shape",
            "proof": {
                "identity_receipts": identities,
                "sequencer_rebuild_proof_path": str(raw_proof),
                "sequencer_rebuild_proof_verification_path": str(verification),
                "sequencer_rebuild_proof": {
                    "path": str(verification),
                    "sha256": sha256(verification),
                    "proof_sha256": sha256(raw_proof),
                    "role": "sequencer-204",
                    "node_id": "node-sequencer-204",
                    "peer_id": "peer-sequencer-204",
                },
            },
        }
        evidence = module._expected_adapter_evidence(plan)
        self.assertEqual(evidence["identity_receipts"][0]["key_mode"], 0o600)
        self.assertEqual(evidence["identity_receipts"][0]["key_uid"], 1001)
        self.assertEqual(evidence["identity_receipts"][0]["key_gid"], 1001)
        self.assertEqual(evidence["sequencer_rebuild_proof"]["path"], str(raw_proof.resolve()))
        self.assertEqual(evidence["sequencer_rebuild_proof"]["sha256"], sha256(raw_proof))
        self.assertEqual(evidence["sequencer_rebuild_proof"]["verification_path"], str(verification.resolve()))
        self.assertEqual(evidence["sequencer_rebuild_proof"]["verification_sha256"], sha256(verification))

    def test_plan_rejects_missing_signature_before_any_node_access(self) -> None:
        self._write_provenance(signature=False)
        result = subprocess.run(self._base_args(), text=True, capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("signature", result.stderr.lower())

    def test_provenance_rejects_tampered_signed_body(self) -> None:
        receipt = json.loads(self.provenance.read_text(encoding="utf-8"))
        receipt["package"]["run_id"] = "tampered"
        body = {key: value for key, value in receipt.items() if key != "binding_digest"}
        receipt["binding_digest"] = hashlib.sha256(
            json.dumps(body, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest()
        self.provenance.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
        result = subprocess.run(
            [
                sys.executable,
                str(PROVENANCE),
                "validate",
                "--provenance",
                str(self.provenance),
                "--package-dir",
                str(self.package),
            ],
            text=True,
            capture_output=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("detached signature verification failed", result.stderr.lower())

    def test_staged_receipt_falls_back_to_copied_detached_files(self) -> None:
        staged = self.root / "staged-evidence"
        staged.mkdir()
        staged_receipt = staged / self.provenance.name
        staged_receipt.write_bytes(self.provenance.read_bytes())
        (staged / self.signature.name).write_bytes(self.signature.read_bytes())
        (staged / self.public_key.name).write_bytes(self.public_key.read_bytes())
        original_signature = self.signature.read_bytes()
        original_public_key = self.public_key.read_bytes()
        self.signature.unlink()
        self.public_key.unlink()
        result = subprocess.run(
            [
                sys.executable,
                str(PROVENANCE),
                "validate",
                "--provenance",
                str(staged_receipt),
                "--package-dir",
                str(self.package),
            ],
            text=True,
            capture_output=True,
        )
        self.signature.write_bytes(original_signature)
        self.public_key.write_bytes(original_public_key)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_apply_writes_full_manifests_and_preserves_order(self) -> None:
        plan = subprocess.run(self._base_args(), text=True, capture_output=True, check=True)
        plan_path = self.out / "transaction.json"
        plan_path.write_text(plan.stdout, encoding="utf-8")
        adapter = self._write_host_adapter()
        result = subprocess.run(
            self._apply_args(plan_path, adapter),
            text=True,
            capture_output=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        receipt = json.loads(result.stdout)
        self.assertEqual(receipt["phase"], "applied")
        self.assertEqual(receipt["mutation_order"], ["storage-205", "sequencer-204"])
        self.assertTrue(receipt["nodes"]["storage-205"]["backup"]["verified"])
        self.assertTrue(receipt["nodes"]["sequencer-204"]["backup"]["verified"])
        self.assertTrue((self.nodes["storage-205"] / "backups").exists())
        self.assertTrue((self.nodes["sequencer-204"] / "backups").exists())
        self.assertTrue(receipt["host_receipt"]["nodes"]["sequencer-204"]["healthz_ok"])
        self.assertTrue(receipt["capacity_apply"]["storage-205"]["verified"])
        for role in ("storage-205", "sequencer-204"):
            self.assertEqual(
                receipt["capacity_apply"][role]["inventory"],
                receipt["capacity"][role]["inventory"],
            )
        self.assertEqual(
            set(receipt["staged"]["storage-205"]["governed_inventory"]),
            {"manifest", "genesis", "registry", "bootstrap", "world"},
        )
        self.assertEqual(receipt["host_receipt"]["plan_digest"], receipt["plan_digest"])
        self.assertEqual(receipt["host_receipt"]["evidence_bindings"], receipt["adapter_binding"]["evidence_bindings"])
        self.assertTrue(receipt["host_receipt"]["captured_at"])

    def test_apply_transaction_only_invocation_recovers_direct_observation_routing(self) -> None:
        plan = subprocess.run(self._base_args(), text=True, capture_output=True, check=True)
        plan_path = self.out / "transaction.json"
        plan_path.write_text(plan.stdout, encoding="utf-8")
        result = subprocess.run(
            [
                str(self.executor_launcher),
                "apply",
                "--transaction",
                str(plan_path),
                "--host-adapter",
                str(self._write_host_adapter()),
            ],
            text=True,
            capture_output=True,
            env=os.environ.copy(),
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["phase"], "applied")

    def test_apply_requires_host_gate_receipt(self) -> None:
        plan = subprocess.run(self._base_args(), text=True, capture_output=True, check=True)
        plan_path = self.out / "transaction.json"
        plan_path.write_text(plan.stdout, encoding="utf-8")
        result = subprocess.run(
            [sys.executable, str(EXECUTOR), "apply", "--transaction", str(plan_path)],
            text=True,
            capture_output=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("host adapter", result.stderr.lower())
        self.assertFalse((self.nodes["storage-205"] / "backups").exists())

    def test_apply_rechecks_capacity_after_plan(self) -> None:
        plan = subprocess.run(self._base_args(), text=True, capture_output=True, check=True)
        transaction = json.loads(plan.stdout)
        transaction["capacity"]["storage-205"]["required_bytes"] = 10**30
        transaction["plan_digest"] = hashlib.sha256(
            json.dumps({key: value for key, value in transaction.items() if key != "plan_digest"}, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest()
        plan_path = self.out / "transaction.json"
        plan_path.write_text(json.dumps(transaction), encoding="utf-8")
        result = subprocess.run(
            self._apply_args(plan_path, self._write_host_adapter()),
            text=True,
            capture_output=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("apply-time capacity", result.stderr.lower())
        self.assertFalse((self.nodes["storage-205"] / "backups").exists())

    def test_apply_rejects_inventory_drift_after_plan(self) -> None:
        plan = subprocess.run(self._base_args(), text=True, capture_output=True, check=True)
        plan_path = self.out / "transaction.json"
        plan_path.write_text(plan.stdout, encoding="utf-8")
        (self.nodes["storage-205"] / "data" / "unexpected-entry").write_text("drift\n", encoding="utf-8")
        result = subprocess.run(
            self._apply_args(plan_path, self._write_host_adapter()),
            text=True,
            capture_output=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("inventory changed", result.stderr.lower())
        self.assertFalse((self.nodes["storage-205"] / "backups").exists())

    def test_rollback_restores_the_stopped_snapshot(self) -> None:
        plan = subprocess.run(self._base_args(), text=True, capture_output=True, check=True)
        plan_path = self.out / "transaction.json"
        plan_path.write_text(plan.stdout, encoding="utf-8")
        adapter = self._write_host_adapter()
        applied = subprocess.run(
            self._apply_args(plan_path, adapter),
            text=True,
            capture_output=True,
            check=True,
        )
        applied_receipt = json.loads(applied.stdout)
        transaction_id = applied_receipt["transaction_id"]
        self.assertNotEqual(sha256(self.nodes["storage-205"] / "current" / "bin" / "oasis7_chain_runtime"), sha256(self.nodes["storage-205"] / "backups" / transaction_id / "snapshot" / "current" / "bin" / "oasis7_chain_runtime"))
        result = subprocess.run(
            [str(self.executor_launcher), "rollback", "--transaction", str(plan_path), "--host-adapter", str(adapter)],
            text=True,
            capture_output=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        receipt = json.loads(result.stdout)
        self.assertEqual(receipt["phase"], "rolled_back")
        self.assertEqual(sha256(self.nodes["storage-205"] / "current" / "bin" / "oasis7_chain_runtime"), sha256(self.nodes["storage-205"] / "backups" / transaction_id / "snapshot" / "current" / "bin" / "oasis7_chain_runtime"))

    def test_transaction_journal_fsyncs_file_and_parent_directory(self) -> None:
        spec = importlib.util.spec_from_file_location("pair_rebuild_durable_test_executor", EXECUTOR)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        journal = self.root / "journal" / "transaction.json"
        fsync_calls: list[int] = []
        original_fsync = module.os.fsync
        module.os.fsync = lambda descriptor: fsync_calls.append(descriptor)
        try:
            module.write_json(journal, {"phase": "prepared"})
        finally:
            module.os.fsync = original_fsync
        self.assertEqual(json.loads(journal.read_text(encoding="utf-8"))["phase"], "prepared")
        self.assertGreaterEqual(len(fsync_calls), 2)

    def test_backup_manifest_fsyncs_file_and_parent_directory(self) -> None:
        spec = importlib.util.spec_from_file_location("pair_rebuild_backup_durable_test_executor", EXECUTOR)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        node_root = self.root / "durable-node"
        node_root.mkdir()
        (node_root / "payload").write_text("before\\n", encoding="utf-8")
        fsync_calls: list[int] = []
        original_fsync = module.os.fsync
        module.os.fsync = lambda descriptor: fsync_calls.append(descriptor)
        try:
            backup = module.snapshot_node({"role": "storage-205", "root": str(node_root)}, "durable-tx")
        finally:
            module.os.fsync = original_fsync
        self.assertTrue(Path(backup["manifest"]).is_file())
        self.assertGreaterEqual(len(fsync_calls), 2)

    def test_copy_entry_fsyncs_nested_files_and_directories(self) -> None:
        spec = importlib.util.spec_from_file_location("pair_rebuild_tree_durable_test_executor", EXECUTOR)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        source = self.root / "tree-source"
        (source / "nested").mkdir(parents=True)
        (source / "nested" / "payload").write_bytes(b"durable-payload\n")
        destination = self.root / "tree-destination"
        fsync_calls: list[int] = []
        original_fsync = module.os.fsync
        module.os.fsync = lambda descriptor: fsync_calls.append(descriptor)
        try:
            module.copy_entry(source, destination)
        finally:
            module.os.fsync = original_fsync
        self.assertEqual((destination / "nested" / "payload").read_bytes(), b"durable-payload\n")
        self.assertGreaterEqual(len(fsync_calls), 4)

    def test_restore_node_fsyncs_restored_files_and_directories(self) -> None:
        spec = importlib.util.spec_from_file_location("pair_rebuild_restore_durable_test_executor", EXECUTOR)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        node_root = self.root / "restore-node"
        (node_root / "nested").mkdir(parents=True)
        (node_root / "nested" / "payload").write_bytes(b"before\n")
        backup = module.snapshot_node({"role": "storage-205", "root": str(node_root)}, "restore-durable-tx")
        (node_root / "nested" / "payload").write_bytes(b"changed\n")
        fsync_calls: list[int] = []
        original_fsync = module.os.fsync
        module.os.fsync = lambda descriptor: fsync_calls.append(descriptor)
        try:
            module.restore_node(backup, "restore-durable-tx")
        finally:
            module.os.fsync = original_fsync
        self.assertEqual((node_root / "nested" / "payload").read_bytes(), b"before\n")
        self.assertGreaterEqual(len(fsync_calls), 4)

    def test_credential_fd_is_consumed_once_and_reused_from_memory(self) -> None:
        spec = importlib.util.spec_from_file_location("pair_rebuild_credential_cache_test_executor", EXECUTOR)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        secret_path = self.root / "credential"
        secret_path.write_bytes(b"fd-secret-for-reobserve\n")
        credential_fd = os.open(secret_path, os.O_RDONLY)
        args = module.argparse.Namespace(credential_env=None, credential_fd=credential_fd)
        try:
            first_env, first_use = module._direct_ssh_environment(args, {})
            try:
                second_env, second_use = module._direct_ssh_environment(args, {})
            except SystemExit as error:
                self.fail(f"credential fd was read again instead of reusing the in-memory secret: {error}")
        finally:
            os.close(credential_fd)
        self.assertTrue(first_use and second_use)
        self.assertEqual(first_env["SSHPASS"], "fd-secret-for-reobserve")
        self.assertEqual(second_env["SSHPASS"], first_env["SSHPASS"])


if __name__ == "__main__":
    unittest.main()
