#!/usr/bin/env bash
# Real NodeRuntime/World execution fixture; no synthetic canonical response server.
set -euo pipefail
cd "$(dirname "$0")/../../.."
mkdir -p crates/oasis7_custody_worker/artifacts
scratch=$(mktemp)
trap 'rm -f "$scratch"' EXIT
env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime --lib --features test_tier_required --no-run --locked --message-format=json >"$scratch"
node --input-type=module - "$scratch" <<'JS'
import { readFileSync, writeFileSync } from 'node:fs';
const records=readFileSync(process.argv[2],'utf8').split('\n').filter(Boolean).map(line=>JSON.parse(line));
const binaries=records.filter(value=>value.reason==='compiler-artifact' && value.target.name==='oasis7_chain_runtime' && value.target.kind.includes('lib') && value.profile.test && value.executable).map(value=>value.executable);
if(binaries.length!==1) throw new Error('expected exactly one real World execution test executable');
writeFileSync('crates/oasis7_custody_worker/artifacts/world-test-binary.txt',binaries[0]+'\n');
JS
