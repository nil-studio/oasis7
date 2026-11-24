#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$repo_root"

run() {
  echo "+ $*"
  "$@"
}

ensure_file_contains() {
  local file=$1
  local pattern=$2
  if ! rg -q -- "$pattern" "$file"; then
    echo "error: pattern not found: $pattern" >&2
    echo "  file=$file" >&2
    exit 1
  fi
}

mkdir -p .tmp
smoke_root=$(mktemp -d .tmp/release-candidate-bundle-smoke.XXXXXX)
trap 'rm -rf "$smoke_root"' EXIT
mkdir -p "$smoke_root/runtime" "$smoke_root/world" "$smoke_root/generated-scenario-world" "$smoke_root/evidence"

printf 'runtime-build-v1\n' >"$smoke_root/runtime/runtime.bin"
printf 'snapshot\n' >"$smoke_root/world/state.txt"
printf 'generated-map\n' >"$smoke_root/generated-scenario-world/snapshot.json"
printf 'generated-journal\n' >"$smoke_root/generated-scenario-world/journal.json"
printf '{"scenario_id":"asteroid_fragment_bootstrap"}\n' >"$smoke_root/world-generation-provenance.json"
printf '{"signers":["signer01"]}\n' >"$smoke_root/world/public_manifest.json"
printf '# smoke evidence\n' >"$smoke_root/evidence/evidence.md"

bundle_path="$smoke_root/candidate.json"
validation_ok="$smoke_root/validation-ok.json"
validation_fail="$smoke_root/validation-fail.log"

run ./scripts/release-candidate-bundle.sh create \
  --bundle "$bundle_path" \
  --candidate-id "public-testnet-rehearsal-smoke-01" \
  --track "public_testnet_rehearsal" \
  --runtime-build-ref "$smoke_root/runtime/runtime.bin" \
  --world-snapshot-ref "$smoke_root/world" \
  --generated-world-sidecar-ref "$smoke_root/generated-scenario-world" \
  --world-generation-provenance-ref "$smoke_root/world-generation-provenance.json" \
  --governance-manifest-ref "$smoke_root/world/public_manifest.json" \
  --evidence-ref "$smoke_root/evidence/evidence.md" \
  --note "smoke test bundle" \
  --allow-dirty-worktree

run ./scripts/release-candidate-bundle.sh validate \
  --bundle "$bundle_path" \
  >"$validation_ok"
ensure_file_contains "$validation_ok" '"validation": "ok"'
ensure_file_contains "$bundle_path" '"candidate_id": "public-testnet-rehearsal-smoke-01"'
ensure_file_contains "$bundle_path" '"generated_world_sidecar"'
ensure_file_contains "$bundle_path" '"world_generation_provenance"'

printf 'mutated\n' >>"$smoke_root/world/state.txt"
set +e
./scripts/release-candidate-bundle.sh validate \
  --bundle "$bundle_path" \
  >"$validation_fail" 2>&1
fail_code=$?
set -e

if [[ "$fail_code" -eq 0 ]]; then
  echo "error: expected validation drift failure after mutating snapshot" >&2
  exit 1
fi
ensure_file_contains "$validation_fail" 'world_snapshot drift detected'

# A copied bundle must validate the current checkout, not the machine that
# first recorded it. Keep one genuinely external file to exercise its contract.
portable_root="$repo_root/$smoke_root/checkouts"
original="$portable_root/original"
relocated="$portable_root/relocated"
portable_bundle="$portable_root/candidate.json"
mkdir -p "$original/scripts" "$original/world"
cp scripts/release-candidate-bundle.sh "$original/scripts/"
printf 'portable runtime\n' >"$original/runtime"
printf 'portable world\n' >"$original/world/state"
printf 'portable evidence\n' >"$original/evidence"
printf 'external governance\n' >"$portable_root/governance"
git init -q "$original"
git -C "$original" add .
git -C "$original" -c user.name=Fixture -c user.email=fixture@example.invalid \
  -c commit.gpgsign=false -c core.hooksPath=/dev/null commit -qm "fixture"
run "$original/scripts/release-candidate-bundle.sh" create \
  --bundle "$portable_bundle" --candidate-id relocation-test \
  --runtime-build-ref runtime --world-snapshot-ref world \
  --governance-manifest-ref "$portable_root/governance" --evidence-ref evidence
run "$original/scripts/release-candidate-bundle.sh" validate \
  --bundle "$portable_bundle" --check-git-head --check-clean-worktree
git clone -q --no-hardlinks "$original" "$relocated"
validate_relocation_failure() {
  local candidate=$1
  local message=$2
  local failure="$portable_root/validation-failure.log"
  if "$relocated/scripts/release-candidate-bundle.sh" validate --bundle "$candidate" >"$failure" 2>&1; then
    echo "error: validation unexpectedly succeeded; expected: $message" >&2
    exit 1
  fi
  ensure_file_contains "$failure" "$message"
}
printf 'wrong checkout bytes\n' >"$relocated/runtime"
validate_relocation_failure "$portable_bundle" 'runtime_build drift detected'
git -C "$relocated" checkout -- runtime
rm -rf "$original"
run "$relocated/scripts/release-candidate-bundle.sh" validate \
  --bundle "$portable_bundle" --check-git-head --check-clean-worktree

printf 'changed evidence\n' >"$relocated/evidence"
validate_relocation_failure "$portable_bundle" 'evidence_refs.*drift detected'
git -C "$relocated" checkout -- evidence
jq '.runtime_build.ref = "../runtime"' "$portable_bundle" >"$portable_root/traversal.json"
validate_relocation_failure "$portable_root/traversal.json" 'runtime_build does not identify the recorded repository path'

cp "$relocated/runtime" "$portable_root/outside"
rm "$relocated/runtime"
ln -s "$portable_root/outside" "$relocated/runtime"
validate_relocation_failure "$portable_bundle" 'runtime_build repository path escapes current checkout'
git -C "$relocated" checkout -- runtime

cp "$relocated/world/state" "$portable_root/outside"
rm "$relocated/world/state"
ln -s "$portable_root/outside" "$relocated/world/state"
validate_relocation_failure "$portable_bundle" 'world_snapshot relocated tree contains a symbolic link'
git -C "$relocated" checkout -- world/state

printf 'changed external governance\n' >"$portable_root/governance"
validate_relocation_failure "$portable_bundle" 'governance_manifest drift detected'
rm "$portable_root/governance"
printf 'external governance\n' >"$relocated/governance"
validate_relocation_failure "$portable_bundle" 'governance_manifest path missing'

echo "release candidate bundle smoke checks passed"
