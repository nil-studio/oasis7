#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

sandbox_root=""

cleanup() {
  [[ -z "$sandbox_root" ]] || rm -rf "$sandbox_root"
}
trap cleanup EXIT

assert_fails_with() {
  local expected="$1"
  shift
  local status=0
  local output
  output="$("$@" 2>&1)" || status=$?
  if [[ "$status" -eq 0 ]]; then
    echo "expected command to fail: $*" >&2
    printf '%s\n' "$output" >&2
    exit 1
  fi
  if [[ "$output" != *"$expected"* ]]; then
    echo "expected failure output to contain: $expected" >&2
    printf '%s\n' "$output" >&2
    exit 1
  fi
}

make_sandbox() {
  [[ -z "$sandbox_root" ]] || rm -rf "$sandbox_root"
  sandbox_root="$(mktemp -d)"
  mkdir -p "$sandbox_root/scripts" "$sandbox_root/doc/testing/templates" "$sandbox_root/doc/p2p"
  cp ./scripts/unified-world-code-terminology-scan.sh "$sandbox_root/scripts/"
}

./scripts/unified-world-code-terminology-scan.sh >/dev/null

# A clean checkout needs no frozen document snapshots or compatibility stub.
make_sandbox
"$sandbox_root/scripts/unified-world-code-terminology-scan.sh" >/dev/null

legacy_content="shared""_network should not re-enter active templates"
content_probe="doc/testing/templates/terminology-probe.template.tsv"
printf 'mode\tclaim\nprobe\t%s\n' "$legacy_content" > "$sandbox_root/$content_probe"
assert_fails_with "$content_probe" "$sandbox_root/scripts/unified-world-code-terminology-scan.sh"
assert_fails_with "$legacy_content" "$sandbox_root/scripts/unified-world-code-terminology-scan.sh"
rm -f "$sandbox_root/$content_probe"

path_probe="doc/testing/templates/shared""-network-regression-probe.template.tsv"
printf 'mode\tclaim\nprobe\tclean contents\n' > "$sandbox_root/$path_probe"
assert_fails_with "$path_probe: legacy terminology in path name" "$sandbox_root/scripts/unified-world-code-terminology-scan.sh"
rm -f "$sandbox_root/$path_probe"

# Existing compatibility wrappers and historical boundary explanations remain
# valid only on their specific surfaces; they do not exempt arbitrary scripts.
legacy_devnet="shared""-devnet"
wrapper="scripts/${legacy_devnet}-rehearsal.sh"
compatibility_line="${legacy_devnet}-rehearsal.sh is a legacy compatibility wrapper"
printf '%s\n' "$compatibility_line" > "$sandbox_root/$wrapper"
printf 'legacy %s\n' "$legacy_content" > "$sandbox_root/doc/p2p/prd.md"
"$sandbox_root/scripts/unified-world-code-terminology-scan.sh" >/dev/null
printf '%s\n' "$compatibility_line" > "$sandbox_root/scripts/unrelated.sh"
assert_fails_with "scripts/unrelated.sh" "$sandbox_root/scripts/unified-world-code-terminology-scan.sh"
rm -f "$sandbox_root/scripts/unrelated.sh"
printf '%s\n' "$legacy_content" > "$sandbox_root/$wrapper"
assert_fails_with "$wrapper" "$sandbox_root/scripts/unified-world-code-terminology-scan.sh"
rm -f "$sandbox_root/$wrapper"
printf '%s\n' "$legacy_content" > "$sandbox_root/doc/p2p/prd.md"
assert_fails_with "doc/p2p/prd.md" "$sandbox_root/scripts/unified-world-code-terminology-scan.sh"

# Former snapshot and policy paths no longer have content exemptions.
make_sandbox
mkdir -p "$sandbox_root/scripts/fixtures/document-corpus-v3/legacy"
printf '{"historical_claim":"%s"}\n' "$legacy_content" > \
  "$sandbox_root/scripts/fixtures/document-corpus-v3/legacy/inventory.json"
assert_fails_with "scripts/fixtures/document-corpus-v3/legacy/inventory.json" \
  "$sandbox_root/scripts/unified-world-code-terminology-scan.sh"

make_sandbox
misused_reference="doc/testing/evidence/shared""-network-"
printf 'unexpected_active_reference = "%s"\n' "$misused_reference" > \
  "$sandbox_root/scripts/document_evidence_policy.py"
assert_fails_with "unexpected_active_reference" \
  "$sandbox_root/scripts/unified-world-code-terminology-scan.sh"

echo "unified-world-code-terminology-scan.test: OK"
