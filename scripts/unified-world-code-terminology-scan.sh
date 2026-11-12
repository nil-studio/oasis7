#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

python3 - "$repo_root" <<'PY'
from __future__ import annotations

import pathlib
import re
import sys

repo_root = pathlib.Path(sys.argv[1]).resolve()
scan_paths = [
    "crates",
    "scripts",
    "doc/testing/templates",
    "doc/engineering/governance/environment-lanes-and-inventory-2026-05-29.md",
    "doc/p2p/prd.md",
    "doc/p2p/prd.index.md",
    "doc/p2p/blockchain/formal-network-tiers-testnet-mechanism.prd.md",
    "doc/p2p/blockchain/formal-network-tiers-testnet-mechanism.design.md",
    "doc/p2p/blockchain/formal-network-tiers-testnet-mechanism.runbook.md",
    "testing-manual.md",
]
pattern = r"shared_devnet|shared_network|shared-devnet|shared-network"
boundary_files = {
    "doc/engineering/governance/environment-lanes-and-inventory-2026-05-29.md",
    "doc/p2p/prd.md",
    "doc/p2p/prd.index.md",
    "doc/p2p/blockchain/formal-network-tiers-testnet-mechanism.prd.md",
    "doc/p2p/blockchain/formal-network-tiers-testnet-mechanism.design.md",
    "doc/p2p/blockchain/formal-network-tiers-testnet-mechanism.runbook.md",
}
boundary_markers = (
    "legacy",
    "旧",
    "历史",
    "不再",
    "不是",
    "不能",
    "不得",
    "不等于",
    "不替代",
    "not a target",
    "no longer",
    "does not replace",
    "not public_testnet",
    "not mainnet",
)
allowed_snippets = {
    "testing-manual.md": (
        "summary.json.evidence_contract.claim_readiness.shared_network_pass_blockers",
        "evidence_contract.claim_readiness.shared_network_pass_blockers",
    ),
    "scripts/network-tier-manifest.sh": (
        '"$tier" == "shared_devnet"',
        "shared_devnet is no longer a manifest tier",
    ),
    "scripts/p2p-mixed-topology-matrix.sh": (
        "shared_network_pass_inputs_ready:",
        "shared_network_pass_blockers:",
        "same_window_shared_network_evidence_ref",
        'shared_network_pass_inputs_ready',
        'shared_network_pass_blockers',
    ),
    "scripts/p2p-mixed-topology-matrix-smoke.sh": (
        'shared_network_pass_blockers',
    ),
    "scripts/public-testnet-rehearsal.sh": (
        "run_capture shared_network_gate",
        'shared_network_gate.rc',
    ),
    "scripts/release-candidate-bundle.sh": (
        "shared_devnet|shared_network)",
        '{"shared_devnet", "shared_network"}',
    ),
    "scripts/shared-devnet-blocker-packet.sh": (
        "shared-devnet-blocker-packet.sh is a legacy compatibility wrapper",
    ),
    "scripts/shared-devnet-rehearsal.sh": (
        "shared-devnet-rehearsal.sh is a legacy compatibility wrapper",
    ),
    "scripts/legacy-shared-devnet-provenance-smoke.sh": (
        "authority=doc/testing/evidence/legacy-shared-devnet-provenance-2026-07-26.md",
        '"shared_devnet"',
        '"shared-devnet-live-reset-20260523-01"',
        'shared_devnet-',
        'shared-network-shared-devnet-',
        'active generated shared_devnet capture reference remains',
        'legacy-shared-devnet-provenance-smoke',
    ),
    "scripts/shared-network-track-gate.sh": (
        "shared-network-track-gate.sh is a legacy compatibility wrapper",
    ),
    "scripts/shared-network-track-gate-smoke.sh": (
        'shared_network_track_gate_smoke',
    ),
    "doc/p2p/blockchain/formal-network-tiers-testnet-mechanism.prd.md": (
        "doc/p2p/blockchain/formal-network-tiers-testnet-mechanism.prd.md",
        "scripts/shared-network-track-gate.sh",
    ),
    "doc/p2p/prd.md": (
        "doc/p2p/blockchain/formal-network-tiers-testnet-mechanism.prd.md",
    ),
}
allowed_path_names = {
    "doc/testing/templates/shared-network-exit-decision-template.md",
    "doc/testing/templates/shared-network-incident-review-template.md",
    "doc/testing/templates/shared-network-incident-template.md",
    "doc/testing/templates/shared-network-mixed-topology-gate-template.md",
    "doc/testing/templates/shared-network-promotion-record-template.md",
    "doc/testing/templates/shared-network-rollback-target-template.md",
    "doc/testing/templates/shared-network-shared-access-check-template.md",
    "doc/testing/templates/shared-network-track-gate-lanes.canary.template.tsv",
    "doc/testing/templates/shared-network-track-gate-lanes.shared_devnet.template.tsv",
    "doc/testing/templates/shared-network-track-gate-lanes.staging.template.tsv",
    "doc/testing/templates/shared-network-track-gate-template.md",
    "scripts/shared-devnet-blocker-packet-smoke.sh",
    "scripts/shared-devnet-blocker-packet.sh",
    "scripts/shared-devnet-rehearsal-smoke.sh",
    "scripts/shared-devnet-rehearsal.sh",
    "scripts/legacy-shared-devnet-provenance-smoke.sh",
    "scripts/shared-network-track-gate-smoke.sh",
    "scripts/shared-network-track-gate.sh",
}

