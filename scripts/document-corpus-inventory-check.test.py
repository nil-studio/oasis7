#!/usr/bin/env python3
"""Regression check confirming the document-corpus hash shard system is retired."""

from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parent.parent
RETIRED_PATHS = (
    "doc/.governance/document-corpus",
    "doc/.governance/document-corpus-inventory.json",
    "doc/testing/evidence/inventory.json",
    "doc/engineering/doc-governance/document-corpus-inventory-v3.design.md",
    ".github/workflows/document-corpus-platform.yml",
    "scripts/document_corpus.py",
    "scripts/document-corpus-inventory.py",
    "scripts/document-corpus-inventory-check.py",
    "scripts/doc-evidence-inventory-check.py",
)


class DocumentCorpusRetirementTest(unittest.TestCase):
    def test_document_corpus_hash_system_is_retired(self) -> None:
        for rel in RETIRED_PATHS:
            with self.subTest(path=rel):
                self.assertFalse((ROOT / rel).exists(), f"retired corpus path still exists: {rel}")


if __name__ == "__main__":
    unittest.main()
