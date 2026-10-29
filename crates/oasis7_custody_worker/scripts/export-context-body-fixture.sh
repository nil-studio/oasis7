#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../../.."
# Structural storage scope plus explicitly historical synthetic SDK replay.
fixture=$(mktemp /tmp/oasis7-context-body-fixture.XXXXXX)
native_fixture=$(mktemp /tmp/oasis7-context-native-fixture.XXXXXX)
trap 'rm -f "$fixture" "$native_fixture"' EXIT
OASIS7_CONTEXT_BODY_FIXTURE_OUTPUT="$fixture" \
OASIS7_CONTEXT_BODY_NATIVE_FIXTURE_OUTPUT="$native_fixture" env -u RUSTC_WRAPPER \
  cargo test --locked -p oasis7_custody_core context_body_export -- --nocapture
test -s "$fixture"
test -s "$native_fixture"
mkdir -p crates/oasis7_custody_worker/artifacts
cp "$fixture" crates/oasis7_custody_worker/artifacts/context-body-fixture.json
cp "$native_fixture" crates/oasis7_custody_worker/artifacts/context-body-native-fixture.json