def iter_scan_files() -> list[pathlib.Path]:
    files: list[pathlib.Path] = []
    for rel in scan_paths:
        path = repo_root / rel
        if path.is_file():
            files.append(path)
            continue
        if path.is_dir():
            files.extend(p for p in path.rglob("*") if p.is_file())
    return sorted(files)

unexpected: list[str] = []
allowed_hits: list[str] = []
scan_re = re.compile(pattern)
scan_files = iter_scan_files()
for path in scan_files:
    rel = path.relative_to(repo_root).as_posix()
    if rel == "scripts/unified-world-code-terminology-scan.sh":
        continue
    try:
        text = path.read_text(encoding="utf-8")
    except UnicodeDecodeError:
        continue
    snippets = allowed_snippets.get(rel, ())
    for line_no, text_line in enumerate(text.splitlines(), start=1):
        if not scan_re.search(text_line):
            continue
        rendered = f"{rel}:{line_no}:{text_line}"
        if any(snippet in text_line for snippet in snippets):
            allowed_hits.append(rendered)
        elif rel in boundary_files and any(marker in text_line for marker in boundary_markers):
            allowed_hits.append(rendered)
        else:
            unexpected.append(rendered)

path_pattern = re.compile(pattern)
allowed_path_hits: list[str] = []
for path in scan_files:
    rel = path.relative_to(repo_root).as_posix()
    if rel == "scripts/unified-world-code-terminology-scan.sh":
        continue
    if not path_pattern.search(rel):
        continue
    if rel in allowed_path_names:
        allowed_path_hits.append(rel)
    else:
        unexpected.append(f"{rel}: legacy terminology in path name")

if unexpected:
    print("unified-world-code-terminology-scan: FAIL")
    print("Unexpected legacy shared world terminology in active code/template surfaces:")
    for line in unexpected:
        print(f"  {line}")
    print()
    print("Allowed compatibility snippets are limited to:")
    for rel, snippets in sorted(allowed_snippets.items()):
        for snippet in snippets:
            print(f"  {rel}: {snippet}")
    raise SystemExit(1)

print("unified-world-code-terminology-scan: OK")
print(f"allowed compatibility content hits: {len(allowed_hits)}")
print(f"allowed compatibility path hits: {len(allowed_path_hits)}")
PY
