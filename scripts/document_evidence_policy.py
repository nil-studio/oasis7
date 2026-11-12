#!/usr/bin/env python3
"""Retired document evidence policy compatibility reference for terminology scan."""


def _retired_legacy_evidence_reference(old_by_path: dict[str, dict[str, str]]) -> tuple[list[str], str]:
    legacy_sources = [path for path, item in old_by_path.items() if item.get("semantic_role") == "legacy_network_rehearsal" and path.startswith("doc/testing/evidence/shared-network-")]
    authority_path = "doc/testing/evidence/legacy-shared-devnet-provenance-2026-07-26.md"
    return legacy_sources, authority_path
