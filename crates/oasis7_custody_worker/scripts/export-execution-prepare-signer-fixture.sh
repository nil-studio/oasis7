#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p artifacts
fixture_output=$(mktemp "${TMPDIR:-/tmp}/oasis7-execution-signer-export.XXXXXX")
trap 'rm -f "$fixture_output"' EXIT
OASIS7_EXECUTION_PREPARE_SIGNER_FIXTURE="$fixture_output" env -u RUSTC_WRAPPER cargo test --locked -p oasis7_custody_core session_v2_prepare_signer_complete_sdk_signatures_and_fixture_export -- --nocapture --test-threads=1
python3 - "$fixture_output" <<'VERIFY'
import json,sys
with open(sys.argv[1]) as f:
    value=json.load(f)
assert value['profile']=='historical-core-fixture-not-paid-worker-current'
assert value['proof']['permit']['operation']=='sign_prepare'
assert value['initialize_proof']['permit']['operation']=='initialize_signer'
assert len(value['expected_original_digest'])==64
VERIFY
mv "$fixture_output" artifacts/execution-prepare-signer-fixture.json
