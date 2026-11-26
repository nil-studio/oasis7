#!/usr/bin/env python3
"""Plan, inventory, and symlink contract tests for validator pair rebuild."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import os
import shutil
import subprocess
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
ROOT = _HELPERS.ROOT
EXECUTOR = _HELPERS.EXECUTOR
ValidatorPairRebuildContractBase = _HELPERS.ValidatorPairRebuildContractBase


class ValidatorPairRebuildPlanContractTests(ValidatorPairRebuildContractBase):
    def test_identity_manifest_rejects_relative_governed_key_path(self) -> None:
        spec = importlib.util.spec_from_file_location("pair_rebuild_identity_manifest_path_test", EXECUTOR)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with self.assertRaisesRegex(SystemExit, "lexically absolute"):
            module._expected_identity_metadata(
                {
                    "expected_key_path": "config/node-keypair.toml",
                    "expected_key_sha256": "a" * 64,
                    "expected_key_size_bytes": 321,
                    "expected_key_mode": 0o600,
                    "expected_key_uid": 1001,
                    "expected_key_gid": 1001,
                },
                "identity manifest entry",
            )

    def test_plan_binds_complete_tree_inventory_to_capacity_and_inode_budget(self) -> None:
        for node in self.nodes.values():
            current = node / "current"
            release = node / "releases" / "known"
            release.mkdir(parents=True)
            (release / "bin").mkdir()
            (release / "bin" / "oasis7_chain_runtime").write_bytes(b"runtime-old\n")
            shutil.rmtree(current)
            current.symlink_to("releases/known", target_is_directory=True)
        result = subprocess.run(self._base_args(), text=True, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        plan = json.loads(result.stdout)
        for role in ("storage-205", "sequencer-204"):
            inventory = plan["capacity"][role]["inventory"]
            self.assertEqual(
                set(inventory),
                {"entry_count", "link_count", "dir_count", "file_count", "total_bytes"},
            )
            self.assertEqual(
                inventory["entry_count"],
                inventory["link_count"] + inventory["dir_count"] + inventory["file_count"],
            )
            self.assertGreaterEqual(inventory["link_count"], 1)
            self.assertGreaterEqual(plan["capacity"][role]["required_inodes"], inventory["entry_count"])

    def test_plan_rejects_unsupported_fifo_before_any_quiesce(self) -> None:
        fifo = self.nodes["storage-205"] / "data" / "unsupported.fifo"
        os.mkfifo(fifo)
        result = subprocess.run(self._base_args(), text=True, capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("unsupported", result.stderr.lower())
        self.assertFalse((self.nodes["storage-205"] / "backups").exists())

    def test_snapshot_failure_removes_partial_backup_for_unsupported_entry(self) -> None:
        spec = importlib.util.spec_from_file_location("pair_rebuild_snapshot_test", EXECUTOR)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        fifo = self.nodes["storage-205"] / "data" / "unsupported.fifo"
        os.mkfifo(fifo)
        with self.assertRaises(Exception):
            module.snapshot_node({"role": "storage-205", "root": str(self.nodes["storage-205"])}, "partial-backup")
        self.assertFalse((self.nodes["storage-205"] / "backups" / "partial-backup").exists())

    def test_plan_digest_is_stable_and_runtime_transaction_metadata_is_separate(self) -> None:
        first = subprocess.run(self._base_args(), text=True, capture_output=True)
        self.assertEqual(first.returncode, 0, first.stderr)
        first_receipt = json.loads(first.stdout)
        second_out = self.root / "out-second"
        second_args = [arg for arg in self._base_args() if arg not in ("--out-dir", str(self.out))]
        second_args.extend(["--out-dir", str(second_out)])
        second = subprocess.run(second_args, text=True, capture_output=True)
        self.assertEqual(second.returncode, 0, second.stderr)
        second_receipt = json.loads(second.stdout)
        self.assertEqual(first_receipt, second_receipt)
        self.assertNotIn("transaction_id", first_receipt)
        self.assertNotIn("created_at", first_receipt)

    def test_plan_rejects_full_status_url_for_204(self) -> None:
        result = subprocess.run(
            self._base_args() + ["--sequencer-proof-url", "http://127.0.0.1:6631/v1/chain/status"],
            text=True,
            capture_output=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("204", result.stderr)

    def test_active_runbook_has_no_sequencer_full_status_command(self) -> None:
        runbook = (ROOT / "doc/p2p/blockchain/public-testnet-governed-bootstrap.runbook.md").read_text(encoding="utf-8")
        self.assertNotIn("6631/v1/chain/status", runbook)
        self.assertNotIn("39.104.204.172:6631/v1/chain/status", runbook)

    def test_plan_rejects_untrusted_signer_even_when_detached_signature_is_valid(self) -> None:
        receipt = json.loads(self.provenance.read_text(encoding="utf-8"))
        receipt["signature"]["signer_id"] = "unlisted-attestor"
        body = {key: value for key, value in receipt.items() if key != "binding_digest"}
        receipt["binding_digest"] = hashlib.sha256(
            json.dumps(body, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest()
        payload_path = self.root / "untrusted-signing-payload.bin"
        payload_path.write_bytes(json.dumps(body, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode())
        subprocess.run(
            ["openssl", "dgst", "-sha256", "-sign", str(self.signing_key), "-out", str(self.signature), str(payload_path)],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        self.provenance.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
        result = subprocess.run(self._base_args(), text=True, capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("trusted signer", result.stderr.lower())

    def test_plan_rejects_unexpected_governed_key(self) -> None:
        extra = self.governed / "unexpected.json"
        extra.write_text("{}\n", encoding="utf-8")
        self._write_provenance(extra_governed=extra)
        result = subprocess.run(self._base_args(), text=True, capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("governed", result.stderr.lower())

    def test_canonical_current_symlink_is_counted_and_accepted(self) -> None:
        for role, node in self.nodes.items():
            current = node / "current"
            shutil_target = node / "releases" / "known"
            shutil_target.mkdir(parents=True)
            (shutil_target / "bin").mkdir()
            (shutil_target / "bin" / "oasis7_chain_runtime").write_bytes(b"runtime-old\n")
            shutil.rmtree(current)
            current.symlink_to("releases/known", target_is_directory=True)
        result = subprocess.run(self._base_args(), text=True, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_arbitrary_current_symlink_is_rejected(self) -> None:
        current = self.nodes["storage-205"] / "current"
        shutil.rmtree(current)
        current.symlink_to(self.root / "package", target_is_directory=True)
        result = subprocess.run(self._base_args(), text=True, capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("current", result.stderr.lower())


if __name__ == "__main__":
    unittest.main()
