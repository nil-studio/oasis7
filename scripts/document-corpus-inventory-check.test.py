#!/usr/bin/env python3
"""Keep retired document inventories and their readers out of the repository."""

from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parent.parent
RETIRED_PATHS = (
    "doc/.governance/document-corpus",
    "doc/.governance/document-corpus-inventory.json",
    "doc/.governance/top-level-directory-registry.json",
    "doc/.governance/doc-root-md-allowlist.txt",
    "doc/.governance/module-root-md-allowlist.txt",
    "doc/testing/evidence/inventory.json",
    "doc/engineering/doc-governance/document-corpus-inventory-v3.design.md",
    ".github/workflows/document-corpus-platform.yml",
    "scripts/document_corpus.py",
    "scripts/document_evidence_policy.py",
    "scripts/document-corpus-inventory.py",
    "scripts/document-corpus-inventory-check.py",
    "scripts/doc-evidence-inventory-check.py",
    "scripts/fixtures/document-corpus-v3",
)


class DocumentCorpusRetirementTest(unittest.TestCase):
    def test_document_corpus_hash_system_is_retired(self) -> None:
        for rel in RETIRED_PATHS:
            with self.subTest(path=rel):
                path = ROOT / rel
                self.assertFalse(path.exists() or path.is_symlink(), f"retired path still exists: {rel}")


if __name__ == "__main__":
    unittest.main()
