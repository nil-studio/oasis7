#!/usr/bin/env python3
"""Retain the trusted BASE entry point for the canonical corpus retirement test."""

import importlib.util
from pathlib import Path
import unittest


spec = importlib.util.spec_from_file_location(
    "document_corpus_retirement",
    Path(__file__).with_name("document-corpus-inventory-check.test.py"),
)
assert spec is not None and spec.loader is not None
canonical = importlib.util.module_from_spec(spec)
spec.loader.exec_module(canonical)


class DocEvidenceInventoryRetirementTest(unittest.TestCase):
    def test_doc_evidence_inventory_shards_are_retired(self) -> None:
        # Share this case's subTest outcome so delegated failures cannot pass.
        canonical.DocumentCorpusRetirementTest.test_document_corpus_hash_system_is_retired(self)


if __name__ == "__main__":
    unittest.main()
