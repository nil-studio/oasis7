#!/usr/bin/env python3
"""Regression check confirming the delegated evidence corpus shard checker is retired."""

from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parent.parent


class DocEvidenceInventoryRetirementTest(unittest.TestCase):
    def test_doc_evidence_inventory_shards_are_retired(self) -> None:
        for rel in (
            "doc/testing/evidence/inventory.json",
            "doc/.governance/document-corpus/evidence",
            "scripts/doc-evidence-inventory-check.py",
        ):
            with self.subTest(path=rel):
                self.assertFalse((ROOT / rel).exists(), f"retired evidence corpus path still exists: {rel}")


if __name__ == "__main__":
    unittest.main()
