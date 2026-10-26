#!/usr/bin/env bash
# Compile the real native SQLite authority fixture before parallel Node tests.
set -euo pipefail
cd "$(dirname "$0")/../../.."
mkdir -p crates/oasis7_custody_worker/artifacts
scratch=$(mktemp)
trap 'rm -f "$scratch"' EXIT
env -u RUSTC_WRAPPER cargo test -p oasis7_launcher_backend --bin oasis7_game_launcher --no-run --locked --message-format=json >"$scratch"
node --input-type=module - "$scratch" <<'JS'
import { readFileSync, writeFileSync } from 'node:fs';
const records = readFileSync(process.argv[2], 'utf8').split('\n').filter(Boolean).map(line => JSON.parse(line));
const binaries = records.filter(value => value.reason === 'compiler-artifact' && value.target.name === 'oasis7_game_launcher' && value.profile.test && value.executable).map(value => value.executable);
if (binaries.length !== 1) throw new Error('expected exactly one real native authority test executable');
writeFileSync('crates/oasis7_custody_worker/artifacts/native-test-binary.txt', binaries[0] + '\n');
JS
